//! Traditional IoT Baseline - Event-Driven Full State Messaging
//!
//! This is a NON-CRDT baseline implementation for comparing HIVE Protocol performance
//! against traditional IoT architectures.
//!
//! # Architecture
//!
//! - **NO CRDT**: No delta-state sync, no automatic convergence
//! - **Periodic Transmission**: Full state messages sent at configurable frequency
//! - **Last-Write-Wins**: Receiver overwrites state with latest message
//! - **Simple TCP**: Client-server or hub-spoke topology (no mesh to avoid n-squared)
//!
//! # Purpose
//!
//! Provides baseline for three-way architectural comparison:
//! 1. Traditional IoT Baseline (this) - Full messages, periodic
//! 2. CAP Full Replication - CRDT delta sync, Query::All
//! 3. CAP Differential Filtering - CRDT delta sync, capability-filtered
//!
//! # Command Line Arguments
//!
//! --node-id <id>              Node identifier (e.g., "soldier-1", "soldier-2")
//! --mode <mode>               "server" or "client"
//! --listen <addr>             Server mode: Listen address (e.g., "0.0.0.0:12345")
//! --connect <addr>            Client mode: Server address (e.g., "soldier-1:12345")
//! --update-frequency <secs>   Transmission period in seconds (default: 5)
//! --num-documents <n>         Number of documents to create (default: 1)
//! --node-type <type>          Node type for metrics (e.g., "soldier", "uav")
//!
//! # Example Usage
//!
//! Server (writer):
//! ```bash
//! traditional_baseline --node-id soldier-1 --mode server --listen 0.0.0.0:12345 --update-frequency 5
//! ```
//!
//! Client (reader):
//! ```bash
//! traditional_baseline --node-id soldier-2 --mode client --connect soldier-1:12345 --update-frequency 5
//! ```
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};

/// Full state message - contains ENTIRE node state (no deltas, no CRDT)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FullStateMessage {
    /// Node identifier
    node_id: String,

    /// Timestamp when message was created (microseconds since UNIX epoch)
    timestamp_us: u128,

    /// Sequence number for this node's messages
    sequence_number: u64,

    /// Complete collection of documents (FULL STATE, not deltas)
    documents: Vec<SimpleDocument>,

    /// Total message size in bytes (for metrics)
    #[serde(skip)]
    message_size_bytes: usize,
}

/// Simple document without CRDT metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimpleDocument {
    /// Document identifier
    doc_id: String,

    /// Document content (application data)
    content: String,

    /// Version number (incremented on each update)
    version: u64,

    /// Last update timestamp (microseconds since UNIX epoch)
    updated_at_us: u128,
}

/// Metrics events for JSON logging (same format as CAP tests)
#[derive(Debug, Serialize)]
#[serde(tag = "event_type")]
enum MetricsEvent {
    DocumentInserted {
        node_id: String,
        doc_id: String,
        timestamp_us: u128,
    },
    MessageSent {
        node_id: String,
        node_type: String,
        message_number: u64,
        message_size_bytes: usize,
        timestamp_us: u128,
    },
    MessageReceived {
        node_id: String,
        from_node_id: String,
        message_size_bytes: usize,
        latency_us: i128,
        timestamp_us: u128,
    },
    DocumentReceived {
        node_id: String,
        doc_id: String,
        inserted_at_us: u128,
        received_at_us: u128,
        latency_us: i128,
        latency_ms: f64,
    },
}

/// Shared node state
#[derive(Clone)]
struct NodeState {
    node_id: String,
    node_type: String,
    documents: Arc<RwLock<HashMap<String, SimpleDocument>>>,
    sequence_number: Arc<RwLock<u64>>,
    update_frequency: Duration,
}

impl NodeState {
    fn new(node_id: String, node_type: String, update_frequency: Duration) -> Self {
        Self {
            node_id,
            node_type,
            documents: Arc::new(RwLock::new(HashMap::new())),
            sequence_number: Arc::new(RwLock::new(0)),
            update_frequency,
        }
    }

    /// Create a new document
    async fn create_document(&self, doc_id: String, content: String) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();

        let doc = SimpleDocument {
            doc_id: doc_id.clone(),
            content,
            version: 1,
            updated_at_us: now,
        };

        self.documents.write().await.insert(doc_id.clone(), doc);

        // Emit metric
        emit_metric(&MetricsEvent::DocumentInserted {
            node_id: self.node_id.clone(),
            doc_id,
            timestamp_us: now,
        });
    }

    /// Get current sequence number and increment
    async fn next_sequence_number(&self) -> u64 {
        let mut seq = self.sequence_number.write().await;
        let current = *seq;
        *seq += 1;
        current
    }

    /// Create a full state message
    async fn create_full_state_message(&self) -> FullStateMessage {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();

        let documents: Vec<SimpleDocument> =
            self.documents.read().await.values().cloned().collect();
        let seq = self.next_sequence_number().await;

        FullStateMessage {
            node_id: self.node_id.clone(),
            timestamp_us: now,
            sequence_number: seq,
            documents,
            message_size_bytes: 0, // Will be set after serialization
        }
    }

    /// Apply received state (simple overwrite - last write wins)
    async fn apply_received_state(&self, message: FullStateMessage) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();

        let latency = (now as i128) - (message.timestamp_us as i128);

        // Emit message received metric
        emit_metric(&MetricsEvent::MessageReceived {
            node_id: self.node_id.clone(),
            from_node_id: message.node_id.clone(),
            message_size_bytes: message.message_size_bytes,
            latency_us: latency,
            timestamp_us: now,
        });

        let mut docs = self.documents.write().await;

        // Overwrite all documents (last-write-wins, no CRDT merge)
        for received_doc in message.documents {
            // Check if this is a new document for us
            let is_new = !docs.contains_key(&received_doc.doc_id);

            if is_new {
                // Emit document received metric
                emit_metric(&MetricsEvent::DocumentReceived {
                    node_id: self.node_id.clone(),
                    doc_id: received_doc.doc_id.clone(),
                    inserted_at_us: received_doc.updated_at_us,
                    received_at_us: now,
                    latency_us: (now as i128) - (received_doc.updated_at_us as i128),
                    latency_ms: ((now as i128) - (received_doc.updated_at_us as i128)) as f64
                        / 1000.0,
                });

                println!(
                    "[{}] ✓ Document received: {} (latency: {:.1}ms)",
                    self.node_id,
                    received_doc.doc_id,
                    ((now as i128) - (received_doc.updated_at_us as i128)) as f64 / 1000.0
                );
            }

            // Simple overwrite (no merge logic)
            docs.insert(received_doc.doc_id.clone(), received_doc);
        }
    }
}

/// Emit metric as JSON to stdout
fn emit_metric(event: &MetricsEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        println!("METRICS: {}", json);
    }
}

/// Server mode: Listen for connections and broadcast full state periodically
async fn run_server(
    listen_addr: SocketAddr,
    state: NodeState,
    num_documents: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === SERVER MODE ===", state.node_id);
    println!("[{}] Listening on {}", state.node_id, listen_addr);
    println!(
        "[{}] Update frequency: {:?}",
        state.node_id, state.update_frequency
    );

    // Create initial documents
    for i in 0..num_documents {
        let doc_id = format!("sim_doc_{}_{}", state.node_id, i);
        let content = format!("Document {} from {}", i, state.node_id);
        state.create_document(doc_id.clone(), content).await;
        println!("[{}] Created document: {}", state.node_id, doc_id);
    }

    // Also create the standard test document for compatibility
    let test_doc_id = "sim_test_001".to_string();
    state
        .create_document(
            test_doc_id.clone(),
            format!("Test document from {}", state.node_id),
        )
        .await;
    println!("[{}] Created test document: {}", state.node_id, test_doc_id);

    let listener = TcpListener::bind(listen_addr).await?;
    let clients = Arc::new(RwLock::new(Vec::<TcpStream>::new()));

    // Spawn connection acceptor task
    let clients_clone = clients.clone();
    let node_id = state.node_id.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("[{}] Client connected: {}", node_id, addr);
                    clients_clone.write().await.push(stream);
                }
                Err(e) => {
                    eprintln!("[{}] Error accepting connection: {}", node_id, e);
                }
            }
        }
    });

    // Spawn periodic broadcast task
    let mut ticker = interval(state.update_frequency);
    loop {
        ticker.tick().await;

        // Create full state message
        let mut message = state.create_full_state_message().await;

        // Serialize message
        let serialized = match serde_json::to_vec(&message) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("[{}] Error serializing message: {}", state.node_id, e);
                continue;
            }
        };

        message.message_size_bytes = serialized.len();

        // Emit metric
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros();
        emit_metric(&MetricsEvent::MessageSent {
            node_id: state.node_id.clone(),
            node_type: state.node_type.clone(),
            message_number: message.sequence_number,
            message_size_bytes: message.message_size_bytes,
            timestamp_us: now,
        });

        // Broadcast to all connected clients
        let mut clients_lock = clients.write().await;
        let mut disconnected_indices = Vec::new();

        for (i, client) in clients_lock.iter_mut().enumerate() {
            // Send message length first (4 bytes)
            let len_bytes = (serialized.len() as u32).to_be_bytes();
            if client.write_all(&len_bytes).await.is_err() {
                disconnected_indices.push(i);
                continue;
            }

            // Send message data
            if client.write_all(&serialized).await.is_err() {
                disconnected_indices.push(i);
                continue;
            }
        }

        // Remove disconnected clients
        for &i in disconnected_indices.iter().rev() {
            clients_lock.remove(i);
            println!("[{}] Client disconnected", state.node_id);
        }

        if !clients_lock.is_empty() {
            println!(
                "[{}] Broadcast full state to {} clients ({} bytes, seq {})",
                state.node_id,
                clients_lock.len(),
                message.message_size_bytes,
                message.sequence_number
            );
        }
    }
}

/// Client mode: Connect to server, send periodic state, receive state
async fn run_client(
    server_addr: String,
    state: NodeState,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === CLIENT MODE ===", state.node_id);
    println!("[{}] Connecting to server: {}", state.node_id, server_addr);
    println!(
        "[{}] Update frequency: {:?}",
        state.node_id, state.update_frequency
    );

    // Connect to server
    let stream = loop {
        match TcpStream::connect(&server_addr).await {
            Ok(s) => {
                println!("[{}] ✓ Connected to server", state.node_id);
                break s;
            }
            Err(e) => {
                eprintln!(
                    "[{}] Connection failed: {}, retrying in 2s...",
                    state.node_id, e
                );
                sleep(Duration::from_secs(2)).await;
            }
        }
    };

    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn send task (periodic full state transmission)
    let state_clone = state.clone();
    let send_task = tokio::spawn(async move {
        let mut ticker = interval(state_clone.update_frequency);
        loop {
            ticker.tick().await;

            // Create full state message
            let mut message = state_clone.create_full_state_message().await;

            // Serialize
            let serialized = match serde_json::to_vec(&message) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("[{}] Error serializing message: {}", state_clone.node_id, e);
                    continue;
                }
            };

            message.message_size_bytes = serialized.len();

            // Emit metric
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros();
            emit_metric(&MetricsEvent::MessageSent {
                node_id: state_clone.node_id.clone(),
                node_type: state_clone.node_type.clone(),
                message_number: message.sequence_number,
                message_size_bytes: message.message_size_bytes,
                timestamp_us: now,
            });

            // Send length prefix
            let len_bytes = (serialized.len() as u32).to_be_bytes();
            if let Err(e) = write_half.write_all(&len_bytes).await {
                eprintln!("[{}] Error sending length: {}", state_clone.node_id, e);
                break;
            }

            // Send data
            if let Err(e) = write_half.write_all(&serialized).await {
                eprintln!("[{}] Error sending data: {}", state_clone.node_id, e);
                break;
            }

            println!(
                "[{}] Sent full state to server ({} bytes, seq {})",
                state_clone.node_id, message.message_size_bytes, message.sequence_number
            );
        }
    });

    // Spawn receive task
    let state_clone = state.clone();
    let receive_task = tokio::spawn(async move {
        loop {
            // Read message length (4 bytes)
            let mut len_bytes = [0u8; 4];
            if let Err(e) = read_half.read_exact(&mut len_bytes).await {
                eprintln!("[{}] Error reading length: {}", state_clone.node_id, e);
                break;
            }

            let msg_len = u32::from_be_bytes(len_bytes) as usize;

            // Read message data
            let mut buffer = vec![0u8; msg_len];
            if let Err(e) = read_half.read_exact(&mut buffer).await {
                eprintln!("[{}] Error reading data: {}", state_clone.node_id, e);
                break;
            }

            // Deserialize
            let mut message: FullStateMessage = match serde_json::from_slice(&buffer) {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!(
                        "[{}] Error deserializing message: {}",
                        state_clone.node_id, e
                    );
                    continue;
                }
            };

            message.message_size_bytes = buffer.len();

            println!(
                "[{}] Received full state from {} ({} bytes, {} docs)",
                state_clone.node_id,
                message.node_id,
                message.message_size_bytes,
                message.documents.len()
            );

            // Apply received state
            state_clone.apply_received_state(message).await;
        }
    });

    // Wait for either task to complete (or fail)
    tokio::select! {
        _ = send_task => {
            eprintln!("[{}] Send task terminated", state.node_id);
        }
        _ = receive_task => {
            eprintln!("[{}] Receive task terminated", state.node_id);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    let mut node_id = "node1".to_string();
    let mut mode = "client".to_string();
    let mut listen_addr = "0.0.0.0:12345".to_string();
    let mut server_addr = "localhost:12345".to_string();
    let mut update_frequency_secs = 5.0;
    let mut num_documents = 1;
    let mut node_type = "unknown".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--node-id" => {
                node_id = args[i + 1].clone();
                i += 2;
            }
            "--mode" => {
                mode = args[i + 1].clone();
                i += 2;
            }
            "--listen" => {
                listen_addr = args[i + 1].clone();
                i += 2;
            }
            "--connect" => {
                server_addr = args[i + 1].clone();
                i += 2;
            }
            "--update-frequency" => {
                update_frequency_secs = args[i + 1].parse().unwrap_or(5.0);
                i += 2;
            }
            "--num-documents" => {
                num_documents = args[i + 1].parse().unwrap_or(1);
                i += 2;
            }
            "--node-type" => {
                node_type = args[i + 1].clone();
                i += 2;
            }
            _ => i += 1,
        }
    }

    let update_frequency = Duration::from_secs_f64(update_frequency_secs);
    let state = NodeState::new(node_id.clone(), node_type, update_frequency);

    println!("========================================");
    println!("Traditional IoT Baseline (NO CRDT)");
    println!("========================================");
    println!("Node ID: {}", node_id);
    println!("Mode: {}", mode);
    println!("Update Frequency: {}s", update_frequency_secs);
    println!();

    match mode.as_str() {
        "server" => {
            let addr: SocketAddr = listen_addr.parse()?;
            run_server(addr, state, num_documents).await?;
        }
        "client" => {
            run_client(server_addr, state).await?;
        }
        _ => {
            eprintln!("Invalid mode: {}. Use 'server' or 'client'", mode);
            std::process::exit(1);
        }
    }

    Ok(())
}
