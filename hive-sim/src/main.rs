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
//! - ROLE: Node role - "soldier", "squad_leader", "platoon_leader", "company_commander"
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

#[cfg(feature = "automerge-backend")]
use hive_protocol::sync::automerge::AutomergeIrohBackend;
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
use hive_protocol::command::{CommandCoordinator, CommandStorage};
use hive_protocol::storage::DittoCommandStorage;

// AutomergeIroh backend components
#[cfg(feature = "automerge-backend")]
use hive_protocol::network::IrohTransport;
#[cfg(feature = "automerge-backend")]
use hive_protocol::storage::AutomergeStore;
use hive_schema::command::v1::{
    command_target::Scope, AckStatus, CommandAcknowledgment, CommandTarget, HierarchicalCommand,
};
use hive_schema::common::v1::Timestamp;

// Lab 3b: Flat mesh coordination with CRDT
use hive_mesh::beacon::{NodeMobility, NodeProfile, NodeResources};
use hive_mesh::FlatMeshCoordinator;

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
    CommandReceived {
        node_id: String,
        command_id: String,
        originator_id: String,
        received_at_us: u128,
        latency_us: u128,
        latency_ms: f64,
    },
    CommandAcknowledged {
        node_id: String,
        command_id: String,
        status: String, // "RECEIVED", "COMPLETED", "FAILED"
        timestamp_us: u128,
    },
    AcknowledgmentReceived {
        node_id: String, // Originator who receives the ack
        command_id: String,
        ack_from_node_id: String, // Subordinate who sent ack
        status: String,           // "RECEIVED", "COMPLETED", "FAILED"
        timestamp_us: u128,
        ack_count: usize,          // How many acks received so far
        expected_ack_count: usize, // Total expected acks
    },
    #[allow(dead_code)] // Will be used for round-trip latency tracking
    AllCommandAcksReceived {
        node_id: String,
        command_id: String,
        issued_at_us: u128,
        all_acked_at_us: u128,
        round_trip_latency_us: u128,
        round_trip_latency_ms: f64,
        ack_count: usize,
    },
    // Phase 4: Propagation latency tracking events
    AggregationStarted {
        node_id: String,
        tier: String,           // "squad", "platoon", "company"
        input_doc_type: String, // What we're aggregating (NodeState, SquadSummary, etc.)
        input_count: usize,     // How many documents we're aggregating
        timestamp_us: u128,
    },
    AggregationCompleted {
        node_id: String,
        tier: String,
        input_doc_type: String,
        output_doc_type: String, // What we produced (SquadSummary, PlatoonSummary, etc.)
        output_doc_id: String,
        input_count: usize,
        processing_time_us: u128, // Time spent aggregating
        timestamp_us: u128,
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

/// Phase 3: Command reception and acknowledgment handler
///
/// Observes commands targeted at this node and sends acknowledgments back.
/// This completes the bidirectional flow: commands down, acknowledgments up.
async fn handle_command_reception(
    node_id: String,
    ditto_store: Arc<hive_protocol::storage::DittoStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] Starting command reception observer...", node_id);

    // Create DittoCommandStorage for observing commands
    let cmd_storage = Arc::new(DittoCommandStorage::new(Arc::clone(&ditto_store)));

    // Set up observer for commands targeting this node
    let node_id_clone = node_id.clone();
    let ditto_store_clone = Arc::clone(&ditto_store);

    let _observer = cmd_storage
        .observe_commands(
            &node_id,
            Box::new(move |command: HierarchicalCommand| {
                let node_id = node_id_clone.clone();
                let ditto_store = Arc::clone(&ditto_store_clone);

                Box::pin(async move {
                    let received_at_us = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros();

                    // Log CommandReceived event
                    let issued_at_us = command
                        .issued_at
                        .as_ref()
                        .map(|t| (t.seconds as u128 * 1_000_000) + (t.nanos as u128 / 1000))
                        .unwrap_or(0);

                    let latency_us = if issued_at_us > 0 {
                        received_at_us.saturating_sub(issued_at_us)
                    } else {
                        0
                    };

                    let event = MetricsEvent::CommandReceived {
                        node_id: node_id.clone(),
                        command_id: command.command_id.clone(),
                        originator_id: command.originator_id.clone(),
                        received_at_us,
                        latency_us,
                        latency_ms: latency_us as f64 / 1000.0,
                    };
                    if let Ok(json) = serde_json::to_string(&event) {
                        println!("{}", json);
                    }

                    // Send acknowledgment
                    let ack = CommandAcknowledgment {
                        command_id: command.command_id.clone(),
                        node_id: node_id.clone(),
                        status: AckStatus::AckCompleted as i32,
                        reason: None,
                        timestamp: Some(Timestamp {
                            seconds: (received_at_us / 1_000_000) as u64,
                            nanos: ((received_at_us % 1_000_000) * 1000) as u32,
                        }),
                    };

                    // Publish acknowledgment to Ditto
                    let ack_id = format!("{}-ack-{}", command.command_id, node_id);
                    if let Err(e) = ditto_store.upsert_command_ack(&ack_id, &ack).await {
                        eprintln!("[{}] Failed to send acknowledgment: {}", node_id, e);
                    } else {
                        // Log CommandAcknowledged event
                        let event = MetricsEvent::CommandAcknowledged {
                            node_id: node_id.clone(),
                            command_id: command.command_id.clone(),
                            status: "COMPLETED".to_string(),
                            timestamp_us: received_at_us,
                        };
                        if let Ok(json) = serde_json::to_string(&event) {
                            println!("{}", json);
                        }
                    }
                })
            }),
        )
        .await?;

    println!("[{}] ✓ Command reception observer active", node_id);

    // Keep the observer alive
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}

/// Phase 3: Acknowledgment collection handler (optional)
///
/// Observes acknowledgments coming back from subordinates for commands issued by this node.
/// This completes the full-duplex flow: originators can track command completion.
///
/// This is OPTIONAL and can be enabled/disabled via CLI flag for experimentation.
async fn handle_acknowledgment_collection(
    node_id: String,
    ditto_store: Arc<hive_protocol::storage::DittoStore>,
    expected_targets: HashMap<String, usize>, // command_id -> expected_ack_count
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Starting acknowledgment collection observer...",
        node_id
    );

    // Track acknowledgments received per command
    let ack_tracker = Arc::new(std::sync::Mutex::new(HashMap::<
        String,
        Vec<CommandAcknowledgment>,
    >::new()));
    let expected_targets_arc = Arc::new(expected_targets);

    // Create DittoCommandStorage for observing acknowledgments
    let cmd_storage = Arc::new(DittoCommandStorage::new(Arc::clone(&ditto_store)));

    // Set up observer for acknowledgments to commands issued by this node
    let node_id_clone = node_id.clone();
    let ack_tracker_clone = Arc::clone(&ack_tracker);
    let expected_targets_clone = Arc::clone(&expected_targets_arc);

    let _observer = cmd_storage
        .observe_acknowledgments(
            &node_id,
            Box::new(move |ack: CommandAcknowledgment| {
                let node_id = node_id_clone.clone();
                let ack_tracker = Arc::clone(&ack_tracker_clone);
                let expected_targets = Arc::clone(&expected_targets_clone);

                Box::pin(async move {
                    let timestamp_us = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros();

                    // Track this acknowledgment
                    let mut tracker = ack_tracker.lock().unwrap();
                    let acks = tracker.entry(ack.command_id.clone()).or_default();
                    acks.push(ack.clone());
                    let ack_count = acks.len();

                    // Get expected count for this command
                    let expected_count =
                        expected_targets.get(&ack.command_id).copied().unwrap_or(1);

                    drop(tracker); // Release lock

                    // Log AcknowledgmentReceived event
                    let status_str = match ack.status {
                        1 => "RECEIVED",
                        2 => "COMPLETED",
                        3 => "FAILED",
                        _ => "UNKNOWN",
                    };

                    let event = MetricsEvent::AcknowledgmentReceived {
                        node_id: node_id.clone(),
                        command_id: ack.command_id.clone(),
                        ack_from_node_id: ack.node_id.clone(),
                        status: status_str.to_string(),
                        timestamp_us,
                        ack_count,
                        expected_ack_count: expected_count,
                    };
                    if let Ok(json) = serde_json::to_string(&event) {
                        println!("{}", json);
                    }

                    // If all acks received, log completion event
                    if ack_count == expected_count {
                        // Note: We'd need the issued_at_us from the original command
                        // For now, just log that all acks are received
                        println!(
                            "[{}] ✓ All acknowledgments received for command {} ({}/{})",
                            node_id, ack.command_id, ack_count, expected_count
                        );
                    }
                })
            }),
        )
        .await?;

    println!("[{}] ✓ Acknowledgment collection observer active", node_id);

    // Keep the observer alive
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}

/// Squad leader aggregation loop (Mode 4)
///
/// Periodically aggregates member NodeStates into SquadSummary and publishes via coordinator.
/// This is the core of hierarchical state aggregation - squad leaders collect member
/// states and create summary documents that get replicated via P2P mesh.
async fn squad_leader_aggregation_loop(
    coordinator: Arc<HierarchicalAggregator>,
    backend: Arc<Box<dyn DataSyncBackend>>,
    squad_id: String,
    node_id: String,
    member_ids: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Started squad leader aggregation for {}",
        node_id, squad_id
    );
    println!("[{}] Squad members: {:?}", node_id, member_ids);

    // Spawn observer for soldier NodeState documents (Lab 4 upward propagation tracking)
    let observer_backend = backend.clone();
    let observer_node_id = node_id.clone();
    let observer_member_ids = member_ids.clone();
    tokio::spawn(async move {
        println!(
            "METRICS: [{}] Starting soldier NodeState observer...",
            observer_node_id
        );

        let query = Query::All;
        if let Ok(mut change_stream) = observer_backend
            .as_ref()
            .document_store()
            .observe("sim_poc", &query)
        {
            loop {
                let event_result =
                    tokio::time::timeout(Duration::from_millis(100), change_stream.receiver.recv())
                        .await;

                match event_result {
                    Ok(Some(change_event)) => {
                        if let ChangeEvent::Updated { document, .. } = change_event {
                            let received_at_us = now_micros();
                            if let Some(doc_id) = &document.id {
                                // Track soldier documents from squad members
                                if doc_id.starts_with("sim_doc_") {
                                    // Check if this is from a squad member
                                    let is_squad_member = observer_member_ids
                                        .iter()
                                        .any(|member| doc_id == &format!("sim_doc_{}", member));

                                    if is_squad_member {
                                        if let Some(created_at_us) = document
                                            .get("timestamp_us")
                                            .and_then(|v| v.as_u64())
                                            .map(|v| v as u128)
                                        {
                                            let latency_us =
                                                received_at_us.saturating_sub(created_at_us);
                                            let latency_ms = latency_us as f64 / 1000.0;

                                            log_metrics(&MetricsEvent::DocumentReceived {
                                                node_id: observer_node_id.clone(),
                                                doc_id: doc_id.clone(),
                                                created_at_us,
                                                last_modified_us: created_at_us,
                                                received_at_us,
                                                latency_us,
                                                latency_ms,
                                                version: 1,
                                                is_first_reception: false,
                                                latency_type: "soldier_to_squad_leader".to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => break,  // Channel closed
                    Err(_) => continue, // Timeout, continue loop
                }
            }
        }
    });

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
            // Log aggregation start
            log_metrics(&MetricsEvent::AggregationStarted {
                node_id: node_id.clone(),
                tier: "squad".to_string(),
                input_doc_type: "NodeState".to_string(),
                input_count: member_states.len(),
                timestamp_us: now_micros(),
            });

            let aggregation_start_time = now_micros();

            // Aggregate into SquadSummary using StateAggregator
            match StateAggregator::aggregate_squad(&squad_id, &node_id, member_states) {
                Ok(squad_summary) => {
                    let timestamp_us = now_micros();
                    let processing_time_us = timestamp_us - aggregation_start_time;

                    // Check if squad summary document exists (create-once pattern)
                    match coordinator.get_squad_summary(&squad_id).await {
                        Ok(None) => {
                            // First time - create document with latency tracking
                            let crdt_start = Instant::now();
                            if let Err(e) = coordinator
                                .create_squad_summary(&squad_id, &squad_summary)
                                .await
                            {
                                eprintln!("[{}] Failed to create squad summary: {}", node_id, e);
                            } else {
                                let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;
                                println!(
                                    "[{}] ✓ Created squad {} ({} members, readiness: {:.2})",
                                    node_id,
                                    squad_id,
                                    squad_summary.member_count,
                                    squad_summary.readiness_score
                                );
                                // Log CRDT create latency for Lab 4 analysis
                                println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"squad_leader\",\"squad_id\":\"{}\",\"operation\":\"create\",\"members_aggregated\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, squad_id, squad_summary.member_count, crdt_latency_ms, timestamp_us
                                );
                            }
                        }
                        Ok(Some(_existing)) => {
                            // Document exists - send delta update with latency tracking
                            use hive_protocol::hierarchy::deltas::SquadDelta;
                            let delta =
                                SquadDelta::from_summary(&squad_summary, timestamp_us as u64);

                            let crdt_start = Instant::now();
                            if let Err(e) = coordinator.update_squad_summary(&squad_id, delta).await
                            {
                                eprintln!("[{}] Failed to update squad summary: {}", node_id, e);
                            } else {
                                let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;
                                println!(
                                    "[{}] ✓ Updated squad {} ({} members, readiness: {:.2})",
                                    node_id,
                                    squad_id,
                                    squad_summary.member_count,
                                    squad_summary.readiness_score
                                );
                                // Log CRDT update latency for Lab 4 analysis
                                println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"squad_leader\",\"squad_id\":\"{}\",\"operation\":\"update\",\"members_aggregated\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, squad_id, squad_summary.member_count, crdt_latency_ms, timestamp_us
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

                    // Log aggregation completion with processing time
                    log_metrics(&MetricsEvent::AggregationCompleted {
                        node_id: node_id.clone(),
                        tier: "squad".to_string(),
                        input_doc_type: "NodeState".to_string(),
                        output_doc_type: "SquadSummary".to_string(),
                        output_doc_id: format!("squad-summary-{}", squad_id),
                        input_count: squad_summary.member_count as usize,
                        processing_time_us,
                        timestamp_us,
                    });

                    // Log aggregation efficiency for Lab 4 analysis
                    let reduction_ratio = squad_summary.member_count as f64;
                    println!(
                        "METRICS: {{\"event_type\":\"AggregationEfficiency\",\"node_id\":\"{}\",\"tier\":\"squad\",\"input_docs\":{},\"output_docs\":1,\"reduction_ratio\":{:.1},\"timestamp_us\":{}}}",
                        node_id, squad_summary.member_count, reduction_ratio, timestamp_us
                    );
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

        // NO POLLING - aggregate continuously with zero delay
        // In production this would be event-driven via member state change stream
        // For now, continuously check and aggregate immediately
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

    // Derive squad IDs from platoon ID
    // platoon-1 → [squad-1A, squad-1B]
    // platoon-2 → [squad-2A, squad-2B], etc.
    let squad_ids: Vec<String> = if let Some(platoon_num) = platoon_id.strip_prefix("platoon-") {
        vec![
            format!("squad-{}A", platoon_num),
            format!("squad-{}B", platoon_num),
        ]
    } else {
        // Fallback for test cases
        vec!["squad-alpha".to_string(), "squad-bravo".to_string()]
    };

    // Track unique document receptions by (doc_id, last_modified_us) for metrics deduplication
    // Using last_modified_us instead of created_at_us because created_at_us is immutable
    let mut seen_doc_updates: HashSet<(String, u128)> = HashSet::new();

    // Helper function to perform aggregation (called when squad summaries change)
    let do_aggregation = |coordinator: Arc<HierarchicalAggregator>,
                          platoon_id: String,
                          node_id: String,
                          squad_ids: Vec<String>| async move {
        // Collect latest squad summaries
        let mut squad_summaries = Vec::new();

        for squad_id in &squad_ids {
            if let Ok(Some(summary)) = coordinator.get_squad_summary(squad_id).await {
                squad_summaries.push(summary);
            }
        }

        // Aggregate when we have all expected squads
        if !squad_summaries.is_empty() && squad_summaries.len() == squad_ids.len() {
            // Log aggregation start
            log_metrics(&MetricsEvent::AggregationStarted {
                node_id: node_id.to_string(),
                tier: "platoon".to_string(),
                input_doc_type: "SquadSummary".to_string(),
                input_count: squad_summaries.len(),
                timestamp_us: now_micros(),
            });

            let aggregation_start_time = now_micros();
            let timestamp_us = aggregation_start_time;

            // Aggregate into PlatoonSummary
            match StateAggregator::aggregate_platoon(&platoon_id, &node_id, squad_summaries) {
                Ok(platoon_summary) => {
                    let processing_time_us = now_micros() - aggregation_start_time;
                    // Check if platoon summary document exists (create-once pattern)
                    match coordinator.get_platoon_summary(&platoon_id).await {
                        Ok(None) => {
                            // First time - create document with latency tracking
                            let crdt_start = Instant::now();
                            if let Err(e) = coordinator
                                .create_platoon_summary(&platoon_id, &platoon_summary)
                                .await
                            {
                                eprintln!("[{}] Failed to create platoon summary: {}", node_id, e);
                            } else {
                                let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;
                                println!(
                                    "[{}] ✓ Created platoon {} ({} squads, {} total members)",
                                    node_id,
                                    platoon_id,
                                    platoon_summary.squad_count,
                                    platoon_summary.total_member_count
                                );
                                // Log CRDT create latency for Lab 4 analysis
                                println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"platoon_leader\",\"platoon_id\":\"{}\",\"operation\":\"create\",\"squads_aggregated\":{},\"total_members\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, platoon_id, platoon_summary.squad_count, platoon_summary.total_member_count, crdt_latency_ms, timestamp_us
                                );
                            }
                        }
                        Ok(Some(_existing)) => {
                            // Document exists - send delta update with latency tracking
                            use hive_protocol::hierarchy::deltas::PlatoonDelta;
                            let delta =
                                PlatoonDelta::from_summary(&platoon_summary, timestamp_us as u64);

                            let crdt_start = Instant::now();
                            if let Err(e) =
                                coordinator.update_platoon_summary(&platoon_id, delta).await
                            {
                                eprintln!("[{}] Failed to update platoon summary: {}", node_id, e);
                            } else {
                                let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;
                                println!(
                                    "[{}] ✓ Updated platoon {} ({} squads, {} total members)",
                                    node_id,
                                    platoon_id,
                                    platoon_summary.squad_count,
                                    platoon_summary.total_member_count
                                );
                                // Log CRDT update latency for Lab 4 analysis
                                println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"platoon_leader\",\"platoon_id\":\"{}\",\"operation\":\"update\",\"squads_aggregated\":{},\"total_members\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, platoon_id, platoon_summary.squad_count, platoon_summary.total_member_count, crdt_latency_ms, timestamp_us
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("[{}] Failed to check platoon summary: {}", node_id, e);
                        }
                    }

                    // Log platoon summary metrics
                    log_metrics(&MetricsEvent::PlatoonSummaryCreated {
                        node_id: node_id.to_string(),
                        platoon_id: platoon_id.to_string(),
                        squad_count: platoon_summary.squad_count as usize,
                        total_member_count: platoon_summary.total_member_count as usize,
                        timestamp_us,
                    });

                    // Log aggregation completion with processing time
                    log_metrics(&MetricsEvent::AggregationCompleted {
                        node_id: node_id.to_string(),
                        tier: "platoon".to_string(),
                        input_doc_type: "SquadSummary".to_string(),
                        output_doc_type: "PlatoonSummary".to_string(),
                        output_doc_id: format!("platoon-summary-{}", platoon_id),
                        input_count: platoon_summary.squad_count as usize,
                        processing_time_us,
                        timestamp_us: now_micros(),
                    });

                    // Log aggregation efficiency for Lab 4 analysis
                    let reduction_ratio = platoon_summary.total_member_count as f64
                        / platoon_summary.squad_count as f64;
                    println!(
                        "METRICS: {{\"event_type\":\"AggregationEfficiency\",\"node_id\":\"{}\",\"tier\":\"platoon\",\"input_docs\":{},\"output_docs\":1,\"reduction_ratio\":{:.1},\"total_members\":{},\"timestamp_us\":{}}}",
                        node_id, platoon_summary.squad_count, reduction_ratio, platoon_summary.total_member_count, now_micros()
                    );
                }
                Err(e) => {
                    eprintln!("[{}] Failed to aggregate platoon: {}", node_id, e);
                }
            }
        }
    };

    // EVENT-DRIVEN: Listen for squad summary changes and aggregate IMMEDIATELY
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

                                    // Note: Storage writes "last_update_us", so check that first
                                    let last_modified_us =
                                        if let Some(ts) = doc.get("last_update_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else if let Some(ts) = doc.get("last_modified_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else {
                                            created_at_us
                                        };

                                    let version = if let Some(v) = doc.get("version") {
                                        v.as_u64().unwrap_or(1)
                                    } else {
                                        1
                                    };

                                    // Track by (doc_id, last_modified_us) to catch each unique update
                                    let update_key = (doc_id.clone(), last_modified_us);
                                    if last_modified_us > 0
                                        && !seen_doc_updates.contains(&update_key)
                                    {
                                        seen_doc_updates.insert(update_key);

                                        // For Initial events, determine if this is truly first reception
                                        // by checking if we've seen this doc_id before with ANY timestamp
                                        let is_first = !seen_doc_updates.iter().any(|(id, ts)| {
                                            id.as_str() == doc_id && *ts != last_modified_us
                                        });

                                        // Calculate latency from last_modified for accurate propagation measurement
                                        let latency_us =
                                            received_at_us.saturating_sub(last_modified_us);
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
                                            is_first_reception: is_first,
                                            latency_type: if is_first {
                                                "creation".to_string()
                                            } else {
                                                "update".to_string()
                                            },
                                        });

                                        // EVENT-DRIVEN: Aggregate immediately when squad summary arrives
                                        do_aggregation(
                                            Arc::clone(&coordinator),
                                            platoon_id.clone(),
                                            node_id.clone(),
                                            squad_ids.clone(),
                                        )
                                        .await;
                                    }
                                }
                            }
                        }

                        // Also aggregate after processing initial snapshot
                        do_aggregation(
                            Arc::clone(&coordinator),
                            platoon_id.clone(),
                            node_id.clone(),
                            squad_ids.clone(),
                        )
                        .await;
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

                                // Note: Storage writes "last_update_us", so check that first
                                let last_modified_us =
                                    if let Some(ts) = document.get("last_update_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if let Some(ts) = document.get("last_modified_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else {
                                        created_at_us
                                    };

                                let version = if let Some(v) = document.get("version") {
                                    v.as_u64().unwrap_or(1)
                                } else {
                                    1
                                };

                                // Track by (doc_id, last_modified_us) to catch each unique update
                                let update_key = (doc_id.clone(), last_modified_us);
                                if last_modified_us > 0 && !seen_doc_updates.contains(&update_key) {
                                    seen_doc_updates.insert(update_key);

                                    // This is ChangeEvent::Updated, so it's always an update
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

                                    // EVENT-DRIVEN: Aggregate immediately when squad summary updated
                                    do_aggregation(
                                        Arc::clone(&coordinator),
                                        platoon_id.clone(),
                                        node_id.clone(),
                                        squad_ids.clone(),
                                    )
                                    .await;
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
    let mut ack_collection_enabled = false;

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
            "--enable-ack-collection" => {
                ack_collection_enabled = true;
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
    let is_hierarchical_mode = std::env::var("MODE")
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

    if is_hierarchical_mode {
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
    let backend = create_backend(&backend_type, &node_id, tcp_listen_port).await?;

    // Initialize backend
    println!("[{}] Initializing backend...", node_id);
    let config = create_backend_config(
        &node_id,
        &backend_type,
        tcp_listen_port,
        tcp_connect_addr.clone(),
    )?;
    backend.initialize(config).await?;
    println!("[{}] ✓ Backend initialized", node_id);

    // Get sync engine once
    let sync_engine = backend.sync_engine();

    // Create subscription for the test collection
    // Use capability-filtered query if HIVE filtering is enabled
    println!("[{}] Creating sync subscription...", node_id);
    let subscription_query = if hive_filter_enabled {
        if is_hierarchical_mode && std::env::var("ROLE").unwrap_or_default() == "platoon_leader" {
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

    // Connect to Automerge peers using static configuration (Issue #235)
    // This is needed because mDNS doesn't work across containerlab network namespaces
    #[cfg(feature = "automerge-backend")]
    if backend_type == "automerge" {
        connect_to_automerge_peers(&sync_engine, &node_id, &tcp_connect_addr).await?;
    }

    // Step 4: Spawn aggregation tasks based on ROLE if in hierarchical mode
    if is_hierarchical_mode {
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
                            if let Some(ditto_backend) =
                                backend.as_any().downcast_ref::<DittoBackend>()
                            {
                                let backend_for_aggregation: Arc<Box<dyn DataSyncBackend>> =
                                    Arc::new(Box::new(ditto_backend.clone()));
                                tokio::spawn(async move {
                                    if let Err(e) = squad_leader_aggregation_loop(
                                        coordinator,
                                        backend_for_aggregation,
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
                            } else {
                                eprintln!(
                                    "[{}] WARNING: Could not clone backend for squad aggregation",
                                    node_id
                                );
                            }

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

                            // Phase 3 (Optional): Spawn acknowledgment collection observer
                            if ack_collection_enabled {
                                let ack_node_id = node_id.clone();
                                let ack_ditto_store = Arc::clone(&ditto_store);

                                // For demo, expect acks from 3 members
                                let expected_targets = HashMap::new();
                                // This will be populated dynamically by the command issuance loop
                                // For now, start empty and we'll observe all acks

                                tokio::spawn(async move {
                                    if let Err(e) = handle_acknowledgment_collection(
                                        ack_node_id.clone(),
                                        ack_ditto_store,
                                        expected_targets,
                                    )
                                    .await
                                    {
                                        eprintln!(
                                            "[{}] Acknowledgment collection error: {}",
                                            ack_node_id, e
                                        );
                                    }
                                });

                                println!(
                                    "[{}] ✓ Acknowledgment collection observer spawned (ENABLED)",
                                    node_id
                                );
                            } else {
                                println!(
                                    "[{}] ⊗ Acknowledgment collection DISABLED (use --enable-ack-collection to enable)",
                                    node_id
                                );
                            }

                            println!("[{}] ✓ Squad leader aggregation task spawned", node_id);
                        }
                        Err(e) => {
                            eprintln!("[{}] ✗ Failed to get DittoStore: {}", node_id, e);
                        }
                    }
                } else {
                    // Try AutomergeIroh backend for hierarchical mode
                    #[cfg(feature = "automerge-backend")]
                    {
                        if let Some(automerge_backend) =
                            backend.as_any().downcast_ref::<AutomergeIrohBackend>()
                        {
                            use hive_protocol::storage::HierarchicalStorageCapable;

                            let storage = automerge_backend.summary_storage();
                            let coordinator = Arc::new(HierarchicalAggregator::new(storage));

                            let cmd_storage = automerge_backend.command_storage();
                            let cmd_coordinator = Arc::new(CommandCoordinator::new(
                                Some(squad_id.clone()),
                                node_id.clone(),
                                member_ids.clone(),
                                cmd_storage,
                            ));

                            let node_id_clone = node_id.clone();
                            let member_ids_clone = member_ids.clone();

                            println!(
                                "[{}] → Squad: {}, Members: {:?} (AutomergeIroh)",
                                node_id, squad_id, member_ids
                            );

                            // Clone backend for aggregation loop
                            let backend_for_aggregation: Arc<Box<dyn DataSyncBackend>> =
                                Arc::new(Box::new((*automerge_backend).clone()));
                            tokio::spawn(async move {
                                if let Err(e) = squad_leader_aggregation_loop(
                                    coordinator,
                                    backend_for_aggregation,
                                    squad_id.clone(),
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

                            // Phase 3: Spawn command demo task
                            let cmd_node_id = node_id.clone();
                            tokio::spawn(async move {
                                sleep(Duration::from_secs(15)).await;
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

                            println!(
                                "[{}] ✓ Squad leader aggregation task spawned (AutomergeIroh)",
                                node_id
                            );
                        } else {
                            eprintln!(
                                "[{}] ✗ Cannot spawn squad leader task: unsupported backend type",
                                node_id
                            );
                        }
                    }

                    #[cfg(not(feature = "automerge-backend"))]
                    {
                        eprintln!(
                            "[{}] ✗ Cannot spawn squad leader task: backend is not DittoBackend",
                            node_id
                        );
                    }
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
                    // Try AutomergeIroh backend for platoon leader
                    #[cfg(feature = "automerge-backend")]
                    {
                        if let Some(automerge_backend) =
                            backend.as_any().downcast_ref::<AutomergeIrohBackend>()
                        {
                            use hive_protocol::storage::HierarchicalStorageCapable;

                            match change_stream_result {
                                Ok(change_stream) => {
                                    let storage = automerge_backend.summary_storage();
                                    let coordinator =
                                        Arc::new(HierarchicalAggregator::new(storage));

                                    let cmd_storage = automerge_backend.command_storage();
                                    let _cmd_coordinator = Arc::new(CommandCoordinator::new(
                                        None,
                                        node_id.clone(),
                                        vec![],
                                        cmd_storage,
                                    ));

                                    let node_id_clone = node_id.clone();

                                    println!(
                                        "[{}] → Platoon: {} (AutomergeIroh)",
                                        node_id, platoon_id
                                    );

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

                                    println!(
                                        "[{}] ✓ Platoon leader aggregation task spawned (AutomergeIroh)",
                                        node_id
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "[{}] ✗ Failed to create change stream: {}",
                                        node_id, e
                                    );
                                }
                            }
                        } else {
                            eprintln!(
                                "[{}] ✗ Cannot spawn platoon leader task: unsupported backend type",
                                node_id
                            );
                        }
                    }

                    #[cfg(not(feature = "automerge-backend"))]
                    {
                        eprintln!(
                            "[{}] ✗ Cannot spawn platoon leader task: backend is not DittoBackend",
                            node_id
                        );
                    }
                }
            }
            _ => {
                println!(
                    "[{}] No aggregation task needed for role: {}",
                    node_id, role
                );
            }
        }

        // Phase 3: Spawn command reception task for ALL nodes in hierarchical mode
        // This enables subordinates to receive commands and send acknowledgments
        if let Some(ditto_backend) = backend.as_any().downcast_ref::<DittoBackend>() {
            if let Ok(ditto_store) = ditto_backend.get_ditto_store() {
                let reception_node_id = node_id.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_command_reception(reception_node_id.clone(), ditto_store).await
                    {
                        eprintln!("[{}] Command reception error: {}", reception_node_id, e);
                    }
                });
                println!("[{}] ✓ Command reception task spawned", node_id);
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
            // In hierarchical mode, nodes run aggregation simulation without ack test
            hierarchical_mode(&*backend, &node_id, &node_type, update_rate_ms).await
        }
        "flat_mesh" => {
            // Lab 3b: Flat P2P mesh with HIVE CRDT (all nodes at same tier)
            flat_mesh_mode(&*backend, &node_id, &node_type, update_rate_ms).await
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
async fn create_backend(
    backend_type: &str,
    node_id: &str,
    tcp_listen_port: Option<u16>,
) -> Result<Box<dyn DataSyncBackend>, Box<dyn std::error::Error>> {
    match backend_type {
        "ditto" => Ok(Box::new(DittoBackend::new())),
        #[cfg(feature = "automerge-backend")]
        "automerge" => {
            // Create AutomergeIrohBackend with persistence and transport
            // This enables hierarchical mode support (HierarchicalStorageCapable trait)
            let persistence_dir = PathBuf::from(format!("/tmp/hive_sim_{}", node_id));
            std::fs::create_dir_all(&persistence_dir)?;

            let store = Arc::new(
                AutomergeStore::open(&persistence_dir)
                    .map_err(|e| format!("Failed to open AutomergeStore: {}", e))?,
            );

            // Get formation ID from environment (used as seed prefix for deterministic keys)
            let formation_id =
                std::env::var("DITTO_APP_ID").unwrap_or_else(|_| "default-formation".to_string());

            // Create deterministic seed from formation ID and node ID (Issue #235)
            // This ensures EndpointIds are predictable for static peer configuration
            let seed = format!("{}/{}", formation_id, node_id);

            // Determine bind address for QUIC transport
            let bind_addr: std::net::SocketAddr = if let Some(port) = tcp_listen_port {
                format!("0.0.0.0:{}", port).parse()?
            } else {
                "0.0.0.0:0".parse()?
            };

            let transport = Arc::new(
                IrohTransport::from_seed_with_discovery_at_addr(&seed, bind_addr)
                    .await
                    .map_err(|e| format!("Failed to create IrohTransport: {}", e))?,
            );

            eprintln!(
                "[{}] Created Automerge transport with seed '{}', EndpointId: {}",
                node_id,
                seed,
                hex::encode(transport.endpoint_id().as_bytes())
            );

            Ok(Box::new(AutomergeIrohBackend::from_parts(store, transport)))
        }
        _ => Err(format!(
            "Unknown backend type: {}. Available: ditto{}",
            backend_type,
            if cfg!(feature = "automerge-backend") {
                ", automerge"
            } else {
                ""
            }
        )
        .into()),
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
        #[cfg(feature = "automerge-backend")]
        "automerge" => {
            // Automerge doesn't require app_id or shared_key
            BackendConfig {
                app_id: format!("automerge-{}", node_id),
                persistence_dir,
                shared_key: None,
                transport,
                extra: HashMap::new(),
            }
        }
        _ => return Err(format!("Unknown backend type: {}", backend_type).into()),
    };

    Ok(config)
}

/// Connect to Automerge peers using static configuration (Issue #235)
///
/// Parses TCP_CONNECT environment variable and establishes QUIC connections
/// to peers using their deterministically-derived EndpointIds.
///
/// # TCP_CONNECT Format
///
/// `peer_name|hostname:port,peer_name2|hostname2:port2,...`
///
/// Example: `node-1|clab-mesh-node-1:9000,node-2|clab-mesh-node-2:9000`
///
/// # EndpointId Derivation
///
/// EndpointIds are derived from seeds using the same algorithm as `IrohTransport::from_seed()`:
/// - Seed format: `{formation_id}/{peer_name}`
/// - Hash: SHA-256 with domain separator "hive-iroh-key-v1:"
/// - Result: ed25519 public key (EndpointId)
#[cfg(feature = "automerge-backend")]
async fn connect_to_automerge_peers(
    sync_engine: &Arc<dyn hive_protocol::sync::SyncEngine>,
    node_id: &str,
    tcp_connect_addr: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tcp_connect = match tcp_connect_addr {
        Some(addr) => addr,
        None => return Ok(()), // No peers to connect
    };

    if tcp_connect.is_empty() {
        return Ok(());
    }

    // Get formation ID for deriving peer EndpointIds
    let formation_id =
        std::env::var("DITTO_APP_ID").unwrap_or_else(|_| "default-formation".to_string());

    eprintln!(
        "[{}] Connecting to Automerge peers from TCP_CONNECT: {}",
        node_id, tcp_connect
    );

    // Parse: "peer_name|hostname:port,peer_name2|hostname2:port2,..."
    for peer_spec in tcp_connect.split(',') {
        let peer_spec = peer_spec.trim();
        if peer_spec.is_empty() {
            continue;
        }

        let parts: Vec<&str> = peer_spec.splitn(2, '|').collect();
        if parts.len() != 2 {
            eprintln!(
                "[{}] Invalid peer spec (expected 'name|address'): {}",
                node_id, peer_spec
            );
            continue;
        }

        let peer_name = parts[0];
        let peer_addr = parts[1];

        // Skip connecting to self
        if peer_name == node_id {
            continue;
        }

        // Derive EndpointId from peer seed
        let peer_seed = format!("{}/{}", formation_id, peer_name);
        let peer_endpoint_id = IrohTransport::endpoint_id_from_seed(&peer_seed);
        let peer_endpoint_hex = hex::encode(peer_endpoint_id.as_bytes());

        eprintln!(
            "[{}] Connecting to peer '{}' at {} (EndpointId: {}...)",
            node_id,
            peer_name,
            peer_addr,
            &peer_endpoint_hex[..16]
        );

        // Connect using the SyncEngine trait method
        match sync_engine
            .connect_to_peer(&peer_endpoint_hex, &[peer_addr.to_string()])
            .await
        {
            Ok(true) => {
                eprintln!("[{}] ✓ Connected to peer '{}'", node_id, peer_name);
            }
            Ok(false) => {
                eprintln!(
                    "[{}] → Waiting for peer '{}' to connect (tie-breaking)",
                    node_id, peer_name
                );
            }
            Err(e) => {
                eprintln!(
                    "[{}] ✗ Failed to connect to peer '{}': {}",
                    node_id, peer_name, e
                );
            }
        }
    }

    Ok(())
}

/// Event-driven hierarchical mode with capability reporting
///
/// This replaces time-based simulation with event-driven, capability-based aggregation:
/// - Soldiers: Send N updates with capabilities, wait for squad summary, exit
/// - Leaders: Perform N aggregation cycles, wait for upward summary, exit
///
/// Benefits:
/// - Deterministic completion (no race conditions)
/// - Capability-based aggregation (emergent behaviors)
/// - Clear causality chain through hierarchy
async fn hierarchical_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === EVENT-DRIVEN HIERARCHICAL MODE ===", node_id);

    // Determine role from environment
    let role = std::env::var("ROLE").unwrap_or_else(|_| "soldier".to_string());

    match role.as_str() {
        "soldier" => soldier_capability_mode(backend, node_id, node_type, update_rate_ms).await,
        "squad_leader" | "platoon_leader" | "company_commander" => {
            // Leaders run aggregation loops spawned earlier, just publish status updates
            leader_status_mode(backend, node_id, node_type, update_rate_ms, &role).await
        }
        _ => {
            println!(
                "[{}] Unknown role: {}, defaulting to soldier mode",
                node_id, role
            );
            soldier_capability_mode(backend, node_id, node_type, update_rate_ms).await
        }
    }
}

/// Soldier capability mode: Send N updates with capabilities, wait for squad summary
async fn soldier_capability_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] Running as SOLDIER with capability reporting", node_id);

    // Mission parameters
    const TARGET_UPDATES: u64 = 10;
    const SAFETY_TIMEOUT_SECS: u64 = 60;

    let update_interval = Duration::from_millis(update_rate_ms);
    let mut message_number: u64 = 0;
    let doc_id = format!("sim_doc_{}", node_id);
    let start_time = Instant::now();

    // Generate soldier capabilities (simple test data for now)
    let capabilities = generate_soldier_capabilities(node_id);
    println!(
        "[{}] Generated {} capabilities:",
        node_id,
        capabilities.len()
    );
    for cap in &capabilities {
        println!("[{}]   - {} (confidence: {:.2})", node_id, cap, 0.85);
    }

    // Send TARGET_UPDATES status updates with capabilities
    while message_number < TARGET_UPDATES {
        message_number += 1;
        let timestamp_us = now_micros();

        // Create update message with capabilities
        let message_content = format!(
            "Status update #{} from {} - Capabilities: {}",
            message_number,
            node_id,
            capabilities.join(", ")
        );

        // Create document fields including capabilities
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
        fields.insert(
            "capabilities".to_string(),
            Value::Array(
                capabilities
                    .iter()
                    .map(|c| Value::String(c.clone()))
                    .collect(),
            ),
        );
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields.clone());

        // Insert/update document with latency tracking
        let crdt_start = Instant::now();
        backend.document_store().upsert("sim_poc", document).await?;
        let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;

        let message_json = serde_json::to_string(&fields)?;
        let message_size_bytes = message_json.len();

        // Log CRDT upsert latency for Lab 4 analysis
        println!(
            "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"soldier\",\"message_number\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
            node_id, message_number, crdt_latency_ms, timestamp_us
        );

        println!(
            "[{}] ✓ Status update #{}/{} sent ({} bytes, {} capabilities)",
            node_id,
            message_number,
            TARGET_UPDATES,
            message_size_bytes,
            capabilities.len()
        );

        log_metrics(&MetricsEvent::MessageSent {
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            message_number,
            message_size_bytes,
            timestamp_us,
        });

        // Check for safety timeout
        if start_time.elapsed().as_secs() > SAFETY_TIMEOUT_SECS {
            println!("[{}] ⚠ Safety timeout reached, exiting", node_id);
            return Ok(());
        }

        sleep(update_interval).await;
    }

    println!("[{}] ✓ All {} status updates sent", node_id, TARGET_UPDATES);

    // Spawn observer for document reception tracking (Lab 4 comprehensive latency)
    let observer_backend: Arc<Box<dyn DataSyncBackend>> =
        if let Some(ditto_backend) = backend.as_any().downcast_ref::<DittoBackend>() {
            Arc::new(Box::new(ditto_backend.clone()))
        } else {
            eprintln!(
                "[{}] WARNING: Could not clone backend for observation",
                node_id
            );
            return Ok(());
        };
    let observer_node_id = node_id.to_string();
    let own_doc_id = doc_id.clone();
    tokio::spawn(async move {
        println!(
            "METRICS: [{}] Starting document reception observer...",
            observer_node_id
        );

        let query = Query::All;
        if let Ok(mut change_stream) = observer_backend
            .as_ref()
            .document_store()
            .observe("sim_poc", &query)
        {
            loop {
                let event_result =
                    tokio::time::timeout(Duration::from_millis(100), change_stream.receiver.recv())
                        .await;

                match event_result {
                    Ok(Some(change_event)) => {
                        if let ChangeEvent::Updated { document, .. } = change_event {
                            let received_at_us = now_micros();
                            if let Some(received_doc_id) = &document.id {
                                // Track peer soldier documents (lateral propagation)
                                if received_doc_id.starts_with("sim_doc_")
                                    && received_doc_id != &own_doc_id
                                {
                                    if let Some(created_at_us) = document
                                        .get("timestamp_us")
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u128)
                                    {
                                        let latency_us =
                                            received_at_us.saturating_sub(created_at_us);
                                        let latency_ms = latency_us as f64 / 1000.0;

                                        log_metrics(&MetricsEvent::DocumentReceived {
                                            node_id: observer_node_id.clone(),
                                            doc_id: received_doc_id.clone(),
                                            created_at_us,
                                            last_modified_us: created_at_us,
                                            received_at_us,
                                            latency_us,
                                            latency_ms,
                                            version: 1,
                                            is_first_reception: false,
                                            latency_type: "peer_soldier".to_string(),
                                        });
                                    }
                                }

                                // Track squad summary from leader (downward propagation)
                                if received_doc_id.contains("-summary") {
                                    if let Some(created_at_us) = document
                                        .get("created_at_us")
                                        .or_else(|| document.get("timestamp_us"))
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u128)
                                    {
                                        let latency_us =
                                            received_at_us.saturating_sub(created_at_us);
                                        let latency_ms = latency_us as f64 / 1000.0;

                                        log_metrics(&MetricsEvent::DocumentReceived {
                                            node_id: observer_node_id.clone(),
                                            doc_id: received_doc_id.clone(),
                                            created_at_us,
                                            last_modified_us: created_at_us,
                                            received_at_us,
                                            latency_us,
                                            latency_ms,
                                            version: 1,
                                            is_first_reception: false,
                                            latency_type: "squad_summary_downward".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => break,  // Channel closed
                    Err(_) => continue, // Timeout, continue loop
                }
            }
        }
    });

    // Now wait to observe squad summary (event-driven exit)
    println!("[{}] Waiting to observe squad summary...", node_id);
    let squad_id = std::env::var("SQUAD_ID").unwrap_or_else(|_| "squad-unknown".to_string());
    let summary_doc_id = format!("{}-summary", squad_id);

    let wait_start = Instant::now();
    const SQUAD_SUMMARY_TIMEOUT_SECS: u64 = 30;

    loop {
        // Try to fetch squad summary
        if let Ok(Some(_summary)) = backend
            .document_store()
            .get("sim_poc", &summary_doc_id)
            .await
        {
            println!("[{}] ✓ Squad summary observed! Mission complete.", node_id);
            return Ok(());
        }

        // Check timeout
        if wait_start.elapsed().as_secs() > SQUAD_SUMMARY_TIMEOUT_SECS {
            println!(
                "[{}] ⚠ Squad summary not observed after {}s, exiting anyway",
                node_id, SQUAD_SUMMARY_TIMEOUT_SECS
            );
            return Ok(());
        }

        sleep(Duration::from_secs(2)).await;
    }
}

/// Leader status mode: Leaders run aggregation loops, this just provides heartbeat
async fn leader_status_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Running as {} with status heartbeat",
        node_id,
        role.to_uppercase()
    );

    // Leaders run for longer as they aggregate
    let test_duration = match role {
        "squad_leader" => Duration::from_secs(90),
        "platoon_leader" => Duration::from_secs(120),
        "company_commander" => Duration::from_secs(150),
        _ => Duration::from_secs(90),
    };

    let update_interval = Duration::from_millis(update_rate_ms);
    let mut message_number: u64 = 0;
    let doc_id = format!("sim_doc_{}", node_id);
    let start_time = Instant::now();

    while start_time.elapsed() < test_duration {
        message_number += 1;
        let timestamp_us = now_micros();

        let message_content = format!(
            "Leader heartbeat #{} from {} ({})",
            message_number, node_id, role
        );

        let mut fields = HashMap::new();
        fields.insert("message".to_string(), Value::String(message_content));
        fields.insert("timestamp_us".to_string(), serde_json::json!(timestamp_us));
        fields.insert("created_by".to_string(), Value::String(node_id.to_string()));
        fields.insert(
            "node_type".to_string(),
            Value::String(node_type.to_string()),
        );
        fields.insert("role".to_string(), Value::String(role.to_string()));
        fields.insert(
            "message_number".to_string(),
            serde_json::json!(message_number),
        );
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields);
        backend.document_store().upsert("sim_poc", document).await?;

        if message_number % 10 == 0 {
            println!("[{}] ✓ Leader heartbeat #{}", node_id, message_number);
        }

        sleep(update_interval).await;
    }

    println!(
        "[{}] {} simulation complete after {:?}",
        node_id,
        role,
        start_time.elapsed()
    );
    Ok(())
}

/// Generate simple test capabilities for a soldier node
fn generate_soldier_capabilities(node_id: &str) -> Vec<String> {
    // Simple capability generation based on node ID hash
    // In production, this would be based on actual platform specs
    let hash = node_id
        .chars()
        .fold(0u32, |acc, c| acc.wrapping_add(c as u32));

    let mut caps = vec![];

    // Every soldier has basic capabilities
    caps.push("communications:radio".to_string());
    caps.push("mobility:ground".to_string());

    // Add varied capabilities based on hash
    if hash % 3 == 0 {
        caps.push("sensor:thermal".to_string());
    }
    if hash % 3 == 1 {
        caps.push("sensor:optical".to_string());
    }
    if hash % 3 == 2 {
        caps.push("sensor:acoustic".to_string());
    }

    if hash % 4 == 0 {
        caps.push("compute:edge".to_string());
    }

    if hash % 5 == 0 {
        caps.push("payload:supply".to_string());
    }

    caps
}

/// Lab 3b: Flat P2P mesh mode with HIVE CRDT
///
/// All nodes operate at the same hierarchy level (Squad) using DynamicHierarchyStrategy
/// for leader election and CRDT for state synchronization.
///
/// This mode tests pure CRDT overhead in a flat mesh topology for comparison with Lab 3 (raw TCP).
async fn flat_mesh_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === FLAT MESH MODE (Lab 3b) ===", node_id);
    println!("[{}] Node type: {}", node_id, node_type);
    println!("[{}] Update rate: {}ms", node_id, update_rate_ms);

    // Create node profile (all nodes at Squad level)
    let profile = NodeProfile {
        mobility: NodeMobility::SemiMobile,
        resources: NodeResources {
            cpu_cores: 4,
            memory_mb: 2048,
            bandwidth_mbps: 100,
            cpu_usage_percent: 30,
            memory_usage_percent: 40,
            battery_percent: Some(80),
        },
        can_parent: true,
        prefer_leaf: false,
        parent_priority: 128,
    };

    // Create flat mesh coordinator
    let coordinator = Arc::new(FlatMeshCoordinator::new(
        node_id.to_string(),
        profile,
        None, // Use default election config
    ));

    println!(
        "[{}] Initialized as flat mesh peer at level: {:?}",
        node_id,
        coordinator.hierarchy_level()
    );

    // Collection for node states (all peers sync here)
    let collection_name = "node_states";

    // Create or update this node's state document
    let update_interval = Duration::from_millis(update_rate_ms);
    let mut sequence = 0u64;
    const TARGET_UPDATES: u64 = 20;

    for _ in 0..TARGET_UPDATES {
        sequence += 1;

        // Create state update document
        let timestamp_us = now_micros();
        let doc_id = format!("flat_mesh_state_{}", node_id);

        let mut fields = HashMap::new();
        fields.insert("node_id".to_string(), Value::String(node_id.to_string()));
        fields.insert(
            "node_type".to_string(),
            Value::String(node_type.to_string()),
        );
        fields.insert("timestamp_us".to_string(), serde_json::json!(timestamp_us));
        fields.insert("sequence_number".to_string(), serde_json::json!(sequence));
        fields.insert(
            "status".to_string(),
            Value::String("operational".to_string()),
        );
        fields.insert(
            "squad_id".to_string(),
            Value::String("flat-mesh".to_string()),
        );
        fields.insert("battery_percent".to_string(), serde_json::json!(80));
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields);

        // Upsert document to CRDT with timing
        let upsert_start = std::time::Instant::now();
        backend
            .document_store()
            .upsert(collection_name, document)
            .await?;
        let upsert_latency_ms = upsert_start.elapsed().as_secs_f64() * 1000.0;

        // Log metrics
        log_metrics(&MetricsEvent::DocumentInserted {
            node_id: node_id.to_string(),
            doc_id: doc_id.clone(),
            timestamp_us: now_micros(),
        });

        println!(
            "[{}] Published state update {}/{} to flat mesh, CRDT_latency: {:.3}ms",
            node_id, sequence, TARGET_UPDATES, upsert_latency_ms
        );

        // Check current role
        let role = coordinator.current_role().await;
        if sequence % 5 == 0 {
            println!(
                "[{}] Current role: {:?}, {} peers",
                node_id,
                role,
                coordinator.peer_count().await
            );
        }

        sleep(update_interval).await;
    }

    println!(
        "[{}] Completed {} updates, keeping process alive for CRDT sync monitoring...",
        node_id, TARGET_UPDATES
    );

    // Keep running to allow CRDT synchronization to continue
    // The CRDT backend handles peer-to-peer sync automatically
    let monitor_duration = Duration::from_secs(60);
    println!(
        "[{}] Monitoring flat mesh CRDT sync for {}s...",
        node_id,
        monitor_duration.as_secs()
    );

    sleep(monitor_duration).await;

    println!(
        "[{}] Flat mesh mode complete - process staying alive for continued sync",
        node_id
    );

    // Keep process running indefinitely for long-running tests
    loop {
        sleep(Duration::from_secs(300)).await;
    }
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

    // Note: Storage writes "last_update_us", so check that first
    let last_modified_us = if let Some(ts) = doc.get("last_update_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else if let Some(ts) = doc.get("last_modified_us") {
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
        if created_at_us > 0 && !test_doc_timestamps.contains(&created_at_us) {
            test_doc_timestamps.insert(created_at_us);

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
                is_first_reception: true, // Inside conditional, so always first reception
                latency_type: latency_type.clone(),
            });
        }
    }
    // Check if this is a platoon summary document
    else if doc_id.starts_with("platoon-")
        && doc_id.ends_with("-summary")
        && created_at_us > 0
        && !test_doc_timestamps.contains(&created_at_us)
    {
        test_doc_timestamps.insert(created_at_us);

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
            is_first_reception: true, // Inside conditional, so always first reception
            latency_type: latency_type.clone(),
        });
    }

    Ok(())
}
