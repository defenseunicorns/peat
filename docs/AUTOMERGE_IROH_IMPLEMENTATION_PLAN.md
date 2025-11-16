# AutomergeIrohBackend Implementation Plan

**Status**: Planning
**Date**: 2025-01-14
**Goal**: Implement fully open-source backend alternative to DittoBackend

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────┐
│ HIVE Protocol Business Logic                             │
│ (Uses StorageBackend + CrdtCapable + SyncCapable)       │
└────────────────┬────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────┐
│ AutomergeIrohBackend                                    │
│ ================================================        │
│                                                         │
│ ┌─────────────────────────────────────────────────┐   │
│ │ Storage Layer (RocksDB + Automerge)             │   │
│ │ • Collection abstraction                        │   │
│ │ • Document persistence (save/load)              │   │
│ │ • CRDT field-level storage                      │   │
│ └─────────────────────────────────────────────────┘   │
│                                                         │
│ ┌─────────────────────────────────────────────────┐   │
│ │ Sync Layer (Automerge sync protocol)            │   │
│ │ • SyncDoc state management                      │   │
│ │ • Sync message generation/reception             │   │
│ │ • Change propagation                            │   │
│ └─────────────────────────────────────────────────┘   │
│                                                         │
│ ┌─────────────────────────────────────────────────┐   │
│ │ Network Layer (Iroh QUIC)                       │   │
│ │ • Endpoint management                           │   │
│ │ • P2P connection establishment                  │   │
│ │ • Stream multiplexing                           │   │
│ │ • Relay coordination                            │   │
│ └─────────────────────────────────────────────────┘   │
│                                                         │
│ ┌─────────────────────────────────────────────────┐   │
│ │ Discovery Layer (Custom - ADR-017)              │   │
│ │ • mDNS discovery                                │   │
│ │ • Static peer configuration                     │   │
│ │ • Relay-assisted discovery                      │   │
│ └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Technology Stack

| Component | Crate | Version | License |
|-----------|-------|---------|---------|
| **CRDT Engine** | `automerge` | 0.7.1 | MIT |
| **Networking** | `iroh` | 0.95.1 | MIT/Apache-2.0 |
| **Persistence** | `rocksdb` | 0.22 | Apache-2.0 |
| **Serialization** | `serde`, `serde_json` | 1.x | MIT/Apache-2.0 |

## Implementation Phases

### Phase 1: Storage Layer (Week 1-2) ✅ **COMPLETE**
**Goal**: Persist Automerge documents to RocksDB

**Status**: ✅ Completed 2025-11-14

**Completed Tasks**:
1. ✅ Created `AutomergeStore` wrapper around RocksDB with LRU cache
2. ✅ Implemented document save/load with Automerge binary serialization
3. ✅ Added Collection abstraction with namespace isolation (prefix-based)
4. ✅ Implemented `AutomergeBackend` with full `StorageBackend` trait
5. ⏭ Deferred to Phase 2: Typed operations (protobuf → JSON → Automerge)

**Key Files**:
- `hive-protocol/src/storage/automerge_store.rs` (239 lines, 6 tests)
- `hive-protocol/src/storage/automerge_backend.rs` (239 lines, 5 tests)
- `hive-protocol/Cargo.toml` (dependencies: iroh, rocksdb, lru)

**Success Criteria**: ✅ All Met
- ✅ Can save/load Automerge documents to RocksDB (binary format)
- ✅ Collection API matches DittoStore pattern (StorageBackend trait)
- ✅ Unit tests for CRUD operations (11 tests total, all passing)

**Implementation Details**:
- Storage: RocksDB with 64MB write buffer, 512 max open files
- Caching: LRU cache (1000 documents) for hot document access
- Collections: Namespace isolation via key prefixing ("cells:cell-1")
- Thread Safety: Arc-based ownership for concurrent access
- Current Strategy: Stores raw bytes in Automerge docs (Phase 2 adds protobuf conversion)

**Commits**:
- 79d5e5b: Phase 1 foundation (AutomergeStore + dependencies)
- 3f6238d: Collection abstraction (6 tests)
- e29d9c2: AutomergeBackend with StorageBackend trait (5 tests)

### Phase 2: CRDT Integration (Week 2-3)
**Goal**: Implement CrdtCapable trait for field-level merging

**Tasks**:
1. Convert protobuf messages to Automerge documents
2. Implement `TypedCollection<M>` for Automerge
3. Map protobuf types to Automerge types (Text, List, Map, Counter)
4. Handle nested objects and arrays

**Conversion Strategy**:
```rust
// Protobuf → JSON → Automerge
let json = serde_json::to_value(&protobuf_msg)?;
let mut doc = Automerge::new();
doc.transact(|tx| {
    populate_from_json(tx, ROOT, &json)?;
})?;
```

**Success Criteria**:
- ✅ Protobuf messages round-trip through Automerge
- ✅ Field-level updates merge correctly
- ✅ Implements `CrdtCapable` trait

### Phase 3: Iroh Network Layer (Week 3-4)
**Goal**: Establish P2P connections via Iroh

**Tasks**:
1. Create `IrohTransport` wrapper around Iroh Endpoint
2. Implement peer discovery (static + relay for now)
3. Handle connection lifecycle (connect/accept/close)
4. Implement bidirectional streams for sync messages

**Key Components**:
```rust
pub struct IrohTransport {
    endpoint: Endpoint,
    peer_connections: Arc<RwLock<HashMap<NodeId, Connection>>>,
}
```

**Success Criteria**:
- ✅ Two nodes can establish Iroh connection
- ✅ Can send/receive data over QUIC streams
- ✅ Connection recovery on failure

### Phase 4: Automerge Sync Protocol (Week 4-5)
**Goal**: Sync Automerge documents over Iroh

**Tasks**:
1. Integrate Automerge's `sync::State` for each peer
2. Generate sync messages when documents change
3. Send sync messages over Iroh streams
4. Receive and apply sync messages
5. Handle concurrent updates and merging

**Sync Flow**:
```text
Node A                          Node B
  │                               │
  ├─ Document updated             │
  ├─ generate_sync_message() ────→│
  │                               ├─ receive_sync_message()
  │                               ├─ apply changes
  │                               ├─ generate_sync_message()
  │←────────────────────────────┤
  ├─ receive_sync_message()       │
  ├─ apply changes                │
  │                               │
  ├─ Synced! ✅                   ├─ Synced! ✅
```

**Success Criteria**:
- ✅ Documents sync between two nodes
- ✅ Concurrent updates merge correctly
- ✅ Network partition recovery works

### Phase 5: SyncCapable Trait (Week 5)
**Goal**: Implement sync lifecycle management

**Tasks**:
1. Implement `SyncCapable` trait methods:
   - `start_sync()` - Begin background sync
   - `stop_sync()` - Stop sync gracefully
   - `sync_stats()` - Return peer count, bytes sent/received
2. Add sync coordinator task
3. Handle peer connection/disconnection events

**Success Criteria**:
- ✅ Implements `SyncCapable` trait
- ✅ Sync can be started/stopped
- ✅ Reports accurate sync statistics

### Phase 6: Discovery Integration (Week 6)
**Goal**: Automatic peer discovery (from ADR-017)

**Tasks**:
1. Implement mDNS discovery plugin
2. Add static peer configuration loader
3. Integrate with Iroh relay discovery
4. Handle peer lifecycle (discovered/lost)

**Key Files**:
- `hive-protocol/src/discovery/` (new module)
- Implemented per ADR-017 design

**Success Criteria**:
- ✅ Peers discover each other on LAN via mDNS
- ✅ Static configuration works for EMCON mode
- ✅ Relay discovery for cross-network peers

### Phase 7: E2E Testing (Week 7)
**Goal**: Comprehensive end-to-end tests

**Tasks**:
1. Create E2E test harness for AutomergeIrohBackend
2. Test scenarios:
   - Two-node sync
   - Three-node mesh
   - Network partition and recovery
   - Concurrent updates
3. Performance benchmarking vs Ditto

**Test Coverage**:
- ✅ Document CRUD operations
- ✅ CRDT field-level merging
- ✅ Network sync reliability
- ✅ Discovery mechanisms
- ✅ Error handling and recovery

## Key Implementation Decisions

### 1. Automerge Document Structure

**Option A: One Automerge doc per CAP document**
```rust
// Each NodeState, CellState is separate Automerge doc
let node_doc = Automerge::new();
store.put("nodes:node_alpha", &node_doc)?;
```

**Option B: Collection-level Automerge doc**
```rust
// All nodes in one Automerge doc with nested maps
let nodes_doc = Automerge::new();
// nodes_doc["node_alpha"] = { ... }
// nodes_doc["node_beta"] = { ... }
```

**Recommendation**: **Option A** - Matches DittoStore pattern, easier to map to existing CAP code.

### 2. Sync Strategy

**Option A: Per-document sync**
- Each document has its own sync state
- Fine-grained control
- More network overhead

**Option B: Collection-level sync**
- One sync state per collection
- More efficient for bulk updates
- Harder to implement selective sync

**Recommendation**: **Option A initially**, migrate to Option B if performance requires.

### 3. Network Protocol

**ALPN**: `b"cap/automerge/1"` - Identifies HIVE Protocol Automerge sync
**Message Format**: Length-prefixed protobuf messages containing Automerge sync messages

```rust
// Wire format:
[4 bytes: message length][N bytes: sync message]
```

## Dependencies to Add

```toml
[dependencies]
# CRDT Engine
automerge = "0.7"

# Networking
iroh = "0.95"

# Storage
rocksdb = "0.22"

# Discovery (Phase 6)
mdns-sd = "0.7"

# Utilities
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
```

## Unknown Unknowns to Discover

As we implement, we'll uncover:

1. **Automerge Performance**: How does it handle CAP's document sizes?
2. **Sync Protocol Complexity**: Are there edge cases in Automerge sync?
3. **Iroh Relay Requirements**: What infrastructure do we need?
4. **Network Failure Modes**: How does connection migration actually work?
5. **Discovery Reliability**: Does mDNS work on tactical networks?
6. **Memory Usage**: How does Automerge CRDT metadata grow?
7. **Conflict Resolution**: Are there CAP-specific merge conflicts?

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Automerge sync protocol complexity | Start with simple two-node sync, expand gradually |
| Iroh API changes (v0.95 → v1.0) | Pin version initially, plan for upgrade |
| Performance issues | Benchmark early, optimize hot paths |
| Network partition handling | Test exhaustively with simulated failures |
| Documentation gaps | Engage with Automerge/Iroh communities |

## Success Criteria (Overall)

### Minimum Viable Product (End of Phase 4)
- ✅ Two CAP nodes sync state via AutomergeIrohBackend
- ✅ Implements `StorageBackend` trait
- ✅ Implements `CrdtCapable` trait
- ✅ Basic sync works over Iroh

### Feature Parity (End of Phase 7)
- ✅ Implements all three traits (StorageBackend, CrdtCapable, SyncCapable)
- ✅ Automatic peer discovery
- ✅ Passes HIVE Protocol E2E test suite
- ✅ Performance within 2x of DittoBackend

### Production Ready (Future)
- ✅ Security integration (PKI, encryption)
- ✅ Multi-path networking
- ✅ Advanced discovery (ADR-017 full implementation)
- ✅ Operational monitoring and debugging

## Next Steps

1. ✅ Create this plan
2. Add dependencies to `Cargo.toml`
3. Create module structure (`automerge_store.rs`, `automerge_backend.rs`)
4. Implement Phase 1: Storage Layer
5. Write first E2E test

---

**Author**: Codex
**Status**: Ready to implement
**Estimated Timeline**: 7-8 weeks to MVP, 12-14 weeks to feature parity
