# ADR-005: Data Synchronization Abstraction Layer

**Status**: Proposed
**Date**: 2025-11-04
**Authors**: Claude, Kit Plummer
**Supersedes**: ADR-002 (Beacon Storage Architecture)

## Context

HIVE Protocol currently has a hard dependency on Ditto SDK for CRDT synchronization, peer discovery, and data persistence. While Ditto provides excellent P2P mesh capabilities, we've encountered several limitations:

### Issues with Current Ditto Integration

1. **Document Update Semantics**: Ditto's DQL `INSERT` creates new documents instead of updating existing ones, requiring complex workarounds for proper CRDT semantics
2. **Limited Query Capabilities**: No support for ORDER BY in DQL, requiring application-level sorting
3. **Proprietary Protocol**: CBOR-based wire protocol is less efficient than columnar approaches (Automerge achieves 85-95% compression vs Ditto's typical compression)
4. **Vendor Lock-in**: Cannot easily switch backends or run in environments where Ditto is unavailable
5. **Testing Complexity**: Real Ditto instances required for E2E tests, making CI/CD brittle

### Strategic Need for Abstraction

The new `crdt-edge` implementation plan (see `CAP_Rust_Implementation_Plan.md`) proposes a custom CRDT library with:
- Automerge-style columnar wire protocol (superior compression)
- Modular architecture (use only what you need)
- CAP-specific extensions (hierarchical organization, capability composition)
- General-purpose applicability (mobile apps, IoT, collaborative tools)

**The fundamental requirement**: Support **both** Ditto and custom implementations simultaneously, allowing:
1. Continued use of Ditto for rapid prototyping and current deployments
2. Migration path to custom implementation for production tactical systems
3. A/B testing between implementations for performance validation
4. Graceful fallback if one backend is unavailable

## Decision

We will define a **Data Synchronization Abstraction Layer** consisting of four core traits that completely isolate HIVE Protocol business logic from the underlying sync engine:

### Core Abstraction Traits

```rust
/// Trait 1: Document Storage and Retrieval
pub trait DocumentStore: Send + Sync {
    /// Store or update a document
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId>;

    /// Retrieve documents matching query
    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>>;

    /// Remove a document
    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()>;

    /// Register observer for live updates
    fn observe(&self, collection: &str, query: &Query) -> Result<ChangeStream>;
}

/// Trait 2: Peer Discovery and Connection Management
pub trait PeerDiscovery: Send + Sync {
    /// Start discovery mechanism
    async fn start(&self) -> Result<()>;

    /// Get list of discovered peers
    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>>;

    /// Manually add a peer
    async fn add_peer(&self, address: &str, transport: TransportType) -> Result<()>;

    /// Wait for specific peer connection
    async fn wait_for_peer(
        &self,
        peer_id: &PeerId,
        timeout: Duration
    ) -> Result<()>;

    /// Register callback for peer events
    fn on_peer_event(&self, callback: Box<dyn Fn(PeerEvent) + Send + Sync>);
}

/// Trait 3: Synchronization Control
pub trait SyncEngine: Send + Sync {
    /// Start synchronization with peers
    async fn start_sync(&self) -> Result<()>;

    /// Stop synchronization
    async fn stop_sync(&self) -> Result<()>;

    /// Create sync subscription for collection
    async fn subscribe(
        &self,
        collection: &str,
        query: &Query
    ) -> Result<SyncSubscription>;

    /// Set sync priority (optional - for priority-based sync)
    async fn set_priority(&self, collection: &str, priority: Priority) -> Result<()>;
}

/// Trait 4: Lifecycle Management
pub trait DataSyncBackend: Send + Sync {
    /// Initialize backend with configuration
    async fn initialize(&self, config: BackendConfig) -> Result<()>;

    /// Shutdown gracefully
    async fn shutdown(&self) -> Result<()>;

    /// Get references to component traits
    fn document_store(&self) -> &dyn DocumentStore;
    fn peer_discovery(&self) -> &dyn PeerDiscovery;
    fn sync_engine(&self) -> &dyn SyncEngine;
}
```

### Supporting Types

```rust
/// Unified document representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<DocumentId>,
    pub fields: HashMap<String, Value>,
    pub updated_at: Timestamp,
}

/// Query abstraction that works for both backends
#[derive(Debug, Clone)]
pub enum Query {
    /// Simple equality match
    Eq { field: String, value: Value },

    /// Multiple conditions (AND)
    And(Vec<Query>),

    /// Multiple conditions (OR)
    Or(Vec<Query>),

    /// All documents in collection
    All,

    /// Custom backend-specific query
    Custom(String),
}

/// Peer information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub address: Option<String>,
    pub transport: TransportType,
    pub connected: bool,
    pub last_seen: Timestamp,
}

/// Backend configuration
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub app_id: String,
    pub persistence_dir: PathBuf,
    pub shared_key: Option<String>,
    pub transport: TransportConfig,
}

/// Transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub tcp_listen_port: Option<u16>,
    pub tcp_connect_address: Option<String>,
    pub enable_mdns: bool,
    pub enable_bluetooth: bool,
}
```

## Implementation Strategy

### Phase 1: Refactor Existing Code to Use Abstraction (Week 1-2)

1. **Create `hive-protocol/src/sync/` module** with trait definitions
2. **Implement `DittoBackend`** that wraps existing DittoStore
3. **Update `CellStore` and `NodeStore`** to use trait instead of concrete DittoStore
4. **No behavior changes** - pure refactoring for abstraction

```rust
// Example: CellStore becomes generic over backend
pub struct CellStore<B: DataSyncBackend> {
    backend: Arc<B>,
    collection: &'static str,
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
            collection: CELL_COLLECTION,
            _sync_sub: sync_sub,
        })
    }

    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        let doc = Document {
            id: None,
            fields: serde_json::to_value(cell)?
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .map(|(k, v)| (k, v))
                .collect(),
            updated_at: cell.updated_at,
        };

        self.backend
            .document_store()
            .upsert(self.collection, doc)
            .await
    }

    pub async fn get_cell(&self, cell_id: &str) -> Result<Option<CellState>> {
        let query = Query::Eq {
            field: "cell_id".to_string(),
            value: Value::String(cell_id.to_string()),
        };

        let mut docs = self.backend
            .document_store()
            .query(self.collection, &query)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        // Sort by timestamp to get latest (LWW semantics)
        docs.sort_by_key(|d| std::cmp::Reverse(d.updated_at));

        let cell: CellState = serde_json::from_value(
            serde_json::to_value(&docs[0].fields)?
        )?;
        Ok(Some(cell))
    }
}
```

### Phase 2: Implement Custom Backend (Weeks 3-12)

Follow the phased approach from `CAP_Rust_Implementation_Plan.md`:

1. **Weeks 1-4**: Core CRDTs and merge logic
2. **Weeks 5-8**: Columnar encoding/decoding
3. **Weeks 9-12**: Storage layer (RocksDB)
4. **Weeks 13-16**: Discovery and transport
5. **Weeks 17-20**: Sync protocol
6. **Weeks 21-24**: CAP-specific extensions

```rust
// Custom backend implementation
pub struct CrdtEdgeBackend {
    repo: Repository,
    discovery: Box<dyn PeerDiscovery>,
    sync: Arc<SyncEngine>,
}

impl DataSyncBackend for CrdtEdgeBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
        // Initialize Repository with RocksDB storage
        self.repo.initialize(&config.persistence_dir).await?;

        // Start discovery (mDNS + manual TCP)
        self.discovery.start().await?;

        Ok(())
    }

    fn document_store(&self) -> &dyn DocumentStore {
        &self.repo as &dyn DocumentStore
    }

    fn peer_discovery(&self) -> &dyn PeerDiscovery {
        &*self.discovery
    }

    fn sync_engine(&self) -> &dyn SyncEngine {
        &*self.sync
    }
}

// Repository implements DocumentStore
impl DocumentStore for Repository {
    async fn upsert(&self, collection: &str, document: Document) -> Result<DocumentId> {
        let coll = self.collection(collection);

        // Check if document exists
        if let Some(existing_id) = document.id {
            // Update existing document (proper LWW semantics)
            coll.update(&existing_id, document.fields).await?;
            Ok(existing_id)
        } else {
            // Insert new document
            coll.insert(document.fields).await
        }
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
        let coll = self.collection(collection);

        // Convert abstract Query to concrete query
        let results = match query {
            Query::Eq { field, value } => {
                coll.find(&format!("{} == :value", field))
                    .bind("value", value)
                    .exec()
                    .await?
            }
            Query::All => {
                coll.find_all().await?
            }
            _ => unimplemented!("Complex queries"),
        };

        // Convert to unified Document format
        Ok(results.into_iter().map(|doc| Document {
            id: Some(doc.id),
            fields: doc.data,
            updated_at: doc.updated_at,
        }).collect())
    }
}
```

### Phase 3: A/B Testing and Migration (Weeks 25-28)

1. **Dual Backend Support**: Run both Ditto and custom side-by-side
2. **Performance Benchmarks**: Compare sync latency, bandwidth, CPU usage
3. **Correctness Validation**: Verify both backends produce identical results
4. **Gradual Migration**: Move non-critical collections first, then mission-critical

```rust
// Test harness supports both backends
pub enum TestBackend {
    Ditto(Arc<DittoBackend>),
    CrdtEdge(Arc<CrdtEdgeBackend>),
}

impl TestBackend {
    pub fn as_trait(&self) -> &dyn DataSyncBackend {
        match self {
            TestBackend::Ditto(b) => b.as_ref() as &dyn DataSyncBackend,
            TestBackend::CrdtEdge(b) => b.as_ref() as &dyn DataSyncBackend,
        }
    }
}

// Tests run against both backends
#[tokio::test]
async fn test_cell_sync_both_backends() {
    for backend_type in [BackendType::Ditto, BackendType::CrdtEdge] {
        let backend1 = create_backend(backend_type, "node1").await;
        let backend2 = create_backend(backend_type, "node2").await;

        let cell_store1 = CellStore::new(backend1).unwrap();
        let cell_store2 = CellStore::new(backend2).unwrap();

        // Same test logic works for both backends
        // ...
    }
}
```

## Consequences

### Positive

1. **Vendor Independence**: Can switch backends without changing business logic
2. **Improved Testing**: Mock implementations for unit tests, both real backends for integration tests
3. **Performance Optimization**: Custom backend can optimize for CAP's specific access patterns
4. **Cost Reduction**: Eliminate Ditto licensing costs for production deployments (if needed)
5. **Protocol Efficiency**: Columnar encoding yields 85-95% compression vs CBOR
6. **Flexibility**: Use Ditto for rapid prototyping, custom for production
7. **Graceful Degradation**: Fall back to Ditto if custom backend has issues

### Negative

1. **Implementation Complexity**: Must maintain two backend implementations
2. **Feature Parity**: Custom backend must match Ditto's P2P mesh capabilities
3. **Development Time**: 24+ weeks to reach feature parity with Ditto
4. **Testing Burden**: Must test both backends for every feature
5. **Trait Limitations**: Rust traits may constrain design choices
6. **Migration Risk**: Moving from Ditto to custom backend requires careful validation

### Neutral

1. **API Surface**: Abstraction layer adds an indirection layer
2. **Learning Curve**: Developers must understand abstraction concepts
3. **Documentation**: Must document both backend-agnostic and backend-specific features

## Alternatives Considered

### Alternative 1: Stay with Ditto Only

**Pros**:
- No abstraction overhead
- Proven P2P mesh
- Faster development

**Cons**:
- Vendor lock-in
- Cannot optimize for CAP's specific needs
- Licensing costs
- Less efficient wire protocol

**Verdict**: Rejected - Strategic risk of vendor dependency outweighs benefits

### Alternative 2: Replace Ditto Immediately

**Pros**:
- No abstraction complexity
- Optimized for CAP from day one

**Cons**:
- 6+ months before any functional system
- High risk if custom implementation fails
- No fallback option

**Verdict**: Rejected - Too risky without proven alternative

### Alternative 3: Use Existing CRDT Libraries (Automerge, Yrs)

**Pros**:
- Battle-tested CRDTs
- Active communities

**Cons**:
- Automerge lacks discovery/networking
- Yrs is WASM-focused, not suitable for embedded
- Neither has CAP-specific hierarchical features
- Still requires integration layer

**Verdict**: Rejected - Insufficient for tactical edge requirements

## Implementation Plan

### Milestone 1: Abstraction Layer (Weeks 1-2)

- [ ] Define all abstraction traits in `hive-protocol/src/sync/traits.rs`
- [ ] Implement `DittoBackend` wrapper
- [ ] Refactor `CellStore` to be generic over `DataSyncBackend`
- [ ] Refactor `NodeStore` to be generic over `DataSyncBackend`
- [ ] Update all E2E tests to use abstraction
- [ ] **Success Criteria**: All existing tests pass with zero behavior changes

### Milestone 2: Custom Backend MVP (Weeks 3-12)

Follow `CAP_Rust_Implementation_Plan.md` phases 1-3:
- [ ] Core CRDT types (LWW-Register, OR-Set, PN-Counter)
- [ ] Columnar encoding/decoding
- [ ] RocksDB storage integration
- [ ] Basic in-memory sync (no networking yet)
- [ ] **Success Criteria**: Two in-process instances can sync documents

### Milestone 3: Networking (Weeks 13-16)

- [ ] TCP transport implementation
- [ ] mDNS discovery
- [ ] Sync protocol over TCP
- [ ] **Success Criteria**: Two processes on same machine can discover and sync

### Milestone 4: CAP Extensions (Weeks 17-20)

- [ ] Hierarchical organization traits
- [ ] Capability composition engine
- [ ] Priority-based sync queues
- [ ] **Success Criteria**: Squad formation works with custom backend

### Milestone 5: Production Readiness (Weeks 21-24)

- [ ] Performance benchmarks (vs Ditto)
- [ ] Stress testing (network partitions, reconnections)
- [ ] E2E test parity (all tests pass with both backends)
- [ ] **Success Criteria**: Custom backend matches Ditto's reliability

## References

- [CAP_Rust_Implementation_Plan.md](../CAP_Rust_Implementation_Plan.md) - Detailed custom implementation design
- [ADR-001](001-hive-protocol-poc.md) - Original Ditto integration decision
- [ADR-002](002-beacon-storage-architecture.md) - Current Ditto storage patterns
- [Automerge Columnar Protocol](https://automerge.org/blog/2023/11/06/automerge-2/) - Wire format inspiration
- [Ditto SDK Documentation](https://docs.ditto.live/) - Current backend reference

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-04 | Proposed abstraction layer | Enable dual backend support |
| TBD | Approved/Rejected | After team review |

## Open Questions

1. **Should we support runtime backend switching?** Or only compile-time selection?
2. **How do we handle backend-specific configuration?** (e.g., Ditto's shared_key vs custom backend's encryption)
3. **Should abstraction layer support transactions?** Or is eventual consistency sufficient?
4. **What's the migration path for existing Ditto data?** Export/import utilities needed?
5. **Should we expose backend-specific features through extensions?** Or strictly limit to common denominator?

## Next Steps

1. Review this ADR with team
2. Prototype abstraction layer with Ditto backend
3. Validate that current E2E tests work through abstraction
4. Begin Phase 1 of custom backend implementation
5. Schedule regular A/B testing checkpoints
