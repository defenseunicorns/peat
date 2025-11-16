//! Example: Bidirectional Hierarchical Command Flow
//!
//! This example demonstrates how to use the bidirectional hierarchical flow
//! to send commands down the hierarchy and collect acknowledgments back up.
//!
//! # Scenario
//!
//! A zone leader issues a command to a squad, which routes it to individual nodes.
//! The nodes execute the command and send acknowledgments back through the hierarchy.
//!
//! # Usage
//!
//! ```bash
//! # Set Ditto credentials
//! export DITTO_APP_ID="your-app-id"
//! export DITTO_SHARED_KEY="your-shared-key"
//!
//! # Run the example
//! cargo run --example bidirectional_command_flow
//! ```

use hive_protocol::command::CommandCoordinator;
use hive_protocol::storage::{ditto_store::DittoConfig, DittoStore};
use hive_schema::command::v1::{
    command_target::Scope, AckStatus, CommandAcknowledgment, CommandTarget, HierarchicalCommand,
};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Bidirectional Hierarchical Command Flow Example ===\n");

    // Load Ditto credentials from environment
    dotenvy::dotenv().ok();

    let app_id = std::env::var("DITTO_APP_ID")
        .map_err(|_| "DITTO_APP_ID not set. Please set it in .env or environment variables.")?;

    let shared_key = std::env::var("DITTO_SHARED_KEY")
        .map_err(|_| "DITTO_SHARED_KEY not set. Please set it in .env or environment variables.")?;

    // Create temporary directory for Ditto persistence
    let temp_dir = tempdir()?;

    // ========================================
    // Setup: Create Ditto store and coordinators
    // ========================================

    println!("1. Setting up Ditto store...");

    let config = DittoConfig {
        app_id,
        persistence_dir: temp_dir.path().join("bidirectional_example"),
        shared_key,
        tcp_listen_port: None,
        tcp_connect_address: None,
    };

    let store = DittoStore::new(config)?;
    store.start_sync()?;

    println!("   ✓ Ditto store initialized\n");

    // Register sync subscriptions for command and acknowledgment collections
    let _cmd_sub = store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM hierarchical_commands")?;

    let _ack_sub = store
        .ditto()
        .sync()
        .register_subscription_v2("SELECT * FROM command_acknowledgments")?;

    println!("2. Registered Ditto sync subscriptions\n");

    // Create coordinators for different roles in the hierarchy
    let zone_leader = CommandCoordinator::new(
        None,                      // Not in a squad (zone level)
        "zone-leader".to_string(), // node_id
        vec![],                    // No direct subordinates (routes via squads)
    );

    let squad_leader = CommandCoordinator::new(
        Some("squad-alpha".to_string()),                  // squad_id
        "squad-leader".to_string(),                       // node_id
        vec!["node-1".to_string(), "node-2".to_string()], // squad members
    );

    let node1 = CommandCoordinator::new(
        Some("squad-alpha".to_string()), // squad_id
        "node-1".to_string(),            // node_id
        vec![],                          // No subordinates (leaf node)
    );

    let node2 = CommandCoordinator::new(
        Some("squad-alpha".to_string()), // squad_id
        "node-2".to_string(),            // node_id
        vec![],                          // No subordinates (leaf node)
    );

    println!("3. Created coordinators:");
    println!("   - Zone Leader (issues commands)");
    println!("   - Squad Leader (routes commands)");
    println!("   - Node 1 (executes commands)");
    println!("   - Node 2 (executes commands)\n");

    // ========================================
    // Downward Flow: Zone Leader Issues Command
    // ========================================

    println!("4. Zone leader issuing command to squad...\n");

    let command = HierarchicalCommand {
        command_id: "mission-recon-001".to_string(),
        originator_id: "zone-leader".to_string(),
        target: Some(CommandTarget {
            scope: Scope::Squad as i32,
            target_ids: vec!["squad-alpha".to_string()],
        }),
        priority: 3,              // IMMEDIATE
        acknowledgment_policy: 4, // BOTH (RECEIVED + COMPLETED)
        buffer_policy: 1,         // BUFFER_AND_RETRY
        conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
        leader_change_policy: 1,  // BUFFER_UNTIL_STABLE
        ..Default::default()
    };

    // Zone leader issues command
    zone_leader.issue_command(command.clone()).await?;

    // Persist to Ditto for CRDT sync
    store.upsert_command(&command.command_id, &command).await?;

    println!("   ✓ Command issued: {}", command.command_id);
    println!("   ✓ Target: Squad Alpha");
    println!("   ✓ Priority: IMMEDIATE");
    println!("   ✓ Policy: Require RECEIVED + COMPLETED acks\n");

    // Simulate propagation delay
    sleep(Duration::from_millis(500)).await;

    // ========================================
    // Squad Leader Receives and Routes Command
    // ========================================

    println!("5. Squad leader receiving and routing command...\n");

    // Squad leader receives command from Ditto
    let received_cmd = store.get_command(&command.command_id).await?;

    if let Some(cmd) = received_cmd {
        // Squad leader processes command (routes to members)
        squad_leader.receive_command(cmd).await?;

        println!("   ✓ Squad leader received command");
        println!("   ✓ Routing to 2 squad members...\n");
    }

    // ========================================
    // Squad Members Execute Command
    // ========================================

    println!("6. Squad members executing command...\n");

    // Node 1 executes
    let node1_cmd = store.get_command(&command.command_id).await?;
    if let Some(cmd) = node1_cmd {
        node1.receive_command(cmd).await?;
        println!("   ✓ Node 1 executed command");

        // Simulate sending acknowledgment
        let ack1 = CommandAcknowledgment {
            command_id: command.command_id.clone(),
            node_id: "node-1".to_string(),
            status: AckStatus::AckCompleted as i32,
            reason: None,
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                nanos: 0,
            }),
        };

        store
            .upsert_command_ack("mission-recon-001-node-1", &ack1)
            .await?;
        println!("   ✓ Node 1 sent COMPLETED acknowledgment");
    }

    // Node 2 executes
    let node2_cmd = store.get_command(&command.command_id).await?;
    if let Some(cmd) = node2_cmd {
        node2.receive_command(cmd).await?;
        println!("   ✓ Node 2 executed command");

        // Simulate sending acknowledgment
        let ack2 = CommandAcknowledgment {
            command_id: command.command_id.clone(),
            node_id: "node-2".to_string(),
            status: AckStatus::AckCompleted as i32,
            reason: None,
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                nanos: 0,
            }),
        };

        store
            .upsert_command_ack("mission-recon-001-node-2", &ack2)
            .await?;
        println!("   ✓ Node 2 sent COMPLETED acknowledgment\n");
    }

    // ========================================
    // Upward Flow: Zone Leader Collects Acknowledgments
    // ========================================

    println!("7. Zone leader collecting acknowledgments...\n");

    // Option 1: Observer-Based (Recommended for Production)
    // Use Ditto observers for event-driven notification
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::unbounded_channel::<()>();

    let observer_query = format!(
        "SELECT * FROM command_acknowledgments WHERE command_id = '{}'",
        command.command_id
    );

    let _observer =
        store
            .ditto()
            .store()
            .register_observer_v2(&observer_query, move |_result| {
                let _ = tx.send(()); // Notify on any ack change
            })?;

    println!("   ✓ Observer registered for acknowledgment events");

    let mut ack_count = 0;
    let expected_acks = 2; // Expecting 2 nodes to acknowledge

    while ack_count < expected_acks {
        tokio::select! {
            _ = rx.recv() => {
                // Acknowledgment event received! Query the latest acks
                let acks = store.query_command_acks(&command.command_id).await?;
                ack_count = acks.len();

                println!("   ✓ Acknowledgment event: {}/{} acks received", ack_count, expected_acks);

                for ack in &acks {
                    let status_str = match ack.status {
                        1 => "RECEIVED",
                        2 => "COMPLETED",
                        3 => "FAILED",
                        _ => "UNKNOWN",
                    };
                    println!("     - Node {}: {}", ack.node_id, status_str);
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                println!("   ⚠ Timeout waiting for acknowledgments");
                break;
            }
        }
    }

    // Final query to ensure we have all acks
    let acks = store.query_command_acks(&command.command_id).await?;

    println!("\n   ✓ Final count: {} acknowledgments", acks.len());

    // ========================================
    // Summary
    // ========================================

    println!("\n=== Summary ===");
    println!("✓ Downward Flow: Command propagated from zone → squad → nodes");
    println!("✓ Execution: Both nodes executed the command");
    println!("✓ Upward Flow: Acknowledgments propagated from nodes → zone");
    println!("✓ Complete: {}/2 nodes acknowledged", acks.len());

    if acks.len() == 2 {
        println!("\n🎉 Mission complete! All nodes acknowledged.\n");
    }

    // Cleanup
    store.stop_sync();

    Ok(())
}
