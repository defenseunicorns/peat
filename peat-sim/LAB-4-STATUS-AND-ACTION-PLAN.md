# Lab 4: Hierarchical PEAT CRDT - Status and Action Plan

**Date**: 2025-11-24
**Epic**: #132 Comprehensive Empirical Validation
**Status**: Partially Implemented, Needs Lab 4-Specific Tests

---

## Executive Summary

Lab 4 (Hierarchical PEAT CRDT) infrastructure **ALREADY EXISTS** but is not structured as a standalone lab for Epic #132 comparison. The hierarchical mode was built for backend comparison tests (Ditto vs Automerge) rather than architecture comparison (Flat vs Hierarchical).

**Key Finding**: We have all the building blocks needed for Lab 4, but need to:
1. Create Lab 4-specific test scripts (mirroring Lab 3b structure)
2. Add proper metrics instrumentation for hierarchy-aware analysis
3. Run tests at scales that compare directly to Labs 1-3b

---

## What Already Exists ✅

### 1. Hierarchical Mode Implementation ✅

**Location**: `peat-sim/src/main.rs:1463-1488`

```rust
async fn hierarchical_mode(
    backend: &dyn DataSyncBackend,
    node_id: &str,
    node_type: &str,
    update_rate_ms: u64,
) -> Result<(), Box<dyn std::error::Error>>
```

**Features**:
- ✅ Role-based execution (soldier, squad_leader, platoon_leader, company_commander)
- ✅ Capability reporting from soldiers
- ✅ Leader aggregation loops
- ✅ Event-driven via CRDT change streams
- ✅ Works with both Ditto and Automerge backends

**Triggered by**: `MODE=hierarchical` environment variable

---

### 2. Hierarchy Core Library ✅

**Location**: `peat-mesh/src/hierarchy/`

**Modules**:
- ✅ `dynamic_strategy.rs` - Capability-based leader election
- ✅ `static_strategy.rs` - Fixed role assignment
- ✅ `hybrid_strategy.rs` - Combined approach
- ✅ `mod.rs` - Trait definitions (HierarchyStrategy, NodeRole)

**Used by Lab 3b**: FlatMeshCoordinator uses DynamicHierarchyStrategy

---

### 3. Hierarchical Topology Generators ✅

**Primary Generator**: `generate-hierarchical-topology.py`

**Features**:
- ✅ Multi-squad platoons
- ✅ Gateway nodes (multi-homed)
- ✅ Isolated squad networks + backhaul
- ✅ Configurable squad sizes
- ✅ Bandwidth constraint support

**Pre-Generated Topologies**:
```
topologies/test-backend-{ditto,automerge}-{24,48,96}n-hierarchical-{1gbps,100mbps,1mbps,256kbps}.yaml
```

**Total**: 24 pre-generated hierarchical topologies (2 backends × 3 scales × 4 bandwidths)

---

### 4. Existing Test Scripts ✅

**Backend Comparison Tests**:
- `test-hierarchical-only.sh` - Runs hierarchical topologies only
- `test-backend-comparison-hierarchical.sh` - Full backend comparison
- `test-peat-hierarchical-suite.sh` - Comprehensive suite

**These test**:
- Ditto vs Automerge backends
- Traditional (O(n²)) vs Hierarchical (O(n log n))
- Multiple scales: 24, 48, 96 nodes
- Multiple bandwidths: 1Gbps, 100Mbps, 1Mbps, 256Kbps

**Test Matrix**: 48 tests (2 backends × 3 scales × 2 topologies × 4 bandwidths)

---

### 5. Documentation ✅

**Existing Guides**:
- `HIERARCHICAL-LAB-TESTING-GUIDE.md` - How to run hierarchical tests
- `HIERARCHICAL-BACKEND-COMPARISON.md` - Backend comparison methodology
- `HIERARCHICAL-GATEWAY-ARCHITECTURE.md` - Network architecture design
- `HIERARCHICAL-FILTERING-ANALYSIS.md` - Differential filtering analysis
- `HIERARCHICAL-METRICS-STATUS.md` - Metrics implementation status

---

## What's Missing for Lab 4 ⏳

### 1. Lab 4-Specific Test Script ⏳

**Need**: `test-lab4-hierarchical-peat-crdt.sh`

**Requirements**:
- Match Lab 3b structure for direct comparison
- Same node counts as Lab 3b: 5, 10, 15, 20, 30, 50 (where feasible)
- Extended scales: 96, 192, 384, 1000 (to show hierarchy benefit)
- Same bandwidths: 1Gbps, 100Mbps, 1Mbps, 256Kbps
- Single backend (Ditto) for consistency with Lab 3b
- Focus on ARCHITECTURE comparison, not backend comparison

**Test Matrix**:
```
Node Counts:  24, 48, 96, 384, 1000 (5 scales)
Bandwidths:   1Gbps, 100Mbps, 1Mbps, 256Kbps (4 bandwidths)
Total:        20 tests (vs 24 for Lab 3b)
```

**Key Difference from Existing Tests**:
- Existing tests compare: Ditto vs Automerge (backend comparison)
- Lab 4 needs: Hierarchical vs Flat (architecture comparison)
- Lab 4 should compare DIRECTLY to Lab 3b results

---

### 2. Hierarchy-Aware Metrics ⏳

**Current Metrics** (from hierarchical_mode):
- Soldier capability updates
- Leader aggregation counts
- Basic timing information

**Needed Metrics** (for Lab 4 analysis):
1. **CRDT Latency by Tier**:
   - Squad-level sync latency (soldier → squad leader)
   - Platoon-level sync latency (squad leader → platoon leader)
   - Company-level sync latency (platoon leader → company HQ)

2. **E2E Propagation**:
   - Time from soldier update → visible at HQ
   - Hop count distribution
   - Path tracking (which tiers were traversed)

3. **Aggregation Efficiency**:
   - Reduction ratio at each tier (N NodeStates → 1 SquadSummary)
   - Total operations: hierarchical vs flat
   - Bandwidth savings measurement

4. **Connection Metrics**:
   - Connections per node by tier
   - Compare to Lab 3b's O(n²) connections

**Instrumentation Needed**:
```rust
// In soldier_capability_mode():
let start = Instant::now();
backend.document_store().upsert("sim_poc", document).await?;
let latency = start.elapsed().as_secs_f64() * 1000.0;
println!("[METRICS] {{\"event_type\":\"CRDTUpsert\",\"node_id\":\"{}\",\"tier\":\"soldier\",\"latency_ms\":{:.3}}}", node_id, latency);

// In squad leader aggregation:
println!("[METRICS] {{\"event_type\":\"SquadAggregation\",\"squad_id\":\"{}\",\"nodes_aggregated\":{},\"latency_ms\":{:.3}}}", squad_id, node_count, latency);
```

---

### 3. Lab 4 Analysis Scripts ⏳

**Need**: `analyze-lab4-results.py`

**Features**:
- Parse hierarchical topology logs
- Extract tier-specific latencies
- Calculate aggregation efficiency
- Generate comparison metrics vs Lab 3b

**Need**: `compare-lab3b-vs-lab4.py`

**Features**:
- Direct flat vs hierarchical comparison
- Show scaling benefit of hierarchy
- Connection count reduction
- Latency improvement at scale
- Bandwidth efficiency gains

---

### 4. Lab 4 Topology Generators (Minor Gap) ⏳

**Current**: Pre-generated topologies for 24, 48, 96 nodes

**Needed**:
- 384-node hierarchical topology (multi-company)
- 1000-node hierarchical topology (battalion-scale)

**Structure for 384 nodes**:
```
Company HQ (1)
├── Platoon 1 Leader (1)
│   ├── Squad 1A Leader (1) + 6 soldiers = 7
│   ├── Squad 1B Leader (1) + 6 soldiers = 7
│   ├── Squad 1C Leader (1) + 6 soldiers = 7
│   └── Squad 1D Leader (1) + 6 soldiers = 7
│   Total: 1 platoon leader + 28 soldiers = 29
├── Platoon 2-4: same structure (29 each)
Total per company: 1 + (4 platoons × 29) = 117

For 384 nodes: Need ~3.3 companies
Adjusted: 3 companies × 128 nodes = 384
```

**Can use existing generator**: Just extend `generate-hierarchical-topology.py` for larger scales

---

### 5. Documentation Updates ⏳

**Need**:
- `LAB-4-TESTING-GUIDE.md` - How to run Lab 4 tests
- `LAB-4-IMPLEMENTATION-SUMMARY.md` - What's implemented, how it works
- `LAB-4-RESULTS-ANALYSIS.md` - Analysis of Lab 4 results (after tests)
- Update `EPIC-132-LAB-COMPARISON-STATUS.md` with Lab 4 details

---

## Lab 4 vs Existing Hierarchical Tests

### Existing Tests Focus

**Backend Comparison** (Ditto vs Automerge):
- Test both backends with same topologies
- Compare CRDT implementations
- Measure which backend is more efficient

**Topology Comparison** (Traditional vs Hierarchical):
- Compare hub-spoke (traditional) to multi-tier (hierarchical)
- Show 95% bandwidth reduction via differential filtering
- Prove O(n log n) scaling vs O(n²)

### Lab 4 Should Focus

**Architecture Comparison** (Flat CRDT vs Hierarchical CRDT):
- Use SAME backend (Ditto) as Lab 3b
- Compare flat mesh (Lab 3b) to hierarchical mesh (Lab 4)
- Isolate pure hierarchy benefit
- Measure at scales where Lab 3b breaks (30-50+ nodes)

**Key Insight**: Lab 4 is NOT about Ditto vs Automerge. It's about **flat P2P with CRDT** (Lab 3b) vs **hierarchical P2P with CRDT** (Lab 4).

---

## Lab 4 Test Plan

### Scales to Test

| Nodes | Topology Structure | Comparison Point |
|-------|-------------------|------------------|
| 24    | 1 platoon, 3 squads × 7 soldiers | Direct comparison to Lab 3b-20 |
| 48    | 2 platoons, 6 squads × 7 soldiers | Lab 3b breaks here |
| 96    | 4 platoons, 12 squads × 7 soldiers | Beyond Lab 3b capacity |
| 384   | 3 companies, ~16 squads/company | Large-scale validation |
| 1000  | 8 companies, 125 soldiers/company | Battalion-scale proof |

### Expected Results

**Lab 3b Baseline** (from existing data):
- 5 nodes: 2.7ms P95 (works fine)
- 20 nodes: 77.9ms P95 (marginal)
- 50 nodes: 399.6ms P95 (unacceptable)

**Lab 4 Hypothesis**:
- 24 nodes: <10ms P95 (squad-level sync fast)
- 48 nodes: <20ms P95 (2 hops: soldier → squad → platoon)
- 96 nodes: <30ms P95 (2-3 hops max)
- 384 nodes: <50ms P95 (3-4 hops: soldier → squad → platoon → company)
- 1000 nodes: <100ms P95 (4-5 hops max, bounded by depth)

**Key Proof**: Lab 4 should maintain bounded latency at scales where Lab 3b fails.

---

## Action Plan

### Phase 1: Metrics Instrumentation (2-4 hours)

1. **Add CRDT latency tracking to hierarchical_mode()**
   - Instrument soldier_capability_mode() upsert operations
   - Instrument leader aggregation operations
   - Add tier labels (soldier, squad, platoon, company)

2. **Add E2E propagation tracking**
   - Track message IDs through hierarchy
   - Measure time from soldier → HQ visibility
   - Count hops through tiers

3. **Add aggregation efficiency metrics**
   - Count documents aggregated at each tier
   - Measure reduction ratio
   - Compare to flat mesh message count

### Phase 2: Lab 4 Test Script (4-6 hours)

1. **Create test-lab4-hierarchical-peat-crdt.sh**
   - Model after test-lab3b-peat-mesh.sh structure
   - Use Ditto backend only (consistency with Lab 3b)
   - Test scales: 24, 48, 96, 384, 1000 nodes
   - Generate CSV results matching Lab 3b format

2. **Extend topology generators if needed**
   - Generate 384-node topology (multi-company)
   - Generate 1000-node topology (battalion-scale)
   - Use existing generator, just larger scales

3. **Quick validation test**
   - quick-test-lab4.sh - 24 nodes, 2 minutes
   - Verify hierarchical mode works with metrics
   - Validate CSV output format

### Phase 3: Run Tests (6-8 hours)

1. **Small scale validation** (24, 48 nodes)
   - Quick smoke test
   - Verify metrics collection
   - Check hierarchy is working

2. **Medium scale tests** (96 nodes)
   - Validate hierarchy benefit emerges
   - Compare to Lab 3b @ 50 nodes

3. **Large scale tests** (384, 1000 nodes)
   - Prove Lab 4 scales where Lab 3b cannot
   - Collect scaling data

### Phase 4: Analysis Scripts (4-6 hours)

1. **Create analyze-lab4-results.py**
   - Parse tier-specific latencies
   - Calculate aggregation efficiency
   - Generate summary statistics

2. **Create compare-lab3b-vs-lab4.py**
   - Direct flat vs hierarchical comparison
   - Scaling curves (latency vs node count)
   - Connection count comparison
   - Bandwidth efficiency calculation

3. **Update compare-all-labs.py**
   - Include Lab 4 in Epic #132 final report
   - Generate visualization charts
   - Complete empirical validation summary

### Phase 5: Documentation (2-3 hours)

1. **Create Lab 4 documentation**
   - LAB-4-TESTING-GUIDE.md
   - LAB-4-IMPLEMENTATION-SUMMARY.md
   - LAB-4-RESULTS-ANALYSIS.md

2. **Update Epic #132 documentation**
   - Update EPIC-132-LAB-COMPARISON-STATUS.md
   - Add Lab 4 to LAB-EXPERIMENTS-SUMMARY.md
   - Create final comparison report

---

## Estimated Timeline

| Phase | Duration | Effort |
|-------|----------|--------|
| Phase 1: Metrics | 2-4 hours | Small |
| Phase 2: Test Scripts | 4-6 hours | Medium |
| Phase 3: Run Tests | 6-8 hours | Large (mostly waiting) |
| Phase 4: Analysis | 4-6 hours | Medium |
| Phase 5: Documentation | 2-3 hours | Small |
| **Total** | **18-27 hours** | **~3-4 days** |

**Critical Path**: Metrics instrumentation → Test script creation → Test execution

---

## Key Decisions

### 1. Backend Choice for Lab 4

**Decision**: Use **Ditto only** for Lab 4

**Rationale**:
- Lab 3b used Ditto (for consistency)
- Epic #132 is about architecture comparison, not backend comparison
- Backend comparison is a separate concern (already tested)
- Single backend = cleaner comparison to Lab 3b

### 2. Node Scales for Lab 4

**Decision**: Test 24, 48, 96, 384, 1000 nodes

**Rationale**:
- 24: Direct comparison to Lab 3b-20
- 48: Where Lab 3b starts to struggle
- 96: Beyond Lab 3b capacity, prove hierarchy works
- 384: Multi-company, battalion-level validation
- 1000: Ultimate proof - Battalion-scale deployment

**Note**: Skip 5, 10, 15, 20, 30 node tests (too small for hierarchy benefit)

### 3. Hierarchy Structure

**Decision**: Use existing **Squad → Platoon → Company** structure

**Squad Size**: 7-8 nodes (including leader)
**Platoon Size**: 4 squads (~29 nodes)
**Company Size**: 4 platoons (~117 nodes)

**Rationale**:
- Matches existing topologies
- Realistic military structure
- Proven to work in backend comparison tests

---

## Risk Assessment

### Low Risk ✅

- **Infrastructure exists**: All core code is implemented
- **Topologies exist**: Pre-generated for 24, 48, 96 nodes
- **Tests exist**: Backend comparison tests prove it works

### Medium Risk ⚠️

- **Metrics instrumentation**: Need to add proper tracking
- **Large-scale topologies**: Need to generate 384, 1000 node topologies
- **Test execution time**: Large scales may take hours

### High Risk ❌

- **None identified**: Lab 4 is mostly integration work

---

## Success Criteria

### Technical Success ✅

1. **Lab 4 tests run successfully** at all scales (24-1000 nodes)
2. **Metrics collected** for all hierarchical tiers
3. **Comparison scripts** generate clear Lab 3b vs Lab 4 analysis
4. **Documentation complete** for Epic #132

### Scientific Success ✅

1. **Hierarchy benefit proven**: Lab 4 maintains <50ms P95 at 1000 nodes
2. **Scaling curves show**: O(log n) for Lab 4 vs O(n²) for Lab 3b
3. **Connection reduction measured**: Lab 4 has O(n log n) connections
4. **Epic #132 complete**: All 5 labs validated empirically

---

## Next Steps

### Immediate (Today)

1. ✅ **Assessment complete** - This document
2. ⏳ **Review with team** - Confirm approach
3. ⏳ **Prioritize phases** - What to do first?

### Short Term (This Week)

4. ⏳ **Phase 1: Add metrics** - Instrument hierarchical_mode()
5. ⏳ **Phase 2: Create test script** - test-lab4-hierarchical-peat-crdt.sh
6. ⏳ **Quick validation** - Run 24-node test

### Medium Term (Next Week)

7. ⏳ **Phase 3: Run tests** - Full test matrix (20 tests)
8. ⏳ **Phase 4: Analysis** - Create comparison scripts
9. ⏳ **Phase 5: Documentation** - Complete Lab 4 docs

### Final (Epic #132 Completion)

10. ⏳ **Generate final report** - All 5 labs compared
11. ⏳ **Publish results** - Reproducible methodology + data
12. ⏳ **Close Epic #132** - Empirical validation complete

---

## Conclusion

Lab 4 infrastructure **ALREADY EXISTS** - we just need to adapt it for Epic #132 comparison:

✅ **What we have**:
- Hierarchical mode implementation
- Topology generators
- Pre-generated topologies (24, 48, 96 nodes)
- Test scripts (backend comparison)
- Core hierarchy library

⏳ **What we need**:
- Lab 4-specific test script (architecture comparison)
- Hierarchy-aware metrics (tier-specific latencies)
- Analysis scripts (Lab 3b vs Lab 4 comparison)
- Large-scale topologies (384, 1000 nodes)
- Lab 4 documentation

**Effort**: 3-4 days of focused work

**Outcome**: Complete Epic #132 with empirical proof that hierarchical CRDT scales logarithmically to 1000+ nodes where flat mesh fails.

🎯 **Goal**: Prove PEAT's hierarchical CRDT maintains <50ms P95 at 1000 nodes (vs 399ms at 50 nodes for Lab 3b flat mesh).
