# ADR-008: Network Simulation Layer (E8)

**Status:** Proposed
**Date:** 2025-11-04
**Decision Makers:** Research Team
**Technical Story:** E8 - Implement network simulation layer to establish baseline performance metrics and validate protocol behavior under realistic network constraints

## Context and Problem Statement

We have implemented the CAP protocol's core synchronization mechanisms using Ditto (E1-E6) and a differential updates framework (E7). Before optimizing the integration between our protocol-level delta operations and Ditto's document model, we need to:

1. **Establish baseline metrics** for current Ditto performance under realistic network conditions
2. **Validate protocol behavior** across varying network quality (9.6Kbps - 1Mbps, 100ms - 5s latency)
3. **Measure actual bandwidth usage** during cell formation and hierarchical operations
4. **Identify optimization opportunities** through data-driven analysis

**Core Challenge:** How do we simulate realistic tactical network conditions with concrete military unit structures to measure protocol performance before production deployment?

### Reference Use Case: Army Company Echelon

To drive meaningful simulation, we model a standard Army company structure:

**Company (HQ + 3 Platoons) = ~42 nodes total:**
- **1 Company HQ** (command element)
- **3 Platoons** × (1 Platoon HQ + 3 Squads)
  - **Platoon HQ:** 1 leader node
  - **Squad (11 personnel):**
    - 9 dismounted soldiers (sensors, comms)
    - 1 ground robot (UGV - autonomous resupply/ISR)
    - 1-2 UAVs (quadcopter drones for reconnaissance)

**Node Count:**
- Company HQ: 1
- Platoon HQs: 3
- Squads: 9 (3 per platoon)
- Personnel per squad: 9
- Robots per squad: 1 UGV
- Drones per squad: 2 UAVs
- **Total: 1 + 3 + (9 × 12) = 112 nodes**

This structure provides:
- **Hierarchical validation:** Company → Platoon → Squad cells
- **Realistic scale:** 112 nodes (exceeds ADR-001's 100+ target)
- **Mixed capabilities:** Human operators, ground robots, aerial drones
- **Tactical scenarios:** Squad maneuvers, platoon coordination, company-wide operations

## Decision Drivers

### Primary Requirements (from ADR-001)
- **Network Constraints:** 9.6Kbps - 1Mbps bandwidth, 100ms - 5s latency
- **Scalability Target:** 100+ nodes in simulation
- **Packet Loss:** 0-30% configurable
- **Partition Scenarios:** Network split/merge events
- **Observability:** Log all network events for analysis

### Immediate Goals (E8 Scope)
- Establish baseline bandwidth metrics for Ditto's current document sync
- Measure convergence time under network constraints
- Validate that protocol maintains consistency during partitions
- Provide data to drive delta optimization decisions (future work)

### Technical Constraints
- Must work with real Ditto instances (not mocks)
- Must support multiple concurrent simulation scenarios
- Must be deterministic and reproducible
- Must generate metrics compatible with baseline tests (E7)
- **Available resource:** Stout Linux workstation for running 100+ nodes

## Considered Options

### Option 1: Linux Network Namespaces + tc/netem (SELECTED)
Use Linux network namespaces to isolate each node with real network stack, apply tc/netem for traffic shaping.

**Pros:**
- **Extremely realistic** - each node has isolated network stack
- **True process isolation** - each Ditto instance runs in own namespace
- **Battle-tested tools** - tc (traffic control), netem (network emulation)
- **Real IP routing** - can model actual network topologies
- **Packet-level control** - bandwidth, latency, loss, jitter, reordering
- **Partition simulation** - drop routes between namespace groups
- **Scales well** - Linux can handle 100+ namespaces on stout hardware

**Cons:**
- **Requires root/sudo** - namespace creation needs CAP_NET_ADMIN
- **Linux-only** - won't run on macOS (CI limitation)
- **Setup complexity** - need scripts to create/destroy namespaces
- **CI consideration** - need separate lightweight option for CI

**Mitigation:**
- Primary simulation runs on dedicated Linux workstation (has root)
- Provide fallback "mock network" mode for macOS/CI (basic scenarios only)
- Document namespace setup in runbook

### Option 2: Ditto Transport Layer Shaping
Intercept Ditto's transport layer to inject delays/drops.

**Pros:**
- Works with real Ditto sync
- No platform dependencies
- Reproducible

**Cons:**
- Ditto SDK doesn't expose transport layer hooks
- Would require forking/patching Ditto
- Violates "let Ditto do Ditto's goodness"
- Not maintainable across SDK updates

### Option 3: Simulation Harness with Application-Level Delays
Build simulation layer that manages multiple Ditto instances with configurable sync delays (sleep/throttle in application code).

**Pros:**
- Works within Ditto's API boundaries
- Cross-platform (macOS, Linux, CI)
- Deterministic and reproducible
- No special permissions required

**Cons:**
- **Cannot control packet-level behavior** - no loss, jitter, reordering
- **Network effects approximated** - sleep() doesn't model queuing, congestion
- **Less realistic** - doesn't stress OS network stack
- **Defeats purpose** - ADR-001 explicitly requires realistic network validation

### Option 4: Pure Mock Simulation
Build mock CRDT implementation for fast simulation.

**Pros:**
- Very fast
- Fully deterministic
- Easy to test edge cases

**Cons:**
- Doesn't validate real Ditto behavior (ADR-001 explicitly rejected this)
- Won't catch Ditto-specific issues
- Defeats purpose of baseline measurement

## Decision Outcome

**Chosen option:** Shadow Network Simulator (with namespace fallback)

Shadow provides the best balance of realism, scalability, and ease of use for E8. It runs unmodified applications in a simulated network environment with precise control over topology, bandwidth, latency, and packet loss.

**Key Advantages:**
- **Deterministic:** Same seed = identical simulation (critical for CI/CD)
- **Scalable:** Hundreds of thousands of processes on modest hardware
- **No root required:** YAML configs, runs anywhere on Linux
- **Simulated time:** Can run faster than real-time for rapid iteration
- **Written in Rust:** Aligns with our stack, active development

**Platform Requirements:**
- **Linux only:** Ubuntu 22.04/24.04, Debian 11/12/13, Fedora 42
- **Kernel:** 5.10+ (standard on modern distros)
- **Available resource:** Stout Linux workstation meets requirements

**Hybrid Approach:**
- **Primary:** Shadow on Linux workstation (full 112-node company simulation)
- **Fallback:** Linux namespaces if Ditto compatibility issues discovered (POC will test)
- **Development:** Basic in-process scenarios for macOS developers (non-networked testing)

**Critical Unknown:** Ditto SDK compatibility with Shadow's syscall interception requires POC validation (see E8.0 below).

## Architecture

### Component Structure (Shadow Mode - Primary)

```
┌───────────────────────────────────────────────────────────────┐
│                    cap-sim Binary                             │
│  - CLI for running simulations                                │
│  - Generates Shadow YAML configs (Army company structure)     │
│  - Invokes Shadow simulator                                   │
│  - Parses Shadow output for metrics                           │
│  - Generates reports (JSON, text, CSV)                        │
└────────────────┬──────────────────────────────────────────────┘
                 │
                 │ generates
                 ▼
         ┌─────────────────┐
         │ scenario.yaml   │ (Shadow config)
         │                 │
         │ - Network graph │
         │ - Host processes│
         │ - Timing params │
         └────────┬────────┘
                  │
                  │ shadow scenario.yaml
                  ▼
┌─────────────────────────────────────────────────────────────┐
│              Shadow Network Simulator                       │
│  - Intercepts syscalls from all processes                   │
│  - Simulates network: TCP, UDP, routing                     │
│  - Applies bandwidth/latency/loss per config                │
│  - Runs in simulated time (can be faster than real-time)    │
│  - Deterministic execution (seed-based)                     │
└────────────────┬────────────────────────────────────────────┘
                 │
      ┌──────────┴──────────┬──────────────┬──────────────┐
      │                     │              │              │
┌─────▼─────────┐  ┌────────▼────────┐  ┌─▼────────┐  ┌─▼────────┐
│ Virtual Host  │  │  Virtual Host   │  │ Virtual  │  │ Virtual  │
│ soldier_1     │  │  soldier_2      │  │ ugv_1    │  │ uav_1    │
│ ┌───────────┐ │  │ ┌─────────────┐ │  │          │  │          │
│ │cap-sim-node│ │  │ │cap-sim-node │ │  │   ...    │  │   ...    │
│ │+ Ditto    │ │  │ │+ Ditto      │ │  │          │  │  (112    │
│ │+ CAP      │ │  │ │+ CAP        │ │  │          │  │   total) │
│ └───────────┘ │  │ └─────────────┘ │  │          │  │          │
│               │  │                 │  │          │  │          │
│ Simulated NIC │  │ Simulated NIC   │  │          │  │          │
│ - Intercepts  │  │ - Intercepts    │  │          │  │          │
│   socket()    │  │   socket()      │  │          │  │          │
│   send()      │  │   send()        │  │          │  │          │
│   recv()      │  │   recv()        │  │          │  │          │
└───────────────┘  └─────────────────┘  └──────────┘  └──────────┘
        │                   │                 │             │
        └───────────────────┴─────────────────┴─────────────┘
                            │
                  ┌─────────▼──────────┐
                  │ Simulated Network  │
                  │ - Graph topology   │
                  │ - Bandwidth limits │
                  │ - Latency/jitter   │
                  │ - Packet loss      │
                  │ - Routing tables   │
                  └────────────────────┘
```

**Key Components:**
- **cap-sim:** Generates Shadow YAML, invokes `shadow`, collects metrics
- **Shadow:** Runs unmodified cap-sim-node binaries with syscall interception
- **Virtual Hosts:** Each node thinks it's on a real network
- **Simulated Network:** Graph-based topology with precise network characteristics
- **No root required:** Shadow runs as regular user

### Core Components

#### 1. SimulationHarness
**Purpose:** Orchestrates multi-node simulations with network constraints

**Responsibilities:**
- Create and manage N simulated nodes
- Apply network profiles (bandwidth, latency, loss)
- Execute scenario scripts (join, partition, merge)
- Collect and aggregate metrics
- Generate comparison reports

**API:**
```rust
pub struct SimulationHarness {
    nodes: Vec<SimulatedNode>,
    network: NetworkSimulator,
    metrics: MetricsCollector,
}

impl SimulationHarness {
    pub async fn new(config: SimConfig) -> Result<Self>;
    pub async fn add_node(&mut self, node_config: NodeConfig) -> NodeId;
    pub async fn run_scenario(&mut self, scenario: Scenario) -> SimulationResult;
    pub fn get_metrics(&self) -> &MetricsCollector;
}
```

#### 2. SimulatedNode
**Purpose:** Represents a single CAP protocol node with Ditto sync

**Responsibilities:**
- Wraps real Ditto store instance
- Implements CAP protocol logic (discovery, cell formation)
- Tracks node-local metrics (bandwidth, message count)
- Respects network constraints from NetworkSimulator

**API:**
```rust
pub struct SimulatedNode {
    id: NodeId,
    ditto: Arc<Ditto>,
    cell_store: CellStore,
    node_store: NodeStore,
    network_config: NetworkConfig,
    metrics: NodeMetrics,
}

impl SimulatedNode {
    pub async fn new(id: NodeId, network: NetworkConfig) -> Result<Self>;
    pub async fn join_cell(&mut self, cell_id: &str) -> Result<()>;
    pub async fn advertise_capabilities(&mut self, caps: Vec<Capability>) -> Result<()>;
    pub async fn sync_tick(&mut self) -> Result<()>; // Controlled sync
    pub fn metrics(&self) -> &NodeMetrics;
}
```

#### 3. NetworkSimulator
**Purpose:** Models network constraints and partition scenarios

**Responsibilities:**
- Apply bandwidth limits (delay sync operations)
- Inject latency (sleep before/after Ditto operations)
- Simulate packet loss (probabilistic sync failures)
- Control partitions (disable sync between node groups)

**API:**
```rust
pub struct NetworkSimulator {
    profiles: HashMap<NodeId, NetworkProfile>,
    partitions: Vec<PartitionConfig>,
}

pub struct NetworkProfile {
    bandwidth_kbps: u32,        // 9.6 - 1000 Kbps
    latency_ms: u32,            // 100 - 5000 ms
    packet_loss_percent: u8,    // 0 - 30%
    jitter_ms: u32,             // Latency variance
}

impl NetworkSimulator {
    pub fn apply_profile(&mut self, node: NodeId, profile: NetworkProfile);
    pub fn create_partition(&mut self, group_a: Vec<NodeId>, group_b: Vec<NodeId>);
    pub fn heal_partition(&mut self, partition_id: usize);
    pub async fn delay_for_bandwidth(&self, node: NodeId, bytes: usize);
}
```

#### 4. MetricsCollector
**Purpose:** Aggregate and analyze simulation metrics

**Responsibilities:**
- Collect per-node metrics (bandwidth, latency, message count)
- Compute aggregate statistics (mean, p50, p95, p99)
- Compare against baseline and targets (from ADR-001)
- Generate reports (JSON, text, charts)

**API:**
```rust
pub struct MetricsCollector {
    node_metrics: HashMap<NodeId, NodeMetrics>,
    timeline: Vec<MetricSnapshot>,
}

pub struct NodeMetrics {
    bytes_sent: usize,
    bytes_received: usize,
    messages_sent: usize,
    messages_received: usize,
    convergence_time_ms: u64,
    sync_errors: usize,
}

impl MetricsCollector {
    pub fn record_event(&mut self, node: NodeId, event: MetricEvent);
    pub fn snapshot(&mut self) -> MetricSnapshot;
    pub fn report(&self) -> SimulationReport;
}
```

## Implementation Approach

### Phase 0: Shadow + Ditto POC (E8.0) ⚠️ **REQUIRED FIRST**

**Goal:** Validate that Ditto SDK works under Shadow's syscall interception

**This is a GO/NO-GO decision point. Shadow is only viable if Ditto works correctly.**

**Tasks:**
1. Install Shadow on Linux workstation
   ```bash
   # Ubuntu/Debian
   sudo apt-get install shadow

   # Or build from source
   git clone https://github.com/shadow/shadow.git
   cd shadow && cargo build --release
   ```

2. Create minimal test: 2 Ditto nodes syncing
   ```yaml
   # poc-ditto-sync.yaml
   general:
     stop_time: 30s
     seed: 42

   network:
     graph:
       type: 1_gbit_switch  # Simple, fast network

   hosts:
     node1:
       network_node_id: 0
       processes:
         - path: ./target/release/ditto-sync-test
           args: "--node-id node1 --mode server"
           start_time: 1s

     node2:
       network_node_id: 0
       processes:
         - path: ./target/release/ditto-sync-test
           args: "--node-id node2 --mode client"
           start_time: 2s
   ```

3. Build simple Rust binary: `ditto-sync-test`
   - Creates Ditto store
   - Inserts a document
   - Waits for sync to peer
   - Verifies document received
   - Exits with success/failure code

4. Run under Shadow: `shadow poc-ditto-sync.yaml`

5. Analyze results:
   - Did both processes start?
   - Did Ditto sync work?
   - Were documents exchanged?
   - Any errors in Shadow output?

**Success Criteria (GO):**
- ✅ Both Ditto instances start successfully
- ✅ Documents sync between nodes
- ✅ No syscall errors in Shadow logs
- ✅ Deterministic (same seed = same result)

**Failure Criteria (NO-GO):**
- ❌ Ditto crashes or hangs under Shadow
- ❌ Syscall errors (unsupported functions)
- ❌ Sync doesn't work (documents don't propagate)
- ❌ Non-deterministic behavior

**If POC Succeeds:** Proceed with Phase 1 (Shadow-based implementation)

**If POC Fails:** Fall back to Option 1 (Linux namespaces + tc/netem)

**Estimated Time:** 1-2 days

---

### Phase 1: Shadow Harness (E8.1)
**Goal:** Run multi-node simulation with Shadow + real Ditto sync

**Prerequisites:** E8.0 POC succeeded (Ditto works under Shadow)

**Tasks:**
- Implement Shadow YAML generator for scenarios
- Build `cap-sim-node` binary (CAP protocol + Ditto)
- Create simple scenario: Squad formation (12 nodes)
- Run under Shadow, collect metrics

**Success Criteria:**
- 12 nodes successfully form a squad cell
- All nodes converge to same CellState
- Shadow metrics show bandwidth/latency
- Results are deterministic (reproducible with same seed)

### Phase 2: Network Constraints (E8.2)
**Goal:** Apply bandwidth/latency constraints

**Tasks:**
- Implement NetworkSimulator with profiles
- Add delay injection for bandwidth limits
- Add latency injection (sleep before sync)
- Run constrained scenario: 5 nodes, 56Kbps, 1s latency

**Success Criteria:**
- Convergence time increases predictably with latency
- Bandwidth limits are respected (measured vs expected)
- Protocol remains consistent under constraints

### Phase 3: Partition Scenarios (E8.3)
**Goal:** Validate CRDT consistency during network splits

**Tasks:**
- Implement partition creation/healing
- Run partition scenario: split 5 nodes into 2 groups, make changes, heal
- Verify eventual consistency after heal

**Success Criteria:**
- Changes made during partition are preserved
- States merge correctly after healing
- No data loss or conflicts

### Phase 4: Baseline Comparison (E8.4)
**Goal:** Generate baseline report for delta optimization decisions

**Tasks:**
- Run full scenario suite (5, 10, 25, 50 nodes)
- Compare against E7 baseline measurements
- Identify bandwidth hotspots
- Document optimization opportunities

**Success Criteria:**
- Baseline report shows actual bandwidth usage vs delta potential
- Clear data-driven recommendations for optimization
- Metrics validate or challenge ADR-001 targets

## Scenarios

### Scenario 1: Squad Formation (12 nodes)
**Purpose:** Baseline cell formation with realistic squad structure

**Setup:**
- 9 dismounted soldiers
- 1 UGV (ground robot)
- 2 UAVs (drones)
- Network: Unconstrained (baseline)

**Steps:**
1. All 12 nodes start, discover each other
2. Form squad cell, elect squad leader (likely human soldier)
3. UGV advertises resupply + ISR capabilities
4. UAVs advertise aerial reconnaissance capabilities
5. Soldiers advertise sensor/comms capabilities
6. Measure convergence time and bandwidth

**Expected Metrics:**
- Convergence time: <5s (ADR-001 target)
- Total bandwidth: ~12 nodes × 10 ops × 477 bytes = ~57KB
- Message count: O(n log n) = ~36-48 messages

### Scenario 2: Platoon Formation (39 nodes)
**Purpose:** Hierarchical cell formation with 3 squads + platoon HQ

**Setup:**
- 1 Platoon HQ
- 3 Squads (12 nodes each from Scenario 1)
- Network: 56Kbps, 500ms latency, 5% loss (tactical radio)

**Steps:**
1. Form 3 squad cells independently (parallel)
2. Squad leaders join platoon cell
3. Platoon HQ aggregates squad capabilities
4. Measure hierarchical convergence time

**Expected Metrics:**
- Squad formation: <10s each (degraded from constraint)
- Platoon formation: <15s total
- Hierarchical latency: <5s for squad→platoon propagation
- Bandwidth per squad: ~150KB over 15s window
- No message loss despite 5% packet loss (CRDT resilience)

### Scenario 3: Company Formation (112 nodes)
**Purpose:** Full-scale company simulation with 3 platoons + company HQ

**Setup:**
- 1 Company HQ
- 3 Platoons (39 nodes each from Scenario 2)
- 9 Squads total
- Network: Mixed constraints
  - Intra-squad: 100Kbps, 100ms (local mesh)
  - Squad-Platoon: 56Kbps, 500ms (JTRS radio)
  - Platoon-Company: 19.2Kbps, 1s (SATCOM)

**Steps:**
1. Form 9 squad cells (parallel)
2. Form 3 platoon cells (squad leaders join)
3. Form company cell (platoon leaders join)
4. Measure end-to-end convergence time
5. Test capability query from Company HQ: "Which squads have UAV ISR?"

**Expected Metrics:**
- Total convergence time: <30s (realistic for company-wide)
- Message count: O(n log n) = ~500-700 messages (not 12,544 if O(n²))
- Bandwidth usage: <10% of available (ADR-001 target)
- Hierarchical query response: <5s

### Scenario 4: Network Partition - Lost Comms
**Purpose:** Validate CRDT consistency when platoon loses contact with company

**Setup:**
- Start with full company (112 nodes)
- Partition: Platoon 2 (39 nodes) isolated from rest

**Steps:**
1. Establish company formation (Scenario 3)
2. Partition network: Platoon 2 isolated for 5 minutes
3. During partition:
   - Platoon 2: Squad 4 UGV goes offline (capability loss)
   - Company HQ: Reassign mission to Platoon 1
4. Heal partition
5. Verify state convergence

**Expected Metrics:**
- Partition detected within 10s (heartbeat timeout)
- Changes preserved during partition (CRDT)
- Convergence after heal: <30s
- Conflict resolution: Automatic (CRDT merge)
- No data loss: Both changes visible after heal

### Scenario 5: Bandwidth Saturation Test
**Purpose:** Validate protocol behavior when network saturates

**Setup:**
- 1 Platoon (39 nodes)
- Network: 19.2Kbps (SATCOM baseline), 2s latency
- Trigger: Rapid capability updates (UAV stream starts)

**Steps:**
1. Form platoon cell
2. All 6 UAVs start streaming ISR data (capability updates)
3. Measure convergence time under saturation

**Expected Metrics:**
- Protocol degrades gracefully (no deadlock)
- Priority-based delivery: Critical updates arrive first
- Staleness: 90% of updates within 30s (ADR-001)
- TTL prevents unbounded queue growth

### Scenario 6: Scale Validation (Variable Company Sizes)
**Purpose:** Validate O(n log n) complexity claim empirically

**Setup:**
- Run company formation with: 12, 39, 112, 200 nodes
- Network: Unconstrained baseline

**Steps:**
1. Form hierarchical structure for each scale
2. Measure message count vs node count
3. Plot log-log to verify sub-quadratic

**Expected Metrics:**
- 12 nodes (1 squad): ~36 messages
- 39 nodes (1 platoon): ~150 messages
- 112 nodes (1 company): ~650 messages
- 200 nodes (notional battalion): ~1400 messages
- Regression fit: O(n^1.2) or better (not O(n²))

## Metrics and Success Criteria

### Performance Metrics
- **Convergence Time:** Time for all nodes to reach consistent state
  - Target: <5s for Priority 1 updates (ADR-001)
- **Bandwidth Usage:** Total bytes transmitted per scenario
  - Target: <10% of available bandwidth (ADR-001)
- **Message Count:** Number of messages vs node count
  - Target: O(n log n) scaling, not O(n²)
- **Staleness:** Time until capability updates visible to all nodes
  - Target: <30s for 90% of updates (ADR-001)

### Reliability Metrics
- **Partition Tolerance:** Consistency maintained during/after partitions
  - Target: 100% eventual consistency
- **Error Rate:** Sync failures under constraints
  - Target: <1% message loss acceptable
- **Recovery Time:** Time to converge after partition heal
  - Target: <10s for small networks (5-10 nodes)

### Comparison Metrics
- **Baseline vs Delta:** Current bandwidth vs E7 projected savings
  - E7 Projection: 79-89% reduction possible
- **Actual vs Target:** Measured performance vs ADR-001 requirements
  - Identify gaps requiring optimization

## Integration with Existing Work

### E7 Delta Framework
- Simulation harness will use existing baseline tests as reference
- Metrics format compatible with E7 baseline measurements
- Data will inform delta optimization priorities

### E2E Testing Harness
- Reuse `E2EHarness` patterns for Ditto instance management
- Extend with network constraint capabilities
- Maintain observer-based sync validation approach

### cap-sim Binary
- CLI interface for running scenarios
- Configuration files for network profiles
- Output formats: JSON (CI), text (human), CSV (analysis)

## Linux Namespace Implementation Details

### Namespace Setup Script
```bash
#!/bin/bash
# create-network-sim.sh - Sets up namespace environment

# 1. Create bridge for mesh connectivity
sudo ip link add br-capsim type bridge
sudo ip addr add 10.1.1.254/24 dev br-capsim
sudo ip link set br-capsim up

# 2. For each node (1 to 112):
for i in {1..112}; do
  # Create namespace
  sudo ip netns add capsim-node$i

  # Create veth pair
  sudo ip link add veth-node$i type veth peer name veth-node${i}-br

  # Attach one end to bridge
  sudo ip link set veth-node${i}-br master br-capsim
  sudo ip link set veth-node${i}-br up

  # Move other end to namespace
  sudo ip link set veth-node$i netns capsim-node$i

  # Configure inside namespace
  sudo ip netns exec capsim-node$i ip addr add 10.1.1.$i/24 dev veth-node$i
  sudo ip netns exec capsim-node$i ip link set veth-node$i up
  sudo ip netns exec capsim-node$i ip link set lo up
  sudo ip netns exec capsim-node$i ip route add default via 10.1.1.254

  # Apply tc/netem rules (example: 56Kbps, 500ms latency, 5% loss)
  sudo ip netns exec capsim-node$i tc qdisc add dev veth-node$i root netem \
    rate 56kbit \
    delay 500ms 50ms \
    loss 5%
done
```

### Running Nodes in Namespaces
```bash
# Launch a single node inside namespace
sudo ip netns exec capsim-node1 \
  /path/to/cap-sim-node \
  --node-id soldier-1-1 \
  --role soldier \
  --capabilities sensor,comms
```

### Cleanup Script
```bash
#!/bin/bash
# teardown-network-sim.sh

# Delete all namespaces
for i in {1..112}; do
  sudo ip netns del capsim-node$i 2>/dev/null || true
done

# Delete bridge
sudo ip link del br-capsim 2>/dev/null || true
```

### Network Profiles by Echelon
```rust
// cap-sim/src/network_profiles.rs
pub struct NetworkProfile {
    pub bandwidth_kbps: u32,
    pub latency_ms: u32,
    pub jitter_ms: u32,
    pub loss_percent: u8,
}

impl NetworkProfile {
    pub fn intra_squad() -> Self {
        Self {
            bandwidth_kbps: 100,    // Local mesh (WiFi/Bluetooth)
            latency_ms: 100,
            jitter_ms: 20,
            loss_percent: 1,
        }
    }

    pub fn squad_to_platoon() -> Self {
        Self {
            bandwidth_kbps: 56,     // JTRS tactical radio
            latency_ms: 500,
            jitter_ms: 100,
            loss_percent: 5,
        }
    }

    pub fn platoon_to_company() -> Self {
        Self {
            bandwidth_kbps: 19,     // SATCOM
            latency_ms: 1000,
            jitter_ms: 200,
            loss_percent: 10,
        }
    }
}
```

## Risks and Mitigations

### Risk: Namespace Overhead on Linux Workstation
**Concern:** 112 namespaces + Ditto instances may overwhelm resources

**Mitigation:**
- Profile single node memory/CPU usage first
- Start with small scenarios (12, 39 nodes), scale up
- Monitor `htop`, set resource limits with cgroups
- Estimated requirement: 16GB RAM, 8+ cores for 112 nodes

### Risk: Network Simulation Accuracy
**Concern:** tc/netem may not perfectly model tactical networks

**Mitigation:**
- Focus on relative comparisons, not absolute predictions
- Validate against real deployment data (when available)
- Document known limitations (e.g., netem doesn't model radio interference)
- Good enough to validate protocol behavior

### Risk: Root Access Required
**Concern:** Need sudo for namespace creation

**Mitigation:**
- Primary simulations run on dedicated Linux workstation (controlled environment)
- Document security considerations (run in VM/container if needed)
- Fallback mode for developers without root (in-process simulation)

### Risk: CI/CD Compatibility
**Concern:** Namespace tests won't run on macOS or containerized CI

**Mitigation:**
- Linux namespace tests are **manual** or **dedicated Linux CI runner**
- Provide lightweight in-process mode for unit/integration tests
- Full simulation is validation tool, not gatekeeper for PR merges

## Open Questions

1. **Simulation Time Model:** Should we use real time or simulated time?
   - **Recommendation:** Real time for Phase 1, consider simulated time for scale tests

2. **Node Behavior:** Should nodes follow scripted or autonomous behavior?
   - **Recommendation:** Scripted for reproducibility, add autonomous mode later

3. **Ditto Configuration:** What Ditto sync settings optimize for simulation?
   - **Action:** Experiment with sync intervals, batch sizes

4. **Metrics Storage:** Where do we store historical simulation results?
   - **Recommendation:** JSON files in `cap-sim/results/`, gitignore large files

## References

- ADR-001: CAP Protocol POC Architecture (network requirements)
- ADR-002: Beacon Storage Architecture (Ditto integration patterns)
- E7 Baseline Tests: `cap-protocol/tests/baseline_ditto_bandwidth_e2e.rs`
- E2E Harness: `cap-protocol/src/testing/e2e_harness.rs`
- Testing Strategy: `docs/TESTING_STRATEGY.md`

## Success Criteria

E8 is complete when:

1. ✅ Simulation harness runs 5+ nodes with real Ditto sync
2. ✅ Network constraints (bandwidth, latency) are applied and measured
3. ✅ Partition scenario validates CRDT consistency
4. ✅ Baseline report compares current vs delta-optimized performance
5. ✅ Data-driven recommendations documented for optimization
6. ✅ All scenarios run in CI without manual intervention
7. ✅ Metrics format enables comparison across scenarios and time

## Next Steps After E8

1. **E9: Delta Optimization** (informed by E8 data)
   - Implement `apply_operation()` integration with Ditto
   - Measure actual bandwidth improvement
   - Validate against E8 baseline

2. **E10: Hierarchical Operations** (requires E8 validation)
   - Implement parent-child cell relationships
   - Measure latency through hierarchy
   - Validate <5s propagation requirement

3. **Production Deployment** (requires E8 + E9 + E10)
   - Real hardware validation
   - Field testing under actual tactical conditions
