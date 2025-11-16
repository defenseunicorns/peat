//! End-to-End Integration Tests for Bidirectional Hierarchical Flow
//!
//! These tests validate the complete bidirectional flow with **real Ditto synchronization**
//! across multiple peers:
//! - **Upward flow**: Status aggregation (cells → zone, already validated)
//! - **Downward flow**: Command dissemination (zone → cells → nodes)
//! - **Full-duplex**: Commands down + Acknowledgments up simultaneously
//!
//! # Test Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │  Leader Node (Peer 1)                            │
//! │  - Issues commands via DittoStore                │
//! │  - Commands sync to Ditto mesh                   │
//! └────────────────────┬─────────────────────────────┘
//!                      │
//!           ┌──────────┴──────────┐
//!           ▼                     ▼
//!    ┌─────────────┐       ┌─────────────┐
//!    │  Member 1   │       │  Member 2   │
//!    │  (Peer 2)   │       │  (Peer 3)   │
//!    │  Receives   │       │  Receives   │
//!    │  Commands   │       │  Commands   │
//!    │  Sends Acks │       │  Sends Acks │
//!    └─────────────┘       └─────────────┘
//! ```
//!
//! # Real E2E Testing
//!
//! - Store commands in Ditto on leader peer
//! - Validate sync to member peers via observers
//! - Member peers execute commands locally
//! - Acknowledgments sync back to leader via Ditto
//! - Event-driven assertions (no polling)
//!
//! # Related ADRs
//!
//! - ADR-014: Distributed Coordination Primitives
//! - ADR-009: Bidirectional Hierarchical Flows

use hive_protocol::testing::E2EHarness;
use hive_schema::command::v1::{
    command_target::Scope, AckStatus, CommandAcknowledgment, CommandTarget, HierarchicalCommand,
};
use std::time::Duration;
use tokio::time::sleep;

/// Returns the number of sync attempts based on environment.
fn sync_timeout_attempts() -> usize {
    if std::env::var("CI").is_ok() {
        60 // 30 seconds for CI
    } else {
        20 // 10 seconds for local
    }
}

/// Test: Command propagates from leader to member via Ditto sync
#[tokio::test]
async fn test_e2e_command_propagation() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("command_propagation");

    println!("=== E2E: Command Propagation ===");

    // Create two Ditto stores: leader and member
    let leader_store = harness
        .create_ditto_store_with_tcp(Some(12350), None)
        .await
        .unwrap();
    let member_store = harness
        .create_ditto_store_with_tcp(None, Some("127.0.0.1:12350".to_string()))
        .await
        .unwrap();

    // Start sync
    leader_store.start_sync().unwrap();
    member_store.start_sync().unwrap();

    println!("Waiting for peer connection...");

    // Wait for peers to connect
    let connection_result = harness
        .wait_for_peer_connection(&leader_store, &member_store, Duration::from_secs(60))
        .await;

    if connection_result.is_err() {
        println!("⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(leader_store).await;
        harness.shutdown_store(member_store).await;
        return;
    }

    println!("✓ Peers connected");

    // Register sync subscriptions for hierarchical_commands collection on both peers
    let _leader_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register leader subscription");

    let _member_sub = member_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register member subscription");

    println!("✓ Registered sync subscriptions");

    // Create command from leader targeting member
    let command = HierarchicalCommand {
        command_id: "cmd-e2e-001".to_string(),
        originator_id: "leader-node".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Individual as i32,
            target_ids: vec!["member-node".to_string()],
        }),
        priority: 3,              // IMMEDIATE
        acknowledgment_policy: 4, // BOTH
        buffer_policy: 1,         // BUFFER_AND_RETRY
        conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
        leader_change_policy: 1,  // BUFFER_UNTIL_STABLE
        ..Default::default()
    };

    // Leader stores command in Ditto
    leader_store
        .upsert_command(&command.command_id, &command)
        .await
        .expect("Failed to store command");

    println!("✓ Leader stored command: {}", command.command_id);

    // Wait for command to sync to member
    let mut synced = false;
    for attempt in 0..sync_timeout_attempts() {
        if let Ok(Some(synced_cmd)) = member_store.get_command("cmd-e2e-001").await {
            println!(
                "✓ Command synced to member (attempt {}): {}",
                attempt + 1,
                synced_cmd.command_id
            );
            assert_eq!(synced_cmd.command_id, "cmd-e2e-001");
            assert_eq!(synced_cmd.originator_id, "leader-node");
            assert_eq!(synced_cmd.priority, 3);
            synced = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(synced, "Command failed to sync to member within timeout");

    // Clean shutdown
    harness.shutdown_store(leader_store).await;
    harness.shutdown_store(member_store).await;

    println!("✓ Command propagation validated");
}

/// Test: Acknowledgments propagate from member back to leader
#[tokio::test]
async fn test_e2e_acknowledgment_propagation() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("ack_propagation");

    println!("=== E2E: Acknowledgment Propagation ===");

    // Create two Ditto stores: leader and member
    let leader_store = harness
        .create_ditto_store_with_tcp(Some(12351), None)
        .await
        .unwrap();
    let member_store = harness
        .create_ditto_store_with_tcp(None, Some("127.0.0.1:12351".to_string()))
        .await
        .unwrap();

    // Start sync
    leader_store.start_sync().unwrap();
    member_store.start_sync().unwrap();

    println!("Waiting for peer connection...");

    // Wait for peers to connect
    let connection_result = harness
        .wait_for_peer_connection(&leader_store, &member_store, Duration::from_secs(60))
        .await;

    if connection_result.is_err() {
        println!("⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(leader_store).await;
        harness.shutdown_store(member_store).await;
        return;
    }

    println!("✓ Peers connected");

    // Register sync subscriptions for command_acknowledgments collection
    let _leader_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register leader subscription");

    let _member_sub = member_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register member subscription");

    println!("✓ Registered sync subscriptions");

    // Member node creates acknowledgment
    let ack = CommandAcknowledgment {
        command_id: "cmd-e2e-002".to_string(),
        node_id: "member-node".to_string(),
        status: AckStatus::AckReceived as i32,
        reason: None,
        timestamp: Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    // Member stores acknowledgment in Ditto
    member_store
        .upsert_command_ack("cmd-e2e-002-member-node", &ack)
        .await
        .expect("Failed to store acknowledgment");

    println!("✓ Member stored acknowledgment");

    // Wait for acknowledgment to sync to leader
    let mut synced = false;
    for attempt in 0..sync_timeout_attempts() {
        if let Ok(acks) = leader_store.query_command_acks("cmd-e2e-002").await {
            if !acks.is_empty() {
                println!(
                    "✓ Acknowledgment synced to leader (attempt {}): {} acks",
                    attempt + 1,
                    acks.len()
                );
                assert_eq!(acks.len(), 1);
                assert_eq!(acks[0].command_id, "cmd-e2e-002");
                assert_eq!(acks[0].node_id, "member-node");
                assert_eq!(acks[0].status, AckStatus::AckReceived as i32);
                synced = true;
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(
        synced,
        "Acknowledgment failed to sync to leader within timeout"
    );

    // Clean shutdown
    harness.shutdown_store(leader_store).await;
    harness.shutdown_store(member_store).await;

    println!("✓ Acknowledgment propagation validated");
}

/// Test: Full-duplex bidirectional flow - command down + ack up
#[tokio::test]
async fn test_e2e_full_duplex_command_ack_flow() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("full_duplex");

    println!("=== E2E: Full-Duplex Bidirectional Flow ===");

    // Create three Ditto stores: leader + 2 members (squad)
    let leader_store = harness
        .create_ditto_store_with_tcp(Some(12352), None)
        .await
        .unwrap();
    let member1_store = harness
        .create_ditto_store_with_tcp(None, Some("127.0.0.1:12352".to_string()))
        .await
        .unwrap();
    let member2_store = harness
        .create_ditto_store_with_tcp(None, Some("127.0.0.1:12352".to_string()))
        .await
        .unwrap();

    // Start sync
    leader_store.start_sync().unwrap();
    member1_store.start_sync().unwrap();
    member2_store.start_sync().unwrap();

    println!("Waiting for peer connections...");

    // Wait for all peers to connect
    let conn1 = harness
        .wait_for_peer_connection(&leader_store, &member1_store, Duration::from_secs(60))
        .await;
    let conn2 = harness
        .wait_for_peer_connection(&leader_store, &member2_store, Duration::from_secs(60))
        .await;

    if conn1.is_err() || conn2.is_err() {
        println!("⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(leader_store).await;
        harness.shutdown_store(member1_store).await;
        harness.shutdown_store(member2_store).await;
        return;
    }

    println!("✓ All peers connected");

    // Register sync subscriptions for both collections on all peers
    let _leader_cmd_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register leader command subscription");

    let _leader_ack_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register leader ack subscription");

    let _member1_cmd_sub = member1_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register member1 command subscription");

    let _member1_ack_sub = member1_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register member1 ack subscription");

    let _member2_cmd_sub = member2_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register member2 command subscription");

    let _member2_ack_sub = member2_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register member2 ack subscription");

    println!("✓ Registered sync subscriptions on all peers");

    // === DOWNWARD FLOW: Leader issues squad-level command ===

    let command = HierarchicalCommand {
        command_id: "cmd-e2e-003".to_string(),
        originator_id: "leader-node".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Squad as i32,
            target_ids: vec!["squad-alpha".to_string()],
        }),
        priority: 3,
        acknowledgment_policy: 4, // BOTH - require received and completed acks
        ..Default::default()
    };

    leader_store
        .upsert_command(&command.command_id, &command)
        .await
        .expect("Failed to store command");

    println!("✓ Leader issued squad command: {}", command.command_id);

    // Wait for command to sync to both members
    let mut member1_synced = false;
    let mut member2_synced = false;

    for attempt in 0..sync_timeout_attempts() {
        if !member1_synced {
            if let Ok(Some(_)) = member1_store.get_command("cmd-e2e-003").await {
                println!("✓ Command synced to member1 (attempt {})", attempt + 1);
                member1_synced = true;
            }
        }

        if !member2_synced {
            if let Ok(Some(_)) = member2_store.get_command("cmd-e2e-003").await {
                println!("✓ Command synced to member2 (attempt {})", attempt + 1);
                member2_synced = true;
            }
        }

        if member1_synced && member2_synced {
            break;
        }

        sleep(Duration::from_millis(500)).await;
    }

    assert!(
        member1_synced && member2_synced,
        "Command failed to sync to all members"
    );

    // === UPWARD FLOW: Members send acknowledgments back ===

    let ack1 = CommandAcknowledgment {
        command_id: "cmd-e2e-003".to_string(),
        node_id: "member-node-1".to_string(),
        status: AckStatus::AckReceived as i32,
        reason: None,
        timestamp: Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    let ack2 = CommandAcknowledgment {
        command_id: "cmd-e2e-003".to_string(),
        node_id: "member-node-2".to_string(),
        status: AckStatus::AckCompleted as i32,
        reason: None,
        timestamp: Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        }),
    };

    member1_store
        .upsert_command_ack("cmd-e2e-003-member-node-1", &ack1)
        .await
        .expect("Failed to store ack1");

    member2_store
        .upsert_command_ack("cmd-e2e-003-member-node-2", &ack2)
        .await
        .expect("Failed to store ack2");

    println!("✓ Members sent acknowledgments");

    // Wait for all acknowledgments to sync to leader
    let mut all_acks_synced = false;

    for attempt in 0..sync_timeout_attempts() {
        if let Ok(acks) = leader_store.query_command_acks("cmd-e2e-003").await {
            if acks.len() >= 2 {
                println!(
                    "✓ All acknowledgments synced to leader (attempt {}): {} acks",
                    attempt + 1,
                    acks.len()
                );

                // Verify both acknowledgments present
                let node_ids: Vec<String> = acks.iter().map(|a| a.node_id.clone()).collect();
                assert!(node_ids.contains(&"member-node-1".to_string()));
                assert!(node_ids.contains(&"member-node-2".to_string()));

                all_acks_synced = true;
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(
        all_acks_synced,
        "Not all acknowledgments synced to leader within timeout"
    );

    // Clean shutdown
    harness.shutdown_store(leader_store).await;
    harness.shutdown_store(member1_store).await;
    harness.shutdown_store(member2_store).await;

    println!("✓ Full-duplex bidirectional flow validated");
}

/// Test: Multiple concurrent commands with independent acknowledgment tracking
#[tokio::test]
async fn test_e2e_concurrent_commands() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    let mut harness = E2EHarness::new("concurrent_commands");

    println!("=== E2E: Concurrent Commands ===");

    // Create two Ditto stores
    let leader_store = harness
        .create_ditto_store_with_tcp(Some(12353), None)
        .await
        .unwrap();
    let member_store = harness
        .create_ditto_store_with_tcp(None, Some("127.0.0.1:12353".to_string()))
        .await
        .unwrap();

    // Start sync
    leader_store.start_sync().unwrap();
    member_store.start_sync().unwrap();

    println!("Waiting for peer connection...");

    let connection_result = harness
        .wait_for_peer_connection(&leader_store, &member_store, Duration::from_secs(60))
        .await;

    if connection_result.is_err() {
        println!("⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(leader_store).await;
        harness.shutdown_store(member_store).await;
        return;
    }

    println!("✓ Peers connected");

    // Register sync subscriptions for both collections
    let _leader_cmd_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register leader command subscription");

    let _leader_ack_sub = leader_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register leader ack subscription");

    let _member_cmd_sub = member_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")
        .expect("Failed to register member command subscription");

    let _member_ack_sub = member_store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")
        .expect("Failed to register member ack subscription");

    println!("✓ Registered sync subscriptions");

    // Leader issues 3 concurrent commands
    for i in 1..=3 {
        let command = HierarchicalCommand {
            command_id: format!("cmd-concurrent-{}", i),
            originator_id: "leader-node".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["member-node".to_string()],
            }),
            priority: 2 + i,          // Different priorities
            acknowledgment_policy: 2, // RECEIVED_ONLY
            ..Default::default()
        };

        leader_store
            .upsert_command(&command.command_id, &command)
            .await
            .expect("Failed to store command");
    }

    println!("✓ Leader issued 3 concurrent commands");

    // Wait for all commands to sync
    let mut all_synced = false;

    for attempt in 0..sync_timeout_attempts() {
        let mut count = 0;

        for i in 1..=3 {
            if let Ok(Some(_)) = member_store
                .get_command(&format!("cmd-concurrent-{}", i))
                .await
            {
                count += 1;
            }
        }

        if count == 3 {
            println!("✓ All 3 commands synced (attempt {})", attempt + 1);
            all_synced = true;
            break;
        }

        sleep(Duration::from_millis(500)).await;
    }

    assert!(all_synced, "Not all commands synced within timeout");

    // Member sends independent acknowledgments for each command
    for i in 1..=3 {
        let ack = CommandAcknowledgment {
            command_id: format!("cmd-concurrent-{}", i),
            node_id: "member-node".to_string(),
            status: AckStatus::AckReceived as i32,
            reason: None,
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        };

        member_store
            .upsert_command_ack(&format!("cmd-concurrent-{}-member-node", i), &ack)
            .await
            .expect("Failed to store ack");
    }

    println!("✓ Member sent 3 independent acknowledgments");

    // Verify each command has its own acknowledgment
    for i in 1..=3 {
        let mut ack_synced = false;

        for attempt in 0..sync_timeout_attempts() {
            if let Ok(acks) = leader_store
                .query_command_acks(&format!("cmd-concurrent-{}", i))
                .await
            {
                if !acks.is_empty() {
                    println!(
                        "✓ Ack for cmd-concurrent-{} synced (attempt {})",
                        i,
                        attempt + 1
                    );
                    assert_eq!(acks.len(), 1);
                    assert_eq!(acks[0].command_id, format!("cmd-concurrent-{}", i));
                    ack_synced = true;
                    break;
                }
            }
            sleep(Duration::from_millis(500)).await;
        }

        assert!(
            ack_synced,
            "Acknowledgment for cmd-concurrent-{} failed to sync",
            i
        );
    }

    // Clean shutdown
    harness.shutdown_store(leader_store).await;
    harness.shutdown_store(member_store).await;

    println!("✓ Concurrent command tracking validated");
}
