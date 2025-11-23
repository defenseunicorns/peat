//! P2P Full Mesh Baseline - Direct Peer-to-Peer Pattern (Lab 3)
//!
//! Tests peer-to-peer mesh scalability with full connectivity.
//!
//! # Architecture
//!
//! ```
//! A ↔ B
//! ↕ ✕ ↕
//! C ↔ D
//! ```
//!
//! - **Peers**: Each maintains connections to ALL other peers
//! - **Connections**: n×(n-1) = O(n²) connections
//! - **Propagation**: Direct peer-to-peer (no hub)
//!
//! # Expected Breaking Point
//!
//! Connection explosion at ~10-50 nodes due to:
//! - O(n²) connection overhead
//! - Per-connection CPU cost
//! - File descriptor limits
//!
//! # Command Line Arguments
//!
//! --node-id <id>              Node identifier (e.g., "peer-1")
//! --listen-port <port>        Port for incoming connections
//! --peers <list>              Comma-separated peer addresses (e.g., "peer-2:12345,peer-3:12345")
//! --update-frequency <secs>   Update period in seconds (default: 5)
//!
//! # Example Usage
//!
//! Peer 1:
//! ```bash
//! p2p_mesh_baseline --node-id peer-1 --listen-port 12345 --peers peer-2:12345,peer-3:12345
//! ```
//!
//! Peer 2:
//! ```bash
//! p2p_mesh_baseline --node-id peer-2 --listen-port 12345 --peers peer-1:12345,peer-3:12345
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};

const TARGET_UPDATES: usize = 20; // Send 20 updates, then idle

/// P2P message (document update)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct P2PMessage {
    origin_node_id: String,
    origin_timestamp_us: u128,
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
    /// Peer sent update to all other peers
    P2PSend {
        node_id: String,
        timestamp_us: u128,
        sequence_number: u64,
        peer_count: usize,
        message_size_bytes: usize,
    },

    /// Peer received update from another peer
    P2PReceive {
        node_id: String,
        from_peer: String,
        timestamp_us: u128,
        latency_us: i128,
        latency_ms: f64,
        message_size_bytes: usize,
    },

    /// Peer connection stats (periodic)
    PeerStats {
        node_id: String,
        timestamp_us: u128,
        active_connections: usize,
        total_received: u64,
        total_sent: u64,
    },
}

/// Shared peer state
#[allow(dead_code)]
struct PeerState {
    node_id: String,
    documents: HashMap<String, Document>,
    outbound_peers: Vec<String>,
    active_connections: usize,
    total_received: u64,
    total_sent: u64,
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

    let listen_port: u16 = args
        .iter()
        .position(|x| x == "--listen-port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .expect("--listen-port required");

    let peers_str = args
        .iter()
        .position(|x| x == "--peers")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_default();

    let update_frequency = args
        .iter()
        .position(|x| x == "--update-frequency")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let peers: Vec<String> = if peers_str.is_empty() {
        vec![]
    } else {
        peers_str.split(',').map(|s| s.to_string()).collect()
    };

    run_peer(node_id, listen_port, peers, update_frequency).await
}

async fn run_peer(
    node_id: String,
    listen_port: u16,
    outbound_peers: Vec<String>,
    update_frequency: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "[{}] Starting P2P mesh peer on port {}, connecting to {} peers",
        node_id,
        listen_port,
        outbound_peers.len()
    );

    let state = Arc::new(RwLock::new(PeerState {
        node_id: node_id.clone(),
        documents: HashMap::new(),
        outbound_peers: outbound_peers.clone(),
        active_connections: 0,
        total_received: 0,
        total_sent: 0,
    }));

    // Start TCP listener for incoming connections
    let listener = TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await?;

    // Spawn inbound connection handler
    let state_clone = state.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    let state = state_clone.clone();
                    let node_id = node_id_clone.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_inbound(node_id, socket, state).await {
                            eprintln!("Error handling inbound from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                }
            }
        }
    });

    // Wait for peers to be ready
    sleep(Duration::from_secs(5)).await;

    // Connect to all outbound peers
    let mut outbound_streams = Vec::new();
    for peer_addr in &outbound_peers {
        match TcpStream::connect(peer_addr).await {
            Ok(stream) => {
                println!("[{}] Connected to peer {}", node_id, peer_addr);
                outbound_streams.push(stream);
                state.write().await.active_connections += 1;
            }
            Err(e) => {
                eprintln!("[{}] Failed to connect to {}: {}", node_id, peer_addr, e);
            }
        }
    }

    // Spawn stats reporter
    let state_clone = state.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(10));
        loop {
            ticker.tick().await;
            let state = state_clone.read().await;
            emit_metric(&MetricsEvent::PeerStats {
                node_id: node_id_clone.clone(),
                timestamp_us: current_time_us(),
                active_connections: state.active_connections,
                total_received: state.total_received,
                total_sent: state.total_sent,
            });
        }
    });

    // Send updates to all peers
    let doc_id = format!("{}-doc", node_id);
    let mut sequence = 0u64;

    for _ in 0..TARGET_UPDATES {
        sequence += 1;

        // Create document
        let doc = Document {
            doc_id: doc_id.clone(),
            content: format!("Update {} from {}", sequence, node_id),
            version: sequence,
            updated_at_us: current_time_us(),
        };

        // Create P2P message
        let msg = P2PMessage {
            origin_node_id: node_id.clone(),
            origin_timestamp_us: current_time_us(),
            sequence_number: sequence,
            documents: vec![doc],
        };

        let msg_bytes = serde_json::to_vec(&msg)?;
        let msg_len = msg_bytes.len() as u32;

        // Send to all outbound peers
        let mut send_count = 0;
        for stream in &mut outbound_streams {
            if let Err(e) = stream.write_all(&msg_len.to_be_bytes()).await {
                eprintln!("[{}] Write length error: {}", node_id, e);
                continue;
            }

            if let Err(e) = stream.write_all(&msg_bytes).await {
                eprintln!("[{}] Write message error: {}", node_id, e);
                continue;
            }

            send_count += 1;
        }

        // Emit send metric
        emit_metric(&MetricsEvent::P2PSend {
            node_id: node_id.clone(),
            timestamp_us: msg.origin_timestamp_us,
            sequence_number: sequence,
            peer_count: send_count,
            message_size_bytes: msg_bytes.len(),
        });

        state.write().await.total_sent += send_count as u64;

        println!("[{}] Sent update {}/{} to {} peers", node_id, sequence, TARGET_UPDATES, send_count);

        sleep(Duration::from_secs(update_frequency)).await;
    }

    println!("[{}] Completed {} updates, idling...", node_id, TARGET_UPDATES);

    // Keep running to receive from other peers
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}

async fn handle_inbound(
    node_id: String,
    mut socket: TcpStream,
    state: Arc<RwLock<PeerState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut length_buf = [0u8; 4];

    while socket.read_exact(&mut length_buf).await.is_ok() {
        let message_len = u32::from_be_bytes(length_buf) as usize;

        // Read message
        let mut message_buf = vec![0u8; message_len];
        socket.read_exact(&mut message_buf).await?;

        // Deserialize
        let msg: P2PMessage = serde_json::from_slice(&message_buf)?;
        let received_at_us = current_time_us();
        let latency = (received_at_us as i128) - (msg.origin_timestamp_us as i128);

        // Emit receive metric
        emit_metric(&MetricsEvent::P2PReceive {
            node_id: node_id.clone(),
            from_peer: msg.origin_node_id.clone(),
            timestamp_us: received_at_us,
            latency_us: latency,
            latency_ms: latency as f64 / 1000.0,
            message_size_bytes: message_len,
        });

        // Update state
        let mut state = state.write().await;
        state.total_received += 1;

        for doc in msg.documents {
            state.documents.insert(doc.doc_id.clone(), doc);
        }
    }

    Ok(())
}
