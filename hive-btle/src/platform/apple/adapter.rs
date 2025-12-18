//! CoreBluetooth BLE adapter implementation
//!
//! This module provides the `CoreBluetoothAdapter` which implements `BleAdapter`
//! using CoreBluetooth framework for iOS and macOS.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;

use super::central::CentralManager;
use super::connection::CoreBluetoothConnection;
use super::delegates::CentralState;
use super::peripheral::PeripheralManager;

/// Internal state for the adapter
struct AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, CoreBluetoothConnection>,
    /// Peripheral identifier to node ID mapping
    identifier_to_node: HashMap<String, NodeId>,
    /// Node ID to peripheral identifier mapping
    node_to_identifier: HashMap<NodeId, String>,
}

impl Default for AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            identifier_to_node: HashMap::new(),
            node_to_identifier: HashMap::new(),
        }
    }
}

/// CoreBluetooth BLE adapter for iOS and macOS
///
/// Implements the `BleAdapter` trait using CoreBluetooth framework.
/// Works on both iOS (13.0+) and macOS (10.15+).
///
/// # Architecture
///
/// The adapter manages both central and peripheral roles:
/// - **CentralManager**: Scanning for devices, connecting as GATT client
/// - **PeripheralManager**: Advertising, hosting GATT server
///
/// # iOS Background Execution
///
/// For iOS apps, ensure Info.plist includes:
/// ```xml
/// <key>UIBackgroundModes</key>
/// <array>
///     <string>bluetooth-central</string>
///     <string>bluetooth-peripheral</string>
/// </array>
/// ```
///
/// # Example
///
/// ```ignore
/// let config = BleConfig::new(NodeId::new(0x12345678));
/// let mut adapter = CoreBluetoothAdapter::new()?;
/// adapter.init(&config).await?;
/// adapter.start().await?;
/// ```
pub struct CoreBluetoothAdapter {
    /// Central manager for scanning and connecting
    central: Arc<CentralManager>,
    /// Peripheral manager for advertising and GATT server
    peripheral: Arc<PeripheralManager>,
    /// Configuration
    config: RwLock<Option<BleConfig>>,
    /// Internal state
    state: RwLock<AdapterState>,
    /// Discovery callback
    discovery_callback: RwLock<Option<DiscoveryCallback>>,
    /// Connection callback
    connection_callback: RwLock<Option<ConnectionCallback>>,
}

impl CoreBluetoothAdapter {
    /// Create a new CoreBluetooth adapter
    ///
    /// This initializes both CBCentralManager and CBPeripheralManager.
    /// The adapters won't be ready until Bluetooth is powered on.
    pub fn new() -> Result<Self> {
        let central = Arc::new(CentralManager::new()?);
        let peripheral = Arc::new(PeripheralManager::new()?);

        log::info!("CoreBluetoothAdapter created");

        Ok(Self {
            central,
            peripheral,
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
        })
    }

    /// Register node ID to peripheral identifier mapping
    pub async fn register_node_identifier(&self, node_id: NodeId, identifier: String) {
        let mut state = self.state.write().await;
        state
            .identifier_to_node
            .insert(identifier.clone(), node_id.clone());
        state.node_to_identifier.insert(node_id, identifier);
    }

    /// Get peripheral identifier for a node ID
    pub async fn get_node_identifier(&self, node_id: &NodeId) -> Option<String> {
        let state = self.state.read().await;
        state.node_to_identifier.get(node_id).cloned()
    }

    /// Get node ID for a peripheral identifier
    pub async fn get_identifier_node(&self, identifier: &str) -> Option<NodeId> {
        let state = self.state.read().await;
        state.identifier_to_node.get(identifier).cloned()
    }

    /// Process events from central and peripheral managers
    async fn process_events(&self) -> Result<()> {
        self.central.process_events().await?;
        self.peripheral.process_events().await?;

        // Check for discovered HIVE nodes and invoke callback
        let hive_peripherals = self.central.get_hive_peripherals().await;
        if let Some(ref callback) = *self.discovery_callback.read().await {
            for peripheral in hive_peripherals {
                // Register the mapping if we have a node ID
                if let Some(node_id) = &peripheral.node_id {
                    self.register_node_identifier(node_id.clone(), peripheral.identifier.clone())
                        .await;
                }

                let device = DiscoveredDevice {
                    address: peripheral.identifier.clone(),
                    name: peripheral.name.clone(),
                    rssi: peripheral.rssi,
                    is_hive_node: peripheral.is_hive_node,
                    node_id: peripheral.node_id.clone(),
                    adv_data: Vec::new(),
                };

                callback(device);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl BleAdapter for CoreBluetoothAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        // Store config
        *self.config.write().await = Some(config.clone());

        // Wait for both managers to be ready
        // Note: In actual implementation, this would poll until state is PoweredOn
        let central_state = self.central.state().await;
        let peripheral_state = self.peripheral.state().await;

        log::info!(
            "CoreBluetoothAdapter initialized for node {:08X} (central: {:?}, peripheral: {:?})",
            config.node_id.as_u32(),
            central_state,
            peripheral_state
        );

        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let config = self.config.read().await;
        let config = config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Register HIVE GATT service
        if let Err(e) = self
            .peripheral
            .register_hive_service(config.node_id.clone())
            .await
        {
            log::warn!("Failed to register HIVE service: {}", e);
        }

        // Start advertising
        if let Err(e) = self
            .peripheral
            .start_advertising(config.node_id.clone(), &config.discovery)
            .await
        {
            log::warn!("Failed to start advertising: {}", e);
        }

        // Start scanning
        if let Err(e) = self.central.start_scan(&config.discovery, None).await {
            log::warn!("Failed to start scanning: {}", e);
        }

        log::info!("CoreBluetoothAdapter started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop scanning
        self.central.stop_scan().await?;

        // Stop advertising
        self.peripheral.stop_advertising().await?;

        // Unregister services
        self.peripheral.unregister_all_services().await?;

        log::info!("CoreBluetoothAdapter stopped");
        Ok(())
    }

    fn is_powered(&self) -> bool {
        // Check central manager state synchronously
        // In real implementation, would need to cache last known state
        true // Placeholder
    }

    fn address(&self) -> Option<String> {
        // CoreBluetooth doesn't expose the local Bluetooth address
        // for privacy reasons (iOS) or API limitations (macOS)
        None
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        // Scan for HIVE service UUID
        let service_uuids = Some(vec![crate::HIVE_SERVICE_UUID.to_string()]);
        self.central.start_scan(config, service_uuids).await
    }

    async fn stop_scan(&self) -> Result<()> {
        self.central.stop_scan().await
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        let ble_config = self.config.read().await;
        let ble_config = ble_config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        self.peripheral
            .start_advertising(ble_config.node_id.clone(), config)
            .await
    }

    async fn stop_advertising(&self) -> Result<()> {
        self.peripheral.stop_advertising().await
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        if let Ok(mut cb) = self.discovery_callback.try_write() {
            *cb = callback;
        }
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        // Look up the peripheral identifier for this node ID
        let identifier = self
            .get_node_identifier(peer_id)
            .await
            .ok_or_else(|| BleError::ConnectionFailed(format!("Unknown node ID: {}", peer_id)))?;

        // Connect via central manager
        self.central.connect(&identifier).await?;

        // Create connection wrapper
        let connection = CoreBluetoothConnection::new(peer_id.clone(), identifier.clone());

        // Store connection
        {
            let mut state = self.state.write().await;
            state
                .connections
                .insert(peer_id.clone(), connection.clone());
        }

        // Notify callback
        if let Some(ref cb) = *self.connection_callback.read().await {
            cb(
                peer_id.clone(),
                ConnectionEvent::Connected {
                    mtu: connection.mtu(),
                    phy: connection.phy(),
                },
            );
        }

        log::info!("Connected to peer {} ({})", peer_id, identifier);
        Ok(Box::new(connection))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let (connection, identifier) = {
            let mut state = self.state.write().await;
            let conn = state.connections.remove(peer_id);
            let id = state.node_to_identifier.get(peer_id).cloned();
            (conn, id)
        };

        if let Some(identifier) = identifier {
            self.central.disconnect(&identifier).await?;
        }

        if connection.is_some() {
            // Notify callback
            if let Some(ref cb) = *self.connection_callback.read().await {
                cb(
                    peer_id.clone(),
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
            state.connections.keys().cloned().collect()
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
        let config = self.config.read().await;
        let config = config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        self.peripheral
            .register_hive_service(config.node_id.clone())
            .await
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        self.peripheral.unregister_all_services().await
    }

    fn supports_coded_phy(&self) -> bool {
        // CoreBluetooth doesn't expose Coded PHY selection
        // It's handled automatically by the system
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        // CoreBluetooth doesn't expose extended advertising
        false
    }

    fn max_mtu(&self) -> u16 {
        // iOS/macOS typically support up to 512 bytes MTU
        // Actual negotiated MTU depends on the remote device
        512
    }

    fn max_connections(&self) -> u8 {
        // iOS/macOS limit varies by device
        // Typically 8-10 simultaneous connections
        8
    }
}

#[cfg(test)]
mod tests {
    // CoreBluetooth tests require actual Apple hardware
    // They should be run on iOS Simulator or macOS

    #[test]
    fn test_adapter_state_default() {
        use super::AdapterState;
        let state = AdapterState::default();
        assert!(state.connections.is_empty());
        assert!(state.identifier_to_node.is_empty());
    }
}
