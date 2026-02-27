# Baseline Testing Requirements for PEAT Protocol Evaluation

**Date:** 2025-11-07
**Purpose:** Define baseline testing framework to appropriately compare CAP's architectural differences
**Context:** Phase 5 protobuf migration complete, ready for comparative analysis

## Executive Summary

To properly evaluate PEAT Protocol's architectural benefits, we need structured baseline comparisons across three dimensions:

1. **CAP Full Replication** vs **CAP Differential Filtering** (capability-based optimization)
2. **PEAT Protocol** vs **Ditto Baseline** (CRDT without capability model)
3. **Topology Modes** (client-server vs hub-spoke vs dynamic mesh)

This document defines the testing framework required to demonstrate CAP's value proposition.

## Background

### PEAT Protocol Architecture

CAP (Capability-Aware Protocol) provides:
- **Capability-based authorization:** Role-based access control at data model level
- **Differential filtering:** Query-based selective sync (`authorized_roles` filtering)
- **Ontology separation:** Schema (peat-schema) independent from protocol (peat-protocol)

### Current Test Status

**Completed (PR #56 - E8 Phase 1):**
- ✅ 12-node squad formation testing
- ✅ 3 topology modes (client-server, hub-spoke, dynamic mesh)
- ✅ 4 bandwidth levels (100Mbps, 10Mbps, 1Mbps, 256Kbps)
- ✅ Both CAP Full and CAP Differential configurations
- ✅ Metrics collection framework (JSON output)

**Limitation:**
Single test document that all nodes are authorized to access → CAP Differential cannot demonstrate bandwidth savings in current test scenario.

## Baseline Comparison Matrix

### Axis 1: Architectural Comparison (PRIMARY)

| Architecture | Description | Data Model | Sync Mechanism |
|--------------|-------------|------------|----------------|
| **Traditional IoT Baseline** | Event-driven messaging | Full state messages | Periodic transmission (no CRDT) |
| **CAP Full Replication** | CRDT-based sync | Delta-state CRDTs | Automatic convergence (Query::All) |
| **CAP Differential Filtering** | CRDT + capability filtering | Delta-state CRDTs | Capability-filtered queries |

**Key Questions:**
1. What is CRDT overhead vs traditional messaging? (CAP Full vs Traditional)
2. What is CAP capability filtering benefit? (CAP Full vs CAP Differential)
3. What is net architectural advantage? (CAP Differential vs Traditional)

**Test Requirement:** Identical test scenarios run with all three configurations

### Axis 2: CAP Filtering Modes (within CRDT architecture)

| Mode | Query Type | Purpose | Expected Benefit |
|------|------------|---------|------------------|
| **CAP Full** | `Query::All` | All data replicated via CRDT | Establishes CRDT baseline performance |
| **CAP Differential** | Capability-filtered | Role-based filtering + CRDT | 50-60% bandwidth reduction in multi-doc scenarios |

**Key Difference:** Same CRDT infrastructure, different query strategies

**Test Requirement:** Multi-document scenario with varied authorization (see Phase 2 requirements below)

### Axis 3: Topology Modes

| Mode | Architecture | Use Case |
|------|--------------|----------|
| **Client-Server** | Star topology - all connect to soldier-1 | Simple centralized command |
| **Hub-Spoke** | Hierarchical - team leaders relay | Military squad structure |
| **Dynamic Mesh** | Full P2P - all interconnected | Resilient tactical networks |

**Key Questions:**
1. How does topology affect CAP's overhead?
2. Which topology benefits most from differential filtering?
3. Does CAP overhead scale differently across topologies?

## Phase 2: Multi-Document Testing Requirements

### Document Diversity Scenario

**Objective:** Create realistic multi-document scenario where differential filtering demonstrates measurable benefit

**Minimum Viable Test:**
- **10 documents** with varied authorization levels
- **Role-based access patterns:**
  - Public: All nodes (2 documents)
  - Leadership: Squad leader + team leaders (3 documents)
  - Team-specific: Team A only (2 documents)
  - Team-specific: Team B only (2 documents)
  - Command: Squad leader only (1 document)

**Expected Results:**
```
CAP Full Replication:
  • All 12 nodes receive all 10 documents = 120 document transfers

CAP Differential Filtering:
  • Node-specific transfers based on authorized_roles
  • Estimated: ~60-70 document transfers (50-60% reduction)

Ditto Baseline:
  • All 12 nodes receive all 10 documents = 120 document transfers
  • (No role-based filtering available)
```

### Document Size Variation

**Current:** Single small test document (<1KB)

**Recommendation:**
- Small documents: 1-5KB (tactical messages)
- Medium documents: 10-50KB (situation reports)
- Large documents: 100KB-1MB (sensor data, imagery metadata)

**Rationale:** Bandwidth impact measurable only with realistic payload sizes

### Update Frequency Testing

**Current:** Static test (insert document, wait for convergence)

**Future:** Dynamic updates
- Document creation
- Document modification (CRDT merge testing)
- High-frequency updates (battlefield telemetry simulation)

**Rationale:** CRDT overhead varies with update patterns

## Baseline Test Protocol

### Standard Test Configuration

**Fixed Parameters:**
- Topology: All 3 modes tested
- Bandwidth: 4 levels (100Mbps, 10Mbps, 1Mbps, 256Kbps)
- Duration: 120 seconds per test
- Nodes: 12-node squad formation

**Variable Parameter:**
- Configuration: Ditto Baseline / CAP Full / CAP Differential

**Execution Order:**
1. Run Ditto Baseline across all topologies/bandwidths
2. Run CAP Full across all topologies/bandwidths
3. Run CAP Differential across all topologies/bandwidths
4. Generate comparative analysis

### Metrics Collection

**Required Metrics (already implemented):**
- Convergence time (ms)
- Per-document latency (mean, P90, P99)
- Bandwidth utilization (measured vs target)
- Document transfer counts

**Additional Metrics (Phase 2):**
- Data volume transferred (bytes per node)
- Number of documents received per node
- Authorization check overhead (if measurable)
- Memory usage (optional)

### Analysis Framework

**Comparison Dimensions:**

1. **Overhead Analysis:**
   ```
   CAP Overhead = (CAP Full - Ditto Baseline) / Ditto Baseline * 100%
   ```

2. **Optimization Benefit:**
   ```
   CAP Benefit = (CAP Full - CAP Differential) / CAP Full * 100%
   ```

3. **Net Performance:**
   ```
   Net Benefit = (Ditto Baseline - CAP Differential) / Ditto Baseline * 100%
   ```

4. **Topology Sensitivity:**
   ```
   Compare overhead/benefit across client-server, hub-spoke, mesh
   ```

## Implementation Roadmap

### Phase 2A: Traditional IoT Baseline Implementation (NEXT - HIGH PRIORITY)

**Priority:** CRITICAL
**Effort:** ~3-4 hours
**Dependencies:** None (protobuf migration complete)

**Objective:** Implement non-CRDT baseline for meaningful architectural comparison

**Tasks:**
1. Create `peat-protocol/examples/traditional_baseline.rs`
2. Implement periodic full-state transmission:
   - Configurable update frequency (1s, 5s, 10s)
   - Full message serialization (JSON or protobuf)
   - TCP client-server or hub-spoke topology
3. Message routing:
   - Client-server: All nodes send to soldier-1
   - Hub-spoke: Team members send to team leader → squad leader
   - NO mesh (n-squared problem)
4. Metrics collection:
   - Message size (full state payload)
   - Transmission frequency
   - Bandwidth utilization
   - Latency (send time → receive time)
5. Update Dockerfile to build `traditional_baseline` binary
6. Test with 2-node and 12-node topologies

**Deliverables:**
- `traditional_baseline.rs` example
- Compatible with existing test infrastructure
- Metrics output format identical to CAP tests

### Phase 2B: Multi-Document Test Harness

**Priority:** HIGH
**Effort:** ~2-3 hours
**Dependencies:** Phase 2A (baseline implementation)

**Tasks:**
1. Extend all three implementations (traditional_baseline, cap_sim_node, ditto_baseline):
   - Support multiple documents (10 documents)
   - Role-based authorization patterns
   - Document generator with varied access control
2. Update metrics collection for per-document tracking
3. Test multi-doc scenario with all three architectures

**Deliverables:**
- Multi-document test scenario
- Role-based authorization test data
- Per-document metrics collection

### Phase 2C: Three-Way Baseline Comparison (NEXT - HIGH PRIORITY)

**Priority:** HIGH
**Effort:** ~2 hours (testing runtime)
**Dependencies:** Phase 2A, 2B

**Tasks:**
1. Run Traditional IoT baseline across topologies/bandwidths
2. Run CAP Full across topologies/bandwidths (already have data)
3. Run CAP Differential across topologies/bandwidths (already have data)
4. Generate comparative analysis:
   - Bandwidth usage comparison
   - Latency comparison
   - CRDT overhead calculation
   - CAP filtering benefit calculation

**Deliverables:**
- Three-way comparison report
- Bandwidth savings quantification
- Architectural ROI analysis

### Phase 2D: Document Size Variation

**Priority:** MEDIUM
**Effort:** ~2-3 hours
**Dependencies:** Phase 2A, 2B

**Tasks:**
1. Add configurable document size to all implementations
2. Test 1KB, 10KB, 100KB, 1MB payloads
3. Analyze bandwidth impact vs document size for:
   - Traditional IoT (full messages scale linearly)
   - CRDT delta sync (deltas may be smaller)

### Phase 2E: Dynamic Update Testing (FUTURE)

**Priority:** MEDIUM
**Effort:** ~3-4 hours
**Dependencies:** Phase 2A, 2B

**Tasks:**
1. Implement continuous document updates
2. Compare:
   - Traditional: Every update = full state retransmission
   - CRDT: Incremental delta updates
3. Measure update propagation latency

### Phase 3: Production Scenario Modeling (FUTURE)

**Priority:** LOW
**Effort:** ~5-8 hours

**Tasks:**
1. Model realistic military tactical network scenario
2. Implement mission-specific document types
3. Test squad movement/network partition scenarios

## Success Criteria

### Minimum Viable Baseline (Phase 2A + 2B + 2C)

✅ **Achieved when:**
1. Traditional IoT baseline implemented and tested
2. Three-way comparison completed:
   - Traditional IoT Baseline (no CRDT, full messages)
   - CAP Full Replication (CRDT, Query::All)
   - CAP Differential (CRDT + capability filtering)
3. Bandwidth comparison quantified:
   - CRDT overhead vs Traditional
   - CAP filtering benefit vs CRDT Full
   - Net architectural advantage
4. Comparative analysis report generated

### Comprehensive Baseline (Phase 2D + 2E)

✅ **Achieved when:**
1. Multi-document scenarios tested (10+ documents, role-based authorization)
2. Document size variation tested (1KB to 1MB payloads)
3. Dynamic updates tested (continuous state changes)
4. All topology modes compared (client-server, hub-spoke)
5. Clear ROI demonstration for PEAT Protocol vs Traditional IoT

## Traditional IoT Baseline Implementation

### Architecture: Event-Driven Full State Messaging

**No CRDT - Simple periodic state transmission:**

```rust
// Traditional IoT baseline approach
struct TraditionalNode {
    node_id: String,
    state: NodeState,
    update_frequency: Duration, // e.g., 5 seconds
}

impl TraditionalNode {
    fn run(&self) {
        loop {
            // Serialize ENTIRE state to JSON/protobuf
            let full_message = serialize_full_state(&self.state);

            // Send to ALL connected peers (or hub)
            self.send_to_all_peers(full_message);

            // Wait for next transmission
            sleep(self.update_frequency);
        }
    }
}
```

**Key Characteristics:**
- **Full state messages:** Every transmission includes complete node state
- **No deltas:** Cannot send incremental changes
- **No convergence:** Receiving nodes overwrite with latest message
- **Topology-aware:** Must use hub-spoke or client-server to avoid n-squared
- **Configurable frequency:** Trade latency vs bandwidth (e.g., 1s, 5s, 10s)

**Bandwidth Challenge:**
- 12 nodes × full state message × transmission frequency
- Example: 1KB message × 12 nodes × (1/5s) = 2.4 KB/s = 19.2 Kbps
- With 10 documents × 1KB each = 120 KB/node → 240 Kbps minimum

**N-Squared Problem:**
In mesh topology, every node sends to every other node:
- 12 nodes × 11 destinations × 1KB × (1/5s) = 26.4 KB/s = 211 Kbps
- **Unsustainable** - requires hub-spoke or client-server topology

## Expected Outcomes

### Hypothesis: PEAT Protocol Value Proposition

**Scenario:** 12-node squad, 10 documents (varied authorization), 10Mbps bandwidth, 5-second update frequency

**Predicted Results:**

| Configuration | Data Model | Transmission Strategy | Bandwidth Used | Convergence Time |
|---------------|------------|----------------------|----------------|------------------|
| **Traditional IoT** | Full messages | Periodic (5s) full state | **~250 Kbps** | 5-10s (depends on frequency) |
| **CAP Full (CRDT)** | Delta CRDTs | Event-driven delta sync | **~100 Kbps** (60% reduction) | ~26s (initial sync) |
| **CAP Differential** | Delta CRDTs + filtering | Capability-filtered deltas | **~60 Kbps** (76% reduction vs Traditional) | ~15s (filtered) |

**Value Proposition:**

1. **CRDT Advantage (CAP Full vs Traditional):**
   - Delta sync: Only send changes, not full state
   - Event-driven: Transmit on change, not on timer
   - Bandwidth savings: ~60% vs periodic full messages

2. **CAP Filtering Advantage (CAP Differential vs CAP Full):**
   - Role-based filtering: Only relevant documents
   - Authorization at sync layer: No unauthorized data transmitted
   - Bandwidth savings: ~40% additional reduction

3. **Combined Advantage (CAP Differential vs Traditional):**
   - **76% bandwidth reduction** (250 Kbps → 60 Kbps)
   - **Automatic convergence** (CRDT guarantees eventual consistency)
   - **Security benefit** (no unauthorized data on wire)

### Risk: Null Result Scenarios

**If Traditional IoT performs better:**
- CRDT overhead exceeds delta sync benefit
- → Investigate: CRDT merge complexity, metadata overhead
- → Consider: Document size where delta sync becomes advantageous

**If CAP filtering overhead is high:**
- Authorization checks dominate performance
- → Profile: Query evaluation performance
- → Optimize: Lazy evaluation, query caching

**If Traditional IoT matches CAP at low frequencies:**
- Low-frequency periodic transmission (e.g., 30s) may be competitive
- → Counter: CAP provides lower latency (event-driven)
- → Counter: CRDT provides guaranteed convergence

## References

### Related Documentation

- `CAP-FULL-VS-DIFFERENTIAL-COMPARISON.md` - E8 Phase 1 results (single document)
- `FUTURE-TESTING-REQUIREMENTS.md` - Detailed Phase 2 requirements
- `E8-THREE-WAY-COMPARISON.md` - Methodology for three-way analysis
- `COMPREHENSIVE_SUMMARY.md` - Detailed Phase 1 test results

### Prior Art

**E8 Phase 1 Testing (PR #56):**
- Established test infrastructure
- Demonstrated consistency across topologies
- Identified limitation: single-document scenario
- Recommendation: Multi-document testing needed

**Protobuf Migration (PR #57, PR #58):**
- Schema separation enables flexible testing
- Protobuf types ready for multi-document scenarios
- Zero simulation code changes required

## Conclusion

**Baseline testing framework defined and ready for execution.**

The protobuf migration (Phase 5) has validated that the simulation infrastructure is stable and ready for comprehensive baseline comparisons. The critical next step is implementing a **Traditional IoT Baseline** (no CRDT, periodic full-state messaging) to enable meaningful architectural comparison.

### Key Insight

To properly evaluate PEAT Protocol's value proposition, we need to compare against **traditional IoT event-driven architectures**, not just CRDT variants. The three-way comparison will demonstrate:

1. **CRDT Advantage:** Delta-state sync vs periodic full messages
2. **CAP Filtering Advantage:** Capability-based filtering vs full replication
3. **Combined Advantage:** PEAT Protocol vs Traditional IoT baseline

### Immediate Next Steps

**Critical Path:**
1. **Phase 2A:** Implement `traditional_baseline.rs` (no CRDT, full messages, periodic transmission)
2. **Phase 2B:** Extend to multi-document scenarios (10 documents, role-based authorization)
3. **Phase 2C:** Run three-way comparison tests and generate ROI analysis

**Expected Timeline:** 5-7 hours total effort, establishes complete architectural baseline

**Recommendation: Proceed with Phase 2A Traditional IoT Baseline implementation immediately.**

---

**Document Version:** 2.0 (CORRECTED)
**Last Updated:** 2025-11-07
**Status:** READY FOR IMPLEMENTATION
**Critical Correction:** Baseline = Traditional IoT (no CRDT), not Ditto (which IS CRDT)
