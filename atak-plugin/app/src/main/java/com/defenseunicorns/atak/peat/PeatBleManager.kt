/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.defenseunicorns.peat.PeatBtle
import com.defenseunicorns.peat.PeatDocument
import com.defenseunicorns.peat.PeatEventType
import com.defenseunicorns.peat.PeatLocation
import com.defenseunicorns.peat.PeatMarker
import com.defenseunicorns.peat.PeatMeshListener
import com.defenseunicorns.peat.PeatPeer
import uniffi.peat_btle.PeerConnectionState
import uniffi.peat_btle.StateCountSummary
import uniffi.peat_btle.DeviceIdentity
import uniffi.peat_btle.decodeMeshGenesis

import uniffi.peat_lite_android.CannedMessageAckEventData
import uniffi.peat_lite_android.CannedMessageType
import uniffi.peat_lite_android.createCannedMessageAckEvent
import uniffi.peat_lite_android.encodeCannedMessageAckEvent
import uniffi.peat_lite_android.decodeCannedMessageAckEvent
import uniffi.peat_lite_android.cannedMessageAckEventMerge

/**
 * Manages PEAT BLE mesh connectivity for the ATAK plugin.
 *
 * This provides BLE mesh transport to sync with WearTAK devices and other
 * PEAT-enabled platforms without requiring network infrastructure.
 *
 * Features:
 * - Automatic peer discovery via BLE scanning
 * - Mesh document sync (health, status, events)
 * - Emergency/ACK event propagation
 * - Power-efficient operation (configurable duty cycle)
 *
 * Note: Uses simple callbacks instead of coroutines to avoid dependency
 * conflicts with ATAK's bundled kotlinx.coroutines.
 *
 * @deprecated This class is deprecated and will be removed in a future release.
 *             Use [PeatNodeJni.createWithConfig] with `enableBle=true` instead for
 *             unified multi-transport operation. The unified approach (ADR-039, #558)
 *             integrates BLE as a transport within peat-ffi rather than running
 *             parallel BLE and Iroh meshes.
 *
 *             Migration path:
 *             1. Replace `PeatBleManager` usage with `PeatNodeJni.createWithConfig(enableBle=true)`
 *             2. BLE peer events will flow through the unified transport manager
 *             3. Document sync happens automatically via Automerge ↔ peat-btle translation
 *
 *             Current status: PeatBleManager continues to work during the transition
 *             period while Android BLE adapter integration is completed in peat-btle.
 */
@Deprecated(
    message = "Use PeatNodeJni.createWithConfig(enableBle=true) for unified BLE transport (ADR-039)",
    replaceWith = ReplaceWith("PeatNodeJni.createWithConfig(appId, sharedKey, storagePath, enableBle = true)")
)
class PeatBleManager(
    private val context: Context,
    private val configuredMeshId: String = "WEARTAK"
) : PeatMeshListener {

    /**
     * The mesh name for display purposes (e.g., "WEARTAK").
     * This is the human-friendly name, not the internal genesis ID.
     */
    val meshId: String = configuredMeshId

    companion object {
        private const val TAG = "PeatBleManager"

        /**
         * Shared genesis for WEARTAK encrypted mesh.
         * All nodes (WearOS, ATAK Plugin) must use this same genesis.
         */
        private const val SHARED_GENESIS_BASE64: String =
            "BwBXRUFSVEFL4O7thA03dXXBNkT+gG22aTRGICECcX5RHtOgIdLBrb7tU7LTxkFLCLP+De21IALSXAbi6ZR/c3VXW9lKWacbM0YqfK9n5JXqob7/stIM63nBMLzJiFTGl9E6wcF8Gz0gUerY2JsBAAAA"

        @Volatile
        private var instance: PeatBleManager? = null

        fun getInstance(): PeatBleManager? = instance
    }

    // BLE mesh instance
    private var peatBtle: PeatBtle? = null

    // Main thread handler for callbacks
    private val mainHandler = Handler(Looper.getMainLooper())

    // State (using simple observables instead of StateFlow to avoid coroutines dependency)
    @Volatile
    private var _isRunning: Boolean = false
    val isRunning: SimpleObservable<Boolean> = SimpleObservable(false)

    @Volatile
    private var _peers: List<PeatPeer> = emptyList()
    val peers: SimpleObservable<List<PeatPeer>> = SimpleObservable(emptyList())

    @Volatile
    private var _connectedPeerCount: Int = 0
    val connectedPeerCount: SimpleObservable<Int> = SimpleObservable(0)

    // Callbacks for ATAK integration
    private var peerEventCallback: ((PeatPeer, PeatEventType) -> Unit)? = null
    private var documentSyncCallback: ((PeatDocument) -> Unit)? = null
    private var markerSyncCallback: ((PeatPeer, PeatMarker) -> Unit)? = null
    private var cannedMessageCallback: ((CannedMessageAckEventData) -> Unit)? = null

    // Storage for canned message documents (key = sourceNode:timestamp)
    private val cannedMessageDocs = mutableMapOf<String, CannedMessageAckEventData>()

    // Sequence counter for outgoing canned messages
    @Volatile
    private var cannedMessageSequence: UInt = 0u

    // Observable for canned message updates (for UI refresh)
    val cannedMessages: SimpleObservable<List<CannedMessageAckEventData>> = SimpleObservable(emptyList())

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
            // Decode shared genesis for encrypted mesh
            val genesisBytes = android.util.Base64.decode(SHARED_GENESIS_BASE64, android.util.Base64.NO_WRAP)
            val genesis = decodeMeshGenesis(genesisBytes)
            if (genesis == null) {
                Log.e(TAG, "Failed to decode shared genesis!")
                return false
            }

            // Generate or load device identity
            val identity = DeviceIdentity.generate()
            val genesisId = genesis.getMeshId()

            Log.i(TAG, "Starting PEAT BLE mesh: $meshId (genesis: $genesisId)")

            peatBtle = PeatBtle(
                context = context,
                meshId = genesisId,  // Use genesis ID for BLE discovery
                identity = identity,
                genesis = genesis
            ).apply {
                init()
                startMesh(this@PeatBleManager)
            }

            _isRunning = true
            isRunning.value = true
            Log.i(TAG, "PEAT BLE mesh started - nodeId: ${peatBtle?.nodeId}, mesh: $meshId")
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

        Log.i(TAG, "Stopping PEAT BLE mesh")

        try {
            peatBtle?.stopMesh()
        } catch (e: Exception) {
            Log.w(TAG, "Error stopping mesh: ${e.message}")
        }

        peatBtle = null
        _isRunning = false
        isRunning.value = false
        _peers = emptyList()
        peers.value = emptyList()
        _connectedPeerCount = 0
        connectedPeerCount.value = 0

        Log.i(TAG, "PEAT BLE mesh stopped")
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
    fun getNodeId(): Long? = peatBtle?.nodeId

    /**
     * Get the internal PeatMesh instance for direct access.
     */
    fun getMesh() = peatBtle?.mesh

    /**
     * Get cached callsign for a node ID (from persisted callsign-to-nodeId mapping).
     * Returns null if no callsign has been received for this node.
     */
    fun getCachedCallsign(nodeId: Long): String? = peatBtle?.getCachedCallsign(nodeId)

    // ========================================================================
    // Connection State Graph API (via PeatMesh)
    // ========================================================================

    /**
     * Get all peers currently in Connected state.
     */
    fun getConnectedPeers(): List<uniffi.peat_btle.PeatPeer> =
        peatBtle?.mesh?.getConnectedPeers() ?: emptyList()

    /**
     * Get all peers in Degraded state (low RSSI).
     */
    fun getDegradedPeers(): List<PeerConnectionState> =
        peatBtle?.mesh?.getDegradedPeers() ?: emptyList()

    /**
     * Get all peers in Lost state (disconnected and not seen for timeout period).
     */
    fun getLostPeers(): List<PeerConnectionState> =
        peatBtle?.mesh?.getLostPeers() ?: emptyList()

    /**
     * Get connection state for a specific peer.
     */
    fun getPeerConnectionState(nodeId: Long): PeerConnectionState? =
        peatBtle?.mesh?.getPeerConnectionState(nodeId.toUInt())

    /**
     * Get summary counts of peers in each connection state (for UI badges).
     */
    fun getConnectionStateCounts(): StateCountSummary? =
        peatBtle?.mesh?.getConnectionStateCounts()

    /**
     * Send an event to all peers.
     */
    fun sendEvent(eventType: PeatEventType) {
        peatBtle?.sendEvent(eventType)
    }

    /**
     * Broadcast position and callsign to all BLE mesh peers.
     * This is critical for watches to see the tablet's callsign.
     *
     * @param lat Latitude in degrees
     * @param lon Longitude in degrees
     * @param alt Altitude in meters (HAE)
     * @param callsign Device callsign
     * @param battery Battery percentage (0-100)
     */
    fun broadcastPosition(lat: Double, lon: Double, alt: Double, callsign: String, battery: Int = 100) {
        // Set Bluetooth adapter name to callsign so it appears in BLE advertisements
        // Only set once per session (when callsign is first known)
        if (!bluetoothNameSet && callsign.isNotBlank()) {
            setBluetoothName(callsign)
        }

        val location = PeatLocation(
            latitude = lat.toFloat(),
            longitude = lon.toFloat(),
            altitude = alt.toFloat()
        )
        peatBtle?.sendEvent(
            eventType = PeatEventType.NONE,
            location = location,
            callsign = callsign,
            battery = battery,
            heartRate = null
        )
        Log.d(TAG, "Broadcast position: callsign=$callsign, lat=$lat, lon=$lon")
    }

    // Track if we've set the Bluetooth name this session
    @Volatile
    private var bluetoothNameSet = false

    /**
     * Set the Bluetooth adapter name to the callsign.
     * This makes the device identifiable in BLE scanners like nRF Connect.
     */
    private fun setBluetoothName(callsign: String) {
        try {
            val bluetoothManager = context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
            val bluetoothAdapter = bluetoothManager.adapter
            if (bluetoothAdapter != null && bluetoothAdapter.name != callsign) {
                val success = bluetoothAdapter.setName(callsign)
                if (success) {
                    Log.i(TAG, "Set Bluetooth adapter name to callsign: $callsign")
                    bluetoothNameSet = true
                } else {
                    Log.w(TAG, "Failed to set Bluetooth adapter name to: $callsign")
                }
            } else if (bluetoothAdapter?.name == callsign) {
                bluetoothNameSet = true  // Already set correctly
            }
        } catch (e: SecurityException) {
            Log.w(TAG, "Missing permission to set Bluetooth name: ${e.message}")
        } catch (e: Exception) {
            Log.w(TAG, "Error setting Bluetooth name: ${e.message}")
        }
    }

    /**
     * Send an emergency alert.
     */
    fun sendEmergency() {
        sendEvent(PeatEventType.EMERGENCY)
    }

    /**
     * Acknowledge an emergency from a peer.
     */
    fun acknowledgeEmergency() {
        sendEvent(PeatEventType.ACK)
    }

    /**
     * Set callback for peer events (emergency, ack, etc).
     */
    fun setPeerEventCallback(callback: ((PeatPeer, PeatEventType) -> Unit)?) {
        peerEventCallback = callback
    }

    /**
     * Set callback for document sync events.
     */
    fun setDocumentSyncCallback(callback: ((PeatDocument) -> Unit)?) {
        documentSyncCallback = callback
    }

    /**
     * Set callback for marker sync events.
     */
    fun setMarkerSyncCallback(callback: ((PeatPeer, PeatMarker) -> Unit)?) {
        markerSyncCallback = callback
    }

    /**
     * Send a marker to all connected peers.
     */
    fun sendMarker(marker: PeatMarker) {
        peatBtle?.sendMarker(marker)
    }

    /**
     * Set callback for canned message events.
     * Called when a canned message document is received and merged.
     */
    fun setCannedMessageCallback(callback: ((CannedMessageAckEventData) -> Unit)?) {
        cannedMessageCallback = callback
    }

    // ========================================================================
    // Canned Message API
    // ========================================================================

    /**
     * Send a canned message to all connected peers.
     *
     * Creates a CannedMessageAckEvent document, encodes it, and broadcasts
     * to the mesh. The source node automatically ACKs the message.
     *
     * @param messageType The type of canned message to send
     * @param targetNode Optional target node ID (0 for broadcast to all)
     * @return The sent message document, or null if mesh not running
     */
    fun sendCannedMessage(messageType: CannedMessageType, targetNode: UInt = 0u): CannedMessageAckEventData? {
        val nodeId = peatBtle?.nodeId ?: run {
            Log.w(TAG, "Cannot send canned message: mesh not running")
            return null
        }

        val timestamp = System.currentTimeMillis().toULong()
        val sequence = cannedMessageSequence++

        // Create the event document (source auto-ACKs)
        val event = createCannedMessageAckEvent(
            messageType,
            nodeId.toUInt(),
            targetNode,
            timestamp,
            sequence
        )

        // Encode to wire format
        val encoded = encodeCannedMessageAckEvent(event)

        // Store locally
        val key = "${event.sourceNode}:${event.timestamp}"
        cannedMessageDocs[key] = event
        updateCannedMessagesObservable()

        Log.i(TAG, "Sent canned message: ${messageType.name} from ${String.format("%08X", nodeId)} (${encoded.size} bytes)")

        // Notify callback
        cannedMessageCallback?.invoke(event)

        return event
    }

    /**
     * Get all stored canned message documents, sorted by timestamp (newest first).
     */
    fun getCannedMessages(): List<CannedMessageAckEventData> {
        return cannedMessageDocs.values.sortedByDescending { it.timestamp }
    }

    /**
     * Clear all stored canned message documents.
     */
    fun clearCannedMessages() {
        cannedMessageDocs.clear()
        updateCannedMessagesObservable()
    }

    private fun updateCannedMessagesObservable() {
        cannedMessages.value = cannedMessageDocs.values.sortedByDescending { it.timestamp }
    }

    // ========================================================================
    // PeatMeshListener implementation
    // ========================================================================

    override fun onMeshUpdated(peers: List<PeatPeer>) {
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

    override fun onPeerEvent(peer: PeatPeer, eventType: PeatEventType) {
        mainHandler.post {
            Log.i(TAG, "Peer event: ${peer.displayName()} -> $eventType")

            when (eventType) {
                PeatEventType.EMERGENCY -> {
                    Log.w(TAG, "EMERGENCY received from ${peer.displayName()}")
                }
                PeatEventType.ACK -> {
                    Log.i(TAG, "ACK received from ${peer.displayName()}")
                }
                else -> {}
            }

            peerEventCallback?.invoke(peer, eventType)
        }
    }

    override fun onDocumentSynced(document: PeatDocument) {
        mainHandler.post {
            Log.d(TAG, "Document synced: nodeId=${document.nodeId}, version=${document.version}")
            documentSyncCallback?.invoke(document)

            // Poll for CannedMessages that arrived via delta sync
            pollCannedMessagesFromDeltaSync()
        }
    }

    /**
     * Poll for CannedMessages that arrived via delta sync (0xB2/0xC0 documents).
     * Delta-synced documents are stored in Rust and need to be polled since they
     * don't come through onDecryptedData callback.
     */
    private fun pollCannedMessagesFromDeltaSync() {
        // TODO: Re-enable when getAllCannedMessages/storeCannedMessageDocument land in peat-btle
    }

    override fun onMarkerSynced(peer: PeatPeer, marker: PeatMarker) {
        mainHandler.post {
            Log.i(TAG, "Marker synced from ${peer.displayName()}: ${marker.uid} " +
                    "type=${marker.type} at (${marker.lat}, ${marker.lon}) callsign=${marker.callsign}")
            markerSyncCallback?.invoke(peer, marker)
        }
    }

    override fun onDecryptedData(peer: PeatPeer?, data: ByteArray) {
        // CannedMessages now flow through delta sync only (0xB2/0xC0 documents)
        // No longer handling raw 0xAF broadcasts here
        if (data.isNotEmpty()) {
            Log.v(TAG, "[DECRYPTED] Received ${data.size} bytes, marker=0x${String.format("%02X", data[0])}")
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
