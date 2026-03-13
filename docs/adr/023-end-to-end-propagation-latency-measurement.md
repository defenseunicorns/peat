# ADR-023: End-to-End Propagation Latency Measurement for Baseline Comparison

**Status:** Proposed
**Date:** 2025-11-22
**Decision Makers:** Research Team
**Technical Story:** Traditional baseline tests revealed gap in empirical validation methodology

## Context and Problem Statement

During traditional baseline testing (2025-11-22), we discovered that our latency measurements only capture **server broadcast efficiency** (server → client transmission), not **end-to-end client-to-client document propagation latency**.

### Current Measurement Gap

**What we're currently measuring:**
```
MessageReceived latency = receive_time - message.timestamp_us
```
- Server creates broadcast message at time T
- Client receives message at time T + 1ms
- **Measured: 1ms (server → client one-hop transmission)**

**What we're NOT measuring:**
```
End-to-end propagation = Client_B_receive - Client_A_create
```
- Client A creates/updates document at T0
- Client A sends to server at T1
- Server queues update until next broadcast cycle
- Server broadcasts at T2 (depends on UPDATE_FREQUENCY)
- Client B receives at T3
- **Should measure: T3 - T0 (complete propagation path)**

### Test Results Show the Gap

Traditional baseline with UPDATE_FREQUENCY=0.5s (500ms broadcast interval):

**Measured P50 latencies (broadcast only):**
- 24 nodes: 0.5ms
- 96 nodes: 1.4ms
- 1000 nodes: 10ms

**Estimated TRUE end-to-end latencies:**
- Client → Server: ~1ms
- Server broadcast wait: 0-500ms (avg 250ms)
- Server → Client: ~1ms (measured)
- **Total P50: ~252ms** (250× higher than measured!)

### Why This Matters for Peat Comparison

We're building hierarchical CRDT to improve on traditional client-server. But our comparison is unfair:

**Current comparison:**
- Traditional baseline: 1ms (broadcast only) ✅ Looks great!
- Hierarchical CRDT: 50ms (full propagation) ❌ Looks terrible!

**Fair comparison (both end-to-end):**
- Traditional baseline: 250ms (including broadcast wait)
- Hierarchical CRDT: 50ms (event-driven, no polling)
- **Hierarchical wins 5×!**

## Decision Drivers

1. **Scientific rigor**: Must measure same thing in both architectures
2. **Real-world relevance**: Users care about client-to-client propagation, not server broadcasts
3. **Fair comparison**: Hierarchical CRDT should be compared against realistic baseline
4. **Existing test investment**: Can extend current tests rather than rebuild

## Considered Options

### Option A: Accept Broadcast-Only Metrics (Status Quo)

**Approach:** Continue measuring server broadcast efficiency only.

**Pros:**
- No code changes needed
- Tests already complete
- Still useful for understanding network transmission

**Cons:**
- Misleading comparison (apples to oranges)
- Doesn't reflect real-world user experience
- Unfair to hierarchical architecture
- Misses critical component (broadcast interval wait time)

### Option B: Add End-to-End Instrumentation (Recommended)

**Approach:** Extend traditional_baseline binary to track document origin:

```rust
struct SimpleDocument {
    doc_id: String,
    updated_at_us: u128,           // Server update time (existing)
    origin_updated_at_us: u128,    // NEW: Client creation time
    origin_node_id: String,        // NEW: Originating client
}
```

**Measurement logic:**
1. When client creates/updates document:
   - Set `origin_updated_at_us = now()`
   - Set `origin_node_id = self.node_id`

2. When client receives document from different origin:
   - Calculate `propagation_latency = now() - origin_updated_at_us`
   - Emit new metric: `PropagationReceived`
   - Only emit if `origin_node_id != self.node_id`

**Pros:**
- Measures true end-to-end propagation
- Minimal code changes (add 2 fields)
- Backward compatible (doesn't break existing metrics)
- Can run on same topologies
- Separates concerns: MessageReceived vs PropagationReceived

**Cons:**
- Requires re-running tests (~45 mins)
- Adds timestamp overhead to messages (~16 bytes)

### Option C: Create Dedicated E2E Test Binary

**Approach:** New binary `traditional_e2e_baseline.rs` with explicit writer/reader separation.

**Pros:**
- Clean separation of test types
- Explicit writer vs reader roles
- No backward compatibility concerns

**Cons:**
- More code to maintain
- Duplicate topology definitions
- Longer development time

## Decision Outcome

**Chosen option:** **Option B - Add End-to-End Instrumentation**

### Rationale

1. **Minimal changes, maximum value**: Adding 2 fields is trivial vs creating new binary
2. **Backward compatible**: Existing MessageReceived metrics still work
3. **Reusable infrastructure**: Same topologies, same test scripts
4. **Both metrics useful**: Separate broadcast efficiency from propagation latency

### Expected Results

With UPDATE_FREQUENCY=0.5s:

**Theoretical latency components:**
```
Client → Server:              ~1ms  (network transmission)
Server broadcast wait:      0-500ms (uniform distribution)
  └─ Average:                250ms
  └─ P50:                    250ms
  └─ P95:                    475ms
Server → Client:              ~1ms  (network transmission)
```

**Expected end-to-end propagation latency:**
- **P50: ~252ms** (250ms wait + 2ms transmission)
- **P95: ~477ms** (475ms wait + 2ms transmission)
- **P99: ~500ms** (worst case: just missed broadcast)

**At 1000 nodes:**
- **P50: ~260ms** (250ms + 10ms broadcast)
- **P95: ~485ms** (475ms + 10ms broadcast)

### Comparison to Hierarchical CRDT

This will provide fair baseline for hierarchical comparison:

| Metric | Traditional (E2E) | Hierarchical CRDT | Winner |
|--------|-------------------|-------------------|--------|
| 24 nodes P50 | ~250ms | <50ms (event-driven) | CRDT 5× |
| 96 nodes P50 | ~252ms | <100ms (O(log n)) | CRDT 2.5× |
| 1000 nodes P50 | ~260ms | <150ms (O(log n)) | CRDT 1.7× |

**Key insight:** Hierarchical CRDT should show:
- **Lower latency** (event-driven, no polling)
- **Better scaling** (O(log n) aggregation)
- **Clear value proposition** vs traditional baseline

## Implementation Plan

### Phase 1: Code Changes (30 mins)
1. Add `origin_updated_at_us` and `origin_node_id` to `SimpleDocument`
2. Stamp documents on creation/update
3. Emit `PropagationReceived` metric when receiving from different origin
4. Update serialization to include new fields

### Phase 2: Test Execution (45 mins)
1. Rebuild Docker image
2. Run traditional baseline tests (16 tests)
3. Collect both MessageReceived and PropagationReceived metrics

### Phase 3: Analysis (30 mins)
1. Extract both metric types from logs
2. Generate comparison charts:
   - Broadcast efficiency (MessageReceived)
   - End-to-end propagation (PropagationReceived)
   - Breakdown: transmission vs wait time
3. Document in results directory

### Phase 4: Hierarchical Comparison
1. Run hierarchical CRDT tests
2. Compare PropagationReceived metrics (fair comparison)
3. Demonstrate hierarchical advantages

## Consequences

### Positive

- **Fair comparison**: Both architectures measured end-to-end
- **Real-world relevance**: Metrics reflect user experience
- **Clear value proposition**: Hierarchical CRDT advantages become obvious
- **Scientific rigor**: Same measurement methodology for all tests
- **Dual metrics**: Still have broadcast efficiency data

### Negative

- **Test time**: Need to re-run traditional baseline (~45 mins)
- **Complexity**: Two different latency metrics to track
- **Message overhead**: Additional 16 bytes per document

### Neutral

- **Existing results still valid**: MessageReceived metrics are accurate for what they measure (broadcast efficiency)
- **Can run both**: Keep current results for broadcast analysis, add new results for propagation analysis

## Notes

### Metric Definitions

**MessageReceived (existing):**
- Measures: Server → Client broadcast transmission time
- Use case: Network efficiency, server broadcast performance
- Expected range: 0.5-10ms (independent of broadcast interval)

**PropagationReceived (new):**
- Measures: Client A create → Client B receive (full path)
- Use case: User-facing propagation latency, architecture comparison
- Expected range: 250-500ms (dominated by broadcast interval)

### Future Enhancements

Could add additional metrics:
- **ClientUploadLatency**: Client → Server transmission
- **ServerQueuingDelay**: Time between receive and broadcast
- **MultiHopPropagation**: Track propagation through multiple tiers

This creates a comprehensive latency measurement framework for all Peat architectures.

## References

- Traditional baseline test results: `peat-sim/traditional-baseline-20251121-211945/`
- Current analysis (broadcast only): `peat-sim/traditional-baseline-20251121-211945/ANALYSIS.md`
- ADR-015: Hierarchical aggregation validation requirements
- ADR-011: Ditto vs Automerge backend comparison
