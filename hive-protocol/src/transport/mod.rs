//! Transport abstraction for mesh topology connections
//!
//! This module provides a backend-agnostic interface for establishing P2P connections
//! in the mesh network. It enables `TopologyManager` and related components to work
//! with both Iroh (explicit transport) and Ditto (implicit transport) backends.
//!
//! ## Architecture
//!
//! - **MeshTransport**: Connection establishment and management
//! - **MeshConnection**: Active connection to a peer
//! - **NodeId**: Mesh network node identifier
//!
//! ## Implementations
//!
//! - **IrohMeshTransport**: Uses `IrohTransport` with explicit connection management
//! - **DittoMeshTransport**: Delegates to Ditto's built-in transport
//!
//! ## Example
//!
//! ```ignore
//! use hive_protocol::transport::{MeshTransport, NodeId};
//!
//! // Create transport (Iroh or Ditto)
//! let transport: Arc<dyn MeshTransport> = ...;
//!
//! // Start transport
//! transport.start().await?;
//!
//! // Connect to peer
//! let peer_id = NodeId::new("node-123".to_string());
//! let conn = transport.connect(&peer_id).await?;
//!
//! // Check connection
//! assert!(conn.is_alive());
//! assert_eq!(conn.peer_id(), &peer_id);
//! ```

use async_trait::async_trait;
use std::error::Error as StdError;
use std::fmt;

#[cfg(feature = "automerge-backend")]
pub mod iroh;

pub mod ditto;

/// Node identifier in the mesh network
///
/// Uniquely identifies a node in the HIVE mesh. This is separate from
/// backend-specific IDs (e.g., Iroh's EndpointId, Ditto's peer ID).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// Create a new node ID from a string
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the node ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for NodeId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
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
/// `TopologyManager` to establish parent-child relationships in the mesh.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_creation() {
        let id = NodeId::new("node-123".to_string());
        assert_eq!(id.as_str(), "node-123");
        assert_eq!(id.to_string(), "node-123");
    }

    #[test]
    fn test_node_id_from_string() {
        let id: NodeId = "node-456".into();
        assert_eq!(id.as_str(), "node-456");
    }

    #[test]
    fn test_node_id_equality() {
        let id1 = NodeId::new("node-123".to_string());
        let id2 = NodeId::new("node-123".to_string());
        let id3 = NodeId::new("node-456".to_string());

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_transport_error_display() {
        let err = TransportError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");

        let err = TransportError::PeerNotFound("node-123".to_string());
        assert_eq!(err.to_string(), "Peer not found: node-123");

        let err = TransportError::NotStarted;
        assert_eq!(err.to_string(), "Transport not started");
    }
}
