---
title: "PRD-005-storage-eviction: Priority-Based Storage Eviction"
status: Draft
issue: "#669"
epic: "#670 — QoS, TTL, and Garbage Collection"
adrs: [016, 019, 034]
---

## Implementation Spec: Priority-Based Storage Eviction Enforcement

### Objective

Wire the existing QoS eviction framework (EvictionController, QoSAwareStorage, EvictionAuditLog, RetentionPolicy) into the live storage path so that storage pressure triggers real document eviction, ordered by QoS class, with full audit logging. The framework code is complete; the work is integration plumbing plus a periodic pressure-check loop.

---

### Current State

**Implemented (in `peat-mesh/src/qos/`):**

| File | What exists | Lines |
|------|------------|-------|
| `eviction.rs` | `EvictionController` — orchestrates eviction cycles with compression-before-evict, per-cycle limits, time bounds, operator-protected docs, recent stats tracking | ~500 |
| `storage.rs` | `QoSAwareStorage` — per-class document tracking, eviction candidate selection with composite scoring (class weight × 10 + age × 5 + idle × 3 + size × 1 + pressure × 2), compression candidates, `StorageMetrics` | ~530 |
| `retention.rs` | `RetentionPolicy` / `RetentionPolicies` — per-class min/max retention, eviction priority (P5=1 first, P1=5 never), compression eligibility, `should_evict(age, pressure)` | ~480 |
| `audit.rs` | `EvictionAuditLog` — bounded `VecDeque<AuditEntry>`, `AuditAction` enum (Evicted, Compressed, Protected, FailedEviction, TtlExpired, SoftDeleted, CleanupCompleted), `AuditSummary`, JSON export | ~640 |
| `lifecycle.rs` | `LifecyclePolicy` — combined QoS + TTL (ADR-016) decision matrix, `make_lifecycle_decision()` | ~530 |

**Not wired:**

1. `EvictionController` is never instantiated in any startup path (`peat-mesh-node.rs`, `PeatMeshBuilder`, etc.)
2. No periodic storage pressure monitoring loop (contrast with `start_periodic_gc` which exists and runs)
3. `QoSAwareStorage` is not connected to `AutomergeStore` — documents are stored/deleted without QoS tracking
4. `EvictionAuditLog` is never instantiated
5. No eviction callback bridges `EvictionController.evict_document()` → `AutomergeStore.delete()`
6. No compression callback exists (Automerge `doc.save()` already compacts, but no explicit compression path)
7. Document puts in `AutomergeStore` don't register with `QoSAwareStorage`

---

### Implementation Steps

#### Step 1: Create `StorageEvictionService` integration struct

**File:** `peat-mesh/src/qos/eviction_service.rs` (new, ~120 lines)

Create a service that owns `EvictionController`, `QoSAwareStorage`, and `EvictionAuditLog`, and exposes a `start_periodic_eviction()` function following the same pattern as `start_periodic_gc()` in `garbage_collection.rs`.

```rust
pub struct StorageEvictionService {
    controller: Arc<EvictionController>,
    storage: Arc<QoSAwareStorage>,
    audit_log: Arc<EvictionAuditLog>,
    running: AtomicBool,
    check_interval: Duration,
}

impl StorageEvictionService {
    pub fn new(
        automerge_store: Arc<AutomergeStore>,
        config: EvictionConfig,
        max_storage_bytes: usize,
    ) -> Self {
        let audit_log = Arc::new(EvictionAuditLog::new(10_000));
        let storage = Arc::new(QoSAwareStorage::new(max_storage_bytes));
        let controller = Arc::new(
            EvictionController::new(storage.clone(), audit_log.clone())
                .with_config(config),
        );

        // Wire eviction callback to AutomergeStore::delete
        let store_ref = automerge_store.clone();
        controller.set_eviction_callback(Box::new(move |doc_id| {
            store_ref.delete(doc_id).map_err(|e| e.to_string())
        }));

        Self { controller, storage, audit_log, running: AtomicBool::new(false),
               check_interval: Duration::from_secs(30) }
    }

    /// Called by AutomergeStore on every put — registers doc for QoS tracking
    pub fn on_document_stored(&self, doc_id: &str, qos_class: QoSClass, size_bytes: usize) {
        self.storage.register_document(
            StoredDocument::new(doc_id, qos_class, size_bytes)
        );
    }

    /// Called by AutomergeStore on every get — updates last-accessed
    pub fn on_document_accessed(&self, doc_id: &str) {
        self.storage.touch_document(doc_id);
    }

    /// Expose metrics for monitoring
    pub fn storage_metrics(&self) -> StorageMetrics { self.storage.metrics() }
    pub fn audit_summary(&self) -> AuditSummary { self.audit_log.summary() }
    pub fn export_audit_log(&self) -> Result<String, serde_json::Error> {
        self.audit_log.export_json()
    }
}

pub fn start_periodic_eviction(
    service: Arc<StorageEvictionService>,
) -> tokio::task::JoinHandle<()> {
    let interval = service.check_interval;
    service.running.store(true, Ordering::SeqCst);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // skip first immediate tick
        while service.running.load(Ordering::SeqCst) {
            ticker.tick().await;
            if let Some(result) = service.controller.run_eviction_cycle() {
                tracing::info!(
                    docs_evicted = result.docs_evicted,
                    bytes_freed = result.bytes_freed,
                    pressure_before = %result.pressure_before,
                    pressure_after = %result.pressure_after,
                    duration_ms = result.duration_ms,
                    "Eviction cycle completed"
                );
            }
        }
    })
}
```

#### Step 2: Wire into `peat-mesh-node.rs` startup

**File:** `peat-mesh/src/bin/peat-mesh-node.rs`

Add after the garbage collector initialization (~line 207), following the exact same pattern:

```rust
// ── QoS eviction service (ADR-019 Phase 4) ─────────────────
let max_storage_bytes: usize = std::env::var("PEAT_MAX_STORAGE_BYTES")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(1024 * 1024 * 1024); // Default 1GB

let eviction_config = EvictionConfig::default(); // 90% threshold, 70% target
let eviction_service = Arc::new(StorageEvictionService::new(
    automerge_store.clone(),
    eviction_config,
    max_storage_bytes,
));
let _eviction_handle = start_periodic_eviction(eviction_service.clone());
info!("QoS eviction service started (interval=30s, threshold=90%)");
```

#### Step 3: QoS classification on document storage

**File:** `peat-mesh/src/storage/automerge_store.rs`

The `AutomergeStore` needs to notify the eviction service when documents are stored. Two approaches (recommend option B):

**Option A (observer pattern):** Add an optional `Arc<StorageEvictionService>` field to `AutomergeStore`, call `on_document_stored()` from `put_inner()`. Requires changing the constructor.

**Option B (external wiring via observer_tx):** `AutomergeStore` already has an `observer_tx: broadcast::Sender<String>` that fires on every document change. Subscribe to `observer_tx` in the eviction service startup, parse the key prefix to determine QoS class, and call `on_document_stored()`. This avoids modifying `AutomergeStore` internals.

For option B, add to `StorageEvictionService`:

```rust
pub fn start_observer(
    self: &Arc<Self>,
    store: &AutomergeStore,
    registry: Arc<QoSRegistry>,
) -> tokio::task::JoinHandle<()> {
    let mut rx = store.subscribe_observer();
    let svc = self.clone();
    tokio::spawn(async move {
        while let Ok(key) = rx.recv().await {
            // Key format: "collection:doc_id"
            if let Some((collection, _doc_id)) = key.split_once(':') {
                let data_type = DataType::from_collection_name(collection);
                let qos_class = registry.classify(data_type);
                // Estimate size (could also fetch from store)
                svc.on_document_stored(&key, qos_class, 0); // size updated on next scan
            }
        }
    })
}
```

**Size tracking challenge:** `AutomergeStore` doesn't expose document sizes on put. Two options:
- (a) Add a `put_with_size` or modify `put_inner` to return byte count (it already calls `doc.save()` which produces `Vec<u8>`)
- (b) Periodic scan: run `scan_prefix` for each collection on a slower cadence (every 5 min) and reconcile sizes in `QoSAwareStorage`

Recommend (a): modify `put_inner` to return the byte count, since it already has it:

```rust
// In automerge_store.rs, change put_inner return type:
fn put_inner(&self, key: &str, doc: &Automerge, notify: bool) -> Result<usize> {
    let bytes = doc.save();
    let size = bytes.len();
    // ... existing persistence logic ...
    Ok(size)
}
```

Then the observer can receive size info via an enhanced channel message or a separate callback.

#### Step 4: Collection-to-QoSClass mapping

**File:** `peat-protocol/src/qos/classification.rs` (already exists)

Add a `DataType::from_collection_name()` method if not already present, mapping collection prefixes to `DataType`:

```rust
impl DataType {
    pub fn from_collection_name(collection: &str) -> Self {
        match collection {
            "contact_reports" => DataType::ContactReport,
            "commands" => DataType::Command,
            "beacons" | "platforms" => DataType::PositionUpdate,
            "alerts" => DataType::Alert,
            "nodes" => DataType::NodeStatus,
            _ => DataType::DebugLog, // Default to lowest priority
        }
    }
}
```

#### Step 5: Register module and re-exports

**File:** `peat-mesh/src/qos/mod.rs`

Add `pub mod eviction_service;` and re-export `StorageEvictionService` and `start_periodic_eviction`.

#### Step 6: Expose storage pressure via broker metrics endpoint

**File:** `peat-mesh/src/broker/mod.rs` (or the appropriate HTTP handler)

Add a `/metrics/storage` or `/qos/storage` endpoint that returns `StorageMetrics` JSON:

```json
{
  "max_bytes": 1073741824,
  "used_bytes": 966367641,
  "utilization": 0.90,
  "by_class": {
    "Critical": { "doc_count": 42, "total_bytes": 1048576 },
    "Bulk": { "doc_count": 10000, "total_bytes": 500000000 }
  }
}
```

Also expose the audit log summary at `/qos/audit`.

---

### Testing Plan

#### Unit Tests (in `eviction_service.rs`)

1. **`test_eviction_service_construction`** — verify controller, storage, and audit_log are properly wired
2. **`test_eviction_callback_deletes`** — register docs in QoSAwareStorage, set storage over threshold, run eviction cycle, verify AutomergeStore deletes were called
3. **`test_p1_never_evicted`** — fill storage to 95% with P1 + P5 docs, run eviction, verify only P5 docs removed
4. **`test_eviction_order`** — register P2-P5 docs, verify P5 evicted before P4 before P3 before P2
5. **`test_retention_minimum_respected`** — P4 doc younger than 5 minutes should not be evicted even at high pressure
6. **`test_audit_log_populated`** — after eviction cycle, verify audit entries contain correct doc_id, qos_class, action, reason

#### Integration Tests (in `peat-mesh/tests/`)

7. **`test_eviction_with_real_automerge_store`** — create `AutomergeStore::in_memory()`, populate with documents across QoS classes, trigger eviction, verify documents actually deleted from store
8. **`test_periodic_eviction_runs`** — start periodic eviction with 100ms interval, add docs that exceed threshold, assert eviction occurs within 500ms
9. **`test_observer_tracks_new_documents`** — put documents via `AutomergeStore`, verify `QoSAwareStorage` registers them via the observer channel
10. **`test_metrics_endpoint`** — (if broker endpoint added) verify JSON response shape and values

#### Stress/Edge Tests

11. **`test_eviction_under_rapid_writes`** — concurrent writes while eviction is running, verify no deadlocks or panics
12. **`test_eviction_with_empty_store`** — run eviction when storage is empty, verify no errors
13. **`test_max_evictions_per_cycle_respected`** — with 2000 evictable docs and `max_evictions_per_cycle=100`, verify only 100 evicted per cycle

---

### Acceptance Criteria

- [ ] At 90% storage pressure, bulk (P5) data is evicted first, then P4, P3, P2 in order
- [ ] P1 (Critical) documents are never evicted regardless of storage pressure
- [ ] Minimum retention times are respected (P2 ≥ 24hr, P3 ≥ 1hr, P4 ≥ 5min, P5 ≥ 1min)
- [ ] `EvictionAuditLog` records every eviction with: doc_id, QoS class, action, reason, timestamp, size
- [ ] Storage pressure metric (`utilization` 0.0–1.0) is queryable for monitoring
- [ ] Periodic eviction loop runs at configurable interval (default 30s)
- [ ] `PEAT_MAX_STORAGE_BYTES` env var controls storage capacity (default 1GB)
- [ ] Operator can mark documents as protected via `EvictionController::mark_protected()`
- [ ] Eviction cycle respects `max_evictions_per_cycle` and `max_cycle_duration_ms` bounds
- [ ] No deadlocks under concurrent read/write/eviction load

---

### Estimated Effort

| Component | New lines | Modified lines | Effort |
|-----------|-----------|---------------|--------|
| `eviction_service.rs` (new integration struct) | ~120 | — | 2–3 hrs |
| `peat-mesh-node.rs` startup wiring | — | ~15 | 30 min |
| `automerge_store.rs` size tracking + observer | — | ~20 | 1 hr |
| `classification.rs` collection→DataType mapping | — | ~15 | 30 min |
| `qos/mod.rs` re-exports | — | ~5 | 10 min |
| Broker metrics endpoint | ~40 | ~10 | 1 hr |
| Unit tests (6 tests) | ~200 | — | 2 hrs |
| Integration tests (4 tests) | ~250 | — | 2–3 hrs |
| Stress tests (3 tests) | ~150 | — | 1–2 hrs |
| **Total** | **~760** | **~65** | **~1.5–2 days** |

The bulk of the logic (eviction scoring, candidate selection, retention policy enforcement, audit logging) is already implemented. This is primarily a wiring and testing effort.
