//! Flat P2P Mesh Coordination with CRDT
//!
//! Provides a flat mesh coordinator where all nodes are peers at the same level,
//! using DynamicHierarchyStrategy for leader election and CRDT for state sync.
//!
//! This enables Lab 3b: P2P mesh with HIVE CRDT overhead measurement.

use crate::beacon::{GeographicBeacon, HierarchyLevel, NodeProfile};
use crate::hierarchy::{DynamicHierarchyStrategy, ElectionConfig, HierarchyStrategy, NodeRole};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Flat mesh peer coordinator
///
/// All nodes operate at the same hierarchy level (Squad) with dynamic
/// leader election based on capabilities.
pub struct FlatMeshCoordinator {
    /// This node's ID
    node_id: String,

    /// This node's profile
    profile: NodeProfile,

    /// Hierarchy strategy for role determination
    strategy: Arc<DynamicHierarchyStrategy>,

    /// Current role in the mesh
    current_role: Arc<RwLock<NodeRole>>,

    /// Known peers
    peers: Arc<RwLock<Vec<GeographicBeacon>>>,
}

impl FlatMeshCoordinator {
    /// Create a new flat mesh coordinator
    ///
    /// # Arguments
    ///
    /// * `node_id` - This node's identifier
    /// * `profile` - This node's capabilities
    /// * `election_config` - Optional election configuration (uses defaults if None)
    pub fn new(
        node_id: String,
        profile: NodeProfile,
        election_config: Option<ElectionConfig>,
    ) -> Self {
        let strategy = Arc::new(DynamicHierarchyStrategy::new(
            HierarchyLevel::Squad, // All nodes at Squad level
            election_config.unwrap_or_default(),
            false, // No level transitions in flat mesh
        ));

        Self {
            node_id,
            profile,
            strategy,
            current_role: Arc::new(RwLock::new(NodeRole::Standalone)),
            peers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Update peer list and re-evaluate role
    ///
    /// # Arguments
    ///
    /// * `beacons` - Current list of peer beacons
    ///
    /// # Returns
    ///
    /// The updated role for this node
    pub async fn update_peers(&self, beacons: Vec<GeographicBeacon>) -> NodeRole {
        // Update peer list
        *self.peers.write().await = beacons.clone();

        // Determine new role based on current peers
        let new_role = self.strategy.determine_role(&self.profile, &beacons);

        // Update current role
        *self.current_role.write().await = new_role;

        new_role
    }

    /// Get current role
    pub async fn current_role(&self) -> NodeRole {
        *self.current_role.read().await
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get hierarchy level (always Squad for flat mesh)
    pub fn hierarchy_level(&self) -> HierarchyLevel {
        HierarchyLevel::Squad
    }

    /// Get peer count
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Check if this node is the mesh leader
    pub async fn is_leader(&self) -> bool {
        *self.current_role.read().await == NodeRole::Leader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{GeoPosition, NodeMobility, NodeResources};

    fn create_test_profile(mobility: NodeMobility, cpu_usage: u8) -> NodeProfile {
        NodeProfile {
            mobility,
            resources: NodeResources {
                cpu_cores: 4,
                memory_mb: 2048,
                bandwidth_mbps: 100,
                cpu_usage_percent: cpu_usage,
                memory_usage_percent: 50,
                battery_percent: Some(80),
            },
            can_parent: true,
            prefer_leaf: false,
            parent_priority: 128,
        }
    }

    fn create_test_beacon(
        node_id: &str,
        position: GeoPosition,
        profile: NodeProfile,
    ) -> GeographicBeacon {
        let mut beacon =
            GeographicBeacon::new(node_id.to_string(), position, HierarchyLevel::Squad);
        beacon.mobility = Some(profile.mobility);
        beacon.resources = Some(profile.resources.clone());
        beacon.can_parent = profile.can_parent;
        beacon.parent_priority = profile.parent_priority;
        beacon
    }

    #[tokio::test]
    async fn test_flat_mesh_single_node() {
        let profile = create_test_profile(NodeMobility::Static, 30);
        let coordinator = FlatMeshCoordinator::new("node1".to_string(), profile, None);

        // With no peers, should be Standalone
        let role = coordinator.update_peers(vec![]).await;
        assert_eq!(role, NodeRole::Standalone);
        assert_eq!(coordinator.hierarchy_level(), HierarchyLevel::Squad);
    }

    #[tokio::test]
    async fn test_flat_mesh_leader_election() {
        // Create three nodes with different capabilities
        let node1_profile = create_test_profile(NodeMobility::Static, 20); // Best
        let node2_profile = create_test_profile(NodeMobility::SemiMobile, 40); // Medium
        let node3_profile = create_test_profile(NodeMobility::Mobile, 70); // Worst

        let node1 = FlatMeshCoordinator::new("node1".to_string(), node1_profile.clone(), None);

        // Create beacons for peers
        let node2_beacon =
            create_test_beacon("node2", GeoPosition::new(37.7750, -122.4195), node2_profile);

        let node3_beacon =
            create_test_beacon("node3", GeoPosition::new(37.7760, -122.4185), node3_profile);

        // Node1 sees node2 and node3
        let role = node1.update_peers(vec![node2_beacon, node3_beacon]).await;

        // Node1 has best capabilities, should be Leader
        assert_eq!(role, NodeRole::Leader);
        assert!(node1.is_leader().await);
        assert_eq!(node1.peer_count().await, 2);
    }

    #[tokio::test]
    async fn test_flat_mesh_member_role() {
        // Node with lower capability
        let node2_profile = create_test_profile(NodeMobility::Mobile, 60);

        let node2 = FlatMeshCoordinator::new("node2".to_string(), node2_profile.clone(), None);

        // Create beacon for higher-capability peer
        let node1_profile = create_test_profile(NodeMobility::Static, 20);
        let node1_beacon =
            create_test_beacon("node1", GeoPosition::new(37.7749, -122.4194), node1_profile);

        // Node2 sees node1 (better capability)
        let role = node2.update_peers(vec![node1_beacon]).await;

        // Node2 should be Member (not Leader)
        assert_eq!(role, NodeRole::Member);
        assert!(!node2.is_leader().await);
    }
}
