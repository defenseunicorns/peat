# ADR-023: Peer Discovery Architecture - Beacon-Based vs DHT

**Status**: Accepted
**Date**: 2025-11-22
**Deciders**: Kit Plummer, Claude Code
**Related**: ADR-017 (P2P Mesh Management)

## Context

HIVE requires peer discovery mechanisms to enable nodes to find and connect to each other in the P2P mesh network. Discovery needs vary by operational scale:

### Tactical/Local Scale (meters to kilometers)
- Squad members discovering squad leader
- Platoon elements coordinating within local area
- Rapid response to topology changes
- **Geographic proximity is critical**

### Strategic/Wide-Area Scale (kilometers to global)
- Cross-region coordination
- Remote command element access
- Disaster recovery/failover
- **Logical connectivity matters more than physical proximity**

### Requirements
1. **Local Discovery**: Fast, low-latency discovery based on geographic proximity
2. **Resilience**: No single point of failure
3. **Minimal Overhead**: Tactical networks have bandwidth constraints
4. **Optional Global Discovery**: Support for wide-area scenarios without impacting local performance
5. **P2P Principles**: Align with standard distributed systems terminology and patterns

## Decision

We adopt a **multi-tier discovery architecture**:

### Tier 1: Beacon-Based Local Discovery (Primary - Implemented)

Use geographic beacon broadcasts for local/tactical discovery:

**Mechanism**:
- Nodes periodically broadcast `GeographicBeacon` messages containing:
  - Node ID
  - Position (lat/lon + geohash)
  - Hierarchy level (Platform, Squad, Platoon, Company)
  - Capabilities/resources
  - TTL (default: 30 seconds)
- Beacons stored in local `BeaconStorage` (memory or persistent)
- `BeaconObserver` filters by geohash prefix for proximity matching
- `PeerSelector` chooses optimal peer based on:
  - Geographic distance
  - Hierarchy level compatibility
  - Node capabilities (static vs mobile, resources)

**When to Use**:
- ✅ Tactical operations (< 10km radius)
- ✅ Real-time coordination
- ✅ Mobile units
- ✅ Bandwidth-constrained environments

**Trade-offs**:
- ✅ Fast discovery (near real-time)
- ✅ Geographic proximity guaranteed
- ✅ Simple implementation
- ❌ Limited to broadcast range
- ❌ Doesn't scale to global/wide-area

### Tier 2: DHT-Based Global Discovery (Future - Reserved)

**Option for wide-area scenarios** using Kademlia DHT (libp2p-kad):

**Mechanism**:
- Distributed hash table for global peer lookups
- XOR distance metric for routing (logical, not geographic)
- O(log n) lookup performance
- Self-healing on peer churn

**When to Use**:
- ✅ Wide-area operations (> 10km)
- ✅ Cross-region coordination
- ✅ Disaster recovery (finding distant peers)
- ✅ Content distribution (maps, intel)

**Trade-offs**:
- ✅ Global reach
- ✅ Scalable to thousands of nodes
- ✅ Resilient to churn
- ❌ Higher latency than beacons
- ❌ Uses logical distance (not geographic)
- ❌ Additional complexity

### Integration Strategy

Both tiers can coexist:

```rust
// Local discovery (default, always active)
let beacon_observer = BeaconObserver::new(storage, local_geohash);
let peer_selector = PeerSelector::new(config, position, hierarchy_level);

// Global discovery (optional, configured per deployment)
#[cfg(feature = "global-discovery")]
let kad_dht = libp2p_kad::Kademlia::new(peer_id, store);
```

**Discovery Flow**:
1. **Local First**: Check beacon storage for nearby peers
2. **Global Fallback**: If no local peers found, query DHT (if enabled)
3. **Caching**: Cache DHT results in local beacon storage

## Rationale

### Why Beacon-Based for Primary Discovery?

1. **Geographic Proximity Matters**: Military operations require coordination with physically nearby units. Beacons use actual lat/lon distance.

2. **Tactical Performance**: Sub-second discovery enables rapid topology formation critical for tactical scenarios.

3. **Simplicity**: No external dependencies, no global state, works offline.

4. **Bandwidth Efficient**: Periodic broadcasts (every 30s) with local storage minimize network overhead.

5. **Aligns with P2P Principles**: Beacons are broadcast to local peers, not to a central server.

### Why Reserve DHT for Future?

1. **Not Needed for Phase 1**: Initial tactical use cases are local (<10km)

2. **Complexity**: DHT adds routing, key management, and maintenance overhead

3. **Can Add Later**: libp2p-kad can be integrated without changing beacon architecture

4. **Clear Use Cases**: Wide-area disaster recovery, cross-region ops justify future addition

### Why Not DHT-Only?

DHT uses **XOR distance** (logical), not **geographic distance**:
- A peer with XOR distance 1 could be on the other side of the world
- For tactical mesh, we need the nearest *physical* peer, not *logical* peer
- Beacons guarantee geographic proximity

## Consequences

### Positive

1. **Fast Local Discovery**: <1 second to discover nearby peers
2. **Geographic Accuracy**: Peer selection based on actual physical proximity
3. **Tactical Suitability**: Optimized for squad/platoon coordination
4. **P2P Nomenclature**: Clean peer-based terminology (not parent/child)
5. **Future-Proof**: Can add DHT tier without breaking existing code

### Negative

1. **Limited Range**: Beacon broadcasts don't scale to global
2. **No Cross-Region Discovery**: Requires DHT for wide-area
3. **Potential Future Refactor**: If DHT added, need discovery abstraction

### Neutral

1. **Feature Flag for DHT**: `global-discovery` feature for optional DHT
2. **Discovery Abstraction Needed**: When adding DHT, create `PeerDiscovery` trait

## Implementation

### Current (Phase 1)
```
hive-mesh/src/beacon/
├── broadcaster.rs      # Periodic beacon broadcasts
├── observer.rs         # Local beacon filtering
└── storage.rs          # In-memory/persistent storage

hive-mesh/src/topology/
├── selection.rs        # PeerSelector (geographic + hierarchy)
├── builder.rs          # TopologyBuilder (peer selection events)
└── manager.rs          # TopologyManager (connection lifecycle)
```

### Future (Phase 2 - Optional)
```
hive-mesh/src/discovery/
├── mod.rs              # PeerDiscovery trait (abstraction)
├── beacon.rs           # BeaconDiscovery (Tier 1)
└── dht.rs              # DhtDiscovery (Tier 2, optional)
    └── uses libp2p-kad
```

## Alternatives Considered

### 1. DHT-Only Discovery
**Rejected**: XOR distance doesn't guarantee geographic proximity needed for tactical mesh.

### 2. mDNS Discovery
**Considered but Limited**: Works well on local networks but doesn't cross subnets. Beacons with geohash work across tactical radio networks.

### 3. Central Discovery Server
**Rejected**: Single point of failure, not P2P, requires infrastructure.

### 4. Gossip Protocol
**Deferred**: Useful for state dissemination, but beacons already provide peer discovery. Gossip may be added later for CRDT sync.

## Related Work

- **BitTorrent**: Uses DHT (Mainline) for global peer discovery + local peer exchange (PEX)
- **IPFS**: Uses Kademlia DHT + mDNS for local + mdns for local
- **Ethereum**: Kademlia DHT for global node discovery
- **Polkadot**: libp2p-kad + mDNS

HIVE follows similar pattern: **Local discovery (beacons) + optional global discovery (DHT)**.

## References

- ADR-017: P2P Mesh Management and Discovery
- libp2p-kad: https://docs.rs/libp2p-kad/
- Kademlia Paper: https://pdos.csail.mit.edu/~petar/papers/maymounkov-kademlia-lncs.pdf
- Geohash: https://en.wikipedia.org/wiki/Geohash

## Decision Log

- **2025-11-22**: Documented beacon-based discovery as primary mechanism
- **2025-11-22**: Reserved DHT as future enhancement for wide-area scenarios
- **2025-11-22**: Established multi-tier discovery architecture
