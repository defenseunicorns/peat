//! Android BLE connection wrapper
//!
//! This module provides the `AndroidConnection` which wraps a BluetoothGatt
//! connection and implements the `BleConnection` trait.

use jni::objects::GlobalRef;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

/// Internal connection state
struct ConnectionState {
    /// Whether the connection is alive
    alive: bool,
    /// Negotiated MTU
    mtu: u16,
    /// Current PHY
    phy: BlePhy,
    /// Last RSSI reading
    rssi: Option<i8>,
    /// Whether services have been discovered
    services_discovered: bool,
}

/// Android BLE connection wrapper
///
/// Wraps a BluetoothGatt connection with state tracking and
/// implements the `BleConnection` trait.
#[derive(Clone)]
pub struct AndroidConnection {
    /// Remote peer ID
    peer_id: NodeId,
    /// Remote device address
    address: String,
    /// BluetoothGatt handle (JNI global ref)
    gatt: Arc<GlobalRef>,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// When the connection was established
    connected_at: Instant,
}

impl AndroidConnection {
    /// Create a new connection wrapper
    pub(crate) fn new(peer_id: NodeId, address: String, gatt: GlobalRef) -> Self {
        let state = ConnectionState {
            alive: true,
            mtu: 23,           // Default BLE MTU, will be updated after MTU exchange
            phy: BlePhy::Le1M, // Default PHY
            rssi: None,
            services_discovered: false,
        };

        Self {
            peer_id,
            address,
            gatt: Arc::new(gatt),
            state: Arc::new(RwLock::new(state)),
            connected_at: Instant::now(),
        }
    }

    /// Get the device address
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Get the BluetoothGatt handle
    pub fn gatt(&self) -> &GlobalRef {
        &self.gatt
    }

    /// Update connection state from JNI callback
    pub async fn update_connection_state(&self, connected: bool) {
        let mut state = self.state.write().await;
        state.alive = connected;
    }

    /// Update MTU from JNI callback
    pub async fn update_mtu(&self, mtu: u16) {
        let mut state = self.state.write().await;
        state.mtu = mtu;
        log::debug!("MTU updated to {} for peer {}", mtu, self.peer_id);
    }

    /// Update PHY from JNI callback
    pub async fn update_phy(&self, phy: BlePhy) {
        let mut state = self.state.write().await;
        state.phy = phy;
        log::debug!("PHY updated to {:?} for peer {}", phy, self.peer_id);
    }

    /// Update RSSI from JNI callback
    pub async fn update_rssi(&self, rssi: i8) {
        let mut state = self.state.write().await;
        state.rssi = Some(rssi);
    }

    /// Mark services as discovered
    pub async fn mark_services_discovered(&self) {
        let mut state = self.state.write().await;
        state.services_discovered = true;
        log::debug!("Services discovered for peer {}", self.peer_id);
    }

    /// Check if services have been discovered
    pub async fn are_services_discovered(&self) -> bool {
        let state = self.state.read().await;
        state.services_discovered
    }

    /// Mark connection as dead
    pub async fn mark_dead(&self) {
        let mut state = self.state.write().await;
        state.alive = false;
    }

    /// Disconnect from the device
    ///
    /// This calls BluetoothGatt.disconnect() followed by BluetoothGatt.close()
    pub async fn disconnect(&self) -> Result<()> {
        // TODO: Implement JNI disconnect
        // 1. Call gatt.disconnect()
        // 2. Wait for onConnectionStateChange callback
        // 3. Call gatt.close()
        self.mark_dead().await;
        log::warn!(
            "Android disconnect not yet implemented for {}",
            self.address
        );
        Ok(())
    }

    /// Request MTU change
    ///
    /// Requests an MTU change via BluetoothGatt.requestMtu()
    /// The result will be delivered via onMtuChanged callback
    pub async fn request_mtu(&self, mtu: u16) -> Result<()> {
        // TODO: Implement JNI MTU request
        // Call gatt.requestMtu(mtu)
        log::warn!(
            "Android MTU request not yet implemented (requested: {})",
            mtu
        );
        Err(BleError::NotSupported(
            "MTU request not yet implemented".to_string(),
        ))
    }

    /// Request PHY change
    ///
    /// Requests a PHY change via BluetoothGatt.setPreferredPhy()
    /// Available PHYs depend on device capabilities
    pub async fn request_phy(&self, tx_phy: BlePhy, rx_phy: BlePhy) -> Result<()> {
        // TODO: Implement JNI PHY request
        // Call gatt.setPreferredPhy(txPhy, rxPhy, PHY_OPTION_NO_PREFERRED)
        log::warn!(
            "Android PHY request not yet implemented (tx: {:?}, rx: {:?})",
            tx_phy,
            rx_phy
        );
        Err(BleError::NotSupported(
            "PHY request not yet implemented".to_string(),
        ))
    }

    /// Read RSSI
    ///
    /// Reads the current RSSI via BluetoothGatt.readRemoteRssi()
    /// The result will be delivered via onReadRemoteRssi callback
    pub async fn read_rssi(&self) -> Result<()> {
        // TODO: Implement JNI RSSI read
        // Call gatt.readRemoteRssi()
        log::warn!("Android RSSI read not yet implemented");
        Err(BleError::NotSupported(
            "RSSI read not yet implemented".to_string(),
        ))
    }

    /// Discover services
    ///
    /// Triggers service discovery via BluetoothGatt.discoverServices()
    /// The result will be delivered via onServicesDiscovered callback
    pub async fn discover_services(&self) -> Result<()> {
        // TODO: Implement JNI service discovery
        // Call gatt.discoverServices()
        log::warn!("Android service discovery not yet implemented");
        Err(BleError::NotSupported(
            "Service discovery not yet implemented".to_string(),
        ))
    }

    /// Read a characteristic value
    ///
    /// # Arguments
    /// * `service_uuid` - UUID of the GATT service
    /// * `char_uuid` - UUID of the characteristic to read
    ///
    /// The result will be delivered via onCharacteristicRead callback
    pub async fn read_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<Vec<u8>> {
        // TODO: Implement JNI characteristic read
        // 1. Get service: gatt.getService(serviceUuid)
        // 2. Get characteristic: service.getCharacteristic(charUuid)
        // 3. Call gatt.readCharacteristic(characteristic)
        // 4. Wait for onCharacteristicRead callback
        log::warn!(
            "Android characteristic read not yet implemented (service: {}, char: {})",
            service_uuid,
            char_uuid
        );
        Err(BleError::NotSupported(
            "Characteristic read not yet implemented".to_string(),
        ))
    }

    /// Write a characteristic value
    ///
    /// # Arguments
    /// * `service_uuid` - UUID of the GATT service
    /// * `char_uuid` - UUID of the characteristic to write
    /// * `value` - Data to write
    /// * `write_type` - Write type (WRITE_TYPE_DEFAULT, WRITE_TYPE_NO_RESPONSE, etc.)
    pub async fn write_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
        value: &[u8],
    ) -> Result<()> {
        // TODO: Implement JNI characteristic write
        // 1. Get service: gatt.getService(serviceUuid)
        // 2. Get characteristic: service.getCharacteristic(charUuid)
        // 3. Set value: characteristic.setValue(value)
        // 4. Call gatt.writeCharacteristic(characteristic)
        // 5. Wait for onCharacteristicWrite callback
        log::warn!(
            "Android characteristic write not yet implemented (service: {}, char: {}, len: {})",
            service_uuid,
            char_uuid,
            value.len()
        );
        Err(BleError::NotSupported(
            "Characteristic write not yet implemented".to_string(),
        ))
    }

    /// Enable notifications for a characteristic
    ///
    /// # Arguments
    /// * `service_uuid` - UUID of the GATT service
    /// * `char_uuid` - UUID of the characteristic
    pub async fn enable_notifications(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<()> {
        // TODO: Implement JNI notification enablement
        // 1. Get service: gatt.getService(serviceUuid)
        // 2. Get characteristic: service.getCharacteristic(charUuid)
        // 3. Enable local notifications: gatt.setCharacteristicNotification(characteristic, true)
        // 4. Get CCCD descriptor: characteristic.getDescriptor(CLIENT_CHARACTERISTIC_CONFIG_UUID)
        // 5. Set descriptor value: descriptor.setValue(BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
        // 6. Write descriptor: gatt.writeDescriptor(descriptor)
        log::warn!(
            "Android notification enablement not yet implemented (service: {}, char: {})",
            service_uuid,
            char_uuid
        );
        Err(BleError::NotSupported(
            "Notification enablement not yet implemented".to_string(),
        ))
    }

    /// Disable notifications for a characteristic
    pub async fn disable_notifications(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<()> {
        // TODO: Implement JNI notification disablement
        log::warn!(
            "Android notification disablement not yet implemented (service: {}, char: {})",
            service_uuid,
            char_uuid
        );
        Err(BleError::NotSupported(
            "Notification disablement not yet implemented".to_string(),
        ))
    }
}

impl BleConnection for AndroidConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        if let Ok(state) = self.state.try_read() {
            state.alive
        } else {
            // If we can't get the lock, assume alive
            true
        }
    }

    fn mtu(&self) -> u16 {
        if let Ok(state) = self.state.try_read() {
            state.mtu
        } else {
            23 // Default BLE MTU
        }
    }

    fn phy(&self) -> BlePhy {
        if let Ok(state) = self.state.try_read() {
            state.phy
        } else {
            BlePhy::Le1M
        }
    }

    fn rssi(&self) -> Option<i8> {
        if let Ok(state) = self.state.try_read() {
            state.rssi
        } else {
            None
        }
    }

    fn connected_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    // Android tests require instrumentation test environment
    //
    // Connection tests would need:
    // - Real Android device or emulator
    // - Another BLE device to connect to
    // - Proper JNI environment setup
}
