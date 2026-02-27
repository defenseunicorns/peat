# Lab 4 vs Lab 3b: Hierarchical vs Flat Mesh CRDT Comparison

**Date**: 2025-11-24
**Status**: FINAL - Correct Propagation Latency Metrics (bug fixes applied)

---

## Executive Summary

Lab 4 (Hierarchical CRDT) demonstrates **sub-second propagation latencies** for cross-tier synchronization via Ditto CRDT, achieving what was expected for directly connected peers.

### Key Finding

**Squad → Platoon propagation latency at 24 nodes:**
- P50: **74.4 ms**
- P95: **197.4 ms**
- P99: **296 ms**

This is realistic performance for CRDT-based distributed state synchronization over TCP mesh.

---

## Lab 4 Results: Squad → Platoon Propagation Latency (CORRECTED)

### 24-Node Test (1 Gbps, 6,712 events)

| Metric | Value |
|--------|-------|
| Min | 13.6 ms |
| **P50** | **74.4 ms** |
| **P95** | **197.4 ms** |
| P99 | 296 ms |
| Max | 1,148 ms |

These numbers are from the corrected measurement code that:
1. Uses `last_update_us` (storage field) instead of `last_modified_us` (non-existent)
2. Tracks updates by `(doc_id, last_modified_us)` tuple, not by `created_at_us` (which is immutable)

---

## Bug Fixes Applied

### Bug 1: Wrong Field Name (main.rs)
**Problem**: Storage writes `last_update_us`, but observer was reading `last_modified_us`
**Fix**: Updated observer to check `last_update_us` first at three locations (~895, ~975, ~2426)

### Bug 2: Deduplication by created_at_us
**Problem**: `test_doc_timestamps` hashset tracked by `created_at_us`, which is immutable
**Result**: Only first update per document was logged; subsequent updates were filtered out
**Fix**: Changed to `seen_doc_updates: HashSet<(String, u128)>` tracking `(doc_id, last_modified_us)`

---

## What the Numbers Mean

| Latency Range | Interpretation |
|---------------|----------------|
| 13-50 ms | Optimal - direct TCP sync, low contention |
| 50-100 ms | Normal - typical network + processing |
| 100-200 ms | Acceptable - some queuing/retransmission |
| 200-1000 ms | Elevated - high contention or reconnection |

P95 of ~200ms is **excellent** for a distributed CRDT system.

---

## Comparison to Lab 3b (Flat Mesh)

| Metric | Lab 3b (Flat) | Lab 4 (Hierarchical) |
|--------|---------------|----------------------|
| 24-node P95 | N/A (max 50) | **197.4 ms** |
| Connection Count | O(n²) | O(n) |
| Document Volume | All docs everywhere | Aggregated at tiers |

### Advantages of Hierarchy
1. **Reduced connections**: Squad members only connect within squad
2. **Reduced document traffic**: N→1 aggregation at each tier
3. **Scalability**: Can support 384+ nodes (flat mesh limited to ~50)

---

## Recommendations

1. **P95 ~200ms is suitable for tactical applications** - not real-time gaming, but appropriate for situational awareness
2. **Hierarchy enables scale** - use for deployments beyond 50 nodes
3. **Monitor P99/Max** for outliers indicating network issues

---

## Test Environment

- Topology: 24-node hierarchical (1 platoon, 3 squads × 7 soldiers + 3 squad leaders + 1 platoon leader)
- Bandwidth: 1 Gbps (unconstrained)
- Duration: 90+ seconds steady state
- Event count: 6,712 update propagations measured

---

## Metrics Reference

| Metric | Source | What It Measures |
|--------|--------|------------------|
| `processing_time_us` | `AggregationCompleted` | Local CPU time to aggregate |
| `latency_ms` (update) | `DocumentReceived` | **Network propagation time** |
| `latency_ms` (create) | `DocumentReceived` | Time since document creation |
| `latency_ms` | `CRDTUpsert` | Local CRDT write time |

**Always use `DocumentReceived.latency_ms` with `latency_type: "update"` for propagation analysis.**
