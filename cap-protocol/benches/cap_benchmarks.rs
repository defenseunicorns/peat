//! Performance Benchmarks for CAP Protocol (Phase 5)
//!
//! Comprehensive benchmark suite measuring performance of core CAP operations:
//! - Cell formation throughput at various scales (10, 50, 100 nodes)
//! - Leader election performance across different cell sizes
//! - Capability aggregation speed with varying capability counts
//! - Rebalancing operation costs (merge, split)
//! - CRDT sync latency across multiple peers
//! - Geographic discovery performance
//! - Capability query performance
//!
//! Run with: `cargo bench`
//! View results in: `target/criterion/`

use cap_protocol::discovery::capability_query::{CapabilityQuery, CapabilityQueryEngine};
use cap_protocol::discovery::geographic::{GeographicBeacon, GeographicDiscovery};
use cap_protocol::discovery::GeoCoordinate;
use cap_protocol::models::capability::{Capability, CapabilityType};
use cap_protocol::models::cell::{CellConfig, CellState};
use cap_protocol::models::node::NodeConfig;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashSet;

/// Benchmark 1: Cell Formation Throughput
///
/// Measures the time to form cells from varying numbers of nodes (10, 50, 100).
/// Tests the scalability of the cell formation algorithm.
fn bench_cell_formation_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("cell_formation_throughput");

    for node_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*node_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(node_count),
            node_count,
            |b, &count| {
                b.iter(|| {
                    // Create nodes
                    let nodes: Vec<NodeConfig> = (0..count)
                        .map(|i| {
                            let mut node = NodeConfig::new("UAV".to_string());
                            node.id = format!("node_{}", i);
                            node.add_capability(Capability::new(
                                format!("cap_{}", i),
                                "Test Cap".to_string(),
                                CapabilityType::Sensor,
                                0.9,
                            ));
                            node
                        })
                        .collect();

                    // Form cells (simplified - group into cells of 5)
                    let mut cells = Vec::new();
                    for chunk in nodes.chunks(5) {
                        let config = CellConfig::new(5);
                        let mut cell = CellState::new(config);
                        for node in chunk {
                            cell.members.insert(node.id.clone());
                            for cap in &node.capabilities {
                                cell.capabilities.push(cap.clone());
                            }
                        }
                        cells.push(cell);
                    }

                    black_box(cells)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 2: Leader Election Performance
///
/// Measures leader election time across cells of varying sizes (5, 10, 20 members).
/// Tests the deterministic leader selection algorithm performance.
fn bench_leader_election(c: &mut Criterion) {
    let mut group = c.benchmark_group("leader_election");

    for cell_size in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(cell_size),
            cell_size,
            |b, &size| {
                // Setup: Create cell with members
                let config = CellConfig::new(size);
                let mut cell = CellState::new(config);
                let members: Vec<String> = (0..size).map(|i| format!("node_{}", i)).collect();

                for member in &members {
                    cell.members.insert(member.clone());
                }

                b.iter(|| {
                    // Elect leader (deterministic - select lowest ID)
                    let mut sorted_members: Vec<_> = cell.members.iter().collect();
                    sorted_members.sort();
                    let leader = sorted_members.first().map(|s| (*s).clone());

                    // Update cell
                    let mut test_cell = cell.clone();
                    test_cell.leader_id = leader;
                    test_cell.timestamp += 1;

                    black_box(test_cell)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 3: Capability Aggregation Speed
///
/// Measures the time to aggregate capabilities from multiple nodes (10, 50, 100 capabilities).
/// Tests the capability merging and deduplication logic.
fn bench_capability_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("capability_aggregation");

    for cap_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*cap_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(cap_count),
            cap_count,
            |b, &count| {
                // Setup: Create nodes with capabilities
                let nodes: Vec<NodeConfig> = (0..count)
                    .map(|i| {
                        let mut node = NodeConfig::new("UAV".to_string());
                        node.id = format!("node_{}", i);

                        // Each node has 3-5 capabilities
                        for j in 0..((i % 3) + 3) {
                            node.add_capability(Capability::new(
                                format!("cap_{}_{}", i, j),
                                format!("Capability {}", j),
                                match j % 4 {
                                    0 => CapabilityType::Sensor,
                                    1 => CapabilityType::Communication,
                                    2 => CapabilityType::Compute,
                                    _ => CapabilityType::Mobility,
                                },
                                0.8 + (j as f32 * 0.02),
                            ));
                        }
                        node
                    })
                    .collect();

                b.iter(|| {
                    // Aggregate all capabilities into a cell
                    let mut aggregated = Vec::new();
                    for node in &nodes {
                        aggregated.extend(node.capabilities.clone());
                    }

                    // Deduplicate by ID
                    let mut seen = HashSet::new();
                    aggregated.retain(|cap| seen.insert(cap.id.clone()));

                    black_box(aggregated)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 4: Rebalancing Operation Cost
///
/// Measures the cost of cell merge and split operations.
/// Tests the rebalancing algorithm performance.
fn bench_rebalancing_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("rebalancing_operations");

    // Benchmark cell merge
    group.bench_function("cell_merge", |b| {
        // Setup: Two cells to merge
        let config1 = CellConfig::new(10);
        let mut cell1 = CellState::new(config1);
        let config2 = CellConfig::new(10);
        let mut cell2 = CellState::new(config2);

        for i in 0..3 {
            cell1.members.insert(format!("node_1_{}", i));
            cell1.capabilities.push(Capability::new(
                format!("cap_1_{}", i),
                "Cap".to_string(),
                CapabilityType::Sensor,
                0.9,
            ));
        }

        for i in 0..3 {
            cell2.members.insert(format!("node_2_{}", i));
            cell2.capabilities.push(Capability::new(
                format!("cap_2_{}", i),
                "Cap".to_string(),
                CapabilityType::Communication,
                0.85,
            ));
        }

        b.iter(|| {
            // Merge cell2 into cell1
            let mut merged = cell1.clone();
            for member in &cell2.members {
                merged.members.insert(member.clone());
            }
            merged.capabilities.extend(cell2.capabilities.clone());
            merged.timestamp += 1;

            black_box(merged)
        });
    });

    // Benchmark cell split
    group.bench_function("cell_split", |b| {
        // Setup: Large cell to split
        let config = CellConfig::new(20);
        let mut cell = CellState::new(config);
        for i in 0..10 {
            cell.members.insert(format!("node_{}", i));
            cell.capabilities.push(Capability::new(
                format!("cap_{}", i),
                "Cap".to_string(),
                CapabilityType::Sensor,
                0.9,
            ));
        }

        b.iter(|| {
            // Split into two cells
            let members: Vec<String> = cell.members.iter().cloned().collect();
            let mid = members.len() / 2;

            let config1 = CellConfig::new(10);
            let mut cell1 = CellState::new(config1);
            let config2 = CellConfig::new(10);
            let mut cell2 = CellState::new(config2);

            for (i, member) in members.iter().enumerate() {
                if i < mid {
                    cell1.members.insert(member.clone());
                } else {
                    cell2.members.insert(member.clone());
                }
            }

            // Distribute capabilities
            for (i, cap) in cell.capabilities.iter().enumerate() {
                if i < cell.capabilities.len() / 2 {
                    cell1.capabilities.push(cap.clone());
                } else {
                    cell2.capabilities.push(cap.clone());
                }
            }

            black_box((cell1, cell2))
        });
    });

    group.finish();
}

/// Benchmark 5: CRDT Sync Latency
///
/// Measures simulated CRDT synchronization latency across 2, 5, and 10 peers.
/// Tests the peer-to-peer state reconciliation performance.
fn bench_crdt_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("crdt_sync_latency");

    for peer_count in [2, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(peer_count),
            peer_count,
            |b, &count| {
                // Setup: Create cell states for each peer
                let cells: Vec<CellState> = (0..count)
                    .map(|i| {
                        let config = CellConfig::new(10);
                        let mut cell = CellState::new(config);
                        cell.members.insert(format!("node_{}", i));
                        cell.timestamp = i as u64;
                        cell
                    })
                    .collect();

                b.iter(|| {
                    // Simulate LWW-Register merge (take latest timestamp)
                    let mut merged = cells[0].clone();

                    for cell in &cells[1..] {
                        if cell.timestamp > merged.timestamp {
                            merged = cell.clone();
                        }
                    }

                    // Merge members (OR-Set semantics - union)
                    for cell in &cells {
                        for member in &cell.members {
                            merged.members.insert(member.clone());
                        }
                    }

                    black_box(merged)
                });
            },
        );
    }

    group.finish();
}

/// Bonus Benchmark: Geographic Discovery Performance
///
/// Measures geohash-based clustering and beacon processing speed.
fn bench_geographic_discovery(c: &mut Criterion) {
    let mut group = c.benchmark_group("geographic_discovery");

    for beacon_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*beacon_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(beacon_count),
            beacon_count,
            |b, &count| {
                // Setup: Create beacons
                let beacons: Vec<GeographicBeacon> = (0..count)
                    .map(|i| {
                        let lat = 37.7749 + (i as f64 * 0.001); // Slight variation
                        let lon = -122.4194 + (i as f64 * 0.001);
                        let pos = GeoCoordinate::new(lat, lon, 100.0).unwrap();
                        GeographicBeacon::new(format!("node_{}", i), pos, vec![])
                    })
                    .collect();

                b.iter(|| {
                    let mut discovery = GeographicDiscovery::new("observer".to_string());

                    for beacon in &beacons {
                        discovery.process_beacon(beacon.clone());
                    }

                    black_box(discovery)
                });
            },
        );
    }

    group.finish();
}

/// Bonus Benchmark: Capability Query Performance
///
/// Measures capability-based search and scoring performance.
fn bench_capability_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("capability_query");

    for node_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*node_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(node_count),
            node_count,
            |b, &count| {
                // Setup: Create diverse node fleet
                let nodes: Vec<NodeConfig> = (0..count)
                    .map(|i| {
                        let mut node = NodeConfig::new("UAV".to_string());
                        node.id = format!("node_{}", i);

                        node.add_capability(Capability::new(
                            format!("sensor_{}", i),
                            "Sensor".to_string(),
                            CapabilityType::Sensor,
                            0.7 + ((i % 3) as f32 * 0.1),
                        ));

                        if i % 2 == 0 {
                            node.add_capability(Capability::new(
                                format!("comms_{}", i),
                                "Comms".to_string(),
                                CapabilityType::Communication,
                                0.8,
                            ));
                        }

                        node
                    })
                    .collect();

                let query = CapabilityQuery::builder()
                    .require_type(CapabilityType::Sensor)
                    .prefer_type(CapabilityType::Communication)
                    .min_confidence(0.7)
                    .limit(10)
                    .build();

                let engine = CapabilityQueryEngine::new();

                b.iter(|| {
                    let matches = engine.query_platforms(&query, &nodes);
                    black_box(matches)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_cell_formation_throughput,
    bench_leader_election,
    bench_capability_aggregation,
    bench_rebalancing_operations,
    bench_crdt_sync,
    bench_geographic_discovery,
    bench_capability_query
);

criterion_main!(benches);
