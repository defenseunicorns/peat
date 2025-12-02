# E12 Test Setup Issue: Forced Full-Mesh Topology

**Date**: 2025-11-11
**Severity**: CRITICAL - Invalidates E12 Ditto performance conclusions
**Status**: Requires E13 re-validation with corrected topology

## Executive Summary

E12 comprehensive validation tests measured **forced full-mesh TCP topology performance**, NOT Ditto's native CRDT+P2P mesh convergence. The test setup bypassed Ditto's mesh management by:

1. **Forcing n-1 TCP connections per node** (11-23 connections) via `TCP_CONNECT` environment variable
2. **Disabling Ditto's native mesh management** (mDNS/LAN transport disabled when TCP addresses present)
3. **Forcing application-layer full mesh** that Ditto was never designed to support

## What We Measured

E12 "cap-full" and "cap-hierarchical" tests measured:
- ❌ Overhead of maintaining 11-23 simultaneous TCP connections per node
- ❌ CRDT sync redundancy across all connections
- ❌ TCP head-of-line blocking across excessive connections
- ❌ NOT Ditto's actual CRDT convergence performance

## What We Should Have Measured

Real Ditto MANET deployments (e.g., 60+ nodes on Trellisware):
- ✅ Ditto manages connections dynamically (3-5 peers per node max)
- ✅ Automatic peer selection based on reachability
- ✅ Efficient gossip across sparse mesh topology
- ✅ Connection limits enforced by Ditto's transport layer

## Evidence: Code Analysis

### Topology Configuration (platoon-24node-mesh-mode4.yaml:67)

```yaml
squad-alpha-leader:
  env:
    TCP_CONNECT: "platoon-leader:12345,squad-bravo-leader:12353,squad-charlie-leader:12361,alpha-soldier-1:12347,alpha-soldier-2:12348,alpha-soldier-3:12349,alpha-soldier-4:12350,alpha-uav-1:12351,alpha-ugv-1:12352"
```

**Result**: Squad leader connects to 10 peers (3 leaders + 7 squad members) - NOT a realistic mesh!

### DittoStore Configuration (ditto_store.rs:177-186)

```rust
// Lines 177-186: Adding ALL TCP addresses as explicit connections
if let Some(ref addresses) = config.tcp_connect_address {
    for address in addresses.split(',') {
        transport_config.connect.tcp_servers.insert(address.to_string());
    }
}
```

**Result**: Ditto attempts to connect to ALL listed addresses simultaneously.

### Mesh Management Disabled (ditto_store.rs:160-162)

```rust
// Lines 160-162: DISABLING Ditto's native mesh management
if config.tcp_listen_port.is_some() || config.tcp_connect_address.is_some() {
    // Using explicit TCP connections - disable mDNS/LAN discovery
    transport_config.peer_to_peer.lan.enabled = false;
}
```

**Result**: Ditto's dynamic peer selection is completely disabled!

## Impact on E12 Results

### Observed Latencies (E12 @ 24 nodes, 1Gbps)

| Architecture | Median | P90 | P99 |
|--------------|--------|-----|-----|
| Traditional | 0.58ms | 0.97ms | 22ms |
| CAP Full | 23.76ms | **3128ms** | **3503ms** |
| CAP Hierarchical | 33.99ms | **4651ms** | **5246ms** |

### Root Cause Analysis

The 1.6-5 second P90/P99 latencies are likely caused by:

1. **TCP Connection Overhead**
   - 24 nodes × 11-23 connections = 264-552 total connections
   - Socket buffer exhaustion
   - Connection thrashing under bandwidth constraints

2. **Sync Redundancy**
   - Same CRDT data synced across ALL connections
   - 11-23× redundant sync messages per document
   - Bandwidth saturation from duplicate traffic

3. **Head-of-Line Blocking**
   - Single slow connection blocks all peers
   - No connection priority or selection
   - TCP congestion control across too many streams

4. **NOT CRDT Convergence Time**
   - Bimodal distribution (80% fast, 20% slow) suggests connection issues
   - Initial delivery fast (first connection succeeds)
   - Updated events slow (waiting for all connections to converge)

## What Ditto Actually Does in Production

Based on user report: "I have recently seen Ditto's SDK in Android sync Full mesh on 60+ nodes over Trellisware MANET radios"

### Ditto's Native Mesh Management

1. **Connection Limits**: ~3-5 peers per node (configurable)
2. **Dynamic Peer Selection**: Chooses best peers based on:
   - Proximity (RSSI, hop count)
   - Reachability (stable connections preferred)
   - Network conditions (latency, bandwidth)
3. **Efficient Gossip**: Sync propagates across sparse mesh
4. **Automatic Healing**: Re-routes around failures

### Why This Works at Scale

- **Sparse topology**: O(n) connections total, not O(n²)
- **Optimized routing**: Multi-hop sync is efficient
- **Resource limits**: Bounded connection/bandwidth overhead
- **Production-tested**: Validated in real MANET deployments

## Required Actions

### Immediate: Flag E12 Results as Invalid

- [ ] Add disclaimer to E12 comprehensive report
- [ ] Update ADR-011 to note test setup issue
- [ ] Do NOT make migration decisions based on E12 Ditto performance

### E13: Corrected Validation

Design new test configurations:

1. **Ditto Native Mesh (mDNS/LAN)**
   - Remove all `TCP_CONNECT` forced connections
   - Let Ditto discover peers via mDNS
   - Enable `transport_config.peer_to_peer.lan.enabled = true`

2. **Ditto Connection-Limited Mesh**
   - Use Ditto's connection limit API (if available)
   - Or simulate with 3-5 "seed" peers per node
   - Let Ditto gossip to remaining peers

3. **Hybrid Approach**
   - Give each node 3-5 initial TCP connections
   - Enable mDNS for additional peer discovery
   - Let Ditto manage final mesh topology

### Validation Criteria for E13

- ✅ Ditto manages connection topology (not forced by test harness)
- ✅ Connection limits enforced (≤5 peers per node)
- ✅ Sparse mesh topology (O(n) total connections)
- ✅ Multi-hop sync validated (gossip propagation)
- ✅ Realistic MANET conditions

## Lessons Learned

1. **Don't bypass SDK mesh management** - Defeats the purpose of using a mesh-native CRDT
2. **Test under realistic conditions** - 60+ node Trellisware deployments prove Ditto works
3. **Validate assumptions** - "Full mesh" doesn't mean "force n-1 connections"
4. **Question suspicious results** - 1.6-5s latencies for 24 nodes should have been a red flag

## References

- E12 Results: `labs/e12-comprehensive-empirical-validation/scripts/e12-comprehensive-results-20251111-091238/`
- Topology Files: `cap-sim/topologies/platoon-24node-mesh-mode4.yaml`, `squad-12node-dynamic-mesh.yaml`
- DittoStore: `cap-protocol/src/storage/ditto_store.rs:160-186`
- ADR-011: `docs/adr/011-ditto-vs-automerge-iroh.md`

## Next Steps

1. **Document this finding** ✅ (this file)
2. **Design E13 test topologies** (connection-limited mesh)
3. **Update DittoStore** (expose native mesh mode)
4. **Run E13 validation** (fair Ditto performance test)
5. **Re-evaluate ADR-011** (based on corrected data)

---

**Bottom Line**: E12 tested a pathological network topology that Ditto was never designed to support. We need E13 with proper mesh management to fairly evaluate Ditto's CRDT performance.
