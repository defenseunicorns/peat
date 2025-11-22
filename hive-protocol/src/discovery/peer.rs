//! Peer Discovery Strategies for Automerge+Iroh Backend (ADR-011 Phase 3)
//!
//! This module implements automatic peer discovery for HIVE Protocol nodes using
//! multiple strategies:
//!
//! - **mDNS Discovery**: Zero-config discovery on local networks
//! - **Static Configuration**: Pre-configured peer lists (TOML files)
//! - **Relay Discovery**: Discovery via Iroh relay servers (future)
//! - **Hybrid Manager**: Coordinates all strategies and merges results
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │      DiscoveryManager                   │
//! │  (Coordinates multiple strategies)      │
//! └──────────┬──────────┬───────────────────┘
//!            │          │
//!    ┌───────┴──┐  ┌────┴─────┐  ┌──────────┐
//!    │  mDNS    │  │  Static  │  │  Relay   │
//!    │Discovery │  │Discovery │  │Discovery │
//!    └──────────┘  └──────────┘  └──────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_protocol::discovery::peer::*;
//!
//! // Create discovery manager
//! let mut manager = DiscoveryManager::new();
//!
//! // Add static config strategy
//! let static_disc = StaticDiscovery::from_file("peers.toml")?;
//! manager.add_strategy(Box::new(static_disc));
//!
//! // Start discovery
//! manager.start().await?;
//!
//! // Get discovered peers
//! let peers = manager.get_peers().await;
//! ```

use async_trait::async_trait;
use iroh::{Endpoint, EndpointId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// Re-export PeerInfo from network module
pub use crate::network::peer_config::{PeerConfig, PeerInfo};

/// Discovery event emitted when peers are found or lost
#[derive(Clone, Debug)]
pub enum DiscoveryEvent {
    /// New peer discovered
    PeerFound(PeerInfo),
    /// Peer lost/offline
    PeerLost(EndpointId),
}

/// Trait for discovery strategies
#[async_trait]
pub trait DiscoveryStrategy: Send + Sync {
    /// Start the discovery process
    async fn start(&mut self) -> anyhow::Result<()>;

    /// Get all currently discovered peers
    async fn discovered_peers(&self) -> Vec<PeerInfo>;

    /// Subscribe to discovery events
    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent>;
}

/// Static peer configuration from TOML files
///
/// Loads pre-configured peer lists from `peers.toml` files. Useful for:
/// - EMCON (emission control) mode where broadcasting is disabled
/// - Known peer sets in tactical environments
/// - Fallback when mDNS is unavailable
pub struct StaticDiscovery {
    peers: Vec<PeerInfo>,
}

impl StaticDiscovery {
    /// Load peers from a TOML configuration file
    ///
    /// Expected format:
    /// ```toml
    /// [[peers]]
    /// name = "Node Alpha"
    /// node_id = "abc123..."
    /// addresses = ["192.168.100.10:5000"]
    /// relay_url = "https://relay.tactical.mil:3479"
    /// ```
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config = PeerConfig::from_file(path)?;
        Ok(Self {
            peers: config.peers,
        })
    }

    /// Create from in-memory peer list
    pub fn from_peers(peers: Vec<PeerInfo>) -> Self {
        Self { peers }
    }
}

#[async_trait]
impl DiscoveryStrategy for StaticDiscovery {
    async fn start(&mut self) -> anyhow::Result<()> {
        tracing::info!(
            "Static: Loaded {} peers from configuration",
            self.peers.len()
        );
        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.peers.clone()
    }

    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent> {
        // Static peers don't change, so return an empty channel
        let (_, rx) = mpsc::channel(1);
        rx
    }
}

/// mDNS-based discovery for zero-config local network peer discovery
///
/// Advertises this node's presence on the local network and discovers other HIVE nodes.
/// Uses the service type `_hive-node._tcp.local` for discovery.
///
/// # Service Advertisement Format
///
/// Each node advertises via mDNS with:
/// - **Service Type**: `_hive-node._tcp.local`
/// - **Instance Name**: `<node-name>._hive-node._tcp.local`
/// - **TXT Records**:
///   - `node_id=<hex-encoded-endpoint-id>` (32-byte Iroh PublicKey)
///   - `version=1` (Protocol version)
/// - **Port**: Iroh endpoint port (auto-assigned)
///
/// # Example
///
/// ```ignore
/// let mdns = MdnsDiscovery::new(endpoint, "UAV-Alpha".to_string())?;
/// manager.add_strategy(Box::new(mdns));
/// ```
pub struct MdnsDiscovery {
    /// Local node's Iroh endpoint
    endpoint: Endpoint,
    /// Local node's friendly name
    node_name: String,
    /// Discovered peers (EndpointId -> PeerInfo)
    discovered: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
    /// Event channel sender
    event_tx: Arc<RwLock<Option<mpsc::Sender<DiscoveryEvent>>>>,
    /// mDNS service daemon handle
    mdns_service: Arc<RwLock<Option<mdns_sd::ServiceDaemon>>>,
}

impl MdnsDiscovery {
    /// Service type for HIVE Protocol nodes on mDNS
    const SERVICE_TYPE: &'static str = "_hive-node._tcp.local.";

    /// Create a new mDNS discovery instance
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Iroh endpoint for this node
    /// * `node_name` - Human-readable name for this node (used in mDNS instance name)
    pub fn new(endpoint: Endpoint, node_name: String) -> anyhow::Result<Self> {
        Ok(Self {
            endpoint,
            node_name,
            discovered: Arc::new(RwLock::new(HashMap::new())),
            event_tx: Arc::new(RwLock::new(None)),
            mdns_service: Arc::new(RwLock::new(None)),
        })
    }

    /// Get local IP address for mDNS advertisement
    fn get_local_ip() -> anyhow::Result<String> {
        use std::net::UdpSocket;

        // Connect to a public DNS server to determine our local interface IP
        // This doesn't actually send any data, just determines routing
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("8.8.8.8:80")?;
        let addr = socket.local_addr()?;
        Ok(addr.ip().to_string())
    }

    /// Stop mDNS discovery and unregister service
    ///
    /// This gracefully shuts down the mDNS daemon, unregistering the service
    /// and stopping browse operations. Reduces multicast traffic when discovery
    /// is no longer needed.
    pub async fn stop(&mut self) {
        // Dropping the ServiceDaemon will automatically unregister and stop browsing
        let _ = self.mdns_service.write().await.take();
        tracing::info!("mDNS: Stopped discovery and unregistered service");
    }
}

#[async_trait]
impl DiscoveryStrategy for MdnsDiscovery {
    async fn start(&mut self) -> anyhow::Result<()> {
        use mdns_sd::{ServiceDaemon, ServiceInfo};
        use std::collections::HashMap as StdHashMap;

        tracing::info!("mDNS: Starting zero-config discovery for local network");

        // Create mDNS service daemon (single daemon for both register and browse)
        let mdns = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

        // Get local endpoint info
        let endpoint_id = self.endpoint.id();
        let node_id_hex = hex::encode(endpoint_id.as_bytes());

        // Note: Iroh uses QUIC with hole punching and relay servers, so we don't
        // advertise a specific port. The actual connectivity is handled by Iroh's
        // endpoint_id. We use port 0 to indicate automatic port assignment.
        let port = 0;

        // Create TXT properties
        let mut properties = StdHashMap::new();
        properties.insert("node_id".to_string(), node_id_hex.clone());
        properties.insert("version".to_string(), "1".to_string());

        // Get local IP address for mDNS advertisement
        let local_ip = Self::get_local_ip().unwrap_or_else(|_| "127.0.0.1".to_string());

        tracing::debug!("mDNS: Using local IP address: {}", local_ip);

        // Create service info
        // ServiceInfo::new(service_type, instance_name, host_name, host_ipv4, port, properties)
        // host_name must end with ".local."
        let host_name = format!("{}.local.", self.node_name);
        let service_info = ServiceInfo::new(
            Self::SERVICE_TYPE,
            &self.node_name,
            &host_name,
            &local_ip,
            port,
            properties,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create mDNS service info: {}", e))?;

        // Register service
        mdns.register(service_info)
            .map_err(|e| anyhow::anyhow!("Failed to register mDNS service: {}", e))?;

        tracing::info!(
            "mDNS: Advertised node '{}' with ID {} on port {}",
            self.node_name,
            &node_id_hex[..16],
            port
        );

        // Give registration a moment to propagate before browsing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Browse for other HIVE nodes using the same daemon
        let receiver = mdns
            .browse(Self::SERVICE_TYPE)
            .map_err(|e| anyhow::anyhow!("Failed to browse mDNS services: {}", e))?;

        tracing::debug!(
            "mDNS: Started browsing for service type: {}",
            Self::SERVICE_TYPE
        );

        // Store mdns daemon (must stay alive for both registration and browsing)
        *self.mdns_service.write().await = Some(mdns);

        // Create event channel
        let (tx, _) = mpsc::channel(100);
        *self.event_tx.write().await = Some(tx.clone());

        // Spawn background task to process mDNS events
        let discovered = Arc::clone(&self.discovered);
        let event_tx = tx;

        tokio::spawn(async move {
            use mdns_sd::ServiceEvent;

            tracing::debug!("mDNS: Background task started, listening for service events");

            while let Ok(event) = receiver.recv_async().await {
                tracing::debug!("mDNS: Received event: {:?}", event);
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        tracing::info!("mDNS: Service resolved: {}", info.get_fullname());
                        // Extract node_id from TXT records
                        if let Some(node_id_hex) = info.get_property_val_str("node_id") {
                            tracing::debug!("mDNS: Found node_id in TXT record: {}", node_id_hex);
                            // Decode hex to bytes
                            match hex::decode(node_id_hex) {
                                Ok(node_id_bytes) => {
                                    tracing::debug!(
                                        "mDNS: Decoded node_id, length: {}",
                                        node_id_bytes.len()
                                    );
                                    if node_id_bytes.len() == 32 {
                                        // Convert to EndpointId
                                        let mut array = [0u8; 32];
                                        array.copy_from_slice(&node_id_bytes);

                                        match EndpointId::from_bytes(&array) {
                                            Ok(endpoint_id) => {
                                                tracing::debug!(
                                                    "mDNS: Successfully created EndpointId"
                                                );
                                                // Extract addresses
                                                let addresses: Vec<String> = info
                                                    .get_addresses()
                                                    .iter()
                                                    .map(|addr| {
                                                        format!("{}:{}", addr, info.get_port())
                                                    })
                                                    .collect();

                                                let peer_info = PeerInfo {
                                                    name: info.get_fullname().to_string(),
                                                    node_id: node_id_hex.to_string(),
                                                    addresses: addresses.clone(),
                                                    relay_url: None,
                                                };

                                                // Add to discovered peers
                                                let mut peers = discovered.write().await;
                                                peers.insert(endpoint_id, peer_info.clone());
                                                let total_peers = peers.len();
                                                drop(peers);

                                                // Emit discovery event
                                                let _ = event_tx
                                                    .send(DiscoveryEvent::PeerFound(peer_info))
                                                    .await;

                                                tracing::info!(
                                                    "mDNS: Discovered peer '{}' at {:?} (total peers: {})",
                                                    info.get_fullname(),
                                                    addresses,
                                                    total_peers
                                                );
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "mDNS: Failed to create EndpointId: {}",
                                                    e
                                                );
                                            }
                                        }
                                    } else {
                                        tracing::warn!(
                                            "mDNS: node_id wrong length: {} bytes, expected 32",
                                            node_id_bytes.len()
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("mDNS: Failed to decode node_id hex: {}", e);
                                }
                            }
                        } else {
                            tracing::debug!("mDNS: No node_id property found in TXT records");
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        // Find and remove peer by fullname
                        let mut peers = discovered.write().await;
                        if let Some((endpoint_id, _)) = peers
                            .iter()
                            .find(|(_, p)| p.name == fullname)
                            .map(|(k, v)| (*k, v.clone()))
                        {
                            peers.remove(&endpoint_id);
                            drop(peers);

                            let _ = event_tx.send(DiscoveryEvent::PeerLost(endpoint_id)).await;

                            tracing::info!("mDNS: Peer '{}' left the network", fullname);
                        }
                    }
                    other_event => {
                        // Log other events for debugging
                        tracing::debug!("mDNS: Received event (ignored): {:?}", other_event);
                    }
                }
            }
            tracing::warn!("mDNS: Background task ended - receiver closed");
        });

        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.discovered.read().await.values().cloned().collect()
    }

    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent> {
        // Return a receiver for events
        // Note: This is a simplified implementation - in production you'd want
        // to support multiple subscribers
        let (_tx, rx) = mpsc::channel(100);

        // Clone events from the main event channel
        // For now, just return an empty receiver - events are handled internally
        rx
    }
}

/// Relay-based discovery using Iroh's relay servers
///
/// Discovers peers that are reachable via relay servers. Useful for:
/// - Cross-network discovery (peers not on same LAN)
/// - NAT traversal scenarios
/// - Fallback connectivity
pub struct RelayDiscovery {
    _endpoint: Endpoint,
    discovered: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
}

impl RelayDiscovery {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            _endpoint: endpoint,
            discovered: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl DiscoveryStrategy for RelayDiscovery {
    async fn start(&mut self) -> anyhow::Result<()> {
        tracing::info!("Relay: Starting relay-based discovery");
        // TODO: Query Iroh for relay-known peers
        // This depends on Iroh's discovery API
        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.discovered.read().await.values().cloned().collect()
    }

    fn event_stream(&self) -> mpsc::Receiver<DiscoveryEvent> {
        let (_, rx) = mpsc::channel(100);
        rx
    }
}

/// Hybrid discovery manager that coordinates multiple strategies
///
/// Aggregates peers from all configured discovery strategies and deduplicates
/// based on EndpointId.
pub struct DiscoveryManager {
    strategies: Vec<Box<dyn DiscoveryStrategy>>,
    all_peers: Arc<RwLock<HashMap<EndpointId, PeerInfo>>>,
}

impl DiscoveryManager {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            all_peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a discovery strategy
    pub fn add_strategy(&mut self, strategy: Box<dyn DiscoveryStrategy>) {
        self.strategies.push(strategy);
    }

    /// Start all discovery strategies
    pub async fn start(&mut self) -> anyhow::Result<()> {
        for strategy in &mut self.strategies {
            strategy.start().await?;
        }

        // Start background task to periodically merge peer lists
        self.update_peers().await;

        Ok(())
    }

    /// Update aggregated peer list from all strategies
    ///
    /// This should be called before querying discovered peers to ensure
    /// the latest peers from all strategies are included.
    pub async fn update_peers(&self) {
        let mut all = self.all_peers.write().await;

        for strategy in &self.strategies {
            for peer in strategy.discovered_peers().await {
                // Parse EndpointId from hex string
                if let Ok(endpoint_id) = peer.endpoint_id() {
                    all.insert(endpoint_id, peer);
                }
            }
        }
    }

    /// Get all discovered peers by querying all strategies
    ///
    /// This queries each strategy's cache directly, avoiding redundant aggregation.
    /// Strategies maintain their caches asynchronously, so this is a fast read.
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let mut all_peers = HashMap::new();

        for strategy in &self.strategies {
            for peer in strategy.discovered_peers().await {
                // Use EndpointId as key to deduplicate peers across strategies
                if let Ok(endpoint_id) = peer.endpoint_id() {
                    all_peers.insert(endpoint_id, peer);
                }
            }
        }

        all_peers.into_values().collect()
    }

    /// Get all discovered peers (alias for get_peers for backward compatibility)
    pub async fn discovered_peers(&self) -> anyhow::Result<Vec<PeerInfo>> {
        Ok(self.get_peers().await)
    }

    /// Get number of discovered peers
    pub async fn peer_count(&self) -> usize {
        self.get_peers().await.len()
    }
}

impl Default for DiscoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_static_discovery() {
        let peer = PeerInfo {
            name: "Test Node".to_string(),
            node_id: "a".repeat(64), // 32 bytes hex
            addresses: vec!["192.168.1.100:5000".to_string()],
            relay_url: None,
        };

        let mut discovery = StaticDiscovery::from_peers(vec![peer.clone()]);
        discovery.start().await.unwrap();

        let peers = discovery.discovered_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].name, "Test Node");
    }

    #[tokio::test]
    async fn test_discovery_manager() {
        let peer1 = PeerInfo {
            name: "Node 1".to_string(),
            node_id: "a".repeat(64),
            addresses: vec!["192.168.1.1:5000".to_string()],
            relay_url: None,
        };

        let peer2 = PeerInfo {
            name: "Node 2".to_string(),
            node_id: "b".repeat(64),
            addresses: vec!["192.168.1.2:5000".to_string()],
            relay_url: None,
        };

        let mut manager = DiscoveryManager::new();
        manager.add_strategy(Box::new(StaticDiscovery::from_peers(vec![peer1, peer2])));

        manager.start().await.unwrap();

        let peers = manager.get_peers().await;
        assert_eq!(peers.len(), 2);
    }

    #[tokio::test]
    async fn test_mdns_service_registration() {
        // Test that mDNS service can be created and registered
        let endpoint = iroh::Endpoint::builder()
            .bind()
            .await
            .expect("Failed to create endpoint");

        let mut mdns = MdnsDiscovery::new(endpoint, "test-node".to_string())
            .expect("Failed to create mDNS discovery");

        // Start should succeed
        mdns.start().await.expect("Failed to start mDNS discovery");

        // Give it a moment to register
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Service should be started (we can't easily test discovery without a second instance)
        // But at least we know it doesn't crash
    }
}
