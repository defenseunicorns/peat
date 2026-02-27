# ADR-049: peat-mesh Extraction (Open Source Sync Layer)

**Status**: IMPLEMENTED (All 8 Phases Complete — PRs #622-#629)
**Date**: 2025-01-31 (proposed) / 2026-02-12 (completed)
**Authors**: Kit Plummer, Codex  
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)  
**Priority**: URGENT - Blocking for Defense Unicorns transition  
**Relates To**: ADR-011 (Automerge + Iroh), ADR-032 (Pluggable Transport), ADR-039 (peat-btle), ADR-041 (Multi-Transport Embedded)

---

## Executive Summary

This ADR defines the extraction of **peat-mesh** - a standalone, open-source CRDT-based mesh synchronization library that serves as a direct alternative to Ditto. This crate provides the foundational sync infrastructure that PEAT Protocol consumes, but contains **zero PEAT-specific semantics**.

**peat-mesh** is to PEAT Protocol what SQLite is to an application - a general-purpose data layer that the application builds upon.

### Strategic Importance

1. **IP Clarity**: Clean separation between sync infrastructure (open source) and PEAT Protocol (proprietary IP)
2. **DU Transition**: Enables Defense Unicorns to receive PEAT Protocol IP without Ditto dependency
3. **Market Position**: Open source Ditto alternative creates competitive dynamics and community adoption
4. **IETF Pathway**: Sync protocol can be standardized independently of PEAT semantics

---

## Context

### The Current State

Today, PEAT Protocol's sync capabilities are intertwined with protocol semantics:

```
┌─────────────────────────────────────────────────────────────────┐
│                 Current: Tightly Coupled                         │
│                                                                  │
│  peat-protocol crate                                            │
│  ├── Hierarchical aggregation logic                             │
│  ├── Capability composition                                     │
│  ├── Cell formation rules                                       │
│  ├── Automerge CRDT operations  ←── tangled                     │
│  ├── Iroh networking            ←── tangled                     │
│  └── Peer discovery             ←── tangled                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### The Target State

Clean separation where peat-mesh is a standalone, reusable sync layer:

```
┌─────────────────────────────────────────────────────────────────┐
│                    PEAT Protocol (Your IP)                       │
│                                                                  │
│  • Hierarchical Aggregation    • Capability Composition         │
│  • Emergent Capability Synthesis                                │
│  • Human-Machine Cell Formation                                 │
│  • PlatformBeacon, CellState, Command schemas                   │
│  • Cell leadership, aggregation rules                           │
│                                                                  │
│              Consumes peat-mesh via MeshProvider                │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│              peat-mesh (Extracted - Open Source)                 │
│              "Ditto Alternative"                                 │
│                                                                  │
│  • CRDT Documents (Automerge)     • Peer Discovery              │
│  • P2P Networking (Iroh)          • Sync Protocol               │
│  • Multi-Transport Support        • Offline-First               │
│  • Collection/Document API        • Subscriptions/Queries       │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Transport Layer                             │   │
│  │   Iroh (QUIC)  │  peat-btle (BLE)  │  Future: LoRa     │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│           NO PEAT SEMANTICS - sync arbitrary documents          │
└─────────────────────────────────────────────────────────────────┘
```

### Why This Matters

| Stakeholder | Benefit |
|-------------|---------|
| **Defense Unicorns** | Receives PEAT Protocol IP without Ditto licensing complexity |
| **Open Source Community** | Gets a Ditto alternative for their own mesh sync needs |
| **PEAT Users** | Can choose sync backend (peat-mesh vs Ditto) based on requirements |
| **IETF Standardization** | Sync protocol can be specified independently |
| **Competitive Position** | Open standard beats proprietary lock-in (Anduril Lattice, etc.) |

---

## Decision

### Extract peat-mesh as Standalone Crate

Create `peat-mesh` as a separate repository/crate that provides:

1. **CRDT-based document storage** (Automerge)
2. **P2P mesh networking** (Iroh + optional transports)
3. **Collection/Document API** for application developers
4. **Subscription and query system**
5. **Offline-first with automatic sync on reconnection**
6. **Pluggable transport architecture**

### Core API Surface

```rust
// peat-mesh/src/lib.rs

/// Main entry point for mesh operations
pub struct PeatMesh {
    // Internal: Automerge docs, Iroh endpoint, peer manager
}

impl PeatMesh {
    /// Create a new mesh instance
    pub async fn new(config: MeshConfig) -> Result<Self, MeshError>;
    
    /// Start mesh networking (peer discovery, sync)
    pub async fn start(&self) -> Result<(), MeshError>;
    
    /// Stop mesh networking gracefully
    pub async fn stop(&self) -> Result<(), MeshError>;
    
    /// Get a collection handle
    pub fn collection(&self, name: &str) -> Collection;
    
    /// Get mesh status
    pub fn status(&self) -> MeshStatus;
    
    /// Get connected peers
    pub fn peers(&self) -> Vec<PeerInfo>;
    
    /// Subscribe to mesh events
    pub fn subscribe_events(&self) -> broadcast::Receiver<MeshEvent>;
}

/// Collection of documents (similar to Ditto collection)
pub struct Collection {
    name: String,
    // Internal: reference to mesh
}

impl Collection {
    /// Get a document by ID
    pub async fn get(&self, id: &str) -> Result<Option<Document>, MeshError>;
    
    /// Upsert a document
    pub async fn upsert(&self, id: &str, value: serde_json::Value) -> Result<(), MeshError>;
    
    /// Delete a document
    pub async fn delete(&self, id: &str) -> Result<(), MeshError>;
    
    /// Query documents
    pub async fn query(&self, query: Query) -> Result<Vec<Document>, MeshError>;
    
    /// Subscribe to document changes
    pub fn subscribe(&self, filter: Option<Filter>) -> DocumentStream;
    
    /// Subscribe to a specific document
    pub fn subscribe_document(&self, id: &str) -> SingleDocumentStream;
}

/// A synchronized document
pub struct Document {
    pub id: String,
    pub value: serde_json::Value,
    pub version: DocumentVersion,
    pub last_modified: Timestamp,
}

/// Query for filtering documents
pub enum Query {
    All,
    ById(String),
    ByIds(Vec<String>),
    Filter(FilterExpression),
}

/// Filter expression for queries and subscriptions
pub enum FilterExpression {
    Equals { field: String, value: serde_json::Value },
    Contains { field: String, value: String },
    GreaterThan { field: String, value: serde_json::Value },
    LessThan { field: String, value: serde_json::Value },
    And(Box<FilterExpression>, Box<FilterExpression>),
    Or(Box<FilterExpression>, Box<FilterExpression>),
}
```

### Configuration

```rust
/// Mesh configuration
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Unique node identifier (generated if not provided)
    pub node_id: Option<NodeId>,
    
    /// Storage path for persistence (None = in-memory only)
    pub storage_path: Option<PathBuf>,
    
    /// Transport configuration
    pub transports: Vec<TransportConfig>,
    
    /// Peer discovery configuration
    pub discovery: DiscoveryConfig,
    
    /// Sync configuration
    pub sync: SyncConfig,
}

#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// Iroh QUIC transport (default)
    Iroh {
        bind_port: Option<u16>,
        relay_url: Option<String>,
    },
    /// Bluetooth Low Energy (requires feature)
    #[cfg(feature = "ble")]
    Ble {
        advertise: bool,
        scan: bool,
        coded_phy: bool,
    },
    /// Custom transport (for future extension)
    Custom(Box<dyn TransportFactory>),
}

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Enable mDNS discovery on local network
    pub mdns: bool,
    /// Bootstrap peers to connect to
    pub bootstrap_peers: Vec<PeerAddr>,
    /// DHT-based discovery
    pub dht: bool,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Sync interval for periodic sync
    pub sync_interval: Duration,
    /// Enable real-time sync on changes
    pub realtime: bool,
    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
}

#[derive(Debug, Clone, Copy)]
pub enum ConflictResolution {
    /// CRDT automatic merge (default, recommended)
    CrdtMerge,
    /// Last-write-wins based on timestamp
    LastWriteWins,
    /// Custom resolver (application provides)
    Custom,
}
```

---

## Open Questions

> **These questions must be resolved by the team before/during implementation.**

### Strategic Questions

#### Q1: API Surface Design

**Should peat-mesh mirror Ditto's API patterns to ease migration, or clean-sheet design?**

| Option | Pros | Cons |
|--------|------|------|
| **Mirror Ditto** | Easy migration for Ditto users, familiar patterns | Inherits Ditto's design decisions (good and bad) |
| **Clean-sheet** | Optimal Rust-native design, no legacy constraints | Learning curve, no migration path |
| **Hybrid** | Rust-native core with Ditto-compatible wrapper | More code to maintain |

*Recommendation*: Hybrid - Rust-native core API, with optional `ditto-compat` feature that provides familiar patterns.

#### Q2: What Existing Code is Being Extracted?

**Is this pulling from current peat-protocol internals, or formalizing the Automerge+Iroh work?**

Current state assessment needed:
- [ ] Audit `peat-protocol/src/sync/` - what's reusable?
- [ ] Audit `peat-protocol/src/transport/` - what's reusable?
- [ ] Identify PEAT-specific code that must NOT be extracted
- [ ] Identify generic sync code that SHOULD be extracted

#### Q3: Transport Layer Ownership

**Does peat-mesh own the transport layer (ADR-032), or consume transports?**

| Option | Description |
|--------|-------------|
| **peat-mesh owns transports** | Transport abstraction lives in peat-mesh, Iroh and BLE are built-in |
| **peat-mesh consumes transports** | Separate `peat-transport` crate, peat-mesh depends on it |
| **Transports are plugins** | peat-mesh defines trait, transports are separate crates |

*Recommendation*: peat-mesh owns the transport trait and includes Iroh by default. peat-btle is an optional feature/dependency. This keeps the "Ditto alternative" self-contained.

#### Q4: Relationship to MeshProvider Trait (ADR-042)

**How does peat-mesh relate to the MeshProvider trait defined yesterday?**

Options:
1. **peat-mesh implements MeshProvider** - The trait is defined in peat-protocol, peat-mesh provides an implementation
2. **MeshProvider moves to peat-mesh** - The trait is part of peat-mesh's public API
3. **MeshProvider is a wrapper** - peat-protocol defines MeshProvider, which wraps peat-mesh internally

*Recommendation*: Option 1 - MeshProvider trait stays in peat-protocol (it's the PEAT-specific interface), peat-mesh implements it. This allows other implementations (Ditto, mock) to also implement MeshProvider.

```rust
// In peat-protocol
pub trait MeshProvider { ... }

// In peat-mesh
impl MeshProvider for PeatMesh { ... }

// In peat-mesh-ditto (if needed)
impl MeshProvider for DittoMesh { ... }
```

#### Q5: Repository Structure

**Separate repo or monorepo with peat-protocol?**

| Option | Pros | Cons |
|--------|------|------|
| **Separate repo** | Clear IP boundary, independent releases, community contributions | Coordination overhead, version sync |
| **Monorepo** | Easier development, atomic changes | IP boundary less clear, harder for community |
| **Cargo workspace** | Best of both - separate crates, shared tooling | Still need clear ownership boundaries |

*Recommendation*: Separate repo (`github.com/revolveteam/peat-mesh`) for clean IP separation and community adoption. PEAT Protocol depends on it as external crate.

### Tactical Questions

#### Q6: Minimum Viable Extraction

**What's the smallest useful extraction for DU transition?**

MVP scope proposal:
- [ ] Collection/Document CRUD operations
- [ ] Single transport (Iroh)
- [ ] Basic peer discovery (bootstrap peers + mDNS)
- [ ] Automerge CRDT sync
- [ ] In-memory + file persistence

Deferred to post-MVP:
- [ ] BLE transport integration
- [ ] Advanced queries
- [ ] DHT discovery
- [ ] Sync policies (QoS, bandwidth limits)

#### Q7: Testing Strategy

**How do we validate peat-mesh independently of PEAT Protocol?**

- [ ] Unit tests for Collection/Document API
- [ ] Integration tests with multiple mesh instances
- [ ] Sync correctness tests (concurrent writes, conflicts)
- [ ] Network partition tests (offline/reconnect)
- [ ] Performance benchmarks (latency, throughput, scale)

#### Q8: Documentation and Examples

**What does a "Ditto alternative" need to be credible?**

- [ ] README with quick start
- [ ] API documentation (rustdoc)
- [ ] Migration guide from Ditto
- [ ] Example applications (chat, shared state, etc.)
- [ ] Performance comparison with Ditto

---

## Architecture

### Crate Structure

```
peat-mesh/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs              # Public API: PeatMesh, Collection, Document
│   ├── config.rs           # MeshConfig, TransportConfig, etc.
│   ├── mesh.rs             # PeatMesh implementation
│   ├── collection.rs       # Collection implementation
│   ├── document.rs         # Document, CRDT operations
│   ├── sync/
│   │   ├── mod.rs
│   │   ├── engine.rs       # Sync engine (Automerge)
│   │   ├── protocol.rs     # Wire protocol for sync messages
│   │   └── merge.rs        # Conflict resolution
│   ├── transport/
│   │   ├── mod.rs          # Transport trait
│   │   ├── iroh.rs         # Iroh QUIC transport
│   │   └── ble.rs          # BLE transport (feature-gated)
│   ├── discovery/
│   │   ├── mod.rs
│   │   ├── mdns.rs         # mDNS local discovery
│   │   ├── bootstrap.rs    # Bootstrap peer list
│   │   └── dht.rs          # DHT discovery (future)
│   ├── storage/
│   │   ├── mod.rs          # Storage trait
│   │   ├── memory.rs       # In-memory storage
│   │   └── file.rs         # File-based persistence
│   └── error.rs            # Error types
├── examples/
│   ├── simple_sync.rs      # Basic two-node sync
│   ├── chat.rs             # Multi-node chat application
│   └── shared_state.rs     # Shared state synchronization
└── tests/
    ├── sync_tests.rs
    ├── conflict_tests.rs
    └── network_tests.rs
```

### Dependencies

```toml
[dependencies]
# CRDT
automerge = "0.5"

# Networking
iroh = "0.35"
iroh-net = "0.35"

# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utilities
thiserror = "1"
tracing = "0.1"
uuid = { version = "1", features = ["v4"] }

[features]
default = ["iroh"]
iroh = []
ble = ["peat-btle"]
full = ["iroh", "ble"]
```

---

## Implementation Plan

### Phase 1: Core Extraction (THIS WEEK - DU Blocking)

**Goal**: Minimal viable peat-mesh that PEAT Protocol can depend on

- [ ] Create `peat-mesh` repository
- [ ] Define public API (PeatMesh, Collection, Document)
- [ ] Extract/implement Automerge document operations
- [ ] Extract/implement Iroh transport
- [ ] Basic peer discovery (bootstrap peers)
- [ ] In-memory storage
- [ ] Unit tests for core operations
- [ ] Integration test: two-node sync

**Deliverable**: `peat-mesh` crate that compiles and syncs documents between two nodes

### Phase 2: PEAT Protocol Integration (Week 2)

- [ ] Implement MeshProvider trait for PeatMesh
- [ ] Refactor peat-protocol to use peat-mesh
- [ ] Verify all existing PEAT tests pass
- [ ] Document migration from embedded sync to peat-mesh

**Deliverable**: PEAT Protocol uses peat-mesh, all tests green

### Phase 3: Production Hardening (Week 3-4)

- [ ] File-based persistence
- [ ] mDNS discovery
- [ ] Reconnection and sync recovery
- [ ] Error handling and logging
- [ ] Performance benchmarks
- [ ] Documentation

**Deliverable**: Production-ready peat-mesh

### Phase 4: BLE Integration (Week 5-6)

- [ ] Integrate peat-btle as optional transport
- [ ] Multi-transport coordination
- [ ] Transport selection logic
- [ ] Mobile platform testing

**Deliverable**: peat-mesh works over BLE

### Phase 5: Community Release (Week 7-8)

- [ ] Public repository setup
- [ ] Apache 2.0 licensing
- [ ] README and getting started guide
- [ ] Example applications
- [ ] Ditto migration guide
- [ ] Announce to community

**Deliverable**: Public open-source release

---

## Success Criteria

### Functional

- [ ] Two nodes can sync documents via Iroh
- [ ] Offline changes merge correctly on reconnection
- [ ] CRDT conflicts resolve deterministically
- [ ] PEAT Protocol works with peat-mesh backend
- [ ] No PEAT-specific code in peat-mesh

### Performance

- [ ] Sync latency < 100ms on local network
- [ ] Supports 100+ documents per collection
- [ ] Supports 50+ concurrent peers
- [ ] Memory usage < 100MB for typical deployment

### Quality

- [ ] >80% test coverage
- [ ] No unsafe code (or well-documented if required)
- [ ] Comprehensive rustdoc
- [ ] CI/CD pipeline with tests

---

## Consequences

### Positive

- **Clean IP separation** - PEAT Protocol is clearly differentiated from sync infrastructure
- **Open source adoption** - Community can use peat-mesh without PEAT
- **DU transition unblocked** - No Ditto dependency in delivered IP
- **Competitive positioning** - Open standard beats proprietary
- **Easier testing** - Can test sync layer independently

### Negative

- **Development effort** - Extraction takes time
- **Two codebases** - Must maintain peat-mesh separately
- **Version coordination** - PEAT Protocol must track peat-mesh versions
- **Community support** - Open source means issue triage, PRs, etc.

### Risks

- **Scope creep** - "Just one more feature" delays delivery
- **API instability** - Changing peat-mesh API breaks PEAT Protocol
- **Performance regression** - Extraction might miss optimizations
- **Incomplete extraction** - PEAT-specific code accidentally included

---

## References

- [Automerge](https://automerge.org/) - CRDT library
- [Iroh](https://iroh.computer/) - P2P networking
- [Ditto](https://ditto.live/) - Commercial mesh sync (competitive reference)
- ADR-011: Automerge + Iroh Integration
- ADR-032: Pluggable Transport Abstraction
- ADR-039: peat-btle Mesh Transport
- ADR-041: Multi-Transport Embedded Integration
- ADR-042: Protocol/Mesh Layer Abstraction (MeshProvider trait)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-01-31 | Extract peat-mesh as standalone crate | IP clarity for DU transition, open source positioning |
| 2025-01-31 | Automerge + Iroh as foundation | Already validated in ADR-011, production-ready |
| 2025-01-31 | Zero PEAT semantics in peat-mesh | Clean separation, general-purpose utility |
| 2026-02-11 | Phase 0: Break reverse deps (PR #622) | Remove all peat-protocol/peat-schema imports from peat-mesh |
| 2026-02-11 | Phase 1: Generic trait surface (PR #623) | DocumentStore, SyncEngine, DiscoveryStrategy traits |
| 2026-02-11 | Phase 2: Transport layer (PR #624) | Multi-transport manager, bypass, health, reconnection |
| 2026-02-11 | Phase 3: Storage/persistence (PR #625) | Automerge, Iroh blobs, negentropy, query, TTL |
| 2026-02-11 | Phase 4: QoS framework (PR #626) | 5-level priority, bandwidth, eviction, GC, audit |
| 2026-02-11 | Phase 5: Security primitives (PR #627) | Ed25519, X25519, ChaCha20, HMAC-SHA256, callsigns |
| 2026-02-12 | Phase 6: Service broker (PR #628) | Axum HTTP + WebSocket, feature-gated |
| 2026-02-12 | Phase 7: PeatMesh facade (PR #629) | Unified entry point, builder, lifecycle, events |
| TBD | API surface design | Clean-sheet Rust-native (not Ditto-compat) |
| TBD | Repository structure | Currently monorepo workspace, separate repo planned |
| TBD | Transport ownership | peat-mesh owns transport trait + Iroh default |

---

**Last Updated**: 2026-02-12
**Status**: IMPLEMENTED — All 8 phases complete (PRs #622-#629)
**Result**: 50,124 lines of standalone mesh code, 1,151 unit tests, zero peat-protocol/peat-schema dependencies
**Next Action**: README, examples, crates.io publish, Collection convenience API
