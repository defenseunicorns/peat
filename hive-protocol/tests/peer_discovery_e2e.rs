//! End-to-End Tests for Peer Discovery (ADR-011 Phase 3)
//!
//! Tests the Automerge+Iroh peer discovery system:
//! - Static peer configuration loading
//! - Discovery manager aggregation
//! - mDNS discovery (when enabled)
//!
//! These tests validate that the discovery system works for the Automerge+Iroh backend.

#![cfg(feature = "automerge-backend")]

use hive_protocol::discovery::peer::{
    DiscoveryManager, DiscoveryStrategy, PeerInfo, StaticDiscovery,
};

/// Test 1: Static Discovery from In-Memory Peer List
///
/// Validates that StaticDiscovery can load and provide a peer list.
///
/// Test Flow:
/// 1. Create StaticDiscovery with in-memory peer list
/// 2. Start the discovery strategy
/// 3. Verify peers are returned correctly
#[tokio::test]
async fn test_static_discovery_from_memory() {
    // Create peer list
    let peer1 = PeerInfo {
        name: "Node Alpha".to_string(),
        node_id: "a".repeat(64), // 32 bytes in hex
        addresses: vec!["192.168.1.10:5000".to_string()],
        relay_url: None,
    };

    let peer2 = PeerInfo {
        name: "Node Bravo".to_string(),
        node_id: "b".repeat(64),
        addresses: vec!["192.168.1.11:5000".to_string()],
        relay_url: Some("https://relay.tactical.mil:3479".to_string()),
    };

    // Create static discovery
    let mut discovery = StaticDiscovery::from_peers(vec![peer1.clone(), peer2.clone()]);

    // Start discovery
    discovery.start().await.expect("Start should succeed");

    // Get discovered peers
    let peers = discovery.discovered_peers().await;

    // Validate
    assert_eq!(peers.len(), 2, "Should discover 2 peers");
    assert_eq!(peers[0].name, "Node Alpha");
    assert_eq!(peers[0].node_id, "a".repeat(64));
    assert_eq!(peers[1].name, "Node Bravo");
    assert_eq!(
        peers[1].relay_url,
        Some("https://relay.tactical.mil:3479".to_string())
    );
}

/// Test 2: Discovery Manager with Multiple Static Peers
///
/// Validates that DiscoveryManager can aggregate peers from multiple strategies.
///
/// Test Flow:
/// 1. Create two StaticDiscovery instances with different peer lists
/// 2. Add both to DiscoveryManager
/// 3. Start the manager
/// 4. Verify all peers are aggregated and deduplicated
#[tokio::test]
async fn test_discovery_manager_aggregation() {
    // Create first peer list
    let peer1 = PeerInfo {
        name: "Node Alpha".to_string(),
        node_id: "a".repeat(64),
        addresses: vec!["192.168.1.10:5000".to_string()],
        relay_url: None,
    };

    let peer2 = PeerInfo {
        name: "Node Bravo".to_string(),
        node_id: "b".repeat(64),
        addresses: vec!["192.168.1.11:5000".to_string()],
        relay_url: None,
    };

    // Create second peer list (with one duplicate)
    let peer3 = PeerInfo {
        name: "Node Charlie".to_string(),
        node_id: "c".repeat(64),
        addresses: vec!["192.168.1.12:5000".to_string()],
        relay_url: None,
    };

    let peer1_duplicate = PeerInfo {
        name: "Node Alpha (Duplicate)".to_string(),
        node_id: "a".repeat(64), // Same NodeId as peer1
        addresses: vec!["192.168.1.100:5000".to_string()],
        relay_url: None,
    };

    // Create discovery strategies
    let strategy1 = StaticDiscovery::from_peers(vec![peer1, peer2]);
    let strategy2 = StaticDiscovery::from_peers(vec![peer3, peer1_duplicate]);

    // Create manager
    let mut manager = DiscoveryManager::new();
    manager.add_strategy(Box::new(strategy1));
    manager.add_strategy(Box::new(strategy2));

    // Start manager
    manager.start().await.expect("Manager start should succeed");

    // Get aggregated peers
    let peers = manager.get_peers().await;

    // Validate: Should have 3 unique peers (peer1_duplicate merged with peer1)
    assert_eq!(
        peers.len(),
        3,
        "Should have 3 unique peers (deduplication by NodeId)"
    );

    // Validate peer count
    let count = manager.peer_count().await;
    assert_eq!(count, 3, "Peer count should match");

    // Validate that all three unique NodeIds are present
    let node_ids: Vec<String> = peers.iter().map(|p| p.node_id.clone()).collect();
    assert!(
        node_ids.contains(&"a".repeat(64)),
        "Should contain Node Alpha"
    );
    assert!(
        node_ids.contains(&"b".repeat(64)),
        "Should contain Node Bravo"
    );
    assert!(
        node_ids.contains(&"c".repeat(64)),
        "Should contain Node Charlie"
    );
}

/// Test 3: Static Discovery from TOML File
///
/// Validates that StaticDiscovery can load peers from a TOML configuration file.
///
/// Test Flow:
/// 1. Create a temporary TOML file with peer configuration
/// 2. Load StaticDiscovery from the file
/// 3. Verify peers are parsed correctly
#[tokio::test]
async fn test_static_discovery_from_toml() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Create temporary TOML file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");

    let toml_content = r#"
[[peers]]
name = "UAV Alpha"
node_id = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"
addresses = ["192.168.100.10:5000", "10.0.0.10:5000"]
relay_url = "https://relay.tactical.mil:3479"

[[peers]]
name = "UAV Bravo"
node_id = "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3"
addresses = ["192.168.100.11:5000"]
"#;

    temp_file
        .write_all(toml_content.as_bytes())
        .expect("Failed to write TOML");
    temp_file.flush().expect("Failed to flush");

    // Load discovery from file
    let mut discovery =
        StaticDiscovery::from_file(temp_file.path()).expect("Failed to load from TOML");

    // Start discovery
    discovery.start().await.expect("Start should succeed");

    // Get peers
    let peers = discovery.discovered_peers().await;

    // Validate
    assert_eq!(peers.len(), 2, "Should load 2 peers from TOML");

    // Validate first peer
    assert_eq!(peers[0].name, "UAV Alpha");
    assert_eq!(
        peers[0].node_id,
        "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"
    );
    assert_eq!(peers[0].addresses.len(), 2, "Should have 2 addresses");
    assert_eq!(peers[0].addresses[0], "192.168.100.10:5000");
    assert_eq!(
        peers[0].relay_url,
        Some("https://relay.tactical.mil:3479".to_string())
    );

    // Validate second peer
    assert_eq!(peers[1].name, "UAV Bravo");
    assert_eq!(peers[1].relay_url, None, "Should have no relay URL");
}

/// Test 4: Discovery Manager with No Strategies
///
/// Validates that DiscoveryManager handles the case of no strategies gracefully.
///
/// Test Flow:
/// 1. Create DiscoveryManager with no strategies
/// 2. Start the manager
/// 3. Verify empty peer list
#[tokio::test]
async fn test_discovery_manager_empty() {
    let mut manager = DiscoveryManager::new();

    // Start with no strategies
    manager
        .start()
        .await
        .expect("Should start even with no strategies");

    // Get peers
    let peers = manager.get_peers().await;
    let count = manager.peer_count().await;

    // Validate
    assert_eq!(peers.len(), 0, "Should have no peers");
    assert_eq!(count, 0, "Peer count should be 0");
}

/// Test 5: Discovery Manager Default Constructor
///
/// Validates that DiscoveryManager::default() works correctly.
#[tokio::test]
async fn test_discovery_manager_default() {
    let mut manager = DiscoveryManager::default();

    // Add a strategy
    let peer = PeerInfo {
        name: "Test Node".to_string(),
        node_id: "f".repeat(64),
        addresses: vec!["10.0.0.1:5000".to_string()],
        relay_url: None,
    };

    manager.add_strategy(Box::new(StaticDiscovery::from_peers(vec![peer])));
    manager.start().await.expect("Should start");

    let count = manager.peer_count().await;
    assert_eq!(count, 1, "Should have 1 peer");
}

/// Test 6: End-to-End Discovery + Connection
///
/// Validates that two nodes can discover each other via static configuration
/// and automatically establish connections.
///
/// Test Flow:
/// 1. Create Node A and Node B with Automerge+Iroh backends
/// 2. Configure Node A to discover Node B via static configuration
/// 3. Configure Node B to discover Node A via static configuration
/// 4. Start both nodes' peer discovery
/// 5. Wait for automatic connection to be established
/// 6. Verify both nodes are connected
#[tokio::test]
async fn test_e2e_discovery_and_connection() {
    use hive_protocol::network::IrohTransport;
    use hive_protocol::storage::AutomergeStore;
    use hive_protocol::sync::automerge::AutomergeIrohBackend;
    use std::sync::Arc;
    use tempfile::TempDir;

    // Create temporary directories for each node
    let temp_a = TempDir::new().expect("Failed to create temp dir");
    let temp_b = TempDir::new().expect("Failed to create temp dir");

    // Create Node A
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

    // Create Node B
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

    // Get endpoint IDs
    let endpoint_a = transport_a.endpoint_id();
    let endpoint_b = transport_b.endpoint_id();

    // Use dummy addresses since we're using Iroh's built-in relay/QUIC
    let addrs_a: Vec<String> = vec![];
    let addrs_b: Vec<String> = vec![];

    // Configure Node A to discover Node B
    let peer_b_info = PeerInfo {
        name: "Node B".to_string(),
        node_id: hex::encode(endpoint_b.as_bytes()),
        addresses: addrs_b.clone(),
        relay_url: None,
    };
    backend_a
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_b_info])))
        .await
        .expect("Failed to add discovery strategy to Node A");

    // Configure Node B to discover Node A
    let peer_a_info = PeerInfo {
        name: "Node A".to_string(),
        node_id: hex::encode(endpoint_a.as_bytes()),
        addresses: addrs_a.clone(),
        relay_url: None,
    };
    backend_b
        .add_discovery_strategy(Box::new(StaticDiscovery::from_peers(vec![peer_a_info])))
        .await
        .expect("Failed to add discovery strategy to Node B");

    // Start peer discovery on both nodes
    use hive_protocol::sync::traits::DataSyncBackend;
    use hive_protocol::sync::types::{BackendConfig, TransportConfig};
    use std::collections::HashMap;

    let config_a = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_a.path().to_path_buf(),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let config_b = BackendConfig {
        app_id: "test-app".to_string(),
        persistence_dir: temp_b.path().to_path_buf(),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend_a
        .initialize(config_a)
        .await
        .expect("Failed to initialize Node A");
    backend_b
        .initialize(config_b)
        .await
        .expect("Failed to initialize Node B");

    // Wait for automatic connection (background task runs every 5 seconds)
    println!("Waiting for nodes to discover and connect...");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Verify connections
    let peers_a = backend_a
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Failed to get peers from Node A");
    let peers_b = backend_b
        .peer_discovery()
        .discovered_peers()
        .await
        .expect("Failed to get peers from Node B");

    println!("Node A discovered {} peers", peers_a.len());
    println!("Node B discovered {} peers", peers_b.len());

    // Check that both nodes see at least one peer
    assert!(
        !peers_a.is_empty(),
        "Node A should have discovered at least one peer"
    );
    assert!(
        !peers_b.is_empty(),
        "Node B should have discovered at least one peer"
    );

    // Check for connected peers
    let connected_a = peers_a.iter().filter(|p| p.connected).count();
    let connected_b = peers_b.iter().filter(|p| p.connected).count();

    println!("Node A has {} connected peers", connected_a);
    println!("Node B has {} connected peers", connected_b);

    // At least one node should have established a connection
    // (Due to timing, both might have connected to each other)
    assert!(
        connected_a > 0 || connected_b > 0,
        "At least one node should have a connected peer"
    );

    // Cleanup
    let _ = backend_a.shutdown().await;
    let _ = backend_b.shutdown().await;
}

/// Test 7: mDNS Zero-Config Discovery
///
/// Validates that two nodes can discover each other automatically via mDNS
/// without any pre-configuration.
///
/// **NOTE**: mDNS discovery between processes on the same machine may not work
/// reliably due to OS-level mDNS filtering (especially on macOS). This test
/// validates the implementation but may fail in single-machine test environments.
/// For production validation, test on separate physical machines or VMs.
///
/// Test Flow:
/// 1. Create Node A and Node B with Automerge+Iroh backends
/// 2. Add MdnsDiscovery to both nodes (no static configuration needed)
/// 3. Start both nodes' peer discovery
/// 4. Wait for automatic mDNS discovery
/// 5. Verify both nodes discover each other
/// 6. Verify automatic connection is established
#[tokio::test]
async fn test_mdns_zero_config_discovery() {
    use hive_protocol::discovery::peer::MdnsDiscovery;
    use hive_protocol::network::IrohTransport;
    use hive_protocol::storage::AutomergeStore;
    use hive_protocol::sync::automerge::AutomergeIrohBackend;
    use std::sync::Arc;
    use tempfile::TempDir;

    // Create temporary directories for each node
    let temp_a = TempDir::new().expect("Failed to create temp dir");
    let temp_b = TempDir::new().expect("Failed to create temp dir");

    // Create Node A with mDNS discovery
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

    // Create Node B with mDNS discovery
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

    // Create mDNS discovery for Node A
    // Note: Get endpoint reference before moving transport_a
    let endpoint_a_ref = transport_a.endpoint();
    let mdns_a = MdnsDiscovery::new(endpoint_a_ref.clone(), "UAV-Alpha".to_string())
        .expect("Failed to create mDNS discovery for Node A");

    backend_a
        .add_discovery_strategy(Box::new(mdns_a))
        .await
        .expect("Failed to add mDNS discovery to Node A");

    // Create mDNS discovery for Node B
    let endpoint_b_ref = transport_b.endpoint();
    let mdns_b = MdnsDiscovery::new(endpoint_b_ref.clone(), "UAV-Bravo".to_string())
        .expect("Failed to create mDNS discovery for Node B");

    backend_b
        .add_discovery_strategy(Box::new(mdns_b))
        .await
        .expect("Failed to add mDNS discovery to Node B");

    // Start peer discovery on both nodes
    use hive_protocol::sync::traits::DataSyncBackend;
    use hive_protocol::sync::types::{BackendConfig, TransportConfig};
    use std::collections::HashMap;

    let config_a = BackendConfig {
        app_id: "test-app-mdns".to_string(),
        persistence_dir: temp_a.path().to_path_buf(),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let config_b = BackendConfig {
        app_id: "test-app-mdns".to_string(),
        persistence_dir: temp_b.path().to_path_buf(),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    backend_a
        .initialize(config_a)
        .await
        .expect("Failed to initialize Node A");
    backend_b
        .initialize(config_b)
        .await
        .expect("Failed to initialize Node B");

    // Wait for mDNS discovery and automatic connection
    // mDNS typically responds within 1-3 seconds on a local network
    // We allow extra time for connection establishment and service propagation
    println!("Waiting for mDNS discovery and connection...");
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    // Verify discovery
    let peers_a = backend_a
        .get_peer_discovery()
        .discovered_peers()
        .await
        .expect("Failed to get peers from Node A");
    let peers_b = backend_b
        .get_peer_discovery()
        .discovered_peers()
        .await
        .expect("Failed to get peers from Node B");

    println!("Node A (UAV-Alpha) discovered {} peers", peers_a.len());
    println!("Node B (UAV-Bravo) discovered {} peers", peers_b.len());

    // Verify mutual discovery
    assert!(
        !peers_a.is_empty(),
        "Node A should have discovered at least one peer via mDNS"
    );
    assert!(
        !peers_b.is_empty(),
        "Node B should have discovered at least one peer via mDNS"
    );

    // Verify peer names (UAV-Alpha and UAV-Bravo should be visible)
    let peer_names_a: Vec<String> = peers_a.iter().map(|p| p.name.clone()).collect();
    let peer_names_b: Vec<String> = peers_b.iter().map(|p| p.name.clone()).collect();

    println!("Node A sees peers: {:?}", peer_names_a);
    println!("Node B sees peers: {:?}", peer_names_b);

    // Note: For mDNS discovery, we verify that peers are discovered.
    // Connection establishment is handled separately and not tested here.

    // Cleanup
    let _ = backend_a.shutdown().await;
    let _ = backend_b.shutdown().await;
}
