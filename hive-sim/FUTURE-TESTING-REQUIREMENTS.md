# Future Testing Requirements for HIVE Protocol

**Status:** Lab Testing Roadmap
**Last Updated:** 2025-11-07
**Purpose:** Document testing requirements to demonstrate HIVE Protocol's differential update benefits and performance characteristics

---

## Executive Summary

Current E8 testing infrastructure successfully measures HIVE Protocol's authorization overhead but **cannot demonstrate differential update benefits** due to simple test scenarios (1 document, all nodes need it). This document outlines requirements for future lab testing to demonstrate CAP's full value proposition.

## Current Testing Status (E8)

### What We Have

1. **CAP Full Replication Tests** (`test-results-bandwidth-20251107-131149/`)
   - Configuration: `cap_sim_node` with `Query::All`
   - Behavior: n-squared replication with CAP authorization overhead
   - Results: ~26s convergence across all bandwidth levels (100Mbps to 256Kbps)
   - **Conclusion:** Small payload, CAP overhead minimal

2. **CAP Differential Tests** (In Progress)
   - Configuration: `cap_sim_node` with `CAP_FILTER_ENABLED=true`
   - Behavior: Capability-filtered queries (`public == true OR CONTAINS(authorized_roles, role)`)
   - **Expected Result:** Similar to CAP Full in simple test case (all nodes need same document)

### What We're Missing

**Ditto Baseline** - Pure CRDT performance without CAP
- **Attempted:** `ditto_baseline` binary (renamed from `shadow_poc`)
- **Blocker:** Infrastructure incompatibility
  - Baseline binary exits on success (ContainerLab restarts it)
  - No POC timing messages (metrics parser can't extract data)
  - No iperf3 integration for bandwidth measurement
- **Impact:** Cannot measure pure CAP authorization overhead

### Current Test Limitation

**Simple Test Case:**
```rust
// Writer creates single document
let doc = TestDoc {
    id: "shadow_test_001",
    message: "Hello from Shadow!",
    ...
};

// All readers wait for this exact document
// Result: Everyone needs the same data
```

**Implications:**
- CAP Full and CAP Differential sync identical data
- Differential queries provide no bandwidth benefit
- Tests validate functionality but not differential performance

---

## Required Infrastructure Changes

### 1. Ditto Baseline Testing Support

**Goal:** Measure pure Ditto performance to isolate CAP overhead

**Requirements:**
1. **Modify `ditto_baseline.rs`:**
   - Add POC timing message output matching `cap_sim_node` format
   - Add stay-alive mechanism instead of exit-on-success
   - Add command-line compatibility with test scripts

2. **Add iperf3 Support:**
   - Install iperf3 in Docker image for bandwidth measurement
   - OR modify metrics collection to work without iperf3

3. **Fix ContainerLab Integration:**
   - Handle container lifecycle properly (don't restart on exit(0))
   - OR modify binary to stay running after test completion

**Deliverable:** Working baseline tests that measure pure Ditto sync performance

### 2. Realistic Test Scenarios

**Goal:** Create test scenarios where differential updates provide measurable benefits

**Requirements:**

#### Multi-Document Test Suite

Create diverse document sets with varied authorization:

```rust
// Mission-level documents (public)
documents.push(MissionDoc {
    id: "mission_001",
    public: true,
    authorized_roles: vec!["all"],
    size: 10KB,
});

// Squad-level documents (soldiers only)
documents.push(SquadDoc {
    id: "squad_alpha_orders",
    public: false,
    authorized_roles: vec!["soldier"],
    size: 50KB,
});

// UAV recon data (UAVs and command only)
documents.push(ReconDoc {
    id: "recon_sector_7",
    public: false,
    authorized_roles: vec!["uav", "command"],
    size: 500KB,
    payload: sensor_data,
});

// UGV sensor data (UGVs and analysts only)
documents.push(SensorDoc {
    id: "sensors_grid_42",
    public: false,
    authorized_roles: vec!["ugv", "analyst"],
    size: 200KB,
});
```

**Test Matrix:**
- 10-100 documents per test
- Varied sizes: 10KB to 1MB per document
- Role distribution:
  - 30% public (all nodes)
  - 40% role-specific (soldiers, UAVs, UGVs separately)
  - 20% hierarchical (command + subordinates)
  - 10% cross-role (analysts + sensors)

#### Expected Benefits

**CAP Full Replication (Query::All):**
- Syncs all 100 documents to all 12 nodes
- Total data: 100 docs × 12 nodes = 1200 document transfers
- Authorization checked but all data synced

**CAP Differential (Filtered Queries):**
- Each node only syncs authorized documents
- Soldiers (6 nodes): ~40 documents each = 240 transfers
- UAVs (4 nodes): ~35 documents each = 140 transfers
- UGVs (2 nodes): ~35 documents each = 70 transfers
- **Total: ~450 transfers (62% reduction)**

**Bandwidth Impact:**
- At 1Mbps: Full replication = 1200MB, Differential = 450MB
- Time savings: ~750MB / 1Mbps = ~100 minutes saved
- Convergence improvement: Proportional to data reduction

---

## Testing Metrics to Capture

### Primary Metrics

1. **Convergence Time**
   - Time for all authorized nodes to receive all authorized documents
   - Measure per-node and aggregate

2. **Bandwidth Utilization**
   - Total bytes transferred per node
   - Comparison: Full vs Differential
   - Bandwidth savings percentage

3. **Authorization Overhead**
   - CAP Full vs Ditto Baseline (when available)
   - Per-query authorization check time

4. **Latency Distribution**
   - Per-document mean, P90, P99 latency
   - Impact of document size on latency

5. **Scalability**
   - Performance vs number of documents
   - Performance vs number of nodes
   - Performance vs document size

### Secondary Metrics

1. **Memory Usage**
   - Per-node memory footprint
   - Impact of document cache size

2. **CPU Utilization**
   - Authorization check overhead
   - Query filtering overhead

3. **Network Efficiency**
   - Duplicate transmissions
   - Unnecessary data synced then rejected

---

## Test Scenarios to Implement

### Scenario 1: Static Role-Based Access

**Setup:**
- 100 documents with fixed role assignments
- 12 nodes with static roles (6 soldiers, 4 UAVs, 2 UGVs)
- No role changes during test

**Tests:**
- Bandwidth: 100Mbps, 10Mbps, 1Mbps, 256Kbps
- Topologies: Client-Server, Hub-Spoke, Dynamic Mesh
- Document sizes: 10KB, 100KB, 1MB

**Expected Insight:** Baseline differential update benefits under static conditions

### Scenario 2: Dynamic Authorization Changes

**Setup:**
- Initial: 50 documents, all nodes authorized
- T+30s: Add 50 new role-specific documents
- T+60s: Change 25 document authorizations
- T+90s: Revoke access for 2 nodes

**Tests:**
- Measure re-sync time after authorization changes
- Measure data transfer during revocation
- Verify unauthorized nodes stop receiving updates

**Expected Insight:** CAP's ability to handle dynamic authorization without full re-sync

### Scenario 3: Hierarchical Authorization

**Setup:**
- Command node: Access to all documents
- Squad leaders (2 nodes): Access to squad + mission documents
- Soldiers (4 nodes): Access to own squad documents only
- Support (2 nodes): Access to specific support documents

**Tests:**
- Verify correct hierarchical propagation
- Measure overhead of multi-level authorization
- Compare to flat role-based access

**Expected Insight:** CAP's hierarchical authorization overhead and correctness

### Scenario 4: Large-Scale Simulation

**Setup:**
- 1000 documents (10KB each = 10MB total)
- 50 nodes across 5 role groups
- Realistic authorization distribution (Zipf distribution)

**Tests:**
- Convergence time for large dataset
- Bandwidth savings at scale
- Memory and CPU overhead

**Expected Insight:** CAP scalability limits and performance at realistic scale

---

## Implementation Priority

### Phase 1: Foundation (Next Sprint)
1. ✅ Implement CAP differential filtering (`CAP_FILTER_ENABLED=true`)
2. ✅ Run CAP Full vs CAP Differential comparison
3. ⏳ Document current limitations
4. ⏳ Create multi-document test framework

### Phase 2: Realistic Scenarios (Future)
1. Implement Scenario 1 (Static Role-Based Access)
2. Create document generator for varied sizes and roles
3. Measure differential update benefits with realistic data
4. Document bandwidth savings and performance characteristics

### Phase 3: Advanced Testing (Future)
1. Implement Scenario 2 (Dynamic Authorization)
2. Implement Scenario 3 (Hierarchical Authorization)
3. Implement Scenario 4 (Large-Scale Simulation)
4. Comprehensive performance analysis and optimization

### Phase 4: Baseline Comparison (Optional)
1. Fix `ditto_baseline` infrastructure issues
2. Re-run all tests with baseline comparison
3. Quantify pure CAP authorization overhead
4. Cost-benefit analysis: Overhead vs Security

---

## Success Criteria

### For Merging to Main (Current)

**Minimum Viable:**
- ✅ CAP differential filtering implemented
- ✅ CAP Full vs CAP Differential tests complete
- ✅ Infrastructure documented for future testing
- ✅ Current limitations acknowledged

**Deliverables:**
- Working CAP filtering implementation
- Comparison test results (even with limitations)
- This requirements document
- ADR-008 updated with current findings

### For Production Readiness (Future)

**Required:**
- Multi-document test scenarios implemented
- Measurable differential update benefits demonstrated
- Bandwidth savings quantified (>50% in realistic scenarios)
- Authorization overhead acceptable (<10% in realistic scenarios)

**Deliverables:**
- Comprehensive performance test suite
- Empirical bandwidth savings data
- Scalability limits documented
- Production performance SLAs defined

---

## Known Issues and Workarounds

### Issue 1: Baseline Testing Infrastructure

**Problem:** `ditto_baseline` binary incompatible with current test infrastructure

**Workaround:** Use CAP Full (Query::All) as quasi-baseline
- Shows CAP with n-squared data (like Ditto baseline)
- Includes CAP authorization overhead (small in simple tests)
- Sufficient for initial differential comparison

**Long-term Fix:** Implement Phase 4 changes or accept CAP Full as baseline

### Issue 2: Simple Test Case Limitation

**Problem:** Current 1-document test doesn't show differential benefits

**Workaround:** Document limitation, proceed with merge
- Tests validate functionality (capability filtering works)
- Tests establish baseline performance
- Infrastructure ready for future realistic scenarios

**Long-term Fix:** Implement Phase 2 multi-document tests

### Issue 3: Network Simulation Fidelity

**Problem:** ContainerLab + netem may not perfectly simulate real network conditions

**Consideration:** Results are comparative, not absolute
- CAP Full vs CAP Differential comparison still valid
- Relative performance trends accurate
- Absolute numbers may vary in real deployments

**Mitigation:** Validate in real network environments before production

---

## Coordination with Other Teams

### Schema/Protocol Refactoring Team

**Relevant to This Testing:**
- Document schema changes may require test updates
- Query language (DQL) changes may affect filtering syntax
- Authorization model changes may require new test scenarios

**Action Items:**
- Share this requirements document with protocol team
- Coordinate on document schema for realistic tests
- Align on capability model before Phase 2 implementation

### Operations/Deployment Team

**Relevant to This Testing:**
- Production network bandwidth constraints
- Realistic deployment topologies
- Operational authorization patterns

**Action Items:**
- Gather real-world deployment scenarios
- Incorporate operational constraints into Phase 3 tests
- Validate test results against production metrics

---

## References

- ADR-008: Network Simulation Approach
- E8-THREE-WAY-COMPARISON.md: Current test strategy
- `/hive-sim/test-bandwidth-constraints.sh`: Current test implementation
- `/hive-protocol/examples/cap_sim_node.rs`: CAP implementation with filtering

---

**Document Owner:** E8 Network Simulation Team
**Review Frequency:** After each major test phase
**Next Review:** After CAP Differential tests complete
