/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.atak.hive

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.revolveteam.hive.HiveBtle
import com.revolveteam.hive.HiveChat
import com.revolveteam.hive.HiveDocument
import com.revolveteam.hive.HiveEventType
import com.revolveteam.hive.HiveLocation
import com.revolveteam.hive.HiveMarker
import com.revolveteam.hive.HiveMeshListener
import com.revolveteam.hive.HivePeer
import uniffi.hive_btle.PeerConnectionState
import uniffi.hive_btle.StateCountSummary
import uniffi.hive_btle.DeviceIdentity
import uniffi.hive_btle.decodeMeshGenesis
import uniffi.hive_lite_android.CannedMessageAckEventData
import uniffi.hive_lite_android.CannedMessageType
import uniffi.hive_lite_android.createCannedMessageAckEvent
import uniffi.hive_lite_android.encodeCannedMessageAckEvent
import uniffi.hive_lite_android.decodeCannedMessageAckEvent
import uniffi.hive_lite_android.cannedMessageAckEventMerge

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
 *
 * @deprecated This class is deprecated and will be removed in a future release.
 *             Use [HiveNodeJni.createWithConfig] with `enableBle=true` instead for
 *             unified multi-transport operation. The unified approach (ADR-039, #558)
 *             integrates BLE as a transport within hive-ffi rather than running
 *             parallel BLE and Iroh meshes.
 *
 *             Migration path:
 *             1. Replace `HiveBleManager` usage with `HiveNodeJni.createWithConfig(enableBle=true)`
 *             2. BLE peer events will flow through the unified transport manager
 *             3. Document sync happens automatically via Automerge ↔ hive-btle translation
 *
 *             Current status: HiveBleManager continues to work during the transition
 *             period while Android BLE adapter integration is completed in hive-btle.
 */
@Deprecated(
    message = "Use HiveNodeJni.createWithConfig(enableBle=true) for unified BLE transport (ADR-039)",
    replaceWith = ReplaceWith("HiveNodeJni.createWithConfig(appId, sharedKey, storagePath, enableBle = true)")
)
class HiveBleManager(
    private val context: Context,
    private val configuredMeshId: String = "WEARTAK"
) : HiveMeshListener {

    /**
     * The mesh name for display purposes (e.g., "WEARTAK").
     * This is the human-friendly name, not the internal genesis ID.
     */
    val meshId: String = configuredMeshId

    companion object {
        private const val TAG = "HiveBleManager"

        /**
         * Shared genesis for WEARTAK encrypted mesh.
         * All nodes (WearOS, ATAK Plugin) must use this same genesis.
         */
        private const val SHARED_GENESIS_BASE64: String =
            "BwBXRUFSVEFL4O7thA03dXXBNkT+gG22aTRGICECcX5RHtOgIdLBrb7tU7LTxkFLCLP+De21IALSXAbi6ZR/c3VXW9lKWacbM0YqfK9n5JXqob7/stIM63nBMLzJiFTGl9E6wcF8Gz0gUerY2JsBAAAA"

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
    private var markerSyncCallback: ((HivePeer, HiveMarker) -> Unit)? = null
    private var chatSyncCallback: ((HiveChat, HivePeer) -> Unit)? = null
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

            Log.i(TAG, "Starting HIVE BLE mesh: $meshId (genesis: $genesisId)")

            hiveBtle = HiveBtle(
                context = context,
                meshId = genesisId,  // Use genesis ID for BLE discovery
                identity = identity,
                genesis = genesis
            ).apply {
                init()
                startMesh(this@HiveBleManager)
            }

            _isRunning = true
            isRunning.value = true
            Log.i(TAG, "HIVE BLE mesh started - nodeId: ${hiveBtle?.nodeId}, mesh: $meshId")
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
     * Get the internal HiveMesh instance for direct access.
     */
    fun getMesh() = hiveBtle?.mesh

    // ========================================================================
    // Connection State Graph API (via HiveMesh)
    // ========================================================================

    /**
     * Get all peers currently in Connected state.
     */
    fun getConnectedPeers(): List<uniffi.hive_btle.HivePeer> =
        hiveBtle?.mesh?.getConnectedPeers() ?: emptyList()

    /**
     * Get all peers in Degraded state (low RSSI).
     */
    fun getDegradedPeers(): List<PeerConnectionState> =
        hiveBtle?.mesh?.getDegradedPeers() ?: emptyList()

    /**
     * Get all peers in Lost state (disconnected and not seen for timeout period).
     */
    fun getLostPeers(): List<PeerConnectionState> =
        hiveBtle?.mesh?.getLostPeers() ?: emptyList()

    /**
     * Get connection state for a specific peer.
     */
    fun getPeerConnectionState(nodeId: Long): PeerConnectionState? =
        hiveBtle?.mesh?.getPeerConnectionState(nodeId.toUInt())

    /**
     * Get summary counts of peers in each connection state (for UI badges).
     */
    fun getConnectionStateCounts(): StateCountSummary? =
        hiveBtle?.mesh?.getConnectionStateCounts()

    /**
     * Send an event to all peers.
     */
    fun sendEvent(eventType: HiveEventType) {
        hiveBtle?.sendEvent(eventType)
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
        val location = HiveLocation(
            latitude = lat.toFloat(),
            longitude = lon.toFloat(),
            altitude = alt.toFloat()
        )
        hiveBtle?.sendEvent(
            eventType = HiveEventType.NONE,
            location = location,
            callsign = callsign,
            battery = battery,
            heartRate = null
        )
        Log.d(TAG, "Broadcast position: callsign=$callsign, lat=$lat, lon=$lon")
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

    /**
     * Set callback for marker sync events.
     */
    fun setMarkerSyncCallback(callback: ((HivePeer, HiveMarker) -> Unit)?) {
        markerSyncCallback = callback
    }

    /**
     * Send a marker to all connected peers.
     */
    fun sendMarker(marker: HiveMarker) {
        hiveBtle?.sendMarker(marker)
    }

    /**
     * Set callback for chat sync events.
     */
    fun setChatSyncCallback(callback: ((HiveChat, HivePeer) -> Unit)?) {
        chatSyncCallback = callback
    }

    /**
     * Set callback for canned message events.
     * Called when a canned message document is received and merged.
     */
    fun setCannedMessageCallback(callback: ((CannedMessageAckEventData) -> Unit)?) {
        cannedMessageCallback = callback
    }

    /**
     * Send a chat message to all connected peers.
     * @param sender Sender callsign (max 16 chars)
     * @param message Message text (max 140 chars)
     */
    fun sendChat(sender: String, message: String) {
        hiveBtle?.sendChat(sender, message)
    }

    /**
     * Send a chat message to all connected peers.
     * @param chat The chat message to send
     */
    fun sendChat(chat: HiveChat) {
        hiveBtle?.sendChat(chat)
    }

    /**
     * Get chat messages from the CRDT since a given timestamp.
     * @param sinceTimestamp Only return messages newer than this timestamp (0 for all)
     * @return List of HiveChat messages from the mesh CRDT
     */
    fun getChatMessagesSince(sinceTimestamp: Long): List<HiveChat> {
        return hiveBtle?.getChatMessagesSince(sinceTimestamp) ?: emptyList()
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
        val nodeId = hiveBtle?.nodeId ?: run {
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

        // Broadcast to mesh (convert List<UByte> to ByteArray)
        val bytes = ByteArray(encoded.size) { encoded[it].toByte() }
        hiveBtle?.broadcastBytes(bytes)

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

    override fun onMarkerSynced(peer: HivePeer, marker: HiveMarker) {
        mainHandler.post {
            Log.i(TAG, "Marker synced from ${peer.displayName()}: ${marker.uid} " +
                    "type=${marker.type} at (${marker.lat}, ${marker.lon}) callsign=${marker.callsign}")
            markerSyncCallback?.invoke(peer, marker)
        }
    }

    override fun onChatReceived(chat: HiveChat, fromPeer: HivePeer) {
        mainHandler.post {
            Log.d(TAG, "Chat received from ${fromPeer.displayName()}: ${chat.sender} says '${chat.message}'")
            chatSyncCallback?.invoke(chat, fromPeer)
        }
    }

    override fun onDecryptedData(peer: HivePeer?, data: ByteArray) {
        // Check for canned message marker (0xAF)
        if (data.isEmpty() || data[0] != 0xAF.toByte()) {
            return
        }

        mainHandler.post {
            try {
                // Decode as CannedMessageAckEvent using hive-lite
                val incoming = decodeCannedMessageAckEvent(data)
                if (incoming == null) {
                    Log.w(TAG, "Failed to decode canned message from ${peer?.displayName()}")
                    return@post
                }

                // Document key based on source node and timestamp
                val key = "${incoming.sourceNode}:${incoming.timestamp}"

                // Merge with existing document or store new
                val merged = cannedMessageDocs[key]?.let { existing ->
                    Log.d(TAG, "Merging canned message $key: existing has ${existing.acks.size} ACKs, incoming has ${incoming.acks.size} ACKs")
                    cannedMessageAckEventMerge(existing, incoming)
                } ?: incoming

                cannedMessageDocs[key] = merged
                updateCannedMessagesObservable()
                Log.i(TAG, "Canned message $key now has ${merged.acks.size} ACKs: ${merged.acks.map { it.nodeId }}")

                // Notify callback with merged document
                cannedMessageCallback?.invoke(merged)
            } catch (e: Exception) {
                Log.e(TAG, "Error processing canned message: ${e.message}", e)
            }
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
