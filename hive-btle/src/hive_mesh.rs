//! HiveMesh - Unified mesh management facade
//!
//! This module provides the main entry point for HIVE BLE mesh operations.
//! It composes peer management, document sync, and observer notifications
//! into a single interface that platform implementations can use.
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::hive_mesh::{HiveMesh, HiveMeshConfig};
//! use hive_btle::observer::{HiveEvent, HiveObserver};
//! use hive_btle::NodeId;
//! use std::sync::Arc;
//!
//! // Create mesh configuration
//! let config = HiveMeshConfig::new(NodeId::new(0x12345678), "ALPHA-1", "DEMO");
//!
//! // Create mesh instance
//! let mesh = HiveMesh::new(config);
//!
//! // Add observer for events
//! struct MyObserver;
//! impl HiveObserver for MyObserver {
//!     fn on_event(&self, event: HiveEvent) {
//!         println!("Event: {:?}", event);
//!     }
//! }
//! mesh.add_observer(Arc::new(MyObserver));
//!
//! // Platform BLE callbacks
//! mesh.on_ble_discovered("device-uuid", Some("HIVE_DEMO-AABBCCDD"), -65, Some("DEMO"), now_ms);
//! mesh.on_ble_connected("device-uuid", now_ms);
//! mesh.on_ble_data_received("device-uuid", &data, now_ms);
//!
//! // Periodic maintenance
//! if let Some(sync_data) = mesh.tick(now_ms) {
//!     // Broadcast sync_data to connected peers
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::{string::String, sync::Arc, vec::Vec};
#[cfg(feature = "std")]
use std::sync::Arc;

use crate::document_sync::DocumentSync;
use crate::observer::{DisconnectReason, HiveEvent, HiveObserver};
use crate::peer::{HivePeer, PeerManagerConfig};
use crate::peer_manager::PeerManager;
use crate::sync::crdt::{EventType, PeripheralType};
use crate::NodeId;

#[cfg(feature = "std")]
use crate::observer::ObserverManager;

/// Configuration for HiveMesh
#[derive(Debug, Clone)]
pub struct HiveMeshConfig {
    /// Our node ID
    pub node_id: NodeId,

    /// Our callsign (e.g., "ALPHA-1")
    pub callsign: String,

    /// Mesh ID to filter peers (e.g., "DEMO")
    pub mesh_id: String,

    /// Peripheral type for this device
    pub peripheral_type: PeripheralType,

    /// Peer management configuration
    pub peer_config: PeerManagerConfig,

    /// Sync interval in milliseconds (how often to broadcast state)
    pub sync_interval_ms: u64,

    /// Whether to auto-broadcast on emergency/ack
    pub auto_broadcast_events: bool,
}

impl HiveMeshConfig {
    /// Create a new configuration with required fields
    pub fn new(node_id: NodeId, callsign: &str, mesh_id: &str) -> Self {
        Self {
            node_id,
            callsign: callsign.into(),
            mesh_id: mesh_id.into(),
            peripheral_type: PeripheralType::SoldierSensor,
            peer_config: PeerManagerConfig::with_mesh_id(mesh_id),
            sync_interval_ms: 5000,
            auto_broadcast_events: true,
        }
    }

    /// Set peripheral type
    pub fn with_peripheral_type(mut self, ptype: PeripheralType) -> Self {
        self.peripheral_type = ptype;
        self
    }

    /// Set sync interval
    pub fn with_sync_interval(mut self, interval_ms: u64) -> Self {
        self.sync_interval_ms = interval_ms;
        self
    }

    /// Set peer timeout
    pub fn with_peer_timeout(mut self, timeout_ms: u64) -> Self {
        self.peer_config.peer_timeout_ms = timeout_ms;
        self
    }

    /// Set max peers (for embedded systems)
    pub fn with_max_peers(mut self, max: usize) -> Self {
        self.peer_config.max_peers = max;
        self
    }
}

/// Main facade for HIVE BLE mesh operations
///
/// Composes peer management, document sync, and observer notifications.
/// Platform implementations call into this from their BLE callbacks.
#[cfg(feature = "std")]
pub struct HiveMesh {
    /// Configuration
    config: HiveMeshConfig,

    /// Peer manager
    peer_manager: PeerManager,

    /// Document sync
    document_sync: DocumentSync,

    /// Observer manager
    observers: ObserverManager,

    /// Last sync broadcast time
    last_sync_ms: std::sync::atomic::AtomicU64,

    /// Last cleanup time
    last_cleanup_ms: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "std")]
impl HiveMesh {
    /// Create a new HiveMesh instance
    pub fn new(config: HiveMeshConfig) -> Self {
        let peer_manager = PeerManager::new(config.node_id, config.peer_config.clone());
        let document_sync = DocumentSync::with_peripheral_type(
            config.node_id,
            &config.callsign,
            config.peripheral_type,
        );

        Self {
            config,
            peer_manager,
            document_sync,
            observers: ObserverManager::new(),
            last_sync_ms: std::sync::atomic::AtomicU64::new(0),
            last_cleanup_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    // ==================== Configuration ====================

    /// Get our node ID
    pub fn node_id(&self) -> NodeId {
        self.config.node_id
    }

    /// Get our callsign
    pub fn callsign(&self) -> &str {
        &self.config.callsign
    }

    /// Get the mesh ID
    pub fn mesh_id(&self) -> &str {
        &self.config.mesh_id
    }

    /// Get the device name for BLE advertising
    pub fn device_name(&self) -> String {
        format!(
            "HIVE_{}-{:08X}",
            self.config.mesh_id,
            self.config.node_id.as_u32()
        )
    }

    // ==================== Observer Management ====================

    /// Add an observer for mesh events
    pub fn add_observer(&self, observer: Arc<dyn HiveObserver>) {
        self.observers.add(observer);
    }

    /// Remove an observer
    pub fn remove_observer(&self, observer: &Arc<dyn HiveObserver>) {
        self.observers.remove(observer);
    }

    // ==================== User Actions ====================

    /// Send an emergency alert
    ///
    /// Returns the document bytes to broadcast to all peers.
    pub fn send_emergency(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_emergency(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        data
    }

    /// Send an ACK response
    ///
    /// Returns the document bytes to broadcast to all peers.
    pub fn send_ack(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_ack(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        data
    }

    /// Clear the current event (emergency or ack)
    pub fn clear_event(&self) {
        self.document_sync.clear_event();
    }

    /// Check if emergency is active
    pub fn is_emergency_active(&self) -> bool {
        self.document_sync.is_emergency_active()
    }

    /// Check if ACK is active
    pub fn is_ack_active(&self) -> bool {
        self.document_sync.is_ack_active()
    }

    /// Get current event type
    pub fn current_event(&self) -> Option<EventType> {
        self.document_sync.current_event()
    }

    // ==================== BLE Callbacks (Platform -> Mesh) ====================

    /// Called when a BLE device is discovered
    ///
    /// Returns `Some(HivePeer)` if this is a new HIVE peer on our mesh.
    pub fn on_ble_discovered(
        &self,
        identifier: &str,
        name: Option<&str>,
        rssi: i8,
        mesh_id: Option<&str>,
        now_ms: u64,
    ) -> Option<HivePeer> {
        let (node_id, is_new) = self
            .peer_manager
            .on_discovered(identifier, name, rssi, mesh_id, now_ms)?;

        let peer = self.peer_manager.get_peer(node_id)?;

        if is_new {
            self.notify(HiveEvent::PeerDiscovered { peer: peer.clone() });
            self.notify_mesh_state_changed();
        }

        Some(peer)
    }

    /// Called when a BLE connection is established (outgoing)
    ///
    /// Returns the NodeId if this identifier is known.
    pub fn on_ble_connected(&self, identifier: &str, now_ms: u64) -> Option<NodeId> {
        let node_id = self.peer_manager.on_connected(identifier, now_ms)?;
        self.notify(HiveEvent::PeerConnected { node_id });
        self.notify_mesh_state_changed();
        Some(node_id)
    }

    /// Called when a BLE connection is lost
    pub fn on_ble_disconnected(
        &self,
        identifier: &str,
        reason: DisconnectReason,
    ) -> Option<NodeId> {
        let (node_id, reason) = self.peer_manager.on_disconnected(identifier, reason)?;
        self.notify(HiveEvent::PeerDisconnected { node_id, reason });
        self.notify_mesh_state_changed();
        Some(node_id)
    }

    /// Called when a remote device connects to us (incoming connection)
    ///
    /// Use this when we're acting as a peripheral and a central connects to us.
    pub fn on_incoming_connection(&self, identifier: &str, node_id: NodeId, now_ms: u64) -> bool {
        let is_new = self
            .peer_manager
            .on_incoming_connection(identifier, node_id, now_ms);

        if is_new {
            if let Some(peer) = self.peer_manager.get_peer(node_id) {
                self.notify(HiveEvent::PeerDiscovered { peer });
            }
        }

        self.notify(HiveEvent::PeerConnected { node_id });
        self.notify_mesh_state_changed();

        is_new
    }

    /// Called when data is received from a peer
    ///
    /// Parses the document, merges it, and generates appropriate events.
    /// Returns the source NodeId and whether the document contained an event.
    pub fn on_ble_data_received(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Get node ID from identifier
        let node_id = self.peer_manager.get_node_id(identifier)?;

        // Merge the document
        let result = self.document_sync.merge_document(data)?;

        // Record sync
        self.peer_manager.record_sync(node_id, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            total_count: result.total_count,
        })
    }

    /// Called when data is received but we don't have the identifier mapped
    ///
    /// Use this when receiving data from a peripheral we discovered.
    pub fn on_ble_data_received_from_node(
        &self,
        node_id: NodeId,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Merge the document
        let result = self.document_sync.merge_document(data)?;

        // Record sync
        self.peer_manager.record_sync(node_id, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            total_count: result.total_count,
        })
    }

    /// Called when data is received without a known identifier
    ///
    /// This is the simplest data receive method - it extracts the source node_id
    /// from the document itself. Use this when you don't track identifiers
    /// (e.g., ESP32 NimBLE).
    pub fn on_ble_data(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Merge the document (extracts node_id internally)
        let result = self.document_sync.merge_document(data)?;

        // Record sync using the source_node from the merged document
        self.peer_manager.record_sync(result.source_node, now_ms);

        // Add the peer if not already known (creates peer entry from document data)
        self.peer_manager
            .on_incoming_connection(identifier, result.source_node, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            total_count: result.total_count,
        })
    }

    // ==================== Periodic Maintenance ====================

    /// Periodic tick - call this regularly (e.g., every second)
    ///
    /// Performs:
    /// - Stale peer cleanup
    /// - Periodic sync broadcast (if interval elapsed)
    ///
    /// Returns `Some(data)` if a sync broadcast is needed.
    pub fn tick(&self, now_ms: u64) -> Option<Vec<u8>> {
        use std::sync::atomic::Ordering;

        // Cleanup stale peers
        let last_cleanup = self.last_cleanup_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last_cleanup) >= self.config.peer_config.cleanup_interval_ms {
            self.last_cleanup_ms.store(now_ms, Ordering::Relaxed);
            let removed = self.peer_manager.cleanup_stale(now_ms);
            for node_id in &removed {
                self.notify(HiveEvent::PeerLost { node_id: *node_id });
            }
            if !removed.is_empty() {
                self.notify_mesh_state_changed();
            }
        }

        // Check if sync broadcast is needed
        let last_sync = self.last_sync_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last_sync) >= self.config.sync_interval_ms {
            self.last_sync_ms.store(now_ms, Ordering::Relaxed);
            // Only broadcast if we have connected peers
            if self.peer_manager.connected_count() > 0 {
                return Some(self.document_sync.build_document());
            }
        }

        None
    }

    // ==================== State Queries ====================

    /// Get all known peers
    pub fn get_peers(&self) -> Vec<HivePeer> {
        self.peer_manager.get_peers()
    }

    /// Get connected peers only
    pub fn get_connected_peers(&self) -> Vec<HivePeer> {
        self.peer_manager.get_connected_peers()
    }

    /// Get a specific peer by NodeId
    pub fn get_peer(&self, node_id: NodeId) -> Option<HivePeer> {
        self.peer_manager.get_peer(node_id)
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peer_manager.peer_count()
    }

    /// Get connected peer count
    pub fn connected_count(&self) -> usize {
        self.peer_manager.connected_count()
    }

    /// Check if a device mesh ID matches our mesh
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        self.peer_manager.matches_mesh(device_mesh_id)
    }

    /// Get total counter value
    pub fn total_count(&self) -> u64 {
        self.document_sync.total_count()
    }

    /// Get document version
    pub fn document_version(&self) -> u32 {
        self.document_sync.version()
    }

    /// Get document version (alias)
    pub fn version(&self) -> u32 {
        self.document_sync.version()
    }

    /// Update health status (battery percentage)
    pub fn update_health(&self, battery_percent: u8) {
        self.document_sync.update_health(battery_percent);
    }

    /// Build current document for transmission
    pub fn build_document(&self) -> Vec<u8> {
        self.document_sync.build_document()
    }

    /// Get peers that should be synced with
    pub fn peers_needing_sync(&self, now_ms: u64) -> Vec<HivePeer> {
        self.peer_manager.peers_needing_sync(now_ms)
    }

    // ==================== Internal Helpers ====================

    fn notify(&self, event: HiveEvent) {
        self.observers.notify(event);
    }

    fn notify_mesh_state_changed(&self) {
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
    }
}

/// Result from receiving BLE data
#[derive(Debug, Clone)]
pub struct DataReceivedResult {
    /// Node that sent this data
    pub source_node: NodeId,

    /// Whether this contained an emergency event
    pub is_emergency: bool,

    /// Whether this contained an ACK event
    pub is_ack: bool,

    /// Whether the counter changed (new data)
    pub counter_changed: bool,

    /// Updated total count
    pub total_count: u64,
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::observer::CollectingObserver;

    fn create_mesh(node_id: u32, callsign: &str) -> HiveMesh {
        let config = HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST");
        HiveMesh::new(config)
    }

    #[test]
    fn test_mesh_creation() {
        let mesh = create_mesh(0x12345678, "ALPHA-1");

        assert_eq!(mesh.node_id().as_u32(), 0x12345678);
        assert_eq!(mesh.callsign(), "ALPHA-1");
        assert_eq!(mesh.mesh_id(), "TEST");
        assert_eq!(mesh.device_name(), "HIVE_TEST-12345678");
    }

    #[test]
    fn test_peer_discovery() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover a peer
        let peer = mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );

        assert!(peer.is_some());
        let peer = peer.unwrap();
        assert_eq!(peer.node_id.as_u32(), 0x22222222);

        // Check events were generated
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDiscovered { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::MeshStateChanged { .. })));
    }

    #[test]
    fn test_connection_lifecycle() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover and connect
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );

        let node_id = mesh.on_ble_connected("device-uuid", 2000);
        assert_eq!(node_id, Some(NodeId::new(0x22222222)));
        assert_eq!(mesh.connected_count(), 1);

        // Disconnect
        let node_id = mesh.on_ble_disconnected("device-uuid", DisconnectReason::RemoteRequest);
        assert_eq!(node_id, Some(NodeId::new(0x22222222)));
        assert_eq!(mesh.connected_count(), 0);

        // Check events
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerConnected { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDisconnected { .. })));
    }

    #[test]
    fn test_emergency_flow() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        let observer2 = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer2.clone());

        // mesh1 sends emergency
        let doc = mesh1.send_emergency(1000);
        assert!(mesh1.is_emergency_active());

        // mesh2 receives it
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_emergency);
        assert_eq!(result.source_node.as_u32(), 0x11111111);

        // Check events on mesh2
        let events = observer2.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::EmergencyReceived { .. })));
    }

    #[test]
    fn test_ack_flow() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        let observer2 = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer2.clone());

        // mesh1 sends ACK
        let doc = mesh1.send_ack(1000);
        assert!(mesh1.is_ack_active());

        // mesh2 receives it
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_ack);

        // Check events on mesh2
        let events = observer2.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::AckReceived { .. })));
    }

    #[test]
    fn test_tick_cleanup() {
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_peer_timeout(10_000);
        let mesh = HiveMesh::new(config);

        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover a peer
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );
        assert_eq!(mesh.peer_count(), 1);

        // Tick at t=5000 - not stale yet
        mesh.tick(5000);
        assert_eq!(mesh.peer_count(), 1);

        // Tick at t=20000 - peer is stale (10s timeout exceeded)
        mesh.tick(20000);
        assert_eq!(mesh.peer_count(), 0);

        // Check PeerLost event
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerLost { .. })));
    }

    #[test]
    fn test_tick_sync_broadcast() {
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_sync_interval(5000);
        let mesh = HiveMesh::new(config);

        // Discover and connect a peer first
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );
        mesh.on_ble_connected("device-uuid", 1000);

        // First tick at t=0 sets last_sync
        let _result = mesh.tick(0);
        // May or may not broadcast depending on initial state

        // Tick before interval - no broadcast
        let result = mesh.tick(3000);
        assert!(result.is_none());

        // After interval - should broadcast
        let result = mesh.tick(6000);
        assert!(result.is_some());

        // Immediate second tick - no broadcast (interval not elapsed)
        let result = mesh.tick(6100);
        assert!(result.is_none());

        // After another interval - should broadcast again
        let result = mesh.tick(12000);
        assert!(result.is_some());
    }

    #[test]
    fn test_incoming_connection() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Incoming connection from unknown peer
        let is_new = mesh.on_incoming_connection("central-uuid", NodeId::new(0x22222222), 1000);

        assert!(is_new);
        assert_eq!(mesh.peer_count(), 1);
        assert_eq!(mesh.connected_count(), 1);

        // Check events
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDiscovered { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerConnected { .. })));
    }

    #[test]
    fn test_mesh_filtering() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        // Wrong mesh - ignored
        let peer = mesh.on_ble_discovered(
            "device-uuid-1",
            Some("HIVE_OTHER-22222222"),
            -65,
            Some("OTHER"),
            1000,
        );
        assert!(peer.is_none());
        assert_eq!(mesh.peer_count(), 0);

        // Correct mesh - accepted
        let peer = mesh.on_ble_discovered(
            "device-uuid-2",
            Some("HIVE_TEST-33333333"),
            -65,
            Some("TEST"),
            1000,
        );
        assert!(peer.is_some());
        assert_eq!(mesh.peer_count(), 1);
    }
}
