# ADR-047: Android BLE Hybrid Integration Architecture

**Status**: Proposed
**Date**: 2025-01-28
**Authors**: Kit Plummer, Claude
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-039 (HIVE-BTLE Mesh Transport), ADR-032 (Pluggable Transport Abstraction), ADR-041 (Multi-Transport Embedded Integration)

---

## Executive Summary

This ADR defines the **Hybrid Integration Architecture** for Android BLE transport, where Kotlin handles BLE radio operations (scanning, advertising, GATT) while Rust handles mesh logic (peer management, encryption, CRDT sync). This approach leverages Android's mature BLE stack while maintaining hive-btle's cross-platform mesh protocol, enabling unified transport management via `TransportManager` for the ATAK plugin.

---

## Context

### Current State (Dual System)

The ATAK plugin currently runs two parallel, disconnected transport systems:

```
ATAK Plugin
├── HiveNodeJni (hive-ffi)
│   └── IrohTransport (QUIC)
│       └── Iroh mesh peers
│
└── HiveBleManager (hive-btle AAR)  ← SEPARATE, NOT UNIFIED
    └── Direct BLE mesh
        └── WearTAK devices
```

**Problems**:
1. Data doesn't flow between transports (PLI only goes over Iroh, not BLE)
2. Dual initialization, dual state management, dual callbacks
3. `HiveBleManager` is deprecated but still required
4. Android BLE adapter in hive-btle is a stub (Kotlin does actual BLE work)

### ADR-039 Vision

ADR-039 proposed unified transport via `TransportManager`:

```
ATAK Plugin
└── HiveNodeJni (hive-ffi)
    └── TransportManager (PACE policy)
        ├── IrohTransport (Primary)
        └── HiveBleTransport (Alternate)  ← UNIFIED
            └── Both Iroh AND BLE peers
```

### The Android BLE Challenge

Unlike Linux (BlueZ) and Apple (CoreBluetooth), Android BLE presents unique challenges:

| Challenge | Linux/Apple | Android |
|-----------|-------------|---------|
| Lifecycle | System daemon | Activity/Service bound |
| Permissions | Root or group | Runtime permissions per-call |
| Background | Always available | Doze mode, battery optimization |
| JNI overhead | N/A | Significant for callbacks |
| API style | Async callbacks | Callback + coroutines hybrid |

**Failed Approach**: Pure Rust Android adapter via JNI
- High latency for scan/advertise callbacks crossing JNI boundary
- Complex lifecycle management from Rust
- Permission handling requires Android Context
- Doze/battery optimization callbacks difficult to handle

---

## Decision

### Option C: Hybrid Architecture

**Kotlin** handles all Android BLE operations:
- BLE scanning and advertising
- GATT server and client operations
- Android lifecycle (Activity, Service, permissions)
- Background execution (foreground service, WorkManager)

**Rust** handles all mesh protocol operations:
- Peer state machine (discovered → connected → syncing)
- Mesh encryption (Phase 1 mesh-wide, Phase 2 per-peer E2EE)
- CRDT sync protocol (HiveDocument, delta encoding)
- Observer notifications (HiveEvent dispatch)
- TransportManager integration

**Bridge**: Kotlin ↔ Rust via JNI callbacks:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ATAK Plugin (Kotlin)                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    HiveNodeJni (FFI boundary)                 │  │
│  └───────────────────────────────┬───────────────────────────────┘  │
│                                  │                                  │
│  ┌───────────────────────────────┼───────────────────────────────┐  │
│  │              TransportManager │ (Rust)                        │  │
│  │  ┌────────────────────────────┼────────────────────────────┐  │  │
│  │  │        HiveBleTransport<AndroidBridgeAdapter>           │  │  │
│  │  │                            │                            │  │  │
│  │  │   ┌────────────────────────┼──────────────────────────┐ │  │  │
│  │  │   │              AndroidBridgeAdapter                 │ │  │  │
│  │  │   │  (Rust struct with JNI callback pointers)         │ │  │  │
│  │  │   └────────────────────────┼──────────────────────────┘ │  │  │
│  │  └────────────────────────────┼────────────────────────────┘  │  │
│  └───────────────────────────────┼───────────────────────────────┘  │
│                                  │ JNI                              │
│  ┌───────────────────────────────┼───────────────────────────────┐  │
│  │              AndroidBleDelegate (Kotlin)                      │  │
│  │  - BluetoothLeScanner                                         │  │
│  │  - BluetoothLeAdvertiser                                      │  │
│  │  - BluetoothGattServer / BluetoothGatt                        │  │
│  │  - Lifecycle management                                       │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Architecture

### 1. Rust Side: AndroidBridgeAdapter

```rust
// hive-btle/src/platform/android/bridge_adapter.rs

use jni::JNIEnv;
use jni::objects::{GlobalRef, JObject};

/// Android BLE adapter that delegates to Kotlin via JNI
pub struct AndroidBridgeAdapter {
    /// Reference to Kotlin AndroidBleDelegate instance
    delegate: GlobalRef,
    /// Cached JVM reference for callbacks
    jvm: JavaVM,
    /// Node configuration
    config: BleConfig,
    /// Internal state (managed in Rust)
    state: RwLock<AdapterState>,
}

#[async_trait]
impl BleAdapter for AndroidBridgeAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        self.config = config.clone();

        // Call Kotlin: delegate.initialize(nodeId, meshId, powerProfile)
        let env = self.jvm.attach_current_thread()?;
        env.call_method(
            &self.delegate,
            "initialize",
            "(JLjava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Long(config.node_id.as_u32() as i64),
                JValue::Object(&env.new_string(&config.mesh_id)?),
                JValue::Object(&env.new_string(config.power_profile.name())?),
            ],
        )?;

        Ok(())
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        let env = self.jvm.attach_current_thread()?;
        env.call_method(&self.delegate, "startScan", "()V", &[])?;
        Ok(())
    }

    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()> {
        let env = self.jvm.attach_current_thread()?;

        // Build beacon data in Rust, pass to Kotlin for advertising
        let beacon = HiveBeacon::new(&self.config);
        let beacon_bytes = beacon.encode();
        let byte_array = env.byte_array_from_slice(&beacon_bytes)?;

        env.call_method(
            &self.delegate,
            "startAdvertising",
            "([B)V",
            &[JValue::Object(&byte_array)],
        )?;
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        let env = self.jvm.attach_current_thread()?;

        // Get device address from discovery cache
        let address = self.state.read().await
            .get_address_for_node(peer_id)
            .ok_or(BleError::PeerNotFound)?;

        // Call Kotlin to initiate connection
        let address_str = env.new_string(&address)?;
        env.call_method(
            &self.delegate,
            "connect",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&address_str)],
        )?;

        // Connection result arrives via callback
        // Return placeholder that gets populated on callback
        Ok(Box::new(AndroidBleConnection::pending(*peer_id)))
    }

    // ... other BleAdapter methods delegate similarly
}
```

### 2. JNI Callbacks (Rust → receives from Kotlin)

```rust
// hive-btle/src/platform/android/jni_callbacks.rs

/// Called by Kotlin when a HIVE device is discovered
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_hive_AndroidBleDelegate_onDeviceDiscovered(
    env: JNIEnv,
    _class: JClass,
    adapter_ptr: jlong,
    address: JString,
    name: JString,
    rssi: jint,
    is_hive: jboolean,
    node_id: jlong,
    adv_data: jbyteArray,
) {
    let adapter = unsafe { &*(adapter_ptr as *const AndroidBridgeAdapter) };

    let address: String = env.get_string(address).unwrap().into();
    let name: Option<String> = env.get_string(name).ok().map(|s| s.into());
    let adv_data: Vec<u8> = env.convert_byte_array(adv_data).unwrap_or_default();

    let discovered = DiscoveredDevice {
        address,
        name,
        rssi: rssi as i8,
        is_hive_node: is_hive != 0,
        node_id: if node_id > 0 { Some(NodeId::new(node_id as u32)) } else { None },
        adv_data,
    };

    // Dispatch to Rust mesh logic
    adapter.handle_discovery(discovered);
}

/// Called by Kotlin when GATT data is received
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_hive_AndroidBleDelegate_onSyncDataReceived(
    env: JNIEnv,
    _class: JClass,
    adapter_ptr: jlong,
    device_address: JString,
    data: jbyteArray,
) {
    let adapter = unsafe { &*(adapter_ptr as *const AndroidBridgeAdapter) };

    let address: String = env.get_string(device_address).unwrap().into();
    let data: Vec<u8> = env.convert_byte_array(data).unwrap();

    // Dispatch to Rust mesh logic (decryption, CRDT merge, etc.)
    adapter.handle_sync_data(&address, &data);
}

/// Called by Kotlin when connection state changes
#[no_mangle]
pub extern "system" fn Java_com_revolveteam_hive_AndroidBleDelegate_onConnectionStateChanged(
    env: JNIEnv,
    _class: JClass,
    adapter_ptr: jlong,
    device_address: JString,
    connected: jboolean,
    mtu: jint,
) {
    let adapter = unsafe { &*(adapter_ptr as *const AndroidBridgeAdapter) };

    let address: String = env.get_string(device_address).unwrap().into();

    if connected != 0 {
        adapter.handle_connected(&address, mtu as u16);
    } else {
        adapter.handle_disconnected(&address, DisconnectReason::RemoteRequest);
    }
}
```

### 3. Kotlin Side: AndroidBleDelegate

```kotlin
// hive-btle/android/src/main/java/com/revolveteam/hive/AndroidBleDelegate.kt

class AndroidBleDelegate(
    private val context: Context,
    private val adapterPtr: Long,  // Pointer to Rust AndroidBridgeAdapter
) {
    private val bluetoothManager = context.getSystemService(BluetoothManager::class.java)
    private val bluetoothAdapter = bluetoothManager.adapter
    private var scanner: BluetoothLeScanner? = null
    private var advertiser: BluetoothLeAdvertiser? = null
    private var gattServer: BluetoothGattServer? = null
    private val connections = mutableMapOf<String, BluetoothGatt>()

    // JNI native callbacks (implemented in Rust)
    private external fun onDeviceDiscovered(
        adapterPtr: Long,
        address: String,
        name: String?,
        rssi: Int,
        isHive: Boolean,
        nodeId: Long,
        advData: ByteArray
    )
    private external fun onSyncDataReceived(adapterPtr: Long, deviceAddress: String, data: ByteArray)
    private external fun onConnectionStateChanged(adapterPtr: Long, deviceAddress: String, connected: Boolean, mtu: Int)

    fun initialize(nodeId: Long, meshId: String, powerProfile: String) {
        // Set up GATT server with HIVE service
        gattServer = bluetoothManager.openGattServer(context, gattServerCallback)
        gattServer?.addService(buildHiveService(nodeId))

        scanner = bluetoothAdapter.bluetoothLeScanner
        advertiser = bluetoothAdapter.bluetoothLeAdvertiser
    }

    fun startScan() {
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()

        val filters = listOf(
            ScanFilter.Builder()
                .setServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
                .build()
        )

        scanner?.startScan(filters, settings, scanCallback)
    }

    fun startAdvertising(beaconData: ByteArray) {
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY)
            .setConnectable(true)
            .build()

        val data = AdvertiseData.Builder()
            .addServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
            .addServiceData(ParcelUuid(HIVE_SERVICE_UUID), beaconData)
            .build()

        advertiser?.startAdvertising(settings, data, advertiseCallback)
    }

    private val scanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            val device = result.device
            val record = result.scanRecord

            // Check if HIVE device
            val isHive = record?.serviceUuids?.contains(ParcelUuid(HIVE_SERVICE_UUID)) == true
            val nodeId = parseNodeIdFromAdvertisement(record)

            // Callback to Rust
            onDeviceDiscovered(
                adapterPtr,
                device.address,
                device.name,
                result.rssi,
                isHive,
                nodeId ?: 0L,
                record?.bytes ?: byteArrayOf()
            )
        }
    }

    private val gattServerCallback = object : BluetoothGattServerCallback() {
        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray
        ) {
            if (characteristic.uuid == SYNC_DATA_CHARACTERISTIC_UUID) {
                // Callback to Rust with received data
                onSyncDataReceived(adapterPtr, device.address, value)
            }

            if (responseNeeded) {
                gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
            }
        }
    }

    // Called by Rust to send data to a peer
    fun writeCharacteristic(deviceAddress: String, characteristicUuid: UUID, data: ByteArray): Boolean {
        val gatt = connections[deviceAddress] ?: return false
        val service = gatt.getService(HIVE_SERVICE_UUID) ?: return false
        val characteristic = service.getCharacteristic(characteristicUuid) ?: return false

        characteristic.value = data
        return gatt.writeCharacteristic(characteristic)
    }

    companion object {
        val HIVE_SERVICE_UUID = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
        val SYNC_DATA_CHARACTERISTIC_UUID = UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")
    }
}
```

### 4. hive-ffi Integration

```rust
// hive-ffi/src/lib.rs

#[cfg(all(feature = "bluetooth", target_os = "android"))]
pub fn create_with_ble_transport(
    config: &NodeConfig,
    jni_env: JNIEnv,
    delegate: JObject,
) -> Result<HiveNode> {
    // Create Rust adapter with JNI bridge
    let bridge_adapter = AndroidBridgeAdapter::new(jni_env, delegate, &config.ble_config)?;

    // Wrap in HiveBleTransport
    let ble_transport = HiveBleTransport::new(bridge_adapter);

    // Create TransportManager with both transports
    let mut transport_manager = TransportManager::new(config.transport_config.clone());

    // Register Iroh (if enabled)
    if config.enable_iroh {
        let iroh = IrohMeshTransport::new(...)?;
        transport_manager.register(Arc::new(iroh));
    }

    // Register BLE
    transport_manager.register(Arc::new(ble_transport));

    // Create unified HiveNode
    Ok(HiveNode {
        transport_manager: Arc::new(transport_manager),
        // ...
    })
}
```

### 5. ATAK Plugin Migration

```kotlin
// HivePluginLifecycle.kt - AFTER migration

class HivePluginLifecycle : PluginLifecycle {
    private var hiveNode: Long = 0  // Native handle
    private var bleDelegate: AndroidBleDelegate? = null

    override fun onStart() {
        // Create BLE delegate (Kotlin side)
        bleDelegate = AndroidBleDelegate(context, adapterPtr = 0)  // Updated after node creation

        // Create unified node with BLE transport
        hiveNode = HiveJni.createWithBleTransport(
            NodeConfig(
                appId = formationId,
                sharedKey = formationKey,
                enableIroh = true,
                enableBle = true,
                bleMeshId = "WEARTAK",
                blePowerProfile = "balanced",
            ),
            bleDelegate!!
        )

        // Update delegate with native pointer
        bleDelegate?.setAdapterPtr(HiveJni.getBleAdapterPtr(hiveNode))

        // Single callback registration - data flows through TransportManager
        HiveJni.setEventCallback(hiveNode) { event ->
            when (event) {
                is HiveEvent.PeerDiscovered -> updatePeerList(event.peer)
                is HiveEvent.DocumentSynced -> refreshMap()
                is HiveEvent.EmergencyReceived -> showAlert(event.fromNode)
            }
        }
    }

    override fun onStop() {
        HiveJni.destroy(hiveNode)
        bleDelegate = null
    }
}
```

---

## Migration Plan

### Phase 1: AndroidBridgeAdapter (Week 1-2)

1. Create `hive-btle/src/platform/android/bridge_adapter.rs`
2. Implement `BleAdapter` trait with JNI delegation
3. Create JNI callback functions
4. Unit tests with mock JNI

**Deliverables**:
- [ ] `AndroidBridgeAdapter` struct
- [ ] JNI callback implementations
- [ ] Compile for Android targets

### Phase 2: Kotlin AndroidBleDelegate (Week 2-3)

1. Create `AndroidBleDelegate.kt` in hive-btle Android module
2. Implement scanning, advertising, GATT server/client
3. Wire up native callbacks
4. Test on Android device

**Deliverables**:
- [ ] `AndroidBleDelegate` class
- [ ] HIVE GATT service implementation
- [ ] Device discovery working
- [ ] Data exchange working

### Phase 3: TransportManager Integration (Week 3-4)

1. Add `bluetooth` feature to hive-ffi
2. Create `create_with_ble_transport` FFI function
3. Register `HiveBleTransport<AndroidBridgeAdapter>` with `TransportManager`
4. Expose via UniFFI/JNI

**Deliverables**:
- [ ] hive-ffi `bluetooth` feature
- [ ] FFI function for BLE transport creation
- [ ] TransportManager integration

### Phase 4: ATAK Plugin Migration (Week 4-5)

1. Update `HivePluginLifecycle` to use unified transport
2. Remove direct `HiveBleManager` usage
3. Test WearTAK interoperability
4. Deprecation warnings for old API

**Deliverables**:
- [ ] Updated ATAK plugin
- [ ] WearTAK sync verified
- [ ] HiveBleManager deprecated

### Phase 5: Cleanup (Week 5-6)

1. Remove deprecated `HiveBleManager`
2. Update documentation
3. Performance optimization
4. Battery consumption testing

**Deliverables**:
- [ ] Clean codebase
- [ ] Updated ADR-039 with implementation notes
- [ ] Battery benchmark results

---

## Alternatives Considered

### Option A: Keep Dual System
Keep `HiveBleManager` separate from `HiveNodeJni`.

**Rejected**: Data doesn't flow between transports; duplicated state management.

### Option B: Pure Rust Android Adapter
Implement Android BLE entirely in Rust via JNI.

**Rejected**:
- High JNI callback latency
- Complex Android lifecycle handling from Rust
- Permission management requires Kotlin/Java
- Doze mode handling difficult

### Option C: Hybrid (Selected)
Kotlin handles BLE radio, Rust handles mesh protocol.

**Selected**:
- Best of both worlds
- Kotlin excels at Android lifecycle
- Rust excels at protocol logic
- Clean separation of concerns

---

## Security Considerations

1. **JNI Pointer Safety**: `adapterPtr` must be validated before dereferencing
2. **Callback Reentrancy**: JNI callbacks may occur on different threads
3. **Data Copying**: Byte arrays cross JNI boundary (copy overhead acceptable)
4. **Permission Enforcement**: Kotlin side enforces Android BLE permissions

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Single transport API | Unified `HiveNode` | Code review |
| WearTAK interop | Unchanged behavior | Field test |
| ATAK plugin simplification | Remove HiveBleManager | Line count |
| Battery impact | <5% regression | Benchmark |
| Callback latency | <10ms | Profiling |

---

## References

1. ADR-039: HIVE-BTLE Mesh Transport
2. ADR-032: Pluggable Transport Abstraction
3. ADR-041: Multi-Transport Embedded Integration
4. [Android BLE Best Practices](https://developer.android.com/guide/topics/connectivity/bluetooth/ble-overview)
5. [JNI Tips](https://developer.android.com/training/articles/perf-jni)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-01-28 | Select Option C (Hybrid) | Kotlin excels at Android lifecycle; Rust excels at protocol |
| 2025-01-28 | Use JNI callbacks not UniFFI | Lower latency, more control over threading |
| 2025-01-28 | Keep AndroidBleDelegate in hive-btle | Single source of truth for BLE protocol |
