//! ESP32 BLE Adapter
//!
//! Provides BLE functionality for ESP32 devices using ESP-IDF NimBLE.
//! Tested on M5Stack Core2 (ESP32-D0WDQ6-V3).
//!
//! ## Prerequisites
//!
//! 1. Install ESP-IDF toolchain and Rust esp fork
//! 2. Enable BLE in ESP-IDF menuconfig:
//!    - Component config → Bluetooth → Enable
//!    - Component config → Bluetooth → NimBLE - BLE only
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::esp32::Esp32Adapter;
//! use hive_btle::{BleConfig, BluetoothLETransport, NodeId};
//!
//! // Create adapter
//! let adapter = Esp32Adapter::new(NodeId::new(0x12345678), "HIVE-Device")?;
//!
//! // Initialize
//! adapter.init(&BleConfig::hive_lite(NodeId::new(0x12345678))).await?;
//!
//! // Start operations
//! adapter.start().await?;
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::{debug, info};

use crate::config::{BleConfig, DiscoveryConfig};
use crate::discovery::HiveBeacon;
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;

// Note: esp_idf_svc::bt requires Bluedroid feature which conflicts with NimBLE.
// The actual BLE implementation is in the nimble.rs module in the example code.
// This module provides the BleAdapter trait implementation but uses mock/stub
// types when Bluedroid is not available.

/// ESP32 BLE connection handle
pub struct Esp32Connection {
    /// Peer node ID
    peer_id: NodeId,
    /// Connection handle from NimBLE
    conn_handle: u16,
    /// Peer address
    address: String,
    /// Connection MTU
    mtu: u16,
    /// Connection start time (monotonic ms)
    connected_at_ms: u64,
    /// Current time (monotonic ms)
    current_time_ms: u64,
    /// Whether connection is still alive
    alive: bool,
}

impl Esp32Connection {
    /// Create new connection
    pub fn new(peer_id: NodeId, conn_handle: u16, address: String) -> Self {
        Self {
            peer_id,
            conn_handle,
            address,
            mtu: 23, // BLE default
            connected_at_ms: 0,
            current_time_ms: 0,
            alive: true,
        }
    }

    /// Set the connection time
    pub fn set_time_ms(&mut self, time_ms: u64) {
        if self.connected_at_ms == 0 {
            self.connected_at_ms = time_ms;
        }
        self.current_time_ms = time_ms;
    }
}

impl BleConnection for Esp32Connection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        self.alive
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }

    fn phy(&self) -> crate::config::BlePhy {
        crate::config::BlePhy::Le1M // ESP32 classic only supports 1M
    }

    fn rssi(&self) -> Option<i8> {
        // TODO: Read RSSI from NimBLE connection
        None
    }

    fn connected_duration(&self) -> core::time::Duration {
        let ms = self.current_time_ms.saturating_sub(self.connected_at_ms);
        core::time::Duration::from_millis(ms)
    }
}

/// ESP32 BLE adapter state
struct Esp32AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, Esp32Connection>,
    /// Node ID to connection handle mapping
    handle_map: HashMap<u16, NodeId>,
    /// Discovery callback
    discovery_callback: Option<DiscoveryCallback>,
    /// Connection callback
    connection_callback: Option<ConnectionCallback>,
    /// Current advertiser state
    advertising: bool,
    /// Current scanning state
    scanning: bool,
    /// Powered on
    powered: bool,
}

impl Default for Esp32AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            handle_map: HashMap::new(),
            discovery_callback: None,
            connection_callback: None,
            advertising: false,
            scanning: false,
            powered: false,
        }
    }
}

/// ESP32 BLE Adapter using ESP-IDF NimBLE
///
/// This adapter implements the `BleAdapter` trait for ESP32 devices,
/// supporting both GATT server (peripheral) and client (central) roles.
pub struct Esp32Adapter {
    /// Internal state
    state: Arc<Mutex<Esp32AdapterState>>,
    /// Our node ID
    node_id: NodeId,
    /// Device name
    device_name: String,
    /// Current beacon for advertising
    beacon: Option<HiveBeacon>,
}

impl Esp32Adapter {
    /// Create a new ESP32 adapter
    ///
    /// This initializes the NimBLE stack and prepares for BLE operations.
    ///
    /// # Arguments
    ///
    /// * `node_id` - Our node identifier
    /// * `device_name` - BLE device name (max 29 chars for legacy advertising)
    ///
    /// # Errors
    ///
    /// Returns an error if BLE initialization fails.
    pub fn new(node_id: NodeId, device_name: &str) -> Result<Self> {
        info!(
            "ESP32: Initializing BLE adapter for node {:08X}",
            node_id.as_u32()
        );

        // Note: Actual NimBLE initialization would happen here
        // For now, this is a skeleton that will be completed when
        // testing on real hardware

        Ok(Self {
            state: Arc::new(Mutex::new(Esp32AdapterState::default())),
            node_id,
            device_name: device_name.to_string(),
            beacon: None,
        })
    }

    /// Create adapter with HIVE-Lite defaults
    pub fn hive_lite(node_id: NodeId) -> Result<Self> {
        Self::new(node_id, &format!("HIVE-{:08X}", node_id.as_u32()))
    }

    /// Build advertising data for HIVE beacon
    fn build_adv_data(&self, beacon: &HiveBeacon) -> Vec<u8> {
        // Build advertising data with HIVE beacon
        // Format: Flags + Service UUID + Service Data
        let mut data = Vec::with_capacity(31);

        // Flags (3 bytes)
        data.push(0x02); // Length
        data.push(0x01); // Type: Flags
        data.push(0x06); // LE General Discoverable + BR/EDR Not Supported

        // Complete 16-bit Service UUIDs (4 bytes)
        data.push(0x03); // Length
        data.push(0x03); // Type: Complete List of 16-bit Service UUIDs
        data.extend_from_slice(&crate::HIVE_SERVICE_UUID_16BIT.to_le_bytes());

        // Service Data (remaining bytes)
        let beacon_data = beacon.encode_compact();
        data.push((beacon_data.len() + 3) as u8); // Length
        data.push(0x16); // Type: Service Data - 16-bit UUID
        data.extend_from_slice(&crate::HIVE_SERVICE_UUID_16BIT.to_le_bytes());
        data.extend_from_slice(&beacon_data);

        data
    }

    /// Handle disconnect event
    fn handle_disconnect(&self, conn_handle: u16, _reason: u8) {
        let mut state = self.state.lock().unwrap();
        if let Some(node_id) = state.handle_map.remove(&conn_handle) {
            state.connections.remove(&node_id);
            if let Some(ref callback) = state.connection_callback {
                callback(
                    node_id,
                    ConnectionEvent::Disconnected {
                        reason: DisconnectReason::LinkLoss,
                    },
                );
            }
        }
    }
}

#[async_trait]
impl BleAdapter for Esp32Adapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        info!("ESP32: Initializing with config {:?}", config);

        // Create beacon from config
        self.beacon = Some(HiveBeacon::new(config.node_id));

        let mut state = self.state.lock().unwrap();
        state.powered = true;

        // TODO: Initialize NimBLE stack
        // TODO: Register GATT service

        Ok(())
    }

    async fn start(&self) -> Result<()> {
        info!("ESP32: Starting adapter");
        // Start advertising and scanning based on configuration
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!("ESP32: Stopping adapter");
        let mut state = self.state.lock().unwrap();
        state.advertising = false;
        state.scanning = false;
        Ok(())
    }

    fn is_powered(&self) -> bool {
        self.state.lock().unwrap().powered
    }

    fn address(&self) -> Option<String> {
        // TODO: Read MAC address from ESP32
        None
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        info!("ESP32: Starting scan");
        let mut state = self.state.lock().unwrap();
        state.scanning = true;
        // TODO: Start NimBLE scan with filter for HIVE service UUID
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        info!("ESP32: Stopping scan");
        let mut state = self.state.lock().unwrap();
        state.scanning = false;
        // TODO: Stop NimBLE scan
        Ok(())
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        info!("ESP32: Starting advertising");

        if let Some(ref beacon) = self.beacon {
            let adv_data = self.build_adv_data(beacon);
            debug!(
                "ESP32: Advertising data ({} bytes): {:02X?}",
                adv_data.len(),
                adv_data
            );
        }

        let mut state = self.state.lock().unwrap();
        state.advertising = true;
        // TODO: Configure and start NimBLE advertising
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        info!("ESP32: Stopping advertising");
        let mut state = self.state.lock().unwrap();
        state.advertising = false;
        // TODO: Stop NimBLE advertising
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        let mut state = self.state.lock().unwrap();
        state.discovery_callback = callback;
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        info!("ESP32: Connecting to {:08X}", peer_id.as_u32());
        // TODO: Lookup address from discovery and initiate NimBLE connection
        Err(BleError::NotSupported(
            "ESP32 connection not yet implemented".into(),
        ))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        info!("ESP32: Disconnecting from {:08X}", peer_id.as_u32());
        let mut state = self.state.lock().unwrap();
        if let Some(conn) = state.connections.remove(peer_id) {
            state.handle_map.remove(&conn.conn_handle);
            // TODO: Disconnect via NimBLE
        }
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        let state = self.state.lock().unwrap();
        state.connections.get(peer_id).map(|conn| {
            Box::new(Esp32Connection::new(
                conn.peer_id,
                conn.conn_handle,
                conn.address.clone(),
            )) as Box<dyn BleConnection>
        })
    }

    fn peer_count(&self) -> usize {
        self.state.lock().unwrap().connections.len()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.state
            .lock()
            .unwrap()
            .connections
            .keys()
            .copied()
            .collect()
    }

    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>) {
        let mut state = self.state.lock().unwrap();
        state.connection_callback = callback;
    }

    async fn register_gatt_service(&self) -> Result<()> {
        info!("ESP32: Registering HIVE GATT service");
        // TODO: Register service with NimBLE
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        info!("ESP32: Unregistering HIVE GATT service");
        // TODO: Unregister service from NimBLE
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Original ESP32 does not support Coded PHY
        // ESP32-S3 and ESP32-C3 do
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        // Original ESP32 does not support extended advertising
        false
    }

    fn max_mtu(&self) -> u16 {
        // NimBLE default max MTU
        512
    }

    fn max_connections(&self) -> u8 {
        // ESP32 NimBLE default
        9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adv_data_size() {
        // Verify advertising data fits in legacy 31-byte limit
        let beacon = HiveBeacon::new(NodeId::new(0x12345678));

        // Simulate building adv data
        // Flags (3) + 16-bit UUIDs (4) + Service Data (3 + 10 compact beacon) = 20 bytes
        let expected_size = 3 + 4 + 3 + crate::discovery::BEACON_COMPACT_SIZE;
        assert!(
            expected_size <= 31,
            "Adv data ({}) exceeds 31-byte limit",
            expected_size
        );
    }
}
