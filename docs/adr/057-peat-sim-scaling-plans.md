# ADR-057: Peat-Sim Scaling Plans — 1K / 1.2K / 10K Nodes

**Status:** Accepted
**Date:** 2026-03-20
**Decision Makers:** Research Team
**Technical Story:** Validate PEAT protocol's O(n log n) hierarchical scaling thesis at division-scale (10K nodes) through a phased progression from 1K containerlab nodes to 10K process-per-node simulation.

## Context and Problem Statement

Peat-sim's single-machine ContainerLab architecture has been validated at 1,000 nodes (traditional baseline) but hierarchical CRDT testing has only been attempted at 384 nodes (with partial success — 141/447 containers started). The Linux kernel imposes a hard limit of 1,023 veth pairs per bridge, which is the architectural ceiling for a single ContainerLab deployment. To validate the PEAT protocol's scaling thesis (O(n log n) hierarchical vs O(n^2) traditional), we need to reach 10,000 nodes.

## Decision Drivers

1. Linux kernel 1,023 veth pair limit per bridge
2. Need to validate O(n log n) hierarchical vs O(n^2) traditional scaling
3. Previous 384-node hierarchical attempt only achieved 31% container startup
4. Docker per-container overhead (~11 MB) limits containerized approaches at 10K
5. Existing DNS retry and circuit breaker patterns in peat-sim binary

## Considered Options

### Option 1: Single ContainerLab (Plan A — ~900 nodes)

Extend existing ContainerLab approach to ~895 nodes (6 companies x 4 platoons x 4 squads x 8 soldiers + leaders), staying under the 1,023 bridge limit. Mitigate container startup thundering herd with tiered WAIT_FOR_START signaling.

### Option 2: Dual ContainerLab + Router (Plan B — 1.2K nodes)

Two ContainerLab deployments on separate Docker bridges connected by a router container. Each bridge under 597 veth pairs. Cross-bridge connections use IP-based addressing.

### Option 3: Process-Per-Node (Plan C — 10K nodes)

Run 10,000 peat-sim processes directly on a large VM (96 vCPU, 384 GB RAM). No Docker overhead. Application-level delay for network simulation. ~110 GB RSS total.

## Decision

Implement all three options as a progression. Each builds on the previous:

1. **Plan A first** — foundation for everything, validates at 1K
2. **Plan B** — proves multi-bridge works, unblocks scale beyond 1,023
3. **Plan C Phase 1** — process-per-node MVP on single large VM

## Architecture

### Plan A: ~900 Hierarchical CRDT Nodes (Single Machine)

**Node math (corrected):** The hierarchy per company is: 1 commander + 4 platoon leaders + 4×4 squad leaders + 4×4×8 soldiers = 149 nodes/company + 1 battalion HQ.

| Companies | Soldiers | Squad Leaders | Platoon Leaders | COs | HQ | **Total** | Under 1,023? |
|---|---|---|---|---|---|---|---|
| 6 | 768 | 96 | 24 | 6 | 1 | **895** | Yes |
| 7 | 896 | 112 | 28 | 7 | 1 | **1,044** | **No** |

**Default: 6 companies = 895 nodes**, safely under the 1,023 bridge limit. The generator warns and recommends `--split` (Plan B) for 7+ companies.

> **Note:** An earlier draft claimed 7 companies = 1,015 nodes. The actual count is 1,044 because squad leaders were inadvertently double-counted. The generator (`generate-scaling-topology.py`) computes and validates the correct total.

**Deliverables:**
- Parameterized topology generator (`topologies/generate-scaling-topology.py`)
- Staged deployment orchestrator (`scripts/deploy-staged.sh` — tiered WAIT_FOR_START: HQ -> COs -> PLs -> SLs -> soldiers, batches of 100)
- Makefile targets (`lab-1000n`, `lab-1000n-generate`, `lab-1000n-staged`, `lab-1000n-status`, `lab-1000n-destroy`)
- Metrics collection via existing bind-mount /data/logs pattern

**Key risk:** Container startup thundering herd. Mitigated by tiered WAIT_FOR_START signaling.

**Infrastructure:** Existing 124 GB / 32-core machine or 1x m5.8xlarge (~$1.85/hr GovCloud). ~10 GB RAM, ~4 min deploy.

### Plan B: 1,200 Nodes (Breaking the Bridge Limit)

**Architecture:**
```
Single Machine
├── ContainerLab "peat-1200a" (bridge A, 172.30.0.0/22)
│   ├── ~597 nodes (4 companies + gateway-a)
│   └── Companies 1-4 hierarchy
├── ContainerLab "peat-1200b" (bridge B, 172.31.0.0/22)
│   ├── ~597 nodes (4 companies + gateway-b)
│   └── Companies 5-8 hierarchy
└── Router container (connected to both bridges)
    ├── IP forwarding + iptables FORWARD rules
    └── Battalion HQ sits on bridge A; companies 5-8 connect through router
```

**Key risk:** Cross-bridge DNS resolution. Docker DNS only resolves within same network. Mitigated by IP-based addressing for the 4 cross-bridge connections (company CO -> battalion HQ).

### Plan C: 10,000 Nodes (Full Scale)

**Architecture:** Process-per-node on a single large VM (m5.24xlarge: 96 vCPU, 384 GB).

```
1x m5.24xlarge (96 vCPU, 384 GB RAM)
├── Process orchestrator (Python)
│   ├── Config generator (JSON per node from hierarchy spec)
│   ├── Tiered process launcher (batches of 200, tier-by-tier)
│   ├── Health monitor (TCP probe per process)
│   └── Metrics aggregator (combines 10K JSONL files)
└── 10,000 peat-sim processes
    ├── ~67 companies x 149 nodes/company = ~9,983 + HQ
    ├── Each process binds unique port on localhost
    ├── TCP_CONNECT uses 127.0.0.1:PORT (no DNS needed)
    └── Total: ~110 GB RSS
```

**Phasing:**
- Phase 1 (MVP): Single large VM, process-per-node, application-level delay. 1K -> 5K -> 10K incremental. ~10-14 days.
- Phase 2: Multi-VM fallback with WireGuard mesh if needed. +5 days.
- Phase 3 (optional): K8s via peat-mesh Helm chart on EKS. +8 days.

**Key risks and mitigations:**

| Risk | Mitigation |
|---|---|
| File descriptor exhaustion | ulimit -n 100000 before launch; ~5-10 FDs per process |
| Port exhaustion | Explicit port assignment (10K of 64K available) |
| CRDT state size at 10K | Hierarchical aggregation bounds per-node state; monitor RSS |
| TCP connection storms | Tiered startup, 200-process batches, 2-5s between batches |

## Verification Criteria

### Plan A (~900)

- All 895 containers reach Running state (6 companies default; adjustable)
- Every squad leader logs 8 DocumentReceived events from soldiers
- Company commanders produce CompanySummary aggregation docs
- P50/P95/P99 latency at each tier measured and recorded
- 3 consecutive runs with >95% sync success

### Plan B (1.2K)

- Two separate bridges visible with <1,023 interfaces each
- Cross-bridge ping succeeds
- All ~1,195 containers running
- Soldiers on bridge B produce updates reaching battalion HQ on bridge A
- Cross-bridge latency overhead documented vs same-bridge latency

### Plan C (10K)

- pgrep -c peat-sim returns ~10,000
- Total RSS ~110 GB (no OOM kills)
- All 67 company commanders produce CompanySummary docs
- End-to-end: soldier update reaches battalion HQ via hierarchy
- Scaling curve: latency at 1K/5K/10K shows O(n log n) for hierarchical
- Zero process crashes

## Cost Summary

| | Plan A (~900) | Plan B (~1,200) | Plan C (10K) |
|---|---|---|---|
| Architecture | Single ContainerLab | Dual ContainerLab + router | Process-per-node |
| Default nodes | 895 (6 co) | 1,193 (8 co, 4+4 split) | ~9,984 (67 co) |
| Machines | 1 (existing) | 1 (existing) | 1 large cloud VM |
| RAM | ~10 GB | ~13 GB | ~110 GB |
| Cost/run | ~$3 | ~$3 | ~$11 |
| Effort | 5-7 days | 7-10 days (after A) | 10-14 days (Phase 1) |

## Consequences

### Positive

1. **Phased De-Risking**
   - Each plan validates assumptions before committing to the next scale increment
   - Plan A validates tiered startup and hierarchical CRDT at 1K before investing in multi-bridge or process-per-node infrastructure

2. **Scaling Thesis Validation**
   - Provides concrete data points at 1K, 1.2K, and 10K to confirm or refute O(n log n) hierarchical scaling
   - Comparison against O(n^2) traditional baseline at each tier

3. **Reusable Infrastructure**
   - Parameterized topology generator and staged deployment orchestrator benefit all future experiments
   - Process-per-node orchestrator (Plan C) can be repurposed for other large-scale protocol tests

4. **Cost Efficiency**
   - Plans A and B run on existing hardware at near-zero marginal cost
   - Plan C on a single VM keeps cloud spend under $11/run

### Negative

1. **Cumulative Effort**
   - Full progression (A + B + C Phase 1) requires 22-31 engineering days
   - May delay other research priorities if the team is resource-constrained

2. **Plan C Fidelity**
   - Process-per-node with application-level delay is less realistic than containerized network simulation
   - Results at 10K may not perfectly predict real-world performance with actual network stacks

3. **Single-VM Ceiling**
   - Plan C Phase 1 is bounded by a single VM's resources (384 GB)
   - Scaling beyond 10K would require the multi-VM Phase 2 work

### Risks and Mitigation

**Risk 1: Plan A thundering herd persists despite tiered startup**
- **Impact:** HIGH — container startup failures repeat the 384-node experience
- **Likelihood:** MEDIUM — tiered signaling is untested at ~900 nodes
- **Mitigation:** Start with 500-node smoke test (3 companies); tune batch size and inter-batch delay before full 6-company deployment

**Risk 2: Cross-bridge routing (Plan B) introduces unacceptable latency**
- **Impact:** MEDIUM — may invalidate cross-bridge latency comparisons
- **Likelihood:** LOW — routing adds microseconds, not milliseconds
- **Mitigation:** Measure same-bridge vs cross-bridge latency delta; document as known variable

**Risk 3: Plan C RSS exceeds 384 GB**
- **Impact:** HIGH — OOM kills invalidate the entire 10K run
- **Likelihood:** MEDIUM — depends on CRDT state growth under hierarchical aggregation
- **Mitigation:** Incremental ramp (1K -> 5K -> 10K) with RSS monitoring at each stage; abort and investigate if RSS exceeds 300 GB before reaching 10K

**Risk 4: File descriptor or port exhaustion at 10K**
- **Impact:** HIGH — processes fail to bind or connect
- **Likelihood:** LOW — ulimit and explicit port assignment are well-understood mitigations
- **Mitigation:** Pre-flight check script validates ulimit, available ports, and kernel parameters before launch

## References

- ADR-015: Experimental Validation Framework and Hierarchical Aggregation Requirements
- ADR-008: Network Simulation Layer
- [EPIC: Peat-Sim Scaling Validation — 900 / 1.2K / 10K Nodes (#724)](https://github.com/defenseunicorns/peat/issues/724)
  - [Plan A: ~900 nodes (#725)](https://github.com/defenseunicorns/peat/issues/725)
  - [Plan B: ~1,200 dual-bridge nodes (#726)](https://github.com/defenseunicorns/peat/issues/726)
  - [Plan C: 10,000 process-per-node (#727)](https://github.com/defenseunicorns/peat/issues/727)
- peat-sim/topologies/lab4-384n-1gbps.yaml — env var pattern reference
- peat-sim/entrypoint.sh — WAIT_FOR_START mechanism
- peat-sim/src/main.rs — DNS retry, connection staggering

---

**Decision Record:**
- **Proposed:** 2026-03-20
- **Accepted:** 2026-03-20
- **Supersedes:** None
- **Superseded by:** None

**Authors:** Research Team
**Reviewers:** Research Team
