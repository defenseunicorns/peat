//! BlueZ adapter implementation using the `bluer` crate

use async_trait::async_trait;
use bluer::{
    adv::{Advertisement, AdvertisementHandle},
    Adapter, Address, Session,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::{NodeId, HIVE_SERVICE_UUID};

use super::BluerConnection;

/// Internal state for the adapter
struct AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, BluerConnection>,
    /// Device address to node ID mapping
    address_to_node: HashMap<Address, NodeId>,
    /// Node ID to device address mapping
    node_to_address: HashMap<NodeId, Address>,
    /// Discovered devices (address -> device info)
    discovered: HashMap<Address, DiscoveredDevice>,
}

impl Default for AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            address_to_node: HashMap::new(),
            node_to_address: HashMap::new(),
            discovered: HashMap::new(),
        }
    }
}

/// Linux/BlueZ BLE adapter
///
/// Implements the `BleAdapter` trait using the `bluer` crate for
/// BlueZ D-Bus communication.
pub struct BluerAdapter {
    /// BlueZ session
    session: Session,
    /// BlueZ adapter
    adapter: Adapter,
    /// Configuration
    config: RwLock<Option<BleConfig>>,
    /// Internal state
    state: RwLock<AdapterState>,
    /// Advertisement handle (keeps advertisement alive)
    adv_handle: RwLock<Option<AdvertisementHandle>>,
    /// Discovery callback
    discovery_callback: RwLock<Option<DiscoveryCallback>>,
    /// Connection callback
    connection_callback: RwLock<Option<ConnectionCallback>>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

impl BluerAdapter {
    /// Create a new BlueZ adapter
    ///
    /// This connects to the system D-Bus and gets the default Bluetooth adapter.
    pub async fn new() -> Result<Self> {
        let session = Session::new().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to create BlueZ session: {}", e))
        })?;

        let adapter = session
            .default_adapter()
            .await
            .map_err(|e| BleError::AdapterNotAvailable)?;

        // Check if adapter is powered
        let powered = adapter.is_powered().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to check adapter power: {}", e))
        })?;

        if !powered {
            // Try to power on the adapter
            adapter.set_powered(true).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to power on adapter: {}", e))
            })?;
        }

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            session,
            adapter,
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            adv_handle: RwLock::new(None),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            shutdown_tx,
        })
    }

    /// Create adapter with a specific adapter name (e.g., "hci0")
    pub async fn with_adapter_name(name: &str) -> Result<Self> {
        let session = Session::new().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to create BlueZ session: {}", e))
        })?;

        let adapter = session.adapter(name).map_err(|e| {
            BleError::PlatformError(format!("Failed to get adapter '{}': {}", name, e))
        })?;

        let powered = adapter.is_powered().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to check adapter power: {}", e))
        })?;

        if !powered {
            adapter.set_powered(true).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to power on adapter: {}", e))
            })?;
        }

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            session,
            adapter,
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            adv_handle: RwLock::new(None),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            shutdown_tx,
        })
    }

    /// Get the adapter name (e.g., "hci0")
    pub fn adapter_name(&self) -> &str {
        self.adapter.name()
    }

    /// Build HIVE advertisement
    fn build_advertisement(&self, config: &BleConfig) -> Advertisement {
        let mut adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids: vec![HIVE_SERVICE_UUID].into_iter().collect(),
            local_name: Some(format!("HIVE-{:08X}", config.node_id.as_u32())),
            discoverable: Some(true),
            ..Default::default()
        };

        // Set TX power if supported
        adv.tx_power = Some(config.discovery.tx_power_dbm as i16);

        adv
    }

    /// Parse HIVE beacon from advertising data
    fn parse_hive_beacon(
        &self,
        address: Address,
        name: Option<String>,
        rssi: i16,
        service_data: &HashMap<bluer::Uuid, Vec<u8>>,
        manufacturer_data: &HashMap<u16, Vec<u8>>,
    ) -> Option<DiscoveredDevice> {
        // Check if this is a HIVE node by looking for our service UUID
        let is_hive = service_data.contains_key(&HIVE_SERVICE_UUID);

        // Try to extract node ID from name (HIVE-XXXXXXXX format)
        let node_id = name.as_ref().and_then(|n| {
            if n.starts_with("HIVE-") {
                NodeId::parse(&n[5..])
            } else {
                None
            }
        });

        Some(DiscoveredDevice {
            address: address.to_string(),
            name,
            rssi: rssi as i8,
            is_hive_node: is_hive || node_id.is_some(),
            node_id,
            adv_data: Vec::new(), // TODO: serialize full adv data
        })
    }

    /// Register node ID to address mapping
    pub async fn register_node_address(&self, node_id: NodeId, address: Address) {
        let mut state = self.state.write().await;
        state.address_to_node.insert(address, node_id.clone());
        state.node_to_address.insert(node_id, address);
    }

    /// Get address for a node ID
    pub async fn get_node_address(&self, node_id: &NodeId) -> Option<Address> {
        let state = self.state.read().await;
        state.node_to_address.get(node_id).copied()
    }
}

#[async_trait]
impl BleAdapter for BluerAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        *self.config.write().await = Some(config.clone());
        log::info!(
            "BluerAdapter initialized for node {:08X}",
            config.node_id.as_u32()
        );
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let config = self.config.read().await;
        let config = config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Start advertising
        self.start_advertising(&config.discovery).await?;

        // Start scanning
        self.start_scan(&config.discovery).await?;

        log::info!("BluerAdapter started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop advertising
        self.stop_advertising().await?;

        // Stop scanning
        self.stop_scan().await?;

        // Signal shutdown to background tasks
        let _ = self.shutdown_tx.send(());

        log::info!("BluerAdapter stopped");
        Ok(())
    }

    fn is_powered(&self) -> bool {
        // Use cached value or check synchronously
        // Note: bluer's is_powered() is async, so we'd need to poll it
        true // Assume powered since we check in new()
    }

    fn address(&self) -> Option<String> {
        // Would need async to get this from bluer
        None
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        use bluer::DiscoveryFilter;
        use bluer::DiscoveryTransport;

        let filter = DiscoveryFilter {
            transport: Some(DiscoveryTransport::Le),
            duplicate_data: Some(!config.filter_duplicates),
            ..Default::default()
        };

        self.adapter
            .set_discovery_filter(filter)
            .await
            .map_err(|e| {
                BleError::DiscoveryFailed(format!("Failed to set discovery filter: {}", e))
            })?;

        // Start discovery
        let discover =
            self.adapter.discover_devices().await.map_err(|e| {
                BleError::DiscoveryFailed(format!("Failed to start discovery: {}", e))
            })?;

        // Spawn task to handle discovered devices
        let callback = self.discovery_callback.read().await.clone();
        let state = Arc::new(self.state.read().await);
        let adapter = self.adapter.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let mut discover = std::pin::pin!(discover);

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        log::debug!("Discovery task shutting down");
                        break;
                    }
                    event = discover.next() => {
                        match event {
                            Some(bluer::AdapterEvent::DeviceAdded(addr)) => {
                                if let Ok(device) = adapter.device(addr) {
                                    // Get device properties
                                    let name = device.name().await.ok().flatten();
                                    let rssi = device.rssi().await.ok().flatten().unwrap_or(0);
                                    let service_data = device.service_data().await.ok().flatten().unwrap_or_default();
                                    let manufacturer_data = device.manufacturer_data().await.ok().flatten().unwrap_or_default();

                                    let discovered = DiscoveredDevice {
                                        address: addr.to_string(),
                                        name: name.clone(),
                                        rssi: rssi as i8,
                                        is_hive_node: name.as_ref().map(|n| n.starts_with("HIVE-")).unwrap_or(false),
                                        node_id: name.and_then(|n| {
                                            if n.starts_with("HIVE-") {
                                                NodeId::parse(&n[5..])
                                            } else {
                                                None
                                            }
                                        }),
                                        adv_data: Vec::new(),
                                    };

                                    log::debug!("Discovered device: {} (HIVE: {})",
                                        discovered.address, discovered.is_hive_node);

                                    if let Some(ref cb) = callback {
                                        cb(discovered);
                                    }
                                }
                            }
                            Some(bluer::AdapterEvent::DeviceRemoved(addr)) => {
                                log::debug!("Device removed: {}", addr);
                            }
                            None => break,
                            _ => {}
                        }
                    }
                }
            }
        });

        log::info!("BLE scanning started");
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        // Discovery is stopped when the stream is dropped
        // The shutdown signal will cause the task to exit
        log::info!("BLE scanning stopped");
        Ok(())
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        let ble_config = self.config.read().await;
        let ble_config = ble_config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let adv = self.build_advertisement(ble_config);

        let handle =
            self.adapter.advertise(adv).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to start advertising: {}", e))
            })?;

        *self.adv_handle.write().await = Some(handle);

        log::info!(
            "BLE advertising started for HIVE-{:08X}",
            ble_config.node_id.as_u32()
        );
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        // Drop the advertisement handle to stop advertising
        *self.adv_handle.write().await = None;
        log::info!("BLE advertising stopped");
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        // Use blocking write since this is a sync method
        // In practice, this should be called before start()
        if let Ok(mut cb) = self.discovery_callback.try_write() {
            *cb = callback;
        }
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        // Look up the address for this node ID
        let address = self
            .get_node_address(peer_id)
            .await
            .ok_or_else(|| BleError::ConnectionFailed(format!("Unknown node ID: {}", peer_id)))?;

        let device = self
            .adapter
            .device(address)
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to get device: {}", e)))?;

        // Connect to the device
        device
            .connect()
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        // Create connection wrapper
        let connection = BluerConnection::new(peer_id.clone(), device).await?;

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

        log::info!("Connected to peer {}", peer_id);
        Ok(Box::new(connection))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let connection = {
            let mut state = self.state.write().await;
            state.connections.remove(peer_id)
        };

        if let Some(conn) = connection {
            conn.disconnect().await?;

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
        // Use try_read to avoid blocking
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
        // TODO: Implement GATT server registration
        // This will be done in #405 (GATT Service Definition)
        log::warn!("GATT service registration not yet implemented");
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        // TODO: Implement GATT server unregistration
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Check if LE Coded PHY is supported
        // This would require checking adapter capabilities
        // For now, assume BLE 5.0+ adapters support it
        true
    }

    fn supports_extended_advertising(&self) -> bool {
        // Check if extended advertising is supported
        true
    }

    fn max_mtu(&self) -> u16 {
        // BlueZ typically supports up to 517 bytes MTU
        517
    }

    fn max_connections(&self) -> u8 {
        // BlueZ default is 7 connections
        7
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require actual Bluetooth hardware
    // They should be run with --ignored flag on a Linux system

    #[tokio::test]
    #[ignore = "Requires BlueZ and Bluetooth hardware"]
    async fn test_adapter_creation() {
        use super::*;

        let adapter = BluerAdapter::new().await;
        assert!(
            adapter.is_ok(),
            "Failed to create adapter: {:?}",
            adapter.err()
        );
    }

    #[tokio::test]
    #[ignore = "Requires BlueZ and Bluetooth hardware"]
    async fn test_adapter_init() {
        use super::*;
        use crate::BleConfig;

        let mut adapter = BluerAdapter::new().await.unwrap();
        let config = BleConfig::new(NodeId::new(0x12345678));
        let result = adapter.init(&config).await;
        assert!(result.is_ok());
    }
}
