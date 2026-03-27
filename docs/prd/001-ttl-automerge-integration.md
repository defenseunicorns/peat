---
title: "PRD-001-ttl-automerge-integration: TTL and Automerge Store Integration"
status: Draft
issue: "#667"
epic: "#670 — QoS, TTL, and Garbage Collection"
adrs: [016, 019, 034]
---

## Implementation Spec: Integrate TtlManager with Automerge Store

### 1. Objective

Wire the existing `TtlManager` and `TtlConfig` into the Automerge storage backend so that documents are automatically registered with collection-specific TTLs on creation and expired by the background cleanup task.

### 2. Current State

**Implemented (not wired):**

| Component | File | What it does |
|-----------|------|-------------|
| `TtlConfig` | `peat-mesh/src/storage/ttl.rs` | Per-collection TTL durations, eviction strategies, offline retention policies. Presets: `tactical()`, `long_duration()`, `offline_node()`. |
| `TtlManager` | `peat-mesh/src/storage/ttl_manager.rs` | BTreeMap-based expiry tracking. `set_ttl(key, duration)` schedules a doc for deletion. `start_background_cleanup()` spawns tokio task every 10s. `cleanup_expired()` calls `AutomergeStore::delete()`. |
| `AutomergeStore` | `peat-mesh/src/storage/automerge_store.rs` | redb-backed document store with `put()`, `delete()`, `scan_prefix()`. Emits change notifications via broadcast channels. |
| `AutomergeSyncCoordinator` | `peat-mesh/src/storage/automerge_sync.rs` | Sync engine. Takes `Arc<AutomergeStore>` + transport. No TTL awareness. |
| `GarbageCollector` | `peat-mesh/src/qos/garbage_collection.rs` | Tombstone GC (ADR-034). Runs every 5 min. Handles resurrection policy. Separate concern from TTL. |
| Node binary | `peat-mesh/src/bin/peat-mesh-node.rs` | Creates `AutomergeStore` (line 172), `GarbageCollector` (line 201), `AutomergeSyncCoordinator` (line 213). **Never creates TtlManager.** |

**The gap:** `TtlManager` is fully implemented but never instantiated. No code path calls `set_ttl()` when documents are inserted. The background cleanup task is never started.

### 3. Implementation Steps

#### Step 1: Instantiate TtlManager in node startup

**File:** `peat-mesh/src/bin/peat-mesh-node.rs`

After the `AutomergeStore` is created (~line 174) and before the sync coordinator:

```rust
// ── TTL manager (ADR-016) ──────────────────────────────────────
let ttl_config = TtlConfig::tactical(); // or from env var PEAT_TTL_PRESET
let ttl_manager = Arc::new(TtlManager::new(automerge_store.clone(), ttl_config));
ttl_manager.start_background_cleanup();
info!("TTL manager started (preset=tactical)");
```

Add a `PEAT_TTL_PRESET` env var to select preset:
```rust
let ttl_config = match std::env::var("PEAT_TTL_PRESET").as_deref() {
    Ok("tactical") => TtlConfig::tactical(),
    Ok("long_duration") => TtlConfig::long_duration(),
    Ok("offline") => TtlConfig::offline_node(),
    _ => TtlConfig::tactical(), // default
};
```

Add `TtlManager` to the imports block (line 14-18).

#### Step 2: Register TTLs on document put

**File:** `peat-mesh/src/storage/automerge_store.rs`

Option A (recommended): Add a TTL-aware put method rather than modifying the existing `put()`:

```rust
impl AutomergeStore {
    /// Save a document and register it for TTL expiration.
    ///
    /// Delegates to `put()` for persistence, then calls
    /// `TtlManager::set_ttl()` if a TTL is configured for this
    /// document's collection.
    pub fn put_with_ttl(
        &self,
        key: &str,
        doc: &Automerge,
        ttl_manager: &TtlManager,
    ) -> Result<()> {
        self.put(key, doc)?;

        // Extract collection from key (format: "collection/doc_id")
        if let Some(collection) = key.split('/').next() {
            if let Some(ttl) = ttl_manager.config().get_collection_ttl(collection) {
                ttl_manager.set_ttl(key, ttl)?;
            }
        }

        Ok(())
    }
}
```

Option B (alternative): Give `AutomergeStore` an optional `Arc<TtlManager>` field, set via `set_ttl_manager()`, and auto-register in `put()`. This avoids changing every call site but couples the store to the TTL manager.

**Recommendation:** Start with Option A for minimal blast radius. Callers that need TTL explicitly opt in. The node binary and beacon subsystem wire it; sync-received documents skip TTL (they get their own TTL on the receiving node via the put path).

#### Step 3: Wire TTL registration into beacon storage

**File:** `peat-mesh/src/beacon/storage.rs` and its implementations

Wherever `BeaconStorage::save_beacon()` calls `AutomergeStore::put()`, change to `put_with_ttl()`. This ensures all beacon documents get the configured `beacon_ttl` (5 min for tactical preset).

If beacon storage doesn't directly call `AutomergeStore::put()`, trace the call chain and add TTL registration at the appropriate layer.

#### Step 4: Wire TTL registration into typed collections

**File:** `peat-mesh/src/storage/typed_collection.rs`

`TypedCollection` likely wraps `AutomergeStore` for type-safe upsert. Add a `ttl_manager: Option<Arc<TtlManager>>` field and use `put_with_ttl()` when set.

#### Step 5: Implement EvictionStrategy execution

**File:** New method on `TtlManager` or a new `EvictionExecutor` in `peat-mesh/src/storage/ttl_manager.rs`

```rust
impl TtlManager {
    /// Run eviction based on the configured strategy.
    /// Called from the background cleanup task alongside `cleanup_expired()`.
    pub fn run_eviction(&self) -> Result<usize> {
        match self.config.evict_strategy {
            EvictionStrategy::OldestFirst => self.evict_oldest_first(),
            EvictionStrategy::StoragePressure { threshold_pct } => {
                self.evict_under_pressure(threshold_pct)
            }
            EvictionStrategy::KeepLastN(n) => self.evict_keep_last_n(n),
            EvictionStrategy::None => Ok(0),
        }
    }

    fn evict_oldest_first(&self) -> Result<usize> {
        // For each collection with a TTL, scan documents ordered by
        // last_updated_at, delete those older than 2x TTL.
        // Uses AutomergeStore::scan_prefix() + extract timestamp.
        todo!()
    }

    fn evict_under_pressure(&self, threshold_pct: u8) -> Result<usize> {
        // Check redb file size vs configured max.
        // If over threshold, evict oldest documents.
        todo!()
    }

    fn evict_keep_last_n(&self, n: usize) -> Result<usize> {
        // For each collection, scan_prefix(), sort by timestamp, delete all but last N.
        todo!()
    }
}
```

Update `start_background_cleanup()` to also call `run_eviction()` each cycle.

#### Step 6: Implement OfflineRetentionPolicy

**File:** `peat-mesh/src/storage/ttl_manager.rs`

Add connectivity awareness:

```rust
impl TtlManager {
    /// Switch to offline TTLs (shorter durations to conserve storage)
    pub fn apply_offline_policy(&self) {
        if let Some(ref policy) = self.config.offline_policy {
            // Store the offline state so background cleanup uses offline_ttl
            self.is_offline.store(true, Ordering::Relaxed);
        }
    }

    /// Switch back to online TTLs
    pub fn apply_online_policy(&self) {
        self.is_offline.store(false, Ordering::Relaxed);
    }

    /// Get effective TTL for a collection (respects online/offline state)
    pub fn effective_ttl(&self, collection: &str) -> Option<Duration> {
        if self.is_offline.load(Ordering::Relaxed) {
            if let Some(ref policy) = self.config.offline_policy {
                return Some(policy.offline_ttl);
            }
        }
        self.config.get_collection_ttl(collection)
    }
}
```

Add an `is_offline: Arc<AtomicBool>` field to `TtlManager`.

Wire connectivity changes from `PartitionDetector` (already exists in the codebase) to call `apply_offline_policy()` / `apply_online_policy()`.

#### Step 7: Stop TTL manager on shutdown

**File:** `peat-mesh/src/bin/peat-mesh-node.rs`

In the shutdown handler (where `sync_cancel_tx.send(true)` is called), add:

```rust
ttl_manager.stop_background_cleanup();
info!("TTL manager stopped");
```

### 4. Code Sketch — Full wiring in node binary

```rust
// In peat-mesh-node.rs run(), after AutomergeStore creation:

use peat_mesh::storage::{TtlConfig, TtlManager};

// ── TTL manager ────────────────────────────────────────────
let ttl_config = match std::env::var("PEAT_TTL_PRESET").as_deref() {
    Ok("tactical") | Err(_) => TtlConfig::tactical(),
    Ok("long_duration")      => TtlConfig::long_duration(),
    Ok("offline")             => TtlConfig::offline_node(),
    Ok(other)                 => {
        warn!("Unknown PEAT_TTL_PRESET '{}', using tactical", other);
        TtlConfig::tactical()
    }
};
let ttl_manager = Arc::new(TtlManager::new(automerge_store.clone(), ttl_config));
ttl_manager.start_background_cleanup();
info!(
    preset = std::env::var("PEAT_TTL_PRESET").unwrap_or_default(),
    beacon_ttl = ?ttl_manager.config().beacon_ttl,
    position_ttl = ?ttl_manager.config().position_ttl,
    "TTL manager started"
);

// Pass ttl_manager into broker state so HTTP handlers can register TTLs:
// broker_state.ttl_manager = Some(ttl_manager.clone());
```

### 5. Testing Plan

| Test | File | What it validates |
|------|------|------------------|
| `test_document_expires_after_ttl` | `peat-mesh/src/storage/ttl_manager.rs` | Already exists (100ms TTL, verify deletion). Keep. |
| `test_background_cleanup` | `peat-mesh/src/storage/ttl_manager.rs` | Already exists (11s wait). Keep. |
| `test_put_with_ttl_registers_expiry` | `peat-mesh/src/storage/automerge_store.rs` | New: `put_with_ttl()` with tactical config -> verify `pending_count() == 1`. |
| `test_put_with_ttl_no_ttl_collection` | `peat-mesh/src/storage/automerge_store.rs` | New: `put_with_ttl()` for `hierarchical_commands/` (no TTL) -> `pending_count() == 0`. |
| `test_eviction_oldest_first` | `peat-mesh/src/storage/ttl_manager.rs` | New: Insert 20 beacon docs with timestamps, run eviction, verify oldest removed. |
| `test_eviction_keep_last_n` | `peat-mesh/src/storage/ttl_manager.rs` | New: Insert 15 docs, KeepLastN(5), verify only 5 remain. |
| `test_offline_policy_applies_shorter_ttl` | `peat-mesh/src/storage/ttl_manager.rs` | New: `apply_offline_policy()`, verify `effective_ttl()` returns offline duration. |
| `test_ttl_preset_from_env` | `peat-mesh/src/bin/` (integration) | New: Set `PEAT_TTL_PRESET=offline`, verify `TtlConfig::offline_node()` is used. |
| `test_synced_doc_gets_ttl_on_receiver` | `peat-mesh/src/storage/automerge_sync.rs` | New: Simulate sync receive, verify TTL set on receiving node. |

### 6. Acceptance Criteria

- [ ] `TtlManager` is instantiated in `peat-mesh-node.rs` with a configurable preset via `PEAT_TTL_PRESET` env var
- [ ] `start_background_cleanup()` is called at startup, `stop_background_cleanup()` at shutdown
- [ ] Beacon documents expire after `beacon_ttl` (5 min tactical, 30s offline)
- [ ] Position documents expire after `position_ttl` (10 min tactical, 60s offline)
- [ ] `put_with_ttl()` method exists on `AutomergeStore` and auto-registers TTL based on collection name
- [ ] `EvictionStrategy::OldestFirst` and `KeepLastN` are implemented and execute during background cleanup
- [ ] `OfflineRetentionPolicy` applies shorter TTLs when `apply_offline_policy()` is called
- [ ] All existing `ttl_manager.rs` tests continue to pass
- [ ] At least 5 new integration tests covering the wiring (see Testing Plan above)
- [ ] No regression in sync behavior (synced documents do not skip TTL on the receiving node)

### 7. Estimated Effort

**Size: Small-Medium (~50-120 lines of new wiring code, ~150-200 lines of new tests)**

- Step 1 (node startup wiring): ~20 lines — 1 hour
- Step 2 (`put_with_ttl`): ~15 lines — 1 hour
- Steps 3-4 (beacon/typed_collection wiring): ~20 lines — 2 hours
- Step 5 (eviction execution): ~60 lines — 3 hours
- Step 6 (offline policy): ~30 lines — 2 hours
- Step 7 (shutdown): ~5 lines — 15 min
- Tests: ~200 lines — 3 hours

**Total: ~1.5 days of focused work**

The core wiring (Steps 1-3, 7) is the minimum viable integration and can be done in half a day. Steps 5-6 are enhancements that can follow in a subsequent PR if needed.
