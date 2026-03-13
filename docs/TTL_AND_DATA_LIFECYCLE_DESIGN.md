# TTL and Data Lifecycle Management - Ditto Implementation

> **Architectural Decision**: See [ADR-016: TTL and Data Lifecycle Abstraction](adr/016-ttl-and-data-lifecycle-abstraction.md) for backend-agnostic design rationale and requirements for Automerge, Yjs, and future CRDT implementations.

## Overview

This document describes the **Ditto-specific implementation** of the TTL and data lifecycle management strategy defined in ADR-016. It focuses on how to leverage Ditto's built-in deletion and tombstone mechanisms to satisfy the three-tier lifecycle abstraction:

1. **Soft-Delete**: Application-level `_deleted` flag (Tier 1)
2. **Hard Delete**: Ditto tombstone propagation via DELETE (Tier 2)
3. **Local Eviction**: Ditto EVICT for storage optimization (Tier 3)

## Distributed Deletion Challenges

### The CAP Theorem and Deletion in Disconnected Environments

Deleting data in a distributed, potentially disconnected environment presents unique challenges that stem from the CAP theorem:

1. **Partition Tolerance Requirement**: Tactical edge devices MUST continue operating when disconnected from the mesh
2. **Eventual Consistency**: Deletions must propagate reliably even when nodes are offline for extended periods  
3. **Tombstone Synchronization**: Deleted documents must be tracked across the mesh to prevent resurrection

### Key Architectural Challenges

#### 1. **Delete-Then-Disconnect Problem**
\`\`\`
Time 0: Node A deletes document D1
Time 1: Node A goes offline
Time 2: Node B (offline since Time -1) comes online with stale copy of D1
Time 3: Stale D1 replicates to Node C
Result: Deleted document "resurrects" across the mesh
\`\`\`

**Ditto's Solution**: Tombstones persist and sync for \`TOMBSTONE_TTL_HOURS\` (default 7 days), ensuring deletions propagate even to nodes that were offline during the delete operation.

#### 2. **Concurrent Delete-Update (Husking)**
\`\`\`
Time 0: Node A updates D1.position = {lat: 10, lon: 20}
Time 0: Node B deletes D1
Time 1: Nodes sync  
Result: "Husked document" where updated fields exist but all others are null
\`\`\`

**Mitigation**: Peat Protocol uses soft-delete patterns for high-churn data (beacons, positions) to avoid husking issues.

#### 3. **Storage Constraints on Edge Devices**
\`\`\`
Tactical edge device: 256MB storage
Tombstones: ~1KB each
7-day TTL = potentially 10,000s of tombstones accumulating
\`\`\`

**Solution**: Shorter tombstone TTL on tactical edge (configurable), with EVICT for aggressive local cleanup while preserving mesh integrity.

### Ditto's Deletion Model

Ditto provides two complementary deletion mechanisms:

#### DELETE: System-Wide Permanent Removal
- Creates **tombstone** (compressed document ID + deletion metadata)
- Tombstone syncs across all peers within \`TOMBSTONE_TTL_HOURS\` window
- After TTL expires, tombstones are automatically reaped
- **Non-recoverable**: Document cannot be restored after deletion

#### EVICT: Local-Only Storage Optimization
- Removes document from **local** device storage only
- Document persists on all remote peers
- Useful for edge devices with storage constraints
- **Recoverable**: Document will re-sync from peers if needed

## Ditto Tombstone TTL Configuration

### Runtime Configuration via ALTER SYSTEM

Ditto provides 5 system properties for tombstone management:

\`\`\`sql
-- 1. Enable automatic tombstone eviction
ALTER SYSTEM SET TOMBSTONE_TTL_ENABLED = true

-- 2. Set tombstone expiration (hours)
ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = 168  -- 7 days (Edge SDK default)

-- 3. Reaping frequency (days between cleanup scans)
ALTER SYSTEM SET DAYS_BETWEEN_REAPING = 1

-- 4. Enable preferred hour scheduling
ALTER SYSTEM SET ENABLE_REAPER_PREFERRED_HOUR_SCHEDULING = true

-- 5. Schedule reaping for off-peak hours (0-23)
ALTER SYSTEM SET REAPER_PREFERRED_HOUR = 3  -- 3 AM local time
\`\`\`

### Environment Variable Configuration

**Important**: Scheduling parameters must be set via environment variables **before** starting Ditto (runtime changes not respected):

\`\`\`bash
export TOMBSTONE_TTL_ENABLED=true
export TOMBSTONE_TTL_HOURS=168
export DAYS_BETWEEN_REAPING=1  
export ENABLE_REAPER_PREFERRED_HOUR_SCHEDULING=true
export REAPER_PREFERRED_HOUR=3
\`\`\`

### Critical Constraint: Edge vs Server TTL

**⚠️ Never set Edge SDK TTL > Server TTL**

- **Edge SDK default**: 7 days (168 hours)
- **Ditto Cloud default**: 30 days (720 hours)
- **Violation consequence**: Tombstones sent back to server after removal → resource exhaustion

### Default Settings

- **Tombstone reaper**: Enabled by default (all SDKs)
- **Reaping frequency**: Once per day
- **TTL**: 7 days (Edge), 30 days (Cloud)

## Implementation Strategies

### 1. Soft-Delete Pattern (Beacons, Positions)

Avoids husking by marking documents as deleted rather than removing them:

\`\`\`rust
// Soft-delete: Update document with _deleted flag
let query = format!(
    "UPDATE beacons SET _deleted = true, _deleted_at = :now WHERE _id = :id"
);
store.execute(query, params).await?;

// Query excludes soft-deleted documents  
let active_beacons = "SELECT * FROM beacons WHERE _deleted != true";
\`\`\`

**Benefits**:
- No husking risk (update-only operation)
- Deletion propagates via normal CRDT sync
- Tombstone TTL not consumed by high-churn data

**Cleanup**: Periodic hard delete of soft-deleted documents older than 2× collection TTL

\`\`\`rust
// Background job: Clean up soft-deleted beacons after 10 minutes
let threshold = now() - Duration::from_secs(600);
let query = format!(
    "EVICT FROM beacons WHERE _deleted = true AND _deleted_at < :threshold"
);
store.execute(query, params).await?;
\`\`\`

### 2. Hard Delete (Capabilities, Cells)

For low-churn, structured data where concurrent updates are coordinated:

\`\`\`rust
// Hard delete: Creates tombstone
let query = format!("EVICT FROM capabilities WHERE _id = :id");
store.execute(query, params).await?;
\`\`\`

**Use When**:
- Updates are coordinated (e.g., leader election with locking)
- Document lifecycle is discrete (created once, deleted once)
- Resurrection would cause correctness issues

### 3. EVICT for Local Storage Management

Edge devices with storage constraints evict stale local data:

\`\`\`rust
// Evict old beacons from local storage only (no tombstone)
let query = format!(
    "EVICT FROM beacons 
     WHERE last_updated_at < :threshold 
     ORDER BY last_updated_at ASC 
     LIMIT 1000"
);
store.execute(query, params).await?;
\`\`\`

**Use Cases**:
- Tactical edge devices with <1GB storage
- Offline nodes syncing after prolonged disconnect
- Storage pressure recovery (>80% utilization)

**Important**: EVICT does not create tombstones, so evicted documents may re-sync from peers

### 4. Time-Based Lifecycle with Soft-Delete

Application enforces TTL by periodically soft-deleting expired documents:

\`\`\`rust
pub struct DataLifecycleManager {
    store: Arc<DittoStore>,
    beacon_ttl: Duration,
    position_ttl: Duration,
}

impl DataLifecycleManager {
    /// Soft-delete expired beacons (prevents husking)
    pub async fn cleanup_expired_beacons(&self) -> Result<usize> {
        let threshold = SystemTime::now() - self.beacon_ttl;
        let threshold_secs = threshold.duration_since(UNIX_EPOCH)?.as_secs();

        // Phase 1: Soft-delete expired beacons
        let query = format!(
            "UPDATE beacons SET _deleted = true, _deleted_at = :now
             WHERE last_updated_at < :threshold AND _deleted != true"
        );

        let result = self.store.execute(query, params).await?;
        Ok(result.mutated_document_ids().len())
    }

    /// Hard delete soft-deleted beacons (cleanup phase)
    pub async fn purge_soft_deleted_beacons(&self) -> Result<usize> {
        // Delete documents soft-deleted >10 minutes ago
        let threshold = SystemTime::now() - (self.beacon_ttl * 2);
        let threshold_secs = threshold.duration_since(UNIX_EPOCH)?.as_secs();

        let query = format!(
            "EVICT FROM beacons
             WHERE _deleted = true AND _deleted_at < :threshold
             LIMIT 1000"
        );

        let result = self.store.execute(query, params).await?;
        Ok(result.mutated_document_ids().len())
    }

    /// EVICT from local storage (for edge devices)
    pub async fn evict_old_positions(&self) -> Result<usize> {
        let threshold = SystemTime::now() - self.position_ttl;
        let threshold_secs = threshold.duration_since(UNIX_EPOCH)?.as_secs();

        let query = format!(
            "EVICT FROM node_positions
             WHERE last_updated_at < :threshold
             ORDER BY last_updated_at ASC 
             LIMIT 1000"
        );

        let result = self.store.execute(query, params).await?;
        Ok(result.mutated_document_ids().len())
    }
}
\`\`\`

## TTL Configuration Hierarchy

### 1. Ditto System-Level (Tombstone TTL)

Set via ALTER SYSTEM or environment variables (as documented above).

### 2. Application-Level Lifecycle Policies

Collection-specific TTLs managed by Peat Protocol:

\`\`\`rust
pub struct TtlConfig {
    /// Ditto tombstone TTL (hours) - must be set via ALTER SYSTEM
    pub tombstone_ttl_hours: u32,

    /// Collection-specific soft-delete TTLs
    pub beacon_ttl: Duration,
    pub position_ttl: Duration,
    pub capability_ttl: Duration,

    /// EVICT strategy for edge devices
    pub evict_strategy: EvictionStrategy,

    /// Offline retention policy
    pub offline_policy: Option<OfflineRetentionPolicy>,
}

pub enum EvictionStrategy {
    /// Evict oldest documents first
    OldestFirst,
    /// Evict based on storage pressure threshold
    StoragePressure { threshold_pct: u8 },
    /// Keep only last N documents per collection
    KeepLastN(usize),
    /// No automatic eviction
    None,
}

pub struct OfflineRetentionPolicy {
    /// When online: keep data for this long
    pub online_ttl: Duration,
    /// When offline: more aggressive purging
    pub offline_ttl: Duration,
    /// Always keep last N items per collection
    pub keep_last_n: usize,
}
\`\`\`

### Recommended Configurations

#### Tactical Operations (High-Churn Data)
\`\`\`rust
TtlConfig {
    tombstone_ttl_hours: 168,  // 7 days (Edge SDK default)
    beacon_ttl: Duration::from_secs(300),       // 5 min soft-delete
    position_ttl: Duration::from_secs(600),     // 10 min soft-delete
    capability_ttl: Duration::from_secs(7200),  // 2 hours hard delete
    evict_strategy: EvictionStrategy::OldestFirst,
    offline_policy: Some(OfflineRetentionPolicy {
        online_ttl: Duration::from_secs(600),
        offline_ttl: Duration::from_secs(60),  // 1 min when offline
        keep_last_n: 10,
    }),
}
\`\`\`

#### Long-Duration Operations (ISR, Surveillance)
\`\`\`rust
TtlConfig {
    tombstone_ttl_hours: 168,  // 7 days
    beacon_ttl: Duration::from_secs(600),       // 10 min
    position_ttl: Duration::from_secs(3600),    // 1 hour
    capability_ttl: Duration::from_secs(172800), // 48 hours
    evict_strategy: EvictionStrategy::StoragePressure { threshold_pct: 80 },
    offline_policy: None,  // No special offline handling
}
\`\`\`

#### Offline Node (Storage-Constrained)
\`\`\`rust
TtlConfig {
    tombstone_ttl_hours: 72,  // 3 days (shorter for edge)
    beacon_ttl: Duration::from_secs(30),   // 30 sec EVICT
    position_ttl: Duration::from_secs(60), // 1 min EVICT
    capability_ttl: Duration::from_secs(300),  // 5 min EVICT
    evict_strategy: EvictionStrategy::KeepLastN(10),
    offline_policy: Some(OfflineRetentionPolicy {
        online_ttl: Duration::from_secs(300),
        offline_ttl: Duration::from_secs(30),
        keep_last_n: 5,  // Minimal retention
    }),
}
\`\`\`

## Operational Considerations

### Tombstone Monitoring

Monitor tombstone accumulation to prevent storage exhaustion:

\`\`\`sql
-- Query tombstone count (if Ditto exposes introspection)
-- Note: This may require Ditto-specific APIs
SELECT COUNT(*) FROM _system_tombstones
\`\`\`

If tombstones accumulate excessively:
1. Verify \`TOMBSTONE_TTL_ENABLED = true\`
2. Check \`DAYS_BETWEEN_REAPING\` isn't too large (should be ≤3)
3. Ensure no nodes have Edge TTL > Server TTL
4. Consider soft-delete patterns to reduce tombstone churn

### Performance Guidelines

#### Batch Deletion Limits
- **Edge SDK**: EVICT max 1,000 documents per query
- **Server**: EVICT max 30,000 documents per query
- Use \`LIMIT\` clause to prevent performance degradation

\`\`\`rust
// Good: Batched eviction
let query = format!(
    "EVICT FROM beacons WHERE _deleted = true LIMIT 1000"
);

// Bad: Unbounded eviction (can cause performance issues)
let query = format!(
    "EVICT FROM beacons WHERE _deleted = true"  // No LIMIT!
);
\`\`\`

#### Reaping Schedule
- **Default**: Once per day
- **Large datasets**: Schedule during off-peak hours using \`REAPER_PREFERRED_HOUR\`
- **Tactical ops**: May increase to 2-3 times/day for storage-constrained devices

### Storage Budgeting

Estimate storage requirements for tombstone retention:

\`\`\`
Tombstone size: ~1KB (compressed)
Delete rate: D deletions/day
TTL: T days
Storage needed: D × T × 1KB

Example (tactical ops with soft-delete):
- 1,000 beacon updates/hour
- Soft-delete after 5 min → ~12,000 soft-deletes/day
- Hard delete after 10 min → ~12,000 tombstones/day
- 7-day TTL
- Storage: 12,000 × 7 × 1KB = 84 MB for tombstones

Without soft-delete:
- 24,000 updates/hour → 576,000 tombstones/day
- 7-day TTL
- Storage: 576,000 × 7 × 1KB = 4 GB for tombstones (unsustainable!)
\`\`\`

**Key Insight**: Soft-delete pattern reduces tombstone storage by 95%+ for high-churn data.

## Implementation Phases

### Phase 1: Soft-Delete Pattern (Immediate)

Add soft-delete support to high-churn collections:

\`\`\`rust
// Add to DittoStore
impl DittoStore {
    pub async fn soft_delete_beacon(&self, beacon_id: &str) -> Result<()> {
        let query = format!(
            "UPDATE beacons SET _deleted = true, _deleted_at = :now WHERE _id = :id"
        );
        self.execute(query, params).await?;
        Ok(())
    }

    pub async fn cleanup_soft_deleted(&self, collection: &str, older_than: Duration) -> Result<usize> {
        let threshold = SystemTime::now() - older_than;
        let threshold_secs = threshold.duration_since(UNIX_EPOCH)?.as_secs();

        let query = format!(
            "EVICT FROM {} WHERE _deleted = true AND _deleted_at < :threshold LIMIT 1000",
            collection
        );

        let result = self.execute(query, params).await?;
        Ok(result.mutated_document_ids().len())
    }
}
\`\`\`

### Phase 2: Tombstone TTL Configuration

Add Ditto system configuration helpers:

\`\`\`rust
impl DittoStore {
    /// Configure Ditto tombstone TTL (must call before heavy deletion workload)
    pub async fn configure_tombstone_ttl(&self, config: TombstoneTtlConfig) -> Result<()> {
        // Execute ALTER SYSTEM commands
        self.execute("ALTER SYSTEM SET TOMBSTONE_TTL_ENABLED = true", ()).await?;
        self.execute(
            format!("ALTER SYSTEM SET TOMBSTONE_TTL_HOURS = {}", config.ttl_hours),
            ()
        ).await?;
        self.execute(
            format!("ALTER SYSTEM SET DAYS_BETWEEN_REAPING = {}", config.days_between_reaping),
            ()
        ).await?;

        Ok(())
    }
}
\`\`\`

### Phase 3: EVICT for Edge Devices

Implement storage pressure monitoring and eviction:

\`\`\`rust
pub struct StorageManager {
    store: Arc<DittoStore>,
    evict_strategy: EvictionStrategy,
}

impl StorageManager {
    pub async fn check_storage_pressure(&self) -> Result<f32> {
        // Check available storage (platform-specific)
        let total = self.get_total_storage()?;
        let used = self.get_used_storage()?;
        Ok(used as f32 / total as f32)
    }

    pub async fn evict_if_needed(&self) -> Result<usize> {
        let pressure = self.check_storage_pressure().await?;

        if pressure > 0.8 {  // 80% threshold
            self.evict_oldest(1000).await
        } else {
            Ok(0)
        }
    }
}
\`\`\`

### Phase 4: Offline Retention Policy

Connectivity-aware lifecycle management:

\`\`\`rust
pub struct ConnectivityAwareRetention {
    store: Arc<DittoStore>,
    policy: OfflineRetentionPolicy,
}

impl ConnectivityAwareRetention {
    pub async fn monitor(&self) {
        let mut connectivity = self.store.connectivity_monitor();

        while let Some(state) = connectivity.next().await {
            match state {
                ConnectivityState::Offline => {
                    self.apply_offline_policy().await;
                }
                ConnectivityState::Online => {
                    self.apply_online_policy().await;
                }
            }
        }
    }

    async fn apply_offline_policy(&self) {
        // More aggressive eviction when offline
        for collection in &["beacons", "node_positions"] {
            self.keep_last_n(collection, self.policy.keep_last_n).await;
        }
    }
}
\`\`\`

## Summary

### Ditto Deletion Model (Corrected Understanding)

1. **DELETE** creates tombstones that sync mesh-wide and auto-reap after TTL
2. **EVICT** removes documents locally only (no tombstone, may re-sync)
3. **Tombstone TTL** is configured via \`ALTER SYSTEM\` or environment variables
4. **Soft-delete pattern** avoids husking and reduces tombstone churn

### Peat Protocol Strategy

1. **High-churn data** (beacons, positions): Soft-delete pattern
2. **Low-churn data** (capabilities, cells): Hard delete (EVICT)
3. **Edge devices**: Aggressive local EVICT based on storage pressure
4. **Offline nodes**: Keep-last-N retention policy

### Configuration Hierarchy

- **System-level**: Ditto tombstone TTL (ALTER SYSTEM or env vars)
- **Application-level**: Collection-specific soft-delete TTLs
- **Device-level**: EVICT strategy based on storage constraints

## Related ADRs

- ADR-001: Peat Protocol POC (CRDT-based state management)
- ADR-002: Beacon Storage Architecture (Ditto integration patterns)

## References

- [Ditto Delete Documentation](https://docs.ditto.live/sdk/latest/crud/delete)
- [Ditto Data Model](https://docs.ditto.live/concepts/data-model)
