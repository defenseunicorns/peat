//! Producer-Only Baseline - Telemetry/Logging Pattern (Lab 1)
//!
//! Tests pure server ingress capacity without broadcast overhead.
//!
//! # Architecture
//!
//! ```
//! Clients (producers) → Server (aggregator)
//! ```
//!
//! - **Clients**: Send periodic updates to server (upload only)
//! - **Server**: Receive and aggregate (NO broadcast back to clients)
//! - **Metrics**: Upload throughput, server CPU, queue depth
//!
//! # Key Difference from Lab 2 (Full Replication)
//!
//! - **Lab 1 (this)**: O(n) ingress only - server receives from n clients
//! - **Lab 2**: O(n²) broadcast - server receives from n clients AND broadcasts to n-1 clients
//!
//! # Expected Result
//!
//! Server should handle more nodes than Lab 2 (500-1000 nodes vs 384-500 nodes)
//! because there's no O(n²) broadcast overhead.
//!
//! # Command Line Arguments
//!
//! --node-id <id>              Node identifier
//! --mode <mode>               "server" or "client"
//! --listen <addr>             Server: Listen address (e.g., "0.0.0.0:12345")
//! --connect <addr>            Client: Server address (e.g., "server:12345")
//! --update-frequency <secs>   Update period in seconds (default: 5)
//! --num-documents <n>         Number of documents (default: 1)
//!
//! # Example Usage
//!
//! Server:
//! ```bash
//! producer_only_baseline --node-id server --mode server --listen 0.0.0.0:12345
//! ```
//!
//! Client:
//! ```bash
//! producer_only_baseline --node-id client-1 --mode client --connect server:12345 --update-frequency 5
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

const TARGET_UPDATES: usize = 20; // Send 20 updates, then idle (server continues receiving)

/// Upload message from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadMessage {
    node_id: String,
    timestamp_us: u128,
    sequence_number: u64,
    documents: Vec<Document>,
}

/// Simple document
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Document {
    doc_id: String,
    content: String,
    version: u64,
    updated_at_us: u128,
}

/// Metrics events
#[derive(Debug, Serialize)]
#[serde(tag = "event_type")]
enum MetricsEvent {
    /// Client uploaded data to server
    Upload {
        node_id: String,
        timestamp_us: u128,
        message_size_bytes: usize,
        document_count: usize,
        sequence_number: u64,
    },

    /// Server received upload from client
    IngressReceived {
        node_id: String,
        from_client: String,
        timestamp_us: u128,
        latency_us: i128,
        latency_ms: f64,
        message_size_bytes: usize,
        document_count: usize,
    },

    /// Server aggregation stats (periodic)
    ServerStats {
        node_id: String,
        timestamp_us: u128,
        total_clients_seen: usize,
        total_messages_received: u64,
        total_documents: usize,
        queue_depth: usize,
    },
}

/// Shared server state
struct ServerState {
    documents: HashMap<String, Document>,
    clients_seen: HashMap<String, u64>, // client_id -> last_sequence
    total_messages: u64,
}

fn emit_metric(event: &MetricsEvent) {
    println!("METRICS: {}", serde_json::to_string(event).unwrap());
}

fn current_time_us() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let node_id = args
        .iter()
        .position(|x| x == "--node-id")
        .and_then(|i| args.get(i + 1))
        .expect("--node-id required")
        .clone();

    let mode = args
        .iter()
        .position(|x| x == "--mode")
        .and_then(|i| args.get(i + 1))
        .expect("--mode required (server or client)")
        .clone();

    match mode.as_str() {
        "server" => {
            let listen = args
                .iter()
                .position(|x| x == "--listen")
                .and_then(|i| args.get(i + 1))
                .expect("--listen required for server")
                .parse::<SocketAddr>()?;

            run_server(node_id, listen).await
        }
        "client" => {
            let connect = args
                .iter()
                .position(|x| x == "--connect")
                .and_then(|i| args.get(i + 1))
                .expect("--connect required for client")
                .clone();

            let update_frequency = args
                .iter()
                .position(|x| x == "--update-frequency")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse().ok())
                .unwrap_or(5);

            run_client(node_id, connect, update_frequency).await
        }
        _ => panic!("Mode must be 'server' or 'client'"),
    }
}

async fn run_server(
    node_id: String,
    listen_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Starting producer-only server on {}",
        node_id, listen_addr
    );

    let state = Arc::new(RwLock::new(ServerState {
        documents: HashMap::new(),
        clients_seen: HashMap::new(),
        total_messages: 0,
    }));

    let listener = TcpListener::bind(listen_addr).await?;

    // Spawn stats reporter
    let state_clone = state.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(10));
        loop {
            ticker.tick().await;
            let state = state_clone.read().await;
            emit_metric(&MetricsEvent::ServerStats {
                node_id: node_id_clone.clone(),
                timestamp_us: current_time_us(),
                total_clients_seen: state.clients_seen.len(),
                total_messages_received: state.total_messages,
                total_documents: state.documents.len(),
                queue_depth: 0, // TODO: Add queue depth tracking if needed
            });
        }
    });

    // Accept connections
    loop {
        let (socket, addr) = listener.accept().await?;
        let state = state.clone();
        let node_id = node_id.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_client(node_id, socket, addr, state).await {
                eprintln!("Error handling client {}: {}", addr, e);
            }
        });
    }
}

async fn handle_client(
    node_id: String,
    mut socket: TcpStream,
    _addr: SocketAddr,
    state: Arc<RwLock<ServerState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut length_buf = [0u8; 4];

    while socket.read_exact(&mut length_buf).await.is_ok() {
        let message_len = u32::from_be_bytes(length_buf) as usize;

        // Read message
        let mut message_buf = vec![0u8; message_len];
        socket.read_exact(&mut message_buf).await?;

        // Deserialize
        let msg: UploadMessage = serde_json::from_slice(&message_buf)?;
        let received_at_us = current_time_us();
        let latency = (received_at_us as i128) - (msg.timestamp_us as i128);

        // Emit ingress metric
        emit_metric(&MetricsEvent::IngressReceived {
            node_id: node_id.clone(),
            from_client: msg.node_id.clone(),
            timestamp_us: received_at_us,
            latency_us: latency,
            latency_ms: latency as f64 / 1000.0,
            message_size_bytes: message_len,
            document_count: msg.documents.len(),
        });

        // Update server state (aggregate)
        let mut state = state.write().await;
        state.total_messages += 1;
        state
            .clients_seen
            .insert(msg.node_id.clone(), msg.sequence_number);

        for doc in msg.documents {
            state.documents.insert(doc.doc_id.clone(), doc);
        }
    }

    Ok(())
}

async fn run_client(
    node_id: String,
    server_addr: String,
    update_frequency: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] Connecting to server at {}", node_id, server_addr);

    // Wait for server to be ready
    sleep(Duration::from_secs(5)).await;

    let mut stream = TcpStream::connect(&server_addr).await?;
    println!("[{}] Connected to server", node_id);

    let mut sequence = 0u64;
    let doc_id = format!("{}-doc", node_id);

    for _ in 0..TARGET_UPDATES {
        sequence += 1;

        // Create document
        let doc = Document {
            doc_id: doc_id.clone(),
            content: format!("Update {} from {}", sequence, node_id),
            version: sequence,
            updated_at_us: current_time_us(),
        };

        // Create upload message
        let msg = UploadMessage {
            node_id: node_id.clone(),
            timestamp_us: current_time_us(),
            sequence_number: sequence,
            documents: vec![doc],
        };

        // Serialize
        let msg_bytes = serde_json::to_vec(&msg)?;
        let msg_len = msg_bytes.len() as u32;

        // Emit upload metric
        emit_metric(&MetricsEvent::Upload {
            node_id: node_id.clone(),
            timestamp_us: msg.timestamp_us,
            message_size_bytes: msg_bytes.len(),
            document_count: 1,
            sequence_number: sequence,
        });

        // Send length prefix + message
        stream.write_all(&msg_len.to_be_bytes()).await?;
        stream.write_all(&msg_bytes).await?;

        println!("[{}] Sent update {}/{}", node_id, sequence, TARGET_UPDATES);

        // Wait before next update
        sleep(Duration::from_secs(update_frequency)).await;
    }

    println!(
        "[{}] Completed {} updates, idling...",
        node_id, TARGET_UPDATES
    );

    // Keep connection alive but idle
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
