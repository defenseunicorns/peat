//! Configuration types for the HiveMesh facade.

use crate::topology::TopologyConfig;
use crate::transport::TransportManagerConfig;
use std::path::PathBuf;
use std::time::Duration;

/// Top-level configuration for HiveMesh.
///
/// Composes topology, discovery, and security settings into a single
/// configuration struct that drives the [`crate::mesh::HiveMesh`] facade.
#[derive(Debug, Clone, Default)]
pub struct MeshConfig {
    /// Node identifier. Auto-generated (UUID v4) if `None`.
    pub node_id: Option<String>,
    /// Optional path for persistent storage.
    pub storage_path: Option<PathBuf>,
    /// Topology formation configuration.
    pub topology: TopologyConfig,
    /// Peer discovery configuration.
    pub discovery: MeshDiscoveryConfig,
    /// Security configuration.
    pub security: SecurityConfig,
    /// Transport manager configuration for multi-transport selection.
    pub transport_manager: Option<TransportManagerConfig>,
}

/// Discovery settings for mesh peer discovery.
#[derive(Debug, Clone)]
pub struct MeshDiscoveryConfig {
    /// Enable mDNS-based peer discovery.
    pub mdns_enabled: bool,
    /// Service name advertised during discovery.
    pub service_name: String,
    /// Discovery broadcast interval.
    pub interval: Duration,
}

impl Default for MeshDiscoveryConfig {
    fn default() -> Self {
        Self {
            mdns_enabled: true,
            service_name: "hive-mesh".to_string(),
            interval: Duration::from_secs(30),
        }
    }
}

/// Security settings for mesh communications.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable encryption for mesh communications.
    pub encryption_enabled: bool,
    /// Require cryptographic peer identity verification.
    pub require_peer_verification: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encryption_enabled: true,
            require_peer_verification: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MeshConfig defaults ──────────────────────────────────────

    #[test]
    fn test_mesh_config_default_node_id_is_none() {
        let cfg = MeshConfig::default();
        assert!(cfg.node_id.is_none());
    }

    #[test]
    fn test_mesh_config_default_storage_path_is_none() {
        let cfg = MeshConfig::default();
        assert!(cfg.storage_path.is_none());
    }

    #[test]
    fn test_mesh_config_default_topology() {
        let cfg = MeshConfig::default();
        // TopologyConfig default reevaluation_interval is 30s
        assert_eq!(
            cfg.topology.reevaluation_interval,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn test_mesh_config_default_discovery() {
        let cfg = MeshConfig::default();
        assert!(cfg.discovery.mdns_enabled);
        assert_eq!(cfg.discovery.service_name, "hive-mesh");
    }

    #[test]
    fn test_mesh_config_default_security() {
        let cfg = MeshConfig::default();
        assert!(cfg.security.encryption_enabled);
        assert!(!cfg.security.require_peer_verification);
    }

    #[test]
    fn test_mesh_config_custom_values() {
        let cfg = MeshConfig {
            node_id: Some("custom-node".to_string()),
            storage_path: Some(PathBuf::from("/tmp/mesh")),
            discovery: MeshDiscoveryConfig {
                mdns_enabled: false,
                service_name: "my-mesh".to_string(),
                interval: Duration::from_secs(10),
            },
            security: SecurityConfig {
                encryption_enabled: false,
                require_peer_verification: true,
            },
            ..Default::default()
        };
        assert_eq!(cfg.node_id.as_deref(), Some("custom-node"));
        assert_eq!(
            cfg.storage_path.as_deref(),
            Some(std::path::Path::new("/tmp/mesh"))
        );
        assert!(!cfg.discovery.mdns_enabled);
        assert_eq!(cfg.discovery.service_name, "my-mesh");
        assert_eq!(cfg.discovery.interval, Duration::from_secs(10));
        assert!(!cfg.security.encryption_enabled);
        assert!(cfg.security.require_peer_verification);
    }

    #[test]
    fn test_mesh_config_clone() {
        let cfg = MeshConfig {
            node_id: Some("cloned".to_string()),
            ..Default::default()
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.node_id, cfg.node_id);
    }

    #[test]
    fn test_mesh_config_debug() {
        let cfg = MeshConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("MeshConfig"));
    }

    // ── MeshDiscoveryConfig defaults ─────────────────────────────

    #[test]
    fn test_discovery_config_default_mdns_enabled() {
        let cfg = MeshDiscoveryConfig::default();
        assert!(cfg.mdns_enabled);
    }

    #[test]
    fn test_discovery_config_default_service_name() {
        let cfg = MeshDiscoveryConfig::default();
        assert_eq!(cfg.service_name, "hive-mesh");
    }

    #[test]
    fn test_discovery_config_default_interval() {
        let cfg = MeshDiscoveryConfig::default();
        assert_eq!(cfg.interval, Duration::from_secs(30));
    }

    #[test]
    fn test_discovery_config_custom() {
        let cfg = MeshDiscoveryConfig {
            mdns_enabled: false,
            service_name: "custom".to_string(),
            interval: Duration::from_secs(5),
        };
        assert!(!cfg.mdns_enabled);
        assert_eq!(cfg.service_name, "custom");
        assert_eq!(cfg.interval, Duration::from_secs(5));
    }

    #[test]
    fn test_discovery_config_clone() {
        let cfg = MeshDiscoveryConfig::default();
        let cloned = cfg.clone();
        assert_eq!(cloned.service_name, cfg.service_name);
    }

    #[test]
    fn test_discovery_config_debug() {
        let cfg = MeshDiscoveryConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("MeshDiscoveryConfig"));
    }

    // ── SecurityConfig defaults ──────────────────────────────────

    #[test]
    fn test_security_config_default_encryption_enabled() {
        let cfg = SecurityConfig::default();
        assert!(cfg.encryption_enabled);
    }

    #[test]
    fn test_security_config_default_peer_verification_disabled() {
        let cfg = SecurityConfig::default();
        assert!(!cfg.require_peer_verification);
    }

    #[test]
    fn test_security_config_custom() {
        let cfg = SecurityConfig {
            encryption_enabled: false,
            require_peer_verification: true,
        };
        assert!(!cfg.encryption_enabled);
        assert!(cfg.require_peer_verification);
    }

    #[test]
    fn test_security_config_clone() {
        let cfg = SecurityConfig {
            encryption_enabled: true,
            require_peer_verification: true,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.encryption_enabled, cfg.encryption_enabled);
        assert_eq!(
            cloned.require_peer_verification,
            cfg.require_peer_verification
        );
    }

    #[test]
    fn test_security_config_debug() {
        let cfg = SecurityConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("SecurityConfig"));
    }
}
