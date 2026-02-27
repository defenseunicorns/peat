//! Iroh Test Peer — runs on rpi-ci2 for dual-transport functional test.
//!
//! This peer participates in the QUIC side of the dual-transport test while
//! a separate BLE responder runs on rpi-ci for the BLE side.
//!
//! # What it does
//!
//! 1. Creates a PeatNode with FUNCTEST credentials and fixed port 42009
//! 2. Starts sync and publishes a "PI-QUIC" platform
//! 3. Prints `PEER_NODE_ID=<hex>` so the Makefile can capture it
//! 4. Polls for the Android's "ANDROID-DUAL" platform (up to 60s)
//! 5. Reports PASS/FAIL and exits
//!
//! # Running
//!
//! ```bash
//! CXXFLAGS="-include cstdint" cargo run --example iroh_test_peer -p peat-ffi --features sync
//! ```

use peat_ffi::{create_node, NodeConfig};
use std::sync::Arc;

fn main() {
    println!("=== Iroh Test Peer (QUIC side of dual-transport test) ===\n");

    let storage_path =
        std::env::var("PEAT_STORAGE_PATH").unwrap_or_else(|_| "/tmp/peat-iroh-test-peer".into());
    std::fs::create_dir_all(&storage_path).expect("Failed to create storage directory");

    // Use the same FUNCTEST formation and TEST_KEY as the Android test app
    // TEST_KEY = [0x01..0x20] → base64
    let shared_key = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=".to_string();

    let config = NodeConfig {
        app_id: "FUNCTEST".into(),
        shared_key,
        bind_address: Some("0.0.0.0:42009".into()),
        storage_path: storage_path.clone(),
        transport: None, // Iroh-only (no BLE on this peer)
    };

    let node: Arc<peat_ffi::PeatNode> = match create_node(config) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create node: {:?}", e);
            std::process::exit(1);
        }
    };

    // Print node ID for Makefile to capture
    let node_id = node.node_id();
    println!("PEER_NODE_ID={}", node_id);
    println!("Endpoint: {}", node.endpoint_addr());

    // Publish our platform so Android can discover it via QUIC sync
    let platform_json = serde_json::json!({
        "id": "pi-quic-test",
        "name": "PI-QUIC",
        "platform_type": "SENSOR",
        "lat": 33.749,
        "lon": -84.388,
        "hae": 0.0,
        "status": "active",
        "capabilities": ["QUIC"],
        "readiness": 1.0
    });

    match node.put_document("platforms", "pi-quic-test", &platform_json.to_string()) {
        Ok(()) => println!("Published platform: PI-QUIC"),
        Err(e) => {
            eprintln!("Failed to publish platform: {:?}", e);
            std::process::exit(1);
        }
    }

    // Start sync
    if let Err(e) = node.start_sync() {
        eprintln!("Failed to start sync: {:?}", e);
        std::process::exit(1);
    }
    println!("Sync started, waiting for Android peer...\n");

    // Poll for Android's platform (up to 60s)
    let timeout = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_secs(2);
    let mut found = false;

    while start.elapsed() < timeout {
        std::thread::sleep(poll_interval);

        let peers = node.peer_count();
        let platforms = node.get_platforms().unwrap_or_default();

        print!(
            "\r[{:.0}s] peers={}, platforms={}",
            start.elapsed().as_secs_f64(),
            peers,
            platforms.len()
        );

        for p in &platforms {
            if p.name == "ANDROID-DUAL" || p.id == "android-dual-test" {
                println!("\n\nReceived platform: {} (id={})", p.name, p.id);
                found = true;
                break;
            }
        }

        if found {
            break;
        }
    }

    println!();
    if found {
        println!("Test PASSED");
        std::process::exit(0);
    } else {
        println!("Test FAILED — Android platform not received within 60s");
        std::process::exit(1);
    }
}
