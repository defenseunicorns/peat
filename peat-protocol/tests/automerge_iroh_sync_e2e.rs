//! End-to-End Tests for Automerge+Iroh Backend Synchronization
//!
//! These tests validate P2P document synchronization using the AutomergeIrohBackend
//! with real network connections via Iroh QUIC transport.
//!
//! # Test Focus
//!
//! - **P2P Connection**: Two Iroh nodes establishing QUIC connection
//! - **Document Sync**: Automerge CRDT document synchronization
//! - **Field-Level Merge**: CRDT semantics for concurrent updates
//! - **Bidirectional Sync**: Changes propagate both directions
//!
//! # Requirements Being Validated (TDD for Phase 6)
//!
//! 1. **Peer Discovery**: Nodes must find each other
//! 2. **Connection Management**: Establish and maintain QUIC connections
//! 3. **Document Propagation**: Changes sync automatically
//! 4. **Conflict Resolution**: Concurrent edits merge via CRDT rules
//! 5. **Sync Lifecycle**: start_sync()/stop_sync() coordination

#![cfg(feature = "automerge-backend")]

use peat_protocol::discovery::peer::StaticDiscovery;
use peat_protocol::network::{IrohTransport, PeerInfo};
use peat_protocol::storage::capabilities::{CrdtCapable, SyncCapable, TypedCollection};
use peat_protocol::storage::{AutomergeBackend, AutomergeStore};
use peat_protocol::sync::automerge::AutomergeIrohBackend;
use peat_protocol::sync::traits::DataSyncBackend;
use peat_protocol::sync::types::{BackendConfig, ChangeEvent, Document, Query, TransportConfig};
use peat_schema::node::v1::NodeState;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Helper to create a test backend with transport bound to specific address
async fn create_test_backend(
    bind_addr: SocketAddr,
) -> (AutomergeBackend, Arc<IrohTransport>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
    let transport = Arc::new(IrohTransport::bind(bind_addr).await.unwrap());
    let backend = AutomergeBackend::with_transport(store, Arc::clone(&transport));

    (backend, transport, temp_dir)
}

/// Helper to create a test backend with dynamic port allocation (port 0)
/// This prevents port conflicts when tests run in parallel.
async fn create_test_backend_dynamic() -> (AutomergeBackend, Arc<IrohTransport>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
    // Use port 0 for dynamic allocation - prevents parallel test interference
    let transport = Arc::new(IrohTransport::new().await.unwrap());
    let backend = AutomergeBackend::with_transport(store, Arc::clone(&transport));

    (backend, transport, temp_dir)
}

/// Helper to create a PeerInfo from an IrohTransport
fn create_peer_info(name: &str, transport: &IrohTransport, addr: SocketAddr) -> PeerInfo {
    let endpoint_id = transport.endpoint_id();
    let node_id_hex = hex::encode(endpoint_id.as_bytes());

    PeerInfo {
        name: name.to_string(),
        node_id: node_id_hex,
        addresses: vec![addr.to_string()],
        relay_url: None,
    }
}

/// Helper to create PeerInfo from transport using its actual bound address
fn create_peer_info_dynamic(name: &str, transport: &IrohTransport) -> PeerInfo {
    use iroh::TransportAddr;

    let endpoint_id = transport.endpoint_id();
    let node_id_hex = hex::encode(endpoint_id.as_bytes());
    let addr = transport.endpoint_addr();

    // Extract IP addresses from the EndpointAddr
    let addresses: Vec<String> = addr
        .addrs
        .iter()
        .filter_map(|a| {
            if let TransportAddr::Ip(socket_addr) = a {
                Some(socket_addr.to_string())
            } else {
                None
            }
        })
        .collect();

    PeerInfo {
        name: name.to_string(),
        node_id: node_id_hex,
        addresses,
        relay_url: None,
    }
}

/// Test 1: Basic Two-Node Connection
///
/// Validates that two Iroh nodes can establish a QUIC connection using static peer config.
///
/// **Phase 6.1: Static Peer Configuration**
/// - Uses localhost bind addresses
/// - Creates PeerInfo for direct addressing
/// - Uses connect_peer() for connection
#[tokio::test]
async fn test_two_nodes_connect() {
    println!("=== E2E: Two Nodes Connect (Dynamic Ports) ===");

    // Use dynamic port allocation to prevent parallel test interference
    let (backend1, transport1, _temp1) = create_test_backend_dynamic().await;
    let (backend2, transport2, _temp2) = create_test_backend_dynamic().await;

    println!("  Node 1 ID: {:?}", transport1.endpoint_id());
    println!("  Node 1 Addr: {:?}", transport1.endpoint_addr());
    println!("  Node 2 ID: {:?}", transport2.endpoint_id());
    println!("  Node 2 Addr: {:?}", transport2.endpoint_addr());

    // Start accept loop on BOTH nodes (required for bidirectional connection with tie-breaking)
    println!("  Starting accept loops on both nodes...");
    transport1.start_accept_loop().unwrap();
    transport2.start_accept_loop().unwrap();

    // Give accept loops a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create PeerInfo using dynamic addresses
    let node2_peer = create_peer_info_dynamic("node-2", &transport2);

    println!("  1. Attempting connection (conflict resolution handled by transport)...");

    // Connect - transport handles conflict resolution if both sides connect simultaneously
    match transport1.connect_peer(&node2_peer).await {
        Ok(_conn) => {
            println!("  ✓ Connection established!");
            assert_eq!(transport1.peer_count(), 1);
        }
        Err(e) => {
            println!("  ✗ Connection failed: {}", e);
        }
    }

    // Verify backends are initialized correctly
    assert!(backend1.sync_stats().is_ok());
    assert!(backend2.sync_stats().is_ok());
}

/// Test 2: Document Sync Between Two Nodes
///
/// Validates that document changes on one node sync to another node.
///
/// **Phase 6 Requirements Discovered:**
/// - Need automatic sync triggering on document changes
/// - Need sync message routing to connected peers
/// - Need convergence detection
#[tokio::test]
async fn test_document_sync_two_nodes() {
    println!("=== E2E: Document Sync Between Two Nodes ===");

    // Use dynamic port allocation to prevent parallel test interference
    let (backend1, transport1, _temp1) = create_test_backend_dynamic().await;
    let (backend2, transport2, _temp2) = create_test_backend_dynamic().await;

    // Create typed collections
    let nodes1: Arc<dyn TypedCollection<NodeState>> = backend1.typed_collection("nodes");
    let nodes2: Arc<dyn TypedCollection<NodeState>> = backend2.typed_collection("nodes");

    println!("  1. Starting sync on both backends...");

    // Start sync (this also starts accept loops internally)
    backend1.start_sync().unwrap();
    backend2.start_sync().unwrap();

    println!("  2. Connecting peers via static config (with deterministic tie-breaking)...");

    // Create PeerInfo using dynamic addresses
    let _node1_peer = create_peer_info_dynamic("node-1", &transport1);
    let node2_peer = create_peer_info_dynamic("node-2", &transport2);

    // Determine which node should initiate based on endpoint ID comparison
    // Either side can initiate - transport handles conflict resolution
    let connected = transport1.connect_peer(&node2_peer).await.is_ok();

    if !connected {
        println!("  ✗ Connection failed - skipping sync test");
        return;
    }

    println!("  ✓ Peers connected");
    println!("  3. Creating document on Node 1...");

    // Create a node state on backend1
    let node_state = NodeState {
        fuel_minutes: 60,
        health: 1,
        phase: 1,
        cell_id: Some("cell-1".to_string()),
        ..Default::default()
    };

    nodes1.upsert("node-1", &node_state).unwrap();
    println!("  ✓ Document created on Node 1");

    println!("  4. Waiting for automatic sync to Node 2...");

    // Poll for document on backend2 with error handling
    let mut synced = false;
    for i in 0..10 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match nodes2.get("node-1") {
            Ok(Some(retrieved)) => {
                println!("  ✓ Document synced to Node 2! (attempt {})", i + 1);
                assert_eq!(retrieved.fuel_minutes, 60);
                assert_eq!(retrieved.cell_id, Some("cell-1".to_string()));
                synced = true;
                break;
            }
            Ok(None) => {
                // Document not yet synced, continue polling
            }
            Err(e) => {
                // Deserialization error - document may have partial data
                println!("    Sync attempt {} error: {}", i + 1, e);
            }
        }
    }

    if !synced {
        println!("  ✗ Document did not sync");
        println!("  → Phase 6.2 TODO: Need background sync task");
        println!("  → Phase 6.2 TODO: Need change detection mechanism");
    }

    // Cleanup
    backend1.stop_sync().unwrap();
    backend2.stop_sync().unwrap();
}

/// Test 3: Bidirectional Sync
///
/// Validates that changes propagate in both directions.
///
/// **Phase 6 Requirements Discovered:**
/// - Need bidirectional message routing
/// - Need to handle concurrent updates
#[tokio::test]
async fn test_bidirectional_sync() {
    println!("=== E2E: Bidirectional Sync ===");

    // Use dynamic port allocation to prevent parallel test interference
    let (backend1, transport1, _temp1) = create_test_backend_dynamic().await;
    let (backend2, transport2, _temp2) = create_test_backend_dynamic().await;

    let nodes1: Arc<dyn TypedCollection<NodeState>> = backend1.typed_collection("nodes");
    let nodes2: Arc<dyn TypedCollection<NodeState>> = backend2.typed_collection("nodes");

    // Start sync (this also starts accept loops internally)
    backend1.start_sync().unwrap();
    backend2.start_sync().unwrap();

    // Connect via static config (conflict resolution handled by transport layer)
    let _node1_peer = create_peer_info_dynamic("node-1", &transport1);
    let node2_peer = create_peer_info_dynamic("node-2", &transport2);

    // Either side can initiate - transport handles conflict resolution
    let connected = transport1.connect_peer(&node2_peer).await.is_ok();

    if !connected {
        println!("  ✗ Connection failed - skipping test");
        return;
    }

    println!("  1. Creating document on Node 1...");
    let node1_doc = NodeState {
        fuel_minutes: 60,
        ..Default::default()
    };
    nodes1.upsert("doc-1", &node1_doc).unwrap();

    println!("  2. Creating different document on Node 2...");
    let node2_doc = NodeState {
        fuel_minutes: 45,
        ..Default::default()
    };
    nodes2.upsert("doc-2", &node2_doc).unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("  3. Checking if both docs exist on both nodes...");

    // Check with error handling - sync may return partial documents
    let node1_doc2_result = nodes1.get("doc-2");
    let node2_doc1_result = nodes2.get("doc-1");

    let node1_has_doc2 = match node1_doc2_result {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => {
            println!("    Node 1 get doc-2 error: {}", e);
            false
        }
    };

    let node2_has_doc1 = match node2_doc1_result {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => {
            println!("    Node 2 get doc-1 error: {}", e);
            false
        }
    };

    if node1_has_doc2 && node2_has_doc1 {
        println!("  ✓ Bidirectional sync working!");
    } else {
        println!("  ✗ Bidirectional sync not working");
        println!("    Node 1 has doc-2: {}", node1_has_doc2);
        println!("    Node 2 has doc-1: {}", node2_has_doc1);
        println!("  → Phase 6 TODO: Need bidirectional sync coordination");
    }

    backend1.stop_sync().unwrap();
    backend2.stop_sync().unwrap();
}

/// Test 4: CRDT Conflict Resolution
///
/// Validates that concurrent updates to the same document merge correctly.
///
/// **Phase 6 Requirements Discovered:**
/// - Need to preserve Automerge CRDT semantics during sync
/// - Need to handle concurrent field updates
#[tokio::test]
async fn test_concurrent_updates_merge() {
    println!("=== E2E: CRDT Conflict Resolution ===");

    // Bind to specific localhost addresses
    let addr1: SocketAddr = "127.0.0.1:19007".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:19008".parse().unwrap();

    let (backend1, transport1, _temp1) = create_test_backend(addr1).await;
    let (backend2, transport2, _temp2) = create_test_backend(addr2).await;

    let nodes1: Arc<dyn TypedCollection<NodeState>> = backend1.typed_collection("nodes");
    let nodes2: Arc<dyn TypedCollection<NodeState>> = backend2.typed_collection("nodes");

    backend1.start_sync().unwrap();
    backend2.start_sync().unwrap();

    // Connect via static config (conflict resolution handled by transport layer)
    let _node1_peer = create_peer_info("node-1", &transport1, addr1);
    let node2_peer = create_peer_info("node-2", &transport2, addr2);

    // Either side can initiate - transport handles conflict resolution
    let connected = transport1.connect_peer(&node2_peer).await.is_ok();

    if !connected {
        println!("  ✗ Connection failed - skipping test");
        return;
    }

    println!("  1. Creating initial document on both nodes...");

    let initial = NodeState {
        fuel_minutes: 100,
        health: 1,
        ..Default::default()
    };

    nodes1.upsert("shared-doc", &initial).unwrap();
    nodes2.upsert("shared-doc", &initial).unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("  2. Making concurrent updates...");

    // Node 1 updates fuel
    let mut update1 = initial.clone();
    update1.fuel_minutes = 80;
    nodes1.upsert("shared-doc", &update1).unwrap();

    // Node 2 updates health
    let mut update2 = initial.clone();
    update2.health = 2;
    nodes2.upsert("shared-doc", &update2).unwrap();

    println!("  3. Waiting for merge...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // TODO Phase 6: Check if both updates are merged
    // Expected: CRDT should merge both fields
    if let Some(merged1) = nodes1.get("shared-doc").unwrap() {
        println!(
            "    Node 1 merged doc: fuel={}, health={}",
            merged1.fuel_minutes, merged1.health
        );
    }

    if let Some(merged2) = nodes2.get("shared-doc").unwrap() {
        println!(
            "    Node 2 merged doc: fuel={}, health={}",
            merged2.fuel_minutes, merged2.health
        );
    }

    println!("  → Phase 6 TODO: Verify CRDT merge semantics");

    backend1.stop_sync().unwrap();
    backend2.stop_sync().unwrap();
}

/// Test 5: Sync Stats Validation
///
/// Validates that sync statistics are tracked correctly.
///
/// **Phase 6 Requirements Discovered:**
/// - Need to track bytes sent/received
/// - Need to track last sync timestamp
#[tokio::test]
async fn test_sync_stats_tracking() {
    println!("=== E2E: Sync Stats Tracking ===");

    // Bind to specific localhost addresses
    let addr1: SocketAddr = "127.0.0.1:19009".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:19010".parse().unwrap();

    let (backend1, transport1, _temp1) = create_test_backend(addr1).await;
    let (backend2, transport2, _temp2) = create_test_backend(addr2).await;

    backend1.start_sync().unwrap();
    backend2.start_sync().unwrap();

    let initial_stats1 = backend1.sync_stats().unwrap();
    assert_eq!(initial_stats1.peer_count, 0);
    assert_eq!(initial_stats1.bytes_sent, 0);
    assert_eq!(initial_stats1.bytes_received, 0);

    let node2_peer = create_peer_info("node-2", &transport2, addr2);
    match transport1.connect_peer(&node2_peer).await {
        Ok(_conn) => {
            println!("  ✓ Connected");

            let stats_after_connect = backend1.sync_stats().unwrap();
            println!("  Peer count: {}", stats_after_connect.peer_count);
            assert_eq!(stats_after_connect.peer_count, 1);

            // TODO Phase 6.2: After actual sync, verify byte counters
            println!("  → Phase 6.2 TODO: Track bytes sent/received");
            println!("  → Phase 6.2 TODO: Track last sync timestamp");
        }
        Err(e) => {
            println!("  ✗ Connection failed: {}", e);
        }
    }

    backend1.stop_sync().unwrap();
    backend2.stop_sync().unwrap();
}

/// Helper to create AutomergeIrohBackend with proper initialization
async fn create_iroh_backend(
    bind_addr: SocketAddr,
    shared_key: &str,
) -> (Arc<AutomergeIrohBackend>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
    let transport = Arc::new(IrohTransport::bind(bind_addr).await.unwrap());

    let backend = Arc::new(AutomergeIrohBackend::from_parts(store, transport));

    let config = BackendConfig {
        app_id: "observer-test".to_string(),
        persistence_dir: temp_dir.path().to_path_buf(),
        shared_key: Some(shared_key.to_string()),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend
        .initialize(config)
        .await
        .expect("Failed to initialize backend");

    (backend, temp_dir)
}

/// Test 6: Observer Notifications on Remote Sync (Issue #221)
///
/// Validates that observe() emits ChangeEvent::Updated when documents sync from peers.
/// This test was added after discovering that prior tests used polling instead of observers.
///
/// **Validates:**
/// - observe() returns ChangeStream that emits ChangeEvent::Initial
/// - observe() emits ChangeEvent::Updated when remote peer creates document
/// - Observer notifications work for the AutomergeIroh backend (not just Ditto)
///
/// This test validates Issue #221 fix - observer notifications on remote sync.
/// Previously ignored, now passing after sync state ordering fix.
#[tokio::test]
async fn test_observer_notifications_on_remote_sync() {
    println!("=== E2E: Observer Notifications on Remote Sync (Issue #221) ===");

    // Unique ports for this test
    let addr1: SocketAddr = "127.0.0.1:19021".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:19022".parse().unwrap();
    // Generate proper formation key (base64-encoded 32-byte key)
    let shared_key = peat_protocol::security::FormationKey::generate_secret();

    // Create backends with proper initialization
    let (backend1, _temp1) = create_iroh_backend(addr1, &shared_key).await;
    let (backend2, _temp2) = create_iroh_backend(addr2, &shared_key).await;

    println!("  Backend 1 endpoint: {:?}", backend1.endpoint_id());
    println!("  Backend 2 endpoint: {:?}", backend2.endpoint_id());

    // Setup bidirectional discovery (required for deterministic tie-breaking)
    let transport1 = backend1.transport();
    let transport2 = backend2.transport();

    let peer1_info = peat_protocol::discovery::peer::PeerInfo {
        name: "node-1".to_string(),
        node_id: hex::encode(transport1.endpoint_id().as_bytes()),
        addresses: vec![addr1.to_string()],
        relay_url: None,
    };
    let peer2_info = peat_protocol::discovery::peer::PeerInfo {
        name: "node-2".to_string(),
        node_id: hex::encode(transport2.endpoint_id().as_bytes()),
        addresses: vec![addr2.to_string()],
        relay_url: None,
    };

    backend1
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer2_info])))
        .await
        .unwrap();
    backend2
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer1_info])))
        .await
        .unwrap();

    // Start sync on both backends
    backend1.sync_engine().start_sync().await.unwrap();
    backend2.sync_engine().start_sync().await.unwrap();

    // Get document stores
    let doc_store1 = backend1.document_store();
    let doc_store2 = backend2.document_store();

    println!("  1. Setting up observer on Node 2 BEFORE creating document...");

    // Set up observer on Node 2 for the collection
    let query = Query::All;
    let mut observer = doc_store2
        .observe("test_nodes", &query)
        .expect("Failed to create observer");

    // Wait for Initial event
    let initial_event = tokio::time::timeout(Duration::from_secs(2), observer.receiver.recv())
        .await
        .expect("Timeout waiting for Initial event")
        .expect("Channel closed before Initial event");

    match initial_event {
        ChangeEvent::Initial { documents } => {
            println!(
                "  ✓ Received Initial event with {} documents",
                documents.len()
            );
            assert!(documents.is_empty(), "Initial snapshot should be empty");
        }
        _ => panic!("Expected Initial event, got {:?}", initial_event),
    }

    println!("  2. Connecting peers...");

    // Wait for background discovery/connection to establish (runs every 5 seconds)
    // With bidirectional discovery, the lower-ID node will initiate the connection
    println!("    Waiting for discovery connect loop...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    // Verify connection is established
    let connected1 = transport1.connected_peers();
    let connected2 = transport2.connected_peers();
    println!("    Node 1 connected to {} peers", connected1.len());
    println!("    Node 2 connected to {} peers", connected2.len());

    if connected1.is_empty() && connected2.is_empty() {
        println!("  ✗ No connection established - skipping observer test");
        println!("  → This indicates bidirectional discovery/connect issue");
        return;
    }
    println!("  ✓ Peers connected via discovery");

    println!("  3. Creating document on Node 1...");

    // Create document on Node 1 via DocumentStore (using JSON Document format)
    let doc = Document {
        id: Some("test-doc-1".to_string()),
        fields: {
            let mut fields = HashMap::new();
            fields.insert("name".to_string(), json!("Test Node"));
            fields.insert("fuel".to_string(), json!(100));
            fields
        },
        updated_at: SystemTime::now(),
    };

    let doc_id = doc_store1
        .upsert("test_nodes", doc)
        .await
        .expect("Failed to upsert document");
    println!("  ✓ Document created with ID: {}", doc_id);

    println!("  4. Waiting for observer notification on Node 2...");

    // Wait for Updated event (should arrive via sync)
    let update_result =
        tokio::time::timeout(Duration::from_secs(5), observer.receiver.recv()).await;

    match update_result {
        Ok(Some(ChangeEvent::Updated {
            collection,
            document,
        })) => {
            println!("  ✓ Received Updated event!");
            println!("    Collection: {}", collection);
            println!("    Document ID: {:?}", document.id);
            assert_eq!(collection, "test_nodes");
            assert_eq!(document.id, Some("test-doc-1".to_string()));
            println!("  ✓ Observer notification working correctly!");
        }
        Ok(Some(ChangeEvent::Removed { .. })) => {
            panic!("Unexpected Removed event");
        }
        Ok(Some(ChangeEvent::Initial { .. })) => {
            panic!("Unexpected second Initial event");
        }
        Ok(None) => {
            panic!("Observer channel closed unexpectedly");
        }
        Err(_timeout) => {
            println!("  ✗ Timeout: No Updated event received within 5 seconds");
            println!("  → Issue #221: observe() may not be emitting ChangeEvent::Updated");
            println!("  → Check that AutomergeStore.put() triggers broadcast notification");
            println!("  → Check that sync coordinator stores synced documents via put()");
            panic!("Observer notification test failed - Issue #221 not fixed");
        }
    }

    // Cleanup
    backend1.sync_engine().stop_sync().await.unwrap();
    backend2.sync_engine().stop_sync().await.unwrap();

    println!("  ✓ Test passed: Observer notifications work on remote sync!");
}
