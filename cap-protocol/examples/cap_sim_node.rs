//! CAP Network Simulation Node
//!
//! Generic network simulation node that works with any `DataSyncBackend`.
//! This replaces the Ditto-specific `shadow_poc.rs` with a trait-based implementation.
//!
//! # What This Tests
//!
//! - Backend initialization (Ditto, Automerge, or custom)
//! - Peer discovery across simulated network
//! - Document creation and replication
//! - CRDT sync with network constraints
//!
//! # Architecture
//!
//! ContainerLab runs multiple containers with this binary:
//! - Writer nodes: Create test documents
//! - Reader nodes: Wait to receive documents
//! - All nodes use pluggable sync backend
//!
//! # Success Criteria
//!
//! - Backend initializes successfully
//! - Peers discover each other
//! - Documents sync between nodes
//! - Works with network constraints (latency, bandwidth, loss)
//!
//! # Command Line Arguments
//!
//! --node-id <id>         Node identifier (e.g., "node1", "node2")
//! --mode <mode>          "writer" (creates document) or "reader" (waits for document)
//! --backend <type>       Sync backend to use ("ditto", "automerge")
//! --tcp-listen <port>    Optional: Listen for TCP connections on this port
//! --tcp-connect <addr>   Optional: Connect to TCP peer at this address (e.g., "node1:12345")
//!
//! # Exit Codes
//!
//! 0: Success (document synced)
//! 1: Failure (timeout, error, or document not received)
//!
//! # Environment Variables (Backend-specific)
//!
//! **Ditto Backend:**
//! - DITTO_APP_ID: Application ID from Ditto portal
//! - DITTO_OFFLINE_TOKEN: Offline license token
//! - DITTO_SHARED_KEY: Shared encryption key
//!
//! **Automerge Backend:**
//! - (None required - uses local storage only)

use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::sync::{
    BackendConfig, ChangeEvent, ChangeStream, DataSyncBackend, Document, Query, TransportConfig,
    Value,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

// Mode 4: Hierarchical aggregation imports
#[allow(unused_imports)]
use cap_protocol::hierarchy::StateAggregator;
#[allow(unused_imports)]
use cap_protocol::models::{NodeConfig, NodeState};
use cap_protocol::storage::DittoStore;

/// Test document structure (currently unused - documents are created dynamically)
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
        inserted_at_us: u128, // From document
        received_at_us: u128, // Local time
        latency_us: u128,     // Difference
        latency_ms: f64,
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
}

/// Squad leader aggregation loop (Mode 4)
///
/// Periodically aggregates member NodeStates into SquadSummary and publishes to Ditto
async fn squad_leader_aggregation_loop(
    store: Arc<DittoStore>,
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
        // (Current sim stores simple messages, not full NodeState documents)
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
            // Note: In production, this would query actual NodeState documents from Ditto
            let state = NodeState {
                position: Some(cap_schema::common::v1::Position {
                    latitude: 0.0,
                    longitude: 0.0,
                    altitude: 0.0,
                }),
                fuel_minutes: 100,
                health: cap_schema::node::v1::HealthStatus::Nominal.into(),
                phase: cap_schema::node::v1::Phase::Hierarchy.into(),
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
                    // Publish to squad_summaries collection
                    if let Err(e) = store.upsert_squad_summary(&squad_id, &squad_summary).await {
                        eprintln!("[{}] Failed to upsert squad summary: {}", node_id, e);
                    } else {
                        println!(
                            "[{}] ✓ Aggregated squad {} ({} members, readiness: {:.2})",
                            node_id,
                            squad_id,
                            squad_summary.member_count,
                            squad_summary.readiness_score
                        );
                    }
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
/// Event-driven aggregation triggered by squad summary updates via Ditto P2P mesh
async fn platoon_leader_aggregation_loop(
    mut change_stream: ChangeStream,
    store: Arc<DittoStore>,
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

    // Clone store for aggregation task
    let store_clone = Arc::clone(&store);
    let platoon_id_clone = platoon_id.clone();
    let node_id_clone = node_id.clone();
    let squad_ids_clone = squad_ids.clone();

    // Spawn periodic aggregation task
    let aggregation_handle = tokio::spawn(async move {
        loop {
            // Collect latest squad summaries
            let mut squad_summaries = Vec::new();

            for squad_id in &squad_ids_clone {
                if let Ok(Some(summary)) = store_clone.get_squad_summary(squad_id).await {
                    squad_summaries.push(summary);
                }
            }

            if squad_summaries.len() == 3 {
                // Aggregate into PlatoonSummary
                match StateAggregator::aggregate_platoon(
                    &platoon_id_clone,
                    &node_id_clone,
                    squad_summaries,
                ) {
                    Ok(platoon_summary) => {
                        // Publish to platoon_summaries collection
                        if let Err(e) = store_clone
                            .upsert_platoon_summary(&platoon_id_clone, &platoon_summary)
                            .await
                        {
                            eprintln!(
                                "[{}] Failed to upsert platoon summary: {}",
                                node_id_clone, e
                            );
                        } else {
                            println!(
                                "[{}] ✓ Aggregated platoon {} ({} squads, {} total members)",
                                node_id_clone,
                                platoon_id_clone,
                                platoon_summary.squad_count,
                                platoon_summary.total_member_count
                            );
                        }
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
                                    // Extract timestamp for latency calculation
                                    if let Some(ts_value) = doc.get("timestamp_us") {
                                        let inserted_at_us = ts_value.as_u64().unwrap_or(0) as u128;
                                        if inserted_at_us > 0 {
                                            let latency_us =
                                                received_at_us.saturating_sub(inserted_at_us);
                                            let latency_ms = latency_us as f64 / 1000.0;

                                            println!(
                                                "[{}] ✓ Squad summary received (initial): {} (latency: {:.3}ms)",
                                                node_id, doc_id, latency_ms
                                            );

                                            log_metrics(&MetricsEvent::DocumentReceived {
                                                node_id: node_id.to_string(),
                                                doc_id: doc_id.to_string(),
                                                inserted_at_us,
                                                received_at_us,
                                                latency_us,
                                                latency_ms,
                                            });
                                        }
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
                                // Extract timestamp for latency calculation
                                if let Some(ts_value) = document.get("timestamp_us") {
                                    let inserted_at_us = ts_value.as_u64().unwrap_or(0) as u128;
                                    if inserted_at_us > 0 {
                                        let latency_us =
                                            received_at_us.saturating_sub(inserted_at_us);
                                        let latency_ms = latency_us as f64 / 1000.0;

                                        println!(
                                            "[{}] ✓ Squad summary received: {} (latency: {:.3}ms)",
                                            node_id, doc_id, latency_ms
                                        );

                                        log_metrics(&MetricsEvent::DocumentReceived {
                                            node_id: node_id.to_string(),
                                            doc_id: doc_id.to_string(),
                                            inserted_at_us,
                                            received_at_us,
                                            latency_us,
                                            latency_ms,
                                        });
                                    }
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
    let mut cap_filter_enabled = false;

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
            "--cap-filter" => {
                cap_filter_enabled = true;
            }
            _ => {}
        }
        i += 1;
    }

    // Check for CAP_FILTER_ENABLED environment variable
    if let Ok(val) = std::env::var("CAP_FILTER_ENABLED") {
        cap_filter_enabled = val.to_lowercase() == "true" || val == "1";
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

    println!("[{}] CAP Network Simulation Node starting", node_id);
    println!("[{}] Mode: {}", node_id, mode);
    println!("[{}] Backend: {}", node_id, backend_type);
    println!("[{}] Node Type: {}", node_id, node_type);
    println!("[{}] Update Rate: {}ms", node_id, update_rate_ms);
    println!(
        "[{}] CAP Filtering: {}",
        node_id,
        if cap_filter_enabled {
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

    // Get sync engine once (don't create multiple Arcs)
    let sync_engine = backend.sync_engine();

    // Create subscription for the test collection
    // Use capability-filtered query if CAP filtering is enabled
    println!("[{}] Creating sync subscription...", node_id);
    let subscription_query = if cap_filter_enabled {
        if hierarchical_mode && std::env::var("ROLE").unwrap_or_default() == "platoon_leader" {
            // Platoon leaders ONLY subscribe to squad_summaries, not individual NodeStates
            println!(
                "[{}]   → Subscribing to squad_summaries (hierarchical mode)",
                node_id
            );
            Query::Custom("collection_name == 'squad_summaries'".to_string())
        } else {
            // Existing CAP-filtered query for soldiers and squad leaders
            println!(
                "[{}]   → Using CAP-filtered query for role: {}",
                node_id, node_type
            );
            Query::Custom(format!(
                "public == true OR CONTAINS(authorized_roles, '{}')",
                node_type
            ))
        }
    } else {
        // Full replication mode: Subscribe to all documents (current behavior)
        println!("[{}]   → Using full replication (Query::All)", node_id);
        Query::All
    };
    let _subscription = sync_engine
        .subscribe("sim_poc", &subscription_query)
        .await?;
    println!("[{}] ✓ Sync subscription created", node_id);

    // Start sync (on the same sync_engine instance)
    // Peer discovery happens automatically when sync starts
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
                            let store_clone = Arc::clone(&ditto_store);
                            let node_id_clone = node_id.clone();

                            println!(
                                "[{}] → Squad: {}, Members: {:?}",
                                node_id, squad_id, member_ids
                            );

                            tokio::spawn(async move {
                                if let Err(e) = squad_leader_aggregation_loop(
                                    store_clone,
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
                            let store_clone = Arc::clone(&ditto_store);
                            let node_id_clone = node_id.clone();

                            println!("[{}] → Platoon: {}", node_id, platoon_id);

                            tokio::spawn(async move {
                                if let Err(e) = platoon_leader_aggregation_loop(
                                    change_stream,
                                    store_clone,
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
            println!("[{}] ✓✓✓ POC SUCCESS ✓✓✓", node_id);
            // Shutdown gracefully
            backend.shutdown().await?;
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[{}] ✗✗✗ POC FAILED: {} ✗✗✗", node_id, e);
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
        // Future: "automerge" => Ok(Box::new(AutomergeBackend::new())),
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
    let persistence_dir = PathBuf::from(format!("/tmp/cap_sim_{}", node_id));
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
        "automerge" => {
            // Automerge doesn't need app_id or shared_key
            BackendConfig {
                app_id: format!("cap_sim_{}", node_id),
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

        let document = Document::with_id(doc_id.clone(), fields.clone());

        // Calculate approximate message size (JSON serialization)
        let message_json = serde_json::to_string(&fields)?;
        let message_size_bytes = message_json.len();

        // Insert/update document
        backend.document_store().upsert("sim_poc", document).await?;

        println!(
            "[{}] ✓ Update #{} sent ({} bytes)",
            node_id, message_number, message_size_bytes
        );

        // Log metrics for first message (for backward compatibility)
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
        Value::String("Hello from CAP Simulation!".to_string()),
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

    // Subscribe to all documents in the collection (already done in main, but get the observer)
    let mut change_stream = backend.document_store().observe("sim_poc", &Query::All)?;

    // Track which periodic updates we've received
    let mut received_updates = HashSet::new();
    let mut test_doc_received = false;

    let timeout = Duration::from_secs(20);
    let start = Instant::now();

    // Listen for document changes via observer
    loop {
        // Check timeout
        if start.elapsed() > timeout {
            if !test_doc_received {
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
                                &mut test_doc_received,
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
                            &mut test_doc_received,
                        )
                        .await?;

                        // Continue running to maintain connection for ack pattern
                        // Readers should stay alive until the test timeout
                    }
                    ChangeEvent::Removed { .. } => {
                        // Ignore removals for this test
                    }
                }
            }
            Ok(None) => {
                // Channel closed
                return Err("Change stream closed unexpectedly".into());
            }
            Err(_) => {
                // Timeout waiting for event - continue loop to check overall timeout
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
    test_doc_received: &mut bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let received_at_us = now_micros();

    // Extract document ID
    let doc_id = doc.id.as_ref().ok_or("Document missing ID")?;

    // Extract timestamp
    let inserted_at_us = if let Some(ts_value) = doc.get("timestamp_us") {
        ts_value.as_u64().unwrap_or(0) as u128
    } else {
        0
    };

    let latency_us = if inserted_at_us > 0 {
        received_at_us.saturating_sub(inserted_at_us)
    } else {
        0
    };
    let latency_ms = latency_us as f64 / 1000.0;

    // Check if this is a periodic update document
    if doc_id.starts_with("sim_doc_soldier") {
        // Extract message number to track unique updates
        if let Some(msg_num_value) = doc.get("message_number") {
            let msg_num = msg_num_value.as_u64().unwrap_or(0);

            // Only log if this is a new update we haven't seen
            if !received_updates.contains(&msg_num) {
                received_updates.insert(msg_num);

                println!(
                    "[{}] ✓ Periodic update #{} received (latency: {:.3}ms)",
                    node_id, msg_num, latency_ms
                );

                // Log per-update latency metrics
                log_metrics(&MetricsEvent::DocumentReceived {
                    node_id: node_id.to_string(),
                    doc_id: format!("{}_msg{}", doc_id, msg_num),
                    inserted_at_us,
                    received_at_us,
                    latency_us,
                    latency_ms,
                });
            }
        }
    }
    // Check if this is the test document
    else if doc_id == "sim_test_001" && !*test_doc_received {
        *test_doc_received = true;

        println!(
            "[{}] ✓ Test document received (latency: {:.3}ms)",
            node_id, latency_ms
        );

        // Verify content
        if let Some(Value::String(message)) = doc.get("message") {
            if message == "Hello from CAP Simulation!" {
                println!("[{}] ✓ Document content verified", node_id);

                // Log test document metrics
                log_metrics(&MetricsEvent::DocumentReceived {
                    node_id: node_id.to_string(),
                    doc_id: "sim_test_001".to_string(),
                    inserted_at_us,
                    received_at_us,
                    latency_us,
                    latency_ms,
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

                                // Create updated document with new acked_by array
                                let mut updated_fields = HashMap::new();
                                for (k, v) in current_doc.fields.iter() {
                                    updated_fields.insert(k.clone(), v.clone());
                                }
                                updated_fields
                                    .insert("acked_by".to_string(), serde_json::json!(acked_by));

                                let updated_doc = Document {
                                    id: Some("sim_test_001".to_string()),
                                    fields: updated_fields,
                                    updated_at: current_doc.updated_at,
                                };

                                // Update the document
                                backend
                                    .document_store()
                                    .upsert("sim_poc", updated_doc)
                                    .await?;

                                println!(
                                    "[{}] ✓ Acknowledgment sent (acked_by count: {})",
                                    node_id,
                                    acked_by.len()
                                );

                                // Log acknowledgment metrics
                                log_metrics(&MetricsEvent::DocumentAcknowledged {
                                    node_id: node_id.to_string(),
                                    doc_id: "sim_test_001".to_string(),
                                    timestamp_us: now_micros(),
                                });
                            } else {
                                println!("[{}] Already acknowledged this document", node_id);
                            }
                        }
                    }
                }
            }
        }
    }
    // Check if this is a squad summary document (Mode 4 hierarchical)
    else if doc_id.starts_with("squad-") {
        println!(
            "[{}] ✓ Squad summary received: {} (latency: {:.3}ms)",
            node_id, doc_id, latency_ms
        );

        // Log squad summary reception metrics
        log_metrics(&MetricsEvent::DocumentReceived {
            node_id: node_id.to_string(),
            doc_id: doc_id.to_string(),
            inserted_at_us,
            received_at_us,
            latency_us,
            latency_ms,
        });
    }

    Ok(())
}
