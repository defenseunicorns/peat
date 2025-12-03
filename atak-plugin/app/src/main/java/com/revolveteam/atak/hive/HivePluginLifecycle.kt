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

        @Volatile
        private var instance: HivePluginLifecycle? = null

        fun getInstance(): HivePluginLifecycle? = instance
    }

    private var hiveFfiInitialized = false
    private var hiveNodeJni: HiveNodeJni? = null

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
    }

    private fun createHiveNodeJni(context: Context) {
        try {
            // Create storage directory for HIVE data
            // ATAK plugins must use ATAK's root directory (atak/) on external storage
            // The plugin context has no access to its own data directory

            var hiveDir: File? = null

            // Method 1: Use ATAK's FileSystemUtils to get the atak root
            try {
                val atakRoot = FileSystemUtils.getRoot()
                Log.d(TAG, "ATAK root from FileSystemUtils: $atakRoot")
                if (atakRoot != null && atakRoot.exists()) {
                    hiveDir = File(atakRoot, "plugins/hive")
                    if (!hiveDir.exists()) {
                        val created = hiveDir.mkdirs()
                        Log.d(TAG, "Created ATAK plugins/hive dir: $created")
                    }
                }
            } catch (e: Exception) {
                Log.w(TAG, "FileSystemUtils.getRoot() failed: ${e.message}")
            }

            // Method 2: Fall back to standard external storage atak directory
            if (hiveDir == null || !hiveDir.exists()) {
                val externalStorage = Environment.getExternalStorageDirectory()
                Log.d(TAG, "External storage: $externalStorage")
                val atakDir = File(externalStorage, "atak/plugins/hive")
                if (!atakDir.exists()) {
                    val created = atakDir.mkdirs()
                    Log.d(TAG, "Created external atak/plugins/hive dir: $created")
                }
                if (atakDir.exists()) {
                    hiveDir = atakDir
                }
            }

            // Method 3: Last resort - try /sdcard/atak directly
            if (hiveDir == null || !hiveDir.exists()) {
                hiveDir = File("/sdcard/atak/plugins/hive")
                if (!hiveDir.exists()) {
                    val created = hiveDir.mkdirs()
                    Log.d(TAG, "Created /sdcard/atak/plugins/hive dir: $created")
                }
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

            Log.d(TAG, "Using HIVE formation: $appId")
            Log.d(TAG, "Creating HIVE node with storage: ${hiveDir.absolutePath}")

            hiveNodeJni = HiveNodeJni.create(appId, sharedKey, hiveDir.absolutePath)

            if (hiveNodeJni != null) {
                val nodeId = hiveNodeJni?.nodeId() ?: "unknown"
                Log.i(TAG, "HIVE node created - ID: ${nodeId.take(16)}...")

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

    fun getHiveNodeJni(): HiveNodeJni? = hiveNodeJni

    fun getPeerCount(): Int = hiveNodeJni?.peerCount() ?: 0

    fun getNodeId(): String? = hiveNodeJni?.nodeId()
}
