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

use std::collections::HashMap;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerDelegate, CBCharacteristic, CBPeripheral,
    CBPeripheralDelegate, CBPeripheralManager, CBPeripheralManagerDelegate, CBService,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString};
use tokio::sync::mpsc;

use crate::NodeId;

// ============================================================================
// Event Types
// ============================================================================

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

/// CBCentralManager/CBPeripheralManager state
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
    pub fn from_raw(value: isize) -> Self {
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
        identifier: String,
        error: Option<String>,
    },
    /// Characteristics discovered for a service
    CharacteristicsDiscovered {
        identifier: String,
        service_uuid: String,
        error: Option<String>,
    },
    /// Characteristic value read/updated
    CharacteristicValueUpdated {
        identifier: String,
        characteristic_uuid: String,
        value: Vec<u8>,
        error: Option<String>,
    },
    /// Characteristic value written
    CharacteristicWritten {
        identifier: String,
        characteristic_uuid: String,
        error: Option<String>,
    },
    /// Notification state changed
    NotificationStateChanged {
        identifier: String,
        characteristic_uuid: String,
        enabled: bool,
        error: Option<String>,
    },
    /// RSSI read
    RssiRead {
        identifier: String,
        rssi: i8,
        error: Option<String>,
    },
    /// MTU updated
    MtuUpdated { identifier: String, mtu: u16 },
}

/// Events from CBPeripheralManagerDelegate (GATT server events)
#[derive(Debug, Clone)]
pub enum PeripheralManagerEvent {
    /// Peripheral manager state changed
    StateChanged(CentralState),
    /// Service was added
    ServiceAdded {
        service_uuid: String,
        error: Option<String>,
    },
    /// Started advertising
    AdvertisingStarted { error: Option<String> },
    /// Central subscribed to characteristic
    CentralSubscribed {
        central_identifier: String,
        characteristic_uuid: String,
    },
    /// Central unsubscribed from characteristic
    CentralUnsubscribed {
        central_identifier: String,
        characteristic_uuid: String,
    },
    /// Received read request from central
    ReadRequest {
        request_id: u64,
        central_identifier: String,
        characteristic_uuid: String,
        offset: usize,
    },
    /// Received write request from central
    WriteRequest {
        request_id: u64,
        central_identifier: String,
        characteristic_uuid: String,
        value: Vec<u8>,
        offset: usize,
        response_needed: bool,
    },
    /// Ready to update subscribers
    ReadyToUpdateSubscribers,
}

// ============================================================================
// Objective-C Delegate Classes
// ============================================================================

/// Ivars for RustCentralManagerDelegate
pub struct CentralDelegateIvars {
    event_tx: Mutex<Option<mpsc::Sender<CentralEvent>>>,
    /// Discovered peripherals stored by identifier for later connection
    peripherals: Mutex<HashMap<String, Retained<CBPeripheral>>>,
}

impl Default for CentralDelegateIvars {
    fn default() -> Self {
        Self {
            event_tx: Mutex::new(None),
            peripherals: Mutex::new(HashMap::new()),
        }
    }
}

declare_class!(
    /// Objective-C class implementing CBCentralManagerDelegate
    pub struct RustCentralManagerDelegate;

    unsafe impl ClassType for RustCentralManagerDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "RustCentralManagerDelegate";
    }

    impl DeclaredClass for RustCentralManagerDelegate {
        type Ivars = CentralDelegateIvars;
    }

    unsafe impl NSObjectProtocol for RustCentralManagerDelegate {}

    unsafe impl CBCentralManagerDelegate for RustCentralManagerDelegate {
        #[method(centralManagerDidUpdateState:)]
        fn central_manager_did_update_state(&self, central: &CBCentralManager) {
            let state_raw = unsafe { central.state() };
            let state = CentralState::from_raw(state_raw.0);
            log::debug!("Central manager state changed: {:?}", state);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(CentralEvent::StateChanged(state));
                }
            }
        }

        #[method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:)]
        fn central_manager_did_discover_peripheral(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            advertisement_data: &NSDictionary<NSString, objc2::runtime::AnyObject>,
            rssi: &NSNumber,
        ) {
            let identifier = unsafe {
                let uuid = peripheral.identifier();
                uuid.UUIDString().to_string()
            };

            let name = unsafe { peripheral.name().map(|s| s.to_string()) };
            let rssi_val = rssi.as_i8();

            // Check if this is a HIVE node by looking at the name
            let is_hive_node = name.as_ref().map(|n| n.starts_with("HIVE-")).unwrap_or(false);
            let node_id = name.as_ref().and_then(|n| {
                if n.starts_with("HIVE-") {
                    NodeId::parse(&n[5..])
                } else {
                    None
                }
            });

            log::debug!(
                "Discovered peripheral: {} ({:?}) RSSI: {} HIVE: {}",
                identifier, name, rssi_val, is_hive_node
            );

            // Store the peripheral for later connection
            if let Ok(mut guard) = self.ivars().peripherals.lock() {
                guard.insert(identifier.clone(), peripheral.retain());
            }

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(CentralEvent::DiscoveredPeripheral {
                        identifier,
                        name,
                        rssi: rssi_val,
                        advertisement_data: Vec::new(), // TODO: Parse advertisement data
                        is_hive_node,
                        node_id,
                    });
                }
            }
        }

        #[method(centralManager:didConnectPeripheral:)]
        fn central_manager_did_connect_peripheral(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            log::info!("Connected to peripheral: {}", identifier);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(CentralEvent::Connected { identifier });
                }
            }
        }

        #[method(centralManager:didDisconnectPeripheral:error:)]
        fn central_manager_did_disconnect_peripheral(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });
            log::info!("Disconnected from peripheral: {} (error: {:?})", identifier, error_str);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(CentralEvent::Disconnected {
                        identifier,
                        error: error_str,
                    });
                }
            }
        }

        #[method(centralManager:didFailToConnectPeripheral:error:)]
        fn central_manager_did_fail_to_connect_peripheral(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let error_str = error
                .map(|e| unsafe { e.localizedDescription().to_string() })
                .unwrap_or_else(|| "Unknown error".to_string());
            log::warn!("Failed to connect to peripheral: {} ({})", identifier, error_str);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(CentralEvent::ConnectionFailed {
                        identifier,
                        error: error_str,
                    });
                }
            }
        }
    }
);

impl RustCentralManagerDelegate {
    /// Create a new delegate with the given event sender
    pub fn new(event_tx: mpsc::Sender<CentralEvent>) -> Retained<Self> {
        let this = Self::alloc();
        let this = this.set_ivars(CentralDelegateIvars {
            event_tx: Mutex::new(Some(event_tx)),
            peripherals: Mutex::new(HashMap::new()),
        });
        unsafe { msg_send_id![super(this), init] }
    }

    /// Get as a protocol object for setting as delegate
    pub fn as_protocol(&self) -> &ProtocolObject<dyn CBCentralManagerDelegate> {
        ProtocolObject::from_ref(self)
    }

    /// Get a stored peripheral by identifier
    pub fn get_peripheral(&self, identifier: &str) -> Option<Retained<CBPeripheral>> {
        self.ivars()
            .peripherals
            .lock()
            .ok()
            .and_then(|guard| guard.get(identifier).cloned())
    }

    /// Remove a peripheral from storage
    pub fn remove_peripheral(&self, identifier: &str) -> Option<Retained<CBPeripheral>> {
        self.ivars()
            .peripherals
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(identifier))
    }
}

/// Ivars for RustPeripheralDelegate
pub struct PeripheralDelegateIvars {
    event_tx: Mutex<Option<mpsc::Sender<PeripheralEvent>>>,
}

impl Default for PeripheralDelegateIvars {
    fn default() -> Self {
        Self {
            event_tx: Mutex::new(None),
        }
    }
}

declare_class!(
    /// Objective-C class implementing CBPeripheralDelegate
    pub struct RustPeripheralDelegate;

    unsafe impl ClassType for RustPeripheralDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "RustPeripheralDelegate";
    }

    impl DeclaredClass for RustPeripheralDelegate {
        type Ivars = PeripheralDelegateIvars;
    }

    unsafe impl NSObjectProtocol for RustPeripheralDelegate {}

    unsafe impl CBPeripheralDelegate for RustPeripheralDelegate {
        #[method(peripheral:didDiscoverServices:)]
        fn peripheral_did_discover_services(
            &self,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });
            log::debug!("Services discovered for {}: error={:?}", identifier, error_str);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::ServicesDiscovered {
                        identifier,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheral:didDiscoverCharacteristicsForService:error:)]
        fn peripheral_did_discover_characteristics_for_service(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let service_uuid = unsafe { service.UUID().UUIDString().to_string() };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::CharacteristicsDiscovered {
                        identifier,
                        service_uuid,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheral:didUpdateValueForCharacteristic:error:)]
        fn peripheral_did_update_value_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let characteristic_uuid = unsafe { characteristic.UUID().UUIDString().to_string() };
            let value = unsafe {
                characteristic.value()
                    .map(|d| d.bytes().to_vec())
                    .unwrap_or_default()
            };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::CharacteristicValueUpdated {
                        identifier,
                        characteristic_uuid,
                        value,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheral:didWriteValueForCharacteristic:error:)]
        fn peripheral_did_write_value_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let characteristic_uuid = unsafe { characteristic.UUID().UUIDString().to_string() };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::CharacteristicWritten {
                        identifier,
                        characteristic_uuid,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheral:didUpdateNotificationStateForCharacteristic:error:)]
        fn peripheral_did_update_notification_state_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let characteristic_uuid = unsafe { characteristic.UUID().UUIDString().to_string() };
            let enabled = unsafe { characteristic.isNotifying() };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::NotificationStateChanged {
                        identifier,
                        characteristic_uuid,
                        enabled,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheral:didReadRSSI:error:)]
        fn peripheral_did_read_rssi(
            &self,
            peripheral: &CBPeripheral,
            rssi: &NSNumber,
            error: Option<&NSError>,
        ) {
            let identifier = unsafe {
                peripheral.identifier().UUIDString().to_string()
            };
            let rssi_val = rssi.as_i8();
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralEvent::RssiRead {
                        identifier,
                        rssi: rssi_val,
                        error: error_str,
                    });
                }
            }
        }
    }
);

impl RustPeripheralDelegate {
    /// Create a new delegate with the given event sender
    pub fn new(event_tx: mpsc::Sender<PeripheralEvent>) -> Retained<Self> {
        let this = Self::alloc();
        let this = this.set_ivars(PeripheralDelegateIvars {
            event_tx: Mutex::new(Some(event_tx)),
        });
        unsafe { msg_send_id![super(this), init] }
    }

    /// Get as a protocol object for setting as delegate
    pub fn as_protocol(&self) -> &ProtocolObject<dyn CBPeripheralDelegate> {
        ProtocolObject::from_ref(self)
    }
}

/// Ivars for RustPeripheralManagerDelegate
pub struct PeripheralManagerDelegateIvars {
    event_tx: Mutex<Option<mpsc::Sender<PeripheralManagerEvent>>>,
}

impl Default for PeripheralManagerDelegateIvars {
    fn default() -> Self {
        Self {
            event_tx: Mutex::new(None),
        }
    }
}

declare_class!(
    /// Objective-C class implementing CBPeripheralManagerDelegate
    pub struct RustPeripheralManagerDelegate;

    unsafe impl ClassType for RustPeripheralManagerDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "RustPeripheralManagerDelegate";
    }

    impl DeclaredClass for RustPeripheralManagerDelegate {
        type Ivars = PeripheralManagerDelegateIvars;
    }

    unsafe impl NSObjectProtocol for RustPeripheralManagerDelegate {}

    unsafe impl CBPeripheralManagerDelegate for RustPeripheralManagerDelegate {
        #[method(peripheralManagerDidUpdateState:)]
        fn peripheral_manager_did_update_state(&self, peripheral: &CBPeripheralManager) {
            let state_raw = unsafe { peripheral.state() };
            let state = CentralState::from_raw(state_raw.0);
            log::debug!("Peripheral manager state changed: {:?}", state);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralManagerEvent::StateChanged(state));
                }
            }
        }

        #[method(peripheralManager:didAddService:error:)]
        fn peripheral_manager_did_add_service(
            &self,
            _peripheral: &CBPeripheralManager,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let service_uuid = unsafe { service.UUID().UUIDString().to_string() };
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });
            log::debug!("Service {} added: error={:?}", service_uuid, error_str);

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralManagerEvent::ServiceAdded {
                        service_uuid,
                        error: error_str,
                    });
                }
            }
        }

        #[method(peripheralManagerDidStartAdvertising:error:)]
        fn peripheral_manager_did_start_advertising(
            &self,
            _peripheral: &CBPeripheralManager,
            error: Option<&NSError>,
        ) {
            let error_str = error.map(|e| unsafe { e.localizedDescription().to_string() });
            if let Some(ref e) = error_str {
                log::warn!("Advertising failed to start: {}", e);
            } else {
                log::info!("Advertising started successfully");
            }

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralManagerEvent::AdvertisingStarted { error: error_str });
                }
            }
        }

        #[method(peripheralManagerIsReadyToUpdateSubscribers:)]
        fn peripheral_manager_is_ready_to_update_subscribers(
            &self,
            _peripheral: &CBPeripheralManager,
        ) {
            log::trace!("Ready to update subscribers");

            if let Ok(guard) = self.ivars().event_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.try_send(PeripheralManagerEvent::ReadyToUpdateSubscribers);
                }
            }
        }
    }
);

impl RustPeripheralManagerDelegate {
    /// Create a new delegate with the given event sender
    pub fn new(event_tx: mpsc::Sender<PeripheralManagerEvent>) -> Retained<Self> {
        let this = Self::alloc();
        let this = this.set_ivars(PeripheralManagerDelegateIvars {
            event_tx: Mutex::new(Some(event_tx)),
        });
        unsafe { msg_send_id![super(this), init] }
    }

    /// Get as a protocol object for setting as delegate
    pub fn as_protocol(&self) -> &ProtocolObject<dyn CBPeripheralManagerDelegate> {
        ProtocolObject::from_ref(self)
    }
}

// ============================================================================
// Legacy compatibility - keep existing structs for channel forwarding
// ============================================================================

/// Legacy CentralDelegate for channel forwarding (used internally)
pub struct CentralDelegate {
    pub event_tx: mpsc::Sender<CentralEvent>,
}

impl CentralDelegate {
    pub fn new(event_tx: mpsc::Sender<CentralEvent>) -> Self {
        Self { event_tx }
    }
}

/// Legacy PeripheralDelegate for channel forwarding
pub struct PeripheralDelegate {
    pub event_tx: mpsc::Sender<PeripheralEvent>,
}

impl PeripheralDelegate {
    pub fn new(event_tx: mpsc::Sender<PeripheralEvent>) -> Self {
        Self { event_tx }
    }
}

/// Legacy PeripheralManagerDelegate for channel forwarding
pub struct PeripheralManagerDelegate {
    pub event_tx: mpsc::Sender<PeripheralManagerEvent>,
}

impl PeripheralManagerDelegate {
    pub fn new(event_tx: mpsc::Sender<PeripheralManagerEvent>) -> Self {
        Self { event_tx }
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
