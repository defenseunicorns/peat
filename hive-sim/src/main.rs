//! HIVE Protocol Network Simulation Node
//!
//! Reference implementation for simulating and testing the HIVE protocol.
//! This replaces the initial placeholder with a full-featured simulation node.
//!
//! # What This Tests
//!
//! - Backend initialization (Ditto CRDT sync backend)
//! - Peer discovery across simulated network
//! - Document creation and replication
//! - Hierarchical state aggregation (Mode 4)
//! - CRDT sync with network constraints
//!
//! # Architecture
//!
//! ContainerLab runs multiple containers with this binary:
//! - Writer nodes: Create test documents and periodic updates
//! - Reader nodes: Wait to receive documents
//! - Squad leaders: Aggregate member states into SquadSummary
//! - Platoon leaders: Aggregate squad summaries into PlatoonSummary
//! - All nodes use Ditto CRDT backend for P2P mesh synchronization
//!
//! # Success Criteria
//!
//! - Backend initializes successfully
//! - Peers discover each other via P2P mesh
//! - Documents sync between nodes
//! - Hierarchical aggregation creates summary documents with -summary suffix
//! - Tier-by-tier latency metrics are collected
//! - Works with network constraints (latency, bandwidth, loss)
//!
//! # Command Line Arguments
//!
//! --node-id <id>         Node identifier (e.g., "node1", "squad-1A-leader")
//! --mode <mode>          "writer" (creates documents) or "reader" (waits for documents)
//! --backend <type>       Sync backend to use (default: "ditto")
//! --tcp-listen <port>    Optional: Listen for TCP connections on this port
//! --tcp-connect <addr>   Optional: Connect to TCP peer at this address
//! --node-type <type>     Node type for authorization (e.g., "soldier", "squad_leader")
//! --update-rate-ms <ms>  Update rate in milliseconds (default: 5000)
//! --hive-filter          Enable HIVE capability-based filtering
//!
//! # Environment Variables
//!
//! **Ditto Backend (Required):**
//! - DITTO_APP_ID: Application ID from Ditto portal
//! - DITTO_OFFLINE_TOKEN: Offline license token
//! - DITTO_SHARED_KEY: Shared encryption key
//!
//! **Hierarchical Mode (Mode 4):**
//! - MODE: Set to "hierarchical" to enable hierarchical aggregation
//! - ROLE: Node role - "soldier", "squad_leader", "platoon_leader", "battalion_hq"
//! - SQUAD_ID: Squad identifier for members (e.g., "squad-1A")
//! - SQUAD_MEMBERS: Comma-separated member IDs for squad leaders
//! - PLATOON_ID: Platoon identifier for platoon leaders
//!
//! **HIVE Filtering:**
//! - HIVE_FILTER_ENABLED: Set to "true" or "1" to enable differential updates
//!
//! # Exit Codes
//!
//! 0: Success (document synced, all operations completed)
//! 1: Failure (timeout, error, or document not received)

use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::sync::{
    BackendConfig, ChangeEvent, ChangeStream, DataSyncBackend, Document, Query, TransportConfig,
    Value,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

// Mode 4: Hierarchical aggregation imports
use hive_protocol::hierarchy::{HierarchicalAggregator, StateAggregator};
use hive_protocol::models::{NodeConfig, NodeState};

// Phase 3: Command dissemination imports
use hive_protocol::command::CommandCoordinator;
use hive_protocol::storage::DittoCommandStorage;
use hive_schema::command::v1::{command_target::Scope, CommandTarget, HierarchicalCommand};

/// Test document structure
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestDoc {
    id: String,
    message: String,
    timestamp: u64, // Unix timestamp in microseconds
}

/// Metrics event for JSON logging
#[derive(Debug, serde::Serialize)]
#[serde(tag = "event_type")]
enum MetricsEvent {
    DocumentInserted {
        node_id: String,
        doc_id: String,
        timestamp_us: u128, // Unix timestamp in microseconds
    },
    DocumentReceived {
        node_id: String,
        doc_id: String,
        created_at_us: u128,    // When document was first created
        last_modified_us: u128, // When document was last updated
        received_at_us: u128,   // When we received it
        latency_us: u128,       // Propagation time
        latency_ms: f64,
        version: u64,             // Document version
        is_first_reception: bool, // true = creation sync, false = update/recovery sync
        latency_type: String,     // "creation", "update", or "recovery"
    },
    MessageSent {
        node_id: String,
        node_type: String,
        message_number: u64,
        message_size_bytes: usize,
        timestamp_us: u128,
    },
    DocumentAcknowledged {
        node_id: String,
        doc_id: String,
        timestamp_us: u128,
    },
    AllAcksReceived {
        node_id: String,
        doc_id: String,
        inserted_at_us: u128,
        all_acked_at_us: u128,
        round_trip_latency_us: u128,
        round_trip_latency_ms: f64,
        ack_count: usize,
    },
    SquadSummaryCreated {
        node_id: String,
        squad_id: String,
        member_count: usize,
        readiness_score: f64,
        timestamp_us: u128,
    },
    PlatoonSummaryCreated {
        node_id: String,
        platoon_id: String,
        squad_count: usize,
        total_member_count: usize,
        timestamp_us: u128,
    },
    // Phase 3: Command dissemination events
    CommandIssued {
        node_id: String,
        command_id: String,
        target_scope: String, // "Node", "Squad", "Platoon", "Battalion"
        target_ids: Vec<String>,
        priority: i32,
        timestamp_us: u128,
    },
    #[allow(dead_code)] // Will be used when command reception is implemented
    CommandReceived {
        node_id: String,
        command_id: String,
        originator_id: String,
        received_at_us: u128,
        latency_us: u128,
        latency_ms: f64,
    },
    #[allow(dead_code)] // Will be used when command acknowledgment is implemented
    CommandAcknowledged {
        node_id: String,
        command_id: String,
        status: String, // "RECEIVED", "COMPLETED", "FAILED"
        timestamp_us: u128,
    },
    #[allow(dead_code)] // Will be used when command acknowledgment is implemented
    AllCommandAcksReceived {
        node_id: String,
        command_id: String,
        issued_at_us: u128,
        all_acked_at_us: u128,
        round_trip_latency_us: u128,
        round_trip_latency_ms: f64,
        ack_count: usize,
    },
}

/// Phase 3: Simple command demonstration function
///
/// Issues a test command to demonstrate command dissemination capability.
/// This is a minimal demonstration - full command loop implementation would
/// include command reception, acknowledgment handling, and metrics tracking.
async fn demo_command_issuance(
    coordinator: Arc<CommandCoordinator>,
    node_id: String,
    target_scope: Scope,
    target_ids: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Generate a simple command ID
    let command_id = format!(
        "{}-cmd-{}",
        node_id,
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros()
    );

    let command = HierarchicalCommand {
        command_id: command_id.clone(),
        originator_id: node_id.clone(),
        target: Some(CommandTarget {
            scope: target_scope as i32,
            target_ids: target_ids.clone(),
        }),
        priority: 3,              // IMMEDIATE
        acknowledgment_policy: 2, // ACK_REQUIRED
        buffer_policy: 1,         // BUFFER_AND_RETRY
        conflict_policy: 2,       // HIGHEST_PRIORITY_WINS
        leader_change_policy: 1,  // REISSUE_FROM_NEW_LEADER
        ..Default::default()
    };

    // Issue the command
    coordinator.issue_command(command.clone()).await?;

    // Log metrics event
    let event = MetricsEvent::CommandIssued {
        node_id: node_id.clone(),
        command_id,
        target_scope: format!("{:?}", target_scope),
        target_ids,
        priority: 3,
        timestamp_us: SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros(),
    };
    println!("{}", serde_json::to_string(&event)?);

    Ok(())
}

/// Squad leader aggregation loop (Mode 4)
///
/// Periodically aggregates member NodeStates into SquadSummary and publishes via coordinator.
/// This is the core of hierarchical state aggregation - squad leaders collect member
/// states and create summary documents that get replicated via P2P mesh.
async fn squad_leader_aggregation_loop(
    coordinator: Arc<HierarchicalAggregator>,
    squad_id: String,
    node_id: String,
    member_ids: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Started squad leader aggregation for {}",
        node_id, squad_id
    );
    println!("[{}] Squad members: {:?}", node_id, member_ids);

    loop {
        // Collect member states using synthetic NodeConfig/NodeState
        // In production, this would query actual NodeState documents from Ditto
        let mut member_states = Vec::new();

        for member_id in &member_ids {
            // Create minimal NodeConfig for aggregation
            let config = NodeConfig {
                id: member_id.clone(),
                platform_type: "Simulated".to_string(),
                capabilities: vec![],
                comm_range_m: 1000.0,
                max_speed_mps: 10.0,
                operator_binding: None,
                created_at: None,
            };

            // Create minimal operational NodeState
            let state = NodeState {
                position: Some(hive_schema::common::v1::Position {
                    latitude: 0.0,
                    longitude: 0.0,
                    altitude: 0.0,
                }),
                fuel_minutes: 100,
                health: hive_schema::node::v1::HealthStatus::Nominal.into(),
                phase: hive_schema::node::v1::Phase::Hierarchy.into(),
                cell_id: Some(squad_id.clone()),
                zone_id: None,
                timestamp: None,
            };

            member_states.push((config, state));
        }

        if !member_states.is_empty() {
            // Aggregate into SquadSummary using StateAggregator
            match StateAggregator::aggregate_squad(&squad_id, &node_id, member_states) {
                Ok(squad_summary) => {
                    let timestamp_us = now_micros();

                    // Check if squad summary document exists (create-once pattern)
                    match coordinator.get_squad_summary(&squad_id).await {
                        Ok(None) => {
                            // First time - create document
                            if let Err(e) = coordinator
                                .create_squad_summary(&squad_id, &squad_summary)
                                .await
                            {
                                eprintln!("[{}] Failed to create squad summary: {}", node_id, e);
                            } else {
                                println!(
                                    "[{}] ✓ Created squad {} ({} members, readiness: {:.2})",
                                    node_id,
                                    squad_id,
                                    squad_summary.member_count,
                                    squad_summary.readiness_score
                                );
                            }
                        }
                        Ok(Some(_existing)) => {
                            // Document exists - send delta update
                            use hive_protocol::hierarchy::deltas::SquadDelta;
                            let delta =
                                SquadDelta::from_summary(&squad_summary, timestamp_us as u64);

                            if let Err(e) = coordinator.update_squad_summary(&squad_id, delta).await
                            {
                                eprintln!("[{}] Failed to update squad summary: {}", node_id, e);
                            } else {
                                println!(
                                    "[{}] ✓ Updated squad {} ({} members, readiness: {:.2})",
                                    node_id,
                                    squad_id,
                                    squad_summary.member_count,
                                    squad_summary.readiness_score
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("[{}] Failed to check squad summary: {}", node_id, e);
                        }
                    }

                    // Log squad summary metrics
                    log_metrics(&MetricsEvent::SquadSummaryCreated {
                        node_id: node_id.clone(),
                        squad_id: squad_id.clone(),
                        member_count: squad_summary.member_count as usize,
                        readiness_score: squad_summary.readiness_score as f64,
                        timestamp_us,
                    });
                }
                Err(e) => {
                    eprintln!("[{}] Failed to aggregate squad: {}", node_id, e);
                }
            }
        } else {
            println!(
                "[{}] [Squad Leader] No operational members to aggregate",
                node_id
            );
        }

        // Wait 5 seconds before next aggregation
        sleep(Duration::from_secs(5)).await;
    }
}

/// Platoon leader aggregation loop (Mode 4)
///
/// Event-driven aggregation triggered by squad summary updates via P2P mesh.
/// Platoon leaders observe squad summaries arriving from squad leaders and aggregate
/// them into PlatoonSummary documents.
async fn platoon_leader_aggregation_loop(
    mut change_stream: ChangeStream,
    coordinator: Arc<HierarchicalAggregator>,
    platoon_id: String,
    node_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Started platoon leader aggregation for {}",
        node_id, platoon_id
    );
    println!(
        "[{}] Observing squad summary change stream for P2P latency measurement",
        node_id
    );

    let squad_ids = vec!["squad-alpha", "squad-bravo", "squad-charlie"];

    // Clone coordinator for aggregation task
    let coordinator_clone = Arc::clone(&coordinator);
    let platoon_id_clone = platoon_id.clone();
    let node_id_clone = node_id.clone();
    let squad_ids_clone = squad_ids.clone();

    // Spawn periodic aggregation task
    let aggregation_handle = tokio::spawn(async move {
        loop {
            // Collect latest squad summaries
            let mut squad_summaries = Vec::new();

            for squad_id in &squad_ids_clone {
                if let Ok(Some(summary)) = coordinator_clone.get_squad_summary(squad_id).await {
                    squad_summaries.push(summary);
                }
            }

            if squad_summaries.len() == 3 {
                let timestamp_us = now_micros();

                // Aggregate into PlatoonSummary
                match StateAggregator::aggregate_platoon(
                    &platoon_id_clone,
                    &node_id_clone,
                    squad_summaries,
                ) {
                    Ok(platoon_summary) => {
                        // Check if platoon summary document exists (create-once pattern)
                        match coordinator_clone
                            .get_platoon_summary(&platoon_id_clone)
                            .await
                        {
                            Ok(None) => {
                                // First time - create document
                                if let Err(e) = coordinator_clone
                                    .create_platoon_summary(&platoon_id_clone, &platoon_summary)
                                    .await
                                {
                                    eprintln!(
                                        "[{}] Failed to create platoon summary: {}",
                                        node_id_clone, e
                                    );
                                } else {
                                    println!(
                                        "[{}] ✓ Created platoon {} ({} squads, {} total members)",
                                        node_id_clone,
                                        platoon_id_clone,
                                        platoon_summary.squad_count,
                                        platoon_summary.total_member_count
                                    );
                                }
                            }
                            Ok(Some(_existing)) => {
                                // Document exists - send delta update
                                use hive_protocol::hierarchy::deltas::PlatoonDelta;
                                let delta = PlatoonDelta::from_summary(
                                    &platoon_summary,
                                    timestamp_us as u64,
                                );

                                if let Err(e) = coordinator_clone
                                    .update_platoon_summary(&platoon_id_clone, delta)
                                    .await
                                {
                                    eprintln!(
                                        "[{}] Failed to update platoon summary: {}",
                                        node_id_clone, e
                                    );
                                } else {
                                    println!(
                                        "[{}] ✓ Updated platoon {} ({} squads, {} total members)",
                                        node_id_clone,
                                        platoon_id_clone,
                                        platoon_summary.squad_count,
                                        platoon_summary.total_member_count
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "[{}] Failed to check platoon summary: {}",
                                    node_id_clone, e
                                );
                            }
                        }

                        // Log platoon summary metrics
                        log_metrics(&MetricsEvent::PlatoonSummaryCreated {
                            node_id: node_id_clone.clone(),
                            platoon_id: platoon_id_clone.clone(),
                            squad_count: platoon_summary.squad_count as usize,
                            total_member_count: platoon_summary.total_member_count as usize,
                            timestamp_us,
                        });
                    }
                    Err(e) => {
                        eprintln!("[{}] Failed to aggregate platoon: {}", node_id_clone, e);
                    }
                }
            }

            // Aggregate every 5 seconds using latest data
            sleep(Duration::from_secs(5)).await;
        }
    });

    // Listen for squad summary changes via P2P mesh (for latency measurement)
    loop {
        // Wait for next change event with timeout
        let event =
            tokio::time::timeout(Duration::from_millis(500), change_stream.receiver.recv()).await;

        match event {
            Ok(Some(change_event)) => {
                match change_event {
                    ChangeEvent::Initial { documents } => {
                        // Process initial snapshot
                        for doc in documents {
                            let received_at_us = now_micros();
                            if let Some(doc_id) = &doc.id {
                                if doc_id.starts_with("squad-") {
                                    // Extract timestamps with proper delta sync semantics
                                    let created_at_us = if let Some(ts) = doc.get("created_at_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if let Some(ts) = doc.get("timestamp_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else {
                                        0
                                    };

                                    let last_modified_us =
                                        if let Some(ts) = doc.get("last_modified_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else {
                                            created_at_us
                                        };

                                    let version = if let Some(v) = doc.get("version") {
                                        v.as_u64().unwrap_or(1)
                                    } else {
                                        1
                                    };

                                    if created_at_us > 0 {
                                        // This is Initial event, so it's always first reception
                                        let latency_us =
                                            received_at_us.saturating_sub(created_at_us);
                                        let latency_ms = latency_us as f64 / 1000.0;

                                        println!(
                                            "[{}] ✓ Squad summary received (initial): {} (latency: {:.3}ms)",
                                            node_id, doc_id, latency_ms
                                        );

                                        log_metrics(&MetricsEvent::DocumentReceived {
                                            node_id: node_id.to_string(),
                                            doc_id: doc_id.to_string(),
                                            created_at_us,
                                            last_modified_us,
                                            received_at_us,
                                            latency_us,
                                            latency_ms,
                                            version,
                                            is_first_reception: true,
                                            latency_type: "creation".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    ChangeEvent::Updated { document, .. } => {
                        // Process document update (this is where P2P propagation is measured)
                        let received_at_us = now_micros();
                        if let Some(doc_id) = &document.id {
                            if doc_id.starts_with("squad-") {
                                // Extract timestamps with proper delta sync semantics
                                let created_at_us = if let Some(ts) = document.get("created_at_us")
                                {
                                    ts.as_u64().unwrap_or(0) as u128
                                } else if let Some(ts) = document.get("timestamp_us") {
                                    ts.as_u64().unwrap_or(0) as u128
                                } else {
                                    0
                                };

                                let last_modified_us =
                                    if let Some(ts) = document.get("last_modified_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else {
                                        created_at_us
                                    };

                                let version = if let Some(v) = document.get("version") {
                                    v.as_u64().unwrap_or(1)
                                } else {
                                    1
                                };

                                if created_at_us > 0 {
                                    // Assume update since this is ChangeEvent::Updated
                                    let latency_us =
                                        received_at_us.saturating_sub(last_modified_us);
                                    let latency_ms = latency_us as f64 / 1000.0;

                                    println!(
                                        "[{}] ✓ Squad summary received: {} (latency: {:.3}ms)",
                                        node_id, doc_id, latency_ms
                                    );

                                    log_metrics(&MetricsEvent::DocumentReceived {
                                        node_id: node_id.to_string(),
                                        doc_id: doc_id.to_string(),
                                        created_at_us,
                                        last_modified_us,
                                        received_at_us,
                                        latency_us,
                                        latency_ms,
                                        version,
                                        is_first_reception: false,
                                        latency_type: "update".to_string(),
                                    });
                                }
                            }
                        }
                    }
                    ChangeEvent::Removed { .. } => {
                        // Ignore removals
                    }
                }
            }
            Ok(None) => {
                // Channel closed
                aggregation_handle.abort();
                return Err("Change stream closed unexpectedly".into());
            }
            Err(_) => {
                // Timeout waiting for event - continue loop
                continue;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    let mut node_id = None;
    let mut mode = None;
    let mut backend_type = None;
    let mut tcp_listen_port = None;
    let mut tcp_connect_addr = None;
    let mut node_type = None;
    let mut update_rate_ms = None;
    let mut hive_filter_enabled = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--node-id" => {
                i += 1;
                if i < args.len() {
                    node_id = Some(args[i].clone());
                }
            }
            "--mode" => {
                i += 1;
                if i < args.len() {
                    mode = Some(args[i].clone());
                }
            }
            "--backend" => {
                i += 1;
                if i < args.len() {
                    backend_type = Some(args[i].clone());
                }
            }
            "--tcp-listen" => {
                i += 1;
                if i < args.len() {
                    tcp_listen_port = Some(args[i].parse::<u16>().expect("Invalid port"));
                }
            }
            "--tcp-connect" => {
                i += 1;
                if i < args.len() {
                    tcp_connect_addr = Some(args[i].clone());
                }
            }
            "--node-type" => {
                i += 1;
                if i < args.len() {
                    node_type = Some(args[i].clone());
                }
            }
            "--update-rate-ms" => {
                i += 1;
                if i < args.len() {
                    update_rate_ms = Some(args[i].parse::<u64>().expect("Invalid update rate"));
                }
            }
            "--hive-filter" => {
                hive_filter_enabled = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Check for HIVE_FILTER_ENABLED environment variable
    if let Ok(val) = std::env::var("HIVE_FILTER_ENABLED") {
        hive_filter_enabled = val.to_lowercase() == "true" || val == "1";
    }

    // Check for MODE environment variable (Mode 4: hierarchical aggregation)
    let hierarchical_mode = std::env::var("MODE")
        .unwrap_or_else(|_| String::new())
        .to_lowercase()
        == "hierarchical";

    let node_id = node_id.expect("--node-id required");
    let mode = mode.expect("--mode required");
    let backend_type = backend_type.unwrap_or_else(|| "ditto".to_string());
    let node_type = node_type.unwrap_or_else(|| "unknown".to_string());
    let update_rate_ms = update_rate_ms.unwrap_or(5000); // Default: 5 seconds

    println!("[{}] HIVE Network Simulation Node starting", node_id);
    println!("[{}] Mode: {}", node_id, mode);
    println!("[{}] Backend: {}", node_id, backend_type);
    println!("[{}] Node Type: {}", node_id, node_type);
    println!("[{}] Update Rate: {}ms", node_id, update_rate_ms);
    println!(
        "[{}] HIVE Filtering: {}",
        node_id,
        if hive_filter_enabled {
            "ENABLED (differential updates)"
        } else {
            "DISABLED (full replication)"
        }
    );

    if hierarchical_mode {
        println!("[{}] MODE 4: Hierarchical aggregation enabled", node_id);
    }

    if let Some(port) = tcp_listen_port {
        println!("[{}] TCP: Will listen on port {}", node_id, port);
    }
    if let Some(ref addr) = tcp_connect_addr {
        println!("[{}] TCP: Will connect to {}", node_id, addr);
    }

    // Create backend
    println!("[{}] Creating {} backend...", node_id, backend_type);
    let backend = create_backend(&backend_type)?;

    // Initialize backend
    println!("[{}] Initializing backend...", node_id);
    let config = create_backend_config(&node_id, &backend_type, tcp_listen_port, tcp_connect_addr)?;
    backend.initialize(config).await?;
    println!("[{}] ✓ Backend initialized", node_id);

    // Get sync engine once
    let sync_engine = backend.sync_engine();

    // Create subscription for the test collection
    // Use capability-filtered query if HIVE filtering is enabled
    println!("[{}] Creating sync subscription...", node_id);
    let subscription_query = if hive_filter_enabled {
        if hierarchical_mode && std::env::var("ROLE").unwrap_or_default() == "platoon_leader" {
            // Platoon leaders ONLY subscribe to squad_summaries, not individual NodeStates
            println!(
                "[{}]   → Subscribing to squad_summaries (hierarchical mode)",
                node_id
            );
            Query::Custom("collection_name == 'squad_summaries'".to_string())
        } else {
            // Existing HIVE-filtered query for soldiers and squad leaders
            println!(
                "[{}]   → Using HIVE-filtered query for role: {}",
                node_id, node_type
            );
            Query::Custom(format!(
                "public == true OR CONTAINS(authorized_roles, '{}')",
                node_type
            ))
        }
    } else {
        // Full replication mode: Subscribe to all documents
        println!("[{}]   → Using full replication (Query::All)", node_id);
        Query::All
    };
    let _subscription = sync_engine
        .subscribe("sim_poc", &subscription_query)
        .await?;
    println!("[{}] ✓ Sync subscription created", node_id);

    // Start sync
    println!("[{}] Starting sync...", node_id);
    sync_engine.start_sync().await?;
    println!("[{}] ✓ Sync started", node_id);

    // Step 4: Spawn aggregation tasks based on ROLE if in hierarchical mode
    if hierarchical_mode {
        let role = std::env::var("ROLE").unwrap_or_default();

        match role.as_str() {
            "squad_leader" => {
                println!("[{}] Spawning squad leader aggregation task...", node_id);

                let squad_id =
                    std::env::var("SQUAD_ID").unwrap_or_else(|_| "squad-unknown".to_string());
                let squad_members_str =
                    std::env::var("SQUAD_MEMBERS").unwrap_or_else(|_| String::new());
                let member_ids: Vec<String> = squad_members_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().to_string())
                    .collect();

                // Get DittoStore from backend
                if let Some(ditto_backend) = backend.as_any().downcast_ref::<DittoBackend>() {
                    match ditto_backend.get_ditto_store() {
                        Ok(ditto_store) => {
                            // Wrap DittoStore in DittoSummaryStorage for backend abstraction
                            let storage =
                                Arc::new(hive_protocol::storage::DittoSummaryStorage::new(
                                    Arc::clone(&ditto_store),
                                ));
                            let coordinator = Arc::new(HierarchicalAggregator::new(storage));

                            // Phase 3: Instantiate CommandCoordinator for command dissemination
                            let cmd_storage =
                                Arc::new(DittoCommandStorage::new(Arc::clone(&ditto_store)));
                            let cmd_coordinator = Arc::new(CommandCoordinator::new(
                                Some(squad_id.clone()),
                                node_id.clone(),
                                member_ids.clone(),
                                cmd_storage,
                            ));

                            let node_id_clone = node_id.clone();
                            let member_ids_clone = member_ids.clone();

                            println!(
                                "[{}] → Squad: {}, Members: {:?}",
                                node_id, squad_id, member_ids
                            );

                            // Spawn aggregation loop
                            tokio::spawn(async move {
                                if let Err(e) = squad_leader_aggregation_loop(
                                    coordinator,
                                    squad_id,
                                    node_id_clone.clone(),
                                    member_ids,
                                )
                                .await
                                {
                                    eprintln!(
                                        "[{}] Squad leader aggregation error: {}",
                                        node_id_clone, e
                                    );
                                }
                            });

                            // Phase 3: Spawn command demo task (issue one command every 30 seconds)
                            let cmd_node_id = node_id.clone();
                            tokio::spawn(async move {
                                sleep(Duration::from_secs(15)).await; // Wait before first command
                                loop {
                                    if let Err(e) = demo_command_issuance(
                                        Arc::clone(&cmd_coordinator),
                                        cmd_node_id.clone(),
                                        Scope::Individual,
                                        member_ids_clone.clone(),
                                    )
                                    .await
                                    {
                                        eprintln!(
                                            "[{}] Command issuance error: {}",
                                            cmd_node_id, e
                                        );
                                    }
                                    sleep(Duration::from_secs(30)).await;
                                }
                            });

                            println!("[{}] ✓ Squad leader aggregation task spawned", node_id);
                        }
                        Err(e) => {
                            eprintln!("[{}] ✗ Failed to get DittoStore: {}", node_id, e);
                        }
                    }
                } else {
                    eprintln!(
                        "[{}] ✗ Cannot spawn squad leader task: backend is not DittoBackend",
                        node_id
                    );
                }
            }
            "platoon_leader" => {
                println!("[{}] Spawning platoon leader aggregation task...", node_id);

                let platoon_id =
                    std::env::var("PLATOON_ID").unwrap_or_else(|_| "platoon-1".to_string());

                // Create observer for squad summaries arriving via P2P mesh
                let change_stream_result = backend.document_store().observe(
                    "sim_poc",
                    &Query::Custom("collection_name == 'squad_summaries'".to_string()),
                );

                // Get DittoStore from backend
                if let Some(ditto_backend) = backend.as_any().downcast_ref::<DittoBackend>() {
                    match (ditto_backend.get_ditto_store(), change_stream_result) {
                        (Ok(ditto_store), Ok(change_stream)) => {
                            // Wrap DittoStore in DittoSummaryStorage for backend abstraction
                            let storage =
                                Arc::new(hive_protocol::storage::DittoSummaryStorage::new(
                                    Arc::clone(&ditto_store),
                                ));
                            let coordinator = Arc::new(HierarchicalAggregator::new(storage));

                            // Phase 3: Instantiate CommandCoordinator for command dissemination
                            // Platoon leader can command squads in the platoon
                            let cmd_storage =
                                Arc::new(DittoCommandStorage::new(Arc::clone(&ditto_store)));
                            let _cmd_coordinator = Arc::new(CommandCoordinator::new(
                                None, // Platoon leader is not in a squad
                                node_id.clone(),
                                vec![], // Squad IDs will be determined dynamically
                                cmd_storage,
                            ));

                            let node_id_clone = node_id.clone();

                            println!("[{}] → Platoon: {}", node_id, platoon_id);

                            tokio::spawn(async move {
                                if let Err(e) = platoon_leader_aggregation_loop(
                                    change_stream,
                                    coordinator,
                                    platoon_id,
                                    node_id_clone.clone(),
                                )
                                .await
                                {
                                    eprintln!(
                                        "[{}] Platoon leader aggregation error: {}",
                                        node_id_clone, e
                                    );
                                }
                            });

                            println!("[{}] ✓ Platoon leader aggregation task spawned", node_id);
                        }
                        (Err(e), _) => {
                            eprintln!("[{}] ✗ Failed to get DittoStore: {}", node_id, e);
                        }
                        (_, Err(e)) => {
                            eprintln!("[{}] ✗ Failed to create change stream: {}", node_id, e);
                        }
                    }
                } else {
                    eprintln!(
                        "[{}] ✗ Cannot spawn platoon leader task: backend is not DittoBackend",
                        node_id
                    );
                }
            }
            _ => {
                println!(
                    "[{}] No aggregation task needed for role: {}",
                    node_id, role
                );
            }
        }
    }

    // Wait a moment for peer discovery
    println!("[{}] Waiting for peer discovery (5s)...", node_id);
    sleep(Duration::from_secs(5)).await;

    // Execute mode-specific behavior
    let result = match mode.as_str() {
        "writer" => writer_mode(&*backend, &node_id, &node_type, update_rate_ms).await,
        "reader" => reader_mode(&*backend, &node_id).await,
        "hierarchical" => {
            // In hierarchical mode, leaders aggregate data but also act as writers
            writer_mode(&*backend, &node_id, &node_type, update_rate_ms).await
        }
        _ => {
            eprintln!("[{}] ✗ Invalid mode: {}", node_id, mode);
            std::process::exit(1);
        }
    };

    match result {
        Ok(()) => {
            println!("[{}] ✓✓✓ HIVE SIMULATION SUCCESS ✓✓✓", node_id);
            // Shutdown gracefully
            backend.shutdown().await?;
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[{}] ✗✗✗ HIVE SIMULATION FAILED: {} ✗✗✗", node_id, e);
            backend.shutdown().await.ok();
            std::process::exit(1);
        }
    }
}

/// Get current Unix timestamp in microseconds
fn now_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros()
}

/// Log metrics event as JSON to stderr (for parsing)
fn log_metrics(event: &MetricsEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        eprintln!("METRICS: {}", json);
    }
}

/// Create a backend instance based on type
fn create_backend(
    backend_type: &str,
) -> Result<Box<dyn DataSyncBackend>, Box<dyn std::error::Error>> {
    match backend_type {
        "ditto" => Ok(Box::new(DittoBackend::new())),
        _ => Err(format!("Unknown backend type: {}", backend_type).into()),
    }
}

/// Create backend configuration from environment and CLI args
fn create_backend_config(
    node_id: &str,
    backend_type: &str,
    tcp_listen_port: Option<u16>,
    tcp_connect_addr: Option<String>,
) -> Result<BackendConfig, Box<dyn std::error::Error>> {
    let persistence_dir = PathBuf::from(format!("/tmp/hive_sim_{}", node_id));
    std::fs::create_dir_all(&persistence_dir)?;

    let enable_mdns = tcp_listen_port.is_none() && tcp_connect_addr.is_none();
    let transport = TransportConfig {
        tcp_listen_port,
        tcp_connect_address: tcp_connect_addr.clone(),
        enable_mdns,
        enable_bluetooth: false,
        enable_websocket: false,
        custom: HashMap::new(),
    };

    eprintln!(
        "[{}] Transport config: listen={:?}, connect={:?}, mdns={}",
        node_id, tcp_listen_port, tcp_connect_addr, enable_mdns
    );

    let config = match backend_type {
        "ditto" => {
            // Load Ditto-specific environment variables
            let app_id = std::env::var("DITTO_APP_ID")?;
            let shared_key = std::env::var("DITTO_SHARED_KEY")?;

            BackendConfig {
                app_id,
                persistence_dir,
                shared_key: Some(shared_key),
                transport,
                extra: HashMap::new(),
            }
        }
        _ => return Err(format!("Unknown backend type: {}", backend_type).into()),
    };

    Ok(config)
}

/// Writer mode: Send periodic updates at configured rate
async fn writer_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === WRITER MODE ===", node_id);
    println!("[{}] Sending updates every {}ms", node_id, update_rate_ms);

    let update_interval = Duration::from_millis(update_rate_ms);
    let mut message_number: u64 = 0;
    let doc_id = format!("sim_doc_{}", node_id);

    // Send periodic updates for 15 seconds (test duration)
    let test_duration = Duration::from_secs(15);
    let start_time = Instant::now();

    while start_time.elapsed() < test_duration {
        message_number += 1;
        let timestamp_us = now_micros();

        // Create update message
        let message_content = format!(
            "Update #{} from {} ({})",
            message_number, node_id, node_type
        );

        // Create document fields
        let mut fields = HashMap::new();
        fields.insert(
            "message".to_string(),
            Value::String(message_content.clone()),
        );
        fields.insert("timestamp_us".to_string(), serde_json::json!(timestamp_us));
        fields.insert("created_by".to_string(), Value::String(node_id.to_string()));
        fields.insert(
            "node_type".to_string(),
            Value::String(node_type.to_string()),
        );
        fields.insert(
            "message_number".to_string(),
            serde_json::json!(message_number),
        );
        // Add HIVE authorization field
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields.clone());

        // Calculate approximate message size
        let message_json = serde_json::to_string(&fields)?;
        let message_size_bytes = message_json.len();

        // Insert/update document
        backend.document_store().upsert("sim_poc", document).await?;

        println!(
            "[{}] ✓ Update #{} sent ({} bytes)",
            node_id, message_number, message_size_bytes
        );

        // Log metrics for first message
        if message_number == 1 {
            log_metrics(&MetricsEvent::DocumentInserted {
                node_id: node_id.to_string(),
                doc_id: doc_id.clone(),
                timestamp_us,
            });
        }

        // Log message sent metrics
        log_metrics(&MetricsEvent::MessageSent {
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            message_number,
            message_size_bytes,
            timestamp_us,
        });

        // Wait for next update interval
        sleep(update_interval).await;
    }

    println!(
        "[{}] Writer complete: sent {} updates over {:?}",
        node_id,
        message_number,
        start_time.elapsed()
    );

    // Create test document with acknowledgment pattern
    println!("[{}] Creating test document with ack pattern...", node_id);
    let test_timestamp_us = now_micros();
    let expected_acks = 11; // Number of reader nodes in 12-node topology

    let mut test_fields = HashMap::new();
    test_fields.insert(
        "message".to_string(),
        Value::String("Hello from HIVE Simulation!".to_string()),
    );
    test_fields.insert(
        "timestamp_us".to_string(),
        serde_json::json!(test_timestamp_us),
    );
    test_fields.insert("ack_required".to_string(), Value::Bool(true));
    test_fields.insert(
        "acked_by".to_string(),
        serde_json::json!(Vec::<String>::new()),
    );
    test_fields.insert(
        "expected_acks".to_string(),
        serde_json::json!(expected_acks),
    );
    test_fields.insert("public".to_string(), Value::Bool(true));

    let test_doc = Document::with_id("sim_test_001".to_string(), test_fields);
    backend.document_store().upsert("sim_poc", test_doc).await?;

    // Log metrics for the test document
    log_metrics(&MetricsEvent::DocumentInserted {
        node_id: node_id.to_string(),
        doc_id: "sim_test_001".to_string(),
        timestamp_us: test_timestamp_us,
    });

    println!(
        "[{}] ✓ Test document created, waiting for {} acknowledgments...",
        node_id, expected_acks
    );

    // Create observer for the test document to watch for acks
    let ack_query = Query::Eq {
        field: "_id".to_string(),
        value: Value::String("sim_test_001".to_string()),
    };
    let mut ack_stream = backend.document_store().observe("sim_poc", &ack_query)?;

    // Wait for all acknowledgments with timeout
    let ack_timeout = Duration::from_secs(30);
    let ack_start = Instant::now();

    loop {
        if ack_start.elapsed() > ack_timeout {
            eprintln!("[{}] ✗ Timeout waiting for acknowledgments", node_id);
            return Err("Timeout: Not all acknowledgments received".into());
        }

        // Wait for next change event
        let event =
            tokio::time::timeout(Duration::from_millis(100), ack_stream.receiver.recv()).await;

        match event {
            Ok(Some(change_event)) => {
                let doc = match &change_event {
                    ChangeEvent::Updated { document, .. } => document,
                    ChangeEvent::Initial { documents } => {
                        if let Some(d) = documents.first() {
                            d
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                // Check acked_by array
                if let Some(acked_by_value) = doc.get("acked_by") {
                    if let Some(acked_by) = acked_by_value.as_array() {
                        let ack_count = acked_by.len();

                        if ack_count > 0 {
                            println!(
                                "[{}] Received {} acknowledgments so far...",
                                node_id, ack_count
                            );
                        }

                        if ack_count >= expected_acks {
                            let all_acked_at_us = now_micros();
                            let round_trip_latency_us = all_acked_at_us - test_timestamp_us;
                            let round_trip_latency_ms = round_trip_latency_us as f64 / 1000.0;

                            println!("[{}] ✓ All {} acknowledgments received! Round-trip latency: {:.3}ms",
                                     node_id, ack_count, round_trip_latency_ms);

                            // Log round-trip metrics
                            log_metrics(&MetricsEvent::AllAcksReceived {
                                node_id: node_id.to_string(),
                                doc_id: "sim_test_001".to_string(),
                                inserted_at_us: test_timestamp_us,
                                all_acked_at_us,
                                round_trip_latency_us,
                                round_trip_latency_ms,
                                ack_count,
                            });

                            return Ok(());
                        }
                    }
                }
            }
            Ok(None) => {
                return Err("Change stream closed unexpectedly".into());
            }
            Err(_) => {
                // Timeout - continue checking
                continue;
            }
        }
    }
}

/// Reader mode: Use event-driven observer to monitor updates
async fn reader_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === READER MODE ===", node_id);
    println!("[{}] Using event-driven observer for updates...", node_id);

    // Subscribe to all documents in the collection
    let mut change_stream = backend.document_store().observe("sim_poc", &Query::All)?;

    // Track which periodic updates we've received
    let mut received_updates = HashSet::new();
    // Track unique test document insertions by timestamp
    let mut test_doc_timestamps = HashSet::new();

    let timeout = Duration::from_secs(20);
    let start = Instant::now();

    // Listen for document changes via observer
    loop {
        // Check timeout
        if start.elapsed() > timeout {
            if test_doc_timestamps.is_empty() {
                return Err("Timeout: Test document not received".into());
            }
            break;
        }

        // Wait for next change event with timeout
        let event =
            tokio::time::timeout(Duration::from_millis(100), change_stream.receiver.recv()).await;

        match event {
            Ok(Some(change_event)) => {
                match change_event {
                    ChangeEvent::Initial { documents } => {
                        // Process initial snapshot
                        for doc in documents {
                            process_document(
                                &doc,
                                node_id,
                                backend,
                                &mut received_updates,
                                &mut test_doc_timestamps,
                            )
                            .await?;
                        }
                    }
                    ChangeEvent::Updated { document, .. } => {
                        // Process document update
                        process_document(
                            &document,
                            node_id,
                            backend,
                            &mut received_updates,
                            &mut test_doc_timestamps,
                        )
                        .await?;
                    }
                    ChangeEvent::Removed { .. } => {
                        // Ignore removals
                    }
                }
            }
            Ok(None) => {
                // Channel closed
                return Err("Change stream closed unexpectedly".into());
            }
            Err(_) => {
                // Timeout waiting for event - continue loop
                continue;
            }
        }
    }

    Ok(())
}

/// Process a document and log latency metrics
async fn process_document(
    doc: &Document,
    node_id: &str,
    backend: &dyn DataSyncBackend,
    received_updates: &mut HashSet<u64>,
    test_doc_timestamps: &mut HashSet<u128>,
) -> Result<(), Box<dyn std::error::Error>> {
    let received_at_us = now_micros();

    // Extract document ID
    let doc_id = doc.id.as_ref().ok_or("Document missing ID")?;

    // Extract timestamps with proper delta sync semantics
    let created_at_us = if let Some(ts) = doc.get("created_at_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else if let Some(ts) = doc.get("timestamp_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else {
        0
    };

    let last_modified_us = if let Some(ts) = doc.get("last_modified_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else {
        created_at_us
    };

    let version = if let Some(v) = doc.get("version") {
        v.as_u64().unwrap_or(1)
    } else {
        1
    };

    // Track which documents we've seen
    let is_first_reception = !test_doc_timestamps.contains(&created_at_us);

    // Calculate appropriate latency based on context
    let (latency_us, latency_type) = if is_first_reception {
        (
            received_at_us.saturating_sub(created_at_us),
            "creation".to_string(),
        )
    } else {
        (
            received_at_us.saturating_sub(last_modified_us),
            "update".to_string(),
        )
    };

    let latency_ms = latency_us as f64 / 1000.0;

    // Check if this is a periodic update document
    if doc_id.starts_with("sim_doc_") {
        if let Some(msg_num_value) = doc.get("message_number") {
            let msg_num = msg_num_value.as_u64().unwrap_or(0);

            if !received_updates.contains(&msg_num) {
                received_updates.insert(msg_num);

                println!(
                    "[{}] ✓ Periodic update #{} received (latency: {:.3}ms)",
                    node_id, msg_num, latency_ms
                );

                log_metrics(&MetricsEvent::DocumentReceived {
                    node_id: node_id.to_string(),
                    doc_id: format!("{}_msg{}", doc_id, msg_num),
                    created_at_us,
                    last_modified_us,
                    received_at_us,
                    latency_us,
                    latency_ms,
                    version,
                    is_first_reception,
                    latency_type: latency_type.clone(),
                });
            }
        }
    }
    // Check if this is the test document
    else if doc_id == "sim_test_001" {
        if created_at_us > 0 && !test_doc_timestamps.contains(&created_at_us) {
            test_doc_timestamps.insert(created_at_us);

            println!(
                "[{}] ✓ Test document received (latency: {:.3}ms)",
                node_id, latency_ms
            );

            // Verify content
            if let Some(Value::String(message)) = doc.get("message") {
                if message == "Hello from HIVE Simulation!" {
                    println!("[{}] ✓ Document content verified", node_id);

                    log_metrics(&MetricsEvent::DocumentReceived {
                        node_id: node_id.to_string(),
                        doc_id: "sim_test_001".to_string(),
                        created_at_us,
                        last_modified_us,
                        received_at_us,
                        latency_us,
                        latency_ms,
                        version,
                        is_first_reception,
                        latency_type: latency_type.clone(),
                    });

                    // Check if acknowledgment is required
                    if let Some(Value::Bool(ack_required)) = doc.get("ack_required") {
                        if *ack_required {
                            println!(
                                "[{}] Acknowledgment required - updating document...",
                                node_id
                            );

                            // Query the current document to get the latest acked_by array
                            let query = Query::Eq {
                                field: "_id".to_string(),
                                value: Value::String("sim_test_001".to_string()),
                            };
                            let docs = backend.document_store().query("sim_poc", &query).await?;

                            if let Some(current_doc) = docs.first() {
                                // Get current acked_by array
                                let mut acked_by: Vec<String> =
                                    if let Some(acked) = current_doc.get("acked_by") {
                                        if let Some(arr) = acked.as_array() {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect()
                                        } else {
                                            Vec::new()
                                        }
                                    } else {
                                        Vec::new()
                                    };

                                // Add this node if not already in the list
                                if !acked_by.contains(&node_id.to_string()) {
                                    acked_by.push(node_id.to_string());

                                    // Create updated document
                                    let mut updated_fields = HashMap::new();
                                    for (k, v) in current_doc.fields.iter() {
                                        updated_fields.insert(k.clone(), v.clone());
                                    }
                                    updated_fields.insert(
                                        "acked_by".to_string(),
                                        serde_json::json!(acked_by),
                                    );

                                    let updated_doc = Document {
                                        id: Some("sim_test_001".to_string()),
                                        fields: updated_fields,
                                        updated_at: current_doc.updated_at,
                                    };

                                    backend
                                        .document_store()
                                        .upsert("sim_poc", updated_doc)
                                        .await?;

                                    println!(
                                        "[{}] ✓ Acknowledgment sent (acked_by count: {})",
                                        node_id,
                                        acked_by.len()
                                    );

                                    log_metrics(&MetricsEvent::DocumentAcknowledged {
                                        node_id: node_id.to_string(),
                                        doc_id: "sim_test_001".to_string(),
                                        timestamp_us: now_micros(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Check if this is a squad summary document (Mode 4 hierarchical)
    else if doc_id.starts_with("squad-") && doc_id.ends_with("-summary") {
        println!(
            "[{}] ✓ Squad summary received: {} (latency: {:.3}ms)",
            node_id, doc_id, latency_ms
        );

        log_metrics(&MetricsEvent::DocumentReceived {
            node_id: node_id.to_string(),
            doc_id: doc_id.to_string(),
            created_at_us,
            last_modified_us,
            received_at_us,
            latency_us,
            latency_ms,
            version,
            is_first_reception,
            latency_type: latency_type.clone(),
        });
    }
    // Check if this is a platoon summary document
    else if doc_id.starts_with("platoon-") && doc_id.ends_with("-summary") {
        println!(
            "[{}] ✓ Platoon summary received: {} (latency: {:.3}ms)",
            node_id, doc_id, latency_ms
        );

        log_metrics(&MetricsEvent::DocumentReceived {
            node_id: node_id.to_string(),
            doc_id: doc_id.to_string(),
            created_at_us,
            last_modified_us,
            received_at_us,
            latency_us,
            latency_ms,
            version,
            is_first_reception,
            latency_type: latency_type.clone(),
        });
    }

    Ok(())
}
