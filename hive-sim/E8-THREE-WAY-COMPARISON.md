# E8 Three-Way Performance Comparison Strategy

## Executive Summary

This document outlines the strategy for measuring HIVE Protocol's performance impact and benefits through a comprehensive three-way comparison across bandwidth-constrained networks.

## Test Configurations

### 1. Ditto Baseline (Pure CRDT)
**Binary:** `ditto_baseline` (renamed from `shadow_poc`)
**Configuration:** `USE_BASELINE=true`
**Behavior:**
- Pure Ditto SDK without HIVE Protocol layer
- Full n-squared replication (all nodes sync all data)
- No authorization checks
- No capability metadata overhead

**Purpose:** Establishes the performance baseline - fastest possible sync with Ditto

### 2. CAP Full Replication (CAP with n-squared data)
**Binary:** `cap_sim_node`
**Configuration:** `CAP_FILTER_ENABLED=false` (default)
**Behavior:**
- HIVE Protocol layer on top of Ditto
- Uses `Query::All` - subscribes to all documents
- Full n-squared replication (same data volume as baseline)
- Authorization checks performed but all data synced

**Purpose:** Measures pure HIVE Protocol overhead without differential update benefits

### 3. CAP Differential Updates (CAP with filtered replication)
**Binary:** `cap_sim_node`
**Configuration:** `CAP_FILTER_ENABLED=true`
**Behavior:**
- HIVE Protocol layer on top of Ditto
- Uses capability-filtered queries: `public == true OR CONTAINS(authorized_roles, '<node_type>')`
- Differential replication (only authorized data synced)
- Authorization checks performed AND data volume reduced

**Purpose:** Measures CAP with differential update benefits (authorization + bandwidth savings)

## Test Matrix

Each configuration tested across:
- **4 Bandwidth Levels:** 100Mbps, 10Mbps, 1Mbps, 256Kbps
- **3 Topology Modes:** Client-Server, Hub-Spoke, Dynamic Mesh
- **Total:** 12 test combinations per configuration = 36 tests total

## Key Metrics

1. **Convergence Time** - Time for all nodes to receive updates
2. **Mean Latency** - Average per-update transmission time
3. **P90/P99 Latency** - Tail latency distribution
4. **Measured Bandwidth** - Actual network throughput
5. **Sync Success Rate** - Percentage of nodes achieving sync

## Expected Results

### Hypothesis 1: CAP Authorization Overhead
**Comparison:** Ditto Baseline vs CAP Full Replication

**Expected Findings:**
- CAP Full should show **small overhead** (2-5%) due to:
  - Capability metadata storage
  - Authorization check logic
  - Additional abstraction layers

**What This Measures:** Pure cost of HIVE Protocol authorization

### Hypothesis 2: Differential Update Benefits
**Comparison:** CAP Full vs CAP Differential

**Expected Findings:**
- **CURRENT TEST LIMITATION:** With simple test case (1 document, everyone needs it), differential benefits will be minimal
- Filtered queries may show small overhead from query evaluation
- Future tests with realistic data (multiple documents, varied authorization) should show:
  - Reduced bandwidth usage (only authorized data synced)
  - Potentially faster convergence (less data to transfer)
  - Better scalability (O(authorized docs) vs O(all docs))

**What This Measures:** Bandwidth savings from selective replication

### Hypothesis 3: Net CAP Value Proposition
**Comparison:** Ditto Baseline vs CAP Differential

**Expected Findings:**
- Small overhead in current simple test case
- Future tests with realistic scenarios should show net benefit:
  - Authorization overhead < Bandwidth savings
  - Fine-grained access control with minimal performance cost

## Current Test Limitations

### Simple Test Case
The current test uses a single test document that all nodes need to verify sync functionality.

**Implications:**
- Differential updates can't demonstrate bandwidth benefits
- All three configurations sync ~same data volume
- Results show authorization overhead but not replication benefits

### To Demonstrate Full CAP Benefits

Future tests should include:
1. **Multiple Documents:** 10-100 documents with varied authorization
2. **Role-Based Access:** Soldiers see squad data, UAVs see different data
3. **Hierarchical Data:** Team-level, squad-level, mission-level documents
4. **Larger Payloads:** KB-MB per document to show bandwidth impact

## Test Execution Plan

1. ✅ **CAP Full Replication** - Already completed (test-results-bandwidth-20251107-131149)
2. ⏳ **Ditto Baseline** - Run bandwidth tests with `USE_BASELINE=true`
3. ⏳ **CAP Differential** - Run bandwidth tests with `CAP_FILTER_ENABLED=true`
4. ⏳ **Generate Comparison Report** - Analyze all three result sets

## Analysis Framework

### Performance Impact Formula
```
CAP_Authorization_Overhead = (CAP_Full - Ditto_Baseline) / Ditto_Baseline
CAP_Differential_Benefit = (CAP_Full - CAP_Differential) / CAP_Full
CAP_Net_Impact = (CAP_Differential - Ditto_Baseline) / Ditto_Baseline
```

### Success Criteria

**HIVE Protocol is viable if:**
- Authorization overhead < 10% in realistic scenarios
- Differential updates provide measurable bandwidth savings (future tests)
- Net impact acceptable given security benefits

## Merge to Main Readiness

Before merging this work:

1. ✅ Rename `shadow_poc` → `ditto_baseline` (completed)
2. ✅ Implement CAP filtering infrastructure (completed)
3. ⏳ Complete three-way testing (in progress)
4. ⏳ Document performance characteristics
5. ⏳ Coordinate with schema/protocol refactoring team
6. ⏳ Update ADR-008 with empirical results

## Files and Artifacts

### Test Scripts
- `test-bandwidth-constraints.sh` - CAP Full tests
- `test-bandwidth-baseline.sh` - Ditto Baseline tests
- (CAP Differential uses same script with `CAP_FILTER_ENABLED=true`)

### Result Directories
- `test-results-bandwidth-20251107-131149/` - CAP Full (completed)
- `test-results-baseline-<timestamp>/` - Ditto Baseline (pending)
- `test-results-cap-differential-<timestamp>/` - CAP Differential (pending)

### Binaries
- `ditto_baseline` - Pure Ditto (no CAP)
- `cap_sim_node` - HIVE Protocol (configurable filtering)

## Next Steps

1. Wait for Docker build to complete
2. Run Ditto Baseline tests (12 combinations)
3. Run CAP Differential tests (12 combinations)
4. Generate comprehensive comparison report
5. Update ADR-008 with findings
6. Merge to main and coordinate with protocol team

---

**Document Status:** Living document, updated as tests complete
**Last Updated:** 2025-11-07
**Author:** E8 Network Simulation Team
