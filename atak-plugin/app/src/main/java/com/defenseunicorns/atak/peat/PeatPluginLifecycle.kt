/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

import android.content.Context
import android.os.Environment
import android.util.Log
import com.atak.plugins.impl.AbstractPlugin
import com.atak.plugins.impl.PluginContextProvider
import com.atakmap.coremap.filesystem.FileSystemUtils
import gov.tak.api.plugin.IServiceController
import java.io.File

/**
 * Peat Plugin Lifecycle Manager
 *
 * Main entry point for the Peat ATAK plugin. Extends AbstractPlugin
 * as per ATAK SDK 5.6 pattern.
 *
 * Uses direct JNI bindings to bypass JNA/UniFFI symbol lookup issues
 * caused by Android's linker namespace isolation.
 */
class PeatPluginLifecycle(serviceController: IServiceController) : AbstractPlugin(
    serviceController,
    PeatTool(serviceController.getService(PluginContextProvider::class.java).pluginContext),
    PeatMapComponent()
) {
    companion object {
        private const val TAG = "PeatPluginLifecycle"
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
        private var instance: PeatPluginLifecycle? = null

        fun getInstance(): PeatPluginLifecycle? = instance
    }

    private var peatFfiInitialized = false
    private var peatNodeJni: PeatNodeJni? = null

    // BLE mesh manager for WearTAK sync
    private var peatBleManager: PeatBleManager? = null

    // Current cell assignment (organizational unit within mesh)
    @Volatile
    private var currentCellId: String = DEFAULT_CELL_ID

    init {
        instance = this
        val pluginContext = serviceController.getService(PluginContextProvider::class.java).pluginContext

        // Initialize native library loader
        PeatNativeLoader.init(pluginContext)

        // Load peat-ffi native library
        try {
            PeatNativeLoader.loadLibrary("peat_ffi")
            Log.i(TAG, "peat-ffi native library loaded via System.load()")

            // Register JNI native methods (required due to Android namespace isolation)
            if (PeatJni.initNatives()) {
                // Test JNI bindings (bypasses JNA which has symbol lookup issues)
                if (PeatJni.test()) {
                    peatFfiInitialized = true
                    val version = PeatJni.peatVersion()
                    Log.i(TAG, "Peat JNI bindings working! Version: $version")

                    // Create Peat node for P2P sync
                    createPeatNodeJni(pluginContext)
                } else {
                    Log.e(TAG, "JNI bindings test failed")
                    peatFfiInitialized = false
                }
            } else {
                Log.e(TAG, "Failed to register JNI native methods")
                peatFfiInitialized = false
            }
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load peat-ffi native library: ${e.message}", e)
            peatFfiInitialized = false
        } catch (e: Exception) {
            Log.e(TAG, "Error initializing peat-ffi: ${e.message}", e)
            peatFfiInitialized = false
        }

        Log.i(TAG, "Peat Plugin initialized (FFI: $peatFfiInitialized)")

        // Initialize BLE mesh for WearTAK sync
        initBleManager(pluginContext)
    }

    private fun initBleManager(context: Context) {
        // ADR-039 Migration: Check if unified transport handles BLE
        // If the peat-ffi node was created with enableBle=true, we don't need
        // the deprecated PeatBleManager. However, during the transition period,
        // we keep PeatBleManager as a fallback for Android BLE adapter integration.
        val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
        val unifiedBleEnabled = prefs.getBoolean("enable_ble", true)

        // M5 Migration: PeatBleManager is deprecated. Features will migrate to
        // PeatNodeJni unified transport. BLE mesh still runs via PeatBleManager
        // during the transition period until chat/markers/canned messages migrate.
        try {
            // Get mesh ID from preferences, system properties, or use default
            val meshId = prefs.getString("mesh_id", null)
                ?: System.getProperty("peat.mesh_id")
                ?: System.getenv("PEAT_MESH_ID")
                ?: DEFAULT_MESH_ID

            // Load cell ID from preferences (organizational unit within mesh)
            currentCellId = prefs.getString("cell_id", null)
                ?: DEFAULT_CELL_ID
            Log.i(TAG, "Cell assignment: $currentCellId (mesh: $meshId)")

            @Suppress("DEPRECATION")
            peatBleManager = PeatBleManager(context, meshId)

            if (peatBleManager?.hasPermissions() == true) {
                val started = peatBleManager?.start() ?: false
                Log.i(TAG, "Peat BLE mesh started (fallback): $started [unified BLE requested: $unifiedBleEnabled]")

                // Bridge BLE peer discovery to Rust TransportManager (ADR-047)
                // This makes PACE routing aware of BLE-reachable peers
                peatBleManager?.setPeerEventCallback { peer, _ ->
                    try {
                        val nodeId = peer.nodeId
                        if (nodeId != null) {
                            val peerId = String.format("%08X", nodeId)
                            if (peer.isConnected) {
                                peatNodeJni?.bleAddPeer(peerId)
                            } else {
                                peatNodeJni?.bleRemovePeer(peerId)
                            }
                        }
                    } catch (e: Exception) {
                        Log.w(TAG, "Error bridging BLE peer event to Rust: ${e.message}")
                    }
                }
            } else {
                Log.w(TAG, "BLE permissions not granted - mesh not started. " +
                    "Required: ${peatBleManager?.getRequiredPermissions()?.joinToString()}")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize BLE manager: ${e.message}", e)
        }
    }

    private fun createPeatNodeJni(context: Context) {
        try {
            // IMPORTANT: Clean up any existing node before creating a new one.
            // This prevents database lock issues when plugin reloads without ATAK restart.
            if (peatNodeJni != null) {
                Log.i(TAG, "Destroying existing Peat node before creating new one")
                try {
                    peatNodeJni?.close()
                } catch (e: Exception) {
                    Log.w(TAG, "Error closing previous node: ${e.message}")
                }
                peatNodeJni = null
            }

            // Create storage directory for Peat data
            // CRITICAL: redb uses mmap which DOES NOT work on Android's FUSE-mounted
            // external storage (/storage/emulated/0/). We MUST use internal app storage.

            // Use ATAK's internal data directory: /data/user/0/com.atakmap.app.civ/files/peat
            // This is NOT the sdcard path - it's the app's private internal storage
            val peatDir = File("/data/user/0/com.atakmap.app.civ/files/peat")
            if (!peatDir.exists()) {
                val created = peatDir.mkdirs()
                Log.d(TAG, "Created ATAK internal files/peat dir: $created")
            }

            Log.d(TAG, "Peat dir: ${peatDir.absolutePath}")
            Log.d(TAG, "Peat dir exists: ${peatDir.exists()}, writable: ${peatDir.canWrite()}, readable: ${peatDir.canRead()}")

            // Get Peat formation credentials from system properties or defaults
            val appId = System.getProperty("peat.app_id")
                ?: System.getenv("PEAT_APP_ID")
                ?: "default-atak-formation"

            val sharedKey = System.getProperty("peat.shared_key")
                ?: System.getenv("PEAT_SHARED_KEY")
                ?: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" // 32 zero bytes base64

            // Get BLE configuration from preferences
            val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
            val enableBle = prefs.getBoolean("enable_ble", true) // Enable BLE by default (ADR-039)
            val blePowerProfile = prefs.getString("ble_power_profile", "balanced")

            Log.d(TAG, "Using Peat formation: $appId")
            Log.d(TAG, "Creating Peat node with storage: ${peatDir.absolutePath}, BLE: $enableBle")

            // Use unified transport with BLE enabled (ADR-039, #558)
            // This integrates BLE as a transport within peat-ffi rather than running
            // parallel BLE and Iroh meshes.
            peatNodeJni = PeatNodeJni.createWithConfig(
                appId,
                sharedKey,
                peatDir.absolutePath,
                enableBle = enableBle,
                blePowerProfile = blePowerProfile
            )

            if (peatNodeJni != null) {
                val nodeId = peatNodeJni?.nodeId() ?: "unknown"
                Log.i(TAG, "Peat node created - ID: ${nodeId.take(16)}... (unified transport, BLE: $enableBle)")

                // Signal BLE transport as started if BLE is enabled (ADR-047)
                if (enableBle) {
                    try {
                        peatNodeJni?.bleSetStarted(true)
                        Log.i(TAG, "BLE transport signaled as started for PACE routing")
                    } catch (e: Exception) {
                        Log.w(TAG, "Failed to signal BLE started (may not be compiled with bluetooth feature): ${e.message}")
                    }
                }

                // Start sync
                val syncStarted = peatNodeJni?.startSync() ?: false
                Log.i(TAG, "Peat sync started: $syncStarted, peer count: ${peatNodeJni?.peerCount() ?: 0}")
            } else {
                Log.e(TAG, "Failed to create Peat node (returned null)")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create Peat node: ${e.message}", e)
        }
    }

    fun isPeatFfiAvailable(): Boolean = peatFfiInitialized

    fun getPeatNodeJni(): PeatNodeJni? {
        // First check our instance
        if (peatNodeJni != null) {
            return peatNodeJni
        }
        // Try to recover from global singleton (survives APK replacement)
        // Wrapped in try-catch because native library may not be loaded
        try {
            val recovered = PeatNodeJni.getInstance()
            if (recovered != null) {
                Log.i(TAG, "Recovered Peat node from global singleton")
                peatNodeJni = recovered
            }
        } catch (e: UnsatisfiedLinkError) {
            // Native library not loaded - peat-ffi not available
            Log.d(TAG, "Peat FFI native library not available")
        } catch (e: Exception) {
            Log.w(TAG, "Error recovering Peat node: ${e.message}")
        }
        return peatNodeJni
    }

    fun getPeerCount(): Int = getPeatNodeJni()?.peerCount() ?: 0

    fun getNodeId(): String? = getPeatNodeJni()?.nodeId()

    fun getConnectedPeers(): String = getPeatNodeJni()?.connectedPeers() ?: "[]"

    // ========================================================================
    // BLE Mesh Accessors
    // ========================================================================

    fun getPeatBleManager(): PeatBleManager? = peatBleManager

    fun isBleAvailable(): Boolean {
        // Prefer unified transport query via JNI (M5)
        try {
            val unified = peatNodeJni?.bleIsAvailable()
            if (unified == true) return true
        } catch (_: Exception) { }
        // Fall back to legacy PeatBleManager during transition
        return peatBleManager?.isRunning?.value == true
    }

    fun getBlePeerCount(): Int {
        // Prefer unified transport query via JNI (M5)
        try {
            val unified = peatNodeJni?.blePeerCount() ?: 0
            if (unified > 0) return unified
        } catch (_: Exception) { }
        // Fall back to legacy PeatBleManager during transition
        return peatBleManager?.connectedPeerCount?.value ?: 0
    }

    fun startBleMesh(): Boolean {
        return peatBleManager?.start() ?: false
    }

    fun stopBleMesh() {
        peatBleManager?.stop()
        // Signal BLE transport stopped to Rust TransportManager (ADR-047)
        try {
            peatNodeJni?.bleSetStarted(false)
        } catch (_: Exception) { }
    }

    fun getCurrentMeshId(): String {
        return peatBleManager?.meshId ?: DEFAULT_MESH_ID
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
        val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
        prefs.edit().putString("cell_id", cellId).apply()
    }

    /**
     * Get list of available cell names (NATO phonetic alphabet).
     */
    fun getAvailableCells(): List<String> = NATO_PHONETIC_CELLS

    fun setMeshId(context: Context, meshId: String) {
        // Save to preferences
        val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
        prefs.edit().putString("mesh_id", meshId).apply()

        // Fully destroy old BLE mesh before creating new one
        Log.i(TAG, "Changing mesh ID from ${peatBleManager?.meshId} to: $meshId")
        peatBleManager?.destroy()  // Use destroy() not stop() to fully clean up
        peatBleManager = null

        // Small delay to ensure BLE stack cleans up
        Thread.sleep(500)

        // Create and start new mesh with new ID
        peatBleManager = PeatBleManager(context, meshId)
        if (peatBleManager?.hasPermissions() == true) {
            val started = peatBleManager?.start() ?: false
            Log.i(TAG, "New BLE mesh started: $started with meshId: $meshId")
        }
    }

    // ==================== Peat Configuration Settings ====================

    /**
     * Get the canned message document TTL in seconds.
     * Controls how long ACK-tracked messages are kept in memory.
     */
    fun getCannedMessageTtlSeconds(context: Context): Int {
        val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
        return prefs.getInt("canned_message_ttl_seconds", DEFAULT_CANNED_MESSAGE_TTL_SECONDS)
    }

    /**
     * Set the canned message document TTL in seconds.
     * Changes take effect immediately without restart.
     */
    fun setCannedMessageTtlSeconds(context: Context, ttlSeconds: Int) {
        val prefs = context.getSharedPreferences("peat_prefs", Context.MODE_PRIVATE)
        prefs.edit().putInt("canned_message_ttl_seconds", ttlSeconds).apply()
        Log.i(TAG, "[CONFIG] Canned message TTL set to ${ttlSeconds}s")

        // Notify listeners of config change (if any components need immediate update)
        onConfigChanged?.invoke("canned_message_ttl_seconds", ttlSeconds)
    }

    // Callback for config changes (optional - components can register to receive updates)
    var onConfigChanged: ((key: String, value: Any) -> Unit)? = null
}
