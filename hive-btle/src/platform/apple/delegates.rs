//! Objective-C delegate implementations for CoreBluetooth
//!
//! CoreBluetooth uses the delegate pattern for callbacks. This module defines
//! Rust structs that implement the required Objective-C protocols and forward
//! events to Rust async channels.
//!
//! ## Delegate Protocols
//!
//! - `CBCentralManagerDelegate`: Receives central manager state and discovery events
//! - `CBPeripheralDelegate`: Receives GATT client events (reads, writes, notifications)
//! - `CBPeripheralManagerDelegate`: Receives GATT server events

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::platform::{ConnectionEvent, DiscoveredDevice};
use crate::NodeId;

/// Events from CBCentralManagerDelegate
#[derive(Debug, Clone)]
pub enum CentralEvent {
    /// Central manager state changed
    StateChanged(CentralState),
    /// Discovered a peripheral during scanning
    DiscoveredPeripheral {
        /// Peripheral identifier (UUID string)
        identifier: String,
        /// Advertised name
        name: Option<String>,
        /// RSSI in dBm
        rssi: i8,
        /// Advertisement data
        advertisement_data: Vec<u8>,
        /// Is this a HIVE node?
        is_hive_node: bool,
        /// Parsed node ID if HIVE node
        node_id: Option<NodeId>,
    },
    /// Connected to a peripheral
    Connected {
        /// Peripheral identifier
        identifier: String,
    },
    /// Disconnected from a peripheral
    Disconnected {
        /// Peripheral identifier
        identifier: String,
        /// Error if disconnection was unexpected
        error: Option<String>,
    },
    /// Failed to connect to a peripheral
    ConnectionFailed {
        /// Peripheral identifier
        identifier: String,
        /// Error description
        error: String,
    },
}

/// CBCentralManager state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CentralState {
    /// State unknown, update imminent
    Unknown,
    /// Bluetooth is resetting
    Resetting,
    /// Bluetooth is not supported on this device
    Unsupported,
    /// App is not authorized to use Bluetooth
    Unauthorized,
    /// Bluetooth is powered off
    PoweredOff,
    /// Bluetooth is powered on and ready
    PoweredOn,
}

impl CentralState {
    /// Convert from CBManagerState integer value
    pub fn from_raw(value: i64) -> Self {
        match value {
            0 => CentralState::Unknown,
            1 => CentralState::Resetting,
            2 => CentralState::Unsupported,
            3 => CentralState::Unauthorized,
            4 => CentralState::PoweredOff,
            5 => CentralState::PoweredOn,
            _ => CentralState::Unknown,
        }
    }

    /// Check if Bluetooth is ready to use
    pub fn is_ready(&self) -> bool {
        matches!(self, CentralState::PoweredOn)
    }
}

/// Events from CBPeripheralDelegate (GATT client events)
#[derive(Debug, Clone)]
pub enum PeripheralEvent {
    /// Services discovered on peripheral
    ServicesDiscovered {
        /// Peripheral identifier
        identifier: String,
        /// Error if discovery failed
        error: Option<String>,
    },
    /// Characteristics discovered for a service
    CharacteristicsDiscovered {
        /// Peripheral identifier
        identifier: String,
        /// Service UUID
        service_uuid: String,
        /// Error if discovery failed
        error: Option<String>,
    },
    /// Characteristic value read
    CharacteristicRead {
        /// Peripheral identifier
        identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// Read value
        value: Vec<u8>,
        /// Error if read failed
        error: Option<String>,
    },
    /// Characteristic value written
    CharacteristicWritten {
        /// Peripheral identifier
        identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// Error if write failed
        error: Option<String>,
    },
    /// Characteristic value changed (notification/indication)
    CharacteristicChanged {
        /// Peripheral identifier
        identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// New value
        value: Vec<u8>,
    },
    /// Notification state changed
    NotificationStateChanged {
        /// Peripheral identifier
        identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// Whether notifications are now enabled
        enabled: bool,
        /// Error if state change failed
        error: Option<String>,
    },
    /// MTU updated
    MtuUpdated {
        /// Peripheral identifier
        identifier: String,
        /// New MTU value
        mtu: u16,
    },
    /// RSSI read
    RssiRead {
        /// Peripheral identifier
        identifier: String,
        /// RSSI value in dBm
        rssi: i8,
        /// Error if read failed
        error: Option<String>,
    },
}

/// Events from CBPeripheralManagerDelegate (GATT server events)
#[derive(Debug, Clone)]
pub enum PeripheralManagerEvent {
    /// Peripheral manager state changed
    StateChanged(CentralState), // Uses same state enum
    /// Service was added
    ServiceAdded {
        /// Service UUID
        service_uuid: String,
        /// Error if add failed
        error: Option<String>,
    },
    /// Started advertising
    AdvertisingStarted {
        /// Error if advertising failed to start
        error: Option<String>,
    },
    /// Central subscribed to characteristic
    CentralSubscribed {
        /// Central identifier
        central_identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
    },
    /// Central unsubscribed from characteristic
    CentralUnsubscribed {
        /// Central identifier
        central_identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
    },
    /// Received read request from central
    ReadRequest {
        /// Request identifier for response
        request_id: u64,
        /// Central identifier
        central_identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// Offset for read
        offset: usize,
    },
    /// Received write request from central
    WriteRequest {
        /// Request identifier for response
        request_id: u64,
        /// Central identifier
        central_identifier: String,
        /// Characteristic UUID
        characteristic_uuid: String,
        /// Written value
        value: Vec<u8>,
        /// Offset for write
        offset: usize,
        /// Whether response is required
        response_needed: bool,
    },
    /// Ready to update subscribers
    ReadyToUpdateSubscribers,
}

/// CBCentralManagerDelegate implementation
///
/// This struct is registered as the delegate for CBCentralManager and forwards
/// events to a Rust channel.
pub struct CentralDelegate {
    /// Channel to send events
    event_tx: mpsc::Sender<CentralEvent>,
}

impl CentralDelegate {
    /// Create a new central delegate
    pub fn new(event_tx: mpsc::Sender<CentralEvent>) -> Self {
        Self { event_tx }
    }

    /// Called when central manager state changes
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)centralManagerDidUpdateState:(CBCentralManager *)central
    /// ```
    pub fn central_manager_did_update_state(&self, state: i64) {
        let state = CentralState::from_raw(state);
        log::debug!("Central manager state changed: {:?}", state);

        let _ = self.event_tx.try_send(CentralEvent::StateChanged(state));
    }

    /// Called when a peripheral is discovered during scanning
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)centralManager:(CBCentralManager *)central
    ///     didDiscoverPeripheral:(CBPeripheral *)peripheral
    ///     advertisementData:(NSDictionary<NSString *, id> *)advertisementData
    ///     RSSI:(NSNumber *)RSSI
    /// ```
    pub fn central_manager_did_discover_peripheral(
        &self,
        identifier: String,
        name: Option<String>,
        rssi: i8,
        advertisement_data: Vec<u8>,
    ) {
        // Check if this is a HIVE node by looking at the name
        let is_hive_node = name
            .as_ref()
            .map(|n| n.starts_with("HIVE-"))
            .unwrap_or(false);

        let node_id = name.as_ref().and_then(|n| {
            if n.starts_with("HIVE-") {
                NodeId::parse(&n[5..])
            } else {
                None
            }
        });

        log::debug!(
            "Discovered peripheral: {} ({:?}) RSSI: {} HIVE: {}",
            identifier,
            name,
            rssi,
            is_hive_node
        );

        let _ = self.event_tx.try_send(CentralEvent::DiscoveredPeripheral {
            identifier,
            name,
            rssi,
            advertisement_data,
            is_hive_node,
            node_id,
        });
    }

    /// Called when connected to a peripheral
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)centralManager:(CBCentralManager *)central
    ///     didConnectPeripheral:(CBPeripheral *)peripheral
    /// ```
    pub fn central_manager_did_connect_peripheral(&self, identifier: String) {
        log::info!("Connected to peripheral: {}", identifier);
        let _ = self
            .event_tx
            .try_send(CentralEvent::Connected { identifier });
    }

    /// Called when disconnected from a peripheral
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)centralManager:(CBCentralManager *)central
    ///     didDisconnectPeripheral:(CBPeripheral *)peripheral
    ///     error:(NSError *)error
    /// ```
    pub fn central_manager_did_disconnect_peripheral(
        &self,
        identifier: String,
        error: Option<String>,
    ) {
        log::info!(
            "Disconnected from peripheral: {} (error: {:?})",
            identifier,
            error
        );
        let _ = self
            .event_tx
            .try_send(CentralEvent::Disconnected { identifier, error });
    }

    /// Called when connection to a peripheral fails
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)centralManager:(CBCentralManager *)central
    ///     didFailToConnectPeripheral:(CBPeripheral *)peripheral
    ///     error:(NSError *)error
    /// ```
    pub fn central_manager_did_fail_to_connect_peripheral(
        &self,
        identifier: String,
        error: String,
    ) {
        log::warn!(
            "Failed to connect to peripheral: {} ({})",
            identifier,
            error
        );
        let _ = self
            .event_tx
            .try_send(CentralEvent::ConnectionFailed { identifier, error });
    }
}

/// CBPeripheralDelegate implementation
///
/// This struct is registered as the delegate for connected CBPeripheral objects
/// and forwards GATT client events to a Rust channel.
pub struct PeripheralDelegate {
    /// Channel to send events
    event_tx: mpsc::Sender<PeripheralEvent>,
}

impl PeripheralDelegate {
    /// Create a new peripheral delegate
    pub fn new(event_tx: mpsc::Sender<PeripheralEvent>) -> Self {
        Self { event_tx }
    }

    /// Called when services are discovered
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didDiscoverServices:(NSError *)error
    /// ```
    pub fn peripheral_did_discover_services(&self, identifier: String, error: Option<String>) {
        log::debug!("Services discovered for {}: error={:?}", identifier, error);
        let _ = self
            .event_tx
            .try_send(PeripheralEvent::ServicesDiscovered { identifier, error });
    }

    /// Called when characteristics are discovered for a service
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didDiscoverCharacteristicsForService:(CBService *)service
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_did_discover_characteristics(
        &self,
        identifier: String,
        service_uuid: String,
        error: Option<String>,
    ) {
        log::debug!(
            "Characteristics discovered for {} service {}: error={:?}",
            identifier,
            service_uuid,
            error
        );
        let _ = self
            .event_tx
            .try_send(PeripheralEvent::CharacteristicsDiscovered {
                identifier,
                service_uuid,
                error,
            });
    }

    /// Called when a characteristic value is read
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didUpdateValueForCharacteristic:(CBCharacteristic *)characteristic
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_did_update_value_for_characteristic(
        &self,
        identifier: String,
        characteristic_uuid: String,
        value: Vec<u8>,
        error: Option<String>,
    ) {
        log::trace!(
            "Characteristic {} value updated for {}: {} bytes",
            characteristic_uuid,
            identifier,
            value.len()
        );
        let _ = self.event_tx.try_send(PeripheralEvent::CharacteristicRead {
            identifier,
            characteristic_uuid,
            value,
            error,
        });
    }

    /// Called when a characteristic value is written
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didWriteValueForCharacteristic:(CBCharacteristic *)characteristic
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_did_write_value_for_characteristic(
        &self,
        identifier: String,
        characteristic_uuid: String,
        error: Option<String>,
    ) {
        log::trace!(
            "Characteristic {} written for {}: error={:?}",
            characteristic_uuid,
            identifier,
            error
        );
        let _ = self
            .event_tx
            .try_send(PeripheralEvent::CharacteristicWritten {
                identifier,
                characteristic_uuid,
                error,
            });
    }

    /// Called when notification state changes for a characteristic
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didUpdateNotificationStateForCharacteristic:(CBCharacteristic *)characteristic
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_did_update_notification_state(
        &self,
        identifier: String,
        characteristic_uuid: String,
        enabled: bool,
        error: Option<String>,
    ) {
        log::debug!(
            "Notification state for {} char {}: enabled={} error={:?}",
            identifier,
            characteristic_uuid,
            enabled,
            error
        );
        let _ = self
            .event_tx
            .try_send(PeripheralEvent::NotificationStateChanged {
                identifier,
                characteristic_uuid,
                enabled,
                error,
            });
    }

    /// Called when RSSI is read
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheral:(CBPeripheral *)peripheral
    ///     didReadRSSI:(NSNumber *)RSSI
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_did_read_rssi(&self, identifier: String, rssi: i8, error: Option<String>) {
        let _ = self.event_tx.try_send(PeripheralEvent::RssiRead {
            identifier,
            rssi,
            error,
        });
    }
}

/// CBPeripheralManagerDelegate implementation
///
/// This struct is registered as the delegate for CBPeripheralManager and forwards
/// GATT server events to a Rust channel.
pub struct PeripheralManagerDelegate {
    /// Channel to send events
    event_tx: mpsc::Sender<PeripheralManagerEvent>,
}

impl PeripheralManagerDelegate {
    /// Create a new peripheral manager delegate
    pub fn new(event_tx: mpsc::Sender<PeripheralManagerEvent>) -> Self {
        Self { event_tx }
    }

    /// Called when peripheral manager state changes
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManagerDidUpdateState:(CBPeripheralManager *)peripheral
    /// ```
    pub fn peripheral_manager_did_update_state(&self, state: i64) {
        let state = CentralState::from_raw(state);
        log::debug!("Peripheral manager state changed: {:?}", state);
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::StateChanged(state));
    }

    /// Called when a service is added
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManager:(CBPeripheralManager *)peripheral
    ///     didAddService:(CBService *)service
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_manager_did_add_service(&self, service_uuid: String, error: Option<String>) {
        log::debug!("Service {} added: error={:?}", service_uuid, error);
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::ServiceAdded {
                service_uuid,
                error,
            });
    }

    /// Called when advertising starts
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManagerDidStartAdvertising:(CBPeripheralManager *)peripheral
    ///     error:(NSError *)error
    /// ```
    pub fn peripheral_manager_did_start_advertising(&self, error: Option<String>) {
        if let Some(ref e) = error {
            log::warn!("Advertising failed to start: {}", e);
        } else {
            log::info!("Advertising started successfully");
        }
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::AdvertisingStarted { error });
    }

    /// Called when a central subscribes to a characteristic
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManager:(CBPeripheralManager *)peripheral
    ///     central:(CBCentral *)central
    ///     didSubscribeToCharacteristic:(CBCharacteristic *)characteristic
    /// ```
    pub fn peripheral_manager_central_did_subscribe(
        &self,
        central_identifier: String,
        characteristic_uuid: String,
    ) {
        log::debug!(
            "Central {} subscribed to {}",
            central_identifier,
            characteristic_uuid
        );
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::CentralSubscribed {
                central_identifier,
                characteristic_uuid,
            });
    }

    /// Called when a central unsubscribes from a characteristic
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManager:(CBPeripheralManager *)peripheral
    ///     central:(CBCentral *)central
    ///     didUnsubscribeFromCharacteristic:(CBCharacteristic *)characteristic
    /// ```
    pub fn peripheral_manager_central_did_unsubscribe(
        &self,
        central_identifier: String,
        characteristic_uuid: String,
    ) {
        log::debug!(
            "Central {} unsubscribed from {}",
            central_identifier,
            characteristic_uuid
        );
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::CentralUnsubscribed {
                central_identifier,
                characteristic_uuid,
            });
    }

    /// Called when a read request is received
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManager:(CBPeripheralManager *)peripheral
    ///     didReceiveReadRequest:(CBATTRequest *)request
    /// ```
    pub fn peripheral_manager_did_receive_read_request(
        &self,
        request_id: u64,
        central_identifier: String,
        characteristic_uuid: String,
        offset: usize,
    ) {
        log::trace!(
            "Read request from {} for {} offset {}",
            central_identifier,
            characteristic_uuid,
            offset
        );
        let _ = self.event_tx.try_send(PeripheralManagerEvent::ReadRequest {
            request_id,
            central_identifier,
            characteristic_uuid,
            offset,
        });
    }

    /// Called when write requests are received
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManager:(CBPeripheralManager *)peripheral
    ///     didReceiveWriteRequests:(NSArray<CBATTRequest *> *)requests
    /// ```
    pub fn peripheral_manager_did_receive_write_request(
        &self,
        request_id: u64,
        central_identifier: String,
        characteristic_uuid: String,
        value: Vec<u8>,
        offset: usize,
        response_needed: bool,
    ) {
        log::trace!(
            "Write request from {} for {} ({} bytes)",
            central_identifier,
            characteristic_uuid,
            value.len()
        );
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::WriteRequest {
                request_id,
                central_identifier,
                characteristic_uuid,
                value,
                offset,
                response_needed,
            });
    }

    /// Called when ready to send updates to subscribers
    ///
    /// # Objective-C
    /// ```objc
    /// - (void)peripheralManagerIsReadyToUpdateSubscribers:(CBPeripheralManager *)peripheral
    /// ```
    pub fn peripheral_manager_is_ready_to_update_subscribers(&self) {
        log::trace!("Ready to update subscribers");
        let _ = self
            .event_tx
            .try_send(PeripheralManagerEvent::ReadyToUpdateSubscribers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_central_state_from_raw() {
        assert_eq!(CentralState::from_raw(0), CentralState::Unknown);
        assert_eq!(CentralState::from_raw(4), CentralState::PoweredOff);
        assert_eq!(CentralState::from_raw(5), CentralState::PoweredOn);
        assert_eq!(CentralState::from_raw(99), CentralState::Unknown);
    }

    #[test]
    fn test_central_state_is_ready() {
        assert!(!CentralState::Unknown.is_ready());
        assert!(!CentralState::PoweredOff.is_ready());
        assert!(CentralState::PoweredOn.is_ready());
    }
}
