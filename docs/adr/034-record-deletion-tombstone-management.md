# ADR-034: Record Deletion and Tombstone Management

**Status**: Proposed
**Date**: 2025-12-09
**Authors**: Kit Plummer, Codex
**Relates to**: ADR-019 (Sync Modes), ADR-011 (Automerge Backend), ADR-024 (Flexible Hierarchy)

---

## Context

### The CRDT Deletion Problem

CRDTs achieve eventual consistency through merge operations, but deletion creates a fundamental challenge:

```
Timeline:
  t=0: Document "beacon-123" exists with {lat: 37.0, lon: -122.0}

  Node A (t=1): delete("beacon-123")
  Node B (t=1): update("beacon-123", {lat: 38.0, lon: -121.0})  // concurrent

  After sync: What is the state of beacon-123?
```

**Without explicit deletion semantics:**
- Add-wins: Deleted item reappears after sync (data resurrection)
- Delete-wins: Valid concurrent updates are lost
- Last-write-wins: Non-deterministic based on timestamps

### Tombstone Accumulation

The standard CRDT solution is **tombstones** - markers that record "this item was deleted":

```rust
struct Tombstone {
    id: DocumentId,
    deleted_at: Timestamp,
    deleted_by: NodeId,
}
```

**Problem**: Tombstones accumulate forever because:
1. Any node might be offline and still have the live document
2. Without the tombstone, sync would resurrect the document
3. No safe point to garbage collect without coordination

**In Peat's tactical context:**
- A 10-node squad generating 1 beacon/second for 8 hours = 288,000 records
- If 10% are "deleted" (superseded), that's 28,800 tombstones
- Tombstones sync forever, consuming bandwidth on reconnection

### Peat-Specific Considerations

| Data Type | Deletion Semantics | Retention Need |
|-----------|-------------------|----------------|
| Beacons/Positions | Superseded by newer (implicit) | None - latest only |
| Contact Reports | Rarely deleted, audit trail | Long-term |
| Commands | May be cancelled/superseded | Audit trail |
| Nodes/Cells | Leave mesh (explicit) | Short-term tombstone |
| Tracks | Stale after time window | TTL-based |
| Alerts | Acknowledged/dismissed | Short-term |

### Current State

Peat currently has **no explicit deletion mechanism**:
- `DocumentStore::delete()` is not implemented
- Old documents accumulate indefinitely
- `SyncMode::LatestOnly` discards history but not documents
- No tombstone or TTL infrastructure

---

## Decision Drivers

### Requirements

1. **Deterministic semantics**: All nodes must converge to the same state
2. **Offline resilience**: Must work with extended disconnection (hours/days)
3. **Bandwidth efficiency**: Minimize sync overhead from deletion metadata
4. **Audit compliance**: Some data types require deletion records
5. **Memory bounds**: Prevent unbounded tombstone growth
6. **Per-collection policy**: Different data types need different strategies

### Constraints

1. **No central coordinator**: Can't rely on server to authorize deletion
2. **Clock skew**: Nodes may have drifted clocks (no GPS)
3. **Hierarchy direction**: Deletes may need to flow up, down, or both
4. **Automerge compatibility**: Must work within Automerge's merge semantics

---

## Decision

### Hybrid Deletion Strategy

We adopt a **hybrid approach** combining multiple strategies based on data semantics:

```
┌─────────────────────────────────────────────────────────────┐
│                   Deletion Strategy Matrix                   │
├─────────────────┬──────────────┬─────────────┬─────────────┤
│ Collection      │ Strategy     │ Tombstone   │ Propagation │
│                 │              │ TTL         │ Direction   │
├─────────────────┼──────────────┼─────────────┼─────────────┤
│ beacons         │ Implicit TTL │ None        │ N/A         │
│ platforms       │ Implicit TTL │ None        │ N/A         │
│ tracks          │ Explicit+TTL │ 1 hour      │ Bidirectional│
│ nodes           │ Tombstone    │ 24 hours    │ Bidirectional│
│ cells           │ Tombstone    │ 24 hours    │ Up only     │
│ contact_reports │ Soft Delete  │ Forever     │ Up only     │
│ commands        │ Soft Delete  │ Forever     │ Down only   │
│ alerts          │ Explicit+TTL │ 4 hours     │ Bidirectional│
└─────────────────┴──────────────┴─────────────┴─────────────┘
```

### Strategy Definitions

#### 1. Implicit TTL (Supersession)

For high-frequency position data, documents are **implicitly superseded** by newer versions:

```rust
/// Document is considered "deleted" if:
/// 1. A newer document with same key exists, OR
/// 2. Document age exceeds TTL
pub struct ImplicitTTLPolicy {
    /// Maximum age before document is garbage collected
    ttl: Duration,
    /// Key field for supersession (e.g., "node_id" for beacons)
    supersession_key: String,
}
```

**Behavior:**
- No explicit delete operation
- Query filters out documents older than TTL
- Sync skips documents older than TTL
- Garbage collection runs periodically

**Integration with SyncMode:**
- `LatestOnly`: Only syncs current document per key
- `WindowedHistory`: Syncs documents within time window
- Implicit TTL applies on top of SyncMode

#### 2. Explicit Tombstone with TTL

For data requiring explicit deletion with bounded retention:

```rust
pub struct TombstonePolicy {
    /// How long to retain tombstones before garbage collection
    tombstone_ttl: Duration,
    /// Conflict resolution: true = delete wins, false = update wins
    delete_wins: bool,
}

/// Tombstone record synced alongside documents
#[derive(Clone, Serialize, Deserialize)]
pub struct Tombstone {
    /// ID of deleted document
    pub document_id: DocumentId,
    /// Collection the document belonged to
    pub collection: String,
    /// When deletion occurred (for TTL calculation)
    pub deleted_at: SystemTime,
    /// Node that initiated deletion
    pub deleted_by: NodeId,
    /// Lamport timestamp for ordering
    pub lamport: u64,
    /// Optional reason for deletion
    pub reason: Option<String>,
}
```

**Conflict Resolution:**
```
delete_wins = true:
  delete(doc) + update(doc) → deleted

delete_wins = false:
  delete(doc) + update(doc) → updated (delete ignored)
```

**Tombstone Lifecycle:**
```
1. Node A calls delete("track-123")
2. Tombstone created with TTL=1hr
3. Tombstone syncs to all peers
4. Peers mark local "track-123" as deleted
5. After TTL, tombstone garbage collected
6. If offline node reconnects after TTL:
   - Its "track-123" may resurrect (acceptable for tracks)
   - Or: "resurrection window" triggers re-delete
```

#### 3. Soft Delete (Audit Trail)

For data requiring permanent deletion records:

```rust
/// Soft delete marks document as deleted but retains it
pub struct SoftDeletePolicy {
    /// Field name for deletion flag
    deleted_field: String,  // default: "_deleted"
    /// Field name for deletion timestamp
    deleted_at_field: String,  // default: "_deleted_at"
    /// Whether to include soft-deleted docs in queries by default
    include_deleted_default: bool,
}
```

**Document transformation:**
```json
// Before delete
{
  "id": "contact-report-456",
  "type": "HOSTILE",
  "location": {...}
}

// After soft delete
{
  "id": "contact-report-456",
  "type": "HOSTILE",
  "location": {...},
  "_deleted": true,
  "_deleted_at": "2025-12-09T12:00:00Z",
  "_deleted_by": "node-alpha"
}
```

**Query behavior:**
```rust
// Default: exclude deleted
store.query("contact_reports", Query::All)  // excludes _deleted=true

// Include deleted explicitly
store.query("contact_reports", Query::And(vec![
    Query::All,
    Query::IncludeDeleted,
]))
```

---

### API Design

```rust
/// Deletion configuration per collection
#[derive(Debug, Clone)]
pub enum DeletionPolicy {
    /// No explicit deletion, documents superseded by newer versions
    ImplicitTTL {
        ttl: Duration,
        supersession_key: Option<String>,
    },

    /// Explicit tombstones with bounded retention
    Tombstone {
        tombstone_ttl: Duration,
        delete_wins: bool,
    },

    /// Soft delete with permanent audit trail
    SoftDelete {
        include_deleted_default: bool,
    },

    /// No deletion allowed for this collection
    Immutable,
}

/// Extended DocumentStore trait
#[async_trait]
pub trait DocumentStore {
    // Existing methods...

    /// Delete a document according to collection policy
    async fn delete(
        &self,
        collection: &str,
        id: &DocumentId,
        reason: Option<&str>,
    ) -> Result<DeleteResult>;

    /// Delete multiple documents matching a query
    async fn delete_where(
        &self,
        collection: &str,
        query: &Query,
        reason: Option<&str>,
    ) -> Result<DeleteBatchResult>;

    /// Check if a document is deleted (tombstoned or soft-deleted)
    async fn is_deleted(
        &self,
        collection: &str,
        id: &DocumentId,
    ) -> Result<bool>;

    /// Restore a soft-deleted document (if policy allows)
    async fn restore(
        &self,
        collection: &str,
        id: &DocumentId,
    ) -> Result<RestoreResult>;

    /// Get deletion policy for a collection
    fn deletion_policy(&self, collection: &str) -> DeletionPolicy;
}

/// Result of a delete operation
pub struct DeleteResult {
    pub deleted: bool,
    pub tombstone_id: Option<String>,
    pub expires_at: Option<SystemTime>,
}
```

---

### Tombstone Sync Protocol

Tombstones sync alongside documents in a dedicated channel:

```
Wire Format v3 (extends v2 from ADR-019 Amendment):

Byte 0: Message Type
  0x01 = FullHistory sync message
  0x02 = LatestOnly document
  0x03 = WindowedHistory sync message
  0x04 = Tombstone             // NEW
  0x05 = TombstoneAck          // NEW
  0x06 = GarbageCollect        // NEW

Tombstone Message:
┌─────────┬────────────┬──────────────┬─────────────┬─────────┐
│ Type    │ Collection │ Document ID  │ Deleted At  │ Reason  │
│ (1 byte)│ (var)      │ (var)        │ (8 bytes)   │ (var)   │
└─────────┴────────────┴──────────────┴─────────────┴─────────┘
```

**Sync behavior:**
1. On connect, exchange tombstone sets (like sync state)
2. Apply incoming tombstones to local store
3. Tombstones propagate according to direction policy
4. Garbage collection coordinated via GC messages

---

### Garbage Collection

**Per-node GC (no coordination):**
```rust
impl GarbageCollector {
    /// Run garbage collection for expired tombstones
    pub async fn collect_tombstones(&self) -> GcResult {
        let now = SystemTime::now();
        let mut collected = 0;

        for tombstone in self.store.tombstones() {
            let policy = self.store.deletion_policy(&tombstone.collection);
            if let DeletionPolicy::Tombstone { tombstone_ttl, .. } = policy {
                let age = now.duration_since(tombstone.deleted_at)?;
                if age > tombstone_ttl {
                    self.store.remove_tombstone(&tombstone.id)?;
                    collected += 1;
                }
            }
        }

        Ok(GcResult { tombstones_collected: collected })
    }

    /// Run garbage collection for expired implicit-TTL documents
    pub async fn collect_expired_documents(&self) -> GcResult {
        let now = SystemTime::now();
        let mut collected = 0;

        for (collection, policy) in self.store.collection_policies() {
            if let DeletionPolicy::ImplicitTTL { ttl, .. } = policy {
                let cutoff = now - ttl;
                let expired = self.store.query(
                    &collection,
                    Query::Lt {
                        field: "updated_at".to_string(),
                        value: cutoff.into(),
                    },
                ).await?;

                for doc in expired {
                    self.store.hard_delete(&collection, &doc.id).await?;
                    collected += 1;
                }
            }
        }

        Ok(GcResult { documents_collected: collected })
    }
}
```

**Resurrection handling:**

If a node reconnects after tombstone TTL with a document that was deleted:

```rust
pub enum ResurrectionPolicy {
    /// Allow resurrection (document comes back)
    Allow,
    /// Re-delete with fresh tombstone
    ReDelete,
    /// Reject sync of resurrected documents
    Reject,
}
```

Default: `ReDelete` for most collections, `Allow` for beacons (they'll be superseded anyway).

---

### Configuration

```rust
/// Default deletion policies per collection
impl DeletionPolicy {
    pub fn default_for_collection(collection: &str) -> Self {
        match collection {
            // Position data - implicit TTL, superseded by newer
            "beacons" | "platforms" => Self::ImplicitTTL {
                ttl: Duration::from_secs(3600), // 1 hour
                supersession_key: Some("node_id".to_string()),
            },

            // Tracks - explicit tombstone with short TTL
            "tracks" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(3600),
                delete_wins: true,
            },

            // Network membership - tombstone with longer TTL
            "nodes" | "cells" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(86400), // 24 hours
                delete_wins: true,
            },

            // Audit-required data - soft delete forever
            "contact_reports" | "commands" | "audit_logs" => Self::SoftDelete {
                include_deleted_default: false,
            },

            // Alerts - tombstone with medium TTL
            "alerts" => Self::Tombstone {
                tombstone_ttl: Duration::from_secs(14400), // 4 hours
                delete_wins: false, // Update-wins: alert update cancels delete
            },

            // Default: soft delete for safety
            _ => Self::SoftDelete {
                include_deleted_default: false,
            },
        }
    }
}
```

---

## Consequences

### Positive

1. **Deterministic semantics**: Each collection has clear delete behavior
2. **Bounded tombstone growth**: TTL prevents unbounded accumulation
3. **Audit compliance**: Soft delete preserves history where needed
4. **Bandwidth efficient**: Implicit TTL requires no deletion metadata
5. **Offline resilient**: Works without coordination

### Negative

1. **Complexity**: Multiple strategies to understand and configure
2. **Resurrection risk**: Documents can reappear after tombstone TTL
3. **Clock dependency**: TTL relies on reasonably synchronized clocks
4. **Storage overhead**: Soft delete never frees storage

### Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Document resurrection | Medium | Low | Re-delete policy, short TTLs for critical data |
| Clock skew breaks TTL | Medium | Medium | Use Lamport clocks for ordering, wall clock for TTL |
| Tombstone sync storms | Low | Medium | Rate limit tombstone sync, batch processing |
| Soft delete bloat | Medium | Low | Periodic archival to cold storage |

---

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] `DeletionPolicy` enum and configuration
- [ ] `Tombstone` struct and storage
- [ ] `DocumentStore::delete()` implementation
- [ ] Per-collection policy registry

### Phase 2: Sync Protocol
- [ ] Wire format v3 with tombstone messages
- [ ] Tombstone exchange on peer connect
- [ ] Direction-aware tombstone propagation

### Phase 3: Garbage Collection
- [ ] Periodic GC task
- [ ] Resurrection detection and handling
- [ ] Implicit TTL document cleanup

### Phase 4: Query Integration
- [ ] `Query::IncludeDeleted` variant
- [ ] Soft delete field filtering
- [ ] Tombstone-aware query execution

---

## Alternatives Considered

### 1. Pure Tombstone (No TTL)

Keep tombstones forever for guaranteed consistency.

**Rejected because**: Unbounded growth unacceptable for tactical edge devices.

### 2. Vector Clock GC

Use vector clocks to determine when all nodes have seen a tombstone.

**Rejected because**: Requires coordination, doesn't work with extended offline.

### 3. Centralized Delete Authority

Require leader approval for deletes.

**Rejected because**: Violates offline-first principle, single point of failure.

### 4. No Delete Support

Documents are immutable, use versioning instead.

**Considered but deferred**: May be appropriate for some collections, but explicit delete needed for node/cell membership.

---

## References

- [Automerge Deletion Semantics](https://automerge.org/)
- [CRDTs and Deletion](https://martin.kleppmann.com/2020/07/06/crdt-hard-parts-hydra.html)
- [Tombstones in Distributed Systems](https://en.wikipedia.org/wiki/Tombstone_(data_store))
- ADR-019: Sync Modes and Subscription Granularity
- ADR-011: Automerge Backend Selection

---

## Appendix: Decision Matrix

| Requirement | Implicit TTL | Tombstone+TTL | Soft Delete |
|-------------|--------------|---------------|-------------|
| No explicit action needed | ✅ | ❌ | ❌ |
| Immediate effect | ❌ | ✅ | ✅ |
| Audit trail | ❌ | ❌ | ✅ |
| Bounded storage | ✅ | ✅ | ❌ |
| Resurrection safe | ✅ | ⚠️ | ✅ |
| Works offline | ✅ | ✅ | ✅ |
| Bandwidth efficient | ✅ | ⚠️ | ⚠️ |

Legend: ✅ = Full support, ⚠️ = Partial/conditional, ❌ = Not supported
