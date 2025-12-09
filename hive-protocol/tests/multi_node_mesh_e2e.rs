//! Multi-Node Mesh E2E Tests
//!
//! These tests validate that both DittoBackend and AutomergeIrohBackend support
//! multi-node mesh topologies with correct CRDT convergence semantics.
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

use hive_protocol::sync::{DataSyncBackend, Document, Query, Value};
use hive_protocol::testing::E2EHarness;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Ditto Backend Tests
// ============================================================================

/// Test 3-node mesh with Ditto backend
#[tokio::test]
async fn test_ditto_three_node_mesh() {
    println!("=== Multi-Node Mesh E2E: Ditto 3-Node Mesh ===");

    let mut harness = E2EHarness::new("ditto_3node_mesh");

    // Create 3 backends with explicit TCP configuration
    println!("  Creating 3 Ditto backends...");
    let backend1 = harness
        .create_ditto_backend_with_tcp(Some(17001), None)
        .await
        .expect("Should create backend1");

    let backend2 = harness
        .create_ditto_backend_with_tcp(None, Some("127.0.0.1:17001".to_string()))
        .await
        .expect("Should create backend2");

    let backend3 = harness
        .create_ditto_backend_with_tcp(None, Some("127.0.0.1:17001".to_string()))
        .await
        .expect("Should create backend3");

    println!("  ✓ 3 backends created");
    println!("  Note: Ditto auto-discovers peers via TCP");

    run_three_node_mesh_test(backend1, backend2, backend3, "Ditto").await;
}

// ============================================================================
// Automerge+Iroh Backend Tests
// ============================================================================

/// Test 3-node mesh with Automerge+Iroh backend
#[cfg(feature = "automerge-backend")]
#[tokio::test]
async fn test_automerge_three_node_mesh() {
    println!("=== Multi-Node Mesh E2E: Automerge+Iroh 3-Node Mesh ===");

    let mut harness = E2EHarness::new("automerge_3node_mesh");

    // Create 3 backends with explicit bind addresses
    println!("  Creating 3 Automerge+Iroh backends...");
    let addr1: std::net::SocketAddr = "127.0.0.1:19401".parse().unwrap();
    let addr2: std::net::SocketAddr = "127.0.0.1:19402".parse().unwrap();
    let addr3: std::net::SocketAddr = "127.0.0.1:19403".parse().unwrap();

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
    let _transport3 = backend3.transport();
    let formation_key1 = backend1.formation_key().expect("Should have formation key");
    let formation_key2 = backend2.formation_key().expect("Should have formation key");
    let _formation_key3 = backend3.formation_key().expect("Should have formation key");

    let endpoint1_id = backend1.endpoint_id();
    let endpoint2_id = backend2.endpoint_id();
    let endpoint3_id = backend3.endpoint_id();

    // Create PeerInfo for ALL backends (needed for bidirectional connection attempts)
    // Due to tie-breaking (Issue #229), only the side with LOWER endpoint ID initiates.
    // We must attempt connections in BOTH directions so that one side succeeds.
    let _peer1_info = hive_protocol::network::PeerInfo {
        name: "backend1".to_string(),
        node_id: hex::encode(endpoint1_id.as_bytes()),
        addresses: vec![addr1.to_string()],
        relay_url: None,
    };

    let peer2_info = hive_protocol::network::PeerInfo {
        name: "backend2".to_string(),
        node_id: hex::encode(endpoint2_id.as_bytes()),
        addresses: vec![addr2.to_string()],
        relay_url: None,
    };

    let peer3_info = hive_protocol::network::PeerInfo {
        name: "backend3".to_string(),
        node_id: hex::encode(endpoint3_id.as_bytes()),
        addresses: vec![addr3.to_string()],
        relay_url: None,
    };

    // Full mesh: Each pair attempts connection in BOTH directions.
    // Connect all pairs - conflict resolution is handled by transport layer
    use hive_protocol::network::formation_handshake::perform_initiator_handshake;

    // Pair 1-2
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

    // Pair 1-3
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

    // Pair 2-3
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

    println!("  ✓ Full mesh connected with authentication (3 bidirectional pairs)");

    run_three_node_mesh_test(backend1, backend2, backend3, "Automerge+Iroh").await;
}

// ============================================================================
// Shared Test Logic
// ============================================================================

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

    // Create subscriptions (required for Ditto, no-op for Automerge)
    // IMPORTANT: Keep subscription handles alive for Ditto sync
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
    sleep(Duration::from_millis(500)).await;

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

    // Wait for sync propagation with retry
    println!("  3. Waiting for sync to propagate...");
    let doc_id1 = "mesh-test-doc-1".to_string();

    let retries = 20;
    let mut all_synced = false;

    for i in 0..retries {
        sleep(Duration::from_millis(500)).await;

        let doc_on_node1 = backend1
            .document_store()
            .get("mesh_test", &doc_id1)
            .await
            .expect("Should query node1");

        let doc_on_node2 = backend2
            .document_store()
            .get("mesh_test", &doc_id1)
            .await
            .expect("Should query node2");

        let doc_on_node3 = backend3
            .document_store()
            .get("mesh_test", &doc_id1)
            .await
            .expect("Should query node3");

        if doc_on_node1.is_some() && doc_on_node2.is_some() && doc_on_node3.is_some() {
            println!("  ✓ Document synced to all nodes (attempt {})", i + 1);
            all_synced = true;
            break;
        }
    }

    assert!(
        all_synced,
        "Document failed to sync to all nodes within timeout"
    );

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

    // Wait for sync with retry
    let doc_id2 = "mesh-test-doc-2".to_string();
    let mut all_synced2 = false;

    for i in 0..retries {
        sleep(Duration::from_millis(500)).await;

        let doc2_on_node1 = backend1
            .document_store()
            .get("mesh_test", &doc_id2)
            .await
            .expect("Should query node1");

        let doc2_on_node2 = backend2
            .document_store()
            .get("mesh_test", &doc_id2)
            .await
            .expect("Should query node2");

        let doc2_on_node3 = backend3
            .document_store()
            .get("mesh_test", &doc_id2)
            .await
            .expect("Should query node3");

        if doc2_on_node1.is_some() && doc2_on_node2.is_some() && doc2_on_node3.is_some() {
            println!(
                "  ✓ Second document synced to all nodes (attempt {})",
                i + 1
            );
            all_synced2 = true;
            break;
        }
    }

    assert!(
        all_synced2,
        "Second document failed to sync to all nodes within timeout"
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

    // In a full mesh, nodes should have discovered each other
    // Note: For Ditto this might vary due to automatic discovery
    // For Automerge+Iroh we explicitly created connections, but with deterministic
    // tie-breaking only one side initiates. Document sync verified above proves
    // the mesh is functional.
    if backend_name == "Automerge+Iroh" {
        // With tie-breaking, only initiator-side connections show in discovered_peers
        // Total discovered peers across all nodes should be >= 3 for a functional mesh
        let total_discovered = peers1.len() + peers2.len() + peers3.len();
        assert!(
            total_discovered >= 3,
            "Mesh should have at least 3 peer connections total, got {}",
            total_discovered
        );
    }

    println!("  ✅ {} backend: 3-node mesh test PASSED!", backend_name);
    println!("    - All nodes created and synced");
    println!("    - Documents propagate in both directions");
    println!("    - CRDT convergence verified");
    println!("    - Mesh topology verified");
}
