# E8: Network Simulation Layer - Getting Started

## Overview

Epic 8 implements a realistic network simulation layer using the **Shadow network simulator** to validate CAP protocol performance under tactical network constraints.

### What is Shadow?

Shadow is a Rust-based network simulator that runs **real, unmodified applications** in a simulated network environment by intercepting system calls. It's designed for scientific network research and won the 2022 USENIX Best Paper Award.

**Key advantages for CAP**:
- **Deterministic**: Same seed = identical simulation (critical for CI/CD)
- **Scalable**: Can simulate 100+ nodes on modest hardware
- **No root required**: YAML configs, runs anywhere on Linux
- **Simulated time**: Can run faster than real-time for rapid iteration
- **Written in Rust**: Aligns with our stack, active development

## Quick Start

### 1. Review the Documentation

Start by reviewing these key documents:

- **[E8-IMPLEMENTATION-PLAN.md](E8-IMPLEMENTATION-PLAN.md)**: Comprehensive implementation guide
- **[ADR-008](adr/008-network-simulation-layer.md)**: Network simulation architecture decision
- **[shadow-evaluation.md](shadow-evaluation.md)**: Shadow vs alternatives analysis

### 2. Begin with E8.0 POC (CRITICAL)

**Issue**: [#36 - E8.0: Shadow + Ditto POC](https://github.com/kitplummer/cap/issues/36)

This is a **GO/NO-GO decision point**. We must validate that Ditto SDK works correctly under Shadow's syscall interception before proceeding.

**Timeline**: 1-2 days

**Tasks**:
1. Install Shadow on Linux workstation
2. Create minimal Ditto sync test binary
3. Run 2-node sync test under Shadow
4. Analyze results and make GO/NO-GO decision

**If GO**: Proceed with E8.1-E8.4 (Shadow-based approach)
**If NO-GO**: Fall back to Linux namespaces + tc/netem approach

### 3. Installation

#### Install Shadow

**⚠️ Important**: Shadow requires **Rust nightly** to build from source.

```bash
# Install Rust nightly
rustup toolchain install nightly

# Clone and checkout stable release
git clone https://github.com/shadow/shadow.git
cd shadow
git checkout v3.3.0

# Set nightly for Shadow directory only
rustup override set nightly

# Build and install (to ~/.local/bin, no sudo required!)
./setup build --clean
./setup install

# Add to PATH if needed
export PATH="$HOME/.local/bin:$PATH"
```

**📖 Detailed installation guide**: See [E8_SHADOW_INSTALLATION.md](E8_SHADOW_INSTALLATION.md) for troubleshooting and details.

#### Verify Installation

```bash
shadow --version
# Expected: Shadow 3.3.0 — v3.3.0-0-g5a05740ba
```

#### System Requirements

- **OS**: Linux (Ubuntu 22.04+, Debian 11+, Fedora 42)
- **Kernel**: 5.10+
- **RAM**: 16GB+ (for 112-node simulations)
- **CPU**: 8+ cores recommended
- **Root**: Not required (major advantage!)

### 4. Run the POC

Once Shadow is installed and E8.0 code is implemented:

```bash
# Build the POC binary
cargo build --release --example shadow_poc

# Run under Shadow
shadow cap-sim/scenarios/poc-ditto-sync.yaml

# Check results
cat shadow.data/hosts/*/stdout.log
```

### 5. Next Steps After POC

If POC succeeds, proceed with:

- **[#37 - E8.1: Shadow Harness](https://github.com/kitplummer/cap/issues/37)** (3-4 days)
  - Build `cap-sim` and `cap-sim-node` binaries
  - Implement Shadow YAML generator
  - Create first 12-node squad formation scenario

- **[#38 - E8.2: Network Constraints](https://github.com/kitplummer/cap/issues/38)** (2-3 days)
  - Implement tactical network profiles (100Kbps/56Kbps/19.2Kbps)
  - Apply bandwidth/latency/loss constraints
  - Create constrained scenarios

- **[#39 - E8.3: Network Partitions](https://github.com/kitplummer/cap/issues/39)** (2-3 days)
  - Implement partition/heal scenarios
  - Validate CRDT consistency during network splits
  - Test split-brain scenarios

- **[#40 - E8.4: Baseline Analysis](https://github.com/kitplummer/cap/issues/40)** (2-3 days)
  - Run full scenario matrix (12, 39, 112 nodes)
  - Generate comprehensive baseline report
  - Compare against ADR-001 targets

## Project Structure

After E8 implementation, the structure will be:

```
cap/
├── cap-sim/
│   ├── src/
│   │   ├── bin/
│   │   │   ├── cap_sim.rs          # Orchestrator (generate/run/report)
│   │   │   └── cap_sim_node.rs     # Simulation node binary
│   │   ├── shadow_generator.rs     # Shadow YAML generator
│   │   ├── network_profiles.rs     # Network constraint profiles
│   │   ├── metrics/                # Metrics collection
│   │   ├── runner.rs               # Batch scenario runner
│   │   └── report.rs               # Report generator
│   └── scenarios/
│       ├── schema.json              # Scenario format definition
│       ├── poc-ditto-sync.yaml      # E8.0: POC scenario
│       ├── squad-formation.yaml     # E8.1: 12 nodes
│       ├── squad-constrained.yaml   # E8.2: Constrained network
│       ├── platoon-formation.yaml   # E8.2: 39 nodes
│       ├── company-formation.yaml   # E8.2: 112 nodes
│       └── partition-*.yaml         # E8.3: Partition scenarios
├── cap-protocol/
│   └── examples/
│       └── shadow_poc.rs            # E8.0: Minimal Ditto sync test
└── docs/
    ├── E8-IMPLEMENTATION-PLAN.md    # Comprehensive plan
    ├── E8_SHADOW_POC_RESULTS.md     # E8.0: POC results (to be created)
    └── E8_BASELINE_REPORT.md        # E8.4: Baseline report (to be created)
```

## Reference Scenarios

### Scenario 1: Squad Formation (12 nodes)
- 9 dismounted soldiers
- 1 UGV (ground robot)
- 2 UAVs (drones)
- **Network**: Unconstrained (baseline)
- **Expected**: <5s convergence, ~57KB total bandwidth

### Scenario 2: Platoon Formation (39 nodes)
- 1 Platoon HQ
- 3 Squads (12 nodes each)
- **Network**: 56Kbps, 500ms latency, 5% loss (tactical radio)
- **Expected**: <15s convergence, hierarchical cell formation

### Scenario 3: Company Formation (112 nodes)
- 1 Company HQ
- 3 Platoons (39 nodes each)
- 9 Squads total
- **Network**: Mixed constraints (100/56/19.2 Kbps by echelon)
- **Expected**: <30s convergence, O(n log n) message complexity

### Scenario 4: Network Partition
- Start: Full company (112 nodes)
- Event: Isolate Platoon 2 for 5 minutes
- Changes: UGV offline in Platoon 2, mission reassignment at Company HQ
- **Expected**: Both changes preserved, <30s convergence after heal

## Success Metrics

### Performance
- **Convergence Time**: <5s for Priority 1 updates (ADR-001 target)
- **Bandwidth Usage**: <10% of available bandwidth (ADR-001 target)
- **Message Count**: O(n log n) scaling, not O(n²)
- **Staleness**: <30s for 90% of updates (ADR-001)

### Reliability
- **Partition Tolerance**: 100% eventual consistency
- **Error Rate**: <1% message loss acceptable
- **Recovery Time**: <10s for small networks (5-10 nodes)

### Comparison
- **Baseline vs Delta**: Current bandwidth vs E7 projected savings (79-89% reduction)
- **Actual vs Target**: Measured performance vs ADR-001 requirements

## Resources

### Documentation
- Shadow Website: https://shadow.github.io/
- Shadow Docs: https://shadow.github.io/docs/guide/
- Shadow GitHub: https://github.com/shadow/shadow
- Shadow Paper: "Co-opting Linux Processes for High Performance Network Simulation" (USENIX 2022)

### CAP Project Resources
- E8 Implementation Plan: [docs/E8-IMPLEMENTATION-PLAN.md](E8-IMPLEMENTATION-PLAN.md)
- ADR-008: [docs/adr/008-network-simulation-layer.md](adr/008-network-simulation-layer.md)
- Shadow Evaluation: [docs/shadow-evaluation.md](shadow-evaluation.md)
- Project Plan: [docs/CAP-POC-Project-Plan.md](CAP-POC-Project-Plan.md)

### GitHub Issues
- **Parent**: [#9 - E8: Network Simulation Layer](https://github.com/kitplummer/cap/issues/9)
- **E8.0**: [#36 - Shadow + Ditto POC](https://github.com/kitplummer/cap/issues/36) ⚠️ START HERE
- **E8.1**: [#37 - Shadow Harness](https://github.com/kitplummer/cap/issues/37)
- **E8.2**: [#38 - Network Constraints](https://github.com/kitplummer/cap/issues/38)
- **E8.3**: [#39 - Network Partitions](https://github.com/kitplummer/cap/issues/39)
- **E8.4**: [#40 - Baseline Analysis](https://github.com/kitplummer/cap/issues/40)

## Timeline

| Week | Phase | Deliverable |
|------|-------|-------------|
| Week 1 | E8.0 POC | GO/NO-GO decision on Shadow |
| Week 1-2 | E8.1 Harness | Squad formation (12 nodes) working |
| Week 2 | E8.2 Constraints | Platoon formation (39 nodes) with constraints |
| Week 2-3 | E8.3 Partitions | Company (112 nodes) with partition scenarios |
| Week 3 | E8.4 Analysis | Comprehensive baseline report |

**Total**: ~3 weeks (12-15 days)

## Questions?

- Check [E8-IMPLEMENTATION-PLAN.md](E8-IMPLEMENTATION-PLAN.md) for detailed guidance
- Review [ADR-008](adr/008-network-simulation-layer.md) for architectural decisions
- Post questions on GitHub issues for specific sub-phases

## Ready to Start?

1. ✅ Review this guide
2. ✅ Read [E8-IMPLEMENTATION-PLAN.md](E8-IMPLEMENTATION-PLAN.md)
3. ✅ Install Shadow on Linux workstation
4. ✅ Start with [Issue #36 - E8.0 POC](https://github.com/kitplummer/cap/issues/36)

---

**Document Version**: 1.0
**Last Updated**: 2025-11-05
**Next Review**: After E8.0 POC completion
