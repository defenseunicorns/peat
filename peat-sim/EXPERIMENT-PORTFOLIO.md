# CAP Experiment Portfolio - Scaling Roadmap

**Version:** 1.0
**Date:** 2025-11-08
**Status:** Planning Phase

---

## Executive Summary

This document outlines the comprehensive experiment portfolio for validating CAP (Capability-Aware Protocol) performance from **12 nodes to 192+ nodes**, establishing a systematic scaling roadmap from squad-level operations through battalion-scale deployments.

**Current State:** Squad-level (12 nodes) validated across 32 test scenarios
**Target State:** Battalion-level (192+ nodes) with proven scalability characteristics
**Critical Gap:** n-squared scaling behavior unknown beyond 12 nodes

---

## 1. Current Baseline (Complete ✅)

### Phase 1-2: Proof of Concept and Squad Operations

**Validated Scales:**
- **2 nodes** - POC bidirectional sync
- **12 nodes** - Squad formation (soldiers, UAVs, UGVs)

**Architectures Tested:**
1. Traditional IoT Baseline (NO CRDT, periodic full-state)
2. CAP Full Replication (CRDT delta-state, Query::All)
3. CAP Differential Filtering (CRDT + capability filtering)

**Test Coverage:**
- 32 test scenarios (8 Traditional + 12 CAP Full + 12 CAP Differential)
- 4 bandwidth levels: 100Mbps, 10Mbps, 1Mbps, 256Kbps
- 3 topologies: Client-Server, Hub-Spoke, Dynamic Mesh
- **Results:** 26-second convergence, consistent across bandwidth constraints

**Known Performance:**
- **12 nodes** = 66 potential connections (n×(n-1)/2)
- Convergence: ~26 seconds
- Bandwidth: CAP Differential achieves target 60-70% reduction vs Traditional
- Reliability: 100% success rate, no test failures

---

## 2. Scaling Challenge

### The n-Squared Problem

| Scale | Node Count | Potential Connections | Multiplier vs Squad |
|-------|-----------|----------------------|---------------------|
| **Squad (Current)** | 12 | 66 | 1× (baseline) |
| **Platoon** | 24 | 276 | **4.2×** |
| **Half-Company** | 48 | 1,128 | **17.1×** |
| **Company** | 96 | 4,560 | **69.1×** |
| **Battalion** | 192 | 18,336 | **277.8×** |

### Critical Questions

1. **Performance Threshold** - At what scale does convergence time explode?
2. **Architecture Comparison** - When does Traditional IoT completely fail?
3. **Filtering Effectiveness** - Does CAP Differential maintain bandwidth efficiency at 50+ nodes?
4. **Practical Limits** - What's the maximum viable node count per architecture?
5. **Hierarchical Benefits** - Does vertical aggregation improve scalability?

---

## 3. Phase 3A: Platoon Operations (24 nodes)

### Overview

**Scale:** 2× current (12 → 24 nodes)
**Topology:** **Multi-squad hierarchy** (FIRST TIME)
**Estimated Duration:** ~45-60 minutes per test suite

### Hierarchical Structure

```
Platoon Leader (1)
├── Squad Alpha Leader (1)
│   ├── Alpha Soldiers (5)
│   ├── Alpha UAVs (1)
│   └── Alpha UGV (1)
├── Squad Bravo Leader (1)
│   ├── Bravo Soldiers (5)
│   ├── Bravo UAVs (1)
│   └── Bravo UGV (1)
└── Squad Charlie Leader (1)
    ├── Charlie Soldiers (5)
    ├── Charlie UAVs (1)
    └── Charlie UGV (1)

Total: 24 nodes (1 platoon + 3 squads × 8 nodes)
```

### New Capabilities Tested

1. **Multi-Squad Coordination** - 3 squads operating concurrently
2. **Vertical Aggregation** - Platoon Leader aggregates data from 3 Squad Leaders
3. **Inter-Squad Communication** - Cross-squad data sharing via platoon level
4. **Hierarchical Filtering** - Capability filtering across 2-tier hierarchy

### Test Matrix

**Topologies (4 total):**
1. **Client-Server** - All nodes → Platoon Leader (central aggregation)
2. **Hub-Spoke** - Squads → Squad Leaders → Platoon Leader (hierarchical)
3. **Dynamic Mesh** - Full peer-to-peer with hierarchical roles
4. **Hybrid Platoon** ⭐ NEW - Intra-squad mesh, inter-squad via leaders

**Architectures:**
- Traditional IoT Baseline (Client-Server, Hub-Spoke only)
- CAP Full Replication (all 4 topologies)
- CAP Differential Filtering (all 4 topologies)

**Bandwidth Levels:**
- 100Mbps, 10Mbps, 1Mbps, 256Kbps (same as Phase 1-2)

**Total Test Scenarios:**
- Traditional: 2 topologies × 4 bandwidths = 8 tests
- CAP Full: 4 topologies × 4 bandwidths = 16 tests
- CAP Differential: 4 topologies × 4 bandwidths = 16 tests
- **Total: 40 test scenarios**

### Success Criteria

**Performance Targets:**
- ✅ **Convergence time < 60 seconds** (2.3× baseline for 2× nodes)
- ✅ **Zero test failures** (100% reliability)
- ✅ **Bandwidth reduction maintained** (60-70% vs Traditional)
- ✅ **Hierarchical aggregation validated** (Platoon Leader sees all squads)

**Validation Gates:**
- All 24 nodes successfully deploy
- Inter-squad data propagates through hierarchy
- Capability filtering works across platoon boundary
- No deadlocks or split-brain scenarios

### Resource Requirements

**Infrastructure:**
- Docker containers: 24 concurrent
- Network interfaces: ~96 (4 per container)
- RAM: ~6GB (250MB per container)
- Disk: ~200MB for logs/metrics
- Test duration: ~45-60 minutes

**Topologies to Create:**
```
topologies/platoon-24node-client-server.yaml
topologies/platoon-24node-hub-spoke.yaml
topologies/platoon-24node-dynamic-mesh.yaml
topologies/platoon-24node-hybrid.yaml
```

### Implementation Steps

1. **Generate Topology Files** - Create 4 new 24-node YAML topologies
2. **Update Test Harness** - Extend test scripts for 24-node scenarios
3. **Validation Run** - Single topology smoke test
4. **Full Test Suite** - All 40 scenarios
5. **Analysis** - Compare against 12-node baseline
6. **Documentation** - Update ADR-008 with findings

---

## 4. Phase 3B: Half-Company Operations (48 nodes)

### Overview

**Scale:** 4× current (12 → 48 nodes)
**Topology:** 2 platoons or 6 squads
**Estimated Duration:** ~90-120 minutes per test suite

### Hierarchical Structure (Option A: Dual Platoon)

```
Company Leader (1)
├── Platoon 1 Leader (1)
│   ├── Squad Alpha (8 nodes)
│   ├── Squad Bravo (8 nodes)
│   └── Squad Charlie (8 nodes)
└── Platoon 2 Leader (1)
    ├── Squad Delta (8 nodes)
    ├── Squad Echo (8 nodes)
    └── Squad Foxtrot (8 nodes)

Total: 48 nodes (1 company + 2 platoons × 3 squads × 8 nodes)
```

### New Capabilities Tested

1. **3-Tier Hierarchy** - Company → Platoon → Squad → Individual
2. **Multi-Platoon Coordination** - 2 platoons operating concurrently
3. **Deeper Aggregation** - Company Leader aggregates from 2 Platoon Leaders
4. **Cross-Platoon Communication** - Inter-platoon data sharing

### Test Matrix

**Topologies (4 total):**
1. Client-Server (all → Company Leader)
2. Hub-Spoke (hierarchical 3-tier)
3. Dynamic Mesh (full peer-to-peer)
4. Hybrid Company (intra-squad mesh, platoon/company aggregation)

**Total Test Scenarios:**
- Traditional: 2 topologies × 4 bandwidths = 8 tests
- CAP Full: 4 topologies × 4 bandwidths = 16 tests
- CAP Differential: 4 topologies × 4 bandwidths = 16 tests
- **Total: 40 test scenarios**

### Success Criteria

**Performance Targets:**
- ✅ **Convergence time < 120 seconds** (estimate)
- ✅ **Bandwidth reduction > 70%** (filtering more critical at scale)
- ✅ **3-tier aggregation validated**
- ✅ **Traditional IoT may start failing** (expected)

**Critical Validation:**
- Does CAP Differential maintain sub-linear scaling?
- At what point does Traditional IoT become impractical?
- Does hub-spoke outperform mesh at this scale?

### Resource Requirements

**Infrastructure:**
- Docker containers: 48 concurrent
- RAM: ~12GB
- Disk: ~400MB for logs/metrics
- Test duration: ~90-120 minutes

---

## 5. Phase 3C: Company Operations (96 nodes)

### Overview

**Scale:** 8× current (12 → 96 nodes)
**Topology:** 4 platoons or 12 squads
**Estimated Duration:** ~3-4 hours per test suite

### Hierarchical Structure

```
Company Leader (1)
├── Platoon 1-4 Leaders (4)
│   └── Each Platoon: 3 squads × ~8 nodes

Total: 96 nodes
```

### New Capabilities Tested

1. **Large-Scale CRDT Performance** - 4,560 potential connections
2. **Bandwidth Efficiency at Scale** - Filtering effectiveness critical
3. **Traditional IoT Failure Point** - Likely complete breakdown
4. **Resource Limits** - Docker/system scalability

### Test Matrix

**Reduced Scope** (focus on viable architectures):
- Traditional IoT: **SKIP** (expected to fail)
- CAP Full: 4 topologies × 2 bandwidths (100Mbps, 10Mbps) = 8 tests
- CAP Differential: 4 topologies × 4 bandwidths = 16 tests
- **Total: 24 test scenarios**

### Success Criteria

**Performance Targets:**
- ✅ **Convergence time < 300 seconds** (5 minutes)
- ✅ **CAP Differential viable** (60-70% bandwidth reduction maintained)
- ✅ **CAP Full may struggle** (n-squared overhead)
- ⚠️ **Traditional IoT expected failure** (document breakdown mode)

**Critical Validation:**
- Maximum viable node count for CAP Full Replication?
- Does CAP Differential scale sub-linearly?
- Resource consumption per architecture

### Resource Requirements

**Infrastructure:**
- Docker containers: 96 concurrent ⚠️ **May need multi-host setup**
- RAM: ~24GB
- Disk: ~800MB for logs/metrics
- Test duration: ~3-4 hours

**Infrastructure Decision Point:**
- Single-host limit likely reached
- May need Kubernetes or multi-host ContainerLab
- Consider cloud deployment (AWS, GCP)

---

## 6. Phase 3D: Battalion Operations (192+ nodes)

### Overview

**Scale:** 16× current (12 → 192 nodes)
**Topology:** 8 platoons, 24 squads, or 2 companies
**Status:** Stretch goal, infrastructure TBD

### Hierarchical Structure

```
Battalion Leader (1)
├── Company 1 Leader (1)
│   └── 4 platoons × 3 squads × ~8 nodes
└── Company 2 Leader (1)
    └── 4 platoons × 3 squads × ~8 nodes

Total: 192+ nodes (18,336 potential connections)
```

### Critical Questions

1. **Infrastructure Viability** - Can we deploy 192 Docker containers?
2. **Test Duration** - 6+ hours realistic?
3. **CAP Differential Limit** - Does it still work at this scale?
4. **Practical Value** - Is this scale representative of real deployments?

### Success Criteria

**Performance Targets:**
- ✅ **Convergence time < 600 seconds** (10 minutes)
- ✅ **CAP Differential only viable architecture** (expected)
- ⚠️ **Multi-host deployment required**

**Go/No-Go Decision:**
- If Phase 3C shows CAP Differential working well, proceed
- If Phase 3C shows convergence time explosion, STOP
- Document maximum viable scale

### Resource Requirements

**Infrastructure:**
- Multi-host Kubernetes cluster OR
- Cloud-based ContainerLab deployment
- RAM: ~48GB across hosts
- Disk: ~1.6GB for logs/metrics
- Test duration: ~6-8 hours

---

## 7. Test Execution Strategy

### Incremental Validation Approach

**Phase Gate Structure:**

```
Phase 3A (24 nodes) - GO/NO-GO GATE 1
├─ Success? → Proceed to Phase 3B
└─ Failure? → Debug, optimize, retry

Phase 3B (48 nodes) - GO/NO-GO GATE 2
├─ Success? → Proceed to Phase 3C
└─ Failure? → Document scaling limit, END

Phase 3C (96 nodes) - GO/NO-GO GATE 3
├─ Success? → Consider Phase 3D
└─ Failure? → Document scaling limit, END

Phase 3D (192+ nodes) - STRETCH GOAL
└─ Proceed only if 3C shows sub-linear scaling
```

### Success Metrics Per Phase

| Phase | Convergence Target | Bandwidth Target | Reliability Target |
|-------|-------------------|------------------|-------------------|
| 3A (24 nodes) | < 60s | 60-70% reduction | 100% success |
| 3B (48 nodes) | < 120s | > 70% reduction | 95%+ success |
| 3C (96 nodes) | < 300s | > 70% reduction | 90%+ success |
| 3D (192+ nodes) | < 600s | > 70% reduction | 80%+ success |

### Failure Modes to Document

1. **Traditional IoT Breakdown** - At what scale does it fail completely?
2. **CAP Full n-Squared Limit** - When does convergence explode?
3. **Infrastructure Limits** - Docker/system resource exhaustion
4. **Network Congestion** - Bandwidth saturation scenarios

---

## 8. Infrastructure Requirements Summary

### Phase 3A (24 nodes) - Single Host ✅

- **Deployment:** ContainerLab, single host
- **RAM:** 6GB
- **CPU:** 8 cores recommended
- **Duration:** ~60 minutes
- **Risk:** LOW

### Phase 3B (48 nodes) - Single Host ⚠️

- **Deployment:** ContainerLab, single host (pushing limits)
- **RAM:** 12GB
- **CPU:** 16 cores recommended
- **Duration:** ~120 minutes
- **Risk:** MEDIUM (may need optimization)

### Phase 3C (96 nodes) - Multi-Host Required 🔴

- **Deployment:** Multi-host ContainerLab OR Kubernetes
- **RAM:** 24GB across hosts
- **CPU:** 32+ cores across hosts
- **Duration:** ~4 hours
- **Risk:** HIGH (infrastructure complexity)

### Phase 3D (192+ nodes) - Cloud Deployment 🔴

- **Deployment:** Cloud-based (AWS EKS, GCP GKE)
- **RAM:** 48GB+ across cluster
- **CPU:** 64+ cores across cluster
- **Duration:** ~8 hours
- **Risk:** VERY HIGH (cost, complexity)

---

## 9. Topology Design Patterns

### Naming Convention

```
topologies/[formation]-[nodecount]node-[pattern].yaml

Examples:
- platoon-24node-client-server.yaml
- platoon-24node-hub-spoke.yaml
- platoon-24node-hybrid.yaml
- company-96node-hierarchical.yaml
```

### Topology Templates

**Client-Server Pattern:**
```yaml
# All nodes connect to single leader
topology:
  nodes:
    leader:
      kind: linux
      image: peat-sim-node:latest
    node-1..N:
      kind: linux
      image: peat-sim-node:latest
  links:
    - endpoints: ["leader:eth1", "node-1:eth1"]
    - endpoints: ["leader:eth2", "node-2:eth1"]
    # ... all nodes → leader
```

**Hub-Spoke (Hierarchical) Pattern:**
```yaml
# Squads → Squad Leaders → Platoon Leader
topology:
  nodes:
    platoon-leader: [...]
    squad-alpha-leader: [...]
    squad-bravo-leader: [...]
    squad-charlie-leader: [...]
    # Individual soldiers/UAVs/UGVs per squad
  links:
    # Platoon level
    - endpoints: ["platoon-leader:eth1", "squad-alpha-leader:eth1"]
    - endpoints: ["platoon-leader:eth2", "squad-bravo-leader:eth1"]
    - endpoints: ["platoon-leader:eth3", "squad-charlie-leader:eth1"]
    # Squad level (each squad leader → squad members)
    # ...
```

**Hybrid Pattern (NEW for Phase 3A):**
```yaml
# Intra-squad: Full mesh
# Inter-squad: Via squad leaders to platoon leader
# Combines mesh resilience with hierarchical efficiency
```

---

## 10. Metrics and Analysis

### Key Performance Indicators (KPIs)

**Per Test Scenario:**
1. **Convergence Time** - First insert → all nodes synced
2. **Mean Latency** - Average per-update propagation time
3. **P90/P99 Latency** - Tail latency distribution
4. **Bandwidth Consumed** - Total bytes transmitted
5. **Message Count** - Total messages sent (Traditional) or CRDT operations (CAP)
6. **Success Rate** - Percentage of nodes achieving sync

**Cross-Phase Comparison:**
1. **Scaling Factor** - Convergence time growth rate
2. **Bandwidth Efficiency** - % reduction vs Traditional at each scale
3. **Architecture Viability** - At what scale does each architecture fail?

### Analysis Tools

**Existing:**
- `analyze_metrics.py` - Extracts latency, convergence from logs
- `generate-executive-summary.sh` - Auto-generates comparison tables

**To Create:**
- `analyze-scaling-trends.py` - Plot convergence vs node count
- `compare-architectures.py` - Side-by-side architecture comparison
- `generate-scaling-report.sh` - Multi-phase comprehensive report

---

## 11. Risk Assessment

### Phase 3A (24 nodes) - LOW RISK ✅

**Risks:**
- Container deployment issues (low likelihood)
- New hybrid topology bugs (medium likelihood)

**Mitigation:**
- Start with single topology validation
- Leverage existing test infrastructure
- 2× scale is conservative jump

### Phase 3B (48 nodes) - MEDIUM RISK ⚠️

**Risks:**
- Resource exhaustion on single host
- Convergence time may exceed acceptable limits
- Traditional IoT expected failure

**Mitigation:**
- Monitor system resources during tests
- Reduce test scope if needed (fewer bandwidths)
- Document failure modes

### Phase 3C (96 nodes) - HIGH RISK 🔴

**Risks:**
- Multi-host deployment complexity
- Infrastructure costs (if cloud-based)
- Test duration too long (4+ hours)
- CAP Full may fail at this scale

**Mitigation:**
- Evaluate infrastructure options before committing
- Consider reduced test matrix
- May need to optimize CAP implementation

### Phase 3D (192+ nodes) - VERY HIGH RISK 🔴

**Risks:**
- Infrastructure availability/cost
- Test duration impractical (8+ hours)
- May exceed CAP Differential limits

**Mitigation:**
- Go/No-Go decision based on Phase 3C results
- Consider simulation vs actual deployment
- May be research/stretch goal only

---

## 12. Implementation Roadmap

### Timeline Estimates

| Phase | Duration | Effort | Dependencies |
|-------|----------|--------|--------------|
| **3A Planning** | 2 hours | Create topologies, update scripts | None (ready now) |
| **3A Execution** | 1 hour | Run 40 test scenarios | Planning complete |
| **3A Analysis** | 1 hour | Generate reports, analyze | Execution complete |
| **3A Gate Review** | 30 min | GO/NO-GO decision | Analysis complete |
| **3B Planning** | 3 hours | Design 3-tier hierarchy | 3A success |
| **3B Execution** | 2 hours | Run 40 test scenarios | Planning complete |
| **3B Analysis** | 1 hour | Compare vs 3A | Execution complete |
| **3B Gate Review** | 30 min | GO/NO-GO decision | Analysis complete |
| **3C Planning** | 4 hours | Multi-host setup | 3B success |
| **3C Execution** | 4 hours | Run 24 test scenarios | Infrastructure ready |
| **3C Analysis** | 2 hours | Scaling trend analysis | Execution complete |
| **3C Gate Review** | 1 hour | GO/NO-GO for 3D | Analysis complete |
| **3D** | TBD | Cloud deployment (if viable) | 3C shows sub-linear scaling |

**Total Estimated Time (3A-3C):** 16-20 hours over 2-3 weeks

### Immediate Next Steps (Phase 3A)

1. **Create Platoon Topologies** (~1 hour)
   - `platoon-24node-client-server.yaml`
   - `platoon-24node-hub-spoke.yaml`
   - `platoon-24node-dynamic-mesh.yaml`
   - `platoon-24node-hybrid.yaml`

2. **Update Test Scripts** (~30 min)
   - Add 24-node test mode to `test-bandwidth-constraints.sh`
   - Create `run-e8-phase3a-platoon.sh` orchestrator

3. **Validation Run** (~15 min)
   - Single topology smoke test (client-server, 100Mbps)

4. **Full Test Suite** (~60 min)
   - All 40 scenarios (3 architectures × 4 topologies × varies)

5. **Analysis and Report** (~1 hour)
   - Generate scaling comparison
   - Update ADR-008
   - GO/NO-GO decision for Phase 3B

---

## 13. Documentation Updates

### ADR-008 Updates Required

After each phase completion:

**Section to Add: "Scaling Validation Results"**
```markdown
## Scaling Validation Results

### Phase 3A: Platoon Operations (24 nodes)
- Convergence time: [MEASURED]ms (target: <60s)
- Bandwidth efficiency: [MEASURED]% reduction
- Architecture viability: [Assessment]
- Hierarchical aggregation: [Validated/Issues]

### Phase 3B: Half-Company Operations (48 nodes)
- [Results TBD]

### Phase 3C: Company Operations (96 nodes)
- [Results TBD]
```

### Other Documentation

- **README.md** - Update with Phase 3 status
- **QUICK-START.md** - Add 24-node test instructions
- **E8-OPTIMIZATION-PLAN.md** - Mark completed phases

---

## 14. Success Criteria Summary

### Phase 3A (24 nodes) - Must Achieve

- ✅ All 24 nodes deploy successfully
- ✅ Convergence time < 60 seconds
- ✅ Bandwidth reduction 60-70% maintained
- ✅ Zero test failures
- ✅ Hierarchical aggregation working (Platoon Leader sees all squads)

### Phase 3B (48 nodes) - Must Achieve

- ✅ 3-tier hierarchy validated (Company → Platoon → Squad)
- ✅ Convergence time < 120 seconds
- ✅ CAP Differential still viable
- ⚠️ Traditional IoT failure documented

### Phase 3C (96 nodes) - Must Achieve

- ✅ Convergence time < 300 seconds
- ✅ CAP Differential shows sub-linear scaling
- 📊 CAP Full scaling limits documented
- 📊 Traditional IoT breakdown mode documented

### Phase 3D (192+ nodes) - Stretch Goal

- ✅ Infrastructure deployment successful
- ✅ Convergence time < 600 seconds
- 📊 Maximum viable scale documented

---

## 15. Open Questions

1. **Multi-Host Deployment** - What infrastructure for Phase 3C?
   - Multi-host ContainerLab (if supported)
   - Kubernetes (EKS, GKE, local k3s)
   - Cloud VMs with Docker Swarm

2. **Cost-Benefit Analysis** - Is Phase 3D worth the effort?
   - What real-world scenarios need 192+ nodes?
   - Can we simulate instead of deploying?

3. **CAP Implementation Optimizations** - Do we need code changes?
   - Current implementation optimized for <100 nodes?
   - Should we add connection pooling, batching, etc.?

4. **Test Duration Management** - How to handle 4-8 hour test runs?
   - Run overnight/weekend?
   - Parallelize across multiple hosts?
   - Reduce test matrix further?

5. **Hierarchical Filtering** - How should capability filtering work?
   - Should Platoon Leader filter based on platoon-wide needs?
   - Or just aggregate unfiltered from Squad Leaders?

---

## Appendix A: Node Count Breakdown by Formation

### Squad (12 nodes) ✅ VALIDATED
```
1 Squad Leader
5 Soldiers (infantry with weapons, comms)
1 UAV (aerial reconnaissance)
1 UGV (ground support)
4 Support roles (medic, engineer, etc.)
```

### Platoon (24 nodes) - Phase 3A Target
```
1 Platoon Leader
3 Squad Leaders
21 Squad members (3 squads × 7 members each)
```

### Company (96 nodes) - Phase 3C Target
```
1 Company Leader
4 Platoon Leaders
12 Squad Leaders (4 platoons × 3 squads)
79 Individual soldiers/UAVs/UGVs
```

### Battalion (192+ nodes) - Phase 3D Stretch
```
1 Battalion Leader
2 Company Leaders
8 Platoon Leaders (2 companies × 4 platoons)
24 Squad Leaders (8 platoons × 3 squads)
~157 Individual members
```

---

## Appendix B: Capability Filtering Hierarchy

### How Filtering Works at Scale

**Squad Level (12 nodes):**
- Each node publishes capabilities: `["weapons", "comms", "sensors"]`
- Each node subscribes based on needs: Query by capability
- Example: UAV queries `capability == "ground-threats"`

**Platoon Level (24 nodes) - NEW:**
- Squad Leaders aggregate squad capabilities
- Platoon Leader subscribes to squad-level summaries
- Inter-squad queries filtered by capability

**Company Level (96 nodes):**
- Platoon Leaders aggregate platoon capabilities
- Company Leader sees high-level operational picture
- Fine-grained data stays at lower levels

**Key Principle:** Only replicate data needed at each hierarchical level

---

## Appendix C: Comparison with Military MANET Research

### Related Work

**Typical MANET Studies:**
- 10-50 nodes (most common)
- 100+ nodes (rare, usually simulation)
- 1000+ nodes (very rare, theoretical)

**CAP Advantage:**
- Real deployment (Docker-based, not pure simulation)
- CRDT-based (automatic conflict resolution)
- Capability filtering (novel approach)

**Our Contribution:**
- Validate 12 → 24 → 48 → 96 → 192 node scaling
- Compare Traditional vs CAP architectures
- Document real-world convergence characteristics
- Prove bandwidth efficiency at scale

---

**End of Document**

---

**Version History:**
- **v1.0** (2025-11-08) - Initial comprehensive roadmap
- Future updates will track phase completion and results
