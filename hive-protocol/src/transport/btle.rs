//! Bluetooth LE Transport Adapter
//!
//! This module provides integration between hive-btle and hive-protocol's
//! transport abstraction. It wraps `hive_btle::BluetoothLETransport` and
//! implements the `MeshTransport` and `Transport` traits from ADR-032.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   hive-protocol                              │
//! │  ┌─────────────────────────────────────────────────────────┐ │
//! │  │          HiveBleTransport (this module)                 │ │
//! │  │   Implements: MeshTransport, Transport                  │ │
//! │  └───────────────────────┬─────────────────────────────────┘ │
//! │                          │ wraps                             │
//! │  ┌───────────────────────▼─────────────────────────────────┐ │
//! │  │       hive_btle::BluetoothLETransport<A>                │ │
//! │  │   Implements: hive_btle::MeshTransport                  │ │
//! │  └─────────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_protocol::transport::btle::HiveBleTransport;
//! use hive_btle::{BleConfig, BluetoothLETransport, platform::StubAdapter};
//!
//! // Create hive-btle transport
//! let config = BleConfig::default();
//! let adapter = StubAdapter::default();
//! let btle = BluetoothLETransport::new(config, adapter);
//!
//! // Wrap in adapter for hive-protocol
//! let transport = HiveBleTransport::new(btle);
//!
//! // Register with TransportManager
//! manager.register(Arc::new(transport));
//! ```

use async_trait::async_trait;
use hive_btle::platform::BleAdapter;
use hive_btle::{BluetoothLETransport, MeshTransport as BtleMeshTransport};
use std::collections::HashSet;
use std::sync::RwLock;
use std::time::Instant;
use tokio::sync::mpsc;

use super::capabilities::{Transport, TransportCapabilities, TransportType};
use super::{
    ConnectionHealth, ConnectionState, DisconnectReason, MeshConnection, MeshTransport, NodeId,
    PeerEvent, PeerEventReceiver, Result, TransportError, PEER_EVENT_CHANNEL_CAPACITY,
};

// =============================================================================
// NodeId Conversion
// =============================================================================

/// Convert hive-btle NodeId (u32) to hive-protocol NodeId (String)
fn btle_to_hive_node_id(btle_id: &hive_btle::NodeId) -> NodeId {
    NodeId::new(format!("{:08X}", btle_id.as_u32()))
}

/// Convert hive-protocol NodeId (String) to hive-btle NodeId (u32)
fn hive_to_btle_node_id(hive_id: &NodeId) -> Option<hive_btle::NodeId> {
    let s = hive_id
        .as_str()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    u32::from_str_radix(s, 16).ok().map(hive_btle::NodeId::new)
}

// =============================================================================
// Connection Adapter
// =============================================================================

/// Adapter for hive-btle connections
struct BleConnectionAdapter {
    /// Original connection from hive-btle
    inner: Box<dyn hive_btle::BleConnection>,
    /// Converted node ID
    peer_id: NodeId,
    /// When the connection was established
    connected_at: Instant,
}

impl MeshConnection for BleConnectionAdapter {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    fn connected_at(&self) -> Instant {
        self.connected_at
    }

    fn disconnect_reason(&self) -> Option<DisconnectReason> {
        if self.is_alive() {
            None
        } else {
            Some(DisconnectReason::Unknown)
        }
    }
}

// =============================================================================
// Transport Adapter
// =============================================================================

/// Bluetooth LE transport adapter for hive-protocol
///
/// Wraps `hive_btle::BluetoothLETransport` and implements the transport
/// abstraction from ADR-032.
pub struct HiveBleTransport<A: BleAdapter> {
    /// Wrapped hive-btle transport
    inner: BluetoothLETransport<A>,
    /// Converted capabilities
    capabilities: TransportCapabilities,
    /// Known reachable peers (discovered via BLE)
    reachable_peers: RwLock<HashSet<NodeId>>,
    /// Peer event senders
    event_senders: RwLock<Vec<mpsc::Sender<PeerEvent>>>,
    /// Start time for tracking
    started: RwLock<Option<Instant>>,
}

impl<A: BleAdapter + Send + Sync + 'static> HiveBleTransport<A> {
    /// Create a new BLE transport adapter
    pub fn new(inner: BluetoothLETransport<A>) -> Self {
        let btle_caps = inner.capabilities();
        let capabilities = Self::convert_capabilities(btle_caps);

        Self {
            inner,
            capabilities,
            reachable_peers: RwLock::new(HashSet::new()),
            event_senders: RwLock::new(Vec::new()),
            started: RwLock::new(None),
        }
    }

    /// Convert hive-btle capabilities to hive-protocol capabilities
    fn convert_capabilities(btle: &hive_btle::TransportCapabilities) -> TransportCapabilities {
        TransportCapabilities {
            transport_type: TransportType::BluetoothLE,
            max_bandwidth_bps: btle.max_bandwidth_bps,
            typical_latency_ms: btle.typical_latency_ms,
            max_range_meters: btle.max_range_meters,
            bidirectional: btle.bidirectional,
            reliable: btle.reliable,
            battery_impact: btle.battery_impact,
            supports_broadcast: btle.supports_broadcast,
            requires_pairing: btle.requires_pairing,
            max_message_size: btle.max_message_size,
        }
    }

    /// Mark a peer as reachable (discovered via BLE scan)
    pub fn add_reachable_peer(&self, peer_id: NodeId) {
        self.reachable_peers.write().unwrap().insert(peer_id);
    }

    /// Remove a peer from reachable set
    pub fn remove_reachable_peer(&self, peer_id: &NodeId) {
        self.reachable_peers.write().unwrap().remove(peer_id);
    }

    /// Emit a peer event to all subscribers
    fn emit_event(&self, event: PeerEvent) {
        let senders = self.event_senders.read().unwrap();
        for sender in senders.iter() {
            let _ = sender.try_send(event.clone());
        }
    }

    /// Get the underlying hive-btle transport
    pub fn inner(&self) -> &BluetoothLETransport<A> {
        &self.inner
    }

    /// Get the node ID of this transport
    pub fn node_id(&self) -> NodeId {
        btle_to_hive_node_id(self.inner.node_id())
    }
}

#[async_trait]
impl<A: BleAdapter + Send + Sync + 'static> MeshTransport for HiveBleTransport<A> {
    async fn start(&self) -> Result<()> {
        self.inner
            .start()
            .await
            .map_err(|e| TransportError::Other(e.to_string().into()))?;
        *self.started.write().unwrap() = Some(Instant::now());
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.inner
            .stop()
            .await
            .map_err(|e| TransportError::Other(e.to_string().into()))?;
        *self.started.write().unwrap() = None;
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        let btle_peer_id = hive_to_btle_node_id(peer_id).ok_or_else(|| {
            TransportError::PeerNotFound(format!("Invalid NodeId format: {}", peer_id))
        })?;

        let conn = self
            .inner
            .connect(&btle_peer_id)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let connected_at = Instant::now();

        // Emit connected event
        self.emit_event(PeerEvent::Connected {
            peer_id: peer_id.clone(),
            connected_at,
        });

        Ok(Box::new(BleConnectionAdapter {
            inner: conn,
            peer_id: peer_id.clone(),
            connected_at,
        }))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let btle_peer_id = hive_to_btle_node_id(peer_id).ok_or_else(|| {
            TransportError::PeerNotFound(format!("Invalid NodeId format: {}", peer_id))
        })?;

        // Get connection duration before disconnecting
        let connection_duration = if let Some(conn) = self.get_connection(peer_id) {
            conn.connected_at().elapsed()
        } else {
            std::time::Duration::ZERO
        };

        self.inner
            .disconnect(&btle_peer_id)
            .await
            .map_err(|e| TransportError::Other(e.to_string().into()))?;

        // Emit disconnected event
        self.emit_event(PeerEvent::Disconnected {
            peer_id: peer_id.clone(),
            reason: DisconnectReason::LocalClosed,
            connection_duration,
        });

        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        let btle_peer_id = hive_to_btle_node_id(peer_id)?;
        let conn = self.inner.get_connection(&btle_peer_id)?;

        Some(Box::new(BleConnectionAdapter {
            inner: conn,
            peer_id: peer_id.clone(),
            connected_at: Instant::now(), // Approximation - we don't track exact time
        }))
    }

    fn peer_count(&self) -> usize {
        self.inner.peer_count()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.inner
            .connected_peers()
            .iter()
            .map(btle_to_hive_node_id)
            .collect()
    }

    fn subscribe_peer_events(&self) -> PeerEventReceiver {
        let (tx, rx) = mpsc::channel(PEER_EVENT_CHANNEL_CAPACITY);
        self.event_senders.write().unwrap().push(tx);
        rx
    }

    fn get_peer_health(&self, peer_id: &NodeId) -> Option<ConnectionHealth> {
        let conn = self.get_connection(peer_id)?;
        if conn.is_alive() {
            Some(ConnectionHealth {
                rtt_ms: self.capabilities.typical_latency_ms,
                rtt_variance_ms: 10,
                packet_loss_percent: 0,
                state: ConnectionState::Healthy,
                last_activity: Instant::now(),
            })
        } else {
            Some(ConnectionHealth {
                state: ConnectionState::Dead,
                ..Default::default()
            })
        }
    }
}

impl<A: BleAdapter + Send + Sync + 'static> Transport for HiveBleTransport<A> {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    fn is_available(&self) -> bool {
        self.started.read().unwrap().is_some()
    }

    fn signal_quality(&self) -> Option<u8> {
        // Could be enhanced to return average RSSI across connections
        Some(75) // Default placeholder
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        // Check if peer was discovered or is connected
        self.reachable_peers.read().unwrap().contains(peer_id) || self.is_connected(peer_id)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use hive_btle::platform::StubAdapter;
    use hive_btle::BleConfig;

    fn create_test_transport() -> HiveBleTransport<StubAdapter> {
        let config = BleConfig::default();
        let adapter = StubAdapter::default();
        let btle = BluetoothLETransport::new(config, adapter);
        HiveBleTransport::new(btle)
    }

    #[test]
    fn test_node_id_conversion_roundtrip() {
        let btle_id = hive_btle::NodeId::new(0x12345678);
        let hive_id = btle_to_hive_node_id(&btle_id);
        assert_eq!(hive_id.as_str(), "12345678");

        let back = hive_to_btle_node_id(&hive_id).unwrap();
        assert_eq!(back.as_u32(), 0x12345678);
    }

    #[test]
    fn test_node_id_conversion_with_prefix() {
        let hive_id = NodeId::new("0x12345678".to_string());
        let btle_id = hive_to_btle_node_id(&hive_id).unwrap();
        assert_eq!(btle_id.as_u32(), 0x12345678);
    }

    #[test]
    fn test_node_id_conversion_lowercase() {
        let hive_id = NodeId::new("abcdef12".to_string());
        let btle_id = hive_to_btle_node_id(&hive_id).unwrap();
        assert_eq!(btle_id.as_u32(), 0xABCDEF12);
    }

    #[test]
    fn test_node_id_conversion_invalid() {
        let hive_id = NodeId::new("not_hex".to_string());
        assert!(hive_to_btle_node_id(&hive_id).is_none());
    }

    #[test]
    fn test_capabilities_conversion() {
        let transport = create_test_transport();
        let caps = transport.capabilities();

        assert_eq!(caps.transport_type, TransportType::BluetoothLE);
        assert!(caps.max_bandwidth_bps > 0);
        assert!(caps.max_range_meters > 0);
    }

    #[test]
    fn test_reachable_peers() {
        let transport = create_test_transport();
        let peer = NodeId::new("12345678".to_string());

        assert!(!transport.can_reach(&peer));

        transport.add_reachable_peer(peer.clone());
        assert!(transport.can_reach(&peer));

        transport.remove_reachable_peer(&peer);
        assert!(!transport.can_reach(&peer));
    }

    #[test]
    fn test_not_available_before_start() {
        let transport = create_test_transport();
        assert!(!transport.is_available());
    }

    #[tokio::test]
    async fn test_available_after_start() {
        let transport = create_test_transport();
        transport.start().await.unwrap();
        assert!(transport.is_available());

        transport.stop().await.unwrap();
        assert!(!transport.is_available());
    }

    #[test]
    fn test_peer_count_initially_zero() {
        let transport = create_test_transport();
        assert_eq!(transport.peer_count(), 0);
    }

    #[test]
    fn test_connected_peers_initially_empty() {
        let transport = create_test_transport();
        assert!(transport.connected_peers().is_empty());
    }

    #[test]
    fn test_node_id() {
        let config = hive_btle::BleConfig::default();
        let adapter = StubAdapter::default();
        let btle = BluetoothLETransport::new(config.clone(), adapter);
        let transport = HiveBleTransport::new(btle);

        let node_id = transport.node_id();
        let expected = format!("{:08X}", config.node_id.as_u32());
        assert_eq!(node_id.as_str(), expected);
    }
}
