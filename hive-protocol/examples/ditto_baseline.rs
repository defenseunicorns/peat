//! Ditto Baseline - Pure Ditto Performance Without HIVE Protocol
//!
//! This binary provides baseline Ditto performance metrics without HIVE Protocol overhead.
//! Used for comparison testing to measure CAP's performance impact.
//!
//! # What This Tests
//!
//! - Pure Ditto SDK performance (no CAP overhead)
//! - Document creation and replication baseline
//! - CRDT sync performance without capability checks
//! - Baseline metrics for HIVE Protocol comparison
//!
//! # Architecture
//!
//! ContainerLab runs multiple instances of this binary:
//! - Writer nodes: Create test documents
//! - Reader nodes: Wait to receive documents
//! - Uses raw Ditto SDK without HIVE Protocol layer
//!
//! # Success Criteria
//!
//! - Ditto instances start successfully
//! - Peers discover each other
//! - Document syncs from writer to readers
//! - Provides baseline performance metrics
//! - Comparable with cap_sim_node results
//!
//! # Command Line Arguments
//!
//! --node-id <id>         Node identifier (e.g., "node1", "node2")
//! --mode <mode>          "writer" (creates document) or "reader" (waits for document)
//! --tcp-listen <port>    Optional: Listen for TCP connections on this port
//! --tcp-connect <addr>   Optional: Connect to TCP peer at this address (e.g., "11.0.0.1:12345")
//!
//! # Exit Codes
//!
//! 0: Success (document synced)
//! 1: Failure (timeout, error, or document not received)
//!
//! # Environment Variables (Required)
//!
//! - DITTO_APP_ID: Application ID from Ditto portal
//! - DITTO_OFFLINE_TOKEN: Offline license token
//! - DITTO_SHARED_KEY: Shared encryption key

use dittolive_ditto::prelude::*;
use dittolive_ditto::AppId;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Test document structure
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestDoc {
    id: String,
    message: String,
    timestamp: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Parse args into a simple structure
    let mut node_id = None;
    let mut mode = None;
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

    println!("[{}] Shadow + Ditto POC starting", node_id);
    println!("[{}] Mode: {}", node_id, mode);

    if let Some(port) = tcp_listen_port {
        println!("[{}] TCP: Will listen on port {}", node_id, port);
    }
    if let Some(ref addr) = tcp_connect_addr {
        println!("[{}] TCP: Will connect to {}", node_id, addr);
    }

    // Initialize Ditto
    println!("[{}] Initializing Ditto...", node_id);
    let ditto = match create_ditto_instance(&node_id, tcp_listen_port, tcp_connect_addr) {
        Ok(d) => {
            println!("[{}] ✓ Ditto initialized", node_id);
            d
        }
        Err(e) => {
            eprintln!("[{}] ✗ Failed to initialize Ditto: {}", node_id, e);
            std::process::exit(1);
        }
    };

    // Create subscription for peer discovery
    // Peers only discover each other when they have common subscriptions
    println!("[{}] Creating subscription...", node_id);
    let _subscription = ditto.store().collection("baseline")?.find_all().subscribe();
    println!("[{}] ✓ Subscription created", node_id);

    // Start sync
    println!("[{}] Starting sync...", node_id);
    if let Err(e) = ditto.start_sync() {
        eprintln!("[{}] ✗ Failed to start sync: {}", node_id, e);
        std::process::exit(1);
    }
    println!("[{}] ✓ Sync started", node_id);

    // Wait a moment for peer discovery
    println!("[{}] Waiting for peer discovery (5s)...", node_id);
    sleep(Duration::from_secs(5));

    // Execute mode-specific behavior
    let result = match mode.as_str() {
        "writer" => writer_mode(&ditto, &node_id),
        "reader" => reader_mode(&ditto, &node_id),
        _ => {
            eprintln!("[{}] ✗ Invalid mode: {}", node_id, mode);
            std::process::exit(1);
        }
    };

    match result {
        Ok(()) => {
            println!("[{}] ✓✓✓ POC SUCCESS ✓✓✓", node_id);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[{}] ✗✗✗ POC FAILED: {} ✗✗✗", node_id, e);
            std::process::exit(1);
        }
    }
}

/// Create and configure a Ditto instance
fn create_ditto_instance(
    node_id: &str,
    tcp_listen_port: Option<u16>,
    tcp_connect_addr: Option<String>,
) -> Result<Ditto, Box<dyn std::error::Error>> {
    // Load environment variables
    let offline_token = std::env::var("DITTO_OFFLINE_TOKEN")?;
    let shared_key = std::env::var("DITTO_SHARED_KEY")?;

    // Create persistence directory
    let persistence_dir = format!("/tmp/ditto_baseline_{}", node_id);
    std::fs::create_dir_all(&persistence_dir)?;

    // Step 1: Create persistent root
    let root = Arc::new(PersistentRoot::new(&persistence_dir)?);

    // Step 2: Create Ditto with SharedKey identity
    let ditto = Ditto::builder()
        .with_root(root)
        .with_identity(|ditto_root| {
            let app_id = AppId::from_env("DITTO_APP_ID")?;
            identity::SharedKey::new(ditto_root, app_id, shared_key.trim())
        })?
        .build()?;

    // Step 3: Activate with offline token
    ditto.set_offline_only_license_token(&offline_token)?;

    // Step 4: Configure transports
    ditto.update_transport_config(|transport_config| {
        // Disable BLE and HTTP
        transport_config.peer_to_peer.bluetooth_le.enabled = false;
        transport_config.listen.http.enabled = false;

        // If TCP config provided, use explicit TCP transport
        if tcp_listen_port.is_some() || tcp_connect_addr.is_some() {
            // Disable mDNS/LAN when using explicit TCP
            transport_config.peer_to_peer.lan.enabled = false;

            // Configure TCP listener if specified
            if let Some(port) = tcp_listen_port {
                transport_config.listen.tcp.enabled = true;
                transport_config.listen.tcp.interface_ip = "0.0.0.0".to_string();
                transport_config.listen.tcp.port = port;
                println!(
                    "[{}] Transport config: TCP listener on 0.0.0.0:{}",
                    node_id, port
                );
            } else {
                transport_config.listen.tcp.enabled = false;
            }

            // Configure TCP client connection if specified
            if let Some(ref addr) = tcp_connect_addr {
                transport_config.connect.tcp_servers.insert(addr.clone());
                println!(
                    "[{}] Transport config: TCP client connecting to {}",
                    node_id, addr
                );
            }
        } else {
            // No TCP config - use LAN/mDNS (won't work in Shadow but useful for native testing)
            transport_config.listen.tcp.enabled = false;
            transport_config.peer_to_peer.lan.enabled = true;
            println!("[{}] Transport config: LAN/mDNS enabled", node_id);
        }
    });

    Ok(ditto)
}

/// Writer mode: Create a test document
fn writer_mode(ditto: &Ditto, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] === WRITER MODE ===", node_id);

    // Create test document
    let doc = TestDoc {
        id: "shadow_test_001".to_string(),
        message: "Hello from Shadow!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };

    println!("[{}] Creating test document: {:?}", node_id, doc);

    // Insert document using Collection API
    let doc_json = serde_json::json!({
        "_id": doc.id,
        "message": doc.message,
        "timestamp": doc.timestamp,
        "created_by": node_id,
    });

    ditto.store().collection("baseline")?.upsert(doc_json)?;

    println!("[{}] ✓ Document inserted", node_id);

    // Wait a bit for sync to propagate
    println!("[{}] Waiting for sync propagation (10s)...", node_id);
    sleep(Duration::from_secs(10));

    println!("[{}] Writer complete", node_id);
    Ok(())
}

/// Reader mode: Wait for document to arrive
fn reader_mode(ditto: &Ditto, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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

        // Query for test document using Collection API
        let docs = ditto
            .store()
            .collection("baseline")?
            .find("_id == 'shadow_test_001'")
            .exec()?;

        if let Some(doc) = docs.first() {
            println!("[{}] ✓ Document received!", node_id);

            // Verify document contents
            if let Ok(message) = doc.get::<String>("message") {
                println!("[{}] Message: {}", node_id, message);
                if message == "Hello from Shadow!" {
                    println!("[{}] ✓ Document content verified", node_id);
                    return Ok(());
                }
            }
        }

        // Wait before next poll
        sleep(Duration::from_millis(500));
        print!(".");
        std::io::Write::flush(&mut std::io::stdout())?;
    }
}
