# Lab 4: Comprehensive Hierarchical Latency Instrumentation Plan

**Date**: 2025-11-24
**Status**: Planning phase - defining full latency measurement requirements

---

## Current State Assessment

### What We Have ✅

**Aggregation Processing Time**:
- `AggregationCompleted` events with `processing_time_us`
- Shows local aggregation computation time (5-10μs)
- Does NOT show propagation time across hierarchy

**Document Reception at Platoon Level**:
- `DocumentReceived` events from Ditto backend
- Shows squad→platoon propagation (377-528ms)
- Includes `created_at_us`, `received_at_us`, `latency_us`

**Soldier Message Sends**:
- `MessageSent` events with timestamps
- Message size tracking
- Does NOT track reception or propagation

### What We're Missing ❌

**Upward Propagation**:
1. Soldier NodeState → Squad Leader reception time
2. Squad Summary → Platoon Leader reception time (✅ have this)
3. Platoon Summary → Company Leader reception time

**Lateral Propagation**:
1. Soldier update → Peer soldiers in same squad
2. Soldier update → Soldiers in different squads (same platoon)
3. Soldier update → Soldiers in different platoons

**End-to-End Flows**:
1. Soldier upsert → All hierarchy levels receive it
2. Soldier upsert → All peers across platoon/company receive it

---

## Required Metrics Architecture

### Tier 1: Soldier Level

**When soldier upserts NodeState document**:
```json
{
  "event_type": "CRDTUpsert",
  "node_id": "alpha-soldier-1",
  "tier": "soldier",
  "doc_id": "nodestate-alpha-soldier-1",
  "doc_type": "NodeState",
  "upsert_timestamp_us": 1234567890,
  "message_number": 42,
  "doc_size_bytes": 314
}
```

**When soldier receives peer NodeState**:
```json
{
  "event_type": "DocumentReceived",
  "node_id": "alpha-soldier-2",
  "tier": "soldier",
  "doc_id": "nodestate-alpha-soldier-1",
  "doc_type": "NodeState",
  "peer_node_id": "alpha-soldier-1",
  "created_at_us": 1234567890,
  "received_at_us": 1234568000,
  "latency_us": 110,
  "same_squad": true,
  "same_platoon": true
}
```

**When soldier receives SquadSummary from leader**:
```json
{
  "event_type": "DocumentReceived",
  "node_id": "alpha-soldier-1",
  "tier": "soldier",
  "doc_id": "squad-summary-squad-alpha",
  "doc_type": "SquadSummary",
  "created_at_us": 1234568500,
  "received_at_us": 1234569000,
  "latency_us": 500,
  "from_my_squad_leader": true
}
```

### Tier 2: Squad Leader Level

**When squad leader receives soldier NodeState**:
```json
{
  "event_type": "DocumentReceived",
  "node_id": "squad-alpha-leader",
  "tier": "squad_leader",
  "doc_id": "nodestate-alpha-soldier-1",
  "doc_type": "NodeState",
  "soldier_id": "alpha-soldier-1",
  "created_at_us": 1234567890,
  "received_at_us": 1234567950,
  "latency_us": 60,
  "is_my_squad_member": true
}
```

**When squad leader upserts SquadSummary** (already have AggregationCompleted):
```json
{
  "event_type": "CRDTUpsert",
  "node_id": "squad-alpha-leader",
  "tier": "squad_leader",
  "doc_id": "squad-summary-squad-alpha",
  "doc_type": "SquadSummary",
  "upsert_timestamp_us": 1234568500,
  "input_count": 6,
  "doc_size_bytes": 256
}
```

**When squad leader receives peer SquadSummary**:
```json
{
  "event_type": "DocumentReceived",
  "node_id": "squad-alpha-leader",
  "tier": "squad_leader",
  "doc_id": "squad-summary-squad-bravo",
  "doc_type": "SquadSummary",
  "peer_squad_id": "squad-bravo",
  "created_at_us": 1234568600,
  "received_at_us": 1234569100,
  "latency_us": 500,
  "same_platoon": true
}
```

### Tier 3: Platoon Leader Level

**When platoon leader receives SquadSummary** (✅ already have this):
```json
{
  "event_type": "DocumentReceived",
  "node_id": "platoon-leader",
  "doc_id": "squad-alpha-summary",
  "created_at_us": 1764009864773457,
  "received_at_us": 1764009865301516,
  "latency_us": 528059,
  "latency_ms": 528.059
}
```

**When platoon leader upserts PlatoonSummary**:
```json
{
  "event_type": "CRDTUpsert",
  "node_id": "platoon-leader",
  "tier": "platoon_leader",
  "doc_id": "platoon-summary-platoon-1",
  "doc_type": "PlatoonSummary",
  "upsert_timestamp_us": 1234570000,
  "input_count": 3,
  "doc_size_bytes": 512
}
```

### Tier 4: Company Leader Level

**When company leader receives PlatoonSummary**:
```json
{
  "event_type": "DocumentReceived",
  "node_id": "company-leader",
  "tier": "company_leader",
  "doc_id": "platoon-summary-platoon-1",
  "doc_type": "PlatoonSummary",
  "created_at_us": 1234570000,
  "received_at_us": 1234570500,
  "latency_us": 500
}
```

---

## Implementation Strategy

### Option 1: Comprehensive Instrumentation (Recommended)

**Scope**: Add DocumentReceived tracking at ALL tiers
**Timeline**: 2-3 hours implementation + rebuild + test
**Benefit**: Complete hierarchical flow visibility

**Files to Modify**:
1. `peat-sim/src/main.rs`:
   - Add DocumentReceived tracking in `soldier_capability_mode()`
   - Add DocumentReceived tracking in `squad_leader_capability_mode()`
   - Add DocumentReceived tracking in `platoon_leader_capability_mode()`
   - Add CRDTUpsert tracking when summaries are created

2. Leverage existing Ditto metadata:
   - Documents have `created_at` timestamps
   - Can calculate latency = `now() - created_at`
   - Need to observe document changes and emit metrics

**Key Insight**: We need to add observers for document reception at each tier, similar to what platoon leader already has.

### Option 2: Targeted Critical Path (Faster)

**Scope**: Add only soldier→squad and lateral peer-to-peer tracking
**Timeline**: 1-2 hours implementation
**Benefit**: Captures most critical latencies for hierarchy validation

**Focus**:
1. Soldier receives peer NodeState (lateral propagation)
2. Squad leader receives soldier NodeState (upward propagation)
3. Soldier receives SquadSummary from leader (downward propagation)

**Rationale**:
- Platoon→Company we already have
- Soldier→Squad is the critical first hop
- Peer-to-peer shows PEAT vs flat mesh difference

### Option 3: Use Existing Metrics + Inference (Fastest)

**Scope**: No new code, extract from existing logs
**Timeline**: 30 minutes
**Benefit**: Can start full test suite immediately

**What We Can Measure Now**:
1. ✅ Aggregation processing time (5-10μs)
2. ✅ Squad→Platoon propagation (377-528ms from DocumentReceived)
3. ✅ Message send frequency (5 sec intervals)
4. ⚠️ Missing: Soldier→Squad latency
5. ⚠️ Missing: Peer-to-peer lateral propagation

**Limitation**: Can't measure first hop or lateral flows

---

## Analysis Requirements for Lab 4

### Core Questions to Answer

**Hierarchical Scaling Hypothesis**:
1. Does latency grow with log(N) or N²?
2. Is upward propagation bounded at each tier?
3. Does aggregation reduce bandwidth proportionally?

**Comparison to Lab 3b (Flat Mesh)**:
1. At what scale does flat mesh fail vs hierarchy succeed?
2. What is the latency difference at common scales (24, 48, 96 nodes)?
3. What is the bandwidth reduction from aggregation?

### Minimum Viable Metrics

**To prove hierarchical scaling**:
1. ✅ Aggregation processing time per tier (have it)
2. ✅ Cross-tier propagation time (have squad→platoon, need soldier→squad)
3. ❌ Peer-to-peer propagation time (needed for comparison to flat mesh)
4. ✅ Aggregation ratios per tier (have input_count)
5. ✅ Document sizes per tier (have message_size_bytes)

**Verdict**: We need at least soldier→squad and peer-to-peer metrics

---

## Recommended Path Forward

### Phase 4: Critical Path Instrumentation (Option 2)

**Implement these 3 metrics** (1-2 hours):

1. **Soldier receives peer NodeState**:
   - Observe NodeState collection for changes
   - Filter for documents from peer soldiers
   - Calculate latency = receive_time - document.created_at
   - Emit DocumentReceived metric

2. **Squad leader receives soldier NodeState**:
   - Observe NodeState collection for changes
   - Filter for documents from squad members
   - Calculate latency = receive_time - document.created_at
   - Emit DocumentReceived metric

3. **Soldier receives SquadSummary**:
   - Observe SquadSummary collection for changes
   - Filter for summary from own squad leader
   - Calculate latency = receive_time - document.created_at
   - Emit DocumentReceived metric

### Phase 5: Run Full Test Suite

With these metrics, we can measure:
- ✅ Upward flow: soldier → squad → platoon
- ✅ Lateral flow: soldier → peer soldiers
- ✅ Downward flow: squad leader → soldiers
- ✅ Aggregation efficiency at all tiers
- ✅ Scaling behavior as N increases

### Phase 6: Analysis & Comparison

Compare Lab 4 vs Lab 3b:
- Latency at scale: bounded vs explosive
- Bandwidth: aggregated vs full replication
- Scaling pattern: O(log n) vs O(n²)

---

## Questions for User

1. **Scope Decision**: Should we implement comprehensive (Option 1), critical path (Option 2), or proceed with existing metrics (Option 3)?

2. **Timeline Priority**: Is 2-3 hours for comprehensive instrumentation acceptable, or should we optimize for faster results?

3. **Analysis Depth**: Do you need every possible latency measurement, or is the critical path (soldier↔squad, squad↔platoon) sufficient to prove hierarchical scaling?

4. **Comparison Focus**: Is the primary goal to show Lab 4 scales where Lab 3b fails, or to measure absolute latencies at all tiers?

My recommendation: **Option 2 (Critical Path)** gives us the minimum viable metrics to prove hierarchical scaling with only 1-2 hours of additional work, then we can run the full 20-test suite and complete Epic #132.
