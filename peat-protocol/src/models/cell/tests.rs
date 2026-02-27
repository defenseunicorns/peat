//! Tests for Cell CRDT operations

use super::*;
use crate::models::{Capability, CapabilityType};

#[test]
fn test_squad_add_remove_member() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    // Add members
    assert!(squad.add_member("node_1".to_string()));
    assert!(squad.add_member("node_2".to_string()));
    assert_eq!(squad.member_count(), 2);

    // Try to add duplicate
    assert!(!squad.add_member("node_1".to_string()));
    assert_eq!(squad.member_count(), 2);

    // Remove member
    assert!(squad.remove_member("node_1"));
    assert_eq!(squad.member_count(), 1);

    // Try to remove non-existent member
    assert!(!squad.remove_member("node_3"));
}

#[test]
fn test_squad_capacity() {
    let config = CellConfig::new(2);
    let mut squad = CellState::new(config);

    assert!(squad.add_member("node_1".to_string()));
    assert!(squad.add_member("node_2".to_string()));
    assert!(squad.is_full());

    // Can't add more when full
    assert!(!squad.add_member("node_3".to_string()));
}

#[test]
fn test_squad_leader_election() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());
    squad.add_member("node_2".to_string());

    // Set leader
    assert!(squad.set_leader("node_1".to_string()).is_ok());
    assert_eq!(squad.leader_id, Some("node_1".to_string()));
    assert!(squad.is_leader("node_1"));
    assert!(!squad.is_leader("node_2"));

    // Try to set non-member as leader
    assert!(squad.set_leader("node_3".to_string()).is_err());

    // Clear leader
    squad.clear_leader();
    assert_eq!(squad.leader_id, None);
}

#[test]
fn test_squad_leader_removal() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());
    squad.set_leader("node_1".to_string()).unwrap();

    // Remove leader - should clear leader_id
    squad.remove_member("node_1");
    assert_eq!(squad.leader_id, None);
}

#[test]
fn test_squad_capabilities() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    let cap1 = Capability::new(
        "camera_1".to_string(),
        "HD Camera".to_string(),
        CapabilityType::Sensor,
        0.9,
    );
    let cap2 = Capability::new(
        "gps_1".to_string(),
        "GPS".to_string(),
        CapabilityType::Sensor,
        1.0,
    );
    let cap3 = Capability::new(
        "compute_1".to_string(),
        "Edge Compute".to_string(),
        CapabilityType::Compute,
        0.8,
    );

    squad.add_capability(cap1.clone());
    squad.add_capability(cap2);
    squad.add_capability(cap3);

    assert_eq!(squad.capabilities.len(), 3);

    // Try to add duplicate
    squad.add_capability(cap1);
    assert_eq!(squad.capabilities.len(), 3);

    // Check capability types
    assert!(squad.has_capability_type(CapabilityType::Sensor));
    assert!(squad.has_capability_type(CapabilityType::Compute));
    assert!(!squad.has_capability_type(CapabilityType::Mobility));

    // Get by type
    let sensors = squad.get_capabilities_by_type(CapabilityType::Sensor);
    assert_eq!(sensors.len(), 2);
}

#[test]
fn test_squad_platoon_assignment() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    assert_eq!(squad.platoon_id, None);

    squad.assign_platoon("platoon_1".to_string());
    assert_eq!(squad.platoon_id, Some("platoon_1".to_string()));

    squad.leave_platoon();
    assert_eq!(squad.platoon_id, None);
}

#[test]
fn test_squad_merge() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let squad2 = CellState::new(config);

    // Squad1 has some members
    squad1.add_member("node_1".to_string());
    squad1.add_member("node_2".to_string());

    // Squad2 has different members
    let mut squad2_temp = squad2.clone();
    squad2_temp.add_member("node_2".to_string());
    squad2_temp.add_member("node_3".to_string());

    // Merge squad2 into squad1
    squad1.merge(&squad2_temp);

    // Should have union of members
    assert_eq!(squad1.member_count(), 3);
    assert!(squad1.is_member("node_1"));
    assert!(squad1.is_member("node_2"));
    assert!(squad1.is_member("node_3"));
}

#[test]
fn test_squad_merge_leader() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let mut squad2 = CellState::new(config);

    squad1.add_member("node_1".to_string());
    squad2.add_member("node_2".to_string());

    squad1.set_leader("node_1".to_string()).unwrap();

    // Squad2 has a later leader update
    std::thread::sleep(std::time::Duration::from_secs(1));
    squad2.set_leader("node_2".to_string()).unwrap();

    // Merge - squad2's leader should win (newer timestamp)
    squad1.merge(&squad2);
    assert_eq!(squad1.leader_id, Some("node_2".to_string()));
}

#[test]
fn test_squad_merge_capabilities() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let mut squad2 = CellState::new(config);

    let cap1 = Capability::new(
        "camera".to_string(),
        "Camera".to_string(),
        CapabilityType::Sensor,
        0.9,
    );
    let cap2 = Capability::new(
        "gps".to_string(),
        "GPS".to_string(),
        CapabilityType::Sensor,
        1.0,
    );

    squad1.add_capability(cap1);
    squad2.add_capability(cap2);

    squad1.merge(&squad2);

    // Should have union of capabilities
    assert_eq!(squad1.capabilities.len(), 2);
}

#[test]
fn test_squad_is_valid() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    // Not valid with 0 members (min_size is 2)
    assert!(!squad.is_valid());

    squad.add_member("node_1".to_string());
    assert!(!squad.is_valid());

    squad.add_member("node_2".to_string());
    assert!(squad.is_valid());
}

#[test]
fn test_cell_config_with_id() {
    let custom_id = "custom_cell_id".to_string();
    let config = CellConfig::with_id(custom_id.clone(), 10);

    assert_eq!(config.id, custom_id);
    assert_eq!(config.max_size, 10);
    assert_eq!(config.min_size, 2);
    assert!(config.created_at.is_some());
}

#[test]
fn test_cell_config_new_generates_uuid() {
    let config1 = CellConfig::new(5);
    let config2 = CellConfig::new(5);

    // Each config should have a unique ID
    assert_ne!(config1.id, config2.id);
}

#[test]
fn test_cell_state_get_id() {
    let config = CellConfig::with_id("test_id".to_string(), 5);
    let squad = CellState::new(config);

    assert_eq!(squad.get_id(), Some("test_id"));
}

#[test]
fn test_cell_state_get_id_no_config() {
    let mut squad = CellState::new(CellConfig::new(5));
    squad.config = None;

    assert_eq!(squad.get_id(), None);
}

#[test]
fn test_squad_add_member_when_full() {
    let config = CellConfig::new(2);
    let mut squad = CellState::new(config);

    assert!(squad.add_member("node_1".to_string()));
    assert!(squad.add_member("node_2".to_string()));
    assert!(squad.is_full());

    // Try to add when full - should fail
    assert!(!squad.add_member("node_3".to_string()));
    assert_eq!(squad.member_count(), 2);
}

#[test]
fn test_squad_remove_non_existent_member() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());

    // Try to remove member that doesn't exist
    assert!(!squad.remove_member("node_2"));
    assert!(!squad.remove_member(""));
    assert_eq!(squad.member_count(), 1);
}

#[test]
fn test_squad_set_leader_not_member() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());

    // Try to set leader who isn't a member
    let result = squad.set_leader("node_2".to_string());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Leader must be a squad member");
}

#[test]
fn test_squad_clear_leader() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());
    squad.set_leader("node_1".to_string()).unwrap();
    assert!(squad.is_leader("node_1"));

    squad.clear_leader();
    assert!(!squad.is_leader("node_1"));
    assert_eq!(squad.leader_id, None);
}

#[test]
fn test_squad_is_member() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.add_member("node_1".to_string());
    squad.add_member("node_2".to_string());

    assert!(squad.is_member("node_1"));
    assert!(squad.is_member("node_2"));
    assert!(!squad.is_member("node_3"));
    assert!(!squad.is_member(""));
}

#[test]
fn test_squad_capabilities_duplicate_handling() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    let cap = Capability::new(
        "cap_1".to_string(),
        "Capability 1".to_string(),
        CapabilityType::Sensor,
        0.9,
    );

    squad.add_capability(cap.clone());
    assert_eq!(squad.capabilities.len(), 1);

    // Add duplicate - should not increase count
    squad.add_capability(cap.clone());
    assert_eq!(squad.capabilities.len(), 1);

    // Add capability with different ID
    let cap2 = Capability::new(
        "cap_2".to_string(),
        "Capability 2".to_string(),
        CapabilityType::Sensor,
        0.8,
    );
    squad.add_capability(cap2);
    assert_eq!(squad.capabilities.len(), 2);
}

#[test]
fn test_squad_has_capability_type_empty() {
    let config = CellConfig::new(5);
    let squad = CellState::new(config);

    // No capabilities initially
    assert!(!squad.has_capability_type(CapabilityType::Sensor));
    assert!(!squad.has_capability_type(CapabilityType::Compute));
}

#[test]
fn test_squad_get_capabilities_by_type_empty() {
    let config = CellConfig::new(5);
    let squad = CellState::new(config);

    let caps = squad.get_capabilities_by_type(CapabilityType::Sensor);
    assert_eq!(caps.len(), 0);
}

#[test]
fn test_squad_merge_empty_squads() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let squad2 = CellState::new(config);

    // Both empty
    squad1.merge(&squad2);
    assert_eq!(squad1.member_count(), 0);
    assert_eq!(squad1.capabilities.len(), 0);
}

#[test]
fn test_squad_merge_with_older_timestamp() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let mut squad2 = CellState::new(config);

    squad1.add_member("node_1".to_string());

    // Update squad1's timestamp to be newer
    std::thread::sleep(std::time::Duration::from_millis(10));
    squad1.set_leader("node_1".to_string()).unwrap();

    // squad2 is older - its leader shouldn't win
    squad2.add_member("node_2".to_string());

    // Merge older squad2 into newer squad1
    squad1.merge(&squad2);

    // squad1's leader should remain
    assert_eq!(squad1.leader_id, Some("node_1".to_string()));

    // But members should be merged
    assert_eq!(squad1.member_count(), 2);
}

#[test]
fn test_squad_platoon_assignment_multiple_times() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    squad.assign_platoon("platoon_1".to_string());
    assert_eq!(squad.platoon_id, Some("platoon_1".to_string()));

    // Reassign to different platoon
    squad.assign_platoon("platoon_2".to_string());
    assert_eq!(squad.platoon_id, Some("platoon_2".to_string()));

    // Leave platoon
    squad.leave_platoon();
    assert_eq!(squad.platoon_id, None);
}

#[test]
fn test_squad_is_full_no_config() {
    let mut squad = CellState::new(CellConfig::new(5));
    squad.config = None;

    // Should return false when no config
    assert!(!squad.is_full());
}

#[test]
fn test_squad_is_valid_no_config() {
    let mut squad = CellState::new(CellConfig::new(5));
    squad.add_member("node_1".to_string());
    squad.add_member("node_2".to_string());

    squad.config = None;

    // Should return false when no config
    assert!(!squad.is_valid());
}

#[test]
fn test_squad_update_timestamp() {
    let config = CellConfig::new(5);
    let mut squad = CellState::new(config);

    let initial_ts = squad.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

    std::thread::sleep(std::time::Duration::from_millis(10));
    squad.update_timestamp();

    let new_ts = squad.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
    assert!(new_ts >= initial_ts);
}

#[test]
fn test_cell_config_default_min_size() {
    let config = CellConfig::new(10);

    // min_size should always be 2
    assert_eq!(config.min_size, 2);
}

#[test]
fn test_squad_merge_capabilities_union() {
    let config = CellConfig::new(5);
    let mut squad1 = CellState::new(config.clone());
    let mut squad2 = CellState::new(config);

    let cap1 = Capability::new(
        "cap_1".to_string(),
        "Cap 1".to_string(),
        CapabilityType::Sensor,
        0.9,
    );
    let cap2 = Capability::new(
        "cap_2".to_string(),
        "Cap 2".to_string(),
        CapabilityType::Compute,
        0.8,
    );
    let cap3 = Capability::new(
        "cap_3".to_string(),
        "Cap 3".to_string(),
        CapabilityType::Mobility,
        0.7,
    );

    squad1.add_capability(cap1.clone());
    squad1.add_capability(cap2.clone());

    squad2.add_capability(cap2.clone()); // Duplicate
    squad2.add_capability(cap3);

    squad1.merge(&squad2);

    // Should have 3 unique capabilities
    assert_eq!(squad1.capabilities.len(), 3);
}

#[test]
fn test_squad_merge_members_union() {
    let config = CellConfig::new(10);
    let mut squad1 = CellState::new(config.clone());
    let mut squad2 = CellState::new(config);

    squad1.add_member("node_1".to_string());
    squad1.add_member("node_2".to_string());

    squad2.add_member("node_2".to_string()); // Duplicate
    squad2.add_member("node_3".to_string());
    squad2.add_member("node_4".to_string());

    squad1.merge(&squad2);

    // Should have 4 unique members
    assert_eq!(squad1.member_count(), 4);
    assert!(squad1.is_member("node_1"));
    assert!(squad1.is_member("node_2"));
    assert!(squad1.is_member("node_3"));
    assert!(squad1.is_member("node_4"));
}
