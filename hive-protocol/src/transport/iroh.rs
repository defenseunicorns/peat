//! Iroh mesh transport implementation
//!
//! This module provides a `MeshTransport` implementation backed by Iroh's QUIC transport.
//! It integrates with static peer configuration for discovery and manages NodeId ↔ EndpointId mapping.

use super::{MeshConnection, MeshTransport, NodeId, Result, TransportError};
use crate::network::iroh_transport::IrohTransport;
use crate::network::peer_config::PeerConfig;
use async_trait::async_trait;
use iroh::endpoint::Connection;
use iroh::EndpointId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Iroh-based mesh transport implementation
///
/// Wraps `IrohTransport` to provide the `MeshTransport` interface with:
/// - NodeId ↔ EndpointId mapping for discovery
/// - Static peer configuration integration
/// - Connection lifecycle management
///
/// # Example
///
/// ```ignore
/// use hive_protocol::transport::iroh::IrohMeshTransport;
/// use hive_protocol::network::peer_config::PeerConfig;
/// use hive_protocol::network::iroh_transport::IrohTransport;
///
/// let iroh_transport = Arc::new(IrohTransport::new().await?);
/// let peer_config = PeerConfig::from_file("peers.toml")?;
/// let mesh_transport = IrohMeshTransport::new(iroh_transport, peer_config);
///
/// mesh_transport.start().await?;
/// ```
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
    connections: Arc<RwLock<HashMap<NodeId, IrohMeshConnection>>>,
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
        self.node_to_endpoint
            .write()
            .unwrap()
            .insert(node_id.clone(), endpoint_id);
        self.endpoint_to_node
            .write()
            .unwrap()
            .insert(endpoint_id, node_id);
    }

    /// Get NodeId from EndpointId (for incoming connections)
    pub fn get_node_id(&self, endpoint_id: &EndpointId) -> Option<NodeId> {
        self.endpoint_to_node
            .read()
            .unwrap()
            .get(endpoint_id)
            .cloned()
    }

    /// Get EndpointId from NodeId (for outgoing connections)
    pub fn get_endpoint_id(&self, node_id: &NodeId) -> Option<EndpointId> {
        self.node_to_endpoint.read().unwrap().get(node_id).copied()
    }

    /// Get the underlying IrohTransport
    pub fn transport(&self) -> &Arc<IrohTransport> {
        &self.transport
    }
}

#[async_trait]
impl MeshTransport for IrohMeshTransport {
    async fn start(&self) -> Result<()> {
        // Start Iroh accept loop
        self.transport
            .start_accept_loop()
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        // Load static peer config and register peers
        let config = self.peer_config.read().unwrap();
        for peer_info in &config.peers {
            let node_id = NodeId::new(peer_info.name.clone());
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
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        // Close all connections
        let connections = self
            .connections
            .write()
            .unwrap()
            .drain()
            .collect::<Vec<_>>();
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
        let _endpoint_id = self
            .node_to_endpoint
            .read()
            .unwrap()
            .get(peer_id)
            .copied()
            .ok_or_else(|| TransportError::PeerNotFound(peer_id.as_str().to_string()))?;

        // Get peer info from static config
        let peer_info = {
            let config = self.peer_config.read().unwrap();
            config
                .peers
                .iter()
                .find(|p| p.name == peer_id.as_str())
                .cloned()
                .ok_or_else(|| TransportError::PeerNotFound(peer_id.as_str().to_string()))?
        };

        // Connect using IrohTransport (Issue #229: returns Option<Connection>)
        let conn_opt = self
            .transport
            .connect_peer(&peer_info)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        match conn_opt {
            Some(conn) => {
                // New connection - wrap in MeshConnection and store
                let mesh_conn = IrohMeshConnection::new(peer_id.clone(), conn);
                self.connections
                    .write()
                    .unwrap()
                    .insert(peer_id.clone(), mesh_conn.clone());
                Ok(Box::new(mesh_conn))
            }
            None => {
                // Already connected (they were initiator) - return existing connection
                self.connections
                    .read()
                    .unwrap()
                    .get(peer_id)
                    .cloned()
                    .map(|c| Box::new(c) as Box<dyn MeshConnection>)
                    .ok_or_else(|| {
                        TransportError::ConnectionFailed(
                            "Connection exists in transport but not in mesh".to_string(),
                        )
                    })
            }
        }
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
            .cloned()
            .map(|c| Box::new(c) as Box<dyn MeshConnection>)
    }

    fn peer_count(&self) -> usize {
        self.connections.read().unwrap().len()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.connections.read().unwrap().keys().cloned().collect()
    }
}

/// Iroh mesh connection implementation
///
/// Wraps an Iroh QUIC connection with the `MeshConnection` interface.
#[derive(Clone)]
pub struct IrohMeshConnection {
    peer_id: NodeId,
    connection: Connection,
}

impl IrohMeshConnection {
    /// Create a new Iroh mesh connection
    pub fn new(peer_id: NodeId, connection: Connection) -> Self {
        Self {
            peer_id,
            connection,
        }
    }

    /// Get the underlying Iroh connection
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

impl MeshConnection for IrohMeshConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Iroh connections don't expose a direct is_closed() method
        // For now, we assume the connection is alive if it exists
        // TODO: Implement proper liveness check when Iroh provides API
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::peer_config::{LocalConfig, PeerInfo};
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_iroh_mesh_transport_creation() {
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::empty();
        let mesh_transport = IrohMeshTransport::new(transport, peer_config);

        assert_eq!(mesh_transport.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_peer_registration() {
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::empty();
        let mesh_transport = IrohMeshTransport::new(transport.clone(), peer_config);

        // Register a peer
        let node_id = NodeId::new("test-node".to_string());
        let endpoint_id = transport.endpoint_id();
        mesh_transport.register_peer(node_id.clone(), endpoint_id);

        // Verify mapping
        assert_eq!(mesh_transport.get_endpoint_id(&node_id), Some(endpoint_id));
        assert_eq!(mesh_transport.get_node_id(&endpoint_id), Some(node_id));
    }

    #[tokio::test]
    async fn test_start_stop_lifecycle() {
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::empty();
        let mesh_transport = IrohMeshTransport::new(transport.clone(), peer_config);

        // Start
        mesh_transport.start().await.unwrap();
        assert!(transport.is_accept_loop_running());

        // Stop
        mesh_transport.stop().await.unwrap();
        assert!(!transport.is_accept_loop_running());
    }

    #[tokio::test]
    async fn test_connect_to_unknown_peer() {
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::empty();
        let mesh_transport = IrohMeshTransport::new(transport, peer_config);

        mesh_transport.start().await.unwrap();

        // Try to connect to unknown peer
        let unknown_peer = NodeId::new("unknown".to_string());
        let result = mesh_transport.connect(&unknown_peer).await;

        assert!(result.is_err());
        match result {
            Err(TransportError::PeerNotFound(_)) => {}
            _ => panic!("Expected PeerNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_disconnect() {
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let peer_config = PeerConfig::empty();
        let mesh_transport = IrohMeshTransport::new(transport, peer_config);

        mesh_transport.start().await.unwrap();

        // Disconnect from non-existent peer should not error
        let peer_id = NodeId::new("test".to_string());
        let result = mesh_transport.disconnect(&peer_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_static_config_peer_registration() {
        // Create transport
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = Arc::new(IrohTransport::bind(bind_addr).await.unwrap());
        let endpoint_id = transport.endpoint_id();

        // Create config with one peer
        let peer_config = PeerConfig {
            local: LocalConfig::default(),
            formation: None,
            peers: vec![PeerInfo {
                name: "test-peer".to_string(),
                node_id: hex::encode(endpoint_id.as_bytes()),
                addresses: vec!["127.0.0.1:9999".to_string()],
                relay_url: None,
            }],
        };

        let mesh_transport = IrohMeshTransport::new(transport, peer_config);

        // Start should register peers from config
        mesh_transport.start().await.unwrap();

        // Verify peer was registered
        let node_id = NodeId::new("test-peer".to_string());
        assert_eq!(mesh_transport.get_endpoint_id(&node_id), Some(endpoint_id));
    }
}
