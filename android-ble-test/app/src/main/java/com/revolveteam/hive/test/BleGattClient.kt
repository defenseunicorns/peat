/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.hive.test

import android.annotation.SuppressLint
import android.bluetooth.*
import android.bluetooth.le.*
import android.content.Context
import android.os.ParcelUuid
import android.util.Log
import kotlinx.coroutines.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.UUID
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * Android BLE GATT client implementing the HIVE GATT protocol.
 *
 * Uses raw Android BLE APIs (not HiveBtle AAR) to prove the GATT protocol
 * directly. Wraps callback-based APIs into sequential coroutines.
 *
 * GATT Service: f47ac10b-58cc-4372-a567-0e02b2c3d479
 * Characteristics:
 *   Node Info   (0x0001) - read:          peer identity
 *   Sync State  (0x0002) - read/notify:   peer's sync document
 *   Sync Data   (0x0003) - write/indicate: send our sync document
 */
@SuppressLint("MissingPermission")
class BleGattClient(private val context: Context) {

    companion object {
        private const val TAG = "BleGattClient"

        // HIVE GATT UUIDs — derived from base service UUID
        // Characteristic UUIDs replace bytes [2:3] of service UUID with char ID
        val HIVE_SERVICE_UUID: UUID =
            UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
        val NODE_INFO_UUID: UUID =
            UUID.fromString("f47a0001-58cc-4372-a567-0e02b2c3d479")
        val SYNC_STATE_UUID: UUID =
            UUID.fromString("f47a0002-58cc-4372-a567-0e02b2c3d479")
        val SYNC_DATA_UUID: UUID =
            UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")

        // 16-bit UUID used in scan filter (BT SIG base UUID form)
        val HIVE_SERVICE_UUID_16BIT: UUID =
            UUID.fromString("0000f47a-0000-1000-8000-00805f9b34fb")
    }

    data class DiscoveredDevice(
        val device: BluetoothDevice,
        val name: String?,
        val rssi: Int
    )

    data class NodeInfo(
        val nodeId: Long,
        val protocolVersion: Int,
        val hierarchyLevel: Int,
        val capabilities: Int,
        val batteryPercent: Int
    ) {
        val nodeIdHex: String get() = String.format("%08X", nodeId)
    }

    data class SyncResult(
        val nodeInfo: NodeInfo,
        val bytesWritten: Int,
        val bytesRead: Int,
        val latencyMs: Long
    )

    private var bluetoothGatt: BluetoothGatt? = null
    private val gattMutex = Mutex()

    // Scan for HIVE devices.
    // The BLE device name format is "HIVE-{NODE_ID}" (e.g. "HIVE-C8E32F88").
    // The mesh ID is carried in scan response data, not the device name.
    suspend fun scan(meshId: String, timeoutMs: Long = 15_000): DiscoveredDevice {
        val adapter = BluetoothAdapter.getDefaultAdapter()
            ?: throw IllegalStateException("Bluetooth not available")
        val scanner = adapter.bluetoothLeScanner
            ?: throw IllegalStateException("BLE scanner not available (is Bluetooth on?)")

        val hivePrefix = "HIVE-"
        Log.i(TAG, "Scanning for HIVE devices (prefix: $hivePrefix, mesh: $meshId)")

        return suspendCancellableCoroutine { cont ->
            val callback = object : ScanCallback() {
                override fun onScanResult(callbackType: Int, result: ScanResult) {
                    val name = result.device.name ?: result.scanRecord?.deviceName
                    Log.d(TAG, "Scan result: name=$name, addr=${result.device.address}, rssi=${result.rssi}")

                    if (name != null && name.startsWith(hivePrefix)) {
                        Log.i(TAG, "Found HIVE device: $name (${result.device.address}), RSSI: ${result.rssi}")
                        scanner.stopScan(this)
                        if (cont.isActive) {
                            cont.resume(DiscoveredDevice(result.device, name, result.rssi))
                        }
                    }
                }

                override fun onScanFailed(errorCode: Int) {
                    Log.e(TAG, "Scan failed with error: $errorCode")
                    if (cont.isActive) {
                        cont.resumeWithException(
                            RuntimeException("BLE scan failed: error $errorCode")
                        )
                    }
                }
            }

            // Scan with service UUID filter and also by name
            val filters = listOf(
                ScanFilter.Builder()
                    .setServiceUuid(ParcelUuid(HIVE_SERVICE_UUID_16BIT))
                    .build()
            )
            val settings = ScanSettings.Builder()
                .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
                .build()

            scanner.startScan(filters, settings, callback)

            // Also start an unfiltered scan as fallback (some devices don't
            // advertise service UUIDs in scan response)
            val unfilteredCallback = object : ScanCallback() {
                override fun onScanResult(callbackType: Int, result: ScanResult) {
                    val name = result.device.name ?: result.scanRecord?.deviceName
                    if (name != null && name.startsWith(hivePrefix)) {
                        Log.i(TAG, "Found HIVE device (unfiltered): $name (${result.device.address}), RSSI: ${result.rssi}")
                        scanner.stopScan(this)
                        scanner.stopScan(callback)
                        if (cont.isActive) {
                            cont.resume(DiscoveredDevice(result.device, name, result.rssi))
                        }
                    }
                }
            }
            scanner.startScan(unfilteredCallback)

            cont.invokeOnCancellation {
                scanner.stopScan(callback)
                scanner.stopScan(unfilteredCallback)
            }

            // Timeout
            MainScope().launch {
                delay(timeoutMs)
                scanner.stopScan(callback)
                scanner.stopScan(unfilteredCallback)
                if (cont.isActive) {
                    cont.resumeWithException(
                        RuntimeException("BLE scan timed out after ${timeoutMs}ms")
                    )
                }
            }
        }
    }

    @Volatile
    private var pendingServiceDiscovery: CancellableContinuation<BluetoothGattService>? = null
    @Volatile
    private var pendingCharRead: CancellableContinuation<ByteArray>? = null
    @Volatile
    private var pendingCharWrite: CancellableContinuation<Unit>? = null

    // Full GATT callback that handles all async operations
    private val gattCallback = object : BluetoothGattCallback() {
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            Log.d(TAG, "Connection state: $newState (status=$status)")
        }

        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            val cont = pendingServiceDiscovery ?: return
            pendingServiceDiscovery = null

            if (status == BluetoothGatt.GATT_SUCCESS) {
                val service = gatt.getService(HIVE_SERVICE_UUID)
                    ?: gatt.services?.find { svc ->
                        svc.uuid == HIVE_SERVICE_UUID ||
                        svc.uuid == HIVE_SERVICE_UUID_16BIT
                    }

                if (service != null) {
                    Log.i(TAG, "Found HIVE GATT service: ${service.uuid}")
                    if (cont.isActive) cont.resume(service)
                } else {
                    val allServices = gatt.services?.map { it.uuid } ?: emptyList()
                    Log.e(TAG, "HIVE service not found. Available: $allServices")
                    if (cont.isActive) {
                        cont.resumeWithException(RuntimeException("HIVE GATT service not found"))
                    }
                }
            } else {
                if (cont.isActive) {
                    cont.resumeWithException(RuntimeException("Service discovery failed: $status"))
                }
            }
        }

        @Suppress("DEPRECATION")
        override fun onCharacteristicRead(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
            status: Int
        ) {
            val cont = pendingCharRead ?: return
            pendingCharRead = null

            if (status == BluetoothGatt.GATT_SUCCESS) {
                val data = characteristic.value ?: ByteArray(0)
                Log.d(TAG, "Read ${data.size} bytes from ${characteristic.uuid}")
                if (cont.isActive) cont.resume(data)
            } else {
                if (cont.isActive) {
                    cont.resumeWithException(RuntimeException("Read failed: status=$status"))
                }
            }
        }

        override fun onCharacteristicWrite(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
            status: Int
        ) {
            val cont = pendingCharWrite ?: return
            pendingCharWrite = null

            if (status == BluetoothGatt.GATT_SUCCESS) {
                Log.d(TAG, "Write succeeded to ${characteristic.uuid}")
                if (cont.isActive) cont.resume(Unit)
            } else {
                if (cont.isActive) {
                    cont.resumeWithException(RuntimeException("Write failed: status=$status"))
                }
            }
        }
    }

    // Connect with the unified callback, with retry for transient GATT errors
    // (status 133/147 are common on Samsung/Android and usually succeed on retry)
    suspend fun connectAndDiscover(
        device: BluetoothDevice,
        maxRetries: Int = 3
    ): Pair<BluetoothGatt, BluetoothGattService> {
        var lastException: Exception? = null

        for (attempt in 1..maxRetries) {
            try {
                Log.i(TAG, "Connecting to ${device.address} (attempt $attempt/$maxRetries)...")

                val gatt = connectGatt(device)

                // Delay for connection to stabilize
                delay(500)

                // Clear GATT cache to avoid stale service discovery results
                // (Android caches GATT services per MAC address)
                refreshGattCache(gatt)
                delay(200)

                // Discover services (with 10s timeout)
                val service = withTimeout(10_000) {
                    suspendCancellableCoroutine<BluetoothGattService> { cont ->
                        pendingServiceDiscovery = cont
                        gatt.discoverServices()
                    }
                }

                return Pair(gatt, service)
            } catch (e: Exception) {
                lastException = e
                Log.w(TAG, "Connection attempt $attempt failed: ${e.message}")
                // Clean up stale GATT state before retry
                bluetoothGatt?.let {
                    it.disconnect()
                    it.close()
                    bluetoothGatt = null
                }
                if (attempt < maxRetries) {
                    val backoff = attempt * 1500L
                    Log.i(TAG, "Retrying in ${backoff}ms...")
                    delay(backoff)
                }
            }
        }

        throw lastException ?: RuntimeException("Connection failed after $maxRetries attempts")
    }

    private suspend fun connectGatt(device: BluetoothDevice): BluetoothGatt {
        return suspendCancellableCoroutine { cont ->
            val connectCallback = object : BluetoothGattCallback() {
                override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
                    gattCallback.onConnectionStateChange(g, status, newState)
                    when (newState) {
                        BluetoothProfile.STATE_CONNECTED -> {
                            Log.i(TAG, "Connected to ${device.address}")
                            bluetoothGatt = g
                            if (cont.isActive) cont.resume(g)
                        }
                        BluetoothProfile.STATE_DISCONNECTED -> {
                            val err = RuntimeException("Connection failed (status=$status)")
                            if (cont.isActive) {
                                cont.resumeWithException(err)
                            }
                            // Cancel any pending operations if disconnected after connect
                            pendingServiceDiscovery?.let { if (it.isActive) it.resumeWithException(err) }
                            pendingServiceDiscovery = null
                            pendingCharRead?.let { if (it.isActive) it.resumeWithException(err) }
                            pendingCharRead = null
                            pendingCharWrite?.let { if (it.isActive) it.resumeWithException(err) }
                            pendingCharWrite = null
                        }
                    }
                }

                override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
                    gattCallback.onServicesDiscovered(g, status)
                }

                @Suppress("DEPRECATION")
                override fun onCharacteristicRead(
                    g: BluetoothGatt,
                    characteristic: BluetoothGattCharacteristic,
                    status: Int
                ) {
                    gattCallback.onCharacteristicRead(g, characteristic, status)
                }

                override fun onCharacteristicWrite(
                    g: BluetoothGatt,
                    characteristic: BluetoothGattCharacteristic,
                    status: Int
                ) {
                    gattCallback.onCharacteristicWrite(g, characteristic, status)
                }
            }

            device.connectGatt(context, false, connectCallback, BluetoothDevice.TRANSPORT_LE)
            cont.invokeOnCancellation { bluetoothGatt?.disconnect() }
        }
    }

    // Read Node Info characteristic (9 bytes)
    // Format: [node_id(4, BE), protocol_version(1), hierarchy_level(1), capabilities(2, BE), battery(1)]
    suspend fun readNodeInfo(
        gatt: BluetoothGatt,
        service: BluetoothGattService
    ): NodeInfo = gattMutex.withLock {
        val char = service.getCharacteristic(NODE_INFO_UUID)
            ?: throw RuntimeException("Node Info characteristic not found")

        val data = suspendCancellableCoroutine<ByteArray> { cont ->
            pendingCharRead = cont
            @Suppress("DEPRECATION")
            gatt.readCharacteristic(char)
        }

        if (data.size < 4) {
            throw RuntimeException("Node Info too short: ${data.size} bytes (expected >= 4)")
        }

        val nodeId = ((data[0].toLong() and 0xFF) shl 24) or
                ((data[1].toLong() and 0xFF) shl 16) or
                ((data[2].toLong() and 0xFF) shl 8) or
                (data[3].toLong() and 0xFF)

        // Support both 4-byte (node ID only) and 9-byte (full) formats
        NodeInfo(
            nodeId = nodeId,
            protocolVersion = if (data.size > 4) data[4].toInt() and 0xFF else 0,
            hierarchyLevel = if (data.size > 5) data[5].toInt() and 0xFF else 0,
            capabilities = if (data.size > 7) ((data[6].toInt() and 0xFF) shl 8) or (data[7].toInt() and 0xFF) else 0,
            batteryPercent = if (data.size > 8) data[8].toInt() and 0xFF else 255
        )
    }

    // Write sync data to the Sync Data characteristic
    @Suppress("DEPRECATION")
    suspend fun writeSyncData(
        gatt: BluetoothGatt,
        service: BluetoothGattService,
        data: ByteArray
    ): Unit = gattMutex.withLock {
        val char = service.getCharacteristic(SYNC_DATA_UUID)
            ?: throw RuntimeException("Sync Data characteristic not found")

        suspendCancellableCoroutine<Unit> { cont ->
            pendingCharWrite = cont
            char.value = data
            char.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
            gatt.writeCharacteristic(char)
        }
    }

    // Read Sync State characteristic (peer's sync document)
    @Suppress("DEPRECATION")
    suspend fun readSyncState(
        gatt: BluetoothGatt,
        service: BluetoothGattService
    ): ByteArray = gattMutex.withLock {
        val char = service.getCharacteristic(SYNC_STATE_UUID)
            ?: throw RuntimeException("Sync State characteristic not found")

        suspendCancellableCoroutine<ByteArray> { cont ->
            pendingCharRead = cont
            gatt.readCharacteristic(char)
        }
    }

    // Full sync operation: read node info, write our data, read their state
    suspend fun performSync(
        gatt: BluetoothGatt,
        service: BluetoothGattService,
        ourSyncData: ByteArray
    ): SyncResult {
        val startTime = System.currentTimeMillis()

        // Step 1: Read peer's Node Info
        val nodeInfo = readNodeInfo(gatt, service)
        Log.i(TAG, "Peer node: 0x${nodeInfo.nodeIdHex}, proto=${nodeInfo.protocolVersion}")

        // Step 2: Write our sync document
        writeSyncData(gatt, service, ourSyncData)
        Log.i(TAG, "Wrote ${ourSyncData.size} bytes to Sync Data")

        // Small delay to let responder process
        delay(100)

        // Step 3: Read peer's sync state
        val peerState = readSyncState(gatt, service)
        Log.i(TAG, "Read ${peerState.size} bytes from Sync State")

        val latency = System.currentTimeMillis() - startTime

        return SyncResult(
            nodeInfo = nodeInfo,
            bytesWritten = ourSyncData.size,
            bytesRead = peerState.size,
            latencyMs = latency
        )
    }

    /** Clear Android's GATT service cache via hidden BluetoothGatt.refresh() API */
    private fun refreshGattCache(gatt: BluetoothGatt): Boolean {
        return try {
            val method = gatt.javaClass.getMethod("refresh")
            method.invoke(gatt) as? Boolean ?: false
        } catch (e: Exception) {
            Log.w(TAG, "GATT cache refresh not available: ${e.message}")
            false
        }
    }

    fun disconnect() {
        bluetoothGatt?.let { gatt ->
            Log.i(TAG, "Disconnecting...")
            gatt.disconnect()
            gatt.close()
            bluetoothGatt = null
        }
    }

}
