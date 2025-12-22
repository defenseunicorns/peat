//! Peer management for HIVE BLE mesh
//!
//! This module provides centralized peer tracking, connection management,
//! and sync scheduling. It replaces the duplicated peer management logic
//! that was previously in iOS, Android, and ESP32 implementations.
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::peer_manager::PeerManager;
//! use hive_btle::peer::PeerManagerConfig;
//! use hive_btle::NodeId;
//!
//! let config = PeerManagerConfig::with_mesh_id("DEMO");
//! let manager = PeerManager::new(NodeId::new(0x12345678), config);
//!
//! // Called by platform BLE adapter on discovery
//! if let Some(node_id) = manager.on_discovered("device-uuid", Some("HIVE_DEMO-AABBCCDD"), -70, Some("DEMO")) {
//!     println!("Discovered peer: {:08X}", node_id.as_u32());
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "std")]
use std::collections::BTreeMap;
#[cfg(feature = "std")]
use std::sync::RwLock;

#[cfg(not(feature = "std"))]
use spin::RwLock;

use crate::observer::{DisconnectReason, HiveEvent};
use crate::peer::{HivePeer, PeerManagerConfig};
use crate::NodeId;

/// Centralized peer manager for HIVE mesh
///
/// Tracks discovered peers, their connection state, and sync history.
/// Thread-safe and designed for use from platform BLE callbacks.
pub struct PeerManager {
    /// Configuration
    config: PeerManagerConfig,

    /// Our node ID
    node_id: NodeId,

    /// Peers indexed by NodeId
    #[cfg(feature = "std")]
    peers: RwLock<BTreeMap<NodeId, HivePeer>>,
    #[cfg(not(feature = "std"))]
    peers: RwLock<BTreeMap<NodeId, HivePeer>>,

    /// Map from platform identifier to NodeId for quick lookup
    #[cfg(feature = "std")]
    identifier_map: RwLock<BTreeMap<String, NodeId>>,
    #[cfg(not(feature = "std"))]
    identifier_map: RwLock<BTreeMap<String, NodeId>>,

    /// Last sync timestamp per peer (for cooldown)
    #[cfg(feature = "std")]
    sync_history: RwLock<BTreeMap<NodeId, u64>>,
    #[cfg(not(feature = "std"))]
    sync_history: RwLock<BTreeMap<NodeId, u64>>,
}

impl PeerManager {
    /// Create a new peer manager
    pub fn new(node_id: NodeId, config: PeerManagerConfig) -> Self {
        Self {
            config,
            node_id,
            peers: RwLock::new(BTreeMap::new()),
            identifier_map: RwLock::new(BTreeMap::new()),
            sync_history: RwLock::new(BTreeMap::new()),
        }
    }

    /// Get our node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the mesh ID
    pub fn mesh_id(&self) -> &str {
        &self.config.mesh_id
    }

    /// Check if a device mesh ID matches our mesh
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        self.config.matches_mesh(device_mesh_id)
    }

    /// Handle a discovered BLE device
    ///
    /// Called by the platform BLE adapter when a device is discovered during scanning.
    /// Parses the device name to extract NodeId and mesh ID.
    ///
    /// Returns `Some((node_id, is_new))` if this is a HIVE device on our mesh,
    /// where `is_new` indicates if this is a newly discovered peer.
    /// Returns `None` if the device should be ignored.
    pub fn on_discovered(
        &self,
        identifier: &str,
        name: Option<&str>,
        rssi: i8,
        mesh_id: Option<&str>,
        now_ms: u64,
    ) -> Option<(NodeId, bool)> {
        // Check mesh ID match
        if !self.matches_mesh(mesh_id) {
            return None;
        }

        // Parse node ID from name (format: "HIVE_MESH-XXXXXXXX")
        let node_id = parse_node_id_from_name(name)?;

        // Don't track ourselves
        if node_id == self.node_id {
            return None;
        }

        let mut peers = self.peers.write().unwrap();
        let mut id_map = self.identifier_map.write().unwrap();

        // Check if we already have this peer by identifier (different device, same node)
        if let Some(&existing_node_id) = id_map.get(identifier) {
            if existing_node_id != node_id {
                // Identifier changed node IDs - remove old mapping
                peers.remove(&existing_node_id);
            }
        }

        // Check max peers limit
        if peers.len() >= self.config.max_peers && !peers.contains_key(&node_id) {
            return None; // At capacity
        }

        let is_new = !peers.contains_key(&node_id);

        // Update or insert peer
        let peer = peers.entry(node_id).or_insert_with(|| {
            HivePeer::new(
                node_id,
                identifier.to_string(),
                mesh_id.map(|s| s.to_string()),
                name.map(|s| s.to_string()),
                rssi,
            )
        });

        // Update existing peer
        peer.rssi = rssi;
        peer.touch(now_ms);
        if let Some(n) = name {
            peer.name = Some(n.to_string());
        }

        // Update identifier map
        id_map.insert(identifier.to_string(), node_id);

        Some((node_id, is_new))
    }

    /// Handle a peer connection
    ///
    /// Called by the platform BLE adapter when a connection is established.
    /// Returns the NodeId if found, or None if this identifier is unknown.
    pub fn on_connected(&self, identifier: &str, now_ms: u64) -> Option<NodeId> {
        let id_map = self.identifier_map.read().unwrap();
        let node_id = id_map.get(identifier).copied()?;
        drop(id_map);

        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(&node_id) {
            peer.is_connected = true;
            peer.touch(now_ms);
        }

        Some(node_id)
    }

    /// Handle a peer disconnection
    ///
    /// Called by the platform BLE adapter when a connection is lost.
    /// Returns the NodeId and disconnect reason if found.
    pub fn on_disconnected(
        &self,
        identifier: &str,
        reason: DisconnectReason,
    ) -> Option<(NodeId, DisconnectReason)> {
        let id_map = self.identifier_map.read().unwrap();
        let node_id = id_map.get(identifier).copied()?;
        drop(id_map);

        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(&node_id) {
            peer.is_connected = false;
        }

        Some((node_id, reason))
    }

    /// Register a peer from an incoming BLE connection
    ///
    /// Called when a remote device connects to us as a peripheral.
    /// Creates a peer entry if one doesn't exist for this identifier.
    pub fn on_incoming_connection(&self, identifier: &str, node_id: NodeId, now_ms: u64) -> bool {
        // Don't track ourselves
        if node_id == self.node_id {
            return false;
        }

        let mut peers = self.peers.write().unwrap();
        let mut id_map = self.identifier_map.write().unwrap();

        // Check max peers limit
        if peers.len() >= self.config.max_peers && !peers.contains_key(&node_id) {
            return false;
        }

        let is_new = !peers.contains_key(&node_id);

        let peer = peers.entry(node_id).or_insert_with(|| {
            HivePeer::new(
                node_id,
                identifier.to_string(),
                Some(self.config.mesh_id.clone()),
                None,
                -70, // Default RSSI for incoming connections
            )
        });

        peer.is_connected = true;
        peer.touch(now_ms);

        // Update identifier if changed
        if peer.identifier != identifier {
            id_map.remove(&peer.identifier);
            peer.identifier = identifier.to_string();
        }
        id_map.insert(identifier.to_string(), node_id);

        is_new
    }

    /// Check if we should sync with a peer
    ///
    /// Returns true if enough time has passed since the last sync (cooldown).
    pub fn should_sync_with(&self, node_id: NodeId, now_ms: u64) -> bool {
        let history = self.sync_history.read().unwrap();
        match history.get(&node_id) {
            Some(&last_sync) => now_ms.saturating_sub(last_sync) >= self.config.sync_cooldown_ms,
            None => true, // Never synced
        }
    }

    /// Record that we synced with a peer
    pub fn record_sync(&self, node_id: NodeId, now_ms: u64) {
        let mut history = self.sync_history.write().unwrap();
        history.insert(node_id, now_ms);
    }

    /// Clean up stale peers
    ///
    /// Removes peers that haven't been seen within the timeout period.
    /// Returns list of removed NodeIds for generating PeerLost events.
    pub fn cleanup_stale(&self, now_ms: u64) -> Vec<NodeId> {
        let mut peers = self.peers.write().unwrap();
        let mut id_map = self.identifier_map.write().unwrap();
        let mut history = self.sync_history.write().unwrap();

        let mut removed = Vec::new();

        // Find stale peers
        let stale: Vec<NodeId> = peers
            .iter()
            .filter(|(_, peer)| peer.is_stale(now_ms, self.config.peer_timeout_ms))
            .map(|(&node_id, _)| node_id)
            .collect();

        // Remove them
        for node_id in stale {
            if let Some(peer) = peers.remove(&node_id) {
                id_map.remove(&peer.identifier);
                history.remove(&node_id);
                removed.push(node_id);
            }
        }

        removed
    }

    /// Get all known peers
    pub fn get_peers(&self) -> Vec<HivePeer> {
        let peers = self.peers.read().unwrap();
        peers.values().cloned().collect()
    }

    /// Get connected peers only
    pub fn get_connected_peers(&self) -> Vec<HivePeer> {
        let peers = self.peers.read().unwrap();
        peers.values().filter(|p| p.is_connected).cloned().collect()
    }

    /// Get a specific peer by NodeId
    pub fn get_peer(&self, node_id: NodeId) -> Option<HivePeer> {
        let peers = self.peers.read().unwrap();
        peers.get(&node_id).cloned()
    }

    /// Get a peer by platform identifier
    pub fn get_peer_by_identifier(&self, identifier: &str) -> Option<HivePeer> {
        let id_map = self.identifier_map.read().unwrap();
        let node_id = id_map.get(identifier).copied()?;
        drop(id_map);

        let peers = self.peers.read().unwrap();
        peers.get(&node_id).cloned()
    }

    /// Get NodeId for a platform identifier
    pub fn get_node_id(&self, identifier: &str) -> Option<NodeId> {
        let id_map = self.identifier_map.read().unwrap();
        id_map.get(identifier).copied()
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.read().unwrap().len()
    }

    /// Get connected peer count
    pub fn connected_count(&self) -> usize {
        self.peers
            .read()
            .unwrap()
            .values()
            .filter(|p| p.is_connected)
            .count()
    }

    /// Get peers that need sync (connected and past cooldown)
    pub fn peers_needing_sync(&self, now_ms: u64) -> Vec<HivePeer> {
        let peers = self.peers.read().unwrap();
        let history = self.sync_history.read().unwrap();

        peers
            .values()
            .filter(|peer| {
                if !peer.is_connected {
                    return false;
                }
                match history.get(&peer.node_id) {
                    Some(&last_sync) => {
                        now_ms.saturating_sub(last_sync) >= self.config.sync_cooldown_ms
                    }
                    None => true,
                }
            })
            .cloned()
            .collect()
    }

    /// Generate events for current mesh state
    ///
    /// Useful for notifying observers of the current state after initialization.
    pub fn generate_state_event(&self) -> HiveEvent {
        HiveEvent::MeshStateChanged {
            peer_count: self.peer_count(),
            connected_count: self.connected_count(),
        }
    }
}

/// Parse a NodeId from a HIVE device name
///
/// Expected format: "HIVE_MESH-XXXXXXXX" where XXXXXXXX is the hex node ID
fn parse_node_id_from_name(name: Option<&str>) -> Option<NodeId> {
    let name = name?;

    // Find the last hyphen and parse hex after it
    let hyphen_pos = name.rfind('-')?;
    let hex_part = &name[hyphen_pos + 1..];

    // Parse as hex (should be 8 characters)
    if hex_part.len() != 8 {
        return None;
    }

    u32::from_str_radix(hex_part, 16).ok().map(NodeId::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_node_id_from_name() {
        assert_eq!(
            parse_node_id_from_name(Some("HIVE_DEMO-12345678")),
            Some(NodeId::new(0x12345678))
        );
        assert_eq!(
            parse_node_id_from_name(Some("HIVE_ALPHA-AABBCCDD")),
            Some(NodeId::new(0xAABBCCDD))
        );
        assert_eq!(parse_node_id_from_name(Some("Invalid")), None);
        assert_eq!(parse_node_id_from_name(Some("HIVE_DEMO-123")), None); // Too short
        assert_eq!(parse_node_id_from_name(None), None);
    }

    #[test]
    fn test_peer_discovery() {
        let config = PeerManagerConfig::with_mesh_id("DEMO");
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // Discover a peer
        let result = manager.on_discovered(
            "device-uuid-1",
            Some("HIVE_DEMO-22222222"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert!(result.is_some());
        let (node_id, is_new) = result.unwrap();
        assert_eq!(node_id.as_u32(), 0x22222222);
        assert!(is_new);

        // Same peer again - not new
        let result = manager.on_discovered(
            "device-uuid-1",
            Some("HIVE_DEMO-22222222"),
            -60,
            Some("DEMO"),
            2000,
        );
        assert!(result.is_some());
        let (_, is_new) = result.unwrap();
        assert!(!is_new);

        // Check peer is tracked
        assert_eq!(manager.peer_count(), 1);
        let peer = manager.get_peer(NodeId::new(0x22222222)).unwrap();
        assert_eq!(peer.rssi, -60); // Updated
    }

    #[test]
    fn test_mesh_filtering() {
        let config = PeerManagerConfig::with_mesh_id("ALPHA");
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // Wrong mesh - ignored
        let result = manager.on_discovered(
            "device-uuid-1",
            Some("HIVE_BETA-22222222"),
            -65,
            Some("BETA"),
            1000,
        );
        assert!(result.is_none());
        assert_eq!(manager.peer_count(), 0);

        // Correct mesh - accepted
        let result = manager.on_discovered(
            "device-uuid-2",
            Some("HIVE_ALPHA-33333333"),
            -65,
            Some("ALPHA"),
            1000,
        );
        assert!(result.is_some());
        assert_eq!(manager.peer_count(), 1);
    }

    #[test]
    fn test_self_filtering() {
        let config = PeerManagerConfig::with_mesh_id("DEMO");
        let manager = PeerManager::new(NodeId::new(0x12345678), config);

        // Discovering ourselves - ignored
        let result = manager.on_discovered(
            "my-device-uuid",
            Some("HIVE_DEMO-12345678"),
            -30,
            Some("DEMO"),
            1000,
        );
        assert!(result.is_none());
        assert_eq!(manager.peer_count(), 0);
    }

    #[test]
    fn test_connection_lifecycle() {
        let config = PeerManagerConfig::with_mesh_id("DEMO");
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // Discover
        manager.on_discovered(
            "device-uuid-1",
            Some("HIVE_DEMO-22222222"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert_eq!(manager.connected_count(), 0);

        // Connect
        let node_id = manager.on_connected("device-uuid-1", 2000);
        assert_eq!(node_id, Some(NodeId::new(0x22222222)));
        assert_eq!(manager.connected_count(), 1);

        // Disconnect
        let result = manager.on_disconnected("device-uuid-1", DisconnectReason::RemoteRequest);
        assert!(result.is_some());
        assert_eq!(manager.connected_count(), 0);
        assert_eq!(manager.peer_count(), 1); // Still tracked
    }

    #[test]
    fn test_stale_cleanup() {
        let config = PeerManagerConfig::with_mesh_id("DEMO").peer_timeout(10_000);
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // Discover at t=1000
        manager.on_discovered(
            "device-uuid-1",
            Some("HIVE_DEMO-22222222"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert_eq!(manager.peer_count(), 1);

        // Not stale at t=5000
        let removed = manager.cleanup_stale(5000);
        assert!(removed.is_empty());
        assert_eq!(manager.peer_count(), 1);

        // Stale at t=20000 (10s timeout exceeded)
        let removed = manager.cleanup_stale(20000);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].as_u32(), 0x22222222);
        assert_eq!(manager.peer_count(), 0);
    }

    #[test]
    fn test_sync_cooldown() {
        let config = PeerManagerConfig::with_mesh_id("DEMO");
        let manager = PeerManager::new(NodeId::new(0x11111111), config);
        let peer_id = NodeId::new(0x22222222);

        // Never synced - should sync
        assert!(manager.should_sync_with(peer_id, 1000));

        // Record sync
        manager.record_sync(peer_id, 1000);

        // Too soon - shouldn't sync (cooldown is 30s)
        assert!(!manager.should_sync_with(peer_id, 5000));

        // After cooldown - should sync
        assert!(manager.should_sync_with(peer_id, 35000));
    }

    #[test]
    fn test_max_peers_limit() {
        let config = PeerManagerConfig::with_mesh_id("DEMO").max_peers(2);
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // First two accepted
        let result = manager.on_discovered(
            "uuid-1",
            Some("HIVE_DEMO-22222222"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert!(result.is_some());

        let result = manager.on_discovered(
            "uuid-2",
            Some("HIVE_DEMO-33333333"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert!(result.is_some());

        // Third rejected - at capacity
        let result = manager.on_discovered(
            "uuid-3",
            Some("HIVE_DEMO-44444444"),
            -65,
            Some("DEMO"),
            1000,
        );
        assert!(result.is_none());
        assert_eq!(manager.peer_count(), 2);
    }

    #[test]
    fn test_incoming_connection() {
        let config = PeerManagerConfig::with_mesh_id("DEMO");
        let manager = PeerManager::new(NodeId::new(0x11111111), config);

        // Incoming connection from unknown peer
        let is_new = manager.on_incoming_connection("central-uuid", NodeId::new(0x22222222), 1000);
        assert!(is_new);
        assert_eq!(manager.peer_count(), 1);
        assert_eq!(manager.connected_count(), 1);

        // Same peer reconnects - not new
        let is_new = manager.on_incoming_connection("central-uuid", NodeId::new(0x22222222), 2000);
        assert!(!is_new);
    }
}
