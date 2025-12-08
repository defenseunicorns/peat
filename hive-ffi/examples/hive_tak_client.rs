//! HIVE TAK Test Client
//!
//! This example creates a HIVE node that publishes mock data for testing
//! mDNS peer discovery with the ATAK plugin.
//!
//! # Running the Example
//!
//! ```bash
//! CXXFLAGS="-include cstdint" cargo run --example hive_tak_client -p hive-ffi --features sync
//! ```
//!
//! # What It Does
//!
//! 1. Creates a HIVE node with mDNS discovery enabled
//! 2. Publishes mock JSON documents to test sync
//! 3. Starts P2P sync and waits for peers to discover via mDNS
//!
//! The ATAK plugin running with the same formation credentials
//! should discover this node via mDNS and sync data.

use hive_ffi::{create_node, NodeConfig};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("=== HIVE TAK Test Client (mDNS Discovery) ===\n");

    // Use same credentials as ATAK plugin defaults
    let app_id = std::env::var("HIVE_APP_ID").unwrap_or_else(|_| "default-atak-formation".into());
    let shared_key = std::env::var("HIVE_SHARED_KEY")
        .unwrap_or_else(|_| "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into());

    // Create storage directory
    let storage_path =
        std::env::var("HIVE_STORAGE_PATH").unwrap_or_else(|_| "/tmp/hive-tak-client".into());
    std::fs::create_dir_all(&storage_path).expect("Failed to create storage directory");

    println!("Configuration:");
    println!("  Formation: {}", app_id);
    println!("  Storage: {}", storage_path);
    println!();

    println!("Creating HIVE node with mDNS discovery...");
    let config = NodeConfig {
        app_id: app_id.clone(),
        shared_key: shared_key.clone(),
        bind_address: Some("0.0.0.0:42008".into()), // Fixed port for testing
        storage_path: storage_path.clone(),
    };

    let node: Arc<hive_ffi::HiveNode> = match create_node(config) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create HIVE node: {:?}", e);
            return;
        }
    };

    println!("Node ID: {}", node.node_id());
    println!("Endpoint: {}", node.endpoint_addr());
    println!();

    // Publish mock data using generic document API
    publish_mock_data(&node);

    // Verify data was stored
    println!("\n--- Verifying stored data ---");
    match node.list_documents("cells") {
        Ok(docs) => println!("Cells: {} stored", docs.len()),
        Err(e) => println!("Error listing cells: {:?}", e),
    }
    match node.list_documents("tracks") {
        Ok(docs) => println!("Tracks: {} stored", docs.len()),
        Err(e) => println!("Error listing tracks: {:?}", e),
    }
    match node.list_documents("platforms") {
        Ok(docs) => println!("Platforms: {} stored", docs.len()),
        Err(e) => println!("Error listing platforms: {:?}", e),
    }

    // Start sync
    println!("\n--- Starting P2P sync with mDNS discovery ---");
    if let Err(e) = node.start_sync() {
        eprintln!("Failed to start sync: {:?}", e);
        return;
    }
    println!("Sync started. Initial peer count: {}", node.peer_count());

    // Keep running to allow sync and peer discovery
    println!("\nWaiting for mDNS peer discovery... (Ctrl+C to exit)");
    println!("ATAK plugin should discover this node via mDNS.\n");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let peers = node.peer_count();
        let connected = node.connected_peers();
        println!("Peer count: {} {:?}", peers, connected);

        // Re-publish data periodically to update timestamps
        if peers > 0 {
            println!("  Refreshing data for sync...");
            publish_mock_data(&node);
        }
    }
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn publish_mock_data(node: &hive_ffi::HiveNode) {
    println!("--- Publishing mock data ---");

    // Publish cells
    let cells = vec![
        serde_json::json!({
            "id": "cell-alpha-001",
            "name": "Alpha Team",
            "status": "active",
            "platform_count": 4,
            "center_lat": 38.8977,
            "center_lon": -77.0365,
            "capabilities": ["ISR", "EW", "STRIKE"],
            "formation_id": "formation-main",
            "leader_id": "platform-uav-001",
            "last_update": current_timestamp()
        }),
        serde_json::json!({
            "id": "cell-bravo-002",
            "name": "Bravo Team",
            "status": "forming",
            "platform_count": 2,
            "center_lat": 38.9072,
            "center_lon": -77.0369,
            "capabilities": ["LOGISTICS", "COMMS"],
            "formation_id": "formation-main",
            "last_update": current_timestamp()
        }),
    ];

    for cell in cells {
        let id = cell["id"].as_str().unwrap();
        let json = cell.to_string();
        match node.put_document("cells", id, &json) {
            Ok(()) => println!("  Published cell: {}", id),
            Err(e) => eprintln!("  Error publishing cell {}: {:?}", id, e),
        }
    }

    // Publish tracks
    let now = current_timestamp();
    let tracks = vec![
        serde_json::json!({
            "id": "track-001",
            "source_platform": "platform-uav-001",
            "cell_id": "cell-alpha-001",
            "lat": 38.8920,
            "lon": -77.0300,
            "hae": 0.0,
            "classification": "a-h-G-U-C",
            "confidence": 0.85,
            "category": "vehicle",
            "last_update": now
        }),
        serde_json::json!({
            "id": "track-002",
            "source_platform": "platform-uav-002",
            "cell_id": "cell-alpha-001",
            "lat": 38.8950,
            "lon": -77.0280,
            "classification": "a-h-G-U-C-I",
            "confidence": 0.72,
            "category": "person",
            "last_update": now
        }),
    ];

    for track in tracks {
        let id = track["id"].as_str().unwrap();
        let json = track.to_string();
        match node.put_document("tracks", id, &json) {
            Ok(()) => println!("  Published track: {}", id),
            Err(e) => eprintln!("  Error publishing track {}: {:?}", id, e),
        }
    }

    // Publish platforms
    let platforms = vec![
        serde_json::json!({
            "id": "platform-uav-001",
            "name": "RAVEN-1",
            "platform_type": "UAV",
            "lat": 38.8990,
            "lon": -77.0360,
            "hae": 150.0,
            "readiness": 0.95,
            "cell_id": "cell-alpha-001",
            "capabilities": ["ISR", "EW"],
            "status": "ready",
            "last_heartbeat": current_timestamp()
        }),
        serde_json::json!({
            "id": "platform-uav-002",
            "name": "RAVEN-2",
            "platform_type": "UAV",
            "lat": 38.8960,
            "lon": -77.0380,
            "hae": 120.0,
            "readiness": 0.90,
            "cell_id": "cell-alpha-001",
            "capabilities": ["ISR"],
            "status": "active",
            "last_heartbeat": current_timestamp()
        }),
    ];

    for platform in platforms {
        let id = platform["id"].as_str().unwrap();
        let json = platform.to_string();
        match node.put_document("platforms", id, &json) {
            Ok(()) => println!("  Published platform: {}", id),
            Err(e) => eprintln!("  Error publishing platform {}: {:?}", id, e),
        }
    }
}
