use super::{DiscoveryError, DiscoveryEvent, DiscoveryStrategy, PeerInfo, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

/// Configuration for a static peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticPeerConfig {
    /// Unique node identifier
    pub node_id: String,

    /// Network addresses where peer can be reached
    pub addresses: Vec<String>,

    /// Optional relay URL for NAT traversal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,

    /// Connection priority (0-255, higher = more important)
    #[serde(default = "default_priority")]
    pub priority: u8,

    /// Additional metadata about the peer
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_priority() -> u8 {
    128
}

/// Top-level discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// List of static peers to discover
    pub peers: Vec<StaticPeerConfig>,
}

/// Static configuration-based peer discovery
///
/// This strategy loads peer information from a configuration file
/// and immediately reports all peers as discovered. Useful for:
/// - EMCON (Emission Control) operations where broadcasting is not allowed
/// - Pre-planned network topologies
/// - Testing and development
pub struct StaticDiscovery {
    config: DiscoveryConfig,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
    events_rx: Option<mpsc::Receiver<DiscoveryEvent>>,
    started: Arc<RwLock<bool>>,
}

impl StaticDiscovery {
    /// Create a new static discovery from a configuration file
    pub fn from_file(path: &Path) -> Result<Self> {
        let config_str = std::fs::read_to_string(path).map_err(|e| {
            DiscoveryError::ConfigError(format!("Failed to read config file: {}", e))
        })?;

        let config: DiscoveryConfig = toml::from_str(&config_str).map_err(|e| {
            DiscoveryError::ConfigError(format!("Failed to parse TOML config: {}", e))
        })?;

        Self::from_config(config)
    }

    /// Create a new static discovery from a configuration object
    pub fn from_config(config: DiscoveryConfig) -> Result<Self> {
        let (events_tx, events_rx) = mpsc::channel(100);

        Ok(Self {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            events_rx: Some(events_rx),
            started: Arc::new(RwLock::new(false)),
        })
    }

    /// Parse addresses from string format
    fn parse_addresses(address_strs: &[String]) -> Vec<SocketAddr> {
        address_strs
            .iter()
            .filter_map(|s| {
                s.parse::<SocketAddr>()
                    .map_err(|e| {
                        debug!("Failed to parse address '{}': {}", s, e);
                        e
                    })
                    .ok()
            })
            .collect()
    }

    /// Convert a StaticPeerConfig to PeerInfo
    fn config_to_peer_info(peer_config: &StaticPeerConfig) -> Option<PeerInfo> {
        let addresses = Self::parse_addresses(&peer_config.addresses);

        if addresses.is_empty() {
            debug!("Skipping peer {} - no valid addresses", peer_config.node_id);
            return None;
        }

        Some(PeerInfo {
            node_id: peer_config.node_id.clone(),
            addresses,
            relay_url: peer_config.relay_url.clone(),
            last_seen: std::time::Instant::now(),
            metadata: peer_config.metadata.clone(),
        })
    }
}

#[async_trait]
impl DiscoveryStrategy for StaticDiscovery {
    async fn start(&mut self) -> Result<()> {
        let mut started = self.started.write().await;
        if *started {
            info!("Static discovery already started");
            return Ok(());
        }

        info!(
            "Starting static discovery with {} configured peers",
            self.config.peers.len()
        );

        let mut peers = self.peers.write().await;

        for peer_config in &self.config.peers {
            if let Some(peer_info) = Self::config_to_peer_info(peer_config) {
                let node_id = peer_info.node_id.clone();

                info!(
                    "Discovered static peer: {} at {:?} (priority: {})",
                    node_id, peer_info.addresses, peer_config.priority
                );

                peers.insert(node_id.clone(), peer_info.clone());

                // Notify about discovered peer
                if let Err(e) = self
                    .events_tx
                    .send(DiscoveryEvent::PeerFound(peer_info))
                    .await
                {
                    debug!("Failed to send discovery event: {}", e);
                }
            }
        }

        *started = true;

        info!("Static discovery started with {} peers", peers.len());

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let mut started = self.started.write().await;
        if !*started {
            return Ok(());
        }

        info!("Stopping static discovery");
        *started = false;

        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }

    fn event_stream(&mut self) -> Result<mpsc::Receiver<DiscoveryEvent>> {
        self.events_rx
            .take()
            .ok_or(DiscoveryError::EventStreamConsumed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> DiscoveryConfig {
        DiscoveryConfig {
            peers: vec![
                StaticPeerConfig {
                    node_id: "hq-alpha".to_string(),
                    addresses: vec![
                        "10.0.0.100:5000".to_string(),
                        "192.168.1.100:5000".to_string(),
                    ],
                    relay_url: Some("https://relay.example.com:3479".to_string()),
                    priority: 255,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("role".to_string(), "company-hq".to_string());
                        m
                    },
                },
                StaticPeerConfig {
                    node_id: "platoon-1".to_string(),
                    addresses: vec!["10.0.1.50:5000".to_string()],
                    relay_url: None,
                    priority: 200,
                    metadata: HashMap::new(),
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_static_discovery_basic() {
        let config = create_test_config();
        let mut discovery = StaticDiscovery::from_config(config).unwrap();

        // Get event stream before starting
        let mut events = discovery.event_stream().unwrap();

        // Start discovery
        discovery.start().await.unwrap();

        // Should discover 2 peers
        let peers = discovery.discovered_peers().await;
        assert_eq!(peers.len(), 2);

        // Check first peer
        let hq = peers.iter().find(|p| p.node_id == "hq-alpha").unwrap();
        assert_eq!(hq.addresses.len(), 2);
        assert_eq!(
            hq.relay_url,
            Some("https://relay.example.com:3479".to_string())
        );

        // Check events
        let event1 = events.try_recv().unwrap();
        let event2 = events.try_recv().unwrap();

        assert!(matches!(event1, DiscoveryEvent::PeerFound(_)));
        assert!(matches!(event2, DiscoveryEvent::PeerFound(_)));
    }

    #[tokio::test]
    async fn test_parse_addresses() {
        let addresses = vec![
            "10.0.0.1:5000".to_string(),
            "invalid".to_string(),
            "192.168.1.1:8080".to_string(),
        ];

        let parsed = StaticDiscovery::parse_addresses(&addresses);
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_toml_serialization() {
        let config = create_test_config();
        let toml_str = toml::to_string(&config).unwrap();

        let parsed: DiscoveryConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.peers.len(), 2);
        assert_eq!(parsed.peers[0].node_id, "hq-alpha");
    }

    #[tokio::test]
    async fn test_static_discovery_stop_when_not_started() {
        let config = create_test_config();
        let mut discovery = StaticDiscovery::from_config(config).unwrap();
        // Stopping when not started is a no-op
        discovery.stop().await.unwrap();
        assert!(!*discovery.started.read().await);
    }

    #[tokio::test]
    async fn test_static_discovery_start_twice_idempotent() {
        let config = create_test_config();
        let mut discovery = StaticDiscovery::from_config(config).unwrap();
        let _events = discovery.event_stream().unwrap();

        discovery.start().await.unwrap();
        let peers_after_first = discovery.discovered_peers().await;

        // Starting again should be idempotent
        discovery.start().await.unwrap();
        let peers_after_second = discovery.discovered_peers().await;

        assert_eq!(peers_after_first.len(), peers_after_second.len());
    }

    #[tokio::test]
    async fn test_static_discovery_event_stream_consumed() {
        let config = create_test_config();
        let mut discovery = StaticDiscovery::from_config(config).unwrap();

        let _stream = discovery.event_stream().unwrap();
        // Second call should fail
        let result = discovery.event_stream();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DiscoveryError::EventStreamConsumed
        ));
    }

    #[test]
    fn test_config_to_peer_info_no_valid_addresses() {
        let peer_config = StaticPeerConfig {
            node_id: "bad-peer".to_string(),
            addresses: vec!["not-a-socket-addr".to_string()],
            relay_url: None,
            priority: 128,
            metadata: HashMap::new(),
        };

        let result = StaticDiscovery::config_to_peer_info(&peer_config);
        assert!(result.is_none());
    }

    #[test]
    fn test_config_to_peer_info_with_relay() {
        let peer_config = StaticPeerConfig {
            node_id: "relay-peer".to_string(),
            addresses: vec!["10.0.0.1:5000".to_string()],
            relay_url: Some("https://relay.example.com".to_string()),
            priority: 200,
            metadata: {
                let mut m = HashMap::new();
                m.insert("role".to_string(), "hq".to_string());
                m
            },
        };

        let peer_info = StaticDiscovery::config_to_peer_info(&peer_config).unwrap();
        assert_eq!(peer_info.node_id, "relay-peer");
        assert_eq!(
            peer_info.relay_url,
            Some("https://relay.example.com".to_string())
        );
        assert_eq!(peer_info.metadata.get("role"), Some(&"hq".to_string()));
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = StaticDiscovery::from_file(std::path::Path::new("/nonexistent/path.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_default_priority() {
        assert_eq!(default_priority(), 128);
    }

    #[test]
    fn test_static_peer_config_serde() {
        let config = StaticPeerConfig {
            node_id: "node-1".to_string(),
            addresses: vec!["10.0.0.1:5000".to_string()],
            relay_url: None,
            priority: 128,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        // relay_url should be skipped when None
        assert!(!json.contains("relay_url"));

        let parsed: StaticPeerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node_id, "node-1");
        assert_eq!(parsed.priority, 128);
    }
}
