//! Peer discovery strategies for mesh networks
//!
//! This module provides pluggable discovery mechanisms for finding peers:
//! - **StaticDiscovery**: Pre-configured peer list from TOML files
//! - **MdnsDiscovery**: Local network discovery via mDNS/DNS-SD
//! - **HybridDiscovery**: Combines multiple strategies

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::mpsc;

pub mod hybrid;
pub mod mdns;
pub mod static_config;

// Re-export main types for convenience
pub use hybrid::HybridDiscovery;
pub use mdns::MdnsDiscovery;
pub use static_config::{DiscoveryConfig, StaticDiscovery, StaticPeerConfig};

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS error: {0}")]
    MdnsError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Event stream already consumed")]
    EventStreamConsumed,
}

pub type Result<T> = std::result::Result<T, DiscoveryError>;

// Helper function for serde default
fn instant_now() -> Instant {
    Instant::now()
}

/// Information about a discovered peer
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PeerInfo {
    /// Unique identifier for the peer node
    pub node_id: String,

    /// Network addresses where the peer can be reached
    pub addresses: Vec<SocketAddr>,

    /// Optional relay server URL for NAT traversal
    pub relay_url: Option<String>,

    /// When this peer was last seen (not serialized)
    #[serde(skip, default = "instant_now")]
    pub last_seen: Instant,

    /// Additional metadata about the peer
    pub metadata: HashMap<String, String>,
}

impl PeerInfo {
    pub fn new(node_id: String, addresses: Vec<SocketAddr>) -> Self {
        Self {
            node_id,
            addresses,
            relay_url: None,
            last_seen: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_relay(mut self, relay_url: String) -> Self {
        self.relay_url = Some(relay_url);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
}

/// Events emitted by discovery strategies
#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum DiscoveryEvent {
    /// A new peer has been discovered
    PeerFound(PeerInfo),

    /// A previously discovered peer is no longer available
    PeerLost(String), // node_id

    /// A peer's information has been updated
    PeerUpdated(PeerInfo),
}

/// Trait for peer discovery strategies
#[async_trait]
pub trait DiscoveryStrategy: Send + Sync {
    /// Start the discovery process
    async fn start(&mut self) -> Result<()>;

    /// Stop discovery
    async fn stop(&mut self) -> Result<()>;

    /// Get currently discovered peers
    async fn discovered_peers(&self) -> Vec<PeerInfo>;

    /// Subscribe to discovery events
    /// Note: This can only be called once per strategy instance
    fn event_stream(&mut self) -> Result<mpsc::Receiver<DiscoveryEvent>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_info_creation() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let peer = PeerInfo::new("test-node".to_string(), vec![addr])
            .with_relay("https://relay.example.com".to_string())
            .with_metadata("role".to_string(), "squad-leader".to_string());

        assert_eq!(peer.node_id, "test-node");
        assert_eq!(peer.addresses.len(), 1);
        assert_eq!(
            peer.relay_url,
            Some("https://relay.example.com".to_string())
        );
        assert_eq!(peer.metadata.get("role"), Some(&"squad-leader".to_string()));
    }

    #[test]
    fn test_peer_info_no_relay() {
        let addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let peer = PeerInfo::new("node-2".to_string(), vec![addr]);
        assert!(peer.relay_url.is_none());
        assert!(peer.metadata.is_empty());
    }

    #[test]
    fn test_peer_info_update_last_seen() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let mut peer = PeerInfo::new("node-1".to_string(), vec![addr]);
        let before = peer.last_seen;
        std::thread::sleep(std::time::Duration::from_millis(2));
        peer.update_last_seen();
        assert!(peer.last_seen >= before);
    }

    #[test]
    fn test_peer_info_multiple_metadata() {
        let peer = PeerInfo::new("node-1".to_string(), vec![])
            .with_metadata("role".to_string(), "leader".to_string())
            .with_metadata("unit".to_string(), "alpha".to_string());

        assert_eq!(peer.metadata.len(), 2);
        assert_eq!(peer.metadata.get("unit"), Some(&"alpha".to_string()));
    }

    #[test]
    fn test_discovery_error_display() {
        let err = DiscoveryError::MdnsError("timeout".into());
        assert_eq!(err.to_string(), "mDNS error: timeout");

        let err = DiscoveryError::ConfigError("bad toml".into());
        assert_eq!(err.to_string(), "Configuration error: bad toml");

        let err = DiscoveryError::EventStreamConsumed;
        assert_eq!(err.to_string(), "Event stream already consumed");
    }

    #[test]
    fn test_discovery_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err: DiscoveryError = io_err.into();
        assert!(err.to_string().contains("file gone"));
    }

    #[test]
    fn test_discovery_event_variants() {
        let addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let peer = PeerInfo::new("p1".to_string(), vec![addr]);

        let found = DiscoveryEvent::PeerFound(peer.clone());
        let updated = DiscoveryEvent::PeerUpdated(peer);
        let lost = DiscoveryEvent::PeerLost("p1".to_string());

        // Just verify Debug works (no panics)
        let _ = format!("{:?}", found);
        let _ = format!("{:?}", updated);
        let _ = format!("{:?}", lost);
    }

    #[test]
    fn test_peer_info_serialization() {
        let addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let peer = PeerInfo::new("node-1".to_string(), vec![addr])
            .with_relay("https://relay.example.com".to_string());
        let json = serde_json::to_string(&peer).unwrap();
        let deserialized: PeerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.node_id, "node-1");
        assert_eq!(deserialized.addresses.len(), 1);
        assert_eq!(
            deserialized.relay_url,
            Some("https://relay.example.com".to_string())
        );
    }
}
