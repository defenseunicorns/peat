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
 *
 * NOTE: This is a simplified version without FFI loading to avoid
 * JNA/coroutines dependency conflicts with ATAK SDK 5.6 Preview.
 * Full FFI integration will be added when testing on real hardware.
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

        // Initialize native library loader (for future FFI loading)
        HiveNativeLoader.init(pluginContext)

        // NOTE: FFI loading is temporarily disabled to avoid JNA dependency conflicts
        // with ATAK SDK 5.6 Preview. Will be re-enabled when testing on device.
        //
        // try {
        //     HiveNativeLoader.loadLibrary("hive_ffi")
        //     hiveFfiInitialized = true
        //     Log.i(TAG, "hive-ffi native library loaded successfully")
        // } catch (e: UnsatisfiedLinkError) {
        //     Log.e(TAG, "Failed to load hive-ffi native library: ${e.message}")
        // }

        Log.i(TAG, "HIVE Plugin initialized (FFI disabled)")
    }

    fun isHiveFfiAvailable(): Boolean = hiveFfiInitialized
}
