//! Iroh QUIC transport wrapper for P2P networking
//!
//! This module provides a wrapper around Iroh's Endpoint for Peat Protocol.
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
//! on the same L2 network, bridging hostname:port-style addressing and Iroh's
//! EndpointId-based addressing.
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
use iroh::address_lookup::mdns::MdnsAddressLookup;
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
/// This is transport-agnostic - the same event type can be used for different transports.
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

/// ALPN protocol identifier for Peat Protocol Automerge sync
#[cfg(feature = "automerge-backend")]
pub const CAP_AUTOMERGE_ALPN: &[u8] = b"cap/automerge/1";

// =============================================================================
// QUIC Timeout Configuration (Issue #315)
// =============================================================================

/// Maximum idle timeout for QUIC connections (Issue #315)
///
/// When a peer disconnects unexpectedly (crash, kill, network loss), QUIC detects
/// "dead" connections via idle timeout. The default of ~30 seconds is too slow
/// for tactical radio networks where connections can drop at any time.
///
/// Setting this to 5 seconds provides fast disconnect detection suitable for
/// tactical environments while still allowing for brief network jitter.
///
/// Note: In radio networks, a 5-second silence typically indicates a genuine
/// connection loss, not just temporary congestion.
#[cfg(feature = "automerge-backend")]
pub const QUIC_MAX_IDLE_TIMEOUT_SECS: u64 = 5;

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

/// Create a QuicTransportConfig with optimized timeout settings for tactical applications (Issue #315)
///
/// Key settings:
/// - `max_idle_timeout`: 5 seconds (reduced from default ~30s)
/// - `keep_alive_interval`: 1 second (aggressive connection health monitoring)
///
/// This configuration provides:
/// - Fast disconnect detection (~5 seconds vs ~40 seconds default)
/// - Immediate awareness of connection state changes
/// - Designed for tactical radio networks where connections can drop unexpectedly
#[cfg(feature = "automerge-backend")]
fn create_tactical_transport_config() -> iroh::endpoint::QuicTransportConfig {
    let config = iroh::endpoint::QuicTransportConfig::builder()
        // Set maximum idle timeout for faster disconnect detection
        .max_idle_timeout(Some(
            Duration::from_secs(QUIC_MAX_IDLE_TIMEOUT_SECS)
                .try_into()
                .expect("valid idle timeout duration"),
        ))
        // Enable keep-alive packets to prevent healthy connections
        // from timing out and to detect dead connections faster
        .keep_alive_interval(Duration::from_secs(QUIC_KEEP_ALIVE_INTERVAL_SECS))
        .build();

    tracing::debug!(
        max_idle_timeout_secs = QUIC_MAX_IDLE_TIMEOUT_SECS,
        keep_alive_interval_secs = QUIC_KEEP_ALIVE_INTERVAL_SECS,
        "Created tactical QUIC transport config (Issue #315)"
    );

    config
}

/// Default interval for connection recycling (Issue #435 memory leak workaround)
///
/// Connections older than this are eligible for recycling to mitigate upstream
/// iroh memory leak (iroh#3565). Set to 0 to disable recycling.
#[cfg(feature = "automerge-backend")]
pub const CONNECTION_RECYCLE_INTERVAL_SECS: u64 = 60;

/// Iroh QUIC transport for P2P connections
///
/// Wraps Iroh Endpoint to provide CAP-specific networking.
#[cfg(feature = "automerge-backend")]
pub struct IrohTransport {
    /// Iroh endpoint for QUIC connections
    endpoint: Endpoint,
    /// Active peer connections
    connections: Arc<RwLock<HashMap<EndpointId, Connection>>>,
    /// Connection establishment timestamps (Issue #435 memory leak workaround)
    /// Used to track connection age for periodic recycling
    connection_timestamps: Arc<RwLock<HashMap<EndpointId, std::time::Instant>>>,
    /// Accept loop state
    accept_running: Arc<AtomicBool>,
    /// Accept loop task handle
    accept_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// mDNS discovery (optional, for automatic peer discovery on local network)
    /// Uses interior mutability to support deferred initialization via enable_mdns_discovery()
    mdns_discovery: Arc<RwLock<Option<MdnsAddressLookup>>>,
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
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }

    /// Create a new Iroh transport with local network discovery enabled (Issue #226)
    ///
    /// This is the recommended constructor for containerlab and local network testing.
    /// It enables automatic peer discovery via mDNS-like protocol, bridging hostname:port-style
    /// addressing and Iroh's EndpointId-based addressing.
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
        let discovery = MdnsAddressLookup::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        // Create endpoint with the same secret key and discovery enabled
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .address_lookup(discovery.clone())
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
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(Some(discovery))),
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
        hasher.update(b"peat-iroh-key-v1:"); // Domain separator
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

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
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
        hasher.update(b"peat-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);
        let endpoint_id = secret_key.public();

        // Create mDNS discovery service
        let discovery = MdnsAddressLookup::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        tracing::info!(
            seed = seed,
            endpoint_id = %endpoint_id,
            "Created IrohTransport with deterministic key and mDNS discovery"
        );

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .address_lookup(discovery.clone())
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed with discovery")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(Some(discovery))),
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

        // Derive 32 bytes from seed using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(b"peat-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);
        let endpoint_id = secret_key.public();

        // Create mDNS discovery service
        let discovery = MdnsAddressLookup::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        tracing::info!(
            seed = seed,
            endpoint_id = %endpoint_id,
            bind_addr = %bind_addr,
            "Created IrohTransport with deterministic key, mDNS discovery, and bind address"
        );

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .address_lookup(discovery.clone())
            .bind_addr(bind_addr)
            .context("Invalid bind address")?
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed with discovery at addr")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(Some(discovery))),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }

    /// Create transport with deterministic key and specific bind address, WITHOUT mDNS discovery
    ///
    /// This is the FAST constructor for startup optimization. It creates a fully functional
    /// transport without the overhead of mDNS discovery initialization. Use this when:
    /// - Fast startup time is critical (mobile apps, frequent restarts)
    /// - Peers are discovered via static configuration rather than mDNS
    /// - mDNS discovery will be enabled later via `enable_mdns_discovery()`
    ///
    /// # Performance
    ///
    /// This constructor is significantly faster than `from_seed_with_discovery_at_addr()`
    /// because it skips mDNS service initialization. The mDNS setup involves:
    /// - Creating UDP multicast sockets
    /// - Setting up service advertisement
    /// - Starting background discovery tasks
    ///
    /// # Arguments
    ///
    /// * `seed` - Seed for deterministic key generation (e.g., "app-id/device-uuid")
    /// * `bind_addr` - Socket address to bind to (IPv4 only)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Fast startup without mDNS (use static peer config instead)
    /// let seed = format!("{}/{}", app_id, device_uuid);
    /// let addr = "0.0.0.0:9000".parse()?;
    /// let transport = IrohTransport::from_seed_at_addr(&seed, addr).await?;
    ///
    /// // Later, optionally enable mDNS for automatic LAN discovery
    /// transport.enable_mdns_discovery().await?;
    /// ```
    pub async fn from_seed_at_addr(seed: &str, bind_addr: SocketAddr) -> Result<Self> {
        use sha2::{Digest, Sha256};

        // Derive 32 bytes from seed using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(b"peat-iroh-key-v1:"); // Domain separator
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        // Convert hash to secret key bytes
        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        // Create deterministic secret key
        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);
        let endpoint_id = secret_key.public();

        tracing::info!(
            seed = seed,
            endpoint_id = %endpoint_id,
            bind_addr = %bind_addr,
            "Created IrohTransport with deterministic key (NO mDNS discovery - fast startup)"
        );

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .bind_addr(bind_addr)
            .context("Invalid bind address")?
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint from seed at addr")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }

    /// Enable mDNS discovery after transport creation (deferred discovery)
    ///
    /// This allows fast startup with `from_seed_at_addr()` followed by optional
    /// mDNS discovery enablement. The discovery service is started but not wired
    /// into the QUIC endpoint (which doesn't support dynamic discovery addition).
    ///
    /// # How It Works
    ///
    /// Since Iroh endpoints don't support adding discovery after creation, this method:
    /// 1. Creates an MdnsAddressLookup service for this endpoint's ID
    /// 2. Stores it for later access via `mdns_discovery()`
    /// 3. The caller can subscribe to discovery events and connect to discovered peers
    ///
    /// Note: This is a "manual" discovery mode - discovered peers must be connected
    /// explicitly rather than automatically by the QUIC endpoint.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if mDNS discovery was enabled successfully
    /// - `Err` if discovery is already enabled or creation failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create transport without mDNS (fast)
    /// let transport = IrohTransport::from_seed_at_addr(&seed, addr).await?;
    ///
    /// // ... do critical startup work ...
    ///
    /// // Now enable mDNS for LAN peer discovery (non-blocking, runs in background)
    /// transport.enable_mdns_discovery().await?;
    ///
    /// // Subscribe to discovery events
    /// if let Some(mdns) = transport.mdns_discovery() {
    ///     let mut events = mdns.subscribe().await;
    ///     // Handle discovery events...
    /// }
    /// ```
    pub async fn enable_mdns_discovery(&self) -> Result<()> {
        // Check if already enabled
        {
            let guard = self
                .mdns_discovery
                .read()
                .expect("mdns_discovery lock poisoned");
            if guard.is_some() {
                anyhow::bail!("mDNS discovery is already enabled");
            }
        }

        let endpoint_id = self.endpoint.id();

        // Create mDNS discovery service
        let discovery = MdnsAddressLookup::builder()
            .build(endpoint_id)
            .context("Failed to create mDNS discovery")?;

        tracing::info!(
            endpoint_id = %endpoint_id,
            "Enabled mDNS discovery (deferred initialization)"
        );

        // Store the discovery service for later access
        *self
            .mdns_discovery
            .write()
            .expect("mdns_discovery lock poisoned") = Some(discovery);

        Ok(())
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
        hasher.update(b"peat-iroh-key-v1:"); // Domain separator
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
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .bind_addr(bind_addr)
            .context("Invalid bind address")?
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create Iroh endpoint with bind address")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
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
    /// `Ok(conn)` - Connection (new or existing)
    /// `Err(e)` - Connection failed
    ///
    /// # Connection Conflict Resolution (Issue #346)
    ///
    /// This method ALWAYS attempts to connect. When both peers try to connect to each
    /// other simultaneously (common with mDNS discovery), a conflict occurs.
    ///
    /// **Conflict Resolution Algorithm:**
    /// - When conflict detected (we have outgoing + accept loop has incoming)
    /// - Keep the connection initiated by the peer with LOWER endpoint ID
    /// - Close the other connection
    ///
    /// This approach is **event-driven** (not preemptive):
    /// - We don't guess who should initiate based on ID
    /// - We resolve conflicts when they actually occur
    /// - Works correctly for both static configs AND mDNS discovery
    ///
    /// Previous approach (preemptive tie-breaking) failed for static configs where
    /// only one side has the peer in their config.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(conn))` - New connection, caller should do initiator handshake
    /// - `Ok(None)` - Connection handled by accept path, caller should do nothing
    /// - `Err` - Actual connection error
    pub async fn connect(&self, addr: EndpointAddr) -> Result<Option<Connection>> {
        let remote_id = addr.id;
        let our_id = self.endpoint_id();

        // Check if we already have a live connection to this peer
        {
            let connections = self.connections.read().expect("connections lock poisoned");
            if let Some(existing) = connections.get(&remote_id) {
                if existing.close_reason().is_none() {
                    tracing::debug!(
                        "Already have live connection to {:?}, accept path handling",
                        remote_id
                    );
                    // Connection is being handled by accept path - don't duplicate handshake
                    return Ok(None);
                }
                // Connection exists but is dead - we'll replace it below
            }
        }

        tracing::debug!(
            our_id = %our_id,
            remote_id = %remote_id,
            "Connecting to peer (conflict resolution on detection)"
        );

        let conn = self
            .endpoint
            .connect(addr, CAP_AUTOMERGE_ALPN)
            .await
            .context("Failed to connect to peer")?;

        // Store connection, handling potential conflict with accept loop
        // Issue #346: We always store and return our connection. Conflict resolution
        // happens in accept() if there's a simultaneous connection from the peer.
        // This supports both symmetric (both initiate) and asymmetric (one initiates) cases.
        let mut connections = self.connections.write().expect("connections lock poisoned");
        if let Some(existing) = connections.get(&remote_id) {
            if existing.close_reason().is_none() {
                // Accept loop already stored a connection from this peer.
                // Resolve conflict: keep connection initiated by LOWER endpoint ID.
                let we_are_lower = our_id.as_bytes() < remote_id.as_bytes();
                if we_are_lower {
                    // We have lower ID - keep OUR outgoing connection, close theirs
                    tracing::info!(
                        remote_id = %remote_id,
                        our_id = %our_id,
                        "Conflict resolved in connect(): we have lower ID, closing their incoming connection"
                    );
                    if let Some(old) = connections.remove(&remote_id) {
                        // Use code 100 for "connect path conflict resolution"
                        old.close(100u32.into(), b"conflict_connect_lower_wins");
                    }
                } else {
                    // They have lower ID - keep THEIR connection, don't store ours
                    // Return None to indicate accept path is handling
                    tracing::info!(
                        remote_id = %remote_id,
                        our_id = %our_id,
                        "Conflict resolved in connect(): they have lower ID, closing our outgoing connection"
                    );
                    // Use code 101 for "connect path yielding to accept"
                    conn.close(101u32.into(), b"conflict_connect_yield");
                    return Ok(None);
                }
            }
        }

        connections.insert(remote_id, conn.clone());
        drop(connections); // Release lock before emitting event

        // Track connection timestamp for recycling (Issue #435 workaround)
        self.connection_timestamps
            .write()
            .expect("connection_timestamps lock poisoned")
            .insert(remote_id, std::time::Instant::now());

        // NOTE: Connected event is NOT emitted here (Issue #346).
        // The caller must call emit_peer_connected() AFTER successful handshake
        // to prevent sync handlers from racing with the handshake protocol.

        // Spawn connection close monitor for instant disconnect detection (Issue #275)
        self.spawn_connection_monitor(remote_id, conn.clone());

        Ok(Some(conn))
    }

    /// Emit the Connected event for a peer after successful handshake.
    ///
    /// This must be called after the formation handshake succeeds to notify
    /// sync handlers that the connection is ready for use.
    ///
    /// # Issue #346 Fix
    ///
    /// Previously, Connected was emitted immediately when the connection was stored,
    /// which caused sync handlers to race with the handshake. This led to sync
    /// streams being opened before authentication completed, causing handshake failures.
    pub fn emit_peer_connected(&self, endpoint_id: EndpointId) {
        self.emit_event(TransportPeerEvent::Connected {
            endpoint_id,
            connected_at: std::time::Instant::now(),
        });
    }

    /// Connect to a peer using PeerInfo from static configuration
    ///
    /// # Arguments
    ///
    /// * `peer` - PeerInfo with node_id and direct addresses
    ///
    /// # Returns
    ///
    /// - `Ok(Some(conn))` - New connection, caller should do initiator handshake
    /// - `Ok(None)` - Connection handled by accept path, caller should do nothing
    /// - `Err(e)` - Connection failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let peer = config.get_peer("node-1").unwrap();
    /// if let Some(conn) = transport.connect_peer(peer).await? {
    ///     // Do initiator handshake on new connection
    ///     perform_initiator_handshake(&conn, &key).await?;
    /// }
    /// // If None, accept path is handling the handshake
    /// ```
    pub async fn connect_peer(&self, peer: &PeerInfo) -> Result<Option<Connection>> {
        let endpoint_id = peer.endpoint_id()?;
        let socket_addrs = peer.socket_addrs()?;

        // Create EndpointAddr with direct addresses
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
    /// - `Ok(Some(conn))` - New connection, caller should do initiator handshake
    /// - `Ok(None)` - Connection handled by accept path, caller should do nothing
    /// - `Err(e)` - Connection failed
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
    /// if let Some(conn) = transport.connect_by_id(peer_endpoint_id).await? {
    ///     // Do initiator handshake
    /// }
    /// ```
    pub async fn connect_by_id(&self, endpoint_id: EndpointId) -> Result<Option<Connection>> {
        // Create EndpointAddr with just the ID - discovery should have provided addresses
        let addr = EndpointAddr::new(endpoint_id);

        tracing::debug!(
            peer_id = %endpoint_id,
            "Connecting to peer by ID (using discovery-resolved addresses)"
        );

        self.connect(addr).await
    }

    /// Check if mDNS discovery is enabled
    pub fn has_discovery(&self) -> bool {
        self.mdns_discovery
            .read()
            .expect("mdns_discovery lock poisoned")
            .is_some()
    }

    /// Get a clone of the mDNS discovery service (Issue #233)
    ///
    /// This allows subscribing to mDNS discovery events to learn about newly
    /// discovered peers on the local network. The returned discovery service
    /// has a `subscribe()` method that returns a stream of `DiscoveryEvent`.
    ///
    /// # Returns
    ///
    /// `Some(MdnsAddressLookup)` if mDNS discovery is enabled, `None` otherwise.
    /// The discovery service is cloned (Arc internally) so this is cheap.
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
    pub fn mdns_discovery(&self) -> Option<MdnsAddressLookup> {
        self.mdns_discovery
            .read()
            .expect("mdns_discovery lock poisoned")
            .clone()
    }

    /// Accept an incoming connection
    ///
    /// This is a blocking call that waits for the next incoming connection.
    ///
    /// # Returns
    ///
    /// `Ok(Some(conn))` - A new connection that needs authentication
    /// `Ok(None)` - Connection was rejected due to conflict resolution or transient error
    /// `Err(e)` - An error occurred (endpoint closed)
    ///
    /// # Connection Conflict Resolution (Issue #346)
    ///
    /// When accepting a connection, there may be a conflict with an outgoing connection
    /// attempt (race condition when both peers try to connect simultaneously).
    ///
    /// **Conflict Resolution Algorithm:**
    /// - If existing connection to this peer: conflict detected
    /// - Keep connection initiated by peer with LOWER endpoint ID
    /// - Close the other connection
    ///
    /// Example scenarios:
    /// - We (ID=HIGH) accept from peer (ID=LOW): Peer's incoming wins (they're lower)
    /// - We (ID=LOW) accept from peer (ID=HIGH): Our outgoing wins (we're lower)
    ///
    /// # Error Handling (Issue #346)
    ///
    /// - Returns `Ok(None)` for transient errors (failed QUIC handshake, conflict rejection)
    /// - Returns `Err` only when the endpoint is closed (accept loop should stop)
    ///
    /// This ensures the accept loop survives transient network issues.
    pub async fn accept(&self) -> Result<Option<Connection>> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .context("Endpoint closed - no more incoming connections")?;

        // Issue #346: Handle transient errors gracefully
        let conn = match incoming.await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Incoming connection failed during QUIC handshake (transient, continuing)"
                );
                return Ok(None);
            }
        };

        let remote_id = conn.remote_id();
        let our_id = self.endpoint_id();

        let mut connections = self.connections.write().expect("connections lock poisoned");

        // Check for existing connection (conflict detection)
        if let Some(existing) = connections.get(&remote_id) {
            if existing.close_reason().is_none() {
                // Conflict: we have an outgoing connection AND incoming
                // Resolve: keep connection initiated by LOWER endpoint ID
                let they_are_lower = remote_id.as_bytes() < our_id.as_bytes();

                if they_are_lower {
                    // They have lower ID - they should be initiator
                    // This incoming connection IS from them initiating - keep it
                    tracing::info!(
                        our_id = %our_id,
                        remote_id = %remote_id,
                        "Conflict resolved in accept(): they have lower ID, closing our outgoing connection"
                    );
                    if let Some(old) = connections.remove(&remote_id) {
                        // Use code 102 for "accept path closing outgoing"
                        old.close(102u32.into(), b"conflict_accept_closing_outgoing");
                    }
                } else {
                    // We have lower ID - our outgoing connection should be kept
                    // Reject this incoming connection
                    tracing::info!(
                        our_id = %our_id,
                        remote_id = %remote_id,
                        "Conflict resolved in accept(): we have lower ID, rejecting incoming connection"
                    );
                    // Use code 103 for "accept path rejecting incoming"
                    conn.close(103u32.into(), b"conflict_accept_reject_incoming");
                    drop(connections);
                    return Ok(None);
                }
            }
        }

        // Store and return the new connection
        connections.insert(remote_id, conn.clone());
        drop(connections);

        // Track connection timestamp for recycling (Issue #435 workaround)
        self.connection_timestamps
            .write()
            .expect("connection_timestamps lock poisoned")
            .insert(remote_id, std::time::Instant::now());

        // NOTE: Connected event is NOT emitted here (Issue #346).
        // The caller must call emit_peer_connected() AFTER successful handshake
        // to prevent sync handlers from racing with the handshake protocol.

        // Spawn connection close monitor for instant disconnect detection (Issue #275)
        self.spawn_connection_monitor(remote_id, conn.clone());

        Ok(Some(conn))
    }

    /// Get an existing connection to a peer
    pub fn get_connection(&self, endpoint_id: &EndpointId) -> Option<Connection> {
        self.connections
            .read()
            .expect("connections lock poisoned")
            .get(endpoint_id)
            .cloned()
    }

    /// Disconnect from a peer
    pub fn disconnect(&self, endpoint_id: &EndpointId) -> Result<()> {
        // Remove timestamp tracking (Issue #435 workaround)
        self.connection_timestamps
            .write()
            .expect("connection_timestamps lock poisoned")
            .remove(endpoint_id);

        if let Some(conn) = self
            .connections
            .write()
            .expect("connections lock poisoned")
            .remove(endpoint_id)
        {
            conn.close(0u32.into(), b"disconnecting");
            // Emit disconnect event (Issue #275)
            self.emit_event(TransportPeerEvent::Disconnected {
                endpoint_id: *endpoint_id,
                reason: "local disconnect".to_string(),
            });
        }
        Ok(())
    }

    /// Get connections older than the specified duration (Issue #435 workaround)
    ///
    /// Returns a list of EndpointIds for connections that have been established
    /// longer than `max_age`. These connections are candidates for recycling
    /// to mitigate the upstream iroh memory leak (iroh#3565).
    ///
    /// # Arguments
    ///
    /// * `max_age` - Maximum connection age before it becomes eligible for recycling
    ///
    /// # Returns
    ///
    /// Vector of EndpointIds for connections older than `max_age`
    pub fn connections_older_than(&self, max_age: Duration) -> Vec<EndpointId> {
        let now = std::time::Instant::now();
        let timestamps = self
            .connection_timestamps
            .read()
            .expect("connection_timestamps lock poisoned");
        timestamps
            .iter()
            .filter_map(|(id, &connected_at)| {
                if now.duration_since(connected_at) > max_age {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Recycle old connections to mitigate memory leak (Issue #435 workaround)
    ///
    /// Disconnects all connections older than `max_age`. The reconnection manager
    /// will automatically re-establish connections to static-config peers.
    ///
    /// # Arguments
    ///
    /// * `max_age` - Maximum connection age before recycling
    ///
    /// # Returns
    ///
    /// Number of connections recycled
    pub fn recycle_old_connections(&self, max_age: Duration) -> usize {
        let old_connections = self.connections_older_than(max_age);
        let count = old_connections.len();

        for endpoint_id in old_connections {
            tracing::info!(
                peer_id = %endpoint_id,
                "Recycling connection to mitigate memory leak (Issue #435)"
            );
            let _ = self.disconnect(&endpoint_id);
        }

        if count > 0 {
            tracing::info!(
                count = count,
                max_age_secs = max_age.as_secs(),
                "Recycled old connections (Issue #435 memory leak workaround)"
            );
        }

        count
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
        self.event_senders
            .write()
            .expect("event_senders lock poisoned")
            .push(tx);
        rx
    }

    /// Emit a peer event to all subscribers (Issue #275)
    ///
    /// Called internally when connections are established or closed.
    fn emit_event(&self, event: TransportPeerEvent) {
        let senders = self
            .event_senders
            .read()
            .expect("event_senders lock poisoned");
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
        // Store the stable_id to verify we're removing the right connection
        let monitored_stable_id = conn.stable_id();

        self.runtime_handle.spawn(async move {
            // Wait for the connection to close (this completes immediately when closed)
            let close_reason = conn.closed().await;

            tracing::info!(
                ?endpoint_id,
                ?close_reason,
                "Connection closed, emitting disconnect event"
            );

            // Remove from connections map ONLY if it's the same connection we were monitoring.
            // This prevents a race where conflict resolution replaces a connection:
            // 1. connect() stores conn A and spawns monitor for A
            // 2. accept() removes A, closes it, inserts conn B
            // 3. Monitor for A wakes up - must NOT remove B!
            let should_emit_disconnect;
            {
                let mut conns = connections.write().expect("connections lock poisoned");
                if let Some(current_conn) = conns.get(&endpoint_id) {
                    if current_conn.stable_id() == monitored_stable_id {
                        // Same connection - safe to remove
                        conns.remove(&endpoint_id);
                        should_emit_disconnect = true;
                    } else {
                        // Different connection replaced ours - don't remove!
                        tracing::debug!(
                            ?endpoint_id,
                            monitored_id = monitored_stable_id,
                            current_id = current_conn.stable_id(),
                            "Connection was replaced, not removing"
                        );
                        should_emit_disconnect = false;
                    }
                } else {
                    // Already removed - someone else cleaned it up
                    should_emit_disconnect = false;
                }
            }

            // Only emit disconnect if we actually removed this connection
            if should_emit_disconnect {
                let reason = format!("{:?}", close_reason);
                let event = TransportPeerEvent::Disconnected {
                    endpoint_id,
                    reason,
                };
                let senders = event_senders.read().expect("event_senders lock poisoned");
                for sender in senders.iter() {
                    let _ = sender.try_send(event.clone());
                }
            }
        });
    }

    /// Get the number of currently connected peers
    ///
    /// Only counts connections that are still alive (not closed).
    /// Automatically cleans up closed connections from the map.
    pub fn peer_count(&self) -> usize {
        self.cleanup_closed_connections();
        self.connections
            .read()
            .expect("connections lock poisoned")
            .len()
    }

    /// Get all currently connected peer IDs
    ///
    /// Only returns connections that are still alive (not closed).
    /// Automatically cleans up closed connections from the map.
    pub fn connected_peers(&self) -> Vec<EndpointId> {
        self.cleanup_closed_connections();
        self.connections
            .read()
            .expect("connections lock poisoned")
            .keys()
            .copied()
            .collect()
    }

    /// Remove closed connections from the connections map
    ///
    /// Called automatically by `peer_count()` and `connected_peers()`.
    /// Can also be called explicitly to clean up stale connections.
    /// Emits disconnect events for removed connections (Issue #275).
    pub fn cleanup_closed_connections(&self) {
        // Collect closed connections to emit events after releasing lock
        let closed_peers: Vec<(EndpointId, String)> = {
            let mut connections = self.connections.write().expect("connections lock poisoned");
            let mut closed = Vec::new();

            connections.retain(|endpoint_id, conn| {
                if let Some(reason) = conn.close_reason() {
                    // Issue #346: Enhanced diagnostic logging for connection closures
                    // Parse the close reason to identify the source
                    let reason_str = format!("{:?}", reason);
                    let close_source = if reason_str.contains("100")
                        || reason_str.contains("conflict_connect_lower_wins")
                    {
                        "connect() conflict resolution (we had lower ID)"
                    } else if reason_str.contains("101")
                        || reason_str.contains("conflict_connect_yield")
                    {
                        "connect() yielding to accept path"
                    } else if reason_str.contains("102")
                        || reason_str.contains("conflict_accept_closing_outgoing")
                    {
                        "accept() closing our outgoing connection"
                    } else if reason_str.contains("103")
                        || reason_str.contains("conflict_accept_reject_incoming")
                    {
                        "accept() rejecting incoming connection"
                    } else if reason_str.contains("authentication") {
                        "authentication failure"
                    } else if reason_str.contains("TimedOut") {
                        "QUIC idle timeout (no keep-alives received)"
                    } else if reason_str.contains("LocallyClosed") {
                        "local close (unknown source)"
                    } else {
                        "other"
                    };

                    tracing::warn!(
                        endpoint_id = %endpoint_id,
                        reason = %reason_str,
                        close_source = %close_source,
                        "[CLEANUP] Removing closed connection"
                    );
                    closed.push((*endpoint_id, reason_str));
                    false
                } else {
                    true
                }
            });

            closed
        };

        // Clean up timestamps for removed connections (Issue #435 workaround)
        if !closed_peers.is_empty() {
            let mut timestamps = self
                .connection_timestamps
                .write()
                .expect("connection_timestamps lock poisoned");
            for (endpoint_id, _) in &closed_peers {
                timestamps.remove(endpoint_id);
            }
        }

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

        *self.accept_task.write().expect("accept_task lock poisoned") = Some(task);

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
        for (_endpoint_id, conn) in self
            .connections
            .write()
            .expect("connections lock poisoned")
            .drain()
        {
            conn.close(0u32.into(), b"shutdown");
        }

        // Close endpoint
        self.endpoint.close().await;

        Ok(())
    }
}

// Implement SyncTransport trait for IrohTransport (peat-mesh abstraction)
#[cfg(feature = "automerge-backend")]
#[async_trait::async_trait]
impl peat_mesh::storage::sync_transport::SyncTransport for IrohTransport {
    fn get_connection(&self, peer_id: &EndpointId) -> Option<Connection> {
        self.get_connection(peer_id)
    }

    fn connected_peers(&self) -> Vec<EndpointId> {
        self.connected_peers()
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
impl IrohTransport {
    /// Create a transport for local-only testing (no relay servers, no DNS discovery).
    ///
    /// Uses `RelayMode::Disabled` so tests don't depend on external infrastructure.
    pub(crate) async fn new_local() -> Result<Self> {
        let endpoint = Endpoint::empty_builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create local-only Iroh endpoint")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }

    /// Create a transport with deterministic key for local-only testing.
    ///
    /// Like `from_seed` but uses `RelayMode::Disabled` so tests don't depend on
    /// external relay servers.
    pub(crate) async fn from_seed_local(seed: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(b"peat-iroh-key-v1:");
        hasher.update(seed.as_bytes());
        let hash = hasher.finalize();

        let mut seed_bytes = [0u8; 32];
        seed_bytes.copy_from_slice(&hash);

        let secret_key = iroh::SecretKey::from_bytes(&seed_bytes);

        let endpoint = Endpoint::empty_builder()
            .alpns(vec![CAP_AUTOMERGE_ALPN.to_vec()])
            .secret_key(secret_key)
            .transport_config(create_tactical_transport_config())
            .bind()
            .await
            .context("Failed to create local-only Iroh endpoint from seed")?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_timestamps: Arc::new(RwLock::new(HashMap::new())),
            accept_running: Arc::new(AtomicBool::new(false)),
            accept_task: Arc::new(RwLock::new(None)),
            mdns_discovery: Arc::new(RwLock::new(None)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
            runtime_handle: tokio::runtime::Handle::current(),
        })
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use serial_test::serial;

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
    #[serial]
    async fn test_stale_peer_cleanup_issue_244() {
        use std::sync::Arc;

        // Use deterministic keys
        let transport_a = Arc::new(IrohTransport::from_seed_local("test/node-a").await.unwrap());
        let transport_b = Arc::new(IrohTransport::from_seed_local("test/node-b").await.unwrap());

        // Either side can initiate now (conflict resolution handles races)
        let acceptor_addr = transport_b.endpoint_addr();

        // Initially no connections
        assert_eq!(transport_a.peer_count(), 0);
        assert_eq!(transport_b.peer_count(), 0);

        // Start accept loop on transport_b
        transport_b.start_accept_loop().unwrap();

        // Connect from transport_a to transport_b
        let _conn = transport_a.connect(acceptor_addr).await.unwrap();

        // Give the connection time to establish fully
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // transport_a should have 1 connected peer
        assert_eq!(transport_a.peer_count(), 1);

        // Now close transport_b, simulating a peer disconnect
        let _ = transport_b.stop_accept_loop();

        // Close the transport_b connections - this will close the QUIC connection
        for (_id, conn) in transport_b.connections.write().unwrap().drain() {
            conn.close(0u32.into(), b"test_close");
        }

        // Give time for the connection close to propagate
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Now transport_a should report 0 peers (Issue #244 fix)
        assert_eq!(
            transport_a.peer_count(),
            0,
            "Closed connections should be removed from the map"
        );
        assert!(
            transport_a.connected_peers().is_empty(),
            "connected_peers() should not include closed connections"
        );

        // Cleanup
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
    #[serial]
    async fn test_peer_event_on_connect() {
        // Test that emit_peer_connected emits an event (Issue #275, #346)
        // Note: Since Issue #346, Connected events are only emitted AFTER handshake
        // by calling emit_peer_connected(). This test simulates that flow.
        use std::sync::Arc;

        // Use deterministic keys for reliable testing
        let transport = Arc::new(
            IrohTransport::from_seed_local("test-event/node-a")
                .await
                .unwrap(),
        );
        let transport2 = Arc::new(
            IrohTransport::from_seed_local("test-event/node-b")
                .await
                .unwrap(),
        );
        let transport2_id = transport2.endpoint_id();
        let transport2_addr = transport2.endpoint_addr();

        // Subscribe to events BEFORE connecting
        let mut rx = transport.subscribe_peer_events();

        // Start accept on transport2
        transport2.start_accept_loop().unwrap();

        // Connect transport1 to transport2
        let conn = transport.connect(transport2_addr).await.unwrap();
        assert!(conn.is_some(), "Should get connection in asymmetric case");

        // Issue #346: Connected event is only emitted after handshake.
        // For this test, we simulate handshake success by calling it manually.
        transport.emit_peer_connected(transport2_id);

        // Give time for event to be emitted
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Should have received a Connected event
        let event = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await;
        assert!(event.is_ok(), "Should receive connect event");

        if let Ok(Some(TransportPeerEvent::Connected { endpoint_id, .. })) = event {
            assert_eq!(
                endpoint_id, transport2_id,
                "Event should be for connected peer"
            );
        } else {
            panic!("Expected Connected event");
        }

        // Cleanup
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

    /// Test that the tactical transport config is applied with correct timeout values (Issue #315)
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
        // The timeout values are: max_idle_timeout=5s, keep_alive_interval=1s
    }

    /// Test that disconnect is detected within the expected timeout (Issue #315)
    ///
    /// This test verifies that with the reduced idle timeout (5s) and keep-alive (1s),
    /// disconnects are detected much faster than the default ~30-40 seconds.
    #[tokio::test]
    #[serial]
    async fn test_fast_disconnect_detection_issue_315() {
        use std::sync::Arc;

        // Use deterministic keys for reliable testing
        let transport_a = Arc::new(
            IrohTransport::from_seed_local("test-315/node-a")
                .await
                .unwrap(),
        );
        let transport_b = Arc::new(
            IrohTransport::from_seed_local("test-315/node-b")
                .await
                .unwrap(),
        );
        let transport_b_id = transport_b.endpoint_id();

        let acceptor_addr = transport_b.endpoint_addr();

        // Subscribe to events BEFORE connecting
        let mut events = transport_a.subscribe_peer_events();

        // Start accept loop on transport_b
        transport_b.start_accept_loop().unwrap();

        // Connect from transport_a to transport_b
        let conn = transport_a.connect(acceptor_addr).await.unwrap();
        assert!(conn.is_some(), "Should get connection in asymmetric case");

        // Issue #346: Connected event is only emitted after handshake.
        // For this test, we simulate handshake success by calling it manually.
        transport_a.emit_peer_connected(transport_b_id);

        // Wait for connection event
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv()).await;
        assert!(event.is_ok(), "Should receive connect event");

        // Give connection time to stabilize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert_eq!(
            transport_a.peer_count(),
            1,
            "Should have 1 peer before disconnect"
        );

        // Now close transport_b abruptly (simulating crash/kill)
        let _ = transport_b.stop_accept_loop();
        // Close all connections without clean shutdown
        for (_id, conn) in transport_b.connections.write().unwrap().drain() {
            conn.close(0u32.into(), b"crash");
        }
        // Force close the endpoint
        drop(transport_b);

        // Start timing - disconnect should be detected within QUIC_MAX_IDLE_TIMEOUT_SECS
        let start = std::time::Instant::now();

        // Wait for disconnect event - should be MUCH faster than the old ~40s
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

            assert!(
                elapsed.as_secs() <= QUIC_MAX_IDLE_TIMEOUT_SECS + 2,
                "Disconnect should be detected within {} seconds, took {:.1}s (Issue #315)",
                QUIC_MAX_IDLE_TIMEOUT_SECS + 2,
                elapsed.as_secs_f64()
            );
        }

        // Verify peer is removed
        assert_eq!(
            transport_a.peer_count(),
            0,
            "Peer count should be 0 after disconnect"
        );

        drop(transport_a);
    }
}
