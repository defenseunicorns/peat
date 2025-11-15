//! Iroh QUIC transport wrapper for P2P networking
//!
//! This module provides a wrapper around Iroh's Endpoint for CAP Protocol.
//!
//! # Phase 3 Implementation
//!
//! Basic P2P connectivity with:
//! - Endpoint creation and lifecycle
//! - Peer connection management
//! - Bidirectional streams
//! - Static peer configuration
//!
//! # Phase 4 Will Add
//!
//! - Automerge sync protocol
//! - Document sync over streams
//! - Automatic change propagation

#[cfg(feature = "automerge-backend")]
use super::peer_config::PeerInfo;
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::{Connection, Endpoint};
#[cfg(feature = "automerge-backend")]
use iroh::{EndpointAddr, EndpointId};
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::net::SocketAddr;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};

/// ALPN protocol identifier for CAP Protocol Automerge sync
#[cfg(feature = "automerge-backend")]
pub const CAP_AUTOMERGE_ALPN: &[u8] = b"cap/automerge/1";

/// Iroh QUIC transport for P2P connections
///
/// Wraps Iroh Endpoint to provide CAP-specific networking.
#[cfg(feature = "automerge-backend")]
pub struct IrohTransport {
    /// Iroh endpoint for QUIC connections
    endpoint: Endpoint,
    /// Active peer connections
    connections: Arc<RwLock<HashMap<EndpointId, Connection>>>,
}

#[cfg(feature = "automerge-backend")]
impl IrohTransport {
    /// Create a new Iroh transport (binds to any available port)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let transport = IrohTransport::new().await?;
    /// ```
    pub async fn new() -> Result<Self> {
        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .bind()
            .await
            .context("Failed to create Iroh endpoint")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new Iroh transport bound to a specific address
    ///
    /// # Arguments
    ///
    /// * `bind_addr` - Socket address to bind to (e.g., "127.0.0.1:9000")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let addr = "127.0.0.1:9000".parse()?;
    /// let transport = IrohTransport::bind(addr).await?;
    /// ```
    pub async fn bind(bind_addr: SocketAddr) -> Result<Self> {
        // Convert SocketAddr to SocketAddrV4 if it's IPv4
        let bind_addr_v4 = match bind_addr {
            SocketAddr::V4(addr) => addr,
            SocketAddr::V6(_) => anyhow::bail!("Only IPv4 addresses supported for now"),
        };

        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .bind_addr_v4(bind_addr_v4)
            .bind()
            .await
            .context("Failed to create Iroh endpoint with bind address")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get the local endpoint ID
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Get the local endpoint address for sharing with peers
    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Connect to a peer using EndpointAddr
    ///
    /// # Arguments
    ///
    /// * `addr` - Peer's EndpointAddr (includes EndpointId, relay URL, and direct addresses)
    ///
    /// # Returns
    ///
    /// Connection to the peer
    pub async fn connect(&self, addr: EndpointAddr) -> Result<Connection> {
        let endpoint_id = addr.id;

        let conn = self
            .endpoint
            .connect(addr, CAP_AUTOMERGE_ALPN)
            .await
            .context("Failed to connect to peer")?;

        // Store connection
        self.connections
            .write()
            .unwrap()
            .insert(endpoint_id, conn.clone());

        Ok(conn)
    }

    /// Connect to a peer using PeerInfo from static configuration
    ///
    /// # Arguments
    ///
    /// * `peer` - PeerInfo with node_id and direct addresses
    ///
    /// # Returns
    ///
    /// Connection to the peer
    ///
    /// # Example
    ///
    /// ```ignore
    /// let peer = config.get_peer("node-1").unwrap();
    /// let conn = transport.connect_peer(peer).await?;
    /// ```
    pub async fn connect_peer(&self, peer: &PeerInfo) -> Result<Connection> {
        let endpoint_id = peer.endpoint_id()?;
        let socket_addrs = peer.socket_addrs()?;

        // Create EndpointAddr with direct addresses
        // Note: with_ip_addr adds direct addresses one at a time
        let mut addr = EndpointAddr::new(endpoint_id);
        for socket_addr in socket_addrs {
            addr = addr.with_ip_addr(socket_addr);
        }

        self.connect(addr).await
    }

    /// Accept an incoming connection
    ///
    /// This is a blocking call that waits for the next incoming connection.
    ///
    /// # Returns
    ///
    /// The accepted connection
    pub async fn accept(&self) -> Result<Connection> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .context("No incoming connection")?;

        let conn = incoming.await.context("Failed to accept connection")?;
        let endpoint_id = conn.remote_id();

        // Store connection
        self.connections
            .write()
            .unwrap()
            .insert(endpoint_id, conn.clone());

        Ok(conn)
    }

    /// Get an existing connection to a peer
    pub fn get_connection(&self, endpoint_id: &EndpointId) -> Option<Connection> {
        self.connections.read().unwrap().get(endpoint_id).cloned()
    }

    /// Disconnect from a peer
    pub fn disconnect(&self, endpoint_id: &EndpointId) -> Result<()> {
        if let Some(conn) = self.connections.write().unwrap().remove(endpoint_id) {
            conn.close(0u32.into(), b"disconnecting");
        }
        Ok(())
    }

    /// Get the number of connected peers
    pub fn peer_count(&self) -> usize {
        self.connections.read().unwrap().len()
    }

    /// Get all connected peer IDs
    pub fn connected_peers(&self) -> Vec<EndpointId> {
        self.connections.read().unwrap().keys().copied().collect()
    }

    /// Close the transport and all connections
    pub async fn close(self) -> Result<()> {
        // Close all connections
        for (_endpoint_id, conn) in self.connections.write().unwrap().drain() {
            conn.close(0u32.into(), b"shutdown");
        }

        // Close endpoint
        self.endpoint.close().await;

        Ok(())
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_creation() {
        let transport = IrohTransport::new().await.unwrap();
        let endpoint_id = transport.endpoint_id();

        // Endpoint ID should be valid (non-zero)
        assert_ne!(endpoint_id.as_bytes(), &[0u8; 32]);

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_transport_endpoint_addr() {
        let transport = IrohTransport::new().await.unwrap();
        let addr = transport.endpoint_addr();

        // Endpoint addr should match endpoint ID
        assert_eq!(addr.id, transport.endpoint_id());

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_count_initially_zero() {
        let transport = IrohTransport::new().await.unwrap();
        assert_eq!(transport.peer_count(), 0);
        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_connected_peers_initially_empty() {
        let transport = IrohTransport::new().await.unwrap();
        assert!(transport.connected_peers().is_empty());
        transport.close().await.unwrap();
    }
}
