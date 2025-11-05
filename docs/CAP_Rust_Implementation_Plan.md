# CAP Rust Crate Implementation Plan
## Hierarchical CRDT Capability Composition for Distributed Autonomous Systems

**Project Name**: `crdt-edge` (or `edge-sync` - TBD)

**Vision**: A general-purpose, production-ready Rust CRDT library for mobile and edge applications, combining Automerge's superior columnar protocol with Ditto-style discovery and mesh networking. Designed for offline-first apps that need to work on bandwidth-constrained networks.

**Design Philosophy**: 
- **Modular**: Use only what you need (core CRDTs, storage, sync, discovery can be used independently)
- **General Purpose**: Works for any mobile/edge application (chat apps, collaborative tools, field data collection, IoT)
- **CAP-Specific Extensions**: Hierarchical organization and capability composition as optional features
- **Production Ready**: Battle-tested storage, robust networking, comprehensive testing

---

## Executive Summary

This implementation plan defines a new Rust crate that provides:

1. **Automerge-inspired columnar CRDT storage and wire protocol** (85-95% compression)
2. **Ditto-inspired discovery and mesh networking** (peer-to-peer, multi-transport)
3. **CAP-specific hierarchical organization** (O(n log n) scaling)
4. **Priority-based synchronization** (mission-critical data first)
5. **Offline-first architecture** (works disconnected, syncs opportunistically)

**Key Differentiator**: Unlike Automerge (which lacks discovery/networking) or Ditto (which uses CBOR), this crate provides the complete stack optimized for CAP's tactical edge use case.

**Target Platforms**:
- Embedded Linux on tactical platforms (UAVs, ground vehicles, edge compute nodes)
- Mobile C2 applications (Android/iOS tablets for commanders)
- Server-grade systems (battalion/company C2 centers)
- NOT web browsers (no WebAssembly, IndexedDB - these are irrelevant for tactical deployments)

---

## Project Structure

### Modular Architecture

The crate is designed as a collection of optional features that can be used independently:

```toml
# Cargo.toml feature flags
[features]
default = ["storage-rocksdb", "discovery-mdns", "transport-tcp"]

# Core (always included)
# - CRDT types and operations
# - Columnar encoding/decoding
# - Basic document model

# Storage backends (choose one or more)
storage-rocksdb = ["rocksdb"]
storage-sled = ["sled"]
storage-mmap = ["memmap2"]
storage-sqlite = ["rusqlite"]

# Discovery mechanisms (choose one or more)
discovery-mdns = ["mdns"]
discovery-bluetooth = ["btleplug"]
discovery-manual = [] # Manual peer configuration

# Transport protocols (choose one or more)
transport-tcp = ["tokio"]
transport-udp = ["tokio"]
transport-websocket = ["tokio-tungstenite"]
transport-bluetooth = ["btleplug"]
transport-quic = ["quinn"]

# Synchronization features
sync-priority = [] # Priority-based sync queues
sync-compression = ["flate2"] # Optional DEFLATE compression

# CAP-specific features (optional - only for tactical use cases)
cap-hierarchy = [] # Hierarchical organization (Squad/Platoon/Company)
cap-capability = [] # Capability composition and advertisement
cap-aggregation = [] # Hierarchical data aggregation

# Platform-specific
mobile = [] # Mobile-optimized defaults
embedded = [] # Embedded platform optimizations
```

### Example Usage Scenarios

**Scenario 1: Simple Mobile App (Offline Notes)**
```toml
crdt-edge = { version = "0.1", default-features = false, features = [
    "storage-sqlite",
    "transport-websocket"
]}
```

**Scenario 2: IoT Mesh Network**
```toml
crdt-edge = { version = "0.1", features = [
    "storage-sled",
    "discovery-mdns",
    "transport-udp",
    "embedded"
]}
```

**Scenario 3: CAP Tactical Deployment**
```toml
crdt-edge = { version = "0.1", features = [
    "storage-rocksdb",
    "discovery-mdns",
    "transport-tcp",
    "transport-udp",
    "sync-priority",
    "cap-hierarchy",
    "cap-capability",
    "cap-aggregation"
]}
```

### Directory Structure

```
crdt-edge/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs                      # Public API with feature gates
│   │
│   ├── crdt/                       # Core CRDT (always included)
│   │   ├── mod.rs
│   │   ├── types.rs                # LWW-Register, OR-Set, PN-Counter, etc.
│   │   ├── document.rs             # Document abstraction
│   │   ├── operations.rs           # CRDT operations
│   │   └── merge.rs                # Conflict resolution
│   │
│   ├── encoding/                   # Columnar wire protocol (always included)
│   │   ├── mod.rs
│   │   ├── columnar.rs             # Columnar layout engine
│   │   ├── rle.rs                  # Run-Length Encoding
│   │   ├── delta.rs                # Delta encoding
│   │   ├── actor_map.rs            # Actor deduplication
│   │   ├── compression.rs          # Optional DEFLATE
│   │   └── decoder.rs              # Efficient decoding
│   │
│   ├── storage/                    # Persistence layer (feature-gated)
│   │   ├── mod.rs
│   │   ├── traits.rs               # StorageAdapter trait
│   │   ├── memory.rs               # In-memory (always available)
│   │   ├── rocksdb.rs              # #[cfg(feature = "storage-rocksdb")]
│   │   ├── sled.rs                 # #[cfg(feature = "storage-sled")]
│   │   ├── mmap.rs                 # #[cfg(feature = "storage-mmap")]
│   │   ├── sqlite.rs               # #[cfg(feature = "storage-sqlite")]
│   │   └── compaction.rs           # Storage compaction
│   │
│   ├── discovery/                  # Peer discovery (feature-gated)
│   │   ├── mod.rs
│   │   ├── traits.rs               # DiscoveryAdapter trait
│   │   ├── mdns.rs                 # #[cfg(feature = "discovery-mdns")]
│   │   ├── bluetooth.rs            # #[cfg(feature = "discovery-bluetooth")]
│   │   └── manual.rs               # Manual configuration
│   │
│   ├── transport/                  # Network transports (feature-gated)
│   │   ├── mod.rs
│   │   ├── traits.rs               # Transport trait
│   │   ├── tcp.rs                  # #[cfg(feature = "transport-tcp")]
│   │   ├── udp.rs                  # #[cfg(feature = "transport-udp")]
│   │   ├── websocket.rs            # #[cfg(feature = "transport-websocket")]
│   │   ├── bluetooth.rs            # #[cfg(feature = "transport-bluetooth")]
│   │   └── quic.rs                 # #[cfg(feature = "transport-quic")]
│   │
│   ├── sync/                       # Synchronization engine
│   │   ├── mod.rs
│   │   ├── protocol.rs             # Basic sync protocol
│   │   ├── priority_queue.rs       # #[cfg(feature = "sync-priority")]
│   │   ├── delta_sync.rs           # Incremental sync
│   │   ├── backpressure.rs         # Flow control
│   │   └── obsolescence.rs         # Drop stale data
│   │
│   ├── cap/                        # CAP-specific (feature-gated)
│   │   ├── mod.rs                  # #[cfg(feature = "cap-hierarchy")]
│   │   ├── hierarchy/              # Hierarchical organization
│   │   │   ├── mod.rs
│   │   │   ├── organization.rs     # Squad/Platoon/Company
│   │   │   ├── group_formation.rs  # Bootstrap protocols
│   │   │   ├── aggregation.rs      # #[cfg(feature = "cap-aggregation")]
│   │   │   └── routing.rs          # Hierarchical routing
│   │   │
│   │   └── capability/             # #[cfg(feature = "cap-capability")]
│   │       ├── mod.rs
│   │       ├── types.rs            # Capability types
│   │       ├── composition.rs      # Composition algebra
│   │       └── advertisement.rs    # Advertisement protocol
│   │
│   ├── collection/                 # Collection-based organization
│   │   ├── mod.rs
│   │   ├── collection.rs           # Collection abstraction
│   │   ├── query.rs                # Query engine
│   │   └── subscription.rs         # Reactive subscriptions
│   │
│   ├── repo/                       # Repository (multi-document)
│   │   ├── mod.rs
│   │   ├── repository.rs           # Repository implementation
│   │   ├── peer_manager.rs         # Peer lifecycle management
│   │   └── sync_coordinator.rs     # Coordinate sync across docs
│   │
│   └── util/                       # Utilities
│       ├── mod.rs
│       ├── time.rs                 # Time utilities (vector clocks)
│       └── metrics.rs              # Performance metrics
│
├── examples/
│   ├── basic_crdt.rs               # Simple CRDT usage
│   ├── offline_notes_app.rs        # Mobile notes app
│   ├── collaborative_editing.rs    # Real-time collaboration
│   ├── iot_sensor_mesh.rs          # IoT mesh network
│   ├── peer_sync.rs                # Two-peer sync
│   │
│   └── cap/                        # CAP-specific examples
│       ├── squad_formation.rs      # Bootstrap example
│       ├── hierarchical_agg.rs     # Hierarchical aggregation
│       └── capability_composition.rs # Capability discovery
│
├── benches/
│   ├── encoding_bench.rs           # Columnar vs CBOR
│   ├── sync_bench.rs               # Sync performance
│   └── storage_bench.rs            # Persistence benchmarks
│
└── tests/
    ├── integration/
    │   ├── offline_sync.rs         # Offline operation tests
    │   ├── network_partition.rs    # Partition tolerance
    │   └── mobile_scenarios.rs     # Mobile app scenarios
    └── unit/
        └── ...                     # Unit tests throughout
```

---

## API Design Principles

### Simple for General Use

**Basic Document Operations** (no features required):
```rust
use crdt_edge::{Repository, Document};

// Create repository with in-memory storage
let repo = Repository::new_in_memory();

// Create a document
let doc = repo.create_document("notes").await?;

// Modify document
doc.update(|d| {
    d.set(&["title"], "My Note")?;
    d.set(&["content"], "Hello world")?;
    Ok(())
}).await?;

// Read document
let title = doc.get().await?.get(&["title"])?;
```

**Peer-to-Peer Sync** (basic features):
```rust
use crdt_edge::{Repository, transport::TcpTransport};

let repo1 = Repository::new("./data1")?;
let repo2 = Repository::new("./data2")?;

// Connect via TCP
repo1.connect("192.168.1.100:9876", TcpTransport).await?;

// Documents automatically sync
```

**Collections and Queries** (no special features):
```rust
// Insert documents
let notes = repo.collection("notes");
notes.insert(json!({
    "title": "Shopping List",
    "items": ["milk", "eggs"]
})).await?;

// Query
let results = notes
    .find("title LIKE '%List%'")
    .limit(10)
    .exec()
    .await?;

// Live updates
let mut updates = notes.observe();
while let Some(change) = updates.next().await {
    println!("Note changed: {:?}", change);
}
```

### Extended for CAP

**Hierarchical Organization** (requires `cap-hierarchy` feature):
```rust
use crdt_edge::cap::hierarchy::{Squad, GroupFormation, HierarchyLevel};

// Form squads automatically
let formation = GroupFormation::geographic()
    .max_distance(10_000.0) // 10km
    .max_squad_size(7)
    .build();

let squads = formation.form_squads(&discovered_peers).await?;

// Set hierarchy level
repo.set_hierarchy_level(HierarchyLevel::Squad).await?;

// Messages automatically route through hierarchy
```

**Capability Composition** (requires `cap-capability` feature):
```rust
use crdt_edge::cap::capability::{Capability, CompositionEngine};

// Advertise capabilities
let advertiser = repo.capability_advertiser();
advertiser.advertise(vec![
    Capability::sensor("camera"),
    Capability::resource("fuel", 45, 50),
]).await?;

// Detect emergent capabilities
let engine = CompositionEngine::new()
    .add_rule(composition_rules::kill_chain())
    .add_rule(composition_rules::persistent_isr());

let emergent = engine.detect(&squad_members).await?;
```

**Priority Sync** (requires `sync-priority` feature):
```rust
use crdt_edge::sync::Priority;

// Normal update (routine priority)
doc.update(|d| d.set(&["position"], gps)).await?;

// Critical update (high priority)
doc.update_with_priority(Priority::Critical, |d| {
    d.remove_from_set(&["capabilities"], "strike")?;
    Ok(())
}).await?;
```

---

## Use Cases

### General Mobile/Edge Applications

**Offline-First Notes/Todo Apps**
- Core: CRDT documents for notes
- Storage: SQLite for mobile compatibility
- Sync: WebSocket to cloud when online
- No CAP features needed

**Collaborative Editing (Google Docs style)**
- Core: CRDT for text editing
- Storage: In-memory + periodic snapshots
- Sync: Real-time via WebSocket
- Transport: TCP/WebSocket only

**Field Data Collection (Agriculture, Surveys)**
- Core: CRDT for form data
- Storage: SQLite or Sled
- Discovery: Bluetooth when near office
- Sync: Batch upload when connectivity available

**IoT Sensor Networks**
- Core: CRDT for sensor readings
- Storage: Memory-mapped for embedded
- Discovery: mDNS on local network
- Transport: UDP for efficiency

**Mobile Gaming (Turn-based multiplayer)**
- Core: CRDT for game state
- Storage: Platform-specific
- Sync: P2P via local WiFi or through server
- No CAP features needed

### CAP-Specific Tactical Applications

**Autonomous Platform Coordination**
- Full stack with all features
- Hierarchical organization enabled
- Capability composition enabled
- Priority-based sync for mission-critical data
- Discovery via beacons and mDNS

**Key Differences for CAP**:
- Hierarchical aggregation (Squad → Platoon → Company)
- Capability composition (detect emergent capabilities)
- Priority sync (critical updates first)
- Group formation protocols (bootstrap squads)
- Bandwidth optimization (extreme compression needed)

---

## Phase 1: Core CRDT Foundation (Weeks 1-4)

**Goal**: Implement basic CRDT types and operations, ensuring correct convergence properties.

### 1.1 CRDT Types (`src/crdt/types.rs`)

Implement fundamental CRDT types:

```rust
/// Last-Write-Wins Register
/// Used for: Fuel levels, position, simple state
pub struct LWWRegister<T> {
    value: T,
    timestamp: Timestamp,
    actor_id: ActorId,
}

/// Observed-Remove Set (OR-Set)
/// Used for: Squad membership, capability sets
pub struct ORSet<T> {
    elements: HashMap<T, HashSet<(ActorId, Timestamp)>>,
    tombstones: HashSet<(T, ActorId, Timestamp)>,
}

/// Positive-Negative Counter
/// Used for: Ammunition counts, deployable assets
pub struct PNCounter {
    increments: HashMap<ActorId, u64>,
    decrements: HashMap<ActorId, u64>,
}

/// Multi-Value Register (MVRegister)
/// Used for: Detecting conflicts that need human resolution
pub struct MVRegister<T> {
    values: HashMap<Timestamp, (ActorId, T)>,
}

/// Map CRDT (recursive structure)
/// Used for: Document structure, nested capabilities
pub struct CRDTMap {
    fields: HashMap<String, CRDTValue>,
}

/// Unified CRDT value enum
pub enum CRDTValue {
    LWW(LWWRegister<Value>),
    ORSet(ORSet<Value>),
    Counter(PNCounter),
    MVReg(MVRegister<Value>),
    Map(CRDTMap),
}

/// Primitive values
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(Timestamp),
}
```

### 1.2 Operation Types (`src/crdt/operations.rs`)

Define CRDT operations that can be applied and merged:

```rust
/// Core operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    /// Set a value (LWW semantics)
    Set {
        path: FieldPath,
        value: Value,
        timestamp: Timestamp,
        actor: ActorId,
    },
    
    /// Add to set (OR-Set)
    AddToSet {
        path: FieldPath,
        value: Value,
        timestamp: Timestamp,
        actor: ActorId,
    },
    
    /// Remove from set (OR-Set)
    RemoveFromSet {
        path: FieldPath,
        value: Value,
        tombstone: (ActorId, Timestamp),
    },
    
    /// Increment counter
    Increment {
        path: FieldPath,
        amount: u64,
        actor: ActorId,
    },
    
    /// Decrement counter
    Decrement {
        path: FieldPath,
        amount: u64,
        actor: ActorId,
    },
    
    /// Create nested map
    CreateMap {
        path: FieldPath,
        timestamp: Timestamp,
        actor: ActorId,
    },
}

/// Field path (e.g., ["platform", "fuel"])
pub type FieldPath = Vec<String>;

/// Actor identifier (platform/squad/platoon ID)
pub type ActorId = [u8; 16]; // 128-bit UUID

/// Timestamp (logical clock + physical clock)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub logical: u64,
    pub physical: u64, // Unix timestamp in microseconds
    pub actor: ActorId,
}
```

### 1.3 Document Abstraction (`src/crdt/document.rs`)

High-level document API:

```rust
pub struct Document {
    doc_id: DocumentId,
    actor_id: ActorId,
    root: CRDTMap,
    operations: Vec<Operation>,
    heads: Vec<Hash>, // Current document heads
    version_vector: VersionVector,
}

impl Document {
    /// Create new document
    pub fn new(doc_id: DocumentId, actor_id: ActorId) -> Self;
    
    /// Apply local change
    pub fn change<F>(&mut self, f: F) -> Result<ChangeSet>
    where
        F: FnOnce(&mut DocumentMut) -> Result<()>;
    
    /// Get value at path
    pub fn get(&self, path: &[&str]) -> Option<&CRDTValue>;
    
    /// Apply operations from peer
    pub fn apply_operations(&mut self, ops: Vec<Operation>) -> Result<()>;
    
    /// Get operations since specific heads
    pub fn get_changes_since(&self, heads: &[Hash]) -> Vec<Operation>;
    
    /// Merge with another document
    pub fn merge(&mut self, other: &Document) -> Result<MergeResult>;
    
    /// Get current state hash
    pub fn state_hash(&self) -> Hash;
}

/// Mutable document for applying changes
pub struct DocumentMut<'a> {
    doc: &'a mut Document,
}

impl<'a> DocumentMut<'a> {
    /// Set value at path
    pub fn set(&mut self, path: &[&str], value: Value) -> Result<()>;
    
    /// Add to set at path
    pub fn add_to_set(&mut self, path: &[&str], value: Value) -> Result<()>;
    
    /// Remove from set at path
    pub fn remove_from_set(&mut self, path: &[&str], value: Value) -> Result<()>;
    
    /// Increment counter at path
    pub fn increment(&mut self, path: &[&str], amount: u64) -> Result<()>;
}
```

### 1.4 Merge Logic (`src/crdt/merge.rs`)

Implement CRDT merge rules:

```rust
/// Merge engine for combining operations
pub struct MergeEngine;

impl MergeEngine {
    /// Merge two LWW registers (last write wins)
    pub fn merge_lww<T>(a: &LWWRegister<T>, b: &LWWRegister<T>) -> LWWRegister<T> {
        if b.timestamp > a.timestamp {
            b.clone()
        } else if a.timestamp > b.timestamp {
            a.clone()
        } else {
            // Timestamps equal, use actor ID as tiebreaker
            if b.actor_id > a.actor_id {
                b.clone()
            } else {
                a.clone()
            }
        }
    }
    
    /// Merge two OR-Sets (union of elements, respecting tombstones)
    pub fn merge_orset<T>(a: &ORSet<T>, b: &ORSet<T>) -> ORSet<T> {
        let mut result = ORSet::new();
        
        // Union of all elements from both sets
        for (elem, tags_a) in &a.elements {
            let tags_b = b.elements.get(elem).cloned().unwrap_or_default();
            let merged_tags: HashSet<_> = tags_a.union(&tags_b).cloned().collect();
            
            // Only include if not tombstoned
            let live_tags: HashSet<_> = merged_tags
                .into_iter()
                .filter(|tag| !a.tombstones.contains(&(elem.clone(), tag.0, tag.1)))
                .filter(|tag| !b.tombstones.contains(&(elem.clone(), tag.0, tag.1)))
                .collect();
            
            if !live_tags.is_empty() {
                result.elements.insert(elem.clone(), live_tags);
            }
        }
        
        // Union of tombstones
        result.tombstones = a.tombstones.union(&b.tombstones).cloned().collect();
        
        result
    }
    
    /// Merge two PN-Counters (sum of increments/decrements per actor)
    pub fn merge_counter(a: &PNCounter, b: &PNCounter) -> PNCounter {
        let mut result = PNCounter::new();
        
        // Merge increments (max per actor)
        for (actor, count) in a.increments.iter().chain(b.increments.iter()) {
            let current = result.increments.entry(*actor).or_insert(0);
            *current = (*current).max(*count);
        }
        
        // Merge decrements (max per actor)
        for (actor, count) in a.decrements.iter().chain(b.decrements.iter()) {
            let current = result.decrements.entry(*actor).or_insert(0);
            *current = (*current).max(*count);
        }
        
        result
    }
}
```

**Deliverables**:
- [ ] All CRDT types implemented with tests
- [ ] Operation types defined and serializable
- [ ] Document abstraction with change tracking
- [ ] Merge logic with convergence proofs
- [ ] Property-based tests (using proptest) to verify CRDT properties

**Success Criteria**:
- All CRDT merge operations are commutative and associative
- Property tests verify convergence with random operation orders
- 100% test coverage on merge logic

---

## Phase 2: Columnar Encoding (Weeks 5-8)

**Goal**: Implement Automerge-style columnar wire protocol with RLE and delta encoding.

### 2.1 Columnar Layout Engine (`src/encoding/columnar.rs`)

Transform operations into columnar format:

```rust
/// Columnar encoder for CRDT operations
pub struct ColumnarEncoder {
    actor_map: ActorMap,
}

impl ColumnarEncoder {
    /// Encode operations into columnar format
    pub fn encode(&mut self, ops: &[Operation]) -> Result<EncodedDocument> {
        // Step 1: Build actor dictionary
        let actor_indices = self.build_actor_map(ops);
        
        // Step 2: Organize into columns
        let columns = self.columnize(ops, &actor_indices)?;
        
        // Step 3: Apply RLE to each column
        let rle_columns = self.apply_rle(columns)?;
        
        // Step 4: Apply delta encoding where beneficial
        let delta_columns = self.apply_delta_encoding(rle_columns)?;
        
        // Step 5: Write with metadata
        Ok(self.write_document(delta_columns)?)
    }
    
    /// Organize operations into columns
    fn columnize(
        &self,
        ops: &[Operation],
        actor_indices: &HashMap<ActorId, u32>,
    ) -> Result<Columns> {
        let mut columns = Columns::new();
        
        for op in ops {
            // Actor column (reference to actor map)
            columns.actors.push(actor_indices[&op.actor()]);
            
            // Action column (SET, ADD, REMOVE, etc.)
            columns.actions.push(op.action_type());
            
            // Field column (path to field)
            columns.fields.push(op.field_path().clone());
            
            // Value column (actual value)
            columns.values.push(op.value().clone());
            
            // Timestamp column
            columns.timestamps.push(op.timestamp());
        }
        
        Ok(columns)
    }
}

#[derive(Debug)]
pub struct Columns {
    pub actors: Vec<u32>,      // Indices into actor map
    pub actions: Vec<u8>,      // Action type enum
    pub fields: Vec<FieldPath>,
    pub values: Vec<Value>,
    pub timestamps: Vec<Timestamp>,
}
```

### 2.2 Run-Length Encoding (`src/encoding/rle.rs`)

Compress repeated values:

```rust
/// Run-Length Encoding for repeated values
pub struct RLEEncoder;

impl RLEEncoder {
    /// Encode a sequence with RLE
    pub fn encode<T: PartialEq + Clone>(data: &[T]) -> Vec<RLEElement<T>> {
        if data.is_empty() {
            return Vec::new();
        }
        
        let mut result = Vec::new();
        let mut current = data[0].clone();
        let mut count = 1;
        
        for item in &data[1..] {
            if *item == current {
                count += 1;
            } else {
                result.push(RLEElement {
                    value: current.clone(),
                    count,
                });
                current = item.clone();
                count = 1;
            }
        }
        
        // Don't forget last run
        result.push(RLEElement {
            value: current,
            count,
        });
        
        result
    }
    
    /// Decode RLE sequence
    pub fn decode<T: Clone>(encoded: &[RLEElement<T>]) -> Vec<T> {
        encoded
            .iter()
            .flat_map(|elem| std::iter::repeat(elem.value.clone()).take(elem.count))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct RLEElement<T> {
    pub value: T,
    pub count: usize,
}
```

### 2.3 Delta Encoding (`src/encoding/delta.rs`)

Encode differences for numeric sequences:

```rust
/// Delta encoding for sequences of numbers
pub struct DeltaEncoder;

impl DeltaEncoder {
    /// Encode integers as deltas
    pub fn encode_i64(data: &[i64]) -> DeltaSequence {
        if data.is_empty() {
            return DeltaSequence::default();
        }
        
        let mut deltas = Vec::with_capacity(data.len());
        deltas.push(data[0]); // First value absolute
        
        for i in 1..data.len() {
            deltas.push(data[i] - data[i - 1]); // Subsequent values as deltas
        }
        
        DeltaSequence {
            base: data[0],
            deltas,
        }
    }
    
    /// Decode delta sequence
    pub fn decode_i64(seq: &DeltaSequence) -> Vec<i64> {
        if seq.deltas.is_empty() {
            return Vec::new();
        }
        
        let mut result = Vec::with_capacity(seq.deltas.len());
        result.push(seq.deltas[0]);
        
        for delta in &seq.deltas[1..] {
            result.push(result.last().unwrap() + delta);
        }
        
        result
    }
    
    /// Encode with RLE on deltas (powerful combination)
    pub fn encode_with_rle(data: &[i64]) -> CompressedSequence {
        let delta_seq = Self::encode_i64(data);
        let rle_deltas = RLEEncoder::encode(&delta_seq.deltas);
        
        CompressedSequence {
            base: delta_seq.base,
            rle_deltas,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeltaSequence {
    pub base: i64,
    pub deltas: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct CompressedSequence {
    pub base: i64,
    pub rle_deltas: Vec<RLEElement<i64>>,
}
```

### 2.4 Actor Deduplication (`src/encoding/actor_map.rs`)

Dictionary encoding for actor IDs:

```rust
/// Actor map for deduplicating UUIDs
pub struct ActorMap {
    actors: Vec<ActorId>,
    index: HashMap<ActorId, u32>,
}

impl ActorMap {
    pub fn new() -> Self {
        Self {
            actors: Vec::new(),
            index: HashMap::new(),
        }
    }
    
    /// Add actor, return index
    pub fn insert(&mut self, actor: ActorId) -> u32 {
        if let Some(&index) = self.index.get(&actor) {
            return index;
        }
        
        let index = self.actors.len() as u32;
        self.actors.push(actor);
        self.index.insert(actor, index);
        index
    }
    
    /// Get actor by index
    pub fn get(&self, index: u32) -> Option<ActorId> {
        self.actors.get(index as usize).copied()
    }
    
    /// Serialize actor map
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // Write number of actors
        buf.extend_from_slice(&(self.actors.len() as u32).to_le_bytes());
        
        // Write each actor ID (16 bytes each)
        for actor in &self.actors {
            buf.extend_from_slice(actor);
        }
        
        buf
    }
}
```

### 2.5 Efficient Decoder (`src/encoding/decoder.rs`)

Decode columnar format:

```rust
/// Columnar decoder
pub struct ColumnarDecoder;

impl ColumnarDecoder {
    /// Decode document from columnar format
    pub fn decode(encoded: &EncodedDocument) -> Result<Vec<Operation>> {
        // Step 1: Deserialize actor map
        let actor_map = Self::deserialize_actor_map(&encoded.actor_map)?;
        
        // Step 2: Decode each column
        let actors = Self::decode_actor_column(&encoded.actor_column, &actor_map)?;
        let actions = Self::decode_action_column(&encoded.action_column)?;
        let fields = Self::decode_field_column(&encoded.field_column)?;
        let values = Self::decode_value_column(&encoded.value_column)?;
        let timestamps = Self::decode_timestamp_column(&encoded.timestamp_column)?;
        
        // Step 3: Reconstruct operations
        let mut operations = Vec::with_capacity(actors.len());
        
        for i in 0..actors.len() {
            operations.push(Operation::reconstruct(
                actors[i],
                actions[i],
                fields[i].clone(),
                values[i].clone(),
                timestamps[i],
            )?);
        }
        
        Ok(operations)
    }
    
    /// Decode actor column (RLE indices)
    fn decode_actor_column(
        data: &[u8],
        actor_map: &ActorMap,
    ) -> Result<Vec<ActorId>> {
        // Decode RLE
        let indices = Self::decode_rle_u32(data)?;
        
        // Map indices to actor IDs
        indices
            .into_iter()
            .map(|idx| {
                actor_map
                    .get(idx)
                    .ok_or_else(|| Error::InvalidActorIndex(idx))
            })
            .collect()
    }
    
    /// Can decode only specific columns (projection)
    pub fn decode_column(
        encoded: &EncodedDocument,
        column: ColumnType,
    ) -> Result<ColumnData> {
        match column {
            ColumnType::Actors => Self::decode_actor_column_data(encoded),
            ColumnType::Values => Self::decode_value_column_data(encoded),
            // ... other columns
        }
    }
}
```

**Deliverables**:
- [ ] Columnar layout engine
- [ ] RLE encoder/decoder
- [ ] Delta encoder/decoder
- [ ] Actor map with deduplication
- [ ] Efficient decoder with column projection
- [ ] Benchmarks comparing to CBOR

**Success Criteria**:
- 80%+ compression ratio vs. CBOR for typical CAP data
- Decoder can access specific columns without full decode
- Encoding/decoding performance: <1ms for 1000 operations

---

## Phase 3: Storage Layer (Weeks 9-11)

**Goal**: Implement pluggable storage backends with efficient persistence.

### 3.1 Storage Traits (`src/storage/traits.rs`)

Define storage abstraction:

```rust
/// Storage adapter trait
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Save document snapshot
    async fn save_snapshot(
        &self,
        doc_id: &DocumentId,
        heads: &[Hash],
        data: &[u8],
    ) -> Result<()>;
    
    /// Load document snapshot
    async fn load_snapshot(&self, doc_id: &DocumentId) -> Result<Option<Snapshot>>;
    
    /// Save incremental changes
    async fn save_changes(
        &self,
        doc_id: &DocumentId,
        changes: &[u8],
    ) -> Result<()>;
    
    /// Load changes since specific heads
    async fn load_changes_since(
        &self,
        doc_id: &DocumentId,
        since_heads: &[Hash],
    ) -> Result<Vec<u8>>;
    
    /// List all document IDs
    async fn list_documents(&self) -> Result<Vec<DocumentId>>;
    
    /// Compact document (merge snapshot + changes)
    async fn compact(&self, doc_id: &DocumentId) -> Result<()>;
    
    /// Delete document
    async fn delete(&self, doc_id: &DocumentId) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub doc_id: DocumentId,
    pub heads: Vec<Hash>,
    pub data: Vec<u8>,
    pub timestamp: Timestamp,
}
```

### 3.2 RocksDB Backend (`src/storage/rocksdb.rs`)

Production storage implementation:

```rust
use rocksdb::{DB, Options, WriteBatch};

pub struct RocksDBStorage {
    db: Arc<DB>,
}

impl RocksDBStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        let db = DB::open(&opts, path)?;
        
        Ok(Self {
            db: Arc::new(db),
        })
    }
    
    /// Key format: {doc_id}:snapshot:{heads_hash}
    fn snapshot_key(doc_id: &DocumentId, heads: &[Hash]) -> Vec<u8> {
        let heads_hash = Self::hash_heads(heads);
        format!("{}:snapshot:{}", doc_id, hex::encode(heads_hash)).into_bytes()
    }
    
    /// Key format: {doc_id}:changes:{timestamp}
    fn changes_key(doc_id: &DocumentId, timestamp: u64) -> Vec<u8> {
        format!("{}:changes:{:020}", doc_id, timestamp).into_bytes()
    }
}

#[async_trait]
impl StorageAdapter for RocksDBStorage {
    async fn save_snapshot(
        &self,
        doc_id: &DocumentId,
        heads: &[Hash],
        data: &[u8],
    ) -> Result<()> {
        let key = Self::snapshot_key(doc_id, heads);
        
        let snapshot = Snapshot {
            doc_id: doc_id.clone(),
            heads: heads.to_vec(),
            data: data.to_vec(),
            timestamp: Timestamp::now(),
        };
        
        let serialized = bincode::serialize(&snapshot)?;
        
        self.db.put(&key, serialized)?;
        
        Ok(())
    }
    
    async fn load_snapshot(&self, doc_id: &DocumentId) -> Result<Option<Snapshot>> {
        // Find latest snapshot for this document
        let prefix = format!("{}:snapshot:", doc_id).into_bytes();
        
        let iter = self.db.prefix_iterator(&prefix);
        
        // Get last snapshot (keys are sorted)
        if let Some(Ok((key, value))) = iter.last() {
            let snapshot: Snapshot = bincode::deserialize(&value)?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }
    
    async fn compact(&self, doc_id: &DocumentId) -> Result<()> {
        // Load current state
        let snapshot = self.load_snapshot(doc_id).await?;
        let changes = self.load_all_changes(doc_id).await?;
        
        if let Some(mut snap) = snapshot {
            // Merge changes into snapshot
            let mut doc = Document::deserialize(&snap.data)?;
            doc.apply_changes(&changes)?;
            
            // Serialize new snapshot
            let new_data = doc.serialize()?;
            let new_heads = doc.heads().to_vec();
            
            // Write new snapshot
            self.save_snapshot(doc_id, &new_heads, &new_data).await?;
            
            // Delete old snapshot and changes
            self.delete_old_data(doc_id, &snap.heads).await?;
        }
        
        Ok(())
    }
}
```

### 3.3 Sled Backend (`src/storage/sled.rs`)

Pure Rust embedded storage:

```rust
use sled::{Db, IVec};

pub struct SledStorage {
    db: Db,
}

impl SledStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }
}

#[async_trait]
impl StorageAdapter for SledStorage {
    async fn save_snapshot(
        &self,
        doc_id: &DocumentId,
        heads: &[Hash],
        data: &[u8],
    ) -> Result<()> {
        let key = Self::snapshot_key(doc_id, heads);
        
        let snapshot = Snapshot {
            doc_id: doc_id.clone(),
            heads: heads.to_vec(),
            data: data.to_vec(),
            timestamp: Timestamp::now(),
        };
        
        let serialized = bincode::serialize(&snapshot)?;
        
        self.db.insert(key, IVec::from(serialized))?;
        self.db.flush_async().await?;
        
        Ok(())
    }
    
    // ... similar implementations for other methods
}
```

**Deliverables**:
- [ ] Storage adapter trait
- [ ] RocksDB implementation
- [ ] Sled implementation
- [ ] In-memory storage for testing
- [ ] Compaction logic
- [ ] Storage benchmarks

**Success Criteria**:
- RocksDB backend handles 10,000 ops/sec writes
- Compaction reduces storage by 50%+ after 1000 updates
- Storage is safe for concurrent access

---

## Phase 4: Discovery and Transport (Weeks 12-16)

**Goal**: Implement peer discovery and multi-transport networking.

### 4.1 Discovery Traits (`src/discovery/traits.rs`)

```rust
/// Peer discovery mechanism
#[async_trait]
pub trait DiscoveryAdapter: Send + Sync {
    /// Start discovery process
    async fn start(&mut self) -> Result<()>;
    
    /// Stop discovery
    async fn stop(&mut self) -> Result<()>;
    
    /// Get stream of discovered peers
    fn peer_stream(&self) -> impl Stream<Item = PeerInfo>;
    
    /// Advertise self
    async fn advertise(&self, info: &LocalInfo) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addresses: Vec<SocketAddr>,
    pub capabilities: Vec<String>,
    pub hierarchy_level: HierarchyLevel,
    pub last_seen: Timestamp,
}

#[derive(Debug, Clone)]
pub struct LocalInfo {
    pub peer_id: PeerId,
    pub listen_addresses: Vec<SocketAddr>,
    pub capabilities: Vec<String>,
    pub hierarchy_level: HierarchyLevel,
}
```

### 4.2 mDNS Discovery (`src/discovery/mdns.rs`)

Local network discovery:

```rust
use mdns::{Record, RecordKind};

pub struct MDNSDiscovery {
    service_name: String,
    local_info: LocalInfo,
    discovered_peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    peer_tx: broadcast::Sender<PeerInfo>,
}

impl MDNSDiscovery {
    pub fn new(service_name: impl Into<String>) -> Self {
        let (peer_tx, _) = broadcast::channel(100);
        
        Self {
            service_name: service_name.into(),
            local_info: LocalInfo::default(),
            discovered_peers: Arc::new(RwLock::new(HashMap::new())),
            peer_tx,
        }
    }
    
    async fn discovery_loop(&self) -> Result<()> {
        let responder = mdns::Responder::new()?;
        
        loop {
            // Listen for mDNS queries
            for response in responder.iter() {
                if let Some(peer_info) = self.parse_mdns_response(response) {
                    // Notify listeners
                    let _ = self.peer_tx.send(peer_info.clone());
                    
                    // Store in map
                    self.discovered_peers
                        .write()
                        .await
                        .insert(peer_info.peer_id, peer_info);
                }
            }
            
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[async_trait]
impl DiscoveryAdapter for MDNSDiscovery {
    async fn start(&mut self) -> Result<()> {
        tokio::spawn(self.discovery_loop());
        Ok(())
    }
    
    async fn advertise(&self, info: &LocalInfo) -> Result<()> {
        // Broadcast mDNS advertisement
        let responder = mdns::Responder::new()?;
        
        responder.register(
            self.service_name.clone(),
            "_cap._udp".to_string(),
            info.listen_addresses[0].port(),
            &[
                format!("peer_id={}", info.peer_id),
                format!("level={}", info.hierarchy_level),
            ],
        );
        
        Ok(())
    }
    
    fn peer_stream(&self) -> impl Stream<Item = PeerInfo> {
        BroadcastStream::new(self.peer_tx.subscribe())
            .filter_map(|r| async move { r.ok() })
    }
}
```

### 4.3 Squad Beacon Discovery (`src/discovery/beacon.rs`)

CAP-specific hierarchical discovery:

```rust
/// Squad leader beacon for hierarchical discovery
pub struct BeaconDiscovery {
    local_info: LocalInfo,
    beacon_interval: Duration,
    discovery_range: u32, // Max number of hops
}

impl BeaconDiscovery {
    /// Broadcast squad leader beacon
    async fn broadcast_beacon(&self) -> Result<()> {
        let beacon = Beacon {
            peer_id: self.local_info.peer_id,
            hierarchy_level: self.local_info.hierarchy_level,
            squad_id: self.get_squad_id(),
            capabilities: self.local_info.capabilities.clone(),
            member_count: self.get_member_count(),
            timestamp: Timestamp::now(),
        };
        
        let encoded = bincode::serialize(&beacon)?;
        
        // Broadcast on UDP
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(true)?;
        
        socket
            .send_to(&encoded, "255.255.255.255:9876")
            .await?;
        
        Ok(())
    }
    
    /// Listen for squad beacons
    async fn listen_for_beacons(&self) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:9876").await?;
        let mut buf = vec![0u8; 1024];
        
        loop {
            let (len, addr) = socket.recv_from(&mut buf).await?;
            
            if let Ok(beacon) = bincode::deserialize::<Beacon>(&buf[..len]) {
                // Check if this is a relevant beacon
                if self.should_join_squad(&beacon) {
                    self.handle_beacon(beacon, addr).await?;
                }
            }
        }
    }
    
    /// Determine if should join this squad
    fn should_join_squad(&self, beacon: &Beacon) -> bool {
        // Check hierarchy level compatibility
        if beacon.hierarchy_level != self.local_info.hierarchy_level {
            return false;
        }
        
        // Check if squad has capacity
        if beacon.member_count >= MAX_SQUAD_SIZE {
            return false;
        }
        
        // Check capability compatibility
        // ... (implement capability matching logic)
        
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Beacon {
    peer_id: PeerId,
    hierarchy_level: HierarchyLevel,
    squad_id: Option<SquadId>,
    capabilities: Vec<String>,
    member_count: usize,
    timestamp: Timestamp,
}
```

### 4.4 Transport Traits (`src/transport/traits.rs`)

```rust
/// Network transport abstraction
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to peer
    async fn connect(&self, addr: &SocketAddr) -> Result<Box<dyn Connection>>;
    
    /// Listen for incoming connections
    async fn listen(&self, addr: &SocketAddr) -> Result<impl Stream<Item = Result<Box<dyn Connection>>>>;
    
    /// Get transport type
    fn transport_type(&self) -> TransportType;
}

/// Active connection to a peer
#[async_trait]
pub trait Connection: Send + Sync {
    /// Send message
    async fn send(&mut self, data: &[u8]) -> Result<()>;
    
    /// Receive message
    async fn recv(&mut self) -> Result<Vec<u8>>;
    
    /// Close connection
    async fn close(&mut self) -> Result<()>;
    
    /// Get peer address
    fn peer_addr(&self) -> SocketAddr;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    TCP,
    UDP,
    WebSocket,
    Bluetooth,
    Radio,
}
```

### 4.5 TCP Transport (`src/transport/tcp.rs`)

```rust
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct TcpTransport;

#[async_trait]
impl Transport for TcpTransport {
    async fn connect(&self, addr: &SocketAddr) -> Result<Box<dyn Connection>> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Box::new(TcpConnection { stream }))
    }
    
    async fn listen(&self, addr: &SocketAddr) -> Result<impl Stream<Item = Result<Box<dyn Connection>>>> {
        let listener = TcpListener::bind(addr).await?;
        
        Ok(async_stream::stream! {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        yield Ok(Box::new(TcpConnection { stream }) as Box<dyn Connection>);
                    }
                    Err(e) => {
                        yield Err(e.into());
                    }
                }
            }
        })
    }
    
    fn transport_type(&self) -> TransportType {
        TransportType::TCP
    }
}

struct TcpConnection {
    stream: TcpStream,
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, data: &[u8]) -> Result<()> {
        // Length-prefixed message
        let len = data.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }
    
    async fn recv(&mut self) -> Result<Vec<u8>> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        
        // Read message
        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        
        Ok(buf)
    }
    
    async fn close(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
    
    fn peer_addr(&self) -> SocketAddr {
        self.stream.peer_addr().unwrap()
    }
}
```

**Deliverables**:
- [ ] Discovery adapter trait
- [ ] mDNS discovery implementation
- [ ] Squad beacon discovery
- [ ] Transport trait
- [ ] TCP transport
- [ ] UDP transport
- [ ] WebSocket transport (for web compatibility)
- [ ] Transport benchmarks

**Success Criteria**:
- Discover 100 peers on local network in <5 seconds
- Beacon discovery forms squads in <10 seconds
- TCP transport handles 1000 msgs/sec
- All transports work concurrently

---

## Phase 5: Synchronization Engine (Weeks 17-20)

**Goal**: Implement priority-based delta sync with flow control.

### 5.1 Sync Protocol (`src/sync/protocol.rs`)

State machine for sync:

```rust
/// Sync protocol state machine
pub struct SyncProtocol {
    state: SyncState,
    local_heads: Vec<Hash>,
    remote_heads: Option<Vec<Hash>>,
    pending_changes: VecDeque<Change>,
}

#[derive(Debug, Clone, PartialEq)]
enum SyncState {
    Idle,
    Requesting,
    Sending,
    Receiving,
    Complete,
}

impl SyncProtocol {
    /// Initialize sync with peer
    pub fn initiate(&mut self) -> SyncMessage {
        self.state = SyncState::Requesting;
        
        SyncMessage::SyncRequest {
            doc_id: self.doc_id,
            heads: self.local_heads.clone(),
        }
    }
    
    /// Handle incoming sync message
    pub fn handle_message(
        &mut self,
        msg: SyncMessage,
        doc: &Document,
    ) -> Result<Vec<SyncMessage>> {
        match msg {
            SyncMessage::SyncRequest { doc_id, heads } => {
                self.remote_heads = Some(heads.clone());
                
                // Calculate changes needed by peer
                let changes = doc.get_changes_since(&heads)?;
                
                if changes.is_empty() {
                    Ok(vec![SyncMessage::SyncComplete])
                } else {
                    Ok(vec![SyncMessage::Changes {
                        doc_id,
                        changes: self.encode_changes(&changes)?,
                    }])
                }
            }
            
            SyncMessage::Changes { doc_id, changes } => {
                // Decode and apply changes
                let ops = self.decode_changes(&changes)?;
                self.pending_changes.extend(ops);
                
                // Send ack
                Ok(vec![SyncMessage::Ack {
                    doc_id,
                    heads: self.local_heads.clone(),
                }])
            }
            
            SyncMessage::Ack { heads, .. } => {
                self.remote_heads = Some(heads);
                self.state = SyncState::Complete;
                Ok(vec![])
            }
            
            SyncMessage::SyncComplete => {
                self.state = SyncState::Complete;
                Ok(vec![])
            }
        }
    }
    
    /// Encode changes using columnar format
    fn encode_changes(&self, changes: &[Operation]) -> Result<Vec<u8>> {
        let encoder = ColumnarEncoder::new();
        Ok(encoder.encode(changes)?.to_bytes())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SyncMessage {
    SyncRequest {
        doc_id: DocumentId,
        heads: Vec<Hash>,
    },
    Changes {
        doc_id: DocumentId,
        changes: Vec<u8>,
    },
    Ack {
        doc_id: DocumentId,
        heads: Vec<Hash>,
    },
    SyncComplete,
}
```

### 5.2 Priority Queue (`src/sync/priority_queue.rs`)

Mission-critical data first:

```rust
/// Priority-based message queue
pub struct PriorityQueue {
    queues: [VecDeque<QueuedMessage>; 4], // 4 priority levels
}

impl PriorityQueue {
    pub fn new() -> Self {
        Self {
            queues: [
                VecDeque::new(), // Priority 1 (Critical)
                VecDeque::new(), // Priority 2 (Urgent)
                VecDeque::new(), // Priority 3 (Routine)
                VecDeque::new(), // Priority 4 (Bulk)
            ],
        }
    }
    
    /// Push message with priority
    pub fn push(&mut self, msg: QueuedMessage, priority: Priority) {
        let queue_idx = match priority {
            Priority::Critical => 0,
            Priority::Urgent => 1,
            Priority::Routine => 2,
            Priority::Bulk => 3,
        };
        
        self.queues[queue_idx].push_back(msg);
    }
    
    /// Pop highest priority message
    pub fn pop(&mut self) -> Option<QueuedMessage> {
        // Try each priority level in order
        for queue in &mut self.queues {
            if let Some(msg) = queue.pop_front() {
                return Some(msg);
            }
        }
        None
    }
    
    /// Determine priority based on change type
    pub fn classify_priority(&self, change: &Change) -> Priority {
        match &change.operation {
            // Capability loss is critical
            Operation::RemoveFromSet { path, .. }
                if path.starts_with(&["capabilities"]) =>
            {
                Priority::Critical
            }
            
            // Resource threshold warnings are urgent
            Operation::Set { path, value, .. }
                if path.contains(&"fuel") && self.is_low_threshold(value) =>
            {
                Priority::Urgent
            }
            
            // Position updates are routine
            Operation::Set { path, .. } if path.contains(&"position") => {
                Priority::Routine
            }
            
            // Everything else is bulk
            _ => Priority::Bulk,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub doc_id: DocumentId,
    pub changes: Vec<Change>,
    pub timestamp: Timestamp,
    pub ttl: Duration, // Time to live
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 1,
    Urgent = 2,
    Routine = 3,
    Bulk = 4,
}
```

### 5.3 Delta Sync (`src/sync/delta_sync.rs`)

Efficient incremental sync:

```rust
/// Delta synchronization manager
pub struct DeltaSyncManager {
    local_document: Arc<RwLock<Document>>,
    peer_heads: HashMap<PeerId, Vec<Hash>>,
    pending_syncs: HashMap<PeerId, SyncProtocol>,
}

impl DeltaSyncManager {
    /// Sync with specific peer
    pub async fn sync_with_peer(
        &mut self,
        peer_id: PeerId,
        conn: &mut Box<dyn Connection>,
    ) -> Result<()> {
        let doc = self.local_document.read().await;
        
        // Get or create sync protocol for this peer
        let sync = self
            .pending_syncs
            .entry(peer_id)
            .or_insert_with(|| SyncProtocol::new(doc.id(), doc.heads()));
        
        // Initiate sync
        let init_msg = sync.initiate();
        self.send_message(conn, &init_msg).await?;
        
        // Receive and handle responses
        loop {
            let response = self.recv_message(conn).await?;
            
            let replies = sync.handle_message(response, &doc)?;
            
            for reply in replies {
                self.send_message(conn, &reply).await?;
            }
            
            if sync.is_complete() {
                break;
            }
        }
        
        // Apply received changes
        if let Some(changes) = sync.take_pending_changes() {
            let mut doc_mut = self.local_document.write().await;
            doc_mut.apply_operations(changes)?;
        }
        
        // Update peer heads
        self.peer_heads.insert(peer_id, doc.heads());
        
        Ok(())
    }
    
    /// Broadcast changes to all peers
    pub async fn broadcast_changes(&self, changes: Vec<Change>) -> Result<()> {
        // Classify priority
        let priority = self.classify_changes_priority(&changes);
        
        // Encode once
        let encoded = self.encode_changes(&changes)?;
        
        // Send to all connected peers based on priority
        for (peer_id, conn) in &self.connections {
            // Use priority queue for this peer
            if let Some(queue) = self.peer_queues.get_mut(peer_id) {
                queue.push(
                    QueuedMessage {
                        doc_id: self.doc_id(),
                        changes: changes.clone(),
                        timestamp: Timestamp::now(),
                        ttl: self.get_ttl_for_priority(priority),
                    },
                    priority,
                );
            }
        }
        
        Ok(())
    }
}
```

### 5.4 Backpressure (`src/sync/backpressure.rs`)

Flow control for congested networks:

```rust
/// Backpressure manager for flow control
pub struct BackpressureManager {
    window_size: usize,
    in_flight: usize,
    bandwidth_estimate: f64, // bytes per second
}

impl BackpressureManager {
    /// Check if can send more data
    pub fn can_send(&self) -> bool {
        self.in_flight < self.window_size
    }
    
    /// Record sent message
    pub fn record_sent(&mut self, size: usize) {
        self.in_flight += size;
    }
    
    /// Record acknowledged message
    pub fn record_ack(&mut self, size: usize, rtt: Duration) {
        self.in_flight = self.in_flight.saturating_sub(size);
        
        // Update bandwidth estimate
        let bw = size as f64 / rtt.as_secs_f64();
        self.bandwidth_estimate = 0.9 * self.bandwidth_estimate + 0.1 * bw;
        
        // Adjust window size (AIMD algorithm)
        if self.in_flight < self.window_size {
            self.window_size += 1; // Additive increase
        }
    }
    
    /// Record packet loss
    pub fn record_loss(&mut self) {
        self.window_size = self.window_size / 2; // Multiplicative decrease
        self.window_size = self.window_size.max(MIN_WINDOW_SIZE);
    }
    
    /// Get current send rate
    pub fn send_rate(&self) -> f64 {
        self.bandwidth_estimate
    }
}

const MIN_WINDOW_SIZE: usize = 4096; // 4 KB
```

### 5.5 Obsolescence Detection (`src/sync/obsolescence.rs`)

Drop stale data:

```rust
/// Detect and drop obsolete updates
pub struct ObsolescenceDetector;

impl ObsolescenceDetector {
    /// Check if change is still relevant
    pub fn is_relevant(change: &Change, now: Timestamp) -> bool {
        // Check TTL
        if let Some(ttl) = change.ttl {
            if now.duration_since(change.timestamp) > ttl {
                return false;
            }
        }
        
        // Check if data type has specific obsolescence rules
        match &change.operation {
            // Position updates stale after 30 seconds
            Operation::Set { path, .. } if path.contains(&"position") => {
                now.duration_since(change.timestamp) < Duration::from_secs(30)
            }
            
            // Fuel updates stale after 5 minutes
            Operation::Set { path, .. } if path.contains(&"fuel") => {
                now.duration_since(change.timestamp) < Duration::from_secs(300)
            }
            
            // Capability changes never stale (must be communicated)
            Operation::AddToSet { path, .. }
            | Operation::RemoveFromSet { path, .. }
                if path.starts_with(&["capabilities"]) =>
            {
                true
            }
            
            // Default: stale after 10 minutes
            _ => now.duration_since(change.timestamp) < Duration::from_secs(600),
        }
    }
    
    /// Filter out obsolete changes
    pub fn filter_changes(
        changes: Vec<Change>,
        now: Timestamp,
    ) -> Vec<Change> {
        changes
            .into_iter()
            .filter(|change| Self::is_relevant(change, now))
            .collect()
    }
}
```

**Deliverables**:
- [ ] Sync protocol state machine
- [ ] Priority queue implementation
- [ ] Delta sync manager
- [ ] Backpressure/flow control
- [ ] Obsolescence detection
- [ ] Sync integration tests

**Success Criteria**:
- Critical updates arrive within 5 seconds
- Backpressure prevents network congestion
- Obsolete data dropped before transmission
- Sync completes correctly under 30% packet loss

---

## Phase 6: Hierarchical Organization (Weeks 21-24)

**Goal**: Implement CAP-specific hierarchical group formation and aggregation.

### 6.1 Organization Structure (`src/hierarchy/organization.rs`)

```rust
/// Hierarchical organization levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyLevel {
    Platform,
    Squad,
    Platoon,
    Company,
    Battalion,
}

/// Squad structure
pub struct Squad {
    pub id: SquadId,
    pub leader: Option<PeerId>,
    pub members: HashSet<PeerId>,
    pub capabilities: CRDTMap,
    pub formed_at: Timestamp,
}

impl Squad {
    /// Add member to squad
    pub fn add_member(&mut self, peer_id: PeerId) -> Result<()> {
        if self.members.len() >= MAX_SQUAD_SIZE {
            return Err(Error::SquadFull);
        }
        
        self.members.insert(peer_id);
        
        // If no leader, first member becomes leader
        if self.leader.is_none() {
            self.leader = Some(peer_id);
        }
        
        Ok(())
    }
    
    /// Remove member from squad
    pub fn remove_member(&mut self, peer_id: PeerId) {
        self.members.remove(&peer_id);
        
        // If leader leaves, elect new leader
        if self.leader == Some(peer_id) {
            self.leader = self.elect_leader();
        }
    }
    
    /// Elect squad leader (highest capability score)
    fn elect_leader(&self) -> Option<PeerId> {
        self.members
            .iter()
            .max_by_key(|peer| self.get_capability_score(peer))
            .copied()
    }
}

/// Platoon structure
pub struct Platoon {
    pub id: PlatoonId,
    pub leader: Option<PeerId>,
    pub squads: HashMap<SquadId, Squad>,
    pub capabilities: CRDTMap, // Aggregated from squads
}

/// Company structure
pub struct Company {
    pub id: CompanyId,
    pub leader: Option<PeerId>,
    pub platoons: HashMap<PlatoonId, Platoon>,
    pub capabilities: CRDTMap, // Abstracted from platoons
}

const MAX_SQUAD_SIZE: usize = 7;
const MAX_SQUADS_PER_PLATOON: usize = 5;
const MAX_PLATOONS_PER_COMPANY: usize = 5;
```

### 6.2 Group Formation (`src/hierarchy/group_formation.rs`)

Bootstrap protocols:

```rust
/// Group formation strategies
pub enum FormationStrategy {
    /// C2 assigns platforms to squads
    CDirected {
        assignments: HashMap<SquadId, Vec<PeerId>>,
    },
    
    /// Geographic clustering
    Geographic {
        max_distance: f64, // meters
        max_squad_size: usize,
    },
    
    /// Capability-based grouping
    CapabilityBased {
        required_capabilities: Vec<String>,
        complementary: bool,
    },
}

pub struct GroupFormationEngine {
    strategy: FormationStrategy,
    discovered_peers: HashMap<PeerId, PeerInfo>,
    formed_squads: HashMap<SquadId, Squad>,
}

impl GroupFormationEngine {
    /// Execute formation strategy
    pub async fn form_squads(&mut self) -> Result<Vec<Squad>> {
        match &self.strategy {
            FormationStrategy::CDirected { assignments } => {
                self.form_directed_squads(assignments).await
            }
            
            FormationStrategy::Geographic {
                max_distance,
                max_squad_size,
            } => {
                self.form_geographic_squads(*max_distance, *max_squad_size)
                    .await
            }
            
            FormationStrategy::CapabilityBased {
                required_capabilities,
                complementary,
            } => {
                self.form_capability_squads(required_capabilities, *complementary)
                    .await
            }
        }
    }
    
    /// Geographic clustering using distance
    async fn form_geographic_squads(
        &mut self,
        max_distance: f64,
        max_squad_size: usize,
    ) -> Result<Vec<Squad>> {
        let mut squads = Vec::new();
        let mut unassigned: HashSet<_> = self.discovered_peers.keys().copied().collect();
        
        while !unassigned.is_empty() {
            let mut squad = Squad::new();
            
            // Pick arbitrary start point
            let start = *unassigned.iter().next().unwrap();
            squad.add_member(start)?;
            unassigned.remove(&start);
            
            let start_pos = self.get_position(&start)?;
            
            // Find nearby peers
            let mut nearby: Vec<_> = unassigned
                .iter()
                .filter(|peer| {
                    if let Ok(pos) = self.get_position(peer) {
                        pos.distance_to(&start_pos) <= max_distance
                    } else {
                        false
                    }
                })
                .take(max_squad_size - 1)
                .copied()
                .collect();
            
            for peer in nearby {
                squad.add_member(peer)?;
                unassigned.remove(&peer);
            }
            
            squads.push(squad);
        }
        
        Ok(squads)
    }
    
    /// Capability-based clustering
    async fn form_capability_squads(
        &mut self,
        required_capabilities: &[String],
        complementary: bool,
    ) -> Result<Vec<Squad>> {
        let mut squads = Vec::new();
        let mut unassigned: HashSet<_> = self.discovered_peers.keys().copied().collect();
        
        while !unassigned.is_empty() {
            let mut squad = Squad::new();
            let mut needed_caps: HashSet<_> = required_capabilities.iter().cloned().collect();
            
            // Find platforms with required capabilities
            for peer in unassigned.iter() {
                if squad.members.len() >= MAX_SQUAD_SIZE {
                    break;
                }
                
                let peer_caps = self.get_capabilities(peer)?;
                
                if complementary {
                    // Check if peer provides needed capability
                    let provides_needed = peer_caps.iter().any(|cap| needed_caps.contains(cap));
                    
                    if provides_needed {
                        squad.add_member(*peer)?;
                        
                        // Remove provided capabilities from needed
                        for cap in peer_caps {
                            needed_caps.remove(&cap);
                        }
                    }
                } else {
                    // Check if peer has ALL required capabilities
                    let has_all = required_capabilities
                        .iter()
                        .all(|cap| peer_caps.contains(cap));
                    
                    if has_all {
                        squad.add_member(*peer)?;
                    }
                }
            }
            
            // Remove assigned members
            for member in &squad.members {
                unassigned.remove(member);
            }
            
            squads.push(squad);
        }
        
        Ok(squads)
    }
}
```

### 6.3 Hierarchical Aggregation (`src/hierarchy/aggregation.rs`)

Compress data through hierarchy:

```rust
/// Hierarchical aggregation engine
pub struct AggregationEngine {
    compression_rules: Vec<CompressionRule>,
}

impl AggregationEngine {
    /// Aggregate platform data into squad summary
    pub fn aggregate_to_squad(
        &self,
        platforms: &[Document],
    ) -> Result<Document> {
        let mut squad_doc = Document::new_squad();
        
        // Platform count
        squad_doc.set(&["platform_count"], platforms.len() as u64)?;
        
        // Aggregate capabilities (union)
        let all_capabilities: HashSet<String> = platforms
            .iter()
            .flat_map(|doc| self.extract_capabilities(doc))
            .collect();
        
        for cap in all_capabilities {
            squad_doc.add_to_set(&["capabilities"], cap)?;
        }
        
        // Resource pooling (sum)
        let total_fuel: u64 = platforms
            .iter()
            .map(|doc| self.extract_fuel(doc).unwrap_or(0))
            .sum();
        
        squad_doc.set(&["total_fuel"], total_fuel)?;
        
        // Critical resources (minimum - weakest link)
        let min_endurance = platforms
            .iter()
            .map(|doc| self.extract_endurance(doc).unwrap_or(0))
            .min()
            .unwrap_or(0);
        
        squad_doc.set(&["squad_endurance"], min_endurance)?;
        
        // Spatial distribution
        let coverage_area = self.calculate_coverage_area(platforms)?;
        squad_doc.set(&["coverage_area"], coverage_area)?;
        
        // Emergent capabilities (composition detection)
        let emergent = self.detect_emergent_capabilities(platforms)?;
        for cap in emergent {
            squad_doc.add_to_set(&["emergent_capabilities"], cap)?;
        }
        
        Ok(squad_doc)
    }
    
    /// Aggregate squads into platoon summary
    pub fn aggregate_to_platoon(
        &self,
        squads: &[Document],
    ) -> Result<Document> {
        let mut platoon_doc = Document::new_platoon();
        
        // Squad count
        platoon_doc.set(&["squad_count"], squads.len() as u64)?;
        
        // Aggregate capabilities (union with quality scores)
        let capability_availability = self.calculate_capability_availability(squads)?;
        
        for (cap, availability) in capability_availability {
            platoon_doc.set(
                &["capabilities", &cap, "availability"],
                availability,
            )?;
        }
        
        // Mission endurance (based on overlap scheduling)
        let mission_endurance = self.calculate_mission_endurance(squads)?;
        platoon_doc.set(&["mission_endurance"], mission_endurance)?;
        
        // Abstract to mission capabilities
        let mission_caps = self.abstract_to_mission_level(squads)?;
        for (cap_type, capability) in mission_caps {
            platoon_doc.set(&["mission_capabilities", &cap_type], capability)?;
        }
        
        Ok(platoon_doc)
    }
    
    /// Detect emergent capabilities
    fn detect_emergent_capabilities(
        &self,
        platforms: &[Document],
    ) -> Result<Vec<String>> {
        let mut emergent = Vec::new();
        
        // Check for 3D mapping capability
        let has_camera = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "camera"));
        let has_lidar = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "lidar"));
        let has_compute = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "compute"));
        
        if has_camera && has_lidar && has_compute {
            emergent.push("3d_mapping".to_string());
        }
        
        // Check for kill chain
        let has_isr = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "isr"));
        let has_strike = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "strike"));
        let has_bda = platforms
            .iter()
            .any(|doc| self.has_capability(doc, "bda"));
        
        if has_isr && has_strike && has_bda {
            emergent.push("kill_chain".to_string());
        }
        
        // Check for persistent coverage
        let total_endurance: u64 = platforms
            .iter()
            .map(|doc| self.extract_endurance(doc).unwrap_or(0))
            .sum();
        
        let mission_duration = 240; // 4 hours in minutes
        
        if total_endurance >= mission_duration {
            emergent.push("persistent_coverage".to_string());
        }
        
        Ok(emergent)
    }
}

#[derive(Debug, Clone)]
pub struct CompressionRule {
    pub field: String,
    pub method: CompressionMethod,
}

#[derive(Debug, Clone)]
pub enum CompressionMethod {
    Sum,      // Pool resources
    Min,      // Weakest link
    Max,      // Best capability
    Union,    // Combine sets
    Average,  // Statistical aggregation
    Custom(fn(&[Value]) -> Value),
}
```

### 6.4 Hierarchical Routing (`src/hierarchy/routing.rs`)

Message routing through hierarchy:

```rust
/// Hierarchical message router
pub struct HierarchicalRouter {
    local_level: HierarchyLevel,
    squad_id: Option<SquadId>,
    platoon_id: Option<PlatoonId>,
    company_id: Option<CompanyId>,
    peer_levels: HashMap<PeerId, HierarchyLevel>,
}

impl HierarchicalRouter {
    /// Route message to appropriate destination
    pub fn route_message(
        &self,
        msg: &Message,
        destination: &Destination,
    ) -> Vec<PeerId> {
        match destination {
            // Direct peer-to-peer
            Destination::Peer(peer_id) => vec![*peer_id],
            
            // Broadcast within squad
            Destination::Squad(squad_id) => {
                self.get_squad_members(squad_id)
            }
            
            // Send to squad leader
            Destination::SquadLeader(squad_id) => {
                self.get_squad_leader(squad_id).into_iter().collect()
            }
            
            // Send up to platoon level
            Destination::Platoon => {
                // Send to squad leader who forwards to platoon
                if let Some(leader) = self.get_my_squad_leader() {
                    vec![leader]
                } else {
                    vec![]
                }
            }
            
            // Send up to company level
            Destination::Company => {
                // Route through hierarchy
                self.route_to_company()
            }
        }
    }
    
    /// Determine if message should be forwarded
    pub fn should_forward(&self, msg: &Message) -> bool {
        // Forward if:
        // 1. We're a leader and message is going up
        // 2. We're on path to destination
        // 3. Message scope includes our level
        
        match msg.destination {
            Destination::Platoon | Destination::Company => {
                self.is_squad_leader()
            }
            _ => false,
        }
    }
    
    /// Route message to company level
    fn route_to_company(&self) -> Vec<PeerId> {
        // Platform -> Squad Leader -> Platoon Leader -> Company Leader
        
        if self.local_level == HierarchyLevel::Platform {
            // Send to squad leader
            self.get_my_squad_leader().into_iter().collect()
        } else if self.local_level == HierarchyLevel::Squad {
            // Send to platoon leader
            self.get_my_platoon_leader().into_iter().collect()
        } else if self.local_level == HierarchyLevel::Platoon {
            // Send to company leader
            self.get_my_company_leader().into_iter().collect()
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone)]
pub enum Destination {
    Peer(PeerId),
    Squad(SquadId),
    SquadLeader(SquadId),
    Platoon,
    Company,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: MessageId,
    pub source: PeerId,
    pub destination: Destination,
    pub payload: Vec<u8>,
    pub hop_count: u8,
    pub max_hops: u8,
}
```

**Deliverables**:
- [ ] Organization structures (Squad/Platoon/Company)
- [ ] Group formation strategies
- [ ] Hierarchical aggregation engine
- [ ] Hierarchical routing
- [ ] Dynamic rebalancing
- [ ] Integration tests for bootstrap

**Success Criteria**:
- 1000 platforms form squads in <60 seconds
- Aggregation achieves 100x compression (platform to company)
- Routing correctly delivers messages through hierarchy
- System handles 20% platform loss gracefully

---

## Phase 7: Capability Composition (Weeks 25-27)

**Goal**: Implement capability type system and composition algebra.

### 7.1 Capability Types (`src/capability/types.rs`)

```rust
/// Capability type system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    /// Static hardware capability
    Static(StaticCapability),
    
    /// Resource-based capability (depletable)
    Resource(ResourceCapability),
    
    /// Emergent team capability
    Emergent(EmergentCapability),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticCapability {
    pub name: String,
    pub attributes: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCapability {
    pub name: String,
    pub current: u64,
    pub max: u64,
    pub decay_rate: f64, // units per second
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergentCapability {
    pub name: String,
    pub components: Vec<PeerId>,
    pub validity: TimeRange,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Timestamp,
    pub end: Timestamp,
}
```

### 7.2 Composition Algebra (`src/capability/composition.rs`)

```rust
/// Capability composition engine
pub struct CompositionEngine {
    rules: Vec<CompositionRule>,
}

impl CompositionEngine {
    /// Compose capabilities from multiple platforms
    pub fn compose(
        &self,
        platforms: &[PlatformCapabilities],
    ) -> Vec<Capability> {
        let mut composed = Vec::new();
        
        // Apply each composition rule
        for rule in &self.rules {
            if let Some(cap) = rule.try_compose(platforms) {
                composed.push(cap);
            }
        }
        
        composed
    }
}

#[derive(Debug, Clone)]
pub struct CompositionRule {
    pub name: String,
    pub requirements: Vec<Requirement>,
    pub output: CapabilitySpec,
}

impl CompositionRule {
    /// Try to compose capability if requirements met
    pub fn try_compose(
        &self,
        platforms: &[PlatformCapabilities],
    ) -> Option<Capability> {
        // Check if all requirements satisfied
        for req in &self.requirements {
            if !req.is_satisfied(platforms) {
                return None;
            }
        }
        
        // Construct emergent capability
        let components = self.get_contributing_platforms(platforms);
        
        Some(Capability::Emergent(EmergentCapability {
            name: self.output.name.clone(),
            components,
            validity: self.calculate_validity(platforms),
            confidence: self.calculate_confidence(platforms),
        }))
    }
}

#[derive(Debug, Clone)]
pub enum Requirement {
    /// Requires specific capability present
    Has {
        capability: String,
        min_count: usize,
    },
    
    /// Requires resource level
    Resource {
        resource: String,
        min_amount: u64,
    },
    
    /// Requires spatial distribution
    Spatial {
        min_distance: f64,
        max_distance: f64,
    },
    
    /// Requires temporal overlap
    Temporal {
        min_overlap: Duration,
    },
}

/// Example: Kill chain composition
pub fn kill_chain_rule() -> CompositionRule {
    CompositionRule {
        name: "kill_chain".to_string(),
        requirements: vec![
            Requirement::Has {
                capability: "isr".to_string(),
                min_count: 1,
            },
            Requirement::Has {
                capability: "strike".to_string(),
                min_count: 1,
            },
            Requirement::Has {
                capability: "bda".to_string(),
                min_count: 1,
            },
        ],
        output: CapabilitySpec {
            name: "kill_chain".to_string(),
            attributes: HashMap::new(),
        },
    }
}

/// Example: 3D mapping composition
pub fn three_d_mapping_rule() -> CompositionRule {
    CompositionRule {
        name: "3d_mapping".to_string(),
        requirements: vec![
            Requirement::Has {
                capability: "camera".to_string(),
                min_count: 1,
            },
            Requirement::Has {
                capability: "lidar".to_string(),
                min_count: 1,
            },
            Requirement::Has {
                capability: "compute".to_string(),
                min_count: 1,
            },
            Requirement::Spatial {
                min_distance: 10.0,   // meters
                max_distance: 1000.0, // meters
            },
        ],
        output: CapabilitySpec {
            name: "3d_mapping".to_string(),
            attributes: HashMap::new(),
        },
    }
}
```

### 7.3 Capability Advertisement (`src/capability/advertisement.rs`)

```rust
/// Capability advertisement protocol
pub struct CapabilityAdvertiser {
    local_capabilities: Vec<Capability>,
    advertised_capabilities: Arc<RwLock<HashMap<PeerId, Vec<Capability>>>>,
}

impl CapabilityAdvertiser {
    /// Advertise local capabilities
    pub async fn advertise(&self) -> Result<()> {
        let advertisement = CapabilityAdvertisement {
            peer_id: self.peer_id(),
            capabilities: self.local_capabilities.clone(),
            timestamp: Timestamp::now(),
            ttl: Duration::from_secs(300), // 5 minutes
        };
        
        // Broadcast to squad
        self.broadcast_to_squad(&advertisement).await?;
        
        Ok(())
    }
    
    /// Handle received advertisement
    pub async fn handle_advertisement(
        &mut self,
        adv: CapabilityAdvertisement,
    ) -> Result<()> {
        // Store peer capabilities
        self.advertised_capabilities
            .write()
            .await
            .insert(adv.peer_id, adv.capabilities);
        
        // Trigger composition detection
        self.detect_emergent_capabilities().await?;
        
        Ok(())
    }
    
    /// Detect emergent capabilities from peer advertisements
    async fn detect_emergent_capabilities(&self) -> Result<()> {
        let ads = self.advertised_capabilities.read().await;
        
        // Convert to platform capabilities
        let platforms: Vec<_> = ads
            .iter()
            .map(|(peer, caps)| PlatformCapabilities {
                peer_id: *peer,
                capabilities: caps.clone(),
            })
            .collect();
        
        // Try composition
        let engine = CompositionEngine::default();
        let emergent = engine.compose(&platforms);
        
        if !emergent.is_empty() {
            // Advertise emergent capabilities up hierarchy
            self.advertise_emergent(emergent).await?;
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAdvertisement {
    pub peer_id: PeerId,
    pub capabilities: Vec<Capability>,
    pub timestamp: Timestamp,
    pub ttl: Duration,
}
```

**Deliverables**:
- [ ] Capability type system
- [ ] Composition algebra with rules
- [ ] Capability discovery engine
- [ ] Advertisement protocol
- [ ] Example composition rules (kill chain, 3D mapping, etc.)
- [ ] Property tests for composition correctness

**Success Criteria**:
- Composition rules correctly identify emergent capabilities
- Advertisement protocol keeps capabilities fresh
- Engine detects new capabilities within 5 seconds of formation
- No false positives in capability detection

---

## Phase 8: Repository and Collections (Weeks 28-30)

**Goal**: Multi-document repository with collection-based organization.

### 8.1 Repository (`src/repo/repository.rs`)

```rust
/// Multi-document repository
pub struct Repository {
    storage: Arc<dyn StorageAdapter>,
    documents: Arc<RwLock<HashMap<DocumentId, Arc<RwLock<Document>>>>>,
    peer_manager: Arc<PeerManager>,
    sync_coordinator: Arc<SyncCoordinator>,
    collections: Arc<RwLock<HashMap<String, Collection>>>,
}

impl Repository {
    /// Create new repository
    pub fn new(storage: Arc<dyn StorageAdapter>) -> Self {
        Self {
            storage,
            documents: Arc::new(RwLock::new(HashMap::new())),
            peer_manager: Arc::new(PeerManager::new()),
            sync_coordinator: Arc::new(SyncCoordinator::new()),
            collections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Get or create collection
    pub async fn collection(&self, name: &str) -> Collection {
        let mut collections = self.collections.write().await;
        
        collections
            .entry(name.to_string())
            .or_insert_with(|| Collection::new(name, self.clone()))
            .clone()
    }
    
    /// Create new document
    pub async fn create_document(&self, collection: &str) -> Result<DocumentHandle> {
        let doc_id = DocumentId::new();
        let doc = Document::new(doc_id, self.local_actor_id());
        
        // Store document
        self.documents
            .write()
            .await
            .insert(doc_id, Arc::new(RwLock::new(doc)));
        
        Ok(DocumentHandle {
            doc_id,
            repo: self.clone(),
        })
    }
    
    /// Find document by ID
    pub async fn find(&self, doc_id: &DocumentId) -> Result<Option<DocumentHandle>> {
        // Check memory first
        if self.documents.read().await.contains_key(doc_id) {
            return Ok(Some(DocumentHandle {
                doc_id: *doc_id,
                repo: self.clone(),
            }));
        }
        
        // Load from storage
        if let Some(snapshot) = self.storage.load_snapshot(doc_id).await? {
            let doc = Document::deserialize(&snapshot.data)?;
            
            self.documents
                .write()
                .await
                .insert(*doc_id, Arc::new(RwLock::new(doc)));
            
            Ok(Some(DocumentHandle {
                doc_id: *doc_id,
                repo: self.clone(),
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Start syncing with peers
    pub async fn start_sync(&self) -> Result<()> {
        self.peer_manager.start_discovery().await?;
        self.sync_coordinator.start().await?;
        Ok(())
    }
}

/// Handle to a document
#[derive(Clone)]
pub struct DocumentHandle {
    doc_id: DocumentId,
    repo: Repository,
}

impl DocumentHandle {
    /// Get document for reading
    pub async fn get(&self) -> Result<DocumentReader> {
        let docs = self.repo.documents.read().await;
        let doc = docs.get(&self.doc_id).ok_or(Error::DocumentNotFound)?;
        Ok(DocumentReader {
            doc: doc.clone(),
        })
    }
    
    /// Modify document
    pub async fn update<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Document) -> Result<T>,
    {
        let docs = self.repo.documents.read().await;
        let doc = docs.get(&self.doc_id).ok_or(Error::DocumentNotFound)?;
        
        let mut doc_guard = doc.write().await;
        let result = f(&mut doc_guard)?;
        
        // Broadcast changes
        self.repo
            .sync_coordinator
            .broadcast_changes(self.doc_id, doc_guard.get_latest_changes())
            .await?;
        
        Ok(result)
    }
}
```

### 8.2 Collection (`src/collection/collection.rs`)

```rust
/// Collection of related documents
pub struct Collection {
    name: String,
    repo: Repository,
    query_engine: Arc<QueryEngine>,
}

impl Collection {
    /// Create new document in collection
    pub async fn insert(&self, data: Value) -> Result<DocumentHandle> {
        let handle = self.repo.create_document(&self.name).await?;
        
        handle
            .update(|doc| {
                doc.set(&["_collection"], Value::String(self.name.clone()))?;
                doc.set(&["data"], data)?;
                Ok(())
            })
            .await?;
        
        Ok(handle)
    }
    
    /// Query documents in collection
    pub fn find(&self, query: &str) -> Query {
        Query::new(
            self.clone(),
            self.query_engine.parse(query).unwrap(),
        )
    }
    
    /// Observe changes in collection
    pub fn observe(&self) -> impl Stream<Item = CollectionEvent> {
        // Subscribe to document changes in this collection
        let (tx, rx) = mpsc::channel(100);
        
        // Set up observer
        // ... (implementation)
        
        ReceiverStream::new(rx)
    }
}

/// Query builder
pub struct Query {
    collection: Collection,
    filter: QueryFilter,
    limit: Option<usize>,
    sort: Vec<SortSpec>,
}

impl Query {
    /// Execute query
    pub async fn exec(&self) -> Result<Vec<DocumentHandle>> {
        // Get all documents in collection
        let all_docs = self.collection.repo.list_documents(&self.collection.name).await?;
        
        // Apply filter
        let mut filtered = Vec::new();
        for doc in all_docs {
            if self.filter.matches(&doc).await? {
                filtered.push(doc);
            }
        }
        
        // Apply sort
        self.sort_documents(&mut filtered).await?;
        
        // Apply limit
        if let Some(limit) = self.limit {
            filtered.truncate(limit);
        }
        
        Ok(filtered)
    }
    
    /// Subscribe to query results
    pub fn subscribe(&self) -> impl Stream<Item = QueryResult> {
        // Live query subscription
        // ... (implementation)
    }
}
```

**Deliverables**:
- [ ] Multi-document repository
- [ ] Collection abstraction
- [ ] Query engine
- [ ] Subscription/observation API
- [ ] Peer management
- [ ] Sync coordinator

**Success Criteria**:
- Repository handles 10,000 documents
- Queries execute in <10ms for 1000 documents
- Subscriptions update within 100ms of changes
- Concurrent access is safe

---

## Phase 9: Integration and Testing (Weeks 31-34)

**Goal**: End-to-end integration and scale testing.

### 9.1 Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_two_peer_sync() {
        // Create two peers
        let peer1 = create_peer("peer1").await;
        let peer2 = create_peer("peer2").await;
        
        // Create document on peer1
        let doc1 = peer1.repo.create_document("test").await.unwrap();
        doc1.update(|doc| {
            doc.set(&["value"], 42)?;
            Ok(())
        }).await.unwrap();
        
        // Connect peers
        peer1.connect_to(&peer2).await.unwrap();
        
        // Wait for sync
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Verify peer2 received update
        let doc2 = peer2.repo.find(&doc1.doc_id).await.unwrap().unwrap();
        let value = doc2.get().await.unwrap().get(&["value"]).unwrap();
        assert_eq!(value, &Value::Int(42));
    }
    
    #[tokio::test]
    async fn test_offline_sync() {
        let peer1 = create_peer("peer1").await;
        let peer2 = create_peer("peer2").await;
        
        // Make changes while offline
        let doc1 = peer1.repo.create_document("test").await.unwrap();
        doc1.update(|doc| doc.set(&["a"], 1)).await.unwrap();
        
        let doc2 = peer2.repo.find(&doc1.doc_id).await.unwrap().unwrap();
        doc2.update(|doc| doc.set(&["b"], 2)).await.unwrap();
        
        // Connect and sync
        peer1.connect_to(&peer2).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Verify both have all changes
        let final1 = doc1.get().await.unwrap();
        assert_eq!(final1.get(&["a"]).unwrap(), &Value::Int(1));
        assert_eq!(final1.get(&["b"]).unwrap(), &Value::Int(2));
    }
    
    #[tokio::test]
    async fn test_squad_formation() {
        // Create 20 platforms
        let platforms: Vec<_> = (0..20)
            .map(|i| create_platform(&format!("platform_{}", i)))
            .collect();
        
        // Start discovery
        for platform in &platforms {
            platform.start_discovery().await.unwrap();
        }
        
        // Wait for squad formation
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        // Verify squads formed
        let squads = get_formed_squads(&platforms);
        assert!(squads.len() >= 3); // At least 3 squads
        assert!(squads.len() <= 5); // At most 5 squads
        
        // Verify squad sizes reasonable
        for squad in squads {
            assert!(squad.members.len() >= 3);
            assert!(squad.members.len() <= 7);
        }
    }
    
    #[tokio::test]
    async fn test_hierarchical_aggregation() {
        // Create squad with 5 platforms
        let platforms = create_test_squad(5).await;
        
        // Get squad document
        let squad_doc = aggregate_to_squad(&platforms).await.unwrap();
        
        // Verify aggregation
        let platform_count = squad_doc
            .get(&["platform_count"])
            .unwrap()
            .as_u64()
            .unwrap();
        assert_eq!(platform_count, 5);
        
        // Verify capabilities aggregated
        let capabilities = squad_doc.get(&["capabilities"]).unwrap();
        // ... verify capabilities
    }
}
```

### 9.2 Scale Tests

```rust
#[cfg(test)]
mod scale_tests {
    use super::*;
    
    #[tokio::test]
    #[ignore] // Run separately - takes time
    async fn test_1000_peer_bootstrap() {
        let start = Instant::now();
        
        // Create 1000 simulated peers
        let peers = create_simulated_peers(1000).await;
        
        // Start discovery
        for peer in &peers {
            peer.start_discovery().await.unwrap();
        }
        
        // Wait for convergence
        wait_for_squads_formed(&peers, Duration::from_secs(60)).await;
        
        let elapsed = start.elapsed();
        
        // Verify bootstrap completed
        assert!(elapsed < Duration::from_secs(60));
        
        // Verify hierarchy formed
        let stats = analyze_hierarchy(&peers);
        println!("Bootstrap stats: {:?}", stats);
        
        assert!(stats.avg_squad_size >= 3.0);
        assert!(stats.avg_squad_size <= 7.0);
    }
    
    #[tokio::test]
    #[ignore]
    async fn test_bandwidth_efficiency() {
        let peers = create_simulated_peers(100).await;
        
        // Monitor bandwidth
        let monitor = BandwidthMonitor::new();
        
        // Run for 1 hour (simulated)
        simulate_operations(&peers, Duration::from_secs(3600)).await;
        
        let stats = monitor.get_stats();
        
        // Verify bandwidth usage
        let avg_per_peer = stats.total_bytes / 100;
        
        // Should be <1 MB per peer per hour
        assert!(avg_per_peer < 1_000_000);
        
        println!("Bandwidth stats: {:?}", stats);
    }
}
```

**Deliverables**:
- [ ] Integration test suite
- [ ] Scale test suite (1000+ peers)
- [ ] Performance benchmarks
- [ ] Network simulation tests
- [ ] Chaos engineering tests (partition, packet loss)
- [ ] CI/CD pipeline

**Success Criteria**:
- All integration tests pass
- 1000-peer bootstrap completes in <60 seconds
- Bandwidth usage <1 MB/peer/hour for typical operations
- System handles 30% packet loss
- 95%+ test coverage on core modules

---

## Phase 10: Documentation and Polish (Weeks 35-36)

**Goal**: Production-ready documentation and examples.

### 10.1 Documentation

- [ ] API documentation (rustdoc)
- [ ] Architecture guide
- [ ] Protocol specification
- [ ] Integration guide
- [ ] Performance tuning guide
- [ ] Troubleshooting guide

### 10.2 Examples

- [ ] Simple two-peer sync
- [ ] Squad formation demo
- [ ] Capability composition example
- [ ] Web-based C2 dashboard (using WASM)
- [ ] Mobile app integration

### 10.3 Benchmarks

- [ ] Encoding benchmarks (vs CBOR, JSON)
- [ ] Storage benchmarks (vs other backends)
- [ ] Sync benchmarks (various network conditions)
- [ ] Scale benchmarks (10, 100, 1000 peers)

---

## Success Metrics

### Core Library Metrics (All Use Cases)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Compression Ratio** | 80%+ vs JSON | Benchmark suite |
| **Encoding Speed** | <1ms for 1000 ops | Benchmark suite |
| **Storage Efficiency** | 4x better than CBOR | Storage benchmarks |
| **Sync Latency** | <1s for typical updates | Integration tests |
| **Binary Size** | <2 MB (mobile) | Link time measurement |
| **Memory Usage** | <50 MB (1000 docs) | Runtime profiling |
| **Offline Operation** | Unlimited duration | Integration tests |
| **CRDT Convergence** | 100% correct | Property tests |

### Mobile Application Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Battery Impact** | <1% per hour | iOS/Android profiling |
| **Storage Efficiency** | <100 KB per 1000 docs | Mobile benchmarks |
| **Sync Speed** | <5s for 1MB update | Mobile network tests |
| **Startup Time** | <100ms to ready | Launch profiling |

### CAP-Specific Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Bootstrap Time** | <60s for 1000 peers | Scale tests |
| **Bandwidth Usage** | <1 MB/peer/hour | Network monitor |
| **Message Complexity** | O(n log n) | Analytical proof |
| **Priority Latency** | <5s for critical | Integration tests |
| **Aggregation Ratio** | 100:1 compression | Hierarchical tests |

### Functional Requirements

**Core (All Use Cases)**:
- [ ] CRDT correctness (property-based tests pass)
- [ ] Works offline indefinitely
- [ ] Automatic conflict resolution
- [ ] Efficient storage (<100 KB per 1000 docs)
- [ ] Cross-platform (Linux, iOS, Android, embedded)

**Mobile-Specific**:
- [ ] SQLite storage backend works
- [ ] Bluetooth discovery works
- [ ] Battery efficient
- [ ] Small binary size (<2 MB)

**CAP-Specific**:
- [ ] Hierarchical organization works (scale tests pass)
- [ ] Discovery forms squads in <60s
- [ ] Capability composition detects emergent capabilities
- [ ] System handles network partitions gracefully
- [ ] Priority sync delivers critical updates in <5s

---

## Dependencies

### Core Dependencies (Always Included)

```toml
[dependencies]
# Async runtime
tokio = { version = "1.0", features = ["rt", "sync"], optional = true }
async-trait = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"

# Hashing (for actor IDs, content hashing)
blake3 = "1.3"
uuid = { version = "1.0", features = ["v4"] }

# Time
chrono = "0.4"

# Logging
tracing = "0.1"
```

### Optional Dependencies (Feature-Gated)

```toml
# Storage backends
rocksdb = { version = "0.21", optional = true }
sled = { version = "0.34", optional = true }
rusqlite = { version = "0.30", optional = true }
memmap2 = { version = "0.9", optional = true }

# Networking
mdns = { version = "3.0", optional = true }
socket2 = { version = "0.5", optional = true }
quinn = { version = "0.10", optional = true } # QUIC
tokio-tungstenite = { version = "0.20", optional = true } # WebSocket

# Bluetooth (mobile/IoT)
btleplug = { version = "0.11", optional = true }

# Compression
lz4 = { version = "1.24", optional = true }
flate2 = { version = "1.0", optional = true }

# Collections (for advanced queries)
dashmap = { version = "5.4", optional = true }

# Futures/streams
futures = { version = "0.3", optional = true }
async-stream = { version = "0.3", optional = true }
```

### Development Dependencies

```toml
[dev-dependencies]
# Testing
tokio = { version = "1.0", features = ["full"] }
proptest = "1.0" # Property-based testing
criterion = "0.5" # Benchmarking

# Test utilities
tempfile = "3.0"
tracing-subscriber = "0.3"
```

### Platform-Specific Notes

**Mobile (iOS/Android)**:
- Use `rusqlite` for storage (platform-native)
- Bluetooth for local discovery
- WebSocket for cloud sync
- Avoid RocksDB (large binary size)

**Embedded Linux**:
- Use `sled` or `mmap` for storage (pure Rust)
- mDNS for local discovery
- UDP for efficiency
- Optional compression for bandwidth

**Server/Desktop**:
- Use `rocksdb` for performance
- Full feature set available
- TCP/QUIC for reliable transport

---

## Project Timeline

**Total Duration**: 36 weeks (9 months)

| Phase | Weeks | Dependencies |
|-------|-------|--------------|
| **Phase 1: Core CRDT** | 1-4 | None |
| **Phase 2: Columnar Encoding** | 5-8 | Phase 1 |
| **Phase 3: Storage** | 9-11 | Phase 1, 2 |
| **Phase 4: Discovery/Transport** | 12-16 | Phase 1 |
| **Phase 5: Sync Engine** | 17-20 | Phase 1, 2, 4 |
| **Phase 6: Hierarchy** | 21-24 | Phase 1, 5 |
| **Phase 7: Capability** | 25-27 | Phase 1, 6 |
| **Phase 8: Repository** | 28-30 | All previous |
| **Phase 9: Testing** | 31-34 | All previous |
| **Phase 10: Documentation** | 35-36 | All previous |

---

## Risk Mitigation

### Technical Risks

1. **CRDT Convergence Bugs**
   - Mitigation: Extensive property-based testing
   - Fallback: Use proven Automerge CRDT implementations

2. **Network Discovery Failures**
   - Mitigation: Multiple discovery mechanisms (mDNS, beacon, C2-directed)
   - Fallback: Manual peer configuration

3. **Scale Performance**
   - Mitigation: Early performance testing, profiling
   - Fallback: Adjust hierarchy depth, squad sizes

4. **Storage Corruption**
   - Mitigation: Checksums, write-ahead logging
   - Fallback: Snapshot recovery

### Schedule Risks

1. **Complexity Underestimation**
   - Mitigation: Weekly progress reviews
   - Contingency: 20% time buffer built in

2. **Dependency Issues**
   - Mitigation: Early integration of key dependencies
   - Fallback: Alternative libraries identified

---

## Next Steps

1. **Approve this implementation plan**
2. **Set up project repository**
   - Initialize Cargo workspace
   - Set up CI/CD (GitHub Actions)
   - Configure code quality tools (clippy, rustfmt)

3. **Begin Phase 1: Core CRDT Foundation**
   - Start with LWW-Register implementation
   - Write property-based tests
   - Establish coding conventions

4. **Weekly Progress Reviews**
   - Review completed work
   - Adjust timeline as needed
   - Address blockers

---

## Conclusion

This implementation plan provides a comprehensive roadmap for building `crdt-edge` (name TBD), a modular Rust CRDT library that:

**For General Mobile/Edge Applications**:
- **Lightweight Core**: Use just CRDTs + storage for simple offline apps
- **Efficient Sync**: Columnar encoding provides 80%+ compression vs JSON
- **Multiple Storage Options**: SQLite for mobile, RocksDB for servers, Sled for pure Rust
- **Flexible Networking**: Choose transports (TCP, UDP, WebSocket, Bluetooth) as needed
- **Offline-First**: Works indefinitely without connectivity, syncs automatically

**For CAP-Specific Tactical Applications**:
- **Hierarchical Organization**: O(n log n) scaling via Squad/Platoon/Company structure
- **Capability Composition**: Detect emergent team capabilities automatically
- **Priority Sync**: Mission-critical data arrives within 5 seconds
- **Extreme Compression**: 95%+ compression for bandwidth-constrained networks
- **Discovery Protocols**: Bootstrap squads from unorganized platforms

**Key Design Principles**:
1. **Modular**: Only include features you need (via Cargo feature flags)
2. **General-Purpose**: CAP is one use case, not the only use case
3. **Production-Ready**: Battle-tested storage, robust networking, comprehensive tests
4. **Cross-Platform**: Works on mobile, embedded, server platforms

**The phased approach ensures**:
- Steady progress with clear deliverables at each stage
- Core functionality useful before CAP-specific features
- Testing at every level (unit, integration, scale, property-based)
- Documentation for both general and CAP-specific use cases

**Timeline**: 36 weeks (9 months) with:
- Weeks 1-11: Core CRDT + Encoding + Storage (useful for any app)
- Weeks 12-20: Discovery + Transport + Sync (general peer-to-peer)
- Weeks 21-27: CAP-specific features (hierarchy + capabilities)
- Weeks 28-36: Repository + Testing + Documentation

The resulting crate will be valuable to:
- **Mobile developers** building offline-first apps
- **IoT developers** needing edge sync
- **Collaborative tool builders** wanting real-time sync
- **Tactical systems developers** (CAP) needing hierarchical coordination at scale

By making this general-purpose with CAP as an extension, we:
- Get broader community adoption and testing
- Maintain cleaner abstractions
- Enable unforeseen use cases
- Create a more valuable open-source contribution

---

**Document Status**: Ready for Review  
**Last Updated**: 2025-11-03  
**Version**: 1.0
