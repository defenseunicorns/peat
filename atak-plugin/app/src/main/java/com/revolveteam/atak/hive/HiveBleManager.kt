package com.revolveteam.atak.hive

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.revolveteam.hive.HiveBtle
import com.revolveteam.hive.HiveDocument
import com.revolveteam.hive.HiveEventType
import com.revolveteam.hive.HiveMeshListener
import com.revolveteam.hive.HivePeer

/**
 * Manages HIVE BLE mesh connectivity for the ATAK plugin.
 *
 * This provides BLE mesh transport to sync with WearTAK devices and other
 * HIVE-enabled platforms without requiring network infrastructure.
 *
 * Features:
 * - Automatic peer discovery via BLE scanning
 * - Mesh document sync (health, status, events)
 * - Emergency/ACK event propagation
 * - Power-efficient operation (configurable duty cycle)
 *
 * Note: Uses simple callbacks instead of coroutines to avoid dependency
 * conflicts with ATAK's bundled kotlinx.coroutines.
 */
class HiveBleManager(
    private val context: Context,
    val meshId: String = "WEARTAK"
) : HiveMeshListener {

    companion object {
        private const val TAG = "HiveBleManager"

        @Volatile
        private var instance: HiveBleManager? = null

        fun getInstance(): HiveBleManager? = instance
    }

    // BLE mesh instance
    private var hiveBtle: HiveBtle? = null

    // Main thread handler for callbacks
    private val mainHandler = Handler(Looper.getMainLooper())

    // State (using simple observables instead of StateFlow to avoid coroutines dependency)
    @Volatile
    private var _isRunning: Boolean = false
    val isRunning: SimpleObservable<Boolean> = SimpleObservable(false)

    @Volatile
    private var _peers: List<HivePeer> = emptyList()
    val peers: SimpleObservable<List<HivePeer>> = SimpleObservable(emptyList())

    @Volatile
    private var _connectedPeerCount: Int = 0
    val connectedPeerCount: SimpleObservable<Int> = SimpleObservable(0)

    // Callbacks for ATAK integration
    private var peerEventCallback: ((HivePeer, HiveEventType) -> Unit)? = null
    private var documentSyncCallback: ((HiveDocument) -> Unit)? = null

    init {
        instance = this
    }

    /**
     * Check if BLE permissions are granted.
     */
    fun hasPermissions(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Android 12+
            context.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN) == PackageManager.PERMISSION_GRANTED &&
            context.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED &&
            context.checkSelfPermission(Manifest.permission.BLUETOOTH_ADVERTISE) == PackageManager.PERMISSION_GRANTED
        } else {
            // Android 8-11
            context.checkSelfPermission(Manifest.permission.BLUETOOTH) == PackageManager.PERMISSION_GRANTED &&
            context.checkSelfPermission(Manifest.permission.BLUETOOTH_ADMIN) == PackageManager.PERMISSION_GRANTED
        }
    }

    /**
     * Get required permissions for the current Android version.
     */
    fun getRequiredPermissions(): Array<String> {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
                Manifest.permission.BLUETOOTH_ADVERTISE
            )
        } else {
            arrayOf(
                Manifest.permission.BLUETOOTH,
                Manifest.permission.BLUETOOTH_ADMIN
            )
        }
    }

    /**
     * Start the BLE mesh.
     *
     * @return true if started successfully
     */
    fun start(): Boolean {
        if (_isRunning) {
            Log.w(TAG, "BLE mesh already running")
            return true
        }

        if (!hasPermissions()) {
            Log.e(TAG, "BLE permissions not granted")
            return false
        }

        return try {
            Log.i(TAG, "Starting HIVE BLE mesh (meshId: $meshId)")

            hiveBtle = HiveBtle(context, meshId = meshId).apply {
                init()
                startMesh(this@HiveBleManager)
            }

            _isRunning = true
            isRunning.value = true
            Log.i(TAG, "HIVE BLE mesh started - nodeId: ${hiveBtle?.nodeId}")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start BLE mesh: ${e.message}", e)
            false
        }
    }

    /**
     * Stop the BLE mesh.
     */
    fun stop() {
        if (!_isRunning) {
            return
        }

        Log.i(TAG, "Stopping HIVE BLE mesh")

        try {
            hiveBtle?.stopMesh()
        } catch (e: Exception) {
            Log.w(TAG, "Error stopping mesh: ${e.message}")
        }

        hiveBtle = null
        _isRunning = false
        isRunning.value = false
        _peers = emptyList()
        peers.value = emptyList()
        _connectedPeerCount = 0
        connectedPeerCount.value = 0

        Log.i(TAG, "HIVE BLE mesh stopped")
    }

    /**
     * Clean up resources.
     */
    fun destroy() {
        stop()
        instance = null
    }

    /**
     * Get the local node ID.
     */
    fun getNodeId(): Long? = hiveBtle?.nodeId

    /**
     * Send an event to all peers.
     */
    fun sendEvent(eventType: HiveEventType) {
        hiveBtle?.sendEvent(eventType)
    }

    /**
     * Send an emergency alert.
     */
    fun sendEmergency() {
        sendEvent(HiveEventType.EMERGENCY)
    }

    /**
     * Acknowledge an emergency from a peer.
     */
    fun acknowledgeEmergency() {
        sendEvent(HiveEventType.ACK)
    }

    /**
     * Set callback for peer events (emergency, ack, etc).
     */
    fun setPeerEventCallback(callback: ((HivePeer, HiveEventType) -> Unit)?) {
        peerEventCallback = callback
    }

    /**
     * Set callback for document sync events.
     */
    fun setDocumentSyncCallback(callback: ((HiveDocument) -> Unit)?) {
        documentSyncCallback = callback
    }

    // ========================================================================
    // HiveMeshListener implementation
    // ========================================================================

    override fun onMeshUpdated(peers: List<HivePeer>) {
        mainHandler.post {
            _peers = peers
            this.peers.value = peers
            _connectedPeerCount = peers.count { it.isConnected }
            connectedPeerCount.value = _connectedPeerCount

            Log.d(TAG, "Mesh updated: ${peers.size} peers ($_connectedPeerCount connected) " +
                    "[mgr=${System.identityHashCode(this)}, obs=${System.identityHashCode(this.peers)}]")

            // Log peer details for debugging
            peers.forEach { peer ->
                Log.v(TAG, "  - ${peer.displayName()} [${if (peer.isConnected) "connected" else "discovered"}] RSSI: ${peer.rssi}")
            }
        }
    }

    override fun onPeerEvent(peer: HivePeer, eventType: HiveEventType) {
        mainHandler.post {
            Log.i(TAG, "Peer event: ${peer.displayName()} -> $eventType")

            when (eventType) {
                HiveEventType.EMERGENCY -> {
                    Log.w(TAG, "EMERGENCY received from ${peer.displayName()}")
                }
                HiveEventType.ACK -> {
                    Log.i(TAG, "ACK received from ${peer.displayName()}")
                }
                else -> {}
            }

            peerEventCallback?.invoke(peer, eventType)
        }
    }

    override fun onDocumentSynced(document: HiveDocument) {
        mainHandler.post {
            Log.d(TAG, "Document synced: nodeId=${document.nodeId}, version=${document.version}")
            documentSyncCallback?.invoke(document)
        }
    }
}

/**
 * Simple observable wrapper to replace StateFlow without coroutines dependency.
 * Allows UI to observe value changes via callbacks.
 */
class SimpleObservable<T>(initialValue: T) {
    private val listeners = mutableListOf<(T) -> Unit>()

    @Volatile
    var value: T = initialValue
        set(newValue) {
            if (field != newValue) {
                field = newValue
                listeners.forEach { it(newValue) }
            }
        }

    fun observe(listener: (T) -> Unit) {
        listeners.add(listener)
        // Immediately notify with current value
        listener(value)
    }

    fun removeObserver(listener: (T) -> Unit) {
        listeners.remove(listener)
    }
}
