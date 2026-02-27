# ADR-016: TTL and Data Lifecycle Management Abstraction

**Status**: Accepted
**Date**: 2025-01-10
**Authors**: Kit (with Codex)
**Supersedes**: None
**Related ADRs**: [ADR-001](001-peat-protocol-poc.md) (CRDT-based state), [ADR-002](002-beacon-storage-architecture.md) (Ditto storage), [ADR-011](011-ditto-vs-automerge-iroh.md) (Backend abstraction)

## Context

PEAT Protocol operates in disconnected tactical edge environments where nodes may be offline for hours or days. Managing document lifecycle in these environments presents fundamental challenges rooted in the CAP theorem:

### The Distributed Deletion Problem

**Delete-Then-Disconnect Scenario**:
```
Time 0: Node A deletes document D1
Time 1: Node A goes offline (deployment to disconnected area)
Time 2: Node B (offline since Time -1) comes online with stale copy of D1
Time 3: Stale D1 replicates to Node C
Result: Deleted document "resurrects" across the mesh
```

**Concurrent Delete-Update (Husking)**:
```
Time 0: Node A updates D1.position = {lat: 10, lon: 20}
Time 0: Node B deletes D1
Time 1: CRDTs merge
Result: "Husked document" with some fields non-null, others null
```

**Storage Constraints on Tactical Edge**:
```
Tactical edge device: 256MB storage
Deletion metadata: ~1KB per tombstone
Extended disconnection: 7 days
Beacon update rate: 5 seconds
Potential tombstones: 120,000+ (exceeds storage)
```

### Requirements for Lifecycle Management

1. **Partition Tolerance**: Nodes MUST operate correctly when disconnected
2. **Eventual Consistency**: Deletions MUST propagate reliably even to long-offline nodes
3. **Storage Efficiency**: Tactical edge devices have strict storage constraints
4. **Mission Safety**: Incorrect deletion semantics can compromise autonomous operations
5. **Backend Agnostic**: Solution must work across Ditto, Automerge, Yjs, and future CRDTs

## Decision

We adopt a **three-tier data lifecycle abstraction** that separates deletion semantics from CRDT backend implementation:

### Tier 1: Soft-Delete (Application-Level)

**Pattern**: Mark documents with `_deleted` flag instead of removing them.

**Use Cases**: High-churn data subject to concurrent updates
- Beacon heartbeats (5-second updates)
- Position telemetry (1-second updates)
- Capability advertisements (frequent changes)

**Rationale**: Prevents husking by keeping document structure intact during concurrent updates and deletes.

**Backend Requirement**: All CRDT backends MUST support field-level updates.

```rust
// Backend-agnostic soft-delete interface
pub trait SoftDelete {
    async fn soft_delete(&self, collection: &str, doc_id: &str) -> Result<()>;
    async fn cleanup_soft_deleted(&self, collection: &str, ttl_seconds: u64) -> Result<usize>;
}
```

### Tier 2: Hard Delete with Tombstone Propagation (CRDT-Level)

**Pattern**: Create deletion tombstones that sync across the mesh for a configurable TTL window.

**Use Cases**: Permanent removal of sensitive or obsolete data
- Expired mission plans
- Decommissioned node configurations
- Revoked credentials

**Rationale**: Ensures deletions propagate reliably to nodes offline during deletion, preventing resurrection.

**Backend Requirement**: All CRDT backends MUST support:
1. Tombstone creation and synchronization
2. Configurable tombstone TTL (default: 168 hours / 7 days)
3. Automatic tombstone reaping after TTL expiration

```rust
// Backend-agnostic tombstone configuration interface
pub trait TombstoneConfig {
    async fn configure_tombstone_ttl(
        &self,
        ttl_hours: u32,
        enabled: bool,
    ) -> Result<()>;
}
```

### Tier 3: Local Eviction (Storage Optimization)

**Pattern**: Remove documents from local storage WITHOUT creating tombstones.

**Use Cases**: Edge device storage management
- Evict old beacons to free space (will re-sync from peers if needed)
- Maintain "keep last N" policies on resource-constrained devices
- Storage pressure thresholds (e.g., evict when >90% full)

**Rationale**: Allows tactical edge devices with 256MB storage to operate indefinitely without mesh-wide deletion propagation overhead.

**Backend Requirement**: All CRDT backends MUST support local-only document removal that does NOT propagate deletions to peers.

```rust
// Backend-agnostic eviction interface
pub trait LocalEviction {
    async fn evict_local(&self, collection: &str, doc_id: &str) -> Result<()>;
    async fn evict_oldest(&self, collection: &str, limit: usize) -> Result<usize>;
}
```

## Architecture

### TTL Configuration Abstraction

Backend-agnostic configuration that maps to CRDT-specific mechanisms:

```rust
pub struct TtlConfig {
    /// Tombstone TTL (hours) - backend implements via native mechanisms
    pub tombstone_ttl_hours: u32,
    pub tombstone_reaping_enabled: bool,

    /// Soft-delete TTLs (application-enforced)
    pub beacon_ttl: Duration,          // 5 minutes
    pub position_ttl: Duration,        // 10 minutes
    pub capability_ttl: Duration,      // 2 hours

    /// Local eviction strategy (backend implements storage queries)
    pub evict_strategy: EvictionStrategy,
    pub offline_policy: Option<OfflineRetentionPolicy>,
}

pub enum EvictionStrategy {
    OldestFirst,                       // Evict by document age
    StoragePressure { threshold_pct: u8 }, // Evict when storage >X%
    KeepLastN(usize),                  // Maintain fixed-size rolling window
    None,
}

pub struct OfflineRetentionPolicy {
    pub online_ttl: Duration,          // TTL when connected (e.g., 10 min)
    pub offline_ttl: Duration,         // TTL when disconnected (e.g., 60 sec)
    pub keep_last_n: usize,            // Minimum to retain regardless of TTL
}
```

### Configuration Presets for Tactical Environments

```rust
impl TtlConfig {
    /// Tactical edge: Short-lived data, aggressive cleanup, 7-day tombstones
    pub fn tactical() -> Self {
        Self {
            tombstone_ttl_hours: 168,  // 7 days
            beacon_ttl: Duration::from_secs(300),      // 5 min
            position_ttl: Duration::from_secs(600),    // 10 min
            capability_ttl: Duration::from_secs(7200), // 2 hours
            evict_strategy: EvictionStrategy::OldestFirst,
            offline_policy: Some(OfflineRetentionPolicy {
                online_ttl: Duration::from_secs(600),  // 10 min
                offline_ttl: Duration::from_secs(60),  // 1 min
                keep_last_n: 10,
            }),
        }
    }

    /// Long-duration operations: Extended retention, 30-day tombstones
    pub fn long_duration() -> Self { /* ... */ }

    /// Offline node: Aggressive cleanup for storage constraints
    pub fn offline_node() -> Self { /* ... */ }
}
```

## Backend Implementation Requirements

### Ditto Implementation (Reference)

- **Tombstone TTL**: `ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = <value>`
- **Soft-Delete**: DQL `UPDATE collection SET _deleted = true WHERE _id = :id`
- **Hard Delete**: DQL `DELETE FROM collection WHERE _id = :id` (creates tombstone)
- **Local Eviction**: DQL `EVICT FROM collection WHERE _id = :id` (no tombstone)

See [TTL_AND_DATA_LIFECYCLE_DESIGN.md](../TTL_AND_DATA_LIFECYCLE_DESIGN.md) for Ditto-specific implementation details.

### Automerge Implementation (Future)

- **Tombstone TTL**: Must implement via custom tombstone tracking in `SyncState`
- **Soft-Delete**: Set `_deleted: true` in document map
- **Hard Delete**: Use Automerge's deletion API with custom tombstone metadata
- **Local Eviction**: Remove from local `DocHandle` without broadcasting delete change

### Yjs Implementation (Future)

- **Tombstone TTL**: Track deleted doc IDs in Y.Map with expiration timestamps
- **Soft-Delete**: Set `_deleted: true` in Y.Map
- **Hard Delete**: Use `Y.Map.delete()` with custom tombstone Y.Map
- **Local Eviction**: Remove from local state without syncing delete operation

## Consequences

### Positive

1. **Backend Agnostic**: Abstraction works across Ditto, Automerge, Yjs, and future CRDTs
2. **Mission Safety**: Three-tier approach prevents both resurrection and husking
3. **Storage Efficient**: Edge devices can operate indefinitely with local eviction
4. **Testable**: Clear interfaces enable unit testing independent of backend
5. **Configurable**: Presets adapt to different operational contexts (tactical vs long-duration)

### Negative

1. **Complexity**: Three deletion mechanisms increase cognitive load
2. **Backend Burden**: Each CRDT backend must implement all three tiers
3. **Soft-Delete Overhead**: Requires application code to filter `_deleted = true` in queries
4. **Tombstone Storage**: 7-day window can accumulate significant tombstones (mitigated by local eviction)

### Risks and Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Backend doesn't support tombstones | Deletion resurrection | Document requirement in sync abstraction trait; reject backends that can't guarantee |
| Soft-delete not filtered in queries | Zombie data visible | Add `WHERE _deleted != true` to query helpers; lint rule to enforce |
| Eviction removes needed data | Re-sync overhead | Conservative defaults (keep_last_n: 10); storage pressure thresholds (>90%) |
| Tombstone TTL too short | Resurrection of deleted data | Enforce minimum 168 hours (7 days) for tactical edge; warn if exceeded |
| Tombstone TTL too long | Storage exhaustion | Monitor storage; automatic eviction triggers; alert at 80% capacity |

## Validation

### Test Scenarios

1. **Delete-Then-Disconnect**: Node A deletes, goes offline; Node B (offline) comes back → deletion must propagate via tombstone
2. **Concurrent Delete-Update**: Node A updates field; Node B soft-deletes → no husking (document intact with `_deleted = true`)
3. **Storage Exhaustion**: Tactical edge device reaches 90% storage → automatic eviction of oldest beacons
4. **Tombstone Expiration**: After 7 days, tombstones must be reaped automatically

### Monitoring Requirements

Applications using this abstraction MUST monitor:
- `tombstone_count` (gauge) - Current tombstone count
- `soft_deleted_count` (gauge) - Documents with `_deleted = true`
- `storage_bytes_used` (gauge) - Current storage utilization
- `eviction_count` (counter) - Local evictions triggered
- `deletion_resurrection_count` (counter) - Deleted docs that reappeared (critical failure metric)

## References

- [TTL and Data Lifecycle Design](../TTL_AND_DATA_LIFECYCLE_DESIGN.md) - Ditto-specific implementation details
- [ADR-001: PEAT Protocol POC](001-peat-protocol-poc.md) - CRDT-based state management decision
- [ADR-002: Beacon Storage Architecture](002-beacon-storage-architecture.md) - Ditto storage patterns
- [ADR-011: Ditto vs Automerge/Iroh](011-ditto-vs-automerge-iroh.md) - Backend abstraction requirements
- [Ditto DELETE vs EVICT](https://docs.ditto.live/sdk/latest/crud/delete) - Ditto deletion semantics

## Decision Log

**2025-01-10**: Initial decision - Three-tier lifecycle abstraction (soft-delete, hard delete with tombstones, local eviction) with backend-agnostic interfaces. Default tactical preset: 168-hour tombstone TTL, 5-minute beacon soft-delete TTL, oldest-first eviction strategy.
