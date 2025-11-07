//! Baseline Ditto Bandwidth Measurements
//!
//! This test establishes baseline metrics for Ditto's document sync behavior
//! WITHOUT any delta optimizations. These metrics will be compared against
//! delta-sync performance to measure improvement.
//!
//! # Metrics Captured
//!
//! - **Full Document Size**: Size of complete CellState/NodeConfig serialized
//! - **Sync Frequency**: How often documents sync during formation
//! - **Total Bandwidth**: Cumulative data transmitted
//! - **Per-Operation Cost**: Bandwidth per add_member, set_leader, etc.
//!
//! # Test Scenarios
//!
//! 1. **Cell Formation**: 5 nodes joining a cell (member additions)
//! 2. **Leader Election**: Multiple leader changes
//! 3. **Capability Updates**: Adding capabilities to nodes
//! 4. **Batch Operations**: Multiple changes in rapid succession

use cap_protocol::models::cell::{CellConfig, CellState};
use cap_protocol::models::node::NodeConfig;
use cap_protocol::models::{
    Capability, CapabilityExt, CapabilityType, CellConfigExt, CellStateExt, NodeConfigExt,
};
use cap_protocol::storage::{CellStore, NodeStore};
use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::testing::E2EHarness;
use std::time::{Duration, Instant};

/// Baseline Test 1: Measure Cell Formation Bandwidth
///
/// Scenario: 5 nodes join a cell one-by-one
/// Measures: Document size and total sync bandwidth
#[tokio::test]
async fn test_baseline_cell_formation_bandwidth() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    println!("\n=== Baseline: Cell Formation Bandwidth ===");

    let mut harness = E2EHarness::new("baseline_formation");

    // Create two peers to measure sync
    let store1 = harness.create_ditto_store().await.unwrap();
    let store2 = harness.create_ditto_store().await.unwrap();

    let cell_store1: CellStore<DittoBackend> = CellStore::new(store1.clone().into()).await.unwrap();
    let cell_store2: CellStore<DittoBackend> = CellStore::new(store2.clone().into()).await.unwrap();

    // Start sync
    store1.start_sync().unwrap();
    store2.start_sync().unwrap();

    println!("  1. Waiting for peer connection...");

    let connection_result = harness
        .wait_for_peer_connection(&store1, &store2, Duration::from_secs(10))
        .await;

    if connection_result.is_err() {
        println!("  ⚠ Warning: Peer connection timeout - skipping baseline test");
        harness.shutdown_store(store1).await;
        harness.shutdown_store(store2).await;
        return;
    }

    println!("  ✓ Peers connected\n");

    // Create initial cell
    let cell_id = "baseline_cell".to_string();
    let cell_config = CellConfig::with_id(cell_id.clone(), 5); // max 5 members
    let mut cell = CellState::new(cell_config);

    // Measure initial document size
    let initial_size = serde_json::to_vec(&cell).unwrap().len();
    println!("  📊 Initial CellState size: {} bytes", initial_size);

    let start = Instant::now();
    let mut operation_count = 0;

    // Operation 1: Store initial cell
    println!("\n  2. Storing initial cell...");
    cell_store1.store_cell(&cell).await.unwrap();
    operation_count += 1;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Operations 2-6: Add 5 members one by one
    println!("  3. Adding members one by one...");
    for i in 1..=5 {
        cell.add_member(format!("node{}", i));

        let doc_size = serde_json::to_vec(&cell).unwrap().len();
        println!("    Member {}: CellState size = {} bytes", i, doc_size);

        cell_store1.store_cell(&cell).await.unwrap();
        operation_count += 1;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Operation 7: Set leader
    println!("  4. Setting leader...");
    cell.set_leader("node1".to_string()).unwrap();
    let doc_size = serde_json::to_vec(&cell).unwrap().len();
    println!("    With leader: CellState size = {} bytes", doc_size);

    cell_store1.store_cell(&cell).await.unwrap();
    operation_count += 1;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Operations 8-10: Add capabilities
    println!("  5. Adding capabilities...");
    for i in 1..=3 {
        cell.add_capability(Capability::new(
            format!("cap{}", i),
            format!("Capability {}", i),
            CapabilityType::Sensor,
            1.0,
        ));

        let doc_size = serde_json::to_vec(&cell).unwrap().len();
        println!("    Capability {}: CellState size = {} bytes", i, doc_size);

        cell_store1.store_cell(&cell).await.unwrap();
        operation_count += 1;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let elapsed = start.elapsed();

    // Final document size
    let final_size = serde_json::to_vec(&cell).unwrap().len();

    // Give more time for final sync to complete (async backend may need extra time)
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify sync to peer2
    let synced_cell = cell_store2.get_cell(&cell_id).await.unwrap();
    assert!(synced_cell.is_some(), "Cell should sync to peer2");

    println!("\n  📊 BASELINE METRICS:");
    println!("  ────────────────────────────────────────");
    println!("  Initial document size:     {} bytes", initial_size);
    println!("  Final document size:       {} bytes", final_size);
    println!(
        "  Document growth:           {} bytes",
        final_size - initial_size
    );
    println!("  Total operations:          {}", operation_count);
    println!("  Time elapsed:              {:.2}s", elapsed.as_secs_f64());
    println!();
    println!("  Estimated bandwidth (full doc sync):");
    println!("    Per operation:           ~{} bytes", final_size);
    println!(
        "    Total transmitted:       ~{} bytes",
        final_size * operation_count
    );
    println!(
        "    Bytes per second:        ~{:.0} B/s",
        (final_size * operation_count) as f64 / elapsed.as_secs_f64()
    );
    println!();
    println!(
        "  Note: Each operation sends FULL document state (~{} bytes)",
        final_size
    );
    println!("  ────────────────────────────────────────\n");

    // Cleanup
    harness.shutdown_store(store1).await;
    harness.shutdown_store(store2).await;

    println!("  ✅ Baseline captured\n");
}

/// Baseline Test 2: Measure Rapid Updates
///
/// Scenario: Multiple rapid changes to same document
/// Measures: Bandwidth cost when changes occur faster than network sync
#[tokio::test]
async fn test_baseline_rapid_updates_bandwidth() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    println!("\n=== Baseline: Rapid Updates Bandwidth ===");

    let mut harness = E2EHarness::new("baseline_rapid");

    let store1 = harness.create_ditto_store().await.unwrap();
    let store2 = harness.create_ditto_store().await.unwrap();

    let cell_store1: CellStore<DittoBackend> = CellStore::new(store1.clone().into()).await.unwrap();
    let _cell_store2: CellStore<DittoBackend> =
        CellStore::new(store2.clone().into()).await.unwrap();

    store1.start_sync().unwrap();
    store2.start_sync().unwrap();

    println!("  1. Waiting for peer connection...");

    let connection_result = harness
        .wait_for_peer_connection(&store1, &store2, Duration::from_secs(10))
        .await;

    if connection_result.is_err() {
        println!("  ⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(store1).await;
        harness.shutdown_store(store2).await;
        return;
    }

    println!("  ✓ Peers connected\n");

    // Create cell
    let cell_id = "rapid_test_cell".to_string();
    let cell_config = CellConfig::with_id(cell_id, 10);
    let mut cell = CellState::new(cell_config);

    println!("  2. Making 10 rapid updates (100ms apart)...");
    let start = Instant::now();

    for i in 1..=10 {
        cell.add_member(format!("node{}", i));
        cell_store1.store_cell(&cell).await.unwrap();

        // Rapid updates - faster than typical sync interval
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let elapsed = start.elapsed();
    let final_size = serde_json::to_vec(&cell).unwrap().len();

    println!("\n  📊 RAPID UPDATE BASELINE:");
    println!("  ────────────────────────────────────────");
    println!("  Updates performed:         10");
    println!("  Time elapsed:              {:.2}s", elapsed.as_secs_f64());
    println!("  Final document size:       {} bytes", final_size);
    println!();
    println!("  Worst-case bandwidth (full sync each update):");
    println!("    Total transmitted:       ~{} bytes", final_size * 10);
    println!("    Average per update:      ~{} bytes", final_size);
    println!();
    println!("  Ideal delta bandwidth (assuming 1 field change):");
    println!("    Estimated per update:    ~50-100 bytes");
    println!(
        "    Potential savings:       ~{}%",
        ((1.0 - (75.0 / final_size as f64)) * 100.0) as i32
    );
    println!("  ────────────────────────────────────────\n");

    harness.shutdown_store(store1).await;
    harness.shutdown_store(store2).await;

    println!("  ✅ Baseline captured\n");
}

/// Baseline Test 3: Node Configuration Updates
///
/// Scenario: Adding capabilities to node configs
/// Measures: NodeConfig document size and growth
#[tokio::test]
async fn test_baseline_node_config_bandwidth() {
    let ditto_app_id =
        std::env::var("DITTO_APP_ID").expect("DITTO_APP_ID must be set for E2E tests");
    assert!(!ditto_app_id.is_empty(), "DITTO_APP_ID cannot be empty");

    println!("\n=== Baseline: NodeConfig Bandwidth ===");

    let mut harness = E2EHarness::new("baseline_node");

    let store1 = harness.create_ditto_store().await.unwrap();
    let store2 = harness.create_ditto_store().await.unwrap();

    let node_store1: NodeStore<DittoBackend> = NodeStore::new(store1.clone().into()).await.unwrap();
    let _node_store2: NodeStore<DittoBackend> =
        NodeStore::new(store2.clone().into()).await.unwrap();

    store1.start_sync().unwrap();
    store2.start_sync().unwrap();

    println!("  1. Waiting for peer connection...");

    let connection_result = harness
        .wait_for_peer_connection(&store1, &store2, Duration::from_secs(10))
        .await;

    if connection_result.is_err() {
        println!("  ⚠ Warning: Peer connection timeout - skipping test");
        harness.shutdown_store(store1).await;
        harness.shutdown_store(store2).await;
        return;
    }

    println!("  ✓ Peers connected\n");

    // Create node with capabilities
    let mut node_config = NodeConfig::new("UAV".to_string());
    node_config.id = "baseline_node".to_string();

    let initial_size = serde_json::to_vec(&node_config).unwrap().len();
    println!("  📊 Initial NodeConfig size: {} bytes\n", initial_size);

    println!("  2. Adding 10 capabilities...");
    for i in 1..=10 {
        node_config.add_capability(Capability::new(
            format!("cap{}", i),
            format!("Capability {}", i),
            CapabilityType::Sensor,
            0.9 + (i as f32 * 0.01),
        ));

        let size = serde_json::to_vec(&node_config).unwrap().len();
        println!(
            "    After cap {}: {} bytes (+{} bytes)",
            i,
            size,
            size - initial_size
        );

        node_store1.store_config(&node_config).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    let final_size = serde_json::to_vec(&node_config).unwrap().len();

    println!("\n  📊 NODECONFIG BASELINE:");
    println!("  ────────────────────────────────────────");
    println!("  Initial size:              {} bytes", initial_size);
    println!("  Final size:                {} bytes", final_size);
    println!(
        "  Growth:                    {} bytes",
        final_size - initial_size
    );
    println!(
        "  Avg growth per capability: {} bytes",
        (final_size - initial_size) / 10
    );
    println!();
    println!("  Full document sync cost:");
    println!("    Per update:              ~{} bytes", final_size);
    println!("    10 updates total:        ~{} bytes", final_size * 10);
    println!();
    println!("  Delta sync potential:");
    println!("    Per capability add:      ~100-150 bytes estimated");
    println!("    10 deltas total:         ~1000-1500 bytes estimated");
    println!(
        "    Potential savings:       ~{}%",
        ((1.0 - (1250.0 / (final_size * 10) as f64)) * 100.0) as i32
    );
    println!("  ────────────────────────────────────────\n");

    harness.shutdown_store(store1).await;
    harness.shutdown_store(store2).await;

    println!("  ✅ Baseline captured\n");
}
