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
use iroh::endpoint::{Connection, Endpoint, TransportConfig};
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
use std::time::Duration;
#[cfg(feature = "automerge-backend")]
use tokio::sync::mpsc;
#[cfg(feature = "automerge-backend")]
use tokio::task::JoinHandle;

// =============================================================================
// Peer Events (Issue #275)
// =============================================================================

/// Transport-level peer event (Issue #275)
///
/// Emitted when connections are established or closed at the transport level.
/// This is transport-agnostic - the same event type can be used for QUIC, Ditto, etc.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub enum TransportPeerEvent {
    /// New peer connected
    Connected {
        /// The peer's endpoint ID
        endpoint_id: EndpointId,
        /// When the connection was established
        connected_at: std::time::Instant,
    },
    /// Peer disconnected
    Disconnected {
        /// The peer's endpoint ID
        endpoint_id: EndpointId,
        /// Reason for disconnection
        reason: String,
    },
}

/// Channel capacity for transport peer events
#[cfg(feature = "automerge-backend")]
pub const TRANSPORT_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Type alias for transport event receiver
#[cfg(feature = "automerge-backend")]
pub type TransportEventReceiver = mpsc::Receiver<TransportPeerEvent>;

/// Type alias for transport event sender
#[cfg(feature = "automerge-backend")]
pub type TransportEventSender = mpsc::Sender<TransportPeerEvent>;

/// ALPN protocol identifier for HIVE Protocol Automerge sync
#[cfg(feature = "automerge-backend")]
pub const CAP_AUTOMERGE_ALPN: &[u8] = b"cap/automerge/1";

// =============================================================================
// QUIC Timeout Configuration (Issue #315)
// =============================================================================

/// Maximum idle timeout for QUIC connections (Issue #315, #346)
///
/// When a peer disconnects unexpectedly (crash, kill, network loss), QUIC detects
/// "dead" connections via idle timeout.
///
/// ## History
///
/// - Issue #315: Set to 5 seconds for fast tactical disconnect detection
/// - Issue #346: Increased to 30 seconds for hierarchical sync stability
///
/// ## Rationale (Issue #346)
///
/// In hierarchical deployments (96+ nodes), sync at upper levels is sparse:
/// - Squad members → Squad leaders: every few seconds (works with short timeout)
/// - Squad leaders → Platoon leaders: every 10-30 seconds
/// - Platoon leaders → Company: every 30-60+ seconds
///
/// The 5-second timeout caused connections to drop before upper-level sync happened.
/// Ditto uses 60 seconds as their default. We compromise at 30 seconds to balance:
/// - Fast enough disconnect detection for tactical awareness (~30s vs ~40s default)
/// - Long enough for hierarchical sync patterns
///
/// The keep-alive interval (1 second) maintains active connections; this timeout
/// only affects truly idle or dead connections.
#[cfg(feature = "automerge-backend")]
pub const QUIC_MAX_IDLE_TIMEOUT_SECS: u64 = 30;

/// Keep-alive interval for QUIC connections (Issue #315)
///
/// Sending keep-alive packets prevents healthy but inactive connections from
/// timing out and enables faster detection of dead connections.
///
/// Setting this to 1 second ensures:
/// - Multiple keep-alives are sent before the idle timeout expires
/// - Dead connections are detected within ~5 seconds (1 missed + 1 timeout margin)
/// - Acceptable overhead for tactical radio networks (~40 bytes/second)
#[cfg(feature = "automerge-backend")]
pub const QUIC_KEEP_ALIVE_INTERVAL_SECS: u64 = 1;

/// Create a TransportConfig with optimized timeout settings (Issue #315, #346)
///
/// Key settings:
/// - `max_idle_timeout`: 30 seconds (balanced for hierarchical sync)
/// - `keep_alive_interval`: 1 second (aggressive connection health monitoring)
///
/// This configuration provides:
/// - Disconnect detection within ~30 seconds (faster than default ~40s)
/// - Stable connections for hierarchical sync patterns (Issue #346)
/// - Immediate awareness of connection state changes via keep-alives
/// - Designed for tactical/hierarchical deployments with varying sync frequencies
#[cfg(feature = "automerge-backend")]
fn create_tactical_transport_config() -> TransportConfig {
    let mut config = TransportConfig::default();

    // Set maximum idle timeout (Issue #346: increased from 5s to 30s for hierarchical sync)
    // The IdleTimeout type requires conversion from Duration
    config.max_idle_timeout(Some(
        Duration::from_secs(QUIC_MAX_IDLE_TIMEOUT_SECS)
            .try_into()
            .unwrap(),
    ));

    // Enable keep-alive packets every 1 second to prevent healthy connections
    // from timing out and to detect dead connections faster
    config.keep_alive_interval(Some(Duration::from_secs(QUIC_KEEP_ALIVE_INTERVAL_SECS)));

    tracing::debug!(
        max_idle_timeout_secs = QUIC_MAX_IDLE_TIMEOUT_SECS,
        keep_alive_interval_secs = QUIC_KEEP_ALIVE_INTERVAL_SECS,
        "Created tactical QUIC transport config (Issue #315)"
    );

    config
}

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
    /// Event senders for peer events (Issue #275)
    /// Multiple receivers can subscribe via subscribe_peer_events()
    event_senders: Arc<RwLock<Vec<TransportEventSender>>>,
    /// Tokio runtime handle for spawning connection monitor tasks
    runtime_handle: tokio::runtime::Handle,
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
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
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
            .transport_config(create_tactical_transport_config())
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
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
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
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
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
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed with discovery")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Some(discovery),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }

    /// Create transport with deterministic key, mDNS discovery, AND specific bind address (Issue #233)
    ///
    /// This is the recommended constructor for mobile/embedded deployments where you need:
    /// - Deterministic EndpointId (for peer pre-configuration)
    /// - mDNS discovery (for local network peer finding)
    /// - Specific bind address (for firewall/NAT configuration)
    ///
    /// # Arguments
    ///
    /// * `seed` - Seed for deterministic key generation (e.g., "app-id/device-uuid")
    /// * `bind_addr` - Socket address to bind to (IPv4 only)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For Android FFI with discovery enabled
    /// let seed = format!("{}/{}", app_id, device_uuid);
    /// let addr = "0.0.0.0:9000".parse()?;
    /// let transport = IrohTransport::from_seed_with_discovery_at_addr(&seed, addr).await?;
    /// ```
    pub async fn from_seed_with_discovery_at_addr(
        seed: &str,
        bind_addr: SocketAddr,
    ) -> Result<Self> {
        use sha2::{Digest, Sha256};

        // Convert SocketAddr to SocketAddrV4 if it's IPv4
        let bind_addr_v4 = match bind_addr {
            SocketAddr::V4(addr) => addr,
            SocketAddr::V6(_) => anyhow::bail!("Only IPv4 addresses supported for now"),
        };

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
            bind_addr = %bind_addr,
            "Created IrohTransport with deterministic key, mDNS discovery, and bind address"
        );

        let endpoint = Endpoint::builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .discovery(discovery.clone())
            .bind_addr_v4(bind_addr_v4)
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed with discovery at addr")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Some(discovery),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
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
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint with bind address")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: None,
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
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
        drop(connections); // Release lock before emitting event

        // Emit connect event (Issue #275)
        self.emit_event(TransportPeerEvent::Connected {
            endpoint_id: remote_id,
            connected_at: std::time::Instant::now(),
        });

        // Spawn connection close monitor for instant disconnect detection (Issue #275)
        self.spawn_connection_monitor(remote_id, conn.clone());

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

    /// Get a reference to the mDNS discovery service (Issue #233)
    ///
    /// This allows subscribing to mDNS discovery events to learn about newly
    /// discovered peers on the local network. The returned discovery service
    /// has a `subscribe()` method that returns a stream of `DiscoveryEvent`.
    ///
    /// # Returns
    ///
    /// `Some(&MdnsDiscovery)` if mDNS discovery is enabled, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(mdns) = transport.mdns_discovery() {
    ///     let mut stream = mdns.subscribe().await;
    ///     while let Some(event) = stream.next().await {
    ///         match event {
    ///             DiscoveryEvent::Discovered(item) => {
    ///                 // Connect to the newly discovered peer
    ///                 transport.connect_by_id(item.node_id).await?;
    ///             }
    ///             DiscoveryEvent::Expired(node_id) => {
    ///                 // Peer is no longer available
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn mdns_discovery(&self) -> Option<&MdnsDiscovery> {
        self.mdns_discovery.as_ref()
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
        drop(connections); // Release lock before emitting event

        // Emit connect event (Issue #275)
        self.emit_event(TransportPeerEvent::Connected {
            endpoint_id: remote_id,
            connected_at: std::time::Instant::now(),
        });

        // Spawn connection close monitor for instant disconnect detection (Issue #275)
        self.spawn_connection_monitor(remote_id, conn.clone());

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
            // Emit disconnect event (Issue #275)
            self.emit_event(TransportPeerEvent::Disconnected {
                endpoint_id: *endpoint_id,
                reason: "local disconnect".to_string(),
            });
        }
        Ok(())
    }

    // =========================================================================
    // Peer Events (Issue #275)
    // =========================================================================

    /// Subscribe to peer connection events
    ///
    /// Returns a receiver channel that will receive `TransportPeerEvent` notifications
    /// for all connection lifecycle changes (connect, disconnect).
    ///
    /// Multiple subscribers are supported - each gets their own channel.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut events = transport.subscribe_peer_events();
    /// tokio::spawn(async move {
    ///     while let Some(event) = events.recv().await {
    ///         match event {
    ///             TransportPeerEvent::Connected { endpoint_id, .. } => {
    ///                 println!("Peer connected: {:?}", endpoint_id);
    ///             }
    ///             TransportPeerEvent::Disconnected { endpoint_id, reason } => {
    ///                 println!("Peer disconnected: {:?} - {}", endpoint_id, reason);
    ///             }
    ///         }
    ///     }
    /// });
    /// ```
    pub fn subscribe_peer_events(&self) -> TransportEventReceiver {
        let (tx, rx) = mpsc::channel(TRANSPORT_EVENT_CHANNEL_CAPACITY);
        self.event_senders.write().unwrap().push(tx);
        rx
    }

    /// Emit a peer event to all subscribers (Issue #275)
    ///
    /// Called internally when connections are established or closed.
    fn emit_event(&self, event: TransportPeerEvent) {
        let senders = self.event_senders.read().unwrap();
        for sender in senders.iter() {
            // Non-blocking send - drop if channel is full
            let _ = sender.try_send(event.clone());
        }
    }

    /// Spawn a task to monitor a connection for closure (Issue #275)
    ///
    /// This task awaits `connection.closed()` which completes immediately when
    /// the connection closes, regardless of how (graceful, timeout, or abrupt).
    /// When the connection closes, it removes it from the map and emits a
    /// Disconnected event.
    ///
    /// This provides instant disconnect detection, unlike `cleanup_closed_connections()`
    /// which only works after the QUIC idle timeout expires (~30 seconds).
    fn spawn_connection_monitor(&self, endpoint_id: EndpointId, conn: Connection) {
        let connections = Arc::clone(&self.connections);
        let event_senders = Arc::clone(&self.event_senders);

        self.runtime_handle.spawn(async move {
            // Wait for the connection to close (this completes immediately when closed)
            let close_reason = conn.closed().await;

            tracing::info!(
                ?endpoint_id,
                ?close_reason,
                "Connection closed, emitting disconnect event"
            );

            // Remove from connections map
            {
                let mut conns = connections.write().unwrap();
                conns.remove(&endpoint_id);
            }

            // Emit disconnect event
            let reason = format!("{:?}", close_reason);
            let event = TransportPeerEvent::Disconnected {
                endpoint_id,
                reason,
            };
            let senders = event_senders.read().unwrap();
            for sender in senders.iter() {
                let _ = sender.try_send(event.clone());
            }
        });
    }

    /// Get the number of currently connected peers
    ///
    /// Only counts connections that are still alive (not closed).
    /// Automatically cleans up closed connections from the map.
    pub fn peer_count(&self) -> usize {
        self.cleanup_closed_connections();
        self.connections.read().unwrap().len()
    }

    /// Get all currently connected peer IDs
    ///
    /// Only returns connections that are still alive (not closed).
    /// Automatically cleans up closed connections from the map.
    pub fn connected_peers(&self) -> Vec<EndpointId> {
        self.cleanup_closed_connections();
        self.connections.read().unwrap().keys().copied().collect()
    }

    /// Remove closed connections from the connections map
    ///
    /// Called automatically by `peer_count()` and `connected_peers()`.
    /// Can also be called explicitly to clean up stale connections.
    /// Emits disconnect events for removed connections (Issue #275).
    pub fn cleanup_closed_connections(&self) {
        // Collect closed connections to emit events after releasing lock
        let closed_peers: Vec<(EndpointId, String)> = {
            let mut connections = self.connections.write().unwrap();
            let mut closed = Vec::new();

            connections.retain(|endpoint_id, conn| {
                if let Some(reason) = conn.close_reason() {
                    tracing::debug!(?endpoint_id, "Removing closed connection from map");
                    let reason_str = format!("{:?}", reason);
                    closed.push((*endpoint_id, reason_str));
                    false
                } else {
                    true
                }
            });

            closed
        };

        // Emit disconnect events (Issue #275)
        for (endpoint_id, reason) in closed_peers {
            self.emit_event(TransportPeerEvent::Disconnected {
                endpoint_id,
                reason,
            });
        }
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

    /// Test that disconnected peers are removed from the connections map (Issue #244)
    #[tokio::test]
    async fn test_stale_peer_cleanup_issue_244() {
        use std::sync::Arc;

        // Use deterministic keys so we know which direction to connect
        // Lower ID initiates connection (Issue #229 tie-breaking)
        let transport_a = Arc::new(IrohTransport::from_seed("test/node-a").await.unwrap());
        let transport_b = Arc::new(IrohTransport::from_seed("test/node-b").await.unwrap());

        let id_a = transport_a.endpoint_id();
        let id_b = transport_b.endpoint_id();

        // Determine which should initiate (lower ID initiates)
        let (initiator, acceptor, acceptor_addr) = if id_a.as_bytes() < id_b.as_bytes() {
            (
                Arc::clone(&transport_a),
                Arc::clone(&transport_b),
                transport_b.endpoint_addr(),
            )
        } else {
            (
                Arc::clone(&transport_b),
                Arc::clone(&transport_a),
                transport_a.endpoint_addr(),
            )
        };

        // Initially no connections
        assert_eq!(initiator.peer_count(), 0);
        assert_eq!(acceptor.peer_count(), 0);

        // Start accept loop on acceptor
        acceptor.start_accept_loop().unwrap();

        // Connect from initiator to acceptor
        let conn = initiator.connect(acceptor_addr).await.unwrap();
        assert!(conn.is_some(), "Connection should be established");

        // Give the connection time to establish fully
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Initiator should have 1 connected peer
        assert_eq!(initiator.peer_count(), 1);

        // Now close acceptor, simulating a peer disconnect
        let _ = acceptor.stop_accept_loop();

        // Close the acceptor connections - this will close the QUIC connection
        for (_id, conn) in acceptor.connections.write().unwrap().drain() {
            conn.close(0u32.into(), b"test_close");
        }

        // Give time for the connection close to propagate
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Now initiator should report 0 peers (Issue #244 fix)
        // Before the fix, this would still return 1 because closed connections weren't removed
        assert_eq!(
            initiator.peer_count(),
            0,
            "Closed connections should be removed from the map"
        );
        assert!(
            initiator.connected_peers().is_empty(),
            "connected_peers() should not include closed connections"
        );

        // Cleanup - drop the Arcs (connections will close automatically)
        drop(transport_a);
        drop(transport_b);
    }

    #[tokio::test]
    async fn test_peer_event_subscription() {
        // Test that we can subscribe to peer events (Issue #275)
        let transport = IrohTransport::new().await.unwrap();

        // Subscribe to events
        let mut rx = transport.subscribe_peer_events();

        // Verify we can receive from the channel (it should timeout since no events yet)
        let result = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "Should timeout when no events");

        transport.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_event_on_connect() {
        // Test that connect emits an event (Issue #275)
        use std::sync::Arc;

        // Use deterministic keys for reliable testing
        let transport = Arc::new(IrohTransport::from_seed("test-event/node-a").await.unwrap());
        let transport2 = Arc::new(IrohTransport::from_seed("test-event/node-b").await.unwrap());
        let transport2_id = transport2.endpoint_id();
        let transport2_addr = transport2.endpoint_addr();

        // Subscribe to events BEFORE connecting
        let mut rx = transport.subscribe_peer_events();

        // Start accept on transport2
        transport2.start_accept_loop().unwrap();

        // Connect transport1 to transport2
        if let Some(_conn) = transport.connect(transport2_addr).await.unwrap() {
            // Give time for event to be emitted
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Should have received a Connected event
            let event =
                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await;
            assert!(event.is_ok(), "Should receive connect event");

            if let Ok(Some(TransportPeerEvent::Connected { endpoint_id, .. })) = event {
                assert_eq!(
                    endpoint_id, transport2_id,
                    "Event should be for connected peer"
                );
            } else {
                panic!("Expected Connected event");
            }
        }

        // Cleanup - just drop the Arcs (connections will close)
        drop(transport);
        drop(transport2);
    }

    #[tokio::test]
    async fn test_multiple_event_subscribers() {
        // Test that multiple subscribers all receive events (Issue #275)
        let transport = IrohTransport::new().await.unwrap();

        // Subscribe twice
        let mut rx1 = transport.subscribe_peer_events();
        let mut rx2 = transport.subscribe_peer_events();

        // Both should be able to receive (and timeout since no events)
        let result1 = tokio::time::timeout(std::time::Duration::from_millis(50), rx1.recv()).await;
        let result2 = tokio::time::timeout(std::time::Duration::from_millis(50), rx2.recv()).await;

        assert!(result1.is_err(), "Subscriber 1 should timeout");
        assert!(result2.is_err(), "Subscriber 2 should timeout");

        transport.close().await.unwrap();
    }

    /// Test that the tactical transport config is applied with correct timeout values (Issue #315, #346)
    ///
    /// This test verifies that the config can be created without panicking.
    /// The actual timeout values are private in quinn, but this ensures:
    /// - The Duration::try_into() for IdleTimeout works correctly
    /// - The config builder methods are called with valid values
    #[test]
    fn test_tactical_transport_config() {
        // This will panic if the config values are invalid (e.g., Duration too large)
        let _config = create_tactical_transport_config();

        // If we get here, the config was created successfully
        // The timeout values are: max_idle_timeout=30s (Issue #346), keep_alive_interval=1s
    }

    /// Test that disconnect is detected within the expected timeout (Issue #315, #346)
    ///
    /// This test verifies that with the idle timeout (30s) and keep-alive (1s),
    /// disconnects are detected faster than the default ~40 seconds.
    #[tokio::test]
    async fn test_fast_disconnect_detection_issue_315() {
        use std::sync::Arc;

        // Use deterministic keys for reliable testing
        let transport_a = Arc::new(IrohTransport::from_seed("test-315/node-a").await.unwrap());
        let transport_b = Arc::new(IrohTransport::from_seed("test-315/node-b").await.unwrap());

        let id_a = transport_a.endpoint_id();
        let id_b = transport_b.endpoint_id();

        // Determine which should initiate (lower ID initiates)
        let (initiator, acceptor, acceptor_addr) = if id_a.as_bytes() < id_b.as_bytes() {
            (
                Arc::clone(&transport_a),
                Arc::clone(&transport_b),
                transport_b.endpoint_addr(),
            )
        } else {
            (
                Arc::clone(&transport_b),
                Arc::clone(&transport_a),
                transport_a.endpoint_addr(),
            )
        };

        // Subscribe to events BEFORE connecting
        let mut events = initiator.subscribe_peer_events();

        // Start accept loop on acceptor
        acceptor.start_accept_loop().unwrap();

        // Connect from initiator to acceptor
        let conn = initiator.connect(acceptor_addr).await.unwrap();
        assert!(conn.is_some(), "Connection should be established");

        // Wait for connection event
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv()).await;
        assert!(event.is_ok(), "Should receive connect event");

        // Give connection time to stabilize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert_eq!(
            initiator.peer_count(),
            1,
            "Should have 1 peer before disconnect"
        );

        // Now close the acceptor abruptly (simulating crash/kill)
        let _ = acceptor.stop_accept_loop();
        // Close all connections without clean shutdown
        for (_id, conn) in acceptor.connections.write().unwrap().drain() {
            conn.close(0u32.into(), b"crash");
        }
        // Force close the endpoint
        drop(acceptor);

        // Start timing - disconnect should be detected within QUIC_MAX_IDLE_TIMEOUT_SECS
        let start = std::time::Instant::now();

        // Wait for disconnect event - should be MUCH faster than the old ~40s
        // With connection.closed() monitor, this should be almost instant when peer closes cleanly
        // Even with abrupt close, it should be within idle timeout (10s)
        let disconnect_timeout = std::time::Duration::from_secs(QUIC_MAX_IDLE_TIMEOUT_SECS + 2);
        let event = tokio::time::timeout(disconnect_timeout, events.recv()).await;

        let elapsed = start.elapsed();

        assert!(
            event.is_ok(),
            "Should receive disconnect event within timeout"
        );

        if let Ok(Some(TransportPeerEvent::Disconnected { reason, .. })) = event {
            tracing::info!(
                elapsed_secs = elapsed.as_secs_f64(),
                reason = %reason,
                "Disconnect detected (Issue #315)"
            );

            // Verify the timing is reasonable
            // With clean close (via connection.close()), it should be nearly instant
            // This test documents the expected behavior with new config
            assert!(
                elapsed.as_secs() <= QUIC_MAX_IDLE_TIMEOUT_SECS + 2,
                "Disconnect should be detected within {} seconds, took {:.1}s (Issue #315)",
                QUIC_MAX_IDLE_TIMEOUT_SECS + 2,
                elapsed.as_secs_f64()
            );
        }

        // Verify peer is removed
        assert_eq!(
            initiator.peer_count(),
            0,
            "Peer count should be 0 after disconnect"
        );

        drop(transport_a);
        drop(transport_b);
    }
}
