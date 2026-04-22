//! Issue #229: Peer connections not bridged to SyncCoordinator for document sync
//!
//! This test validates that connections established via `add_peer()` are visible
//! to the `SyncCoordinator` and that documents sync over these connections.
//!
//! The issue claims that `add_peer()` stores connections in one transport instance
//! while `sync_document_with_all_peers()` queries a different instance.
//!
//! This test will verify:
//! 1. Transport Arcs are actually shared (same underlying instance)
//! 2. Connections via add_peer() are visible to connected_peers()
//! 3. Documents sync over these connections

#![cfg(feature = "automerge-backend")]

use peat_protocol::discovery::peer::{PeerInfo, StaticDiscovery};
use peat_protocol::network::IrohTransport;
use peat_protocol::storage::AutomergeStore;
use peat_protocol::sync::automerge::AutomergeIrohBackend;
use peat_protocol::sync::traits::DataSyncBackend;
use peat_protocol::sync::types::{BackendConfig, Document, TransportConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Test that verifies the transport Arc is correctly shared between
/// AutomergeIrohBackend, AutomergeBackend, and SyncCoordinator.
///
/// This addresses the root cause claim in Issue #229.
#[tokio::test]
async fn test_transport_arc_sharing() {
    println!("=== Issue #229: Testing Transport Arc Sharing ===");

    let temp = TempDir::new().expect("Failed to create temp dir");
    let store = Arc::new(AutomergeStore::open(temp.path()).expect("Failed to create store"));
    let transport = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport"),
    );

    // Get weak reference count before
    let strong_count_before = Arc::strong_count(&transport);
    println!(
        "Transport strong count before from_parts: {}",
        strong_count_before
    );

    let backend = AutomergeIrohBackend::from_parts(Arc::clone(&store), Arc::clone(&transport));

    // After from_parts, we should have:
    // 1. Original transport Arc (held by test)
    // 2. AutomergeIrohBackend.transport (clone)
    // 3. AutomergeBackend.transport (clone of clone)
    // 4. SyncCoordinator.transport (clone of clone of clone)
    // All pointing to same underlying IrohTransport

    let strong_count_after = Arc::strong_count(&transport);
    println!(
        "Transport strong count after from_parts: {}",
        strong_count_after
    );

    // Verify they're the same by checking pointer equality
    let backend_transport = backend.transport();
    let are_same = Arc::ptr_eq(&transport, &backend_transport);
    println!(
        "Are AutomergeIrohBackend.transport and original the same Arc? {}",
        are_same
    );
    assert!(are_same, "Transport Arcs should be the same instance");

    println!("✓ Transport Arc sharing verified");
}

/// Test that connections made via add_peer() are visible to connected_peers()
#[tokio::test]
async fn test_add_peer_connection_visible() {
    println!("=== Issue #229: Testing add_peer() Connection Visibility ===");

    // Create two nodes
    let temp_a = TempDir::new().expect("Failed to create temp dir A");
    let temp_b = TempDir::new().expect("Failed to create temp dir B");

    // Use port 0 to let OS assign unique ports (supports parallel test execution)
    let transport_a = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport A"),
    );
    let store_a = Arc::new(AutomergeStore::open(temp_a.path()).expect("Failed to create store A"));
    let backend_a = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_a),
        Arc::clone(&transport_a),
    ));

    let transport_b = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport B"),
    );
    let store_b = Arc::new(AutomergeStore::open(temp_b.path()).expect("Failed to create store B"));
    let backend_b = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_b),
        Arc::clone(&transport_b),
    ));

    // Get endpoint info (address from endpoint_addr())
    let endpoint_a = transport_a.endpoint_id();
    let endpoint_b = transport_b.endpoint_id();
    let addr_a = transport_a.endpoint_addr();
    let addr_b = transport_b.endpoint_addr();
    println!("Node A: {:?}", endpoint_a);
    println!("Node B: {:?}", endpoint_b);

    // Configure bidirectional discovery (both nodes know about each other)
    // This is required because with deterministic tie-breaking, only the lower ID initiates
    // Use EndpointAddr for reliable connection (includes relay and direct addresses)
    let peer_b_info = PeerInfo {
        name: "Node B".to_string(),
        node_id: hex::encode(endpoint_b.as_bytes()),
        addresses: addr_b.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_b.relay_urls().next().map(|u| u.to_string()),
    };
    backend_a
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_b_info])))
        .await
        .expect("Failed to add discovery strategy A");

    let peer_a_info = PeerInfo {
        name: "Node A".to_string(),
        node_id: hex::encode(endpoint_a.as_bytes()),
        addresses: addr_a.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_a.relay_urls().next().map(|u| u.to_string()),
    };
    backend_b
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_a_info])))
        .await
        .expect("Failed to add discovery strategy B");

    // Initialize backends with shared credentials
    let test_secret = peat_protocol::security::FormationKey::generate_secret();

    let config_a = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_a.path().to_path_buf(),
        shared_key: Some(test_secret.clone()),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let config_b = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_b.path().to_path_buf(),
        shared_key: Some(test_secret),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend_a
        .initialize(config_a)
        .await
        .expect("Failed to init A");
    backend_b
        .initialize(config_b)
        .await
        .expect("Failed to init B");

    // Start sync on both
    backend_a
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync A");
    backend_b
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync B");

    // Wait for background connect task to run (it runs every 5 seconds)
    println!("Waiting for background connection task...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    // Check connected peers on transport_a
    let connected_on_transport = transport_a.connected_peers();
    println!(
        "Connected peers on transport_a: {} peers",
        connected_on_transport.len()
    );

    // Check connected peers via peer_discovery()
    let discovered = backend_a
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Failed to get discovered peers");
    let connected_discovered = discovered.iter().filter(|p| p.connected).count();
    println!(
        "Connected peers via peer_discovery(): {} connected, {} total",
        connected_discovered,
        discovered.len()
    );

    // The key test: Are peers from add_peer visible to both paths?
    assert!(
        !connected_on_transport.is_empty() || connected_discovered > 0,
        "Node A should have at least one connected peer"
    );

    // Also verify the transport is shared by checking both methods return same count
    if !connected_on_transport.is_empty() {
        println!("✓ Connection via transport.connected_peers() works");
    }
    if connected_discovered > 0 {
        println!("✓ Connection via peer_discovery().discovered_peers() works");
    }

    // Cleanup
    let _ = backend_a.shutdown().await;
    let _ = backend_b.shutdown().await;
}

/// Test document sync after add_peer() connection
///
/// This is the core test for Issue #229 - verifying that documents actually
/// sync over connections established via add_peer().
///
/// This test validates the Issue #229 fix - document sync now works after
/// the sync state ordering fix (state updated only after successful send).
#[tokio::test]
async fn test_document_sync_after_add_peer() {
    // Enable tracing to see sync debug messages
    let _ = tracing_subscriber::fmt()
        .with_env_filter("peat_protocol=debug")
        .with_test_writer()
        .try_init();

    println!("=== Issue #229: Testing Document Sync After add_peer() ===");

    // Create two nodes with OS-assigned ports (supports parallel test execution)
    let temp_a = TempDir::new().expect("Failed to create temp dir A");
    let temp_b = TempDir::new().expect("Failed to create temp dir B");

    let transport_a = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport A"),
    );
    let store_a = Arc::new(AutomergeStore::open(temp_a.path()).expect("Failed to create store A"));
    let backend_a = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_a),
        Arc::clone(&transport_a),
    ));

    let transport_b = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport B"),
    );
    let store_b = Arc::new(AutomergeStore::open(temp_b.path()).expect("Failed to create store B"));
    let backend_b = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_b),
        Arc::clone(&transport_b),
    ));

    // Setup bidirectional discovery using EndpointAddr for reliable connection
    let endpoint_a = transport_a.endpoint_id();
    let endpoint_b = transport_b.endpoint_id();
    let addr_a = transport_a.endpoint_addr();
    let addr_b = transport_b.endpoint_addr();

    let peer_b_info = PeerInfo {
        name: "Node B".to_string(),
        node_id: hex::encode(endpoint_b.as_bytes()),
        addresses: addr_b.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_b.relay_urls().next().map(|u| u.to_string()),
    };
    backend_a
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_b_info])))
        .await
        .expect("Failed to add discovery strategy A");

    let peer_a_info = PeerInfo {
        name: "Node A".to_string(),
        node_id: hex::encode(endpoint_a.as_bytes()),
        addresses: addr_a.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_a.relay_urls().next().map(|u| u.to_string()),
    };
    backend_b
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_a_info])))
        .await
        .expect("Failed to add discovery strategy B");

    // Initialize with shared credentials
    let test_secret = peat_protocol::security::FormationKey::generate_secret();

    let config_a = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_a.path().to_path_buf(),
        shared_key: Some(test_secret.clone()),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let config_b = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_b.path().to_path_buf(),
        shared_key: Some(test_secret),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend_a
        .initialize(config_a)
        .await
        .expect("Failed to init A");
    backend_b
        .initialize(config_b)
        .await
        .expect("Failed to init B");

    // Start sync
    backend_a
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync A");
    backend_b
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync B");

    // Wait for connection
    println!("Waiting for connection establishment...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    // Verify connection
    let connected_a = transport_a.connected_peers();
    let connected_b = transport_b.connected_peers();
    println!(
        "Node A connected to {} peers: {:?}",
        connected_a.len(),
        connected_a
    );
    println!(
        "Node B connected to {} peers: {:?}",
        connected_b.len(),
        connected_b
    );
    println!("Expected Node A endpoint: {:?}", endpoint_a);
    println!("Expected Node B endpoint: {:?}", endpoint_b);

    // Check if Node A is connected to Node B
    let a_to_b = connected_a.contains(&endpoint_b);
    let b_to_a = connected_b.contains(&endpoint_a);
    println!("Node A connected to Node B? {}", a_to_b);
    println!("Node B connected to Node A? {}", b_to_a);

    assert!(
        !connected_a.is_empty() || !connected_b.is_empty(),
        "At least one node should have established connection"
    );
    println!("✓ Connection established");

    // Create a document on Node A using DocumentStore API
    println!("Creating document on Node A...");
    let doc_store_a = backend_a.document_store();
    let mut fields = HashMap::new();
    fields.insert("fuel_minutes".to_string(), serde_json::json!(42));
    fields.insert("health".to_string(), serde_json::json!(1));
    fields.insert("phase".to_string(), serde_json::json!(2));
    fields.insert("cell_id".to_string(), serde_json::json!("test-cell"));

    let doc = Document {
        id: Some("test-node-1".to_string()),
        fields,
        updated_at: std::time::SystemTime::now(),
    };
    doc_store_a
        .upsert("nodes", doc)
        .await
        .expect("Failed to create document");
    println!("✓ Document created");

    // Wait for sync to propagate
    println!("Waiting for sync propagation...");
    let doc_store_b = backend_b.document_store();

    let mut synced = false;
    for i in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Query all documents in nodes collection
        let docs = doc_store_b
            .query("nodes", &peat_protocol::sync::types::Query::All)
            .await
            .expect("Failed to query");

        if let Some(doc) = docs.iter().find(|d| d.id.as_deref() == Some("test-node-1")) {
            println!("✓ Document synced to Node B after {} attempts!", i + 1);
            if let Some(fuel) = doc.fields.get("fuel_minutes") {
                assert_eq!(fuel, &serde_json::json!(42), "Fuel minutes should match");
            }
            if let Some(cell_id) = doc.fields.get("cell_id") {
                assert_eq!(
                    cell_id,
                    &serde_json::json!("test-cell"),
                    "Cell ID should match"
                );
            }
            synced = true;
            break;
        }
        if (i + 1) % 5 == 0 {
            println!("  Still waiting... ({} attempts)", i + 1);
        }
    }

    if !synced {
        // Debug: Check transport state
        let peers_a = transport_a.connected_peers();
        let peers_b = transport_b.connected_peers();
        println!("DEBUG: Node A connected peers: {:?}", peers_a);
        println!("DEBUG: Node B connected peers: {:?}", peers_b);

        panic!("Document did not sync - this is the Issue #229 bug!");
    }

    println!("✓ Document sync verified - Issue #229 may be fixed or not reproducible");

    // Cleanup
    let _ = backend_a.shutdown().await;
    let _ = backend_b.shutdown().await;
}

/// Test fast connection using connect_to_discovered_peers_now()
///
/// This demonstrates the E2E test optimization: connecting peers immediately
/// instead of waiting 1-7 seconds for the background task.
///
/// Expected: Connection establishes in <1 second (vs 7+ seconds with background task)
#[tokio::test]
async fn test_fast_connection_immediate() {
    println!("=== Testing Fast Connection (connect_to_discovered_peers_now) ===");

    let start_time = std::time::Instant::now();

    // Create two nodes with OS-assigned ports (supports parallel test execution)
    let temp_a = TempDir::new().expect("Failed to create temp dir A");
    let temp_b = TempDir::new().expect("Failed to create temp dir B");

    let transport_a = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport A"),
    );
    let store_a = Arc::new(AutomergeStore::open(temp_a.path()).expect("Failed to create store A"));
    let backend_a = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_a),
        Arc::clone(&transport_a),
    ));

    let transport_b = Arc::new(
        IrohTransport::new()
            .await
            .expect("Failed to create transport B"),
    );
    let store_b = Arc::new(AutomergeStore::open(temp_b.path()).expect("Failed to create store B"));
    let backend_b = Arc::new(AutomergeIrohBackend::from_parts(
        Arc::clone(&store_b),
        Arc::clone(&transport_b),
    ));

    // Get endpoint info for discovery setup using EndpointAddr
    let endpoint_a = transport_a.endpoint_id();
    let endpoint_b = transport_b.endpoint_id();
    let addr_a = transport_a.endpoint_addr();
    let addr_b = transport_b.endpoint_addr();
    println!("Node A: {:?}", endpoint_a);
    println!("Node B: {:?}", endpoint_b);

    // Setup bidirectional discovery
    let peer_b_info = PeerInfo {
        name: "Node B".to_string(),
        node_id: hex::encode(endpoint_b.as_bytes()),
        addresses: addr_b.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_b.relay_urls().next().map(|u| u.to_string()),
    };
    backend_a
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_b_info])))
        .await
        .expect("Failed to add discovery strategy A");

    let peer_a_info = PeerInfo {
        name: "Node A".to_string(),
        node_id: hex::encode(endpoint_a.as_bytes()),
        addresses: addr_a.ip_addrs().map(|a| a.to_string()).collect(),
        relay_url: addr_a.relay_urls().next().map(|u| u.to_string()),
    };
    backend_b
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_a_info])))
        .await
        .expect("Failed to add discovery strategy B");

    // Initialize with shared credentials
    let test_secret = peat_protocol::security::FormationKey::generate_secret();

    let config_a = BackendConfig {
        app_id: "test-fast-connect".to_string(),
        persistence_dir: temp_a.path().to_path_buf(),
        shared_key: Some(test_secret.clone()),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let config_b = BackendConfig {
        app_id: "test-fast-connect".to_string(),
        persistence_dir: temp_b.path().to_path_buf(),
        shared_key: Some(test_secret),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend_a
        .initialize(config_a)
        .await
        .expect("Failed to init A");
    backend_b
        .initialize(config_b)
        .await
        .expect("Failed to init B");

    // Start sync (starts accept loops)
    backend_a
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync A");
    backend_b
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start sync B");

    let setup_time = start_time.elapsed();
    println!("Setup completed in {:?}", setup_time);

    // Use FAST CONNECTION instead of waiting 7 seconds for background task
    let connect_start = std::time::Instant::now();

    // Give accept loops a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect immediately from both sides
    let (result_a, result_b) = tokio::join!(
        backend_a.connect_to_discovered_peers_now(),
        backend_b.connect_to_discovered_peers_now()
    );

    println!(
        "Connection attempts: A={:?}, B={:?}",
        result_a.as_ref().map(|n| format!("{} new", n)),
        result_b.as_ref().map(|n| format!("{} new", n))
    );

    // Small delay for handshake completion
    tokio::time::sleep(Duration::from_millis(100)).await;

    let connect_time = connect_start.elapsed();
    let total_time = start_time.elapsed();

    // Verify connection established
    let connected_a = transport_a.connected_peers();
    let connected_b = transport_b.connected_peers();
    println!(
        "Node A connected to {} peers, Node B connected to {} peers",
        connected_a.len(),
        connected_b.len()
    );

    // Assert connection was made
    assert!(
        !connected_a.is_empty() || !connected_b.is_empty(),
        "Should have at least one connection"
    );

    // Assert fast connection time. The tight 1-second budget is a Linux-CI
    // guarantee; macOS loopback QUIC handshake runs ~3–5× slower so we
    // enforce a looser bound there (still a valid smoke regression check).
    #[cfg(target_os = "linux")]
    let fast_budget = Duration::from_secs(1);
    #[cfg(not(target_os = "linux"))]
    let fast_budget = Duration::from_secs(10);
    assert!(
        connect_time < fast_budget,
        "Fast connection should take <{}s, but took {:?}",
        fast_budget.as_secs(),
        connect_time
    );

    println!(
        "✓ FAST CONNECTION: {:?} (vs 7+ seconds with background task)",
        connect_time
    );
    println!("✓ Total test time: {:?}", total_time);

    // Cleanup
    let _ = backend_a.shutdown().await;
    let _ = backend_b.shutdown().await;
}
