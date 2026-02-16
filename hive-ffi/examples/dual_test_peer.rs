//! Dual-Transport Test Peer — runs on rpi-ci for functional testing.
//!
//! This single binary replaces the previous two-binary setup (ble_responder +
//! iroh_test_peer) by using `create_node()` with `enable_ble=true`. The node
//! initializes both BLE (BlueZ/D-Bus) and QUIC (Iroh) transports, matching
//! the architecture of the Android app.
//!
//! # What it does
//!
//! 1. Creates a HiveNode with FUNCTEST credentials, BLE enabled, port 42009
//! 2. Starts sync and publishes a "PI-DUAL" platform
//! 3. Prints `PEER_NODE_ID=<hex>` so the Makefile can capture it
//! 4. Polls for the Android's "ANDROID-DUAL" platform (up to 90s)
//! 5. Reports PASS/FAIL and exits
//!
//! # Building (aarch64 cross-compile for Raspberry Pi)
//!
//! ```bash
//! CXXFLAGS="-include cstdint" cross build --release \
//!     --target aarch64-unknown-linux-gnu \
//!     --example dual_test_peer \
//!     -p hive-ffi --features sync,bluetooth
//! ```

use hive_ffi::{create_node, NodeConfig, TransportConfigFFI};
use std::sync::Arc;

fn main() {
    println!("=== Dual-Transport Test Peer (BLE + QUIC) ===\n");

    let storage_path =
        std::env::var("HIVE_STORAGE_PATH").unwrap_or_else(|_| "/tmp/hive-dual-test-peer".into());

    // Clean previous state for reproducible test runs
    if std::path::Path::new(&storage_path).exists() {
        let _ = std::fs::remove_dir_all(&storage_path);
    }
    std::fs::create_dir_all(&storage_path).expect("Failed to create storage directory");

    // FUNCTEST formation credentials (matches Android test app)
    // TEST_KEY = [0x01..0x20]
    let shared_key = "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=".to_string();

    let config = NodeConfig {
        app_id: "FUNCTEST".into(),
        shared_key,
        bind_address: Some("0.0.0.0:42009".into()),
        storage_path: storage_path.clone(),
        transport: Some(TransportConfigFFI {
            enable_ble: true,
            ble_mesh_id: Some("FUNCTEST".into()),
            ble_power_profile: Some("balanced".into()),
            transport_preference: None,
            collection_routes_json: None,
        }),
    };

    println!("Creating node with BLE + QUIC transports...");
    println!("  App ID:       FUNCTEST");
    println!("  Bind address: 0.0.0.0:42009");
    println!("  BLE enabled:  true");
    println!("  Storage:      {}", storage_path);

    let node: Arc<hive_ffi::HiveNode> = match create_node(config) {
        Ok(n) => {
            println!("Node created successfully");
            n
        }
        Err(e) => {
            eprintln!("Failed to create node: {:?}", e);
            std::process::exit(1);
        }
    };

    // Print node ID for Makefile to capture
    let node_id = node.node_id();
    println!("\nPEER_NODE_ID={}", node_id);
    println!("Endpoint: {}", node.endpoint_addr());

    // Publish our platform so Android can discover it via QUIC sync
    let platform_json = serde_json::json!({
        "id": "pi-dual-test",
        "name": "PI-DUAL",
        "platform_type": "SENSOR",
        "lat": 33.749,
        "lon": -84.388,
        "hae": 0.0,
        "status": "active",
        "capabilities": ["QUIC", "BLE"],
        "readiness": 1.0
    });

    match node.put_document("platforms", "pi-dual-test", &platform_json.to_string()) {
        Ok(()) => println!("Published platform: PI-DUAL"),
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

    // Poll for Android's platform (up to 90s — allows time for BLE discovery + QUIC handshake)
    let timeout = std::time::Duration::from_secs(90);
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
        println!("Test FAILED — Android platform not received within 90s");
        std::process::exit(1);
    }
}
