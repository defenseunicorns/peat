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

use crate::document::ENCRYPTED_MARKER;
use crate::document_sync::DocumentSync;
use crate::observer::{DisconnectReason, HiveEvent, HiveObserver};
use crate::peer::{HivePeer, PeerManagerConfig};
use crate::peer_manager::PeerManager;
use crate::security::MeshEncryptionKey;
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

    /// Optional shared secret for mesh-wide encryption (32 bytes)
    ///
    /// When set, all documents are encrypted using ChaCha20-Poly1305 before
    /// transmission and decrypted upon receipt. All nodes in the mesh must
    /// share the same secret to communicate.
    pub encryption_secret: Option<[u8; 32]>,
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
            encryption_secret: None,
        }
    }

    /// Enable mesh-wide encryption with a shared secret
    ///
    /// All documents will be encrypted using ChaCha20-Poly1305 before
    /// transmission. All mesh participants must use the same secret.
    pub fn with_encryption(mut self, secret: [u8; 32]) -> Self {
        self.encryption_secret = Some(secret);
        self
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

    /// Last sync broadcast time (u32 wraps every ~49 days, sufficient for intervals)
    last_sync_ms: std::sync::atomic::AtomicU32,

    /// Last cleanup time
    last_cleanup_ms: std::sync::atomic::AtomicU32,

    /// Optional mesh-wide encryption key (derived from shared secret)
    encryption_key: Option<MeshEncryptionKey>,
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

        // Derive encryption key from shared secret if configured
        let encryption_key = config
            .encryption_secret
            .map(|secret| MeshEncryptionKey::from_shared_secret(&config.mesh_id, &secret));

        Self {
            config,
            peer_manager,
            document_sync,
            observers: ObserverManager::new(),
            last_sync_ms: std::sync::atomic::AtomicU32::new(0),
            last_cleanup_ms: std::sync::atomic::AtomicU32::new(0),
            encryption_key,
        }
    }

    // ==================== Encryption ====================

    /// Check if mesh-wide encryption is enabled
    pub fn is_encryption_enabled(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Enable mesh-wide encryption with a shared secret
    ///
    /// Derives a ChaCha20-Poly1305 key from the secret using HKDF-SHA256.
    /// All mesh participants must use the same secret to communicate.
    pub fn enable_encryption(&mut self, secret: &[u8; 32]) {
        self.encryption_key = Some(MeshEncryptionKey::from_shared_secret(
            &self.config.mesh_id,
            secret,
        ));
    }

    /// Disable mesh-wide encryption
    pub fn disable_encryption(&mut self) {
        self.encryption_key = None;
    }

    /// Encrypt document bytes for transmission
    ///
    /// Returns the encrypted bytes with ENCRYPTED_MARKER prefix, or the
    /// original bytes if encryption is disabled.
    fn encrypt_document(&self, plaintext: &[u8]) -> Vec<u8> {
        match &self.encryption_key {
            Some(key) => {
                // Encrypt and prepend marker
                match key.encrypt_to_bytes(plaintext) {
                    Ok(ciphertext) => {
                        let mut buf = Vec::with_capacity(2 + ciphertext.len());
                        buf.push(ENCRYPTED_MARKER);
                        buf.push(0x00); // reserved
                        buf.extend_from_slice(&ciphertext);
                        buf
                    }
                    Err(e) => {
                        log::error!("Encryption failed: {}", e);
                        // Fall back to unencrypted on error (shouldn't happen)
                        plaintext.to_vec()
                    }
                }
            }
            None => plaintext.to_vec(),
        }
    }

    /// Decrypt document bytes received from peer
    ///
    /// Returns the decrypted bytes if encrypted and valid, or the original
    /// bytes if not encrypted. Returns None if decryption fails.
    fn decrypt_document<'a>(&self, data: &'a [u8]) -> Option<std::borrow::Cow<'a, [u8]>> {
        // Check for encrypted marker
        if data.len() >= 2 && data[0] == ENCRYPTED_MARKER {
            // Encrypted document
            let _reserved = data[1];
            let encrypted_payload = &data[2..];

            match &self.encryption_key {
                Some(key) => match key.decrypt_from_bytes(encrypted_payload) {
                    Ok(plaintext) => Some(std::borrow::Cow::Owned(plaintext)),
                    Err(e) => {
                        log::warn!("Decryption failed (wrong key or corrupted): {}", e);
                        None
                    }
                },
                None => {
                    log::warn!("Received encrypted document but encryption not enabled");
                    None
                }
            }
        } else {
            // Unencrypted document - pass through
            // If we have encryption enabled, we could optionally reject unencrypted
            // documents for stricter security. For now, accept for backward compat.
            Some(std::borrow::Cow::Borrowed(data))
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
    /// If encryption is enabled, the document is encrypted.
    pub fn send_emergency(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_emergency(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
    }

    /// Send an ACK response
    ///
    /// Returns the document bytes to broadcast to all peers.
    /// If encryption is enabled, the document is encrypted.
    pub fn send_ack(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_ack(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
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

    // ==================== Emergency Management (Document-Based) ====================

    /// Start a new emergency event with ACK tracking
    ///
    /// Creates an emergency event that tracks ACKs from all known peers.
    /// Pass the list of known peer node IDs to track.
    /// Returns the document bytes to broadcast.
    /// If encryption is enabled, the document is encrypted.
    pub fn start_emergency(&self, timestamp: u64, known_peers: &[u32]) -> Vec<u8> {
        let data = self.document_sync.start_emergency(timestamp, known_peers);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
    }

    /// Start a new emergency using all currently known peers
    ///
    /// Convenience method that automatically includes all discovered peers.
    pub fn start_emergency_with_known_peers(&self, timestamp: u64) -> Vec<u8> {
        let peers: Vec<u32> = self
            .peer_manager
            .get_peers()
            .iter()
            .map(|p| p.node_id.as_u32())
            .collect();
        self.start_emergency(timestamp, &peers)
    }

    /// Record our ACK for the current emergency
    ///
    /// Returns the document bytes to broadcast, or None if no emergency is active.
    /// If encryption is enabled, the document is encrypted.
    pub fn ack_emergency(&self, timestamp: u64) -> Option<Vec<u8>> {
        let result = self.document_sync.ack_emergency(timestamp);
        if result.is_some() {
            self.notify(HiveEvent::MeshStateChanged {
                peer_count: self.peer_manager.peer_count(),
                connected_count: self.peer_manager.connected_count(),
            });
        }
        result.map(|data| self.encrypt_document(&data))
    }

    /// Clear the current emergency event
    pub fn clear_emergency(&self) {
        self.document_sync.clear_emergency();
    }

    /// Check if there's an active emergency
    pub fn has_active_emergency(&self) -> bool {
        self.document_sync.has_active_emergency()
    }

    /// Get emergency status info
    ///
    /// Returns (source_node, timestamp, acked_count, pending_count) if emergency is active.
    pub fn get_emergency_status(&self) -> Option<(u32, u64, usize, usize)> {
        self.document_sync.get_emergency_status()
    }

    /// Check if a specific peer has ACKed the current emergency
    pub fn has_peer_acked(&self, peer_id: u32) -> bool {
        self.document_sync.has_peer_acked(peer_id)
    }

    /// Check if all peers have ACKed the current emergency
    pub fn all_peers_acked(&self) -> bool {
        self.document_sync.all_peers_acked()
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

    /// Called when a BLE connection is lost, using NodeId directly
    ///
    /// Alternative to on_ble_disconnected() when only NodeId is known (e.g., ESP32).
    pub fn on_peer_disconnected(&self, node_id: NodeId, reason: DisconnectReason) {
        if self
            .peer_manager
            .on_disconnected_by_node_id(node_id, reason)
        {
            self.notify(HiveEvent::PeerDisconnected { node_id, reason });
            self.notify_mesh_state_changed();
        }
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
    /// If encryption is enabled, decrypts the document first.
    /// Returns the source NodeId and whether the document contained an event.
    pub fn on_ble_data_received(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Get node ID from identifier
        let node_id = self.peer_manager.get_node_id(identifier)?;

        // Decrypt if encrypted
        let decrypted = self.decrypt_document(data)?;

        // Merge the document
        let result = self.document_sync.merge_document(&decrypted)?;

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
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
        })
    }

    /// Called when data is received but we don't have the identifier mapped
    ///
    /// Use this when receiving data from a peripheral we discovered.
    /// If encryption is enabled, decrypts the document first.
    pub fn on_ble_data_received_from_node(
        &self,
        node_id: NodeId,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Decrypt if encrypted
        let decrypted = self.decrypt_document(data)?;

        // Merge the document
        let result = self.document_sync.merge_document(&decrypted)?;

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
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
        })
    }

    /// Called when data is received without a known identifier
    ///
    /// This is the simplest data receive method - it extracts the source node_id
    /// from the document itself. Use this when you don't track identifiers
    /// (e.g., ESP32 NimBLE).
    /// If encryption is enabled, decrypts the document first.
    pub fn on_ble_data(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Decrypt if encrypted
        let decrypted = self.decrypt_document(data)?;

        // Merge the document (extracts node_id internally)
        let result = self.document_sync.merge_document(&decrypted)?;

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
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
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

        // Use u32 for atomic storage (wraps every ~49 days, intervals still work)
        let now_ms_32 = now_ms as u32;

        // Cleanup stale peers
        let last_cleanup = self.last_cleanup_ms.load(Ordering::Relaxed);
        let cleanup_elapsed = now_ms_32.wrapping_sub(last_cleanup);
        if cleanup_elapsed >= self.config.peer_config.cleanup_interval_ms as u32 {
            self.last_cleanup_ms.store(now_ms_32, Ordering::Relaxed);
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
        let sync_elapsed = now_ms_32.wrapping_sub(last_sync);
        if sync_elapsed >= self.config.sync_interval_ms as u32 {
            self.last_sync_ms.store(now_ms_32, Ordering::Relaxed);
            // Only broadcast if we have connected peers
            if self.peer_manager.connected_count() > 0 {
                let doc = self.document_sync.build_document();
                return Some(self.encrypt_document(&doc));
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
    ///
    /// If encryption is enabled, the document is encrypted.
    pub fn build_document(&self) -> Vec<u8> {
        let doc = self.document_sync.build_document();
        self.encrypt_document(&doc)
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

    /// Whether emergency state changed (new emergency or ACK updates)
    pub emergency_changed: bool,

    /// Updated total count
    pub total_count: u64,

    /// Event timestamp (if event present) - use to detect duplicate events
    pub event_timestamp: u64,
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

    // ==================== Encryption Tests ====================

    fn create_encrypted_mesh(node_id: u32, callsign: &str, secret: [u8; 32]) -> HiveMesh {
        let config =
            HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST").with_encryption(secret);
        HiveMesh::new(config)
    }

    #[test]
    fn test_encryption_enabled() {
        let secret = [0x42u8; 32];
        let mesh = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);

        assert!(mesh.is_encryption_enabled());
    }

    #[test]
    fn test_encryption_disabled_by_default() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        assert!(!mesh.is_encryption_enabled());
    }

    #[test]
    fn test_encrypted_document_exchange() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        // mesh1 sends document
        let doc = mesh1.build_document();

        // Document should be encrypted (starts with ENCRYPTED_MARKER)
        assert!(doc.len() >= 2);
        assert_eq!(doc[0], crate::document::ENCRYPTED_MARKER);

        // mesh2 receives and decrypts
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.source_node.as_u32(), 0x11111111);
    }

    #[test]
    fn test_encrypted_emergency_exchange() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        let observer = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer.clone());

        // mesh1 sends emergency
        let doc = mesh1.send_emergency(1000);

        // mesh2 receives and decrypts
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_emergency);

        // Check EmergencyReceived event was fired
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::EmergencyReceived { .. })));
    }

    #[test]
    fn test_wrong_key_fails_decrypt() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32]; // Different key
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret1);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret2);

        // mesh1 sends document
        let doc = mesh1.build_document();

        // mesh2 cannot decrypt (wrong key)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_none());
    }

    #[test]
    fn test_unencrypted_mesh_can_read_unencrypted() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        // mesh1 sends document (unencrypted)
        let doc = mesh1.build_document();

        // mesh2 receives (also unencrypted)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
    }

    #[test]
    fn test_encrypted_mesh_can_receive_unencrypted() {
        // Backward compatibility: encrypted mesh can receive unencrypted docs
        let secret = [0x42u8; 32];
        let mesh1 = create_mesh(0x11111111, "ALPHA-1"); // unencrypted
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret); // encrypted

        // mesh1 sends unencrypted document
        let doc = mesh1.build_document();

        // mesh2 can receive unencrypted (backward compat)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
    }

    #[test]
    fn test_unencrypted_mesh_cannot_receive_encrypted() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret); // encrypted
        let mesh2 = create_mesh(0x22222222, "BRAVO-1"); // unencrypted

        // mesh1 sends encrypted document
        let doc = mesh1.build_document();

        // mesh2 cannot decrypt (no key)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_none());
    }

    #[test]
    fn test_enable_disable_encryption() {
        let mut mesh = create_mesh(0x11111111, "ALPHA-1");

        assert!(!mesh.is_encryption_enabled());

        // Enable encryption
        let secret = [0x42u8; 32];
        mesh.enable_encryption(&secret);
        assert!(mesh.is_encryption_enabled());

        // Build document should now be encrypted
        let doc = mesh.build_document();
        assert_eq!(doc[0], crate::document::ENCRYPTED_MARKER);

        // Disable encryption
        mesh.disable_encryption();
        assert!(!mesh.is_encryption_enabled());

        // Build document should now be unencrypted
        let doc = mesh.build_document();
        assert_ne!(doc[0], crate::document::ENCRYPTED_MARKER);
    }

    #[test]
    fn test_encryption_overhead() {
        let secret = [0x42u8; 32];
        let mesh_encrypted = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh_unencrypted = create_mesh(0x22222222, "BRAVO-1");

        let doc_encrypted = mesh_encrypted.build_document();
        let doc_unencrypted = mesh_unencrypted.build_document();

        // Encrypted doc should be larger by:
        // - 2 bytes marker header (0xAE + reserved)
        // - 12 bytes nonce
        // - 16 bytes auth tag
        // Total: 30 bytes overhead
        let overhead = doc_encrypted.len() - doc_unencrypted.len();
        assert_eq!(overhead, 30); // 2 (marker) + 12 (nonce) + 16 (tag)
    }
}
