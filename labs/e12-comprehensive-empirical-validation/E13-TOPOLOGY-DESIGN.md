# E13 Topology Design: Connection-Limited Mesh

**Date**: 2025-11-11
**Purpose**: Fair evaluation of Ditto CRDT+P2P mesh performance
**Replaces**: E12 forced full-mesh topologies

## Design Principles

### 1. Realistic MANET Conditions

Mimic real Trellisware/MANET deployments:
- **Connection limits**: 3-5 peers per node (not n-1)
- **Sparse mesh**: O(n) total connections, not O(n²)
- **Multi-hop sync**: Test gossip propagation
- **Ditto manages topology**: Don't force specific connections

### 2. Fair Comparison

Enable apples-to-apples comparison:
- Keep Traditional baseline unchanged (client-server)
- Test CAP with realistic Ditto mesh (not forced full-mesh)
- Validate hierarchical aggregation benefits with proper CRDT backend

### 3. Validation Criteria

Ensure tests measure CRDT performance:
- ✅ Ditto's native mesh management enabled
- ✅ Connection limits enforced (3-5 peers max)
- ✅ Sparse mesh topology validated
- ✅ Multi-hop gossip propagation tested

## Topology Approaches

### Approach A: Pure mDNS/LAN Discovery (Ideal but Risky)

**Configuration**:
```yaml
env:
  TCP_LISTEN: ""        # No TCP
  TCP_CONNECT: ""       # No forced connections
  ENABLE_MDNS: "true"   # Let Ditto discover peers
```

**Pros**:
- Most realistic - Ditto manages everything
- No artificial constraints
- Tests Ditto as intended

**Cons**:
- mDNS may not work in Docker/ContainerLab
- Unpredictable peer discovery timing
- Hard to debug connection issues

**Verdict**: Try first, but have backup plan

### Approach B: Connection-Limited Seed Peers (Recommended)

**Configuration**:
```yaml
# Example: 24-node platoon
# Each node gets 3-5 seed peers for initial connectivity
# Ditto gossips to remaining 19-21 nodes via multi-hop

squad-alpha-leader:
  env:
    TCP_LISTEN: "12346"
    # Connect to: platoon leader (1) + 2 other squad leaders (2) = 3 peers
    TCP_CONNECT: "platoon-leader:12345,squad-bravo-leader:12353,squad-charlie-leader:12361"
    ENABLE_MDNS: "false"  # Explicit TCP only (deterministic)

alpha-soldier-1:
  env:
    TCP_LISTEN: "12347"
    # Connect to: squad leader (1) + 2 squad peers (2) = 3 peers
    TCP_CONNECT: "squad-alpha-leader:12346,alpha-soldier-2:12348,alpha-uav-1:12351"
    ENABLE_MDNS: "false"
```

**Connection Limits**:
- Leaders: 3-4 peers (other leaders + selective members)
- Members: 3 peers (leader + 2 squad peers)
- Total: ~3n connections (sparse mesh)

**Pros**:
- Realistic connection limits (3-5 peers)
- Deterministic topology (repeatable tests)
- Tests multi-hop gossip (most nodes not directly connected)
- Works reliably in ContainerLab

**Cons**:
- Still using explicit TCP (not pure Ditto mesh)
- Manual topology design required

**Verdict**: RECOMMENDED - Best balance of realism and determinism

### Approach C: Ring Topology (Simple but Unrealistic)

**Configuration**:
```yaml
# Each node connects to 2-3 neighbors in a ring
node-N:
  TCP_CONNECT: "node-(N-1):port,node-(N+1):port"
```

**Pros**:
- Guaranteed connectivity
- Minimal connections (2-3 per node)
- Easy to implement

**Cons**:
- Artificial topology (not how meshes form)
- Higher latency (average hop count = n/4)
- Doesn't test Ditto's peer selection

**Verdict**: Fallback only if Approach B fails

## Recommended E13 Test Matrix

### Scale Testing (Connection-Limited Mesh)

| Test Name | Nodes | Connections/Node | Total Connections | Topology |
|-----------|-------|------------------|-------------------|----------|
| cap-mesh-12node | 12 | 3-4 | ~40 | Squad (1 leader + 11 members) |
| cap-mesh-24node | 24 | 3-4 | ~80 | Platoon (4 leaders + 20 members) |
| cap-mesh-48node | 48 | 3-5 | ~180 | Company (hierarchical) |
| cap-mesh-96node | 96 | 3-5 | ~360 | Battalion (hierarchical) |

### Bandwidth Testing (24-node Mesh)

| Bandwidth | Purpose |
|-----------|---------|
| 1gbps | Baseline (no congestion) |
| 100mbps | Moderate constraint |
| 1mbps | Severe constraint (tactical radio) |
| 256kbps | Minimum viable (legacy radio) |

### Comparison Matrix

| Architecture | Topology Type | Connections/Node | Avg Hops |
|--------------|---------------|------------------|----------|
| Traditional | Client-Server | 1 (to server) | 1 |
| CAP Mesh (E13) | Connection-Limited | 3-5 | 2-3 |
| CAP Hierarchical (E13) | Connection-Limited | 3-5 | 2-3 |

## Implementation Steps

### 1. Update DittoStore Configuration

Add support for mDNS/LAN transport:

```rust
// ditto_store.rs
pub struct DittoConfig {
    // ... existing fields ...
    pub enable_mdns: bool,  // NEW: Enable mDNS/LAN discovery
    pub connection_limit: Option<usize>,  // NEW: Max peers (if supported by Ditto)
}

impl DittoStore {
    pub fn new(config: DittoConfig) -> Result<Self> {
        // ...
        ditto.update_transport_config(|transport_config| {
            // Option 1: Pure mDNS
            if config.tcp_listen_port.is_none() && config.tcp_connect_address.is_none() {
                transport_config.peer_to_peer.lan.enabled = config.enable_mdns;
                transport_config.peer_to_peer.bluetooth_le.enabled = false;
            }
            // Option 2: Limited TCP + mDNS
            else {
                // Configure limited seed peers
                // ... existing TCP setup ...

                // Keep mDNS enabled for additional discovery?
                transport_config.peer_to_peer.lan.enabled = config.enable_mdns;
            }
        });
        // ...
    }
}
```

### 2. Create New Topology Files

Create connection-limited variants:

- `squad-12node-mesh-limited.yaml` (3-4 peers per node)
- `platoon-24node-mesh-limited.yaml` (3-4 peers per node)
- `platoon-48node-mesh-limited.yaml` (3-5 peers per node)
- `battalion-96node-mesh-limited.yaml` (3-5 peers per node)

### 3. Connection Design Patterns

#### Squad (12 nodes)

```
Leader: 3 connections (3 members)
Members: 3 connections each (leader + 2 peers)
Total: ~36 connections (vs 132 in full mesh)
```

#### Platoon (24 nodes)

```
Platoon Leader: 3 connections (3 squad leaders)
Squad Leaders: 4 connections (platoon leader + 2 other leaders + 1 member)
Members: 3 connections (leader + 2 peers)
Total: ~80 connections (vs 552 in full mesh)
```

#### Company (48 nodes)

```
Company Commander: 4 connections (4 platoon leaders)
Platoon Leaders: 4 connections (commander + 3 other leaders)
Squad Leaders: 4 connections (platoon leader + 2 squad leaders + 1 member)
Members: 3 connections (leader + 2 peers)
Total: ~180 connections (vs 2,256 in full mesh)
```

### 4. Validation Tests

Before running full E13 suite, validate:

1. **Connection count**: Verify each node has ≤5 TCP connections
2. **Mesh connectivity**: All nodes can reach all peers (via multi-hop)
3. **Gossip propagation**: Documents sync across non-connected nodes
4. **Latency distribution**: Should NOT be bimodal (no connection thrashing)

## Success Criteria for E13

### Topology Validation

- ✅ No node has >5 direct connections
- ✅ Total connections = O(n), not O(n²)
- ✅ All nodes achieve full document sync (via gossip)
- ✅ Multi-hop propagation working correctly

### Performance Validation

- ✅ P90/P99 latencies reflect CRDT convergence, not connection overhead
- ✅ Latency scales gracefully with node count (not exponentially)
- ✅ Sparse mesh performs better than E12 forced full-mesh
- ✅ Hierarchical aggregation reduces traffic (vs full mesh)

### Comparison to E12

Expected improvements:
- **Lower P90/P99 latencies**: No TCP connection thrashing
- **Better scaling**: O(n) connections vs O(n²)
- **Higher throughput**: Less redundant sync traffic
- **More stable**: No head-of-line blocking across 23 connections

## Timeline

1. **Design Phase**: Document approach (DONE)
2. **Implementation**: Create new topologies (1-2 days)
3. **Validation**: Test connection limits, gossip (1 day)
4. **E13 Execution**: Run full test suite (2-3 hours)
5. **Analysis**: Compare E13 vs E12 results (1 day)
6. **ADR Update**: Revise conclusions based on fair test (1 day)

## Next Steps

1. Decide on Approach A (mDNS) vs Approach B (limited seed peers)
2. Implement chosen approach in new topology files
3. Update cap_sim_node.rs to support new configuration
4. Run validation tests to verify sparse mesh
5. Execute E13 comprehensive suite

---

**Bottom Line**: E13 will test Ditto under realistic MANET conditions with connection limits, enabling fair evaluation of CRDT+P2P mesh performance.
