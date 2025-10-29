//! Tests for Squad CRDT operations

use super::*;
use crate::models::{Capability, CapabilityType};

#[test]
fn test_squad_add_remove_member() {
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

    // Add members
    assert!(squad.add_member("platform_1".to_string()));
    assert!(squad.add_member("platform_2".to_string()));
    assert_eq!(squad.member_count(), 2);

    // Try to add duplicate
    assert!(!squad.add_member("platform_1".to_string()));
    assert_eq!(squad.member_count(), 2);

    // Remove member
    assert!(squad.remove_member("platform_1"));
    assert_eq!(squad.member_count(), 1);

    // Try to remove non-existent member
    assert!(!squad.remove_member("platform_3"));
}

#[test]
fn test_squad_capacity() {
    let config = SquadConfig::new(2);
    let mut squad = SquadState::new(config);

    assert!(squad.add_member("platform_1".to_string()));
    assert!(squad.add_member("platform_2".to_string()));
    assert!(squad.is_full());

    // Can't add more when full
    assert!(!squad.add_member("platform_3".to_string()));
}

#[test]
fn test_squad_leader_election() {
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

    squad.add_member("platform_1".to_string());
    squad.add_member("platform_2".to_string());

    // Set leader
    assert!(squad.set_leader("platform_1".to_string()).is_ok());
    assert_eq!(squad.leader_id, Some("platform_1".to_string()));
    assert!(squad.is_leader("platform_1"));
    assert!(!squad.is_leader("platform_2"));

    // Try to set non-member as leader
    assert!(squad.set_leader("platform_3".to_string()).is_err());

    // Clear leader
    squad.clear_leader();
    assert_eq!(squad.leader_id, None);
}

#[test]
fn test_squad_leader_removal() {
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

    squad.add_member("platform_1".to_string());
    squad.set_leader("platform_1".to_string()).unwrap();

    // Remove leader - should clear leader_id
    squad.remove_member("platform_1");
    assert_eq!(squad.leader_id, None);
}

#[test]
fn test_squad_capabilities() {
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

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
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

    assert_eq!(squad.platoon_id, None);

    squad.assign_platoon("platoon_1".to_string());
    assert_eq!(squad.platoon_id, Some("platoon_1".to_string()));

    squad.leave_platoon();
    assert_eq!(squad.platoon_id, None);
}

#[test]
fn test_squad_merge() {
    let config = SquadConfig::new(5);
    let mut squad1 = SquadState::new(config.clone());
    let mut squad2 = SquadState::new(config);

    // Squad1 has some members
    squad1.add_member("platform_1".to_string());
    squad1.add_member("platform_2".to_string());

    // Squad2 has different members
    squad2.add_member("platform_2".to_string());
    squad2.add_member("platform_3".to_string());

    // Merge squad2 into squad1
    squad1.merge(&squad2);

    // Should have union of members
    assert_eq!(squad1.member_count(), 3);
    assert!(squad1.is_member("platform_1"));
    assert!(squad1.is_member("platform_2"));
    assert!(squad1.is_member("platform_3"));
}

#[test]
fn test_squad_merge_leader() {
    let config = SquadConfig::new(5);
    let mut squad1 = SquadState::new(config.clone());
    let mut squad2 = SquadState::new(config);

    squad1.add_member("platform_1".to_string());
    squad2.add_member("platform_2".to_string());

    squad1.set_leader("platform_1".to_string()).unwrap();

    // Squad2 has a later leader update
    std::thread::sleep(std::time::Duration::from_secs(1));
    squad2.set_leader("platform_2".to_string()).unwrap();

    // Merge - squad2's leader should win (newer timestamp)
    squad1.merge(&squad2);
    assert_eq!(squad1.leader_id, Some("platform_2".to_string()));
}

#[test]
fn test_squad_merge_capabilities() {
    let config = SquadConfig::new(5);
    let mut squad1 = SquadState::new(config.clone());
    let mut squad2 = SquadState::new(config);

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
    let config = SquadConfig::new(5);
    let mut squad = SquadState::new(config);

    // Not valid with 0 members (min_size is 2)
    assert!(!squad.is_valid());

    squad.add_member("platform_1".to_string());
    assert!(!squad.is_valid());

    squad.add_member("platform_2".to_string());
    assert!(squad.is_valid());
}
