//! CBPeripheralManager wrapper
//!
//! This module provides a Rust wrapper around CoreBluetooth's CBPeripheralManager,
//! which is used for advertising and hosting GATT services (GATT server role).

use std::collections::HashMap;
use std::sync::{Arc, RwLock as StdRwLock};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_core_bluetooth::{
    CBAdvertisementDataLocalNameKey, CBAdvertisementDataServiceUUIDsKey, CBAttributePermissions,
    CBCharacteristic, CBCharacteristicProperties, CBMutableCharacteristic, CBMutableService,
    CBPeripheralManager, CBUUID,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use tokio::sync::{mpsc, RwLock};

use crate::config::DiscoveryConfig;
use crate::error::{BleError, Result};
use crate::NodeId;
use crate::HIVE_SERVICE_UUID;

use super::delegates::{CentralState, PeripheralManagerEvent, RustPeripheralManagerDelegate};

/// Wrapper around CBPeripheralManager for BLE advertising and GATT server
///
/// CBPeripheralManager is the peripheral role in CoreBluetooth, used to:
/// - Advertise the device as a BLE peripheral
/// - Host GATT services with characteristics
/// - Respond to read/write requests from centrals
/// - Send notifications/indications to subscribed centrals
///
/// # Safety
/// This type is marked `Send + Sync` because CoreBluetooth callbacks are
/// dispatched on the main queue and the manager is only accessed from async
/// tasks that ensure proper synchronization.
pub struct PeripheralManager {
    /// The actual CBPeripheralManager instance
    manager: Retained<CBPeripheralManager>,
    /// The delegate that receives callbacks
    delegate: Retained<RustPeripheralManagerDelegate>,
    /// Current state of the peripheral manager
    state: Arc<RwLock<CentralState>>,
    /// Channel receiver for delegate events
    event_rx: Arc<RwLock<mpsc::Receiver<PeripheralManagerEvent>>>,
    /// Whether advertising is active
    advertising: Arc<RwLock<bool>>,
    /// Registered services
    services: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    /// Subscribed centrals by characteristic UUID
    subscribers: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Stored characteristics for notifications (uses std RwLock since Retained is not Send)
    characteristics: Arc<StdRwLock<HashMap<String, Retained<CBMutableCharacteristic>>>>,
}

// SAFETY: PeripheralManager uses interior mutability via Arc<RwLock<_>> for all
// mutable state. The CBPeripheralManager is only accessed from the async context
// and its callbacks are dispatched on the main queue.
unsafe impl Send for PeripheralManager {}
unsafe impl Sync for PeripheralManager {}

/// Information about a registered GATT service
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    /// Service UUID
    pub uuid: String,
    /// Whether service is primary
    pub is_primary: bool,
    /// Characteristics in the service
    pub characteristics: Vec<CharacteristicInfo>,
}

/// Information about a GATT characteristic
#[derive(Debug, Clone)]
pub struct CharacteristicInfo {
    /// Characteristic UUID
    pub uuid: String,
    /// Properties (read, write, notify, etc.)
    pub properties: CharacteristicPropertiesFlags,
    /// Current value
    pub value: Vec<u8>,
}

/// Characteristic properties flags
#[derive(Debug, Clone, Copy, Default)]
pub struct CharacteristicPropertiesFlags {
    /// Can be read
    pub read: bool,
    /// Can be written with response
    pub write: bool,
    /// Can be written without response
    pub write_without_response: bool,
    /// Supports notifications
    pub notify: bool,
    /// Supports indications
    pub indicate: bool,
}

impl CharacteristicPropertiesFlags {
    /// Properties for a readable characteristic
    pub fn readable() -> Self {
        Self {
            read: true,
            ..Default::default()
        }
    }

    /// Properties for a writable characteristic
    pub fn writable() -> Self {
        Self {
            write: true,
            ..Default::default()
        }
    }

    /// Properties for a notify characteristic
    pub fn notify() -> Self {
        Self {
            notify: true,
            ..Default::default()
        }
    }

    /// Properties for a read/write/notify characteristic (typical for HIVE sync)
    pub fn read_write_notify() -> Self {
        Self {
            read: true,
            write: true,
            notify: true,
            ..Default::default()
        }
    }

    /// Convert to CBCharacteristicProperties
    pub fn to_cb_properties(&self) -> CBCharacteristicProperties {
        let mut props = CBCharacteristicProperties::empty();
        if self.read {
            props |= CBCharacteristicProperties::CBCharacteristicPropertyRead;
        }
        if self.write {
            props |= CBCharacteristicProperties::CBCharacteristicPropertyWrite;
        }
        if self.write_without_response {
            props |= CBCharacteristicProperties::CBCharacteristicPropertyWriteWithoutResponse;
        }
        if self.notify {
            props |= CBCharacteristicProperties::CBCharacteristicPropertyNotify;
        }
        if self.indicate {
            props |= CBCharacteristicProperties::CBCharacteristicPropertyIndicate;
        }
        props
    }
}

impl PeripheralManager {
    /// Create a new PeripheralManager
    ///
    /// This initializes the CBPeripheralManager with default options.
    /// The manager won't be ready until `state` becomes `PoweredOn`.
    pub fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);

        // Create delegate
        let delegate = RustPeripheralManagerDelegate::new(event_tx);

        // Create CBPeripheralManager and set delegate
        let manager = unsafe { CBPeripheralManager::new() };
        unsafe {
            manager.setDelegate(Some(delegate.as_protocol()));
        }

        log::info!("CBPeripheralManager initialized");

        Ok(Self {
            manager,
            delegate,
            state: Arc::new(RwLock::new(CentralState::Unknown)),
            event_rx: Arc::new(RwLock::new(event_rx)),
            advertising: Arc::new(RwLock::new(false)),
            services: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            characteristics: Arc::new(StdRwLock::new(HashMap::new())),
        })
    }

    /// Get the current peripheral manager state
    pub async fn state(&self) -> CentralState {
        *self.state.read().await
    }

    /// Wait for the peripheral manager to be ready (powered on)
    pub async fn wait_ready(&self) -> Result<()> {
        loop {
            self.process_events().await?;

            let state = self.state().await;
            match state {
                CentralState::PoweredOn => return Ok(()),
                CentralState::Unsupported => {
                    return Err(BleError::NotSupported(
                        "Bluetooth not supported".to_string(),
                    ))
                }
                CentralState::Unauthorized => {
                    return Err(BleError::PlatformError(
                        "Bluetooth not authorized".to_string(),
                    ))
                }
                CentralState::PoweredOff => {
                    return Err(BleError::PlatformError(
                        "Bluetooth is powered off".to_string(),
                    ))
                }
                CentralState::Unknown | CentralState::Resetting => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Register the HIVE GATT service
    ///
    /// Creates the HIVE BLE service with all required characteristics.
    pub async fn register_hive_service(&self, node_id: NodeId) -> Result<()> {
        // All ObjC work happens in this block, dropped before any await
        // The Retained<> types are not Send, so they can't cross await points
        {
            let (cb_characteristics, char_entries) = self.create_hive_characteristics(node_id)?;

            // Store characteristics (sync lock since it contains non-Send types)
            {
                let mut char_map = self.characteristics.write().unwrap();
                for (uuid, char) in char_entries {
                    char_map.insert(uuid, char);
                }
            }

            // Create and configure service
            unsafe {
                // Create service UUID
                let service_uuid = {
                    let uuid_str = NSString::from_str(&HIVE_SERVICE_UUID.to_string());
                    CBUUID::UUIDWithString(&uuid_str)
                };

                // Create mutable service
                let service = CBMutableService::initWithType_primary(
                    CBMutableService::alloc(),
                    &service_uuid,
                    true,
                );

                // Set characteristics on service
                // CBMutableCharacteristic is a subclass of CBCharacteristic, so we need to cast
                let char_refs: Vec<&CBCharacteristic> = cb_characteristics
                    .iter()
                    .map(|c| {
                        // Cast CBMutableCharacteristic to CBCharacteristic (safe - subclass)
                        let ptr: *const CBMutableCharacteristic = &**c;
                        &*(ptr as *const CBCharacteristic)
                    })
                    .collect();
                let char_array = NSArray::from_slice(&char_refs);
                service.setCharacteristics(Some(&char_array));

                // Add service to manager
                self.manager.addService(&service);
            }

            log::info!(
                "Registered HIVE service with node ID {:08X}",
                node_id.as_u32()
            );

            // Set initial characteristic values in the delegate for read requests
            // Node Info (0001) - contains the node ID
            self.delegate
                .set_characteristic_value("0001", node_id.as_u32().to_le_bytes().to_vec());
        }
        // All ObjC objects are now dropped, safe to await

        // Store service info (async)
        let service_info = ServiceInfo {
            uuid: HIVE_SERVICE_UUID.to_string(),
            is_primary: true,
            characteristics: vec![
                CharacteristicInfo {
                    uuid: "0001".to_string(),
                    properties: CharacteristicPropertiesFlags::readable(),
                    value: node_id.as_u32().to_le_bytes().to_vec(),
                },
                CharacteristicInfo {
                    uuid: "0002".to_string(),
                    properties: CharacteristicPropertiesFlags::read_write_notify(),
                    value: Vec::new(),
                },
                CharacteristicInfo {
                    uuid: "0003".to_string(),
                    properties: CharacteristicPropertiesFlags::read_write_notify(),
                    value: Vec::new(),
                },
                CharacteristicInfo {
                    uuid: "0004".to_string(),
                    properties: CharacteristicPropertiesFlags::writable(),
                    value: Vec::new(),
                },
                CharacteristicInfo {
                    uuid: "0005".to_string(),
                    properties: CharacteristicPropertiesFlags {
                        read: true,
                        notify: true,
                        ..Default::default()
                    },
                    value: Vec::new(),
                },
            ],
        };

        self.services
            .write()
            .await
            .insert(HIVE_SERVICE_UUID.to_string(), service_info);

        Ok(())
    }

    /// Create all HIVE characteristics (synchronous helper)
    fn create_hive_characteristics(
        &self,
        node_id: NodeId,
    ) -> Result<(
        Vec<Retained<CBMutableCharacteristic>>,
        Vec<(String, Retained<CBMutableCharacteristic>)>,
    )> {
        let mut cb_characteristics = Vec::new();
        let mut char_entries = Vec::new();

        // Node Info (0x0001): Read - Node ID
        let node_info_char = self.create_characteristic(
            "0001",
            CharacteristicPropertiesFlags::readable(),
            Some(&node_id.as_u32().to_le_bytes()),
        )?;
        char_entries.push(("0001".to_string(), node_info_char.clone()));
        cb_characteristics.push(node_info_char);

        // Sync State (0x0002): Read/Write/Notify
        let sync_state_char = self.create_characteristic(
            "0002",
            CharacteristicPropertiesFlags::read_write_notify(),
            None,
        )?;
        char_entries.push(("0002".to_string(), sync_state_char.clone()));
        cb_characteristics.push(sync_state_char);

        // Sync Data (0x0003): Read/Write/Notify
        let sync_data_char = self.create_characteristic(
            "0003",
            CharacteristicPropertiesFlags::read_write_notify(),
            None,
        )?;
        char_entries.push(("0003".to_string(), sync_data_char.clone()));
        cb_characteristics.push(sync_data_char);

        // Command (0x0004): Write
        let command_char =
            self.create_characteristic("0004", CharacteristicPropertiesFlags::writable(), None)?;
        char_entries.push(("0004".to_string(), command_char.clone()));
        cb_characteristics.push(command_char);

        // Status (0x0005): Read/Notify
        let status_char = self.create_characteristic(
            "0005",
            CharacteristicPropertiesFlags {
                read: true,
                notify: true,
                ..Default::default()
            },
            None,
        )?;
        char_entries.push(("0005".to_string(), status_char.clone()));
        cb_characteristics.push(status_char);

        Ok((cb_characteristics, char_entries))
    }

    /// Create a CBMutableCharacteristic
    fn create_characteristic(
        &self,
        uuid_str: &str,
        props: CharacteristicPropertiesFlags,
        value: Option<&[u8]>,
    ) -> Result<Retained<CBMutableCharacteristic>> {
        let uuid = unsafe {
            let ns_uuid = NSString::from_str(uuid_str);
            CBUUID::UUIDWithString(&ns_uuid)
        };

        let cb_props = props.to_cb_properties();

        // Set permissions based on properties
        let mut permissions = CBAttributePermissions::empty();
        if props.read {
            permissions |= CBAttributePermissions::Readable;
        }
        if props.write || props.write_without_response {
            permissions |= CBAttributePermissions::Writeable;
        }

        let ns_value = value.map(|v| NSData::with_bytes(v));

        let characteristic = unsafe {
            CBMutableCharacteristic::initWithType_properties_value_permissions(
                CBMutableCharacteristic::alloc(),
                &uuid,
                cb_props,
                ns_value.as_deref(),
                permissions,
            )
        };

        Ok(characteristic)
    }

    /// Unregister all GATT services
    pub async fn unregister_all_services(&self) -> Result<()> {
        unsafe {
            self.manager.removeAllServices();
        }

        self.services.write().await.clear();
        self.characteristics.write().unwrap().clear();
        log::info!("Removed all GATT services");
        Ok(())
    }

    /// Start advertising
    ///
    /// # Arguments
    /// * `node_id` - Node ID to include in advertisement
    /// * `_config` - Discovery configuration (currently unused)
    pub async fn start_advertising(
        &self,
        node_id: NodeId,
        _config: &DiscoveryConfig,
    ) -> Result<()> {
        let local_name = format!("HIVE-{:08X}", node_id.as_u32());

        // Build advertisement data dictionary with local name and service UUIDs
        // This is required for Android devices to discover this iOS peripheral
        unsafe {
            // Create the local name string
            let name_str = NSString::from_str(&local_name);

            // Create the service UUID
            let service_uuid_str = NSString::from_str(&HIVE_SERVICE_UUID.to_string());
            let service_uuid = CBUUID::UUIDWithString(&service_uuid_str);

            // Create array of service UUIDs (cast CBUUID to AnyObject via pointer)
            let uuid_ptr: *const AnyObject = Retained::as_ptr(&service_uuid).cast();
            let service_uuids: Retained<NSArray<AnyObject>> =
                NSArray::from_vec(vec![Retained::from_raw(uuid_ptr as *mut AnyObject).unwrap()]);

            // Build the advertisement dictionary using raw msg_send
            // Create keys and values arrays
            let keys: Retained<NSArray<NSString>> = {
                // Use msg_send to retain the static key since .retain() isn't available
                let key_ptr: *mut NSString = msg_send![CBAdvertisementDataLocalNameKey, retain];
                NSArray::from_vec(vec![Retained::from_raw(key_ptr).unwrap()])
            };
            let values: Retained<NSArray<AnyObject>> = {
                let name_ptr: *const AnyObject = Retained::as_ptr(&name_str).cast();
                NSArray::from_vec(vec![Retained::from_raw(name_ptr as *mut AnyObject).unwrap()])
            };

            // Use dictionaryWithObjects:forKeys: class method
            let dict_ptr: *mut NSDictionary<NSString, AnyObject> = msg_send![
                objc2::class!(NSDictionary),
                dictionaryWithObjects: Retained::as_ptr(&values),
                forKeys: Retained::as_ptr(&keys)
            ];
            let _ad_data = Retained::from_raw(dict_ptr);

            // For now, just start advertising without the dictionary to verify the approach
            // The service UUID is already registered via addService, which CoreBluetooth
            // will automatically include in the advertisement
            self.manager.startAdvertising(None);
        }

        log::info!("Started advertising as {}", local_name);
        *self.advertising.write().await = true;
        Ok(())
    }

    /// Stop advertising
    pub async fn stop_advertising(&self) -> Result<()> {
        unsafe {
            self.manager.stopAdvertising();
        }

        log::info!("Stopped advertising");
        *self.advertising.write().await = false;
        Ok(())
    }

    /// Check if currently advertising
    pub async fn is_advertising(&self) -> bool {
        // Use the actual CoreBluetooth state
        unsafe { self.manager.isAdvertising() }
    }

    /// Set the value for a characteristic
    ///
    /// This value will be returned when a central device reads the characteristic.
    /// Use this to set sync data or other readable values.
    pub fn set_characteristic_value(&self, characteristic_uuid: &str, value: Vec<u8>) {
        self.delegate
            .set_characteristic_value(characteristic_uuid, value);
    }

    /// Get the current value for a characteristic
    pub fn get_characteristic_value(&self, characteristic_uuid: &str) -> Option<Vec<u8>> {
        self.delegate.get_characteristic_value(characteristic_uuid)
    }

    /// Send notification to subscribed centrals
    pub async fn send_notification(&self, characteristic_uuid: &str, value: &[u8]) -> Result<bool> {
        // Use sync lock for characteristics (contains non-Send types)
        let chars = self.characteristics.read().unwrap();
        let characteristic = chars.get(characteristic_uuid).ok_or_else(|| {
            BleError::PlatformError(format!("Unknown characteristic: {}", characteristic_uuid))
        })?;

        let data = NSData::with_bytes(value);

        let result = unsafe {
            self.manager
                .updateValue_forCharacteristic_onSubscribedCentrals(&data, characteristic, None)
        };

        if result {
            log::trace!("Sent notification on {}", characteristic_uuid);
        } else {
            log::debug!("Notification queue full for {}", characteristic_uuid);
        }

        Ok(result)
    }

    /// Get subscribers for a characteristic
    pub async fn get_subscribers(&self, characteristic_uuid: &str) -> Vec<String> {
        let subscribers = self.subscribers.read().await;
        subscribers
            .get(characteristic_uuid)
            .cloned()
            .unwrap_or_default()
    }

    /// Process pending delegate events
    pub async fn process_events(&self) -> Result<()> {
        let mut event_rx = self.event_rx.write().await;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                PeripheralManagerEvent::StateChanged(state) => {
                    log::debug!("Peripheral manager state changed: {:?}", state);
                    *self.state.write().await = state;
                }
                PeripheralManagerEvent::ServiceAdded {
                    service_uuid,
                    error,
                } => {
                    if let Some(e) = error {
                        log::error!("Failed to add service {}: {}", service_uuid, e);
                    } else {
                        log::info!("Service {} added successfully", service_uuid);
                    }
                }
                PeripheralManagerEvent::AdvertisingStarted { error } => {
                    if let Some(e) = error {
                        log::error!("Advertising failed: {}", e);
                        *self.advertising.write().await = false;
                    } else {
                        log::info!("Advertising started successfully");
                    }
                }
                PeripheralManagerEvent::CentralSubscribed {
                    central_identifier,
                    characteristic_uuid,
                } => {
                    log::info!(
                        "Central {} subscribed to {}",
                        central_identifier,
                        characteristic_uuid
                    );
                    let mut subscribers = self.subscribers.write().await;
                    subscribers
                        .entry(characteristic_uuid)
                        .or_default()
                        .push(central_identifier);
                }
                PeripheralManagerEvent::CentralUnsubscribed {
                    central_identifier,
                    characteristic_uuid,
                } => {
                    log::info!(
                        "Central {} unsubscribed from {}",
                        central_identifier,
                        characteristic_uuid
                    );
                    let mut subscribers = self.subscribers.write().await;
                    if let Some(subs) = subscribers.get_mut(&characteristic_uuid) {
                        subs.retain(|id| id != &central_identifier);
                    }
                }
                PeripheralManagerEvent::ReadRequest { .. } => {
                    // TODO: Handle read requests
                    log::debug!("Received read request (handling not implemented)");
                }
                PeripheralManagerEvent::WriteRequest { .. } => {
                    // TODO: Handle write requests
                    log::debug!("Received write request (handling not implemented)");
                }
                PeripheralManagerEvent::ReadyToUpdateSubscribers => {
                    log::trace!("Ready to send more notifications");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_characteristic_properties() {
        let props = CharacteristicPropertiesFlags::read_write_notify();
        assert!(props.read);
        assert!(props.write);
        assert!(props.notify);
        assert!(!props.indicate);

        let cb_props = props.to_cb_properties();
        assert!(cb_props.contains(CBCharacteristicProperties::CBCharacteristicPropertyRead));
        assert!(cb_props.contains(CBCharacteristicProperties::CBCharacteristicPropertyWrite));
        assert!(cb_props.contains(CBCharacteristicProperties::CBCharacteristicPropertyNotify));
    }

    #[test]
    fn test_service_info() {
        let service = ServiceInfo {
            uuid: "D479".to_string(),
            is_primary: true,
            characteristics: vec![CharacteristicInfo {
                uuid: "0001".to_string(),
                properties: CharacteristicPropertiesFlags::readable(),
                value: vec![0xDE, 0xAD, 0xBE, 0xEF],
            }],
        };

        assert!(service.is_primary);
        assert_eq!(service.characteristics.len(), 1);
    }
}
