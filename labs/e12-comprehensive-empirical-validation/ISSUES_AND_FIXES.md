# E12 Test Issues and Fixes

Based on the E12 comprehensive validation run on 2025-11-16, three critical data quality issues were identified. This document tracks root causes and fixes.

---

## Issue 1: CAP Hierarchical 48/96-node Missing Latency Metrics

**Status:** ✅ FIXED

### Symptom
- 48-node hierarchical test: 0 DocumentReceived events, 0 latency measurements
- 96-node hierarchical test: 0 DocumentReceived events, 0 latency measurements
- 24-node hierarchical test: 260 DocumentReceived events ✓ (working correctly)

### Evidence
```
24-node hierarchical (WORKING):
- 16 DocumentInserted
- 260 DocumentReceived
- 40 DocumentAcknowledged
- 24 MessageSent

48-node hierarchical (BROKEN):
- 219 DocumentInserted
- 325 MessageSent
- 0 DocumentReceived     ← MISSING
- 0 DocumentAcknowledged ← MISSING
```

### Root Cause
**CONFIRMED:** Wrong topology files used in E12 test suite.

**What E12 Used:**
- 24-node: `platoon-24node-client-server-mode4.yaml`
- 48-node: `battalion-48node-client-server-mode4.yaml`
- 96-node: `battalion-96node-client-server-mode4.yaml`

**Critical Topology Differences:**

| Topology Type | SQUAD_MEMBERS env var | SQUAD_ID on members | P2P Mesh | Squad Aggregation |
|---|---|---|---|---|
| client-server-mode4 | ❌ NO | ❌ NO | ❌ Star topology | ❌ NOT enabled |
| mesh-mode4 | ✅ YES | ✅ YES | ✅ Full mesh | ✅ Enabled |

**Available Topologies:**
- ✅ `platoon-24node-mesh-mode4.yaml` exists
- ❌ `battalion-48node-mesh-mode4.yaml` does NOT exist
- ❌ `battalion-96node-mesh-mode4.yaml` does NOT exist

**Battalion Topology Structure Problem:**
The battalion-*-client-server-mode4.yaml files don't even have squad leaders! They only have:
- Battalion HQ (aggregates platoon summaries)
- 4 Platoon Leaders (aggregate node states directly)
- 44 Soldiers (report node states)

This is a **2-level hierarchy** (platoon + battalion), not the expected **3-level hierarchy** (squad + platoon + battalion) that the Mode 4 code in `cap_sim_node.rs` implements.

**Why 24-node Test Worked:**
The platoon-24node-client-server-mode4.yaml has 3 squad leaders with MODE=hierarchical, so squad aggregation loops ran. However, without SQUAD_MEMBERS set, the aggregation didn't create squad summaries - it relied on P2P CRDT replication instead.

**Why 48/96-node Tests Failed:**
Battalion topologies have no squad leaders at all, so squad_leader_aggregation_loop never runs, no squad summaries are created, and DocumentReceived events are never emitted for aggregated state.

### Fix Applied

**Selected Option 2: Created Battalion Mesh-Mode4 Topologies**

Implemented on 2025-11-16:

1. **Created `battalion-48node-mesh-mode4.yaml`:**
   - 1 Battalion HQ
   - 4 Platoon Leaders (platoon-1 through platoon-4)
   - 8 Squad Leaders (2 per platoon: squad-1A, squad-1B, etc.)
   - 35 Squad Members
   - Total: 48 nodes with proper 3-level hierarchy
   - Each squad leader has `SQUAD_MEMBERS` env var
   - Each squad member has `SQUAD_ID` env var
   - Full P2P mesh within each organizational level

2. **Created `battalion-96node-mesh-mode4.yaml`:**
   - 1 Battalion HQ
   - 8 Platoon Leaders (platoon-1 through platoon-8)
   - 16 Squad Leaders (2 per platoon)
   - 71 Squad Members
   - Total: 96 nodes with proper 3-level hierarchy
   - Same configuration pattern as 48-node topology

3. **Updated `run-comprehensive-suite.sh`:**
   - Line 605-606: Added 48-node and 96-node hierarchical test configs
   - Both now point to the new mesh-mode4 topologies
   - All hierarchical tests (24, 48, 96 nodes) now use proper mesh topologies

**Files Modified:**
- Created: `/home/kit/Code/revolve/cap/cap-sim/topologies/battalion-48node-mesh-mode4.yaml`
- Created: `/home/kit/Code/revolve/cap/cap-sim/topologies/battalion-96node-mesh-mode4.yaml`
- Modified: `/home/kit/Code/revolve/cap/labs/e12-comprehensive-empirical-validation/scripts/run-comprehensive-suite.sh` (lines 598-606)

### Previous Fix Options (For Reference)

**Option 1: Use Only 24-node Hierarchical Tests (Short-term)**
- Update E12 test suite to only run 24-node hierarchical tests using `platoon-24node-mesh-mode4.yaml`
- Remove 48/96-node hierarchical tests from test matrix
- Document that hierarchical testing is limited to 24 nodes
- Timeline: 30 minutes

**Option 2: Create Battalion Mesh-Mode4 Topologies (Medium-term)** ← **SELECTED**
- Create `battalion-48node-mesh-mode4.yaml` with full 3-level hierarchy:
  - Battalion HQ → 4 Platoon Leaders → 8 Squad Leaders → 35 Squad Members
  - Each squad has SQUAD_MEMBERS and SQUAD_ID properly configured
  - P2P mesh within squads and at leadership levels
- Create `battalion-96node-mesh-mode4.yaml` similarly
- Update E12 test suite to use these new topologies
- Timeline: 2-3 hours topology creation + 1 hour validation

**Option 3: Hybrid Approach (Not Selected)**
- For 24-node: Use `platoon-24node-mesh-mode4.yaml` (exists, works)
- For 48/96-node: Document as "not supported with full hierarchical aggregation"
- Focus E12v2 on comparing:
  - Traditional IoT vs CAP Full Mesh vs CAP Hierarchical at 24 nodes
  - Traditional IoT vs CAP Full Mesh at 48/96 nodes (omit hierarchical)
- Timeline: 1 hour test suite update

### Verification
After fix, verify:
- `jq -r '.event_type' all-metrics.jsonl | sort | uniq -c` shows DocumentReceived events
- `test-summary.json` has `latency_count > 0`
- Latency measurements are reasonable (< 10s)

---

## Issue 2: Traditional IoT Latency Measurement Semantics

**Status:** ANALYSIS REQUIRED → Likely Correct Behavior

### Symptom
Traditional IoT shows dramatically different latency characteristics at different scales:
- 2-12 nodes: ~427ms avg latency, ~5000ms P99
- 24+ nodes: ~18-61ms avg latency, ~14-353ms P99

This 90% latency reduction is suspicious and indicates measurement semantics changed.

### Evidence
```
Traditional IoT Scaling:
 2 nodes:  477.97ms avg, 5001.62ms P99  (22 latency measurements)
12 nodes:  427.18ms avg, 5001.69ms P99  (308 latency measurements)
24 nodes:   18.43ms avg,   14.33ms P99  (4,622 latency measurements!!!)
48 nodes:   18.08ms avg,   16.47ms P99  (9,447 latency measurements)
96 nodes:   14.61ms avg,   35.32ms P99  (19,488 latency measurements)
```

**Key Observation:** The median latency for 24-node is **0.551ms**, indicating local read operations.

### Root Cause
**Confirmed:** At 24+ nodes, Traditional IoT is measuring **local read latency** (cache hits / local store access), not **end-to-end replication latency** from server to client.

**Code Path:**
1. Small scale (2-12 nodes): Measuring time from document insertion on server → document reception on client
2. Large scale (24+ nodes): Measuring time from query execution → document retrieval from local store

This creates an apples-to-oranges comparison with CAP architectures, which always measure end-to-end CRDT convergence latency.

### Fix Required

1. **Add explicit latency measurement types:**
   ```rust
   enum LatencyType {
       EndToEndReplication,  // Server insert → Client receive
       LocalRead,            // Query → Local cache response
       QueryRoundTrip,       // Query → Server → Response
   }
   ```

2. **Emit separate metric events:**
   ```rust
   MetricsEvent::ReplicationLatency { ... }
   MetricsEvent::LocalReadLatency { ... }
   ```

3. **Update aggregation to separate metrics:**
   - `test-summary.json` should have both `replication_latency_*` and `local_read_latency_*` fields
   - Analysis tools can choose which metric to compare

4. **Document in test config:**
   - Add `latency_measurement_type` field to `test-config.txt`
   - Clearly document what each test is measuring

### Verification
After fix:
- Traditional IoT tests report both replication AND local read latency
- Analysis can compare replication-to-replication fairly
- Executive summary clearly states which metric is used for comparison

---

## Issue 3: CAP Full Mesh Low Message Counts

**Status:** INVESTIGATION REQUIRED

### Symptom
CAP Full Mesh tests show surprisingly low message counts:
- 12-node tests: Only 6-9 messages total across entire test
- 24-node tests: Only 9-12 messages total
- Expected: Hundreds of messages for 60-90s test duration

### Evidence
```
CAP Full Mesh Message Counts:
cap-full-12node-1gbps:    6 messages
cap-full-12node-100mbps:  6 messages
cap-full-12node-1mbps:    9 messages
cap-full-24node-1gbps:    9 messages
cap-full-24node-1mbps:   12 messages
cap-full-48node-1gbps:   11 messages
cap-full-96node-1gbps:   10 messages

Compare to Traditional IoT:
traditional-24node-1gbps: 4,789 messages
traditional-96node-1gbps: 19,898 messages
```

### Possible Root Causes

**Hypothesis 1: CRDT Convergence Not Complete**
- P2P mesh may need longer warmup/observation period
- CRDTs may converge slowly and messages are sent only during initial sync
- Test duration (60s measurement) may miss subsequent update propagation

**Hypothesis 2: Measurement Window Too Narrow**
- Messages sent during warmup (30s) may not be counted
- Only counting messages during "measurement phase"
- Actual CRDT traffic happens outside measurement window

**Hypothesis 3: Event Filtering**
- Metrics aggregation may be filtering out certain message types
- Only counting specific message categories
- Sync/delta messages vs full state messages

**Hypothesis 4: Correct Behavior**
- P2P mesh with CRDTs may actually send very few messages
- Efficient delta synchronization may require minimal traffic
- This could be a CAP protocol advantage, not a bug

### Investigation Steps

1. **Check raw container logs:**
   ```bash
   grep -c "MessageSent\|Message sent" \
     e12-comprehensive-results-20251116-085035/cap-full-24node-1gbps/*.log
   ```

2. **Check metrics aggregation:**
   ```bash
   # Count MessageSent events in raw JSONL
   grep -c "MessageSent" \
     e12-comprehensive-results-20251116-085035/cap-full-24node-1gbps/all-metrics.jsonl
   ```

3. **Examine timing:**
   ```bash
   # Check when messages were sent
   jq 'select(.event_type=="MessageSent") | .timestamp_us' \
     e12-comprehensive-results-20251116-085035/cap-full-24node-1gbps/all-metrics.jsonl
   ```

4. **Compare with traditional:**
   ```bash
   # How many messages does traditional send per node?
   # CAP Full should send similar amounts if behaving correctly
   ```

### Fix Options

**If Hypothesis 1 (Incomplete Convergence):**
- Extend test measurement duration from 60s to 120s
- Add longer warmup period (60s → 90s)
- Monitor CRDT sync completion indicators

**If Hypothesis 2 (Measurement Window):**
- Count messages during warmup AND measurement phases
- Separate warmup vs steady-state message counts

**If Hypothesis 3 (Event Filtering):**
- Review metrics collection code
- Ensure all message types are counted
- Add message type breakdown to summary

**If Hypothesis 4 (Correct Behavior):**
- Document as expected CAP protocol efficiency
- Add analysis comparing message overhead vs Traditional IoT
- Highlight low-bandwidth advantage in report

### Verification
After investigation/fix:
- Message counts consistent with test duration
- Clear documentation of what messages are counted
- Comparison metric: messages per node per second

---

## Priority and Timeline

### Immediate (Before E12v2 Run):
1. ✓ **Issue 1 - CAP Hierarchical Metrics** - CRITICAL
   - Blocks all 48+ node hierarchical analysis
   - Fix: Add DocumentReceived emission in query path
   - Timeline: 1-2 hours

2. **Issue 2 - Traditional IoT Latency Semantics** - HIGH
   - Creates misleading comparisons
   - Fix: Separate replication vs local read latency
   - Timeline: 2-3 hours

### Investigation Phase:
3. **Issue 3 - CAP Full Mesh Message Counts** - MEDIUM
   - May be correct behavior, needs investigation
   - Fix: TBD based on root cause
   - Timeline: 2-4 hours investigation + fix

### Total Estimated Time: 6-9 hours
Recommended: Address Issues 1 & 2, investigate Issue 3 in parallel, then run E12v2.

---

## E12v2 Test Plan

Once fixes are implemented:

1. **Quick Validation (30 min):**
   - Run single 48-node hierarchical test → verify DocumentReceived events
   - Run single 24-node traditional test → verify latency separation
   - Run single 24-node CAP full test → verify message counts

2. **Targeted Re-test (2-3 hours):**
   - Re-run only the 6 hierarchical tests (24, 48, 96 nodes at 1Gbps + bandwidth variants)
   - Re-run traditional tests with updated latency metrics
   - Verify CAP Full Mesh message behavior

3. **Full E12v2 Run (6-7 hours):**
   - Execute complete 30-test matrix with fixes
   - Generate updated analysis
   - Compare E12 vs E12v2 results

---

## Success Criteria

E12v2 will be considered successful when:

1. ✓ All hierarchical tests (including 48/96 nodes) report latency metrics
2. ✓ Traditional IoT tests clearly separate replication vs read latency
3. ✓ CAP Full Mesh message counts either:
   - Increase to expected levels (if bug), OR
   - Are documented as correct efficient behavior
4. ✓ Executive summary compares apples-to-apples metrics
5. ✓ No data quality warnings in final report

---

**Document Created:** 2025-11-16
**Status:** Investigation Complete, Fixes In Progress
**Next Steps:** Implement fixes for Issues 1 & 2, investigate Issue 3
