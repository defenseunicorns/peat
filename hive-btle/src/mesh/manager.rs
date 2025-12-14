//! Mesh Manager
//!
//! Manages the mesh topology, connections, and provides parent failover.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(feature = "std")]
use std::sync::RwLock;

use crate::discovery::HiveBeacon;
use crate::error::{BleError, Result};
use crate::{HierarchyLevel, NodeId};

use super::topology::{
    ConnectionState, DisconnectReason, MeshTopology, ParentCandidate, PeerInfo, PeerRole,
    TopologyConfig, TopologyEvent,
};

/// Callback type for topology events
pub type TopologyCallback = Box<dyn Fn(&TopologyEvent) + Send + Sync>;

/// Mesh manager state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManagerState {
    /// Not started
    #[default]
    Stopped,
    /// Starting up
    Starting,
    /// Running and managing topology
    Running,
    /// In parent failover mode
    Failover,
    /// Stopping
    Stopping,
}

/// Manages the BLE mesh topology
///
/// Responsible for:
/// - Tracking parent/child/peer connections
/// - Parent selection and failover
/// - Connection lifecycle management
/// - Publishing topology events
#[cfg(feature = "std")]
pub struct MeshManager {
    /// Our node ID
    node_id: NodeId,
    /// Our hierarchy level
    my_level: HierarchyLevel,
    /// Configuration
    config: TopologyConfig,
    /// Current topology state
    topology: RwLock<MeshTopology>,
    /// Connected peer info
    peers: RwLock<HashMap<NodeId, PeerInfo>>,
    /// Parent candidates from beacons
    candidates: RwLock<Vec<ParentCandidate>>,
    /// Current state
    state: RwLock<ManagerState>,
    /// Event callbacks
    callbacks: RwLock<Vec<TopologyCallback>>,
    /// Monotonic time in milliseconds (for testing without system time)
    /// Using AtomicUsize for 32-bit platform compatibility (ESP32)
    current_time_ms: AtomicUsize,
}

#[cfg(feature = "std")]
impl MeshManager {
    /// Create a new mesh manager
    pub fn new(node_id: NodeId, my_level: HierarchyLevel, config: TopologyConfig) -> Self {
        let topology = MeshTopology::new(my_level, config.max_children, config.max_connections);

        Self {
            node_id,
            my_level,
            config,
            topology: RwLock::new(topology),
            peers: RwLock::new(HashMap::new()),
            candidates: RwLock::new(Vec::new()),
            state: RwLock::new(ManagerState::Stopped),
            callbacks: RwLock::new(Vec::new()),
            current_time_ms: AtomicUsize::new(0),
        }
    }

    /// Get our node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Get our hierarchy level
    pub fn my_level(&self) -> HierarchyLevel {
        self.my_level
    }

    /// Get current state
    pub fn state(&self) -> ManagerState {
        *self.state.read().unwrap()
    }

    /// Start the mesh manager
    pub fn start(&self) -> Result<()> {
        let mut state = self.state.write().unwrap();
        match *state {
            ManagerState::Stopped => {
                *state = ManagerState::Running;
                Ok(())
            }
            _ => Err(BleError::InvalidState("Already started".into())),
        }
    }

    /// Stop the mesh manager
    pub fn stop(&self) -> Result<()> {
        let mut state = self.state.write().unwrap();
        *state = ManagerState::Stopped;

        // Clear topology
        let mut topology = self.topology.write().unwrap();
        topology.parent = None;
        topology.children.clear();
        topology.peers.clear();

        // Clear peers
        self.peers.write().unwrap().clear();

        // Clear candidates
        self.candidates.write().unwrap().clear();

        Ok(())
    }

    /// Register a callback for topology events
    pub fn on_topology_event(&self, callback: TopologyCallback) {
        self.callbacks.write().unwrap().push(callback);
    }

    /// Emit a topology event to all listeners
    fn emit_event(&self, event: TopologyEvent) {
        let callbacks = self.callbacks.read().unwrap();
        for callback in callbacks.iter() {
            callback(&event);
        }
    }

    /// Set the current time (for testing or embedded without RTC)
    /// Note: Uses usize internally for 32-bit platform compatibility
    pub fn set_time_ms(&self, time_ms: u64) {
        self.current_time_ms
            .store(time_ms as usize, Ordering::SeqCst);
    }

    /// Get the current time
    /// Note: Returns u64 but internally stored as usize for 32-bit compatibility
    pub fn time_ms(&self) -> u64 {
        self.current_time_ms.load(Ordering::SeqCst) as u64
    }

    /// Get a snapshot of the current topology
    pub fn topology(&self) -> MeshTopology {
        self.topology.read().unwrap().clone()
    }

    /// Check if we have a parent
    pub fn has_parent(&self) -> bool {
        self.topology.read().unwrap().has_parent()
    }

    /// Get our parent's node ID
    pub fn parent(&self) -> Option<NodeId> {
        self.topology.read().unwrap().parent
    }

    /// Get list of children
    pub fn children(&self) -> Vec<NodeId> {
        self.topology.read().unwrap().children.clone()
    }

    /// Get number of children
    pub fn child_count(&self) -> usize {
        self.topology.read().unwrap().children.len()
    }

    /// Check if we can accept more children
    pub fn can_accept_child(&self) -> bool {
        self.topology.read().unwrap().can_accept_child()
    }

    /// Get all connected peer IDs
    pub fn connected_peers(&self) -> Vec<NodeId> {
        self.topology.read().unwrap().all_connected()
    }

    /// Get peer info for a node
    pub fn get_peer_info(&self, node_id: &NodeId) -> Option<PeerInfo> {
        self.peers.read().unwrap().get(node_id).cloned()
    }

    /// Process a beacon from a discovered node
    ///
    /// This updates our list of potential parents
    pub fn process_beacon(&self, beacon: &HiveBeacon, rssi: i8) {
        // Only consider nodes at higher hierarchy levels as parent candidates
        if beacon.hierarchy_level > self.my_level {
            let candidate = ParentCandidate {
                node_id: beacon.node_id,
                level: beacon.hierarchy_level,
                rssi,
                age_ms: 0,
                failure_count: self
                    .peers
                    .read()
                    .unwrap()
                    .get(&beacon.node_id)
                    .map(|p| p.failure_count)
                    .unwrap_or(0),
            };

            let mut candidates = self.candidates.write().unwrap();

            // Update existing or add new
            if let Some(existing) = candidates.iter_mut().find(|c| c.node_id == beacon.node_id) {
                existing.rssi = rssi;
                existing.age_ms = 0;
                existing.level = beacon.hierarchy_level;
            } else {
                candidates.push(candidate);
            }
        }
    }

    /// Select best parent from candidates
    ///
    /// Returns the best candidate based on RSSI, age, and failure history
    pub fn select_best_parent(&self) -> Option<ParentCandidate> {
        let candidates = self.candidates.read().unwrap();

        candidates
            .iter()
            .filter(|c| {
                c.rssi >= self.config.min_parent_rssi
                    && c.age_ms <= self.config.max_beacon_age_ms
                    && c.failure_count < self.config.max_failures
            })
            .max_by_key(|c| c.score(self.my_level))
            .cloned()
    }

    /// Connect to a node as our parent
    pub fn connect_parent(&self, node_id: NodeId, level: HierarchyLevel, rssi: i8) -> Result<()> {
        let mut topology = self.topology.write().unwrap();

        if topology.has_parent() {
            return Err(BleError::InvalidState("Already have a parent".into()));
        }

        if !topology.set_parent(node_id) {
            return Err(BleError::ConnectionFailed(
                "Cannot accept connection".into(),
            ));
        }

        // Add peer info
        let mut peer_info = PeerInfo::new(node_id, PeerRole::Parent, level);
        peer_info.state = ConnectionState::Connected;
        peer_info.rssi = Some(rssi);
        peer_info.connected_at = Some(self.time_ms());
        peer_info.last_seen_ms = self.time_ms();

        self.peers.write().unwrap().insert(node_id, peer_info);

        // Emit event
        drop(topology); // Release lock before emitting
        self.emit_event(TopologyEvent::ParentConnected {
            node_id,
            level,
            rssi: Some(rssi),
        });

        self.emit_topology_changed();
        Ok(())
    }

    /// Disconnect from our parent
    pub fn disconnect_parent(&self, reason: DisconnectReason) -> Option<NodeId> {
        let old_parent = {
            let mut topology = self.topology.write().unwrap();
            topology.clear_parent()
        };

        if let Some(ref parent_id) = old_parent {
            self.peers.write().unwrap().remove(parent_id);

            self.emit_event(TopologyEvent::ParentDisconnected {
                node_id: *parent_id,
                reason,
            });
            self.emit_topology_changed();
        }

        old_parent
    }

    /// Accept a child connection
    pub fn accept_child(&self, node_id: NodeId, level: HierarchyLevel) -> Result<()> {
        let mut topology = self.topology.write().unwrap();

        if !topology.add_child(node_id) {
            return Err(BleError::ConnectionFailed("Cannot accept child".into()));
        }

        // Add peer info
        let mut peer_info = PeerInfo::new(node_id, PeerRole::Child, level);
        peer_info.state = ConnectionState::Connected;
        peer_info.connected_at = Some(self.time_ms());
        peer_info.last_seen_ms = self.time_ms();

        self.peers.write().unwrap().insert(node_id, peer_info);

        // Emit event
        drop(topology);
        self.emit_event(TopologyEvent::ChildConnected { node_id, level });

        self.emit_topology_changed();
        Ok(())
    }

    /// Remove a child
    pub fn remove_child(&self, node_id: &NodeId, reason: DisconnectReason) -> bool {
        let removed = {
            let mut topology = self.topology.write().unwrap();
            topology.remove_child(node_id)
        };

        if removed {
            self.peers.write().unwrap().remove(node_id);

            self.emit_event(TopologyEvent::ChildDisconnected {
                node_id: *node_id,
                reason,
            });
            self.emit_topology_changed();
        }

        removed
    }

    /// Start parent failover process
    pub fn start_failover(&self) -> Result<()> {
        let mut state = self.state.write().unwrap();
        if *state != ManagerState::Running {
            return Err(BleError::InvalidState("Not running".into()));
        }

        let old_parent = self.disconnect_parent(DisconnectReason::LinkLoss);

        if let Some(old_parent_id) = old_parent {
            *state = ManagerState::Failover;
            drop(state);

            self.emit_event(TopologyEvent::ParentFailoverStarted {
                old_parent: old_parent_id,
            });
        }

        Ok(())
    }

    /// Complete failover by connecting to new parent
    pub fn complete_failover(
        &self,
        new_parent: Option<(NodeId, HierarchyLevel, i8)>,
    ) -> Result<()> {
        let old_parent = {
            // Get old parent from candidates list (it was stored there)
            self.candidates
                .read()
                .unwrap()
                .first()
                .map(|c| c.node_id)
                .unwrap_or_else(|| NodeId::new(0))
        };

        if let Some((node_id, level, rssi)) = new_parent {
            self.connect_parent(node_id, level, rssi)?;

            let mut state = self.state.write().unwrap();
            *state = ManagerState::Running;
            drop(state);

            self.emit_event(TopologyEvent::ParentFailoverCompleted {
                old_parent,
                new_parent: Some(node_id),
            });
        } else {
            let mut state = self.state.write().unwrap();
            *state = ManagerState::Running;
            drop(state);

            self.emit_event(TopologyEvent::ParentFailoverCompleted {
                old_parent,
                new_parent: None,
            });
        }

        Ok(())
    }

    /// Update RSSI for a connected peer
    pub fn update_rssi(&self, node_id: &NodeId, rssi: i8) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.update_rssi(rssi);
            peer.last_seen_ms = self.time_ms();
        }
        drop(peers);

        self.emit_event(TopologyEvent::ConnectionQualityChanged {
            node_id: *node_id,
            rssi,
        });
    }

    /// Record a connection failure for a node
    pub fn record_failure(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().unwrap();
        if let Some(peer) = peers.get_mut(node_id) {
            peer.record_failure();
        }

        // Also update candidate failure count
        let mut candidates = self.candidates.write().unwrap();
        if let Some(candidate) = candidates.iter_mut().find(|c| &c.node_id == node_id) {
            candidate.failure_count = candidate.failure_count.saturating_add(1);
        }
    }

    /// Age all candidates (call periodically)
    pub fn age_candidates(&self, elapsed_ms: u64) {
        let mut candidates = self.candidates.write().unwrap();
        for candidate in candidates.iter_mut() {
            candidate.age_ms = candidate.age_ms.saturating_add(elapsed_ms);
        }

        // Remove candidates that are too old
        candidates.retain(|c| c.age_ms <= self.config.max_beacon_age_ms * 2);
    }

    /// Check if we should switch parents (better option available)
    pub fn should_switch_parent(&self) -> Option<ParentCandidate> {
        let topology = self.topology.read().unwrap();
        let current_parent = topology.parent?;
        drop(topology);

        let peers = self.peers.read().unwrap();
        let current_rssi = peers.get(&current_parent)?.rssi?;
        drop(peers);

        // Find best alternative
        let best = self.select_best_parent()?;

        // Only switch if significantly better (hysteresis)
        if best.rssi > current_rssi + self.config.rssi_hysteresis as i8 {
            Some(best)
        } else {
            None
        }
    }

    /// Helper to emit topology changed event
    fn emit_topology_changed(&self) {
        let topology = self.topology.read().unwrap();
        self.emit_event(TopologyEvent::TopologyChanged {
            child_count: topology.children.len(),
            peer_count: topology.peers.len(),
            has_parent: topology.has_parent(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_manager() -> MeshManager {
        MeshManager::new(
            NodeId::new(0x1234),
            HierarchyLevel::Platform,
            TopologyConfig::default(),
        )
    }

    #[test]
    fn test_manager_creation() {
        let manager = create_manager();
        assert_eq!(manager.node_id().as_u32(), 0x1234);
        assert_eq!(manager.my_level(), HierarchyLevel::Platform);
        assert_eq!(manager.state(), ManagerState::Stopped);
    }

    #[test]
    fn test_start_stop() {
        let manager = create_manager();

        assert!(manager.start().is_ok());
        assert_eq!(manager.state(), ManagerState::Running);

        assert!(manager.stop().is_ok());
        assert_eq!(manager.state(), ManagerState::Stopped);
    }

    #[test]
    fn test_connect_parent() {
        let manager = create_manager();
        manager.start().unwrap();

        let parent_id = NodeId::new(0x5678);
        assert!(manager
            .connect_parent(parent_id, HierarchyLevel::Squad, -50)
            .is_ok());

        assert!(manager.has_parent());
        assert_eq!(manager.parent(), Some(parent_id));

        // Can't connect another parent
        assert!(manager
            .connect_parent(NodeId::new(0x9999), HierarchyLevel::Squad, -50)
            .is_err());
    }

    #[test]
    fn test_disconnect_parent() {
        let manager = create_manager();
        manager.start().unwrap();

        let parent_id = NodeId::new(0x5678);
        manager
            .connect_parent(parent_id, HierarchyLevel::Squad, -50)
            .unwrap();

        let old = manager.disconnect_parent(DisconnectReason::Requested);
        assert_eq!(old, Some(parent_id));
        assert!(!manager.has_parent());
    }

    #[test]
    fn test_accept_child() {
        let manager = MeshManager::new(
            NodeId::new(0x1234),
            HierarchyLevel::Squad,
            TopologyConfig::default(),
        );
        manager.start().unwrap();

        let child_id = NodeId::new(0x0001);
        assert!(manager
            .accept_child(child_id, HierarchyLevel::Platform)
            .is_ok());

        assert_eq!(manager.child_count(), 1);
        assert_eq!(manager.children(), vec![child_id]);
    }

    #[test]
    fn test_max_children() {
        let config = TopologyConfig {
            max_children: 2,
            ..Default::default()
        };

        let manager = MeshManager::new(NodeId::new(0x1234), HierarchyLevel::Squad, config);
        manager.start().unwrap();

        assert!(manager
            .accept_child(NodeId::new(0x0001), HierarchyLevel::Platform)
            .is_ok());
        assert!(manager
            .accept_child(NodeId::new(0x0002), HierarchyLevel::Platform)
            .is_ok());
        assert!(manager
            .accept_child(NodeId::new(0x0003), HierarchyLevel::Platform)
            .is_err());
    }

    #[test]
    fn test_process_beacon() {
        let manager = create_manager();
        manager.start().unwrap();

        let beacon = HiveBeacon {
            node_id: NodeId::new(0x5678),
            hierarchy_level: HierarchyLevel::Squad,
            version: 1,
            seq_num: 1,
            capabilities: 0,
            battery_percent: 100,
            geohash: 0,
        };

        manager.process_beacon(&beacon, -50);

        let best = manager.select_best_parent();
        assert!(best.is_some());
        assert_eq!(best.unwrap().node_id.as_u32(), 0x5678);
    }

    #[test]
    fn test_select_best_parent_rssi() {
        let manager = create_manager();
        manager.start().unwrap();

        // Add two candidates
        let beacon1 = HiveBeacon {
            node_id: NodeId::new(0x1111),
            hierarchy_level: HierarchyLevel::Squad,
            version: 1,
            seq_num: 1,
            capabilities: 0,
            battery_percent: 100,
            geohash: 0,
        };

        let beacon2 = HiveBeacon {
            node_id: NodeId::new(0x2222),
            hierarchy_level: HierarchyLevel::Squad,
            version: 1,
            seq_num: 1,
            capabilities: 0,
            battery_percent: 100,
            geohash: 0,
        };

        manager.process_beacon(&beacon1, -70);
        manager.process_beacon(&beacon2, -50); // Better RSSI

        let best = manager.select_best_parent().unwrap();
        assert_eq!(best.node_id.as_u32(), 0x2222);
    }

    #[test]
    fn test_failover() {
        let manager = create_manager();
        manager.start().unwrap();

        let parent_id = NodeId::new(0x5678);
        manager
            .connect_parent(parent_id, HierarchyLevel::Squad, -50)
            .unwrap();

        // Start failover
        assert!(manager.start_failover().is_ok());
        assert_eq!(manager.state(), ManagerState::Failover);
        assert!(!manager.has_parent());

        // Complete without new parent
        assert!(manager.complete_failover(None).is_ok());
        assert_eq!(manager.state(), ManagerState::Running);
    }

    #[test]
    fn test_event_callback() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let manager = create_manager();
        manager.start().unwrap();

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        manager.on_topology_event(Box::new(move |event| {
            if matches!(event, TopologyEvent::ParentConnected { .. }) {
                called_clone.store(true, Ordering::SeqCst);
            }
        }));

        manager
            .connect_parent(NodeId::new(0x5678), HierarchyLevel::Squad, -50)
            .unwrap();

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_update_rssi() {
        let manager = create_manager();
        manager.start().unwrap();

        let parent_id = NodeId::new(0x5678);
        manager
            .connect_parent(parent_id, HierarchyLevel::Squad, -50)
            .unwrap();

        manager.update_rssi(&parent_id, -60);

        let info = manager.get_peer_info(&parent_id).unwrap();
        assert_eq!(info.rssi, Some(-60));
    }

    #[test]
    fn test_age_candidates() {
        let manager = create_manager();
        manager.start().unwrap();

        let beacon = HiveBeacon {
            node_id: NodeId::new(0x5678),
            hierarchy_level: HierarchyLevel::Squad,
            version: 1,
            seq_num: 1,
            capabilities: 0,
            battery_percent: 100,
            geohash: 0,
        };

        manager.process_beacon(&beacon, -50);

        // Age past the threshold
        manager.age_candidates(25_000);

        // Should be removed
        let best = manager.select_best_parent();
        assert!(best.is_none());
    }
}
