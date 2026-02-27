# Lab 3b Decision Point - Team Input Needed

## Current Status: Epic #132 (75% Complete)

### ✅ Completed Labs (3/4)

| Lab | Architecture | Status | Key Finding |
|-----|--------------|--------|-------------|
| Lab 1 | Producer-Only (no CRDT) | ✅ Complete | No saturation at 1000 nodes |
| Lab 2 | Client-Server (no CRDT) | ✅ Complete | Saturates at 1000n (O(n²) broadcast) |
| Lab 3 | P2P Mesh (no CRDT) | ✅ Complete | Works up to 50 nodes (1,225 connections) |

### 🤔 Proposed Lab 3b (Optional)

**Goal**: P2P Mesh with PEAT CRDT (flat - no hierarchy)

**Why**: Isolate CRDT overhead vs hierarchical benefits
- Lab 3 vs Lab 3b → Shows CRDT overhead
- Lab 3b vs Lab 4 → Shows hierarchical benefits

**Current Issue**: Ditto backend requires hierarchical structure
- `peat-sim` binary expects roles (platoon_leader, squad_leader, etc.)
- Flat mesh initialization fails

### ⏳ Remaining Lab

| Lab | Architecture | Status | Priority |
|-----|--------------|--------|----------|
| Lab 4 | PEAT Hierarchical CRDT | Pending | **HIGH** - Main validation goal |

---

## Decision Needed

### Question for Team

**Should we pursue Lab 3b (P2P flat mesh with CRDT) or skip it?**

### Option A: Skip Lab 3b → Focus on Lab 4
**Pros**:
- Lab 4 is the primary objective (hierarchical scaling proof)
- Already have 3 strong baselines (Labs 1-3)
- Faster to completion (75% → 100%)
- Can infer CRDT overhead from Lab 4 vs Lab 2

**Cons**:
- No direct measurement of CRDT overhead in flat topology
- Can't separate "CRDT cost" from "hierarchy benefit"

**Effort**: None
**Time to Epic completion**: ~1-2 days (just Lab 4)

---

### Option B: Implement Lab 3b (Recommended Approaches)

#### B1: Try Automerge Backend
- May be more flexible than Ditto for flat mesh
- Update topology to use `BACKEND=automerge`

**Pros**: Reuses existing infrastructure
**Cons**: May still fail, Automerge less tested
**Effort**: Medium (1-2 hours)

#### B2: Create Dedicated Binary
- New binary: `peat_flat_mesh.rs`
- Simplified CRDT without hierarchical aggregation
- Direct peer-to-peer CRDT sync

**Pros**: Clean, controlled implementation
**Cons**: Most work, new code to maintain
**Effort**: High (3-4 hours)

#### B3: Minimal Test (Quick Validation)
- 5 nodes only, simplified comparison
- Use existing mode but all as squad members
- Limited but validates CRDT works

**Pros**: Quick validation
**Cons**: Limited data, not comprehensive
**Effort**: Low (30 min - 1 hour)

**Time to Epic completion**: +1-4 hours then Lab 4 (~2-3 days total)

---

## Scientific Value Assessment

### With Lab 3b (Option B)
**Can Answer**:
- ✅ What is CRDT overhead? (Lab 3 vs Lab 3b)
- ✅ What is hierarchy benefit? (Lab 3b vs Lab 4)
- ✅ Does CRDT change 50-node limit from Lab 3?

**Epic Completeness**: 100% (4/4 labs)

### Without Lab 3b (Option A)
**Can Answer**:
- ✅ Architectural limits (Labs 1-3)
- ✅ Hierarchical CRDT scaling (Lab 4)
- ⚠️ CRDT overhead: Inferred, not measured

**Epic Completeness**: 75% (3/4 labs) but covers main objective

---

## Recommendation

**Option A (Skip Lab 3b)** is recommended because:

1. **Lab 4 is the primary validation goal**
   - Proving PEAT scales to 1000+ nodes
   - This is what justifies the architecture

2. **Strong baseline already exists**
   - Lab 1-3 show architectural bottlenecks
   - Lab 4 comparison to these is sufficient

3. **Time/value tradeoff**
   - Lab 3b: Medium effort, nice-to-have data
   - Lab 4: High value, required for Epic #132

4. **Can still estimate CRDT overhead**
   - Compare Lab 4 (hierarchical CRDT) to Lab 2 (hierarchical no-CRDT)
   - Not perfect but gives signal

---

## Team Input Requested

**Questions**:
1. Is measuring pure CRDT overhead (Lab 3b) required for Epic #132?
2. Can we accept inferring CRDT cost from Lab 4 vs Lab 2 comparison?
3. If Lab 3b is required, which implementation approach (B1/B2/B3)?
4. Should we prioritize completing Epic #132 (skip Lab 3b) or comprehensive data (include Lab 3b)?

**Recommendation**: Skip Lab 3b, complete Lab 4, close Epic #132 at 75% with strong validation of hierarchical benefits.

---

## Current Artifacts

**Created for Lab 3b**:
- ✅ `generate-flat-mesh-peat-topology.py` - Topology generator
- ✅ `test-lab3b-peat-mesh.sh` - Test script
- ⚠️ Infrastructure ready, pending backend solution

**Can be completed if team decides Lab 3b is valuable.**

---

## Timeline Impact

| Path | Remaining Work | Estimated Time |
|------|----------------|----------------|
| **Option A** | Lab 4 only | 1-2 days |
| **Option B1** | Lab 3b (Automerge) + Lab 4 | 2-3 days |
| **Option B2** | Lab 3b (new binary) + Lab 4 | 3-4 days |
| **Option B3** | Lab 3b (minimal) + Lab 4 | 2 days |

**Epic #132 goal**: Empirical validation that PEAT scales where other architectures fail ← **Lab 4 achieves this**
