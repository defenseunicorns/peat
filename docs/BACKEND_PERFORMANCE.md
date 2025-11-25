# Backend Performance Comparison (Issue #154)

**Date**: 2025-11-24
**Backends Tested**: Ditto 4.11.x, Automerge 0.7.1 (optional)
**Hardware**: macOS Darwin 24.6.0

## Executive Summary

This document presents benchmark results comparing the Ditto and Automerge backends for the HIVE Protocol. Both backends implement the `DataSyncBackend` trait, providing CRDT-based state management for distributed autonomous systems.

## Benchmarks Overview

| Benchmark | Description | Target |
|-----------|-------------|--------|
| Document Insert | Insert N documents | <5ms per doc |
| Document Update | Update existing document | <5ms |
| Document Query | Query all documents | <1ms per 10 docs |
| Serialization | Document encode/decode | <1KB overhead |
| Memory Overhead | 100 documents | <500MB |
| Squad Telemetry | Realistic workload (10 nodes, 10 updates) | <100ms total |

## Results

### Document Insert Performance

Measures time to insert documents at various scales (10, 100, 1000 documents).

| Scale | Ditto | Automerge | Winner |
|-------|-------|-----------|--------|
| 10 docs | TBD | TBD | - |
| 100 docs | TBD | TBD | - |
| 1000 docs | TBD | TBD | - |

### Document Update Performance

Measures time to update a CellState document (adding members).

| Operation | Ditto | Automerge | Winner |
|-----------|-------|-----------|--------|
| Cell state update | TBD | TBD | - |

### Document Query Performance

Measures time to query all documents from a collection.

| Document Count | Ditto | Automerge | Winner |
|----------------|-------|-----------|--------|
| 10 docs | TBD | TBD | - |
| 100 docs | TBD | TBD | - |

### Serialization Performance

Measures upsert latency as a proxy for serialization overhead.

| Document Size | JSON (baseline) | Ditto | Automerge |
|---------------|-----------------|-------|-----------|
| 5 members, 3 caps | TBD | TBD | TBD |
| 10 members, 10 caps | TBD | TBD | TBD |
| 20 members, 20 caps | TBD | TBD | TBD |

### Memory Overhead

Measures time to create and populate 100 documents.

| Backend | Time | Memory (est.) |
|---------|------|---------------|
| Ditto | TBD | TBD |
| Automerge | TBD | TBD |

### Realistic Workload: Squad Telemetry

Simulates 10 nodes updating position every iteration for 10 iterations (100 total updates).

| Backend | Time | Updates/sec |
|---------|------|-------------|
| Ditto | TBD | TBD |
| Automerge | TBD | TBD |

## Running the Benchmarks

```bash
# Ditto-only benchmarks
cargo bench --bench backend_comparison -p hive-protocol

# Both backends (requires automerge-backend feature)
cargo bench --bench backend_comparison -p hive-protocol --features automerge-backend

# View HTML report
open target/criterion/report/index.html
```

## Analysis

### Ditto Advantages

- **Production-ready**: Commercial CRDT engine with enterprise support
- **Multi-transport**: Built-in TCP, Bluetooth, mDNS discovery
- **Optimized sync**: Delta-based synchronization reduces bandwidth

### Automerge Advantages

- **Open source**: Full control over implementation
- **Pure Rust**: No FFI overhead
- **Flexible**: Can customize sync protocol

## Conclusions

*Results pending benchmark completion*

## Related

- [Issue #154: Backend Performance Benchmarking](https://github.com/kitplummer/hive/issues/154)
- [ADR-007: CRDT Backend Evaluation](docs/adr/007-crdt-backend-evaluation.md)
- [ADR-011: Backend Optionality](docs/adr/011-backend-optionality.md)

---

**Note**: This document will be updated with actual benchmark results once the benchmark suite completes.
