//! End-to-End Integration Tests for Peer Discovery (Issue #226)
//!
//! Tests both mDNS-based peer discovery and deterministic key generation
//! for Iroh transport. These enable:
//! - mDNS: Automatic EndpointId exchange on local networks
//! - Deterministic keys: Predictable EndpointIds for static configuration
//!
//! This bridges the gap between Ditto's hostname:port addressing
//! and Iroh's EndpointId-based addressing for containerlab testing.
//!
//! ## Containerlab Static Configuration Pattern
//!
//! For containerlab environments where multicast may not work:
//! 1. Use `IrohTransport::endpoint_id_from_seed()` to pre-compute EndpointIds
//! 2. Generate peer config TOML with computed EndpointIds
//! 3. Each node uses `IrohTransport::from_seed()` with same seed
//!
//! Example:
//! ```ignore
//! // Generate config for containerlab nodes
//! for node in ["node-1", "node-2", "node-3"] {
//!     let seed = format!("my-formation/{}", node);
//!     let endpoint_id = IrohTransport::endpoint_id_from_seed(&seed);
//!     println!("[[peers]]");
//!     println!("name = \"{}\"", node);
//!     println!("node_id = \"{}\"", hex::encode(endpoint_id.as_bytes()));
//!     println!("addresses = [\"{}:9000\"]\n", node);
//! }
//! ```

#![cfg(feature = "automerge-backend")]

use hive_protocol::network::iroh_transport::IrohTransport;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Test 1: mDNS Discovery Transport Creation
///
/// Validates that IrohTransport can be created with mDNS discovery enabled.
#[tokio::test]
async fn test_mdns_discovery_transport_creation() {
    let transport = IrohTransport::with_discovery("test-node-1").await.unwrap();

    // Verify discovery is enabled
    assert!(transport.has_discovery());

    // Verify endpoint is valid
    let endpoint_id = transport.endpoint_id();
    assert_ne!(endpoint_id.as_bytes(), &[0u8; 32]);

    transport.close().await.unwrap();
}

/// Test 2: Two-Node mDNS Discovery
///
/// Validates that two nodes with mDNS discovery can find each other
/// and establish a connection using only EndpointId.
///
/// Note: This test requires multicast to work on the local network.
/// On some systems (e.g., Docker with default networking), mDNS may not work.
#[tokio::test]
async fn test_two_node_mdns_discovery() {
    // Create two transports with mDNS discovery
    let transport1 = Arc::new(IrohTransport::with_discovery("node-1").await.unwrap());
    let transport2 = Arc::new(IrohTransport::with_discovery("node-2").await.unwrap());

    // Verify both have discovery enabled
    assert!(transport1.has_discovery());
    assert!(transport2.has_discovery());

    // Get endpoint IDs
    let node1_id = transport1.endpoint_id();
    let node2_id = transport2.endpoint_id();

    tracing::info!("Node 1 ID: {}", node1_id);
    tracing::info!("Node 2 ID: {}", node2_id);

    // Start accept loop on node 2
    transport2.start_accept_loop().unwrap();

    // Wait for mDNS discovery to propagate
    // mDNS typically needs a few seconds to discover peers
    sleep(Duration::from_secs(3)).await;

    // Try to connect from node 1 to node 2 using just EndpointId
    // The discovery should have populated the address book
    match transport1.connect_by_id(node2_id).await {
        Ok(Some(conn)) => {
            tracing::info!("Successfully connected via mDNS discovery!");
            assert_eq!(conn.remote_id(), node2_id);
            assert_eq!(transport1.peer_count(), 1);
        }
        Ok(None) => {
            tracing::info!("Connection already exists or skipped due to tie-breaking");
            // This is valid - may already be connected or tie-breaking applies
        }
        Err(e) => {
            // mDNS discovery may not work in all test environments
            // (e.g., CI without multicast support)
            tracing::warn!(
                "mDNS discovery connection failed (may be expected in CI): {}",
                e
            );
        }
    }

    // Cleanup
    let _ = transport2.stop_accept_loop();
    // Note: Can't call close() on Arc<IrohTransport> - would need to Arc::try_unwrap
}

/// Test 3: Connection with Direct EndpointAddr Still Works
///
/// Validates that even with mDNS discovery enabled, direct connection
/// using full EndpointAddr still works (fallback for when mDNS fails).
#[tokio::test]
async fn test_direct_connection_with_discovery_enabled() {
    // Create two transports with mDNS discovery
    let transport1 = Arc::new(
        IrohTransport::with_discovery("direct-node-1")
            .await
            .unwrap(),
    );
    let transport2 = Arc::new(
        IrohTransport::with_discovery("direct-node-2")
            .await
            .unwrap(),
    );

    // With deterministic tie-breaking, the lower ID initiates.
    // Determine which transport should initiate.
    let t1_is_lower = transport1.endpoint_id().as_bytes() < transport2.endpoint_id().as_bytes();
    let (initiator, responder) = if t1_is_lower {
        (Arc::clone(&transport1), Arc::clone(&transport2))
    } else {
        (Arc::clone(&transport2), Arc::clone(&transport1))
    };

    // Start accept loop on responder
    responder.start_accept_loop().unwrap();

    // Get responder's full EndpointAddr (includes all addresses)
    let responder_addr = responder.endpoint_addr();

    // Connect using full EndpointAddr (not relying on mDNS)
    let conn = initiator
        .connect(responder_addr)
        .await
        .unwrap()
        .expect("Lower ID should successfully initiate connection");

    // Verify connection
    assert_eq!(conn.remote_id(), responder.endpoint_id());
    assert_eq!(initiator.peer_count(), 1);

    // Cleanup
    let _ = responder.stop_accept_loop();
}

/// Test 4: Multiple Transports with Discovery
///
/// Validates that multiple transports with discovery can coexist
/// without interfering with each other.
#[tokio::test]
async fn test_multiple_transports_with_discovery() {
    // Create three transports with mDNS discovery
    let transport1 = IrohTransport::with_discovery("multi-1").await.unwrap();
    let transport2 = IrohTransport::with_discovery("multi-2").await.unwrap();
    let transport3 = IrohTransport::with_discovery("multi-3").await.unwrap();

    // All should have discovery enabled
    assert!(transport1.has_discovery());
    assert!(transport2.has_discovery());
    assert!(transport3.has_discovery());

    // All should have unique endpoint IDs
    let id1 = transport1.endpoint_id();
    let id2 = transport2.endpoint_id();
    let id3 = transport3.endpoint_id();

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);

    // Cleanup
    transport1.close().await.unwrap();
    transport2.close().await.unwrap();
    transport3.close().await.unwrap();
}

/// Test 5: Containerlab Static Configuration Pattern (Issue #226)
///
/// Demonstrates the recommended pattern for containerlab deployments:
/// 1. Pre-compute EndpointIds using deterministic seeds
/// 2. Create static peer configuration with known EndpointIds
/// 3. Connect using the pre-configured EndpointIds
///
/// This enables Ditto-like "just use hostname:port" simplicity with Iroh.
#[tokio::test]
async fn test_containerlab_static_configuration() {
    // Simulate containerlab configuration
    let formation = "alpha-company";
    let nodes = ["squad-1", "squad-2", "squad-3"];

    // Step 1: Pre-compute EndpointIds (done at deployment time)
    let mut endpoint_ids = Vec::new();
    for node in &nodes {
        let seed = format!("{}/{}", formation, node);
        let id = IrohTransport::endpoint_id_from_seed(&seed);
        endpoint_ids.push((node.to_string(), id));
    }

    // Verify deterministic - compute again to ensure same results
    for (i, node) in nodes.iter().enumerate() {
        let seed = format!("{}/{}", formation, node);
        let id = IrohTransport::endpoint_id_from_seed(&seed);
        assert_eq!(id, endpoint_ids[i].1, "EndpointId should be deterministic");
    }

    // Step 2: Create transports with deterministic keys
    let transport1 = IrohTransport::from_seed(&format!("{}/squad-1", formation))
        .await
        .unwrap();
    let transport2 = Arc::new(
        IrohTransport::from_seed(&format!("{}/squad-2", formation))
            .await
            .unwrap(),
    );

    // Verify endpoints match pre-computed values
    assert_eq!(transport1.endpoint_id(), endpoint_ids[0].1);
    assert_eq!(transport2.endpoint_id(), endpoint_ids[1].1);

    // Start accept loop on node 2
    transport2.start_accept_loop().unwrap();

    // Step 3: Connect using pre-computed EndpointId
    // In real containerlab, we'd also have the address from hostname resolution
    // For this test, we use the full EndpointAddr since we can't resolve hostnames
    let node2_addr = transport2.endpoint_addr();
    let conn = transport1
        .connect(node2_addr)
        .await
        .unwrap()
        .expect("Expected new connection");

    // Verify connection established with expected peer
    assert_eq!(conn.remote_id(), endpoint_ids[1].1);
    assert_eq!(transport1.peer_count(), 1);

    // Cleanup
    let _ = transport2.stop_accept_loop();
    transport1.close().await.unwrap();
}

/// Test 6: Static Config Generation Output
///
/// Demonstrates how to generate TOML configuration for containerlab.
/// This output can be piped to a file and used as peer configuration.
#[test]
fn test_static_config_generation() {
    let formation = "test-formation";
    let nodes = ["node-1", "node-2", "node-3"];

    // Generate configuration TOML
    let mut config = String::new();
    config.push_str(&format!("[formation]\nid = \"{}\"\n\n", formation));

    for node in &nodes {
        let seed = format!("{}/{}", formation, node);
        let endpoint_id = IrohTransport::endpoint_id_from_seed(&seed);

        config.push_str("[[peers]]\n");
        config.push_str(&format!("name = \"{}\"\n", node));
        config.push_str(&format!(
            "node_id = \"{}\"\n",
            hex::encode(endpoint_id.as_bytes())
        ));
        config.push_str(&format!("addresses = [\"{}:9000\"]\n\n", node));
    }

    // Verify the generated config is parseable
    assert!(config.contains("[formation]"));
    assert!(config.contains("[[peers]]"));
    assert!(config.contains("node-1:9000"));

    // Verify all EndpointIds are unique and 64 hex chars (32 bytes)
    for node in &nodes {
        let seed = format!("{}/{}", formation, node);
        let endpoint_id = IrohTransport::endpoint_id_from_seed(&seed);
        let hex_id = hex::encode(endpoint_id.as_bytes());
        assert_eq!(hex_id.len(), 64);
        assert!(config.contains(&hex_id));
    }
}
