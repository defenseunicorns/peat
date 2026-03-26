//! Ditto mesh transport implementation
//!
//! This module provides a `MeshTransport` implementation backed by Ditto's built-in transport.
//! Since Ditto manages connections internally, this is largely a no-op wrapper that provides
//! the uniform `MeshTransport` interface for topology management.

use super::{
    MeshConnection, MeshTransport, NodeId, PeerEventReceiver, PeerEventSender, Result,
    PEER_EVENT_CHANNEL_CAPACITY,
};
use crate::sync::ditto::DittoBackend;
use async_trait::async_trait;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;

/// Ditto-based mesh transport implementation
///
/// Wraps `DittoBackend` to provide the `MeshTransport` interface with:
/// - Virtual connections (Ditto manages actual transport)
/// - No-op lifecycle management (Ditto handles internally)
/// - Peer discovery via Ditto SDK
///
/// # Design Note
///
/// Ditto's SDK manages peer discovery, connections, and data transport automatically.
/// This wrapper provides the `MeshTransport` interface for consistency with the
/// Iroh implementation, but most operations are no-ops since Ditto doesn't expose
/// explicit connection management.
///
/// # Example
///
/// ```ignore
/// use peat_protocol::transport::ditto::DittoMeshTransport;
/// use peat_protocol::sync::ditto::DittoBackend;
///
/// let ditto_backend = Arc::new(DittoBackend::new());
/// let mesh_transport = DittoMeshTransport::new(ditto_backend);
///
/// // Start/stop are no-ops - Ditto manages lifecycle
/// mesh_transport.start().await?;
/// ```
pub struct DittoMeshTransport {
    /// Underlying Ditto backend
    backend: Arc<DittoBackend>,

    /// Event broadcaster for peer events (Issue #252)
    event_senders: Arc<RwLock<Vec<PeerEventSender>>>,
}

impl DittoMeshTransport {
    /// Create a new Ditto mesh transport
    ///
    /// # Arguments
    ///
    /// * `backend` - Underlying DittoBackend
    pub fn new(backend: Arc<DittoBackend>) -> Self {
        Self {
            backend,
            event_senders: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get the underlying DittoBackend
    pub fn backend(&self) -> &Arc<DittoBackend> {
        &self.backend
    }
}

#[async_trait]
impl MeshTransport for DittoMeshTransport {
    async fn start(&self) -> Result<()> {
        // No-op: Ditto manages its own transport lifecycle
        // The backend is initialized separately via DataSyncBackend::initialize()
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // No-op: Ditto manages its own transport lifecycle
        // The backend is shutdown via DataSyncBackend::shutdown()
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        // Ditto handles connections implicitly via peer discovery
        // Just return a virtual connection representing logical reachability
        Ok(Box::new(DittoMeshConnection::new(peer_id.clone())))
    }

    async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
        // No-op: Ditto manages connections internally
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        // Check if peer is reachable via Ditto
        // For now, we create a virtual connection - in the future we could
        // query Ditto's peer list to verify actual reachability
        Some(Box::new(DittoMeshConnection::new(peer_id.clone())))
    }

    fn peer_count(&self) -> usize {
        // Query Ditto for discovered peer count
        // Note: This is a sync method but discovered_peers is async
        // For now, return 0 - this will be improved in Phase 8.2
        0
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        // Query Ditto for discovered peers and convert to NodeIds
        // Note: This is a sync method but discovered_peers is async
        // For now, return empty vec - this will be improved in Phase 8.2
        vec![]
    }

    fn subscribe_peer_events(&self) -> PeerEventReceiver {
        let (tx, rx) = mpsc::channel(PEER_EVENT_CHANNEL_CAPACITY);
        self.event_senders
            .write()
            .expect("event_senders lock poisoned")
            .push(tx);
        // Note: Ditto handles peer events internally via PresenceObserver
        // In the future, we can bridge Ditto's events to this channel
        rx
    }
}

/// Ditto mesh connection implementation
///
/// Represents a virtual connection to a peer via Ditto's transport.
/// Since Ditto manages connections internally, this is just a wrapper
/// around the peer's NodeId.
pub struct DittoMeshConnection {
    peer_id: NodeId,
    /// When this virtual connection was created
    connected_at: Instant,
}

impl DittoMeshConnection {
    /// Create a new Ditto mesh connection
    pub fn new(peer_id: NodeId) -> Self {
        Self {
            peer_id,
            connected_at: Instant::now(),
        }
    }
}

impl MeshConnection for DittoMeshConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Ditto manages connection state internally
        // We assume the connection is alive if it exists
        // TODO: Query Ditto's peer list to verify actual reachability
        true
    }

    fn connected_at(&self) -> Instant {
        self.connected_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::DataSyncBackend;

    #[tokio::test]
    async fn test_ditto_mesh_transport_creation() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Verify we can access the backend
        assert!(!mesh_transport.backend().is_ready().await);
    }

    #[tokio::test]
    async fn test_start_stop_noop() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Start/stop are no-ops and should always succeed
        mesh_transport.start().await.unwrap();
        mesh_transport.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_virtual_connection() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Connect to a peer (virtual connection)
        let peer_id = NodeId::new("test-peer".to_string());
        let conn = mesh_transport.connect(&peer_id).await.unwrap();

        assert_eq!(conn.peer_id(), &peer_id);
        assert!(conn.is_alive());
    }

    #[tokio::test]
    async fn test_disconnect_noop() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Disconnect should always succeed (no-op)
        let peer_id = NodeId::new("test-peer".to_string());
        mesh_transport.disconnect(&peer_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_connection() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Get connection returns virtual connection
        let peer_id = NodeId::new("test-peer".to_string());
        let conn = mesh_transport.get_connection(&peer_id);

        assert!(conn.is_some());
        if let Some(conn) = conn {
            assert_eq!(conn.peer_id(), &peer_id);
        }
    }

    #[tokio::test]
    async fn test_peer_count_no_backend() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Before initialization, peer count should be 0
        assert_eq!(mesh_transport.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_connected_peers_no_backend() {
        let backend = Arc::new(DittoBackend::new());
        let mesh_transport = DittoMeshTransport::new(backend);

        // Before initialization, no connected peers
        let peers = mesh_transport.connected_peers();
        assert_eq!(peers.len(), 0);
    }
}
