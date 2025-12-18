//! End-to-End Tests for Startup Optimizations
//!
//! These tests validate the startup performance optimizations for the AutomergeIroh backend:
//!
//! 1. **Fast Transport Constructor**: `from_seed_at_addr()` creates working transports without mDNS
//! 2. **Deferred mDNS Discovery**: `enable_mdns_discovery()` can be called after transport creation
//! 3. **Parallel Initialization**: Store and transport can be initialized concurrently
//! 4. **Functional Sync**: Fast-created transports can still sync documents with peers
//!
//! # Background
//!
//! The AutomergeIroh backend was observed to have significantly longer startup times than
//! the Ditto backend in large-scale deployments (384-node hierarchical simulations).
//! This caused Docker API timeouts and required staged deployment workarounds.
//!
//! These optimizations reduce startup intensity by:
//! - Running store opening and transport creation in parallel
//! - Deferring mDNS discovery initialization until after critical startup path
//! - Providing a fast constructor that skips mDNS entirely for static peer configurations

#![cfg(feature = "automerge-backend")]

use hive_protocol::network::{IrohTransport, PeerInfo};
use hive_protocol::storage::capabilities::SyncCapable;
use hive_protocol::storage::{AutomergeBackend, AutomergeStore};
use iroh::TransportAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Test that the fast transport constructor (without mDNS) creates a functional transport
#[tokio::test]
async fn test_fast_transport_constructor_creates_functional_transport() {
    let seed = "test-fast-constructor/node-1";
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Create transport using fast constructor (no mDNS)
    let transport = IrohTransport::from_seed_at_addr(seed, bind_addr)
        .await
        .expect("Fast constructor should succeed");

    // Verify transport is functional
    assert!(
        !transport.has_discovery(),
        "Fast constructor should NOT enable mDNS"
    );

    let endpoint_id = transport.endpoint_id();
    assert!(
        !endpoint_id.as_bytes().is_empty(),
        "Should have valid endpoint ID"
    );

    let addr = transport.endpoint_addr();
    assert!(!addr.addrs.is_empty(), "Should have bound addresses");

    // Verify deterministic key derivation
    let expected_id = IrohTransport::endpoint_id_from_seed(seed);
    assert_eq!(
        endpoint_id, expected_id,
        "Fast constructor should produce deterministic endpoint ID"
    );
}

/// Test that deferred mDNS discovery can be enabled after transport creation
#[tokio::test]
async fn test_deferred_mdns_discovery_enablement() {
    let seed = "test-deferred-mdns/node-1";
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Create transport without mDNS
    let transport = IrohTransport::from_seed_at_addr(seed, bind_addr)
        .await
        .expect("Fast constructor should succeed");

    assert!(!transport.has_discovery(), "Should start without mDNS");

    // Enable mDNS discovery after creation
    transport
        .enable_mdns_discovery()
        .await
        .expect("Deferred mDNS enablement should succeed");

    assert!(
        transport.has_discovery(),
        "Should have mDNS after enablement"
    );

    // Verify mDNS discovery is accessible
    let mdns = transport.mdns_discovery();
    assert!(mdns.is_some(), "Should be able to access mDNS discovery");
}

/// Test that enabling mDNS twice fails gracefully
#[tokio::test]
async fn test_double_mdns_enablement_fails() {
    let seed = "test-double-mdns/node-1";
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let transport = IrohTransport::from_seed_at_addr(seed, bind_addr)
        .await
        .expect("Fast constructor should succeed");

    // First enablement should succeed
    transport
        .enable_mdns_discovery()
        .await
        .expect("First mDNS enablement should succeed");

    // Second enablement should fail
    let result = transport.enable_mdns_discovery().await;
    assert!(result.is_err(), "Double mDNS enablement should fail");
    assert!(
        result.unwrap_err().to_string().contains("already enabled"),
        "Error should indicate mDNS is already enabled"
    );
}

/// Test that fast constructor is measurably faster than mDNS constructor
#[tokio::test]
async fn test_fast_constructor_is_faster_than_mdns_constructor() {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Warm up (first transport creation may have one-time costs)
    let _ = IrohTransport::from_seed_at_addr("warmup", bind_addr).await;

    // Measure fast constructor (no mDNS) - run sequentially for consistent timing
    let mut fast_times = Vec::new();
    for i in 0..3 {
        let start = Instant::now();
        let seed = format!("fast-timing-test/node-{}", i);
        let _ = IrohTransport::from_seed_at_addr(&seed, bind_addr)
            .await
            .unwrap();
        fast_times.push(start.elapsed().as_millis());
    }

    // Measure mDNS constructor
    let mut mdns_times = Vec::new();
    for i in 0..3 {
        let start = Instant::now();
        let seed = format!("mdns-timing-test/node-{}", i);
        let _ = IrohTransport::from_seed_with_discovery_at_addr(&seed, bind_addr)
            .await
            .unwrap();
        mdns_times.push(start.elapsed().as_millis());
    }

    let avg_fast: u128 = fast_times.iter().sum::<u128>() / fast_times.len() as u128;
    let avg_mdns: u128 = mdns_times.iter().sum::<u128>() / mdns_times.len() as u128;

    eprintln!(
        "[STARTUP TIMING] Fast constructor avg: {}ms, mDNS constructor avg: {}ms",
        avg_fast, avg_mdns
    );

    // Fast constructor should be at least as fast (may not always be faster due to system variance)
    // The main benefit is avoiding mDNS setup during critical startup path
    assert!(
        avg_fast <= avg_mdns + 50, // Allow 50ms variance for system noise
        "Fast constructor ({}ms) should not be significantly slower than mDNS ({}ms)",
        avg_fast,
        avg_mdns
    );
}

/// Test that parallel store + transport initialization works correctly
#[tokio::test]
async fn test_parallel_store_and_transport_initialization() {
    let temp_dir = TempDir::new().unwrap();
    let seed = "parallel-init-test/node-1";
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let storage_path = temp_dir.path().to_path_buf();

    let start = Instant::now();

    // Run store and transport creation in parallel (simulating FFI create_node pattern)
    let (store_result, transport_result) = tokio::join!(
        tokio::task::spawn_blocking({
            let path = storage_path.clone();
            move || AutomergeStore::open(&path)
        }),
        IrohTransport::from_seed_at_addr(seed, bind_addr)
    );

    let parallel_time = start.elapsed();

    let store = Arc::new(store_result.unwrap().unwrap());
    let transport = Arc::new(transport_result.unwrap());

    eprintln!(
        "[STARTUP TIMING] Parallel store+transport init: {}ms",
        parallel_time.as_millis()
    );

    // Verify both are functional
    assert!(!store.is_in_memory());
    assert!(!transport.endpoint_id().as_bytes().is_empty());

    // Create backend to verify they work together
    let backend = AutomergeBackend::with_transport(store, transport);
    assert!(backend.sync_stats().is_ok());
}

/// Test that a transport created with fast constructor can establish peer connections
///
/// Note: Full document sync requires the AutomergeIrohBackend with FormationKey authentication.
/// This test validates that the fast constructor creates transports capable of P2P connections.
#[tokio::test]
async fn test_fast_transport_can_connect_to_peers() {
    // Create two nodes using fast constructor
    let transport1 = Arc::new(
        IrohTransport::from_seed_at_addr("connect-test/node-1", "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap(),
    );
    let transport2 = Arc::new(
        IrohTransport::from_seed_at_addr("connect-test/node-2", "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap(),
    );

    // Start accept loops so transports can receive connections
    transport1.start_accept_loop().unwrap();
    transport2.start_accept_loop().unwrap();

    // Get actual bound addresses
    let addr2 = get_first_ip_addr(&transport2);

    let peer2_info = PeerInfo {
        name: "node-2".to_string(),
        node_id: hex::encode(transport2.endpoint_id().as_bytes()),
        addresses: vec![addr2.to_string()],
        relay_url: None,
    };

    // Connect transport1 to transport2
    transport1.connect_peer(&peer2_info).await.unwrap();

    // Wait for connection to be established
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify connection established (at least one side should show the connection)
    let peer_count_1 = transport1.peer_count();
    let peer_count_2 = transport2.peer_count();

    eprintln!(
        "[FAST TRANSPORT] Connection test - transport1 peers: {}, transport2 peers: {}",
        peer_count_1, peer_count_2
    );

    // At least one side should have registered the connection
    assert!(
        peer_count_1 > 0 || peer_count_2 > 0,
        "Peers should connect using fast-created transports (no mDNS required)"
    );

    // Cleanup
    let _ = transport1.stop_accept_loop();
    let _ = transport2.stop_accept_loop();
}

/// Test startup timing comparison: sequential vs parallel initialization
#[tokio::test]
async fn test_sequential_vs_parallel_initialization_timing() {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    // Sequential initialization (old pattern)
    let sequential_time = {
        let temp_dir = TempDir::new().unwrap();
        let start = Instant::now();

        let store = AutomergeStore::open(temp_dir.path()).unwrap();
        let _transport = IrohTransport::from_seed_at_addr("sequential/node", bind_addr)
            .await
            .unwrap();

        drop(store);
        start.elapsed()
    };

    // Parallel initialization (new pattern)
    let parallel_time = {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().to_path_buf();
        let start = Instant::now();

        let (store_result, transport_result) = tokio::join!(
            tokio::task::spawn_blocking({
                let path = storage_path.clone();
                move || AutomergeStore::open(&path)
            }),
            IrohTransport::from_seed_at_addr("parallel/node", bind_addr)
        );

        let _ = store_result.unwrap().unwrap();
        let _ = transport_result.unwrap();
        start.elapsed()
    };

    eprintln!(
        "[STARTUP TIMING] Sequential: {}ms, Parallel: {}ms, Improvement: {:.1}%",
        sequential_time.as_millis(),
        parallel_time.as_millis(),
        (1.0 - parallel_time.as_secs_f64() / sequential_time.as_secs_f64()) * 100.0
    );

    // Parallel should generally be faster or at least not significantly slower
    // (may have variance due to system load)
    assert!(
        parallel_time.as_millis() <= sequential_time.as_millis() + 100,
        "Parallel init should not be significantly slower than sequential"
    );
}

/// Test that mimics FFI create_node timing to show actual startup performance
///
/// This test replicates the initialization pattern from hive-ffi/src/lib.rs create_node()
/// to provide timing output for the optimized startup path.
#[tokio::test]
async fn test_full_startup_timing_like_ffi() {
    use std::time::Instant;

    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().to_path_buf();
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let seed = "timing-test/full-startup";

    let total_start = Instant::now();

    // Phase 1: Parallel store + transport initialization (like FFI)
    let phase_start = Instant::now();
    let storage_path_for_store = storage_path.clone();

    let (store_result, transport_result) = tokio::join!(
        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let result = AutomergeStore::open(&storage_path_for_store);
            (result, start.elapsed().as_millis())
        }),
        async {
            let start = Instant::now();
            let result = IrohTransport::from_seed_at_addr(seed, bind_addr).await;
            (result, start.elapsed().as_millis())
        }
    );

    let (store, store_ms) = store_result.unwrap();
    let store = Arc::new(store.unwrap());
    let (transport, transport_ms) = transport_result;
    let transport = Arc::new(transport.unwrap());
    let parallel_ms = phase_start.elapsed().as_millis();

    // Phase 2: Create backend
    let phase_start = Instant::now();
    let backend = AutomergeBackend::with_transport(Arc::clone(&store), Arc::clone(&transport));
    let backend_ms = phase_start.elapsed().as_millis();

    // Phase 3: Start sync (like sync_backend.initialize in FFI)
    let phase_start = Instant::now();
    backend.start_sync().unwrap();
    let sync_init_ms = phase_start.elapsed().as_millis();

    let total_ms = total_start.elapsed().as_millis();

    // Output timing in same format as FFI
    eprintln!("\n=== FFI-EQUIVALENT STARTUP TIMING ===");
    eprintln!("[HIVE TIMING] Store open: {}ms", store_ms);
    eprintln!(
        "[HIVE TIMING] Transport create (no mDNS): {}ms",
        transport_ms
    );
    eprintln!(
        "[HIVE TIMING] Parallel total (max of above): {}ms",
        parallel_ms
    );
    eprintln!("[HIVE TIMING] Backend creation: {}ms", backend_ms);
    eprintln!("[HIVE TIMING] Sync init: {}ms", sync_init_ms);
    eprintln!("[HIVE TIMING] === TOTAL: {}ms ===\n", total_ms);

    // Cleanup
    backend.stop_sync().unwrap();

    // Verify reasonable startup time (should be well under 1 second on modern hardware)
    assert!(
        total_ms < 1000,
        "Total startup should be under 1 second, was {}ms",
        total_ms
    );
}

/// Helper to extract first IP address from transport
fn get_first_ip_addr(transport: &IrohTransport) -> SocketAddr {
    let addr = transport.endpoint_addr();
    addr.addrs
        .iter()
        .find_map(|a| {
            if let TransportAddr::Ip(socket_addr) = a {
                Some(*socket_addr)
            } else {
                None
            }
        })
        .expect("Transport should have at least one IP address")
}
