//! JNI bridge for Android Bluetooth API
//!
//! This module provides the low-level JNI interface to Android Bluetooth classes.
//! It handles JNI environment management, object lifecycle, and callback registration.

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong};
use jni::{JNIEnv, JavaVM};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::{BleError, Result};
use crate::platform::{ConnectionEvent, DiscoveredDevice};
use crate::NodeId;

/// JNI class names for Android Bluetooth API
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

/// JNI bridge state
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
        // BluetoothManager manager = context.getSystemService(Context.BLUETOOTH_SERVICE);
        // BluetoothAdapter adapter = manager.getAdapter();
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
    /// # TODO
    /// - Create ScanCallback implementation
    /// - Build ScanSettings with appropriate mode
    /// - Add ScanFilters for HIVE service UUID
    /// - Call BluetoothLeScanner.startScan()
    pub fn start_scan(&self) -> Result<()> {
        let _env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        let _scanner = self
            .le_scanner
            .as_ref()
            .ok_or_else(|| BleError::NotSupported("LE Scanner not available".to_string()))?;

        // TODO: Implement JNI scan callback
        // 1. Create Java proxy class for ScanCallback
        // 2. Register native methods for onScanResult, onBatchScanResults, onScanFailed
        // 3. Build ScanSettings (SCAN_MODE_LOW_LATENCY for active, SCAN_MODE_LOW_POWER for passive)
        // 4. Build ScanFilter for HIVE_SERVICE_UUID
        // 5. Call scanner.startScan(filters, settings, callback)

        log::warn!("Android BLE scanning not yet implemented");
        Err(BleError::NotSupported(
            "Android scanning not yet implemented".to_string(),
        ))
    }

    /// Stop BLE scanning
    pub fn stop_scan(&self) -> Result<()> {
        // TODO: Call scanner.stopScan(callback)
        log::warn!("Android BLE stop scanning not yet implemented");
        Ok(())
    }

    /// Start BLE advertising
    ///
    /// # TODO
    /// - Create AdvertiseCallback implementation
    /// - Build AdvertiseSettings with appropriate mode and TX power
    /// - Build AdvertiseData with HIVE service UUID and node ID
    /// - Call BluetoothLeAdvertiser.startAdvertising()
    pub fn start_advertising(&self, node_id: u32, tx_power: i8) -> Result<()> {
        let _env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        let _advertiser = self
            .le_advertiser
            .as_ref()
            .ok_or_else(|| BleError::NotSupported("LE Advertiser not available".to_string()))?;

        // TODO: Implement JNI advertising
        // 1. Build AdvertiseSettings
        //    - setAdvertiseMode(ADVERTISE_MODE_LOW_LATENCY | BALANCED | LOW_POWER)
        //    - setTxPowerLevel(ADVERTISE_TX_POWER_HIGH | MEDIUM | LOW | ULTRA_LOW)
        //    - setConnectable(true)
        // 2. Build AdvertiseData
        //    - addServiceUuid(HIVE_SERVICE_UUID)
        //    - setIncludeDeviceName(true)
        //    - addServiceData(HIVE_SERVICE_UUID, node_id bytes)
        // 3. Create AdvertiseCallback proxy
        // 4. Call advertiser.startAdvertising(settings, data, callback)

        log::warn!(
            "Android BLE advertising not yet implemented (node_id: {:08X}, tx_power: {})",
            node_id,
            tx_power
        );
        Err(BleError::NotSupported(
            "Android advertising not yet implemented".to_string(),
        ))
    }

    /// Stop BLE advertising
    pub fn stop_advertising(&self) -> Result<()> {
        // TODO: Call advertiser.stopAdvertising(callback)
        log::warn!("Android BLE stop advertising not yet implemented");
        Ok(())
    }

    /// Connect to a BLE device by address
    ///
    /// # TODO
    /// - Get BluetoothDevice from adapter.getRemoteDevice(address)
    /// - Create BluetoothGattCallback proxy
    /// - Call device.connectGatt(context, autoConnect=false, callback, TRANSPORT_LE)
    /// - Return BluetoothGatt handle
    pub fn connect_device(&self, address: &str) -> Result<GlobalRef> {
        let mut env = self
            .jvm
            .attach_current_thread()
            .map_err(|e| BleError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        let adapter = self
            .bluetooth_adapter
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Get remote device
        let address_jstring = env
            .new_string(address)
            .map_err(|e| BleError::PlatformError(format!("Failed to create string: {}", e)))?;

        let device = env
            .call_method(
                adapter,
                "getRemoteDevice",
                "(Ljava/lang/String;)Landroid/bluetooth/BluetoothDevice;",
                &[JValue::Object(&address_jstring)],
            )
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to get remote device: {}", e)))?
            .l()
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to convert device: {}", e)))?;

        if device.is_null() {
            return Err(BleError::ConnectionFailed(format!(
                "Device not found: {}",
                address
            )));
        }

        // TODO: Implement GATT connection
        // 1. Create BluetoothGattCallback proxy class
        // 2. Register native callbacks for:
        //    - onConnectionStateChange
        //    - onServicesDiscovered
        //    - onCharacteristicRead
        //    - onCharacteristicWrite
        //    - onCharacteristicChanged
        //    - onMtuChanged
        // 3. Call device.connectGatt(context, false, callback, TRANSPORT_LE)

        log::warn!(
            "Android GATT connection not yet implemented for {}",
            address
        );
        Err(BleError::NotSupported(
            "Android GATT connection not yet implemented".to_string(),
        ))
    }
}

// === JNI Native Method Exports ===
//
// These functions are called from Java/Kotlin code via JNI.
// They must be exported with the correct naming convention.

/// Native callback for scan results
///
/// Called from Java ScanCallback.onScanResult()
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_ScanCallbackProxy_onScanResult(
    _env: JNIEnv,
    _class: JClass,
    _callback_type: jint,
    _result: JObject,
) {
    // TODO: Parse ScanResult and send to channel
    // 1. Get device address from result.getDevice().getAddress()
    // 2. Get device name from result.getScanRecord().getDeviceName()
    // 3. Get RSSI from result.getRssi()
    // 4. Get service UUIDs from result.getScanRecord().getServiceUuids()
    // 5. Get service data from result.getScanRecord().getServiceData()
    // 6. Create DiscoveredDevice and send via scan_tx
    log::trace!("JNI onScanResult callback (not yet implemented)");
}

/// Native callback for connection state changes
///
/// Called from Java BluetoothGattCallback.onConnectionStateChange()
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onConnectionStateChange(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _status: jint,
    _new_state: jint,
) {
    // TODO: Handle connection state change
    // STATE_DISCONNECTED = 0
    // STATE_CONNECTING = 1
    // STATE_CONNECTED = 2
    // STATE_DISCONNECTING = 3
    log::trace!("JNI onConnectionStateChange callback (not yet implemented)");
}

/// Native callback for services discovered
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onServicesDiscovered(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _status: jint,
) {
    log::trace!("JNI onServicesDiscovered callback (not yet implemented)");
}

/// Native callback for characteristic read
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onCharacteristicRead(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _characteristic: JObject,
    _status: jint,
) {
    log::trace!("JNI onCharacteristicRead callback (not yet implemented)");
}

/// Native callback for characteristic write
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onCharacteristicWrite(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _characteristic: JObject,
    _status: jint,
) {
    log::trace!("JNI onCharacteristicWrite callback (not yet implemented)");
}

/// Native callback for characteristic changed (notifications)
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onCharacteristicChanged(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _characteristic: JObject,
) {
    log::trace!("JNI onCharacteristicChanged callback (not yet implemented)");
}

/// Native callback for MTU changed
#[no_mangle]
pub extern "system" fn Java_com_hive_btle_GattCallbackProxy_onMtuChanged(
    _env: JNIEnv,
    _class: JClass,
    _gatt: JObject,
    _mtu: jint,
    _status: jint,
) {
    log::trace!("JNI onMtuChanged callback (not yet implemented)");
}

#[cfg(test)]
mod tests {
    // JNI tests require Android runtime environment
    // They should be run via Android instrumentation tests
}
