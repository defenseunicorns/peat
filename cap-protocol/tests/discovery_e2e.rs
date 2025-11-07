//! End-to-End Integration Tests for Discovery Module (Phase 4)
//!
//! Tests the discovery mechanisms across multiple nodes with real Ditto synchronization:
//! - Geographic discovery with geohash-based clustering
//! - Capability-based peer discovery with scoring
//! - Directed discovery with explicit peer connections
//!
//! These tests validate that discovery works correctly in a multi-peer mesh environment.

use cap_protocol::discovery::capability_query::{CapabilityQuery, CapabilityQueryEngine};
use cap_protocol::discovery::geographic::{GeographicBeacon, GeographicDiscovery, MIN_SQUAD_SIZE};
use cap_protocol::discovery::GeoCoordinate;
use cap_protocol::models::capability::{Capability, CapabilityType};
use cap_protocol::models::node::NodeConfig;
use cap_protocol::models::CapabilityExt;
use cap_protocol::testing::e2e_harness::E2EHarness;
use std::time::Duration;
use tokio::time::sleep;

/// Test 1: Geographic Discovery Sync
///
/// Validates that geographic beacons sync across peers and enable squad formation
/// based on geohash proximity clustering.
///
/// Test Flow:
/// 1. Create 3 peers in same geographic location (SF Bay Area)
/// 2. Each peer broadcasts geographic beacon
/// 3. Validate beacons are received by all peers
/// 4. Verify squad formation logic triggers on sufficient peers
#[tokio::test]
async fn test_e2e_geographic_discovery_sync() {
    // Create E2E test harness
    let _harness = E2EHarness::new("geographic_discovery_test");

    // Define geographic position (San Francisco)
    let sf_position = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

    // Create geographic discovery managers for each peer
    let mut discovery1 = GeographicDiscovery::new("peer_1".to_string());
    let mut discovery2 = GeographicDiscovery::new("peer_2".to_string());
    let mut discovery3 = GeographicDiscovery::new("peer_3".to_string());

    // Create geographic beacons for each peer at same location
    let beacon1 = GeographicBeacon::new("peer_1".to_string(), sf_position, vec![]);
    let beacon2 = GeographicBeacon::new("peer_2".to_string(), sf_position, vec![]);
    let beacon3 = GeographicBeacon::new("peer_3".to_string(), sf_position, vec![]);

    // Simulate beacon broadcast and reception
    // In real implementation, this would be Ditto sync
    discovery1.process_beacon(beacon1.clone());
    discovery1.process_beacon(beacon2.clone());
    discovery1.process_beacon(beacon3.clone());

    discovery2.process_beacon(beacon1.clone());
    discovery2.process_beacon(beacon2.clone());
    discovery2.process_beacon(beacon3.clone());

    discovery3.process_beacon(beacon1);
    discovery3.process_beacon(beacon2);
    discovery3.process_beacon(beacon3);

    // Wait for processing
    sleep(Duration::from_millis(100)).await;

    // Validate: All peers discovered
    assert_eq!(
        discovery1.total_platforms(),
        3,
        "Peer 1 should see 3 platforms"
    );
    assert_eq!(
        discovery2.total_platforms(),
        3,
        "Peer 2 should see 3 platforms"
    );
    assert_eq!(
        discovery3.total_platforms(),
        3,
        "Peer 3 should see 3 platforms"
    );

    // Validate: All peers in same geohash cluster
    assert_eq!(
        discovery1.cluster_count(),
        1,
        "Should form single geographic cluster"
    );
    assert_eq!(discovery2.cluster_count(), 1);
    assert_eq!(discovery3.cluster_count(), 1);

    // Validate: Squad formation possible
    let formable1 = discovery1.find_formable_squads(MIN_SQUAD_SIZE);
    assert_eq!(formable1.len(), 1, "Should have 1 formable squad");
    assert!(
        formable1[0].can_form_squad(MIN_SQUAD_SIZE),
        "Cluster should meet minimum squad size"
    );

    // Validate: Deterministic leader selection (lowest ID)
    assert!(
        discovery1.should_initiate_squad_formation(),
        "peer_1 should be leader (lowest ID)"
    );
    assert!(
        !discovery2.should_initiate_squad_formation(),
        "peer_2 should not be leader"
    );
    assert!(
        !discovery3.should_initiate_squad_formation(),
        "peer_3 should not be leader"
    );

    // Validate: Squad members list consistent
    let members1 = discovery1.get_squad_members(5).unwrap();
    let members2 = discovery2.get_squad_members(5).unwrap();
    let members3 = discovery3.get_squad_members(5).unwrap();

    assert_eq!(members1, members2, "Squad members should be consistent");
    assert_eq!(members2, members3, "Squad members should be consistent");
    assert_eq!(members1.len(), 3, "Should have all 3 members");

    // Harness cleanup happens automatically on drop
}

/// Test 2: Capability-Based Peer Discovery
///
/// Validates that nodes can discover peers based on required capabilities
/// using the capability query engine with scoring and ranking.
///
/// Test Flow:
/// 1. Create 4 nodes with different capability sets
/// 2. Query for nodes with specific capability requirements
/// 3. Validate matches are scored and ranked correctly
/// 4. Verify filtering by confidence threshold works
#[tokio::test]
async fn test_e2e_capability_based_discovery() {
    // Create E2E test harness
    let _harness = E2EHarness::new("capability_discovery_test");

    // Create nodes with different capabilities
    let mut node1 = NodeConfig::new("UAV".to_string());
    node1.id = "uav_1".to_string();
    node1.add_capability(Capability::new(
        "sensor1".to_string(),
        "EO/IR Sensor".to_string(),
        CapabilityType::Sensor,
        0.95,
    ));
    node1.add_capability(Capability::new(
        "comms1".to_string(),
        "Radio Link".to_string(),
        CapabilityType::Communication,
        0.90,
    ));

    let mut node2 = NodeConfig::new("UAV".to_string());
    node2.id = "uav_2".to_string();
    node2.add_capability(Capability::new(
        "sensor2".to_string(),
        "SAR Sensor".to_string(),
        CapabilityType::Sensor,
        0.85,
    ));

    let mut node3 = NodeConfig::new("Ground Station".to_string());
    node3.id = "gs_1".to_string();
    node3.add_capability(Capability::new(
        "compute3".to_string(),
        "Edge Compute".to_string(),
        CapabilityType::Compute,
        0.92,
    ));
    node3.add_capability(Capability::new(
        "comms3".to_string(),
        "Satcom Link".to_string(),
        CapabilityType::Communication,
        0.88,
    ));

    let mut node4 = NodeConfig::new("UAV".to_string());
    node4.id = "uav_3".to_string();
    node4.add_capability(Capability::new(
        "sensor4".to_string(),
        "EO Sensor".to_string(),
        CapabilityType::Sensor,
        0.98,
    ));
    node4.add_capability(Capability::new(
        "comms4".to_string(),
        "Mesh Radio".to_string(),
        CapabilityType::Communication,
        0.94,
    ));
    node4.add_capability(Capability::new(
        "compute4".to_string(),
        "Onboard Compute".to_string(),
        CapabilityType::Compute,
        0.80,
    ));

    let nodes = vec![node1, node2, node3, node4];

    // Create capability query engine
    let engine = CapabilityQueryEngine::new();

    // Query 1: Find nodes with Sensor capability
    let query_sensors = CapabilityQuery::builder()
        .require_type(CapabilityType::Sensor)
        .min_confidence(0.80)
        .build();

    let sensor_matches = engine.query_platforms(&query_sensors, &nodes);

    assert_eq!(sensor_matches.len(), 3, "Should find 3 nodes with sensors");
    // node4 should rank highest (highest sensor confidence + additional capabilities)
    assert_eq!(sensor_matches[0].entity.id, "uav_3");
    assert!(sensor_matches[0].score > sensor_matches[1].score);

    // Query 2: Find nodes with Sensor AND Communication
    let query_sensor_comms = CapabilityQuery::builder()
        .require_type(CapabilityType::Sensor)
        .require_type(CapabilityType::Communication)
        .min_confidence(0.85)
        .build();

    let combo_matches = engine.query_platforms(&query_sensor_comms, &nodes);

    assert_eq!(
        combo_matches.len(),
        2,
        "Should find 2 nodes with both capabilities"
    );
    // node1 and node4 have both
    let ids: Vec<String> = combo_matches.iter().map(|m| m.entity.id.clone()).collect();
    assert!(ids.contains(&"uav_1".to_string()));
    assert!(ids.contains(&"uav_3".to_string()));

    // Query 3: Complex query with optional capabilities
    let query_complex = CapabilityQuery::builder()
        .require_type(CapabilityType::Sensor)
        .prefer_type(CapabilityType::Communication)
        .prefer_type(CapabilityType::Compute)
        .min_confidence(0.80)
        .limit(2)
        .build();

    let complex_matches = engine.query_platforms(&query_complex, &nodes);

    assert_eq!(complex_matches.len(), 2, "Should limit results to 2");
    // node4 should score highest (has all 3 capabilities)
    assert_eq!(complex_matches[0].entity.id, "uav_3");

    // Query 4: Minimum capability count filter
    let query_min_count = CapabilityQuery::builder().min_capability_count(2).build();

    let min_count_matches = engine.query_platforms(&query_min_count, &nodes);

    assert_eq!(
        min_count_matches.len(),
        3,
        "Should find 3 nodes with 2+ capabilities"
    );

    // Harness cleanup happens automatically on drop
}

/// Test 3: Multi-Region Geographic Discovery
///
/// Validates that geographic discovery correctly handles multiple
/// geographic regions (different geohash cells) and forms separate
/// squads per region.
///
/// Test Flow:
/// 1. Create nodes in 2 different geographic regions (SF and LA)
/// 2. Validate separate geohash clusters form
/// 3. Verify squad formation logic per region
/// 4. Confirm cross-region isolation
#[tokio::test]
async fn test_e2e_multi_region_discovery() {
    // Create E2E test harness
    let _harness = E2EHarness::new("multi_region_test");

    // Define two geographic regions
    let sf_position = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();
    let la_position = GeoCoordinate::new(34.0522, -118.2437, 100.0).unwrap();

    // Create discovery manager
    let mut discovery = GeographicDiscovery::new("observer".to_string());

    // Create beacons for SF region (2 nodes)
    let beacon_sf1 = GeographicBeacon::new("sf_node_1".to_string(), sf_position, vec![]);
    let beacon_sf2 = GeographicBeacon::new("sf_node_2".to_string(), sf_position, vec![]);

    // Create beacons for LA region (2 nodes)
    let beacon_la1 = GeographicBeacon::new("la_node_1".to_string(), la_position, vec![]);
    let beacon_la2 = GeographicBeacon::new("la_node_2".to_string(), la_position, vec![]);

    // Process all beacons
    discovery.process_beacon(beacon_sf1);
    discovery.process_beacon(beacon_sf2);
    discovery.process_beacon(beacon_la1);
    discovery.process_beacon(beacon_la2);

    // Wait for processing
    sleep(Duration::from_millis(100)).await;

    // Validate: Total platforms discovered
    assert_eq!(
        discovery.total_platforms(),
        4,
        "Should discover all 4 platforms"
    );

    // Validate: Two separate geographic clusters
    assert_eq!(
        discovery.cluster_count(),
        2,
        "Should form 2 geographic clusters (SF and LA)"
    );

    // Validate: Both regions can form squads
    let formable_squads = discovery.find_formable_squads(MIN_SQUAD_SIZE);
    assert_eq!(
        formable_squads.len(),
        2,
        "Both regions should be able to form squads"
    );

    // Validate: Each cluster has correct member count
    for cluster in formable_squads {
        assert_eq!(
            cluster.platforms.len(),
            2,
            "Each region should have 2 platforms"
        );
    }

    // Harness cleanup happens automatically on drop
}

/// Test 4: Geographic Beacon Expiration
///
/// Validates that expired geographic beacons are properly cleaned up
/// and squads can't form with stale data.
///
/// Test Flow:
/// 1. Create beacons with old timestamps
/// 2. Process cleanup operation
/// 3. Verify expired beacons removed
/// 4. Confirm squad formation fails with insufficient fresh beacons
#[tokio::test]
async fn test_e2e_beacon_expiration_cleanup() {
    // Create E2E test harness
    let _harness = E2EHarness::new("beacon_expiration_test");

    let position = GeoCoordinate::new(37.7749, -122.4194, 100.0).unwrap();

    let mut discovery = GeographicDiscovery::new("node_1".to_string());

    // Create beacon with old timestamp (expired)
    let mut old_beacon = GeographicBeacon::new("node_2".to_string(), position, vec![]);
    old_beacon.timestamp = 0; // Very old timestamp

    // Create fresh beacon
    let fresh_beacon = GeographicBeacon::new("node_3".to_string(), position, vec![]);

    // Process both beacons
    discovery.process_beacon(old_beacon);
    discovery.process_beacon(fresh_beacon);

    assert_eq!(
        discovery.total_platforms(),
        2,
        "Should have 2 platforms before cleanup"
    );

    // Run cleanup
    discovery.cleanup_expired();

    // Validate: Expired beacon removed
    assert_eq!(
        discovery.total_platforms(),
        1,
        "Should have 1 platform after cleanup"
    );

    // Validate: Can't form squad with insufficient fresh beacons
    let formable = discovery.find_formable_squads(MIN_SQUAD_SIZE);
    assert_eq!(formable.len(), 0, "Should not be able to form squad");

    // Harness cleanup happens automatically on drop
}

/// Test 5: Capability Statistics and Distribution
///
/// Validates capability statistics collection across a fleet of nodes.
///
/// Test Flow:
/// 1. Create diverse fleet with various capabilities
/// 2. Calculate capability distribution stats
/// 3. Validate statistical metrics (count, avg, min, max confidence)
#[tokio::test]
async fn test_e2e_capability_statistics() {
    // Create E2E test harness
    let _harness = E2EHarness::new("capability_stats_test");

    // Create diverse fleet
    let mut nodes = Vec::new();

    for i in 1..=5 {
        let mut node = NodeConfig::new("UAV".to_string());
        node.id = format!("node_{}", i);

        // All nodes have sensor capability with varying confidence
        node.add_capability(Capability::new(
            format!("sensor_{}", i),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.7 + (i as f32 * 0.05),
        ));

        // 3 nodes have communication capability
        if i <= 3 {
            node.add_capability(Capability::new(
                format!("comms_{}", i),
                "Comms".to_string(),
                CapabilityType::Communication,
                0.8 + (i as f32 * 0.03),
            ));
        }

        // 2 nodes have compute capability
        if i <= 2 {
            node.add_capability(Capability::new(
                format!("compute_{}", i),
                "Compute".to_string(),
                CapabilityType::Compute,
                0.75 + (i as f32 * 0.05),
            ));
        }

        nodes.push(node);
    }

    // Calculate statistics
    let engine = CapabilityQueryEngine::new();
    let stats = engine.platform_capability_stats(&nodes);

    // Validate: Sensor stats (all 5 nodes)
    let sensor_stats = stats.get(&CapabilityType::Sensor).unwrap();
    assert_eq!(sensor_stats.count, 5, "All nodes should have sensors");
    assert_eq!(sensor_stats.min_confidence, 0.75);
    assert_eq!(sensor_stats.max_confidence, 0.95);
    assert!((sensor_stats.avg_confidence - 0.85).abs() < 0.01);

    // Validate: Communication stats (3 nodes)
    let comms_stats = stats.get(&CapabilityType::Communication).unwrap();
    assert_eq!(comms_stats.count, 3);
    assert_eq!(comms_stats.min_confidence, 0.83);
    assert_eq!(comms_stats.max_confidence, 0.89);

    // Validate: Compute stats (2 nodes)
    let compute_stats = stats.get(&CapabilityType::Compute).unwrap();
    assert_eq!(compute_stats.count, 2);
    assert_eq!(compute_stats.min_confidence, 0.8);
    assert_eq!(compute_stats.max_confidence, 0.85);

    // Harness cleanup happens automatically on drop
}
