//! Integration tests for network partition detection and autonomous operation
//!
//! These tests validate the integration between PartitionDetector,
//! AutonomousOperationHandler, and TopologyBuilder in realistic scenarios.

use hive_mesh::beacon::{GeoPosition, GeographicBeacon, HierarchyLevel};
use hive_mesh::topology::{
    AutonomousOperationHandler, AutonomousState, PartitionConfig, PartitionDetector, TopologyConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

/// Helper to create a beacon at a specific hierarchy level
fn create_beacon(node_id: &str, level: HierarchyLevel, operational: bool) -> GeographicBeacon {
    let position = GeoPosition::new(37.7749, -122.4194);
    let mut beacon = GeographicBeacon::new(node_id.to_string(), position, level);
    beacon.operational = operational;
    beacon
}

/// Helper to create beacon map for partition detector
fn create_beacon_map(beacons: Vec<GeographicBeacon>) -> HashMap<String, GeographicBeacon> {
    beacons
        .into_iter()
        .map(|b| (b.node_id.clone(), b))
        .collect()
}

#[test]
fn test_partition_detector_with_autonomous_handler_integration() {
    // Create autonomous operation handler
    let handler = Arc::new(AutonomousOperationHandler::new());
    assert!(!handler.is_autonomous());

    // Create partition detector with short backoff for testing
    let config = PartitionConfig {
        min_detection_attempts: 2,
        initial_backoff: Duration::from_millis(1),
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Initially no partition - have higher-level beacons
    let beacons_with_parent = create_beacon_map(vec![
        create_beacon("squad1", HierarchyLevel::Squad, true),
        create_beacon("platform1", HierarchyLevel::Platform, true),
    ]);

    let event1 = detector.check_partition(&beacons_with_parent);
    assert!(event1.is_none());
    assert!(!handler.is_autonomous());

    // Simulate partition - no higher-level beacons visible
    let beacons_empty = HashMap::new();

    // First detection attempt
    let event2 = detector.check_partition(&beacons_empty);
    assert!(event2.is_none()); // Not enough attempts yet
    assert!(!handler.is_autonomous());

    // Wait for backoff
    std::thread::sleep(Duration::from_millis(2));

    // Second detection attempt - should detect partition
    let event3 = detector.check_partition(&beacons_empty);
    assert!(event3.is_some());
    assert!(handler.is_autonomous());

    // Verify autonomous state details
    match handler.get_state() {
        AutonomousState::Autonomous {
            detection_attempts, ..
        } => {
            assert_eq!(detection_attempts, 2);
        }
        _ => panic!("Expected Autonomous state"),
    }

    // Simulate partition healing - higher-level beacons visible again
    let event4 = detector.check_partition(&beacons_with_parent);
    assert!(event4.is_some());
    assert!(!handler.is_autonomous());
    assert_eq!(handler.get_state(), AutonomousState::Normal);
}

#[test]
fn test_topology_config_with_partition_detector() {
    // Create autonomous handler and partition detector
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 3,
        initial_backoff: Duration::from_secs(2),
        ..Default::default()
    };

    let detector = Arc::new(Mutex::new(
        PartitionDetector::new(HierarchyLevel::Platform, config)
            .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>),
    ));

    // Create TopologyConfig with partition detector
    let topology_config = TopologyConfig {
        partition_detector: Some(detector.clone()),
        ..Default::default()
    };

    // Verify detector is present in config
    assert!(topology_config.partition_detector.is_some());

    // Simulate partition detection through detector
    let beacons_empty = HashMap::new();

    // First attempt (detection_attempts: 0 -> 1)
    detector.lock().unwrap().check_partition(&beacons_empty);
    assert!(!handler.is_autonomous());
    assert_eq!(detector.lock().unwrap().detection_attempts(), 1);

    // Wait for backoff(1) = 2s * 2^1 = 4s
    std::thread::sleep(Duration::from_secs(4));

    // Second attempt (detection_attempts: 1 -> 2)
    detector.lock().unwrap().check_partition(&beacons_empty);
    assert!(!handler.is_autonomous()); // Still need one more attempt
    assert_eq!(detector.lock().unwrap().detection_attempts(), 2);

    // Wait for backoff(2) = 2s * 2^2 = 8s
    std::thread::sleep(Duration::from_secs(8));

    // Third attempt (detection_attempts: 2 -> 3) - should detect partition
    detector.lock().unwrap().check_partition(&beacons_empty);
    assert!(handler.is_autonomous()); // Should now be autonomous
    assert_eq!(detector.lock().unwrap().detection_attempts(), 3);
}

#[test]
fn test_partition_detection_respects_operational_flag() {
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 1,
        initial_backoff: Duration::from_millis(1),
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Have higher-level beacon but it's NOT operational
    let beacons_non_operational = create_beacon_map(vec![create_beacon(
        "squad1",
        HierarchyLevel::Squad,
        false, // NOT operational
    )]);

    // Should detect partition because non-operational beacons don't count
    let event = detector.check_partition(&beacons_non_operational);
    assert!(event.is_some());
    assert!(handler.is_autonomous());
}

#[test]
fn test_partition_healing_resets_detection_state() {
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 2,
        initial_backoff: Duration::from_millis(1),
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Detect partition
    let beacons_empty = HashMap::new();
    detector.check_partition(&beacons_empty);
    std::thread::sleep(Duration::from_millis(2));
    detector.check_partition(&beacons_empty);
    assert!(handler.is_autonomous());
    assert_eq!(detector.detection_attempts(), 2);

    // Heal partition
    let beacons_with_parent =
        create_beacon_map(vec![create_beacon("squad1", HierarchyLevel::Squad, true)]);
    detector.check_partition(&beacons_with_parent);
    assert!(!handler.is_autonomous());

    // Verify detection state was reset
    assert_eq!(detector.detection_attempts(), 0);
    assert!(!detector.is_partitioned());
}

#[test]
fn test_multiple_higher_level_beacons_prevent_partition() {
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 1,
        initial_backoff: Duration::from_millis(1),
        min_higher_level_beacons: 1, // Default: need at least 1
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Multiple higher-level beacons visible
    let beacons_multiple = create_beacon_map(vec![
        create_beacon("squad1", HierarchyLevel::Squad, true),
        create_beacon("platoon1", HierarchyLevel::Platoon, true),
    ]);

    // Should NOT detect partition
    let event = detector.check_partition(&beacons_multiple);
    assert!(event.is_none());
    assert!(!handler.is_autonomous());
}

#[test]
fn test_company_level_node_always_partitioned() {
    // Company is the highest level, so no higher-level beacons exist
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 1,
        initial_backoff: Duration::from_millis(1),
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Company, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Even with lower-level beacons, Company node should detect partition
    let beacons_lower_levels = create_beacon_map(vec![
        create_beacon("squad1", HierarchyLevel::Squad, true),
        create_beacon("platoon1", HierarchyLevel::Platoon, true),
        create_beacon("platform1", HierarchyLevel::Platform, true),
    ]);

    // Should detect partition (no higher levels exist)
    let event = detector.check_partition(&beacons_lower_levels);
    assert!(event.is_some());
    assert!(handler.is_autonomous());
}

#[test]
fn test_partition_duration_tracking() {
    let handler = Arc::new(AutonomousOperationHandler::new());
    let config = PartitionConfig {
        min_detection_attempts: 1,
        initial_backoff: Duration::from_millis(1),
        ..Default::default()
    };

    let mut detector = PartitionDetector::new(HierarchyLevel::Platform, config)
        .with_handler(handler.clone() as Arc<dyn hive_mesh::topology::PartitionHandler>);

    // Enter partition
    let beacons_empty = HashMap::new();
    detector.check_partition(&beacons_empty);
    assert!(handler.is_autonomous());

    // Simulate some time in autonomous mode
    std::thread::sleep(Duration::from_millis(50));

    // Heal partition
    let beacons_with_parent =
        create_beacon_map(vec![create_beacon("squad1", HierarchyLevel::Squad, true)]);
    let heal_event = detector.check_partition(&beacons_with_parent);

    // Verify we exited autonomous mode
    assert!(!handler.is_autonomous());

    // Verify event has partition duration
    assert!(heal_event.is_some());
    if let Some(hive_mesh::topology::PartitionEvent::PartitionHealed {
        partition_duration, ..
    }) = heal_event
    {
        // Duration should be at least 50ms
        assert!(partition_duration >= Duration::from_millis(50));
    } else {
        panic!("Expected PartitionHealed event");
    }
}
