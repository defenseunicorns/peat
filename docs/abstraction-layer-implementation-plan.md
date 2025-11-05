# Data Sync Abstraction Layer - Implementation Plan

**Branch:** `datasync-abstraction-layer`
**Date:** 2025-11-05
**Epic:** Decouple from Ditto, enable Automerge alternative

## Context Summary

### Current State (After E7)
- ✅ **E1-E6**: Core CAP protocol with Ditto SDK
- ✅ **E7**: Differential updates framework (protocol-level deltas)
- ❌ **Hard Ditto dependency**: All storage code directly uses Ditto SDK
- ❌ **No abstraction**: Cannot swap sync engines

### Relevant ADRs

**ADR-001: CAP Protocol POC**
- O(n log n) message complexity target
- 100+ node scalability requirement
- Network efficiency: 95%+ bandwidth reduction
- Eventual consistency via CRDTs

**ADR-002: Beacon Storage Architecture**
- One document per node for beacons
- Geographic discovery pattern
- Will be superseded by abstraction layer

**ADR-004: Human-Machine Cell Composition**
- Authority levels for human-in-the-loop
- Military rank representation
- Human override capabilities
- Not yet implemented, but abstraction must support

**ADR-005: Data Synchronization Abstraction Layer** ⭐ **PRIMARY**
- Defines four core traits: DocumentStore, PeerDiscovery, SyncEngine, DataSyncBackend
- Enables both Ditto and custom implementations
- Migration path to production systems
- **Status**: Proposed → We're implementing this now

**ADR-006: Security, Authentication, Authorization**
- Zero-trust architecture
- Mutual TLS + shared secrets
- Role-based access control
- Not yet implemented, but abstraction must support

**ADR-007: Automerge-Based Sync Engine** ⭐ **TARGET**
- GOTS/OSS strategy for DoD/NATO
- Eliminates vendor lock-in
- 85-95% compression (vs Ditto's CBOR)
- NATO STANAG candidate
- **Status**: Proposed → Enabled by this abstraction work

## Strategic Goals

### Immediate (This Branch)
1. **Decouple CAP protocol from Ditto SDK**
   - Create trait-based abstraction
   - Refactor existing code to use traits
   - Maintain 100% backward compatibility
   - All existing tests pass

### Medium-Term (Post-Abstraction)
2. **Enable Automerge Implementation**
   - Implement traits with automerge-repo
   - Run CAP protocol on Automerge backend
   - A/B test Ditto vs Automerge performance

### Long-Term (Production)
3. **Tactical Deployment**
   - Pure open-source stack (no licensing)
   - NATO-standardizable architecture
   - Multi-vendor ecosystem

## Architecture Overview

### Current Architecture (Before)
```
┌─────────────────────────────────┐
│   CAP Protocol Business Logic   │
│  (CellStore, NodeStore, etc)    │
└────────────┬────────────────────┘
             │ Directly uses
             ▼
     ┌───────────────┐
     │  Ditto SDK    │ ← Hard dependency
     └───────────────┘
```

### Target Architecture (After)
```
┌─────────────────────────────────┐
│   CAP Protocol Business Logic   │
│  (CellStore, NodeStore, etc)    │
└────────────┬────────────────────┘
             │ Uses traits
             ▼
     ┌───────────────────────┐
     │  Sync Abstraction     │ ← New layer
     │  (Traits defined)     │
     └───────┬───────────────┘
             │
      ┌──────┴───────┐
      ▼              ▼
 ┌──────────┐   ┌──────────────┐
 │  Ditto   │   │  Automerge   │ ← Swappable
 │ Backend  │   │   Backend    │
 └──────────┘   └──────────────┘
```

## Implementation Phases

### Phase 1: Create Trait Definitions (Task 1)

**File**: `cap-protocol/src/sync/mod.rs`

**Define traits from ADR-005:**

```rust
pub mod traits;
pub mod types;
pub mod ditto;  // Ditto implementation

pub use traits::*;
pub use types::*;
```

**Traits to implement:**
1. `DocumentStore` - CRUD + queries + observers
2. `PeerDiscovery` - mDNS, TCP, peer management
3. `SyncEngine` - Start/stop sync, subscriptions
4. `DataSyncBackend` - Lifecycle + composition

**Supporting types:**
- `Document` - Unified document representation
- `Query` - Backend-agnostic query abstraction
- `PeerInfo` - Peer metadata
- `BackendConfig` - Configuration
- `TransportConfig` - Network settings

**Success Criteria:**
- Traits compile
- Types are well-documented
- No implementation yet (pure interfaces)

---

### Phase 2: Implement DittoBackend (Task 2)

**File**: `cap-protocol/src/sync/ditto.rs`

**Wrap existing DittoStore to implement traits:**

```rust
pub struct DittoBackend {
    ditto: Arc<Ditto>,
    // ... internal state
}

impl DataSyncBackend for DittoBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
        // Setup Ditto with config
    }

    fn document_store(&self) -> &dyn DocumentStore {
        self as &dyn DocumentStore
    }

    fn peer_discovery(&self) -> &dyn PeerDiscovery {
        self as &dyn PeerDiscovery
    }

    fn sync_engine(&self) -> &dyn SyncEngine {
        self as &dyn SyncEngine
    }
}

impl DocumentStore for DittoBackend {
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId> {
        // Map to Ditto DQL INSERT/UPDATE
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
        // Map to Ditto DQL SELECT
    }

    // ... rest of trait
}

// Similarly implement PeerDiscovery and SyncEngine
```

**Success Criteria:**
- All four traits fully implemented
- Wraps existing DittoStore functionality
- Unit tests for each trait method
- No behavior changes (pure wrapper)

---

### Phase 3: Refactor CellStore (Task 3)

**File**: `cap-protocol/src/storage/cell_store.rs`

**Current:**
```rust
pub struct CellStore {
    ditto: Arc<Ditto>,
    _sync_sub: SyncSubscription,
}
```

**Target:**
```rust
pub struct CellStore<B: DataSyncBackend> {
    backend: Arc<B>,
    _sync_sub: SyncSubscription,
}

impl<B: DataSyncBackend> CellStore<B> {
    pub fn new(backend: Arc<B>) -> Result<Self> {
        let sync_sub = backend
            .sync_engine()
            .subscribe(CELL_COLLECTION, &Query::All)
            .await?;

        Ok(Self {
            backend,
            _sync_sub: sync_sub,
        })
    }

    pub async fn store_cell(&self, cell: &CellState) -> Result<()> {
        let doc = Document::from_cell(cell)?;
        self.backend
            .document_store()
            .upsert(CELL_COLLECTION, doc)
            .await?;
        Ok(())
    }

    pub async fn get_cell(&self, cell_id: &str) -> Result<Option<CellState>> {
        let query = Query::Eq {
            field: "id".to_string(),
            value: Value::String(cell_id.to_string()),
        };

        let docs = self.backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        docs.first()
            .map(|doc| CellState::from_document(doc))
            .transpose()
    }

    // ... rest of methods
}
```

**Success Criteria:**
- CellStore is generic over `DataSyncBackend`
- All methods use trait abstraction
- Existing tests pass with `DittoBackend`
- No functional changes

---

### Phase 4: Refactor NodeStore (Task 4)

**File**: `cap-protocol/src/storage/node_store.rs`

**Same pattern as CellStore:**
- Make generic over `B: DataSyncBackend`
- Replace direct Ditto calls with trait methods
- Maintain exact same behavior

**Success Criteria:**
- NodeStore is generic
- All tests pass
- No functional changes

---

### Phase 5: Update E2EHarness (Task 5)

**File**: `cap-protocol/src/testing/e2e_harness.rs`

**Make test harness generic:**

```rust
pub struct E2EHarness<B: DataSyncBackend> {
    session_id: String,
    backends: Vec<Arc<B>>,
}

impl<B: DataSyncBackend> E2EHarness<B> {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            backends: Vec::new(),
        }
    }

    pub async fn create_backend(&mut self) -> Result<Arc<B>> {
        let config = BackendConfig {
            app_id: format!("cap-test-{}", self.session_id),
            persistence_dir: self.temp_dir()?,
            // ...
        };

        let backend = B::new(config).await?;
        let arc_backend = Arc::new(backend);
        self.backends.push(arc_backend.clone());
        Ok(arc_backend)
    }
}

// Keep existing Ditto-specific methods for backward compat
impl E2EHarness<DittoBackend> {
    pub async fn create_ditto_store(&mut self) -> Result<Arc<Ditto>> {
        // Existing implementation
    }
}
```

**Success Criteria:**
- Harness works with any backend
- Existing Ditto tests still work
- Ready for Automerge backend tests

---

### Phase 6: Verify No Regressions (Task 6)

**Run full test suite:**

```bash
# Unit tests
cargo test --lib -p cap-protocol

# Integration tests
cargo test --test '*_integration' -p cap-protocol

# E2E tests (requires DITTO_APP_ID)
DITTO_APP_ID=<app-id> cargo test --test '*_e2e' -p cap-protocol

# Baseline tests
DITTO_APP_ID=<app-id> cargo test --test baseline_ditto_bandwidth_e2e
DITTO_APP_ID=<app-id> cargo test --test delta_sync_e2e
```

**Success Criteria:**
- ✅ All 62 unit tests pass
- ✅ All integration tests pass
- ✅ All E2E tests pass
- ✅ Baseline tests show same metrics
- ✅ No performance regressions

---

## File Structure

```
cap-protocol/src/
├── sync/                       ← NEW
│   ├── mod.rs                 ← Re-exports
│   ├── traits.rs              ← Core trait definitions
│   ├── types.rs               ← Supporting types
│   └── ditto.rs               ← Ditto implementation
├── storage/
│   ├── mod.rs
│   ├── ditto_store.rs         ← May deprecate or keep as helper
│   ├── cell_store.rs          ← Refactor to use traits
│   ├── node_store.rs          ← Refactor to use traits
│   └── throttled_node_store.rs ← Refactor if needed
├── testing/
│   └── e2e_harness.rs         ← Refactor to be generic
└── lib.rs                     ← Add `pub mod sync;`
```

## Testing Strategy

### Unit Tests
- Each trait method gets dedicated test
- Mock backend for isolated testing
- No Ditto dependency in unit tests

### Integration Tests
- Test DittoBackend integration
- Verify trait → Ditto mapping works
- Use real Ditto for integration

### E2E Tests
- Existing tests run with DittoBackend
- No changes to test logic
- Same assertions, same results

### Regression Tests
- Compare before/after metrics
- Bandwidth should be identical
- Convergence time should be identical
- No performance degradation

## Success Criteria (Branch Complete)

- [ ] All traits defined and documented
- [ ] DittoBackend fully implements all traits
- [ ] CellStore refactored to use abstraction
- [ ] NodeStore refactored to use abstraction
- [ ] E2EHarness supports generic backends
- [ ] All existing tests pass (100% pass rate)
- [ ] No performance regressions
- [ ] Code formatted with `cargo fmt --all`
- [ ] Clippy passes with no warnings
- [ ] Documentation updated in relevant files

## Future Work (Not This Branch)

### Automerge Backend Implementation
- New file: `cap-protocol/src/sync/automerge.rs`
- Implement `DataSyncBackend` using automerge-repo
- Use automerge's sync protocol for peers
- Leverage columnar storage for bandwidth efficiency

### Performance Comparison
- A/B test Ditto vs Automerge
- Measure bandwidth: CBOR vs columnar
- Measure convergence time
- Validate O(n log n) scaling

### Security Integration (ADR-006)
- Add authentication layer to traits
- Implement mutual TLS
- Add RBAC to document queries
- Cryptographic signatures

### Production Hardening
- Error handling improvements
- Retry logic
- Circuit breakers
- Observability (metrics, traces)

## Migration Path for Future Backends

Adding a new backend is straightforward:

1. **Create new module**: `cap-protocol/src/sync/newbackend.rs`
2. **Implement traits**:
   ```rust
   pub struct NewBackend { /* ... */ }
   impl DataSyncBackend for NewBackend { /* ... */ }
   impl DocumentStore for NewBackend { /* ... */ }
   impl PeerDiscovery for NewBackend { /* ... */ }
   impl SyncEngine for NewBackend { /* ... */ }
   ```
3. **Write backend-specific tests**
4. **Update E2EHarness** to support new backend
5. **Run test suite** to verify behavior

No changes to CAP protocol business logic required!

## References

- **ADR-005**: Data Synchronization Abstraction Layer (trait definitions)
- **ADR-007**: Automerge-Based Sync Engine (target implementation)
- **E7 Work**: Differential updates framework (protocol-level optimization)
- **E2E Test Harness**: `cap-protocol/src/testing/e2e_harness.rs`
- **Current Storage**: `cap-protocol/src/storage/`

## Next Steps

1. Review this plan with team
2. Start with Task 1: Create trait definitions
3. Implement incrementally (one task at a time)
4. Run tests after each task
5. Commit when all tests pass

---

**Last Updated**: 2025-11-05
**Status**: Ready to implement
