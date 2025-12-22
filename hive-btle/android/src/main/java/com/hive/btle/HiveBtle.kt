package com.hive.btle

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.BluetoothLeAdvertiser
import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.ParcelUuid
import android.util.Log
import androidx.core.content.ContextCompat
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import android.os.Handler
import android.os.Looper

/**
 * Main entry point for HIVE BLE operations on Android.
 *
 * This class provides a high-level API for BLE scanning, advertising, and
 * GATT connections, bridging Android's Bluetooth APIs with the native
 * hive-btle Rust implementation.
 *
 * ## Permissions
 *
 * Required permissions depend on Android version:
 * - Android 12+ (API 31): BLUETOOTH_SCAN, BLUETOOTH_CONNECT, BLUETOOTH_ADVERTISE
 * - Android 6-11: BLUETOOTH, BLUETOOTH_ADMIN, ACCESS_FINE_LOCATION
 *
 * ## Usage
 *
 * ```kotlin
 * // Initialize
 * val hiveBtle = HiveBtle(context, nodeId = 0x12345678)
 * hiveBtle.init()
 *
 * // Start scanning for HIVE nodes
 * hiveBtle.startScan { device ->
 *     Log.d("HIVE", "Found: ${device.address}")
 * }
 *
 * // Connect to a device
 * val connection = hiveBtle.connect(deviceAddress)
 *
 * // Start advertising
 * hiveBtle.startAdvertising()
 * ```
 *
 * @param context Android context (Activity, Service, or Application)
 * @param nodeId This node's HIVE ID (32-bit unsigned). If null, auto-generated from Bluetooth MAC address.
 * @param meshId Mesh identifier for mesh isolation (e.g., "DEMO", "ALFA"). Defaults to "DEMO".
 */
class HiveBtle(
    private val context: Context,
    private var _nodeId: Long? = null,
    private val meshId: String = DEFAULT_MESH_ID
) {
    /**
     * This node's HIVE ID. Auto-generated from Bluetooth MAC address if not specified.
     * Available after init() is called.
     */
    val nodeId: Long
        get() = _nodeId ?: 0L

    /**
     * Get the mesh ID this node belongs to.
     */
    fun getMeshId(): String = meshId
    companion object {
        private const val TAG = "HiveBtle"

        /**
         * HIVE BLE Service UUID (canonical: f47ac10b-58cc-4372-a567-0e02b2c3d479)
         *
         * This is the canonical HIVE service UUID used across all platforms.
         */
        val HIVE_SERVICE_UUID: UUID = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")

        /**
         * HIVE BLE Service UUID - 16-bit alias (0xF47A) for space-constrained advertising
         *
         * Used by ESP32/Core2 devices to fit service UUID in BLE advertising payload.
         * Expands to standard Bluetooth SIG base: 0000f47a-0000-1000-8000-00805f9b34fb
         */
        val HIVE_SERVICE_UUID_16: UUID = UUID.fromString("0000f47a-0000-1000-8000-00805f9b34fb")

        /**
         * HIVE Document Characteristic UUID (canonical: f47a0003-58cc-4372-a567-0e02b2c3d479)
         *
         * Used for exchanging CRDT document data between peers.
         * Supports read, write, and notify operations.
         * Maps to CHAR_SYNC_DATA in the canonical protocol.
         */
        val HIVE_CHAR_DOCUMENT: UUID = UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")

        /** HIVE Node Info Characteristic UUID (canonical) */
        val HIVE_CHAR_NODE_INFO: UUID = UUID.fromString("f47a0001-58cc-4372-a567-0e02b2c3d479")

        /** HIVE Sync State Characteristic UUID (canonical) */
        val HIVE_CHAR_SYNC_STATE: UUID = UUID.fromString("f47a0002-58cc-4372-a567-0e02b2c3d479")

        /** HIVE Sync Data Characteristic UUID (canonical) - same as HIVE_CHAR_DOCUMENT */
        val HIVE_CHAR_SYNC_DATA: UUID = UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")

        /** HIVE Command Characteristic UUID (canonical) */
        val HIVE_CHAR_COMMAND: UUID = UUID.fromString("f47a0004-58cc-4372-a567-0e02b2c3d479")

        /** HIVE Status Characteristic UUID (canonical) */
        val HIVE_CHAR_STATUS: UUID = UUID.fromString("f47a0005-58cc-4372-a567-0e02b2c3d479")

        /** Client Characteristic Configuration Descriptor UUID */
        val CCCD_UUID: UUID = UUID.fromString("00002902-0000-1000-8000-00805F9B34FB")

        /** HIVE device name prefix (legacy format) */
        const val HIVE_NAME_PREFIX = "HIVE-"

        /** HIVE device name prefix with mesh ID (new format) */
        const val HIVE_MESH_PREFIX = "HIVE_"

        /** Default mesh ID for demos and testing */
        const val DEFAULT_MESH_ID = "DEMO"

        /**
         * Derive a short mesh ID from an app ID string.
         *
         * The mesh ID is used in BLE device names and should be short (4-8 chars).
         * This function takes a potentially long app_id (e.g., "default-atak-formation")
         * and derives a short mesh ID from it.
         *
         * Strategy:
         * - If app_id is 8 chars or less, use it directly (uppercased)
         * - Otherwise, use first 4 chars of a hash (uppercased hex)
         *
         * @param appId The application/formation ID (e.g., from HIVE_APP_ID env var)
         * @return A short mesh ID suitable for BLE device names
         */
        @JvmStatic
        fun deriveMeshId(appId: String): String {
            val normalized = appId.trim().uppercase()
            return if (normalized.length <= 8) {
                normalized.ifEmpty { DEFAULT_MESH_ID }
            } else {
                // Use first 4 hex chars of hash for longer app IDs
                String.format("%04X", appId.hashCode() and 0xFFFF)
            }
        }

        /**
         * Get the mesh ID from environment or system properties.
         *
         * Checks in order:
         * 1. System property "hive.mesh_id"
         * 2. System property "hive.app_id" (derives mesh ID from it)
         * 3. Environment variable "HIVE_MESH_ID"
         * 4. Environment variable "HIVE_APP_ID" (derives mesh ID from it)
         * 5. Falls back to DEFAULT_MESH_ID ("DEMO")
         *
         * @return The mesh ID to use for this node
         */
        @JvmStatic
        fun getMeshIdFromEnvironment(): String {
            // Direct mesh ID takes priority
            System.getProperty("hive.mesh_id")?.let { return it }
            System.getenv("HIVE_MESH_ID")?.let { return it }

            // Derive from app ID if available
            System.getProperty("hive.app_id")?.let { return deriveMeshId(it) }
            System.getenv("HIVE_APP_ID")?.let { return deriveMeshId(it) }

            return DEFAULT_MESH_ID
        }

        /**
         * Generate a device name in the new mesh format: HIVE_<MESH_ID>-<NODE_ID>
         *
         * @param meshId Mesh identifier (e.g., "DEMO", "ALFA")
         * @param nodeId Node ID as 32-bit unsigned long
         * @return Device name string (e.g., "HIVE_DEMO-12345678")
         */
        @JvmStatic
        fun generateDeviceName(meshId: String, nodeId: Long): String {
            return "HIVE_${meshId}-${String.format("%08X", nodeId)}"
        }

        /**
         * Parse mesh ID and node ID from a device name.
         *
         * Supports both formats:
         * - New: HIVE_<MESH_ID>-<NODE_ID> (e.g., "HIVE_DEMO-12345678")
         * - Legacy: HIVE-<NODE_ID> (e.g., "HIVE-12345678") - returns null meshId
         *
         * @param name Device name to parse
         * @return Pair of (meshId, nodeId) or null if parsing fails
         */
        @JvmStatic
        fun parseDeviceName(name: String): Pair<String?, Long>? {
            return when {
                name.startsWith(HIVE_MESH_PREFIX) -> {
                    // New format: HIVE_MESHID-NODEID
                    val rest = name.removePrefix(HIVE_MESH_PREFIX)
                    val dashIndex = rest.indexOf('-')
                    if (dashIndex <= 0) return null
                    val meshId = rest.substring(0, dashIndex)
                    val nodeIdStr = rest.substring(dashIndex + 1)
                    try {
                        val nodeId = nodeIdStr.toLong(16)
                        Pair(meshId, nodeId)
                    } catch (e: NumberFormatException) {
                        null
                    }
                }
                name.startsWith(HIVE_NAME_PREFIX) -> {
                    // Legacy format: HIVE-NODEID (no mesh ID)
                    val nodeIdStr = name.removePrefix(HIVE_NAME_PREFIX)
                    try {
                        val nodeId = nodeIdStr.toLong(16)
                        Pair(null, nodeId)
                    } catch (e: NumberFormatException) {
                        null
                    }
                }
                else -> null
            }
        }

        /**
         * Check if a device matches our mesh.
         *
         * Returns true if:
         * - The device has the same mesh ID, OR
         * - The device has no mesh ID (legacy format - backwards compatible)
         *
         * @param ourMeshId Our mesh ID
         * @param deviceMeshId Device's mesh ID (null for legacy format)
         * @return true if the device matches our mesh
         */
        @JvmStatic
        fun matchesMesh(ourMeshId: String, deviceMeshId: String?): Boolean {
            return deviceMeshId == null || deviceMeshId == ourMeshId
        }

        /**
         * Derive a NodeId from a BLE MAC address using the native Rust implementation.
         * This ensures consistency across all platforms (Android, iOS, ESP32).
         *
         * @param macAddress MAC address in "AA:BB:CC:DD:EE:FF" format
         * @return NodeId derived from last 4 bytes of MAC, or 0 if parsing fails
         */
        @JvmStatic
        external fun nativeDeriveNodeId(macAddress: String): Long

        init {
            try {
                System.loadLibrary("hive_btle")
                Log.i(TAG, "Loaded hive_btle native library")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }
    }

    // Android Bluetooth components
    private var bluetoothManager: BluetoothManager? = null
    private var bluetoothAdapter: BluetoothAdapter? = null
    private var leScanner: BluetoothLeScanner? = null
    private var leAdvertiser: BluetoothLeAdvertiser? = null

    // Callbacks
    private var scanCallback: ScanCallbackProxy? = null
    private var advertiseCallback: AdvertiseCallbackProxy? = null

    // Active GATT connections (as Central - connecting to others)
    private val connections = ConcurrentHashMap<String, BluetoothGatt>()
    private val gattCallbacks = ConcurrentHashMap<String, GattCallbackProxy>()
    private val connectionIdCounter = AtomicLong(0)

    // GATT Server (as Peripheral - others connect to us)
    private var gattServer: BluetoothGattServer? = null
    private var gattServerCallback: GattServerCallback? = null
    private val connectedCentrals = ConcurrentHashMap<String, BluetoothDevice>() // address -> device
    private var syncDataCharacteristic: BluetoothGattCharacteristic? = null

    // State
    private var isInitialized = false
    private var isScanning = false
    private var isAdvertising = false
    private var isMeshRunning = false

    // Native handle
    private var nativeHandle: Long = 0

    // Mesh management
    private val peers = ConcurrentHashMap<Long, HivePeer>() // nodeId -> peer
    private val addressToNodeId = ConcurrentHashMap<String, Long>() // address -> nodeId
    private var meshListener: HiveMeshListener? = null
    private val handler = Handler(Looper.getMainLooper())
    private var localDocument: HiveDocument? = null
    private var localCounter = mutableListOf<GCounterEntry>()

    // Mesh configuration
    private val PEER_TIMEOUT_MS = 30000L // Remove peers after 30s without advertisement
    private val CLEANUP_INTERVAL_MS = 10000L // Cleanup check interval
    private val SYNC_INTERVAL_MS = 3000L // Sync documents every 3s

    private val cleanupRunnable = object : Runnable {
        override fun run() {
            cleanupStalePeers()
            if (isMeshRunning) {
                handler.postDelayed(this, CLEANUP_INTERVAL_MS)
            }
        }
    }

    private val syncRunnable = object : Runnable {
        override fun run() {
            syncWithPeers()
            if (isMeshRunning) {
                handler.postDelayed(this, SYNC_INTERVAL_MS)
            }
        }
    }

    /**
     * Initialize the HIVE BLE adapter.
     *
     * Must be called before any other operations. Checks for Bluetooth
     * availability and required permissions.
     *
     * @throws IllegalStateException if Bluetooth is not available
     * @throws SecurityException if required permissions are not granted
     */
    fun init() {
        if (isInitialized) {
            Log.w(TAG, "Already initialized")
            return
        }

        // Get Bluetooth manager
        bluetoothManager = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            ?: throw IllegalStateException("Bluetooth not available on this device")

        // Get adapter
        bluetoothAdapter = bluetoothManager?.adapter
            ?: throw IllegalStateException("Bluetooth adapter not available")

        // Check if enabled
        if (bluetoothAdapter?.isEnabled != true) {
            throw IllegalStateException("Bluetooth is not enabled")
        }

        // Get LE scanner
        leScanner = bluetoothAdapter?.bluetoothLeScanner

        // Get LE advertiser (may be null if not supported)
        leAdvertiser = bluetoothAdapter?.bluetoothLeAdvertiser

        // Auto-generate nodeId from adapter address if not provided
        if (_nodeId == null) {
            _nodeId = generateNodeIdFromAdapter()
            Log.i(TAG, "Auto-generated nodeId from adapter: ${String.format("%08X", nodeId)}")
        }

        // Initialize native adapter
        nativeHandle = nativeInit(context, nodeId)
        if (nativeHandle == 0L) {
            throw IllegalStateException("Failed to initialize native adapter")
        }

        isInitialized = true
        Log.i(TAG, "Initialized for node ${String.format("%08X", nodeId)}")
    }

    /**
     * Check if Bluetooth permissions are granted.
     *
     * @return true if all required permissions are granted
     */
    fun hasPermissions(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Android 12+
            hasPermission(Manifest.permission.BLUETOOTH_SCAN) &&
            hasPermission(Manifest.permission.BLUETOOTH_CONNECT) &&
            hasPermission(Manifest.permission.BLUETOOTH_ADVERTISE)
        } else {
            // Android 6-11
            hasPermission(Manifest.permission.BLUETOOTH) &&
            hasPermission(Manifest.permission.BLUETOOTH_ADMIN) &&
            hasPermission(Manifest.permission.ACCESS_FINE_LOCATION)
        }
    }

    private fun hasPermission(permission: String): Boolean {
        return ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
    }

    /**
     * Get the list of required permissions for the current Android version.
     *
     * @return Array of permission strings to request
     */
    fun getRequiredPermissions(): Array<String> {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
                Manifest.permission.BLUETOOTH_ADVERTISE
            )
        } else {
            arrayOf(
                Manifest.permission.BLUETOOTH,
                Manifest.permission.BLUETOOTH_ADMIN,
                Manifest.permission.ACCESS_FINE_LOCATION
            )
        }
    }

    /**
     * Start scanning for HIVE BLE devices.
     *
     * Scans for devices advertising the HIVE service UUID or with names
     * matching the HIVE-XXXXXXXX pattern.
     *
     * @param onDeviceFound Callback invoked when a HIVE device is discovered
     */
    fun startScan(onDeviceFound: ((DiscoveredDevice) -> Unit)? = null) {
        checkInitialized()

        if (isScanning) {
            Log.w(TAG, "Already scanning")
            return
        }

        val scanner = leScanner
            ?: throw IllegalStateException("BLE scanner not available")

        // Scan without strict UUID filter - the M5Stack may not advertise the UUID
        // in a way Android's filter recognizes. We filter by name prefix instead.
        // An empty filter list means scan for all devices.
        val filters = emptyList<ScanFilter>()

        // Build scan settings
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
            .setMatchMode(ScanSettings.MATCH_MODE_AGGRESSIVE)
            .setNumOfMatches(ScanSettings.MATCH_NUM_MAX_ADVERTISEMENT)
            .setReportDelay(0)
            .build()

        // Create callback proxy with the onDeviceFound callback
        scanCallback = ScanCallbackProxy(onDeviceFound)

        try {
            scanner.startScan(filters, settings, scanCallback)
            isScanning = true
            Log.i(TAG, "Started scanning for HIVE devices (no UUID filter)")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_SCAN permission", e)
            throw e
        }
    }

    /**
     * Stop scanning for BLE devices.
     */
    fun stopScan() {
        if (!isScanning) {
            return
        }

        try {
            scanCallback?.let { leScanner?.stopScan(it) }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_SCAN permission", e)
        }

        scanCallback = null
        isScanning = false
        Log.i(TAG, "Stopped scanning")
    }

    /**
     * Start advertising as a HIVE node.
     *
     * Advertises the HIVE service UUID with this node's ID in the
     * service data.
     *
     * @param mode Advertising mode (default: balanced)
     * @param txPower TX power level (default: medium)
     */
    fun startAdvertising(
        mode: Int = AdvertiseSettings.ADVERTISE_MODE_BALANCED,
        txPower: Int = AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM
    ) {
        checkInitialized()

        if (isAdvertising) {
            Log.w(TAG, "Already advertising")
            return
        }

        val advertiser = leAdvertiser
            ?: throw IllegalStateException("BLE advertising not supported on this device")

        // Build advertise settings
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(mode)
            .setTxPowerLevel(txPower)
            .setConnectable(true)
            .setTimeout(0) // Advertise indefinitely
            .build()

        // Build advertise data - just service UUID to stay within 31-byte limit
        // Full 128-bit UUID + service data exceeds the limit
        val data = AdvertiseData.Builder()
            .setIncludeDeviceName(false) // Name goes in scan response
            .addServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
            .build()

        // Build scan response with device name (contains mesh ID and node ID)
        val scanResponse = AdvertiseData.Builder()
            .setIncludeDeviceName(true)
            .build()

        // Create callback proxy
        advertiseCallback = AdvertiseCallbackProxy()

        try {
            advertiser.startAdvertising(settings, data, scanResponse, advertiseCallback)
            isAdvertising = true
            Log.i(TAG, "Started advertising as ${generateDeviceName(meshId, nodeId)}")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_ADVERTISE permission", e)
            throw e
        }
    }

    /**
     * Stop advertising.
     */
    fun stopAdvertising() {
        if (!isAdvertising) {
            return
        }

        try {
            advertiseCallback?.let { leAdvertiser?.stopAdvertising(it) }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_ADVERTISE permission", e)
        }

        advertiseCallback = null
        isAdvertising = false
        Log.i(TAG, "Stopped advertising")
    }

    // ==================== GATT Server (Peripheral Mode) ====================

    /**
     * Start the GATT server to accept incoming connections.
     *
     * This allows iOS and other devices to connect to this Android device
     * and read/write the HIVE document characteristic.
     */
    private fun startGattServer() {
        if (gattServer != null) {
            Log.w(TAG, "GATT server already running")
            return
        }

        val manager = bluetoothManager ?: return

        try {
            gattServerCallback = GattServerCallback()
            gattServer = manager.openGattServer(context, gattServerCallback)

            if (gattServer == null) {
                Log.e(TAG, "Failed to open GATT server")
                return
            }

            // Create the HIVE service
            val service = BluetoothGattService(
                HIVE_SERVICE_UUID,
                BluetoothGattService.SERVICE_TYPE_PRIMARY
            )

            // Create the sync data characteristic with read, write, notify properties
            syncDataCharacteristic = BluetoothGattCharacteristic(
                HIVE_CHAR_DOCUMENT,
                BluetoothGattCharacteristic.PROPERTY_READ or
                        BluetoothGattCharacteristic.PROPERTY_WRITE or
                        BluetoothGattCharacteristic.PROPERTY_NOTIFY,
                BluetoothGattCharacteristic.PERMISSION_READ or
                        BluetoothGattCharacteristic.PERMISSION_WRITE
            )

            // Add CCCD for notifications
            val cccd = BluetoothGattDescriptor(
                CCCD_UUID,
                BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE
            )
            syncDataCharacteristic?.addDescriptor(cccd)

            service.addCharacteristic(syncDataCharacteristic)

            // Add the service to the server
            val added = gattServer?.addService(service) ?: false
            if (added) {
                Log.i(TAG, "GATT server started with HIVE service")
            } else {
                Log.e(TAG, "Failed to add HIVE service to GATT server")
            }

        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission for GATT server", e)
        }
    }

    /**
     * Stop the GATT server.
     */
    private fun stopGattServer() {
        try {
            gattServer?.close()
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing permission to close GATT server", e)
        }
        gattServer = null
        gattServerCallback = null
        connectedCentrals.clear()
        Log.i(TAG, "GATT server stopped")
    }

    /**
     * Send a notification to all connected centrals (devices that connected to us).
     */
    private fun notifyConnectedCentrals(data: ByteArray) {
        val server = gattServer ?: return
        val characteristic = syncDataCharacteristic ?: return

        if (connectedCentrals.isEmpty()) {
            Log.d(TAG, "No connected centrals to notify")
            return
        }

        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                for ((address, device) in connectedCentrals) {
                    val result = server.notifyCharacteristicChanged(device, characteristic, false, data)
                    Log.d(TAG, "Notified central $address: result=$result")
                }
            } else {
                @Suppress("DEPRECATION")
                characteristic.value = data
                for ((address, device) in connectedCentrals) {
                    @Suppress("DEPRECATION")
                    val result = server.notifyCharacteristicChanged(device, characteristic, false)
                    Log.d(TAG, "Notified central $address: result=$result")
                }
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing permission to notify centrals", e)
        }
    }

    /**
     * GATT Server callback for handling incoming connections and requests.
     */
    private inner class GattServerCallback : BluetoothGattServerCallback() {

        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            try {
                val address = device.address
                val name = device.name ?: "Unknown"

                when (newState) {
                    BluetoothProfile.STATE_CONNECTED -> {
                        Log.i(TAG, "Central connected: $name ($address)")
                        connectedCentrals[address] = device

                        // Notify mesh listener about new connection
                        handler.post {
                            meshListener?.onMeshUpdated(peers.values.toList())
                        }
                    }
                    BluetoothProfile.STATE_DISCONNECTED -> {
                        Log.i(TAG, "Central disconnected: $name ($address)")
                        connectedCentrals.remove(address)

                        handler.post {
                            meshListener?.onMeshUpdated(peers.values.toList())
                        }
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onConnectionStateChange", e)
            }
        }

        override fun onCharacteristicReadRequest(
            device: BluetoothDevice,
            requestId: Int,
            offset: Int,
            characteristic: BluetoothGattCharacteristic
        ) {
            try {
                val address = device.address
                Log.d(TAG, "Read request from $address for ${characteristic.uuid}")

                if (characteristic.uuid == HIVE_CHAR_DOCUMENT) {
                    // Return current document state
                    val documentBytes = HiveDocument.encode(nodeId, localCounter, null)
                    val response = if (offset > documentBytes.size) {
                        ByteArray(0)
                    } else {
                        documentBytes.copyOfRange(offset, documentBytes.size)
                    }

                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        offset,
                        response
                    )
                    Log.d(TAG, "Sent ${response.size} bytes to $address")
                } else {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED,
                        0,
                        null
                    )
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onCharacteristicReadRequest", e)
            }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            try {
                val address = device.address
                val dataSize = value?.size ?: 0
                Log.i(TAG, "Write request from $address: $dataSize bytes")

                if (characteristic.uuid == HIVE_CHAR_DOCUMENT && value != null) {
                    // Log raw data for debugging
                    val hexData = value.joinToString(" ") { String.format("%02X", it) }
                    Log.d(TAG, "Received data: $hexData")

                    // Parse the document
                    val document = HiveDocument.decode(value)
                    if (document != null) {
                        Log.i(TAG, "Received document from ${String.format("%08X", document.nodeId)}, event=${document.currentEventType()}")

                        // Handle the document - find or create peer
                        val sourceNodeId = document.nodeId
                        if (sourceNodeId != nodeId && sourceNodeId != 0L) {
                            // Find existing peer or create new one
                            var peer = peers.values.find { it.address == address }
                            if (peer == null) {
                                // Check if we know this node by nodeId
                                peer = peers[sourceNodeId]
                            }

                            if (peer == null) {
                                // New peer from incoming connection
                                // Set lastDocument = null so the first event triggers onPeerEvent
                                peer = HivePeer(
                                    nodeId = sourceNodeId,
                                    address = address,
                                    name = generateDeviceName(meshId, sourceNodeId),
                                    meshId = meshId,
                                    rssi = 0,
                                    isConnected = true,
                                    lastDocument = null,
                                    lastSeen = System.currentTimeMillis()
                                )
                                peers[sourceNodeId] = peer
                                addressToNodeId[address] = sourceNodeId
                                Log.i(TAG, "Added peer from GATT write: ${peer.displayName()}")
                            } else {
                                // Update existing peer
                                if (peer.nodeId != sourceNodeId) {
                                    // NodeId changed - update mapping
                                    peers.remove(peer.nodeId)
                                    val updatedPeer = peer.copy(nodeId = sourceNodeId)
                                    peers[sourceNodeId] = updatedPeer
                                    addressToNodeId[address] = sourceNodeId
                                    peer = updatedPeer
                                }
                            }

                            // Handle document content
                            handlePeerDocumentInternal(peer, document)
                        }
                    } else {
                        Log.w(TAG, "Failed to decode document from $address")
                    }

                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_SUCCESS,
                            0,
                            null
                        )
                    }
                } else {
                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED,
                            0,
                            null
                        )
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onCharacteristicWriteRequest", e)
            }
        }

        override fun onDescriptorReadRequest(
            device: BluetoothDevice,
            requestId: Int,
            offset: Int,
            descriptor: BluetoothGattDescriptor
        ) {
            try {
                if (descriptor.uuid == CCCD_UUID) {
                    val value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, value)
                } else {
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED, 0, null)
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onDescriptorReadRequest", e)
            }
        }

        override fun onDescriptorWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            descriptor: BluetoothGattDescriptor,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            try {
                val address = device.address
                if (descriptor.uuid == CCCD_UUID) {
                    // Client is subscribing to notifications
                    val enabled = value?.contentEquals(BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE) == true
                    Log.i(TAG, "Notification ${if (enabled) "enabled" else "disabled"} for $address")

                    if (responseNeeded) {
                        gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                    }
                } else {
                    if (responseNeeded) {
                        gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED, 0, null)
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onDescriptorWriteRequest", e)
            }
        }

        override fun onServiceAdded(status: Int, service: BluetoothGattService) {
            if (status == BluetoothGatt.GATT_SUCCESS) {
                Log.i(TAG, "GATT service added: ${service.uuid}")
            } else {
                Log.e(TAG, "Failed to add GATT service: status=$status")
            }
        }

        override fun onMtuChanged(device: BluetoothDevice, mtu: Int) {
            try {
                Log.d(TAG, "MTU changed to $mtu for ${device.address}")
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onMtuChanged", e)
            }
        }
    }

    /**
     * Connect to a HIVE device by address.
     *
     * @param address Bluetooth device address (MAC)
     * @param autoConnect Use autoConnect mode (reconnect automatically)
     * @return Connection handle, or null if connection failed
     */
    fun connect(address: String, autoConnect: Boolean = false): HiveConnection? {
        checkInitialized()

        if (connections.containsKey(address)) {
            Log.w(TAG, "Already connected to $address")
            return null
        }

        val adapter = bluetoothAdapter
            ?: throw IllegalStateException("Bluetooth adapter not available")

        try {
            val device = adapter.getRemoteDevice(address)
            val connectionId = connectionIdCounter.incrementAndGet()
            val callback = GattCallbackProxy(connectionId)

            val gatt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                device.connectGatt(context, autoConnect, callback, BluetoothDevice.TRANSPORT_LE)
            } else {
                device.connectGatt(context, autoConnect, callback)
            }

            if (gatt != null) {
                connections[address] = gatt
                gattCallbacks[address] = callback
                Log.i(TAG, "Connecting to $address")
                return HiveConnection(address, gatt, callback)
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
            throw e
        } catch (e: IllegalArgumentException) {
            Log.e(TAG, "Invalid address: $address", e)
        }

        return null
    }

    /**
     * Disconnect from a device.
     *
     * @param address Device address to disconnect
     */
    fun disconnect(address: String) {
        val gatt = connections.remove(address)
        gattCallbacks.remove(address)

        try {
            gatt?.disconnect()
            gatt?.close()
            Log.i(TAG, "Disconnected from $address")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
        }
    }

    // ==================== Mesh Management API ====================

    /**
     * Start the HIVE mesh network.
     *
     * This starts scanning, advertising, and automatically manages
     * connections to discovered HIVE peers. The mesh handles document
     * synchronization automatically.
     *
     * @param listener Callback for mesh events (peer updates, events)
     */
    fun startMesh(listener: HiveMeshListener) {
        checkInitialized()

        if (isMeshRunning) {
            Log.w(TAG, "Mesh already running")
            return
        }

        meshListener = listener
        isMeshRunning = true

        // Start GATT server first (so iOS can connect to us)
        try {
            startGattServer()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start GATT server", e)
        }

        // Start advertising
        try {
            startAdvertising()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start advertising", e)
        }

        // Start scanning with internal handler
        try {
            startScan { device -> onDeviceDiscovered(device) }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start scanning", e)
        }

        // Start periodic tasks
        handler.post(cleanupRunnable)
        handler.postDelayed(syncRunnable, SYNC_INTERVAL_MS)

        Log.i(TAG, "Mesh started for HIVE-${String.format("%08X", nodeId)} with GATT server")
    }

    /**
     * Stop the HIVE mesh network.
     */
    fun stopMesh() {
        if (!isMeshRunning) return

        isMeshRunning = false
        handler.removeCallbacks(cleanupRunnable)
        handler.removeCallbacks(syncRunnable)

        stopScan()
        stopAdvertising()
        stopGattServer()

        // Disconnect all peers
        for (address in connections.keys.toList()) {
            disconnect(address)
        }

        peers.clear()
        addressToNodeId.clear()
        meshListener = null

        Log.i(TAG, "Mesh stopped")
    }

    /**
     * Send an event to all peers in the mesh.
     *
     * @param eventType The event to broadcast
     */
    fun sendEvent(eventType: HiveEventType) {
        if (!isMeshRunning) {
            Log.w(TAG, "Mesh not running, cannot send event")
            return
        }

        Log.i(TAG, "Broadcasting event: $eventType to ${connections.size} peripherals and ${connectedCentrals.size} centrals")

        // Increment our counter
        incrementLocalCounter()

        // Create document with event
        val peripheral = HivePeripheral(
            id = nodeId,
            parentNode = 0,
            peripheralType = HivePeripheralType.SOLDIER_SENSOR,
            callsign = "ANDROID",
            health = HiveHealthStatus(100, null, 0, 0),
            lastEvent = HivePeripheralEvent(eventType, System.currentTimeMillis()),
            timestamp = System.currentTimeMillis()
        )

        val documentBytes = HiveDocument.encode(nodeId, localCounter, peripheral)

        // Send to all connected peripherals (devices we connected to as Central)
        for ((address, gatt) in connections) {
            writeDocumentToGatt(gatt, documentBytes)
        }

        // Send to all connected centrals (devices that connected to us as Peripheral)
        notifyConnectedCentrals(documentBytes)
    }

    /**
     * Get the current list of peers in the mesh.
     */
    fun getPeers(): List<HivePeer> = peers.values.toList()

    /**
     * Get a specific peer by node ID.
     */
    fun getPeer(nodeId: Long): HivePeer? = peers[nodeId]

    /**
     * Check if the mesh is running.
     */
    fun isMeshRunning(): Boolean = isMeshRunning

    // ==================== Internal Mesh Methods ====================

    private fun onDeviceDiscovered(device: DiscoveredDevice) {
        if (!device.isHiveDevice) return

        // Check if we already know this address (peer might have been renamed by document)
        val knownNodeId = addressToNodeId[device.address]
        if (knownNodeId != null) {
            // Update existing peer by address
            peers[knownNodeId]?.let { peer ->
                peer.rssi = device.rssi
                peer.lastSeen = System.currentTimeMillis()
                notifyMeshUpdated()
            }
            return
        }

        // Derive nodeId from name, service data, or address for new peers
        val peerNodeId = device.nodeId ?: nativeDeriveNodeId(device.address)

        // Don't track ourselves
        if (peerNodeId == nodeId) return

        // Check mesh ID - only auto-connect to peers in the same mesh or legacy peers
        val sameMesh = matchesMesh(meshId, device.meshId)
        if (!sameMesh) {
            Log.d(TAG, "Skipping peer ${String.format("%08X", peerNodeId)} - different mesh (${device.meshId} != $meshId)")
            return
        }

        val now = System.currentTimeMillis()
        addressToNodeId[device.address] = peerNodeId

        val existingPeer = peers[peerNodeId]
        if (existingPeer != null) {
            // Update existing peer
            existingPeer.rssi = device.rssi
            existingPeer.lastSeen = now
        } else {
            // New peer discovered
            val peer = HivePeer(
                nodeId = peerNodeId,
                address = device.address,
                name = device.name.ifEmpty { generateDeviceName(device.meshId ?: meshId, peerNodeId) },
                meshId = device.meshId,
                rssi = device.rssi,
                isConnected = false,
                lastDocument = null,
                lastSeen = now
            )
            peers[peerNodeId] = peer
            Log.i(TAG, "New peer discovered: ${peer.displayName()} (mesh: ${device.meshId ?: "legacy"})")

            // Auto-connect to new peer
            connectToPeer(peer)
        }

        notifyMeshUpdated()
    }

    private fun connectToPeer(peer: HivePeer) {
        if (connections.containsKey(peer.address)) {
            Log.d(TAG, "Already connected to ${peer.displayName()}")
            return
        }

        Log.i(TAG, "Connecting to peer: ${peer.displayName()}")

        val adapter = bluetoothAdapter ?: return

        try {
            val device = adapter.getRemoteDevice(peer.address)
            val connectionId = connectionIdCounter.incrementAndGet()
            val callback = GattCallbackProxy(connectionId)

            // Set up document listener
            callback.documentListener = object : HiveDocumentListener {
                override fun onDocumentReceived(data: ByteArray) {
                    handlePeerDocument(peer, data)
                }

                override fun onServicesDiscovered() {
                    Log.i(TAG, "Services discovered for ${peer.displayName()}")
                    peer.isConnected = true
                    notifyMeshUpdated()

                    // Enable notifications first, then read after a delay
                    connections[peer.address]?.let { gatt ->
                        enableNotificationsForGatt(gatt)
                        // Delay read to allow descriptor write to complete
                        handler.postDelayed({
                            readDocumentFromGatt(gatt)
                        }, 500)
                    }
                }

                override fun onConnectionStateChanged(connected: Boolean) {
                    Log.i(TAG, "Peer ${peer.displayName()} connected: $connected")
                    // Find the current peer entry (may have been updated with new nodeId)
                    val currentPeer = peers.values.find { it.address == peer.address }
                    if (currentPeer != null) {
                        currentPeer.isConnected = connected
                        currentPeer.lastSeen = System.currentTimeMillis()
                    }
                    if (!connected) {
                        connections.remove(peer.address)
                        gattCallbacks.remove(peer.address)
                        // Retry connection after a delay if mesh is still running
                        if (isMeshRunning && currentPeer != null) {
                            handler.postDelayed({
                                if (isMeshRunning && !connections.containsKey(peer.address)) {
                                    Log.i(TAG, "Retrying connection to ${currentPeer.displayName()}")
                                    connectToPeer(currentPeer)
                                }
                            }, 2000)
                        }
                    }
                    notifyMeshUpdated()
                }
            }

            val gatt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            } else {
                device.connectGatt(context, false, callback)
            }

            if (gatt != null) {
                connections[peer.address] = gatt
                gattCallbacks[peer.address] = callback
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to connect to peer", e)
        }
    }

    private fun handlePeerDocument(peer: HivePeer, data: ByteArray) {
        val document = HiveDocument.decode(data) ?: return
        val docNodeId = document.nodeId

        Log.d(TAG, "Received document from ${peer.displayName()} (docNodeId=${String.format("%08X", docNodeId)}): event=${document.currentEventType()}")

        // Skip if document is from ourselves
        if (docNodeId == nodeId || docNodeId == 0L) return

        // Check if document is from the connected peer or relayed from another node
        val connectedPeer = peers.values.find { it.address == peer.address }

        if (connectedPeer != null && connectedPeer.nodeId == docNodeId) {
            // Document is from the directly connected peer
            handlePeerDocumentInternal(connectedPeer, document)
        } else if (connectedPeer != null && connectedPeer.nodeId != docNodeId) {
            // Document is RELAYED through connectedPeer from a different originating node
            // Find or create peer entry for the originating nodeId
            var originatingPeer = peers[docNodeId]
            if (originatingPeer == null) {
                // Create a virtual peer for the relayed node (we don't have direct connection)
                originatingPeer = HivePeer(
                    nodeId = docNodeId,
                    address = "", // No direct address - relayed via mesh
                    name = generateDeviceName(meshId, docNodeId),
                    meshId = meshId,
                    rssi = 0,
                    isConnected = false, // Not directly connected
                    lastDocument = null,
                    lastSeen = System.currentTimeMillis()
                )
                peers[docNodeId] = originatingPeer
                Log.i(TAG, "Created relayed peer ${originatingPeer.displayName()} (via ${connectedPeer.displayName()})")
            }
            // Process document for the originating peer
            Log.d(TAG, "Processing relayed document from ${originatingPeer.displayName()} via ${connectedPeer.displayName()}")
            handlePeerDocumentInternal(originatingPeer, document)
        } else {
            // Fallback: peer not in our list yet, use document nodeId
            val newPeer = peers[docNodeId] ?: HivePeer(
                nodeId = docNodeId,
                address = peer.address,
                name = peer.name.ifEmpty { generateDeviceName(meshId, docNodeId) },
                meshId = peer.meshId,
                rssi = peer.rssi,
                isConnected = peer.isConnected,
                lastDocument = null,
                lastSeen = System.currentTimeMillis()
            ).also { peers[docNodeId] = it }
            handlePeerDocumentInternal(newPeer, document)
        }
    }

    private fun handlePeerDocumentInternal(peer: HivePeer, document: HiveDocument) {
        // Store last document
        val previousEvent = peer.lastDocument?.peripheral?.lastEvent
        val previousEventType = previousEvent?.eventType ?: HiveEventType.NONE
        val previousEventTimestamp = previousEvent?.timestamp ?: 0L
        peer.lastDocument = document
        peer.lastSeen = System.currentTimeMillis()

        // Merge counters (CRDT merge)
        mergeCounter(document.counter)

        // Check for new events - trigger if event type changed OR same type with newer timestamp
        val currentEvent = document.peripheral?.lastEvent
        val eventType = currentEvent?.eventType ?: HiveEventType.NONE
        val eventTimestamp = currentEvent?.timestamp ?: 0L
        val isNewEvent = eventType != HiveEventType.NONE && (
            eventType != previousEventType ||
            (eventType == previousEventType && eventTimestamp > previousEventTimestamp)
        )
        if (isNewEvent) {
            Log.i(TAG, "New event from ${peer.displayName()}: $eventType (timestamp=$eventTimestamp, prev=$previousEventTimestamp)")
            handler.post {
                meshListener?.onPeerEvent(peer, eventType)
            }
        }

        handler.post {
            meshListener?.onDocumentSynced(document)
        }

        notifyMeshUpdated()
    }

    private fun mergeCounter(remoteCounter: List<GCounterEntry>) {
        for (entry in remoteCounter) {
            val existing = localCounter.find { it.nodeId == entry.nodeId }
            if (existing != null) {
                if (entry.count > existing.count) {
                    val index = localCounter.indexOf(existing)
                    localCounter[index] = entry
                }
            } else {
                localCounter.add(entry)
            }
        }
    }

    private fun incrementLocalCounter() {
        val existing = localCounter.find { it.nodeId == nodeId }
        if (existing != null) {
            val index = localCounter.indexOf(existing)
            localCounter[index] = GCounterEntry(nodeId, existing.count + 1)
        } else {
            localCounter.add(GCounterEntry(nodeId, 1))
        }
    }

    private fun syncWithPeers() {
        if (connections.isEmpty() && connectedCentrals.isEmpty()) return

        // Create sync document (no event, just counter state - don't increment on sync)
        val documentBytes = HiveDocument.encode(nodeId, localCounter, null)

        // Send to peripherals we connected to
        for ((address, gatt) in connections) {
            writeDocumentToGatt(gatt, documentBytes)
        }

        // Send to centrals that connected to us
        notifyConnectedCentrals(documentBytes)
    }

    private fun cleanupStalePeers() {
        val now = System.currentTimeMillis()
        val staleNodeIds = peers.filter { (_, peer) ->
            now - peer.lastSeen > PEER_TIMEOUT_MS && !peer.isConnected
        }.keys

        if (staleNodeIds.isNotEmpty()) {
            Log.d(TAG, "Removing ${staleNodeIds.size} stale peers")
            for (nodeId in staleNodeIds) {
                val peer = peers.remove(nodeId)
                peer?.let {
                    addressToNodeId.remove(it.address)
                    disconnect(it.address)
                }
            }
            notifyMeshUpdated()
        }
    }

    private fun notifyMeshUpdated() {
        handler.post {
            meshListener?.onMeshUpdated(peers.values.toList())
        }
    }

    /**
     * Generate nodeId from the local Bluetooth adapter's address.
     * Falls back to a persistent random ID if adapter address is unavailable (Android 12+ restrictions).
     * The nodeId is persisted to SharedPreferences to remain consistent across app restarts.
     */
    @Suppress("MissingPermission")
    private fun generateNodeIdFromAdapter(): Long {
        val prefs = context.getSharedPreferences("hive_btle", Context.MODE_PRIVATE)
        val savedNodeId = prefs.getLong("node_id", 0L)

        // Return saved nodeId if we have one
        if (savedNodeId != 0L) {
            Log.i(TAG, "Using persisted nodeId: ${String.format("%08X", savedNodeId)}")
            return savedNodeId
        }

        // Try to get from adapter address first
        val nodeId = try {
            val address = bluetoothAdapter?.address
            if (address != null && address != "02:00:00:00:00:00") {
                // Use native Rust implementation for consistency across platforms
                val derived = nativeDeriveNodeId(address)
                if (derived != 0L) {
                    derived
                } else {
                    deriveNodeIdFromAddressFallback(address)
                }
            } else {
                // Generate random nodeId from UUID (similar to iOS approach)
                val uuid = java.util.UUID.randomUUID()
                val bytes = java.nio.ByteBuffer.allocate(16)
                    .putLong(uuid.mostSignificantBits)
                    .putLong(uuid.leastSignificantBits)
                    .array()
                // Use last 4 bytes like iOS does
                ((bytes[12].toLong() and 0xFF) shl 24) or
                    ((bytes[13].toLong() and 0xFF) shl 16) or
                    ((bytes[14].toLong() and 0xFF) shl 8) or
                    (bytes[15].toLong() and 0xFF)
            }
        } catch (e: SecurityException) {
            // Generate random nodeId
            val uuid = java.util.UUID.randomUUID()
            (uuid.leastSignificantBits and 0xFFFFFFFFL)
        }

        // Persist the nodeId
        prefs.edit().putLong("node_id", nodeId).apply()
        Log.i(TAG, "Generated and persisted new nodeId: ${String.format("%08X", nodeId)}")

        return nodeId
    }

    /**
     * Derive a nodeId from a BLE MAC address (fallback if native call fails).
     * Uses the last 4 bytes of the MAC as a 32-bit node ID.
     */
    private fun deriveNodeIdFromAddressFallback(address: String): Long {
        // MAC format: "AA:BB:CC:DD:EE:FF"
        val parts = address.split(":")
        if (parts.size != 6) return 0L

        return try {
            // Use last 4 bytes of MAC as node ID
            val b2 = parts[2].toLong(16)
            val b3 = parts[3].toLong(16)
            val b4 = parts[4].toLong(16)
            val b5 = parts[5].toLong(16)
            (b2 shl 24) or (b3 shl 16) or (b4 shl 8) or b5
        } catch (e: NumberFormatException) {
            0L
        }
    }

    private fun writeDocumentToGatt(gatt: BluetoothGatt, data: ByteArray) {
        try {
            val service = gatt.getService(HIVE_SERVICE_UUID) ?: return
            val char = service.getCharacteristic(HIVE_CHAR_DOCUMENT) ?: return

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                gatt.writeCharacteristic(char, data, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT)
            } else {
                @Suppress("DEPRECATION")
                char.value = data
                @Suppress("DEPRECATION")
                gatt.writeCharacteristic(char)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to write document", e)
        }
    }

    private fun readDocumentFromGatt(gatt: BluetoothGatt) {
        try {
            val service = gatt.getService(HIVE_SERVICE_UUID) ?: return
            val char = service.getCharacteristic(HIVE_CHAR_DOCUMENT) ?: return
            gatt.readCharacteristic(char)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to read document", e)
        }
    }

    private fun enableNotificationsForGatt(gatt: BluetoothGatt) {
        try {
            val service = gatt.getService(HIVE_SERVICE_UUID) ?: return
            val char = service.getCharacteristic(HIVE_CHAR_DOCUMENT) ?: return

            gatt.setCharacteristicNotification(char, true)

            val descriptor = char.getDescriptor(CCCD_UUID)
            if (descriptor != null) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    gatt.writeDescriptor(descriptor, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
                } else {
                    @Suppress("DEPRECATION")
                    descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    @Suppress("DEPRECATION")
                    gatt.writeDescriptor(descriptor)
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to enable notifications", e)
        }
    }

    /**
     * Disconnect all devices and clean up resources.
     */
    fun shutdown() {
        stopMesh()
        stopScan()
        stopAdvertising()

        // Disconnect all
        for (address in connections.keys.toList()) {
            disconnect(address)
        }

        // Clean up native resources
        if (nativeHandle != 0L) {
            nativeShutdown(nativeHandle)
            nativeHandle = 0
        }

        isInitialized = false
        Log.i(TAG, "Shutdown complete")
    }

    /**
     * Check if scanning is currently active.
     */
    fun isScanning(): Boolean = isScanning

    /**
     * Check if advertising is currently active.
     */
    fun isAdvertising(): Boolean = isAdvertising

    /**
     * Get the number of active connections.
     */
    fun connectionCount(): Int = connections.size

    /**
     * Get list of connected device addresses (devices we connected to as Central).
     */
    fun connectedDevices(): List<String> = connections.keys.toList()

    /**
     * Get the number of connected centrals (devices that connected to us as Peripheral).
     */
    fun connectedCentralsCount(): Int = connectedCentrals.size

    /**
     * Check if GATT server is running.
     */
    fun isGattServerRunning(): Boolean = gattServer != null

    private fun checkInitialized() {
        if (!isInitialized) {
            throw IllegalStateException("HiveBtle not initialized. Call init() first.")
        }
    }

    // Native methods

    private external fun nativeInit(context: Context, nodeId: Long): Long
    private external fun nativeShutdown(handle: Long)
}

/**
 * Represents a discovered HIVE BLE device.
 */
data class DiscoveredDevice(
    val address: String,
    val name: String,
    val rssi: Int,
    val nodeId: Long?,
    val meshId: String?,
    val timestampNanos: Long,
    val isHiveDevice: Boolean = false
)

/**
 * Represents a peer in the HIVE mesh network.
 */
data class HivePeer(
    val nodeId: Long,
    val address: String,
    val name: String,
    val meshId: String?,
    var rssi: Int,
    var isConnected: Boolean,
    var lastDocument: HiveDocument?,
    var lastSeen: Long
) {
    /**
     * Get the display name for this peer.
     * Uses new format (HIVE_MESHID-NODEID) if mesh ID is available,
     * otherwise falls back to legacy format (HIVE-NODEID).
     */
    fun displayName(): String {
        return if (meshId != null) {
            "HIVE_${meshId}-${String.format("%08X", nodeId)}"
        } else {
            "HIVE-${String.format("%08X", nodeId)}"
        }
    }

    /**
     * Get the current event type from this peer's last document.
     */
    fun currentEventType(): HiveEventType = lastDocument?.currentEventType() ?: HiveEventType.NONE
}

/**
 * Listener interface for HIVE mesh events.
 */
interface HiveMeshListener {
    /**
     * Called when the mesh state changes (peers added/removed/updated).
     * @param peers Current list of all known peers
     */
    fun onMeshUpdated(peers: List<HivePeer>)

    /**
     * Called when a peer sends an event (Emergency, ACK, etc.).
     * @param peer The peer that sent the event
     * @param eventType The event type
     */
    fun onPeerEvent(peer: HivePeer, eventType: HiveEventType)

    /**
     * Called when mesh document is synced.
     * @param document The merged document state
     */
    fun onDocumentSynced(document: HiveDocument) {}
}

/**
 * Represents an active GATT connection to a HIVE device.
 */
class HiveConnection internal constructor(
    val address: String,
    private val gatt: BluetoothGatt,
    private val callback: GattCallbackProxy
) {
    /**
     * Set a listener for document events.
     */
    fun setDocumentListener(listener: HiveDocumentListener?) {
        callback.documentListener = listener
    }

    /**
     * Request MTU change.
     *
     * @param mtu Desired MTU size (max 517 for BLE 5.0)
     * @return true if request was initiated
     */
    fun requestMtu(mtu: Int): Boolean {
        return try {
            gatt.requestMtu(mtu)
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Discover GATT services.
     *
     * @return true if discovery was initiated
     */
    fun discoverServices(): Boolean {
        return try {
            gatt.discoverServices()
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Read RSSI for this connection.
     *
     * @return true if read was initiated
     */
    fun readRssi(): Boolean {
        return try {
            gatt.readRemoteRssi()
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Read the HIVE document characteristic.
     *
     * @return true if read was initiated
     */
    fun readDocument(): Boolean {
        return try {
            val service = gatt.getService(HiveBtle.HIVE_SERVICE_UUID)
            if (service == null) {
                Log.e("HiveConnection", "HIVE service not found")
                return false
            }
            val char = service.getCharacteristic(HiveBtle.HIVE_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("HiveConnection", "HIVE document characteristic not found")
                return false
            }
            gatt.readCharacteristic(char)
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Write data to the HIVE document characteristic.
     *
     * @param data The document data to write
     * @return true if write was initiated
     */
    fun writeDocument(data: ByteArray): Boolean {
        return try {
            val service = gatt.getService(HiveBtle.HIVE_SERVICE_UUID)
            if (service == null) {
                Log.e("HiveConnection", "HIVE service not found")
                return false
            }
            val char = service.getCharacteristic(HiveBtle.HIVE_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("HiveConnection", "HIVE document characteristic not found")
                return false
            }
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                gatt.writeCharacteristic(char, data, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT)
            } else {
                @Suppress("DEPRECATION")
                char.value = data
                @Suppress("DEPRECATION")
                gatt.writeCharacteristic(char)
            }
            true
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Enable notifications for the HIVE document characteristic.
     *
     * @return true if notification was enabled
     */
    fun enableDocumentNotifications(): Boolean {
        return try {
            val service = gatt.getService(HiveBtle.HIVE_SERVICE_UUID)
            if (service == null) {
                Log.e("HiveConnection", "HIVE service not found")
                return false
            }
            val char = service.getCharacteristic(HiveBtle.HIVE_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("HiveConnection", "HIVE document characteristic not found")
                return false
            }

            // Enable local notifications
            if (!gatt.setCharacteristicNotification(char, true)) {
                Log.e("HiveConnection", "Failed to enable local notifications")
                return false
            }

            // Write to CCCD to enable remote notifications
            val descriptor = char.getDescriptor(HiveBtle.CCCD_UUID)
            if (descriptor == null) {
                Log.w("HiveConnection", "CCCD descriptor not found, notifications may not work")
                return true  // Local notifications are enabled at least
            }

            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                gatt.writeDescriptor(descriptor, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
            } else {
                @Suppress("DEPRECATION")
                descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                @Suppress("DEPRECATION")
                gatt.writeDescriptor(descriptor)
            }
            true
        } catch (e: SecurityException) {
            Log.e("HiveConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }
}

/**
 * HIVE document event types.
 * Values must match the Rust EventType enum in hive-btle/src/sync/crdt.rs
 */
enum class HiveEventType(val value: Int) {
    NONE(0),
    PING(1),
    NEED_ASSIST(2),
    EMERGENCY(3),
    MOVING(4),
    IN_POSITION(5),
    ACK(6);

    companion object {
        fun fromValue(v: Int): HiveEventType = entries.find { it.value == v } ?: NONE
    }
}

/**
 * HIVE Peripheral type.
 */
enum class HivePeripheralType(val value: Int) {
    UNKNOWN(0),
    SOLDIER_SENSOR(1),
    VEHICLE(2),
    ASSET_TAG(3);

    companion object {
        fun fromValue(v: Int): HivePeripheralType = entries.find { it.value == v } ?: UNKNOWN
    }
}

/**
 * HIVE health status data.
 */
data class HiveHealthStatus(
    val batteryPercent: Int,
    val heartRate: Int?,
    val activityLevel: Int,
    val alerts: Int
) {
    companion object {
        const val ALERT_MAN_DOWN = 0x01
        const val ALERT_LOW_BATTERY = 0x02
        const val ALERT_GEOFENCE = 0x04
        const val ALERT_PANIC = 0x08

        fun decode(data: ByteArray, offset: Int): HiveHealthStatus? {
            if (data.size < offset + 4) return null
            val battery = data[offset].toInt() and 0xFF
            val hr = data[offset + 1].toInt() and 0xFF
            val activity = data[offset + 2].toInt() and 0xFF
            val alerts = data[offset + 3].toInt() and 0xFF
            return HiveHealthStatus(
                batteryPercent = battery,
                heartRate = if (hr > 0) hr else null,
                activityLevel = activity,
                alerts = alerts
            )
        }

        fun encode(status: HiveHealthStatus): ByteArray {
            return byteArrayOf(
                status.batteryPercent.toByte(),
                (status.heartRate ?: 0).toByte(),
                status.activityLevel.toByte(),
                status.alerts.toByte()
            )
        }
    }

    fun hasAlert(flag: Int): Boolean = (alerts and flag) != 0
}

/**
 * HIVE peripheral event.
 */
data class HivePeripheralEvent(
    val eventType: HiveEventType,
    val timestamp: Long
) {
    companion object {
        private const val SIZE = 9

        fun decode(data: ByteArray, offset: Int): HivePeripheralEvent? {
            if (data.size < offset + SIZE) return null
            val eventType = HiveEventType.fromValue(data[offset].toInt() and 0xFF)
            val timestamp = readU64LE(data, offset + 1)
            return HivePeripheralEvent(eventType, timestamp)
        }

        fun encode(event: HivePeripheralEvent): ByteArray {
            val buf = ByteArray(SIZE)
            buf[0] = event.eventType.value.toByte()
            writeU64LE(buf, 1, event.timestamp)
            return buf
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }
}

/**
 * HIVE Peripheral data structure.
 * Format: [id:4][parent:4][type:1][callsign:12][health:4][has_event:1][event:9?][timestamp:8]
 */
data class HivePeripheral(
    val id: Long,
    val parentNode: Long,
    val peripheralType: HivePeripheralType,
    val callsign: String,
    val health: HiveHealthStatus,
    val lastEvent: HivePeripheralEvent?,
    val timestamp: Long
) {
    companion object {
        private const val TAG = "HivePeripheral"
        private const val MIN_SIZE = 34  // Without event
        private const val SIZE_WITH_EVENT = 43

        fun decode(data: ByteArray, offset: Int = 0): HivePeripheral? {
            if (data.size < offset + MIN_SIZE) {
                Log.e(TAG, "Peripheral data too short: ${data.size - offset} bytes (need $MIN_SIZE)")
                return null
            }

            var pos = offset
            val id = readU32LE(data, pos)
            pos += 4
            val parentNode = readU32LE(data, pos)
            pos += 4
            val peripheralType = HivePeripheralType.fromValue(data[pos].toInt() and 0xFF)
            pos += 1

            // Read callsign (12 bytes, null-terminated string)
            val callsignBytes = data.sliceArray(pos until pos + 12)
            val nullIndex = callsignBytes.indexOf(0)
            val callsign = if (nullIndex >= 0) {
                String(callsignBytes, 0, nullIndex, Charsets.UTF_8)
            } else {
                String(callsignBytes, Charsets.UTF_8)
            }
            pos += 12

            val health = HiveHealthStatus.decode(data, pos)
            if (health == null) {
                Log.e(TAG, "Failed to decode health status")
                return null
            }
            pos += 4

            val hasEvent = data[pos] != 0.toByte()
            pos += 1

            val lastEvent = if (hasEvent) {
                if (data.size < offset + SIZE_WITH_EVENT) {
                    Log.e(TAG, "Peripheral with event too short: ${data.size - offset} bytes (need $SIZE_WITH_EVENT)")
                    return null
                }
                val event = HivePeripheralEvent.decode(data, pos)
                pos += 9
                event
            } else {
                null
            }

            if (data.size < pos + 8) {
                Log.e(TAG, "No room for timestamp at offset $pos")
                return null
            }
            val timestamp = readU64LE(data, pos)

            Log.d(TAG, "Decoded: id=${String.format("%08X", id)}, type=$peripheralType, " +
                    "event=${lastEvent?.eventType}, health=${health.batteryPercent}%")

            return HivePeripheral(
                id = id,
                parentNode = parentNode,
                peripheralType = peripheralType,
                callsign = callsign,
                health = health,
                lastEvent = lastEvent,
                timestamp = timestamp
            )
        }

        fun encode(peripheral: HivePeripheral): ByteArray {
            val hasEvent = peripheral.lastEvent != null
            val size = if (hasEvent) SIZE_WITH_EVENT else MIN_SIZE
            val buf = ByteArray(size)
            var pos = 0

            writeU32LE(buf, pos, peripheral.id)
            pos += 4
            writeU32LE(buf, pos, peripheral.parentNode)
            pos += 4
            buf[pos] = peripheral.peripheralType.value.toByte()
            pos += 1

            // Write callsign (12 bytes)
            val callsignBytes = peripheral.callsign.toByteArray(Charsets.UTF_8)
            for (i in 0 until 12) {
                buf[pos + i] = if (i < callsignBytes.size) callsignBytes[i] else 0
            }
            pos += 12

            val healthBytes = HiveHealthStatus.encode(peripheral.health)
            healthBytes.copyInto(buf, pos)
            pos += 4

            buf[pos] = if (hasEvent) 1 else 0
            pos += 1

            if (hasEvent && peripheral.lastEvent != null) {
                val eventBytes = HivePeripheralEvent.encode(peripheral.lastEvent)
                eventBytes.copyInto(buf, pos)
                pos += 9
            }

            writeU64LE(buf, pos, peripheral.timestamp)
            return buf
        }

        private fun readU32LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24))
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU32LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }

    /**
     * Get the current event type, or NONE if no event.
     */
    fun currentEventType(): HiveEventType = lastEvent?.eventType ?: HiveEventType.NONE
}

/**
 * HIVE CRDT GCounter entry.
 */
data class GCounterEntry(
    val nodeId: Long,
    val count: Long
)

/**
 * HIVE document format (compatible with M5Stack HIVE-lite).
 *
 * Wire format:
 * - Header: version (u32 LE) + node_id (u32 LE)
 * - GCounter: num_entries (u32 LE) + [node_id (u32 LE) + count (u64 LE)] * N
 * - Extended: 0xAB marker + reserved (u8) + peripheral_len (u16 LE) + peripheral data
 */
data class HiveDocument(
    val version: Long,
    val nodeId: Long,
    val counter: List<GCounterEntry>,
    val peripheral: HivePeripheral?
) {
    companion object {
        private const val TAG = "HiveDocument"
        private const val EXTENDED_MARKER: Byte = 0xAB.toByte()

        /**
         * Decode a HIVE document from raw bytes.
         *
         * @param data Raw document bytes
         * @return Decoded document, or null if parsing failed
         */
        fun decode(data: ByteArray): HiveDocument? {
            if (data.size < 8) {
                Log.e(TAG, "Document too short: ${data.size} bytes (minimum 8)")
                return null
            }

            try {
                var offset = 0

                // Read header
                val version = readU32LE(data, offset)
                offset += 4
                val nodeId = readU32LE(data, offset)
                offset += 4

                Log.d(TAG, "Header: version=$version, nodeId=${String.format("%08X", nodeId)}")

                // Read GCounter
                if (data.size < offset + 4) {
                    Log.e(TAG, "Document too short for GCounter count")
                    return null
                }
                val numEntries = readU32LE(data, offset).toInt()
                offset += 4

                if (data.size < offset + numEntries * 12) {
                    Log.e(TAG, "Document too short for GCounter entries: need ${offset + numEntries * 12}, have ${data.size}")
                    return null
                }

                val counter = mutableListOf<GCounterEntry>()
                for (i in 0 until numEntries) {
                    val entryNodeId = readU32LE(data, offset)
                    offset += 4
                    val count = readU64LE(data, offset)
                    offset += 8
                    counter.add(GCounterEntry(entryNodeId, count))
                    Log.d(TAG, "GCounter[$i]: nodeId=${String.format("%08X", entryNodeId)}, count=$count")
                }

                // Check for extended data (peripheral)
                var peripheral: HivePeripheral? = null
                if (data.size > offset && data[offset] == EXTENDED_MARKER) {
                    offset += 1  // Skip marker
                    if (data.size >= offset + 3) {
                        val reserved = data[offset].toInt() and 0xFF
                        offset += 1
                        val peripheralLen = readU16LE(data, offset).toInt()
                        offset += 2

                        Log.d(TAG, "Extended: reserved=$reserved, peripheralLen=$peripheralLen")

                        if (data.size >= offset + peripheralLen && peripheralLen > 0) {
                            // Decode full Peripheral structure
                            peripheral = HivePeripheral.decode(data, offset)
                            if (peripheral != null) {
                                Log.d(TAG, "Peripheral: eventType=${peripheral.currentEventType()}, " +
                                        "battery=${peripheral.health.batteryPercent}%")
                            } else {
                                Log.w(TAG, "Failed to decode peripheral data ($peripheralLen bytes)")
                            }
                        }
                    }
                }

                return HiveDocument(version, nodeId, counter, peripheral)

            } catch (e: Exception) {
                Log.e(TAG, "Failed to decode document", e)
                return null
            }
        }

        /**
         * Create an encoded HIVE document with full Peripheral structure.
         *
         * @param nodeId This node's ID
         * @param counter GCounter entries
         * @param peripheral Optional Peripheral data (contains event, health, etc.)
         * @return Encoded document bytes
         */
        fun encode(nodeId: Long, counter: List<GCounterEntry>, peripheral: HivePeripheral? = null): ByteArray {
            val headerSize = 8  // version + nodeId
            val counterSize = 4 + counter.size * 12  // count + entries
            val peripheralBytes = peripheral?.let { HivePeripheral.encode(it) }
            val extendedSize = if (peripheralBytes != null) 4 + peripheralBytes.size else 0  // marker + reserved + len(2) + data
            val totalSize = headerSize + counterSize + extendedSize

            val data = ByteArray(totalSize)
            var offset = 0

            // Write header
            writeU32LE(data, offset, 1)  // version = 1
            offset += 4
            writeU32LE(data, offset, nodeId)
            offset += 4

            // Write GCounter
            writeU32LE(data, offset, counter.size.toLong())
            offset += 4
            for (entry in counter) {
                writeU32LE(data, offset, entry.nodeId)
                offset += 4
                writeU64LE(data, offset, entry.count)
                offset += 8
            }

            // Write extended data (Peripheral)
            if (peripheralBytes != null) {
                data[offset] = EXTENDED_MARKER
                offset += 1
                data[offset] = 0  // reserved
                offset += 1
                writeU16LE(data, offset, peripheralBytes.size.toLong())
                offset += 2
                peripheralBytes.copyInto(data, offset)
            }

            return data
        }

        /**
         * Create an encoded HIVE document with just an event type (simple form).
         *
         * @param nodeId This node's ID
         * @param counter GCounter entries
         * @param eventType Optional event type
         * @return Encoded document bytes
         */
        fun encodeWithEvent(nodeId: Long, counter: List<GCounterEntry>, eventType: HiveEventType = HiveEventType.NONE): ByteArray {
            val peripheral = if (eventType != HiveEventType.NONE) {
                val timestamp = System.currentTimeMillis()
                HivePeripheral(
                    id = nodeId,
                    parentNode = 0,
                    peripheralType = HivePeripheralType.SOLDIER_SENSOR,
                    callsign = "",
                    health = HiveHealthStatus(100, null, 0, 0),
                    lastEvent = HivePeripheralEvent(eventType, timestamp),
                    timestamp = timestamp
                )
            } else null
            return encode(nodeId, counter, peripheral)
        }

        private fun readU16LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8))
        }

        private fun readU32LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24))
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU16LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
        }

        private fun writeU32LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }

    /**
     * Get the total count from all GCounter entries.
     */
    fun totalCount(): Long = counter.sumOf { it.count }

    /**
     * Get the current event type from peripheral data.
     */
    fun currentEventType(): HiveEventType = peripheral?.currentEventType() ?: HiveEventType.NONE

    /**
     * Get the battery percentage from peripheral health data.
     */
    fun batteryPercent(): Int? = peripheral?.health?.batteryPercent
}
