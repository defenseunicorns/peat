//! TAK transport configuration types

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

/// TAK Transport Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakTransportConfig {
    /// Transport mode (Server, MeshSa, or Hybrid)
    pub mode: TakTransportMode,

    /// Client identity for authentication
    pub identity: Option<TakIdentity>,

    /// Message queue configuration (DIL resilience)
    pub queue: QueueConfig,

    /// Reconnection policy
    pub reconnect: ReconnectPolicy,

    /// Protocol options
    pub protocol: ProtocolConfig,

    /// Enable metrics collection
    pub metrics_enabled: bool,
}

impl Default for TakTransportConfig {
    fn default() -> Self {
        Self {
            mode: TakTransportMode::TakServer {
                address: "127.0.0.1:8087".parse().unwrap(),
                use_tls: false,
            },
            identity: None,
            queue: QueueConfig::default(),
            reconnect: ReconnectPolicy::default(),
            protocol: ProtocolConfig::default(),
            metrics_enabled: true,
        }
    }
}

/// Transport mode selection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TakTransportMode {
    /// TAK Server TCP connection
    TakServer {
        /// Server address (host:port)
        address: SocketAddr,
        /// Use SSL/TLS
        use_tls: bool,
    },

    /// Mesh SA UDP multicast
    MeshSa {
        /// Multicast group address
        multicast_group: IpAddr,
        /// Port (typically 6969)
        port: u16,
        /// Network interface to bind (None = default)
        interface: Option<String>,
    },

    /// Dual mode: TAK Server primary, Mesh SA fallback
    Hybrid {
        /// Primary TAK Server config
        server_address: SocketAddr,
        /// Use TLS for server connection
        server_use_tls: bool,
        /// Mesh SA multicast group
        mesh_group: IpAddr,
        /// Mesh SA port
        mesh_port: u16,
    },
}

/// Client identity for TAK authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakIdentity {
    /// Client certificate path (PEM or DER)
    pub client_cert: PathBuf,

    /// Client private key path (PEM or DER)
    pub client_key: PathBuf,

    /// CA certificate for server verification
    pub ca_cert: Option<PathBuf>,

    /// Callsign for TAK identification
    pub callsign: String,

    /// TAK user credentials (alternative to cert auth)
    pub credentials: Option<TakCredentials>,
}

/// TAK user credentials for password authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakCredentials {
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version (XML or Protobuf)
    pub version: TakProtocolVersion,

    /// CoT XML encoding options
    pub xml_options: XmlEncodingOptions,

    /// Maximum message size (bytes)
    pub max_message_size: usize,

    /// Heartbeat interval for connection health
    #[serde(with = "humantime_serde")]
    pub heartbeat_interval: Duration,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: TakProtocolVersion::default(),
            xml_options: XmlEncodingOptions::default(),
            max_message_size: 64 * 1024, // 64 KB
            heartbeat_interval: Duration::from_secs(30),
        }
    }
}

/// TAK Protocol version selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TakProtocolVersion {
    /// Raw XML over TCP (no framing)
    /// Payload: <xml_payload>
    /// Use for: FreeTAKServer, simple integrations
    RawXml,

    /// CoT XML over TCP (legacy, Version 0)
    /// Header: 0xbf 0x00 0xbf <xml_payload>
    /// Use for: debugging, legacy TAK Server compatibility
    XmlTcp,

    /// TAK Protocol v1 (Protobuf) - PREFERRED
    /// Header: 0xbf 0x01 0xbf <protobuf_payload> (Mesh SA)
    /// Header: 0xbf <varint_length> <protobuf_payload> (TAK Server TCP)
    ///
    /// Protobuf is 3-5x smaller than XML, critical for DIL/tactical networks.
    #[default]
    ProtobufV1,
}

/// XML encoding options for CoT messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlEncodingOptions {
    /// Include XML declaration (<?xml version="1.0"?>)
    pub xml_declaration: bool,

    /// Pretty print XML (development only)
    pub pretty_print: bool,

    /// Include Peat extension by default
    pub include_peat_extension: bool,
}

impl Default for XmlEncodingOptions {
    fn default() -> Self {
        Self {
            xml_declaration: false,
            pretty_print: false,
            include_peat_extension: true,
        }
    }
}

/// Message queue configuration for DIL resilience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Maximum queue size (messages)
    pub max_messages: usize,

    /// Maximum queue size (bytes)
    pub max_bytes: usize,

    /// Per-priority queue limits
    pub priority_limits: PriorityQueueLimits,

    /// Filter out stale messages on dequeue
    pub filter_stale: bool,

    /// Persist queue to survive restarts
    pub persistent: bool,

    /// Persistence path (if persistent=true)
    pub persistence_path: Option<PathBuf>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_messages: 1000,
            max_bytes: 10 * 1024 * 1024, // 10 MB
            priority_limits: PriorityQueueLimits::default(),
            filter_stale: true,
            persistent: false,
            persistence_path: None,
        }
    }
}

/// Per-priority queue limits
///
/// Higher priorities get more queue space and are drained first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueLimits {
    /// P1 (Critical): Always accepted, drained first
    pub p1_limit: usize,
    /// P2 (High): High priority
    pub p2_limit: usize,
    /// P3 (Normal): Standard limit
    pub p3_limit: usize,
    /// P4 (Low): Reduced limit
    pub p4_limit: usize,
    /// P5 (Bulk): Minimal limit, first to drop
    pub p5_limit: usize,
}

impl Default for PriorityQueueLimits {
    fn default() -> Self {
        Self {
            p1_limit: 200, // 20% - never dropped
            p2_limit: 300, // 30%
            p3_limit: 250, // 25%
            p4_limit: 150, // 15%
            p5_limit: 100, // 10% - first to drop
        }
    }
}

impl PriorityQueueLimits {
    /// Get limit for a given priority level (1-5)
    pub fn limit_for(&self, priority: u8) -> usize {
        match priority {
            1 => self.p1_limit,
            2 => self.p2_limit,
            3 => self.p3_limit,
            4 => self.p4_limit,
            _ => self.p5_limit,
        }
    }
}

/// Reconnection behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectPolicy {
    /// Enable automatic reconnection
    pub enabled: bool,

    /// Initial retry delay
    #[serde(with = "humantime_serde")]
    pub initial_delay: Duration,

    /// Maximum retry delay (exponential backoff cap)
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,

    /// Backoff multiplier
    pub backoff_multiplier: f64,

    /// Maximum reconnection attempts (None = unlimited)
    pub max_attempts: Option<usize>,

    /// Jitter factor (0.0 - 1.0)
    pub jitter: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
            jitter: 0.1,
        }
    }
}

// Helper module for Duration serialization
mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TakTransportConfig::default();
        assert!(config.metrics_enabled);
        assert!(config.reconnect.enabled);
        assert_eq!(config.queue.max_messages, 1000);
    }

    #[test]
    fn test_priority_limits() {
        let limits = PriorityQueueLimits::default();
        assert_eq!(limits.limit_for(1), 200);
        assert_eq!(limits.limit_for(5), 100);
        assert_eq!(limits.limit_for(99), 100); // Unknown falls to P5
    }

    #[test]
    fn test_protocol_version_default() {
        assert_eq!(
            TakProtocolVersion::default(),
            TakProtocolVersion::ProtobufV1
        );
    }
}
