# E8: Network Simulation Layer - Implementation Plan

**Epic Goal**: Create realistic network simulation with Shadow to establish baseline performance metrics and validate protocol behavior under realistic network constraints.

**Status**: Ready to Start
**Dependencies**: E5 (Hierarchical Operations) - Implemented
**Duration**: Week 7-9 (3 weeks)
**Priority**: P1

## Overview

Epic 8 implements a network simulation layer to:
1. **Establish baseline metrics** for current Ditto performance under realistic network conditions
2. **Validate protocol behavior** across varying network quality (9.6Kbps - 1Mbps, 100ms - 5s latency)
3. **Measure actual bandwidth usage** during cell formation and hierarchical operations
4. **Identify optimization opportunities** through data-driven analysis

### Reference Use Case: Army Company Structure

We'll simulate a standard Army company structure (112 nodes total):
- **1 Company HQ**
- **3 Platoons** (each with 1 HQ + 3 Squads)
- **9 Squads total** (each with 9 soldiers + 1 UGV + 2 UAVs = 12 nodes)

This provides:
- Hierarchical validation (Company → Platoon → Squad)
- Realistic scale (112 nodes exceeds ADR-001's 100+ target)
- Mixed capabilities (humans, ground robots, aerial drones)
- Tactical scenarios for testing

## Implementation Phases

### Phase E8.0: Shadow + Ditto POC (⚠️ GO/NO-GO Decision Point)

**Goal**: Validate that Ditto SDK works under Shadow's syscall interception

**Duration**: 1-2 days

**Why Critical**: Shadow is only viable if Ditto works correctly under its syscall interception. This is a gate for the entire E8 approach.

#### Tasks

1. **Install Shadow on Linux workstation**
   ```bash
   # Check if available via package manager
   sudo apt-cache search shadow | grep network

   # Or build from source
   git clone https://github.com/shadow/shadow.git
   cd shadow && cargo build --release
   ```

2. **Create minimal test binary: `ditto-sync-test`**
   - Location: `cap-protocol/examples/shadow_poc.rs`
   - Features:
     - Creates Ditto store
     - Inserts a test document
     - Waits for sync to peer
     - Verifies document received
     - Exits with success/failure code

3. **Create Shadow config: `poc-ditto-sync.yaml`**
   - Location: `cap-sim/scenarios/poc-ditto-sync.yaml`
   - Defines: 2 nodes, simple network, 30s runtime

4. **Run and validate**
   ```bash
   shadow cap-sim/scenarios/poc-ditto-sync.yaml
   ```

#### Success Criteria (GO)
- ✅ Both Ditto instances start successfully
- ✅ Documents sync between nodes
- ✅ No syscall errors in Shadow logs
- ✅ Deterministic (same seed = same result)

#### Failure Criteria (NO-GO)
- ❌ Ditto crashes or hangs under Shadow
- ❌ Syscall errors (unsupported functions)
- ❌ Sync doesn't work (documents don't propagate)
- ❌ Non-deterministic behavior

**If POC Fails**: Fall back to Linux namespaces + tc/netem approach (see ADR-008 Option 1)

#### Deliverables
- [ ] `cap-protocol/examples/shadow_poc.rs` - Minimal Ditto sync test binary
- [ ] `cap-sim/scenarios/poc-ditto-sync.yaml` - Shadow configuration
- [ ] `docs/E8_SHADOW_POC_RESULTS.md` - POC results and decision
- [ ] Decision: GO (proceed with Shadow) or NO-GO (use namespaces)

---

### Phase E8.1: Shadow Harness Implementation

**Goal**: Build infrastructure to run multi-node CAP protocol simulations under Shadow

**Duration**: 3-4 days

**Prerequisites**: E8.0 POC succeeded

#### Architecture

```
cap-sim (binary)
├── Generates Shadow YAML configs from scenario definitions
├── Invokes Shadow simulator
├── Parses Shadow output for metrics
└── Generates reports (JSON, text, CSV)

cap-sim-node (binary)
├── CAP protocol implementation
├── Ditto sync
├── Command-line interface for node configuration
└── Metrics output (stdout/file)
```

#### Tasks

1. **Create `cap-sim-node` binary**
   - Location: `cap-sim/src/bin/cap_sim_node.rs`
   - CLI arguments:
     ```bash
     cap-sim-node \
       --node-id soldier-1-1 \
       --role soldier \
       --capabilities sensor,comms \
       --cell-id squad-1
     ```
   - Integrates: NodeStore, CellStore, capability composition
   - Outputs: Metrics to stdout in structured format (JSON lines)

2. **Create `cap-sim` orchestrator binary**
   - Location: `cap-sim/src/bin/cap_sim.rs`
   - Commands:
     - `cap-sim generate <scenario>` - Generate Shadow YAML
     - `cap-sim run <scenario>` - Run simulation
     - `cap-sim report <results>` - Analyze results
   - Generates Shadow configs from scenario definitions

3. **Define scenario format**
   - Location: `cap-sim/scenarios/schema.json`
   - YAML schema for defining:
     - Node topology (company/platoon/squad structure)
     - Network constraints per echelon
     - Timeline events (joins, partitions, etc.)

4. **Implement Shadow YAML generator**
   - Location: `cap-sim/src/shadow_generator.rs`
   - Inputs: Scenario YAML
   - Outputs: Shadow configuration YAML
   - Handles:
     - Node process definitions
     - Network graph topology
     - Bandwidth/latency/loss parameters

5. **Create first scenario: Squad Formation (12 nodes)**
   - Location: `cap-sim/scenarios/squad-formation.yaml`
   - 9 soldiers + 1 UGV + 2 UAVs
   - Unconstrained network (baseline)
   - Duration: 60 seconds

#### Success Criteria
- ✅ `cap-sim-node` binary runs under Shadow
- ✅ 12 nodes successfully form a squad cell
- ✅ All nodes converge to same CellState
- ✅ Shadow metrics show bandwidth/latency
- ✅ Results are deterministic (reproducible with same seed)

#### Deliverables
- [ ] `cap-sim/src/bin/cap_sim_node.rs` - Node simulation binary
- [ ] `cap-sim/src/bin/cap_sim.rs` - Orchestrator binary
- [ ] `cap-sim/src/shadow_generator.rs` - Shadow config generator
- [ ] `cap-sim/scenarios/squad-formation.yaml` - First scenario
- [ ] `cap-sim/scenarios/schema.json` - Scenario schema documentation
- [ ] Squad formation demo working under Shadow

---

### Phase E8.2: Network Constraints Implementation

**Goal**: Apply realistic bandwidth/latency/loss constraints to simulate tactical networks

**Duration**: 2-3 days

**Prerequisites**: E8.1 complete

#### Network Profiles by Echelon

Based on ADR-008:

| Echelon | Link Type | Bandwidth | Latency | Loss | Jitter |
|---------|-----------|-----------|---------|------|--------|
| Intra-squad | Local mesh (WiFi/BT) | 100 Kbps | 100ms | 1% | 20ms |
| Squad→Platoon | JTRS tactical radio | 56 Kbps | 500ms | 5% | 100ms |
| Platoon→Company | SATCOM | 19.2 Kbps | 1000ms | 10% | 200ms |

#### Tasks

1. **Define network profiles**
   - Location: `cap-sim/src/network_profiles.rs`
   - Struct: `NetworkProfile { bandwidth_kbps, latency_ms, jitter_ms, loss_percent }`
   - Presets: `intra_squad()`, `squad_to_platoon()`, `platoon_to_company()`

2. **Extend Shadow generator to apply profiles**
   - Map scenario network topology to Shadow graph
   - Apply bandwidth/latency/loss per edge
   - Support hierarchical topology (different profiles per layer)

3. **Create constrained scenarios**
   - `cap-sim/scenarios/squad-formation-constrained.yaml` - 56Kbps, 500ms, 5% loss
   - `cap-sim/scenarios/platoon-formation.yaml` - 3 squads forming platoon (mixed constraints)

4. **Implement metrics collection**
   - Location: `cap-sim/src/metrics/mod.rs`
   - Collect from Shadow output:
     - Per-node bandwidth usage
     - Message count
     - Convergence time
     - Packet loss statistics
   - Generate comparison reports (baseline vs. constrained)

#### Success Criteria
- ✅ Convergence time increases predictably with latency
- ✅ Bandwidth limits are respected (measured vs expected)
- ✅ Protocol remains consistent under constraints
- ✅ Packet loss doesn't prevent eventual consistency

#### Deliverables
- [ ] `cap-sim/src/network_profiles.rs` - Network profile definitions
- [ ] `cap-sim/scenarios/squad-formation-constrained.yaml` - Constrained scenario
- [ ] `cap-sim/scenarios/platoon-formation.yaml` - Multi-cell scenario
- [ ] `cap-sim/src/metrics/mod.rs` - Metrics collection system
- [ ] Metrics showing protocol behavior under constraints

---

### Phase E8.3: Network Partition Scenarios

**Goal**: Validate CRDT consistency during network splits and healing

**Duration**: 2-3 days

**Prerequisites**: E8.2 complete

#### Scenarios

1. **Simple Partition**: Split 12-node squad into 2 groups (6+6)
2. **Platoon Isolation**: Isolate one platoon from company
3. **Split-Brain**: Company HQ loses contact with all platoons

#### Tasks

1. **Extend Shadow generator for dynamic topology**
   - Support timeline events:
     ```yaml
     timeline:
       - at: 30s
         action: partition
         groups: [[node1, node2], [node3, node4]]
       - at: 90s
         action: heal
     ```
   - Modify Shadow network graph at specified times

2. **Create partition scenario: Lost Comms**
   - Location: `cap-sim/scenarios/partition-platoon-isolated.yaml`
   - Setup: Full company (112 nodes)
   - Event: Platoon 2 isolated for 5 minutes
   - Changes during partition:
     - Platoon 2: Squad 4 UGV goes offline (capability loss)
     - Company HQ: Reassign mission to Platoon 1
   - Validate: Both changes preserved after heal

3. **Implement partition validation tests**
   - Location: `cap-protocol/tests/partition_validation_e2e.rs`
   - Verify:
     - Changes preserved during partition
     - States merge correctly after healing
     - No data loss or conflicts
     - Convergence time after heal

#### Success Criteria
- ✅ Partition detected within 10s (heartbeat timeout)
- ✅ Changes preserved during partition (CRDT property)
- ✅ Convergence after heal: <30s
- ✅ Conflict resolution: Automatic (CRDT merge)
- ✅ No data loss: Both changes visible after heal

#### Deliverables
- [ ] Timeline event support in Shadow generator
- [ ] `cap-sim/scenarios/partition-platoon-isolated.yaml` - Partition scenario
- [ ] `cap-protocol/tests/partition_validation_e2e.rs` - Validation tests
- [ ] Partition demonstration video/logs

---

### Phase E8.4: Baseline Comparison & Analysis

**Goal**: Generate comprehensive baseline report comparing current performance against targets

**Duration**: 2-3 days

**Prerequisites**: E8.1, E8.2, E8.3 complete

#### Scenario Matrix

Run full scenario suite with varying scales:

| Scenario | Nodes | Network | Purpose |
|----------|-------|---------|---------|
| Squad Formation | 12 | Unconstrained | Baseline best-case |
| Squad Constrained | 12 | 56Kbps, 500ms, 5% loss | Tactical network |
| Platoon Formation | 39 | Mixed (100/56/19.2 Kbps) | Hierarchical validation |
| Company Formation | 112 | Mixed constraints | Full-scale test |
| Partition Recovery | 112 | Constrained + partition | Resilience test |
| Bandwidth Saturation | 39 | 19.2Kbps SATCOM | Stress test |

#### Metrics to Capture

**Performance Metrics**:
- Convergence time (time to consistent state)
- Bandwidth usage (total bytes transmitted)
- Message count vs node count (validate O(n log n))
- Staleness (time until capability updates visible)

**Reliability Metrics**:
- Partition tolerance (consistency during/after partitions)
- Error rate (sync failures under constraints)
- Recovery time (convergence after partition heal)

**Comparison Metrics**:
- Baseline vs E7 Delta projections
- Actual vs ADR-001 targets
- Identify gaps requiring optimization

#### Tasks

1. **Implement scenario runner**
   - Location: `cap-sim/src/runner.rs`
   - Run multiple scenarios sequentially
   - Aggregate results across runs
   - Generate summary statistics (mean, p50, p95, p99)

2. **Create baseline report generator**
   - Location: `cap-sim/src/report.rs`
   - Inputs: Simulation results (JSON)
   - Outputs:
     - Text summary for humans
     - JSON for programmatic analysis
     - CSV for spreadsheet import
   - Compare against:
     - E7 baseline bandwidth measurements
     - ADR-001 performance targets

3. **Run full scenario matrix**
   - Execute all 6+ scenarios
   - Multiple runs per scenario (validate determinism)
   - Collect and aggregate metrics

4. **Generate E8 baseline report**
   - Location: `docs/E8_BASELINE_REPORT.md`
   - Sections:
     - Executive summary
     - Methodology
     - Results per scenario
     - Comparison to targets
     - Identified optimization opportunities
     - Recommendations for future work

#### Success Criteria
- ✅ Baseline report shows actual bandwidth usage vs delta potential
- ✅ Clear data-driven recommendations for optimization
- ✅ Metrics validate or challenge ADR-001 targets
- ✅ Report enables informed decision-making for E9+

#### Deliverables
- [ ] `cap-sim/src/runner.rs` - Scenario batch runner
- [ ] `cap-sim/src/report.rs` - Report generator
- [ ] `docs/E8_BASELINE_REPORT.md` - Comprehensive baseline analysis
- [ ] Raw metrics data (JSON/CSV in `cap-sim/results/`)
- [ ] Comparison charts/visualizations

---

## Integration Points

### With Existing E2E Harness

The existing `E2EHarness` (cap-protocol/src/testing/e2e_harness.rs) will be:
- **Reused** for: Ditto store creation patterns, observer-based sync validation
- **Extended** for: Network constraint simulation under Shadow
- **Referenced** for: Test isolation patterns (unique persistence dirs)

### With E7 Baseline Tests

E7's `baseline_ditto_bandwidth_e2e.rs` provides:
- Document size measurements
- Sync frequency baselines
- Bandwidth calculation methodology

E8 will:
- Use same measurement methodology for consistency
- Compare simulated network results against E7 baselines
- Validate that E7 delta optimizations work under constrained networks

### With E9 Reference Application

E8 deliverables will feed into E9:
- Simulation scenarios become demo scenarios
- Metrics collection becomes real-time visualization
- Shadow configs inform reference app network simulation mode

---

## Success Criteria Summary

Epic 8 is complete when:

1. ✅ **E8.0 POC**: Ditto works under Shadow (or fallback decision made)
2. ✅ **E8.1 Harness**: Simulation runs 12+ nodes with real Ditto sync
3. ✅ **E8.2 Constraints**: Network bandwidth/latency are applied and measured
4. ✅ **E8.3 Partitions**: Partition scenario validates CRDT consistency
5. ✅ **E8.4 Baseline**: Comprehensive report compares current vs target performance
6. ✅ **Determinism**: All scenarios reproducible with same seed
7. ✅ **Metrics**: Data enables comparison across scenarios and time

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Shadow doesn't support Ditto | High | E8.0 POC validates early; fallback to namespaces |
| Simulation doesn't match reality | Medium | Focus on relative comparisons; validate with real deployment data later |
| 112 nodes overwhelm resources | Medium | Profile early, scale up gradually, use workstation with 16GB+ RAM |
| Timeline slips | Medium | E8.0 is critical path; can defer E8.4 report polish if needed |

---

## Resources Required

### Hardware
- Linux workstation (Ubuntu 22.04+)
- 16GB+ RAM (for 112 nodes)
- 8+ CPU cores
- No root access required (Shadow advantage)

### Software
- Shadow network simulator (Rust)
- Ditto SDK
- Existing CAP protocol codebase

### Time Allocation
- E8.0: 1-2 days (20%)
- E8.1: 3-4 days (35%)
- E8.2: 2-3 days (25%)
- E8.3: 2-3 days (20%)
- E8.4: 2-3 days (buffer)
- **Total: ~12-15 days (3 weeks)**

---

## Next Steps

### Immediate Actions (Week 1)
1. Review this plan with team
2. Set up Linux workstation with Shadow
3. Start E8.0 POC implementation
4. Create GitHub sub-issues for each phase

### Week 1 Deliverables
- E8.0 POC complete
- GO/NO-GO decision on Shadow approach
- E8.1 implementation started (if GO)

---

**Document Version**: 1.0
**Last Updated**: 2025-11-05
**Next Review**: After E8.0 POC completion
