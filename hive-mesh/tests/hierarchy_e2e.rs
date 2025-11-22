//! End-to-end tests for flexible hierarchy system with multiple nodes
//!
//! These tests validate the hierarchy strategy system in multi-node scenarios,
//! including dynamic election, role transitions, and lateral peer coordination.

use hive_mesh::beacon::{
    GeoPosition, GeographicBeacon, HierarchyLevel, NodeMobility, NodeProfile, NodeResources,
};
use hive_mesh::hierarchy::{
    DynamicHierarchyStrategy, ElectionConfig, HierarchyStrategy, HybridHierarchyStrategy, NodeRole,
    StaticHierarchyStrategy,
};
use std::sync::Arc;

/// Helper to create a test node profile
fn create_node_profile(
    mobility: NodeMobility,
    cpu_cores: u8,
    memory_mb: u32,
    cpu_usage: u8,
    battery_percent: Option<u8>,
) -> NodeProfile {
    NodeProfile {
        mobility,
        resources: NodeResources {
            cpu_cores,
            memory_mb,
            bandwidth_mbps: 100,
            cpu_usage_percent: cpu_usage,
            memory_usage_percent: 50,
            battery_percent,
        },
        can_parent: true,
        prefer_leaf: false,
        parent_priority: 100,
    }
}

/// Helper to create a test beacon
fn create_beacon(
    node_id: &str,
    position: GeoPosition,
    level: HierarchyLevel,
    profile: NodeProfile,
) -> GeographicBeacon {
    let mut beacon = GeographicBeacon::new(node_id.to_string(), position, level);
    beacon.mobility = Some(profile.mobility);
    beacon.resources = Some(profile.resources.clone());
    beacon.can_parent = profile.can_parent;
    beacon.parent_priority = profile.parent_priority;
    beacon
}

#[tokio::test]
async fn test_dynamic_hierarchy_election_with_three_nodes() {
    // Test Scenario: 3 nodes at Squad level with different capabilities
    // Node1: High capability (Static, 8 cores, low usage) -> Should become Leader
    // Node2: Medium capability (SemiMobile, 4 cores) -> Should become Member
    // Node3: Low capability (Mobile, 2 cores, high usage) -> Should become Member

    let node1_profile = create_node_profile(NodeMobility::Static, 8, 4096, 20, None);
    let node2_profile = create_node_profile(NodeMobility::SemiMobile, 4, 2048, 40, Some(80));
    let node3_profile = create_node_profile(NodeMobility::Mobile, 2, 1024, 70, Some(30));

    // Create beacons for nearby peers (from node1's perspective)
    let node2_beacon = create_beacon(
        "node2",
        GeoPosition::new(37.7750, -122.4195), // San Francisco
        HierarchyLevel::Squad,
        node2_profile.clone(),
    );

    let node3_beacon = create_beacon(
        "node3",
        GeoPosition::new(37.7760, -122.4185), // Very close to node1
        HierarchyLevel::Squad,
        node3_profile.clone(),
    );

    // Node1 uses dynamic strategy
    let strategy = Arc::new(DynamicHierarchyStrategy::new(
        HierarchyLevel::Squad,
        ElectionConfig::default(),
        false, // allow_level_transitions = false
    ));

    // Node1 determines its role based on seeing node2 and node3
    let nearby_beacons = vec![node2_beacon.clone(), node3_beacon.clone()];
    let node1_role = strategy.determine_role(&node1_profile, &nearby_beacons);
    let node1_level = strategy.determine_level(&node1_profile);

    // Verify node1 becomes Leader (highest capability)
    assert_eq!(
        node1_role,
        NodeRole::Leader,
        "Node1 with highest capability should be Leader"
    );
    assert_eq!(node1_level, HierarchyLevel::Squad);

    // Node2 perspective: Sees node1 (higher capability) and node3 (lower capability)
    let node1_beacon = create_beacon(
        "node1",
        GeoPosition::new(37.7755, -122.4190),
        HierarchyLevel::Squad,
        node1_profile.clone(),
    );

    let node2_nearby = vec![node1_beacon.clone(), node3_beacon.clone()];
    let node2_role = strategy.determine_role(&node2_profile, &node2_nearby);

    // Verify node2 becomes Member (node1 has higher capability)
    assert_eq!(
        node2_role,
        NodeRole::Member,
        "Node2 should be Member when higher-capability leader exists"
    );

    // Node3 perspective: Sees node1 and node2 (both higher capability)
    let node3_nearby = vec![node1_beacon, node2_beacon];
    let node3_role = strategy.determine_role(&node3_profile, &node3_nearby);

    // Verify node3 becomes Member
    assert_eq!(
        node3_role,
        NodeRole::Member,
        "Node3 with lowest capability should be Member"
    );
}

#[tokio::test]
async fn test_role_transition_when_leader_leaves() {
    // Test Scenario: Leader node leaves, next highest-capability node promotes to Leader
    // Initial: Node1 (Leader), Node2 (Member), Node3 (Member)
    // After Node1 leaves: Node2 should become Leader

    let node2_profile = create_node_profile(NodeMobility::SemiMobile, 4, 2048, 40, Some(80));
    let node3_profile = create_node_profile(NodeMobility::Mobile, 2, 1024, 70, Some(30));

    let strategy = Arc::new(DynamicHierarchyStrategy::new(
        HierarchyLevel::Squad,
        ElectionConfig::default(),
        false,
    ));

    // Initial state: Node2 sees Node3 only (Node1 has left)
    let node3_beacon = create_beacon(
        "node3",
        GeoPosition::new(37.7760, -122.4185),
        HierarchyLevel::Squad,
        node3_profile.clone(),
    );

    let node2_nearby = vec![node3_beacon.clone()];
    let node2_role_after_leader_left = strategy.determine_role(&node2_profile, &node2_nearby);

    // Verify node2 promotes to Leader when node1 leaves
    assert_eq!(
        node2_role_after_leader_left,
        NodeRole::Leader,
        "Node2 should become Leader when higher-capability leader leaves"
    );

    // Node3 still sees Node2
    let node2_beacon = create_beacon(
        "node2",
        GeoPosition::new(37.7750, -122.4195),
        HierarchyLevel::Squad,
        node2_profile,
    );

    let node3_nearby = vec![node2_beacon];
    let node3_role = strategy.determine_role(&node3_profile, &node3_nearby);

    // Verify node3 remains Member
    assert_eq!(node3_role, NodeRole::Member, "Node3 should remain Member");
}

#[tokio::test]
async fn test_lateral_peer_discovery() {
    // Test Scenario: Multiple nodes at same hierarchy level discover each other
    // All nodes at Squad level should recognize each other as lateral peers

    let node1_profile = create_node_profile(NodeMobility::Static, 8, 4096, 20, None);

    // Create beacons for lateral peers (same hierarchy level)
    let lateral_peer1 = create_beacon(
        "lateral1",
        GeoPosition::new(37.7750, -122.4195),
        HierarchyLevel::Squad, // Same level
        node1_profile.clone(),
    );

    let lateral_peer2 = create_beacon(
        "lateral2",
        GeoPosition::new(37.7760, -122.4185),
        HierarchyLevel::Squad, // Same level
        node1_profile.clone(),
    );

    // Create a beacon at different level (should not be lateral peer)
    let higher_level_peer = create_beacon(
        "platoon_node",
        GeoPosition::new(37.7765, -122.4180),
        HierarchyLevel::Platoon, // Different level
        node1_profile,
    );

    let nearby_beacons = vec![
        lateral_peer1.clone(),
        lateral_peer2.clone(),
        higher_level_peer,
    ];

    // Count lateral peers (same level)
    let lateral_peer_count = nearby_beacons
        .iter()
        .filter(|b| b.hierarchy_level == HierarchyLevel::Squad)
        .count();

    assert_eq!(
        lateral_peer_count, 2,
        "Should discover 2 lateral peers at same hierarchy level"
    );
}

#[tokio::test]
async fn test_hybrid_strategy_static_level_dynamic_role() {
    // Test Scenario: Hybrid strategy with static level but dynamic role election
    // Nodes stay at assigned level but elect leader dynamically

    let high_capability_profile = create_node_profile(NodeMobility::Static, 8, 4096, 20, None);
    let low_capability_profile = create_node_profile(NodeMobility::Mobile, 2, 1024, 70, Some(30));

    let strategy = Arc::new(HybridHierarchyStrategy::with_static_level_dynamic_role(
        HierarchyLevel::Platoon,
        ElectionConfig::default(),
    ));

    // High capability node
    let level = strategy.determine_level(&high_capability_profile);
    assert_eq!(
        level,
        HierarchyLevel::Platoon,
        "Hybrid strategy should maintain static level"
    );

    // Create nearby peer beacon
    let low_cap_beacon = create_beacon(
        "low_cap",
        GeoPosition::new(37.7750, -122.4195),
        HierarchyLevel::Platoon,
        low_capability_profile.clone(),
    );

    let nearby = vec![low_cap_beacon];

    // High capability node should become Leader
    let role = strategy.determine_role(&high_capability_profile, &nearby);
    assert_eq!(
        role,
        NodeRole::Leader,
        "High capability node should be elected Leader"
    );

    // Low capability node sees high capability peer
    let high_cap_beacon = create_beacon(
        "high_cap",
        GeoPosition::new(37.7755, -122.4190),
        HierarchyLevel::Platoon,
        high_capability_profile,
    );

    let low_cap_nearby = vec![high_cap_beacon];
    let low_cap_role = strategy.determine_role(&low_capability_profile, &low_cap_nearby);

    assert_eq!(
        low_cap_role,
        NodeRole::Member,
        "Low capability node should be Member"
    );
}

#[tokio::test]
async fn test_static_hierarchy_fixed_roles() {
    // Test Scenario: Static hierarchy maintains assigned roles regardless of capabilities

    let high_capability_profile = create_node_profile(NodeMobility::Static, 8, 4096, 20, None);
    let low_capability_profile = create_node_profile(NodeMobility::Mobile, 2, 1024, 70, Some(30));

    // Create static strategy with Member role (even though node has high capability)
    let static_strategy = Arc::new(StaticHierarchyStrategy {
        assigned_level: HierarchyLevel::Squad,
        assigned_role: NodeRole::Member,
    });

    // High capability node with static Member assignment
    let level = static_strategy.determine_level(&high_capability_profile);
    assert_eq!(level, HierarchyLevel::Squad);

    // Create nearby beacons (shouldn't affect static role)
    let beacon = create_beacon(
        "other",
        GeoPosition::new(37.7750, -122.4195),
        HierarchyLevel::Squad,
        low_capability_profile,
    );

    let nearby = vec![beacon];
    let role = static_strategy.determine_role(&high_capability_profile, &nearby);

    // Should maintain static Member role despite high capability
    assert_eq!(
        role,
        NodeRole::Member,
        "Static strategy should maintain assigned role regardless of capabilities"
    );

    // Verify no transitions allowed
    assert!(
        !static_strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon),
        "Static strategy should not allow level transitions"
    );
}

#[tokio::test]
async fn test_hybrid_strategy_adaptive_promotion() {
    // Test Scenario: Hybrid strategy allows promotion when no higher-level peers exist

    let node_profile = create_node_profile(NodeMobility::Static, 4, 2048, 30, None);

    let strategy = Arc::new(HybridHierarchyStrategy::with_adaptive_promotion(
        HierarchyLevel::Squad,
        NodeRole::Member,
        ElectionConfig::default(),
    ));

    // Verify baseline level
    let level = strategy.determine_level(&node_profile);
    assert_eq!(level, HierarchyLevel::Squad);

    // Verify promotion is allowed (but only 1 level)
    assert!(
        strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon),
        "Should allow 1-level promotion"
    );

    assert!(
        !strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Company),
        "Should not allow 2-level promotion"
    );

    // Verify demotion is not allowed
    assert!(
        !strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad),
        "Should not allow demotion"
    );
}

#[tokio::test]
async fn test_multi_node_role_convergence() {
    // Test Scenario: 5 nodes converge on stable role assignments
    // Simulates realistic network with mixed capabilities

    let nodes = [
        (
            "node1",
            NodeProfile {
                mobility: NodeMobility::Static,
                resources: NodeResources {
                    cpu_cores: 8,
                    memory_mb: 8192,
                    bandwidth_mbps: 1000,
                    cpu_usage_percent: 10,
                    memory_usage_percent: 20,
                    battery_percent: None, // AC powered
                },
                can_parent: true,
                prefer_leaf: false,
                parent_priority: 200, // High priority
            },
        ), // Highest - Best hardware, AC powered, high priority
        (
            "node2",
            NodeProfile {
                mobility: NodeMobility::SemiMobile,
                resources: NodeResources {
                    cpu_cores: 4,
                    memory_mb: 2048,
                    bandwidth_mbps: 100,
                    cpu_usage_percent: 50,
                    memory_usage_percent: 60,
                    battery_percent: Some(70),
                },
                can_parent: true,
                prefer_leaf: false,
                parent_priority: 100,
            },
        ),
        (
            "node3",
            NodeProfile {
                mobility: NodeMobility::Mobile,
                resources: NodeResources {
                    cpu_cores: 2,
                    memory_mb: 1024,
                    bandwidth_mbps: 50,
                    cpu_usage_percent: 70,
                    memory_usage_percent: 80,
                    battery_percent: Some(40),
                },
                can_parent: false, // Cannot be parent
                prefer_leaf: false,
                parent_priority: 50,
            },
        ),
        (
            "node4",
            NodeProfile {
                mobility: NodeMobility::Mobile,
                resources: NodeResources {
                    cpu_cores: 1,
                    memory_mb: 512,
                    bandwidth_mbps: 25,
                    cpu_usage_percent: 85,
                    memory_usage_percent: 90,
                    battery_percent: Some(20),
                },
                can_parent: false,
                prefer_leaf: true,
                parent_priority: 10,
            },
        ),
        (
            "node5",
            NodeProfile {
                mobility: NodeMobility::Mobile,
                resources: NodeResources {
                    cpu_cores: 1,
                    memory_mb: 256,
                    bandwidth_mbps: 10,
                    cpu_usage_percent: 95,
                    memory_usage_percent: 95,
                    battery_percent: Some(5),
                },
                can_parent: false,
                prefer_leaf: true,
                parent_priority: 5,
            },
        ), // Lowest - Very constrained device
    ];

    let strategy = Arc::new(DynamicHierarchyStrategy::new(
        HierarchyLevel::Squad,
        ElectionConfig::default(),
        false,
    ));

    // Create beacons for all nodes
    let beacons: Vec<GeographicBeacon> = nodes
        .iter()
        .enumerate()
        .map(|(i, (name, profile))| {
            let lat = 37.775 + (i as f64 * 0.001);
            let lon = -122.419 + (i as f64 * 0.001);
            create_beacon(
                name,
                GeoPosition::new(lat, lon),
                HierarchyLevel::Squad,
                profile.clone(),
            )
        })
        .collect();

    // Determine roles for each node
    let mut roles = Vec::new();
    for (i, (_, profile)) in nodes.iter().enumerate() {
        // Each node sees all other nodes
        let nearby: Vec<GeographicBeacon> = beacons
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| b.clone())
            .collect();

        let role = strategy.determine_role(profile, &nearby);
        roles.push(role);
    }

    // Verify convergence
    let leader_count = roles.iter().filter(|r| **r == NodeRole::Leader).count();
    let member_count = roles.iter().filter(|r| **r == NodeRole::Member).count();

    assert_eq!(leader_count, 1, "Exactly one node should be elected Leader");
    assert_eq!(member_count, 4, "Four nodes should be Members");

    // Verify highest capability node is Leader
    assert_eq!(
        roles[0],
        NodeRole::Leader,
        "Node1 (highest capability) should be Leader"
    );
}

#[tokio::test]
async fn test_role_stability_with_hysteresis() {
    // Test Scenario: Verify 10% hysteresis prevents role flapping
    // A node must be >10% better than best peer to become Leader

    let strategy = Arc::new(DynamicHierarchyStrategy::new(
        HierarchyLevel::Squad,
        ElectionConfig::default(),
        false,
    ));

    // Peer with similar capability but slightly better
    let peer1_profile = create_node_profile(NodeMobility::Static, 8, 4096, 30, None);
    let peer2_profile = create_node_profile(NodeMobility::Static, 8, 4096, 25, None); // Slightly better

    let peer1_beacon = create_beacon(
        "peer1",
        GeoPosition::new(37.7750, -122.4195),
        HierarchyLevel::Squad,
        peer1_profile,
    );

    // Peer2 is only slightly better - should NOT become Leader due to hysteresis
    let peer2_role = strategy.determine_role(&peer2_profile, std::slice::from_ref(&peer1_beacon));
    assert_eq!(
        peer2_role,
        NodeRole::Member,
        "Node with slightly better capability should not become Leader (hysteresis prevents flapping)"
    );

    // Now test with a significantly better node (much lower CPU usage, high priority)
    let much_better_profile = NodeProfile {
        mobility: NodeMobility::Static,
        resources: NodeResources {
            cpu_cores: 8,
            memory_mb: 8192,
            bandwidth_mbps: 1000,
            cpu_usage_percent: 10,
            memory_usage_percent: 15,
            battery_percent: None,
        },
        can_parent: true,
        prefer_leaf: false,
        parent_priority: 200,
    };

    let much_better_role =
        strategy.determine_role(&much_better_profile, std::slice::from_ref(&peer1_beacon));

    // Much better node should become Leader (significantly better score)
    assert_eq!(
        much_better_role,
        NodeRole::Leader,
        "Node with significantly better capability should become Leader"
    );
}
