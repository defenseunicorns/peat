//! Peat Protocol Network Simulation Node
//!
//! Reference implementation for simulating and testing the Peat protocol.
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
//! `--node-id <id>`         Node identifier (e.g., "node1", "squad-1A-leader")
//! `--mode <mode>`          "writer" (creates documents) or "reader" (waits for documents)
//! `--backend <type>`       Sync backend to use (default: "ditto")
//! `--tcp-listen <port>`    Optional: Listen for TCP connections on this port
//! `--tcp-connect <addr>`   Optional: Connect to TCP peer at this address
//! `--node-type <type>`     Node type for authorization (e.g., "soldier", "squad_leader")
//! `--update-rate-ms <ms>`  Update rate in milliseconds (default: 5000)
//! --peat-filter          Enable Peat capability-based filtering
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
//! **Peat Filtering:**
//! - PEAT_FILTER_ENABLED: Set to "true" or "1" to enable differential updates
//!
//! # Exit Codes
//!
//! 0: Success (document synced, all operations completed)
//! 1: Failure (timeout, error, or document not received)

mod metrics;
mod simulation;
mod utils;

use metrics::{init_metrics_file, log_metrics, MetricsEvent};
use utils::time::{extract_timestamp_us, now_micros};

use peat_protocol::credentials::PeatCredentials;
#[cfg(feature = "automerge-backend")]
use peat_protocol::sync::automerge::AutomergeIrohBackend;
use peat_protocol::sync::ditto::DittoBackend;
use peat_protocol::sync::{
    BackendConfig, ChangeEvent, ChangeStream, DataSyncBackend, Document, Query, TransportConfig,
    Value,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

// Mode 4: Hierarchical aggregation imports
use peat_protocol::hierarchy::{HierarchicalAggregator, StateAggregator};
use peat_protocol::models::{NodeConfig, NodeState};

// Phase 3: Command dissemination imports
use peat_protocol::command::{CommandCoordinator, CommandStorage};
use peat_protocol::storage::DittoCommandStorage;

// AutomergeIroh backend components
#[cfg(feature = "automerge-backend")]
use peat_protocol::network::IrohTransport;
#[cfg(feature = "automerge-backend")]
use peat_protocol::storage::{AutomergeStore, StorageBackend};
use peat_schema::command::v1::{
    command_target::Scope, AckStatus, CommandAcknowledgment, CommandTarget, HierarchicalCommand,
};
use peat_schema::common::v1::Timestamp;

// Lab 3b: Flat mesh coordination with CRDT
use peat_mesh::beacon::{NodeMobility, NodeProfile, NodeResources};
use peat_mesh::FlatMeshCoordinator;
#[cfg(feature = "automerge-backend")]
use rand::Rng;

/// Test document structure
#[allow(dead_code)]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestDoc {
    id: String,
    message: String,
    timestamp: u64, // Unix timestamp in microseconds
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
    ditto_store: Arc<peat_protocol::storage::DittoStore>,
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
    ditto_store: Arc<peat_protocol::storage::DittoStore>,
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
/// Event-driven aggregation triggered by member NodeState updates via P2P mesh.
/// Squad leaders observe member states arriving from soldiers and aggregate them
/// into SquadSummary documents. Uses debouncing to avoid excessive aggregation.
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

    // Channel for observer to signal member state updates (triggers aggregation)
    let (update_tx, mut update_rx) = tokio::sync::mpsc::channel::<String>(100);

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
                        // Helper to process a single document for latency tracking
                        let process_doc = |document: &Document, is_first: bool| {
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
                                                is_first_reception: is_first,
                                                latency_type: "soldier_to_squad_leader".to_string(),
                                                source_tier: Some("soldier".to_string()),
                                                dest_tier: Some("squad_leader".to_string()),
                                                is_warmup: None,
                                            });

                                            // Signal aggregation loop that a member updated
                                            let _ = update_tx.try_send(doc_id.clone());
                                        }
                                    }
                                }
                            }
                        };

                        match change_event {
                            ChangeEvent::Initial { documents } => {
                                for doc in documents {
                                    process_doc(&doc, true);
                                }
                            }
                            ChangeEvent::Updated { document, .. } => {
                                process_doc(&document, false);
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => break,  // Channel closed
                    Err(_) => continue, // Timeout, continue loop
                }
            }
        }
    });

    // Helper closure for aggregation — reads real soldier data from CRDT store
    let do_aggregation = |coordinator: Arc<HierarchicalAggregator>,
                          squad_id: String,
                          node_id: String,
                          member_ids: Vec<String>,
                          backend: Arc<Box<dyn DataSyncBackend>>| async move {
        let seed = simulation::PositionSeed::from_env();
        let mut member_states = Vec::new();

        for member_id in &member_ids {
            let doc_id = format!("sim_doc_{}", member_id);
            let platform_type = simulation::assign_platform_type(member_id);
            let capabilities =
                simulation::generate_capabilities(member_id, platform_type, "soldier");

            // Try to read the actual soldier document from CRDT store
            let (lat, lon, fuel, health) =
                match backend.document_store().get("sim_poc", &doc_id).await {
                    Ok(Some(doc)) => {
                        let lat = doc.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let lon = doc.get("lon").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let fuel = doc
                            .get("fuel_minutes")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(100) as u32;
                        let health = doc.get("health").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                        (lat, lon, fuel, health)
                    }
                    _ => {
                        // Startup race: soldier hasn't published yet — use seeded defaults
                        let pos = seed.initial_position(member_id);
                        (pos.0, pos.1, 100u32, 1i32)
                    }
                };

            // NOTE: Platform markers are generated client-side in ATAK from cell hierarchy.
            // No CRDT platform doc publishing needed (causes OOM from revision accumulation).

            let config = NodeConfig {
                id: member_id.clone(),
                platform_type: platform_type.as_str().to_string(),
                capabilities,
                comm_range_m: 1000.0,
                max_speed_mps: platform_type.speed_mps() as f32,
                operator_binding: None,
                created_at: None,
            };

            let state = NodeState {
                position: Some(peat_schema::common::v1::Position {
                    latitude: lat,
                    longitude: lon,
                    altitude: platform_type.default_altitude(),
                }),
                fuel_minutes: fuel,
                health,
                phase: peat_schema::node::v1::Phase::Hierarchy.into(),
                cell_id: Some(squad_id.clone()),
                zone_id: None,
                timestamp: None,
            };

            member_states.push((config, state));
        }

        if member_states.is_empty() {
            return;
        }

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
                        use peat_protocol::hierarchy::deltas::SquadDelta;
                        let delta = SquadDelta::from_summary(&squad_summary, timestamp_us as u64);

                        let crdt_start = Instant::now();
                        if let Err(e) = coordinator.update_squad_summary(&squad_id, delta).await {
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
                    input_bytes: None,
                    output_bytes: None,
                    bytes_saved: None,
                    reduction_ratio: Some(squad_summary.member_count as f64),
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
    };

    // Debounce interval: aggregate at most once per this duration
    // This prevents excessive aggregation when multiple members update simultaneously
    let debounce_interval = Duration::from_millis(100);
    let mut pending_aggregation = false;

    // Initial aggregation on startup
    do_aggregation(
        Arc::clone(&coordinator),
        squad_id.clone(),
        node_id.clone(),
        member_ids.clone(),
        Arc::clone(&backend),
    )
    .await;
    let mut last_aggregation = Instant::now();

    // EVENT-DRIVEN: Wait for member state updates and aggregate with debouncing
    loop {
        // Wait for update signal with timeout (for periodic aggregation fallback)
        let timeout_duration = if pending_aggregation {
            // If we have a pending aggregation, wait only until debounce interval expires
            debounce_interval.saturating_sub(last_aggregation.elapsed())
        } else {
            // Otherwise wait up to 5 seconds for an update (matches soldier update rate)
            Duration::from_secs(5)
        };

        match tokio::time::timeout(timeout_duration, update_rx.recv()).await {
            Ok(Some(_member_id)) => {
                // Member state updated - mark aggregation as pending
                pending_aggregation = true;

                // Check if we can aggregate now (debounce)
                if last_aggregation.elapsed() >= debounce_interval {
                    do_aggregation(
                        Arc::clone(&coordinator),
                        squad_id.clone(),
                        node_id.clone(),
                        member_ids.clone(),
                        Arc::clone(&backend),
                    )
                    .await;
                    last_aggregation = Instant::now();
                    pending_aggregation = false;

                    // Drain any queued updates (they're now stale)
                    while update_rx.try_recv().is_ok() {}
                }
            }
            Ok(None) => {
                // Channel closed, observer terminated
                println!(
                    "[{}] Observer channel closed, stopping aggregation",
                    node_id
                );
                break;
            }
            Err(_) => {
                // Timeout - perform pending aggregation if any, or periodic refresh
                if pending_aggregation || last_aggregation.elapsed() >= Duration::from_secs(5) {
                    do_aggregation(
                        Arc::clone(&coordinator),
                        squad_id.clone(),
                        node_id.clone(),
                        member_ids.clone(),
                        Arc::clone(&backend),
                    )
                    .await;
                    last_aggregation = Instant::now();
                    pending_aggregation = false;
                }
            }
        }
    }

    Ok(())
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

    // Get squad IDs from environment variable if available, or derive from platoon ID
    // SQUAD_IDS format: comma-separated list of squad IDs
    // Fallback patterns:
    //   - company-X-platoon-Y → [company-X-platoon-Y-squad-1, company-X-platoon-Y-squad-2, company-X-platoon-Y-squad-3]
    //   - platoon-1 → [squad-1A, squad-1B]
    let squad_ids: Vec<String> = if let Ok(squad_ids_str) = std::env::var("SQUAD_IDS") {
        // Use environment variable if provided
        squad_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else if platoon_id.contains("-platoon-") {
        // Hierarchical naming: company-X-platoon-Y → squad IDs with same prefix
        // Default to 3 squads per platoon for hierarchical topology
        let num_squads = std::env::var("SQUADS_PER_PLATOON")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        (1..=num_squads)
            .map(|i| format!("{}-squad-{}", platoon_id, i))
            .collect()
    } else if let Some(platoon_num) = platoon_id.strip_prefix("platoon-") {
        // Legacy flat naming: platoon-1 → [squad-1A, squad-1B]
        vec![
            format!("squad-{}A", platoon_num),
            format!("squad-{}B", platoon_num),
        ]
    } else {
        // Fallback for test cases
        vec!["squad-alpha".to_string(), "squad-bravo".to_string()]
    };
    println!(
        "[{}] Using squad IDs for aggregation: {:?}",
        node_id, squad_ids
    );

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
        let mut missing_squads = Vec::new();

        for squad_id in &squad_ids {
            match coordinator.get_squad_summary(squad_id).await {
                Ok(Some(summary)) => {
                    squad_summaries.push(summary);
                }
                Ok(None) => {
                    missing_squads.push(squad_id.clone());
                }
                Err(e) => {
                    eprintln!("[{}] Error fetching squad {}: {}", node_id, squad_id, e);
                    missing_squads.push(squad_id.clone());
                }
            }
        }

        // Log aggregation readiness
        if !missing_squads.is_empty() {
            // Only log occasionally to avoid spam (every 10 seconds based on timestamp)
            let ts_secs = now_micros() / 1_000_000;
            if ts_secs % 10 == 0 {
                println!(
                    "[{}] Aggregation pending: {}/{} squads ready, missing: {:?}",
                    node_id,
                    squad_summaries.len(),
                    squad_ids.len(),
                    missing_squads
                );
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
                            use peat_protocol::hierarchy::deltas::PlatoonDelta;
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
                        input_bytes: None,
                        output_bytes: None,
                        bytes_saved: None,
                        reduction_ratio: Some(platoon_summary.squad_count as f64),
                    });

                    // Log aggregation efficiency for Lab 4 analysis
                    // Reduction ratio = input_docs / output_docs = squad_count / 1
                    let reduction_ratio = platoon_summary.squad_count as f64;
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

    // Track last aggregation time for periodic fallback (Issue #493 - recovery time fix)
    let mut last_aggregation = Instant::now();

    // EVENT-DRIVEN: Listen for squad summary changes and aggregate IMMEDIATELY
    // With periodic fallback every 5 seconds if change stream events fail
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
                                // Match squad summaries from both backends:
                                // - Ditto: "squad-summary-{squad_id}" (starts with "squad-")
                                // - Automerge: "{squad_id}" after prefix strip (e.g., "company-ALPHA-platoon-1-squad-1")
                                if doc_id.starts_with("squad-") || doc_id.contains("-squad-") {
                                    // Extract timestamps with proper delta sync semantics
                                    // Fallback to doc.updated_at for Automerge (which doesn't store timestamps in fields)
                                    let doc_updated_at_us = doc
                                        .updated_at
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_micros();

                                    let created_at_us = if let Some(ts) = doc.get("created_at_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if let Some(ts) = doc.get("timestamp_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else {
                                        doc_updated_at_us // Automerge fallback
                                    };

                                    // Note: Storage writes "last_update_us", so check that first
                                    // Fallback to doc.updated_at for Automerge
                                    let last_modified_us =
                                        if let Some(ts) = doc.get("last_update_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else if let Some(ts) = doc.get("last_modified_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else if doc_updated_at_us > 0 {
                                            doc_updated_at_us // Automerge fallback
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
                                            latency_type: "squad_to_platoon_leader".to_string(),
                                            source_tier: Some("squad_leader".to_string()),
                                            dest_tier: Some("platoon_leader".to_string()),
                                            is_warmup: None,
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
                            // Match squad summaries from both backends:
                            // - Ditto: "squad-summary-{squad_id}" (starts with "squad-")
                            // - Automerge: "{squad_id}" after prefix strip (e.g., "company-ALPHA-platoon-1-squad-1")
                            let matches =
                                doc_id.starts_with("squad-") || doc_id.contains("-squad-");
                            if matches {
                                // Extract timestamps with proper delta sync semantics
                                // Fallback to document.updated_at for Automerge (which doesn't store timestamps in fields)
                                let doc_updated_at_us = document
                                    .updated_at
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_micros();

                                // Squad summaries use "aggregated_at" for their timestamp (protobuf format)
                                let created_at_us = if let Some(ts) = document.get("created_at_us")
                                {
                                    extract_timestamp_us(ts)
                                } else if let Some(ts) = document.get("timestamp_us") {
                                    extract_timestamp_us(ts)
                                } else if let Some(ts) = document.get("aggregated_at") {
                                    extract_timestamp_us(ts)
                                } else {
                                    doc_updated_at_us // Automerge fallback
                                };

                                // Note: Storage writes "last_update_us", so check that first
                                // Squad summaries use "aggregated_at" as their modification time
                                // Fallback to document.updated_at for Automerge
                                let last_modified_us =
                                    if let Some(ts) = document.get("last_update_us") {
                                        extract_timestamp_us(ts)
                                    } else if let Some(ts) = document.get("last_modified_us") {
                                        extract_timestamp_us(ts)
                                    } else if let Some(ts) = document.get("aggregated_at") {
                                        extract_timestamp_us(ts)
                                    } else if doc_updated_at_us > 0 {
                                        doc_updated_at_us // Automerge fallback
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
                                        latency_type: "squad_to_platoon_leader".to_string(),
                                        source_tier: Some("squad_leader".to_string()),
                                        dest_tier: Some("platoon_leader".to_string()),
                                        is_warmup: None,
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
                // Timeout waiting for event - perform periodic aggregation if needed
                // This ensures aggregation happens even when change stream events fail
                // (Issue #493 - fix 32s recovery time due to deserialization errors)
                if last_aggregation.elapsed() >= Duration::from_secs(5) {
                    do_aggregation(
                        Arc::clone(&coordinator),
                        platoon_id.clone(),
                        node_id.clone(),
                        squad_ids.clone(),
                    )
                    .await;
                    last_aggregation = Instant::now();
                }
            }
        }
    }
}

/// Company commander aggregation loop (Mode 4)
///
/// Event-driven aggregation triggered by platoon summary updates via P2P mesh.
/// Company commanders observe platoon summaries arriving from platoon leaders and aggregate
/// them into CompanySummary documents.
async fn company_commander_aggregation_loop(
    mut change_stream: ChangeStream,
    coordinator: Arc<HierarchicalAggregator>,
    company_id: String,
    node_id: String,
    cells_collection: Option<Arc<dyn peat_protocol::storage::Collection>>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Started company commander aggregation for {}",
        node_id, company_id
    );
    println!(
        "[{}] Observing platoon summary change stream for P2P latency measurement",
        node_id
    );

    // Get platoon IDs from environment variable if available, or derive from company ID
    // PLATOON_IDS format: comma-separated list of platoon IDs
    let platoon_ids: Vec<String> = if let Ok(platoon_ids_str) = std::env::var("PLATOON_IDS") {
        platoon_ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else if company_id.contains("-company-") || company_id.starts_with("company-") {
        // Hierarchical naming: company-X → platoon IDs with same prefix
        // Default to 4 platoons per company for hierarchical topology
        let num_platoons = std::env::var("PLATOONS_PER_COMPANY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4);
        (1..=num_platoons)
            .map(|i| format!("{}-platoon-{}", company_id, i))
            .collect()
    } else {
        // Fallback for test cases
        vec!["platoon-1".to_string(), "platoon-2".to_string()]
    };
    println!(
        "[{}] Using platoon IDs for aggregation: {:?}",
        node_id, platoon_ids
    );

    // Track unique document receptions by (doc_id, last_modified_us) for metrics deduplication
    let mut seen_doc_updates: HashSet<(String, u128)> = HashSet::new();

    // Helper function to perform aggregation (called when platoon summaries change)
    let do_aggregation =
        |coordinator: Arc<HierarchicalAggregator>,
         company_id: String,
         node_id: String,
         platoon_ids: Vec<String>,
         cells_coll: Option<Arc<dyn peat_protocol::storage::Collection>>| async move {
            // Collect latest platoon summaries
            let mut platoon_summaries = Vec::new();
            let mut missing_platoons = Vec::new();

            for platoon_id in &platoon_ids {
                match coordinator.get_platoon_summary(platoon_id).await {
                    Ok(Some(summary)) => {
                        platoon_summaries.push(summary);
                    }
                    Ok(None) => {
                        missing_platoons.push(platoon_id.clone());
                    }
                    Err(e) => {
                        eprintln!("[{}] Error fetching platoon {}: {}", node_id, platoon_id, e);
                        missing_platoons.push(platoon_id.clone());
                    }
                }
            }

            // Log aggregation readiness
            if !missing_platoons.is_empty() {
                // Only log occasionally to avoid spam (every 10 seconds based on timestamp)
                let ts_secs = now_micros() / 1_000_000;
                if ts_secs % 10 == 0 {
                    println!(
                        "[{}] Company aggregation: {}/{} platoons ready, missing: {:?}",
                        node_id,
                        platoon_summaries.len(),
                        platoon_ids.len(),
                        missing_platoons
                    );
                }
            }

            // Aggregate when we have at least one platoon summary
            // Note: In distributed P2P systems, we may not always have all platoons synced
            // Relaxed from: platoon_summaries.len() == platoon_ids.len()
            if !platoon_summaries.is_empty() {
                // Build hierarchy breakdown for ATAK cell detail (before aggregation consumes summaries)
                let seed = simulation::PositionSeed::from_env();
                let squads_per_platoon: usize = 3;
                let soldiers_per_squad: usize = 7;
                let hierarchy_json: Vec<serde_json::Value> = platoon_summaries
                    .iter()
                    .map(|ps| {
                        let plt_caps: Vec<String> = ps
                            .aggregated_capabilities
                            .iter()
                            .map(|c| c.name.clone())
                            .filter(|n| !n.is_empty())
                            .collect();
                        let plt_centroid = ps.position_centroid.as_ref();
                        serde_json::json!({
                            "id": ps.platoon_id,
                            "type": "platoon",
                            "squad_count": ps.squad_count,
                            "member_count": ps.total_member_count,
                            "readiness": format!("{:.0}%", ps.readiness_score * 100.0),
                            "avg_fuel": format!("{:.0} min", ps.avg_fuel_minutes),
                            "worst_health": match ps.worst_health {
                                1 => "Nominal",
                                2 => "Degraded",
                                3 => "Critical",
                                _ => "Unknown",
                            },
                            "capabilities": plt_caps,
                            "center_lat": plt_centroid.map(|p| p.latitude).unwrap_or(0.0),
                            "center_lon": plt_centroid.map(|p| p.longitude).unwrap_or(0.0),
                        })
                    })
                    .collect();
                // Build individual platform list embedded in the cell JSON.
                // This avoids the multi-hop sync issue with the platforms collection.
                // Positions are generated from the deterministic layout.
                let mut embedded_platforms: Vec<serde_json::Value> = Vec::new();
                let timestamp_ms = now_micros() / 1000;
                for ps in &platoon_summaries {
                    // Platoon leader
                    let plt_leader = format!("{}-leader", ps.platoon_id);
                    let (lat, lon) = seed.initial_position(&plt_leader);
                    embedded_platforms.push(serde_json::json!({
                        "id": &plt_leader, "platform_type": "platoon_leader", "name": &plt_leader,
                        "status": "ACTIVE", "lat": lat, "lon": lon, "readiness": 1.0,
                        "capabilities": ["Tactical Radio", "C2 Edge Compute"],
                        "cell_id": &ps.platoon_id, "last_heartbeat": timestamp_ms,
                    }));
                    for s in 1..=squads_per_platoon {
                        let squad_id = format!("{}-squad-{}", ps.platoon_id, s);
                        // Squad leader
                        let sq_leader = format!("{}-leader", squad_id);
                        let (lat, lon) = seed.initial_position(&sq_leader);
                        embedded_platforms.push(serde_json::json!({
                            "id": &sq_leader, "platform_type": "squad_leader", "name": &sq_leader,
                            "status": "ACTIVE", "lat": lat, "lon": lon, "readiness": 1.0,
                            "capabilities": ["Tactical Radio"],
                            "cell_id": &squad_id, "last_heartbeat": timestamp_ms,
                        }));
                        for m in 1..=soldiers_per_squad {
                            let member_id = format!("{}-soldier-{}", squad_id, m);
                            let platform_type = simulation::assign_platform_type(&member_id);
                            let (lat, lon) = seed.initial_position(&member_id);
                            let capabilities = simulation::generate_capabilities(
                                &member_id,
                                platform_type,
                                "soldier",
                            );
                            let cap_names = simulation::capability_names(&capabilities);
                            embedded_platforms.push(serde_json::json!({
                                "id": &member_id, "platform_type": platform_type.as_str(),
                                "name": &member_id, "status": "ACTIVE",
                                "lat": lat, "lon": lon, "readiness": 1.0,
                                "capabilities": cap_names,
                                "cell_id": &squad_id, "last_heartbeat": timestamp_ms,
                            }));
                        }
                    }
                }
                // Log aggregation start
                log_metrics(&MetricsEvent::AggregationStarted {
                    node_id: node_id.to_string(),
                    tier: "company".to_string(),
                    input_doc_type: "PlatoonSummary".to_string(),
                    input_count: platoon_summaries.len(),
                    timestamp_us: now_micros(),
                });

                let aggregation_start_time = now_micros();
                let timestamp_us = aggregation_start_time;

                // Aggregate into CompanySummary
                match StateAggregator::aggregate_company(&company_id, &node_id, platoon_summaries) {
                    Ok(company_summary) => {
                        let processing_time_us = now_micros() - aggregation_start_time;
                        // Check if company summary document exists (create-once pattern)
                        match coordinator.get_company_summary(&company_id).await {
                            Ok(None) => {
                                // First time - create document with latency tracking
                                let crdt_start = Instant::now();
                                if let Err(e) = coordinator
                                    .create_company_summary(&company_id, &company_summary)
                                    .await
                                {
                                    eprintln!(
                                        "[{}] Failed to create company summary: {}",
                                        node_id, e
                                    );
                                } else {
                                    let crdt_latency_ms =
                                        crdt_start.elapsed().as_secs_f64() * 1000.0;
                                    println!(
                                        "[{}] ✓ Created company {} ({} platoons, {} total members)",
                                        node_id,
                                        company_id,
                                        company_summary.platoon_count,
                                        company_summary.total_member_count
                                    );
                                    // Log CRDT create latency for Lab 4 analysis
                                    println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"company_commander\",\"company_id\":\"{}\",\"operation\":\"create\",\"platoons_aggregated\":{},\"total_members\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, company_id, company_summary.platoon_count, company_summary.total_member_count, crdt_latency_ms, timestamp_us
                                );

                                    // Log CompanySummaryCreated event for metrics tracking
                                    log_metrics(&MetricsEvent::CompanySummaryCreated {
                                        node_id: node_id.to_string(),
                                        company_id: company_id.clone(),
                                        platoon_count: company_summary.platoon_count,
                                        total_member_count: company_summary.total_member_count,
                                        timestamp_us,
                                    });
                                }
                            }
                            Ok(Some(_existing)) => {
                                // Document exists - send delta update with latency tracking
                                use peat_protocol::hierarchy::deltas::CompanyDelta;
                                let delta = CompanyDelta::from_summary(
                                    &company_summary,
                                    timestamp_us as u64,
                                );

                                let crdt_start = Instant::now();
                                if let Err(e) =
                                    coordinator.update_company_summary(&company_id, delta).await
                                {
                                    eprintln!(
                                        "[{}] Failed to update company summary: {}",
                                        node_id, e
                                    );
                                } else {
                                    let crdt_latency_ms =
                                        crdt_start.elapsed().as_secs_f64() * 1000.0;
                                    println!(
                                        "[{}] ↑ Updated company {} ({} platoons, {} total members)",
                                        node_id,
                                        company_id,
                                        company_summary.platoon_count,
                                        company_summary.total_member_count
                                    );
                                    // Log CRDT update latency for Lab 4 analysis
                                    println!(
                                    "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"company_commander\",\"company_id\":\"{}\",\"operation\":\"delta_update\",\"platoons_aggregated\":{},\"total_members\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
                                    node_id, company_id, company_summary.platoon_count, company_summary.total_member_count, crdt_latency_ms, timestamp_us
                                );

                                    // Log CompanySummaryCreated event for metrics tracking
                                    log_metrics(&MetricsEvent::CompanySummaryCreated {
                                        node_id: node_id.to_string(),
                                        company_id: company_id.clone(),
                                        platoon_count: company_summary.platoon_count,
                                        total_member_count: company_summary.total_member_count,
                                        timestamp_us,
                                    });
                                }
                            }
                            Err(e) => {
                                eprintln!("[{}] Failed to check company summary: {}", node_id, e);
                            }
                        }

                        // Log aggregation completed
                        log_metrics(&MetricsEvent::AggregationCompleted {
                            node_id: node_id.to_string(),
                            tier: "company".to_string(),
                            input_doc_type: "PlatoonSummary".to_string(),
                            output_doc_type: "CompanySummary".to_string(),
                            output_doc_id: format!("company-summary-{}", company_id),
                            input_count: company_summary.platoon_count as usize,
                            processing_time_us,
                            timestamp_us: now_micros(),
                            input_bytes: None,
                            output_bytes: None,
                            bytes_saved: None,
                            reduction_ratio: Some(company_summary.platoon_count as f64),
                        });

                        // Log aggregation efficiency for Lab 4 analysis
                        let reduction_ratio = company_summary.platoon_count as f64;
                        println!(
                        "METRICS: {{\"event_type\":\"AggregationEfficiency\",\"node_id\":\"{}\",\"tier\":\"company\",\"input_docs\":{},\"output_docs\":1,\"reduction_ratio\":{:.1},\"total_members\":{},\"timestamp_us\":{}}}",
                        node_id, company_summary.platoon_count, reduction_ratio, company_summary.total_member_count, now_micros()
                    );

                        // Publish company as a single rolled-up CellInfo for ATAK
                        // The company is the unit visible at this level — platoons are sub-units
                        // aggregated into the company's readiness, position, and member count.
                        if let Some(ref cells) = cells_coll {
                            let centroid = company_summary.position_centroid.as_ref();
                            // Status based on member count — sim soldiers are always operational
                            let status = if company_summary.total_member_count > 0 {
                                "ACTIVE"
                            } else {
                                "OFFLINE"
                            };
                            // Map internal company ID to NATO display name
                            let display_name = std::env::var("COMPANY_DISPLAY_NAME")
                                .unwrap_or_else(|_| {
                                    // Map company ID to NATO display name
                                    match company_id.as_str() {
                                        "company-ALPHA" => "ALPHA".to_string(),
                                        "company-BRAVO" => "BRAVO".to_string(),
                                        "company-CHARLIE" => "CHARLIE".to_string(),
                                        "company-DELTA" => "DELTA".to_string(),
                                        _ => company_id
                                            .strip_prefix("company-")
                                            .unwrap_or(&company_id)
                                            .to_uppercase(),
                                    }
                                });

                            // Extract real aggregated capability names from the CompanySummary
                            // Collect detailed capability names from embedded platforms
                            let mut detailed_caps: Vec<String> = embedded_platforms
                                .iter()
                                .filter_map(|p| p.get("capabilities"))
                                .filter_map(|c| c.as_array())
                                .flat_map(|arr| arr.iter())
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();
                            detailed_caps.sort();
                            if detailed_caps.is_empty() {
                                // Fallback to aggregated type names
                                detailed_caps = company_summary
                                    .aggregated_capabilities
                                    .iter()
                                    .map(|c| c.name.clone())
                                    .filter(|n| !n.is_empty())
                                    .collect();
                            }

                            let cell_json = serde_json::json!({
                                "name": format!("{} ({} PLT, {} PAX)",
                                    display_name,
                                    company_summary.platoon_count,
                                    company_summary.total_member_count),
                                "status": status,
                                "platform_count": company_summary.total_member_count,
                                "center_lat": centroid.map(|p| p.latitude).unwrap_or(0.0),
                                "center_lon": centroid.map(|p| p.longitude).unwrap_or(0.0),
                                "capabilities": &detailed_caps,
                                "formation_id": serde_json::Value::Null,
                                "leader_id": company_summary.leader_id,
                                "last_update": now_micros() / 1000,
                                "readiness": format!("{:.0}%", company_summary.readiness_score * 100.0),
                                "avg_fuel": format!("{:.0} min", company_summary.avg_fuel_minutes),
                                "worst_health": match company_summary.worst_health {
                                    1 => "Nominal",
                                    2 => "Degraded",
                                    3 => "Critical",
                                    _ => "Unknown",
                                },
                                "hierarchy": hierarchy_json,
                                "embedded_platforms": embedded_platforms,
                            });
                            if let Err(e) =
                                cells.upsert(&company_id, cell_json.to_string().into_bytes())
                            {
                                eprintln!("[{}] Failed to publish company cell: {}", node_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[{}] Failed to aggregate company: {}", node_id, e);
                    }
                }
            }
        };

    // NOTE: Platform markers are generated client-side in the ATAK plugin from
    // the cell hierarchy data. No need to publish individual platform docs to the
    // CRDT store — doing so causes unbounded memory growth from Automerge revision
    // history accumulation.

    // Track last aggregation time for periodic fallback (Issue #493 - recovery time fix)
    let mut last_aggregation = Instant::now();

    // EVENT-DRIVEN: Listen for platoon summary changes and aggregate IMMEDIATELY
    // With periodic fallback every 5 seconds if change stream events fail
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
                                // Match platoon summaries
                                if doc_id.contains("platoon-") {
                                    // Fallback to doc.updated_at for Automerge
                                    let doc_updated_at_us = doc
                                        .updated_at
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_micros();

                                    let created_at_us = if let Some(ts) = doc.get("created_at_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if let Some(ts) = doc.get("timestamp_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else {
                                        doc_updated_at_us // Automerge fallback
                                    };

                                    let last_modified_us =
                                        if let Some(ts) = doc.get("last_update_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else if let Some(ts) = doc.get("last_modified_us") {
                                            ts.as_u64().unwrap_or(0) as u128
                                        } else if doc_updated_at_us > 0 {
                                            doc_updated_at_us // Automerge fallback
                                        } else {
                                            created_at_us
                                        };

                                    let update_key = (doc_id.clone(), last_modified_us);
                                    if last_modified_us > 0
                                        && !seen_doc_updates.contains(&update_key)
                                    {
                                        seen_doc_updates.insert(update_key);

                                        let latency_us =
                                            received_at_us.saturating_sub(last_modified_us);
                                        let latency_ms = latency_us as f64 / 1000.0;

                                        println!(
                                            "[{}] ✓ Platoon summary received (initial): {} (latency: {:.3}ms)",
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
                                            version: 1,
                                            is_first_reception: true,
                                            latency_type: "platoon_to_company_commander"
                                                .to_string(),
                                            source_tier: Some("platoon_leader".to_string()),
                                            dest_tier: Some("company_commander".to_string()),
                                            is_warmup: None,
                                        });

                                        // EVENT-DRIVEN: Aggregate immediately when platoon summary arrives
                                        do_aggregation(
                                            Arc::clone(&coordinator),
                                            company_id.clone(),
                                            node_id.clone(),
                                            platoon_ids.clone(),
                                            cells_collection.clone(),
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                    }
                    ChangeEvent::Updated { document, .. } => {
                        let received_at_us = now_micros();
                        if let Some(doc_id) = &document.id {
                            if doc_id.contains("platoon-") {
                                // Fallback to document.updated_at for Automerge
                                let doc_updated_at_us = document
                                    .updated_at
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_micros();

                                let created_at_us = if let Some(ts) = document.get("created_at_us")
                                {
                                    ts.as_u64().unwrap_or(0) as u128
                                } else if let Some(ts) = document.get("timestamp_us") {
                                    ts.as_u64().unwrap_or(0) as u128
                                } else {
                                    doc_updated_at_us // Automerge fallback
                                };

                                let last_modified_us =
                                    if let Some(ts) = document.get("last_update_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if let Some(ts) = document.get("last_modified_us") {
                                        ts.as_u64().unwrap_or(0) as u128
                                    } else if doc_updated_at_us > 0 {
                                        doc_updated_at_us // Automerge fallback
                                    } else {
                                        created_at_us
                                    };

                                let update_key = (doc_id.clone(), last_modified_us);
                                if last_modified_us > 0 && !seen_doc_updates.contains(&update_key) {
                                    seen_doc_updates.insert(update_key);

                                    let latency_us =
                                        received_at_us.saturating_sub(last_modified_us);
                                    let latency_ms = latency_us as f64 / 1000.0;

                                    println!(
                                        "[{}] ✓ Platoon summary received (update): {} (latency: {:.3}ms)",
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
                                        version: 1,
                                        is_first_reception: false,
                                        latency_type: "platoon_to_company_commander".to_string(),
                                        source_tier: Some("platoon_leader".to_string()),
                                        dest_tier: Some("company_commander".to_string()),
                                        is_warmup: None,
                                    });

                                    // EVENT-DRIVEN: Aggregate immediately
                                    do_aggregation(
                                        Arc::clone(&coordinator),
                                        company_id.clone(),
                                        node_id.clone(),
                                        platoon_ids.clone(),
                                        cells_collection.clone(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(None) => {
                return Err("Change stream closed unexpectedly".into());
            }
            Err(_) => {
                // Timeout waiting for event - perform periodic aggregation if needed
                // This ensures aggregation happens even when change stream events fail
                // (Issue #493 - fix 32s recovery time due to deserialization errors)
                if last_aggregation.elapsed() >= Duration::from_secs(5) {
                    do_aggregation(
                        Arc::clone(&coordinator),
                        company_id.clone(),
                        node_id.clone(),
                        platoon_ids.clone(),
                        cells_collection.clone(),
                    )
                    .await;
                    last_aggregation = Instant::now();
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize persistent metrics file logging (writes to /data/logs if mounted)
    init_metrics_file();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    let mut node_id = None;
    let mut mode = None;
    let mut backend_type = None;
    let mut tcp_listen_port = None;
    let mut tcp_connect_addr = None;
    let mut node_type = None;
    let mut update_rate_ms = None;
    let mut peat_filter_enabled = false;
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
            "--peat-filter" => {
                peat_filter_enabled = true;
            }
            "--enable-ack-collection" => {
                ack_collection_enabled = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Check for PEAT_FILTER_ENABLED environment variable
    if let Ok(val) = std::env::var("PEAT_FILTER_ENABLED") {
        peat_filter_enabled = val.to_lowercase() == "true" || val == "1";
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

    println!("[{}] Peat Network Simulation Node starting", node_id);
    println!("[{}] Mode: {}", node_id, mode);
    println!("[{}] Backend: {}", node_id, backend_type);
    println!("[{}] Node Type: {}", node_id, node_type);
    println!("[{}] Update Rate: {}ms", node_id, update_rate_ms);

    // Register with lab orchestrator (if configured)
    let role = std::env::var("ROLE").unwrap_or_else(|_| node_type.clone());
    orchestrator::register(&node_id, &backend_type, &role);
    println!(
        "[{}] Peat Filtering: {}",
        node_id,
        if peat_filter_enabled {
            "ENABLED (differential updates)"
        } else {
            "DISABLED (full replication)"
        }
    );

    if is_hierarchical_mode {
        println!("[{}] MODE 4: Hierarchical aggregation enabled", node_id);
    }

    if let Some(port) = tcp_listen_port {
        // Note: For automerge backend, this is actually QUIC (UDP), not TCP
        // The env var name is TCP_LISTEN for Ditto compatibility but automerge uses QUIC
        if backend_type == "automerge" {
            println!("[{}] QUIC: Will listen on UDP port {}", node_id, port);
        } else {
            println!("[{}] TCP: Will listen on port {}", node_id, port);
        }
    }
    if let Some(ref addr) = tcp_connect_addr {
        if backend_type == "automerge" {
            println!("[{}] QUIC: Will connect to {}", node_id, addr);
        } else {
            println!("[{}] TCP: Will connect to {}", node_id, addr);
        }
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
    // Use capability-filtered query if Peat filtering is enabled
    println!("[{}] Creating sync subscription...", node_id);
    let subscription_query = if peat_filter_enabled {
        if is_hierarchical_mode && std::env::var("ROLE").unwrap_or_default() == "platoon_leader" {
            // Platoon leaders ONLY subscribe to squad_summaries, not individual NodeStates
            println!(
                "[{}]   → Subscribing to squad_summaries (hierarchical mode)",
                node_id
            );
            Query::Custom("collection_name == 'squad_summaries'".to_string())
        } else {
            // Existing Peat-filtered query for soldiers and squad leaders
            println!(
                "[{}]   → Using Peat-filtered query for role: {}",
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

    // Report ready to orchestrator
    orchestrator::ready(&node_id);

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
                                Arc::new(peat_protocol::storage::DittoSummaryStorage::new(
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
                                        let err_str = e.to_string();
                                        if !err_str.contains("conflict resolution") {
                                            // Only log unexpected errors, not conflict rejections
                                            eprintln!(
                                                "[{}] Command issuance error: {}",
                                                cmd_node_id, e
                                            );
                                        }
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
                            use peat_protocol::storage::HierarchicalStorageCapable;

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

                            // Clone backend for aggregation loop (Issue #271: verify transport sharing)
                            let cloned_backend = (*automerge_backend).clone();

                            // Issue #271 debug: Verify transport Arc is shared between original and clone
                            let original_ptr = automerge_backend.transport_arc_ptr();
                            let cloned_ptr = cloned_backend.transport_arc_ptr();
                            if original_ptr != cloned_ptr {
                                eprintln!(
                                    "[{}] ⚠️  Issue #271 BUG: Transport Arc NOT shared!\n  Original: {:?}\n  Clone: {:?}",
                                    node_id, original_ptr, cloned_ptr
                                );
                            } else {
                                eprintln!(
                                    "[{}] ✓ Issue #271: Transport Arc correctly shared: {:?}",
                                    node_id, original_ptr
                                );
                            }

                            let backend_for_aggregation: Arc<Box<dyn DataSyncBackend>> =
                                Arc::new(Box::new(cloned_backend));
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

                            // Flat cell publishing: if this squad leader has COMPANY_ID set,
                            // it publishes a cell directly (no platoon/company hierarchy needed).
                            // This supports flat cells like the DiSCO USV swarm.
                            //
                            // Scenario commands (SIGUSR1/SIGUSR2) are piggybacked on the cell doc
                            // as a "scenario_command" field, since the cells collection is already
                            // synced to ATAK. The separate "commands" collection doesn't sync
                            // reliably because ATAK connects inbound and the push-based sync
                            // only targets outbound peers.
                            if let Ok(company_id) = std::env::var("COMPANY_ID") {
                                let cells_coll =
                                    automerge_backend.storage_backend().collection("cells");
                                let cell_coordinator = automerge_backend.summary_storage();
                                let cell_coordinator =
                                    Arc::new(HierarchicalAggregator::new(cell_coordinator));
                                let cell_squad_id = std::env::var("SQUAD_ID").unwrap_or_default();
                                let cell_node_id = node_id.clone();
                                let cell_company_id = company_id.clone();
                                let display_name = std::env::var("COMPANY_DISPLAY_NAME")
                                    .unwrap_or_else(|_| {
                                        company_id
                                            .strip_prefix("company-")
                                            .unwrap_or(&company_id)
                                            .to_uppercase()
                                    });

                                // Shared scenario state: signal handler sets, cell publisher reads
                                use std::sync::atomic::{AtomicU8, Ordering};
                                let scenario_state = Arc::new(AtomicU8::new(0)); // 0=none, 1=start, 2=stop

                                // Signal handler: SIGUSR1 → start, SIGUSR2 → stop
                                let sig_state = Arc::clone(&scenario_state);
                                let sig_node_id = node_id.clone();
                                tokio::spawn(async move {
                                    use tokio::signal::unix::{signal, SignalKind};
                                    let mut sig1 = signal(SignalKind::user_defined1())
                                        .expect("Failed to register SIGUSR1");
                                    let mut sig2 = signal(SignalKind::user_defined2())
                                        .expect("Failed to register SIGUSR2");
                                    loop {
                                        tokio::select! {
                                            _ = sig1.recv() => {
                                                sig_state.store(1, Ordering::SeqCst);
                                                println!("[{}] ▶ SIGUSR1 received — scenario_command=START_SCENARIO", sig_node_id);
                                            }
                                            _ = sig2.recv() => {
                                                sig_state.store(2, Ordering::SeqCst);
                                                println!("[{}] ■ SIGUSR2 received — scenario_command=STOP_SCENARIO", sig_node_id);
                                            }
                                        }
                                    }
                                });
                                println!("[{}] ✓ Scenario signal handler registered (SIGUSR1/SIGUSR2)", node_id);

                                // Cell publisher loop (includes scenario_command field)
                                tokio::spawn(async move {
                                    // Wait for first aggregation
                                    sleep(Duration::from_secs(10)).await;
                                    loop {
                                        if let Ok(Some(summary)) =
                                            cell_coordinator.get_squad_summary(&cell_squad_id).await
                                        {
                                            let centroid = summary.position_centroid.as_ref();
                                            let status = if summary.operational_count > 0 {
                                                "ACTIVE"
                                            } else {
                                                "OFFLINE"
                                            };
                                            let cap_names: Vec<String> = summary
                                                .aggregated_capabilities
                                                .iter()
                                                .map(|c| c.name.clone())
                                                .filter(|n| !n.is_empty())
                                                .collect();
                                            // Include scenario command if signal was received
                                            let scenario_cmd = match scenario_state.load(Ordering::SeqCst) {
                                                1 => serde_json::json!("START_SCENARIO"),
                                                2 => serde_json::json!("STOP_SCENARIO"),
                                                _ => serde_json::Value::Null,
                                            };
                                            let cell_json = serde_json::json!({
                                                "name": format!("{} ({} USV Swarm)", display_name, summary.member_count),
                                                "status": status,
                                                "platform_count": summary.member_count,
                                                "center_lat": centroid.map(|p| p.latitude).unwrap_or(0.0),
                                                "center_lon": centroid.map(|p| p.longitude).unwrap_or(0.0),
                                                "capabilities": if cap_names.is_empty() { serde_json::json!(["USV_SWARM"]) } else { serde_json::json!(cap_names) },
                                                "formation_id": serde_json::Value::Null,
                                                "leader_id": &cell_node_id,
                                                "last_update": now_micros() / 1000,
                                                "scenario_command": scenario_cmd,
                                            });
                                            let _ = cells_coll.upsert(
                                                &cell_company_id,
                                                cell_json.to_string().into_bytes(),
                                            );
                                        }
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                });
                                println!(
                                    "[{}] ✓ Flat cell publisher spawned for {}",
                                    node_id, company_id
                                );
                            }

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
                                        let err_str = e.to_string();
                                        if !err_str.contains("conflict resolution") {
                                            // Only log unexpected errors, not conflict rejections
                                            eprintln!(
                                                "[{}] Command issuance error: {}",
                                                cmd_node_id, e
                                            );
                                        }
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
                                Arc::new(peat_protocol::storage::DittoSummaryStorage::new(
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
                            use peat_protocol::storage::HierarchicalStorageCapable;

                            // Automerge uses different key format: squad-summary:{squad_id}
                            // Create observer specifically for Automerge's key prefix
                            let automerge_change_stream = backend.document_store().observe(
                                "squad-summary", // Match AutomergeSummaryStorage key prefix
                                &Query::All,     // All squad summaries
                            );

                            match automerge_change_stream {
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
            "company_commander" => {
                println!(
                    "[{}] Spawning company commander aggregation task...",
                    node_id
                );

                let company_id =
                    std::env::var("COMPANY_ID").unwrap_or_else(|_| "company-ALPHA".to_string());

                // Create observer for platoon summaries arriving via P2P mesh
                let change_stream_result = backend.document_store().observe(
                    "sim_poc",
                    &Query::Custom("collection_name == 'platoon_summaries'".to_string()),
                );

                // Get DittoStore from backend
                if let Some(ditto_backend) = backend.as_any().downcast_ref::<DittoBackend>() {
                    match (ditto_backend.get_ditto_store(), change_stream_result) {
                        (Ok(ditto_store), Ok(change_stream)) => {
                            // Wrap DittoStore in DittoSummaryStorage for backend abstraction
                            let storage =
                                Arc::new(peat_protocol::storage::DittoSummaryStorage::new(
                                    Arc::clone(&ditto_store),
                                ));
                            let coordinator = Arc::new(HierarchicalAggregator::new(storage));

                            // Phase 3: Instantiate CommandCoordinator for command dissemination
                            // Company commander can command platoons in the company
                            let cmd_storage =
                                Arc::new(DittoCommandStorage::new(Arc::clone(&ditto_store)));
                            let _cmd_coordinator = Arc::new(CommandCoordinator::new(
                                None, // Company commander is not in a squad
                                node_id.clone(),
                                vec![], // Platoon IDs will be determined dynamically
                                cmd_storage,
                            ));

                            let node_id_clone = node_id.clone();

                            println!("[{}] → Company: {}", node_id, company_id);

                            tokio::spawn(async move {
                                if let Err(e) = company_commander_aggregation_loop(
                                    change_stream,
                                    coordinator,
                                    company_id,
                                    node_id_clone.clone(),
                                    None, // Ditto backend — no cells collection
                                )
                                .await
                                {
                                    eprintln!(
                                        "[{}] Company commander aggregation error: {}",
                                        node_id_clone, e
                                    );
                                }
                            });

                            println!("[{}] ✓ Company commander aggregation task spawned", node_id);
                        }
                        (Err(e), _) => {
                            eprintln!("[{}] ✗ Failed to get DittoStore: {}", node_id, e);
                        }
                        (_, Err(e)) => {
                            eprintln!("[{}] ✗ Failed to create change stream: {}", node_id, e);
                        }
                    }
                } else {
                    // Try AutomergeIroh backend for company commander
                    #[cfg(feature = "automerge-backend")]
                    {
                        if let Some(automerge_backend) =
                            backend.as_any().downcast_ref::<AutomergeIrohBackend>()
                        {
                            use peat_protocol::storage::HierarchicalStorageCapable;

                            // Automerge uses different key format: platoon-summary:{platoon_id}
                            // Create observer specifically for Automerge's key prefix
                            let automerge_change_stream = backend.document_store().observe(
                                "platoon-summary", // Match AutomergeSummaryStorage key prefix
                                &Query::All,       // All platoon summaries
                            );

                            match automerge_change_stream {
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

                                    // Get cells + platforms collections for ATAK publishing
                                    let cells_coll: Option<
                                        Arc<dyn peat_protocol::storage::Collection>,
                                    > = Some(
                                        automerge_backend.storage_backend().collection("cells"),
                                    );
                                    println!(
                                        "[{}] → Company: {} (AutomergeIroh, cells publishing enabled)",
                                        node_id, company_id
                                    );

                                    tokio::spawn(async move {
                                        if let Err(e) = company_commander_aggregation_loop(
                                            change_stream,
                                            coordinator,
                                            company_id,
                                            node_id_clone.clone(),
                                            cells_coll,
                                        )
                                        .await
                                        {
                                            eprintln!(
                                                "[{}] Company commander aggregation error: {}",
                                                node_id_clone, e
                                            );
                                        }
                                    });

                                    println!(
                                        "[{}] ✓ Company commander aggregation task spawned (AutomergeIroh)",
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
                                "[{}] ✗ Cannot spawn company commander task: unsupported backend type",
                                node_id
                            );
                        }
                    }

                    #[cfg(not(feature = "automerge-backend"))]
                    {
                        eprintln!(
                            "[{}] ✗ Cannot spawn company commander task: backend is not DittoBackend",
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
        "hierarchical" => hierarchical_mode(&*backend, &node_id, &node_type, update_rate_ms).await,
        "flat_mesh" => {
            // Lab 3b: Flat P2P mesh with Peat CRDT (all nodes at same tier)
            flat_mesh_mode(&*backend, &node_id, &node_type, update_rate_ms).await
        }
        _ => {
            eprintln!("[{}] ✗ Invalid mode: {}", node_id, mode);
            std::process::exit(1);
        }
    };

    match result {
        Ok(()) => {
            println!("[{}] ✓✓✓ Peat SIMULATION SUCCESS ✓✓✓", node_id);
            // Shutdown gracefully
            backend.shutdown().await?;
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[{}] ✗✗✗ Peat SIMULATION FAILED: {} ✗✗✗", node_id, e);
            backend.shutdown().await.ok();
            std::process::exit(1);
        }
    }
}

/// Lab Orchestrator client - reports node status to central orchestrator
/// The orchestrator URL is read from ORCHESTRATOR_URL environment variable
mod orchestrator {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    fn get_orchestrator_url() -> Option<String> {
        std::env::var("ORCHESTRATOR_URL")
            .ok()
            .filter(|s| !s.is_empty())
    }

    fn parse_url(url: &str) -> Option<(String, u16, String)> {
        // Parse http://host:port/path
        let url = url.strip_prefix("http://").unwrap_or(url);
        let (host_port, path) = if let Some(idx) = url.find('/') {
            (&url[..idx], &url[idx..])
        } else {
            (url, "/")
        };
        let (host, port) = if let Some(idx) = host_port.find(':') {
            (&host_port[..idx], host_port[idx + 1..].parse().ok()?)
        } else {
            (host_port, 80u16)
        };
        Some((host.to_string(), port, path.to_string()))
    }

    fn http_post_once(base_url: &str, endpoint: &str, json_body: &str) -> Result<(), String> {
        use std::net::ToSocketAddrs;

        let full_url = format!("{}{}", base_url.trim_end_matches('/'), endpoint);
        let (host, port, path) = parse_url(&full_url).ok_or("Invalid URL")?;

        // Resolve hostname to IP address (required for connect_timeout)
        let addr = format!("{}:{}", host, port)
            .to_socket_addrs()
            .map_err(|e| format!("DNS resolution failed: {}", e))?
            .next()
            .ok_or("No addresses found")?;

        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .map_err(|e| format!("Connect failed: {}", e))?;

        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path, host, port, json_body.len(), json_body
        );

        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("Write failed: {}", e))?;

        let mut response = [0u8; 256];
        let _ = stream.read(&mut response); // Don't care about response

        Ok(())
    }

    fn http_post_with_retry(
        base_url: &str,
        endpoint: &str,
        json_body: &str,
        retries: u32,
    ) -> Result<(), String> {
        let mut last_err = String::new();
        for attempt in 0..retries {
            match http_post_once(base_url, endpoint, json_body) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = e;
                    if attempt < retries - 1 {
                        // Exponential backoff with jitter: 500ms * 2^attempt + random(0-500ms)
                        // With 10 retries: 500ms, 1s, 2s, 4s, 8s, 16s, 32s... (capped at 30s)
                        let base_ms = 500u64 * (1 << attempt.min(6));
                        let jitter_ms = std::process::id() as u64 % 500;
                        let backoff_ms = base_ms.min(30000) + jitter_ms;
                        std::thread::sleep(Duration::from_millis(backoff_ms));
                    }
                }
            }
        }
        Err(last_err)
    }

    pub fn register(node_id: &str, backend: &str, role: &str) {
        if let Some(url) = get_orchestrator_url() {
            // Initial random delay to spread out registration storm (5-15s)
            // Higher base delay gives orchestrator time to start and DNS to propagate
            let initial_delay_ms = 5000 + (std::process::id() as u64 % 10000);
            std::thread::sleep(Duration::from_millis(initial_delay_ms));

            let body = format!(
                r#"{{"node_id":"{}","backend":"{}","role":"{}"}}"#,
                node_id, backend, role
            );
            // 15 retries with exponential backoff: should span ~2-3 minutes total
            if let Err(e) = http_post_with_retry(&url, "/register", &body, 15) {
                eprintln!("[{}] Orchestrator register failed: {}", node_id, e);
            }
        }
    }

    pub fn ready(node_id: &str) {
        if let Some(url) = get_orchestrator_url() {
            let body = format!(r#"{{"node_id":"{}"}}"#, node_id);
            if let Err(e) = http_post_with_retry(&url, "/ready", &body, 10) {
                eprintln!("[{}] Orchestrator ready failed: {}", node_id, e);
            }
        }
    }

    #[allow(dead_code)]
    pub fn error(node_id: &str, error_msg: &str) {
        if let Some(url) = get_orchestrator_url() {
            let body = format!(
                r#"{{"node_id":"{}","error":"{}"}}"#,
                node_id,
                error_msg.replace('"', "'")
            );
            let _ = http_post_once(&url, "/error", &body);
        }
    }
}

/// Create a backend instance based on type
async fn create_backend(
    backend_type: &str,
    #[allow(unused)] node_id: &str,
    #[allow(unused)] tcp_listen_port: Option<u16>,
) -> Result<Box<dyn DataSyncBackend>, Box<dyn std::error::Error>> {
    match backend_type {
        "ditto" => Ok(Box::new(DittoBackend::new())),
        #[cfg(feature = "automerge-backend")]
        "automerge" => {
            // Create AutomergeIrohBackend with persistence and transport
            // This enables hierarchical mode support (HierarchicalStorageCapable trait)

            // Check for in-memory mode (CAP_IN_MEMORY=true skips all disk I/O)
            let in_memory = std::env::var("CAP_IN_MEMORY")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);

            let store = Arc::new(if in_memory {
                eprintln!(
                    "[{}] AutomergeStore: MEMORY-ONLY mode (no disk persistence)",
                    node_id
                );
                AutomergeStore::in_memory()
            } else {
                let persistence_dir = PathBuf::from(format!("/tmp/peat_sim_{}", node_id));
                std::fs::create_dir_all(&persistence_dir)?;
                AutomergeStore::open(&persistence_dir)
                    .map_err(|e| format!("Failed to open AutomergeStore: {}", e))?
            });

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

            // Use optimized startup (no mDNS) when TCP_CONNECT is configured (hierarchical mode)
            // mDNS discovery adds ~200ms+ overhead that's unnecessary when using static peer config
            let tcp_connect_configured = std::env::var("TCP_CONNECT").is_ok();
            let use_mdns = tcp_listen_port.is_none() && !tcp_connect_configured;

            let transport = Arc::new(if use_mdns {
                // mDNS discovery enabled - use full constructor
                IrohTransport::from_seed_with_discovery_at_addr(&seed, bind_addr)
                    .await
                    .map_err(|e| format!("Failed to create IrohTransport: {}", e))?
            } else {
                // Fast startup without mDNS (PR #451 optimization)
                IrohTransport::from_seed_at_addr(&seed, bind_addr)
                    .await
                    .map_err(|e| format!("Failed to create IrohTransport: {}", e))?
            });

            eprintln!(
                "[{}] Created Automerge transport with seed '{}', EndpointId: {}",
                node_id,
                seed,
                hex::encode(transport.endpoint_id().as_bytes())
            );

            // Spawn background compaction task to prevent memory bloat (Issue #401)
            // Automerge documents accumulate operation history - compaction discards history
            // while preserving current state, significantly reducing memory usage.
            let compaction_store = Arc::clone(&store);
            let compaction_node_id = node_id.to_string();
            tokio::spawn(async move {
                // Wait for initial sync to settle before first compaction
                tokio::time::sleep(Duration::from_secs(60)).await;

                let compaction_interval = Duration::from_secs(300); // Every 5 minutes
                loop {
                    // Compact high-frequency document prefixes
                    let prefixes = [
                        "squad-summary:",   // Squad aggregation summaries
                        "platoon-summary:", // Platoon aggregation summaries
                        "company-summary:", // Company aggregation summaries
                        "node_states:",     // Individual node states
                        "sim_poc:",         // Simulation documents
                    ];

                    let mut total_docs = 0;
                    let mut total_before = 0;
                    let mut total_after = 0;

                    for prefix in &prefixes {
                        match compaction_store.compact_prefix(prefix) {
                            Ok((count, before, after)) => {
                                total_docs += count;
                                total_before += before;
                                total_after += after;
                            }
                            Err(e) => {
                                eprintln!(
                                    "[{}] Compaction error for prefix '{}': {}",
                                    compaction_node_id, prefix, e
                                );
                            }
                        }
                    }

                    if total_docs > 0 {
                        let reduction_pct = if total_before > 0 {
                            100.0 - (total_after as f64 * 100.0 / total_before as f64)
                        } else {
                            0.0
                        };
                        println!(
                            "METRICS: {{\"event_type\":\"Compaction\",\"node_id\":\"{}\",\"documents\":{},\"bytes_before\":{},\"bytes_after\":{},\"reduction_pct\":{:.1},\"timestamp_us\":{}}}",
                            compaction_node_id, total_docs, total_before, total_after, reduction_pct, now_micros()
                        );
                    }

                    tokio::time::sleep(compaction_interval).await;
                }
            });

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

/// Normalize TCP_CONNECT address format for the specified backend.
///
/// TCP_CONNECT can be in two formats:
/// - Automerge format: "peer_name|hostname:port,peer2|host2:port2,..."
/// - Ditto format: "hostname:port,host2:port2,..."
///
/// This function normalizes the format based on the backend:
/// - For Ditto: strips the "peer_name|" prefix if present
/// - For Automerge: keeps as-is (needs peer name for EndpointId derivation)
fn normalize_tcp_connect(tcp_connect: &str, backend_type: &str) -> String {
    if backend_type == "ditto" {
        // Strip peer name prefix for Ditto (it only needs host:port)
        tcp_connect
            .split(',')
            .map(|peer_spec| {
                let peer_spec = peer_spec.trim();
                if let Some((_peer_name, host_port)) = peer_spec.split_once('|') {
                    host_port.to_string()
                } else {
                    peer_spec.to_string()
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(",")
    } else {
        // Automerge needs the full format with peer names
        tcp_connect.to_string()
    }
}

/// Create backend configuration from environment and CLI args
fn create_backend_config(
    node_id: &str,
    backend_type: &str,
    tcp_listen_port: Option<u16>,
    tcp_connect_addr: Option<String>,
) -> Result<BackendConfig, Box<dyn std::error::Error>> {
    let persistence_dir = PathBuf::from(format!("/tmp/peat_sim_{}", node_id));
    std::fs::create_dir_all(&persistence_dir)?;

    // Normalize TCP_CONNECT format for the backend
    let normalized_tcp_connect = tcp_connect_addr
        .as_ref()
        .map(|addr| normalize_tcp_connect(addr, backend_type));

    let enable_mdns = tcp_listen_port.is_none() && normalized_tcp_connect.is_none();
    let transport = TransportConfig {
        tcp_listen_port,
        tcp_connect_address: normalized_tcp_connect.clone(),
        enable_mdns,
        enable_bluetooth: false,
        enable_websocket: false,
        custom: HashMap::new(),
    };

    eprintln!(
        "[{}] Transport config: listen={:?}, connect={:?}, mdns={}",
        node_id, tcp_listen_port, normalized_tcp_connect, enable_mdns
    );

    // Load credentials using unified helper (handles PEAT_* and DITTO_* fallbacks)
    let credentials =
        PeatCredentials::from_env().map_err(|e| format!("Failed to load credentials: {}", e))?;
    let secret_key = credentials
        .require_secret_key()
        .map_err(|e| format!("Failed to load secret key: {}", e))?
        .to_string();

    let config = match backend_type {
        "ditto" => BackendConfig {
            app_id: credentials.app_id().to_string(),
            persistence_dir,
            shared_key: Some(secret_key),
            transport,
            extra: HashMap::new(),
        },
        #[cfg(feature = "automerge-backend")]
        "automerge" => BackendConfig {
            app_id: credentials.app_id().to_string(),
            persistence_dir,
            shared_key: Some(secret_key),
            transport,
            extra: HashMap::new(),
        },
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
/// - Hash: SHA-256 with domain separator "peat-iroh-key-v1:"
/// - Result: ed25519 public key (EndpointId)
#[cfg(feature = "automerge-backend")]
async fn connect_to_automerge_peers(
    sync_engine: &Arc<dyn peat_protocol::sync::SyncEngine>,
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

    // Parse TCP_CONNECT in two formats:
    // 1. Explicit: "peer_name|hostname:port" (Automerge-specific)
    // 2. Containerlab: "clab-{topo}-{node}:port" (unified topology)
    // For containerlab format, extract node name from container name
    for peer_spec in tcp_connect.split(',') {
        let peer_spec = peer_spec.trim();
        if peer_spec.is_empty() {
            continue;
        }

        let (peer_name, peer_addr): (String, &str) = if peer_spec.contains('|') {
            // Explicit format: "peer_name|hostname:port"
            let parts: Vec<&str> = peer_spec.splitn(2, '|').collect();
            if parts.len() != 2 {
                eprintln!(
                    "[{}] Invalid peer spec (expected 'name|address'): {}",
                    node_id, peer_spec
                );
                continue;
            }
            (parts[0].to_string(), parts[1])
        } else {
            // Containerlab format: "clab-{topo}-{node}:port"
            // Extract node name from container hostname
            let addr_parts: Vec<&str> = peer_spec.splitn(2, ':').collect();
            if addr_parts.len() != 2 {
                eprintln!(
                    "[{}] Invalid peer spec (expected 'hostname:port'): {}",
                    node_id, peer_spec
                );
                continue;
            }
            let hostname = addr_parts[0];
            // Container names: clab-{topology}-{node}
            // Extract node name by stripping "clab-{topology}-" prefix
            // The topology name ends before the first occurrence of "company-", "platoon-", "squad-", or "soldier-"
            let node_name = if let Some(idx) = hostname.find("company-") {
                &hostname[idx..]
            } else {
                eprintln!(
                    "[{}] Cannot extract node name from containerlab hostname: {}",
                    node_id, hostname
                );
                continue;
            };
            (node_name.to_string(), peer_spec)
        };

        let peer_name = peer_name.as_str();

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

        // Resolve hostname to IP addresses (containerlab uses Docker DNS)
        // Retry DNS resolution with backoff since containers start in parallel
        // Use 30 attempts with 2-second intervals to handle slow container startup
        let mut resolved_addrs: Vec<String> = Vec::new();
        for attempt in 1..=30 {
            match tokio::net::lookup_host(peer_addr).await {
                Ok(addrs) => {
                    resolved_addrs = addrs.map(|a| a.to_string()).collect();
                    if !resolved_addrs.is_empty() {
                        break;
                    }
                }
                Err(e) => {
                    if attempt == 30 {
                        eprintln!(
                            "[{}] ✗ Failed to resolve '{}' after 30 attempts: {}",
                            node_id, peer_addr, e
                        );
                    } else if attempt % 5 == 0 {
                        eprintln!(
                            "[{}] DNS attempt {}/30 for '{}' still failing, retrying...",
                            node_id, attempt, peer_addr
                        );
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }

        if resolved_addrs.is_empty() {
            eprintln!(
                "[{}] ✗ No addresses resolved for '{}' after retries",
                node_id, peer_addr
            );
            continue;
        }

        eprintln!(
            "[{}] Resolved '{}' to {:?}",
            node_id, peer_addr, resolved_addrs
        );

        // Issue #373: Add staggered connection timing to prevent thundering herd
        // When many nodes start simultaneously, they all try to connect at once,
        // overwhelming responders and causing handshake timeouts.
        // Add a random delay of 0-2 seconds to spread out connection attempts.
        let stagger_delay_ms = rand::thread_rng().gen_range(0..2000);
        tokio::time::sleep(tokio::time::Duration::from_millis(stagger_delay_ms)).await;

        // Connect using the SyncEngine trait method with retry logic
        // Issue #346: connect_peer now always attempts connection
        // Conflict resolution happens on detection (not preemptive)
        // Retry connection up to 10 times with 3-second intervals to handle
        // timing issues where the peer's listener isn't ready yet
        let mut connected = false;
        for conn_attempt in 1..=10 {
            match sync_engine
                .connect_to_peer(&peer_endpoint_hex, &resolved_addrs)
                .await
            {
                Ok(_) => {
                    eprintln!("[{}] ✓ Connected to peer '{}'", node_id, peer_name);
                    connected = true;
                    break;
                }
                Err(e) => {
                    if conn_attempt == 10 {
                        eprintln!(
                            "[{}] ✗ Failed to connect to peer '{}' after 10 attempts: {}",
                            node_id, peer_name, e
                        );
                    } else if conn_attempt % 3 == 0 {
                        eprintln!(
                            "[{}] Connection attempt {}/10 for '{}' failed, retrying...",
                            node_id, conn_attempt, peer_name
                        );
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }
        }
        if !connected {
            eprintln!(
                "[{}] ⚠ Continuing without connection to '{}' - sync may still work via other peers",
                node_id, peer_name
            );
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

/// Soldier capability mode: Send periodic updates with dynamic position, fuel, health, and capabilities.
async fn soldier_capability_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    _node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Determine platform type and initialize simulation state
    let platform_type = simulation::assign_platform_type(node_id);
    let seed = simulation::PositionSeed::from_env();
    let mut sim_state = if platform_type == simulation::PlatformType::Usv {
        // USV nodes use waypoint-based perimeter patrol
        let usv_index: usize = std::env::var("USV_INDEX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let usv_total: usize = std::env::var("USV_TOTAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7);
        simulation::NodeSimState::new_usv_patrol(node_id, &seed, usv_index, usv_total)
    } else {
        simulation::NodeSimState::new(node_id, platform_type, &seed)
    };
    let capabilities = simulation::generate_capabilities(node_id, platform_type, "soldier");
    let cap_names = simulation::capability_names(&capabilities);

    println!(
        "[{}] Running as {} with {} capabilities (center: {:.4}, {:.4})",
        node_id,
        platform_type.as_str(),
        capabilities.len(),
        seed.center_lat,
        seed.center_lon,
    );
    for cap in &cap_names {
        println!("[{}]   - {}", node_id, cap);
    }

    // Mission parameters - run indefinitely unless MAX_UPDATES is set
    let max_updates: u64 = std::env::var("MAX_UPDATES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);

    let update_interval = Duration::from_millis(update_rate_ms);
    let dt_secs = update_rate_ms as f64 / 1000.0;
    let mut message_number: u64 = 0;
    let doc_id = format!("sim_doc_{}", node_id);

    while message_number < max_updates {
        message_number += 1;
        sim_state.tick(dt_secs);
        let timestamp_us = now_micros();
        let (lat, lon) = sim_state.position;

        // Create document fields with dynamic state
        let mut fields = HashMap::new();
        fields.insert("timestamp_us".to_string(), serde_json::json!(timestamp_us));
        fields.insert("created_by".to_string(), Value::String(node_id.to_string()));
        fields.insert(
            "node_type".to_string(),
            Value::String(platform_type.as_str().to_string()),
        );
        fields.insert(
            "message_number".to_string(),
            serde_json::json!(message_number),
        );
        fields.insert("lat".to_string(), serde_json::json!(lat));
        fields.insert("lon".to_string(), serde_json::json!(lon));
        fields.insert(
            "fuel_minutes".to_string(),
            serde_json::json!(sim_state.fuel_minutes_u32()),
        );
        fields.insert("health".to_string(), serde_json::json!(sim_state.health));
        fields.insert(
            "capabilities".to_string(),
            Value::Array(cap_names.iter().map(|c| Value::String(c.clone())).collect()),
        );
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields.clone());

        let crdt_start = Instant::now();
        backend.document_store().upsert("sim_poc", document).await?;
        let crdt_latency_ms = crdt_start.elapsed().as_secs_f64() * 1000.0;

        // NOTE: Platform markers are generated client-side in ATAK.
        // No CRDT platform publishing — causes OOM from revision accumulation.

        let message_json = serde_json::to_string(&fields)?;
        let message_size_bytes = message_json.len();

        println!(
            "METRICS: {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"soldier\",\"platform_type\":\"{}\",\"message_number\":{},\"lat\":{:.6},\"lon\":{:.6},\"fuel\":{},\"health\":{},\"latency_ms\":{:.3},\"timestamp_us\":{}}}",
            node_id, platform_type.as_str(), message_number, lat, lon, sim_state.fuel_minutes_u32(), sim_state.health, crdt_latency_ms, timestamp_us
        );

        if max_updates < 100 || message_number % 10 == 0 {
            println!(
                "[{}] ✓ {} update #{} — pos=({:.5},{:.5}) fuel={} health={}",
                node_id,
                platform_type.as_str(),
                message_number,
                lat,
                lon,
                sim_state.fuel_minutes_u32(),
                sim_state.health
            );
        }

        log_metrics(&MetricsEvent::MessageSent {
            node_id: node_id.to_string(),
            node_type: platform_type.as_str().to_string(),
            message_number,
            message_size_bytes,
            timestamp_us,
        });

        sleep(update_interval).await;
    }

    println!(
        "[{}] ✓ Completed {} status updates",
        node_id, message_number
    );

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
                                            source_tier: Some("soldier".to_string()),
                                            dest_tier: Some("soldier".to_string()),
                                            is_warmup: None,
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
                                            source_tier: Some("squad_leader".to_string()),
                                            dest_tier: Some("soldier".to_string()),
                                            is_warmup: None,
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
    _node_type: &str,
    update_rate_ms: u64,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let seed = simulation::PositionSeed::from_env();
    let (lat, lon) = seed.initial_position(node_id);
    let capabilities =
        simulation::generate_capabilities(node_id, simulation::PlatformType::Soldier, role);
    let cap_names = simulation::capability_names(&capabilities);

    println!(
        "[{}] Running as {} at ({:.5}, {:.5}) with {} capabilities",
        node_id,
        role.to_uppercase(),
        lat,
        lon,
        cap_names.len()
    );

    let update_interval = Duration::from_millis(update_rate_ms);
    let mut message_number: u64 = 0;
    let doc_id = format!("sim_doc_{}", node_id);

    loop {
        message_number += 1;
        let timestamp_us = now_micros();

        let mut fields = HashMap::new();
        fields.insert("timestamp_us".to_string(), serde_json::json!(timestamp_us));
        fields.insert("created_by".to_string(), Value::String(node_id.to_string()));
        fields.insert("node_type".to_string(), Value::String(role.to_string()));
        fields.insert("role".to_string(), Value::String(role.to_string()));
        fields.insert(
            "message_number".to_string(),
            serde_json::json!(message_number),
        );
        fields.insert("lat".to_string(), serde_json::json!(lat));
        fields.insert("lon".to_string(), serde_json::json!(lon));
        fields.insert("fuel_minutes".to_string(), serde_json::json!(100u32));
        fields.insert("health".to_string(), serde_json::json!(1i32));
        fields.insert(
            "capabilities".to_string(),
            Value::Array(cap_names.iter().map(|c| Value::String(c.clone())).collect()),
        );
        fields.insert("public".to_string(), Value::Bool(true));

        let document = Document::with_id(doc_id.clone(), fields);
        backend.document_store().upsert("sim_poc", document).await?;

        // NOTE: Platform markers are generated client-side in ATAK.
        // No CRDT platform publishing — causes OOM from revision accumulation.

        if message_number % 10 == 0 {
            println!("[{}] ✓ {} heartbeat #{}", node_id, role, message_number);
        }

        sleep(update_interval).await;
    }
}

/// Lab 3b: Flat P2P mesh mode with Peat CRDT
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
    let max_updates: u64 = std::env::var("MAX_UPDATES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);

    while sequence < max_updates {
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

        if max_updates < 100 || sequence % 10 == 0 {
            println!(
                "[{}] Published state update {} to flat mesh, CRDT_latency: {:.3}ms",
                node_id, sequence, upsert_latency_ms
            );
        }

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
        node_id, sequence
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

    // Send periodic updates indefinitely until lab is destroyed
    loop {
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
        // Add Peat authorization field
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

    // Listen for document changes indefinitely until lab is destroyed
    loop {
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
    // Fallback to doc.updated_at for Automerge (which doesn't store timestamps in fields)
    let doc_updated_at_us = doc
        .updated_at
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();

    let created_at_us = if let Some(ts) = doc.get("created_at_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else if let Some(ts) = doc.get("timestamp_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else {
        doc_updated_at_us // Automerge fallback
    };

    // Note: Storage writes "last_update_us", so check that first
    // Fallback to doc.updated_at for Automerge
    let last_modified_us = if let Some(ts) = doc.get("last_update_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else if let Some(ts) = doc.get("last_modified_us") {
        ts.as_u64().unwrap_or(0) as u128
    } else if doc_updated_at_us > 0 {
        doc_updated_at_us // Automerge fallback
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
                    source_tier: None,
                    dest_tier: None,
                    is_warmup: None,
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
                if message == "Hello from Peat Simulation!" {
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
                        source_tier: None,
                        dest_tier: None,
                        is_warmup: None,
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
                source_tier: Some("squad_leader".to_string()),
                dest_tier: None, // Will be determined by receiver context
                is_warmup: None,
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
            source_tier: Some("platoon_leader".to_string()),
            dest_tier: None, // Will be determined by receiver context
            is_warmup: None,
        });
    }

    Ok(())
}
