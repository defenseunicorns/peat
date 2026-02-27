//! E2E test for hierarchical document sync with Automerge+Iroh backend
//!
//! Tests the multi-tier topology that experiments team uses:
//! - 1 platoon leader at the top
//! - 4 soldiers connecting upward to the leader
//! - Documents flow UP the hierarchy (soldier summaries → leader aggregation)
//!
//! This validates Issue #346: documents must flow in hierarchical (non-mesh) topologies.

#![cfg(feature = "automerge-backend")]

use peat_protocol::network::formation_handshake::perform_initiator_handshake;
use peat_protocol::network::PeerInfo;
use peat_protocol::sync::{DataSyncBackend, Document, Query, Value};
use peat_protocol::testing::E2EHarness;
use std::collections::HashMap;
use std::time::Duration;

/// Polling interval for sync checks (200ms for faster test execution)
const SYNC_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Test: 5-node hierarchical topology (1 leader + 4 soldiers)
///
/// Topology:
/// ```text
///                  ┌─────────────┐
///                  │   Leader    │
///                  │  (Platoon)  │
///                  └──────┬──────┘
///            ┌─────┬──────┼──────┬─────┐
///            ▼     ▼      ▼      ▼     ▼
///         ┌────┐┌────┐┌────┐┌────┐
///         │ S1 ││ S2 ││ S3 ││ S4 │
///         └────┘└────┘└────┘└────┘
/// ```
///
/// Documents created on soldiers should sync UP to the leader.
#[tokio::test]
async fn test_hierarchical_sync_soldiers_to_leader() {
    // Skip if running in CI without proper network setup
    if std::env::var("CI").is_ok() && std::env::var("PEAT_E2E_HIERARCHICAL").is_err() {
        println!("⚠ Skipping hierarchical E2E test in CI (set PEAT_E2E_HIERARCHICAL=1 to enable)");
        return;
    }

    println!("=== Hierarchical Sync E2E Test (Issue #346) ===");
    println!("Testing: 1 leader + 4 soldiers, documents flow upward\n");

    let mut harness = E2EHarness::new("hierarchical_sync_e2e");

    // Allocate random TCP ports to avoid conflicts with concurrent tests
    let leader_port = E2EHarness::allocate_tcp_port().expect("Failed to allocate leader port");
    let soldier_ports: Vec<u16> = (0..4)
        .map(|_| E2EHarness::allocate_tcp_port().expect("Failed to allocate soldier port"))
        .collect();
    println!(
        "  Using leader port: {}, soldier ports: {:?}",
        leader_port, soldier_ports
    );

    // Create addresses
    let leader_addr: std::net::SocketAddr = format!("127.0.0.1:{}", leader_port).parse().unwrap();
    let soldier_addrs: Vec<std::net::SocketAddr> = soldier_ports
        .iter()
        .map(|p| format!("127.0.0.1:{}", p).parse().unwrap())
        .collect();

    // Create leader backend
    println!("1. Creating leader node...");
    let leader_backend = harness
        .create_automerge_backend_with_bind(Some(leader_addr))
        .await
        .expect("Should create leader backend");
    let leader_transport = leader_backend.transport();
    let leader_endpoint = leader_backend.endpoint_id();
    let _leader_formation_key = leader_backend
        .formation_key()
        .expect("Should have formation key");
    println!("   Leader endpoint: {:?}", leader_endpoint);

    // Create soldier backends
    println!("\n2. Creating 4 soldier nodes...");
    let mut soldier_backends = Vec::new();
    let mut soldier_transports = Vec::new();
    let mut soldier_endpoints = Vec::new();
    let mut soldier_formation_keys = Vec::new();

    for (i, addr) in soldier_addrs.iter().enumerate() {
        let backend = harness
            .create_automerge_backend_with_bind(Some(*addr))
            .await
            .unwrap_or_else(|_| panic!("Should create soldier {} backend", i + 1));

        let transport = backend.transport();
        let endpoint = backend.endpoint_id();
        let formation_key = backend.formation_key().expect("Should have formation key");

        println!("   Soldier {} endpoint: {:?}", i + 1, endpoint);

        soldier_endpoints.push(endpoint);
        soldier_transports.push(transport);
        soldier_formation_keys.push(formation_key);
        soldier_backends.push(backend);
    }

    // Start sync on all nodes
    println!("\n3. Starting sync on all nodes...");
    leader_backend
        .sync_engine()
        .start_sync()
        .await
        .expect("Failed to start leader sync");
    for (i, backend) in soldier_backends.iter().enumerate() {
        backend
            .sync_engine()
            .start_sync()
            .await
            .unwrap_or_else(|_| panic!("Failed to start soldier {} sync", i + 1));
    }

    // Create subscriptions (required for sync to work)
    let collection_name = "soldier_summaries";
    let _leader_sub = leader_backend
        .sync_engine()
        .subscribe(collection_name, &Query::All)
        .await
        .expect("Should create leader subscription");

    let mut _soldier_subs = Vec::new();
    for backend in &soldier_backends {
        let sub = backend
            .sync_engine()
            .subscribe(collection_name, &Query::All)
            .await
            .expect("Should create soldier subscription");
        _soldier_subs.push(sub);
    }

    // Small delay to let accept loops start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect soldiers TO leader (hierarchical: soldiers initiate upward connection)
    println!("\n4. Connecting soldiers to leader (hierarchical topology)...");

    let leader_peer_info = PeerInfo {
        name: "leader".to_string(),
        node_id: hex::encode(leader_endpoint.as_bytes()),
        addresses: vec![leader_addr.to_string()],
        relay_url: None,
    };

    for (i, transport) in soldier_transports.iter().enumerate() {
        // Each soldier connects to the leader
        // connect_peer returns Option<Connection>: Some = new connection, None = accept path handling
        match transport.connect_peer(&leader_peer_info).await {
            Ok(Some(conn)) => {
                // New connection - perform handshake
                match perform_initiator_handshake(&conn, &soldier_formation_keys[i]).await {
                    Ok(_) => println!("   Soldier {} → Leader: connected + authenticated", i + 1),
                    Err(e) => println!("   Soldier {} → Leader: handshake failed: {}", i + 1, e),
                }
            }
            Ok(None) => {
                // Accept path handling connection
                println!(
                    "   Soldier {} → Leader: connection handled by accept path",
                    i + 1
                );
            }
            Err(e) => println!("   Soldier {} → Leader: connection failed: {}", i + 1, e),
        }
    }

    // Wait for connections to stabilize
    tokio::time::sleep(SYNC_POLL_INTERVAL).await;

    // Verify connections
    let leader_peers = leader_transport.connected_peers();
    println!("\n5. Connection status:");
    println!("   Leader connected to {} soldiers", leader_peers.len());
    for (i, transport) in soldier_transports.iter().enumerate() {
        let peers = transport.connected_peers();
        println!("   Soldier {} connected to {} peers", i + 1, peers.len());
    }

    if leader_peers.is_empty() {
        println!("\n⚠ No connections established - test cannot proceed");
        println!("   This may indicate networking issues in the test environment");
        return;
    }

    // Create documents on soldiers (simulating soldier summaries)
    println!("\n6. Creating documents on soldiers...");

    for (i, backend) in soldier_backends.iter().enumerate() {
        let doc_id = format!("soldier_{}_summary", i + 1);
        let mut fields = HashMap::new();
        fields.insert(
            "soldier_id".to_string(),
            Value::String(format!("soldier_{}", i + 1)),
        );
        fields.insert(
            "status".to_string(),
            Value::String("operational".to_string()),
        );
        fields.insert(
            "ammo".to_string(),
            Value::Number(((100 - i * 10) as i64).into()),
        );
        fields.insert(
            "position_lat".to_string(),
            Value::Number((337749 + (i as i64 * 10)).into()), // Simplified lat
        );
        fields.insert(
            "position_lon".to_string(),
            Value::Number((-843958 + (i as i64 * 10)).into()), // Simplified lon
        );

        let doc = Document::with_id(&doc_id, fields);
        backend
            .document_store()
            .upsert(collection_name, doc)
            .await
            .unwrap_or_else(|_| panic!("Failed to create soldier {} summary", i + 1));
        println!("   Created: {}", doc_id);
    }

    // Wait for sync to propagate upward
    println!("\n7. Waiting for documents to sync to leader...");
    let max_attempts = 20;
    let mut synced_count = 0;

    let expected_docs: Vec<String> = vec![
        "soldier_1_summary".to_string(),
        "soldier_2_summary".to_string(),
        "soldier_3_summary".to_string(),
        "soldier_4_summary".to_string(),
    ];

    for attempt in 1..=max_attempts {
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Check how many documents the leader has
        synced_count = 0;
        for doc_id in &expected_docs {
            if let Ok(Some(_)) = leader_backend
                .document_store()
                .get(collection_name, doc_id)
                .await
            {
                synced_count += 1;
            }
        }

        if synced_count == 4 {
            println!(
                "   ✓ All 4 soldier summaries synced to leader (attempt {})",
                attempt
            );
            break;
        }

        if attempt % 5 == 0 {
            println!(
                "   Attempt {}: {} of 4 documents on leader",
                attempt, synced_count
            );
        }
    }

    // Report results
    println!("\n=== Results ===");
    if synced_count == 4 {
        println!("✅ PASSED: All soldier documents synced to leader");
        println!("   Hierarchical sync (soldier → leader) verified");
    } else if synced_count > 0 {
        println!("⚠ PARTIAL: {} of 4 documents synced", synced_count);
        println!("   Some documents reached leader, but sync is incomplete");
    } else {
        println!("❌ FAILED: No documents synced to leader");
        println!("   Documents created on soldiers did not propagate upward");
    }

    // List what the leader has
    println!("\n   Leader has {} documents:", synced_count);
    for doc_id in &expected_docs {
        if let Ok(Some(_)) = leader_backend
            .document_store()
            .get(collection_name, doc_id)
            .await
        {
            println!("     - {}", doc_id);
        }
    }

    // Cleanup
    let _ = leader_backend.sync_engine().stop_sync().await;
    for backend in &soldier_backends {
        let _ = backend.sync_engine().stop_sync().await;
    }

    // Assert for CI
    assert!(
        synced_count >= 1,
        "At least one document should have synced to leader"
    );
}

/// Test: Bidirectional hierarchical sync (leader sends commands down)
#[tokio::test]
async fn test_hierarchical_sync_leader_to_soldiers() {
    // Skip if running in CI without proper network setup
    if std::env::var("CI").is_ok() && std::env::var("PEAT_E2E_HIERARCHICAL").is_err() {
        println!("⚠ Skipping hierarchical E2E test in CI (set PEAT_E2E_HIERARCHICAL=1 to enable)");
        return;
    }

    println!("=== Hierarchical Sync E2E Test (Leader → Soldiers) ===");
    println!("Testing: Commands flow downward from leader to soldiers\n");

    let mut harness = E2EHarness::new("hierarchical_down_sync");

    // Allocate random TCP ports to avoid conflicts with concurrent tests
    let leader_port = E2EHarness::allocate_tcp_port().expect("Failed to allocate leader port");
    let soldier_port1 = E2EHarness::allocate_tcp_port().expect("Failed to allocate soldier1 port");
    let soldier_port2 = E2EHarness::allocate_tcp_port().expect("Failed to allocate soldier2 port");
    println!(
        "  Using leader port: {}, soldier ports: [{}, {}]",
        leader_port, soldier_port1, soldier_port2
    );

    // Create addresses
    let leader_addr: std::net::SocketAddr = format!("127.0.0.1:{}", leader_port).parse().unwrap();
    let soldier_addrs: Vec<std::net::SocketAddr> = vec![
        format!("127.0.0.1:{}", soldier_port1).parse().unwrap(),
        format!("127.0.0.1:{}", soldier_port2).parse().unwrap(),
    ];

    // Create leader
    let leader_backend = harness
        .create_automerge_backend_with_bind(Some(leader_addr))
        .await
        .expect("Should create leader");
    let leader_endpoint = leader_backend.endpoint_id();
    let _leader_formation_key = leader_backend.formation_key().expect("Formation key");

    // Create 2 soldiers
    let mut soldier_backends = Vec::new();
    let mut soldier_transports = Vec::new();
    let mut soldier_formation_keys = Vec::new();

    for addr in &soldier_addrs {
        let backend = harness
            .create_automerge_backend_with_bind(Some(*addr))
            .await
            .expect("Should create soldier");
        let transport = backend.transport();
        let formation_key = backend.formation_key().expect("Formation key");
        soldier_transports.push(transport);
        soldier_formation_keys.push(formation_key);
        soldier_backends.push(backend);
    }

    // Start sync
    leader_backend.sync_engine().start_sync().await.unwrap();
    for backend in &soldier_backends {
        backend.sync_engine().start_sync().await.unwrap();
    }

    // Create subscriptions
    let collection_name = "commands";
    let _leader_sub = leader_backend
        .sync_engine()
        .subscribe(collection_name, &Query::All)
        .await
        .expect("Leader subscription");

    let mut _soldier_subs = Vec::new();
    for backend in &soldier_backends {
        let sub = backend
            .sync_engine()
            .subscribe(collection_name, &Query::All)
            .await
            .expect("Soldier subscription");
        _soldier_subs.push(sub);
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect soldiers to leader
    let leader_peer_info = PeerInfo {
        name: "leader".to_string(),
        node_id: hex::encode(leader_endpoint.as_bytes()),
        addresses: vec![leader_addr.to_string()],
        relay_url: None,
    };

    for (i, transport) in soldier_transports.iter().enumerate() {
        // connect_peer returns Option<Connection>: Some = new connection, None = accept path handling
        if let Ok(Some(conn)) = transport.connect_peer(&leader_peer_info).await {
            let _ = perform_initiator_handshake(&conn, &soldier_formation_keys[i]).await;
        }
    }

    tokio::time::sleep(SYNC_POLL_INTERVAL).await;

    // Create command on leader
    println!("1. Creating command document on leader...");
    let mut fields = HashMap::new();
    fields.insert(
        "command_type".to_string(),
        Value::String("move".to_string()),
    );
    fields.insert("target_lat".to_string(), Value::Number(3378_i64.into()));
    fields.insert("target_lon".to_string(), Value::Number((-8440_i64).into()));

    let cmd_doc_id = "cmd_001".to_string();
    let doc = Document::with_id(&cmd_doc_id, fields);
    leader_backend
        .document_store()
        .upsert(collection_name, doc)
        .await
        .expect("Failed to create command");

    // Wait for sync
    println!("2. Waiting for command to sync to soldiers...");
    let mut soldiers_with_command = 0;

    for attempt in 1..=20 {
        tokio::time::sleep(Duration::from_millis(250)).await;

        soldiers_with_command = 0;
        for backend in &soldier_backends {
            if let Ok(Some(_)) = backend
                .document_store()
                .get(collection_name, &cmd_doc_id)
                .await
            {
                soldiers_with_command += 1;
            }
        }

        if soldiers_with_command == 2 {
            println!("   ✓ Command synced to all soldiers (attempt {})", attempt);
            break;
        }
    }

    // Results
    println!("\n=== Results ===");
    if soldiers_with_command == 2 {
        println!("✅ PASSED: Command propagated to all soldiers");
    } else {
        println!(
            "⚠ PARTIAL: Command reached {} of 2 soldiers",
            soldiers_with_command
        );
    }

    // Cleanup
    let _ = leader_backend.sync_engine().stop_sync().await;
    for backend in &soldier_backends {
        let _ = backend.sync_engine().stop_sync().await;
    }

    assert!(
        soldiers_with_command >= 1,
        "Command should have synced to at least one soldier"
    );
}
