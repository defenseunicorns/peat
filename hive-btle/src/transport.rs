//! Transport trait implementation for HIVE-BTLE
//!
//! Implements the pluggable transport abstraction (ADR-032) for Bluetooth LE,
//! providing the `BluetoothLETransport` struct that can be registered with
//! the `TransportManager`.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

use async_trait::async_trait;
use core::time::Duration;

use crate::config::{BleConfig, BlePhy};
use crate::error::Result;
use crate::platform::BleAdapter;
use crate::NodeId;

/// Transport capabilities for Bluetooth LE
///
/// Advertises what this transport can do, allowing the TransportManager
/// to select the best transport for each message.
#[derive(Debug, Clone)]
pub struct TransportCapabilities {
    /// Maximum bandwidth in bytes/second
    pub max_bandwidth_bps: u64,
    /// Typical latency in milliseconds
    pub typical_latency_ms: u32,
    /// Maximum practical range in meters
    pub max_range_meters: u32,
    /// Supports bidirectional communication
    pub bidirectional: bool,
    /// Supports reliable delivery
    pub reliable: bool,
    /// Battery impact score (0-100, higher = more power)
    pub battery_impact: u8,
    /// Supports broadcast/advertising
    pub supports_broadcast: bool,
    /// Requires pairing before use
    pub requires_pairing: bool,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl TransportCapabilities {
    /// Create default BLE capabilities
    pub fn bluetooth_le() -> Self {
        Self {
            max_bandwidth_bps: 250_000, // ~250 KB/s practical throughput
            typical_latency_ms: 30,
            max_range_meters: 100,
            bidirectional: true,
            reliable: true,
            battery_impact: 15,
            supports_broadcast: true,
            requires_pairing: false,
            max_message_size: 512,
        }
    }

    /// Create capabilities for Coded PHY (long range)
    pub fn bluetooth_le_coded() -> Self {
        Self {
            max_bandwidth_bps: 125_000, // Coded S=8
            typical_latency_ms: 100,
            max_range_meters: 400,
            bidirectional: true,
            reliable: true,
            battery_impact: 20, // Slightly higher due to longer TX time
            supports_broadcast: true,
            requires_pairing: false,
            max_message_size: 512,
        }
    }

    /// Update capabilities based on PHY
    pub fn for_phy(phy: BlePhy) -> Self {
        match phy {
            BlePhy::Le1M => Self::bluetooth_le(),
            BlePhy::Le2M => Self {
                max_bandwidth_bps: 500_000,
                typical_latency_ms: 20,
                max_range_meters: 50,
                ..Self::bluetooth_le()
            },
            BlePhy::LeCodedS2 => Self {
                max_bandwidth_bps: 250_000,
                typical_latency_ms: 50,
                max_range_meters: 200,
                ..Self::bluetooth_le()
            },
            BlePhy::LeCodedS8 => Self::bluetooth_le_coded(),
        }
    }
}

impl Default for TransportCapabilities {
    fn default() -> Self {
        Self::bluetooth_le()
    }
}

/// Connection to a BLE peer
///
/// Represents an active GATT connection to a remote device.
pub trait BleConnection: Send + Sync {
    /// Get the remote peer's node ID
    fn peer_id(&self) -> &NodeId;

    /// Check if connection is still alive
    fn is_alive(&self) -> bool;

    /// Get the negotiated MTU
    fn mtu(&self) -> u16;

    /// Get the current PHY
    fn phy(&self) -> BlePhy;

    /// Get RSSI (signal strength) in dBm
    fn rssi(&self) -> Option<i8>;

    /// Get connection duration
    fn connected_duration(&self) -> Duration;
}

/// Bluetooth LE mesh transport
///
/// Implements the transport abstraction for BLE, providing:
/// - Peer discovery via advertising/scanning
/// - GATT-based data exchange
/// - Connection management
/// - PHY selection
///
/// # Example
///
/// ```ignore
/// use hive_btle::{BluetoothLETransport, BleConfig, NodeId};
///
/// let config = BleConfig::hive_lite(NodeId::new(0x12345678));
/// let transport = BluetoothLETransport::new(config)?;
///
/// transport.start().await?;
/// let conn = transport.connect(&peer_id).await?;
/// ```
pub struct BluetoothLETransport<A: BleAdapter> {
    /// Configuration
    config: BleConfig,
    /// Platform-specific adapter
    adapter: A,
    /// Current capabilities (may change with PHY)
    capabilities: TransportCapabilities,
}

impl<A: BleAdapter> BluetoothLETransport<A> {
    /// Create a new BLE transport with the given adapter
    pub fn new(config: BleConfig, adapter: A) -> Self {
        let capabilities = TransportCapabilities::for_phy(config.phy.preferred_phy);
        Self {
            config,
            adapter,
            capabilities,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &BleConfig {
        &self.config
    }

    /// Get the current capabilities
    pub fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.config.node_id
    }
}

/// Async transport operations
///
/// These are the core transport operations that integrate with
/// the HIVE protocol's transport abstraction (ADR-032).
#[async_trait]
pub trait MeshTransport: Send + Sync {
    /// Start the transport layer
    async fn start(&self) -> Result<()>;

    /// Stop the transport layer
    async fn stop(&self) -> Result<()>;

    /// Connect to a peer by node ID
    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>>;

    /// Disconnect from a peer
    async fn disconnect(&self, peer_id: &NodeId) -> Result<()>;

    /// Get an existing connection
    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>>;

    /// Get the number of connected peers
    fn peer_count(&self) -> usize;

    /// Get list of connected peer IDs
    fn connected_peers(&self) -> Vec<NodeId>;

    /// Check if connected to a specific peer
    fn is_connected(&self, peer_id: &NodeId) -> bool {
        self.get_connection(peer_id).is_some()
    }

    /// Get transport capabilities
    fn capabilities(&self) -> &TransportCapabilities;
}

// Stub implementation - will be filled in by platform-specific code
#[async_trait]
impl<A: BleAdapter + Send + Sync> MeshTransport for BluetoothLETransport<A> {
    async fn start(&self) -> Result<()> {
        // Start advertising and scanning via adapter
        self.adapter.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.adapter.stop().await
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        self.adapter.connect(peer_id).await
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        self.adapter.disconnect(peer_id).await
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        self.adapter.get_connection(peer_id)
    }

    fn peer_count(&self) -> usize {
        self.adapter.peer_count()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.adapter.connected_peers()
    }

    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_for_phy() {
        let caps = TransportCapabilities::for_phy(BlePhy::LeCodedS8);
        assert_eq!(caps.max_range_meters, 400);
        assert_eq!(caps.max_bandwidth_bps, 125_000);
    }

    #[test]
    fn test_capabilities_le2m() {
        let caps = TransportCapabilities::for_phy(BlePhy::Le2M);
        assert_eq!(caps.max_range_meters, 50);
        assert_eq!(caps.max_bandwidth_bps, 500_000);
    }
}
