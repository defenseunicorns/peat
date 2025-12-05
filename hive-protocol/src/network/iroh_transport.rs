//! Iroh QUIC transport wrapper for P2P networking
//!
//! This module provides a wrapper around Iroh's Endpoint for HIVE Protocol.
//!
//! # Features
//!
//! - **Endpoint creation and lifecycle**: QUIC-based P2P connections
//! - **Peer connection management**: Connect, accept, and track connections
//! - **Local network discovery**: Automatic peer discovery via mDNS-like protocol (Issue #226)
//! - **Static peer configuration**: Connect to peers with known addresses
//!
//! # Local Discovery (Issue #226)
//!
//! Iroh's local network discovery uses swarm-discovery to automatically find peers
//! on the same L2 network. This bridges the gap between Ditto's hostname:port
//! addressing and Iroh's EndpointId-based addressing.
//!
//! ```ignore
//! // Create transport with local discovery enabled (recommended for production)
//! let transport = IrohTransport::with_discovery("my-node").await?;
//!
//! // Discovered peers are automatically available for connection
//! let peers = transport.discovered_peers().await;
//! ```

#[cfg(feature = "automerge-backend")]
use super::peer_config::PeerInfo;
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use iroh::discovery::mdns::MdnsDiscovery;
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::{Connection, Endpoint};
#[cfg(feature = "automerge-backend")]
use iroh::{EndpointAddr, EndpointId};
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::net::SocketAddr;
#[cfg(feature = "automerge-backend")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use tokio::task::JoinHandle;

/// ALPN protocol identifier for HIVE Protocol Automerge sync
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
    /// Accept loop state
    accept_running: Arc<AtomicBool>,
    /// Accept loop task handle
    accept_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// mDNS discovery (optional, for automatic peer discovery on local network)
    #[allow(dead_code)]
    mdns_discovery: Option<MdnsDiscovery>,
}

#[cfg(feature = "automerge-backend")]
impl IrohTransport {
    /// Create a new Iroh transport (binds to ALL available interfaces)
    ///
    /// This is the recommended constructor for production use. Iroh automatically
    /// discovers all local network interfaces and advertises them to peers,
    /// enabling multi-network connectivity without additional configuration.
    ///
    /// # Multi-Interface Support (ADR-030)
    ///
    /// When using this constructor, peers will receive addresses for all interfaces:
    /// - LAN IPv4 addresses (e.g., 192.168.1.x)
    /// - External/VPN addresses (e.g., Tailscale)
    /// - IPv6 addresses
    ///
    /// Peers can connect via any advertised address, enabling seamless
    /// operation across multiple networks.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let transport = IrohTransport::new().await?;
    /// // transport.endpoint_addr() now contains ALL interface addresses
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
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
        })
    }

    /// Create a new Iroh transport with local network discovery enabled (Issue #226)
    ///
    /// This is the recommended constructor for containerlab and local network testing.
    /// It enables automatic peer discovery via mDNS-like protocol, bridging the gap
    /// between Ditto's hostname:port addressing and Iroh's EndpointId-based addressing.
    ///
    /// # How Local Discovery Works
    ///
    /// 1. Each node broadcasts its EndpointId and addresses via mDNS/DNS-SD
    /// 2. Other nodes on the same L2 network receive these announcements
    /// 3. Discovered peers are automatically added to the endpoint's address book
    /// 4. Connections can be established using just the EndpointId
    ///
    /// # Arguments
    ///
    /// * `node_name` - Human-readable name for this node (used in discovery announcements)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create transport with local discovery (recommended for containerlab)
    /// let transport = IrohTransport::with_discovery("squad-alpha-1").await?;
    ///
    /// // Wait for peers to be discovered, then connect by EndpointId
    /// let peer_id = transport.discovered_peers().await.first().unwrap();
    /// let conn = transport.connect_by_id(*peer_id).await?;
    /// ```
    pub async fn with_discovery(_node_name: &str) -> Result<Self> {
        // Generate a secret key to derive the endpoint_id
        let mut rng = rand::rng();
        let secret_key = iroh::SecretKey::generate(&mut rng);
        let endpoint_id = secret_key.public();

        // Create mDNS discovery service using the endpoint_id
        let discovery = MdnsDiscovery::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        // Create endpoint with the same secret key and discovery enabled
        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .discovery(discovery.clone())
            .bind()
            .await
            .context("Failed to create Iroh endpoint with mDNS discovery")?;

        tracing::info!(
            endpoint_id = %endpoint.id(),
            "Created IrohTransport with mDNS discovery"
        );

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Some(discovery),
        })
    }

    /// Create a new Iroh transport with deterministic key from seed (Issue #226)
    ///
    /// This is the recommended constructor for containerlab and static configurations
    /// where the EndpointId must be predictable. The secret key is derived from the
    /// seed using HKDF, making the EndpointId deterministic for a given seed.
    ///
    /// # Deterministic Key Generation for Containerlab
    ///
    /// In containerlab environments, we know:
    /// - Container hostnames (e.g., "node-1", "node-2")
    /// - Container IP addresses
    /// - A shared formation key
    ///
    /// By deriving the secret key from `"{formation_id}/{node_name}"`, the EndpointId
    /// becomes predictable and can be configured statically.
    ///
    /// # Arguments
    ///
    /// * `seed` - Seed for deterministic key generation (e.g., "formation-id/node-name")
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create transport with deterministic key for containerlab
    /// let transport = IrohTransport::from_seed("alpha-formation/node-1").await?;
    ///
    /// // The EndpointId is now predictable and can be pre-configured
    /// let endpoint_id = transport.endpoint_id();
    /// println!("Node ID: {}", hex::encode(endpoint_id.as_bytes()));
    /// ```
    pub async fn from_seed(seed: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};

        // Derive 32 bytes from seed using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(b"hive-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);

        tracing::info!(
            seed = seed,
            endpoint_id = %secret_key.public(),
            "Created IrohTransport with deterministic key from seed"
        );

        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
        })
    }

    /// Create a new Iroh transport with deterministic key and mDNS discovery
    ///
    /// Combines deterministic key generation with mDNS discovery for maximum
    /// flexibility in containerlab environments where multicast works.
    ///
    /// # Arguments
    ///
    /// * `seed` - Seed for deterministic key generation
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create transport with deterministic key AND mDNS discovery
    /// let transport = IrohTransport::from_seed_with_discovery("alpha/node-1").await?;
    /// ```
    pub async fn from_seed_with_discovery(seed: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};

        // Derive 32 bytes from seed using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(b"hive-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);
        let endpoint_id = secret_key.public();

        // Create mDNS discovery service
        let discovery = MdnsDiscovery::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        tracing::info!(
            seed = seed,
            endpoint_id = %endpoint_id,
            "Created IrohTransport with deterministic key and mDNS discovery"
        );

        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .discovery(discovery.clone())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed with discovery")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Some(discovery),
        })
    }

    /// Compute the EndpointId from a seed without creating a transport (Issue #226)
    ///
    /// This is useful for generating static peer configurations where you need
    /// to know the EndpointId before starting the node.
    ///
    /// # Arguments
    ///
    /// * `seed` - Seed for deterministic key generation (e.g., "formation-id/node-name")
    ///
    /// # Returns
    ///
    /// The EndpointId that would be generated for this seed
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Pre-compute EndpointIds for all nodes in a containerlab topology
    /// for node in ["node-1", "node-2", "node-3"] {
    ///     let seed = format!("alpha-formation/{}", node);
    ///     let endpoint_id = IrohTransport::endpoint_id_from_seed(&seed);
    ///     println!("{}: {}", node, hex::encode(endpoint_id.as_bytes()));
    /// }
    ///
    /// // Output can be used in TOML config:
    /// // [[peers]]
    /// // name = "node-1"
    /// // node_id = "computed-hex-id"
    /// // addresses = ["node-1:9000"]
    /// ```
    pub fn endpoint_id_from_seed(seed: &str) -> EndpointId {
        use sha2::{Digest, Sha256};

        // Derive 32 bytes from seed using SHA-256 (same as from_seed)
        let mut hasher = Sha256::new();
        hasher.update(b"hive-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key and extract public key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);
        secret_key.public()
    }

    /// Create a new Iroh transport bound to a SINGLE specific address
    ///
    /// **Warning**: This limits the transport to one interface only!
    ///
    /// Use this method only when you need to:
    /// - Run tests with deterministic ports
    /// - Restrict connectivity to a specific network interface (security isolation)
    /// - Debug with a known address
    ///
    /// For production multi-network deployments, use [`IrohTransport::new()`] instead.
    ///
    /// # Arguments
    ///
    /// * `bind_addr` - Socket address to bind to (IPv4 only, e.g., "127.0.0.1:9000")
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For testing only - limits to single interface
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
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
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

    /// Get a reference to the underlying Iroh endpoint
    ///
    /// This is useful for advanced operations like mDNS discovery.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Connect to a peer using EndpointAddr
    ///
    /// # Arguments
    ///
    /// * `addr` - Peer's EndpointAddr (includes EndpointId, relay URL, and direct addresses)
    ///
    /// # Returns
    ///
    /// `Ok(Some(conn))` - New connection that needs handshake
    /// `Ok(None)` - We should not initiate (they have lower ID and will connect to us)
    /// `Err(e)` - Connection failed
    ///
    /// # Note (Issue #229)
    ///
    /// Uses deterministic tie-breaking to prevent simultaneous connection race conditions.
    /// Only the side with the LOWER endpoint ID initiates connections. The side with
    /// higher ID should wait to accept incoming connections instead.
    ///
    /// This ensures exactly one QUIC connection is established between any pair of peers,
    /// avoiding the race where both sides establish connections and then close the "wrong" one.
    ///
    /// Callers MUST check for `None` and skip connection in that case.
    pub async fn connect(&self, addr: EndpointAddr) -> Result<Option<Connection>> {
        let remote_id = addr.id;
        let our_id = self.endpoint_id();

        // Deterministic tie-breaking (Issue #229): only lower ID initiates
        let we_are_lower = our_id.as_bytes() < remote_id.as_bytes();

        if !we_are_lower {
            // We have higher ID - we should NOT initiate
            // The peer with lower ID will connect to us, and we'll accept
            tracing::debug!(
                "Skipping connect to {:?}: they have lower ID and will initiate",
                remote_id
            );
            return Ok(None);
        }

        // Check if we already have a connection to this peer
        {
            let connections = self.connections.read().unwrap();
            if let Some(existing) = connections.get(&remote_id) {
                tracing::debug!("Already have connection to {:?}, reusing", remote_id);
                return Ok(Some(existing.clone()));
            }
        }

        let conn = self
            .endpoint
            .connect(addr, CAP_AUTOMERGE_ALPN)
            .await
            .context("Failed to connect to peer")?;

        // Store connection (check again in case of race with accept loop)
        let mut connections = self.connections.write().unwrap();
        if let Some(_existing) = connections.get(&remote_id) {
            // Race: accept loop stored an incoming connection while we were connecting
            // Since we're the lower ID, we're the initiator - close theirs, use ours
            // Wait, this shouldn't happen since only lower ID connects...
            // But if it does, keep our outgoing connection
            tracing::debug!(
                "Race detected with accept loop for {:?}, keeping our connection",
                remote_id
            );
            // Close the existing (incoming) and use our new one
            if let Some(old) = connections.remove(&remote_id) {
                old.close(0u32.into(), b"replaced by our initiated connection");
            }
        }

        connections.insert(remote_id, conn.clone());
        Ok(Some(conn))
    }

    /// Connect to a peer using PeerInfo from static configuration
    ///
    /// # Arguments
    ///
    /// * `peer` - PeerInfo with node_id and direct addresses
    ///
    /// # Returns
    ///
    /// `Ok(Some(conn))` - New connection that needs handshake
    /// `Ok(None)` - Already connected, no handshake needed (they are the initiator)
    /// `Err(e)` - Connection failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let peer = config.get_peer("node-1").unwrap();
    /// if let Some(conn) = transport.connect_peer(peer).await? {
    ///     // Do initiator handshake
    /// }
    /// ```
    pub async fn connect_peer(&self, peer: &PeerInfo) -> Result<Option<Connection>> {
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

    /// Connect to a peer using only their EndpointId (requires discovery) (Issue #226)
    ///
    /// This method is designed for use with local network discovery enabled.
    /// The discovery system must have already learned about this peer before
    /// a connection can be established.
    ///
    /// # Arguments
    ///
    /// * `endpoint_id` - The peer's EndpointId (discovered via local discovery)
    ///
    /// # Returns
    ///
    /// Connection to the peer
    ///
    /// # Example
    ///
    /// ```ignore
    /// // With discovery enabled, discovered peers can be connected by ID only
    /// let transport = IrohTransport::with_discovery("my-node").await?;
    ///
    /// // Wait for discovery to find peers...
    /// tokio::time::sleep(Duration::from_secs(2)).await;
    ///
    /// // Connect to a discovered peer by their EndpointId
    /// let conn = transport.connect_by_id(peer_endpoint_id).await?;
    /// ```
    pub async fn connect_by_id(&self, endpoint_id: EndpointId) -> Result<Option<Connection>> {
        // Create EndpointAddr with just the ID - discovery should have provided addresses
        let addr = EndpointAddr::new(endpoint_id);

        tracing::debug!(
            peer_id = %endpoint_id,
            "Connecting to peer by ID (using discovery-resolved addresses)"
        );

        // Use connect() which handles tie-breaking (Issue #229)
        self.connect(addr).await
    }

    /// Check if mDNS discovery is enabled
    pub fn has_discovery(&self) -> bool {
        self.mdns_discovery.is_some()
    }

    /// Accept an incoming connection
    ///
    /// This is a blocking call that waits for the next incoming connection.
    ///
    /// # Returns
    ///
    /// `Ok(Some(conn))` - A new connection that needs authentication
    /// `Ok(None)` - A duplicate connection was received and closed (already have one to this peer)
    /// `Err(e)` - An error occurred
    ///
    /// # Note (Issue #229)
    ///
    /// Since only the side with LOWER endpoint ID initiates connections, incoming
    /// connections always come from peers with lower IDs. If we already have a
    /// connection to this peer, it means they're reconnecting - we close the old
    /// connection and accept the new one.
    ///
    /// Callers MUST check for `None` and skip authentication in that case.
    pub async fn accept(&self) -> Result<Option<Connection>> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .context("No incoming connection")?;

        let conn = incoming.await.context("Failed to accept connection")?;
        let remote_id = conn.remote_id();

        let mut connections = self.connections.write().unwrap();

        // Check if we already have a connection to this peer (Issue #229)
        // Since only lower ID initiates, if we have an existing connection to them,
        // it's from a previous connection attempt. Accept the new one.
        if let Some(old_conn) = connections.remove(&remote_id) {
            tracing::debug!(
                "Replacing existing connection from {:?} with new incoming connection",
                remote_id
            );
            old_conn.close(0u32.into(), b"replaced by new connection");
        }

        // Store and return the new connection
        connections.insert(remote_id, conn.clone());
        Ok(Some(conn))
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

    /// Start the accept loop to receive incoming connections
    ///
    /// Spawns a background task that continuously accepts incoming connections.
    /// Connections are automatically stored in the connections map.
    ///
    /// # Example
    ///
    /// ```ignore
    /// transport.start_accept_loop();
    /// ```
    pub fn start_accept_loop(self: &Arc<Self>) -> Result<()> {
        // Check if already running
        if self
            .accept_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            anyhow::bail!("Accept loop already running");
        }

        let transport = Arc::clone(self);
        let accept_running = Arc::clone(&self.accept_running);

        let task = tokio::spawn(async move {
            while accept_running.load(Ordering::Relaxed) {
                match transport.accept().await {
                    Ok(Some(conn)) => {
                        tracing::debug!("Accepted connection from: {:?}", conn.remote_id());
                    }
                    Ok(None) => {
                        // Duplicate connection closed (Issue #229)
                        tracing::debug!("Duplicate connection closed, using existing");
                    }
                    Err(e) => {
                        // Accept loop stopped or endpoint closed
                        tracing::debug!("Accept loop ended: {}", e);
                        break;
                    }
                }
            }
            tracing::debug!("Accept loop stopped");
        });

        *self.accept_task.write().unwrap() = Some(task);

        Ok(())
    }

    /// Stop the accept loop
    ///
    /// Stops accepting new incoming connections. Existing connections remain active.
    pub fn stop_accept_loop(&self) -> Result<()> {
        if !self.accept_running.swap(false, Ordering::SeqCst) {
            anyhow::bail!("Accept loop is not running");
        }

        // Task will stop on next accept() call or timeout
        Ok(())
    }

    /// Check if accept loop is running
    pub fn is_accept_loop_running(&self) -> bool {
        self.accept_running.load(Ordering::Relaxed)
    }

    /// Mark accept loop as externally managed
    ///
    /// Call this when external code (e.g., IrohPeerDiscovery) is managing its own
    /// accept loop with custom handling (like formation handshakes).
    /// This prevents `start_accept_loop()` from starting a duplicate accept loop.
    ///
    /// Returns `Err` if an accept loop is already marked as running.
    pub fn mark_accept_loop_managed(&self) -> Result<()> {
        if self
            .accept_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            anyhow::bail!("Accept loop already running");
        }
        Ok(())
    }

    /// Close the transport and all connections
    pub async fn close(self) -> Result<()> {
        // Stop accept loop if running
        if self.accept_running.load(Ordering::Relaxed) {
            let _ = self.stop_accept_loop();
        }

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

    #[tokio::test]
    async fn test_transport_with_discovery() {
        // Test that with_discovery creates a transport with mDNS discovery enabled (Issue #226)
        let transport = IrohTransport::with_discovery("test-node").await.unwrap();

        // Verify endpoint was created
        let endpoint_id = transport.endpoint_id();
        assert_ne!(endpoint_id.as_bytes(), &[0u8; 32]);

        // Verify discovery is enabled
        assert!(transport.has_discovery());

        // Verify initial state is empty
        assert_eq!(transport.peer_count(), 0);
        assert!(transport.connected_peers().is_empty());

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_transport_without_discovery() {
        // Test that standard new() doesn't enable discovery
        let transport = IrohTransport::new().await.unwrap();

        // Verify discovery is NOT enabled
        assert!(!transport.has_discovery());

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_from_seed_deterministic() {
        // Test that from_seed produces deterministic EndpointIds (Issue #226)
        let seed = "test-formation/node-1";

        // Create two transports from the same seed
        let transport1 = IrohTransport::from_seed(seed).await.unwrap();
        let id1 = transport1.endpoint_id();
        transport1.close().await.unwrap();

        let transport2 = IrohTransport::from_seed(seed).await.unwrap();
        let id2 = transport2.endpoint_id();
        transport2.close().await.unwrap();

        // They should have the same EndpointId
        assert_eq!(id1, id2, "Same seed should produce same EndpointId");
    }

    #[tokio::test]
    async fn test_from_seed_different_seeds() {
        // Test that different seeds produce different EndpointIds
        let transport1 = IrohTransport::from_seed("formation/node-1").await.unwrap();
        let id1 = transport1.endpoint_id();

        let transport2 = IrohTransport::from_seed("formation/node-2").await.unwrap();
        let id2 = transport2.endpoint_id();

        // Different seeds should produce different EndpointIds
        assert_ne!(
            id1, id2,
            "Different seeds should produce different EndpointIds"
        );

        transport1.close().await.unwrap();
        transport2.close().await.unwrap();
    }

    #[test]
    fn test_endpoint_id_from_seed() {
        // Test the static helper function
        let seed = "alpha-formation/node-1";

        let id1 = IrohTransport::endpoint_id_from_seed(seed);
        let id2 = IrohTransport::endpoint_id_from_seed(seed);

        // Should be deterministic
        assert_eq!(id1, id2);

        // Should produce different IDs for different seeds
        let id3 = IrohTransport::endpoint_id_from_seed("alpha-formation/node-2");
        assert_ne!(id1, id3);
    }

    #[tokio::test]
    async fn test_from_seed_matches_static_computation() {
        // Test that from_seed produces the same ID as endpoint_id_from_seed
        let seed = "containerlab/mesh-node-1";

        let computed_id = IrohTransport::endpoint_id_from_seed(seed);

        let transport = IrohTransport::from_seed(seed).await.unwrap();
        let transport_id = transport.endpoint_id();

        assert_eq!(
            computed_id, transport_id,
            "Static and dynamic computation should match"
        );

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_from_seed_with_discovery() {
        // Test that from_seed_with_discovery enables both deterministic keys and mDNS
        let seed = "test-formation/discovery-node";

        let transport = IrohTransport::from_seed_with_discovery(seed).await.unwrap();

        // Should have discovery enabled
        assert!(transport.has_discovery());

        // Should have deterministic endpoint ID
        let expected_id = IrohTransport::endpoint_id_from_seed(seed);
        assert_eq!(transport.endpoint_id(), expected_id);

        transport.close().await.unwrap();
    }
}
