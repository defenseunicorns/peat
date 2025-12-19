package com.hive.btle

import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothProfile
import android.util.Log

/**
 * Proxy class that forwards GATT events to native Rust code via JNI.
 *
 * This class extends Android's BluetoothGattCallback and bridges all GATT
 * events to the hive-btle Rust implementation. It handles connection state
 * changes, service discovery, characteristic reads/writes, and notifications.
 *
 * Usage:
 * ```kotlin
 * val proxy = GattCallbackProxy(connectionId)
 * device.connectGatt(context, false, proxy, BluetoothDevice.TRANSPORT_LE)
 * ```
 *
 * @param connectionId Unique identifier for this connection (used by native code)
 */
class GattCallbackProxy(private val connectionId: Long) : BluetoothGattCallback() {

    companion object {
        private const val TAG = "HiveBtle.GattCallback"

        // GATT status codes
        const val GATT_SUCCESS = BluetoothGatt.GATT_SUCCESS

        // Connection states
        const val STATE_DISCONNECTED = BluetoothProfile.STATE_DISCONNECTED
        const val STATE_CONNECTING = BluetoothProfile.STATE_CONNECTING
        const val STATE_CONNECTED = BluetoothProfile.STATE_CONNECTED
        const val STATE_DISCONNECTING = BluetoothProfile.STATE_DISCONNECTING

        init {
            try {
                System.loadLibrary("hive_btle")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }
    }

    /**
     * Called when the connection state changes.
     *
     * @param gatt The GATT client
     * @param status Status of the operation (GATT_SUCCESS if successful)
     * @param newState The new connection state
     */
    override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
        val stateStr = when (newState) {
            STATE_DISCONNECTED -> "DISCONNECTED"
            STATE_CONNECTING -> "CONNECTING"
            STATE_CONNECTED -> "CONNECTED"
            STATE_DISCONNECTING -> "DISCONNECTING"
            else -> "UNKNOWN($newState)"
        }
        Log.i(TAG, "Connection state changed: $stateStr (status=$status)")

        val address = gatt.device?.address ?: ""
        nativeOnConnectionStateChange(connectionId, address, status, newState)

        // Auto-discover services on connect
        if (newState == STATE_CONNECTED && status == GATT_SUCCESS) {
            Log.d(TAG, "Starting service discovery")
            gatt.discoverServices()
        }
    }

    /**
     * Called when GATT services have been discovered.
     *
     * @param gatt The GATT client
     * @param status Status of the discovery operation
     */
    override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
        Log.i(TAG, "Services discovered (status=$status)")

        if (status == GATT_SUCCESS) {
            // Log discovered services
            for (service in gatt.services) {
                Log.d(TAG, "  Service: ${service.uuid}")
                for (char in service.characteristics) {
                    Log.d(TAG, "    Char: ${char.uuid} (props=${char.properties})")
                }
            }
        }

        val address = gatt.device?.address ?: ""
        val serviceUuids = if (status == GATT_SUCCESS) {
            gatt.services.map { it.uuid.toString() }.toTypedArray()
        } else {
            emptyArray()
        }

        nativeOnServicesDiscovered(connectionId, address, status, serviceUuids)
    }

    /**
     * Called when a characteristic read operation completes.
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic that was read
     * @param status Status of the read operation
     */
    @Deprecated("Deprecated in API 33")
    override fun onCharacteristicRead(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        status: Int
    ) {
        val value = characteristic.value ?: ByteArray(0)
        Log.d(TAG, "Characteristic read: ${characteristic.uuid} (${value.size} bytes, status=$status)")

        nativeOnCharacteristicRead(
            connectionId,
            characteristic.service.uuid.toString(),
            characteristic.uuid.toString(),
            status,
            value
        )
    }

    /**
     * Called when a characteristic read operation completes (API 33+).
     */
    override fun onCharacteristicRead(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray,
        status: Int
    ) {
        Log.d(TAG, "Characteristic read: ${characteristic.uuid} (${value.size} bytes, status=$status)")

        nativeOnCharacteristicRead(
            connectionId,
            characteristic.service.uuid.toString(),
            characteristic.uuid.toString(),
            status,
            value
        )
    }

    /**
     * Called when a characteristic write operation completes.
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic that was written
     * @param status Status of the write operation
     */
    override fun onCharacteristicWrite(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        status: Int
    ) {
        Log.d(TAG, "Characteristic write: ${characteristic.uuid} (status=$status)")

        nativeOnCharacteristicWrite(
            connectionId,
            characteristic.service.uuid.toString(),
            characteristic.uuid.toString(),
            status
        )
    }

    /**
     * Called when a characteristic value changes (notification/indication).
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic whose value changed
     */
    @Deprecated("Deprecated in API 33")
    override fun onCharacteristicChanged(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic
    ) {
        val value = characteristic.value ?: ByteArray(0)
        Log.d(TAG, "Characteristic changed: ${characteristic.uuid} (${value.size} bytes)")

        nativeOnCharacteristicChanged(
            connectionId,
            characteristic.service.uuid.toString(),
            characteristic.uuid.toString(),
            value
        )
    }

    /**
     * Called when a characteristic value changes (API 33+).
     */
    override fun onCharacteristicChanged(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray
    ) {
        Log.d(TAG, "Characteristic changed: ${characteristic.uuid} (${value.size} bytes)")

        nativeOnCharacteristicChanged(
            connectionId,
            characteristic.service.uuid.toString(),
            characteristic.uuid.toString(),
            value
        )
    }

    /**
     * Called when a descriptor write operation completes.
     *
     * @param gatt The GATT client
     * @param descriptor The descriptor that was written
     * @param status Status of the write operation
     */
    override fun onDescriptorWrite(
        gatt: BluetoothGatt,
        descriptor: BluetoothGattDescriptor,
        status: Int
    ) {
        Log.d(TAG, "Descriptor write: ${descriptor.uuid} (status=$status)")

        nativeOnDescriptorWrite(
            connectionId,
            descriptor.characteristic.service.uuid.toString(),
            descriptor.characteristic.uuid.toString(),
            descriptor.uuid.toString(),
            status
        )
    }

    /**
     * Called when the MTU for a connection changes.
     *
     * @param gatt The GATT client
     * @param mtu The new MTU size
     * @param status Status of the MTU request
     */
    override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) {
        Log.i(TAG, "MTU changed: $mtu (status=$status)")
        nativeOnMtuChanged(connectionId, mtu, status)
    }

    /**
     * Called when the PHY is updated.
     *
     * @param gatt The GATT client
     * @param txPhy The new TX PHY
     * @param rxPhy The new RX PHY
     * @param status Status of the PHY update
     */
    override fun onPhyUpdate(gatt: BluetoothGatt, txPhy: Int, rxPhy: Int, status: Int) {
        Log.i(TAG, "PHY updated: tx=$txPhy, rx=$rxPhy (status=$status)")
        nativeOnPhyUpdate(connectionId, txPhy, rxPhy, status)
    }

    /**
     * Called when the RSSI is read.
     *
     * @param gatt The GATT client
     * @param rssi The RSSI value
     * @param status Status of the RSSI read
     */
    override fun onReadRemoteRssi(gatt: BluetoothGatt, rssi: Int, status: Int) {
        Log.d(TAG, "RSSI read: $rssi dBm (status=$status)")
        nativeOnReadRemoteRssi(connectionId, rssi, status)
    }

    // Native methods implemented in Rust via JNI

    private external fun nativeOnConnectionStateChange(
        connectionId: Long,
        address: String,
        status: Int,
        newState: Int
    )

    private external fun nativeOnServicesDiscovered(
        connectionId: Long,
        address: String,
        status: Int,
        serviceUuids: Array<String>
    )

    private external fun nativeOnCharacteristicRead(
        connectionId: Long,
        serviceUuid: String,
        charUuid: String,
        status: Int,
        value: ByteArray
    )

    private external fun nativeOnCharacteristicWrite(
        connectionId: Long,
        serviceUuid: String,
        charUuid: String,
        status: Int
    )

    private external fun nativeOnCharacteristicChanged(
        connectionId: Long,
        serviceUuid: String,
        charUuid: String,
        value: ByteArray
    )

    private external fun nativeOnDescriptorWrite(
        connectionId: Long,
        serviceUuid: String,
        charUuid: String,
        descriptorUuid: String,
        status: Int
    )

    private external fun nativeOnMtuChanged(connectionId: Long, mtu: Int, status: Int)

    private external fun nativeOnPhyUpdate(connectionId: Long, txPhy: Int, rxPhy: Int, status: Int)

    private external fun nativeOnReadRemoteRssi(connectionId: Long, rssi: Int, status: Int)
}
