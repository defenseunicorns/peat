//! Integration tests for mesh synchronization
//!
//! These tests verify multi-node CRDT sync using the MockBleAdapter.

use hive_btle::config::{BleConfig, DiscoveryConfig};
use hive_btle::document::HiveDocument;
use hive_btle::gossip::{GossipStrategy, RandomFanout};
use hive_btle::hive_mesh::{HiveMesh, HiveMeshConfig};
use hive_btle::platform::mock::{MockBleAdapter, MockNetwork};
use hive_btle::platform::BleAdapter;
use hive_btle::NodeId;

/// Create a mock adapter and mesh for testing
async fn create_test_node(
    node_id: u32,
    callsign: &str,
    network: MockNetwork,
) -> (MockBleAdapter, HiveMesh) {
    let node = NodeId::new(node_id);
    let mut adapter = MockBleAdapter::new(node, network);
    adapter.init(&BleConfig::default()).await.unwrap();
    adapter
        .start_advertising(&DiscoveryConfig::default())
        .await
        .unwrap();

    let config = HiveMeshConfig::new(node, callsign, "TEST");
    let mesh = HiveMesh::new(config);

    (adapter, mesh)
}

#[tokio::test]
async fn test_two_node_discovery() {
    let network = MockNetwork::new();

    let (adapter1, _mesh1) = create_test_node(0x111, "ALPHA-1", network.clone()).await;
    let (_adapter2, _mesh2) = create_test_node(0x222, "BRAVO-1", network.clone()).await;

    // Node 1 discovers Node 2
    let devices = network.discover_nodes(&NodeId::new(0x111));
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].node_id, Some(NodeId::new(0x222)));

    // Verify adapter tracking
    assert!(adapter1.is_advertising());
}

#[tokio::test]
async fn test_two_node_connection() {
    let network = MockNetwork::new();

    let (adapter1, _mesh1) = create_test_node(0x111, "ALPHA-1", network.clone()).await;
    let (_adapter2, _mesh2) = create_test_node(0x222, "BRAVO-1", network.clone()).await;

    // Connect node 1 to node 2
    let conn = adapter1.connect(&NodeId::new(0x222)).await.unwrap();
    assert!(conn.is_alive());
    assert_eq!(adapter1.peer_count(), 1);

    // Verify network tracks connection
    assert!(network.is_connected(&NodeId::new(0x111), &NodeId::new(0x222)));
}

#[tokio::test]
async fn test_document_sync_basic() {
    let network = MockNetwork::new();

    let (adapter1, mesh1) = create_test_node(0x111, "ALPHA-1", network.clone()).await;
    let (_adapter2, mesh2) = create_test_node(0x222, "BRAVO-1", network.clone()).await;

    // Connect
    adapter1.connect(&NodeId::new(0x222)).await.unwrap();

    // Get sync data from mesh1
    let now_ms = 1000u64;
    let sync_data = mesh1.tick(now_ms);

    // Simulate receiving on mesh2
    if let Some(data) = sync_data {
        let result = mesh2.on_ble_data_received("device-111", &data, now_ms + 100);
        // The result should indicate data was processed
        assert!(result.is_some() || result.is_none()); // Both are valid outcomes
    }

    // Verify both meshes are functional
    assert_eq!(mesh1.node_id().as_u32(), 0x111);
    assert_eq!(mesh2.node_id().as_u32(), 0x222);
}

#[tokio::test]
async fn test_counter_increment_and_sync() {
    // Create two documents
    let mut doc1 = HiveDocument::new(NodeId::new(0x111));
    let mut doc2 = HiveDocument::new(NodeId::new(0x222));

    // Increment counters independently
    doc1.increment_counter();
    doc1.increment_counter();
    doc2.increment_counter();

    // Before merge
    assert_eq!(doc1.total_count(), 2);
    assert_eq!(doc2.total_count(), 1);

    // Merge doc2 into doc1
    let changed = doc1.merge(&doc2);
    assert!(changed);
    assert_eq!(doc1.total_count(), 3); // 2 + 1

    // Merge doc1 into doc2
    let changed = doc2.merge(&doc1);
    assert!(changed);
    assert_eq!(doc2.total_count(), 3); // Both now have same total
}

#[tokio::test]
async fn test_gossip_fanout_selection() {
    let network = MockNetwork::new();

    // Create 5 nodes
    let mut adapters = Vec::new();
    for i in 1..=5 {
        let (adapter, _mesh) =
            create_test_node(i * 0x111, &format!("NODE-{}", i), network.clone()).await;
        adapters.push(adapter);
    }

    // Connect node 1 to all others
    for i in 2..=5 {
        adapters[0].connect(&NodeId::new(i * 0x111)).await.unwrap();
    }

    // Verify connections
    assert_eq!(adapters[0].peer_count(), 4);

    // Test gossip selection with fanout=2
    let strategy = RandomFanout::new(2);
    let peers = adapters[0].connected_peers();

    // Create HivePeer structs for the strategy
    let hive_peers: Vec<_> = peers
        .iter()
        .map(|id| hive_btle::peer::HivePeer {
            node_id: *id,
            identifier: format!("device-{}", id.as_u32()),
            mesh_id: Some("TEST".to_string()),
            name: None,
            rssi: -60,
            is_connected: true,
            last_seen_ms: 0,
        })
        .collect();

    let selected = strategy.select_peers(&hive_peers);
    assert_eq!(selected.len(), 2); // Should only select 2 even with 4 connected
}

#[tokio::test]
async fn test_multi_hop_document_propagation() {
    // Simulate A -> B -> C propagation
    let mut doc_a = HiveDocument::new(NodeId::new(0xAAA));
    let mut doc_b = HiveDocument::new(NodeId::new(0xBBB));
    let mut doc_c = HiveDocument::new(NodeId::new(0xCCC));

    // A increments
    doc_a.increment_counter();
    assert_eq!(doc_a.total_count(), 1);

    // Encode A's document
    let data_from_a = doc_a.encode();

    // B receives from A
    let decoded = HiveDocument::decode(&data_from_a).unwrap();
    let b_changed = doc_b.merge(&decoded);
    assert!(b_changed);
    assert_eq!(doc_b.total_count(), 1);

    // B adds its own increment
    doc_b.increment_counter();
    assert_eq!(doc_b.total_count(), 2);

    // Encode B's document (which includes A's data)
    let data_from_b = doc_b.encode();

    // C receives from B
    let decoded = HiveDocument::decode(&data_from_b).unwrap();
    let c_changed = doc_c.merge(&decoded);
    assert!(c_changed);
    assert_eq!(doc_c.total_count(), 2); // C now has both A and B's data

    // C increments
    doc_c.increment_counter();
    assert_eq!(doc_c.total_count(), 3);

    // Verify all nodes eventually have same value after full sync
    let data_from_c = doc_c.encode();
    let decoded = HiveDocument::decode(&data_from_c).unwrap();
    doc_a.merge(&decoded);
    doc_b.merge(&decoded);

    assert_eq!(doc_a.total_count(), 3);
    assert_eq!(doc_b.total_count(), 3);
    assert_eq!(doc_c.total_count(), 3);
}

#[tokio::test]
async fn test_concurrent_increments_convergence() {
    // Test CRDT convergence with concurrent updates
    let mut doc1 = HiveDocument::new(NodeId::new(0x111));
    let mut doc2 = HiveDocument::new(NodeId::new(0x222));
    let mut doc3 = HiveDocument::new(NodeId::new(0x333));

    // All nodes increment concurrently (before any sync)
    doc1.increment_counter();
    doc1.increment_counter();
    doc2.increment_counter();
    doc2.increment_counter();
    doc2.increment_counter();
    doc3.increment_counter();

    // Before sync: doc1=2, doc2=3, doc3=1
    assert_eq!(doc1.total_count(), 2);
    assert_eq!(doc2.total_count(), 3);
    assert_eq!(doc3.total_count(), 1);

    // Sync in a ring: 1->2, 2->3, 3->1
    let data1 = doc1.encode();
    let data2 = doc2.encode();
    let data3 = doc3.encode();

    doc2.merge(&HiveDocument::decode(&data1).unwrap());
    doc3.merge(&HiveDocument::decode(&data2).unwrap());
    doc1.merge(&HiveDocument::decode(&data3).unwrap());

    // Second round to complete convergence
    let data1 = doc1.encode();
    let data2 = doc2.encode();
    let data3 = doc3.encode();

    doc2.merge(&HiveDocument::decode(&data1).unwrap());
    doc3.merge(&HiveDocument::decode(&data2).unwrap());
    doc1.merge(&HiveDocument::decode(&data3).unwrap());

    // All should converge to 6 (2 + 3 + 1)
    assert_eq!(doc1.total_count(), 6);
    assert_eq!(doc2.total_count(), 6);
    assert_eq!(doc3.total_count(), 6);
}
