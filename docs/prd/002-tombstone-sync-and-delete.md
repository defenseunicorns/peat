---
title: "PRD-002-tombstone-sync-and-delete: Tombstone Sync Protocol and DocumentStore::delete()"
status: Draft
issue: "#668"
epic: "#670 — QoS, TTL, and Garbage Collection"
adrs: [016, 019, 034]
---

## Implementation Spec: Wire Tombstone Sync Protocol and DocumentStore::delete()

### Objective

Connect the existing deletion primitives (DeletionPolicy, Tombstone, TombstoneSyncMessage, GarbageCollector, PropagationDirection) into a working end-to-end pipeline. Today the pieces exist independently; this issue wires them together so that `DocumentStore::delete()` creates the right artifact, the sync layer exchanges it with peers respecting direction policy, GC cleans up expired tombstones, and resurrection detection handles offline-node reconnection.

---

### Current State

**What exists and works:**

| Component | Location | Status |
|-----------|----------|--------|
| `DeletionPolicy` enum (ImplicitTTL, Tombstone, SoftDelete, Immutable) | `peat-mesh/src/qos/deletion.rs` | Complete |
| `DeletionPolicyRegistry` with per-collection defaults | `peat-mesh/src/qos/deletion.rs` | Complete |
| `PropagationDirection` (Bidirectional, UpOnly, DownOnly, SystemWide) with `allows_up()` / `allows_down()` | `peat-mesh/src/qos/deletion.rs` | Complete |
| `TombstoneSyncMessage` with binary encode/decode | `peat-mesh/src/qos/deletion.rs` | Complete |
| `TombstoneBatch` with binary encode/decode | `peat-mesh/src/qos/deletion.rs` | Complete |
| `SyncMessageType::Tombstone` (0x04), `TombstoneBatch` (0x05), `TombstoneAck` (0x06) | `peat-mesh/src/storage/automerge_sync.rs` | Complete |
| `SyncCoordinator::send_tombstones_to_peer()`, `sync_tombstones_with_peer()`, `apply_tombstone()` | `peat-mesh/src/storage/automerge_sync.rs` | Complete |
| `SyncCoordinator::propagate_tombstone_to_peers()` | `peat-mesh/src/storage/automerge_sync.rs` | Partial — see gaps |
| `AutomergeStore` tombstone CRUD (`put_tombstone`, `has_tombstone`, `get_all_tombstones`, `remove_tombstone`) | `peat-mesh/src/storage/automerge_store.rs` | Complete |
| `GarbageCollector<S>` with `run_gc()`, `collect_tombstones()`, `start_periodic_gc()` | `peat-mesh/src/qos/garbage_collection.rs` | Complete |
| `ResurrectionPolicy` with `check_resurrection()`, `handle_resurrection()` | `peat-mesh/src/qos/garbage_collection.rs` | Complete |
| `GcStore` trait implemented by `AutomergeStore` | `peat-mesh/src/storage/automerge_store.rs` | Complete |
| `AutomergeSyncBackend::delete()` with full policy dispatch | `peat-protocol/src/sync/automerge.rs:739-830` | Has TODOs |
| `Query::IncludeDeleted`, `Query::DeletedOnly` | `peat-mesh/src/sync/types.rs` | Defined |
| `InMemoryStore` handles `IncludeDeleted`/`DeletedOnly` | `peat-mesh/src/sync/in_memory.rs` | Complete |

**What's NOT wired (the gaps):**

1. **`AutomergeSyncBackend::delete()` uses placeholder node ID and Lamport=0** — `peat-protocol/src/sync/automerge.rs:770-779` has `"local"` as `deleted_by` and `0` as Lamport. Needs actual node ID from the backend config and a monotonic Lamport counter.

2. **`delete()` does not trigger tombstone propagation** — After creating a tombstone, `delete()` stores it locally and removes the document, but never calls `sync_tombstones_with_peer()` or sends a `TombstoneSyncMessage` to connected peers. The tombstone sits locally until the next full peer-connect exchange.

3. **Direction-aware propagation is incomplete** — `propagate_tombstone_to_peers()` at `automerge_sync.rs:2324-2392` correctly filters `SystemWide`/`Bidirectional` vs `UpOnly`/`DownOnly`, but for `UpOnly`/`DownOnly` it skips propagation entirely with a comment saying "handled by PeatMesh layer." No PeatMesh layer handler exists for this. The hierarchy module (`peat-mesh/src/hierarchy/`) provides `NodeRole` (Leader/Member/Standalone) and `HierarchyLevel`, but there's no mapping from peer EndpointId to "is this my parent or child."

4. **Resurrection detection not connected to sync receive path** — `GarbageCollector::check_resurrection()` exists but is never called from the sync receive path. When `SyncCoordinator::apply_tombstone()` or `receive_sync_message()` processes an incoming document, it doesn't check whether that document was previously tombstoned and the tombstone expired.

5. **`Query::IncludeDeleted` not handled in AutomergeSyncBackend** — The `InMemoryStore` handles it, but the Automerge backend's `query()` implementation does not filter out `_deleted=true` docs by default, nor does it honor `IncludeDeleted`/`DeletedOnly` variants.

6. **GC not started anywhere** — `GarbageCollector::start_periodic_gc()` exists but is never called from `AutomergeSyncBackend::initialize()` or the mesh node binary.

---

### Implementation Steps

#### Step 1: Wire node ID and Lamport into `delete()` (~20 lines)

**File:** `peat-protocol/src/sync/automerge.rs`

- Add a `lamport_counter: Arc<AtomicU64>` field to `AutomergeSyncBackend`.
- In `delete()`, replace `"local"` with `self.config.app_id` (or a dedicated `node_id` field).
- Replace `0` Lamport with `self.lamport_counter.fetch_add(1, Ordering::SeqCst)`.
- Initialize `lamport_counter` in `AutomergeSyncBackend::new()`.

```rust
// In the Tombstone branch of delete():
let lamport = self.lamport_counter.fetch_add(1, Ordering::SeqCst);
let node_id = &self.node_id; // from config
let tombstone = Tombstone::new(doc_id.clone(), collection, node_id, lamport);
```

#### Step 2: Trigger tombstone propagation from `delete()` (~15 lines)

**File:** `peat-protocol/src/sync/automerge.rs`

After storing the tombstone in the `DeletionPolicy::Tombstone` branch of `delete()`:

- Build a `TombstoneSyncMessage::from_tombstone(tombstone.clone())`.
- If a `SyncCoordinator` reference is available, call `send_single_tombstone_to_peer()` for each connected peer (filtered by direction).
- If no coordinator ref, enqueue the tombstone in a `pending_tombstones: Arc<Mutex<Vec<TombstoneSyncMessage>>>` that gets drained on next sync round.

```rust
// After storing tombstone locally:
if let Some(coordinator) = self.sync_coordinator.as_ref() {
    let msg = TombstoneSyncMessage::from_tombstone(tombstone.clone());
    tokio::spawn({
        let coordinator = coordinator.clone();
        async move {
            coordinator.propagate_tombstone_to_all(&msg).await;
        }
    });
}
```

#### Step 3: Add peer hierarchy context for direction filtering (~30 lines)

**Files:**
- `peat-mesh/src/storage/automerge_sync.rs` (modify `propagate_tombstone_to_peers`)
- `peat-mesh/src/storage/automerge_sync.rs` (add `PeerHierarchyInfo`)

Add a `peer_hierarchy: Arc<RwLock<HashMap<EndpointId, PeerRelationship>>>` to `SyncCoordinator` where `PeerRelationship` is `Parent | Child | Peer`.

Modify `propagate_tombstone_to_peers()` to actually filter:

```rust
PropagationDirection::UpOnly => {
    target_peers.into_iter()
        .filter(|p| self.peer_relationship(p) == PeerRelationship::Parent)
        .collect()
}
PropagationDirection::DownOnly => {
    target_peers.into_iter()
        .filter(|p| self.peer_relationship(p) == PeerRelationship::Child)
        .collect()
}
```

The hierarchy module's `NodeRole` and `HierarchyLevel` can populate this mapping. When a peer connects and exchanges beacons, the `HierarchyLevel` comparison determines the relationship.

#### Step 4: Connect resurrection detection to sync receive path (~25 lines)

**File:** `peat-mesh/src/storage/automerge_sync.rs`

In the existing `receive_sync_message()` method, after processing a document sync (type `DeltaSync` or `StateSnapshot`):

```rust
// After applying sync for doc_key:
let (collection, doc_id) = parse_doc_key(&doc_key);
if let Some(gc) = self.garbage_collector.as_ref() {
    if let Some(policy) = gc.check_resurrection(&collection, &doc_id, SystemTime::now())? {
        match gc.handle_resurrection(&collection, &doc_id)? {
            ResurrectionPolicy::ReDelete => {
                // Create fresh tombstone and propagate
                let tombstone = Tombstone::new(&doc_id, &collection, &self.node_id, self.next_lamport());
                self.store.put_tombstone(&tombstone)?;
                self.store.delete(&doc_key)?;
                let msg = TombstoneSyncMessage::from_tombstone(tombstone);
                self.propagate_tombstone_to_peers(&msg, peer_id).await;
            }
            ResurrectionPolicy::Reject => {
                self.store.delete(&doc_key)?;
            }
            ResurrectionPolicy::Allow => { /* accept the document */ }
        }
    }
}
```

To make `check_resurrection()` actually detect resurrections, add a `deleted_history: Arc<RwLock<HashSet<String>>>` to `SyncCoordinator` (or `GarbageCollector`). When a tombstone is collected by GC, record its key in `deleted_history`. When `check_resurrection()` finds a key in `deleted_history` with no active tombstone, it knows it's a resurrection.

#### Step 5: Handle `Query::IncludeDeleted` and `DeletedOnly` in Automerge backend (~20 lines)

**File:** `peat-protocol/src/sync/automerge.rs`

In the `query()` method of the `DocumentStore` impl:

```rust
async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
    let (inner_query, include_deleted, deleted_only) = match query {
        Query::IncludeDeleted(inner) => (inner.as_ref(), true, false),
        Query::DeletedOnly => (&Query::All, false, true),
        other => (other, false, false),
    };

    let mut results = self.query_inner(collection, inner_query).await?;

    if deleted_only {
        results.retain(|d| d.fields.get("_deleted").and_then(|v| v.as_bool()).unwrap_or(false));
    } else if !include_deleted {
        results.retain(|d| !d.fields.get("_deleted").and_then(|v| v.as_bool()).unwrap_or(false));
    }

    Ok(results)
}
```

#### Step 6: Start GC from backend initialization (~10 lines)

**File:** `peat-protocol/src/sync/automerge.rs` (in `initialize()` or `new()`)

```rust
// In AutomergeSyncBackend::initialize():
let gc = GarbageCollector::new(self.store.clone(), GcConfig::default());
let gc_handle = gc.start_periodic_gc();
self.gc_handle = Some(gc_handle);
```

Also ensure GC is stopped in `shutdown()`.

---

### Code Sketch: Full Delete Flow

```
User calls DocumentStore::delete("nodes", "node-42", Some("decommissioned"))
  │
  ├─ deletion_policy("nodes") → Tombstone { ttl: 24h, delete_wins: true }
  │
  ├─ Create Tombstone { doc_id: "node-42", collection: "nodes",
  │                      deleted_by: "node-alpha", lamport: 17,
  │                      reason: "decommissioned" }
  │
  ├─ Store tombstone locally (tombstones HashMap)
  │
  ├─ Remove document "nodes:node-42" from store
  │
  ├─ Build TombstoneSyncMessage { tombstone, direction: Bidirectional }
  │
  ├─ propagate_tombstone_to_peers()
  │     ├─ direction=Bidirectional → send to ALL connected peers
  │     ├─ For each peer: send_single_tombstone_to_peer(peer_id, msg)
  │     │     └─ Wire: [doc_key_len][doc_key][0x04][payload_len][encoded TombstoneSyncMessage]
  │     └─ (If direction=UpOnly, filter to parent peers only)
  │
  └─ Return DeleteResult { deleted: true, tombstone_id: "nodes:node-42",
                           expires_at: now+24h }

--- 24 hours later ---

GarbageCollector::run_gc()
  ├─ get_all_tombstones() → find "nodes:node-42" (age > 24h)
  ├─ record "nodes:node-42" in deleted_history
  └─ remove_tombstone("nodes", "node-42")

--- Offline node reconnects ---

SyncCoordinator::receive_sync_message("nodes:node-42", peer_id, DeltaSync)
  ├─ Apply Automerge sync message
  ├─ check_resurrection("nodes", "node-42") → key in deleted_history, no tombstone
  ├─ handle_resurrection("nodes", "node-42") → ResurrectionPolicy::ReDelete
  ├─ Create fresh tombstone, store it, delete doc
  └─ Propagate new tombstone to all peers
```

---

### Testing Plan

#### Unit Tests (extend existing)

1. **`peat-mesh/src/qos/deletion.rs` tests:**
   - `test_delete_creates_tombstone_with_real_lamport` — verify Lamport increments
   - `test_delete_creates_tombstone_with_node_id` — verify actual node ID used

2. **`peat-protocol/src/sync/automerge.rs` tests:**
   - `test_delete_implicit_ttl_noop` — delete("beacons", id) returns deleted=false
   - `test_delete_tombstone_creates_and_removes` — delete("nodes", id) creates tombstone and removes doc
   - `test_delete_soft_delete_marks_document` — delete("contact_reports", id) sets _deleted=true
   - `test_delete_immutable_rejected` — delete on Immutable policy returns deleted=false
   - `test_query_excludes_soft_deleted_by_default` — default query skips _deleted docs
   - `test_query_include_deleted` — `Query::IncludeDeleted(Query::All)` returns soft-deleted docs
   - `test_query_deleted_only` — `Query::DeletedOnly` returns only soft-deleted docs

#### Integration Tests (extend existing)

3. **`peat-mesh/tests/gc_tombstone_integration.rs`:**
   - `test_gc_starts_and_collects_expired_tombstones` — verify periodic GC collects after TTL
   - `test_gc_records_deleted_history` — after GC, deleted_history contains expired key
   - `test_resurrection_detected_after_gc` — document synced after tombstone GC triggers ReDelete

4. **`peat-protocol/tests/tombstone_sync_e2e.rs`:**
   - `test_delete_propagates_tombstone_to_peers` — delete on node A sends tombstone to node B
   - `test_tombstone_direction_up_only` — contact_reports tombstone only sent to parent
   - `test_tombstone_direction_down_only` — commands tombstone only sent to children
   - `test_tombstone_direction_system_wide` — propagates to all peers
   - `test_offline_reconnect_resurrection_redelete` — offline node syncs tombstoned doc, gets re-deleted

#### Property Tests (optional)

5. **Tombstone convergence:** Two nodes with different tombstone sets converge after exchange
6. **Lamport ordering:** Concurrent deletes from different nodes produce consistent ordering

---

### Acceptance Criteria

- [ ] `delete("beacons", id)` returns `DeleteResult { deleted: false }` (ImplicitTTL → no-op)
- [ ] `delete("nodes", id)` creates tombstone with real node ID and monotonic Lamport, removes document, returns `DeleteResult { deleted: true, tombstone_id: Some(...), expires_at: now+24h }`
- [ ] `delete("contact_reports", id)` sets `_deleted=true`, `_deleted_at`, optional `_deleted_reason` on document, does NOT remove it
- [ ] `delete("commands", id)` sets `_deleted=true` (SoftDelete policy)
- [ ] Delete on Immutable collection returns `DeleteResult { deleted: false }`
- [ ] After `delete("nodes", id)`, tombstone is sent to all connected peers within same sync round
- [ ] `contact_reports` tombstones propagate only to parent peers (`UpOnly`)
- [ ] `commands` tombstones propagate only to child peers (`DownOnly`)
- [ ] `Query::All` on a collection with soft-deleted docs excludes them
- [ ] `Query::IncludeDeleted(Query::All)` returns soft-deleted docs
- [ ] `Query::DeletedOnly` returns only soft-deleted docs
- [ ] GC starts automatically on backend init and collects tombstones past their TTL
- [ ] Offline node reconnecting with a tombstoned document triggers `ResurrectionPolicy::ReDelete` for `nodes` collection
- [ ] Offline node reconnecting with a tombstoned beacon triggers `ResurrectionPolicy::Allow`
- [ ] All existing tests in `gc_tombstone_integration.rs` and `tombstone_sync_e2e.rs` continue to pass

---

### Estimated Effort

| Step | Lines Changed | Effort |
|------|---------------|--------|
| 1. Node ID + Lamport in delete() | ~20 | Small |
| 2. Tombstone propagation from delete() | ~15 | Small |
| 3. Peer hierarchy context for direction | ~30 | Medium (needs hierarchy integration) |
| 4. Resurrection detection in sync path | ~25 | Medium |
| 5. IncludeDeleted/DeletedOnly in query() | ~20 | Small |
| 6. GC startup in backend init | ~10 | Small |
| **Tests** | ~150 | Medium |
| **Total** | **~270 lines** | **~2-3 days** |

Steps 1, 2, 5, 6 are independent and can be done in parallel. Step 3 is the most involved because it requires plumbing hierarchy info into the sync layer. Step 4 depends on step 3 for proper propagation of re-delete tombstones.

Suggested PR sequence:
1. PR 1: Steps 1 + 2 + 6 (delete correctness + propagation + GC startup)
2. PR 2: Step 5 (query filtering)
3. PR 3: Steps 3 + 4 (direction-aware propagation + resurrection detection)
