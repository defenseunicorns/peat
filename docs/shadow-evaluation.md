# Shadow Network Simulator - Evaluation for CAP Protocol E8

**Date:** 2025-11-04
**Context:** Evaluating Shadow as alternative to Linux namespaces for E8 network simulation

## What is Shadow?

Shadow is a Rust/C-based network simulator that runs **real, unmodified applications** in a **simulated network environment** by intercepting system calls. It's designed for scientific network research and won the 2022 USENIX Best Paper Award.

**Key Innovation:** Applications think they're running on a real network, but Shadow intercepts syscalls and routes traffic through an internal simulated network with configurable topology, bandwidth, latency, and packet loss.

## Shadow vs Linux Namespaces

| Aspect | Shadow | Linux Namespaces + tc/netem |
|--------|--------|----------------------------|
| **Realism** | Simulated network (not real kernel stack) | Real kernel network stack |
| **Determinism** | ✅ Fully deterministic, reproducible | ⚠️ Timing-dependent, harder to reproduce |
| **Scalability** | ✅ Hundreds of thousands of processes | ⚠️ Limited by physical resources (~100-200) |
| **Performance** | ✅ Simulated time - can run faster than real-time | ❌ Real-time only |
| **Setup Complexity** | ✅ YAML config, no root required | ❌ Complex bash scripts, requires root |
| **CI/CD** | ✅ Runs anywhere (Linux), no privileges | ❌ Requires root, platform-specific |
| **Network Control** | ✅ Precise packet-level control via config | ⚠️ tc/netem approximations |
| **Application Changes** | ✅ Run unmodified binaries | ✅ Run unmodified binaries |
| **Ditto Compatibility** | ❓ Need to test - syscall interception may work | ✅ Known to work |
| **Debugging** | ✅ Deterministic replay | ⚠️ Harder to reproduce timing issues |

## How Shadow Would Work for CAP

### Configuration Example: Squad Formation

```yaml
general:
  stop_time: 5 min
  seed: 42  # Deterministic
  parallelism: 4

network:
  graph:
    type: gml
    inline: |
      graph [
        # Intra-squad mesh (100Kbps, 100ms)
        node [
          id 0
          host_bandwidth_down "100 Kbit"
          host_bandwidth_up "100 Kbit"
        ]
        edge [
          source 0
          target 0
          latency "100 ms"
          packet_loss 0.01
        ]
      ]

hosts:
  # 9 soldiers
  soldier_1:
    network_node_id: 0
    processes:
      - path: ./target/release/cap-sim-node
        args: "--node-id soldier-1 --role soldier --capabilities sensor,comms"
        start_time: 1s

  soldier_2:
    network_node_id: 0
    processes:
      - path: ./target/release/cap-sim-node
        args: "--node-id soldier-2 --role soldier --capabilities sensor,comms"
        start_time: 1s

  # ... soldiers 3-9 ...

  # 1 UGV
  ugv_1:
    network_node_id: 0
    processes:
      - path: ./target/release/cap-sim-node
        args: "--node-id ugv-1 --role robot --capabilities resupply,isr"
        start_time: 2s

  # 2 UAVs
  uav_1:
    network_node_id: 0
    processes:
      - path: ./target/release/cap-sim-node
        args: "--node-id uav-1 --role drone --capabilities aerial_recon"
        start_time: 3s

  uav_2:
    network_node_id: 0
    processes:
      - path: ./target/release/cap-sim-node
        args: "--node-id uav-2 --role drone --capabilities aerial_recon"
        start_time: 3s
```

### Company Hierarchy Example

For a full company (112 nodes), we'd define:
- **Multiple network nodes** (squads, platoons, company HQ)
- **Different latencies** between layers (squad→platoon: 500ms, platoon→company: 1s)
- **Variable bandwidth** by echelon (100Kbps local, 56Kbps radio, 19.2Kbps SATCOM)

```yaml
network:
  graph:
    type: gml
    inline: |
      graph [
        # Squad 1 node
        node [id 1, host_bandwidth_down "100 Kbit"]

        # Platoon 1 node
        node [id 10, host_bandwidth_down "56 Kbit"]

        # Company HQ node
        node [id 100, host_bandwidth_down "19.2 Kbit"]

        # Squad 1 -> Platoon 1
        edge [source 1, target 10, latency "500 ms", packet_loss 0.05]

        # Platoon 1 -> Company HQ
        edge [source 10, target 100, latency "1000 ms", packet_loss 0.10]
      ]
```

## Advantages for CAP Protocol E8

### 1. Deterministic, Reproducible Testing
- **Same seed = identical simulation** every time
- Critical for debugging race conditions and timing issues
- CI/CD friendly - tests are stable

### 2. Simulated Time = Faster Iteration
- Can run 30-minute scenarios in seconds (if CPU-bound)
- "Fast-forward" through quiet periods
- Accelerates baseline measurement collection

### 3. Zero Infrastructure Overhead
- No root access needed
- No namespace setup/teardown scripts
- Runs on developer laptops, CI servers, anywhere
- Just: `shadow scenario.yaml`

### 4. Precise Network Control
- Exact bandwidth/latency/loss specification
- Can model complex topologies (hierarchical company structure)
- Dynamic network changes (partitions) via config

### 5. Scalability Beyond Hardware Limits
- Can simulate 112 nodes on modest hardware
- Shadow's syscall interception is lightweight
- Network is simulated, not real - no actual packets

### 6. Built-in Metrics
- Shadow tracks network statistics automatically
- Bandwidth usage, latency, packet loss per host
- Integrates with analysis tools

## Concerns & Risks

### 1. Ditto SDK Compatibility ⚠️
**Question:** Will Ditto's Rust SDK work under Shadow's syscall interception?

**Need to test:**
- Ditto uses standard sockets/networking APIs (likely works)
- Ditto may use advanced syscalls Shadow doesn't support (could break)
- Ditto's internal threading/timing assumptions (may behave differently)

**Mitigation:**
- Create a simple test: 2 Ditto instances syncing under Shadow
- If it works: Shadow is a great fit
- If it breaks: Fall back to namespaces or hybrid approach

### 2. Realism vs Simulation
Shadow simulates TCP/UDP, not the real Linux kernel stack. For CAP:
- **Probably fine:** We care about protocol correctness, not kernel-specific behavior
- **Validate:** Compare Shadow results vs real deployment (eventually)

### 3. Learning Curve
Team needs to learn Shadow's configuration and quirks.

**Mitigation:**
- Start with simple scenarios (squad formation)
- Shadow docs are good, active community
- Simpler than namespace scripting

### 4. Network Abstraction Differences
Shadow's network model may not perfectly match tactical radio behavior.

**Mitigation:**
- Focus on relative comparisons (baseline vs delta)
- Good enough to validate protocol behavior
- Real field testing validates accuracy later

## Recommendation

**Proposal:** Make Shadow the **primary** E8 approach, with namespaces as a validation fallback.

### Phased Approach

**Phase 1: Shadow Proof-of-Concept (E8.0)**
- Install Shadow on Linux workstation
- Create simple test: 2 Ditto nodes syncing in Shadow
- If successful → proceed with Phase 2
- If broken → fall back to namespace approach

**Phase 2: Shadow-Based Scenarios (E8.1-E8.4)**
- Implement all 6 scenarios using Shadow configs
- Collect baseline metrics
- Compare against E7 projections

**Phase 3: Validation (Optional)**
- Run select scenarios in namespaces to validate Shadow accuracy
- Document any differences
- Use Shadow for iteration, namespaces for final validation

## Shadow vs ADR-008 Proposal

### What Changes
- **Architecture:** Replace `LinuxNamespaceOrchestrator` with `ShadowSimulator`
- **Configuration:** YAML configs instead of bash scripts
- **No root required:** Democratizes testing (any developer can run)
- **Faster iteration:** Simulated time accelerates testing

### What Stays the Same
- **Army company structure:** Still use 112-node company as reference
- **Network profiles:** Same bandwidth/latency values, just in YAML
- **6 scenarios:** Same tactical scenarios
- **Metrics:** Same measurement goals
- **cap-sim binary:** Generates Shadow YAML, runs simulations, collects metrics

## Next Steps

1. **Review Shadow with team** - Discuss pros/cons
2. **Proof-of-concept test** - Does Ditto work under Shadow?
3. **Update ADR-008** - If POC succeeds, revise to use Shadow
4. **Implement E8.1** - Build Shadow-based simulation harness

## Resources

- **Shadow Website:** https://shadow.github.io/
- **Documentation:** https://shadow.github.io/docs/guide/
- **GitHub:** https://github.com/shadow/shadow
- **Paper:** "Co-opting Linux Processes for High Performance Network Simulation" (USENIX 2022)
- **Installation:** Likely available via package manager or build from source

## Example: Full Scenario Config Structure

```yaml
# cap-sim/scenarios/squad-formation.yaml
general:
  stop_time: 5 min
  seed: 42
  parallelism: 4
  log_level: info

network:
  graph:
    type: gml
    file: scenarios/networks/squad-mesh.gml

hosts:
  <<: *squad_1_nodes    # YAML merge for 12 squad nodes
  <<: *metrics_collector # Shadow can collect stats

experimental:
  use_cpu_pinning: true
  max_unapplied_cpu_latency: 1 ms
```

This approach would significantly simplify E8 implementation while providing better determinism and scalability than the namespace approach.
