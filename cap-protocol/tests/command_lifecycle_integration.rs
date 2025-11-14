//! Single-node integration tests for command lifecycle
//!
//! These tests validate the complete command flow through CommandCoordinator
//! and DittoStore integration, including:
//! - Command issuance and storage
//! - Command reception and routing
//! - Command execution
//! - Acknowledgment generation and tracking
//! - Policy-based behavior (acknowledgment, buffer, conflict)

use cap_protocol::{
    command::CommandCoordinator,
    storage::{ditto_store::DittoConfig, DittoStore},
};
use cap_schema::command::v1::{
    command_target::Scope, AckStatus, CommandAcknowledgment, CommandTarget, HierarchicalCommand,
};
use std::time::Duration;
use tokio::time::sleep;

/// Helper to create a test DittoStore
async fn create_test_store(test_name: &str) -> DittoStore {
    dotenvy::dotenv().ok();

    let app_id = std::env::var("DITTO_APP_ID")
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .expect("DITTO_APP_ID required for test");

    let shared_key = std::env::var("DITTO_SHARED_KEY")
        .ok()
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .expect("DITTO_SHARED_KEY required for test");

    let persistence_dir =
        std::path::PathBuf::from(format!("/tmp/cap-persistence-test-{}", test_name));
    // Clean up any existing directory from previous failed runs
    let _ = std::fs::remove_dir_all(&persistence_dir);

    let config = DittoConfig {
        app_id,
        persistence_dir,
        shared_key,
        tcp_listen_port: None,
        tcp_connect_address: None,
    };

    let store = DittoStore::new(config).expect("Failed to create Ditto store");
    store.start_sync().expect("Failed to start sync");
    store
}

#[tokio::test]
async fn test_command_issue_and_persist() {
    // Setup: Create coordinator and store
    let store = create_test_store("test_command_issue").await;
    let coordinator = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-leader".to_string(),
        vec!["node-1".to_string(), "node-2".to_string()],
    );

    // Create a command targeting squad members
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-001".to_string(),
        originator_id: "node-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Squad as i32,
            target_ids: vec!["squad-alpha".to_string()],
        }),
        priority: 3,              // IMMEDIATE
        acknowledgment_policy: 4, // BOTH
        buffer_policy: 1,         // BUFFER_AND_RETRY
        conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
        leader_change_policy: 1,  // BUFFER_UNTIL_STABLE
        ..Default::default()
    };

    // Issue command through coordinator
    coordinator
        .issue_command(command.clone())
        .await
        .expect("Failed to issue command");

    // Persist command to Ditto
    store
        .upsert_command(&command.command_id, &command)
        .await
        .expect("Failed to persist command");

    // Verify command was stored
    let retrieved = store
        .get_command("cmd-lifecycle-001")
        .await
        .expect("Failed to retrieve command")
        .expect("Command should exist");

    assert_eq!(retrieved.command_id, "cmd-lifecycle-001");
    assert_eq!(retrieved.originator_id, "node-leader");
    assert_eq!(retrieved.priority, 3);

    // Verify command status in coordinator
    let status = coordinator
        .get_command_status("cmd-lifecycle-001")
        .await
        .expect("Status should exist");

    assert_eq!(status.command_id, "cmd-lifecycle-001");
    assert_eq!(status.state, 1); // PENDING

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_command_reception_and_execution() {
    // Setup: Create coordinator for a squad member
    let store = create_test_store("test_command_reception").await;
    let coordinator = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-1".to_string(),
        vec![], // Not a leader, just a member
    );

    // Create a command targeting this specific node
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-002".to_string(),
        originator_id: "node-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Individual as i32,
            target_ids: vec!["node-1".to_string()],
        }),
        priority: 3,
        acknowledgment_policy: 4, // BOTH - require received and completed acks
        ..Default::default()
    };

    // Persist command to Ditto (simulating reception from leader)
    store
        .upsert_command(&command.command_id, &command)
        .await
        .expect("Failed to persist command");

    // Receive and process command
    coordinator
        .receive_command(command.clone())
        .await
        .expect("Failed to receive command");

    // Wait for execution to complete
    sleep(Duration::from_millis(200)).await;

    // Verify command was executed
    let status = coordinator
        .get_command_status("cmd-lifecycle-002")
        .await
        .expect("Status should exist");

    assert_eq!(status.command_id, "cmd-lifecycle-002");
    assert_eq!(status.state, 3); // COMPLETED

    // Verify acknowledgment was generated
    let acks = coordinator
        .get_command_acknowledgments("cmd-lifecycle-002")
        .await;

    assert!(!acks.is_empty());
    assert_eq!(acks[0].node_id, "node-1");
    assert_eq!(acks[0].status, AckStatus::AckReceived as i32);

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_acknowledgment_persistence() {
    // Setup: Create store and coordinator
    let store = create_test_store("test_ack_persistence").await;
    let coordinator = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-1".to_string(),
        vec![],
    );

    // Create and receive command
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-003".to_string(),
        originator_id: "node-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Individual as i32,
            target_ids: vec!["node-1".to_string()],
        }),
        priority: 3,
        acknowledgment_policy: 2, // RECEIVED_ONLY
        ..Default::default()
    };

    coordinator
        .receive_command(command.clone())
        .await
        .expect("Failed to receive command");

    // Get acknowledgments from coordinator
    let acks = coordinator
        .get_command_acknowledgments("cmd-lifecycle-003")
        .await;

    assert!(!acks.is_empty());
    let ack = &acks[0];

    // Persist acknowledgment to Ditto
    let ack_id = format!("{}-{}", ack.command_id, ack.node_id);
    store
        .upsert_command_ack(&ack_id, ack)
        .await
        .expect("Failed to persist acknowledgment");

    // Wait a bit for Ditto to process the write
    sleep(Duration::from_millis(100)).await;

    // Retrieve acknowledgments from Ditto
    let retrieved_acks = store
        .query_command_acks("cmd-lifecycle-003")
        .await
        .expect("Failed to query acknowledgments");

    assert_eq!(retrieved_acks.len(), 1);
    assert_eq!(retrieved_acks[0].command_id, "cmd-lifecycle-003");
    assert_eq!(retrieved_acks[0].node_id, "node-1");
    assert_eq!(retrieved_acks[0].status, AckStatus::AckReceived as i32);

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_squad_command_routing() {
    // Setup: Create leader coordinator
    let store = create_test_store("test_squad_routing").await;
    let leader = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-leader".to_string(),
        vec!["node-1".to_string(), "node-2".to_string()],
    );

    // Create squad-level command
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-004".to_string(),
        originator_id: "zone-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Squad as i32,
            target_ids: vec!["squad-alpha".to_string()],
        }),
        priority: 3,
        acknowledgment_policy: 4, // BOTH
        ..Default::default()
    };

    // Leader receives command
    leader
        .receive_command(command.clone())
        .await
        .expect("Failed to receive command");

    // Verify leader routes it to subordinates (routing logic tested in unit tests)
    // For integration test, we verify the command can be persisted
    store
        .upsert_command(&command.command_id, &command)
        .await
        .expect("Failed to persist command");

    let retrieved = store
        .get_command("cmd-lifecycle-004")
        .await
        .expect("Failed to retrieve command")
        .expect("Command should exist");

    assert_eq!(retrieved.command_id, "cmd-lifecycle-004");

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_acknowledgment_policy_none() {
    // Setup: Create coordinator
    let store = create_test_store("test_ack_policy_none").await;
    let coordinator = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-1".to_string(),
        vec![],
    );

    // Create command with NONE acknowledgment policy
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-005".to_string(),
        originator_id: "node-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Individual as i32,
            target_ids: vec!["node-1".to_string()],
        }),
        priority: 3,
        acknowledgment_policy: 1, // NONE
        ..Default::default()
    };

    coordinator
        .receive_command(command.clone())
        .await
        .expect("Failed to receive command");

    // Wait for execution
    sleep(Duration::from_millis(200)).await;

    // Verify NO acknowledgments were generated
    let acks = coordinator
        .get_command_acknowledgments("cmd-lifecycle-005")
        .await;

    assert!(
        acks.is_empty(),
        "No acknowledgments should be generated with NONE policy"
    );

    // But command should still be executed
    let status = coordinator
        .get_command_status("cmd-lifecycle-005")
        .await
        .expect("Status should exist");

    assert_eq!(status.state, 3); // COMPLETED

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_command_not_applicable() {
    // Setup: Create coordinator for node-1
    let store = create_test_store("test_not_applicable").await;
    let coordinator = CommandCoordinator::new(
        Some("squad-alpha".to_string()),
        "node-1".to_string(),
        vec![],
    );

    // Create command targeting a different node
    let command = HierarchicalCommand {
        command_id: "cmd-lifecycle-006".to_string(),
        originator_id: "node-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Individual as i32,
            target_ids: vec!["node-2".to_string()], // Different node
        }),
        priority: 3,
        acknowledgment_policy: 2,
        ..Default::default()
    };

    // Receive command (should be ignored)
    coordinator
        .receive_command(command.clone())
        .await
        .expect("Failed to receive command");

    // Wait a bit
    sleep(Duration::from_millis(100)).await;

    // Verify command was NOT executed (no status created)
    let status = coordinator.get_command_status("cmd-lifecycle-006").await;

    assert!(status.is_none(), "Command should not be executed");

    // Verify NO acknowledgments were generated
    let acks = coordinator
        .get_command_acknowledgments("cmd-lifecycle-006")
        .await;

    assert!(
        acks.is_empty(),
        "No acknowledgments for non-applicable command"
    );

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_multiple_acknowledgments_collection() {
    // Setup: Simulate multiple nodes acknowledging a command
    let store = create_test_store("test_multiple_acks").await;

    // Create acknowledgments from different nodes
    let ack1 = CommandAcknowledgment {
        command_id: "cmd-lifecycle-007".to_string(),
        node_id: "node-1".to_string(),
        status: AckStatus::AckReceived as i32,
        reason: None,
        timestamp: Some(cap_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    let ack2 = CommandAcknowledgment {
        command_id: "cmd-lifecycle-007".to_string(),
        node_id: "node-2".to_string(),
        status: AckStatus::AckCompleted as i32,
        reason: None,
        timestamp: Some(cap_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    let ack3 = CommandAcknowledgment {
        command_id: "cmd-lifecycle-007".to_string(),
        node_id: "node-3".to_string(),
        status: AckStatus::AckCompleted as i32,
        reason: None,
        timestamp: Some(cap_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    // Persist all acknowledgments
    store
        .upsert_command_ack("cmd-lifecycle-007-node-1", &ack1)
        .await
        .expect("Failed to persist ack1");
    store
        .upsert_command_ack("cmd-lifecycle-007-node-2", &ack2)
        .await
        .expect("Failed to persist ack2");
    store
        .upsert_command_ack("cmd-lifecycle-007-node-3", &ack3)
        .await
        .expect("Failed to persist ack3");

    // Query all acknowledgments for the command
    let acks = store
        .query_command_acks("cmd-lifecycle-007")
        .await
        .expect("Failed to query acks");

    assert_eq!(acks.len(), 3);

    // Verify all nodes are represented
    let node_ids: Vec<String> = acks.iter().map(|a| a.node_id.clone()).collect();
    assert!(node_ids.contains(&"node-1".to_string()));
    assert!(node_ids.contains(&"node-2".to_string()));
    assert!(node_ids.contains(&"node-3".to_string()));

    // Verify statuses
    let received_count = acks
        .iter()
        .filter(|a| a.status == AckStatus::AckReceived as i32)
        .count();
    let completed_count = acks
        .iter()
        .filter(|a| a.status == AckStatus::AckCompleted as i32)
        .count();

    assert_eq!(received_count, 1);
    assert_eq!(completed_count, 2);

    store.stop_sync();
    drop(store);
    sleep(Duration::from_millis(100)).await;
}
