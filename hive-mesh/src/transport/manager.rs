//! Transport Manager for multi-transport coordination
//!
//! This module provides the `TransportManager` which coordinates multiple
//! transport implementations, selecting the best one for each message
//! based on requirements and current conditions.
//!
//! ## Architecture (ADR-032 + ADR-042)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Application Layer                              │
//! │         ┌────────────────────────────────────┐                           │
//! │         │        Transport Manager           │ ◄── Transport Selection   │
//! │         │   (Multi-Transport Coordinator)    │     Message Requirements  │
//! │         └──────────────┬─────────────────────┘                           │
//! │                        │                                                 │
//! │    ┌───────────────────┼───────────────────────┐                         │
//! │    ▼                   ▼              ▼        ▼                          │
//! │ ┌──────────┐    ┌────────────┐ ┌──────────┐ ┌────────────┐               │
//! │ │  UDP     │    │   QUIC     │ │ Bluetooth│ │   LoRa     │               │
//! │ │ Bypass   │    │  (Iroh)    │ │    LE    │ │            │               │
//! │ │(ADR-042) │    └────────────┘ └──────────┘ └────────────┘               │
//! │ └──────────┘                                                              │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use hive_mesh::transport::{
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
//!
//! // Send via bypass for low-latency delivery (ADR-042)
//! let bypass_req = MessageRequirements::bypass(5);
//! manager.send_bypass("position_updates", &position_bytes, None).await?;
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use super::bypass::{BypassMessage, BypassTarget, UdpBypassChannel};
use super::capabilities::{
    MessageRequirements, PaceLevel, PeerDistance, RangeMode, Transport, TransportId,
    TransportInstance, TransportMode, TransportPolicy, TransportType,
};
use super::{NodeId, Result, TransportError};
use std::collections::HashSet;
use tokio::sync::broadcast;
use tokio::sync::RwLock as TokioRwLock;

/// Storage type for registered transport instances
type TransportInstanceMap = HashMap<TransportId, (TransportInstance, Arc<dyn Transport>)>;

// =============================================================================
// Transport Manager Configuration
// =============================================================================

/// Configuration for TransportManager
#[derive(Debug, Clone)]
pub struct TransportManagerConfig {
    /// Transport preference order (first = highest preference)
    /// Used for legacy TransportType-based selection
    pub preference_order: Vec<TransportType>,

    /// Enable automatic transport fallback on failure
    pub enable_fallback: bool,

    /// Cache transport selection per peer
    pub cache_peer_transport: bool,

    /// Minimum score difference to switch transports
    pub switch_threshold: i32,

    /// Default PACE policy for transport selection (ADR-032)
    /// If set, takes precedence over preference_order
    pub default_policy: Option<TransportPolicy>,

    /// Transport mode (Single, Redundant, Bonded, LoadBalanced)
    pub transport_mode: TransportMode,
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
            default_policy: None,
            transport_mode: TransportMode::Single,
        }
    }
}

impl TransportManagerConfig {
    /// Create config with a PACE policy
    pub fn with_policy(policy: TransportPolicy) -> Self {
        Self {
            default_policy: Some(policy),
            ..Default::default()
        }
    }

    /// Set the transport mode
    pub fn with_mode(mut self, mode: TransportMode) -> Self {
        self.transport_mode = mode;
        self
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
/// - PACE policy (Primary, Alternate, Contingency, Emergency)
/// - Historical success with peer
///
/// Also manages the UDP bypass channel (ADR-042) for low-latency,
/// high-frequency data that bypasses CRDT synchronization.
///
/// ## PACE Policy (ADR-032)
///
/// When a PACE policy is configured, transport selection follows:
/// 1. Try primary transports first
/// 2. Fall back to alternate if no primary available
/// 3. Use contingency for degraded operation
/// 4. Emergency as last resort
///
/// ```ignore
/// let policy = TransportPolicy::new("tactical")
///     .primary(vec!["iroh-eth0", "iroh-wlan0"])
///     .alternate(vec!["iroh-starlink"])
///     .contingency(vec!["lora-primary"])
///     .emergency(vec!["ble-mesh"]);
///
/// let config = TransportManagerConfig::with_policy(policy);
/// let manager = TransportManager::new(config);
/// ```
pub struct TransportManager {
    /// Registered transports by type (legacy API)
    transports: HashMap<TransportType, Arc<dyn Transport>>,

    /// Registered transports by ID (ADR-032 PACE API)
    transport_instances: RwLock<TransportInstanceMap>,

    /// Active transport per peer (learned from successful deliveries)
    peer_transports: RwLock<HashMap<NodeId, TransportType>>,

    /// Active transport ID per peer (PACE-based)
    peer_transport_ids: RwLock<HashMap<NodeId, TransportId>>,

    /// Peer distance estimates
    peer_distances: RwLock<HashMap<NodeId, PeerDistance>>,

    /// Configuration
    config: TransportManagerConfig,

    /// UDP bypass channel for low-latency ephemeral data (ADR-042)
    ///
    /// When set, the manager can route messages with `bypass_sync: true`
    /// through this channel instead of CRDT transports.
    bypass_channel: Option<Arc<TokioRwLock<UdpBypassChannel>>>,
}

impl TransportManager {
    /// Create a new TransportManager with the given configuration
    pub fn new(config: TransportManagerConfig) -> Self {
        Self {
            transports: HashMap::new(),
            transport_instances: RwLock::new(HashMap::new()),
            peer_transports: RwLock::new(HashMap::new()),
            peer_transport_ids: RwLock::new(HashMap::new()),
            peer_distances: RwLock::new(HashMap::new()),
            config,
            bypass_channel: None,
        }
    }

    /// Create a new TransportManager with bypass channel support (ADR-042)
    pub fn with_bypass(config: TransportManagerConfig, bypass: UdpBypassChannel) -> Self {
        Self {
            transports: HashMap::new(),
            transport_instances: RwLock::new(HashMap::new()),
            peer_transports: RwLock::new(HashMap::new()),
            peer_transport_ids: RwLock::new(HashMap::new()),
            peer_distances: RwLock::new(HashMap::new()),
            config,
            bypass_channel: Some(Arc::new(TokioRwLock::new(bypass))),
        }
    }

    /// Set the bypass channel after construction
    pub fn set_bypass_channel(&mut self, bypass: UdpBypassChannel) {
        self.bypass_channel = Some(Arc::new(TokioRwLock::new(bypass)));
    }

    /// Check if bypass channel is available
    pub fn has_bypass_channel(&self) -> bool {
        self.bypass_channel.is_some()
    }

    /// Check if a collection is configured for bypass
    pub async fn is_bypass_collection(&self, collection: &str) -> bool {
        if let Some(ref bypass) = self.bypass_channel {
            bypass.read().await.is_bypass_collection(collection)
        } else {
            false
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

    // =========================================================================
    // PACE Transport Instance API (ADR-032)
    // =========================================================================

    /// Register a transport instance by ID
    ///
    /// This is the preferred API for multi-instance transports (e.g., multiple NICs).
    ///
    /// # Arguments
    ///
    /// * `instance` - Transport instance metadata
    /// * `transport` - The transport implementation
    ///
    /// # Example
    ///
    /// ```ignore
    /// let instance = TransportInstance::new("iroh-eth0", TransportType::Quic, caps)
    ///     .with_interface("eth0");
    /// manager.register_instance(instance, Arc::new(transport));
    /// ```
    pub fn register_instance(&self, instance: TransportInstance, transport: Arc<dyn Transport>) {
        let id = instance.id.clone();
        self.transport_instances
            .write()
            .unwrap()
            .insert(id, (instance, transport));
    }

    /// Unregister a transport instance by ID
    pub fn unregister_instance(
        &self,
        id: &TransportId,
    ) -> Option<(TransportInstance, Arc<dyn Transport>)> {
        self.transport_instances.write().unwrap().remove(id)
    }

    /// Get a transport instance by ID
    pub fn get_instance(&self, id: &TransportId) -> Option<Arc<dyn Transport>> {
        self.transport_instances
            .read()
            .unwrap()
            .get(id)
            .map(|(_, t)| Arc::clone(t))
    }

    /// Get all registered instance IDs
    pub fn registered_instance_ids(&self) -> Vec<TransportId> {
        self.transport_instances
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Get IDs of available transport instances
    pub fn available_instance_ids(&self) -> HashSet<TransportId> {
        self.transport_instances
            .read()
            .unwrap()
            .iter()
            .filter(|(_, (inst, transport))| inst.available && transport.is_available())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get IDs of available transports that can reach a peer
    pub fn available_instances_for_peer(&self, peer_id: &NodeId) -> Vec<TransportId> {
        self.transport_instances
            .read()
            .unwrap()
            .iter()
            .filter(|(_, (inst, transport))| {
                inst.available && transport.is_available() && transport.can_reach(peer_id)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get the current PACE level based on available transports
    ///
    /// Returns the best PACE level for which at least one transport is available.
    pub fn current_pace_level(&self) -> PaceLevel {
        match &self.config.default_policy {
            Some(policy) => policy.current_level(&self.available_instance_ids()),
            None => {
                // No policy - if any transport available, consider it "Primary"
                if !self.available_instance_ids().is_empty() {
                    PaceLevel::Primary
                } else {
                    PaceLevel::None
                }
            }
        }
    }

    /// Select transport(s) using PACE policy
    ///
    /// Returns transport IDs in priority order based on PACE policy and availability.
    /// The number of transports returned depends on the configured TransportMode:
    /// - Single: Returns at most one transport
    /// - Redundant: Returns multiple for simultaneous send
    /// - LoadBalanced: Returns all available for distribution
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Target peer
    /// * `requirements` - Message requirements
    ///
    /// # Returns
    ///
    /// Vector of transport IDs in priority order
    pub fn select_transports_pace(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Vec<TransportId> {
        let policy = match &self.config.default_policy {
            Some(p) => p,
            None => return Vec::new(), // No PACE policy configured
        };

        let instances = self.transport_instances.read().unwrap();
        let available_for_peer: HashSet<_> = instances
            .iter()
            .filter(|(_, (inst, transport))| {
                inst.available
                    && transport.is_available()
                    && transport.can_reach(peer_id)
                    && transport.capabilities().meets_requirements(requirements)
            })
            .map(|(id, _)| id.clone())
            .collect();

        // Get candidates in PACE order
        let candidates: Vec<TransportId> = policy
            .ordered()
            .filter(|id| available_for_peer.contains(*id))
            .cloned()
            .collect();

        // Apply transport mode
        match &self.config.transport_mode {
            TransportMode::Single => candidates.into_iter().take(1).collect(),
            TransportMode::Redundant {
                min_paths,
                max_paths,
            } => {
                let min = *min_paths as usize;
                let max = max_paths.map(|m| m as usize).unwrap_or(candidates.len());
                candidates.into_iter().take(max.max(min)).collect()
            }
            TransportMode::Bonded => candidates, // All for bandwidth aggregation
            TransportMode::LoadBalanced { .. } => candidates, // All for distribution
        }
    }

    /// Select the best single transport using PACE policy
    ///
    /// Convenience wrapper that returns just the first (best) transport.
    pub fn select_transport_pace(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Option<TransportId> {
        self.select_transports_pace(peer_id, requirements)
            .into_iter()
            .next()
    }

    /// Record successful transport use for a peer (PACE version)
    pub fn record_success_pace(&self, peer_id: &NodeId, transport_id: TransportId) {
        if self.config.cache_peer_transport {
            self.peer_transport_ids
                .write()
                .unwrap()
                .insert(peer_id.clone(), transport_id);
        }
    }

    /// Clear cached transport for a peer (PACE version)
    pub fn clear_cache_pace(&self, peer_id: &NodeId) {
        self.peer_transport_ids.write().unwrap().remove(peer_id);
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

    // =========================================================================
    // Bypass Channel Methods (ADR-042)
    // =========================================================================

    /// Send data via the UDP bypass channel
    ///
    /// Sends data directly via UDP, bypassing CRDT synchronization.
    /// Use for high-frequency, low-latency, or ephemeral data.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name (must be configured for bypass)
    /// * `data` - Raw data to send (already serialized)
    /// * `target` - Optional target for unicast; uses collection config if None
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Data sent successfully
    /// * `Err(TransportError)` - Send failed or bypass not available
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Send position update via bypass
    /// manager.send_bypass("position_updates", &position_bytes, None).await?;
    ///
    /// // Send to specific peer via unicast
    /// let target = "192.168.1.100:5150".parse().unwrap();
    /// manager.send_bypass("commands", &cmd_bytes, Some(target)).await?;
    /// ```
    pub async fn send_bypass(
        &self,
        collection: &str,
        data: &[u8],
        target: Option<SocketAddr>,
    ) -> Result<()> {
        let bypass = self
            .bypass_channel
            .as_ref()
            .ok_or_else(|| TransportError::Other("Bypass channel not configured".into()))?;

        bypass
            .read()
            .await
            .send_to_collection(collection, target, data)
            .await
            .map_err(|e| TransportError::Other(e.to_string().into()))
    }

    /// Send data via bypass channel with explicit target
    ///
    /// Lower-level method for sending to a specific target.
    ///
    /// # Arguments
    ///
    /// * `target` - Target address (unicast, multicast, or broadcast)
    /// * `collection` - Collection name for header
    /// * `data` - Raw data to send
    pub async fn send_bypass_to(
        &self,
        target: BypassTarget,
        collection: &str,
        data: &[u8],
    ) -> Result<()> {
        let bypass = self
            .bypass_channel
            .as_ref()
            .ok_or_else(|| TransportError::Other("Bypass channel not configured".into()))?;

        bypass
            .read()
            .await
            .send(target, collection, data)
            .await
            .map_err(|e| TransportError::Other(e.to_string().into()))
    }

    /// Subscribe to incoming bypass messages
    ///
    /// Returns a broadcast receiver for all incoming bypass channel messages.
    ///
    /// # Returns
    ///
    /// * `Ok(Receiver)` - Subscription successful
    /// * `Err(TransportError)` - Bypass not available
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut rx = manager.subscribe_bypass().await?;
    /// while let Ok(msg) = rx.recv().await {
    ///     println!("Bypass message from {}: {} bytes",
    ///         msg.source, msg.data.len());
    /// }
    /// ```
    pub async fn subscribe_bypass(&self) -> Result<broadcast::Receiver<BypassMessage>> {
        let bypass = self
            .bypass_channel
            .as_ref()
            .ok_or_else(|| TransportError::Other("Bypass channel not configured".into()))?;

        Ok(bypass.read().await.subscribe())
    }

    /// Subscribe to bypass messages for a specific collection
    ///
    /// Returns the collection hash and a receiver. Filter received messages
    /// by comparing `msg.collection_hash == hash`.
    ///
    /// # Arguments
    ///
    /// * `collection` - Collection name to subscribe to
    ///
    /// # Returns
    ///
    /// * `Ok((hash, Receiver))` - Subscription successful with collection hash
    /// * `Err(TransportError)` - Bypass not available
    pub async fn subscribe_bypass_collection(
        &self,
        collection: &str,
    ) -> Result<(u32, broadcast::Receiver<BypassMessage>)> {
        let bypass = self
            .bypass_channel
            .as_ref()
            .ok_or_else(|| TransportError::Other("Bypass channel not configured".into()))?;

        Ok(bypass.read().await.subscribe_collection(collection))
    }

    /// Route a message based on requirements
    ///
    /// If `requirements.bypass_sync` is `true` and bypass channel is available,
    /// returns `RouteDecision::Bypass`. Otherwise returns the selected transport.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - Target peer (ignored for bypass)
    /// * `requirements` - Message requirements
    ///
    /// # Returns
    ///
    /// Decision on how to route the message
    pub fn route_message(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> RouteDecision {
        // Check if bypass is requested and available
        if requirements.bypass_sync && self.bypass_channel.is_some() {
            return RouteDecision::Bypass;
        }
        // Fall back to normal transport if bypass not available or not requested

        // Select normal transport
        match self.select_transport(peer_id, requirements) {
            Some(transport_type) => RouteDecision::Transport(transport_type),
            None => RouteDecision::NoRoute,
        }
    }
}

/// Routing decision for a message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// Use UDP bypass channel
    Bypass,
    /// Use specified transport
    Transport(TransportType),
    /// No suitable route available
    NoRoute,
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
    use crate::transport::bypass::{BypassChannelConfig, UdpBypassChannel};
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

    // =========================================================================
    // Bypass Integration Tests (ADR-042)
    // =========================================================================

    #[tokio::test]
    async fn test_no_bypass_channel_by_default() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        assert!(!manager.has_bypass_channel());
        assert!(!manager.is_bypass_collection("test").await);
    }

    #[test]
    fn test_route_message_without_bypass() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // Normal requirements - should select transport
        let requirements = MessageRequirements::default();
        let decision = manager.route_message(&peer, &requirements);
        assert_eq!(decision, RouteDecision::Transport(TransportType::Quic));

        // Bypass requested but not available - should fall back to transport
        // Note: We use a generous latency (100ms) so QUIC (10ms) can be selected
        let bypass_req = MessageRequirements {
            bypass_sync: true,
            max_latency_ms: Some(100), // QUIC has 10ms typical latency
            ..Default::default()
        };
        let decision = manager.route_message(&peer, &bypass_req);
        // Falls back to QUIC since bypass not available
        assert_eq!(decision, RouteDecision::Transport(TransportType::Quic));
    }

    #[tokio::test]
    async fn test_subscribe_bypass_not_configured() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let result = manager.subscribe_bypass().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_route_decision_equality() {
        assert_eq!(RouteDecision::Bypass, RouteDecision::Bypass);
        assert_eq!(
            RouteDecision::Transport(TransportType::Quic),
            RouteDecision::Transport(TransportType::Quic)
        );
        assert_ne!(RouteDecision::Bypass, RouteDecision::NoRoute);
        assert_ne!(
            RouteDecision::Transport(TransportType::Quic),
            RouteDecision::Transport(TransportType::LoRa)
        );
    }

    // =========================================================================
    // PACE Instance API Tests
    // =========================================================================

    #[test]
    fn test_register_instance() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let instance = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer));

        manager.register_instance(instance, transport);

        assert!(manager.get_instance(&"iroh-eth0".to_string()).is_some());
        assert!(manager.get_instance(&"nonexistent".to_string()).is_none());
    }

    #[test]
    fn test_unregister_instance() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let instance = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()));

        manager.register_instance(instance, transport);

        let removed = manager.unregister_instance(&"iroh-eth0".to_string());
        assert!(removed.is_some());
        let (inst, _) = removed.unwrap();
        assert_eq!(inst.id, "iroh-eth0");

        // Should be gone now
        assert!(manager.get_instance(&"iroh-eth0".to_string()).is_none());

        // Unregistering again returns None
        let removed_again = manager.unregister_instance(&"iroh-eth0".to_string());
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_registered_instance_ids() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        // Empty initially
        assert!(manager.registered_instance_ids().is_empty());

        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let inst2 = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );

        manager.register_instance(
            inst1,
            Arc::new(MockTransport::new(TransportCapabilities::quic())),
        );
        manager.register_instance(
            inst2,
            Arc::new(MockTransport::new(TransportCapabilities::lora(7))),
        );

        let ids = manager.registered_instance_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"iroh-eth0".to_string()));
        assert!(ids.contains(&"lora-915".to_string()));
    }

    #[test]
    fn test_available_instance_ids() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        // Available instance
        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport1 = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register_instance(inst1, transport1);

        // Unavailable instance (transport unavailable)
        let inst2 = TransportInstance::new(
            "lora-off",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let transport2 = Arc::new(MockTransport::new(TransportCapabilities::lora(7)).unavailable());
        manager.register_instance(inst2, transport2);

        // Unavailable instance (instance.available = false)
        let mut inst3 = TransportInstance::new(
            "ble-disabled",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        );
        inst3.available = false;
        let transport3 = Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()));
        manager.register_instance(inst3, transport3);

        let available = manager.available_instance_ids();
        assert_eq!(available.len(), 1);
        assert!(available.contains("iroh-eth0"));
    }

    #[test]
    fn test_available_instances_for_peer() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Instance that can reach peer
        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, transport1);

        // Instance that cannot reach peer
        let inst2 = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let transport2 = Arc::new(MockTransport::new(TransportCapabilities::lora(7)));
        manager.register_instance(inst2, transport2);

        // Unavailable instance that could reach peer
        let inst3 = TransportInstance::new(
            "ble-off",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        );
        let transport3 = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le())
                .with_peer(peer.clone())
                .unavailable(),
        );
        manager.register_instance(inst3, transport3);

        let for_peer = manager.available_instances_for_peer(&peer);
        assert_eq!(for_peer.len(), 1);
        assert_eq!(for_peer[0], "iroh-eth0");
    }

    // =========================================================================
    // current_pace_level() Tests
    // =========================================================================

    #[test]
    fn test_current_pace_level_no_policy_with_available() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        // Register an available instance
        let inst = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register_instance(inst, transport);

        // No policy: if any transport available, returns Primary
        assert_eq!(manager.current_pace_level(), PaceLevel::Primary);
    }

    #[test]
    fn test_current_pace_level_no_policy_none_available() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        // No instances at all
        assert_eq!(manager.current_pace_level(), PaceLevel::None);
    }

    #[test]
    fn test_current_pace_level_no_policy_all_unavailable() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        // Register an unavailable instance
        let inst = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()).unavailable());
        manager.register_instance(inst, transport);

        assert_eq!(manager.current_pace_level(), PaceLevel::None);
    }

    #[test]
    fn test_current_pace_level_with_policy_primary() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0"])
            .alternate(vec!["lora-915"])
            .emergency(vec!["ble-mesh"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        // Register iroh-eth0 as available
        let inst = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register_instance(inst, transport);

        assert_eq!(manager.current_pace_level(), PaceLevel::Primary);
    }

    #[test]
    fn test_current_pace_level_with_policy_alternate() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0"])
            .alternate(vec!["lora-915"])
            .emergency(vec!["ble-mesh"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        // Only alternate is available
        let inst = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::lora(7)));
        manager.register_instance(inst, transport);

        assert_eq!(manager.current_pace_level(), PaceLevel::Alternate);
    }

    #[test]
    fn test_current_pace_level_with_policy_emergency() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0"])
            .alternate(vec!["lora-915"])
            .emergency(vec!["ble-mesh"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        // Only emergency is available
        let inst = TransportInstance::new(
            "ble-mesh",
            TransportType::BluetoothLE,
            TransportCapabilities::bluetooth_le(),
        );
        let transport = Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()));
        manager.register_instance(inst, transport);

        assert_eq!(manager.current_pace_level(), PaceLevel::Emergency);
    }

    #[test]
    fn test_current_pace_level_with_policy_none_available() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0"])
            .alternate(vec!["lora-915"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        // No instances registered
        assert_eq!(manager.current_pace_level(), PaceLevel::None);
    }

    // =========================================================================
    // select_transports_pace() Tests
    // =========================================================================

    #[test]
    fn test_select_transports_pace_no_policy() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        // No policy => empty vec
        let selected = manager.select_transports_pace(&peer, &requirements);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_select_transports_pace_single_mode() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0", "iroh-wlan0"])
            .alternate(vec!["lora-915"]);

        let config = TransportManagerConfig::with_policy(policy).with_mode(TransportMode::Single);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Register two available primary instances that can reach peer
        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        let inst2 = TransportInstance::new(
            "iroh-wlan0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // Single mode: at most 1
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], "iroh-eth0");
    }

    #[test]
    fn test_select_transports_pace_redundant_mode() {
        let policy = TransportPolicy::new("test")
            .primary(vec!["iroh-eth0", "iroh-wlan0"])
            .alternate(vec!["lora-915"]);

        let config =
            TransportManagerConfig::with_policy(policy).with_mode(TransportMode::redundant(2));
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        let inst2 = TransportInstance::new(
            "iroh-wlan0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        let inst3 = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let t3 =
            Arc::new(MockTransport::new(TransportCapabilities::lora(7)).with_peer(peer.clone()));
        manager.register_instance(inst3, t3);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // Redundant { min_paths: 2, max_paths: None } => takes max(len, min) = all 3
        assert!(selected.len() >= 2);
    }

    #[test]
    fn test_select_transports_pace_redundant_bounded() {
        let policy = TransportPolicy::new("test").primary(vec!["t1", "t2", "t3", "t4"]);

        let config = TransportManagerConfig::with_policy(policy)
            .with_mode(TransportMode::redundant_bounded(1, 2));
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Register 4 instances
        for name in &["t1", "t2", "t3", "t4"] {
            let inst =
                TransportInstance::new(*name, TransportType::Quic, TransportCapabilities::quic());
            let t =
                Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
            manager.register_instance(inst, t);
        }

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // Redundant { min_paths: 1, max_paths: Some(2) } => takes max(2, 1) = 2
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_transports_pace_bonded_mode() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0", "iroh-wlan0"]);

        let config = TransportManagerConfig::with_policy(policy).with_mode(TransportMode::Bonded);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        let inst2 = TransportInstance::new(
            "iroh-wlan0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // Bonded: returns all candidates
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_transports_pace_load_balanced_mode() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0", "iroh-wlan0"]);

        let config = TransportManagerConfig::with_policy(policy)
            .with_mode(TransportMode::LoadBalanced { weights: None });
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        let inst2 = TransportInstance::new(
            "iroh-wlan0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // LoadBalanced: returns all candidates
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_select_transports_pace_filters_by_requirements() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0", "lora-915"]);

        let config = TransportManagerConfig::with_policy(policy).with_mode(TransportMode::Bonded);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC is reliable
        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        // LoRa is NOT reliable
        let inst2 = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::lora(7)).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        // Require reliability => should filter out LoRa
        let requirements = MessageRequirements {
            reliable: true,
            ..Default::default()
        };
        let selected = manager.select_transports_pace(&peer, &requirements);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], "iroh-eth0");
    }

    #[test]
    fn test_select_transports_pace_filters_unreachable_peer() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0", "lora-915"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Can reach peer
        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        // Cannot reach peer
        let inst2 = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let t2 = Arc::new(MockTransport::new(TransportCapabilities::lora(7)));
        manager.register_instance(inst2, t2);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], "iroh-eth0");
    }

    // =========================================================================
    // select_transport_pace() Tests
    // =========================================================================

    #[test]
    fn test_select_transport_pace_returns_first() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0", "iroh-wlan0"]);

        let config = TransportManagerConfig::with_policy(policy).with_mode(TransportMode::Bonded);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        let inst1 = TransportInstance::new(
            "iroh-eth0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t1 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst1, t1);

        let inst2 = TransportInstance::new(
            "iroh-wlan0",
            TransportType::Quic,
            TransportCapabilities::quic(),
        );
        let t2 =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register_instance(inst2, t2);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transport_pace(&peer, &requirements);

        assert_eq!(selected, Some("iroh-eth0".to_string()));
    }

    #[test]
    fn test_select_transport_pace_returns_none_no_policy() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        assert_eq!(manager.select_transport_pace(&peer, &requirements), None);
    }

    #[test]
    fn test_select_transport_pace_returns_none_no_candidates() {
        let policy = TransportPolicy::new("test").primary(vec!["iroh-eth0"]);

        let config = TransportManagerConfig::with_policy(policy);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        // No instances registered
        assert_eq!(manager.select_transport_pace(&peer, &requirements), None);
    }

    // =========================================================================
    // record_success_pace() and clear_cache_pace() Tests
    // =========================================================================

    #[test]
    fn test_record_success_pace_caching_enabled() {
        let config = TransportManagerConfig {
            cache_peer_transport: true,
            ..Default::default()
        };
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        manager.record_success_pace(&peer, "iroh-eth0".to_string());

        let cached = manager.peer_transport_ids.read().unwrap();
        assert_eq!(cached.get(&peer), Some(&"iroh-eth0".to_string()));
    }

    #[test]
    fn test_record_success_pace_caching_disabled() {
        let config = TransportManagerConfig {
            cache_peer_transport: false,
            ..Default::default()
        };
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        manager.record_success_pace(&peer, "iroh-eth0".to_string());

        let cached = manager.peer_transport_ids.read().unwrap();
        assert!(cached.get(&peer).is_none());
    }

    #[test]
    fn test_clear_cache_pace() {
        let config = TransportManagerConfig {
            cache_peer_transport: true,
            ..Default::default()
        };
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        manager.record_success_pace(&peer, "iroh-eth0".to_string());

        // Verify it's cached
        assert!(manager
            .peer_transport_ids
            .read()
            .unwrap()
            .get(&peer)
            .is_some());

        manager.clear_cache_pace(&peer);

        // Verify it's cleared
        assert!(manager
            .peer_transport_ids
            .read()
            .unwrap()
            .get(&peer)
            .is_none());
    }

    #[test]
    fn test_clear_cache_pace_nonexistent_peer() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("nonexistent".to_string());

        // Should not panic
        manager.clear_cache_pace(&peer);
    }

    // =========================================================================
    // select_transport_for_distance() Tests
    // =========================================================================

    #[test]
    fn test_select_transport_for_distance_no_distance() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        let requirements = MessageRequirements::default();
        let result = manager.select_transport_for_distance(&peer, &requirements);

        assert!(result.is_some());
        let (transport_type, range_mode) = result.unwrap();
        assert_eq!(transport_type, TransportType::Quic);
        assert!(range_mode.is_none());
    }

    #[test]
    fn test_select_transport_for_distance_with_distance() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // Set distance for peer
        let distance = PeerDistance {
            peer_id: peer.clone(),
            distance_meters: 1000,
            source: super::super::capabilities::DistanceSource::Configured,
            last_updated: Instant::now(),
        };
        manager.update_peer_distance(distance);

        let requirements = MessageRequirements::default();
        let result = manager.select_transport_for_distance(&peer, &requirements);

        assert!(result.is_some());
        let (transport_type, range_mode) = result.unwrap();
        assert_eq!(transport_type, TransportType::Quic);
        // Range mode is None because placeholder logic doesn't do runtime downcasting
        assert!(range_mode.is_none());
    }

    #[test]
    fn test_select_transport_for_distance_no_suitable_transport() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        let result = manager.select_transport_for_distance(&peer, &requirements);
        assert!(result.is_none());
    }

    // =========================================================================
    // TransportManagerConfig builder Tests
    // =========================================================================

    #[test]
    fn test_config_with_policy() {
        let policy = TransportPolicy::new("tactical")
            .primary(vec!["iroh-eth0"])
            .alternate(vec!["lora-915"]);

        let config = TransportManagerConfig::with_policy(policy);

        assert!(config.default_policy.is_some());
        let p = config.default_policy.unwrap();
        assert_eq!(p.name, "tactical");
        assert_eq!(p.primary.len(), 1);
        assert_eq!(p.alternate.len(), 1);
        // Verify defaults are preserved
        assert!(config.enable_fallback);
        assert!(config.cache_peer_transport);
        assert_eq!(config.switch_threshold, 10);
        assert!(matches!(config.transport_mode, TransportMode::Single));
    }

    #[test]
    fn test_config_with_mode() {
        let config = TransportManagerConfig::default().with_mode(TransportMode::Bonded);

        assert!(matches!(config.transport_mode, TransportMode::Bonded));
    }

    #[test]
    fn test_config_with_policy_and_mode_chained() {
        let policy = TransportPolicy::new("test").primary(vec!["t1"]);
        let config =
            TransportManagerConfig::with_policy(policy).with_mode(TransportMode::redundant(3));

        assert!(config.default_policy.is_some());
        assert!(matches!(
            config.transport_mode,
            TransportMode::Redundant {
                min_paths: 3,
                max_paths: None
            }
        ));
    }

    // =========================================================================
    // connect() error paths Tests
    // =========================================================================

    #[tokio::test]
    async fn test_connect_no_suitable_transport() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        let result = manager.connect(&peer, &requirements).await;
        assert!(result.is_err());
        match result {
            Err(TransportError::PeerNotFound(_)) => {} // expected
            Err(other) => panic!("Expected PeerNotFound, got: {}", other),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[tokio::test]
    async fn test_connect_unreachable_peer() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        // Register QUIC but the peer is not in reachable_peers
        let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register(quic);

        let peer = NodeId::new("unreachable-peer".to_string());
        let requirements = MessageRequirements::default();

        let result = manager.connect(&peer, &requirements).await;
        assert!(result.is_err());
    }

    // =========================================================================
    // connect_with_fallback() Tests
    // =========================================================================

    #[tokio::test]
    async fn test_connect_with_fallback_disabled() {
        let config = TransportManagerConfig {
            enable_fallback: false,
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // QUIC registered but can't reach peer (will fail connect)
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // BLE also available
        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        // Both can reach, both will succeed, so first should succeed.
        // Let's test the error path where the first fails:
        // We need a transport that can reach but fails to connect.
        // The MockTransport connects if peer is in reachable_peers.
        // Actually, both will succeed, so let's just test with no reachable transports.

        let peer_unreachable = NodeId::new("nobody".to_string());
        let requirements = MessageRequirements::default();

        let result = manager
            .connect_with_fallback(&peer_unreachable, &requirements)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_with_fallback_no_candidates() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        let result = manager.connect_with_fallback(&peer, &requirements).await;
        assert!(result.is_err());
        match result {
            Err(ref e) => {
                let err_msg = format!("{}", e);
                assert!(err_msg.contains("No suitable transport"));
            }
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    // =========================================================================
    // route_message() NoRoute Tests
    // =========================================================================

    #[test]
    fn test_route_message_no_route() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements::default();

        // No transports registered => NoRoute
        let decision = manager.route_message(&peer, &requirements);
        assert_eq!(decision, RouteDecision::NoRoute);
    }

    #[test]
    fn test_route_message_bypass_requested_no_channel() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        let requirements = MessageRequirements {
            bypass_sync: true,
            ..Default::default()
        };

        // bypass requested but no channel and no transports => NoRoute
        let decision = manager.route_message(&peer, &requirements);
        assert_eq!(decision, RouteDecision::NoRoute);
    }

    // =========================================================================
    // RouteDecision construction Tests
    // =========================================================================

    #[test]
    fn test_route_decision_no_route() {
        let decision = RouteDecision::NoRoute;
        assert_eq!(decision, RouteDecision::NoRoute);
        assert_ne!(decision, RouteDecision::Bypass);
        assert_ne!(decision, RouteDecision::Transport(TransportType::Quic));
    }

    #[test]
    fn test_route_decision_debug() {
        let bypass = RouteDecision::Bypass;
        let transport = RouteDecision::Transport(TransportType::LoRa);
        let no_route = RouteDecision::NoRoute;

        assert!(format!("{:?}", bypass).contains("Bypass"));
        assert!(format!("{:?}", transport).contains("LoRa"));
        assert!(format!("{:?}", no_route).contains("NoRoute"));
    }

    #[test]
    fn test_route_decision_clone() {
        let original = RouteDecision::Transport(TransportType::BluetoothLE);
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    // =========================================================================
    // TransportManager Debug and misc Tests
    // =========================================================================

    #[test]
    fn test_transport_manager_debug() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        manager.register(quic);

        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("TransportManager"));
        assert!(debug_str.contains("Quic"));
    }

    #[test]
    fn test_registered_transports() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        assert!(manager.registered_transports().is_empty());

        let quic = Arc::new(MockTransport::new(TransportCapabilities::quic()));
        let ble = Arc::new(MockTransport::new(TransportCapabilities::bluetooth_le()));
        manager.register(quic);
        manager.register(ble);

        let registered = manager.registered_transports();
        assert_eq!(registered.len(), 2);
        assert!(registered.contains(&TransportType::Quic));
        assert!(registered.contains(&TransportType::BluetoothLE));
    }

    #[tokio::test]
    async fn test_set_bypass_channel() {
        let config = TransportManagerConfig::default();
        let mut manager = TransportManager::new(config);

        assert!(!manager.has_bypass_channel());

        let bypass_config = BypassChannelConfig::new();
        let bypass = UdpBypassChannel::new(bypass_config).await.unwrap();
        manager.set_bypass_channel(bypass);

        assert!(manager.has_bypass_channel());
    }

    #[test]
    fn test_record_success_caching_disabled() {
        let config = TransportManagerConfig {
            cache_peer_transport: false,
            ..Default::default()
        };
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());
        manager.record_success(&peer, TransportType::Quic);

        // Cache should be empty since caching is disabled
        let cached = manager.peer_transports.read().unwrap();
        assert!(cached.get(&peer).is_none());
    }

    #[test]
    fn test_select_transport_cached_transport_invalid() {
        let config = TransportManagerConfig {
            cache_peer_transport: true,
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Register BLE that is available and can reach peer
        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le()).with_peer(peer.clone()),
        );
        manager.register(ble);

        // Cache a transport type that is NOT registered (e.g., LoRa)
        manager.record_success(&peer, TransportType::LoRa);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transport(&peer, &requirements);

        // Should fall through cached transport (LoRa not registered) and select BLE
        assert_eq!(selected, Some(TransportType::BluetoothLE));
    }

    #[test]
    fn test_select_transport_cached_transport_unavailable() {
        let config = TransportManagerConfig {
            cache_peer_transport: true,
            ..Default::default()
        };
        let mut manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Register QUIC that is available
        let quic =
            Arc::new(MockTransport::new(TransportCapabilities::quic()).with_peer(peer.clone()));
        manager.register(quic);

        // Register BLE that is unavailable
        let ble = Arc::new(
            MockTransport::new(TransportCapabilities::bluetooth_le())
                .with_peer(peer.clone())
                .unavailable(),
        );
        manager.register(ble);

        // Cache BLE (which is unavailable)
        manager.record_success(&peer, TransportType::BluetoothLE);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transport(&peer, &requirements);

        // Should fall through cached BLE (unavailable) and select QUIC
        assert_eq!(selected, Some(TransportType::Quic));
    }

    #[test]
    fn test_pace_fallback_order() {
        // Test that PACE selection follows policy order when primary fails
        let policy = TransportPolicy::new("test")
            .primary(vec!["dead-transport"])
            .alternate(vec!["lora-915"]);

        let config = TransportManagerConfig::with_policy(policy).with_mode(TransportMode::Single);
        let manager = TransportManager::new(config);

        let peer = NodeId::new("peer-1".to_string());

        // Only register the alternate (primary is not registered)
        let inst = TransportInstance::new(
            "lora-915",
            TransportType::LoRa,
            TransportCapabilities::lora(7),
        );
        let t =
            Arc::new(MockTransport::new(TransportCapabilities::lora(7)).with_peer(peer.clone()));
        manager.register_instance(inst, t);

        let requirements = MessageRequirements::default();
        let selected = manager.select_transports_pace(&peer, &requirements);

        // Should fall back to alternate
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], "lora-915");
    }

    #[test]
    fn test_get_peer_distance_none() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let peer = NodeId::new("unknown-peer".to_string());
        assert!(manager.get_peer_distance(&peer).is_none());
    }

    #[tokio::test]
    async fn test_send_bypass_not_configured() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let result = manager.send_bypass("test_collection", b"hello", None).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Bypass channel not configured"));
    }

    #[tokio::test]
    async fn test_send_bypass_to_not_configured() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let target = BypassTarget::Broadcast { port: 5150 };
        let result = manager
            .send_bypass_to(target, "test_collection", b"hello")
            .await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Bypass channel not configured"));
    }

    #[tokio::test]
    async fn test_subscribe_bypass_collection_not_configured() {
        let config = TransportManagerConfig::default();
        let manager = TransportManager::new(config);

        let result = manager.subscribe_bypass_collection("test").await;
        assert!(result.is_err());
    }
}
