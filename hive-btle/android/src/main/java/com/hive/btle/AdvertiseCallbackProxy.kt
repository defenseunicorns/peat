package com.hive.btle

import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseSettings
import android.util.Log

/**
 * Proxy class that forwards BLE advertising events to native Rust code via JNI.
 *
 * This class extends Android's AdvertiseCallback and bridges advertising
 * start/stop events to the hive-btle Rust implementation.
 *
 * Usage:
 * ```kotlin
 * val proxy = AdvertiseCallbackProxy()
 * bluetoothLeAdvertiser.startAdvertising(settings, data, proxy)
 * ```
 */
class AdvertiseCallbackProxy : AdvertiseCallback() {

    companion object {
        private const val TAG = "HiveBtle.AdvertiseCb"

        init {
            try {
                System.loadLibrary("hive_btle")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }
    }

    /**
     * Called when advertising starts successfully.
     *
     * @param settingsInEffect The actual settings used for advertising
     */
    override fun onStartSuccess(settingsInEffect: AdvertiseSettings) {
        val mode = when (settingsInEffect.mode) {
            AdvertiseSettings.ADVERTISE_MODE_LOW_POWER -> "LOW_POWER"
            AdvertiseSettings.ADVERTISE_MODE_BALANCED -> "BALANCED"
            AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY -> "LOW_LATENCY"
            else -> "UNKNOWN"
        }
        val txPower = when (settingsInEffect.txPowerLevel) {
            AdvertiseSettings.ADVERTISE_TX_POWER_ULTRA_LOW -> "ULTRA_LOW"
            AdvertiseSettings.ADVERTISE_TX_POWER_LOW -> "LOW"
            AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM -> "MEDIUM"
            AdvertiseSettings.ADVERTISE_TX_POWER_HIGH -> "HIGH"
            else -> "UNKNOWN"
        }
        Log.i(TAG, "Advertising started: mode=$mode, txPower=$txPower, connectable=${settingsInEffect.isConnectable}")

        nativeOnStartSuccess(
            settingsInEffect.mode,
            settingsInEffect.txPowerLevel,
            settingsInEffect.isConnectable,
            settingsInEffect.timeout
        )
    }

    /**
     * Called when advertising fails to start.
     *
     * @param errorCode Error code indicating the failure reason
     */
    override fun onStartFailure(errorCode: Int) {
        val errorMsg = when (errorCode) {
            ADVERTISE_FAILED_DATA_TOO_LARGE -> "Data too large"
            ADVERTISE_FAILED_TOO_MANY_ADVERTISERS -> "Too many advertisers"
            ADVERTISE_FAILED_ALREADY_STARTED -> "Already started"
            ADVERTISE_FAILED_INTERNAL_ERROR -> "Internal error"
            ADVERTISE_FAILED_FEATURE_UNSUPPORTED -> "Feature unsupported"
            else -> "Unknown error"
        }
        Log.e(TAG, "Advertising failed: $errorMsg (code=$errorCode)")

        nativeOnStartFailure(errorCode, errorMsg)
    }

    // Native methods implemented in Rust via JNI

    /**
     * Native callback for advertising start success.
     *
     * @param mode Advertising mode (LOW_POWER=0, BALANCED=1, LOW_LATENCY=2)
     * @param txPowerLevel TX power level
     * @param isConnectable Whether advertising is connectable
     * @param timeout Advertising timeout in milliseconds (0 = no timeout)
     */
    private external fun nativeOnStartSuccess(
        mode: Int,
        txPowerLevel: Int,
        isConnectable: Boolean,
        timeout: Int
    )

    /**
     * Native callback for advertising start failure.
     *
     * @param errorCode Android advertise error code
     * @param errorMessage Human-readable error message
     */
    private external fun nativeOnStartFailure(errorCode: Int, errorMessage: String)
}
