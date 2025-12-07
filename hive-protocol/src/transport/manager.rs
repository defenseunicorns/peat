//! Transport Manager for multi-transport coordination
//!
//! This module provides the `TransportManager` which coordinates multiple
//! transport implementations, selecting the best one for each message
//! based on requirements and current conditions.
//!
//! ## Architecture (ADR-032)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Application Layer                              │
//! │         ┌────────────────────────────────────┐                           │
//! │         │        Transport Manager           │ ◄── Transport Selection   │
//! │         │   (Multi-Transport Coordinator)    │     Message Requirements  │
//! │         └──────────────┬─────────────────────┘                           │
//! │                        │                                                 │
//! │         ┌──────────────┴──────────────┐                                  │
//! │         ▼              ▼              ▼              ▼                   │
//! │  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐            │
//! │  │   QUIC     │ │ Bluetooth  │ │   LoRa     │ │ WiFi Direct│            │
//! │  │  (Iroh)    │ │    LE      │ │            │ │            │            │
//! │  └────────────┘ └────────────┘ └────────────┘ └────────────┘            │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use hive_protocol::transport::{
//!     TransportManager, TransportManagerConfig,
//!     MessageRequirements, MessagePriority, TransportType,
//! };
//!
//! // Create manager with configuration
//! let config = TransportManagerConfig::default();
//! let mut manager = TransportManager::new(config);
//!
//! // Register transports
//! manager.register(Arc::new(quic_transport));
//! manager.register(Arc::new(ble_transport));
//!
//! // Select best transport for message
//! let requirements = MessageRequirements {
//!     reliable: true,
//!     priority: MessagePriority::High,
//!     ..Default::default()
//! };
//!
//! if let Some(transport_type) = manager.select_transport(&peer_id, &requirements) {
//!     println!("Selected transport: {}", transport_type);
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::capabilities::{MessageRequirements, PeerDistance, RangeMode, Transport, TransportType};
use super::{NodeId, Result, TransportError};

// =============================================================================
// Transport Manager Configuration
// =============================================================================

/// Configuration for TransportManager
#[derive(Debug, Clone)]
pub struct TransportManagerConfig {
    /// Transport preference order (first = highest preference)
    pub preference_order: Vec<TransportType>,

    /// Enable automatic transport fallback on failure
    pub enable_fallback: bool,

    /// Cache transport selection per peer
    pub cache_peer_transport: bool,

    /// Minimum score difference to switch transports
    pub switch_threshold: i32,
}

impl Default for TransportManagerConfig {
    fn default() -> Self {
        Self {
            preference_order: vec![
                TransportType::Quic,
                TransportType::WifiDirect,
                TransportType::BluetoothLE,
                TransportType::LoRa,
            ],
            enable_fallback: true,
            cache_peer_transport: true,
            switch_threshold: 10,
        }
    }
}

// =============================================================================
// Transport Manager
// =============================================================================

/// Manages multiple transports and handles transport selection
///
/// TransportManager coordinates multiple transport implementations,
/// selecting the best one for each message based on:
/// - Message requirements (reliability, latency, size)
/// - Transport capabilities
/// - Current availability and signal quality
/// - User preference order
/// - Historical success with peer
pub struct TransportManager {
    /// Registered transports by type
    transports: HashMap<TransportType, Arc<dyn Transport>>,

    /// Active transport per peer (learned from successful deliveries)
    peer_transports: RwLock<HashMap<NodeId, TransportType>>,

    /// Peer distance estimates
    peer_distances: RwLock<HashMap<NodeId, PeerDistance>>,

    /// Configuration
    config: TransportManagerConfig,
}

impl TransportManager {
    /// Create a new TransportManager with the given configuration
    pub fn new(config: TransportManagerConfig) -> Self {
        Self {
            transports: HashMap::new(),
            peer_transports: RwLock::new(HashMap::new()),
            peer_distances: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Register a transport
    ///
    /// The transport will be available for selection based on its capabilities.
    pub fn register(&mut self, transport: Arc<dyn Transport>) {
        let transport_type = transport.capabilities().transport_type;
        self.transports.insert(transport_type, transport);
    }

    /// Unregister a transport
    ///
    /// Returns the removed transport, if it was registered.
    pub fn unregister(&mut self, transport_type: TransportType) -> Option<Arc<dyn Transport>> {
        self.transports.remove(&transport_type)
    }

    /// Get a registered transport by type
    pub fn get_transport(&self, transport_type: TransportType) -> Option<&Arc<dyn Transport>> {
        self.transports.get(&transport_type)
    }

    /// Get all registered transport types
    pub fn registered_transports(&self) -> Vec<TransportType> {
        self.transports.keys().copied().collect()
    }

    /// Get transports that are currently available and can reach the peer
    pub fn available_transports(&self, peer_id: &NodeId) -> Vec<TransportType> {
        self.transports
            .iter()
            .filter(|(_, t)| t.is_available() && t.can_reach(peer_id))
            .map(|(tt, _)| *tt)
            .collect()
    }

    /// Select the best transport for a peer and message requirements
    ///
    /// Returns the transport type that best matches the requirements,
    /// or `None` if no suitable transport is available.
    ///
    /// # Selection Algorithm
    ///
    /// 1. Filter transports by availability and reachability
    /// 2. Filter by hard requirements (reliability, bandwidth, message size)
    /// 3. Score remaining transports based on:
    ///    - Latency (for high-priority messages)
    ///    - Power consumption (if power-sensitive)
    ///    - User preference order
    ///    - Signal quality (for wireless)
    /// 4. Return highest-scoring transport
    pub fn select_transport(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Option<TransportType> {
        // Check cache first if enabled
        if self.config.cache_peer_transport {
            if let Some(&cached) = self.peer_transports.read().unwrap().get(peer_id) {
                // Verify cached transport still valid
                if let Some(transport) = self.transports.get(&cached) {
                    if transport.is_available()
                        && transport.can_reach(peer_id)
                        && transport.capabilities().meets_requirements(requirements)
                    {
                        return Some(cached);
                    }
                }
            }
        }

        // Find available transports that meet requirements
        let candidates: Vec<_> = self
            .available_transports(peer_id)
            .into_iter()
            .filter_map(|tt| {
                let transport = self.transports.get(&tt)?;
                let caps = transport.capabilities();

                // Check hard requirements
                if !caps.meets_requirements(requirements) {
                    return None;
                }

                // Check latency requirement
                if let Some(max_latency) = requirements.max_latency_ms {
                    let est_delivery = transport.estimate_delivery_ms(requirements.message_size);
                    if est_delivery > max_latency {
                        return None;
                    }
                }

                // Calculate preference bonus
                let preference_bonus = self
                    .config
                    .preference_order
                    .iter()
                    .position(|&t| t == tt)
                    .map(|idx| 20 - (idx as i32 * 5))
                    .unwrap_or(0);

                let score = transport.calculate_score(requirements, preference_bonus);
                Some((tt, score))
            })
            .collect();

        // Return highest-scoring transport
        candidates
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .map(|(tt, _)| tt)
    }

    /// Select transport with distance-based range mode adaptation
    ///
    /// Returns the best transport type and optionally a recommended range mode
    /// if the transport supports dynamic range configuration.
    pub fn select_transport_for_distance(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Option<(TransportType, Option<RangeMode>)> {
        let transport_type = self.select_transport(peer_id, requirements)?;

        // Get distance estimate if available
        let distance = self
            .peer_distances
            .read()
            .unwrap()
            .get(peer_id)
            .map(|d| d.distance_meters);

        // If we have a configurable transport, get recommended mode
        let range_mode = if let Some(_dist) = distance {
            // This would need runtime trait casting - for now return None
            // In a full implementation, we'd use trait objects with downcast
            None // Placeholder - see implementation note below
        } else {
            None
        };

        Some((transport_type, range_mode))
    }

    /// Record successful transport use for a peer
    ///
    /// This updates the peer transport cache for future selections.
    pub fn record_success(&self, peer_id: &NodeId, transport_type: TransportType) {
        if self.config.cache_peer_transport {
            self.peer_transports
                .write()
                .unwrap()
                .insert(peer_id.clone(), transport_type);
        }
    }

    /// Clear cached transport for a peer
    ///
    /// Call this when a transport fails for a peer.
    pub fn clear_cache(&self, peer_id: &NodeId) {
        self.peer_transports.write().unwrap().remove(peer_id);
    }

    /// Update distance estimate for a peer
    pub fn update_peer_distance(&self, distance: PeerDistance) {
        self.peer_distances
            .write()
            .unwrap()
            .insert(distance.peer_id.clone(), distance);
    }

    /// Get current distance estimate for a peer
    pub fn get_peer_distance(&self, peer_id: &NodeId) -> Option<PeerDistance> {
        self.peer_distances.read().unwrap().get(peer_id).cloned()
    }

    /// Connect to a peer using the best available transport
    ///
    /// This is a convenience method that selects the transport and connects.
    pub async fn connect(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Result<(TransportType, Box<dyn super::MeshConnection>)> {
        let transport_type = self
            .select_transport(peer_id, requirements)
            .ok_or_else(|| {
                TransportError::PeerNotFound(format!("No suitable transport for {}", peer_id))
            })?;

        let transport = self
            .transports
            .get(&transport_type)
            .ok_or(TransportError::NotStarted)?;

        let connection = transport.connect(peer_id).await?;

        // Record successful connection
        self.record_success(peer_id, transport_type);

        Ok((transport_type, connection))
    }

    /// Connect with fallback to alternative transports
    ///
    /// Tries the primary transport first, then falls back to others if enabled.
    pub async fn connect_with_fallback(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Result<(TransportType, Box<dyn super::MeshConnection>)> {
        // Get all candidate transports sorted by score
        let candidates: Vec<_> = self
            .available_transports(peer_id)
            .into_iter()
            .filter_map(|tt| {
                let transport = self.transports.get(&tt)?;
                if !transport.capabilities().meets_requirements(requirements) {
                    return None;
                }
                let preference_bonus = self
                    .config
                    .preference_order
                    .iter()
                    .position(|&t| t == tt)
                    .map(|idx| 20 - (idx as i32 * 5))
                    .unwrap_or(0);
                let score = transport.calculate_score(requirements, preference_bonus);
                Some((tt, score))
            })
            .collect();

        let mut sorted: Vec<_> = candidates;
        sorted.sort_by(|a, b| b.1.cmp(&a.1)); // Sort descending by score

        if sorted.is_empty() {
            return Err(TransportError::PeerNotFound(format!(
                "No suitable transport for {}",
                peer_id
            )));
        }

        let mut last_error = None;

        for (transport_type, _) in sorted {
            let transport = match self.transports.get(&transport_type) {
                Some(t) => t,
                None => continue,
            };

            match transport.connect(peer_id).await {
                Ok(conn) => {
                    self.record_success(peer_id, transport_type);
                    return Ok((transport_type, conn));
                }
                Err(e) => {
                    if !self.config.enable_fallback {
                        return Err(e);
                    }
                    last_error = Some(e);
                    self.clear_cache(peer_id);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            TransportError::PeerNotFound(format!("All transports failed for {}", peer_id))
        }))
    }
}

impl std::fmt::Debug for TransportManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportManager")
            .field("transports", &self.transports.keys().collect::<Vec<_>>())
            .field("config", &self.config)
            .finish()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::capabilities::{MessagePriority, TransportCapabilities};
    use crate::transport::{MeshConnection, MeshTransport, PeerEventReceiver};
    use async_trait::async_trait;
    use std::time::Instant;
    use tokio::sync::mpsc;

    // Mock transport for testing
    struct MockTransport {
        caps: TransportCapabilities,
        available: bool,
        reachable_peers: Vec<NodeId>,
        signal: Option<u8>,
    }

    impl MockTransport {
        fn new(caps: TransportCapabilities) -> Self {
            Self {
                caps,
                available: true,
                reachable_peers: vec![],
                signal: None,
            }
        }

        fn with_peer(mut self, peer: NodeId) -> Self {
            self.reachable_peers.push(peer);
            self
        }

        #[allow(dead_code)]
        fn with_signal(mut self, signal: u8) -> Self {
            self.signal = Some(signal);
            self
        }

        fn unavailable(mut self) -> Self {
            self.available = false;
            self
        }
    }

    struct MockConnection {
        peer_id: NodeId,
        connected_at: Instant,
    }

    impl MeshConnection for MockConnection {
        fn peer_id(&self) -> &NodeId {
            &self.peer_id
        }

        fn is_alive(&self) -> bool {
            true
        }

        fn connected_at(&self) -> Instant {
            self.connected_at
        }
    }

    #[async_trait]
    impl MeshTransport for MockTransport {
        async fn start(&self) -> Result<()> {
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
            if self.reachable_peers.contains(peer_id) {
                Ok(Box::new(MockConnection {
                    peer_id: peer_id.clone(),
                    connected_at: Instant::now(),
                }))
            } else {
                Err(TransportError::PeerNotFound(peer_id.to_string()))
            }
        }

        async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
            Ok(())
        }

        fn get_connection(&self, _peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
            None
        }

        fn peer_count(&self) -> usize {
            0
        }

        fn connected_peers(&self) -> Vec<NodeId> {
            vec![]
        }

        fn subscribe_peer_events(&self) -> PeerEventReceiver {
            let (_tx, rx) = mpsc::channel(1);
            rx
        }
    }

    impl Transport for MockTransport {
        fn capabilities(&self) -> &TransportCapabilities {
            &self.caps
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn signal_quality(&self) -> Option<u8> {
            self.signal
        }

        fn can_reach(&self, peer_id: &NodeId) -> bool {
            self.reachable_peers.contains(peer_id)
        }
    }

    #[test]
    fn test_register_transport() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register(transport);

        assert!(manager.get_transport(TransportType::Quic).is_some());
        assert!(manager.get_transport(TransportType::LoRa).is_none());
    }

    #[test]
    fn test_unregister_transport() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register(transport);

        let removed = manager.unregister(TransportType::Quic);
        assert!(removed.is_some());
        assert!(manager.get_transport(TransportType::Quic).is_none());
    }

    #[test]
    fn test_available_transports() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC available and can reach peer
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // BLE available but can't reach peer
        let ble = Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()));
        manager.register(ble);

        // LoRa unavailable
        let lora = Arc::new(
            MockTransport::new(TransportCapabilities::lora(7))
                .unavailable()
                .with_peer(peer.clone()),
        );
        manager.register(lora);

        let available = manager.available_transports(&peer);
        assert_eq!(available.len(), 1);
        assert!(available.contains(&TransportType::Quic));
    }

    #[test]
    fn test_select_transport_by_reliability() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC is reliable
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // LoRa is not reliable by default
        let lora =
            Arc::new(MockTransport::new(TransportCapabilities::lora(7)).with_peer(peer.clone()));
        manager.register(lora);

        // Require reliability
        let requirements = MessageRequirements {
            reliable: true,
            ..Default::default()
        };

        let selected = manager.select_transport(&peer, &requirements);
        assert_eq!(selected, Some(TransportType::Quic));
    }

    #[test]
    fn test_select_transport_by_preference() {
        let config = TransportManagerConfig {
            preference_order: vec![TransportType::BluetoothLE, TransportType::Quic],
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Both transports available
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transport(&peer, &requirements);

        // BLE preferred over QUIC in this config
        assert_eq!(selected, Some(TransportType::BluetoothLE));
    }

    #[test]
    fn test_select_transport_by_latency() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC has 10ms latency
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // LoRa has 100ms+ latency
        let mut lora_caps = TransportCapabilities::lora(7);
        lora_caps.reliable = true; // Make it reliable for this test
        let lora = Arc::new(MockTransport::new(lora_caps).with_peer(peer.clone()));
        manager.register(lora);

        // High priority message - should prefer low latency
        let requirements = MessageRequirements {
            priority: MessagePriority::High,
            reliable: true,
            ..Default::default()
        };

        let selected = manager.select_transport(&peer, &requirements);
        assert_eq!(selected, Some(TransportType::Quic));
    }

    #[test]
    fn test_select_transport_with_latency_requirement() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC: 10ms latency
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // LoRa SF12: ~1000ms latency
        let mut lora_caps = TransportCapabilities::lora(12);
        lora_caps.reliable = true;
        let lora = Arc::new(MockTransport::new(lora_caps).with_peer(peer.clone()));
        manager.register(lora);

        // Strict latency requirement - should exclude LoRa
        let requirements = MessageRequirements {
            reliable: true,
            max_latency_ms: Some(50),
            ..Default::default()
        };

        let selected = manager.select_transport(&peer, &requirements);
        assert_eq!(selected, Some(TransportType::Quic));
    }

    #[test]
    fn test_select_transport_no_match() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Only unreliable LoRa available
        let lora =
            Arc::new(MockTransport::new(TransportCapabilities::lora(7)).with_peer(peer.clone()));
        manager.register(lora);

        // Require reliability
        let requirements = MessageRequirements {
            reliable: true,
            ..Default::default()
        };

        let selected = manager.select_transport(&peer, &requirements);
        assert_eq!(selected, None);
    }

    #[test]
    fn test_peer_transport_caching() {
        let config = TransportManagerConfig {
            cache_peer_transport: true,
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        // Record BLE success
        manager.record_success(&peer, TransportType::BluetoothLE);

        // Should return cached BLE even though QUIC might score higher
        let requirements = MessageRequirements::default();
        let selected = manager.select_transport(&peer, &requirements);
        assert_eq!(selected, Some(TransportType::BluetoothLE));

        // Clear cache
        manager.clear_cache(&peer);

        // Now should select based on score
        let selected = manager.select_transport(&peer, &requirements);
        // With default preference order, QUIC should be selected
        assert_eq!(selected, Some(TransportType::Quic));
    }

    #[test]
    fn test_power_sensitive_selection() {
        // Use empty preference order so only power consumption matters
        let config = TransportManagerConfig {
            preference_order: vec![],
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC: 20 battery impact
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // BLE: 15 battery impact (more efficient)
        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        // Power-sensitive requirement
        let requirements = MessageRequirements {
            power_sensitive: true,
            ..Default::default()
        };

        let selected = manager.select_transport(&peer, &requirements);
        // BLE should be preferred due to lower power consumption
        assert_eq!(selected, Some(TransportType::BluetoothLE));
    }

    #[tokio::test]
    async fn test_connect_selects_transport() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        let requirements = MessageRequirements::default();
        let result = manager.connect(&peer, &requirements).await;

        assert!(result.is_ok());
        let (transport_type, conn) = result.unwrap();
        assert_eq!(transport_type, TransportType::Quic);
        assert_eq!(conn.peer_id(), &peer);
    }

    #[tokio::test]
    async fn test_connect_with_fallback() {
        let config = TransportManagerConfig {
            enable_fallback: true,
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC can't reach peer
        let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register(quic);

        // BLE can reach peer
        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        let requirements = MessageRequirements::default();
        let result = manager.connect_with_fallback(&peer, &requirements).await;

        assert!(result.is_ok());
        let (transport_type, _) = result.unwrap();
        assert_eq!(transport_type, TransportType::BluetoothLE);
    }

    #[test]
    fn test_distance_tracking() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let distance = PeerDistance {
            peer_id: peer.clone(),
            distance_meters: 500,
            source: super::super::capabilities::DistanceSource::Gps {
                confidence_meters: 10,
            },
            last_updated: Instant::now(),
        };

        manager.update_peer_distance(distance);

        let retrieved = manager.get_peer_distance(&peer);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().distance_meters, 500);
    }
}
