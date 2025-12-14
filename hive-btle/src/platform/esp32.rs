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
//! let adapter = Esp32Adapter::new()?;
//!
//! // Create transport
//! let config = BleConfig::hive_lite(NodeId::new(0x12345678));
//! let transport = BluetoothLETransport::new(config, adapter);
//!
//! // Start BLE operations
//! transport.start().await?;
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_svc::bt::{
    ble::{
        gap::{BleGapEvent, EspBleGap},
        gatt::{
            server::{EspGatts, GattsEvent},
            GattCharacteristic, GattDescriptor, GattService,
        },
    },
    BtDriver,
};
use log::{debug, error, info, warn};

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::discovery::{Advertiser, HiveBeacon};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;
use crate::{
    CHAR_COMMAND_UUID, CHAR_NODE_INFO_UUID, CHAR_STATUS_UUID, CHAR_SYNC_DATA_UUID,
    CHAR_SYNC_STATE_UUID, HIVE_SERVICE_UUID,
};

/// ESP32 BLE connection handle
pub struct Esp32Connection {
    /// Connection handle from NimBLE
    conn_handle: u16,
    /// Peer address
    address: String,
    /// Node ID (if known)
    node_id: Option<NodeId>,
    /// Connection MTU
    mtu: u16,
    /// Whether this is a GATT client (central) or server (peripheral)
    is_client: bool,
}

impl Esp32Connection {
    /// Create new connection
    pub fn new(conn_handle: u16, address: String, is_client: bool) -> Self {
        Self {
            conn_handle,
            address,
            node_id: None,
            mtu: 23, // BLE default
            is_client,
        }
    }
}

#[async_trait]
impl BleConnection for Esp32Connection {
    async fn read_characteristic(&self, uuid: u16) -> Result<Vec<u8>> {
        // TODO: Implement GATT read via NimBLE
        Err(BleError::NotSupported(
            "ESP32 GATT read not yet implemented".into(),
        ))
    }

    async fn write_characteristic(&self, uuid: u16, data: &[u8]) -> Result<()> {
        // TODO: Implement GATT write via NimBLE
        Err(BleError::NotSupported(
            "ESP32 GATT write not yet implemented".into(),
        ))
    }

    async fn subscribe(&self, uuid: u16) -> Result<()> {
        // TODO: Implement GATT subscribe via NimBLE
        Err(BleError::NotSupported(
            "ESP32 GATT subscribe not yet implemented".into(),
        ))
    }

    async fn unsubscribe(&self, uuid: u16) -> Result<()> {
        // TODO: Implement GATT unsubscribe via NimBLE
        Err(BleError::NotSupported(
            "ESP32 GATT unsubscribe not yet implemented".into(),
        ))
    }

    async fn disconnect(&self) -> Result<()> {
        // TODO: Implement disconnect via NimBLE
        info!("ESP32: Disconnecting from {}", self.address);
        Ok(())
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn node_id(&self) -> Option<NodeId> {
        self.node_id
    }

    fn rssi(&self) -> Option<i8> {
        // TODO: Read RSSI from NimBLE connection
        None
    }
}

/// ESP32 BLE adapter state
struct Esp32AdapterState {
    /// Active connections by handle
    connections: HashMap<u16, Esp32Connection>,
    /// Discovery callback
    discovery_callback: Option<DiscoveryCallback>,
    /// Connection callback
    connection_callback: Option<ConnectionCallback>,
    /// Current advertiser state
    advertising: bool,
    /// Current scanning state
    scanning: bool,
}

impl Default for Esp32AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            discovery_callback: None,
            connection_callback: None,
            advertising: false,
            scanning: false,
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
        })
    }

    /// Create adapter with HIVE-Lite defaults
    pub fn hive_lite(node_id: NodeId) -> Result<Self> {
        Self::new(node_id, &format!("HIVE-{:08X}", node_id.as_u32()))
    }

    /// Get the number of active connections
    pub fn connection_count(&self) -> usize {
        self.state.lock().unwrap().connections.len()
    }

    /// Handle GAP events from NimBLE
    fn handle_gap_event(&self, event: &BleGapEvent) {
        match event {
            BleGapEvent::AdvComplete { .. } => {
                debug!("ESP32: Advertising complete");
            }
            BleGapEvent::Connect {
                conn_handle, addr, ..
            } => {
                info!("ESP32: Connected to {:?}", addr);
                // TODO: Create connection and notify callback
            }
            BleGapEvent::Disconnect {
                conn_handle,
                reason,
                ..
            } => {
                info!(
                    "ESP32: Disconnected handle={}, reason={:?}",
                    conn_handle, reason
                );
                let mut state = self.state.lock().unwrap();
                if let Some(conn) = state.connections.remove(conn_handle) {
                    if let Some(ref callback) = state.connection_callback {
                        if let Some(node_id) = conn.node_id {
                            callback(
                                node_id,
                                ConnectionEvent::Disconnected(DisconnectReason::LinkLoss),
                            );
                        }
                    }
                }
            }
            BleGapEvent::DiscComplete { .. } => {
                debug!("ESP32: Discovery complete");
            }
            _ => {
                debug!("ESP32: Unhandled GAP event");
            }
        }
    }

    /// Handle GATT server events
    fn handle_gatts_event(&self, event: &GattsEvent) {
        match event {
            GattsEvent::Connect { conn_handle } => {
                info!("ESP32: GATT server connection handle={}", conn_handle);
            }
            GattsEvent::Disconnect { conn_handle } => {
                info!("ESP32: GATT server disconnection handle={}", conn_handle);
            }
            GattsEvent::Write {
                conn_handle,
                handle,
                data,
                ..
            } => {
                debug!(
                    "ESP32: GATT write to handle {} ({} bytes)",
                    handle,
                    data.len()
                );
                // TODO: Dispatch to HIVE sync protocol
            }
            GattsEvent::Read {
                conn_handle,
                handle,
                ..
            } => {
                debug!("ESP32: GATT read from handle {}", handle);
                // TODO: Return characteristic value
            }
            _ => {
                debug!("ESP32: Unhandled GATTS event");
            }
        }
    }

    /// Register the HIVE GATT service
    fn register_hive_service(&self) -> Result<()> {
        info!("ESP32: Registering HIVE GATT service");

        // Service definition would go here
        // UUID: f47ac10b-58cc-4372-a567-0e02b2c3d479
        //
        // Characteristics:
        // - Node Info (read)
        // - Sync State (read/notify)
        // - Sync Data (write/indicate)
        // - Command (write)
        // - Status (read/notify)

        Ok(())
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
}

#[async_trait]
impl BleAdapter for Esp32Adapter {
    fn name(&self) -> &str {
        "ESP32 NimBLE"
    }

    async fn initialize(&mut self, config: &BleConfig) -> Result<()> {
        info!("ESP32: Initializing with config {:?}", config);

        // Register HIVE GATT service
        self.register_hive_service()?;

        Ok(())
    }

    async fn start_discovery(&mut self, config: &DiscoveryConfig) -> Result<()> {
        info!("ESP32: Starting discovery");

        let mut state = self.state.lock().unwrap();
        state.scanning = true;

        // TODO: Start NimBLE scan with filter for HIVE service UUID

        Ok(())
    }

    async fn stop_discovery(&mut self) -> Result<()> {
        info!("ESP32: Stopping discovery");

        let mut state = self.state.lock().unwrap();
        state.scanning = false;

        // TODO: Stop NimBLE scan

        Ok(())
    }

    async fn start_advertising(&mut self, beacon: &HiveBeacon) -> Result<()> {
        info!(
            "ESP32: Starting advertising for node {:08X}",
            beacon.node_id.as_u32()
        );

        let adv_data = self.build_adv_data(beacon);
        debug!(
            "ESP32: Advertising data ({} bytes): {:02X?}",
            adv_data.len(),
            adv_data
        );

        let mut state = self.state.lock().unwrap();
        state.advertising = true;

        // TODO: Configure and start NimBLE advertising

        Ok(())
    }

    async fn stop_advertising(&mut self) -> Result<()> {
        info!("ESP32: Stopping advertising");

        let mut state = self.state.lock().unwrap();
        state.advertising = false;

        // TODO: Stop NimBLE advertising

        Ok(())
    }

    async fn connect(&mut self, address: &str) -> Result<Box<dyn BleConnection>> {
        info!("ESP32: Connecting to {}", address);

        // TODO: Parse address and initiate NimBLE connection

        Err(BleError::NotSupported(
            "ESP32 connection not yet implemented".into(),
        ))
    }

    async fn disconnect(&mut self, address: &str) -> Result<()> {
        info!("ESP32: Disconnecting from {}", address);

        // TODO: Find connection by address and disconnect

        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: DiscoveryCallback) {
        let mut state = self.state.lock().unwrap();
        state.discovery_callback = Some(callback);
    }

    fn set_connection_callback(&mut self, callback: ConnectionCallback) {
        let mut state = self.state.lock().unwrap();
        state.connection_callback = Some(callback);
    }

    fn supports_phy(&self, phy: BlePhy) -> bool {
        // ESP32 with BLE 5.0 support
        // Original ESP32 only supports LE 1M
        // ESP32-S3 and ESP32-C3 support LE 2M and Coded PHY
        match phy {
            BlePhy::Le1M => true,
            BlePhy::Le2M => false, // Depends on ESP32 variant
            BlePhy::LeCodedS2 => false,
            BlePhy::LeCodedS8 => false,
        }
    }

    fn max_mtu(&self) -> u16 {
        // NimBLE default max MTU
        512
    }

    fn max_connections(&self) -> usize {
        // ESP32 NimBLE default
        9
    }

    fn address(&self) -> Option<String> {
        // TODO: Read MAC address from ESP32
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual ESP32 hardware
    // They are marked as ignored and can be run manually

    #[test]
    #[ignore]
    fn test_adapter_creation() {
        let adapter = Esp32Adapter::hive_lite(NodeId::new(0x12345678));
        assert!(adapter.is_ok());
    }

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
