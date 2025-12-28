//! Test peer connection state tracking and emergency/ACK flow
//!
//! Verifies that hive-btle properly tracks peer connection state
//! and handles emergency/ACK propagation.

use hive_btle::hive_mesh::{HiveMesh, HiveMeshConfig};
use hive_btle::NodeId;

/// Test that receiving data from a peer marks them as connected
#[test]
fn test_peer_marked_connected_on_data_receive() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = HiveMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = HiveMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = HiveMesh::new(config_a);
    let mesh_b = HiveMesh::new(config_b);

    // Initially, mesh_a has no peers
    assert_eq!(
        mesh_a.get_peers().len(),
        0,
        "Mesh A should start with no peers"
    );

    // Build document from mesh_b
    let doc_b = mesh_b.build_document();
    println!("Doc from B: {} bytes", doc_b.len());

    // Mesh A receives data from "peer-b" identifier
    let result = mesh_a.on_ble_data("peer-b", &doc_b, 1000);
    assert!(
        result.is_some(),
        "on_ble_data should return Some for valid peer data"
    );

    // Now mesh_a should have peer_b registered
    let peers = mesh_a.get_peers();
    assert_eq!(
        peers.len(),
        1,
        "Mesh A should have 1 peer after receiving data"
    );

    // The peer should be marked as connected
    let peer_b = &peers[0];
    assert_eq!(peer_b.node_id, node_b);
    assert!(
        peer_b.is_connected,
        "Peer B should be marked as CONNECTED after receiving data"
    );

    println!(
        "PASS: Peer {} is_connected={}",
        peer_b.node_id.as_u32(),
        peer_b.is_connected
    );
}

/// Test full emergency/ACK flow between two nodes
#[test]
fn test_emergency_ack_flow() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = HiveMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = HiveMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = HiveMesh::new(config_a);
    let mesh_b = HiveMesh::new(config_b);

    // Step 1: Exchange initial documents to register peers
    let doc_a = mesh_a.build_document();
    let doc_b = mesh_b.build_document();

    let result_a = mesh_a.on_ble_data("peer-b", &doc_b, 1000);
    let result_b = mesh_b.on_ble_data("peer-a", &doc_a, 1000);

    assert!(result_a.is_some(), "A should process B's document");
    assert!(result_b.is_some(), "B should process A's document");

    // Verify peers are registered
    assert_eq!(mesh_a.get_peers().len(), 1, "A should have 1 peer");
    assert_eq!(mesh_b.get_peers().len(), 1, "B should have 1 peer");

    println!("Step 1 PASS: Both nodes see each other as peers");

    // Step 2: Node A sends EMERGENCY
    let emergency_doc = mesh_a.start_emergency_with_known_peers(2000);
    println!("Emergency doc from A: {} bytes", emergency_doc.len());

    // Verify A has active emergency
    assert!(
        mesh_a.has_active_emergency(),
        "A should have active emergency after start_emergency"
    );

    let status_a = mesh_a.get_emergency_status();
    assert!(status_a.is_some(), "A should have emergency status");
    let (source, ts, acked, pending) = status_a.unwrap();
    println!(
        "A's emergency: source={:08X} ts={} acked={} pending={}",
        source, ts, acked, pending
    );
    assert_eq!(source, node_a.as_u32(), "Emergency source should be A");

    println!("Step 2 PASS: A created emergency");

    // Step 3: Node B receives emergency document
    let result_b = mesh_b.on_ble_data("peer-a", &emergency_doc, 2100);
    assert!(
        result_b.is_some(),
        "B should process A's emergency document"
    );

    let result = result_b.unwrap();
    println!(
        "B received: emergency={} ack={} counter_changed={} emergency_changed={}",
        result.is_emergency, result.is_ack, result.counter_changed, result.emergency_changed
    );

    // B should now have the emergency
    assert!(
        mesh_b.has_active_emergency(),
        "B should have active emergency after receiving"
    );

    let status_b = mesh_b.get_emergency_status();
    assert!(status_b.is_some(), "B should have emergency status");
    let (source_b, ts_b, acked_b, pending_b) = status_b.unwrap();
    println!(
        "B's emergency: source={:08X} ts={} acked={} pending={}",
        source_b, ts_b, acked_b, pending_b
    );
    assert_eq!(
        source_b,
        node_a.as_u32(),
        "B's emergency source should be A"
    );

    println!("Step 3 PASS: B received emergency");

    // Step 4: Node B sends ACK
    let ack_doc = mesh_b.ack_emergency(2200);
    assert!(ack_doc.is_some(), "B should be able to ACK the emergency");
    let ack_doc = ack_doc.unwrap();
    println!("ACK doc from B: {} bytes", ack_doc.len());

    // Verify B has acked (in its own state)
    assert!(
        mesh_b.has_peer_acked(node_b.as_u32()),
        "B should show itself as acked"
    );

    println!("Step 4 PASS: B created ACK");

    // Step 5: Node A receives ACK
    let result_a = mesh_a.on_ble_data("peer-b", &ack_doc, 2300);
    assert!(result_a.is_some(), "A should process B's ACK document");

    let result = result_a.unwrap();
    println!(
        "A received: emergency={} ack={} counter_changed={} emergency_changed={}",
        result.is_emergency, result.is_ack, result.counter_changed, result.emergency_changed
    );

    // A should now see B as acked
    let a_sees_b_acked = mesh_a.has_peer_acked(node_b.as_u32());
    println!("A sees B acked: {}", a_sees_b_acked);
    assert!(a_sees_b_acked, "A should see B as having ACKed");

    println!("Step 5 PASS: A received B's ACK");

    // Final verification
    let status_a = mesh_a.get_emergency_status().unwrap();
    println!(
        "Final A status: acked={} pending={}",
        status_a.2, status_a.3
    );

    println!("\n=== FULL EMERGENCY/ACK FLOW TEST PASSED ===");
}

/// Test that multiple peers are properly tracked
#[test]
fn test_multiple_peer_registration() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);
    let node_c = NodeId::new(0xCCCCCCCC);

    let config_a = HiveMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = HiveMeshConfig::new(node_b, "BRAVO", "TEST");
    let config_c = HiveMeshConfig::new(node_c, "CHARLIE", "TEST");

    let mesh_a = HiveMesh::new(config_a);
    let mesh_b = HiveMesh::new(config_b);
    let mesh_c = HiveMesh::new(config_c);

    // Build documents
    let _doc_a = mesh_a.build_document();
    let doc_b = mesh_b.build_document();
    let doc_c = mesh_c.build_document();

    // A receives from B and C
    mesh_a.on_ble_data("peer-b", &doc_b, 1000);
    mesh_a.on_ble_data("peer-c", &doc_c, 1100);

    // A should have 2 peers, both connected
    let peers_a = mesh_a.get_peers();
    assert_eq!(peers_a.len(), 2, "A should have 2 peers");

    for peer in &peers_a {
        println!(
            "Peer {:08X}: is_connected={}",
            peer.node_id.as_u32(),
            peer.is_connected
        );
        assert!(peer.is_connected, "All peers should be connected");
    }

    // Verify connected peer count
    let connected = mesh_a.get_connected_peers();
    assert_eq!(connected.len(), 2, "A should have 2 connected peers");

    println!("PASS: Multiple peers registered correctly");
}
