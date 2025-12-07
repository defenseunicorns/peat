//! M1 Vignette End-to-End Tests
//!
//! These tests validate the full M1 object tracking scenario across
//! distributed human-machine-AI teams using REAL multi-node sync
//! via AutomergeIroh backends.
//!
//! Note: M1TestHarness tests are disabled until E2EHarness is available
//! for automerge-backend in hive-protocol.

// M1TestHarness requires hive_protocol::testing::E2EHarness which is only
// available with the ditto-backend feature. These tests are disabled until
// an automerge-backend equivalent is available.

/*
use hive_inference::testing::M1TestHarness;
use std::time::Duration;

/// Test Phase 1: Team Initialization with REAL sync
#[tokio::test]
async fn test_phase1_initialization_real_sync() {
    let mut harness = M1TestHarness::new("e2e_phase1_init_real");
    harness.initialize().await.expect("Init should succeed");
    let duration = harness.phase1_initialization().await.expect("Phase 1 should succeed");
    assert!(duration < Duration::from_secs(30));
    harness.shutdown().await.expect("Shutdown should succeed");
}

// ... other M1TestHarness tests ...
*/

/// Test Team Fixture Creation (no sync needed)
#[tokio::test]
async fn test_team_fixtures() {
    use hive_inference::testing::TeamFixture;

    let alpha = TeamFixture::alpha();
    let bravo = TeamFixture::bravo();

    // Alpha should be UAV-based
    assert_eq!(alpha.name, "Alpha");
    assert_eq!(alpha.team.member_count(), 3);
    assert_eq!(alpha.network_id, "network-a");

    // Bravo should be UGV-based
    assert_eq!(bravo.name, "Bravo");
    assert_eq!(bravo.team.member_count(), 3);
    assert_eq!(bravo.network_id, "network-b");

    // Teams should have different platform IDs
    let alpha_ids = alpha.platform_ids();
    let bravo_ids = bravo.platform_ids();
    for id in &alpha_ids {
        assert!(
            !bravo_ids.contains(id),
            "Platform IDs should be unique across teams"
        );
    }
}

/// Test Simulated C2 (no sync needed)
#[tokio::test]
async fn test_simulated_c2() {
    use hive_inference::messages::{Position, Priority};
    use hive_inference::testing::SimulatedC2;

    let mut c2 = SimulatedC2::new("Test-C2");

    // Issue command
    let cmd = c2.issue_track_command("Test target", Priority::High, None);
    assert_eq!(c2.command_count(), 1);
    assert!(c2.get_command_timestamp(&cmd.command_id).is_some());

    // Receive tracks
    let track = hive_inference::messages::TrackUpdate::new(
        "TRACK-001",
        "person",
        0.85,
        Position::new(33.77, -84.39),
        "Alpha-2",
        "Alpha-3",
        "1.0.0",
    );
    c2.receive_track(track);

    assert_eq!(c2.track_count(), 1);
    assert!(c2.get_latest_track("TRACK-001").is_some());
}

/// Test Coordinator Fixture (no sync needed)
#[tokio::test]
async fn test_coordinator_fixture() {
    use hive_inference::testing::CoordinatorFixture;

    let mut coord = CoordinatorFixture::bridge("Test-Bridge", "net-a", "net-b");

    assert!(coord.is_bridge);
    assert!(coord.connects("net-a", "net-b"));
    assert!(!coord.connects("net-a", "net-c"));

    coord.register_team("Alpha");
    coord.register_team("Bravo");
    coord.register_team("Alpha"); // Duplicate should not add

    assert_eq!(coord.teams.len(), 2);
}
