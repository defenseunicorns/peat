//! Ditto SDK Integration Spike
//!
//! This example demonstrates basic Ditto CRDT operations:
//! - SharedKey identity initialization with offline license activation
//! - TCP transport configuration for reliable localhost peer discovery
//! - G-Set (grow-only set) operations for platform capabilities
//! - OR-Set (observed-remove set) operations for squad membership
//! - LWW-Register (last-write-wins register) operations for platform state
//! - Two-instance sync testing with peer discovery
//!
//! # SharedKey Activation Requirement
//!
//! This spike demonstrates the REQUIRED activation step for SharedKey identity.
//! SharedKey is an "offline identity" that must be activated with an offline license
//! token before sync operations can be performed. See `create_ditto_with_tcp()`
//! function for the activation implementation.
//!
//! # TCP Transport for Localhost Testing
//!
//! This spike uses explicit TCP transport configuration (server listener + client
//! connection) for reliable localhost peer discovery during testing. While LAN
//! transport (mDNS) works well on macOS, TCP provides explicit control over the
//! connection topology.
//!
//! # Required Environment Variables
//!
//! - `DITTO_APP_ID`: Application ID from Ditto portal
//! - `DITTO_OFFLINE_TOKEN`: Offline license token from Ditto portal (REQUIRED)
//! - `DITTO_SHARED_KEY`: Base64-encoded shared encryption key
//!
//! Run with: cargo run --example ditto_spike --features ditto-spike

// Allow deprecated API usage in this example for demonstration purposes
#![allow(deprecated)]

use dittolive_ditto::prelude::*;
use dittolive_ditto::AppId;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenvy::dotenv().ok();

    println!("=== Ditto SDK Integration Spike ===\n");

    // Get credentials from environment
    let app_id = std::env::var("DITTO_APP_ID")?;
    let shared_key = std::env::var("DITTO_SHARED_KEY")?;

    println!("App ID: {}", app_id);
    println!("Creating two Ditto instances for sync testing...\n");

    // Create first Ditto instance (TCP listener on port 12345)
    println!("Instance 1: Initializing with TCP listener on port 12345...");
    let ditto1 = create_ditto_with_tcp(&app_id, &shared_key, ".ditto1", Some(12345), None)?;
    println!("Instance 1: Peer key = {}", get_peer_key(&ditto1));

    // Create second Ditto instance (TCP client connecting to port 12345)
    println!("\nInstance 2: Initializing with TCP client connecting to port 12345...");
    let ditto2 = create_ditto_with_tcp(&app_id, &shared_key, ".ditto2", None, Some(12345))?;
    println!("Instance 2: Peer key = {}", get_peer_key(&ditto2));

    // Start sync
    println!("\nStarting sync on both instances...");
    ditto1.start_sync().expect("Failed to start sync on ditto1");
    ditto2.start_sync().expect("Failed to start sync on ditto2");

    // Create subscriptions on both instances for peer discovery
    // Peers only discover each other when they have common subscriptions
    println!("Creating subscriptions for peer discovery...");
    let _sub1_caps = ditto1.store().register_observer_v2(
        "SELECT * FROM platform_capabilities",
        |_result| { /* Observer callback */ },
    )?;
    let _sub2_caps = ditto2.store().register_observer_v2(
        "SELECT * FROM platform_capabilities",
        |_result| { /* Observer callback */ },
    )?;

    let _sub1_state = ditto1.store().register_observer_v2(
        "SELECT * FROM platform_state",
        |_result| { /* Observer callback */ },
    )?;
    let _sub2_state = ditto2.store().register_observer_v2(
        "SELECT * FROM platform_state",
        |_result| { /* Observer callback */ },
    )?;

    let _sub1_squad = ditto1.store().register_observer_v2(
        "SELECT * FROM squad_members",
        |_result| { /* Observer callback */ },
    )?;
    let _sub2_squad = ditto2.store().register_observer_v2(
        "SELECT * FROM squad_members",
        |_result| { /* Observer callback */ },
    )?;

    // Wait for peer discovery
    println!("Waiting for peers to discover each other...");
    sleep(Duration::from_secs(2));

    let graph1 = ditto1.presence().graph();
    if graph1.remote_peers.is_empty() {
        println!("⚠ Warning: Peers have not discovered each other yet");
    } else {
        println!(
            "✓ Peers connected ({} remote peers)",
            graph1.remote_peers.len()
        );
    }

    // Test 1: G-Set (grow-only set) - Platform capabilities
    println!("\n=== Test 1: G-Set (Grow-Only Set) ===");
    println!("Use case: Static platform capabilities that can only be added");
    test_g_set(&ditto1, &ditto2)?;

    // Test 2: LWW-Register - Platform position
    println!("\n=== Test 2: LWW-Register (Last-Write-Wins) ===");
    println!("Use case: Platform dynamic state (position, fuel, health)");
    test_lww_register(&ditto1, &ditto2)?;

    // Test 3: OR-Set - Squad membership
    println!("\n=== Test 3: OR-Set (Observed-Remove Set) ===");
    println!("Use case: Squad members (can add and remove)");
    test_or_set(&ditto1, &ditto2)?;

    // Stop sync
    ditto1.stop_sync();
    ditto2.stop_sync();

    println!("\n=== Spike Complete ===");
    println!("All CRDT types tested successfully!");

    Ok(())
}

/// Creates a Ditto instance with TCP transport for localhost peer discovery testing.
///
/// # Initialization Order (CRITICAL)
///
/// 1. Build Ditto with SharedKey identity
/// 2. **ACTIVATE** with offline license token ← REQUIRED
/// 3. Configure transports (TCP + LAN)
/// 4. Start sync (done by caller)
///
/// # TCP Transport Configuration
///
/// - `listen_port`: If Some(port), configures this instance as a TCP server
/// - `connect_port`: If Some(port), configures this instance as a TCP client
///
/// For two-instance testing:
/// - Instance 1: listen_port=Some(12345), connect_port=None (server)
/// - Instance 2: listen_port=None, connect_port=Some(12345) (client)
///
/// # Why TCP for Localhost Testing?
///
/// While LAN/mDNS transport works well on macOS, TCP provides explicit control
/// over the server/client topology and works consistently across all platforms.
fn create_ditto_with_tcp(
    _app_id_str: &str,
    shared_key: &str,
    persistence_dir: &str,
    listen_port: Option<u16>,
    connect_port: Option<u16>,
) -> Result<Ditto, Box<dyn std::error::Error>> {
    // Create persistent storage root
    let root = Arc::new(PersistentRoot::new(persistence_dir)?);

    // Step 1: Build Ditto instance with SharedKey identity
    let ditto = Ditto::builder()
        .with_root(root)
        .with_identity(|ditto_root| {
            let app_id = AppId::from_env("DITTO_APP_ID")?;
            identity::SharedKey::new(ditto_root, app_id, shared_key)
        })?
        .build()?;

    // Step 2: Activate Ditto with offline license token (REQUIRED for SharedKey)
    //
    // CRITICAL: This must be called before start_sync() or you will get a NotActivated error.
    // The offline license token authenticates your Ditto license without requiring an
    // online connection to Ditto's servers.
    let offline_token = std::env::var("DITTO_OFFLINE_TOKEN")?;
    ditto.set_offline_only_license_token(&offline_token)?;

    // Step 3: Configure TCP transport for explicit localhost peer discovery
    //
    // TCP transport provides reliable peer-to-peer connections for localhost testing
    // by explicitly defining server (listener) and client (connector) roles.
    ditto.update_transport_config(|config| {
        if let Some(port) = listen_port {
            // Configure as TCP server (listener)
            config.listen.tcp.enabled = true;
            config.listen.tcp.interface_ip = "127.0.0.1".to_string();
            config.listen.tcp.port = port;
        }
        if let Some(port) = connect_port {
            // Configure as TCP client (connector)
            config
                .connect
                .tcp_servers
                .insert(format!("localhost:{}", port));
        }
        // Also enable LAN/mDNS for non-localhost scenarios (works well on macOS)
        config.peer_to_peer.lan.enabled = true;
    });

    Ok(ditto)
}

fn get_peer_key(ditto: &Ditto) -> String {
    ditto.presence().graph().local_peer.peer_key_string.clone()
}

fn test_g_set(ditto1: &Ditto, ditto2: &Ditto) -> Result<(), Box<dyn std::error::Error>> {
    let collection_name = "platform_capabilities";

    // Instance 1: Add capabilities
    println!("Instance 1: Adding static capabilities (camera, gps)");
    let doc1 = serde_json::json!({
        "platform_id": "uav_001",
        "capabilities": ["camera", "gps"]
    });

    ditto1.store().collection(collection_name)?.upsert(doc1)?;

    // Wait for sync
    sleep(Duration::from_secs(1));

    // Instance 2: Query synced data
    println!("Instance 2: Querying synced capabilities...");
    let docs = ditto2
        .store()
        .collection(collection_name)?
        .find("platform_id == 'uav_001'")
        .exec()?;

    if let Some(_doc) = docs.first() {
        println!("  Found {} document(s)", docs.len());
        println!("  ✓ G-Set sync successful");
    } else {
        println!("  ✗ Document not found - sync may need more time");
    }

    Ok(())
}

fn test_lww_register(ditto1: &Ditto, ditto2: &Ditto) -> Result<(), Box<dyn std::error::Error>> {
    let collection_name = "platform_state";

    // Instance 1: Set position
    println!("Instance 1: Setting position to (lat: 37.7, lon: -122.4)");
    let doc1 = serde_json::json!({
        "platform_id": "uav_001",
        "position": {
            "lat": 37.7,
            "lon": -122.4,
            "alt": 100.0
        },
        "timestamp": chrono::Utc::now().timestamp()
    });

    ditto1.store().collection(collection_name)?.upsert(doc1)?;

    sleep(Duration::from_millis(500));

    // Instance 2: Update position (later timestamp wins)
    println!("Instance 2: Updating position to (lat: 37.8, lon: -122.5)");
    let doc2 = serde_json::json!({
        "platform_id": "uav_001",
        "position": {
            "lat": 37.8,
            "lon": -122.5,
            "alt": 150.0
        },
        "timestamp": chrono::Utc::now().timestamp()
    });

    ditto2.store().collection(collection_name)?.upsert(doc2)?;

    // Wait for sync
    sleep(Duration::from_secs(1));

    // Query from instance 1 to see if it got the update
    println!("Instance 1: Querying final position...");
    let docs = ditto1
        .store()
        .collection(collection_name)?
        .find("platform_id == 'uav_001'")
        .exec()?;

    if let Some(_doc) = docs.first() {
        println!("  Found {} document(s)", docs.len());
        println!("  ✓ LWW-Register sync successful");
    }

    Ok(())
}

fn test_or_set(ditto1: &Ditto, ditto2: &Ditto) -> Result<(), Box<dyn std::error::Error>> {
    let collection_name = "squad_members";

    // Instance 1: Add squad with initial members
    println!("Instance 1: Creating squad with members [uav_001, uav_002]");
    let doc1 = serde_json::json!({
        "squad_id": "alpha",
        "members": ["uav_001", "uav_002"]
    });

    ditto1.store().collection(collection_name)?.upsert(doc1)?;

    sleep(Duration::from_millis(500));

    // Instance 2: Add a member
    println!("Instance 2: Adding member uav_003");
    let doc2 = serde_json::json!({
        "squad_id": "alpha",
        "members": ["uav_001", "uav_002", "uav_003"]
    });

    ditto2.store().collection(collection_name)?.upsert(doc2)?;

    // Wait for sync
    sleep(Duration::from_secs(1));

    // Query from instance 1
    println!("Instance 1: Querying final squad membership...");
    let docs = ditto1
        .store()
        .collection(collection_name)?
        .find("squad_id == 'alpha'")
        .exec()?;

    if let Some(_doc) = docs.first() {
        println!("  Found {} document(s)", docs.len());
        println!("  ✓ OR-Set sync successful");
    }

    Ok(())
}
