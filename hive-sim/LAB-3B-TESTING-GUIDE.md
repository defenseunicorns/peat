# Lab 3b Testing Guide

**Implementation Date**: 2025-11-23
**Status**: Ready for Testing
**Goal**: Measure CRDT overhead in flat P2P mesh topology

---

## Quick Start

### 1. Build Container (In Progress)

```bash
# Unified container with all lab binaries
docker build -f hive-sim/Dockerfile -t hive-sim-node:latest .
```

**Status**: Building in background (check with `docker images`)

### 2. Quick Validation (5 nodes, 2 minutes)

```bash
cd hive-sim
./quick-test-lab3b.sh
```

**What it does**:
- Deploys 5-node flat mesh with 1Gbps
- Runs for 2 minutes
- Collects logs and analyzes
- Validates flat_mesh mode works

**Expected output**:
```
✅ All nodes initialized in flat mesh mode
✅ All nodes at Squad hierarchy level
✅ CRDT documents published
🎉 Lab 3b validation PASSED!
```

### 3. Full Lab 3b Suite

```bash
# Run comprehensive test (24 configurations)
./test-lab3b-hive-mesh.sh
```

**Test matrix**:
- Node counts: 5, 10, 15, 20, 30, 50
- Bandwidths: 1Gbps, 100Mbps, 1Mbps, 256Kbps
- Duration: 120s per test
- Total time: ~1.5 hours

---

## What Gets Tested

### Flat Mesh Coordinator (Core HIVE)

```
hive-mesh::FlatMeshCoordinator
├─ DynamicHierarchyStrategy: Leader election
├─ NodeRole: Leader/Member/Standalone
├─ Peer tracking: Visibility and coordination
└─ All nodes at Squad level (flat topology)
```

### CRDT Synchronization

```
CRDT Backend (Ditto)
├─ Document publishing: Node state updates
├─ P2P sync: Automatic peer-to-peer
├─ State convergence: All nodes see all updates
└─ Latency tracking: Via METRICS events
```

### Expected Behavior

**Initialization**:
```
[peer-1] === FLAT MESH MODE (Lab 3b) ===
[peer-1] Initialized as flat mesh peer at level: Squad
[peer-1] Published state update 1/20 to flat mesh
```

**Leader Election**:
```
[peer-1] Current role: Leader, 4 peers
[peer-2] Current role: Member, 4 peers
```

**CRDT Sync**:
```
METRICS: {"event_type":"DocumentInserted","node_id":"peer-1",...}
```

---

## Analysis

### Quick Analysis

```bash
# Analyze single test run
python3 analyze-lab3b-results.py <results-dir>
```

**Output**:
- Flat mesh coordination stats
- CRDT publishing counts
- Success criteria validation

### Compare to Lab 3

```bash
# Measure CRDT overhead
python3 analyze-lab3b-results.py \
    hive-flat-mesh-<timestamp> \
    p2p-mesh-comprehensive-<timestamp>
```

**Comparison metrics**:
- Lab 3 (raw TCP): Direct peer-to-peer messaging
- Lab 3b (HIVE CRDT): Same topology + CRDT sync
- **Overhead**: Difference in latency/bandwidth

---

## Success Criteria

### Must Pass ✅

1. **Flat mesh initialization**
   - All nodes start in `flat_mesh` mode
   - All nodes use `FlatMeshCoordinator`
   - All nodes at Squad hierarchy level

2. **Leader election**
   - Nodes determine roles (Leader/Member)
   - Based on capabilities (mobility, resources)
   - No failures or deadlocks

3. **CRDT publishing**
   - Each node publishes 20 updates
   - Documents appear in CRDT backend
   - METRICS events logged

4. **No crashes**
   - All containers stay running
   - No panic or fatal errors
   - Clean shutdown possible

### Nice to Have 📊

5. **Peer visibility**
   - Nodes see N-1 peers
   - Coordinator tracks peer count

6. **CRDT convergence**
   - Eventually see all peer states
   - Measured via query results

---

## Troubleshooting

### Container won't start

```bash
# Check image exists
docker images | grep hive-sim-node

# Check logs
docker logs clab-<topology-name>-peer-1

# Verify environment
docker exec clab-<topology-name>-peer-1 env | grep MODE
```

### No flat mesh mode

**Symptom**: Logs don't show "FLAT MESH MODE"

**Causes**:
- MODE not set to `flat_mesh` in topology
- Old container image (rebuild needed)
- Wrong binary executed

**Fix**:
```bash
# Rebuild container
docker build -f hive-sim/Dockerfile -t hive-sim-node:latest .

# Verify MODE in topology
grep MODE <topology-file>.yaml
```

### No CRDT publishing

**Symptom**: Zero "DocumentInserted" events

**Causes**:
- Ditto credentials missing
- Network connectivity issues
- Backend initialization failed

**Fix**:
```bash
# Check environment variables
docker exec clab-<name>-peer-1 env | grep DITTO

# Check backend logs
docker logs clab-<name>-peer-1 2>&1 | grep -i ditto
```

### Coordinator not initializing

**Symptom**: No "Initialized as flat mesh peer"

**Causes**:
- FlatMeshCoordinator compilation issue
- Missing hive-mesh dependency
- Runtime panic before init

**Fix**:
```bash
# Check for panics
docker logs clab-<name>-peer-1 2>&1 | grep -i panic

# Verify binary includes flat_mesh
docker exec clab-<name>-peer-1 /usr/local/bin/hive-sim --help
```

---

## Comparison to Other Labs

### Lab 3: P2P Mesh (Raw TCP)

**Architecture**: Direct TCP connections, simple message passing
**Complexity**: O(n²) connections, no CRDT
**Result**: Works up to 50 nodes, ~0.13ms P95 latency

### Lab 3b: Flat Mesh with CRDT

**Architecture**: Same O(n²) connections + HIVE CRDT sync
**Complexity**: P2P + CRDT overhead
**Expected**: Similar scaling, higher latency due to CRDT

### Comparison Value

**CRDT Overhead** = Lab 3b latency - Lab 3 latency

This isolates the pure cost of CRDT synchronization from:
- Hierarchy aggregation (Lab 4)
- Capability filtering
- Message routing

---

## Next Steps After Lab 3b

1. **Analyze Results**
   - Compare to Lab 3 baseline
   - Measure CRDT overhead
   - Identify saturation points

2. **Document Findings**
   - Update LAB-3B-DECISION-SUMMARY.md
   - Create comparative charts
   - Add to Epic #132 report

3. **Proceed to Lab 4**
   - Hierarchical HIVE CRDT
   - Compare Lab 3b (flat) vs Lab 4 (hierarchical)
   - Measure hierarchy benefit

4. **Complete Epic #132**
   - All 5 labs complete
   - Comprehensive empirical validation
   - Proof that HIVE scales where others fail

---

## Files

### Implementation
- `hive-mesh/src/flat_mesh.rs` - Core coordinator
- `hive-sim/src/main.rs` - Integration (flat_mesh_mode)
- `hive-sim/generate-flat-mesh-hive-topology.py` - Topology generator

### Testing
- `quick-test-lab3b.sh` - 5-node validation
- `test-lab3b-hive-mesh.sh` - Full test suite
- `analyze-lab3b-results.py` - Results analysis

### Documentation
- `LAB-3B-IMPLEMENTATION-SUMMARY.md` - Implementation details
- `LAB-3B-TESTING-GUIDE.md` - This file
- `LAB-3B-DECISION-SUMMARY.md` - Original decision doc

---

## Questions?

Check the implementation summary:
```bash
cat LAB-3B-IMPLEMENTATION-SUMMARY.md
```

Review core code:
```bash
cat ../hive-mesh/src/flat_mesh.rs
```

Test the coordinator directly:
```bash
cd ../hive-mesh && cargo test flat_mesh
```
