/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.atak.hive

import android.content.Context
import android.os.Environment
import android.util.Log
import com.atak.plugins.impl.AbstractPlugin
import com.atak.plugins.impl.PluginContextProvider
import com.atakmap.coremap.filesystem.FileSystemUtils
import gov.tak.api.plugin.IServiceController
import java.io.File

/**
 * HIVE Plugin Lifecycle Manager
 *
 * Main entry point for the HIVE ATAK plugin. Extends AbstractPlugin
 * as per ATAK SDK 5.6 pattern.
 *
 * Uses direct JNI bindings to bypass JNA/UniFFI symbol lookup issues
 * caused by Android's linker namespace isolation.
 */
class HivePluginLifecycle(serviceController: IServiceController) : AbstractPlugin(
    serviceController,
    HiveTool(serviceController.getService(PluginContextProvider::class.java).pluginContext),
    HiveMapComponent()
) {
    companion object {
        private const val TAG = "HivePluginLifecycle"
        const val DEFAULT_MESH_ID = "WEARTAK"
        const val DEFAULT_CELL_ID = "ALPHA"

        /**
         * NATO phonetic alphabet for cell naming.
         * Cells are organizational units within a mesh (squads/teams).
         */
        val NATO_PHONETIC_CELLS = listOf(
            "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT",
            "GOLF", "HOTEL", "INDIA", "JULIET", "KILO", "LIMA",
            "MIKE", "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO",
            "SIERRA", "TANGO", "UNIFORM", "VICTOR", "WHISKEY",
            "XRAY", "YANKEE", "ZULU"
        )

        // Configuration defaults
        const val DEFAULT_CANNED_MESSAGE_TTL_SECONDS = 300  // 5 minutes

        @Volatile
        private var instance: HivePluginLifecycle? = null

        fun getInstance(): HivePluginLifecycle? = instance
    }

    private var hiveFfiInitialized = false
    private var hiveNodeJni: HiveNodeJni? = null

    // BLE mesh manager for WearTAK sync
    private var hiveBleManager: HiveBleManager? = null

    // Current cell assignment (organizational unit within mesh)
    @Volatile
    private var currentCellId: String = DEFAULT_CELL_ID

    init {
        instance = this
        val pluginContext = serviceController.getService(PluginContextProvider::class.java).pluginContext

        // Initialize native library loader
        HiveNativeLoader.init(pluginContext)

        // Load hive-ffi native library
        try {
            HiveNativeLoader.loadLibrary("hive_ffi")
            Log.i(TAG, "hive-ffi native library loaded via System.load()")

            // Register JNI native methods (required due to Android namespace isolation)
            if (HiveJni.initNatives()) {
                // Test JNI bindings (bypasses JNA which has symbol lookup issues)
                if (HiveJni.test()) {
                    hiveFfiInitialized = true
                    val version = HiveJni.hiveVersion()
                    Log.i(TAG, "HIVE JNI bindings working! Version: $version")

                    // Create HIVE node for P2P sync
                    createHiveNodeJni(pluginContext)
                } else {
                    Log.e(TAG, "JNI bindings test failed")
                    hiveFfiInitialized = false
                }
            } else {
                Log.e(TAG, "Failed to register JNI native methods")
                hiveFfiInitialized = false
            }
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load hive-ffi native library: ${e.message}", e)
            hiveFfiInitialized = false
        } catch (e: Exception) {
            Log.e(TAG, "Error initializing hive-ffi: ${e.message}", e)
            hiveFfiInitialized = false
        }

        Log.i(TAG, "HIVE Plugin initialized (FFI: $hiveFfiInitialized)")

        // Initialize BLE mesh for WearTAK sync
        initBleManager(pluginContext)
    }

    private fun initBleManager(context: Context) {
        // ADR-039 Migration: Check if unified transport handles BLE
        // If the hive-ffi node was created with enableBle=true, we don't need
        // the deprecated HiveBleManager. However, during the transition period,
        // we keep HiveBleManager as a fallback for Android BLE adapter integration.
        val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
        val unifiedBleEnabled = prefs.getBoolean("enable_ble", true)

        // For now, still initialize HiveBleManager as fallback since Android
        // BLE adapter callbacks in hive-btle are not yet complete.
        // TODO(#558): Remove this once Android BLE adapter integration is complete
        // and unified transport fully handles BLE on Android.
        try {
            // Get mesh ID from preferences, system properties, or use default
            val meshId = prefs.getString("mesh_id", null)
                ?: System.getProperty("hive.mesh_id")
                ?: System.getenv("HIVE_MESH_ID")
                ?: DEFAULT_MESH_ID

            // Load cell ID from preferences (organizational unit within mesh)
            currentCellId = prefs.getString("cell_id", null)
                ?: DEFAULT_CELL_ID
            Log.i(TAG, "Cell assignment: $currentCellId (mesh: $meshId)")

            @Suppress("DEPRECATION")
            hiveBleManager = HiveBleManager(context, meshId)

            if (hiveBleManager?.hasPermissions() == true) {
                val started = hiveBleManager?.start() ?: false
                Log.i(TAG, "HIVE BLE mesh started (fallback): $started [unified BLE requested: $unifiedBleEnabled]")

                // Bridge BLE peer discovery to Rust TransportManager (ADR-047)
                // This makes PACE routing aware of BLE-reachable peers
                hiveBleManager?.setPeerEventCallback { peer, _ ->
                    try {
                        val nodeId = peer.nodeId
                        if (nodeId != null) {
                            val peerId = String.format("%08X", nodeId)
                            if (peer.isConnected) {
                                hiveNodeJni?.bleAddPeer(peerId)
                            } else {
                                hiveNodeJni?.bleRemovePeer(peerId)
                            }
                        }
                    } catch (e: Exception) {
                        Log.w(TAG, "Error bridging BLE peer event to Rust: ${e.message}")
                    }
                }
            } else {
                Log.w(TAG, "BLE permissions not granted - mesh not started. " +
                    "Required: ${hiveBleManager?.getRequiredPermissions()?.joinToString()}")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize BLE manager: ${e.message}", e)
        }
    }

    private fun createHiveNodeJni(context: Context) {
        try {
            // IMPORTANT: Clean up any existing node before creating a new one.
            // This prevents database lock issues when plugin reloads without ATAK restart.
            if (hiveNodeJni != null) {
                Log.i(TAG, "Destroying existing HIVE node before creating new one")
                try {
                    hiveNodeJni?.close()
                } catch (e: Exception) {
                    Log.w(TAG, "Error closing previous node: ${e.message}")
                }
                hiveNodeJni = null
            }

            // Create storage directory for HIVE data
            // CRITICAL: redb uses mmap which DOES NOT work on Android's FUSE-mounted
            // external storage (/storage/emulated/0/). We MUST use internal app storage.

            // Use ATAK's internal data directory: /data/user/0/com.atakmap.app.civ/files/hive
            // This is NOT the sdcard path - it's the app's private internal storage
            val hiveDir = File("/data/user/0/com.atakmap.app.civ/files/hive")
            if (!hiveDir.exists()) {
                val created = hiveDir.mkdirs()
                Log.d(TAG, "Created ATAK internal files/hive dir: $created")
            }

            Log.d(TAG, "HIVE dir: ${hiveDir.absolutePath}")
            Log.d(TAG, "HIVE dir exists: ${hiveDir.exists()}, writable: ${hiveDir.canWrite()}, readable: ${hiveDir.canRead()}")

            // Get HIVE formation credentials from system properties or defaults
            val appId = System.getProperty("hive.app_id")
                ?: System.getenv("HIVE_APP_ID")
                ?: "default-atak-formation"

            val sharedKey = System.getProperty("hive.shared_key")
                ?: System.getenv("HIVE_SHARED_KEY")
                ?: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" // 32 zero bytes base64

            // Get BLE configuration from preferences
            val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
            val enableBle = prefs.getBoolean("enable_ble", true) // Enable BLE by default (ADR-039)
            val blePowerProfile = prefs.getString("ble_power_profile", "balanced")

            Log.d(TAG, "Using HIVE formation: $appId")
            Log.d(TAG, "Creating HIVE node with storage: ${hiveDir.absolutePath}, BLE: $enableBle")

            // Use unified transport with BLE enabled (ADR-039, #558)
            // This integrates BLE as a transport within hive-ffi rather than running
            // parallel BLE and Iroh meshes.
            hiveNodeJni = HiveNodeJni.createWithConfig(
                appId,
                sharedKey,
                hiveDir.absolutePath,
                enableBle = enableBle,
                blePowerProfile = blePowerProfile
            )

            if (hiveNodeJni != null) {
                val nodeId = hiveNodeJni?.nodeId() ?: "unknown"
                Log.i(TAG, "HIVE node created - ID: ${nodeId.take(16)}... (unified transport, BLE: $enableBle)")

                // Signal BLE transport as started if BLE is enabled (ADR-047)
                if (enableBle) {
                    try {
                        hiveNodeJni?.bleSetStarted(true)
                        Log.i(TAG, "BLE transport signaled as started for PACE routing")
                    } catch (e: Exception) {
                        Log.w(TAG, "Failed to signal BLE started (may not be compiled with bluetooth feature): ${e.message}")
                    }
                }

                // Start sync
                val syncStarted = hiveNodeJni?.startSync() ?: false
                Log.i(TAG, "HIVE sync started: $syncStarted, peer count: ${hiveNodeJni?.peerCount() ?: 0}")
            } else {
                Log.e(TAG, "Failed to create HIVE node (returned null)")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create HIVE node: ${e.message}", e)
        }
    }

    fun isHiveFfiAvailable(): Boolean = hiveFfiInitialized

    fun getHiveNodeJni(): HiveNodeJni? {
        // First check our instance
        if (hiveNodeJni != null) {
            return hiveNodeJni
        }
        // Try to recover from global singleton (survives APK replacement)
        // Wrapped in try-catch because native library may not be loaded
        try {
            val recovered = HiveNodeJni.getInstance()
            if (recovered != null) {
                Log.i(TAG, "Recovered HIVE node from global singleton")
                hiveNodeJni = recovered
            }
        } catch (e: UnsatisfiedLinkError) {
            // Native library not loaded - hive-ffi not available
            Log.d(TAG, "HIVE FFI native library not available")
        } catch (e: Exception) {
            Log.w(TAG, "Error recovering HIVE node: ${e.message}")
        }
        return hiveNodeJni
    }

    fun getPeerCount(): Int = getHiveNodeJni()?.peerCount() ?: 0

    fun getNodeId(): String? = getHiveNodeJni()?.nodeId()

    fun getConnectedPeers(): String = getHiveNodeJni()?.connectedPeers() ?: "[]"

    // ========================================================================
    // BLE Mesh Accessors
    // ========================================================================

    fun getHiveBleManager(): HiveBleManager? = hiveBleManager

    fun isBleAvailable(): Boolean = hiveBleManager?.isRunning?.value == true

    fun getBlePeerCount(): Int = hiveBleManager?.connectedPeerCount?.value ?: 0

    fun startBleMesh(): Boolean {
        return hiveBleManager?.start() ?: false
    }

    fun stopBleMesh() {
        hiveBleManager?.stop()
        // Signal BLE transport stopped to Rust TransportManager (ADR-047)
        try {
            hiveNodeJni?.bleSetStarted(false)
        } catch (_: Exception) { }
    }

    fun getCurrentMeshId(): String {
        return hiveBleManager?.meshId ?: DEFAULT_MESH_ID
    }

    /**
     * Get the current cell ID (organizational unit within the mesh).
     * Cells use NATO phonetic names: ALPHA, BRAVO, CHARLIE, etc.
     */
    fun getCurrentCellId(): String {
        return currentCellId
    }

    /**
     * Set the current cell ID.
     * @param cellId Must be a valid NATO phonetic name from NATO_PHONETIC_CELLS
     */
    fun setCurrentCellId(context: Context, cellId: String) {
        if (cellId !in NATO_PHONETIC_CELLS) {
            Log.w(TAG, "Invalid cell ID: $cellId. Must be NATO phonetic (ALPHA, BRAVO, etc.)")
            return
        }
        Log.i(TAG, "Changing cell from $currentCellId to $cellId")
        currentCellId = cellId

        // Persist to preferences
        val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
        prefs.edit().putString("cell_id", cellId).apply()
    }

    /**
     * Get list of available cell names (NATO phonetic alphabet).
     */
    fun getAvailableCells(): List<String> = NATO_PHONETIC_CELLS

    fun setMeshId(context: Context, meshId: String) {
        // Save to preferences
        val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
        prefs.edit().putString("mesh_id", meshId).apply()

        // Fully destroy old BLE mesh before creating new one
        Log.i(TAG, "Changing mesh ID from ${hiveBleManager?.meshId} to: $meshId")
        hiveBleManager?.destroy()  // Use destroy() not stop() to fully clean up
        hiveBleManager = null

        // Small delay to ensure BLE stack cleans up
        Thread.sleep(500)

        // Create and start new mesh with new ID
        hiveBleManager = HiveBleManager(context, meshId)
        if (hiveBleManager?.hasPermissions() == true) {
            val started = hiveBleManager?.start() ?: false
            Log.i(TAG, "New BLE mesh started: $started with meshId: $meshId")
        }
    }

    // ==================== HIVE Configuration Settings ====================

    /**
     * Get the canned message document TTL in seconds.
     * Controls how long ACK-tracked messages are kept in memory.
     */
    fun getCannedMessageTtlSeconds(context: Context): Int {
        val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
        return prefs.getInt("canned_message_ttl_seconds", DEFAULT_CANNED_MESSAGE_TTL_SECONDS)
    }

    /**
     * Set the canned message document TTL in seconds.
     * Changes take effect immediately without restart.
     */
    fun setCannedMessageTtlSeconds(context: Context, ttlSeconds: Int) {
        val prefs = context.getSharedPreferences("hive_prefs", Context.MODE_PRIVATE)
        prefs.edit().putInt("canned_message_ttl_seconds", ttlSeconds).apply()
        Log.i(TAG, "[CONFIG] Canned message TTL set to ${ttlSeconds}s")

        // Notify listeners of config change (if any components need immediate update)
        onConfigChanged?.invoke("canned_message_ttl_seconds", ttlSeconds)
    }

    // Callback for config changes (optional - components can register to receive updates)
    var onConfigChanged: ((key: String, value: Any) -> Unit)? = null
}
