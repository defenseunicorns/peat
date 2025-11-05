# Data Sync Abstraction Layer - Refactoring Plan

## Status

✅ **Task 1: Create trait definitions** - COMPLETED (commit: dc407d3)
✅ **Task 2: Implement DittoBackend** - COMPLETED (commit: dc407d3)
✅ **Task 3: Refactor CellStore** - COMPLETED (library compiles, E2E tests pending)
⏭️ **Task 4: Update E2E tests** - NEXT
🔜 **Task 5: Refactor NodeStore** - PENDING
🔜 **Task 6: Verify no regressions** - PENDING

---

## Task 3: Refactor CellStore to be Generic

### Current State
**File**: `cap-protocol/src/storage/cell_store.rs`

CellStore currently:
- Takes `DittoStore` directly as dependency
- Creates Ditto-specific `SyncSubscription`
- Uses DittoStore methods (`upsert`, `query`, `remove`)
- Converts between `CellState` and `serde_json::Value`

### Refactoring Strategy

#### 1. Make CellStore Generic
```rust
pub struct CellStore<B: DataSyncBackend> {
    backend: Arc<B>,
    _sync_sub: SyncSubscription,  // Use abstraction's SyncSubscription
}
```

#### 2. Update Constructor
```rust
impl<B: DataSyncBackend> CellStore<B> {
    pub async fn new(backend: Arc<B>) -> Result<Self> {
        // Use abstract SyncEngine to create subscription
        let sync_sub = backend
            .sync_engine()
            .subscribe(CELL_COLLECTION, &Query::All)
            .await?;

        Ok(Self {
            backend,
            _sync_sub: sync_sub,
        })
    }
}
```

#### 3. Convert CRUD Operations
Use `DocumentStore` trait methods instead of `DittoStore`:

```rust
// OLD: Direct DQL strings
let where_clause = format!("cell_id == '{}'", cell_id);
let docs = self.store.query(CELL_COLLECTION, &where_clause).await?;

// NEW: Abstract Query types
let query = Query::Eq {
    field: "cell_id".to_string(),
    value: Value::String(cell_id.to_string()),
};
let docs = self.backend
    .document_store()
    .query(CELL_COLLECTION, &query)
    .await?;
```

#### 4. Document Conversion Helpers
Add helper methods to convert between `CellState` and `Document`:

```rust
impl<B: DataSyncBackend> CellStore<B> {
    fn cell_to_document(cell: &CellState) -> Result<Document> {
        let fields = serde_json::to_value(cell)?
            .as_object()
            .ok_or_else(|| Error::Internal("Failed to serialize cell".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Document {
            id: None,  // Let backend assign ID
            fields,
            updated_at: SystemTime::now(),
        })
    }

    fn document_to_cell(doc: &Document) -> Result<CellState> {
        let json_val = serde_json::to_value(&doc.fields)?;
        Ok(serde_json::from_value(json_val)?)
    }
}
```

#### 5. Update All Methods
Transform each method to use abstraction:
- `store_cell()` → use `DocumentStore::upsert()`
- `get_cell()` → use `DocumentStore::query()` with `Query::Eq`
- `get_valid_cells()` → use `Query::All` and filter in code
- `get_cells_by_zone()` → use `Query::Eq` on platoon_id
- `get_cells_with_capability()` → use `Query::All` and filter
- `get_available_cells()` → use `Query::All` and filter
- `delete_cell()` → use `DocumentStore::remove()`

### Implementation Checklist

- [ ] Add generic parameter `B: DataSyncBackend` to CellStore
- [ ] Update constructor to use `SyncEngine::subscribe()`
- [ ] Add `cell_to_document()` helper
- [ ] Add `document_to_cell()` helper
- [ ] Refactor `store_cell()` to use DocumentStore
- [ ] Refactor `get_cell()` to use Query abstraction
- [ ] Refactor `get_valid_cells()` to use Query::All
- [ ] Refactor `get_cells_by_zone()` to use Query::Eq
- [ ] Refactor `get_cells_with_capability()` to use Query::All
- [ ] Refactor `get_available_cells()` to use Query::All
- [ ] Refactor mutation methods (add_member, remove_member, etc.)
- [ ] Refactor `delete_cell()` to use DocumentStore::remove()
- [ ] Update `store()` accessor to return backend reference
- [ ] Update tests to use `DittoBackend` instead of `DittoStore`
- [ ] Verify all existing tests still pass

### Breaking Changes
- `CellStore::new()` signature changes to take `Arc<B: DataSyncBackend>`
- `CellStore::store()` returns `&B` instead of `&DittoStore`
- Constructor is now `async` (requires await at call sites)

### Migration Path for Callers
```rust
// OLD
let ditto_store = DittoStore::new(config)?;
let cell_store = CellStore::new(ditto_store);

// NEW
let backend = DittoBackend::new();
backend.initialize(config).await?;
backend.sync_engine().start_sync().await?;
let cell_store = CellStore::new(Arc::new(backend)).await?;
```

---

## Task 4: Refactor NodeStore (Similar Pattern)

**File**: `cap-protocol/src/storage/node_store.rs`

Apply same pattern as CellStore:
1. Generic over `B: DataSyncBackend`
2. Use `DocumentStore` for CRUD
3. Use `Query` abstraction instead of DQL
4. Add `node_to_document()` / `document_to_node()` helpers
5. Update all callers

---

## Task 5: Update E2EHarness

**File**: `cap-protocol/src/testing/e2e_harness.rs`

Update test harness to work with abstraction:
1. Make generic over `B: DataSyncBackend` or use `DittoBackend` directly
2. Update `setup_node()` to create backend instead of `DittoStore`
3. Pass backend to `CellStore::new()` and `NodeStore::new()`
4. Ensure backward compatibility with existing E2E tests

---

## Task 6: Verification

Run full test suite to ensure no regressions:
```bash
make test           # All tests
make test-e2e       # E2E tests specifically
make coverage       # Check coverage hasn't decreased
```

Expected results:
- All existing tests pass
- No behavior changes
- Test execution time similar
- Coverage maintained or improved

---

## Notes

### Why Keep DittoStore?
We keep the existing `DittoStore` because:
1. It's a thin wrapper with useful helpers (query, upsert methods)
2. `DittoBackend` wraps it to implement traits
3. No need to duplicate Ditto-specific initialization logic
4. Cleaner separation: DittoStore = Ditto SDK wrapper, DittoBackend = trait implementation

### Testing Strategy
- Unit tests verify conversion logic
- Integration tests verify single-node CRUD
- E2E tests verify multi-peer sync (unchanged)

### Performance Impact
- Minimal: trait dispatch via vtables is negligible
- Extra Arc clone on backend access (already cloning DittoStore)
- Query translation adds minimal overhead

---

## Success Criteria

✅ All existing tests pass
✅ No behavior changes (E2E tests prove this)
✅ Code compiles with no warnings
✅ Clippy passes
✅ Coverage maintained
✅ CellStore and NodeStore are backend-agnostic
✅ Can swap in mock backend for testing
✅ Foundation for Automerge backend (ADR-007)
