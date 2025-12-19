package com.hive.btle

import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.util.Log

/**
 * Proxy class that forwards BLE scan results to native Rust code via JNI.
 *
 * This class extends Android's ScanCallback and bridges scan events to the
 * hive-btle Rust implementation. When a BLE device is discovered, the scan
 * result is parsed and forwarded to native code for processing.
 *
 * Usage:
 * ```kotlin
 * val proxy = ScanCallbackProxy()
 * bluetoothLeScanner.startScan(filters, settings, proxy)
 * ```
 */
class ScanCallbackProxy(
    private val onDeviceFound: ((DiscoveredDevice) -> Unit)? = null
) : ScanCallback() {

    companion object {
        private const val TAG = "HiveBtle.ScanCallback"

        init {
            // Load native library
            try {
                System.loadLibrary("hive_btle")
                Log.i(TAG, "Loaded hive_btle native library")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }
    }

    /**
     * Called when a BLE device is discovered during scanning.
     *
     * Extracts device information from the ScanResult and forwards it to
     * native code for HIVE protocol processing.
     *
     * @param callbackType Type of callback (CALLBACK_TYPE_ALL_MATCHES, etc.)
     * @param result The scan result containing device information
     */
    override fun onScanResult(callbackType: Int, result: ScanResult) {
        try {
            val device = result.device
            val scanRecord = result.scanRecord

            // Extract device info
            val address = device.address
            val name = scanRecord?.deviceName ?: device.name ?: ""
            val rssi = result.rssi

            // Extract service UUIDs (look for HIVE service)
            val serviceUuids = scanRecord?.serviceUuids?.map { it.toString() } ?: emptyList()

            // Extract service data for HIVE service UUID
            // HIVE uses 16-bit UUID 0xF47A (matches M5Stack Core2 demo firmware)
            val hiveServiceData = scanRecord?.getServiceData(
                android.os.ParcelUuid.fromString(HiveBtle.HIVE_SERVICE_UUID.toString())
            )

            // Check if this is a HIVE device (by name prefix or service UUID)
            val isHiveDevice = name.startsWith(HiveBtle.HIVE_MESH_PREFIX) ||
                name.startsWith(HiveBtle.HIVE_NAME_PREFIX) ||
                serviceUuids.any { it.contains("F47A", ignoreCase = true) } ||
                hiveServiceData != null

            // Parse mesh ID and node ID from device name
            // Supports both new format (HIVE_MESHID-NODEID) and legacy format (HIVE-NODEID)
            val parsed = HiveBtle.parseDeviceName(name)
            var meshId: String? = parsed?.first
            var nodeId: Long? = parsed?.second

            // If name parsing failed, try service data (4 bytes, big-endian node ID)
            if (nodeId == null && hiveServiceData != null && hiveServiceData.size >= 4) {
                nodeId = ((hiveServiceData[0].toLong() and 0xFF) shl 24) or
                    ((hiveServiceData[1].toLong() and 0xFF) shl 16) or
                    ((hiveServiceData[2].toLong() and 0xFF) shl 8) or
                    (hiveServiceData[3].toLong() and 0xFF)
            }

            // Debug: log service data if present
            if (hiveServiceData != null) {
                Log.d(TAG, "HIVE service data (${hiveServiceData.size} bytes): ${hiveServiceData.joinToString(" ") { String.format("%02X", it) }}")
            }

            Log.d(TAG, "Scan result: $address ($name) RSSI=$rssi, isHive=$isHiveDevice, meshId=$meshId, nodeId=${nodeId?.let { String.format("%08X", it) }}")

            // Create discovered device and invoke callback
            val discoveredDevice = DiscoveredDevice(
                address = address,
                name = name,
                rssi = rssi,
                nodeId = nodeId,
                meshId = meshId,
                timestampNanos = result.timestampNanos,
                isHiveDevice = isHiveDevice
            )

            // Invoke Kotlin callback for UI updates
            onDeviceFound?.invoke(discoveredDevice)

            // Forward to native code
            nativeOnScanResult(
                callbackType,
                address,
                name,
                rssi,
                serviceUuids.toTypedArray(),
                hiveServiceData,
                result.timestampNanos
            )
        } catch (e: Exception) {
            Log.e(TAG, "Error processing scan result", e)
        }
    }

    /**
     * Called when batch scan results are available.
     *
     * Processes each result individually through onScanResult.
     *
     * @param results List of scan results
     */
    override fun onBatchScanResults(results: MutableList<ScanResult>) {
        Log.d(TAG, "Batch scan results: ${results.size} devices")
        for (result in results) {
            onScanResult(ScanSettings.CALLBACK_TYPE_ALL_MATCHES, result)
        }
    }

    /**
     * Called when scanning fails.
     *
     * @param errorCode Error code indicating the failure reason
     */
    override fun onScanFailed(errorCode: Int) {
        val errorMsg = when (errorCode) {
            SCAN_FAILED_ALREADY_STARTED -> "Scan already started"
            SCAN_FAILED_APPLICATION_REGISTRATION_FAILED -> "App registration failed"
            SCAN_FAILED_INTERNAL_ERROR -> "Internal error"
            SCAN_FAILED_FEATURE_UNSUPPORTED -> "Feature unsupported"
            else -> "Unknown error"
        }
        Log.e(TAG, "Scan failed: $errorMsg (code=$errorCode)")
        nativeOnScanFailed(errorCode, errorMsg)
    }

    // Native methods implemented in Rust via JNI

    /**
     * Native callback for scan results.
     *
     * @param callbackType Type of scan callback
     * @param address Bluetooth device address (MAC)
     * @param name Device name (may be empty)
     * @param rssi Signal strength in dBm
     * @param serviceUuids Array of advertised service UUIDs
     * @param hiveServiceData HIVE service data bytes (may be null)
     * @param timestampNanos Timestamp of the scan result
     */
    private external fun nativeOnScanResult(
        callbackType: Int,
        address: String,
        name: String,
        rssi: Int,
        serviceUuids: Array<String>,
        hiveServiceData: ByteArray?,
        timestampNanos: Long
    )

    /**
     * Native callback for scan failures.
     *
     * @param errorCode Android scan error code
     * @param errorMessage Human-readable error message
     */
    private external fun nativeOnScanFailed(errorCode: Int, errorMessage: String)
}
