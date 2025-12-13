//! Platform abstraction layer for BLE
//!
//! This module defines the traits that platform-specific implementations
//! must implement to provide BLE functionality.
//!
//! ## Supported Platforms
//!
//! - **Linux**: BlueZ via D-Bus (`bluer` crate)
//! - **Android**: JNI to Android Bluetooth APIs
//! - **macOS/iOS**: CoreBluetooth
//! - **Windows**: WinRT Bluetooth APIs
//! - **Embedded**: ESP-IDF NimBLE
//!
//! ## Architecture
//!
//! Each platform provides an implementation of `BleAdapter` that handles:
//! - Adapter initialization and power management
//! - Discovery (scanning and advertising)
//! - GATT server and client operations
//! - Connection management

use async_trait::async_trait;

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::Result;
use crate::transport::BleConnection;
use crate::NodeId;

// Platform-specific modules (conditionally compiled)
#[cfg(all(feature = "linux", target_os = "linux"))]
pub mod linux;

#[cfg(feature = "android")]
pub mod android;

#[cfg(any(feature = "macos", feature = "ios"))]
pub mod apple;

#[cfg(feature = "windows")]
pub mod windows;

#[cfg(feature = "embedded")]
pub mod embedded;

/// Discovered BLE device
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    /// Device address (MAC or platform-specific)
    pub address: String,
    /// Device name (if available)
    pub name: Option<String>,
    /// RSSI in dBm
    pub rssi: i8,
    /// Is this a HIVE node?
    pub is_hive_node: bool,
    /// Parsed HIVE node ID (if HIVE node)
    pub node_id: Option<NodeId>,
    /// Raw advertising data
    pub adv_data: Vec<u8>,
}

/// Callback for discovered devices
pub type DiscoveryCallback = Box<dyn Fn(DiscoveredDevice) + Send + Sync>;

/// Callback for connection events
pub type ConnectionCallback = Box<dyn Fn(NodeId, ConnectionEvent) + Send + Sync>;

/// Connection event types
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Connection established
    Connected {
        /// Negotiated MTU
        mtu: u16,
        /// Connection PHY
        phy: BlePhy,
    },
    /// Connection lost
    Disconnected {
        /// Reason for disconnection
        reason: DisconnectReason,
    },
    /// MTU changed
    MtuChanged {
        /// New MTU value
        new_mtu: u16,
    },
    /// PHY changed
    PhyChanged {
        /// New PHY
        new_phy: BlePhy,
    },
    /// RSSI updated
    RssiUpdated {
        /// New RSSI value in dBm
        rssi: i8,
    },
}

/// Reason for disconnection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Disconnected by local request
    LocalRequest,
    /// Disconnected by remote device
    RemoteRequest,
    /// Connection timeout
    Timeout,
    /// Link loss (device out of range)
    LinkLoss,
    /// Connection failed
    ConnectionFailed,
    /// Unknown reason
    Unknown,
}

/// Platform-specific BLE adapter
///
/// This is the main abstraction trait that each platform must implement.
/// It provides all BLE functionality needed by the transport layer.
#[async_trait]
pub trait BleAdapter: Send + Sync {
    /// Initialize the adapter with the given configuration
    async fn init(&mut self, config: &BleConfig) -> Result<()>;

    /// Start the adapter (begin advertising and/or scanning)
    async fn start(&self) -> Result<()>;

    /// Stop the adapter
    async fn stop(&self) -> Result<()>;

    /// Check if the adapter is powered on
    fn is_powered(&self) -> bool;

    /// Get the adapter's Bluetooth address
    fn address(&self) -> Option<String>;

    // === Discovery ===

    /// Start scanning for devices
    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()>;

    /// Stop scanning
    async fn stop_scan(&self) -> Result<()>;

    /// Start advertising
    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()>;

    /// Stop advertising
    async fn stop_advertising(&self) -> Result<()>;

    /// Set callback for discovered devices
    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>);

    // === Connections ===

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

    /// Set callback for connection events
    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>);

    // === GATT ===

    /// Register the HIVE GATT service
    async fn register_gatt_service(&self) -> Result<()>;

    /// Unregister the HIVE GATT service
    async fn unregister_gatt_service(&self) -> Result<()>;

    // === Capabilities ===

    /// Check if Coded PHY is supported
    fn supports_coded_phy(&self) -> bool;

    /// Check if extended advertising is supported
    fn supports_extended_advertising(&self) -> bool;

    /// Get maximum supported MTU
    fn max_mtu(&self) -> u16;

    /// Get maximum number of connections
    fn max_connections(&self) -> u8;
}

/// Stub adapter for testing and platforms without BLE
#[derive(Debug, Default)]
pub struct StubAdapter {
    powered: bool,
}

#[async_trait]
impl BleAdapter for StubAdapter {
    async fn init(&mut self, _config: &BleConfig) -> Result<()> {
        self.powered = true;
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    fn is_powered(&self) -> bool {
        self.powered
    }

    fn address(&self) -> Option<String> {
        Some("00:00:00:00:00:00".to_string())
    }

    async fn start_scan(&self, _config: &DiscoveryConfig) -> Result<()> {
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        Ok(())
    }

    async fn start_advertising(&self, _config: &DiscoveryConfig) -> Result<()> {
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        Ok(())
    }

    fn set_discovery_callback(&mut self, _callback: Option<DiscoveryCallback>) {}

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        Err(crate::error::BleError::NotSupported(format!(
            "Stub adapter cannot connect to {}",
            peer_id
        )))
    }

    async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
        Ok(())
    }

    fn get_connection(&self, _peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        None
    }

    fn peer_count(&self) -> usize {
        0
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        Vec::new()
    }

    fn set_connection_callback(&mut self, _callback: Option<ConnectionCallback>) {}

    async fn register_gatt_service(&self) -> Result<()> {
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        false
    }

    fn max_mtu(&self) -> u16 {
        23
    }

    fn max_connections(&self) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stub_adapter() {
        let mut adapter = StubAdapter::default();
        assert!(!adapter.is_powered());

        adapter.init(&BleConfig::default()).await.unwrap();
        assert!(adapter.is_powered());
        assert_eq!(adapter.peer_count(), 0);
        assert!(!adapter.supports_coded_phy());
    }
}
