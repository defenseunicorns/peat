//! Static peer configuration for Automerge+Iroh mesh networking
//!
//! This module provides TOML-based static peer configuration for establishing
//! a mesh network without requiring relay servers or mDNS discovery.
//!
//! # Phase 6.1 Implementation
//!
//! Simple static mesh configuration for testing and small deployments.
//! Production deployments will add relay and mDNS in Phase 7.
//!
//! # Example Configuration
//!
//! ```toml
//! [local]
//! bind_address = "127.0.0.1:9000"
//!
//! # Formation key for shared secret authentication (similar to Ditto SharedKey)
//! [formation]
//! id = "alpha-company"
//! shared_key = "base64-encoded-32-byte-key"
//!
//! [[peers]]
//! name = "node-1"
//! node_id = "6eb2a534751444f1353b29aa307c78c1f72acfbb06bb8696103dfeede1f4f854"
//! addresses = ["127.0.0.1:9001"]
//! ```

#[cfg(feature = "automerge-backend")]
use crate::security::FormationKey;
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "automerge-backend")]
use std::net::SocketAddr;
#[cfg(feature = "automerge-backend")]
use std::path::Path;

/// Static peer mesh configuration
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Optional local node configuration
    #[serde(default)]
    pub local: LocalConfig,
    /// Optional formation configuration for shared secret authentication
    #[serde(default)]
    pub formation: Option<FormationConfig>,
    /// List of static peers to connect to
    #[serde(default)]
    pub peers: Vec<PeerInfo>,
}

/// Formation configuration for shared secret authentication
///
/// Similar to Ditto's SharedKey identity - all nodes in the formation
/// must have the same formation ID and shared key to sync.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationConfig {
    /// Formation identifier (e.g., "alpha-company")
    pub id: String,
    /// Base64-encoded 32-byte shared secret
    pub shared_key: String,
}

/// Local node configuration
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    /// Bind address for this node (e.g., "127.0.0.1:9000" or "0.0.0.0:0")
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    /// Optional: Override node ID (hex-encoded PublicKey)
    pub node_id: Option<String>,
}

/// Static peer information
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Human-readable peer name
    pub name: String,
    /// Iroh PublicKey (EndpointId) in hex format
    pub node_id: String,
    /// Direct addresses to try connecting to
    pub addresses: Vec<String>,
    /// Optional relay URL (Phase 7)
    pub relay_url: Option<String>,
}

#[cfg(feature = "automerge-backend")]
fn default_bind_address() -> String {
    "0.0.0.0:0".to_string()
}

#[cfg(feature = "automerge-backend")]
impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            node_id: None,
        }
    }
}

#[cfg(feature = "automerge-backend")]
impl PeerConfig {
    /// Load peer configuration from a TOML file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to TOML configuration file
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = PeerConfig::from_file("peers.toml")?;
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents =
            std::fs::read_to_string(path.as_ref()).context("Failed to read peer config file")?;
        Self::from_toml(&contents)
    }

    /// Parse peer configuration from TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str).context("Failed to parse TOML peer config")
    }

    /// Create an empty configuration
    pub fn empty() -> Self {
        Self {
            local: LocalConfig::default(),
            formation: None,
            peers: Vec::new(),
        }
    }

    /// Get the formation key if configured
    ///
    /// Returns `None` if no formation is configured, or an error if the
    /// shared key is invalid.
    pub fn formation_key(&self) -> Result<Option<FormationKey>> {
        match &self.formation {
            Some(config) => {
                let key = FormationKey::from_base64(&config.id, &config.shared_key)
                    .map_err(|e| anyhow::anyhow!("Invalid formation key: {}", e))?;
                Ok(Some(key))
            }
            None => Ok(None),
        }
    }

    /// Check if formation authentication is required
    pub fn requires_formation_auth(&self) -> bool {
        self.formation.is_some()
    }

    /// Parse bind address as SocketAddr
    pub fn bind_socket_addr(&self) -> Result<SocketAddr> {
        self.local
            .bind_address
            .parse()
            .context("Invalid bind address")
    }

    /// Get peer by name
    pub fn get_peer(&self, name: &str) -> Option<&PeerInfo> {
        self.peers.iter().find(|p| p.name == name)
    }
}

#[cfg(feature = "automerge-backend")]
impl PeerInfo {
    /// Parse node_id as EndpointId
    pub fn endpoint_id(&self) -> Result<EndpointId> {
        // Decode hex string to bytes
        let bytes = hex::decode(&self.node_id).context("Failed to decode node_id hex")?;

        // EndpointId is a 32-byte PublicKey
        if bytes.len() != 32 {
            anyhow::bail!(
                "Invalid node_id length: expected 32 bytes, got {}",
                bytes.len()
            );
        }

        // Convert Vec<u8> to [u8; 32]
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);

        // Convert bytes to EndpointId
        EndpointId::from_bytes(&array).context("Failed to construct EndpointId from bytes")
    }

    /// Parse addresses as SocketAddr list
    pub fn socket_addrs(&self) -> Result<Vec<SocketAddr>> {
        self.addresses
            .iter()
            .map(|addr| {
                addr.parse()
                    .with_context(|| format!("Invalid address: {}", addr))
            })
            .collect()
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_config() {
        let config = PeerConfig::from_toml("").unwrap();
        assert_eq!(config.peers.len(), 0);
        assert_eq!(config.local.bind_address, "0.0.0.0:0");
    }

    #[test]
    fn test_parse_local_config() {
        let toml = r#"
            [local]
            bind_address = "127.0.0.1:9000"
        "#;

        let config = PeerConfig::from_toml(toml).unwrap();
        assert_eq!(config.local.bind_address, "127.0.0.1:9000");
        assert_eq!(config.bind_socket_addr().unwrap().port(), 9000);
    }

    #[test]
    fn test_parse_peers() {
        let toml = r#"
            [[peers]]
            name = "node-1"
            node_id = "6eb2a534751444f1353b29aa307c78c1f72acfbb06bb8696103dfeede1f4f854"
            addresses = ["127.0.0.1:9001"]

            [[peers]]
            name = "node-2"
            node_id = "b654917328aea8ccfae00463d63642eb4904bd276fecb4caf94dd740a76b5567"
            addresses = ["127.0.0.1:9002", "192.168.1.100:9002"]
        "#;

        let config = PeerConfig::from_toml(toml).unwrap();
        assert_eq!(config.peers.len(), 2);

        let peer1 = &config.peers[0];
        assert_eq!(peer1.name, "node-1");
        assert_eq!(peer1.addresses.len(), 1);

        let peer2 = &config.peers[1];
        assert_eq!(peer2.name, "node-2");
        assert_eq!(peer2.addresses.len(), 2);

        // Test SocketAddr parsing
        let addrs = peer2.socket_addrs().unwrap();
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].port(), 9002);
    }

    #[test]
    fn test_endpoint_id_parsing() {
        let peer = PeerInfo {
            name: "test".to_string(),
            node_id: "6eb2a534751444f1353b29aa307c78c1f72acfbb06bb8696103dfeede1f4f854".to_string(),
            addresses: vec![],
            relay_url: None,
        };

        let endpoint_id = peer.endpoint_id().unwrap();
        // Verify it's 32 bytes
        assert_eq!(endpoint_id.as_bytes().len(), 32);
    }

    #[test]
    fn test_get_peer_by_name() {
        let toml = r#"
            [[peers]]
            name = "alice"
            node_id = "6eb2a534751444f1353b29aa307c78c1f72acfbb06bb8696103dfeede1f4f854"
            addresses = ["127.0.0.1:9001"]

            [[peers]]
            name = "bob"
            node_id = "b654917328aea8ccfae00463d63642eb4904bd276fecb4caf94dd740a76b5567"
            addresses = ["127.0.0.1:9002"]
        "#;

        let config = PeerConfig::from_toml(toml).unwrap();

        assert!(config.get_peer("alice").is_some());
        assert!(config.get_peer("bob").is_some());
        assert!(config.get_peer("charlie").is_none());
    }

    #[test]
    fn test_parse_formation_config() {
        // Generate a valid base64 secret for testing
        let secret = FormationKey::generate_secret();

        let toml = format!(
            r#"
            [formation]
            id = "alpha-company"
            shared_key = "{}"

            [local]
            bind_address = "127.0.0.1:9000"
        "#,
            secret
        );

        let config = PeerConfig::from_toml(&toml).unwrap();

        assert!(config.formation.is_some());
        let formation = config.formation.as_ref().unwrap();
        assert_eq!(formation.id, "alpha-company");
        assert!(config.requires_formation_auth());
    }

    #[test]
    fn test_formation_key_creation() {
        let secret = FormationKey::generate_secret();

        let toml = format!(
            r#"
            [formation]
            id = "bravo-team"
            shared_key = "{}"
        "#,
            secret
        );

        let config = PeerConfig::from_toml(&toml).unwrap();
        let key = config.formation_key().unwrap();

        assert!(key.is_some());
        assert_eq!(key.unwrap().formation_id(), "bravo-team");
    }

    #[test]
    fn test_no_formation_config() {
        let config = PeerConfig::empty();

        assert!(config.formation.is_none());
        assert!(!config.requires_formation_auth());
        assert!(config.formation_key().unwrap().is_none());
    }

    #[test]
    fn test_invalid_formation_key() {
        let toml = r#"
            [formation]
            id = "test"
            shared_key = "not-valid-base64!!!"
        "#;

        let config = PeerConfig::from_toml(toml).unwrap();
        assert!(config.formation_key().is_err());
    }
}
