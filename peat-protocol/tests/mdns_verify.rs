//! Quick verification test for mDNS discovery (Issue #233)
//!
//! This test ACTUALLY verifies that mDNS discovery works between two nodes.
//! Unlike the existing test that silently passes on failure, this one will fail
//! if mDNS discovery doesn't work.

#![cfg(feature = "automerge-backend")]

use peat_protocol::network::iroh_transport::IrohTransport;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Test that mDNS discovery actually enables peer connection by EndpointId only
///
/// This is a STRICT test - it will FAIL if mDNS discovery doesn't work.
///
/// Key insight: mDNS discovery only populates the address book. BOTH sides
/// need to attempt connection for the deterministic tie-breaking to work.
/// The side with the lower EndpointId will actually establish the connection.
#[tokio::test]
async fn test_mdns_discovery_actually_works() {
    // Enable logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("peat_protocol=debug,iroh=debug")
        .with_test_writer()
        .try_init();

    println!("=== Creating two transports with mDNS discovery ===");

    // Create two transports with Iroh's MdnsDiscovery
    let transport1 = Arc::new(
        IrohTransport::with_discovery("verify-node-1")
            .await
            .unwrap(),
    );
    let transport2 = Arc::new(
        IrohTransport::with_discovery("verify-node-2")
            .await
            .unwrap(),
    );

    let id1 = transport1.endpoint_id();
    let id2 = transport2.endpoint_id();

    println!("Node 1 ID: {}", hex::encode(id1.as_bytes()));
    println!("Node 2 ID: {}", hex::encode(id2.as_bytes()));
    println!("Node 1 has discovery: {}", transport1.has_discovery());
    println!("Node 2 has discovery: {}", transport2.has_discovery());

    // Start accept loops on BOTH nodes (bidirectional)
    transport1.start_accept_loop().unwrap();
    transport2.start_accept_loop().unwrap();

    println!("=== Waiting 3 seconds for mDNS discovery to propagate ===");
    sleep(Duration::from_secs(3)).await;

    println!("=== Both nodes attempting connection (simulating SyncCoordinator) ===");

    // IMPORTANT: Both sides must attempt connection for tie-breaking to work
    // This simulates what the SyncCoordinator's background task does
    let t1 = Arc::clone(&transport1);
    let t2 = Arc::clone(&transport2);
    let id1_clone = id1;
    let id2_clone = id2;

    // Spawn connection attempts from both sides concurrently
    let handle1 = tokio::spawn(async move { t1.connect_by_id(id2_clone).await });
    let handle2 = tokio::spawn(async move { t2.connect_by_id(id1_clone).await });

    // Wait for both connection attempts
    let (result1, result2) = tokio::join!(handle1, handle2);

    println!("Node 1 connect result: {:?}", result1);
    println!("Node 2 connect result: {:?}", result2);

    // Give a moment for connection to stabilize
    sleep(Duration::from_millis(500)).await;

    println!("Transport 1 peer count: {}", transport1.peer_count());
    println!("Transport 2 peer count: {}", transport2.peer_count());

    // Verify at least one side has a connection
    let connected = transport1.peer_count() > 0 || transport2.peer_count() > 0;

    if connected {
        println!("=== SUCCESS: mDNS discovery and connection WORKS ===");
    } else {
        println!("=== FAILURE: mDNS discovery works but connection failed ===");
        println!("This suggests the tie-breaking logic or connection path has an issue.");
        panic!("mDNS discovery failed - no connection established");
    }

    assert!(
        connected,
        "At least one transport should have a peer connection"
    );
}
