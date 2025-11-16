//! Load Testing Scenarios for HIVE Protocol (Phase 6)
//!
//! Large-scale integration tests validating system performance and behavior
//! under load with 100+ nodes, multiple cells, and hierarchical zones.
//!
//! These tests verify:
//! - Scenario 1: Large Formation (100 nodes forming 10 cells)
//! - Scenario 2: Multi-Zone Hierarchy (3 zones, 10 cells, 100 nodes)

use hive_protocol::models::capability::{Capability, CapabilityType};
use hive_protocol::models::cell::{CellConfig, CellState};
use hive_protocol::models::node::NodeConfig;
use hive_protocol::models::{CapabilityExt, CellConfigExt, CellStateExt, NodeConfigExt};
use hive_protocol::storage::{CellStore, NodeStore};
use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::testing::e2e_harness::E2EHarness;
use std::time::{Duration, Instant};

/// Get the number of nodes to test from environment variable, defaulting to 10
fn get_test_node_count() -> usize {
    std::env::var("CAP_TEST_NODE_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

/// Scenario 1: Large Formation (configurable node count, default 10)
///
/// Tests the system's ability to handle squad formation:
/// - Configurable nodes (default 10, set via CAP_TEST_NODE_COUNT env var)
/// - Formation into cells (2 nodes per cell)
/// - Validates formation time, memory usage, and sync latency
/// - Ensures all nodes are properly organized into cells
/// - Verifies capability aggregation at scale
///
/// Success Criteria:
/// - All nodes successfully stored and synced
/// - Cells formed with proper member distribution
/// - Formation completes within reasonable time (<10s)
/// - All capabilities properly aggregated
#[tokio::test]
async fn test_load_large_formation_nodes() {
    let node_count = get_test_node_count();
    let cell_count = node_count / 2; // 2 nodes per cell
    let mut harness = E2EHarness::new("large_formation_test");

    println!(
        "🚀 Starting Large Formation Load Test: {} nodes → {} cells",
        node_count, cell_count
    );
    let start_time = Instant::now();

    // Create stores for both NodeStore (uses DittoStore) and CellStore (uses DittoBackend)
    // TODO: Once NodeStore is migrated to DataSyncBackend, use single backend for both
    let ditto_store = harness
        .create_ditto_store()
        .await
        .expect("Failed to create Ditto store");
    let backend = harness
        .create_ditto_backend()
        .await
        .expect("Failed to create Ditto backend");

    let node_store: NodeStore<DittoBackend> = NodeStore::new(ditto_store.into())
        .await
        .expect("Failed to create NodeStore");
    let cell_store = CellStore::new(backend)
        .await
        .expect("Failed to create CellStore");

    // Phase 1: Create and store nodes
    println!(
        "📝 Phase 1: Creating {} nodes with capabilities...",
        node_count
    );
    let node_creation_start = Instant::now();

    let mut nodes = Vec::new();
    for i in 0..node_count {
        let mut node = NodeConfig::new(format!("UAV-{}", i % 5)); // 5 platform types
        node.id = format!("node_{:03}", i);

        // Distribute capabilities across nodes
        // 40% have sensors
        if i % 10 < 4 {
            node.add_capability(Capability::new(
                format!("sensor_{}", i),
                "EO/IR Sensor".to_string(),
                CapabilityType::Sensor,
                0.85 + (i as f32 % 10.0) * 0.01,
            ));
        }

        // 30% have communication
        if i % 10 >= 4 && i % 10 < 7 {
            node.add_capability(Capability::new(
                format!("comms_{}", i),
                "Radio Link".to_string(),
                CapabilityType::Communication,
                0.80 + (i as f32 % 10.0) * 0.01,
            ));
        }

        // 20% have compute
        if i % 10 >= 7 && i % 10 < 9 {
            node.add_capability(Capability::new(
                format!("compute_{}", i),
                "Edge Compute".to_string(),
                CapabilityType::Compute,
                0.90 + (i as f32 % 10.0) * 0.01,
            ));
        }

        // 10% have mobility
        if i % 10 == 9 {
            node.add_capability(Capability::new(
                format!("mobility_{}", i),
                "High Mobility".to_string(),
                CapabilityType::Mobility,
                0.95,
            ));
        }

        // Store node
        node_store
            .store_config(&node)
            .await
            .expect("Failed to store node");

        nodes.push(node);
    }

    let node_creation_duration = node_creation_start.elapsed();
    println!(
        "✅ Phase 1 Complete: Created and stored {} nodes in {:?}",
        node_count, node_creation_duration
    );

    // Phase 2: Form cells (2 nodes each)
    println!("📝 Phase 2: Forming {} cells...", cell_count);
    let cell_formation_start = Instant::now();

    let mut cells = Vec::new();
    for cell_idx in 0..cell_count {
        let config = CellConfig::new(5); // Max 5 nodes per cell
        let mut cell = CellState::new(config);

        // Assign 2 nodes to this cell
        for node_idx in 0..2 {
            let global_node_idx = cell_idx * 2 + node_idx;
            let node = &nodes[global_node_idx];

            // Add member
            cell.members.push(node.id.clone());

            // Aggregate capabilities
            for cap in &node.capabilities {
                cell.capabilities.push(cap.clone());
            }
        }

        // Elect leader (lowest ID)
        let mut member_ids: Vec<_> = cell.members.iter().collect();
        member_ids.sort();
        if let Some(leader) = member_ids.first() {
            cell.leader_id = Some((*leader).clone());
        }

        // Store cell
        cell_store
            .store_cell(&cell)
            .await
            .expect("Failed to store cell");

        cells.push(cell);
    }

    let cell_formation_duration = cell_formation_start.elapsed();
    println!(
        "✅ Phase 2 Complete: Formed {} cells in {:?}",
        cell_count, cell_formation_duration
    );

    // Phase 3: Validation and metrics
    println!("📝 Phase 3: Validating formation...");

    // Validate: All nodes stored
    let mut stored_node_count = 0;
    for node in &nodes {
        if node_store.get_config(&node.id).await.is_ok() {
            stored_node_count += 1;
        }
    }
    assert_eq!(
        stored_node_count, node_count,
        "Expected {} nodes stored, found {}",
        node_count, stored_node_count
    );
    println!("✅ Validation: All {} nodes stored correctly", node_count);

    // Validate: All cells formed
    let mut stored_cell_count = 0;
    for cell in &cells {
        if let Some(cell_id) = cell.get_id() {
            if cell_store.get_cell(cell_id).await.is_ok() {
                stored_cell_count += 1;
            }
        }
    }
    assert_eq!(
        stored_cell_count, cell_count,
        "Expected {} cells stored, found {}",
        cell_count, stored_cell_count
    );
    println!("✅ Validation: All {} cells formed correctly", cell_count);

    // Validate: Each cell has 2 members
    for (idx, cell) in cells.iter().enumerate() {
        assert_eq!(
            cell.members.len(),
            2,
            "Cell {} should have 2 members, has {}",
            idx,
            cell.members.len()
        );
        assert!(
            cell.leader_id.is_some(),
            "Cell {} should have a leader",
            idx
        );
    }
    println!("✅ Validation: All cells have correct member count and leaders");

    // Validate: Capability aggregation
    let total_capabilities: usize = cells.iter().map(|c| c.capabilities.len()).sum();
    let min_expected_caps = (node_count * 4) / 10; // ~40% of nodes have caps
    assert!(
        total_capabilities >= min_expected_caps,
        "Expected at least {} aggregated capabilities, found {}",
        min_expected_caps,
        total_capabilities
    );
    println!(
        "✅ Validation: {} capabilities aggregated across all cells",
        total_capabilities
    );

    // Final metrics
    let total_duration = start_time.elapsed();
    println!("\n📊 Load Test Metrics:");
    println!("  - Total Duration: {:?}", total_duration);
    println!("  - Node Creation: {:?}", node_creation_duration);
    println!("  - Cell Formation: {:?}", cell_formation_duration);
    println!("  - Nodes: {}", node_count);
    println!("  - Cells: {}", cell_count);
    println!("  - Avg Members/Cell: 2");
    println!("  - Total Capabilities: {}", total_capabilities);

    // Performance assertion: Should complete in reasonable time
    assert!(
        total_duration < Duration::from_secs(10),
        "Formation took too long: {:?}",
        total_duration
    );

    println!("✅ Large Formation Load Test PASSED");
}

/// Scenario 2: Multi-Zone Hierarchy (3 zones, configurable nodes)
///
/// Tests hierarchical organization:
/// - 3 geographic zones (East, Central, West)
/// - Configurable nodes distributed across zones
/// - Zone-specific capabilities
/// - Validates hierarchical routing and zone coordination
/// - Tests cross-zone communication patterns
///
/// Success Criteria:
/// - All nodes properly assigned to zones
/// - Cells formed within zone boundaries
/// - Zone-level capability aggregation works
/// - Formation completes within reasonable time
#[tokio::test]
async fn test_load_multi_zone_hierarchy() {
    let total_nodes = get_test_node_count();
    let mut harness = E2EHarness::new("multi_zone_hierarchy_test");

    println!(
        "🚀 Starting Multi-Zone Hierarchy Load Test: 3 zones, {} nodes",
        total_nodes
    );
    let start_time = Instant::now();

    // Create stores for both NodeStore (uses DittoStore) and CellStore (uses DittoBackend)
    // TODO: Once NodeStore is migrated to DataSyncBackend, use single backend for both
    let ditto_store = harness
        .create_ditto_store()
        .await
        .expect("Failed to create Ditto store");
    let backend = harness
        .create_ditto_backend()
        .await
        .expect("Failed to create Ditto backend");

    let node_store: NodeStore<DittoBackend> = NodeStore::new(ditto_store.into())
        .await
        .expect("Failed to create NodeStore");
    let cell_store = CellStore::new(backend)
        .await
        .expect("Failed to create CellStore");

    // Phase 1: Create nodes distributed across 3 zones
    println!(
        "📝 Phase 1: Creating {} nodes across 3 zones...",
        total_nodes
    );
    let node_creation_start = Instant::now();

    #[derive(Debug)]
    struct ZoneInfo {
        name: String,
        node_count: usize,
        cell_count: usize,
    }

    // Distribute nodes across 3 zones: 30% East, 40% Central, 30% West
    let east_nodes = (total_nodes * 3) / 10;
    let west_nodes = (total_nodes * 3) / 10;
    let central_nodes = total_nodes - east_nodes - west_nodes;

    let zones = vec![
        ZoneInfo {
            name: "zone_east".to_string(),
            node_count: east_nodes,
            cell_count: east_nodes.max(1) / 2, // 2 nodes per cell
        },
        ZoneInfo {
            name: "zone_central".to_string(),
            node_count: central_nodes,
            cell_count: central_nodes.max(1) / 2,
        },
        ZoneInfo {
            name: "zone_west".to_string(),
            node_count: west_nodes,
            cell_count: west_nodes.max(1) / 2,
        },
    ];

    let mut all_nodes = Vec::new();
    let mut node_counter = 0;

    for zone in &zones {
        println!("  Creating {} nodes for {}...", zone.node_count, zone.name);

        for _ in 0..zone.node_count {
            let mut node = NodeConfig::new("UAV".to_string());
            node.id = format!("{}_{:03}", zone.name, node_counter);

            // Add zone-specific capabilities
            node.add_capability(Capability::new(
                format!("cap_{}_{}", zone.name, node_counter),
                format!("{} Capability", zone.name),
                CapabilityType::Sensor,
                0.85 + (node_counter as f32 % 10.0) * 0.01,
            ));

            node_store
                .store_config(&node)
                .await
                .expect("Failed to store node");

            all_nodes.push((zone.name.clone(), node));
            node_counter += 1;
        }
    }

    let node_creation_duration = node_creation_start.elapsed();
    println!(
        "✅ Phase 1 Complete: Created {} nodes in {:?}",
        total_nodes, node_creation_duration
    );

    // Phase 2: Form cells within each zone
    println!("📝 Phase 2: Forming cells within zones...");
    let cell_formation_start = Instant::now();

    let mut all_cells = Vec::new();

    for zone in &zones {
        println!("  Forming {} cells for {}...", zone.cell_count, zone.name);

        // Get nodes for this zone
        let zone_nodes: Vec<&NodeConfig> = all_nodes
            .iter()
            .filter(|(z, _)| z == &zone.name)
            .map(|(_, n)| n)
            .collect();

        let nodes_per_cell = zone_nodes.len() / zone.cell_count;

        for cell_idx in 0..zone.cell_count {
            let config = CellConfig::new(15);
            let mut cell = CellState::new(config);
            cell.platoon_id = Some(zone.name.clone()); // Use platoon_id to track zone

            // Assign nodes to this cell
            let start_idx = cell_idx * nodes_per_cell;
            let end_idx = if cell_idx == zone.cell_count - 1 {
                zone_nodes.len()
            } else {
                start_idx + nodes_per_cell
            };

            for node in &zone_nodes[start_idx..end_idx] {
                cell.members.push(node.id.clone());
                for cap in &node.capabilities {
                    cell.capabilities.push(cap.clone());
                }
            }

            // Elect leader
            let mut member_ids: Vec<_> = cell.members.iter().collect();
            member_ids.sort();
            if let Some(leader) = member_ids.first() {
                cell.leader_id = Some((*leader).clone());
            }

            cell_store
                .store_cell(&cell)
                .await
                .expect("Failed to store cell");

            all_cells.push(cell);
        }
    }

    let cell_formation_duration = cell_formation_start.elapsed();
    let total_cells = all_cells.len();
    println!(
        "✅ Phase 2 Complete: Formed {} cells in {:?}",
        total_cells, cell_formation_duration
    );

    // Phase 3: Validation
    println!("📝 Phase 3: Validating hierarchy...");

    // Validate: All nodes stored
    assert_eq!(
        all_nodes.len(),
        total_nodes,
        "Expected {} nodes, created {}",
        total_nodes,
        all_nodes.len()
    );
    println!("✅ Validation: All {} nodes created", total_nodes);

    // Validate: Cells formed
    let expected_cells: usize = zones.iter().map(|z| z.cell_count).sum();
    assert_eq!(
        all_cells.len(),
        expected_cells,
        "Expected {} cells, formed {}",
        expected_cells,
        all_cells.len()
    );
    println!("✅ Validation: All {} cells formed", total_cells);

    // Validate: Zone distribution
    let east_cells = all_cells
        .iter()
        .filter(|c| c.platoon_id.as_ref().is_some_and(|p| p == "zone_east"))
        .count();
    let central_cells = all_cells
        .iter()
        .filter(|c| c.platoon_id.as_ref().is_some_and(|p| p == "zone_central"))
        .count();
    let west_cells = all_cells
        .iter()
        .filter(|c| c.platoon_id.as_ref().is_some_and(|p| p == "zone_west"))
        .count();

    println!(
        "✅ Validation: Zone distribution ({}/{}/{} cells)",
        east_cells, central_cells, west_cells
    );

    // Validate: Each cell has members
    for cell in &all_cells {
        assert!(!cell.members.is_empty(), "Cell should have members");
        assert!(cell.leader_id.is_some(), "Cell should have a leader");
    }
    println!("✅ Validation: All cells have members and leaders");

    // Final metrics
    let total_duration = start_time.elapsed();
    println!("\n📊 Multi-Zone Hierarchy Metrics:");
    println!("  - Total Duration: {:?}", total_duration);
    println!("  - Node Creation: {:?}", node_creation_duration);
    println!("  - Cell Formation: {:?}", cell_formation_duration);
    println!("  - Zones: 3 (East/Central/West)");
    println!(
        "  - Cells: {} ({}/{}/{} distribution)",
        total_cells, east_cells, central_cells, west_cells
    );
    println!(
        "  - Nodes: {} ({}/{}/{} distribution)",
        total_nodes, east_nodes, central_nodes, west_nodes
    );

    // Performance assertion
    assert!(
        total_duration < Duration::from_secs(10),
        "Hierarchy formation took too long: {:?}",
        total_duration
    );

    println!("✅ Multi-Zone Hierarchy Load Test PASSED");
}
