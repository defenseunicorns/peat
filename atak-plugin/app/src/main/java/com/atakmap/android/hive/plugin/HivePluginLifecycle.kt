package com.atakmap.android.hive.plugin

import android.util.Log
import com.atak.plugins.impl.AbstractPlugin
import com.atak.plugins.impl.PluginContextProvider
import gov.tak.api.plugin.IServiceController

/**
 * HIVE Plugin Lifecycle Manager
 *
 * Main entry point for the HIVE ATAK plugin. Extends AbstractPlugin
 * as per ATAK SDK 5.6 pattern.
 */
class HivePluginLifecycle(serviceController: IServiceController) : AbstractPlugin(
    serviceController,
    HiveTool(serviceController.getService(PluginContextProvider::class.java).pluginContext),
    HiveMapComponent()
) {
    companion object {
        private const val TAG = "HivePluginLifecycle"

        @Volatile
        private var instance: HivePluginLifecycle? = null

        fun getInstance(): HivePluginLifecycle? = instance
    }

    private var hiveFfiInitialized = false

    init {
        instance = this
        val pluginContext = serviceController.getService(PluginContextProvider::class.java).pluginContext

        // Initialize native library loader
        HiveNativeLoader.init(pluginContext)

        // Load hive-ffi native library
        try {
            HiveNativeLoader.loadLibrary("hive_ffi")
            hiveFfiInitialized = true
            Log.i(TAG, "hive-ffi native library loaded successfully")

            // Test FFI by getting version
            val hiveVersion = uniffi.hive_ffi.hiveVersion()
            Log.i(TAG, "HIVE FFI version: $hiveVersion")
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load hive-ffi native library: ${e.message}")
            hiveFfiInitialized = false
        } catch (e: Exception) {
            Log.e(TAG, "Error initializing hive-ffi: ${e.message}")
            hiveFfiInitialized = false
        }

        Log.i(TAG, "HIVE Plugin initialized (FFI: $hiveFfiInitialized)")
    }

    fun isHiveFfiAvailable(): Boolean = hiveFfiInitialized
}
