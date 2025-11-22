//! Topology Manager for mesh connection lifecycle
//!
//! This module implements the TopologyManager which coordinates topology-driven
//! connection establishment by listening to topology events and managing transport
//! connections accordingly.

use super::{TopologyBuilder, TopologyEvent};
use hive_protocol::transport::{MeshConnection, MeshTransport, NodeId};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Topology Manager
///
/// Manages mesh connections based on topology formation events.
/// Wraps a TopologyBuilder and MeshTransport to automatically establish
/// and tear down connections as the topology changes.
///
/// # Architecture
///
/// - Subscribes to topology events from TopologyBuilder
/// - Reacts to PeerSelected/Changed/Lost events
/// - Establishes peer connections via MeshTransport
/// - Tears down stale connections
///
/// # Example
///
/// ```ignore
/// use hive_mesh::topology::{TopologyManager, TopologyBuilder};
/// use hive_protocol::transport::MeshTransport;
///
/// let builder = TopologyBuilder::new(...);
/// let transport: Arc<dyn MeshTransport> = ...;
/// let manager = TopologyManager::new(builder, transport);
///
/// manager.start().await?;
/// ```
pub struct TopologyManager {
    /// Topology builder for peer selection
    builder: TopologyBuilder,

    /// Transport abstraction for connections
    transport: Arc<dyn MeshTransport>,

    /// Current peer connection (if any)
    peer_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,

    /// Current selected peer node ID (if any)
    selected_peer_id: Arc<RwLock<Option<NodeId>>>,

    /// Background task handle
    task_handle: RwLock<Option<JoinHandle<()>>>,
}

impl TopologyManager {
    /// Create a new topology manager
    ///
    /// # Arguments
    ///
    /// * `builder` - TopologyBuilder for peer selection
    /// * `transport` - Transport abstraction for connections
    pub fn new(builder: TopologyBuilder, transport: Arc<dyn MeshTransport>) -> Self {
        Self {
            builder,
            transport,
            peer_connection: Arc::new(RwLock::new(None)),
            selected_peer_id: Arc::new(RwLock::new(None)),
            task_handle: RwLock::new(None),
        }
    }

    /// Start topology management
    ///
    /// Starts both the topology builder and the event listener that manages connections.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start the transport
        self.transport.start().await?;

        // Start the topology builder
        self.builder.start().await;

        // Subscribe to topology events
        if let Some(rx) = self.builder.subscribe() {
            let transport = self.transport.clone();
            let peer_connection = self.peer_connection.clone();
            let selected_peer_id = self.selected_peer_id.clone();

            let handle = tokio::spawn(async move {
                Self::event_loop(rx, transport, peer_connection, selected_peer_id).await;
            });

            *self.task_handle.write().unwrap() = Some(handle);
        }

        Ok(())
    }

    /// Stop topology management
    ///
    /// Stops the topology builder and disconnects from selected peer.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Abort the event loop task
        if let Some(handle) = self.task_handle.write().unwrap().take() {
            handle.abort();
        }

        // Stop the topology builder
        self.builder.stop().await;

        // Disconnect from current selected peer
        let current_selected_peer_id = self.selected_peer_id.write().unwrap().take();
        if let Some(selected_peer_id) = current_selected_peer_id {
            if let Err(e) = self.transport.disconnect(&selected_peer_id).await {
                warn!("Failed to disconnect from selected peer during stop: {}", e);
            }
        }

        // Stop the transport
        self.transport.stop().await?;

        Ok(())
    }

    /// Get current selected peer node ID
    pub fn get_selected_peer_id(&self) -> Option<NodeId> {
        self.selected_peer_id.read().unwrap().clone()
    }

    /// Check if currently connected to a specific peer
    pub fn is_connected_to_peer(&self, node_id: &NodeId) -> bool {
        self.selected_peer_id
            .read()
            .unwrap()
            .as_ref()
            .map(|id| id == node_id)
            .unwrap_or(false)
    }

    /// Get the underlying topology builder
    pub fn builder(&self) -> &TopologyBuilder {
        &self.builder
    }

    /// Event processing loop
    ///
    /// Listens to topology events and manages connections accordingly.
    async fn event_loop(
        mut rx: mpsc::UnboundedReceiver<TopologyEvent>,
        transport: Arc<dyn MeshTransport>,
        peer_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,
        selected_peer_id: Arc<RwLock<Option<NodeId>>>,
    ) {
        while let Some(event) = rx.recv().await {
            match event {
                TopologyEvent::PeerSelected {
                    selected_peer_id: new_peer_id,
                    ..
                } => {
                    info!("Peer selected: {}", new_peer_id);
                    let node_id = NodeId::new(new_peer_id.clone());

                    // Connect to the selected peer
                    match transport.connect(&node_id).await {
                        Ok(conn) => {
                            *peer_connection.write().unwrap() = Some(conn);
                            *selected_peer_id.write().unwrap() = Some(node_id);
                            info!("Successfully connected to peer: {}", new_peer_id);
                        }
                        Err(e) => {
                            warn!("Failed to connect to peer {}: {}", new_peer_id, e);
                        }
                    }
                }

                TopologyEvent::PeerChanged {
                    old_peer_id,
                    new_peer_id,
                    ..
                } => {
                    info!("Selected peer changed: {} -> {}", old_peer_id, new_peer_id);

                    // Disconnect from old peer
                    let old_id = NodeId::new(old_peer_id.clone());
                    if let Err(e) = transport.disconnect(&old_id).await {
                        warn!("Failed to disconnect from old peer {}: {}", old_peer_id, e);
                    }

                    // Connect to new peer
                    let new_id = NodeId::new(new_peer_id.clone());
                    match transport.connect(&new_id).await {
                        Ok(conn) => {
                            *peer_connection.write().unwrap() = Some(conn);
                            *selected_peer_id.write().unwrap() = Some(new_id);
                            info!("Successfully changed to peer: {}", new_peer_id);
                        }
                        Err(e) => {
                            warn!("Failed to connect to new peer {}: {}", new_peer_id, e);
                        }
                    }
                }

                TopologyEvent::PeerLost { lost_peer_id } => {
                    info!("Selected peer lost: {}", lost_peer_id);

                    // Clear peer connection
                    *peer_connection.write().unwrap() = None;
                    *selected_peer_id.write().unwrap() = None;

                    // Disconnect from lost peer
                    let node_id = NodeId::new(lost_peer_id.clone());
                    if let Err(e) = transport.disconnect(&node_id).await {
                        warn!(
                            "Failed to disconnect from lost peer {}: {}",
                            lost_peer_id, e
                        );
                    }

                    debug!("Cleared connection to lost peer: {}", lost_peer_id);
                }

                TopologyEvent::PeerAdded { linked_peer_id } => {
                    info!("Linked peer added: {}", linked_peer_id);
                    // Linked peers connect TO us, so no action needed here
                    // The transport layer handles incoming connections automatically
                }

                TopologyEvent::PeerRemoved { linked_peer_id } => {
                    info!("Linked peer removed (beacon expired): {}", linked_peer_id);

                    // Disconnect from stale linked peer
                    let node_id = NodeId::new(linked_peer_id.clone());
                    if transport.is_connected(&node_id) {
                        if let Err(e) = transport.disconnect(&node_id).await {
                            warn!(
                                "Failed to disconnect from stale linked peer {}: {}",
                                linked_peer_id, e
                            );
                        } else {
                            debug!("Disconnected from stale linked peer: {}", linked_peer_id);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_protocol::transport::{
        MeshConnection as MeshConnectionTrait, MeshTransport, NodeId, Result,
    };
    use std::sync::Arc;

    // Mock transport for testing
    struct MockTransport {
        started: Arc<RwLock<bool>>,
        stopped: Arc<RwLock<bool>>,
        connections: Arc<RwLock<Vec<NodeId>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                started: Arc::new(RwLock::new(false)),
                stopped: Arc::new(RwLock::new(false)),
                connections: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn is_started(&self) -> bool {
            *self.started.read().unwrap()
        }

        fn is_stopped(&self) -> bool {
            *self.stopped.read().unwrap()
        }

        fn has_connection(&self, node_id: &NodeId) -> bool {
            self.connections
                .read()
                .unwrap()
                .iter()
                .any(|id| id == node_id)
        }
    }

    struct MockConnection {
        peer_id: NodeId,
    }

    impl MeshConnectionTrait for MockConnection {
        fn peer_id(&self) -> &NodeId {
            &self.peer_id
        }

        fn is_alive(&self) -> bool {
            true
        }
    }

    #[async_trait::async_trait]
    impl MeshTransport for MockTransport {
        async fn start(&self) -> Result<()> {
            *self.started.write().unwrap() = true;
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            *self.stopped.write().unwrap() = true;
            Ok(())
        }

        async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnectionTrait>> {
            self.connections.write().unwrap().push(peer_id.clone());
            Ok(Box::new(MockConnection {
                peer_id: peer_id.clone(),
            }))
        }

        async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
            self.connections.write().unwrap().retain(|id| id != peer_id);
            Ok(())
        }

        fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnectionTrait>> {
            if self.has_connection(peer_id) {
                Some(Box::new(MockConnection {
                    peer_id: peer_id.clone(),
                }))
            } else {
                None
            }
        }

        fn peer_count(&self) -> usize {
            self.connections.read().unwrap().len()
        }

        fn connected_peers(&self) -> Vec<NodeId> {
            self.connections.read().unwrap().clone()
        }
    }

    // Minimal test that doesn't require BeaconObserver
    #[test]
    fn test_node_id_api() {
        let node_id1 = NodeId::new("test-node".to_string());
        let node_id2 = NodeId::new("test-node".to_string());
        let node_id3 = NodeId::new("other-node".to_string());

        assert_eq!(node_id1, node_id2);
        assert_ne!(node_id1, node_id3);
    }

    #[test]
    fn test_mock_transport_creation() {
        let transport = MockTransport::new();
        assert!(!transport.is_started());
        assert!(!transport.is_stopped());
        assert_eq!(transport.peer_count(), 0);
    }
}
