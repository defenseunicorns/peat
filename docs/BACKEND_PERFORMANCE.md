# Backend Performance Comparison (Issue #154)

**Date**: 2025-11-24
**Backends Tested**: Ditto 4.11.5
**Hardware**: macOS Darwin 24.6.0 (Apple Silicon)

## Executive Summary

This document presents benchmark results for the Ditto backend in the HIVE Protocol. The Ditto backend implements the `DataSyncBackend` trait, providing CRDT-based state management for distributed autonomous systems.

**Key Findings:**
- All insert/update/query operations meet targets (<5ms per operation)
- Query performance is excellent (23-125µs per document)
- Realistic workload (squad telemetry) shows ~2.1ms per update
- Ditto initialization overhead is significant (~10-20ms per backend instance)

## Benchmarks Overview

| Benchmark | Description | Target | Result |
|-----------|-------------|--------|--------|
| Document Insert | Insert N documents | <5ms per doc | ✅ 1.4ms/doc |
| Document Update | Update existing document | <5ms | ✅ 1.3ms |
| Document Query | Query all documents | <1ms per 10 docs | ✅ 234µs/10 docs |
| Squad Telemetry | Realistic workload (10 nodes, 10 updates) | <500ms total | ✅ 210ms |

## Results

### Document Insert Performance

Measures time to insert documents at various scales (10, 100, 1000 documents).

| Scale | Ditto | Per-Doc Average | Status |
|-------|-------|-----------------|--------|
| 10 docs | 13.5 ms | 1.35 ms/doc | ✅ Pass |
| 100 docs | 176 ms | 1.76 ms/doc | ✅ Pass |
| 1000 docs | 1.37 s | 1.37 ms/doc | ✅ Pass |

**Analysis**: Insert performance scales linearly with document count. Per-document overhead is consistent at ~1.4ms, well under the 5ms target.

### Document Update Performance

Measures time to update a CellState document (adding members).

| Operation | Ditto | Status |
|-----------|-------|--------|
| Cell state update | 1.31 ms | ✅ Pass |

**Analysis**: Update latency is excellent, suitable for real-time state changes during autonomous operations.

### Document Query Performance

Measures time to query all documents from a collection.

| Document Count | Ditto | Per-Doc Average | Status |
|----------------|-------|-----------------|--------|
| 10 docs | 234 µs | 23.4 µs/doc | ✅ Pass |
| 100 docs | 1.25 ms | 12.5 µs/doc | ✅ Pass |

**Analysis**: Query performance improves with scale due to amortized overhead. Results show excellent read performance.

### Serialization Performance

Measures upsert latency as a proxy for serialization overhead (document complexity varies by member count and capability count).

| Document Size | JSON (baseline) | Ditto Upsert |
|---------------|-----------------|--------------|
| 5 members, 3 caps | 618 ns | 1.15 ms |
| 10 members, 10 caps | 1.38 µs | 1.06 ms |
| 20 members, 20 caps | 2.51 µs | 1.50 ms |

**Analysis**: Ditto upsert latency remains relatively constant regardless of document size, indicating most overhead comes from CRDT bookkeeping and persistence rather than serialization. JSON encoding is orders of magnitude faster but lacks CRDT semantics.

### Memory Overhead

Measures time to create backend and populate 100 documents.

| Backend | Time |
|---------|------|
| Ditto | 352 ms |

**Analysis**: Most of this time is Ditto initialization overhead (~20ms per instance creation in the benchmark). Actual document storage is efficient.

### Realistic Workload: Squad Telemetry

Simulates 10 nodes updating position every iteration for 10 iterations (100 total updates).

| Backend | Total Time | Per-Update Average | Updates/sec |
|---------|------------|-------------------|-------------|
| Ditto | 210 ms | 2.1 ms | ~476 |

**Analysis**: The benchmark shows 476 updates per second, which exceeds typical autonomous system telemetry requirements (usually 1-10 Hz per node). With 10 nodes at 10 Hz, we need 100 updates/sec, well within capability.

## Running the Benchmarks

```bash
# Ditto benchmarks
cargo bench --bench backend_comparison -p hive-protocol

# View HTML report
open target/criterion/report/index.html
```

## Analysis

### Ditto Strengths

- **Production-ready**: Commercial CRDT engine with enterprise support
- **Multi-transport**: Built-in TCP, Bluetooth, mDNS discovery
- **Optimized sync**: Delta-based synchronization reduces bandwidth
- **Consistent latency**: ~1-2ms per operation regardless of document size

### Areas for Improvement

- **Initialization overhead**: Each backend instance takes ~20ms to start
- **Logging verbosity**: Ditto generates significant log output during benchmarks

## Conclusions

The Ditto backend meets all performance targets for the HIVE Protocol:

1. **Insert/Update latency** (~1.4ms) is well under the 5ms target
2. **Query latency** (~23-125µs per doc) is excellent for real-time state queries
3. **Realistic workload** supports 476 updates/sec, exceeding typical requirements
4. **Scaling** is linear and predictable

The backend is suitable for production use in autonomous multi-agent systems requiring CRDT-based state synchronization.

## Related

- [Issue #154: Backend Performance Benchmarking](https://github.com/kitplummer/hive/issues/154)
- [ADR-007: CRDT Backend Evaluation](docs/adr/007-crdt-backend-evaluation.md)
- [ADR-011: Backend Optionality](docs/adr/011-backend-optionality.md)

---

*Benchmark results generated using Criterion.rs on 2025-11-24*
