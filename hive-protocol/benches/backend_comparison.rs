//! Backend Comparison Benchmarks (Issue #154)
//!
//! Compares Ditto vs Automerge backend performance for CAP-specific operations:
//! - Document operations (insert, update, query)
//! - Serialization overhead (wire size)
//! - Update latency
//!
//! These benchmarks measure the actual cost of CRDT operations for our domain models,
//! helping make an evidence-based decision between backends for the HIVE Protocol.
//!
//! Run with:
//!   cargo bench --bench backend_comparison                           # Ditto only
//!   cargo bench --bench backend_comparison --features automerge-backend # Both backends

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use hive_protocol::models::capability::{Capability, CapabilityExt, CapabilityType};
use hive_protocol::models::cell::{CellConfig, CellConfigExt, CellState, CellStateExt};
use hive_protocol::models::node::{NodeConfig, NodeConfigExt};
use hive_protocol::sync::{BackendConfig, DataSyncBackend, Document, TransportConfig, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::runtime::Runtime;

// Ditto backend (always available)
use hive_protocol::sync::ditto::DittoBackend;

// Automerge backend (optional)
#[cfg(feature = "automerge-backend")]
use hive_protocol::sync::automerge::AutomergeBackend;

// ============================================================================
// Document Creation Helpers
// ============================================================================

/// Create a CellState document with specified complexity
fn create_cell_document(member_count: usize, capability_count: usize) -> Document {
    let config = CellConfig::with_id("benchmark_cell".to_string(), (member_count * 2) as u32);
    let mut cell = CellState::new(config);

    for i in 0..member_count {
        cell.add_member(format!("node_{}", i));
    }

    if member_count > 0 {
        cell.set_leader("node_0".to_string()).ok();
    }

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

    let json = serde_json::to_value(&cell).unwrap();
    let fields: HashMap<String, Value> = serde_json::from_value(json).unwrap();
    Document::with_id(format!("cell_{}", member_count), fields)
}

/// Create a NodeConfig document with specified complexity
fn create_node_document(capability_count: usize) -> Document {
    let mut node = NodeConfig::new("UAV".to_string());
    node.id = format!("benchmark_node_{}", capability_count);

    for i in 0..capability_count {
        node.add_capability(Capability::new(
            format!("cap_{}", i),
            format!("Capability {}", i),
            CapabilityType::Sensor,
            0.9 + (i as f32 * 0.01),
        ));
    }

    let json = serde_json::to_value(&node).unwrap();
    let fields: HashMap<String, Value> = serde_json::from_value(json).unwrap();
    Document::with_id(&node.id, fields)
}

// ============================================================================
// Backend Creation Helpers
// ============================================================================

/// Create Ditto backend for benchmarking
fn create_ditto_backend(suffix: &str) -> DittoBackend {
    dotenvy::dotenv().ok();

    let app_id = std::env::var("DITTO_APP_ID").unwrap_or_else(|_| "benchmark_app".to_string());
    let shared_key =
        std::env::var("DITTO_SHARED_KEY").unwrap_or_else(|_| "benchmark_key".to_string());

    let config = BackendConfig {
        app_id,
        persistence_dir: PathBuf::from(format!("/tmp/ditto_bench_{}", suffix)),
        shared_key: Some(shared_key),
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let backend = DittoBackend::new();
    let rt = Runtime::new().unwrap();
    rt.block_on(backend.initialize(config)).unwrap();
    backend
}

#[cfg(feature = "automerge-backend")]
fn create_automerge_backend(suffix: &str) -> AutomergeBackend {
    let config = BackendConfig {
        app_id: "benchmark_app".to_string(),
        persistence_dir: PathBuf::from(format!("/tmp/automerge_bench_{}", suffix)),
        shared_key: None,
        transport: TransportConfig::default(),
        extra: HashMap::new(),
    };

    let backend = AutomergeBackend::new();
    let rt = Runtime::new().unwrap();
    rt.block_on(backend.initialize(config)).unwrap();
    backend
}

// ============================================================================
// Benchmark 1: Document Insert Performance
// ============================================================================

fn bench_document_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_insert");

    for doc_count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*doc_count as u64));

        // Ditto benchmark
        group.bench_with_input(
            BenchmarkId::new("ditto", doc_count),
            doc_count,
            |b, &count| {
                let backend = create_ditto_backend(&format!("insert_{}", count));
                let doc_store = backend.document_store();
                let rt = Runtime::new().unwrap();

                b.iter(|| {
                    for i in 0..count {
                        let doc = create_node_document(3);
                        let mut doc_with_id = doc.clone();
                        doc_with_id.id = Some(format!("node_insert_{}_{}", count, i));
                        rt.block_on(doc_store.upsert("benchmark_nodes", doc_with_id))
                            .unwrap();
                    }
                    black_box(count)
                });

                rt.block_on(backend.shutdown()).ok();
            },
        );

        // Automerge benchmark
        #[cfg(feature = "automerge-backend")]
        group.bench_with_input(
            BenchmarkId::new("automerge", doc_count),
            doc_count,
            |b, &count| {
                let backend = create_automerge_backend(&format!("insert_{}", count));
                let doc_store = backend.document_store();
                let rt = Runtime::new().unwrap();

                b.iter(|| {
                    for i in 0..count {
                        let doc = create_node_document(3);
                        let mut doc_with_id = doc.clone();
                        doc_with_id.id = Some(format!("node_insert_{}_{}", count, i));
                        rt.block_on(doc_store.upsert("benchmark_nodes", doc_with_id))
                            .unwrap();
                    }
                    black_box(count)
                });

                rt.block_on(backend.shutdown()).ok();
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 2: Document Update Performance
// ============================================================================

fn bench_document_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_update");

    // Ditto benchmark
    group.bench_function("ditto/cell_state_update", |b| {
        let backend = create_ditto_backend("update");
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        // Initial insert
        let doc = create_cell_document(5, 3);
        rt.block_on(doc_store.upsert("benchmark_cells", doc.clone()))
            .unwrap();

        let mut update_counter = 0;
        b.iter(|| {
            // Update document (simulating state change)
            let mut updated_doc = doc.clone();
            if let Some(members) = updated_doc.fields.get_mut("members") {
                if let Some(arr) = members.as_array_mut() {
                    arr.push(Value::String(format!("new_member_{}", update_counter)));
                }
            }
            update_counter += 1;
            rt.block_on(doc_store.upsert("benchmark_cells", updated_doc))
                .unwrap();
            black_box(update_counter)
        });

        rt.block_on(backend.shutdown()).ok();
    });

    // Automerge benchmark
    #[cfg(feature = "automerge-backend")]
    group.bench_function("automerge/cell_state_update", |b| {
        let backend = create_automerge_backend("update");
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        // Initial insert
        let doc = create_cell_document(5, 3);
        rt.block_on(doc_store.upsert("benchmark_cells", doc.clone()))
            .unwrap();

        let mut update_counter = 0;
        b.iter(|| {
            let mut updated_doc = doc.clone();
            if let Some(members) = updated_doc.fields.get_mut("members") {
                if let Some(arr) = members.as_array_mut() {
                    arr.push(Value::String(format!("new_member_{}", update_counter)));
                }
            }
            update_counter += 1;
            rt.block_on(doc_store.upsert("benchmark_cells", updated_doc))
                .unwrap();
            black_box(update_counter)
        });

        rt.block_on(backend.shutdown()).ok();
    });

    group.finish();
}

// ============================================================================
// Benchmark 3: Document Query Performance
// ============================================================================

fn bench_document_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_query");

    for doc_count in [10, 100].iter() {
        // Ditto benchmark
        group.bench_with_input(
            BenchmarkId::new("ditto", doc_count),
            doc_count,
            |b, &count| {
                let backend = create_ditto_backend(&format!("query_{}", count));
                let doc_store = backend.document_store();
                let rt = Runtime::new().unwrap();

                // Pre-populate with documents
                for i in 0..count {
                    let mut doc = create_node_document(3);
                    doc.id = Some(format!("query_node_{}", i));
                    rt.block_on(doc_store.upsert("benchmark_nodes", doc))
                        .unwrap();
                }

                b.iter(|| {
                    let query = hive_protocol::sync::Query::All;
                    let results = rt
                        .block_on(doc_store.query("benchmark_nodes", &query))
                        .unwrap();
                    black_box(results.len())
                });

                rt.block_on(backend.shutdown()).ok();
            },
        );

        // Automerge benchmark
        #[cfg(feature = "automerge-backend")]
        group.bench_with_input(
            BenchmarkId::new("automerge", doc_count),
            doc_count,
            |b, &count| {
                let backend = create_automerge_backend(&format!("query_{}", count));
                let doc_store = backend.document_store();
                let rt = Runtime::new().unwrap();

                // Pre-populate with documents
                for i in 0..count {
                    let mut doc = create_node_document(3);
                    doc.id = Some(format!("query_node_{}", i));
                    rt.block_on(doc_store.upsert("benchmark_nodes", doc))
                        .unwrap();
                }

                b.iter(|| {
                    let query = hive_protocol::sync::Query::All;
                    let results = rt
                        .block_on(doc_store.query("benchmark_nodes", &query))
                        .unwrap();
                    black_box(results.len())
                });

                rt.block_on(backend.shutdown()).ok();
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 4: Serialization Size (JSON baseline)
// ============================================================================

fn bench_serialization_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization_size");

    for (members, caps) in [(5, 3), (10, 10), (20, 20)].iter() {
        let id = format!("{}m_{}c", members, caps);

        // JSON baseline (for comparison)
        group.bench_function(format!("json/{}", id), |b| {
            let doc = create_cell_document(*members, *caps);
            b.iter(|| {
                let json_bytes = serde_json::to_vec(&doc.fields).unwrap();
                black_box(json_bytes.len())
            });
        });

        // Ditto serialization (measure via upsert latency as proxy)
        group.bench_function(format!("ditto/{}", id), |b| {
            let backend = create_ditto_backend(&format!("serial_{}_{}", members, caps));
            let doc_store = backend.document_store();
            let rt = Runtime::new().unwrap();
            let doc = create_cell_document(*members, *caps);

            b.iter(|| {
                rt.block_on(doc_store.upsert("benchmark_serial", doc.clone()))
                    .unwrap();
                black_box(())
            });

            rt.block_on(backend.shutdown()).ok();
        });

        // Automerge serialization
        #[cfg(feature = "automerge-backend")]
        group.bench_function(format!("automerge/{}", id), |b| {
            let backend = create_automerge_backend(&format!("serial_{}_{}", members, caps));
            let doc_store = backend.document_store();
            let rt = Runtime::new().unwrap();
            let doc = create_cell_document(*members, *caps);

            b.iter(|| {
                rt.block_on(doc_store.upsert("benchmark_serial", doc.clone()))
                    .unwrap();
                black_box(())
            });

            rt.block_on(backend.shutdown()).ok();
        });
    }

    group.finish();
}

// ============================================================================
// Benchmark 5: Memory Overhead (Document Count)
// ============================================================================

fn bench_memory_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_overhead");

    // Ditto: 100 documents
    group.bench_function("ditto/100_documents", |b| {
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            let backend = create_ditto_backend("memory_100");
            let doc_store = backend.document_store();

            for i in 0..100 {
                let mut doc = create_cell_document(5, 3);
                doc.id = Some(format!("cell_{}", i));
                rt.block_on(doc_store.upsert("memory_test", doc)).unwrap();
            }

            rt.block_on(backend.shutdown()).ok();
            black_box(())
        });
    });

    // Automerge: 100 documents
    #[cfg(feature = "automerge-backend")]
    group.bench_function("automerge/100_documents", |b| {
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            let backend = create_automerge_backend("memory_100");
            let doc_store = backend.document_store();

            for i in 0..100 {
                let mut doc = create_cell_document(5, 3);
                doc.id = Some(format!("cell_{}", i));
                rt.block_on(doc_store.upsert("memory_test", doc)).unwrap();
            }

            rt.block_on(backend.shutdown()).ok();
            black_box(())
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 6: Realistic Workload - Squad Telemetry
// ============================================================================

fn bench_squad_telemetry(c: &mut Criterion) {
    let mut group = c.benchmark_group("squad_telemetry");
    group.sample_size(20); // Reduce sample size for slower benchmarks

    // Simulate: 10 nodes updating position every second for 10 iterations
    let node_count = 10;
    let iterations = 10;

    // Ditto workload
    group.bench_function("ditto/10_nodes_10_updates", |b| {
        let backend = create_ditto_backend("telemetry");
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            for iter in 0..iterations {
                for node_id in 0..node_count {
                    let mut fields = HashMap::new();
                    fields.insert(
                        "node_id".to_string(),
                        Value::String(format!("node_{}", node_id)),
                    );
                    fields.insert(
                        "lat".to_string(),
                        Value::Number(
                            serde_json::Number::from_f64(37.7749 + iter as f64 * 0.001).unwrap(),
                        ),
                    );
                    fields.insert(
                        "lon".to_string(),
                        Value::Number(
                            serde_json::Number::from_f64(-122.4194 + node_id as f64 * 0.001)
                                .unwrap(),
                        ),
                    );
                    fields.insert(
                        "timestamp".to_string(),
                        Value::Number(serde_json::Number::from(iter * 1000)),
                    );

                    let doc = Document::with_id(format!("telemetry_node_{}", node_id), fields);
                    rt.block_on(doc_store.upsert("telemetry", doc)).unwrap();
                }
            }
            black_box(node_count * iterations)
        });

        rt.block_on(backend.shutdown()).ok();
    });

    // Automerge workload
    #[cfg(feature = "automerge-backend")]
    group.bench_function("automerge/10_nodes_10_updates", |b| {
        let backend = create_automerge_backend("telemetry");
        let doc_store = backend.document_store();
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            for iter in 0..iterations {
                for node_id in 0..node_count {
                    let mut fields = HashMap::new();
                    fields.insert(
                        "node_id".to_string(),
                        Value::String(format!("node_{}", node_id)),
                    );
                    fields.insert(
                        "lat".to_string(),
                        Value::Number(
                            serde_json::Number::from_f64(37.7749 + iter as f64 * 0.001).unwrap(),
                        ),
                    );
                    fields.insert(
                        "lon".to_string(),
                        Value::Number(
                            serde_json::Number::from_f64(-122.4194 + node_id as f64 * 0.001)
                                .unwrap(),
                        ),
                    );
                    fields.insert(
                        "timestamp".to_string(),
                        Value::Number(serde_json::Number::from(iter * 1000)),
                    );

                    let doc = Document::with_id(format!("telemetry_node_{}", node_id), fields);
                    rt.block_on(doc_store.upsert("telemetry", doc)).unwrap();
                }
            }
            black_box(node_count * iterations)
        });

        rt.block_on(backend.shutdown()).ok();
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_document_insert,
    bench_document_update,
    bench_document_query,
    bench_serialization_size,
    bench_memory_overhead,
    bench_squad_telemetry,
);

criterion_main!(benches);
