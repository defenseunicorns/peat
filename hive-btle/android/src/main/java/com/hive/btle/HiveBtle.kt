package com.hive.btle

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothManager
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
 * @param nodeId This node's HIVE ID (32-bit unsigned)
 */
class HiveBtle(
    private val context: Context,
    private val nodeId: Long
) {
    companion object {
        private const val TAG = "HiveBtle"

        /**
         * HIVE BLE Service UUID (16-bit: 0xF47A)
         *
         * This matches the M5Stack Core2 demo firmware for interoperability testing.
         * The canonical HIVE service UUID is 0xD479 but the M5Stack uses 0xF47A.
         */
        val HIVE_SERVICE_UUID: UUID = UUID.fromString("0000F47A-0000-1000-8000-00805F9B34FB")

        /**
         * HIVE Document Characteristic UUID (16-bit: 0xF47B)
         *
         * Used for exchanging CRDT document data between peers.
         * Supports read, write, and notify operations.
         */
        val HIVE_CHAR_DOCUMENT: UUID = UUID.fromString("0000F47B-0000-1000-8000-00805F9B34FB")

        /** HIVE Node Info Characteristic UUID (legacy, not used by M5Stack) */
        val HIVE_CHAR_NODE_INFO: UUID = UUID.fromString("00000001-F47A-0000-1000-00805F9B34FB")

        /** HIVE Sync State Characteristic UUID (legacy, not used by M5Stack) */
        val HIVE_CHAR_SYNC_STATE: UUID = UUID.fromString("00000002-F47A-0000-1000-00805F9B34FB")

        /** HIVE Sync Data Characteristic UUID (legacy, not used by M5Stack) */
        val HIVE_CHAR_SYNC_DATA: UUID = UUID.fromString("00000003-F47A-0000-1000-00805F9B34FB")

        /** HIVE Command Characteristic UUID (legacy, not used by M5Stack) */
        val HIVE_CHAR_COMMAND: UUID = UUID.fromString("00000004-F47A-0000-1000-00805F9B34FB")

        /** HIVE Status Characteristic UUID (legacy, not used by M5Stack) */
        val HIVE_CHAR_STATUS: UUID = UUID.fromString("00000005-F47A-0000-1000-00805F9B34FB")

        /** Client Characteristic Configuration Descriptor UUID */
        val CCCD_UUID: UUID = UUID.fromString("00002902-0000-1000-8000-00805F9B34FB")

        /** HIVE device name prefix */
        const val HIVE_NAME_PREFIX = "HIVE-"

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

    // Active GATT connections
    private val connections = ConcurrentHashMap<String, BluetoothGatt>()
    private val gattCallbacks = ConcurrentHashMap<String, GattCallbackProxy>()
    private val connectionIdCounter = AtomicLong(0)

    // State
    private var isInitialized = false
    private var isScanning = false
    private var isAdvertising = false

    // Native handle
    private var nativeHandle: Long = 0

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

        // Build scan filters for HIVE devices
        val filters = listOf(
            ScanFilter.Builder()
                .setServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
                .build()
        )

        // Build scan settings
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
            .setMatchMode(ScanSettings.MATCH_MODE_AGGRESSIVE)
            .setNumOfMatches(ScanSettings.MATCH_NUM_MAX_ADVERTISEMENT)
            .setReportDelay(0)
            .build()

        // Create callback proxy
        scanCallback = ScanCallbackProxy()

        try {
            scanner.startScan(filters, settings, scanCallback)
            isScanning = true
            Log.i(TAG, "Started scanning for HIVE devices")
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

        // Build advertise data
        // Include HIVE service UUID and node ID in service data
        val nodeIdBytes = byteArrayOf(
            (nodeId shr 24).toByte(),
            (nodeId shr 16).toByte(),
            (nodeId shr 8).toByte(),
            nodeId.toByte()
        )

        val data = AdvertiseData.Builder()
            .setIncludeDeviceName(false) // Name goes in scan response
            .addServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
            .addServiceData(ParcelUuid(HIVE_SERVICE_UUID), nodeIdBytes)
            .build()

        // Build scan response with device name
        val scanResponse = AdvertiseData.Builder()
            .setIncludeDeviceName(true)
            .build()

        // Create callback proxy
        advertiseCallback = AdvertiseCallbackProxy()

        try {
            advertiser.startAdvertising(settings, data, scanResponse, advertiseCallback)
            isAdvertising = true
            Log.i(TAG, "Started advertising as HIVE-${String.format("%08X", nodeId)}")
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

    /**
     * Disconnect all devices and clean up resources.
     */
    fun shutdown() {
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
     * Get list of connected device addresses.
     */
    fun connectedDevices(): List<String> = connections.keys.toList()

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
    val timestampNanos: Long
)

/**
 * Represents an active GATT connection to a HIVE device.
 */
class HiveConnection internal constructor(
    val address: String,
    private val gatt: BluetoothGatt,
    private val callback: GattCallbackProxy
) {
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
}
