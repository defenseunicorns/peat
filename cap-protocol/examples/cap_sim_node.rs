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
use cap_protocol::sync::{BackendConfig, DataSyncBackend, Document, Query, TransportConfig, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Test document structure
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestDoc {
    id: String,
    message: String,
    timestamp: u64,
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
            _ => {}
        }
        i += 1;
    }

    let node_id = node_id.expect("--node-id required");
    let mode = mode.expect("--mode required");
    let backend_type = backend_type.unwrap_or_else(|| "ditto".to_string());

    println!("[{}] CAP Network Simulation Node starting", node_id);
    println!("[{}] Mode: {}", node_id, mode);
    println!("[{}] Backend: {}", node_id, backend_type);

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
    println!("[{}] Creating sync subscription...", node_id);
    let _subscription = sync_engine.subscribe("sim_poc", &Query::All).await?;
    println!("[{}] ✓ Sync subscription created", node_id);

    // Start sync (on the same sync_engine instance)
    // Peer discovery happens automatically when sync starts
    println!("[{}] Starting sync...", node_id);
    sync_engine.start_sync().await?;
    println!("[{}] ✓ Sync started", node_id);

    // Wait a moment for peer discovery
    println!("[{}] Waiting for peer discovery (5s)...", node_id);
    sleep(Duration::from_secs(5)).await;

    // Execute mode-specific behavior
    let result = match mode.as_str() {
        "writer" => writer_mode(&*backend, &node_id).await,
        "reader" => reader_mode(&*backend, &node_id).await,
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

/// Writer mode: Create a test document
async fn writer_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === WRITER MODE ===", node_id);

    // Create test document
    let doc = TestDoc {
        id: "sim_test_001".to_string(),
        message: "Hello from CAP Simulation!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    println!("[{}] Creating test document: {:?}", node_id, doc);

    // Convert to Document
    let mut fields = HashMap::new();
    fields.insert("message".to_string(), Value::String(doc.message.clone()));
    fields.insert("timestamp".to_string(), Value::Number(doc.timestamp.into()));
    fields.insert("created_by".to_string(), Value::String(node_id.to_string()));

    let document = Document::with_id(doc.id, fields);

    // Insert via trait
    backend.document_store().upsert("sim_poc", document).await?;

    println!("[{}] ✓ Document inserted", node_id);

    // Wait for sync to propagate
    println!("[{}] Waiting for sync propagation (10s)...", node_id);
    sleep(Duration::from_secs(10)).await;

    println!("[{}] Writer complete", node_id);
    Ok(())
}

/// Reader mode: Wait for document to arrive
async fn reader_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === READER MODE ===", node_id);

    let timeout = Duration::from_secs(20);
    let start = Instant::now();

    println!(
        "[{}] Waiting for test document (timeout: {:?})...",
        node_id, timeout
    );

    // Poll for document
    loop {
        if start.elapsed() > timeout {
            return Err("Timeout: Document not received".into());
        }

        // Query for test document using trait
        let query = Query::Eq {
            field: "_id".to_string(),
            value: Value::String("sim_test_001".to_string()),
        };

        let docs = backend.document_store().query("sim_poc", &query).await?;

        if let Some(doc) = docs.first() {
            println!("[{}] ✓ Document received!", node_id);

            // Verify document contents
            if let Some(Value::String(message)) = doc.get("message") {
                println!("[{}] Message: {}", node_id, message);
                if message == "Hello from CAP Simulation!" {
                    println!("[{}] ✓ Document content verified", node_id);
                    return Ok(());
                }
            }
        }

        // Wait before next poll
        sleep(Duration::from_millis(500)).await;
        print!(".");
        use std::io::Write;
        std::io::stdout().flush()?;
    }
}
