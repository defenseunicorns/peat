//! Transport abstraction for mesh topology connections
//!
//! This module provides backend-agnostic types and traits for establishing
//! P2P connections in the mesh network. It enables `TopologyManager` and
//! related components to work with any transport backend.
//!
//! ## Core Types
//!
//! - **NodeId**: Mesh network node identifier
//! - **MeshTransport**: Connection establishment and management trait
//! - **MeshConnection**: Active connection to a peer trait
//! - **PeerEvent**: Connection lifecycle events

use async_trait::async_trait;
use std::error::Error as StdError;
use std::fmt;
use std::time::Instant;
use tokio::sync::mpsc;

// Submodules moved from hive-protocol (ADR-049 Phase 2)
pub mod bypass;
pub mod capabilities;
pub mod health;
pub mod manager;
pub mod reconnection;

#[cfg(feature = "lite-transport")]
pub mod lite;

#[cfg(feature = "bluetooth")]
pub mod btle;

// Re-exports from submodules
pub use bypass::{
    BypassChannelConfig, BypassCollectionConfig, BypassError, BypassHeader, BypassMessage,
    BypassMetrics, BypassMetricsSnapshot, BypassTarget, BypassTransport, MessageEncoding,
    UdpBypassChannel, UdpConfig,
};
pub use capabilities::{
    ConfigurableTransport, DistanceSource, MessagePriority, MessageRequirements, PaceLevel,
    PeerDistance, RangeMode, RangeModeConfig, Transport, TransportCapabilities, TransportId,
    TransportInstance, TransportMode, TransportPolicy, TransportType,
};
pub use health::{HealthMonitor, HeartbeatConfig};
pub use manager::{RouteDecision, TransportManager, TransportManagerConfig};

#[cfg(feature = "bluetooth")]
pub use btle::HiveBleTransport;

// =============================================================================
// Node Identity
// =============================================================================

/// Node identifier in the mesh network
///
/// Uniquely identifies a node in the mesh. This is separate from
/// backend-specific IDs (e.g., Iroh's EndpointId, Ditto's peer ID).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    /// Create a new node ID from a string
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the node ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for NodeId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

// =============================================================================
// Peer Events
// =============================================================================

/// Peer connection lifecycle events
///
/// Applications can subscribe to these events to react to peer state changes.
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// New peer connected successfully
    Connected {
        /// The peer's node ID
        peer_id: NodeId,
        /// When the connection was established
        connected_at: Instant,
    },

    /// Peer disconnected
    Disconnected {
        /// The peer's node ID
        peer_id: NodeId,
        /// Reason for disconnection (if known)
        reason: DisconnectReason,
        /// How long the connection was active
        connection_duration: std::time::Duration,
    },

    /// Connection quality degraded
    Degraded {
        /// The peer's node ID
        peer_id: NodeId,
        /// Current health metrics
        health: ConnectionHealth,
    },

    /// Attempting to reconnect to a peer
    Reconnecting {
        /// The peer's node ID
        peer_id: NodeId,
        /// Current attempt number (1-indexed)
        attempt: u32,
        /// Maximum attempts configured (None = infinite)
        max_attempts: Option<u32>,
    },

    /// Reconnection attempt failed
    ReconnectFailed {
        /// The peer's node ID
        peer_id: NodeId,
        /// Current attempt number
        attempt: u32,
        /// Error message
        error: String,
        /// Whether more retries will be attempted
        will_retry: bool,
    },
}

/// Reason for peer disconnection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Remote peer initiated close
    RemoteClosed,
    /// Connection timed out
    Timeout,
    /// Network error occurred
    NetworkError(String),
    /// Local side requested disconnect
    LocalClosed,
    /// Connection was idle too long
    IdleTimeout,
    /// Application-level error
    ApplicationError(String),
    /// Unknown reason
    Unknown,
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisconnectReason::RemoteClosed => write!(f, "remote closed"),
            DisconnectReason::Timeout => write!(f, "timeout"),
            DisconnectReason::NetworkError(e) => write!(f, "network error: {}", e),
            DisconnectReason::LocalClosed => write!(f, "local closed"),
            DisconnectReason::IdleTimeout => write!(f, "idle timeout"),
            DisconnectReason::ApplicationError(e) => write!(f, "application error: {}", e),
            DisconnectReason::Unknown => write!(f, "unknown"),
        }
    }
}

/// Connection health metrics
#[derive(Debug, Clone)]
pub struct ConnectionHealth {
    /// Round-trip time in milliseconds (smoothed average)
    pub rtt_ms: u32,
    /// RTT variance in milliseconds
    pub rtt_variance_ms: u32,
    /// Estimated packet loss percentage (0-100)
    pub packet_loss_percent: u8,
    /// Current connection state
    pub state: ConnectionState,
    /// Last successful communication
    pub last_activity: Instant,
}

impl Default for ConnectionHealth {
    fn default() -> Self {
        Self {
            rtt_ms: 0,
            rtt_variance_ms: 0,
            packet_loss_percent: 0,
            state: ConnectionState::Healthy,
            last_activity: Instant::now(),
        }
    }
}

/// Connection state for health monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is healthy
    Healthy,
    /// Connection is degraded (high latency/loss)
    Degraded,
    /// Connection is suspected dead (missed heartbeats)
    Suspect,
    /// Connection confirmed dead
    Dead,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Healthy => write!(f, "healthy"),
            ConnectionState::Degraded => write!(f, "degraded"),
            ConnectionState::Suspect => write!(f, "suspect"),
            ConnectionState::Dead => write!(f, "dead"),
        }
    }
}

// =============================================================================
// Error Types
// =============================================================================

/// Error type for mesh transport operations
#[derive(Debug)]
pub enum TransportError {
    /// Connection failed to establish
    ConnectionFailed(String),
    /// Peer not found or unreachable
    PeerNotFound(String),
    /// Connection already exists
    AlreadyConnected(String),
    /// Transport not started
    NotStarted,
    /// Generic transport error
    Other(Box<dyn StdError + Send + Sync>),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            TransportError::PeerNotFound(msg) => write!(f, "Peer not found: {}", msg),
            TransportError::AlreadyConnected(msg) => write!(f, "Already connected: {}", msg),
            TransportError::NotStarted => write!(f, "Transport not started"),
            TransportError::Other(err) => write!(f, "Transport error: {}", err),
        }
    }
}

impl StdError for TransportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TransportError::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// Result type alias for transport operations
pub type Result<T> = std::result::Result<T, TransportError>;

/// Channel capacity for peer events
pub const PEER_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Type alias for peer event receiver
pub type PeerEventReceiver = mpsc::Receiver<PeerEvent>;

/// Type alias for peer event sender
pub type PeerEventSender = mpsc::Sender<PeerEvent>;

// =============================================================================
// Transport Traits
// =============================================================================

/// Transport abstraction for mesh topology connections
///
/// This trait defines the connection management operations needed by
/// `TopologyManager` to establish parent-child relationships in the mesh.
#[async_trait]
pub trait MeshTransport: Send + Sync {
    /// Start the transport layer
    async fn start(&self) -> Result<()>;

    /// Stop the transport layer
    async fn stop(&self) -> Result<()>;

    /// Connect to a peer by node ID
    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>>;

    /// Disconnect from a peer
    async fn disconnect(&self, peer_id: &NodeId) -> Result<()>;

    /// Get an existing connection to a peer
    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>>;

    /// Get the number of connected peers
    fn peer_count(&self) -> usize;

    /// Get list of connected peer IDs
    fn connected_peers(&self) -> Vec<NodeId>;

    /// Check if connected to a specific peer
    fn is_connected(&self, peer_id: &NodeId) -> bool {
        self.get_connection(peer_id).is_some()
    }

    /// Subscribe to peer connection events
    fn subscribe_peer_events(&self) -> PeerEventReceiver;

    /// Get connection health for a specific peer
    fn get_peer_health(&self, peer_id: &NodeId) -> Option<ConnectionHealth> {
        self.get_connection(peer_id)
            .map(|_| ConnectionHealth::default())
    }
}

/// Active connection to a mesh peer
///
/// This trait abstracts over backend-specific connection types.
pub trait MeshConnection: Send + Sync {
    /// Get the remote peer's node ID
    fn peer_id(&self) -> &NodeId;

    /// Check if connection is still alive
    fn is_alive(&self) -> bool;

    /// Get the time when this connection was established
    fn connected_at(&self) -> Instant;

    /// Get the disconnect reason if the connection is closed
    fn disconnect_reason(&self) -> Option<DisconnectReason> {
        if self.is_alive() {
            None
        } else {
            Some(DisconnectReason::Unknown)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_creation() {
        let id = NodeId::new("node-123".to_string());
        assert_eq!(id.as_str(), "node-123");
        assert_eq!(id.to_string(), "node-123");
    }

    #[test]
    fn test_node_id_from_string() {
        let id: NodeId = "node-456".into();
        assert_eq!(id.as_str(), "node-456");
    }

    #[test]
    fn test_node_id_equality() {
        let id1 = NodeId::new("node-123".to_string());
        let id2 = NodeId::new("node-123".to_string());
        let id3 = NodeId::new("node-456".to_string());

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_transport_error_display() {
        let err = TransportError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");

        let err = TransportError::PeerNotFound("node-123".to_string());
        assert_eq!(err.to_string(), "Peer not found: node-123");

        let err = TransportError::NotStarted;
        assert_eq!(err.to_string(), "Transport not started");
    }
}
