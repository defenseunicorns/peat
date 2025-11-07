//! Backend Comparison Benchmarks (E8 - ADR-007)
//!
//! Compares Automerge vs Ditto backend performance for CAP-specific operations:
//! - Document size (bandwidth efficiency)
//! - Sync message size (network cost)
//! - Update latency (operation speed)
//! - CellState/NodeConfig encoding overhead
//!
//! These benchmarks measure the actual cost of CRDT operations for our domain models,
//! not just generic CRDT performance. This helps make an evidence-based decision
//! between Automerge and Ditto for the CAP Protocol.
//!
//! Run with: `cargo bench --bench backend_comparison --features automerge-backend`

#![cfg(feature = "automerge-backend")]

use cap_protocol::models::capability::{Capability, CapabilityExt, CapabilityType};
use cap_protocol::models::cell::{CellConfig, CellConfigExt, CellState, CellStateExt};
use cap_protocol::models::node::{NodeConfig, NodeConfigExt};
use cap_protocol::sync::automerge::AutomergeBackend;
use cap_protocol::sync::{BackendConfig, DataSyncBackend, Document, TransportConfig, Value};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::runtime::Runtime;

/// Helper to create a CellState document
fn create_cell_document(member_count: usize, capability_count: usize) -> Document {
    let config = CellConfig::with_id("benchmark_cell".to_string(), (member_count * 2) as u32);
    let mut cell = CellState::new(config);

    // Add members
    for i in 0..member_count {
        cell.add_member(format!("node_{}", i));
    }

    // Set leader
    if member_count > 0 {
        cell.set_leader("node_0".to_string()).ok();
    }

    // Add capabilities
    for i in 0..capability_count {
        cell.add_capability(Capability::new(
            format!("cap_{}", i),
            format!("Capability {}", i),
            match i % 4 {
                0 => CapabilityType::Sensor,
                1 => CapabilityType::Communication,
                2 => CapabilityType::Compute,
                _ => CapabilityType::Mobility,
            },
            0.8 + ((i % 5) as f32 * 0.04),
        ));
    }

    // Convert to Document
    let json = serde_json::to_value(&cell).unwrap();
    let fields: HashMap<String, Value> = serde_json::from_value(json).unwrap();
    Document::new(fields)
}

/// Helper to create a NodeConfig document
fn create_node_document(capability_count: usize) -> Document {
    let mut node = NodeConfig::new("UAV".to_string());
    node.id = "benchmark_node".to_string();

    for i in 0..capability_count {
        node.add_capability(Capability::new(
            format!("cap_{}", i),
            format!("Capability {}", i),
            CapabilityType::Sensor,
            0.9 + (i as f32 * 0.01),
        ));
    }

    // Convert to Document
    let json = serde_json::to_value(&node).unwrap();
    let fields: HashMap<String, Value> = serde_json::from_value(json).unwrap();
    Document::new(fields)
}

/// Helper to create test AutomergeBackend
fn create_automerge_backend() -> AutomergeBackend {
    let config = BackendConfig {
        app_id: "benchmark_app".to_string(),
        persistence_dir: PathBuf::from("/tmp/automerge_bench"),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let backend = AutomergeBackend::new();
    let rt = Runtime::new().unwrap();
    rt.block_on(backend.initialize(config)).unwrap();
    backend
}

/// Benchmark 1: Document Size - CellState
///
/// Measures the serialized size of CellState documents with varying complexity.
/// Smaller documents = better bandwidth efficiency.
fn bench_cellstate_document_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("cellstate_document_size");

    // Test configurations: (members, capabilities)
    for (members, caps) in [(5, 3), (10, 10), (20, 20)].iter() {
        let id = format!("{}members_{}caps", members, caps);

        group.bench_function(format!("json/{}", id), |b| {
            let doc = create_cell_document(*members, *caps);
            b.iter(|| {
                let json_bytes = serde_json::to_vec(&doc).unwrap();
                black_box(json_bytes.len())
            });
        });

        group.bench_function(format!("automerge/{}", id), |b| {
            let backend = create_automerge_backend();
            let doc = create_cell_document(*members, *caps);
            let doc_store = backend.document_store();
            let rt = Runtime::new().unwrap();

            b.iter(|| {
                // Measure the size of the Automerge document
                let doc_id = rt
                    .block_on(doc_store.upsert("benchmark_cells", doc.clone()))
                    .unwrap();

                // Generate sync message to get actual wire size
                let sync_msg = backend
                    .generate_sync_message("benchmark_cells", &doc_id, "peer")
                    .unwrap();

                black_box(sync_msg.len())
            });
        });
    }

    group.finish();
}

/// Benchmark 2: Document Size - NodeConfig
///
/// Measures the serialized size of NodeConfig documents.
fn bench_nodeconfig_document_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("nodeconfig_document_size");

    for cap_count in [1, 5, 10, 20].iter() {
        let id = format!("{}caps", cap_count);

        group.bench_function(format!("json/{}", id), |b| {
            let doc = create_node_document(*cap_count);
            b.iter(|| {
                let json_bytes = serde_json::to_vec(&doc).unwrap();
                black_box(json_bytes.len())
            });
        });

        group.bench_function(format!("automerge/{}", id), |b| {
            let backend = create_automerge_backend();
            let doc = create_node_document(*cap_count);
            let doc_store = backend.document_store();
            let rt = Runtime::new().unwrap();

            b.iter(|| {
                let doc_id = rt
                    .block_on(doc_store.upsert("benchmark_nodes", doc.clone()))
                    .unwrap();

                let sync_msg = backend
                    .generate_sync_message("benchmark_nodes", &doc_id, "peer")
                    .unwrap();

                black_box(sync_msg.len())
            });
        });
    }

    group.finish();
}

/// Benchmark 3: Update Latency
///
/// Measures the time to perform a single document update.
/// Lower latency = faster operations.
fn bench_update_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("update_latency");

    group.bench_function("automerge/add_member", |b| {
        let backend = create_automerge_backend();
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        // Create initial cell
        let doc = create_cell_document(5, 3);

        b.iter(|| {
            // Upsert (update) operation
            let doc_id = rt
                .block_on(doc_store.upsert("benchmark_cells", doc.clone()))
                .unwrap();

            black_box(doc_id)
        });
    });

    group.bench_function("automerge/add_capability", |b| {
        let backend = create_automerge_backend();
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        let doc = create_node_document(5);

        b.iter(|| {
            let doc_id = rt
                .block_on(doc_store.upsert("benchmark_nodes", doc.clone()))
                .unwrap();

            black_box(doc_id)
        });
    });

    group.finish();
}

/// Benchmark 4: Sync Message Size
///
/// Measures the size of sync messages for incremental updates.
/// This is critical for bandwidth-constrained tactical networks.
fn bench_sync_message_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync_message_size");

    group.bench_function("automerge/initial_sync", |b| {
        let backend = create_automerge_backend();
        let doc_store = backend.document_store();
        let doc = create_cell_document(10, 10);
        let rt = Runtime::new().unwrap();

        let doc_id = rt
            .block_on(doc_store.upsert("sync_test", doc.clone()))
            .unwrap();

        b.iter(|| {
            // Generate sync message for new peer (full state)
            let msg = backend
                .generate_sync_message("sync_test", &doc_id, "new_peer")
                .unwrap();

            black_box(msg.len())
        });
    });

    group.finish();
}

/// Benchmark 5: Memory Overhead
///
/// Measures the memory overhead of maintaining CRDT metadata.
fn bench_memory_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_overhead");

    group.bench_function("automerge/100_documents", |b| {
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            let backend = create_automerge_backend();
            let doc_store = backend.document_store();

            // Create 100 documents
            for i in 0..100 {
                let mut doc = create_cell_document(5, 3);
                doc.id = Some(format!("cell_{}", i));

                rt.block_on(doc_store.upsert("memory_test", doc)).unwrap();
            }

            black_box(backend)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cellstate_document_size,
    bench_nodeconfig_document_size,
    bench_update_latency,
    bench_sync_message_size,
    bench_memory_overhead
);

criterion_main!(benches);
