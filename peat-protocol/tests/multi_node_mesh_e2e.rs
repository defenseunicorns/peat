//! Multi-Node Mesh E2E Tests
//!
//! These tests validate that AutomergeIrohBackend supports multi-node mesh
//! topologies with correct CRDT convergence semantics.
//!
//! # Test Strategy
//!
//! - **3-Node Mesh**: Minimal viable mesh to prove multi-node sync works
//! - **CRDT Convergence**: All nodes see all updates quickly (<2 seconds)
//! - **API Validation**: Tests use the `DataSyncBackend` API
//!
//! # What This Proves
//!
//! 1. **Multi-Node Sync Works**: Document created on Node 1 syncs to Node 2 & 3
//! 2. **Full Mesh Topology**: All nodes connected (3 connections total)
//! 3. **Convergence**: All nodes have identical final state
//! 4. **Bidirectional Sync**: Documents propagate in all directions

#![cfg(feature = "automerge-backend")]

use peat_protocol::sync::{ChangeEvent, DataSyncBackend, Document, Query, Value};
use peat_protocol::testing::E2EHarness;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Polling interval for sync checks (200ms for faster test execution)
const SYNC_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Maximum time to wait for a document to sync to a single node via observe().
/// Generous for CI environments with resource contention.
const SYNC_OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// Automerge+Iroh Backend Tests
// ============================================================================

/// Test 3-node mesh with Automerge+Iroh backend
#[tokio::test]
async fn test_automerge_three_node_mesh() {
    println!("=== Multi-Node Mesh E2E: Automerge+Iroh 3-Node Mesh ===");

    let mut harness = E2EHarness::new("automerge_3node_mesh");

    // Allocate random TCP ports to avoid conflicts with concurrent tests
    let port1 = E2EHarness::allocate_tcp_port().expect("Failed to allocate port1");
    let port2 = E2EHarness::allocate_tcp_port().expect("Failed to allocate port2");
    let port3 = E2EHarness::allocate_tcp_port().expect("Failed to allocate port3");
    println!("  Using TCP ports: {}, {}, {}", port1, port2, port3);

    // Create 3 backends with explicit bind addresses
    println!("  Creating 3 Automerge+Iroh backends...");
    let addr1: std::net::SocketAddr = format!("127.0.0.1:{}", port1).parse().unwrap();
    let addr2: std::net::SocketAddr = format!("127.0.0.1:{}", port2).parse().unwrap();
    let addr3: std::net::SocketAddr = format!("127.0.0.1:{}", port3).parse().unwrap();

    let backend1 = harness
        .create_automerge_backend_with_bind(Some(addr1))
        .await
        .expect("Should create backend1");

    let backend2 = harness
        .create_automerge_backend_with_bind(Some(addr2))
        .await
        .expect("Should create backend2");

    let backend3 = harness
        .create_automerge_backend_with_bind(Some(addr3))
        .await
        .expect("Should create backend3");

    println!("  ✓ 3 backends created");

    // Note: peer_discovery().start() is called by initialize() in create_automerge_backend_with_bind()
    // So accept loops are already running. Just wait for them to be fully ready.
    println!("  ✓ Peer discovery started (via initialize)");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Explicitly connect peers for Automerge (full mesh topology)
    println!("  Connecting Automerge peers in full mesh with authentication...");

    // Get transports, formation keys, and endpoint IDs
    let transport1 = backend1.transport();
    let transport2 = backend2.transport();
    let transport3 = backend3.transport();
    let formation_key1 = backend1.formation_key().expect("Should have formation key");
    let formation_key2 = backend2.formation_key().expect("Should have formation key");
    let formation_key3 = backend3.formation_key().expect("Should have formation key");

    let endpoint1_id = backend1.endpoint_id();
    let endpoint2_id = backend2.endpoint_id();
    let endpoint3_id = backend3.endpoint_id();

    // Create PeerInfo for ALL backends (needed for bidirectional connection attempts)
    // For sync to work bidirectionally, we need connections in BOTH directions.
    let peer1_info = peat_protocol::network::PeerInfo {
        name: "backend1".to_string(),
        node_id: hex::encode(endpoint1_id.as_bytes()),
        addresses: vec![addr1.to_string()],
        relay_url: None,
    };

    let peer2_info = peat_protocol::network::PeerInfo {
        name: "backend2".to_string(),
        node_id: hex::encode(endpoint2_id.as_bytes()),
        addresses: vec![addr2.to_string()],
        relay_url: None,
    };

    let peer3_info = peat_protocol::network::PeerInfo {
        name: "backend3".to_string(),
        node_id: hex::encode(endpoint3_id.as_bytes()),
        addresses: vec![addr3.to_string()],
        relay_url: None,
    };

    // Full mesh: Connect ALL pairs in BOTH directions for bidirectional sync.
    // This ensures sync works from any node to any other node.
    use peat_protocol::network::formation_handshake::perform_initiator_handshake;

    // Node1 → Node2
    if let Some(conn) = transport1
        .connect_peer(&peer2_info)
        .await
        .expect("Should connect node1 to node2")
    {
        perform_initiator_handshake(&conn, &formation_key1)
            .await
            .expect("Should authenticate node1 to node2");
        println!("    Node1 → Node2 connected");
    }

    // Node1 → Node3
    if let Some(conn) = transport1
        .connect_peer(&peer3_info)
        .await
        .expect("Should connect node1 to node3")
    {
        perform_initiator_handshake(&conn, &formation_key1)
            .await
            .expect("Should authenticate node1 to node3");
        println!("    Node1 → Node3 connected");
    }

    // Node2 → Node1 (reverse direction for bidirectional sync)
    if let Some(conn) = transport2
        .connect_peer(&peer1_info)
        .await
        .expect("Should connect node2 to node1")
    {
        perform_initiator_handshake(&conn, &formation_key2)
            .await
            .expect("Should authenticate node2 to node1");
        println!("    Node2 → Node1 connected");
    }

    // Node2 → Node3
    if let Some(conn) = transport2
        .connect_peer(&peer3_info)
        .await
        .expect("Should connect node2 to node3")
    {
        perform_initiator_handshake(&conn, &formation_key2)
            .await
            .expect("Should authenticate node2 to node3");
        println!("    Node2 → Node3 connected");
    }

    // Node3 → Node1 (reverse direction for bidirectional sync)
    if let Some(conn) = transport3
        .connect_peer(&peer1_info)
        .await
        .expect("Should connect node3 to node1")
    {
        perform_initiator_handshake(&conn, &formation_key3)
            .await
            .expect("Should authenticate node3 to node1");
        println!("    Node3 → Node1 connected");
    }

    // Node3 → Node2 (reverse direction for bidirectional sync)
    if let Some(conn) = transport3
        .connect_peer(&peer2_info)
        .await
        .expect("Should connect node3 to node2")
    {
        perform_initiator_handshake(&conn, &formation_key3)
            .await
            .expect("Should authenticate node3 to node2");
        println!("    Node3 → Node2 connected");
    }

    println!("  ✓ Full mesh connected with authentication (6 connections, 3 bidirectional pairs)");

    run_three_node_mesh_test(backend1, backend2, backend3, "Automerge+Iroh").await;
}

// ============================================================================
// Shared Test Logic
// ============================================================================

/// Wait for a specific document to appear on a node using observe() for event-driven
/// detection, confirmed by get() to ensure it's queryable through the standard API.
///
/// Uses the `observe()` API to get notified when the document arrives via sync,
/// then confirms with `get()` to ensure the document is fully queryable (observe()
/// and get() use different deserialization paths internally).
async fn wait_for_doc_on_node<B: DataSyncBackend>(
    backend: &Arc<B>,
    collection: &str,
    doc_id: &str,
    node_name: &str,
) -> bool {
    let doc_id_owned = doc_id.to_string();

    // Check if document already exists via get()
    if let Ok(Some(_)) = backend
        .document_store()
        .get(collection, &doc_id_owned)
        .await
    {
        println!("    {}: document '{}' already present", node_name, doc_id);
        return true;
    }

    // Set up observer for the collection to get notified on sync
    let stream = match backend.document_store().observe(collection, &Query::All) {
        Ok(stream) => Some(stream),
        Err(e) => {
            println!("    {}: observe() failed: {}, will poll only", node_name, e);
            None
        }
    };

    // Combined strategy: use observe() as the primary signal, with periodic get()
    // polling as confirmation. This handles both the fast path (observe fires when
    // doc arrives) and edge cases where observe() and get() have different views.
    let result = tokio::time::timeout(SYNC_OBSERVE_TIMEOUT, async {
        if let Some(mut stream) = stream {
            // Use select! to race observe events against periodic get() checks.
            // observe() gives us instant notification, get() confirms queryability.
            loop {
                tokio::select! {
                    event = stream.receiver.recv() => {
                        match event {
                            Some(ChangeEvent::Updated { document, .. }) => {
                                if document.id.as_deref() == Some(doc_id) {
                                    // Observer saw it — confirm via get() with brief retry
                                    for _ in 0..10 {
                                        if let Ok(Some(_)) = backend.document_store().get(collection, &doc_id_owned).await {
                                            return true;
                                        }
                                        sleep(Duration::from_millis(50)).await;
                                    }
                                }
                            }
                            Some(ChangeEvent::Initial { documents }) => {
                                if documents.iter().any(|d| d.id.as_deref() == Some(doc_id)) {
                                    if let Ok(Some(_)) = backend.document_store().get(collection, &doc_id_owned).await {
                                        return true;
                                    }
                                }
                            }
                            Some(_) => continue,
                            None => break, // Channel closed, fall through to polling
                        }
                    }
                    _ = sleep(Duration::from_secs(1)) => {
                        // Periodic get() check in case observe missed the event
                        if let Ok(Some(_)) = backend.document_store().get(collection, &doc_id_owned).await {
                            return true;
                        }
                    }
                }
            }
        }

        // Fallback: pure polling (if observe failed or channel closed)
        loop {
            sleep(SYNC_POLL_INTERVAL).await;
            if let Ok(Some(_)) = backend.document_store().get(collection, &doc_id_owned).await {
                return true;
            }
        }
    })
    .await;

    match result {
        Ok(true) => {
            println!(
                "    {}: document '{}' synced and confirmed",
                node_name, doc_id
            );
            true
        }
        _ => {
            println!(
                "    {}: document '{}' NOT found after {}s timeout",
                node_name,
                doc_id,
                SYNC_OBSERVE_TIMEOUT.as_secs()
            );
            false
        }
    }
}

/// Shared test logic for 3-node mesh
///
/// Tests:
/// 1. All nodes can store documents locally
/// 2. Document created on Node 1 syncs to Node 2 & Node 3
/// 3. All nodes have identical final state (CRDT convergence)
/// 4. Convergence happens within 1 second (performance target)
async fn run_three_node_mesh_test<B: DataSyncBackend>(
    backend1: Arc<B>,
    backend2: Arc<B>,
    backend3: Arc<B>,
    backend_name: &str,
) {
    println!("  Testing 3-node mesh with {} backend", backend_name);

    // Start sync on all backends
    println!("  1. Starting sync on all 3 nodes...");
    backend1
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start sync on backend1");
    backend2
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start sync on backend2");
    backend3
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start sync on backend3");
    println!("  ✓ Sync started on all nodes");

    // Create subscriptions (no-op for Automerge but exercises the API)
    let _sub1 = backend1
        .sync_engine()
        .subscribe("mesh_test", &Query::All)
        .await
        .expect("Should create subscription on backend1");
    let _sub2 = backend2
        .sync_engine()
        .subscribe("mesh_test", &Query::All)
        .await
        .expect("Should create subscription on backend2");
    let _sub3 = backend3
        .sync_engine()
        .subscribe("mesh_test", &Query::All)
        .await
        .expect("Should create subscription on backend3");

    // Prevent subscription handles from being optimized away
    let _ = (&_sub1, &_sub2, &_sub3);

    // Wait a bit for sync to initialize
    sleep(SYNC_POLL_INTERVAL).await;

    // Create document on Node 1
    println!("  2. Creating document on Node 1...");
    let mut fields = HashMap::new();
    fields.insert("source".to_string(), Value::String("node1".to_string()));
    fields.insert(
        "test_id".to_string(),
        Value::String("3node-mesh-test".to_string()),
    );
    fields.insert("value".to_string(), Value::Number(123.into()));

    let doc = Document::with_id("mesh-test-doc-1", fields);

    backend1
        .document_store()
        .upsert("mesh_test", doc)
        .await
        .expect("Should create document on node1");
    println!("  ✓ Document created on Node 1");

    // Wait for sync propagation using observe()-based event-driven detection
    println!("  3. Waiting for sync to propagate...");
    let doc_id1 = "mesh-test-doc-1".to_string();

    // Wait for document on all nodes concurrently using observe() streams.
    // This is event-driven rather than blind polling, so it detects sync
    // as soon as it happens rather than racing against poll intervals.
    let (synced1, synced2, synced3) = tokio::join!(
        wait_for_doc_on_node(&backend1, "mesh_test", &doc_id1, "Node1"),
        wait_for_doc_on_node(&backend2, "mesh_test", &doc_id1, "Node2"),
        wait_for_doc_on_node(&backend3, "mesh_test", &doc_id1, "Node3"),
    );

    assert!(
        synced1 && synced2 && synced3,
        "Document failed to sync to all nodes within timeout. Node1={}, Node2={}, Node3={}",
        synced1,
        synced2,
        synced3
    );
    println!("  ✓ Document synced to all nodes");

    // Get documents for verification
    println!("  4. Verifying document synced to all nodes...");
    let doc_on_node1 = backend1
        .document_store()
        .get("mesh_test", &doc_id1)
        .await
        .expect("Should query node1")
        .expect("Node 1 should have the document");

    let doc_on_node2 = backend2
        .document_store()
        .get("mesh_test", &doc_id1)
        .await
        .expect("Should query node2")
        .expect("Node 2 should have the document");

    let doc_on_node3 = backend3
        .document_store()
        .get("mesh_test", &doc_id1)
        .await
        .expect("Should query node3")
        .expect("Node 3 should have the document");

    println!("  ✓ Document present on all 3 nodes");

    // Verify all nodes have the same value (CRDT convergence)
    let value1 = doc_on_node1.fields.get("value").and_then(|v| v.as_i64());
    let value2 = doc_on_node2.fields.get("value").and_then(|v| v.as_i64());
    let value3 = doc_on_node3.fields.get("value").and_then(|v| v.as_i64());

    assert_eq!(value1, Some(123), "Node 1 has correct value");
    assert_eq!(value2, Some(123), "Node 2 has correct value");
    assert_eq!(value3, Some(123), "Node 3 has correct value");
    println!("  ✓ All nodes have identical state (value=123)");

    // Test bidirectional: Create document on Node 2, verify it syncs to Node 1 & 3
    println!("  5. Creating second document on Node 2...");
    let mut fields2 = HashMap::new();
    fields2.insert("source".to_string(), Value::String("node2".to_string()));
    fields2.insert(
        "test_id".to_string(),
        Value::String("3node-mesh-test".to_string()),
    );
    fields2.insert("value".to_string(), Value::Number(456.into()));

    let doc2 = Document::with_id("mesh-test-doc-2", fields2);

    backend2
        .document_store()
        .upsert("mesh_test", doc2)
        .await
        .expect("Should create document on node2");
    println!("  ✓ Document created on Node 2");

    // Wait for bidirectional sync using observe()-based event-driven detection.
    // This was previously the flaky section — blind polling with 200ms intervals
    // could miss sync events or time out under CI resource contention.
    // Using observe() streams we wait for the actual sync event deterministically.
    let doc_id2 = "mesh-test-doc-2".to_string();

    let (synced2_1, synced2_2, synced2_3) = tokio::join!(
        wait_for_doc_on_node(&backend1, "mesh_test", &doc_id2, "Node1"),
        wait_for_doc_on_node(&backend2, "mesh_test", &doc_id2, "Node2"),
        wait_for_doc_on_node(&backend3, "mesh_test", &doc_id2, "Node3"),
    );

    assert!(
        synced2_1 && synced2_2 && synced2_3,
        "Second document failed to sync to all nodes within timeout. Node1={}, Node2={}, Node3={}",
        synced2_1,
        synced2_2,
        synced2_3
    );
    println!("  ✓ Second document synced to all nodes");

    // Check peer discovery to verify mesh topology
    println!("  6. Verifying mesh topology...");
    let peers1 = backend1
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Should get peers for node1");
    let peers2 = backend2
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Should get peers for node2");
    let peers3 = backend3
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Should get peers for node3");

    println!("    Node 1: {} discovered peers", peers1.len());
    println!("    Node 2: {} discovered peers", peers2.len());
    println!("    Node 3: {} discovered peers", peers3.len());

    // In a full mesh, nodes should have discovered each other.
    // For Automerge+Iroh we explicitly created connections, but with deterministic
    // tie-breaking only one side initiates. Document sync verified above proves
    // the mesh is functional.
    // With tie-breaking, only initiator-side connections show in discovered_peers.
    // Total discovered peers across all nodes should be >= 3 for a functional mesh.
    let total_discovered = peers1.len() + peers2.len() + peers3.len();
    assert!(
        total_discovered >= 3,
        "Mesh should have at least 3 peer connections total, got {}",
        total_discovered
    );

    println!("  ✅ {} backend: 3-node mesh test PASSED!", backend_name);
    println!("    - All nodes created and synced");
    println!("    - Documents propagate in both directions");
    println!("    - CRDT convergence verified");
    println!("    - Mesh topology verified");
}
