//! WinRT BLE adapter implementation
//!
//! Main adapter that implements the `BleAdapter` trait using Windows BLE APIs.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::config::{BleConfig, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;

use super::advertiser::BleAdvertiser;
use super::connection::WinRtConnection;
use super::gatt_server::GattServer;
use super::watcher::BleWatcher;

/// Internal adapter state
struct AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, WinRtConnection>,
    /// Bluetooth address to node ID mapping
    address_to_node: HashMap<u64, NodeId>,
    /// Node ID to Bluetooth address mapping
    node_to_address: HashMap<NodeId, u64>,
}

impl Default for AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            address_to_node: HashMap::new(),
            node_to_address: HashMap::new(),
        }
    }
}

/// WinRT BLE adapter for Windows
///
/// Implements the `BleAdapter` trait using Windows BLE APIs.
/// Requires Windows 10 version 1703+ for basic functionality,
/// and version 1803+ for GATT server support.
///
/// # Architecture
///
/// The adapter manages:
/// - **BleWatcher**: For scanning/discovering devices
/// - **BleAdvertiser**: For advertising our presence
/// - **GattServer**: For hosting the HIVE GATT service
/// - **WinRtConnection**: For connecting to other devices as GATT client
///
/// # Example
///
/// ```ignore
/// let config = BleConfig::new(NodeId::new(0x12345678));
/// let mut adapter = WinRtBleAdapter::new()?;
/// adapter.init(&config).await?;
/// adapter.start().await?;
/// ```
pub struct WinRtBleAdapter {
    /// BLE scanner
    watcher: Arc<RwLock<BleWatcher>>,
    /// BLE advertiser
    advertiser: Arc<RwLock<BleAdvertiser>>,
    /// GATT server
    gatt_server: Arc<RwLock<Option<GattServer>>>,
    /// Configuration
    config: RwLock<Option<BleConfig>>,
    /// Internal state
    state: RwLock<AdapterState>,
    /// Discovery callback
    discovery_callback: RwLock<Option<DiscoveryCallback>>,
    /// Connection callback
    connection_callback: RwLock<Option<ConnectionCallback>>,
    /// Whether the adapter is initialized
    initialized: bool,
}

impl WinRtBleAdapter {
    /// Create a new WinRT BLE adapter
    pub fn new() -> Result<Self> {
        let watcher = BleWatcher::new()?;
        let advertiser = BleAdvertiser::new()?;

        log::info!("WinRtBleAdapter created");

        Ok(Self {
            watcher: Arc::new(RwLock::new(watcher)),
            advertiser: Arc::new(RwLock::new(advertiser)),
            gatt_server: Arc::new(RwLock::new(None)),
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            initialized: false,
        })
    }

    /// Register a node ID to address mapping
    pub async fn register_node_address(&self, node_id: NodeId, address: u64) {
        let mut state = self.state.write().await;
        state.address_to_node.insert(address, node_id);
        state.node_to_address.insert(node_id, address);
    }

    /// Get address for a node ID
    pub async fn get_node_address(&self, node_id: &NodeId) -> Option<u64> {
        let state = self.state.read().await;
        state.node_to_address.get(node_id).copied()
    }

    /// Get node ID for an address
    pub async fn get_address_node(&self, address: u64) -> Option<NodeId> {
        let state = self.state.read().await;
        state.address_to_node.get(&address).copied()
    }

    /// Process discovered devices and invoke callbacks
    pub async fn process_discoveries(&self) -> Result<()> {
        let watcher = self.watcher.read().await;
        let hive_peripherals = watcher.get_hive_peripherals();

        if let Some(ref callback) = *self.discovery_callback.read().await {
            for peripheral in hive_peripherals {
                // Register the mapping if we have a node ID
                if let Some(node_id) = peripheral.node_id {
                    self.register_node_address(node_id, peripheral.address)
                        .await;
                }

                let device: DiscoveredDevice = peripheral.into();
                callback(device);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl BleAdapter for WinRtBleAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        // Store config
        *self.config.write().await = Some(config.clone());

        // Initialize GATT server
        let mut gatt_server = GattServer::new(config.node_id)?;
        gatt_server.init().await?;
        *self.gatt_server.write().await = Some(gatt_server);

        self.initialized = true;

        log::info!(
            "WinRtBleAdapter initialized for node {:08X}",
            config.node_id.as_u32()
        );

        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let config = self.config.read().await;
        let config = config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Start GATT server advertising
        if let Some(ref mut server) = *self.gatt_server.write().await {
            server.start_advertising()?;
        }

        // Start BLE advertising
        {
            let mut advertiser = self.advertiser.write().await;
            advertiser.start_advertising(config.node_id, &config.discovery)?;
        }

        // Start scanning
        {
            let mut watcher = self.watcher.write().await;
            watcher.start_scan(&config.discovery)?;
        }

        log::info!("WinRtBleAdapter started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop scanning
        {
            let mut watcher = self.watcher.write().await;
            watcher.stop_scan()?;
        }

        // Stop advertising
        {
            let mut advertiser = self.advertiser.write().await;
            advertiser.stop_advertising()?;
        }

        // Stop GATT server
        if let Some(ref mut server) = *self.gatt_server.write().await {
            server.stop_advertising()?;
        }

        log::info!("WinRtBleAdapter stopped");
        Ok(())
    }

    fn is_powered(&self) -> bool {
        // Windows doesn't provide a simple way to check adapter power state
        // We assume powered if we initialized successfully
        self.initialized
    }

    fn address(&self) -> Option<String> {
        // Windows doesn't easily expose the local Bluetooth address
        // We'd need to enumerate adapters which requires additional code
        None
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        let mut watcher = self.watcher.write().await;
        watcher.start_scan(config)
    }

    async fn stop_scan(&self) -> Result<()> {
        let mut watcher = self.watcher.write().await;
        watcher.stop_scan()
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        let ble_config = self.config.read().await;
        let ble_config = ble_config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let mut advertiser = self.advertiser.write().await;
        advertiser.start_advertising(ble_config.node_id, config)
    }

    async fn stop_advertising(&self) -> Result<()> {
        let mut advertiser = self.advertiser.write().await;
        advertiser.stop_advertising()
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        if let Ok(mut cb) = self.discovery_callback.try_write() {
            *cb = callback;
        }
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        // Look up the Bluetooth address for this node ID
        let address = self
            .get_node_address(peer_id)
            .await
            .ok_or_else(|| BleError::ConnectionFailed(format!("Unknown node ID: {}", peer_id)))?;

        // Create connection
        let mut connection = WinRtConnection::new(*peer_id, address);
        connection.connect().await?;

        // Store connection
        {
            let mut state = self.state.write().await;
            state.connections.insert(*peer_id, connection.clone());
        }

        // Notify callback
        if let Some(ref cb) = *self.connection_callback.read().await {
            cb(
                *peer_id,
                ConnectionEvent::Connected {
                    mtu: connection.mtu(),
                    phy: connection.phy(),
                },
            );
        }

        log::info!("Connected to peer {} at {:012X}", peer_id, address);
        Ok(Box::new(connection))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let connection = {
            let mut state = self.state.write().await;
            state.connections.remove(peer_id)
        };

        if let Some(mut conn) = connection {
            conn.disconnect();

            // Notify callback
            if let Some(ref cb) = *self.connection_callback.read().await {
                cb(
                    *peer_id,
                    ConnectionEvent::Disconnected {
                        reason: DisconnectReason::LocalRequest,
                    },
                );
            }

            log::info!("Disconnected from peer {}", peer_id);
        }

        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        if let Ok(state) = self.state.try_read() {
            state
                .connections
                .get(peer_id)
                .map(|c| Box::new(c.clone()) as Box<dyn BleConnection>)
        } else {
            None
        }
    }

    fn peer_count(&self) -> usize {
        if let Ok(state) = self.state.try_read() {
            state.connections.len()
        } else {
            0
        }
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        if let Ok(state) = self.state.try_read() {
            state.connections.keys().copied().collect()
        } else {
            Vec::new()
        }
    }

    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>) {
        if let Ok(mut cb) = self.connection_callback.try_write() {
            *cb = callback;
        }
    }

    async fn register_gatt_service(&self) -> Result<()> {
        if let Some(ref mut server) = *self.gatt_server.write().await {
            server.start_advertising()?;
        }
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        if let Some(ref mut server) = *self.gatt_server.write().await {
            server.stop_advertising()?;
        }
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Windows doesn't expose Coded PHY to applications
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        // Extended advertising requires Windows 10 1903+
        // We'd need to check the OS version to be accurate
        true
    }

    fn max_mtu(&self) -> u16 {
        // Windows typically supports up to 512 bytes MTU
        512
    }

    fn max_connections(&self) -> u8 {
        // Windows supports multiple connections (typically 7-10)
        8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_state_default() {
        let state = AdapterState::default();
        assert!(state.connections.is_empty());
        assert!(state.address_to_node.is_empty());
    }
}
