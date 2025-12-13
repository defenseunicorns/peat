//! Mesh topology tracking and events
//!
//! Tracks the mesh topology including parent/child/peer relationships
//! and publishes events when the topology changes.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::{HierarchyLevel, NodeId};

/// State of a connection to a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// Connection in progress
    Connecting,
    /// Connected and active
    Connected,
    /// Disconnecting
    Disconnecting,
}

/// Role of a peer in the mesh topology
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRole {
    /// Our parent (we are a child of this node)
    Parent,
    /// Our child (this node is a child of us)
    Child,
    /// Peer at the same level (sibling)
    Peer,
}

/// Information about a peer in the mesh
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Node ID of the peer
    pub node_id: NodeId,
    /// Role in the topology
    pub role: PeerRole,
    /// Connection state
    pub state: ConnectionState,
    /// Hierarchy level of the peer
    pub hierarchy_level: HierarchyLevel,
    /// Last known RSSI
    pub rssi: Option<i8>,
    /// Time since connection established
    pub connected_at: Option<u64>,
    /// Number of messages received
    pub messages_received: u32,
    /// Number of messages sent
    pub messages_sent: u32,
    /// Number of connection failures
    pub failure_count: u8,
    /// Last seen timestamp (monotonic ms)
    pub last_seen_ms: u64,
}

impl PeerInfo {
    /// Create new peer info
    pub fn new(node_id: NodeId, role: PeerRole, hierarchy_level: HierarchyLevel) -> Self {
        Self {
            node_id,
            role,
            state: ConnectionState::Disconnected,
            hierarchy_level,
            rssi: None,
            connected_at: None,
            messages_received: 0,
            messages_sent: 0,
            failure_count: 0,
            last_seen_ms: 0,
        }
    }

    /// Check if this peer is connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Update RSSI value
    pub fn update_rssi(&mut self, rssi: i8) {
        self.rssi = Some(rssi);
    }

    /// Record a connection failure
    pub fn record_failure(&mut self) {
        self.failure_count = self.failure_count.saturating_add(1);
    }

    /// Reset failure count on successful connection
    pub fn reset_failures(&mut self) {
        self.failure_count = 0;
    }
}

/// Current mesh topology state
#[derive(Debug, Clone, Default)]
pub struct MeshTopology {
    /// Our parent node (if we have one)
    pub parent: Option<NodeId>,
    /// Our children nodes
    pub children: Vec<NodeId>,
    /// Peer nodes at our level
    pub peers: Vec<NodeId>,
    /// Our hierarchy level
    pub my_level: HierarchyLevel,
    /// Maximum children we can accept
    pub max_children: u8,
    /// Maximum total connections
    pub max_connections: u8,
}

impl MeshTopology {
    /// Create a new mesh topology
    pub fn new(my_level: HierarchyLevel, max_children: u8, max_connections: u8) -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            peers: Vec::new(),
            my_level,
            max_children,
            max_connections,
        }
    }

    /// Get total number of connections
    pub fn connection_count(&self) -> usize {
        let parent_count = if self.parent.is_some() { 1 } else { 0 };
        parent_count + self.children.len() + self.peers.len()
    }

    /// Check if we can accept more connections
    pub fn can_accept_connection(&self) -> bool {
        self.connection_count() < self.max_connections as usize
    }

    /// Check if we can accept more children
    pub fn can_accept_child(&self) -> bool {
        self.children.len() < self.max_children as usize && self.can_accept_connection()
    }

    /// Check if we have a parent
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    /// Add a parent
    pub fn set_parent(&mut self, node_id: NodeId) -> bool {
        if self.parent.is_some() {
            return false;
        }
        if !self.can_accept_connection() {
            return false;
        }
        self.parent = Some(node_id);
        true
    }

    /// Remove parent
    pub fn clear_parent(&mut self) -> Option<NodeId> {
        self.parent.take()
    }

    /// Add a child
    pub fn add_child(&mut self, node_id: NodeId) -> bool {
        if !self.can_accept_child() {
            return false;
        }
        if self.children.contains(&node_id) {
            return false;
        }
        self.children.push(node_id);
        true
    }

    /// Remove a child
    pub fn remove_child(&mut self, node_id: &NodeId) -> bool {
        if let Some(pos) = self.children.iter().position(|n| n == node_id) {
            self.children.remove(pos);
            true
        } else {
            false
        }
    }

    /// Add a peer
    pub fn add_peer(&mut self, node_id: NodeId) -> bool {
        if !self.can_accept_connection() {
            return false;
        }
        if self.peers.contains(&node_id) {
            return false;
        }
        self.peers.push(node_id);
        true
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, node_id: &NodeId) -> bool {
        if let Some(pos) = self.peers.iter().position(|n| n == node_id) {
            self.peers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get all connected node IDs
    pub fn all_connected(&self) -> Vec<NodeId> {
        let mut nodes = Vec::with_capacity(self.connection_count());
        if let Some(ref parent) = self.parent {
            nodes.push(parent.clone());
        }
        nodes.extend(self.children.iter().cloned());
        nodes.extend(self.peers.iter().cloned());
        nodes
    }

    /// Check if a node is connected in any role
    pub fn is_connected(&self, node_id: &NodeId) -> bool {
        self.parent.as_ref() == Some(node_id)
            || self.children.contains(node_id)
            || self.peers.contains(node_id)
    }

    /// Get the role of a connected node
    pub fn get_role(&self, node_id: &NodeId) -> Option<PeerRole> {
        if self.parent.as_ref() == Some(node_id) {
            Some(PeerRole::Parent)
        } else if self.children.contains(node_id) {
            Some(PeerRole::Child)
        } else if self.peers.contains(node_id) {
            Some(PeerRole::Peer)
        } else {
            None
        }
    }
}

/// Events that occur when the mesh topology changes
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    /// Connected to a parent node
    ParentConnected {
        /// The parent's node ID
        node_id: NodeId,
        /// Parent's hierarchy level
        level: HierarchyLevel,
        /// Signal strength
        rssi: Option<i8>,
    },
    /// Disconnected from parent
    ParentDisconnected {
        /// The parent's node ID
        node_id: NodeId,
        /// Reason for disconnect
        reason: DisconnectReason,
    },
    /// A child connected to us
    ChildConnected {
        /// The child's node ID
        node_id: NodeId,
        /// Child's hierarchy level
        level: HierarchyLevel,
    },
    /// A child disconnected
    ChildDisconnected {
        /// The child's node ID
        node_id: NodeId,
        /// Reason for disconnect
        reason: DisconnectReason,
    },
    /// A peer connected
    PeerConnected {
        /// The peer's node ID
        node_id: NodeId,
    },
    /// A peer disconnected
    PeerDisconnected {
        /// The peer's node ID
        node_id: NodeId,
        /// Reason for disconnect
        reason: DisconnectReason,
    },
    /// Topology changed (general notification)
    TopologyChanged {
        /// Number of children
        child_count: usize,
        /// Number of peers
        peer_count: usize,
        /// Have parent
        has_parent: bool,
    },
    /// Parent failover started
    ParentFailoverStarted {
        /// Previous parent
        old_parent: NodeId,
    },
    /// Parent failover completed
    ParentFailoverCompleted {
        /// Previous parent
        old_parent: NodeId,
        /// New parent (if found)
        new_parent: Option<NodeId>,
    },
    /// Connection quality changed
    ConnectionQualityChanged {
        /// Node ID
        node_id: NodeId,
        /// New RSSI
        rssi: i8,
    },
}

/// Reason for a disconnection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisconnectReason {
    /// Normal disconnect requested
    Requested,
    /// Connection timed out
    Timeout,
    /// Remote device disconnected
    RemoteDisconnect,
    /// Connection supervision timeout
    SupervisionTimeout,
    /// Link loss
    LinkLoss,
    /// Local device error
    LocalError,
    /// Unknown reason
    #[default]
    Unknown,
}

/// Candidate for parent selection
#[derive(Debug, Clone)]
pub struct ParentCandidate {
    /// Node ID
    pub node_id: NodeId,
    /// Hierarchy level
    pub level: HierarchyLevel,
    /// Signal strength
    pub rssi: i8,
    /// Time since last beacon (ms)
    pub age_ms: u64,
    /// Previous failure count
    pub failure_count: u8,
}

impl ParentCandidate {
    /// Calculate a score for this candidate (higher = better)
    ///
    /// Factors:
    /// - RSSI (signal strength) - primary factor
    /// - Age (freshness of beacon)
    /// - Failure history
    /// - Hierarchy level preference
    pub fn score(&self, my_level: HierarchyLevel) -> i32 {
        let mut score = 0i32;

        // RSSI contributes -100 to 0 (typical range is -100 to -30 dBm)
        // Scale to give strong signals a big boost
        score += (self.rssi as i32 + 100) * 2; // 0-140 range

        // Prefer fresh beacons (within last 5 seconds)
        if self.age_ms < 1000 {
            score += 20;
        } else if self.age_ms < 3000 {
            score += 10;
        } else if self.age_ms < 5000 {
            score += 5;
        }
        // Old beacons get no bonus

        // Penalize previous failures
        score -= (self.failure_count as i32) * 15;

        // Prefer parents at the level above us
        let ideal_level = match my_level {
            HierarchyLevel::Platform => HierarchyLevel::Squad,
            HierarchyLevel::Squad => HierarchyLevel::Platoon,
            HierarchyLevel::Platoon => HierarchyLevel::Company,
            HierarchyLevel::Company => HierarchyLevel::Company, // Company has no parent
        };

        if self.level == ideal_level {
            score += 30;
        } else if self.level > my_level {
            score += 15;
        }
        // Same level or lower gets no hierarchy bonus

        score
    }
}

/// Configuration for topology management
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// Maximum children to accept
    pub max_children: u8,
    /// Maximum total connections
    pub max_connections: u8,
    /// Minimum RSSI to consider for parent (-100 to 0 dBm)
    pub min_parent_rssi: i8,
    /// Maximum beacon age to consider (ms)
    pub max_beacon_age_ms: u64,
    /// Parent supervision timeout (ms)
    pub parent_timeout_ms: u64,
    /// Connection attempt timeout (ms)
    pub connect_timeout_ms: u64,
    /// Maximum connection failures before blacklisting
    pub max_failures: u8,
    /// Failover delay after parent loss (ms)
    pub failover_delay_ms: u64,
    /// RSSI hysteresis for switching parents (dB)
    pub rssi_hysteresis: u8,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            max_children: 3,
            max_connections: 7,
            min_parent_rssi: -85,
            max_beacon_age_ms: 10_000,
            parent_timeout_ms: 5_000,
            connect_timeout_ms: 10_000,
            max_failures: 3,
            failover_delay_ms: 1_000,
            rssi_hysteresis: 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_topology_new() {
        let topology = MeshTopology::new(HierarchyLevel::Squad, 3, 7);
        assert_eq!(topology.my_level, HierarchyLevel::Squad);
        assert_eq!(topology.max_children, 3);
        assert_eq!(topology.max_connections, 7);
        assert!(topology.parent.is_none());
        assert!(topology.children.is_empty());
    }

    #[test]
    fn test_set_parent() {
        let mut topology = MeshTopology::new(HierarchyLevel::Platform, 3, 7);
        let parent_id = NodeId::new(0x1234);

        assert!(topology.set_parent(parent_id.clone()));
        assert_eq!(topology.parent, Some(parent_id.clone()));
        assert_eq!(topology.connection_count(), 1);

        // Can't set another parent
        assert!(!topology.set_parent(NodeId::new(0x5678)));
    }

    #[test]
    fn test_add_children() {
        let mut topology = MeshTopology::new(HierarchyLevel::Squad, 2, 7);

        assert!(topology.add_child(NodeId::new(0x1111)));
        assert!(topology.add_child(NodeId::new(0x2222)));
        // Max children reached
        assert!(!topology.add_child(NodeId::new(0x3333)));

        assert_eq!(topology.children.len(), 2);
    }

    #[test]
    fn test_connection_limit() {
        let mut topology = MeshTopology::new(HierarchyLevel::Squad, 5, 3);

        assert!(topology.set_parent(NodeId::new(0x0001)));
        assert!(topology.add_child(NodeId::new(0x0002)));
        assert!(topology.add_peer(NodeId::new(0x0003)));
        // Max connections reached
        assert!(!topology.add_child(NodeId::new(0x0004)));
        assert!(!topology.add_peer(NodeId::new(0x0005)));

        assert_eq!(topology.connection_count(), 3);
    }

    #[test]
    fn test_remove_child() {
        let mut topology = MeshTopology::new(HierarchyLevel::Squad, 3, 7);
        let child_id = NodeId::new(0x1111);

        topology.add_child(child_id.clone());
        assert!(topology.remove_child(&child_id));
        assert!(!topology.remove_child(&child_id)); // Already removed
        assert!(topology.children.is_empty());
    }

    #[test]
    fn test_get_role() {
        let mut topology = MeshTopology::new(HierarchyLevel::Squad, 3, 7);
        let parent_id = NodeId::new(0x0001);
        let child_id = NodeId::new(0x0002);
        let peer_id = NodeId::new(0x0003);
        let unknown_id = NodeId::new(0x9999);

        topology.set_parent(parent_id.clone());
        topology.add_child(child_id.clone());
        topology.add_peer(peer_id.clone());

        assert_eq!(topology.get_role(&parent_id), Some(PeerRole::Parent));
        assert_eq!(topology.get_role(&child_id), Some(PeerRole::Child));
        assert_eq!(topology.get_role(&peer_id), Some(PeerRole::Peer));
        assert_eq!(topology.get_role(&unknown_id), None);
    }

    #[test]
    fn test_all_connected() {
        let mut topology = MeshTopology::new(HierarchyLevel::Squad, 3, 7);
        topology.set_parent(NodeId::new(0x0001));
        topology.add_child(NodeId::new(0x0002));
        topology.add_peer(NodeId::new(0x0003));

        let all = topology.all_connected();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_parent_candidate_score() {
        let candidate = ParentCandidate {
            node_id: NodeId::new(0x1234),
            level: HierarchyLevel::Squad,
            rssi: -50,
            age_ms: 500,
            failure_count: 0,
        };

        // Platform looking for Squad parent
        let score = candidate.score(HierarchyLevel::Platform);
        // RSSI: (-50 + 100) * 2 = 100
        // Age: +20 (< 1000ms)
        // Failures: 0
        // Hierarchy: +30 (ideal level)
        assert_eq!(score, 150);
    }

    #[test]
    fn test_parent_candidate_score_with_failures() {
        let candidate = ParentCandidate {
            node_id: NodeId::new(0x1234),
            level: HierarchyLevel::Squad,
            rssi: -50,
            age_ms: 500,
            failure_count: 2,
        };

        let score = candidate.score(HierarchyLevel::Platform);
        // Base: 150 - (2 * 15) = 120
        assert_eq!(score, 120);
    }

    #[test]
    fn test_peer_info() {
        let mut peer = PeerInfo::new(
            NodeId::new(0x1234),
            PeerRole::Child,
            HierarchyLevel::Platform,
        );

        assert!(!peer.is_connected());
        assert_eq!(peer.failure_count, 0);

        peer.record_failure();
        peer.record_failure();
        assert_eq!(peer.failure_count, 2);

        peer.reset_failures();
        assert_eq!(peer.failure_count, 0);
    }

    #[test]
    fn test_topology_config_default() {
        let config = TopologyConfig::default();
        assert_eq!(config.max_children, 3);
        assert_eq!(config.max_connections, 7);
        assert_eq!(config.min_parent_rssi, -85);
    }
}
