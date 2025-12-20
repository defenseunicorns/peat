//! JNI bridge for Android Bluetooth API
//!
//! This module provides the low-level JNI interface to Android Bluetooth classes.
//! It handles JNI environment management, object lifecycle, and callback registration.
//!
//! ## Architecture
//!
//! The Kotlin `HiveBtle` class handles BLE scanning/advertising using Android APIs.
//! When events occur (scan results, GATT events), the Kotlin proxy classes call
//! native methods defined here, which then forward events to Rust channels.
//!
//! ```text
//! Android BLE API -> Kotlin Proxy -> JNI Native -> Rust Channel -> AndroidAdapter
//! ```

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jint, jlong};
use jni::{JNIEnv, JavaVM};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::platform::{ConnectionEvent, DisconnectReason, DiscoveredDevice};
use crate::NodeId;

/// HIVE BLE Service UUID (canonical: f47ac10b-58cc-4372-a567-0e02b2c3d479)
/// Used to identify HIVE nodes during BLE scanning.
/// This is the canonical HIVE service UUID matching all platforms.
#[allow(dead_code)]
pub const HIVE_SERVICE_UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";

/// HIVE Sync Data Characteristic UUID (derived from base service UUID)
/// Used for exchanging CRDT document data between peers.
#[allow(dead_code)]
pub const HIVE_DOC_CHAR_UUID: &str = "f47a0003-58cc-4372-a567-0e02b2c3d479";

/// Global state for JNI callbacks
/// This is necessary because JNI callbacks are static functions that can't access instance state
static GLOBAL_STATE: OnceLock<Mutex<GlobalState>> = OnceLock::new();

/// Global state shared between JNI callbacks
struct GlobalState {
    /// Channel sender for scan results
    scan_tx: Option<mpsc::Sender<DiscoveredDevice>>,
    /// Channel sender for connection events
    connection_tx: Option<mpsc::Sender<(NodeId, ConnectionEvent)>>,
    /// Connection ID to NodeId mapping
    connection_map: HashMap<i64, NodeId>,
    /// Address to NodeId mapping (for connection events before we have NodeId)
    address_to_node: HashMap<String, NodeId>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            scan_tx: None,
            connection_tx: None,
            connection_map: HashMap::new(),
            address_to_node: HashMap::new(),
        }
    }
}

/// Initialize global state with channels
pub fn init_global_state(
    scan_tx: mpsc::Sender<DiscoveredDevice>,
    connection_tx: mpsc::Sender<(NodeId, ConnectionEvent)>,
) {
    let state = GlobalState {
        scan_tx: Some(scan_tx),
        connection_tx: Some(connection_tx),
        connection_map: HashMap::new(),
        address_to_node: HashMap::new(),
    };

    let _ = GLOBAL_STATE.set(Mutex::new(state));
    log::info!("JNI global state initialized");
}

/// Register a connection ID to NodeId mapping
#[allow(dead_code)]
pub fn register_connection(connection_id: i64, node_id: NodeId, address: String) {
    if let Some(state) = GLOBAL_STATE.get() {
        if let Ok(mut state) = state.lock() {
            state.connection_map.insert(connection_id, node_id.clone());
            state.address_to_node.insert(address, node_id);
        }
    }
}

/// Unregister a connection
#[allow(dead_code)]
pub fn unregister_connection(connection_id: i64) {
    if let Some(state) = GLOBAL_STATE.get() {
        if let Ok(mut state) = state.lock() {
            state.connection_map.remove(&connection_id);
        }
    }
}

/// JNI class names for Android Bluetooth API
#[allow(dead_code)]
pub mod class_names {
    pub const BLUETOOTH_ADAPTER: &str = "android/bluetooth/BluetoothAdapter";
    pub const BLUETOOTH_DEVICE: &str = "android/bluetooth/BluetoothDevice";
    pub const BLUETOOTH_GATT: &str = "android/bluetooth/BluetoothGatt";
    pub const BLUETOOTH_GATT_CALLBACK: &str = "android/bluetooth/BluetoothGattCallback";
    pub const BLUETOOTH_GATT_SERVICE: &str = "android/bluetooth/BluetoothGattService";
    pub const BLUETOOTH_GATT_CHARACTERISTIC: &str = "android/bluetooth/BluetoothGattCharacteristic";
    pub const BLUETOOTH_LE_SCANNER: &str = "android/bluetooth/le/BluetoothLeScanner";
    pub const BLUETOOTH_LE_ADVERTISER: &str = "android/bluetooth/le/BluetoothLeAdvertiser";
    pub const SCAN_CALLBACK: &str = "android/bluetooth/le/ScanCallback";
    pub const SCAN_RESULT: &str = "android/bluetooth/le/ScanResult";
    pub const SCAN_SETTINGS: &str = "android/bluetooth/le/ScanSettings";
    pub const SCAN_FILTER: &str = "android/bluetooth/le/ScanFilter";
    pub const ADVERTISE_CALLBACK: &str = "android/bluetooth/le/AdvertiseCallback";
    pub const ADVERTISE_DATA: &str = "android/bluetooth/le/AdvertiseData";
    pub const ADVERTISE_SETTINGS: &str = "android/bluetooth/le/AdvertiseSettings";
}

/// GATT status codes
#[allow(dead_code)]
pub mod gatt_status {
    pub const GATT_SUCCESS: i32 = 0;
    pub const GATT_READ_NOT_PERMITTED: i32 = 2;
    pub const GATT_WRITE_NOT_PERMITTED: i32 = 3;
    pub const GATT_INSUFFICIENT_AUTHENTICATION: i32 = 5;
    pub const GATT_REQUEST_NOT_SUPPORTED: i32 = 6;
    pub const GATT_INSUFFICIENT_ENCRYPTION: i32 = 15;
}

/// Connection states
#[allow(dead_code)]
pub mod connection_state {
    pub const STATE_DISCONNECTED: i32 = 0;
    pub const STATE_CONNECTING: i32 = 1;
    pub const STATE_CONNECTED: i32 = 2;
    pub const STATE_DISCONNECTING: i32 = 3;
}

/// JNI bridge state
#[allow(dead_code)]
pub struct JniBridge {
    /// Java VM reference (thread-safe)
    jvm: JavaVM,
    /// Android Context (global ref)
    context: GlobalRef,
    /// BluetoothAdapter instance (global ref)
    bluetooth_adapter: Option<GlobalRef>,
    /// BluetoothLeScanner instance (global ref)
    le_scanner: Option<GlobalRef>,
    /// BluetoothLeAdvertiser instance (global ref)
    le_advertiser: Option<GlobalRef>,
    /// Channel for scan results
    scan_tx: mpsc::Sender<DiscoveredDevice>,
    /// Channel for connection events
    connection_tx: mpsc::Sender<(NodeId, ConnectionEvent)>,
}

impl JniBridge {
    /// Create a new JNI bridge
    ///
    /// # Safety
    /// The caller must ensure that `env` is a valid JNI environment and
    /// `context` is a valid Android Context object.
    pub unsafe fn new(
        env: &mut JNIEnv,
        context: JObject,
        scan_tx: mpsc::Sender<DiscoveredDevice>,
        connection_tx: mpsc::Sender<(NodeId, ConnectionEvent)>,
    ) -> Result<Self> {
        // Initialize global state for callbacks
        init_global_state(scan_tx.clone(), connection_tx.clone());

        // Get JavaVM for thread-safe access
        let jvm = env
            .get_java_vm()
            .map_err(|e| BleError::PlatformError(format!("Failed to get JavaVM: {}", e)))?;

        // Create global reference to context
        let context = env
            .new_global_ref(context)
            .map_err(|e| BleError::PlatformError(format!("Failed to create context ref: {}", e)))?;

        Ok(Self {
            jvm,
            context,
            bluetooth_adapter: None,
            le_scanner: None,
            le_advertiser: None,
            scan_tx,
            connection_tx,
        })
    }

    /// Initialize the Bluetooth adapter
    pub fn init_adapter(&mut self) -> Result<()> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        // Get BluetoothAdapter via BluetoothManager
        let bluetooth_service = env
            .get_static_field(
                "android/content/Context",
                "BLUETOOTH_SERVICE",
                "Ljava/lang/String;",
            )
            .map_err(|e| {
                BleError::PlatformError(format!("Failed to get BLUETOOTH_SERVICE: {}", e))
            })?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert to object: {}", e)))?;

        let manager = env
            .call_method(
                &self.context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&bluetooth_service)],
            )
            .map_err(|e| BleError::PlatformError(format!("Failed to get BluetoothManager: {}", e)))?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert manager: {}", e)))?;

        let adapter = env
            .call_method(
                &manager,
                "getAdapter",
                "()Landroid/bluetooth/BluetoothAdapter;",
                &[],
            )
            .map_err(|e| BleError::PlatformError(format!("Failed to get BluetoothAdapter: {}", e)))?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert adapter: {}", e)))?;

        if adapter.is_null() {
            return Err(BleError::AdapterNotAvailable);
        }

        // Store global reference
        let adapter_ref = env
            .new_global_ref(&adapter)
            .map_err(|e| BleError::PlatformError(format!("Failed to create adapter ref: {}", e)))?;
        self.bluetooth_adapter = Some(adapter_ref);

        // Get LE Scanner
        let scanner = env
            .call_method(
                &adapter,
                "getBluetoothLeScanner",
                "()Landroid/bluetooth/le/BluetoothLeScanner;",
                &[],
            )
            .map_err(|e| BleError::PlatformError(format!("Failed to get LE scanner: {}", e)))?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert scanner: {}", e)))?;

        if !scanner.is_null() {
            let scanner_ref = env.new_global_ref(&scanner).map_err(|e| {
                BleError::PlatformError(format!("Failed to create scanner ref: {}", e))
            })?;
            self.le_scanner = Some(scanner_ref);
        }

        // Get LE Advertiser
        let advertiser = env
            .call_method(
                &adapter,
                "getBluetoothLeAdvertiser",
                "()Landroid/bluetooth/le/BluetoothLeAdvertiser;",
                &[],
            )
            .map_err(|e| BleError::PlatformError(format!("Failed to get LE advertiser: {}", e)))?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert advertiser: {}", e)))?;

        if !advertiser.is_null() {
            let advertiser_ref = env.new_global_ref(&advertiser).map_err(|e| {
                BleError::PlatformError(format!("Failed to create advertiser ref: {}", e))
            })?;
            self.le_advertiser = Some(advertiser_ref);
        }

        log::info!("JniBridge adapter initialized");
        Ok(())
    }

    /// Check if Bluetooth is enabled
    pub fn is_enabled(&self) -> Result<bool> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        let adapter = self
            .bluetooth_adapter
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let enabled = env
            .call_method(adapter, "isEnabled", "()Z", &[])
            .map_err(|e| BleError::PlatformError(format!("Failed to check isEnabled: {}", e)))?
            .z()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert boolean: {}", e)))?;

        Ok(enabled)
    }

    /// Get the adapter's Bluetooth address
    pub fn get_address(&self) -> Result<Option<String>> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        let adapter = self
            .bluetooth_adapter
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let address_obj = env
            .call_method(adapter, "getAddress", "()Ljava/lang/String;", &[])
            .map_err(|e| BleError::PlatformError(format!("Failed to get address: {}", e)))?
            .l()
            .map_err(|e| BleError::PlatformError(format!("Failed to convert address: {}", e)))?;

        if address_obj.is_null() {
            return Ok(None);
        }

        let address: String = env
            .get_string(&JString::from(address_obj))
            .map_err(|e| BleError::PlatformError(format!("Failed to convert string: {}", e)))?
            .into();

        Ok(Some(address))
    }

    /// Start BLE scanning
    ///
    /// Note: Scanning is actually initiated from Kotlin via HiveBtle.startScan().
    /// This method is kept for API compatibility but returns Ok since the Kotlin
    /// side handles the actual scanning.
    pub fn start_scan(&self) -> Result<()> {
        // Scanning is initiated from Kotlin HiveBtle class
        // The native callbacks will receive scan results
        log::info!("BLE scanning should be started from Kotlin HiveBtle.startScan()");
        Ok(())
    }

    /// Stop BLE scanning
    pub fn stop_scan(&self) -> Result<()> {
        // Scanning is stopped from Kotlin HiveBtle class
        log::info!("BLE scanning should be stopped from Kotlin HiveBtle.stopScan()");
        Ok(())
    }

    /// Start BLE advertising
    ///
    /// Note: Advertising is actually initiated from Kotlin via HiveBtle.startAdvertising().
    pub fn start_advertising(&self, node_id: u32, tx_power: i8) -> Result<()> {
        log::info!(
            "BLE advertising should be started from Kotlin HiveBtle.startAdvertising() (node_id: {:08X}, tx_power: {})",
            node_id,
            tx_power
        );
        Ok(())
    }

    /// Stop BLE advertising
    pub fn stop_advertising(&self) -> Result<()> {
        log::info!("BLE advertising should be stopped from Kotlin HiveBtle.stopAdvertising()");
        Ok(())
    }

    /// Connect to a BLE device by address
    ///
    /// Note: Connection is actually initiated from Kotlin via HiveBtle.connect().
    /// The Kotlin side creates a GattCallbackProxy that will call our native callbacks.
    pub fn connect_device(&self, address: &str) -> Result<GlobalRef> {
        log::info!(
            "BLE connection to {} should be initiated from Kotlin HiveBtle.connect()",
            address
        );
        // Return an error since we can't actually return a GlobalRef from here
        // The connection flow is: Kotlin initiates -> GATT callbacks come to native
        Err(BleError::NotSupported(
            "Connection should be initiated from Kotlin HiveBtle.connect()".to_string(),
        ))
    }
}

// ============================================================================
// JNI Native Method Exports - HiveBtle Lifecycle
// ============================================================================

/// Native initialization for HiveBtle
///
/// Called from Kotlin HiveBtle.init()
///
/// JNI Signature: (Landroid/content/Context;J)J
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_HiveBtle_nativeInit<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    _context: JObject<'local>,
    node_id: jlong,
) -> jlong {
    log::info!(
        "HiveBtle native init called for node {:08X}",
        node_id as u32
    );

    // Initialize global state if not already done
    // For now, we create dummy channels - the real channels will be set up
    // when AndroidAdapter is created
    if GLOBAL_STATE.get().is_none() {
        let (scan_tx, _scan_rx) = mpsc::channel(100);
        let (connection_tx, _connection_rx) = mpsc::channel(100);
        init_global_state(scan_tx, connection_tx);
    }

    // Return a non-zero handle to indicate success
    // In a full implementation, this would return a pointer to native state
    node_id
}

/// Native shutdown for HiveBtle
///
/// Called from Kotlin HiveBtle.shutdown()
///
/// JNI Signature: (J)V
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_HiveBtle_nativeShutdown<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    log::info!("HiveBtle native shutdown called for handle {}", handle);
    // Clean up native resources if needed
}

/// Derive NodeId from a BLE MAC address string
///
/// Called from Kotlin HiveBtle.nativeDeriveNodeId()
///
/// JNI Signature: (Ljava/lang/String;)J
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_HiveBtle_nativeDeriveNodeId<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    mac_address: JString<'local>,
) -> jlong {
    let mac_str: String = match env.get_string(&mac_address) {
        Ok(s) => s.into(),
        Err(e) => {
            log::error!("Failed to get MAC address string: {:?}", e);
            return 0;
        }
    };

    match NodeId::from_mac_string(&mac_str) {
        Some(node_id) => {
            log::debug!(
                "Derived NodeId {:08X} from MAC {}",
                node_id.as_u32(),
                mac_str
            );
            node_id.as_u32() as jlong
        }
        None => {
            log::warn!("Failed to parse MAC address: {}", mac_str);
            0
        }
    }
}

// ============================================================================
// JNI Native Method Exports - Scan Callbacks
// ============================================================================

/// Native callback for scan results
///
/// Called from Kotlin ScanCallbackProxy.nativeOnScanResult()
///
/// JNI Signature: (ILjava/lang/String;Ljava/lang/String;I[Ljava/lang/String;[BJ)V
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_ScanCallbackProxy_nativeOnScanResult<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    _callback_type: jint,
    address: JString<'local>,
    name: JString<'local>,
    rssi: jint,
    service_uuids: JObjectArray<'local>,
    hive_service_data: JByteArray<'local>,
    _timestamp_nanos: jlong,
) {
    // Extract address
    let address: String = match env.get_string(&address) {
        Ok(s) => s.into(),
        Err(e) => {
            log::error!("Failed to get address string: {}", e);
            return;
        }
    };

    // Extract name
    let name: String = match env.get_string(&name) {
        Ok(s) => s.into(),
        Err(e) => {
            log::warn!("Failed to get name string: {}", e);
            String::new()
        }
    };

    // Extract service UUIDs
    let mut uuids = Vec::new();
    if !service_uuids.is_null() {
        if let Ok(len) = env.get_array_length(&service_uuids) {
            for i in 0..len {
                if let Ok(uuid_obj) = env.get_object_array_element(&service_uuids, i) {
                    if let Ok(uuid_str) = env.get_string(&JString::from(uuid_obj)) {
                        uuids.push(uuid_str.into());
                    }
                }
            }
        }
    }

    // Extract HIVE service data to get node ID
    let mut node_id: Option<NodeId> = None;
    if !hive_service_data.is_null() {
        if let Ok(data) = env.convert_byte_array(hive_service_data) {
            if data.len() >= 4 {
                // Node ID is stored as big-endian 4 bytes
                let id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                node_id = Some(NodeId::new(id));
                log::debug!("Extracted HIVE node ID: {:08X}", id);
            }
        }
    }

    // Check if this is a HIVE device
    let is_hive = name.starts_with("HIVE-")
        || uuids
            .iter()
            .any(|u: &String| u.to_uppercase().contains("D479"));

    log::debug!(
        "Scan result: {} ({}) RSSI={} HIVE={} nodeId={:?}",
        address,
        name,
        rssi,
        is_hive,
        node_id
    );

    // Create DiscoveredDevice and send via channel
    let device = DiscoveredDevice {
        address: address.clone(),
        name: if name.is_empty() {
            None
        } else {
            Some(name.clone())
        },
        rssi: rssi as i8,
        is_hive_node: is_hive,
        node_id,
        adv_data: Vec::new(), // Raw adv data not easily available from parsed result
    };

    // Send to channel
    if let Some(state) = GLOBAL_STATE.get() {
        if let Ok(state) = state.lock() {
            if let Some(ref tx) = state.scan_tx {
                if let Err(e) = tx.try_send(device) {
                    log::warn!("Failed to send scan result: {}", e);
                }
            }
        }
    }
}

/// Native callback for scan failures
///
/// Called from Kotlin ScanCallbackProxy.nativeOnScanFailed()
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_ScanCallbackProxy_nativeOnScanFailed<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    error_code: jint,
    error_message: JString<'local>,
) {
    let msg: String = env
        .get_string(&error_message)
        .map(|s| s.into())
        .unwrap_or_else(|_| "Unknown error".to_string());

    log::error!("BLE scan failed: {} (code={})", msg, error_code);
}

// ============================================================================
// JNI Native Method Exports - GATT Callbacks
// ============================================================================

/// Native callback for connection state changes
///
/// Called from Kotlin GattCallbackProxy.nativeOnConnectionStateChange()
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnConnectionStateChange<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    address: JString<'local>,
    status: jint,
    new_state: jint,
) {
    let address: String = env
        .get_string(&address)
        .map(|s| s.into())
        .unwrap_or_default();

    log::info!(
        "Connection state change: conn={} addr={} status={} state={}",
        connection_id,
        address,
        status,
        new_state
    );

    // Get NodeId for this connection
    let node_id = if let Some(state) = GLOBAL_STATE.get() {
        if let Ok(state) = state.lock() {
            state
                .connection_map
                .get(&connection_id)
                .cloned()
                .or_else(|| state.address_to_node.get(&address).cloned())
        } else {
            None
        }
    } else {
        None
    };

    let node_id = match node_id {
        Some(id) => id,
        None => {
            // Create a temporary NodeId from address hash if we don't have one
            let hash = address
                .bytes()
                .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            NodeId::new(hash)
        }
    };

    // Create connection event
    let event = match new_state {
        state if state == connection_state::STATE_CONNECTED => {
            if status == gatt_status::GATT_SUCCESS {
                ConnectionEvent::Connected {
                    mtu: 23, // Default, will be updated by MTU callback
                    phy: BlePhy::Le1M,
                }
            } else {
                ConnectionEvent::Disconnected {
                    reason: DisconnectReason::ConnectionFailed,
                }
            }
        }
        state if state == connection_state::STATE_DISCONNECTED => ConnectionEvent::Disconnected {
            reason: if status == gatt_status::GATT_SUCCESS {
                DisconnectReason::LocalRequest
            } else {
                DisconnectReason::RemoteRequest
            },
        },
        _ => return, // Ignore connecting/disconnecting states
    };

    // Send event
    if let Some(state) = GLOBAL_STATE.get() {
        if let Ok(state) = state.lock() {
            if let Some(ref tx) = state.connection_tx {
                if let Err(e) = tx.try_send((node_id, event)) {
                    log::warn!("Failed to send connection event: {}", e);
                }
            }
        }
    }
}

/// Native callback for services discovered
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnServicesDiscovered<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    address: JString<'local>,
    status: jint,
    service_uuids: JObjectArray<'local>,
) {
    let address: String = env
        .get_string(&address)
        .map(|s| s.into())
        .unwrap_or_default();

    let mut uuids: Vec<String> = Vec::new();
    if !service_uuids.is_null() {
        if let Ok(len) = env.get_array_length(&service_uuids) {
            for i in 0..len {
                if let Ok(uuid_obj) = env.get_object_array_element(&service_uuids, i) {
                    if let Ok(uuid_str) = env.get_string(&JString::from(uuid_obj)) {
                        uuids.push(uuid_str.into());
                    }
                }
            }
        }
    }

    let has_hive_service = uuids.iter().any(|u| u.to_uppercase().contains("D479"));

    log::info!(
        "Services discovered: conn={} addr={} status={} services={} hive={}",
        connection_id,
        address,
        status,
        uuids.len(),
        has_hive_service
    );

    // Send ServicesDiscovered event
    if status == gatt_status::GATT_SUCCESS {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::ServicesDiscovered { has_hive_service };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

/// Native callback for characteristic read
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnCharacteristicRead<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    service_uuid: JString<'local>,
    char_uuid: JString<'local>,
    status: jint,
    value: JByteArray<'local>,
) {
    let service: String = env
        .get_string(&service_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let char: String = env
        .get_string(&char_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let data: Vec<u8> = env.convert_byte_array(value).unwrap_or_default();

    log::debug!(
        "Characteristic read: conn={} service={} char={} status={} len={}",
        connection_id,
        service,
        char,
        status,
        data.len()
    );

    // Send DataReceived event
    if status == gatt_status::GATT_SUCCESS && !data.is_empty() {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::DataReceived { data };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

/// Native callback for characteristic write
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnCharacteristicWrite<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    service_uuid: JString<'local>,
    char_uuid: JString<'local>,
    status: jint,
) {
    let service: String = env
        .get_string(&service_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let char: String = env
        .get_string(&char_uuid)
        .map(|s| s.into())
        .unwrap_or_default();

    log::debug!(
        "Characteristic write: conn={} service={} char={} status={}",
        connection_id,
        service,
        char,
        status
    );
}

/// Native callback for characteristic changed (notifications)
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnCharacteristicChanged<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    service_uuid: JString<'local>,
    char_uuid: JString<'local>,
    value: JByteArray<'local>,
) {
    let service: String = env
        .get_string(&service_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let char: String = env
        .get_string(&char_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let data: Vec<u8> = env.convert_byte_array(value).unwrap_or_default();

    log::debug!(
        "Characteristic notification: conn={} service={} char={} len={}",
        connection_id,
        service,
        char,
        data.len()
    );

    // Send DataReceived event for notifications
    if !data.is_empty() {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::DataReceived { data };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

/// Native callback for descriptor write
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnDescriptorWrite<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    connection_id: jlong,
    service_uuid: JString<'local>,
    char_uuid: JString<'local>,
    descriptor_uuid: JString<'local>,
    status: jint,
) {
    let service: String = env
        .get_string(&service_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let char: String = env
        .get_string(&char_uuid)
        .map(|s| s.into())
        .unwrap_or_default();
    let desc: String = env
        .get_string(&descriptor_uuid)
        .map(|s| s.into())
        .unwrap_or_default();

    log::debug!(
        "Descriptor write: conn={} service={} char={} desc={} status={}",
        connection_id,
        service,
        char,
        desc,
        status
    );
}

/// Native callback for MTU changed
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnMtuChanged(
    _env: JNIEnv,
    _class: JClass,
    connection_id: jlong,
    mtu: jint,
    status: jint,
) {
    log::info!(
        "MTU changed: conn={} mtu={} status={}",
        connection_id,
        mtu,
        status
    );

    if status == gatt_status::GATT_SUCCESS {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::MtuChanged { mtu: mtu as u16 };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

/// Native callback for PHY update
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnPhyUpdate(
    _env: JNIEnv,
    _class: JClass,
    connection_id: jlong,
    tx_phy: jint,
    rx_phy: jint,
    status: jint,
) {
    // Map Android PHY constants to our BlePhy enum
    // Android: PHY_LE_1M=1, PHY_LE_2M=2, PHY_LE_CODED=3
    let phy = match tx_phy {
        1 => BlePhy::Le1M,
        2 => BlePhy::Le2M,
        3 => BlePhy::LeCodedS2, // Android doesn't distinguish S2/S8
        _ => BlePhy::Le1M,
    };

    log::info!(
        "PHY update: conn={} tx={} rx={} status={} -> {:?}",
        connection_id,
        tx_phy,
        rx_phy,
        status,
        phy
    );

    if status == gatt_status::GATT_SUCCESS {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::PhyChanged { phy };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

/// Native callback for RSSI read
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_nativeOnReadRemoteRssi(
    _env: JNIEnv,
    _class: JClass,
    connection_id: jlong,
    rssi: jint,
    status: jint,
) {
    log::debug!(
        "RSSI read: conn={} rssi={} status={}",
        connection_id,
        rssi,
        status
    );

    if status == gatt_status::GATT_SUCCESS {
        if let Some(state) = GLOBAL_STATE.get() {
            if let Ok(state) = state.lock() {
                if let Some(node_id) = state.connection_map.get(&connection_id) {
                    if let Some(ref tx) = state.connection_tx {
                        let event = ConnectionEvent::RssiUpdated { rssi: rssi as i8 };
                        let _ = tx.try_send((node_id.clone(), event));
                    }
                }
            }
        }
    }
}

// ============================================================================
// JNI Native Method Exports - Advertise Callbacks
// ============================================================================

/// Native callback for advertising start success
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_AdvertiseCallbackProxy_nativeOnStartSuccess(
    _env: JNIEnv,
    _class: JClass,
    mode: jint,
    tx_power_level: jint,
    is_connectable: jboolean,
    timeout: jint,
) {
    log::info!(
        "Advertising started: mode={} txPower={} connectable={} timeout={}",
        mode,
        tx_power_level,
        is_connectable != 0,
        timeout
    );
}

/// Native callback for advertising start failure
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_AdvertiseCallbackProxy_nativeOnStartFailure<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    error_code: jint,
    error_message: JString<'local>,
) {
    let msg: String = env
        .get_string(&error_message)
        .map(|s| s.into())
        .unwrap_or_else(|_| "Unknown error".to_string());

    log::error!("Advertising failed: {} (code={})", msg, error_code);
}

#[cfg(test)]
mod tests {
    // JNI tests require Android runtime environment
    // They should be run via Android instrumentation tests
}
