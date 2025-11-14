# ADR-017: P2P Mesh Management and Discovery Architecture

**Status**: Proposed  
**Date**: 2025-11-14  
**Authors**: Kit Plummer, Claude  
**Supersedes**: None  
**Relates To**: ADR-001 (CAP Protocol POC), ADR-002 (Beacon Storage), ADR-011 (Automerge + Iroh), ADR-013 (Distributed Software & AI Ops)

---

## Context and Problem Statement

### The P2P Gap

ADR-011 established **Automerge + Iroh** as the foundation for CAP's networking stack. However, analysis revealed a critical architectural gap:

**Iroh provides** (~20% of P2P mesh problem):
- ✅ Reliable QUIC connections between peers
- ✅ Multi-path networking (Starlink + MANET + 5G)
- ✅ Connection migration and resilience
- ✅ NAT traversal via relay servers
- ✅ Transport-level encryption (TLS 1.3)

**Iroh does NOT provide** (~80% of P2P mesh problem):
- ❌ Peer discovery mechanisms (assumes you know EndpointId + addresses)
- ❌ Mesh topology optimization and hierarchy
- ❌ Geographic-based node organization
- ❌ Selective data routing based on military command structure
- ❌ Parent/child relationship management
- ❌ Mesh healing and failover logic
- ❌ Aggregation and filtering logic
- ❌ Beacon lifecycle management

**Critical insight**: Iroh provides excellent **point-to-point connections**, but CAP requires **intelligent mesh coordination** on top of those connections to enable hierarchical military operations.

### Tactical Requirements

CAP must support military command hierarchy where:

1. **Platforms** discover each other geographically and form squads
2. **Squad leaders** aggregate platform data and coordinate local actions
3. **Platoon leaders** coordinate multiple squads
4. **Company HQ** receives aggregated situational awareness
5. **All levels** can operate autonomously during network partitions

This requires **application-level mesh intelligence** that determines:
- Which peers should connect to whom (topology)
- What data should flow where (routing)
- How to reorganize when nodes fail (healing)
- How to minimize bandwidth usage (aggregation)

### Testing Reality: Containerlab vs Simulation

**Challenge**: Shadow network simulator failed due to incomplete TCP socket option support, forcing migration to Containerlab.

**Containerlab characteristics**:
- ✅ Real Linux containers with actual networking stacks
- ✅ Real Rust code, real Automerge+Iroh, real measurements
- ✅ Network partition simulation via Linux `tc`
- ✅ Observable connections and traffic flows
- ❌ Limited to ~200-500 nodes on single machine
- ❌ Cannot simulate actual geographic movement
- ❌ Expensive to scale to 1000+ nodes

**Implication**: Validation strategy must prove O(n log n) scaling through:
1. **Empirical testing** at achievable scale (50-200 nodes)
2. **Mathematical modeling** validated against empirical data
3. **Extrapolation** to 1000+ node scenarios

---

## Decision

**We will build a three-layer P2P mesh coordination architecture:**

### Layer 1: Discovery Strategies (What Peers Exist?)
- **mDNS Discovery**: Zero-config local network peer discovery
- **Static Configuration**: Pre-planned peer lists for EMCON operations
- **Relay Discovery**: Cross-network peer discovery via self-hosted relays

### Layer 2: Mesh Topology Management (Who Connects to Whom?)
- **Geographic Beacons**: Nodes broadcast position/status via Automerge CRDTs
- **Hierarchical Organization**: Automatic parent/child relationships based on geography + command structure
- **Connection Management**: Maintain connections to parent, children, and lateral peers
- **Mesh Healing**: Automatic parent failover and topology reorganization

### Layer 3: Data Flow Control (What Goes Where?)
- **Selective Routing**: Route data based on destination and hierarchy
- **Aggregation Logic**: Squad/Platoon leaders summarize before forwarding
- **Priority Handling**: Critical commands bypass aggregation
- **Bandwidth Optimization**: Minimize redundant data transmission

All layers integrate with **Automerge CRDT replication** over **Iroh QUIC transport**.

---

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    CAP Application Layer                     │
│  (Tactical Coordination, Mission Planning, Deconfliction)   │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│              Layer 3: Data Flow Control                      │
│  • SelectiveRouter: Route data based on hierarchy           │
│  • Aggregator: Summarize data at squad/platoon levels       │
│  • PriorityQueue: Ensure critical data flows first          │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│           Layer 2: Mesh Topology Management                  │
│  • BeaconBroadcaster: Advertise node position/status        │
│  • TopologyManager: Maintain hierarchical connections       │
│  • MeshHealer: Detect failures, reorganize topology         │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│              Layer 1: Discovery Strategies                   │
│  • MdnsDiscovery: Local network peer finding                │
│  • StaticConfig: Pre-configured peer lists                  │
│  • RelayDiscovery: Cross-network via relay servers          │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                  Automerge + Iroh Layer                      │
│  • Automerge: CRDT document sync                            │
│  • Iroh: Multi-path QUIC transport                          │
│  • Self-hosted Relay: NAT traversal infrastructure          │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Design

### Layer 1: Discovery Strategies

#### 1.1 Discovery Trait

```rust
// File: cap-discovery/src/lib.rs

use async_trait::async_trait;
use iroh::endpoint::{Endpoint, EndpointId};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub endpoint_id: EndpointId,
    pub addresses: Vec<SocketAddr>,
    pub relay_url: Option<RelayUrl>,
    pub last_seen: Instant,
}

#[derive(Clone, Debug)]
pub enum DiscoveryEvent {
    PeerFound(PeerInfo),
    PeerLost(EndpointId),
}

/// Trait for peer discovery strategies
#[async_trait]
pub trait DiscoveryStrategy: Send + Sync {
    /// Start discovery process
    async fn start(&mut self) -> Result<()>;
    
    /// Stop discovery
    async fn stop(&mut self) -> Result<()>;
    
    /// Get currently discovered peers
    async fn discovered_peers(&self) -> Vec<PeerInfo>;
    
    /// Subscribe to discovery events
    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent>;
}
```

#### 1.2 mDNS Discovery (Local Networks)

```rust
// File: cap-discovery/src/mdns.rs

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_type: String, // "_cap._quic.local."
    discovered: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
    events_rx: Option<mpsc::Receiver<DiscoveryEvent>>,
}

impl MdnsDiscovery {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new()?;
        let (events_tx, events_rx) = mpsc::channel(100);
        
        Ok(Self {
            daemon,
            service_type: "_cap._quic.local.".to_string(),
            discovered: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            events_rx: Some(events_rx),
        })
    }
    
    /// Advertise this node on the local network
    pub fn advertise(
        &self,
        endpoint_id: EndpointId,
        port: u16,
        node_name: &str,
    ) -> Result<()> {
        let service = ServiceInfo::new(
            &self.service_type,
            &format!("cap-{}", endpoint_id),
            &format!("_cap._quic"),
            "",
            port,
            None,
        )?;
        
        // Add EndpointId as TXT record
        let mut properties = HashMap::new();
        properties.insert("endpoint_id".to_string(), endpoint_id.to_string());
        properties.insert("node_name".to_string(), node_name.to_string());
        service.set_properties(properties)?;
        
        self.daemon.register(service)?;
        Ok(())
    }
}

#[async_trait]
impl DiscoveryStrategy for MdnsDiscovery {
    async fn start(&mut self) -> Result<()> {
        let receiver = self.daemon.browse(&self.service_type)?;
        let discovered = self.discovered.clone();
        let events_tx = self.events_tx.clone();
        
        // Spawn background task to process mDNS events
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(peer_info) = Self::parse_service_info(&info) {
                            discovered.write().await.insert(
                                peer_info.endpoint_id.clone(),
                                peer_info.clone()
                            );
                            let _ = events_tx.send(
                                DiscoveryEvent::PeerFound(peer_info)
                            ).await;
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        // Extract EndpointId from fullname
                        if let Some(endpoint_id) = Self::extract_endpoint_id(&fullname) {
                            discovered.write().await.remove(&endpoint_id);
                            let _ = events_tx.send(
                                DiscoveryEvent::PeerLost(endpoint_id)
                            ).await;
                        }
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
    
    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent> {
        self.events_rx.take().expect("event_stream called twice")
    }
}
```

**Complexity**: Low (~300 LOC)  
**Timeline**: 3-4 days  
**Testing**: Easily testable in Containerlab with bridge networking

#### 1.3 Static Configuration Discovery

```rust
// File: cap-discovery/src/static_config.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct StaticPeerConfig {
    pub endpoint_id: String,
    pub addresses: Vec<String>,
    pub relay_url: Option<String>,
    pub priority: u8, // Connection priority (0-255)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub peers: Vec<StaticPeerConfig>,
}

pub struct StaticDiscovery {
    config: DiscoveryConfig,
    peers: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
    events_rx: Option<mpsc::Receiver<DiscoveryEvent>>,
}

impl StaticDiscovery {
    pub fn from_file(path: &Path) -> Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config: DiscoveryConfig = toml::from_str(&config_str)?;
        
        let (events_tx, events_rx) = mpsc::channel(100);
        
        Ok(Self {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            events_rx: Some(events_rx),
        })
    }
}

#[async_trait]
impl DiscoveryStrategy for StaticDiscovery {
    async fn start(&mut self) -> Result<()> {
        // Load all configured peers immediately
        let mut peers = self.peers.write().await;
        
        for peer_config in &self.config.peers {
            let endpoint_id = EndpointId::from_str(&peer_config.endpoint_id)?;
            let addresses: Vec<SocketAddr> = peer_config.addresses
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect();
            
            let peer_info = PeerInfo {
                endpoint_id: endpoint_id.clone(),
                addresses,
                relay_url: peer_config.relay_url
                    .as_ref()
                    .and_then(|s| RelayUrl::from_str(s).ok()),
                last_seen: Instant::now(),
            };
            
            peers.insert(endpoint_id.clone(), peer_info.clone());
            
            // Notify about discovered peer
            let _ = self.events_tx.send(
                DiscoveryEvent::PeerFound(peer_info)
            ).await;
        }
        
        Ok(())
    }
    
    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }
    
    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent> {
        self.events_rx.take().expect("event_stream called twice")
    }
}
```

**Example configuration:**

```toml
# config/peers.toml

[[peers]]
endpoint_id = "company_hq_alpha"
addresses = ["10.0.0.100:5000", "192.168.1.100:5000"]
relay_url = "https://hq-relay.tactical.mil:3479"
priority = 255  # Highest priority - always connect

[[peers]]
endpoint_id = "platoon_1_leader"
addresses = ["10.0.1.50:5000"]
priority = 200

[[peers]]
endpoint_id = "squad_2_leader"
addresses = ["10.0.2.30:5000"]
priority = 150
```

**Complexity**: Low (~200 LOC)  
**Timeline**: 2-3 days  
**Testing**: Trivial - just config file parsing

#### 1.4 Hybrid Discovery Manager

```rust
// File: cap-discovery/src/hybrid.rs

pub struct HybridDiscovery {
    strategies: Vec<Box<dyn DiscoveryStrategy>>,
    combined_peers: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
}

impl HybridDiscovery {
    pub fn new() -> Self {
        let (events_tx, _) = mpsc::channel(1000);
        
        Self {
            strategies: Vec::new(),
            combined_peers: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
        }
    }
    
    pub fn add_strategy(&mut self, strategy: Box<dyn DiscoveryStrategy>) {
        self.strategies.push(strategy);
    }
    
    pub async fn start_all(&mut self) -> Result<()> {
        for strategy in &mut self.strategies {
            strategy.start().await?;
            
            // Subscribe to each strategy's events
            let mut events = strategy.event_stream();
            let combined_peers = self.combined_peers.clone();
            let events_tx = self.events_tx.clone();
            
            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    match event {
                        DiscoveryEvent::PeerFound(peer_info) => {
                            combined_peers.write().await.insert(
                                peer_info.endpoint_id.clone(),
                                peer_info.clone()
                            );
                            let _ = events_tx.send(
                                DiscoveryEvent::PeerFound(peer_info)
                            ).await;
                        }
                        DiscoveryEvent::PeerLost(endpoint_id) => {
                            combined_peers.write().await.remove(&endpoint_id);
                            let _ = events_tx.send(
                                DiscoveryEvent::PeerLost(endpoint_id)
                            ).await;
                        }
                    }
                }
            });
        }
        
        Ok(())
    }
    
    pub async fn all_discovered_peers(&self) -> Vec<PeerInfo> {
        self.combined_peers.read().await.values().cloned().collect()
    }
}
```

**Usage:**

```rust
// Combine multiple discovery strategies
let mut discovery = HybridDiscovery::new();

// Add mDNS for local network
let mdns = MdnsDiscovery::new()?;
discovery.add_strategy(Box::new(mdns));

// Add static config for EMCON mode
let static_disc = StaticDiscovery::from_file("config/peers.toml")?;
discovery.add_strategy(Box::new(static_disc));

// Start all strategies
discovery.start_all().await?;

// Get all discovered peers from all sources
let peers = discovery.all_discovered_peers().await;
```

---

### Layer 2: Mesh Topology Management

#### 2.1 Geographic Beacon System

From ADR-002, each node broadcasts its presence via Automerge CRDT:

```rust
// File: cap-mesh/src/beacon.rs

use geohash::encode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeographicBeacon {
    pub node_id: String,
    pub endpoint_id: EndpointId,
    pub position: GeoPosition,
    pub geohash: String, // Precision 7 (~153m cells)
    pub hierarchy_level: HierarchyLevel,
    pub capabilities: Vec<String>,
    pub operational: bool,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HierarchyLevel {
    Platform = 0,
    Squad = 1,
    Platoon = 2,
    Company = 3,
}

pub struct BeaconBroadcaster {
    automerge_doc: Arc<Mutex<AutomergeBackend>>,
    node_id: String,
    broadcast_interval: Duration,
}

impl BeaconBroadcaster {
    pub async fn start(&self) {
        let mut interval = tokio::time::interval(self.broadcast_interval);
        
        loop {
            interval.tick().await;
            
            // Update beacon in Automerge document
            let beacon = self.get_current_beacon();
            self.broadcast_beacon(&beacon).await;
        }
    }
    
    async fn broadcast_beacon(&self, beacon: &GeographicBeacon) {
        let doc = self.automerge_doc.lock().await;
        
        // Store in collection: node_beacons/{node_id}
        doc.upsert_document(
            "node_beacons",
            &self.node_id,
            &serde_json::to_value(beacon).unwrap()
        ).await;
        
        // Automerge + Iroh will replicate to all connected peers
    }
    
    fn get_current_beacon(&self) -> GeographicBeacon {
        let position = self.get_current_position();
        let geohash = encode(
            Coord { x: position.lon, y: position.lat },
            7 // Precision for ~153m cells
        ).unwrap();
        
        GeographicBeacon {
            node_id: self.node_id.clone(),
            endpoint_id: self.get_endpoint_id(),
            position,
            geohash,
            hierarchy_level: self.get_hierarchy_level(),
            capabilities: self.get_capabilities(),
            operational: true,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}
```

**Beacon Observation:**

```rust
pub struct BeaconObserver {
    automerge_doc: Arc<Mutex<AutomergeBackend>>,
    nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
    my_geohash: String,
}

impl BeaconObserver {
    pub async fn start(&self) {
        // Subscribe to beacon collection changes
        let mut events = self.automerge_doc.lock().await
            .observe_collection("node_beacons")
            .await;
        
        while let Some(event) = events.recv().await {
            match event {
                CollectionEvent::DocumentChanged { doc_id, document } => {
                    let beacon: GeographicBeacon = 
                        serde_json::from_value(document).unwrap();
                    
                    // Only track beacons in same or adjacent geohash cells
                    if self.is_nearby(&beacon.geohash) {
                        self.nearby_beacons.write().await
                            .insert(doc_id, beacon);
                    }
                }
                CollectionEvent::DocumentDeleted { doc_id } => {
                    self.nearby_beacons.write().await.remove(&doc_id);
                }
            }
        }
    }
    
    fn is_nearby(&self, other_geohash: &str) -> bool {
        // Check if within same cell or adjacent cells
        geohash::neighbor(&self.my_geohash, geohash::Direction::N) == other_geohash ||
        geohash::neighbor(&self.my_geohash, geohash::Direction::S) == other_geohash ||
        // ... check all 8 directions
        self.my_geohash == other_geohash
    }
    
    pub async fn get_nearby_beacons(&self) -> Vec<GeographicBeacon> {
        self.nearby_beacons.read().await.values().cloned().collect()
    }
}
```

**TTL and Cleanup:**

```rust
pub struct BeaconJanitor {
    nearby_beacons: Arc<RwLock<HashMap<String, GeographicBeacon>>>,
    ttl: Duration, // e.g., 30 seconds
}

impl BeaconJanitor {
    pub async fn start(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            self.cleanup_expired().await;
        }
    }
    
    async fn cleanup_expired(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let mut beacons = self.nearby_beacons.write().await;
        beacons.retain(|_, beacon| {
            now - beacon.timestamp < self.ttl.as_secs()
        });
    }
}
```

**Complexity**: Moderate (~500 LOC)  
**Timeline**: 1 week  
**Testing**: Containerlab can simulate geography with network topology

#### 2.2 Hierarchical Topology Manager

```rust
// File: cap-mesh/src/topology.rs

pub struct MeshTopology {
    pub my_node_id: String,
    pub my_level: HierarchyLevel,
    pub parent: Option<EndpointId>,
    pub children: Vec<EndpointId>,
    pub peers: Vec<EndpointId>, // Same hierarchy level
}

pub struct TopologyManager {
    topology: Arc<RwLock<MeshTopology>>,
    beacon_observer: Arc<BeaconObserver>,
    endpoint: Arc<Endpoint>,
    connections: Arc<RwLock<HashMap<EndpointId, Connection>>>,
}

impl TopologyManager {
    /// Determine optimal parent based on hierarchy and proximity
    pub async fn select_parent(
        &self,
        beacons: &[GeographicBeacon]
    ) -> Option<EndpointId> {
        let my_level = self.topology.read().await.my_level;
        let target_level = match my_level {
            HierarchyLevel::Platform => HierarchyLevel::Squad,
            HierarchyLevel::Squad => HierarchyLevel::Platoon,
            HierarchyLevel::Platoon => HierarchyLevel::Company,
            HierarchyLevel::Company => return None, // Top level
        };
        
        // Find closest node at next level up
        let my_position = self.get_my_position();
        
        beacons.iter()
            .filter(|b| b.hierarchy_level == target_level && b.operational)
            .min_by_key(|b| {
                let distance = self.haversine_distance(&my_position, &b.position);
                (distance * 1000.0) as u64 // Convert to meters
            })
            .map(|b| b.endpoint_id.clone())
    }
    
    /// Establish connection to parent
    pub async fn connect_to_parent(&self, parent_id: EndpointId) -> Result<()> {
        let conn = self.endpoint.connect(parent_id.clone(), ALPN_CAP).await?;
        
        self.connections.write().await.insert(parent_id.clone(), conn);
        self.topology.write().await.parent = Some(parent_id);
        
        tracing::info!("Connected to parent: {}", parent_id);
        Ok(())
    }
    
    /// Accept child connection
    pub async fn accept_child(&self, child_id: EndpointId, conn: Connection) {
        self.connections.write().await.insert(child_id.clone(), conn);
        self.topology.write().await.children.push(child_id.clone());
        
        tracing::info!("Accepted child: {}", child_id);
    }
    
    /// Maintain mesh connections
    pub async fn maintain_connections(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        
        loop {
            interval.tick().await;
            
            // Check parent connection health
            if let Some(parent_id) = self.topology.read().await.parent.clone() {
                if !self.is_connection_alive(&parent_id).await {
                    tracing::warn!("Parent connection lost, finding new parent");
                    self.find_new_parent().await;
                }
            }
            
            // Prune dead children
            self.prune_dead_children().await;
            
            // Maintain peer connections (lateral coordination)
            self.maintain_peer_connections().await;
        }
    }
    
    async fn is_connection_alive(&self, peer_id: &EndpointId) -> bool {
        let conns = self.connections.read().await;
        if let Some(conn) = conns.get(peer_id) {
            // TODO: Implement heartbeat mechanism
            !conn.is_closed()
        } else {
            false
        }
    }
    
    async fn find_new_parent(&self) {
        let beacons = self.beacon_observer.get_nearby_beacons().await;
        if let Some(new_parent) = self.select_parent(&beacons).await {
            if let Err(e) = self.connect_to_parent(new_parent).await {
                tracing::error!("Failed to connect to new parent: {}", e);
            }
        } else {
            tracing::warn!("No available parent found, operating autonomously");
        }
    }
    
    async fn prune_dead_children(&self) {
        let mut topology = self.topology.write().await;
        let connections = self.connections.read().await;
        
        topology.children.retain(|child_id| {
            if let Some(conn) = connections.get(child_id) {
                !conn.is_closed()
            } else {
                false
            }
        });
    }
}
```

**Complexity**: High (~800 LOC)  
**Timeline**: 2 weeks  
**Testing**: Containerlab can show actual parent/child connections forming

---

### Layer 3: Data Flow Control

#### 3.1 Selective Router

```rust
// File: cap-mesh/src/router.rs

pub struct SelectiveRouter {
    topology: Arc<RwLock<MeshTopology>>,
    my_node_id: String,
    my_level: HierarchyLevel,
}

impl SelectiveRouter {
    /// Determine if this node should consume the document
    pub fn should_consume(&self, doc: &Document) -> bool {
        // Check if document is relevant to this node's level
        match &doc.metadata.target_level {
            Some(level) => *level <= self.my_level,
            None => true, // No restriction, consume everything
        }
    }
    
    /// Determine if this node should forward the document
    pub fn should_forward(&self, doc: &Document) -> bool {
        match &doc.metadata.destination {
            Some(dest) if dest == &self.my_node_id => false, // Final destination
            Some(_) => true, // Needs routing
            None => {
                // Broadcast to hierarchy
                match doc.metadata.direction {
                    DataDirection::Upward => self.my_level < HierarchyLevel::Company,
                    DataDirection::Downward => self.my_level > HierarchyLevel::Platform,
                    DataDirection::Lateral => true,
                }
            }
        }
    }
    
    /// Get next hop for routing
    pub async fn next_hop(&self, doc: &Document) -> Option<EndpointId> {
        let topology = self.topology.read().await;
        
        match doc.metadata.direction {
            DataDirection::Upward => topology.parent.clone(),
            DataDirection::Downward => {
                // Route to appropriate child based on geohash or explicit target
                self.select_child_for_document(doc, &topology).await
            }
            DataDirection::Lateral => {
                // Route to peers at same level
                self.select_peer_for_document(doc, &topology).await
            }
        }
    }
    
    /// Route document through mesh
    pub async fn route_document(
        &self,
        doc: Document,
        endpoint: &Endpoint,
    ) -> Result<()> {
        // Consume if relevant
        if self.should_consume(&doc) {
            self.process_locally(&doc).await;
        }
        
        // Forward if needed
        if self.should_forward(&doc) {
            if let Some(next_hop) = self.next_hop(&doc).await {
                self.send_to_peer(endpoint, next_hop, doc).await?;
            }
        }
        
        Ok(())
    }
}
```

#### 3.2 Hierarchical Aggregator

```rust
// File: cap-mesh/src/aggregator.rs

pub struct HierarchicalAggregator {
    my_level: HierarchyLevel,
    aggregation_interval: Duration,
    pending_data: Arc<Mutex<Vec<Document>>>,
}

impl HierarchicalAggregator {
    /// Aggregate platform telemetry at squad level
    pub async fn aggregate_telemetry(
        &self,
        platform_data: Vec<Document>
    ) -> Document {
        // Example: Squad leader receives individual platform positions
        // Aggregates into squad-level situational awareness
        
        let positions: Vec<GeoPosition> = platform_data
            .iter()
            .filter_map(|d| d.data.get("position").and_then(|p| {
                serde_json::from_value(p.clone()).ok()
            }))
            .collect();
        
        let centroid = self.calculate_centroid(&positions);
        let spread = self.calculate_spread(&positions);
        
        Document {
            id: format!("squad_{}_summary", self.my_node_id),
            metadata: DocumentMetadata {
                source_level: HierarchyLevel::Squad,
                target_level: Some(HierarchyLevel::Platoon),
                direction: DataDirection::Upward,
                priority: Priority::Normal,
                ..Default::default()
            },
            data: json!({
                "squad_position": centroid,
                "formation_spread": spread,
                "platform_count": platform_data.len(),
                "operational_status": self.assess_operational_status(&platform_data),
            }),
        }
    }
    
    /// Start aggregation loop
    pub async fn start(&self, router: Arc<SelectiveRouter>, endpoint: Arc<Endpoint>) {
        let mut interval = tokio::time::interval(self.aggregation_interval);
        
        loop {
            interval.tick().await;
            
            // Aggregate pending data
            let data = {
                let mut pending = self.pending_data.lock().await;
                std::mem::take(&mut *pending)
            };
            
            if !data.is_empty() {
                let aggregated = self.aggregate_telemetry(data).await;
                router.route_document(aggregated, &endpoint).await;
            }
        }
    }
}
```

**Complexity**: Moderate-High (~600 LOC)  
**Timeline**: 1.5 weeks  
**Testing**: Containerlab can measure bandwidth reduction from aggregation

---

## Testing Strategy in Containerlab

### Test Infrastructure Setup

```yaml
# containerlab/topology.yaml

name: cap-mesh-test

topology:
  nodes:
    # Company HQ
    hq:
      kind: linux
      image: cap-node:latest
      env:
        NODE_ID: company_hq
        HIERARCHY_LEVEL: Company
        POSITION: "37.7749,-122.4194,100"
      
    # Platoon Leaders (2)
    platoon-1:
      kind: linux
      image: cap-node:latest
      env:
        NODE_ID: platoon_1
        HIERARCHY_LEVEL: Platoon
        POSITION: "37.7800,-122.4100,80"
    
    platoon-2:
      kind: linux
      image: cap-node:latest
      env:
        NODE_ID: platoon_2
        HIERARCHY_LEVEL: Platoon
        POSITION: "37.7700,-122.4300,75"
    
    # Squad Leaders (4, 2 per platoon)
    squad-1-1:
      kind: linux
      image: cap-node:latest
      env:
        NODE_ID: squad_1_1
        HIERARCHY_LEVEL: Squad
        POSITION: "37.7820,-122.4080,70"
    
    # ... more squads
    
    # Platforms (16, 4 per squad)
    platform-1-1-1:
      kind: linux
      image: cap-node:latest
      env:
        NODE_ID: platform_1_1_1
        HIERARCHY_LEVEL: Platform
        POSITION: "37.7825,-122.4075,60"
    
    # ... more platforms (scale to 50-200 total nodes)

  links:
    # Network topology simulating tactical radio mesh
    - endpoints: ["hq:eth1", "platoon-1:eth1"]
      mtu: 1500
    - endpoints: ["hq:eth1", "platoon-2:eth1"]
    - endpoints: ["platoon-1:eth1", "squad-1-1:eth1"]
    # ... more links
```

### Network Conditions Simulation

```bash
# Simulate MANET characteristics on specific links
sudo tc qdisc add dev eth1 root netem delay 50ms 20ms loss 10% corrupt 2%

# Simulate Starlink
sudo tc qdisc add dev eth2 root netem delay 600ms 100ms loss 1%

# Simulate tactical radio (high loss, variable latency)
sudo tc qdisc add dev eth3 root netem delay 500ms 1000ms loss 25% corrupt 5%
```

### Validation Test Suite

#### Test 1: Discovery Validation (10 nodes)

**Objective**: Verify all three discovery mechanisms work

```rust
#[tokio::test]
async fn test_mdns_discovery_in_containers() {
    // Launch 10 containers on same bridge network
    let nodes = launch_containerlab_topology("test-mdns-10node.yaml").await;
    
    // Wait for discovery
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify each node discovered all others
    for node in &nodes {
        let discovered = node.get_discovered_peers().await;
        assert_eq!(discovered.len(), 9, "Node {} didn't discover all peers", node.id);
    }
}

#[tokio::test]
async fn test_static_config_discovery() {
    // Use static config instead of mDNS
    let nodes = launch_with_static_config("test-static-10node.yaml").await;
    
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    for node in &nodes {
        let discovered = node.get_discovered_peers().await;
        assert_eq!(discovered.len(), 9);
    }
}
```

#### Test 2: Beacon Broadcasting (50 nodes)

**Objective**: Verify beacons replicate correctly via Automerge+Iroh

```rust
#[tokio::test]
async fn test_beacon_replication() {
    let nodes = launch_containerlab_topology("test-beacon-50node.yaml").await;
    
    // Wait for beacons to propagate
    tokio::time::sleep(Duration::from_secs(10)).await;
    
    // Each node should see beacons from geographically nearby nodes
    for node in &nodes {
        let nearby_beacons = node.get_nearby_beacons().await;
        
        // Verify beacon contains expected fields
        for beacon in nearby_beacons {
            assert!(!beacon.node_id.is_empty());
            assert!(beacon.timestamp > 0);
            assert!(beacon.geohash.len() == 7);
        }
    }
}
```

#### Test 3: Hierarchical Topology Formation (100 nodes)

**Objective**: Verify parent/child relationships form correctly

```rust
#[tokio::test]
async fn test_hierarchy_formation() {
    // Launch 100 nodes: 1 HQ, 2 Platoons, 8 Squads, 89 Platforms
    let nodes = launch_containerlab_topology("test-hierarchy-100node.yaml").await;
    
    // Wait for topology to stabilize
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Verify HQ has 2 children (platoons)
    let hq = nodes.get_by_id("company_hq");
    assert_eq!(hq.get_children().await.len(), 2);
    
    // Verify each platoon has 4 children (squads)
    for platoon_id in &["platoon_1", "platoon_2"] {
        let platoon = nodes.get_by_id(platoon_id);
        assert_eq!(platoon.get_children().await.len(), 4);
    }
    
    // Verify each squad has ~11 children (platforms)
    // (89 platforms / 8 squads ≈ 11 per squad)
    for squad_id in 1..=8 {
        let squad = nodes.get_by_id(&format!("squad_{}", squad_id));
        let children = squad.get_children().await;
        assert!(children.len() >= 10 && children.len() <= 12);
    }
    
    // Verify all platforms have a parent (squad leader)
    for node in nodes.filter_by_level(HierarchyLevel::Platform) {
        assert!(node.get_parent().await.is_some());
    }
}
```

#### Test 4: Data Routing and Aggregation (100 nodes)

**Objective**: Verify data flows correctly through hierarchy

```rust
#[tokio::test]
async fn test_upward_data_flow() {
    let nodes = launch_containerlab_topology("test-routing-100node.yaml").await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Platform sends telemetry update
    let platform = nodes.get_by_id("platform_1_1_1");
    platform.send_telemetry(json!({
        "position": {"lat": 37.7825, "lon": -122.4075},
        "status": "operational"
    })).await;
    
    // Wait for aggregation and routing
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify squad leader received it
    let squad = nodes.get_by_id("squad_1_1");
    let received = squad.get_received_documents().await;
    assert!(received.iter().any(|d| d.source == "platform_1_1_1"));
    
    // Verify platoon received aggregated update (not raw platform data)
    let platoon = nodes.get_by_id("platoon_1");
    let received = platoon.get_received_documents().await;
    let squad_summary = received.iter()
        .find(|d| d.source == "squad_1_1" && d.id.contains("summary"));
    assert!(squad_summary.is_some());
    
    // Verify HQ received platoon aggregate (not squad or platform data)
    let hq = nodes.get_by_id("company_hq");
    let received = hq.get_received_documents().await;
    assert!(received.iter().any(|d| d.source == "platoon_1"));
    assert!(!received.iter().any(|d| d.source.contains("platform")));
}
```

#### Test 5: Mesh Healing and Failover (50 nodes)

**Objective**: Verify topology recovers from failures

```rust
#[tokio::test]
async fn test_parent_failover() {
    let nodes = launch_containerlab_topology("test-failover-50node.yaml").await;
    tokio::time::sleep(Duration::from_secs(20)).await;
    
    // Kill a squad leader
    let squad_leader_id = "squad_1_1";
    kill_container(squad_leader_id).await;
    
    // Wait for detection and recovery
    tokio::time::sleep(Duration::from_secs(15)).await;
    
    // Verify all platforms that were children found new parents
    let orphaned_platforms = nodes.filter_by_former_parent(squad_leader_id);
    for platform in orphaned_platforms {
        let new_parent = platform.get_parent().await;
        assert!(new_parent.is_some());
        assert_ne!(new_parent.unwrap(), squad_leader_id);
    }
}
```

#### Test 6: Bandwidth Measurement (100 nodes)

**Objective**: Validate O(n log n) bandwidth claim

```rust
#[tokio::test]
async fn test_bandwidth_scaling() {
    // Test at multiple scales
    let node_counts = vec![25, 50, 100, 200];
    let mut results = Vec::new();
    
    for n in node_counts {
        let nodes = launch_containerlab_topology(
            &format!("test-bandwidth-{}node.yaml", n)
        ).await;
        
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        // Measure total bandwidth for 1 minute
        let start = Instant::now();
        tokio::time::sleep(Duration::from_secs(60)).await;
        let duration = start.elapsed();
        
        // Collect bandwidth from all interfaces
        let total_bytes = nodes.iter()
            .map(|n| n.get_interface_bytes().await)
            .sum::<u64>();
        
        let bandwidth_mbps = (total_bytes * 8) as f64 / duration.as_secs_f64() / 1_000_000.0;
        
        results.push((n, bandwidth_mbps));
    }
    
    // Verify O(n log n) scaling
    // bandwidth should grow slower than O(n²)
    // For n=100 vs n=25: bandwidth ratio should be < (100/25)² = 16
    let ratio_25_to_100 = results[2].1 / results[0].1;
    assert!(ratio_25_to_100 < 10.0, 
        "Bandwidth scaling worse than O(n log n): ratio = {}", ratio_25_to_100);
}
```

#### Test 7: Network Partition Resilience (50 nodes)

**Objective**: Verify autonomous operation during partition

```rust
#[tokio::test]
async fn test_network_partition() {
    let nodes = launch_containerlab_topology("test-partition-50node.yaml").await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    // Partition network: cut all links to HQ and Platoon 1
    partition_network(&["hq", "platoon_1"], &["platoon_2"]).await;
    
    // Wait for partition detection
    tokio::time::sleep(Duration::from_secs(10)).await;
    
    // Verify Platoon 2 side continues operating autonomously
    let squad_2_1 = nodes.get_by_id("squad_2_1");
    let platforms = squad_2_1.get_children().await;
    assert!(!platforms.is_empty());
    
    // Send data from platform in partitioned side
    let platform = nodes.get_by_id("platform_2_1_1");
    platform.send_telemetry(json!({"status": "operational"})).await;
    
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify data reached squad leader despite partition
    let received = squad_2_1.get_received_documents().await;
    assert!(received.iter().any(|d| d.source == "platform_2_1_1"));
    
    // Heal partition
    heal_network_partition().await;
    
    // Wait for reconnection
    tokio::time::sleep(Duration::from_secs(15)).await;
    
    // Verify HQ eventually receives data from partitioned side
    let hq = nodes.get_by_id("company_hq");
    let received = hq.get_received_documents().await;
    assert!(received.iter().any(|d| d.metadata.source_node.contains("platoon_2")));
}
```

---

## Implementation Timeline

### Phase 1: Discovery Layer (Weeks 1-2)
**Goal**: Multiple discovery strategies working

- [ ] Week 1: mDNS Discovery + Static Config
  - [ ] DiscoveryStrategy trait
  - [ ] MdnsDiscovery implementation
  - [ ] StaticDiscovery implementation
  - [ ] HybridDiscovery manager
  - [ ] Unit tests
  
- [ ] Week 2: Integration and Validation
  - [ ] Containerlab test topology (10 nodes)
  - [ ] Discovery validation tests
  - [ ] Documentation

**Milestone**: 10 containerized nodes discover each other via mDNS

### Phase 2: Beacon System (Weeks 3-4)
**Goal**: Geographic presence broadcasting working

- [ ] Week 3: Beacon Broadcasting
  - [ ] GeographicBeacon struct
  - [ ] BeaconBroadcaster implementation
  - [ ] Automerge integration
  - [ ] Geohash encoding
  
- [ ] Week 4: Beacon Observation
  - [ ] BeaconObserver implementation
  - [ ] TTL and expiration
  - [ ] BeaconJanitor cleanup
  - [ ] Integration tests (50 nodes)

**Milestone**: 50 nodes broadcasting beacons, visible to nearby peers

### Phase 3: Topology Management (Weeks 5-7)
**Goal**: Hierarchical connections forming automatically

- [ ] Week 5: Basic Topology
  - [ ] MeshTopology struct
  - [ ] TopologyManager implementation
  - [ ] Parent selection algorithm
  - [ ] Connection establishment
  
- [ ] Week 6: Connection Maintenance
  - [ ] Heartbeat mechanism
  - [ ] Dead connection detection
  - [ ] Connection pruning
  - [ ] Peer discovery integration
  
- [ ] Week 7: Testing and Validation
  - [ ] Hierarchy formation tests (100 nodes)
  - [ ] Parent/child verification
  - [ ] Connection stability tests

**Milestone**: 100 nodes form 4-level hierarchy automatically

### Phase 4: Data Routing (Weeks 8-10)
**Goal**: Data flows correctly through hierarchy

- [ ] Week 8: Selective Router
  - [ ] SelectiveRouter implementation
  - [ ] should_consume logic
  - [ ] should_forward logic
  - [ ] next_hop routing
  
- [ ] Week 9: Aggregation
  - [ ] HierarchicalAggregator implementation
  - [ ] Telemetry aggregation
  - [ ] Status summarization
  - [ ] Aggregation timing
  
- [ ] Week 10: Integration Testing
  - [ ] Upward data flow tests
  - [ ] Downward command tests
  - [ ] Bandwidth measurement
  - [ ] Aggregation validation

**Milestone**: Data flows up hierarchy with aggregation, bandwidth < O(n log n)

### Phase 5: Mesh Healing (Weeks 11-12)
**Goal**: Topology recovers from failures

- [ ] Week 11: Failover Logic
  - [ ] Parent failure detection
  - [ ] Alternative parent search
  - [ ] Graceful reconnection
  - [ ] State synchronization
  
- [ ] Week 12: Testing and Optimization
  - [ ] Parent failover tests
  - [ ] Network partition tests
  - [ ] Recovery time measurement
  - [ ] Edge case handling

**Milestone**: Mesh recovers from node failures within 10 seconds

---

## Success Criteria

### Functional Requirements

**Discovery** (Phase 1):
- [ ] mDNS discovers all peers on same network within 5 seconds
- [ ] Static config loads peers successfully
- [ ] Hybrid discovery combines multiple strategies
- [ ] Discovery events trigger correctly

**Beacons** (Phase 2):
- [ ] All nodes broadcast beacons every 5 seconds
- [ ] Beacons replicate to nearby nodes via Automerge+Iroh
- [ ] Expired beacons removed within TTL window (30s)
- [ ] Geohash filtering reduces beacon propagation

**Topology** (Phase 3):
- [ ] Platforms automatically connect to nearest squad leader
- [ ] Squad leaders connect to platoon leaders
- [ ] Platoon leaders connect to company HQ
- [ ] Connections maintained with heartbeats

**Routing** (Phase 4):
- [ ] Platform data reaches HQ through aggregation
- [ ] Commands flow downward to correct nodes
- [ ] Selective consumption prevents data flooding
- [ ] Aggregation reduces bandwidth at each level

**Healing** (Phase 5):
- [ ] Parent failure detected within 10 seconds
- [ ] New parent found and connected within 10 seconds
- [ ] Network partition isolated correctly
- [ ] Partition healing restores connectivity

### Performance Requirements

**Bandwidth** (validated in Containerlab):
- [ ] 100 nodes: < 10 Mbps total aggregate
- [ ] 200 nodes: < 25 Mbps total aggregate
- [ ] Bandwidth growth: O(n log n) or better
- [ ] Aggregation reduces upstream bandwidth by 80%+

**Latency**:
- [ ] Platform → Squad: < 100ms
- [ ] Squad → Platoon: < 200ms
- [ ] Platoon → Company: < 300ms
- [ ] End-to-end: < 600ms (acceptable for tactical coordination)

**Scalability** (extrapolated from empirical):
- [ ] 50 nodes: Measured in Containerlab
- [ ] 100 nodes: Measured in Containerlab
- [ ] 200 nodes: Measured in Containerlab or cloud
- [ ] 1000 nodes: Mathematical model validated against 200-node data

**Reliability**:
- [ ] Parent failover: < 10s recovery
- [ ] Network partition: Autonomous operation maintained
- [ ] Partition healing: < 30s full synchronization
- [ ] Zero data loss during failover (Automerge guarantees)

---

## Risks and Mitigations

### Risk 1: Containerlab Resource Limits

**Risk**: Cannot scale beyond ~200 nodes on single machine

**Mitigation**:
1. Use cloud deployment for 200+ node tests
2. Mathematical modeling validated against 100-200 node empirical data
3. Focus on proving O(n log n) scaling property, not absolute node count
4. Demonstrate bandwidth reduction from aggregation at achievable scale

### Risk 2: mDNS Reliability in Containers

**Risk**: mDNS may not work reliably in all container networking modes

**Mitigation**:
1. Test both bridge and host networking modes
2. Static configuration provides fallback
3. Document known limitations
4. Relay-based discovery works in all scenarios

### Risk 3: Hierarchy Formation Complexity

**Risk**: Parent selection algorithm may create unstable topologies

**Mitigation**:
1. Implement hysteresis in parent switching (prevent flapping)
2. Use connection quality metrics in addition to proximity
3. Extensive simulation testing before Containerlab validation
4. Manual override capability for testing

### Risk 4: Beacon Flooding

**Risk**: Beacon replication could consume excessive bandwidth

**Mitigation**:
1. Geohash filtering limits propagation
2. TTL prevents accumulation
3. Broadcast interval tunable (default 5s)
4. Monitor bandwidth in Containerlab tests

---

## Alternatives Considered

### Alternative 1: Use Libp2p for P2P Mesh

**Approach**: Use libp2p (Rust) for discovery, routing, and mesh management

**Pros**:
- ✅ Battle-tested P2P library
- ✅ Built-in discovery (mDNS, DHT, etc.)
- ✅ Pubsub for data distribution
- ✅ Large community

**Cons**:
- ❌ No native CRDT support (would need Automerge integration)
- ❌ No QUIC multipath (uses standard QUIC)
- ❌ Not optimized for hierarchical military structures
- ❌ Complex configuration
- ❌ ~8,000 LOC to integrate

**Verdict**: Rejected - Iroh provides better tactical networking, custom mesh logic is simpler

### Alternative 2: Full Custom Implementation

**Approach**: Build everything from scratch including QUIC

**Pros**:
- ✅ Maximum control and optimization
- ✅ No external dependencies

**Cons**:
- ❌ 20,000+ LOC required
- ❌ Years of development time
- ❌ Reinventing well-solved problems
- ❌ Maintenance burden

**Verdict**: Rejected - Not feasible within timeline

### Alternative 3: Use DHT for Discovery

**Approach**: Distributed hash table for peer discovery instead of mDNS/static

**Pros**:
- ✅ Scales to very large networks
- ✅ No central coordination needed

**Cons**:
- ❌ Overkill for tactical networks (< 1000 nodes)
- ❌ Poor EMCON compatibility (requires broadcast)
- ❌ Adds latency to discovery
- ❌ Complex implementation

**Verdict**: Rejected for initial version - Can add later if needed

---

## Open Questions

1. **Geohash precision tradeoff**: 7-char (~153m) vs 6-char (~1.2km) for beacon clustering?
   - Test both in Containerlab to measure bandwidth vs discovery latency

2. **Aggregation intervals**: How often should squad/platoon leaders aggregate?
   - Start with 5s, measure latency vs bandwidth tradeoff

3. **Parent selection stability**: How to prevent parent switching oscillation?
   - Implement hysteresis (require 20% better metric to switch)

4. **Maximum children per node**: Should squad leaders limit child connections?
   - Start unlimited, add limits if connection management becomes issue

5. **Heartbeat interval**: How frequently to check parent connection health?
   - Start with 10s, adjust based on failover recovery time requirements

---

## References

1. [Iroh Documentation](https://iroh.computer/docs)
2. [Automerge Documentation](https://automerge.org/docs)
3. [Containerlab Documentation](https://containerlab.dev)
4. [mdns-sd Crate](https://crates.io/crates/mdns-sd)
5. [Geohash Algorithm](https://en.wikipedia.org/wiki/Geohash)
6. ADR-001: CAP Protocol POC Architecture
7. ADR-002: Beacon Storage Architecture
8. ADR-011: CRDT + Networking Stack Selection
9. ADR-013: Distributed Software & AI Operations

---

## Appendix A: Containerlab Test Commands

```bash
# Deploy topology
sudo containerlab deploy -t topology.yaml

# Inspect node
sudo docker exec -it clab-cap-mesh-test-platform-1-1-1 bash

# Monitor traffic on interface
sudo tcpdump -i eth1 -w capture.pcap

# Measure bandwidth
sudo iftop -i br-cap-mesh

# Simulate network partition
sudo iptables -A FORWARD -s 172.20.1.0/24 -d 172.20.2.0/24 -j DROP

# Add latency/loss to interface
sudo tc qdisc add dev eth1 root netem delay 100ms loss 5%

# Destroy topology
sudo containerlab destroy -t topology.yaml
```

---

## Appendix B: Monitoring and Observability

### Metrics to Collect

```rust
// Prometheus metrics
pub struct MeshMetrics {
    // Discovery
    pub discovered_peers: IntGauge,
    pub discovery_events: IntCounter,
    
    // Topology
    pub parent_changes: IntCounter,
    pub child_connections: IntGauge,
    pub connection_failures: IntCounter,
    
    // Routing
    pub documents_consumed: IntCounter,
    pub documents_forwarded: IntCounter,
    pub documents_aggregated: IntCounter,
    
    // Bandwidth
    pub bytes_sent: IntCounter,
    pub bytes_received: IntCounter,
    pub bandwidth_mbps: Gauge,
    
    // Latency
    pub routing_latency_ms: Histogram,
    pub aggregation_latency_ms: Histogram,
}
```

### Grafana Dashboard

- Discovery events timeline
- Topology graph visualization
- Bandwidth usage per node/level
- Latency heatmap (platform → HQ)
- Failover recovery times
- Aggregation ratio (input bytes / output bytes)

---

**Last Updated**: 2025-11-14  
**Next Review**: After Phase 1 completion (Week 2)  
**Decision Status**: Proposed - Pending team review
