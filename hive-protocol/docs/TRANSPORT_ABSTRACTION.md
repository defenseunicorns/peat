# Transport Abstraction for Mesh Topology

**Status**: Design Document
**Created**: 2025-11-21
**Related**: Issue #122 (Phase 3 Week 5), EPIC 2, ADR-011 (Iroh), ADR-017 (Mesh Intelligence)

## Overview

This document defines a backend-agnostic transport abstraction layer for mesh topology formation. The abstraction enables `TopologyManager` and related components to establish P2P connections without coupling to specific networking backends (Iroh vs Ditto).

## Problem Statement

**Current State**:
- **AutomergeBackend** (with Iroh): Requires explicit `IrohTransport`, `Endpoint`, and `Connection` management
- **DittoBackend**: Has built-in peer discovery and transport handling via Ditto SDK

**Challenge**: `TopologyManager` needs to establish parent-child connections for hierarchy formation, but the topology layer (hive-mesh) must remain backend-agnostic.

**Goal**: Create a clean abstraction that:
1. Provides uniform connection API for topology formation
2. Delegates to backend-specific capabilities
3. Supports both Iroh (explicit) and Ditto (implicit) transports
4. Follows Ports & Adapters pattern (like `BeaconStorage`)

## Architecture

### Layer Model

```
┌─────────────────────────────────────────────────────────┐
│ Layer 3: Topology Formation (hive-mesh)                │
│ - TopologyBuilder                                       │
│ - TopologyManager                                       │
│ - Parent selection logic                                │
└──────────────────┬──────────────────────────────────────┘
                   │ Uses MeshTransport trait
                   ▼
┌─────────────────────────────────────────────────────────┐
│ Layer 2: Transport Abstraction (hive-protocol)        │
│ - MeshTransport trait                                   │
│ - MeshConnection trait                                  │
│ - Backend-agnostic connection API                       │
└──────────────────┬──────────────────────────────────────┘
                   │ Implemented by adapters
                   ▼
┌─────────────────────────────────────────────────────────┐
│ Layer 1: Backend Adapters (hive-protocol)             │
│ - IrohMeshTransport (wraps IrohTransport)             │
│ - DittoMeshTransport (wraps DittoBackend)             │
└──────────────────┬──────────────────────────────────────┘
                   │ Delegates to
                   ▼
┌─────────────────────────────────────────────────────────┐
│ Layer 0: Backend Implementation                         │
│ - Iroh: Endpoint, Connection, QUIC streams              │
│ - Ditto: Built-in P2P transport                         │
└─────────────────────────────────────────────────────────┘
```

### Core Traits

#### 1. MeshTransport Trait

Defines the connection establishment and management API.

```rust
use async_trait::async_trait;
use std::error::Error as StdError;
use std::fmt;

/// Node identifier in the mesh network
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error type for mesh transport operations
#[derive(Debug)]
pub enum TransportError {
    /// Connection failed to establish
    ConnectionFailed(String),

    /// Peer not found or unreachable
    PeerNotFound(String),

    /// Connection already exists
    AlreadyConnected(String),

    /// Transport not started
    NotStarted,

    /// Generic transport error
    Other(Box<dyn StdError + Send + Sync>),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            TransportError::PeerNotFound(msg) => write!(f, "Peer not found: {}", msg),
            TransportError::AlreadyConnected(msg) => write!(f, "Already connected: {}", msg),
            TransportError::NotStarted => write!(f, "Transport not started"),
            TransportError::Other(err) => write!(f, "Transport error: {}", err),
        }
    }
}

impl StdError for TransportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TransportError::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, TransportError>;

/// Transport abstraction for mesh topology connections
///
/// This trait defines the connection management operations needed by
/// TopologyManager to establish parent-child relationships in the mesh.
///
/// # Design Principles
///
/// - **Backend Agnostic**: No direct dependency on Iroh or Ditto
/// - **Delegation**: Each implementation delegates to its backend's capabilities
/// - **Async**: All operations are async for non-blocking I/O
/// - **Lifecycle Management**: Explicit start/stop for connection handling
///
/// # Implementations
///
/// - **IrohMeshTransport**: Uses `IrohTransport` with explicit connections
/// - **DittoMeshTransport**: Delegates to Ditto's built-in transport
#[async_trait]
pub trait MeshTransport: Send + Sync {
    /// Start the transport layer
    ///
    /// For Iroh: Starts accept loop to receive incoming connections
    /// For Ditto: No-op (Ditto handles this internally)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transport started successfully
    /// * `Err(TransportError)` - Start operation failed
    async fn start(&self) -> Result<()>;

    /// Stop the transport layer
    ///
    /// For Iroh: Stops accept loop and closes connections
    /// For Ditto: No-op (Ditto manages lifecycle)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Transport stopped successfully
    /// * `Err(TransportError)` - Stop operation failed
    async fn stop(&self) -> Result<()>;

    /// Connect to a peer by node ID
    ///
    /// Establishes a connection to the specified peer. The connection
    /// mechanism is backend-specific:
    ///
    /// - **Iroh**: Uses discovery (static config, mDNS) to resolve NodeId → EndpointAddr,
    ///   then establishes QUIC connection
    /// - **Ditto**: Delegates to Ditto's peer discovery and connection handling
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The node ID of the peer to connect to
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn MeshConnection>)` - Connection established
    /// * `Err(TransportError)` - Connection failed
    ///
    /// # Implementation Notes
    ///
    /// - Should be idempotent: connecting to an already-connected peer returns existing connection
    /// - Should handle peer discovery automatically (using backend-specific mechanisms)
    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>>;

    /// Disconnect from a peer
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The node ID of the peer to disconnect from
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Peer disconnected successfully
    /// * `Err(TransportError)` - Disconnect operation failed
    async fn disconnect(&self, peer_id: &NodeId) -> Result<()>;

    /// Get an existing connection to a peer
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The node ID of the peer
    ///
    /// # Returns
    ///
    /// * `Some(Box<dyn MeshConnection>)` - Connection exists
    /// * `None` - No connection to this peer
    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>>;

    /// Get the number of connected peers
    fn peer_count(&self) -> usize;

    /// Get list of connected peer IDs
    fn connected_peers(&self) -> Vec<NodeId>;

    /// Check if connected to a specific peer
    fn is_connected(&self, peer_id: &NodeId) -> bool {
        self.get_connection(peer_id).is_some()
    }
}
```

#### 2. MeshConnection Trait

Represents an active connection to a peer.

```rust
/// Active connection to a mesh peer
///
/// This trait abstracts over backend-specific connection types:
/// - Iroh: `iroh::endpoint::Connection`
/// - Ditto: Virtual connection (peer reachable via Ditto)
///
/// # Design Note
///
/// Initially minimal - just peer identification. Stream operations
/// will be added when needed for data exchange beyond CRDT sync.
pub trait MeshConnection: Send + Sync {
    /// Get the remote peer's node ID
    fn peer_id(&self) -> &NodeId;

    /// Check if connection is still alive
    ///
    /// For Iroh: Checks QUIC connection status
    /// For Ditto: Always returns true (Ditto handles failures internally)
    fn is_alive(&self) -> bool;
}
```

## Backend Implementations

### 1. IrohMeshTransport

**Responsibilities**:
- Wraps `IrohTransport` with `MeshTransport` interface
- Integrates with peer discovery (static config, future: mDNS)
- Manages NodeId ↔ EndpointId mapping
- Delegates to `IrohTransport` for actual QUIC connections

**Implementation Strategy**:

```rust
use crate::network::iroh_transport::IrohTransport;
use crate::network::peer_config::PeerConfig;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use iroh::{EndpointId, endpoint::Connection};

pub struct IrohMeshTransport {
    /// Underlying Iroh transport
    transport: Arc<IrohTransport>,

    /// Static peer configuration (for discovery)
    peer_config: Arc<RwLock<PeerConfig>>,

    /// NodeId → EndpointId mapping (for discovery)
    node_to_endpoint: Arc<RwLock<HashMap<NodeId, EndpointId>>>,

    /// EndpointId → NodeId mapping (for incoming connections)
    endpoint_to_node: Arc<RwLock<HashMap<EndpointId, NodeId>>>,

    /// Connections by NodeId
    connections: Arc<RwLock<HashMap<NodeId, Arc<IrohMeshConnection>>>>,
}

impl IrohMeshTransport {
    /// Create a new Iroh mesh transport
    ///
    /// # Arguments
    ///
    /// * `transport` - Underlying IrohTransport
    /// * `peer_config` - Static peer configuration for discovery
    pub fn new(transport: Arc<IrohTransport>, peer_config: PeerConfig) -> Self {
        Self {
            transport,
            peer_config: Arc::new(RwLock::new(peer_config)),
            node_to_endpoint: Arc::new(RwLock::new(HashMap::new())),
            endpoint_to_node: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a peer (NodeId → EndpointId mapping)
    ///
    /// This is called during discovery to map node IDs to Iroh endpoint IDs.
    /// Used by both static config and future mDNS discovery.
    pub fn register_peer(&self, node_id: NodeId, endpoint_id: EndpointId) {
        self.node_to_endpoint.write().unwrap().insert(node_id.clone(), endpoint_id);
        self.endpoint_to_node.write().unwrap().insert(endpoint_id, node_id);
    }
}

#[async_trait]
impl MeshTransport for IrohMeshTransport {
    async fn start(&self) -> Result<()> {
        // Start Iroh accept loop
        self.transport
            .start_accept_loop()
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        // Load static peer config and register peers
        let config = self.peer_config.read().unwrap();
        for (node_id_str, peer_info) in &config.peers {
            let node_id = NodeId::new(node_id_str.clone());
            if let Ok(endpoint_id) = peer_info.endpoint_id() {
                self.register_peer(node_id, endpoint_id);
            }
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop accept loop
        self.transport
            .stop_accept_loop()
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        // Close all connections
        let connections = self.connections.write().unwrap().drain().collect::<Vec<_>>();
        for (_node_id, _conn) in connections {
            // Connections will be closed when dropped
        }

        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        // Check if already connected
        if let Some(conn) = self.get_connection(peer_id) {
            return Ok(conn);
        }

        // Resolve NodeId → EndpointId
        let endpoint_id = self
            .node_to_endpoint
            .read()
            .unwrap()
            .get(peer_id)
            .copied()
            .ok_or_else(|| TransportError::PeerNotFound(peer_id.as_str().to_string()))?;

        // Get peer info from static config
        let peer_info = self
            .peer_config
            .read()
            .unwrap()
            .get_peer(peer_id.as_str())
            .cloned()
            .ok_or_else(|| TransportError::PeerNotFound(peer_id.as_str().to_string()))?;

        // Connect using IrohTransport
        let conn = self
            .transport
            .connect_peer(&peer_info)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        // Wrap in MeshConnection
        let mesh_conn = Arc::new(IrohMeshConnection::new(peer_id.clone(), conn));

        // Store connection
        self.connections
            .write()
            .unwrap()
            .insert(peer_id.clone(), mesh_conn.clone());

        Ok(Box::new(mesh_conn))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        // Remove connection from map
        if let Some(_conn) = self.connections.write().unwrap().remove(peer_id) {
            // Connection will be closed when dropped
        }
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        self.connections
            .read()
            .unwrap()
            .get(peer_id)
            .map(|c| Box::new(c.clone()) as Box<dyn MeshConnection>)
    }

    fn peer_count(&self) -> usize {
        self.connections.read().unwrap().len()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.connections.read().unwrap().keys().cloned().collect()
    }
}

pub struct IrohMeshConnection {
    peer_id: NodeId,
    connection: Connection,
}

impl IrohMeshConnection {
    fn new(peer_id: NodeId, connection: Connection) -> Self {
        Self { peer_id, connection }
    }
}

impl MeshConnection for IrohMeshConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Check QUIC connection status
        // Note: Iroh Connection doesn't expose is_closed(), so we'd need to
        // attempt a stream or check via other means
        true // Simplified for now
    }
}
```

### 2. DittoMeshTransport

**Responsibilities**:
- Wraps `DittoBackend` with `MeshTransport` interface
- Delegates all operations to Ditto's built-in transport
- Connection management is implicit (Ditto handles it)

**Implementation Strategy**:

```rust
use crate::sync::ditto::DittoBackend;
use std::sync::Arc;

pub struct DittoMeshTransport {
    /// Underlying Ditto backend
    backend: Arc<DittoBackend>,
}

impl DittoMeshTransport {
    pub fn new(backend: Arc<DittoBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl MeshTransport for DittoMeshTransport {
    async fn start(&self) -> Result<()> {
        // No-op: Ditto manages its own transport lifecycle
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // No-op: Ditto manages its own transport lifecycle
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        // Ditto handles connections implicitly via peer discovery
        // Just return a virtual connection representing reachability
        Ok(Box::new(DittoMeshConnection::new(peer_id.clone())))
    }

    async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
        // No-op: Ditto manages connections internally
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        // Check if peer is reachable via Ditto
        // This would require querying Ditto's peer list
        // For now, simplified:
        Some(Box::new(DittoMeshConnection::new(peer_id.clone())))
    }

    fn peer_count(&self) -> usize {
        // Query Ditto for connected peer count
        // Simplified for now
        0
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        // Query Ditto for connected peers
        // Simplified for now
        vec![]
    }
}

pub struct DittoMeshConnection {
    peer_id: NodeId,
}

impl DittoMeshConnection {
    fn new(peer_id: NodeId) -> Self {
        Self { peer_id }
    }
}

impl MeshConnection for DittoMeshConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Ditto manages connection state internally
        true
    }
}
```

## Integration with TopologyManager

### Updated Architecture (from ADR-017 Section 2.2)

```rust
use hive_mesh::topology::{TopologyBuilder, TopologyEvent};
use crate::transport::{MeshTransport, NodeId};
use std::sync::Arc;

pub struct TopologyManager {
    /// Topology builder for parent selection
    builder: TopologyBuilder,

    /// Transport abstraction for connections
    transport: Arc<dyn MeshTransport>,

    /// Current parent connection (if any)
    parent_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,
}

impl TopologyManager {
    pub fn new(
        builder: TopologyBuilder,
        transport: Arc<dyn MeshTransport>,
    ) -> Self {
        Self {
            builder,
            transport,
            parent_connection: Arc::new(RwLock::new(None)),
        }
    }

    /// Start topology management
    pub async fn start(&self) -> Result<()> {
        // Start transport
        self.transport.start().await?;

        // Start topology builder
        self.builder.start().await;

        // Subscribe to topology events
        if let Some(mut rx) = self.builder.subscribe() {
            let transport = self.transport.clone();
            let parent_connection = self.parent_connection.clone();

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        TopologyEvent::ParentSelected { parent_id, .. } => {
                            // Connect to selected parent
                            let node_id = NodeId::new(parent_id.clone());
                            if let Ok(conn) = transport.connect(&node_id).await {
                                *parent_connection.write().unwrap() = Some(conn);
                                tracing::info!("Connected to parent: {}", parent_id);
                            }
                        }
                        TopologyEvent::ParentChanged { old_parent_id, new_parent_id, .. } => {
                            // Disconnect from old parent
                            let old_id = NodeId::new(old_parent_id);
                            let _ = transport.disconnect(&old_id).await;

                            // Connect to new parent
                            let new_id = NodeId::new(new_parent_id.clone());
                            if let Ok(conn) = transport.connect(&new_id).await {
                                *parent_connection.write().unwrap() = Some(conn);
                                tracing::info!("Re-parented to: {}", new_parent_id);
                            }
                        }
                        TopologyEvent::ParentLost { parent_id } => {
                            // Clear parent connection
                            *parent_connection.write().unwrap() = None;
                            let node_id = NodeId::new(parent_id);
                            let _ = transport.disconnect(&node_id).await;
                        }
                        _ => {}
                    }
                }
            });
        }

        Ok(())
    }

    /// Stop topology management
    pub async fn stop(&self) -> Result<()> {
        self.builder.stop().await;
        self.transport.stop().await?;
        Ok(())
    }
}
```

## Implementation Plan

### Phase 1: Core Abstraction (Week 5)

1. **Define Traits** (transport/mod.rs):
   - `MeshTransport` trait
   - `MeshConnection` trait
   - `TransportError` enum
   - `NodeId` type

2. **Implement IrohMeshTransport** (transport/iroh.rs):
   - Wrap `IrohTransport`
   - Integrate with `PeerConfig` for discovery
   - NodeId ↔ EndpointId mapping
   - Connection management

3. **Implement DittoMeshTransport** (transport/ditto.rs):
   - Wrap `DittoBackend`
   - No-op for most operations (Ditto handles internally)
   - Virtual connections

4. **Unit Tests**:
   - Test both implementations
   - Mock transport for topology tests

### Phase 2: TopologyManager Integration (Week 5)

1. **Update TopologyManager**:
   - Accept `Arc<dyn MeshTransport>`
   - Handle topology events (parent selected/changed/lost)
   - Establish/tear down connections

2. **Integration Tests**:
   - Test with IrohMeshTransport
   - Test with DittoMeshTransport
   - Verify parent-child connections

### Phase 3: Discovery Integration (Week 6+)

1. **Enhance IrohMeshTransport**:
   - Add mDNS discovery integration
   - Dynamic peer registration
   - Automatic NodeId resolution

2. **Connection Health**:
   - Heartbeat mechanism
   - Dead connection detection
   - Automatic reconnection

## Key Decisions

### 1. Separation of Concerns

**Decision**: Transport abstraction is separate from CRDT sync

**Rationale**:
- CRDT sync (Automerge, Ditto) is data-plane concern
- Connection management is control-plane concern
- Topology formation needs connections, not sync

### 2. Minimal Interface

**Decision**: Start with minimal `MeshConnection` (just peer_id, is_alive)

**Rationale**:
- YAGNI: Don't add stream operations until needed
- Topology formation only needs connection establishment
- Can extend later if data exchange needed beyond CRDT sync

### 3. Backend Delegation

**Decision**: Ditto implementation is mostly no-ops

**Rationale**:
- Ditto SDK handles transport internally
- Forcing explicit connection management breaks Ditto's model
- Virtual connections represent logical reachability

### 4. Discovery Coupling

**Decision**: IrohMeshTransport owns peer discovery integration

**Rationale**:
- NodeId → EndpointAddr resolution is transport-specific
- Static config is simplest (already implemented)
- mDNS can be added later without changing interface

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_iroh_mesh_transport_lifecycle() {
        let iroh_transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::default();
        let mesh_transport = IrohMeshTransport::new(iroh_transport, peer_config);

        // Start
        mesh_transport.start().await.unwrap();
        assert!(mesh_transport.transport.is_accept_loop_running());

        // Stop
        mesh_transport.stop().await.unwrap();
        assert!(!mesh_transport.transport.is_accept_loop_running());
    }

    #[tokio::test]
    async fn test_iroh_mesh_connect_disconnect() {
        // Create two transports
        let transport1 = create_test_transport("node-1", 9001).await;
        let transport2 = create_test_transport("node-2", 9002).await;

        transport1.start().await.unwrap();
        transport2.start().await.unwrap();

        // Connect
        let node2_id = NodeId::new("node-2".to_string());
        let conn = transport1.connect(&node2_id).await.unwrap();
        assert_eq!(conn.peer_id(), &node2_id);
        assert!(transport1.is_connected(&node2_id));

        // Disconnect
        transport1.disconnect(&node2_id).await.unwrap();
        assert!(!transport1.is_connected(&node2_id));
    }

    #[tokio::test]
    async fn test_ditto_mesh_transport_noop() {
        let ditto_backend = Arc::new(DittoBackend::test_instance());
        let mesh_transport = DittoMeshTransport::new(ditto_backend);

        // Start/stop are no-ops
        mesh_transport.start().await.unwrap();
        mesh_transport.stop().await.unwrap();

        // Connect returns virtual connection
        let peer_id = NodeId::new("test-peer".to_string());
        let conn = mesh_transport.connect(&peer_id).await.unwrap();
        assert_eq!(conn.peer_id(), &peer_id);
        assert!(conn.is_alive());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_topology_manager_with_iroh_transport() {
    // Setup: Create 3 nodes (1 platoon, 2 squads)
    let platoon_transport = create_test_transport("platoon-1", 9001).await;
    let squad1_transport = create_test_transport("squad-1", 9002).await;
    let squad2_transport = create_test_transport("squad-2", 9003).await;

    // Create TopologyManagers
    let platoon_mgr = create_topology_manager(
        "platoon-1",
        HierarchyLevel::Platoon,
        platoon_transport,
    ).await;

    let squad1_mgr = create_topology_manager(
        "squad-1",
        HierarchyLevel::Squad,
        squad1_transport,
    ).await;

    // Start all
    platoon_mgr.start().await.unwrap();
    squad1_mgr.start().await.unwrap();

    // Wait for squad to select platoon as parent
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify connection established
    let platoon_id = NodeId::new("platoon-1".to_string());
    assert!(squad1_mgr.is_connected_to_parent(&platoon_id));
}
```

## References

- **ADR-011**: AutomergeIrohBackend architecture
- **ADR-017**: P2P Mesh Intelligence (Section 2.2: TopologyManager)
- **Issue #122**: Phase 3 - Topology Management
- **AUTOMERGE_IROH_PROGRESS.md**: Current Iroh implementation status
- **BeaconStorage trait**: Ports & Adapters pattern precedent (hive-mesh/src/beacon/storage.rs)

## Appendix: File Structure

```
hive-protocol/
├── src/
│   ├── transport/
│   │   ├── mod.rs              # MeshTransport, MeshConnection traits
│   │   ├── iroh.rs             # IrohMeshTransport implementation
│   │   ├── ditto.rs            # DittoMeshTransport implementation
│   │   └── node_id.rs          # NodeId type
│   ├── topology/
│   │   └── manager.rs          # TopologyManager (uses MeshTransport)
│   └── network/
│       └── iroh_transport.rs   # Existing IrohTransport (unchanged)
```
