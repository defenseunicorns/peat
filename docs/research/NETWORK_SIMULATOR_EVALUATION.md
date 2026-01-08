# Network Simulator Evaluation

**Date**: November 2024
**Purpose**: Evaluate network simulation alternatives for HIVE Protocol validation
**Context**: Shadow network simulator proved incompatible with Ditto SDK

---

## Requirements

For HIVE Protocol E8 network simulation, we need:

1. **Run Real Ditto Binaries**: Must execute unmodified Ditto SDK (Rust application with complex networking)
2. **Network Constraints**: Bandwidth limiting, latency injection, packet loss, jitter
3. **Network Partitions**: Selective connectivity between node groups
4. **Scale**: Support 112 nodes (Army company: 3 platoons × 3 squads × 12 nodes + command)
5. **Reproducibility**: Deterministic or at least consistent results
6. **Telemetry**: Ability to collect metrics on sync times, bandwidth usage
7. **Timeline**: Quick to implement (within 3-week E8 timeline)

---

## Options Evaluated

### 1. ContainerLab

**Type**: Container-based network emulation
**Website**: https://containerlab.dev/

#### Overview
ContainerLab uses Docker containers to create network topologies with realistic link impairments. Originally designed for network device emulation, it works with any containerized application.

#### Pros
- ✅ **Runs real applications**: Any Docker container, including Ditto
- ✅ **Declarative config**: YAML topology definitions (easy to version control)
- ✅ **Full network constraints**:
  - Bandwidth limiting via `tc tbf` (token bucket filter)
  - Latency/jitter via `tc netem delay`
  - Packet loss via `tc netem loss`
- ✅ **Built-in tooling**: `containerlab tools netem` commands
- ✅ **Active development**: Modern project, active community
- ✅ **Container isolation**: Better than raw processes, easier cleanup
- ✅ **Portable**: Containers run anywhere Docker runs
- ✅ **Scale**: Proven with hundreds of containers
- ✅ **Good docs**: Clear examples, active discussions

#### Cons
- ⚠️  **Requires Linux with tc module**: Docker Desktop (Mac/Windows) lacks tc support
  - Workaround: Run in Linux VM (Multipass, VirtualBox)
  - Not an issue for Linux development machines
- ⚠️  **Container overhead**: Slightly more resources than raw namespaces
  - But: 112 containers is well within reasonable limits
- ⚠️  **Less deterministic**: Real-time execution (same as namespaces)

#### E8 Suitability
**Rating**: ⭐⭐⭐⭐⭐ **EXCELLENT**

- Perfect fit for our use case
- Ditto containerizes easily (`FROM rust:1.86` base)
- YAML topology maps cleanly to Army structure
- Constraints implemented via proven tc/netem
- Easy to script, automate, reproduce

#### Example Topology
```yaml
name: cap-squad-formation

topology:
  nodes:
    soldier1:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: soldier1
        ROLE: rifleman

    soldier2:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: soldier2
        ROLE: team_leader

    uav1:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: uav1
        ROLE: scout

  links:
    - endpoints: ["soldier1:eth1", "soldier2:eth1"]
      # Tactical radio constraints
      impairments:
        delay: 50ms
        jitter: 10ms
        loss: 1%
        rate: 56kbps

    - endpoints: ["soldier2:eth2", "uav1:eth1"]
      # Better link for UAV
      impairments:
        delay: 20ms
        rate: 256kbps
```

#### Implementation Effort
**Estimate**: 2-3 days

- Day 1: Create Ditto-based `hive-sim-node` container
- Day 2: Test 2-node topology, validate constraints work
- Day 3: Scale to 12-node squad, automate scenario generation

---

### 2. Mininet

**Type**: Network emulator using Linux namespaces
**Website**: http://mininet.org/

#### Overview
Mininet is a Python framework that creates network topologies using Linux process namespaces. Originally created for SDN research (Software-Defined Networking with OpenFlow).

#### Pros
- ✅ **Runs real applications**: Hosts run unmodified binaries
- ✅ **Python API**: Clean, well-documented API
- ✅ **Mature project**: Stable, widely used in research/education
- ✅ **Lightweight**: Direct namespace usage (minimal overhead)
- ✅ **tc/netem support**: Full constraint capabilities
- ✅ **Scale**: Tested with 1000+ nodes

#### Cons
- ⚠️  **SDN-focused**: Most examples/docs are OpenFlow-centric
- ⚠️  **Python 2 legacy**: Recently migrated to Python 3, some cruft
- ⚠️  **Manual scripting**: Need to write Python code (no declarative config)
- ⚠️  **Less portable**: Tied to Linux host, no container isolation

#### E8 Suitability
**Rating**: ⭐⭐⭐⭐ **VERY GOOD**

- This is essentially what we planned (namespaces + Python wrapper)
- Would simplify our implementation vs raw bash/ip commands
- Python API nice for scripting scenarios
- But: Less modern than ContainerLab, SDN focus not relevant

#### Example Code
```python
from mininet.net import Mininet
from mininet.link import TCLink

net = Mininet(link=TCLink)

# Create hosts
soldier1 = net.addHost('soldier1')
soldier2 = net.addHost('soldier2')

# Create link with constraints
net.addLink(
    soldier1, soldier2,
    bw=0.056,  # 56 kbps
    delay='50ms',
    loss=1
)

net.start()

# Run Ditto on each host
soldier1.cmd('/path/to/hive-sim-node --node-id soldier1 &')
soldier2.cmd('/path/to/hive-sim-node --node-id soldier2 &')

# Wait for scenario to complete
time.sleep(60)

net.stop()
```

#### Implementation Effort
**Estimate**: 3-4 days

- Day 1: Learn Mininet API, create simple topology
- Days 2-3: Build scenario generator, integrate hive-sim-node
- Day 4: Scale to 12-node squad, test constraints

---

### 3. ns-3 + DCE (Direct Code Execution)

**Type**: Discrete-event network simulator with real code execution
**Website**: https://www.nsnam.org/

#### Overview
ns-3 is a discrete-event network simulator written in C++. DCE (Direct Code Execution) is a framework that allows running real applications within ns-3 simulations.

#### Pros
- ✅ **Deterministic**: Discrete-event simulation (reproducible results)
- ✅ **Time dilation**: Can simulate faster than real-time
- ✅ **Sophisticated models**: Detailed protocol models, mobility, etc.
- ✅ **DCE runs real code**: Can execute real Linux network stack and applications
- ✅ **Research-grade**: Used extensively in academic research

#### Cons
- ❌ **Complex integration**: Applications must be compiled as DCE modules
- ❌ **C++ heavy**: ns-3 is C++, DCE integration requires C++ knowledge
- ❌ **DCE less active**: DCE project has slowed development (last major update 2019)
- ❌ **Steep learning curve**: Both ns-3 and DCE have significant complexity
- ❌ **Build complexity**: Getting Ditto to work with DCE would be challenging
- ⚠️  **Not true "unmodified" code**: Requires recompilation with DCE hooks

#### E8 Suitability
**Rating**: ⭐⭐ **POOR**

- Determinism and time dilation are attractive
- But: Integration effort too high for 3-week timeline
- DCE support for Rust applications unclear
- Risk that Ditto won't work even with DCE (similar to Shadow)
- Overkill for our needs (we don't need mobility models, etc.)

#### Implementation Effort
**Estimate**: 2-3 weeks (too long)

- Week 1: Learn ns-3, understand DCE framework
- Week 2: Attempt Ditto integration with DCE
- Week 3: Debug issues (high risk of blockers)

**Verdict**: Not viable for E8 timeline.

---

### 4. CORE (Common Open Research Emulator)

**Type**: Network emulator with GUI, uses Linux namespaces
**Website**: https://coreemu.github.io/core/

#### Overview
CORE is a network emulator developed by the Naval Research Laboratory. It provides a GUI for creating network topologies and runs real applications in lightweight containers (namespaces).

#### Pros
- ✅ **Runs real applications**: Real protocols, real code
- ✅ **GUI + API**: Visual topology builder + Python scripting
- ✅ **Military pedigree**: Developed by NRL, used in DoD research
- ✅ **Connects to real networks**: Can bridge emulated and physical networks
- ✅ **tc/netem support**: Full constraint capabilities
- ✅ **Good for demos**: Visual representation helpful for presentations

#### Cons
- ⚠️  **Smaller community**: Less active than Mininet or ContainerLab
- ⚠️  **GUI-centric**: Designed for interactive use (automation possible but secondary)
- ⚠️  **Installation complexity**: More dependencies than alternatives
- ⚠️  **Less modern**: Codebase shows age (though actively maintained)

#### E8 Suitability
**Rating**: ⭐⭐⭐ **GOOD**

- Viable option, similar capabilities to Mininet
- GUI could be useful for demos/visualizations
- Military heritage relevant to Army use case
- But: Heavier than needed, smaller community

#### Implementation Effort
**Estimate**: 3-4 days

- Day 1: Install CORE, learn GUI and Python API
- Days 2-3: Create scenario scripts, integrate hive-sim-node
- Day 4: Test constraints, scale to squad level

---

### 5. Raw Linux Namespaces (Original Plan)

**Type**: Direct Linux kernel features
**Implementation**: Bash scripts + `ip netns` + `tc` commands

#### Pros
- ✅ **No dependencies**: Uses built-in Linux features
- ✅ **Maximum control**: Direct access to kernel networking
- ✅ **Lightweight**: No abstraction overhead
- ✅ **Educational**: Learn kernel networking deeply

#### Cons
- ❌ **Manual everything**: Must script all topology management
- ❌ **Error-prone**: Easy to leak namespaces, leave dangling interfaces
- ❌ **No high-level API**: Raw bash/shell commands
- ❌ **Not portable**: Scripts tied to specific Linux setup
- ❌ **Debugging hard**: No tooling, must use low-level commands

#### E8 Suitability
**Rating**: ⭐⭐⭐ **ADEQUATE**

- Will work, but painful
- Most effort goes to infrastructure, not simulation
- High risk of bugs in namespace management
- Hard to reproduce scenarios reliably

#### Implementation Effort
**Estimate**: 4-5 days

- Days 1-2: Build namespace utilities (create, connect, cleanup)
- Day 3: Traffic control utilities
- Days 4-5: Scenario orchestration, debugging cleanup issues

---

## Comparison Matrix

| Feature | ContainerLab | Mininet | ns-3 + DCE | CORE | Raw Namespaces |
|---------|-------------|---------|-----------|------|----------------|
| **Runs Real Ditto** | ✅ Yes (container) | ✅ Yes | ⚠️  Complex | ✅ Yes | ✅ Yes |
| **Bandwidth Limits** | ✅ tc tbf | ✅ tc tbf | ✅ Simulated | ✅ tc tbf | ✅ tc tbf |
| **Latency/Loss** | ✅ tc netem | ✅ tc netem | ✅ Simulated | ✅ tc netem | ✅ tc netem |
| **Declarative Config** | ✅ YAML | ❌ Python | ⚠️  C++ | ⚠️  XML/GUI | ❌ Scripts |
| **Deterministic** | ❌ Real-time | ❌ Real-time | ✅ Discrete-event | ❌ Real-time | ❌ Real-time |
| **Scale (112 nodes)** | ✅ Proven | ✅ Proven | ⚠️  Unclear | ✅ Likely | ✅ Yes |
| **Learning Curve** | ⭐⭐ Low | ⭐⭐⭐ Medium | ⭐⭐⭐⭐⭐ High | ⭐⭐⭐ Medium | ⭐⭐⭐⭐ Med-High |
| **Implementation Time** | 2-3 days | 3-4 days | 2-3 weeks | 3-4 days | 4-5 days |
| **Community/Support** | ⭐⭐⭐⭐⭐ Excellent | ⭐⭐⭐⭐ Good | ⭐⭐⭐ OK | ⭐⭐ Small | N/A |
| **Documentation** | ⭐⭐⭐⭐⭐ Excellent | ⭐⭐⭐⭐ Good | ⭐⭐⭐ OK | ⭐⭐⭐ OK | N/A |
| **Portability** | ✅ Containers | ⚠️  Linux-only | ⚠️  Linux-only | ⚠️  Linux-only | ⚠️  Linux-only |
| **Cleanup/Isolation** | ✅ Easy | ⚠️  Manual | ✅ Automatic | ⚠️  Manual | ⚠️  Manual |

---

## Recommendation

### 🏆 Primary Choice: **ContainerLab**

**Rationale**:

1. **Best fit for requirements**: Runs real Ditto binaries in well-isolated containers with full network constraint support
2. **Modern tooling**: Declarative YAML configs, active development, excellent docs
3. **Fastest implementation**: 2-3 days vs 3-5 days for alternatives
4. **Best long-term**: Portable (containers), maintainable (YAML), extensible
5. **Industry adoption**: Growing use in network testing/research
6. **Low risk**: Proven approach (containers + tc/netem)

### 🥈 Backup Choice: **Mininet**

**Use if**:
- Running on Docker Desktop (Mac/Windows) and can't use Linux VM
- Prefer Python API over YAML configs
- Want absolute minimal overhead (no containers)

**Rationale**: Essentially what we planned (namespaces), but with Python wrapper instead of bash. Solid choice, just less modern than ContainerLab.

### ❌ Not Recommended:
- **ns-3 + DCE**: Too complex, high risk, exceeds timeline
- **CORE**: Heavier than needed, smaller community
- **Raw Namespaces**: Too much infrastructure work, error-prone

---

## ContainerLab Implementation Plan

### Phase 1: Proof of Concept (Day 1)
1. Install ContainerLab on Linux machine
2. Create simple `hive-sim-node` Dockerfile with Ditto
3. Test 2-node topology with constraints
4. Validate document sync works between containers

### Phase 2: Army Company Topology (Days 2-3)
1. Create YAML topology for 12-node squad
2. Test bandwidth/latency/loss constraints
3. Implement metrics collection
4. Scale to 112-node company (3 platoons)

### Phase 3: Scenarios (Days 3-4)
1. Baseline scenario (no constraints)
2. Tactical radio constraints (56 Kbps, 50ms, 1% loss)
3. Network partition scenario (split platoons)
4. Analysis and comparison

### Timeline Impact
- **Original plan**: 4-5 days for raw namespaces
- **ContainerLab**: 3-4 days
- **Net savings**: 1-2 days
- **Added benefits**: Better isolation, YAML configs, easier debugging

---

## System Requirements

### ContainerLab
- **OS**: Linux (Ubuntu 20.04+, Fedora, etc.)
  - ⚠️  Docker Desktop (Mac/Windows): Requires Linux VM with tc support
- **Docker**: Docker Engine 20.10+ or Podman
- **Kernel**: Linux 4.9+ with tc/netem modules
- **RAM**: 4GB minimum, 8GB+ recommended for 112 nodes
- **Disk**: 10GB for images and container storage

### Installation
```bash
# Ubuntu/Debian
sudo bash -c "$(curl -sL https://get.containerlab.dev)"

# Verify installation
containerlab version

# Check tc support (required)
which tc && tc qdisc help | grep netem
```

---

## Decision

**Recommendation**: Use **ContainerLab** for E8 network simulation.

**Justification**:
- Fastest path to working simulation (2-3 days)
- Best long-term maintainability (YAML configs)
- Proven scale (hundreds of containers)
- Active community and development
- Modern tooling and excellent documentation

**Next Steps**:
1. Install ContainerLab on development machine
2. Create `hive-sim-node` Docker image with Ditto
3. Test 2-node proof of concept
4. Proceed with full E8.1 implementation

---

## References

- **ContainerLab**: https://containerlab.dev/
- **ContainerLab Link Impairments**: https://containerlab.dev/manual/impairments/
- **Mininet**: http://mininet.org/
- **ns-3**: https://www.nsnam.org/
- **ns-3 DCE**: https://github.com/direct-code-execution/ns-3-dce
- **CORE**: https://coreemu.github.io/core/
- **Linux tc**: https://man7.org/linux/man-pages/man8/tc.8.html
- **Linux netem**: https://man7.org/linux/man-pages/man8/tc-netem.8.html
