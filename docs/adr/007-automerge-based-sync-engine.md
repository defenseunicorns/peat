# ADR-007: Automerge-Based Sync Engine (Rip-and-Replace Strategy)

**Status**: Proposed
**Date**: 2025-11-04
**Authors**: Claude, Kit Plummer
**Supersedes**: ADR-005 (Data Sync Abstraction Layer)
**Replaces**: ADR-002 (Beacon Storage Architecture - Ditto-based)

## Context

### Business Constraints

**Critical Requirement**: Eliminate Ditto licensing dependency to avoid:
- **Vendor lock-in** with proprietary SDK
- **Licensing costs** for production tactical deployments
- **Legal constraints** on distribution and modification
- **Support dependencies** on third-party vendor availability

### Strategic Value: OSS/GOTS and NATO Standardization

**Government Off-The-Shelf (GOTS) Opportunity**

An open-source, Automerge-based sync engine positions CAP Protocol as **Government Off-The-Shelf (GOTS)** software, providing critical advantages:

1. **Open Architecture Compliance**
   - Aligns with DoD's **Modular Open Systems Approach (MOSA)**
   - Supports **Open Mission Systems (OMS)** initiative for unmanned systems
   - Enables vendor-neutral integration across platforms
   - Facilitates competition and innovation in tactical autonomous systems

2. **NATO Standardization Path**
   - **STANAG Candidate**: CAP Protocol + automerge-edge could become a NATO standard for autonomous platform coordination
   - **Interoperability**: Allied forces can adopt without licensing barriers
   - **Multi-National Development**: NATO members can contribute improvements
   - **Coalition Operations**: Shared technology base for combined operations

3. **Acquisition Benefits**
   - **Reduced Program Risk**: No proprietary dependencies to negotiate
   - **Faster ATO Process**: Full source code inspection for cybersecurity review
   - **Lower TCO**: No per-unit licensing fees for fleet deployments
   - **Sovereign Control**: Nations maintain full control over critical infrastructure

4. **Industrial Base Advantages**
   - **Prime Contractor Friendly**: Defense contractors can integrate freely
   - **SME Participation**: Lower barriers for small innovative companies
   - **Technology Transfer**: Allies can adapt for national requirements
   - **Export Control**: Simpler ITAR/EAR compliance for open-source components

### Comparison: Proprietary vs GOTS

| Aspect | Ditto (Proprietary) | automerge-edge (GOTS) |
|--------|---------------------|------------------------|
| **Licensing** | Commercial, per-seat | Apache-2.0 or MIT (free) |
| **Source Access** | SDK only, core closed | Full source transparency |
| **Modification Rights** | Restricted by EULA | Unlimited modification |
| **NATO Sharing** | License complications | Freely sharable |
| **Multi-National Dev** | Blocked by IP | Encouraged and supported |
| **Acquisition** | Complex procurement | Simplified as GOTS |
| **Vendor Lock-in** | High | Zero |
| **Export Control** | Complex | Streamlined for OSS |
| **Standardization** | Proprietary barriers | Open standards candidate |
| **Long-term Support** | Vendor-dependent | Community + govt sustainment |

### NATO Standardization Precedents

Historical examples of successful defense technology standardization:

1. **Link 16 (STANAG 5516)** - Tactical data link standard
2. **CBRN Systems (AEP-66)** - Chemical, Biological, Radiological, Nuclear detection
3. **ATDL-1/VMF (STANAG 5500)** - Variable message format for tactical messaging
4. **ASTERIX (STANAG 4761)** - Air traffic surveillance data format

**CAP Protocol + automerge-edge** could become the **STANAG for autonomous platform coordination**, analogous to how Link 16 standardized data sharing between manned platforms.

### Open Architecture Alignment

The DoD's **Digital Engineering Strategy** and **Modular Open Systems Approach (MOSA)** explicitly require:

> "Use of open standards, architectures, and practices to enable innovation, competition, and evolutionary acquisition throughout the system lifecycle."

**automerge-edge meets these requirements**:

✅ **Open Standards**: Uses IETF protocols (TCP, TLS), standard encoding (Automerge columnar format)
✅ **Modular Design**: Pluggable discovery, transport, storage, security
✅ **Well-Defined Interfaces**: Clear API boundaries, documented behavior
✅ **Data Rights**: Full Government Purpose Rights (GPR) via permissive license
✅ **Vendor Neutrality**: No single-source dependency
✅ **Technology Refresh**: Can swap components without system redesign

### Coalition Operations Benefits

**Scenario: US + NATO Allies conduct joint autonomous ISR mission**

With Ditto (Proprietary):
- ❌ Each nation needs separate licenses
- ❌ Export approvals for SDK distribution
- ❌ Cannot modify for national requirements
- ❌ Vendor must approve multi-national access
- ❌ Complex procurement across nations

With automerge-edge (GOTS):
- ✅ All nations freely adopt and deploy
- ✅ Simplified technology transfer
- ✅ Each nation can adapt to doctrine
- ✅ Collaborative development and testing
- ✅ Single acquisition for entire coalition

### Industry and Academic Collaboration

Open-source approach enables broader innovation ecosystem:

**Defense Contractors**:
- General Dynamics, Lockheed Martin, Northrop Grumman can integrate freely
- Small defense tech companies (Shield AI, Anduril, etc.) can build on foundation
- International partners (BAE Systems, Thales, etc.) can contribute

**Research Institutions**:
- MIT, CMU, Stanford can extend for research
- NATO Science & Technology Organization (STO) can evaluate and standardize
- DARPA programs can build on proven foundation

**Open Source Community**:
- Rust ecosystem benefits from mature P2P sync library
- Feedback loop improves robustness
- Security researchers can audit and report vulnerabilities

### Path to NATO STANAG

**Proposed Timeline**:

1. **Year 1: Demonstrate in CAP Protocol**
   - Prove capability in US tactical autonomous systems
   - Publish performance benchmarks and test results
   - Present at DoD and NATO conferences

2. **Year 2: Multi-National Trials**
   - Coordinate with NATO NIAG (Industrial Advisory Group)
   - Conduct interoperability tests with allied systems
   - Gather feedback from NATO member nations

3. **Year 3: Draft STANAG Proposal**
   - Work with NATO Standardization Office (NSO)
   - Define conformance requirements
   - Establish certification process

4. **Year 4-5: Ratification and Adoption**
   - NATO member approval process
   - Integration into allied autonomous systems
   - Establish maintenance and evolution governance

### Government Sustainment Model

Unlike proprietary software dependent on vendor lifecycle, GOTS software has sustainable government ownership:

**Sustainment Options**:
1. **Government In-House**: DoD software factories (Kessel Run, AFWerX) can maintain
2. **Contractor Support**: Any qualified contractor can provide support (competition)
3. **Federally Funded R&D**: SBIR/STTR programs can fund enhancements
4. **Open Source Community**: Leverage broader ecosystem contributions

### Risk Mitigation for OSS Approach

**Concern**: "Open source means less secure"

**Reality**: Security through obscurity is not effective. Open source enables:
- Public security audits (more eyes on code)
- Faster vulnerability patching (community response)
- Cryptographic verification (no hidden backdoors)
- Government security teams can audit directly

**Concern**: "No vendor support"

**Reality**: GOTS software can have multiple support providers:
- Government software factories
- Prime contractors (competed)
- Original developers (commercial support model)
- NATO member nation support teams

**Concern**: "Adversaries can study the code"

**Reality**: Security should not depend on secrecy of algorithms:
- Military cryptography is public (AES, SHA-256, etc.)
- Link 16 specifications are documented
- Security comes from key management, not code secrecy
- adversaries will reverse-engineer anyway

### Technical Analysis

After prototyping with Ditto SDK, we've identified fundamental limitations:
1. **Document update semantics** - No true upsert, creating duplicate documents
2. **Query limitations** - No ORDER BY, limited filtering, no aggregations
3. **Wire protocol inefficiency** - CBOR-based vs superior columnar encoding
4. **Complexity** - Large SDK with many unnecessary features for our use case
5. **Testing brittleness** - Requires real Ditto instances, complicates CI/CD

### Strategic Decision: Rip-and-Replace vs Abstraction

**Decision**: **Rip-and-replace Ditto** with custom implementation built on Automerge

**Rationale**:
- **Abstraction overhead not justified**: Supporting two backends doubles maintenance burden
- **Ditto won't be used in production**: Licensing makes it non-viable for deployment
- **Cleaner codebase**: Direct integration with Automerge avoids indirection
- **Faster development**: Focus effort on one implementation, not maintaining two
- **Better testing**: Mock-friendly architecture without SDK dependencies

**Migration Path**:
1. Keep Ditto for **reference only** during development (compare behaviors)
2. Build new implementation in **parallel crate** (`automerge-edge`)
3. **Swap CellStore/NodeStore** to use new crate once feature-complete
4. **Remove Ditto dependency** entirely from production code
5. Keep Ditto examples as **historical reference** in separate branch

## Decision

Build **`automerge-edge`** - a general-purpose, reusable Rust crate for offline-first, peer-to-peer data synchronization, using Automerge as the CRDT foundation.

### Crate Architecture

```
automerge-edge/
├── Cargo.toml              # Standalone crate, published to crates.io
├── README.md               # General-purpose marketing (not CAP-specific)
├── LICENSE                 # Apache-2.0 or MIT (permissive)
│
├── src/
│   ├── lib.rs              # Public API
│   │
│   ├── core/               # Automerge integration
│   │   ├── mod.rs
│   │   ├── document.rs     # Automerge document wrapper
│   │   ├── sync.rs         # Automerge sync protocol
│   │   └── storage.rs      # Persistence layer
│   │
│   ├── discovery/          # Peer discovery (pluggable)
│   │   ├── mod.rs
│   │   ├── mdns.rs         # mDNS/DNS-SD discovery
│   │   ├── manual.rs       # Manual peer configuration
│   │   └── traits.rs       # Discovery plugin trait
│   │
│   ├── transport/          # Network transports (pluggable)
│   │   ├── mod.rs
│   │   ├── tcp.rs          # TCP transport
│   │   ├── quic.rs         # QUIC transport (future)
│   │   └── traits.rs       # Transport plugin trait
│   │
│   ├── repo/               # Repository (multi-document management)
│   │   ├── mod.rs
│   │   ├── repository.rs   # Main API
│   │   ├── collection.rs   # Collection abstraction
│   │   └── query.rs        # Query engine
│   │
│   ├── sync/               # Synchronization coordination
│   │   ├── mod.rs
│   │   ├── peer_manager.rs # Peer lifecycle
│   │   ├── sync_engine.rs  # Orchestrate sync across peers
│   │   └── priority.rs     # Priority-based sync (optional)
│   │
│   └── security/           # Security layer (from ADR-006)
│       ├── mod.rs
│       ├── auth.rs         # Device/user authentication
│       ├── authz.rs        # Authorization (RBAC)
│       ├── crypto.rs       # Encryption
│       └── audit.rs        # Audit logging
│
├── examples/
│   ├── basic_sync.rs       # Simple two-peer sync
│   ├── collections.rs      # Collection-based usage
│   ├── offline_notes.rs    # Offline notes app
│   └── iot_mesh.rs         # IoT sensor network
│
└── tests/
    ├── sync_tests.rs       # Two-peer sync tests
    ├── partition_tests.rs  # Network partition tolerance
    └── e2e_tests.rs        # End-to-end scenarios
```

### Core Design Principles

1. **Automerge as CRDT Foundation**
   - Use `automerge` crate for all CRDT operations
   - Leverage columnar encoding (85-95% compression)
   - Rich CRDT types: maps, lists, text, counters

2. **Modular Architecture**
   - **Discovery** is pluggable (mDNS, manual, Bluetooth, etc.)
   - **Transport** is pluggable (TCP, QUIC, WebSocket, etc.)
   - **Storage** is pluggable (RocksDB, SQLite, in-memory)
   - **Security** is optional but integrated (from ADR-006)

3. **General-Purpose by Design**
   - Not CAP-specific - usable for any offline-first app
   - Collections API similar to MongoDB/Ditto
   - Observable changes for reactive UIs
   - Works on mobile, embedded, server, desktop

4. **Production-Ready**
   - Comprehensive error handling
   - Instrumentation and metrics
   - Testing at all levels (unit, integration, E2E)
   - Performance benchmarks vs Ditto

## Architecture Deep-Dive

### 1. Automerge Integration Layer

Automerge provides CRDTs, but we need to add:

```rust
use automerge::{Automerge, ReadDoc, transaction::Transactable};

/// Wrapper around Automerge document with metadata
pub struct Document {
    /// Underlying Automerge document
    doc: Automerge,

    /// Document ID (UUID)
    id: DocumentId,

    /// Collection name (for organization)
    collection: String,

    /// Local metadata (not synced)
    metadata: DocumentMetadata,
}

impl Document {
    /// Create new document
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            doc: Automerge::new(),
            id: DocumentId::new_v4(),
            collection: collection.into(),
            metadata: DocumentMetadata::default(),
        }
    }

    /// Update document (transactional)
    pub fn update<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Automerge) -> Result<R>,
    {
        let result = f(&mut self.doc)?;
        self.metadata.updated_at = SystemTime::now();
        Ok(result)
    }

    /// Get value at path
    pub fn get(&self, path: &[&str]) -> Result<Value> {
        let mut obj = self.doc.root();
        for &key in path {
            obj = self.doc.get(obj, key)?;
        }
        Ok(Value::from_automerge(obj))
    }

    /// Set value at path
    pub fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        self.doc.transaction(|tx| {
            let mut obj = tx.root();
            for &key in &path[..path.len() - 1] {
                obj = tx.get(obj, key)?;
            }
            tx.put(obj, path[path.len() - 1], value.to_automerge())?;
            Ok(())
        })
    }

    /// Generate sync message for peer
    pub fn generate_sync_message(&mut self, peer_state: &SyncState) -> Result<Vec<u8>> {
        automerge::sync::generate_sync_message(&mut self.doc, peer_state)
    }

    /// Receive sync message from peer
    pub fn receive_sync_message(&mut self, message: &[u8]) -> Result<()> {
        automerge::sync::receive_sync_message(&mut self.doc, message)
    }

    /// Get document as JSON (for querying)
    pub fn to_json(&self) -> Result<serde_json::Value> {
        automerge::export(&self.doc)
    }
}
```

### 2. Repository API (High-Level Interface)

```rust
/// Repository manages multiple documents with collections
pub struct Repository {
    /// Storage backend
    storage: Box<dyn StorageBackend>,

    /// Peer manager (discovery + connections)
    peers: Arc<PeerManager>,

    /// Sync engine (coordinates sync across peers)
    sync: Arc<SyncEngine>,

    /// Security manager (optional)
    security: Option<Arc<SecurityManager>>,

    /// Document cache (in-memory)
    documents: Arc<RwLock<HashMap<DocumentId, Document>>>,
}

impl Repository {
    /// Create new repository with RocksDB storage
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let storage = RocksDbStorage::new(path)?;
        let peers = Arc::new(PeerManager::new());
        let sync = Arc::new(SyncEngine::new(peers.clone()));

        Ok(Self {
            storage: Box::new(storage),
            peers,
            sync,
            security: None,
            documents: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create in-memory repository (for testing)
    pub fn new_in_memory() -> Self {
        Self {
            storage: Box::new(InMemoryStorage::new()),
            peers: Arc::new(PeerManager::new()),
            sync: Arc::new(SyncEngine::new(peers.clone())),
            security: None,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Access a collection
    pub fn collection(&self, name: impl Into<String>) -> Collection {
        Collection::new(name.into(), self.clone())
    }

    /// Start peer discovery
    pub async fn start_discovery(&self, config: DiscoveryConfig) -> Result<()> {
        match config.method {
            DiscoveryMethod::Mdns => {
                let discovery = MdnsDiscovery::new(config)?;
                self.peers.add_discovery(Box::new(discovery)).await?;
            }
            DiscoveryMethod::Manual(addrs) => {
                for addr in addrs {
                    self.peers.add_manual_peer(addr).await?;
                }
            }
        }
        Ok(())
    }

    /// Start synchronization
    pub async fn start_sync(&self) -> Result<()> {
        self.sync.start().await
    }

    /// Connect to specific peer
    pub async fn connect(&self, addr: &str) -> Result<PeerId> {
        let transport = TcpTransport::new();
        self.peers.connect(addr, Box::new(transport)).await
    }
}

/// Collection provides MongoDB-like API
pub struct Collection {
    name: String,
    repo: Repository,
}

impl Collection {
    /// Insert document into collection
    pub async fn insert(&self, data: serde_json::Value) -> Result<DocumentId> {
        let mut doc = Document::new(&self.name);

        // Convert JSON to Automerge operations
        doc.update(|automerge| {
            populate_from_json(automerge, data)?;
            Ok(())
        })?;

        // Store locally
        let doc_id = doc.id;
        self.repo.storage.store(&doc).await?;
        self.repo.documents.write().await.insert(doc_id, doc.clone());

        // Broadcast to peers
        self.repo.sync.broadcast_document(&doc).await?;

        Ok(doc_id)
    }

    /// Find documents matching query
    pub async fn find(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        // Load all documents in collection
        let docs = self.repo.storage.load_collection(&self.name).await?;

        // Convert to JSON for querying
        let json_docs: Vec<_> = docs
            .into_iter()
            .map(|doc| doc.to_json())
            .collect::<Result<_>>()?;

        // Apply query (simple implementation - can be optimized)
        let filtered = json_docs
            .into_iter()
            .filter(|doc| matches_query(doc, query))
            .collect();

        Ok(filtered)
    }

    /// Find one document
    pub async fn find_one(&self, query: &str) -> Result<Option<serde_json::Value>> {
        self.find(query).await.map(|mut docs| docs.pop())
    }

    /// Update documents matching query
    pub async fn update(&self, query: &str, update: serde_json::Value) -> Result<usize> {
        let docs = self.repo.storage.load_collection(&self.name).await?;
        let mut count = 0;

        for mut doc in docs {
            if matches_query(&doc.to_json()?, query) {
                doc.update(|automerge| {
                    apply_update(automerge, &update)?;
                    Ok(())
                })?;

                self.repo.storage.store(&doc).await?;
                self.repo.sync.broadcast_document(&doc).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Observe changes to collection
    pub fn observe(&self) -> ChangeStream {
        // Return a stream of changes
        self.repo.sync.subscribe_collection(&self.name)
    }
}
```

### 3. Peer Discovery (Pluggable)

```rust
/// Discovery plugin trait
#[async_trait]
pub trait DiscoveryPlugin: Send + Sync {
    /// Start discovery
    async fn start(&mut self) -> Result<()>;

    /// Stop discovery
    async fn stop(&mut self) -> Result<()>;

    /// Get discovered peers
    async fn discovered_peers(&self) -> Vec<PeerInfo>;

    /// Stream of discovery events
    fn event_stream(&self) -> mpsc::UnboundedReceiver<DiscoveryEvent>;
}

/// mDNS-based discovery (for local networks)
pub struct MdnsDiscovery {
    service_name: String,
    port: u16,
    discovered: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    events: mpsc::UnboundedSender<DiscoveryEvent>,
}

#[async_trait]
impl DiscoveryPlugin for MdnsDiscovery {
    async fn start(&mut self) -> Result<()> {
        // Register mDNS service
        let mdns = mdns_sd::ServiceDaemon::new()?;
        let service_type = format!("_{}.{}", self.service_name, "_tcp.local.");

        mdns.register(mdns_sd::ServiceInfo::new(
            &service_type,
            &format!("{}-{}", self.service_name, uuid::Uuid::new_v4()),
            &format!("{}:{}", get_local_ip()?, self.port),
            "automerge-edge discovery",
        )?)?;

        // Browse for other peers
        let receiver = mdns.browse(&service_type)?;
        let discovered = self.discovered.clone();
        let events = self.events.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
                        let peer_info = PeerInfo {
                            peer_id: PeerId::from_string(&info.get_fullname()),
                            address: info.get_addresses().iter().next().unwrap().to_string(),
                            port: info.get_port(),
                        };

                        discovered.write().await.insert(peer_info.peer_id, peer_info.clone());
                        events.send(DiscoveryEvent::PeerFound(peer_info)).ok();
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                        let peer_id = PeerId::from_string(&fullname);
                        discovered.write().await.remove(&peer_id);
                        events.send(DiscoveryEvent::PeerLost(peer_id)).ok();
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.discovered.read().await.values().cloned().collect()
    }

    fn event_stream(&self) -> mpsc::UnboundedReceiver<DiscoveryEvent> {
        // Clone receiver
        self.events.subscribe()
    }
}

/// Manual peer configuration (for tactical edge with known addresses)
pub struct ManualDiscovery {
    peers: Vec<PeerInfo>,
}

impl ManualDiscovery {
    pub fn new(peers: Vec<PeerInfo>) -> Self {
        Self { peers }
    }
}

#[async_trait]
impl DiscoveryPlugin for ManualDiscovery {
    async fn start(&mut self) -> Result<()> {
        // Nothing to start - peers are static
        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.peers.clone()
    }

    // ... other methods
}
```

### 4. Transport Layer (Pluggable)

```rust
/// Transport plugin trait
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to peer
    async fn connect(&self, address: &str) -> Result<Box<dyn Connection>>;

    /// Listen for incoming connections
    async fn listen(&self, address: &str) -> Result<Box<dyn Listener>>;
}

/// Connection abstraction
#[async_trait]
pub trait Connection: Send + Sync {
    /// Send message
    async fn send(&mut self, message: &[u8]) -> Result<()>;

    /// Receive message
    async fn recv(&mut self) -> Result<Vec<u8>>;

    /// Get peer ID
    fn peer_id(&self) -> PeerId;

    /// Close connection
    async fn close(&mut self) -> Result<()>;
}

/// TCP transport implementation
pub struct TcpTransport;

#[async_trait]
impl Transport for TcpTransport {
    async fn connect(&self, address: &str) -> Result<Box<dyn Connection>> {
        let stream = TcpStream::connect(address).await?;
        Ok(Box::new(TcpConnection::new(stream)))
    }

    async fn listen(&self, address: &str) -> Result<Box<dyn Listener>> {
        let listener = TcpListener::bind(address).await?;
        Ok(Box::new(TcpListener { listener }))
    }
}

/// TCP connection wrapper
pub struct TcpConnection {
    stream: TcpStream,
    peer_id: PeerId,
    read_buf: BytesMut,
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, message: &[u8]) -> Result<()> {
        // Frame message with length prefix
        let len = message.len() as u32;
        self.stream.write_u32(len).await?;
        self.stream.write_all(message).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>> {
        // Read length prefix
        let len = self.stream.read_u32().await? as usize;

        // Read message
        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    async fn close(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}
```

### 5. Sync Engine (Orchestration)

```rust
/// Sync engine coordinates synchronization across peers
pub struct SyncEngine {
    peer_manager: Arc<PeerManager>,
    sync_states: Arc<RwLock<HashMap<(DocumentId, PeerId), SyncState>>>,
    change_subscribers: Arc<RwLock<HashMap<String, Vec<mpsc::UnboundedSender<Change>>>>>,
}

impl SyncEngine {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self {
            peer_manager,
            sync_states: Arc::new(RwLock::new(HashMap::new())),
            change_subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start sync loop for all connected peers
    pub async fn start(&self) -> Result<()> {
        let peers = self.peer_manager.connected_peers().await;

        for peer_id in peers {
            self.start_peer_sync(peer_id).await?;
        }

        // Subscribe to peer events
        let mut events = self.peer_manager.event_stream();
        let engine = self.clone();

        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    PeerEvent::Connected(peer_id) => {
                        engine.start_peer_sync(peer_id).await.ok();
                    }
                    PeerEvent::Disconnected(peer_id) => {
                        engine.stop_peer_sync(peer_id).await.ok();
                    }
                }
            }
        });

        Ok(())
    }

    /// Sync a specific document with peer
    async fn sync_document(
        &self,
        doc: &mut Document,
        peer_id: PeerId,
        conn: &mut dyn Connection,
    ) -> Result<()> {
        // Get sync state for this doc/peer pair
        let mut sync_states = self.sync_states.write().await;
        let sync_state = sync_states
            .entry((doc.id, peer_id))
            .or_insert_with(|| SyncState::new());

        // Generate sync message
        let message = doc.generate_sync_message(sync_state)?;

        // Send to peer
        conn.send(&message).await?;

        // Receive response
        let response = conn.recv().await?;

        // Apply changes
        doc.receive_sync_message(&response)?;

        // Notify subscribers of changes
        self.notify_subscribers(&doc.collection, doc).await;

        Ok(())
    }

    /// Subscribe to changes in a collection
    pub fn subscribe_collection(&self, collection: &str) -> ChangeStream {
        let (tx, rx) = mpsc::unbounded_channel();

        self.change_subscribers
            .write()
            .unwrap()
            .entry(collection.to_string())
            .or_insert_with(Vec::new)
            .push(tx);

        ChangeStream { receiver: rx }
    }

    /// Broadcast document to all peers
    pub async fn broadcast_document(&self, doc: &Document) -> Result<()> {
        let peers = self.peer_manager.connected_peers().await;

        for peer_id in peers {
            if let Some(mut conn) = self.peer_manager.get_connection(peer_id).await {
                self.sync_document(&mut doc.clone(), peer_id, &mut *conn).await?;
            }
        }

        Ok(())
    }
}
```

## Integration with Security (ADR-006)

Security integrates at multiple layers:

```rust
impl Repository {
    /// Create repository with security enabled
    pub async fn new_secure(
        path: impl AsRef<Path>,
        security_config: SecurityConfig,
    ) -> Result<Self> {
        let mut repo = Self::new(path).await?;

        // Initialize security manager
        let security = SecurityManager::new(security_config)?;
        repo.security = Some(Arc::new(security));

        // Wrap transport with TLS
        let tls_transport = TlsTransport::wrap(TcpTransport::new(), security.clone());
        repo.peers.set_transport(Box::new(tls_transport)).await;

        Ok(repo)
    }
}

/// Secure collection wrapper
impl Collection {
    /// Insert with authorization check
    pub async fn insert_secure(
        &self,
        data: serde_json::Value,
        entity: &AuthenticatedEntity,
    ) -> Result<DocumentId> {
        // Check authorization
        if let Some(security) = &self.repo.security {
            security.authorize(entity, Permission::WriteCollection, &self.name)?;
        }

        // Encrypt document
        let encrypted = if let Some(security) = &self.repo.security {
            security.encrypt_document(&data)?
        } else {
            data
        };

        // Store
        self.insert(encrypted).await
    }
}
```

## CAP Protocol Integration

CAP Protocol uses `automerge-edge` as a library:

```rust
// In cap-protocol/Cargo.toml
[dependencies]
automerge-edge = { version = "0.1", features = ["security", "priority-sync"] }

// In cap-protocol/src/storage/mod.rs
use automerge_edge::{Repository, Collection};

pub struct CellStore {
    repo: Arc<Repository>,
    collection: Collection,
}

impl CellStore {
    pub async fn new(repo: Arc<Repository>) -> Result<Self> {
        Ok(Self {
            collection: repo.collection("cells"),
            repo,
        })
    }

    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        let data = serde_json::to_value(cell)?;
        let doc_id = self.collection.insert(data).await?;
        Ok(doc_id.to_string())
    }

    pub async fn get_cell(&self, cell_id: &str) -> Result<Option<CellState>> {
        let query = format!("config.id == '{}'", cell_id);
        let doc = self.collection.find_one(&query).await?;
        Ok(doc.map(|d| serde_json::from_value(d)).transpose()?)
    }

    pub async fn set_leader(&self, cell_id: &str, leader_id: String) -> Result<()> {
        let query = format!("config.id == '{}'", cell_id);
        let update = serde_json::json!({ "leader_id": leader_id });
        self.collection.update(&query, update).await?;
        Ok(())
    }
}
```

## Migration Strategy

### Phase 1: Create Standalone Crate (Weeks 1-8)

**Goal**: Build `automerge-edge` crate with basic functionality

Tasks:
- [ ] Set up standalone crate structure
- [ ] Integrate Automerge for CRDTs
- [ ] Implement Repository and Collection APIs
- [ ] Add RocksDB storage backend
- [ ] Implement TCP transport
- [ ] Implement mDNS discovery
- [ ] Write comprehensive tests
- [ ] **Milestone**: Two processes can sync documents over TCP

### Phase 2: Feature Parity with Ditto (Weeks 9-16)

**Goal**: Match capabilities currently used by CAP Protocol

Tasks:
- [ ] Collection queries (find, find_one, update)
- [ ] Observable changes (ChangeStream)
- [ ] Peer lifecycle management
- [ ] Connection recovery and retry
- [ ] **Milestone**: All CAP storage tests pass with automerge-edge

### Phase 3: Add Security (Weeks 17-20)

**Goal**: Integrate security from ADR-006

Tasks:
- [ ] Device authentication (PKI)
- [ ] TLS transport wrapper
- [ ] Authorization checks
- [ ] Encrypted storage
- [ ] **Milestone**: Secure sync with authenticated peers

### Phase 4: Replace Ditto in CAP Protocol (Weeks 21-24)

**Goal**: Complete migration

Tasks:
- [ ] Update CellStore to use automerge-edge
- [ ] Update NodeStore to use automerge-edge
- [ ] Update E2E tests
- [ ] Remove Ditto dependency
- [ ] Performance benchmarks (vs Ditto baseline)
- [ ] **Milestone**: CAP Protocol fully operational without Ditto

### Phase 5: Publish and Promote (Weeks 25+)

**Goal**: Make available to broader community

Tasks:
- [ ] Publish to crates.io
- [ ] Write comprehensive documentation
- [ ] Create tutorial and examples
- [ ] Blog post announcing Ditto alternative
- [ ] Engage with Automerge community

## Advantages of This Approach

### Technical Benefits

1. **Better CRDT Foundation**: Automerge's columnar encoding is superior to Ditto's CBOR
2. **Cleaner Architecture**: Direct integration, no abstraction overhead
3. **Testability**: Easy to mock, no SDK dependencies
4. **Transparency**: Full source code visibility and control
5. **Performance**: Can optimize for CAP's specific access patterns

### Business Benefits

1. **No Licensing Costs**: Open-source, permissive license
2. **No Vendor Lock-in**: Own the entire stack
3. **Reusable Asset**: Can be used in other projects
4. **Community Building**: Potential for external contributors
5. **Competitive Advantage**: Differentiated IP

### Ecosystem Benefits

1. **Fills Gap**: Automerge lacks networking/discovery
2. **General Purpose**: Useful beyond CAP Protocol
3. **Production Ready**: Unlike many CRDT research projects
4. **Modern Rust**: Idiomatic, async, type-safe
5. **Open Source**: Apache-2.0 or MIT license

## Risks and Mitigations

### Risk 1: Development Time

**Risk**: Building from scratch takes longer than using Ditto
**Mitigation**:
- Incremental development (MVP first)
- Leverage existing crates (Automerge, RocksDB, mDNS)
- Keep Ditto reference for behavior comparison

### Risk 2: Feature Gap

**Risk**: Missing advanced Ditto features (offline sync, conflict resolution)
**Mitigation**:
- Automerge handles CRDTs and conflict resolution
- Only implement features CAP actually needs
- Iterative development based on requirements

### Risk 3: Testing Complexity

**Risk**: Need to test P2P mesh behavior thoroughly
**Mitigation**:
- Learn from Ditto E2E test patterns
- Use test harness with multiple in-process instances
- Property-based testing for CRDT invariants

### Risk 4: Performance

**Risk**: Custom implementation may be slower than Ditto
**Mitigation**:
- Benchmark against Ditto throughout development
- Profile and optimize hot paths
- Automerge's columnar encoding is proven efficient

## Success Criteria

1. **Feature Complete**: Matches Ditto capabilities used by CAP
2. **Performance Equivalent**: Within 20% of Ditto on key metrics
3. **Test Coverage**: 80%+ coverage, all E2E tests passing
4. **Documentation**: Complete API docs and tutorials
5. **Zero Ditto Dependency**: CAP Protocol compiles without Ditto
6. **Reusability**: At least one example of non-CAP usage

## References

- [Automerge Repository](https://github.com/automerge/automerge) - CRDT foundation
- [Automerge 2.0 Blog Post](https://automerge.org/blog/2023/11/06/automerge-2/) - Columnar encoding
- [CAP_Rust_Implementation_Plan.md](../CAP_Rust_Implementation_Plan.md) - Detailed design
- [ADR-006](006-security-authentication-authorization.md) - Security integration
- [Ditto SDK Documentation](https://docs.ditto.live/) - Reference for feature parity

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-04 | Rip-and-replace Ditto with automerge-edge | Licensing constraints, better architecture |
| 2025-11-04 | Use Automerge as CRDT foundation | Proven columnar encoding, active development |
| 2025-11-04 | Build as standalone, reusable crate | Maximize value beyond CAP Protocol |
| TBD | Approved/Rejected | After team review |

## Strategic Value Proposition

### For US Department of Defense

**Immediate Benefits**:
- Eliminate proprietary licensing costs ($X million saved over program lifecycle)
- Faster ATO/security certification (full source review)
- Sovereign control over critical autonomy infrastructure
- No vendor dependency for mission-critical systems

**Long-term Benefits**:
- Foundation for DoD-wide autonomous systems coordination
- Reference implementation for Modular Open Systems Approach (MOSA)
- Competitive market for integration and support contractors
- Technology advantage without vendor lock-in

### For NATO Allies

**Interoperability**:
- Common protocol for multi-national autonomous operations
- No licensing barriers for coalition partners
- Each nation can adapt to national doctrine
- Shared investment in common capability

**Industrial Base**:
- European defense contractors can integrate and support
- Stimulates allied autonomous systems development
- Technology transfer without ITAR complications
- Jobs and capability development in member nations

### For Defense Industry

**Prime Contractors**:
- Freedom to integrate without licensing negotiations
- Can offer differentiated solutions on common foundation
- Reduced program risk from vendor dependencies
- Competitive advantage with proven technology

**Small/Medium Enterprises**:
- Lower barriers to entry (no SDK licensing costs)
- Can innovate on proven foundation
- Compete for government contracts
- Contribute to standardization process

### For Open Source Ecosystem

**Rust Community**:
- Production-grade P2P sync library
- Real-world CRDT implementation at scale
- Embedded/edge computing example
- Security and cryptography patterns

**Automerge Project**:
- Networking layer contribution
- Discovery and transport extensions
- Production validation and feedback
- Expanded user base and use cases

### For Academia

**Research Value**:
- Open platform for CRDT research
- Distributed systems experimentation
- Human-autonomy teaming studies
- Coalition coordination algorithms

**Educational Value**:
- Real-world distributed systems example
- Security and cryptography case study
- Government software development model
- Open architecture principles

## Recommended Next Steps

### Immediate (Next 2 Weeks)

1. **Stakeholder Alignment**
   - Present ADR-007 to CAP Protocol team
   - Discuss OSS/GOTS strategy with government sponsors
   - Identify NATO contacts for future coordination
   - Engage with Automerge maintainers about collaboration

2. **Technical Validation**
   - Prototype basic Automerge integration
   - Benchmark sync performance vs Ditto
   - Validate columnar encoding benefits
   - Assess effort for peer discovery and transport

3. **Licensing Strategy**
   - Select open source license (recommend: Apache-2.0)
   - Plan for government rights assertions
   - Identify export control considerations
   - Define contribution process

### Short-term (Next 3 Months)

1. **Core Development** (Weeks 1-8)
   - Create automerge-edge repository
   - Implement basic sync functionality
   - Add TCP transport and mDNS discovery
   - Achieve feature parity for CAP Protocol needs

2. **Security Integration** (Weeks 9-12)
   - Implement device authentication (ADR-006)
   - Add TLS transport wrapper
   - Integrate authorization checks
   - Enable encrypted storage

3. **CAP Integration** (Week 13)
   - Port CellStore/NodeStore to automerge-edge
   - Update E2E tests
   - Validate performance
   - Remove Ditto dependency

### Medium-term (6-12 Months)

1. **Production Hardening**
   - Comprehensive testing (unit, integration, E2E)
   - Performance optimization
   - Security audit
   - Documentation and examples

2. **Community Building**
   - Publish to crates.io
   - Present at RustConf / EuroRust
   - Engage with Automerge community
   - Attract early adopters

3. **Government Engagement**
   - Present at DoD software conferences
   - Demo at NATO NIAG meetings
   - Coordinate with program offices
   - Identify pilot programs

### Long-term (1-3 Years)

1. **Standardization**
   - Draft technical specification
   - Coordinate with NATO Standardization Office
   - Multi-national interoperability tests
   - Conformance certification process

2. **Ecosystem Growth**
   - Support contractor integrations
   - Enable academic research
   - Foster community contributions
   - Expand use cases beyond tactical autonomy

3. **Sustainment**
   - Establish governance model
   - Define support options
   - Plan evolution roadmap
   - Ensure long-term viability

## Open Questions

1. **Should we fork Automerge or use as-is?**
   - Risk: Breaking changes in Automerge upstream
   - Option: Fork and vendor for stability
   - Recommendation: Use as dependency initially, fork only if needed

2. **What license should automerge-edge use?**
   - Apache-2.0 (same as Automerge) for ecosystem consistency?
   - MIT for maximum permissiveness?
   - Recommendation: **Apache-2.0** (matches Automerge, DoD-friendly, NATO-compatible)

3. **Should we target crates.io publication from day one?**
   - Or keep private until proven in CAP Protocol?
   - Recommendation: **Public from day one** (builds community, attracts contributors, demonstrates commitment to OSS)

4. **How to handle schema evolution?**
   - Automerge is schema-less, but CAP models are typed
   - Need versioning strategy for breaking changes
   - Recommendation: Use semantic versioning, document migration paths

5. **Should we contribute improvements back to Automerge?**
   - e.g., networking layer, discovery, etc.
   - Coordinate with Automerge maintainers?
   - Recommendation: **Yes** - submit networking/transport as separate Automerge modules, maintain good upstream relationship

6. **When to engage NATO Standardization Office?**
   - Too early risks premature specification
   - Too late misses opportunity for input
   - Recommendation: **Year 2** after proving capability in CAP Protocol, before architecture solidifies

7. **How to balance CAP-specific vs general-purpose?**
   - automerge-edge should be general-purpose
   - CAP-specific features (hierarchy, capability composition) as separate layer
   - Recommendation: Keep automerge-edge pure sync engine, build `cap-protocol-core` on top
