//! CoreBluetooth connection wrapper
//!
//! This module provides a connection wrapper for CoreBluetooth peripherals,
//! implementing the `BleConnection` trait.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

use super::delegates::{PeripheralDelegate, PeripheralEvent};

/// Internal connection state
struct ConnectionState {
    /// Whether the connection is alive
    alive: bool,
    /// Negotiated MTU
    mtu: u16,
    /// Current PHY (CoreBluetooth doesn't expose PHY, assume 1M)
    phy: BlePhy,
    /// Last RSSI reading
    rssi: Option<i8>,
    /// Whether services have been discovered
    services_discovered: bool,
}

/// CoreBluetooth connection wrapper
///
/// Wraps a CBPeripheral connection with state tracking and
/// implements the `BleConnection` trait.
#[derive(Clone)]
pub struct CoreBluetoothConnection {
    /// Remote peer ID
    peer_id: NodeId,
    /// Peripheral identifier (UUID string)
    identifier: String,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// When the connection was established
    connected_at: Instant,
    /// Channel receiver for peripheral events
    event_rx: Arc<RwLock<mpsc::Receiver<PeripheralEvent>>>,
    /// Peripheral delegate (must be kept alive)
    delegate: Arc<PeripheralDelegate>,
}

impl CoreBluetoothConnection {
    /// Create a new connection wrapper
    ///
    /// # Arguments
    /// * `peer_id` - HIVE node ID of the remote peer
    /// * `identifier` - CoreBluetooth peripheral identifier (UUID)
    pub fn new(peer_id: NodeId, identifier: String) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let delegate = Arc::new(PeripheralDelegate::new(event_tx));

        // TODO: Set this delegate on the CBPeripheral
        // peripheral.setDelegate_(delegate_obj);

        let state = ConnectionState {
            alive: true,
            mtu: 23,           // Default BLE MTU, will be updated after connection
            phy: BlePhy::Le1M, // CoreBluetooth doesn't expose PHY selection
            rssi: None,
            services_discovered: false,
        };

        Self {
            peer_id,
            identifier,
            state: Arc::new(RwLock::new(state)),
            connected_at: Instant::now(),
            event_rx: Arc::new(RwLock::new(event_rx)),
            delegate,
        }
    }

    /// Get the peripheral identifier
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Update connection state from delegate callback
    pub async fn update_connection_state(&self, connected: bool) {
        let mut state = self.state.write().await;
        state.alive = connected;
    }

    /// Update MTU (called after MTU exchange completes)
    pub async fn update_mtu(&self, mtu: u16) {
        let mut state = self.state.write().await;
        state.mtu = mtu;
        log::debug!("MTU updated to {} for peer {}", mtu, self.peer_id);
    }

    /// Update RSSI
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

    /// Disconnect from the peripheral
    ///
    /// Note: On CoreBluetooth, disconnection is handled by the CentralManager,
    /// not the peripheral itself.
    pub async fn disconnect(&self) -> Result<()> {
        // TODO: Signal CentralManager to disconnect
        // This should call centralManager.cancelPeripheralConnection_(peripheral)

        self.mark_dead().await;
        log::warn!(
            "CoreBluetoothConnection::disconnect({}) - Must be called via CentralManager",
            self.identifier
        );
        Ok(())
    }

    /// Discover services on the peripheral
    pub async fn discover_services(&self, service_uuids: Option<Vec<String>>) -> Result<()> {
        // TODO: Call CBPeripheral.discoverServices:
        //
        // Example objc2 code:
        // ```
        // let uuids = service_uuids.map(|uuids| {
        //     NSArray::from_vec(uuids.into_iter().map(|u| {
        //         CBUUID::UUIDWithString_(&NSString::from_str(&u))
        //     }).collect())
        // });
        // peripheral.discoverServices_(uuids.as_ref());
        // ```

        log::warn!(
            "CoreBluetoothConnection::discover_services({}) - Not yet implemented",
            self.identifier
        );

        Err(BleError::NotSupported(
            "CoreBluetooth service discovery not yet implemented".to_string(),
        ))
    }

    /// Discover characteristics for a service
    pub async fn discover_characteristics(
        &self,
        service_uuid: &str,
        characteristic_uuids: Option<Vec<String>>,
    ) -> Result<()> {
        // TODO: Call CBPeripheral.discoverCharacteristics:forService:
        //
        // 1. Look up CBService from peripheral.services
        // 2. Call peripheral.discoverCharacteristics:forService:

        log::warn!(
            "CoreBluetoothConnection::discover_characteristics({}, {}) - Not yet implemented",
            self.identifier,
            service_uuid
        );

        Err(BleError::NotSupported(
            "CoreBluetooth characteristic discovery not yet implemented".to_string(),
        ))
    }

    /// Read a characteristic value
    pub async fn read_characteristic(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<Vec<u8>> {
        // TODO: Call CBPeripheral.readValueForCharacteristic:
        //
        // 1. Look up CBService and CBCharacteristic
        // 2. Call peripheral.readValueForCharacteristic:
        // 3. Wait for delegate callback with value

        log::warn!(
            "CoreBluetoothConnection::read_characteristic({}, {}) - Not yet implemented",
            service_uuid,
            characteristic_uuid
        );

        Err(BleError::NotSupported(
            "CoreBluetooth characteristic read not yet implemented".to_string(),
        ))
    }

    /// Write a characteristic value
    pub async fn write_characteristic(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
        value: &[u8],
        with_response: bool,
    ) -> Result<()> {
        // TODO: Call CBPeripheral.writeValue:forCharacteristic:type:
        //
        // Write type:
        // - CBCharacteristicWriteWithResponse (0) if with_response
        // - CBCharacteristicWriteWithoutResponse (1) if !with_response
        //
        // Example objc2 code:
        // ```
        // let data = NSData::from_vec(value.to_vec());
        // let write_type = if with_response {
        //     CBCharacteristicWriteType::WithResponse
        // } else {
        //     CBCharacteristicWriteType::WithoutResponse
        // };
        // peripheral.writeValue_forCharacteristic_type_(&data, &characteristic, write_type);
        // ```

        log::warn!(
            "CoreBluetoothConnection::write_characteristic({}, {}, {} bytes, response={}) - Not yet implemented",
            service_uuid,
            characteristic_uuid,
            value.len(),
            with_response
        );

        Err(BleError::NotSupported(
            "CoreBluetooth characteristic write not yet implemented".to_string(),
        ))
    }

    /// Enable notifications for a characteristic
    pub async fn enable_notifications(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<()> {
        // TODO: Call CBPeripheral.setNotifyValue:forCharacteristic:
        //
        // Example objc2 code:
        // ```
        // peripheral.setNotifyValue_forCharacteristic_(true, &characteristic);
        // ```

        log::warn!(
            "CoreBluetoothConnection::enable_notifications({}, {}) - Not yet implemented",
            service_uuid,
            characteristic_uuid
        );

        Err(BleError::NotSupported(
            "CoreBluetooth notifications not yet implemented".to_string(),
        ))
    }

    /// Disable notifications for a characteristic
    pub async fn disable_notifications(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<()> {
        // TODO: Call CBPeripheral.setNotifyValue:forCharacteristic: with false

        log::warn!(
            "CoreBluetoothConnection::disable_notifications({}, {}) - Not yet implemented",
            service_uuid,
            characteristic_uuid
        );

        Err(BleError::NotSupported(
            "CoreBluetooth notification disable not yet implemented".to_string(),
        ))
    }

    /// Read RSSI
    pub async fn read_rssi(&self) -> Result<()> {
        // TODO: Call CBPeripheral.readRSSI()

        log::warn!(
            "CoreBluetoothConnection::read_rssi({}) - Not yet implemented",
            self.identifier
        );

        Err(BleError::NotSupported(
            "CoreBluetooth RSSI read not yet implemented".to_string(),
        ))
    }

    /// Process pending delegate events
    pub async fn process_events(&self) -> Result<()> {
        let mut event_rx = self.event_rx.write().await;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                PeripheralEvent::ServicesDiscovered { error, .. } => {
                    if error.is_none() {
                        self.mark_services_discovered().await;
                    }
                }
                PeripheralEvent::MtuUpdated { mtu, .. } => {
                    self.update_mtu(mtu).await;
                }
                PeripheralEvent::RssiRead { rssi, error, .. } => {
                    if error.is_none() {
                        self.update_rssi(rssi).await;
                    }
                }
                _ => {
                    // Other events handled by higher-level code
                }
            }
        }

        Ok(())
    }
}

impl BleConnection for CoreBluetoothConnection {
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
    use super::*;

    #[test]
    fn test_connection_creation() {
        let node_id = NodeId::new(0xDEADBEEF);
        let identifier = "12345678-1234-1234-1234-123456789ABC".to_string();
        let conn = CoreBluetoothConnection::new(node_id.clone(), identifier.clone());

        assert_eq!(conn.peer_id(), &node_id);
        assert_eq!(conn.identifier(), identifier);
        assert!(conn.is_alive());
        assert_eq!(conn.mtu(), 23);
        assert_eq!(conn.phy(), BlePhy::Le1M);
    }

    #[tokio::test]
    async fn test_connection_state_updates() {
        let node_id = NodeId::new(0xDEADBEEF);
        let conn = CoreBluetoothConnection::new(
            node_id,
            "12345678-1234-1234-1234-123456789ABC".to_string(),
        );

        // Update MTU
        conn.update_mtu(247).await;
        assert_eq!(conn.mtu(), 247);

        // Update RSSI
        conn.update_rssi(-65).await;
        assert_eq!(conn.rssi(), Some(-65));

        // Mark dead
        conn.mark_dead().await;
        assert!(!conn.is_alive());
    }
}
