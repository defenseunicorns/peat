package com.revolveteam.atak.hive

import android.util.Log

/**
 * Direct JNI bindings for HIVE FFI.
 *
 * This bypasses JNA/UniFFI which has symbol lookup issues on Android
 * due to linker namespace isolation. Uses standard JNI with native
 * method declarations that map directly to Rust #[no_mangle] exports.
 *
 * IMPORTANT: Call initNatives() after System.load() to register the native methods.
 * This is required because Android's classloader namespace isolation prevents
 * automatic JNI symbol lookup.
 */
object HiveJni {
    private const val TAG = "HiveJni"
    private var initialized = false

    /**
     * Initialize native methods by registering them via JNI RegisterNatives.
     * This MUST be called after System.load() and before any other native methods.
     *
     * @return true if initialization succeeded
     */
    fun initNatives(): Boolean {
        if (initialized) {
            Log.d(TAG, "Already initialized")
            return true
        }

        return try {
            Log.d(TAG, "Calling nativeInit to register native methods...")
            nativeInit()
            initialized = true
            Log.i(TAG, "Native methods registered successfully")
            true
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "nativeInit failed - UnsatisfiedLinkError: ${e.message}")
            false
        } catch (e: Exception) {
            Log.e(TAG, "nativeInit failed - Exception: ${e.message}")
            false
        }
    }

    /**
     * Native initialization function - registers all other native methods.
     * This is the ONLY function that can be found via standard JNI lookup
     * after System.load() due to Android namespace isolation.
     */
    @JvmStatic
    private external fun nativeInit()

    /**
     * Get HIVE library version string.
     */
    @JvmStatic
    external fun hiveVersion(): String

    /**
     * Test that JNI bindings work.
     */
    @JvmStatic
    external fun testJni(): String

    /**
     * Create a HIVE node and return its handle.
     * @param appId Formation/app identifier
     * @param sharedKey Base64-encoded shared key
     * @param storagePath Path for persistent storage
     * @return Handle (pointer) to the HiveNode, or 0 on failure
     */
    @JvmStatic
    external fun createNodeJni(appId: String, sharedKey: String, storagePath: String): Long

    /**
     * Get the node ID (hex-encoded public key) for a node handle.
     * @param handle Node handle from createNodeJni
     * @return Node ID string, or error message
     */
    @JvmStatic
    external fun nodeIdJni(handle: Long): String

    /**
     * Get the current peer count for a node.
     * @param handle Node handle from createNodeJni
     * @return Number of connected peers, or -1 on error
     */
    @JvmStatic
    external fun peerCountJni(handle: Long): Int

    /**
     * Get connected peer IDs as a JSON array.
     * @param handle Node handle from createNodeJni
     * @return JSON array of hex-encoded peer IDs, e.g. ["abc123...", "def456..."]
     */
    @JvmStatic
    external fun connectedPeersJni(handle: Long): String

    /**
     * Start sync for a node.
     * @param handle Node handle from createNodeJni
     * @return true if sync started successfully
     */
    @JvmStatic
    external fun startSyncJni(handle: Long): Boolean

    /**
     * Free a HiveNode handle.
     * Must be called when done with a node to avoid memory leaks.
     * @param handle Node handle from createNodeJni
     */
    @JvmStatic
    external fun freeNodeJni(handle: Long)

    /**
     * Test if JNI bindings are working.
     * @return true if JNI is functional
     */
    fun test(): Boolean {
        if (!initialized) {
            Log.e(TAG, "JNI test failed - not initialized. Call initNatives() first.")
            return false
        }
        return try {
            val version = hiveVersion()
            val testMsg = testJni()
            Log.i(TAG, "JNI test passed - Version: $version, Message: $testMsg")
            true
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "JNI test failed - UnsatisfiedLinkError: ${e.message}")
            false
        } catch (e: Exception) {
            Log.e(TAG, "JNI test failed - Exception: ${e.message}")
            false
        }
    }
}

/**
 * Wrapper class for a HIVE node using JNI.
 * Provides a more idiomatic Kotlin API over the raw JNI functions.
 */
class HiveNodeJni private constructor(private val handle: Long) : AutoCloseable {

    companion object {
        private const val TAG = "HiveNodeJni"

        /**
         * Create a new HIVE node.
         * @param appId Formation/app identifier
         * @param sharedKey Base64-encoded shared key
         * @param storagePath Path for persistent storage
         * @return HiveNodeJni instance, or null on failure
         */
        fun create(appId: String, sharedKey: String, storagePath: String): HiveNodeJni? {
            return try {
                val handle = HiveJni.createNodeJni(appId, sharedKey, storagePath)
                if (handle != 0L) {
                    Log.i(TAG, "Created HIVE node with handle: $handle")
                    HiveNodeJni(handle)
                } else {
                    Log.e(TAG, "Failed to create HIVE node (handle=0)")
                    null
                }
            } catch (e: Exception) {
                Log.e(TAG, "Exception creating HIVE node: ${e.message}", e)
                null
            }
        }
    }

    /**
     * Get this node's ID (hex-encoded public key).
     */
    fun nodeId(): String = HiveJni.nodeIdJni(handle)

    /**
     * Get the current number of connected peers.
     */
    fun peerCount(): Int = HiveJni.peerCountJni(handle)

    /**
     * Get connected peer IDs as a JSON array string.
     * @return JSON array of hex-encoded peer IDs
     */
    fun connectedPeers(): String = HiveJni.connectedPeersJni(handle)

    /**
     * Start P2P sync.
     * @return true if sync started successfully
     */
    fun startSync(): Boolean = HiveJni.startSyncJni(handle)

    /**
     * Free the native node resources.
     */
    override fun close() {
        Log.d(TAG, "Closing HIVE node handle: $handle")
        HiveJni.freeNodeJni(handle)
    }
}
