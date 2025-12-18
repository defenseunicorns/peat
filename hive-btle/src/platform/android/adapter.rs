//! Android BLE adapter implementation
//!
//! This module provides the `AndroidAdapter` which implements `BleAdapter`
//! using JNI bindings to the Android Bluetooth API.

use async_trait::async_trait;
use jni::objects::JObject;
use jni::JNIEnv;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DiscoveredDevice, DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;

use super::connection::AndroidConnection;
use super::jni_bridge::JniBridge;

/// Internal state for the adapter
struct AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, AndroidConnection>,
    /// Device address to node ID mapping
    address_to_node: HashMap<String, NodeId>,
    /// Node ID to device address mapping
    node_to_address: HashMap<NodeId, String>,
    /// Discovered devices (address -> device info)
    discovered: HashMap<String, DiscoveredDevice>,
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

/// Android BLE adapter
///
/// Implements the `BleAdapter` trait using JNI bindings to Android Bluetooth APIs.
///
/// # Requirements
///
/// - Android 6.0 (API 23) or later
/// - BLUETOOTH, BLUETOOTH_ADMIN permissions
/// - ACCESS_FINE_LOCATION permission (for scanning)
/// - For BLE 5.0 features: Android 8.0 (API 26)+
///
/// # Example
///
/// ```ignore
/// // From Android Activity or Service with JNI environment
/// let mut adapter = AndroidAdapter::new(&mut env, context)?;
/// adapter.init(&config).await?;
/// adapter.start().await?;
/// ```
pub struct AndroidAdapter {
    /// JNI bridge for Android Bluetooth API calls
    jni_bridge: Arc<RwLock<JniBridge>>,
    /// Configuration
    config: RwLock<Option<BleConfig>>,
    /// Internal state
    state: RwLock<AdapterState>,
    /// Discovery callback
    discovery_callback: RwLock<Option<DiscoveryCallback>>,
    /// Connection callback
    connection_callback: RwLock<Option<ConnectionCallback>>,
    /// Channel receiver for scan results (from JNI callbacks)
    scan_rx: RwLock<mpsc::Receiver<DiscoveredDevice>>,
    /// Channel receiver for connection events (from JNI callbacks)
    connection_rx: RwLock<mpsc::Receiver<(NodeId, ConnectionEvent)>>,
    /// Whether scanning is active
    scanning: RwLock<bool>,
    /// Whether advertising is active
    advertising: RwLock<bool>,
}

impl AndroidAdapter {
    /// Create a new Android BLE adapter
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `env` is a valid JNI environment
    /// - `context` is a valid Android Context (Activity, Service, or Application)
    /// - This is called from the Android main thread or a thread attached to the JVM
    ///
    /// # Arguments
    ///
    /// * `env` - JNI environment from Android runtime
    /// * `context` - Android Context for accessing system services
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Bluetooth adapter is not available on the device
    /// - Required permissions are not granted
    /// - JNI initialization fails
    pub unsafe fn new(env: &mut JNIEnv, context: JObject) -> Result<Self> {
        // Create channels for JNI callbacks
        let (scan_tx, scan_rx) = mpsc::channel(100);
        let (connection_tx, connection_rx) = mpsc::channel(100);

        // Create JNI bridge
        let jni_bridge = JniBridge::new(env, context, scan_tx, connection_tx)?;

        Ok(Self {
            jni_bridge: Arc::new(RwLock::new(jni_bridge)),
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            scan_rx: RwLock::new(scan_rx),
            connection_rx: RwLock::new(connection_rx),
            scanning: RwLock::new(false),
            advertising: RwLock::new(false),
        })
    }

    /// Register node ID to address mapping
    pub async fn register_node_address(&self, node_id: NodeId, address: String) {
        let mut state = self.state.write().await;
        state
            .address_to_node
            .insert(address.clone(), node_id.clone());
        state.node_to_address.insert(node_id, address);
    }

    /// Get address for a node ID
    pub async fn get_node_address(&self, node_id: &NodeId) -> Option<String> {
        let state = self.state.read().await;
        state.node_to_address.get(node_id).cloned()
    }

    /// Process scan result from JNI callback
    async fn process_scan_result(&self, device: DiscoveredDevice) {
        // Store in discovered map
        {
            let mut state = self.state.write().await;
            state
                .discovered
                .insert(device.address.clone(), device.clone());

            // If it's a HIVE node, register the mapping
            if let Some(node_id) = &device.node_id {
                state
                    .address_to_node
                    .insert(device.address.clone(), node_id.clone());
                state
                    .node_to_address
                    .insert(node_id.clone(), device.address.clone());
            }
        }

        // Invoke callback
        if let Some(ref cb) = *self.discovery_callback.read().await {
            cb(device);
        }
    }

    /// Process connection event from JNI callback
    async fn process_connection_event(&self, node_id: NodeId, event: ConnectionEvent) {
        // Update connection state
        match &event {
            ConnectionEvent::Disconnected { .. } => {
                let mut state = self.state.write().await;
                state.connections.remove(&node_id);
            }
            _ => {}
        }

        // Invoke callback
        if let Some(ref cb) = *self.connection_callback.read().await {
            cb(node_id, event);
        }
    }

    /// Start background task to process JNI callbacks
    fn start_callback_processor(&self) -> tokio::task::JoinHandle<()> {
        let scan_rx = self.scan_rx.clone();
        let connection_rx = self.connection_rx.clone();
        let discovery_callback = self.discovery_callback.clone();
        let connection_callback = self.connection_callback.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            // TODO: Implement callback processing loop
            // This will process messages from JNI callbacks and invoke Rust callbacks
            log::debug!("Android callback processor started (not yet fully implemented)");
        })
    }
}

#[async_trait]
impl BleAdapter for AndroidAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        // Initialize JNI bridge
        {
            let mut bridge = self.jni_bridge.write().await;
            bridge.init_adapter()?;
        }

        // Store config
        *self.config.write().await = Some(config.clone());

        log::info!(
            "AndroidAdapter initialized for node {:08X}",
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

        log::info!("AndroidAdapter started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop advertising
        self.stop_advertising().await?;

        // Stop scanning
        self.stop_scan().await?;

        log::info!("AndroidAdapter stopped");
        Ok(())
    }

    fn is_powered(&self) -> bool {
        // Check via JNI bridge
        if let Ok(bridge) = self.jni_bridge.try_read() {
            bridge.is_enabled().unwrap_or(false)
        } else {
            false
        }
    }

    fn address(&self) -> Option<String> {
        if let Ok(bridge) = self.jni_bridge.try_read() {
            bridge.get_address().ok().flatten()
        } else {
            None
        }
    }

    async fn start_scan(&self, _config: &DiscoveryConfig) -> Result<()> {
        let bridge = self.jni_bridge.read().await;
        bridge.start_scan()?;
        *self.scanning.write().await = true;
        log::info!("Android BLE scanning started");
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        let bridge = self.jni_bridge.read().await;
        bridge.stop_scan()?;
        *self.scanning.write().await = false;
        log::info!("Android BLE scanning stopped");
        Ok(())
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        let ble_config = self.config.read().await;
        let ble_config = ble_config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let bridge = self.jni_bridge.read().await;
        bridge.start_advertising(ble_config.node_id.as_u32(), config.tx_power_dbm)?;
        *self.advertising.write().await = true;

        log::info!(
            "Android BLE advertising started for HIVE-{:08X}",
            ble_config.node_id.as_u32()
        );
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        let bridge = self.jni_bridge.read().await;
        bridge.stop_advertising()?;
        *self.advertising.write().await = false;
        log::info!("Android BLE advertising stopped");
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
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

        // Connect via JNI
        let bridge = self.jni_bridge.read().await;
        let gatt_ref = bridge.connect_device(&address)?;

        // Create connection wrapper
        let connection = AndroidConnection::new(peer_id.clone(), address.clone(), gatt_ref);

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

        log::info!("Connected to peer {} at {}", peer_id, address);
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
                        reason: crate::platform::DisconnectReason::LocalRequest,
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
        // TODO: Implement GATT server registration
        // 1. Create BluetoothGattServer via BluetoothManager.openGattServer()
        // 2. Create BluetoothGattService with HIVE_SERVICE_UUID
        // 3. Add characteristics for each HIVE characteristic
        // 4. Call gattServer.addService(service)
        log::warn!("Android GATT service registration not yet implemented");
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        // TODO: Implement GATT server cleanup
        // Call gattServer.close()
        log::warn!("Android GATT service unregistration not yet implemented");
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Check if device supports LE Coded PHY (BLE 5.0+)
        // Requires Android 8.0 (API 26) and hardware support
        // TODO: Check via BluetoothAdapter.isLeCodedPhySupported()
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        // Check if device supports extended advertising (BLE 5.0+)
        // Requires Android 8.0 (API 26) and hardware support
        // TODO: Check via BluetoothAdapter.isLeExtendedAdvertisingSupported()
        false
    }

    fn max_mtu(&self) -> u16 {
        // Android supports up to 517 bytes MTU (BLE 5.0+)
        // Older devices may be limited to 185 or 247 bytes
        517
    }

    fn max_connections(&self) -> u8 {
        // Android typically supports 7 simultaneous BLE connections
        // Actual limit depends on device/chipset
        7
    }
}

#[cfg(test)]
mod tests {
    // Android tests require instrumentation test environment
    // They should be run via `./gradlew connectedAndroidTest`
    //
    // Example test structure:
    //
    // #[test]
    // fn test_adapter_creation() {
    //     // This would be called from Android JUnit test
    //     // with proper JNI environment setup
    // }
}
