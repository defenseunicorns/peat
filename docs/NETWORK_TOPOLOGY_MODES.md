# Network Topology Modes

**Status**: Validated
**Date**: November 2024

## Overview

HIVE Protocol network validation supports three topology modes to validate different aspects of hierarchical coordination:

1. **Client-Server**: Simple validation (all → central server)
2. **Hub-Spoke**: Realistic hierarchical structure (orchestrated static)
3. **Dynamic Mesh**: Autonomous discovery (goal state)

Each mode tests progressively more complex behavior while using the same containerlab infrastructure.

---

## Mode 1: Client-Server (Simple Validation)

### Description
All nodes connect to a single central server (squad leader). This is the simplest topology for validating basic sync functionality.

### Configuration
```yaml
soldier-1:
  env:
    MODE: writer
    TCP_LISTEN: "12345"
    # No TCP_CONNECT - acts as server

soldier-2...soldier-9, ugv-1, uav-1, uav-2:
  env:
    MODE: reader
    TCP_LISTEN: "12346"  # Unique port per node
    TCP_CONNECT: "soldier-1:12345"  # All connect to soldier-1
```

### Network Topology
```
        soldier-1 (server)
       /    |    |    \
      /     |    |     \
   s-2    s-3  ugv-1  uav-1
   s-4    s-5  uav-2
   s-6    s-7
   s-8    s-9
```

### What This Tests
- ✅ Basic Ditto sync functionality
- ✅ ContainerLab infrastructure
- ✅ Document propagation (writer → readers)
- ✅ Network constraints impact
- ✅ Baseline bandwidth metrics

### Limitations
- **Single point of failure**: If soldier-1 fails, no sync
- **Not realistic**: Real squads don't have central servers
- **Worst-case bandwidth**: All traffic through one node
- **No hierarchy**: Doesn't test HIVE protocol's hierarchical design

### When to Use
- Initial infrastructure validation
- Quick smoke tests
- Baseline performance metrics
- Debugging sync issues

### File
`topologies/squad-12node-client-server.yaml`

---

## Mode 2: Hub-Spoke (Hierarchical Static)

### Description
Realistic military squad structure with squad leader, team leaders, and relay nodes (UGV). Connections are statically configured to match tactical hierarchy.

### Configuration
```yaml
# Squad Leader (Hub)
soldier-1:
  env:
    MODE: writer
    TCP_LISTEN: "12345"

# Fire Team 1 (connects to squad leader)
soldier-2, soldier-3, soldier-4, soldier-5:
  env:
    MODE: reader
    TCP_CONNECT: "soldier-1:12345"

# Team Leader 2 (Sub-hub)
soldier-6:
  env:
    MODE: reader
    TCP_LISTEN: "12350"
    TCP_CONNECT: "soldier-1:12345"

# Fire Team 2 (connects to team leader)
soldier-7, soldier-8, soldier-9:
  env:
    MODE: reader
    TCP_CONNECT: "soldier-6:12350"

# UGV (Relay - connects to both leaders)
ugv-1:
  env:
    MODE: reader
    TCP_CONNECT: "soldier-1:12345,soldier-6:12350"

# UAVs (Aerial relay)
uav-1:
  env:
    TCP_CONNECT: "soldier-1:12345"
uav-2:
  env:
    TCP_CONNECT: "soldier-6:12350"
```

### Network Topology
```
         UAV-1
          |
      soldier-1 (squad leader)
      /    |    \
     /     |     \
  s-2   s-3,4,5  UGV-1
                  |    \
            soldier-6   \
             (team ldr)  \
            /   |   \     \
          s-7  s-8  s-9  UAV-2
```

### What This Tests
- ✅ Hierarchical sync patterns (ADR-009)
- ✅ Multi-hop propagation (s-9 → s-6 → s-1)
- ✅ Relay behavior (UGV as comm hub)
- ✅ O(n log n) message complexity
- ✅ Realistic tactical structure
- ✅ Network constraint impact on hierarchy

### Advantages
- **Realistic**: Matches actual squad organization
- **Hierarchical**: Tests HIVE protocol's intended design
- **Resilient**: Multiple paths (UGV provides redundancy)
- **Scalable pattern**: Extends to platoon/company
- **Measurable**: Can track sync through hierarchy

### What This Tests Specifically
1. **Hierarchical Convergence**: Time for document to reach all nodes through hierarchy
2. **Message Complexity**: Count messages (should be O(n log n), not O(n²))
3. **Relay Efficiency**: UGV as comm hub vs direct connections
4. **Constraint Impact**: How latency/bandwidth affects multi-hop sync

### When to Use
- Validating hierarchical protocol design
- Measuring O(n log n) scaling
- Testing realistic tactical scenarios
- Establishing performance baselines for company scale

### File
`topologies/squad-12node-hub-spoke.yaml`

---

## Mode 3: Dynamic Mesh (Autonomous Discovery)

### Description
Fully autonomous peer discovery with no pre-configured connections. Nodes discover each other dynamically and form a mesh network. This is the **goal state** for HIVE protocol.

### Configuration (Target)
```yaml
soldier-1...soldier-9, ugv-1, uav-1, uav-2:
  env:
    MODE: autonomous  # No writer/reader distinction
    # No TCP_LISTEN or TCP_CONNECT
    # Discovery via mDNS/broadcast/beacon
```

### Network Topology
```
    Dynamic mesh - changes based on:
    - Proximity (simulated via network constraints)
    - Node availability
    - Beacon broadcasts
    - Neighbor discovery
```

### What This Tests
- ✅ Autonomous cell formation
- ✅ Dynamic topology adaptation
- ✅ Partition/heal scenarios
- ✅ Leader election
- ✅ Capability-based routing
- ✅ Beacon protocol
- ✅ Real HIVE protocol behavior

### Implementation Options

#### Option A: Ditto LAN Discovery (mDNS)
**Approach**: Use Ditto SDK's built-in mDNS discovery

**Requirements**:
1. Enable multicast in ContainerLab network
2. Configure Ditto for LAN transport:
   ```rust
   ditto.set_transport_config(TransportConfig {
       peer_to_peer: PeerToPeer::LAN,  // mDNS
       enable_sync_with_big_peer: false,
   });
   ```
3. No explicit TCP configuration needed

**Status**: **INVESTIGATING** - Does mDNS work in Docker/ContainerLab?

#### Option B: UDP Broadcast Discovery
**Approach**: Broadcast beacon messages on local network

**Requirements**:
1. Implement beacon broadcast in `cap_sim_node.rs`
2. Listen for beacons from peers
3. Establish TCP connections to discovered peers

**Status**: Requires code changes (blocked by Issue #45)

#### Option C: Full HIVE Protocol
**Approach**: Implement complete HIVE protocol with beacons, discovery, cell formation

**Requirements**:
1. Beacon protocol (ADR-003)
2. Discovery protocol
3. Cell formation logic
4. Leader election
5. Hierarchical routing

**Status**: Future work (post-Issue #45)

### When to Use
- Production-ready testing
- Autonomous behavior validation
- Partition/heal scenarios
- Full protocol testing

### File
`topologies/squad-12node-dynamic.yaml` (future)

---

## Implementation Status

| Mode | Status | File | Blocks |
|------|--------|------|--------|
| Mode 1: Client-Server | 🟡 Ready to implement | `squad-12node-client-server.yaml` | None |
| Mode 2: Hub-Spoke | 🟡 Ready to implement | `squad-12node-hub-spoke.yaml` | None |
| Mode 3: Dynamic | 🔴 Investigating | `squad-12node-dynamic.yaml` | mDNS feasibility |

---

## Discovery Mechanisms: Deep Dive

### Question: Can we use mDNS with ContainerLab?

**Investigation needed**:

1. **Docker Multicast Support**
   - Does ContainerLab's Docker network support multicast?
   - Can we enable it with network configuration?
   - Does it require host network mode?

2. **Ditto LAN Discovery**
   - Does Ditto's mDNS work in containers?
   - What ports/protocols does it use?
   - Does it require special network configuration?

3. **Alternative: DNS-SD**
   - Can we use DNS service discovery?
   - Does ContainerLab provide DNS for containers?
   - Can we register services dynamically?

### Test Plan

1. **Test mDNS in ContainerLab** (2 hours)
   - Deploy 2-node topology
   - Enable Ditto LAN discovery
   - Check if peers discover each other
   - Monitor mDNS traffic with tcpdump

2. **Test UDP Broadcast** (2 hours)
   - Send broadcast packets from containers
   - Check if other containers receive
   - Measure latency and reliability

3. **Document Findings** (1 hour)
   - What works out of the box?
   - What requires configuration?
   - What's blocked by limitations?

---

## Scaling to Company (112 nodes)

### Mode 1: Client-Server
❌ **Not Recommended** - Single server can't handle 112 connections

### Mode 2: Hub-Spoke (Hierarchical)
✅ **Recommended** - Natural extension:
```
Company HQ (root)
  ├─ Platoon 1 HQ
  │   ├─ Squad 1 Leader → 9 soldiers, 1 UGV, 2 UAVs
  │   ├─ Squad 2 Leader → 9 soldiers, 1 UGV, 2 UAVs
  │   └─ Squad 3 Leader → 9 soldiers, 1 UGV, 2 UAVs
  ├─ Platoon 2 HQ
  │   └─ ...
  └─ Platoon 3 HQ
      └─ ...
```

### Mode 3: Dynamic Mesh
✅ **Goal State** - Scales naturally with discovery

---

## Makefile Integration

```makefile
# Mode 1: Client-Server
sim-deploy-squad-simple:
	containerlab deploy -t topologies/squad-12node-client-server.yaml

# Mode 2: Hub-Spoke
sim-deploy-squad-hierarchical:
	containerlab deploy -t topologies/squad-12node-hub-spoke.yaml

# Mode 3: Dynamic (future)
sim-deploy-squad-dynamic:
	containerlab deploy -t topologies/squad-12node-dynamic.yaml
```

---

## Success Criteria

### Mode 1 Complete
- [x] Topology YAML created
- [ ] All nodes sync with central server
- [ ] Baseline metrics collected
- [ ] Document propagation < 5s

### Mode 2 Complete
- [ ] Hierarchical topology YAML created
- [ ] Multi-hop sync working (s-9 → s-6 → s-1)
- [ ] O(n log n) message count validated
- [ ] UGV relay behavior confirmed

### Mode 3 Complete
- [ ] mDNS feasibility determined
- [ ] Dynamic discovery working
- [ ] Autonomous mesh formation
- [ ] Partition/heal scenarios tested

---

## References

- **Issue**: #52 (E8 Phase 1)
- **ADR-008**: Network Simulation Layer
- **ADR-009**: Bidirectional Hierarchical Flows
- **Ditto Docs**: Transport configuration
- **ContainerLab Docs**: Network configuration

---

## Next Steps

1. **Immediate** (NOW):
   - Investigate mDNS in ContainerLab
   - Test Ditto LAN discovery
   - Document findings

2. **Phase 1 Completion**:
   - Implement Mode 1 (client-server)
   - Implement Mode 2 (hub-spoke)
   - Collect baseline metrics

3. **Future** (Post-Issue #45):
   - Implement Mode 3 (dynamic mesh)
   - Full HIVE protocol testing
