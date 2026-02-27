# ADR-002: Beacon Storage Architecture for Geographic Discovery

**Status**: Accepted
**Date**: 2025-10-29
**Decision Makers**: PEAT Protocol Team
**Related**: E3.1 Geographic Self-Organization

## Context

The PEAT protocol's geographic discovery system requires nodes to continuously broadcast their position and status as "beacons" across a Ditto mesh network. Each node must be able to discover nearby nodes to autonomously form cells during the discovery phase.

Two architectural approaches were considered for beacon storage:

1. **One Document Per Node**: Each node maintains its own beacon document (`node_beacons/{node_id}`)
2. **Single Shared Document**: All nodes update a single shared document (`beacons/current` with nested node data)

## Decision

**We will use one document per node for beacon storage.**

Each node will maintain its own beacon document in the `node_beacons` collection:

```rust
// Collection: "node_beacons"
// Document ID: node_id
{
  "_id": "node_alpha",
  "position": {
    "lat": 37.7749,
    "lon": -122.4194,
    "alt": 100.0
  },
  "geohash_cell": "9q8yyk8",
  "operational": true,
  "timestamp": 1698765432,
  "capabilities": ["sensor", "comms"],
  "_ttl": 30  // Ditto auto-removal after 30 seconds
}
```

### Beacon Lifecycle

1. **Creation**: Node starts → creates beacon document with TTL
2. **Update**: Node moves → updates beacon with new position/timestamp
3. **Expiration**: Node stops updating → Ditto removes document after TTL
4. **Discovery**: Other nodes query beacons by geohash proximity
5. **Janitor**: Each node runs periodic cleanup of stale in-memory cache

## Consequences

### Positive

1. **Write Scalability**: Each node writes only to its own document
   - No write conflicts between platforms
   - N nodes can update simultaneously without contention
   - Simple LWW-Register CRDT semantics per document

2. **Query Efficiency**: Geohash-based spatial queries
   ```rust
   // Find nearby beacons
   ditto.store()
       .collection("node_beacons")
       .find("geohash_cell == $0", my_geohash)
       .exec()
   ```

3. **Selective Sync**: Ditto can filter beacons by proximity
   - Only sync beacons from nearby platforms
   - Reduced bandwidth in large-scale deployments
   - Better DDIL (Denied/Degraded/Intermittent/Limited) resilience

4. **Fault Isolation**: Corrupted beacon doesn't affect other platforms
   - Clear ownership boundaries
   - Independent failure modes

5. **Automatic Cleanup**: Ditto TTL handles ghost platforms
   - Node crashes → beacon expires automatically
   - No manual cleanup required at mesh level
   - Prevents stale data accumulation in distributed store

6. **Independent Lifecycle**: Each beacon has clear ownership
   - Node controls its own data
   - Clean deletion when node decommissions
   - No coordination required for removal

### Negative

1. **Document Count**: N nodes = N documents
   - Slight overhead in Ditto metadata
   - Acceptable tradeoff for scalability

2. **Query Complexity**: Must query collection vs. single document lookup
   - Minimal impact with proper indexing
   - Geohash queries are highly efficient

### Neutral

1. **Two-Layer TTL Strategy**:
   - **Ditto Layer**: Document TTL prevents mesh accumulation
   - **Memory Layer**: Local janitor cleans in-memory cache
   - Provides defense-in-depth against stale data

## Implementation Details

### Ditto Integration (E3.4)

```rust
use dittolive_ditto::prelude::*;

pub struct BeaconBroadcaster {
    ditto: Arc<Ditto>,
    node_id: String,
}

impl BeaconBroadcaster {
    pub fn broadcast_beacon(&self, beacon: &GeographicBeacon) -> Result<()> {
        self.ditto.store()
            .collection("node_beacons")
            .upsert_with_id(&self.node_id, |doc| {
                doc.set("position", serde_json::to_value(&beacon.position)?)?;
                doc.set("geohash_cell", &beacon.geohash_cell)?;
                doc.set("operational", beacon.operational)?;
                doc.set("timestamp", beacon.timestamp)?;
                doc.set("capabilities", &beacon.capabilities)?;
                doc.set("_ttl", 30)?; // Auto-expire after 30 seconds
                Ok(())
            })?;
        Ok(())
    }

    pub fn observe_nearby_beacons(
        &self,
        geohash: &str,
        discovery: Arc<Mutex<GeographicDiscovery>>
    ) -> Result<()> {
        self.ditto.store()
            .collection("node_beacons")
            .find(&format!("geohash_cell == '{}'", geohash))
            .observe(move |docs, _event| {
                let mut discovery = discovery.lock().unwrap();
                for doc in docs {
                    let beacon = parse_beacon(doc)?;
                    discovery.process_beacon(beacon);
                }
                Ok(())
            })?;
        Ok(())
    }
}
```

### Local Janitor Service

```rust
pub struct BeaconJanitor {
    discovery: Arc<Mutex<GeographicDiscovery>>,
    interval: Duration,
}

impl BeaconJanitor {
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(self.interval);
        loop {
            interval.tick().await;
            let mut discovery = self.discovery.lock().unwrap();
            discovery.cleanup_expired();
        }
    }
}

// Start janitor
let janitor = BeaconJanitor::new(discovery.clone(), Duration::from_secs(10));
tokio::spawn(async move {
    janitor.run().await;
});
```

## Alternatives Considered

### Single Shared Document

**Structure**:
```json
{
  "_id": "current",
  "nodes": {
    "node_alpha": { /* beacon data */ },
    "node_bravo": { /* beacon data */ }
  }
}
```

**Rejected Because**:
- Write conflicts: Every node update touches same document
- Document size grows linearly with fleet (problematic at scale)
- Must sync entire document even for single node update
- Complex Map CRDT with nested structure
- No selective sync by proximity
- Deletion complexity (tombstones required)
- Performance degradation with large fleets

## References

- [Ditto TTL Documentation](https://docs.ditto.live/concepts/document-ttl)
- [Ditto Query Language](https://docs.ditto.live/concepts/dql)
- PEAT Protocol Specification: Discovery Phase (E3.1)
- Swarm Robotics Patterns: Decentralized State Management

## Notes

- TTL value (30s) is configurable but balances:
  - Responsiveness: Detect offline nodes quickly
  - Network efficiency: Reduce unnecessary re-broadcasts
  - DDIL tolerance: Account for intermittent connectivity

- Geohash precision 7 (~153m cells) provides good clustering granularity:
  - Fine enough for tactical cell formation
  - Coarse enough to avoid excessive fragmentation

- Future consideration: Dynamic TTL based on node velocity
  - Fast-moving nodes: shorter TTL (more frequent updates)
  - Stationary nodes: longer TTL (reduce bandwidth)
