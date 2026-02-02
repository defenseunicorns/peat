/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import com.atakmap.android.dropdown.DropDownMapComponent
import com.atakmap.android.ipc.AtakBroadcast.DocumentedIntentFilter
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.atakmap.coremap.maps.coords.GeoPoint
import com.revolveteam.atak.hive.model.HiveCell
import com.revolveteam.atak.hive.model.HivePlatform
import com.revolveteam.atak.hive.model.HiveTrack
import com.revolveteam.atak.hive.overlay.HiveCellOverlay
import com.revolveteam.atak.hive.overlay.HivePlatformOverlay
import com.revolveteam.atak.hive.overlay.HiveTrackOverlay
import com.revolveteam.hive.HiveChat
import com.revolveteam.hive.HiveDocument
import com.revolveteam.hive.HiveEventType
import com.revolveteam.hive.HiveMarker
import com.revolveteam.hive.HivePeer
import uniffi.hive_lite_android.CannedMessageAckEventData
import uniffi.hive_lite_android.CannedMessageType
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap

/**
 * HIVE Map Component
 *
 * Main component for the HIVE plugin. Extends DropDownMapComponent
 * to integrate with ATAK's dropdown system.
 *
 * NOTE: This is a simplified version without coroutines/Flow to avoid
 * dependency conflicts with ATAK SDK 5.6 Preview's bundled libraries.
 */
class HiveMapComponent : DropDownMapComponent() {

    companion object {
        private const val TAG = "HiveMapComponent"
        private const val REFRESH_INTERVAL_MS = 2000L // Refresh every 2 seconds
    }

    private lateinit var pluginContext: Context
    private lateinit var mapView: MapView
    private var dropDownReceiver: HiveDropDownReceiver? = null
    private var trackOverlay: HiveTrackOverlay? = null
    private var cellOverlay: HiveCellOverlay? = null
    private var platformOverlay: HivePlatformOverlay? = null
    private val refreshHandler = Handler(Looper.getMainLooper())
    private var isRefreshing = false

    // Self-position broadcaster for PLI
    private var selfPositionBroadcaster: SelfPositionBroadcaster? = null
    private var _pliBroadcastEnabled = false
    val pliBroadcastEnabled: Boolean get() = _pliBroadcastEnabled
    private var _lastBroadcastStatus: String = "Not started"
    val lastBroadcastStatus: String get() = _lastBroadcastStatus

    // Simple state management without coroutines
    private val _cells = mutableListOf<HiveCell>()
    val cells: List<HiveCell> get() = _cells.toList()

    private val _platforms = mutableListOf<HivePlatform>()
    val platforms: List<HivePlatform> get() = _platforms.toList() + _blePeerPlatforms.values

    private val _tracks = mutableListOf<HiveTrack>()
    val tracks: List<HiveTrack> get() = _tracks.toList()

    /** Get the self callsign from ATAK's self marker */
    val selfCallsign: String
        get() = if (::mapView.isInitialized) {
            mapView.selfMarker?.getMetaString("callsign", null)
                ?: mapView.selfMarker?.title
                ?: "Self"
        } else "Self"

    // BLE peer platforms for cell membership (nodeId -> platform)
    private val _blePeerPlatforms = mutableMapOf<Long, HivePlatform>()

    /**
     * Per-peer state cache for delta sync (hive-btle 0.0.10+).
     * Delta documents contain only changed fields, so we cache full state
     * and merge incoming updates incrementally.
     */
    /**
     * Battery reading with timestamp for drain rate calculation.
     */
    private data class BatteryReading(val percent: Int, val timestamp: Long)

    private data class PeerState(
        var callsign: String? = null,
        var latitude: Float? = null,
        var longitude: Float? = null,
        var altitude: Float? = null,
        var batteryPercent: Int? = null,
        var heartRate: Int? = null,
        var activityLevel: Int? = null,
        var lastSeen: Long = 0,
        // Battery history for drain rate calculation (keep last 10 readings)
        val batteryHistory: MutableList<BatteryReading> = mutableListOf()
    ) {
        companion object {
            private const val MAX_BATTERY_HISTORY = 10
            private const val MIN_READINGS_FOR_ESTIMATE = 2
            private const val MIN_TIME_SPAN_MS = 60_000L  // Need at least 1 min of data
        }

        /**
         * Record a battery reading and compute estimated time remaining.
         * Returns estimated minutes remaining, or null if insufficient data.
         */
        fun recordBatteryAndEstimate(percent: Int, timestamp: Long): Int? {
            // Only record if battery changed or enough time passed
            val lastReading = batteryHistory.lastOrNull()
            if (lastReading != null && lastReading.percent == percent &&
                timestamp - lastReading.timestamp < 30_000L) {
                // Same battery, less than 30s - skip
                return computeTimeRemaining()
            }

            batteryHistory.add(BatteryReading(percent, timestamp))

            // Keep only last N readings
            while (batteryHistory.size > MAX_BATTERY_HISTORY) {
                batteryHistory.removeAt(0)
            }

            return computeTimeRemaining()
        }

        /**
         * Compute estimated time remaining based on drain rate.
         */
        fun computeTimeRemaining(): Int? {
            if (batteryHistory.size < MIN_READINGS_FOR_ESTIMATE) return null

            val oldest = batteryHistory.first()
            val newest = batteryHistory.last()
            val timeSpanMs = newest.timestamp - oldest.timestamp

            if (timeSpanMs < MIN_TIME_SPAN_MS) return null  // Not enough time span

            val percentDrop = oldest.percent - newest.percent
            if (percentDrop <= 0) return null  // Battery not draining (charging or stable)

            // Calculate drain rate in percent per minute
            val drainRatePerMinute = percentDrop.toDouble() / (timeSpanMs / 60_000.0)
            if (drainRatePerMinute <= 0.001) return null  // Too slow to estimate

            // Estimate time remaining
            val minutesRemaining = (newest.percent / drainRatePerMinute).toInt()

            // Sanity check: cap at 48 hours (2880 minutes)
            return minutesRemaining.coerceIn(0, 2880)
        }
    }
    private val _peerStateCache = mutableMapOf<Long, PeerState>()

    /**
     * Active emergencies - maps nodeId to timestamp when emergency was received.
     * Used to show emergency status on platforms and trigger alerts.
     */
    private val _activeEmergencies = mutableMapOf<Long, Long>()
    val activeEmergencies: Map<Long, Long> get() = _activeEmergencies.toMap()

    /** Callback for emergency events (for UI alerts) */
    var onEmergencyReceived: ((nodeId: Long, callsign: String, lat: Double, lon: Double) -> Unit)? = null

    /** Callback for when emergency is cleared (for UI refresh) */
    var onEmergencyCleared: ((nodeId: Long) -> Unit)? = null

    /** Callback for when platform data changes (for UI refresh) */
    var onPlatformsChanged: (() -> Unit)? = null

    /**
     * Cached marker with source peer information for UI display.
     */
    data class CachedMarker(
        val marker: HiveMarker,
        val sourcePeerName: String,
        val receivedAt: Long = System.currentTimeMillis()
    )

    /**
     * Marker cache for mesh-synced map markers (deduplication by UID).
     * Maps marker UID -> CachedMarker with source peer info for UI display.
     */
    private val _markerCache = ConcurrentHashMap<String, CachedMarker>()
    val markers: Map<String, CachedMarker> get() = _markerCache.toMap()

    /** Callback for when a marker is received (for UI/map display) */
    var onMarkerReceived: ((marker: HiveMarker, sourcePeer: HivePeer) -> Unit)? = null

    /**
     * ACK info for tracking who acknowledged a message.
     */
    data class ChatAck(
        val sender: String,         // Callsign of the acker
        val timestamp: Long         // When the ACK was sent
    )

    /**
     * Chat message with ACK associations for UI display.
     */
    data class CachedChat(
        val chat: HiveChat,
        val isSelf: Boolean,        // True if sent by this device
        val acks: List<ChatAck> = emptyList()
    ) {
        /** Get the message ID string for correlation (format: "XXXXXXXX:timestamp") */
        fun messageId(): String = chat.messageIdString()
    }

    /**
     * Get chat messages from the mesh CRDT with ACK associations.
     * Returns messages sorted by timestamp, with ACKs associated to their parent messages.
     */
    val chatMessages: List<CachedChat>
        get() {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager() ?: return emptyList()
            val myNodeId = bleManager.getNodeId() ?: 0L

            // Get all messages from CRDT
            val allMessages = bleManager.getChatMessagesSince(0)

            // Separate regular messages from ACK replies
            val regularMessages = mutableListOf<HiveChat>()
            val ackMessages = mutableListOf<HiveChat>()

            for (msg in allMessages) {
                val isAckReply = msg.isReply() && msg.message.equals("ACK", ignoreCase = true)
                if (isAckReply) {
                    ackMessages.add(msg)
                } else {
                    regularMessages.add(msg)
                }
            }

            // Build ACK associations
            val acksByMessageId = ackMessages.groupBy { it.replyToIdString() }

            // Build CachedChat list with ACKs
            return regularMessages
                .sortedBy { it.timestamp }
                .map { msg ->
                    val messageId = msg.messageIdString()
                    val acks = acksByMessageId[messageId]?.map { ack ->
                        ChatAck(sender = ack.sender, timestamp = ack.timestamp)
                    } ?: emptyList()

                    CachedChat(
                        chat = msg,
                        isSelf = msg.originNode == myNodeId,
                        acks = acks
                    )
                }
        }

    /** Callback for when a chat message is received (for UI refresh) */
    var onChatReceived: ((chat: HiveChat, fromPeer: HivePeer) -> Unit)? = null

    private var _connectionStatus = ConnectionStatus.DISCONNECTED
    val connectionStatus: ConnectionStatus get() = _connectionStatus

    val peerCount: Int get() = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0

    // Selected cell for hierarchical navigation
    private var _selectedCellId: String? = null
    val selectedCellId: String? get() = _selectedCellId

    private var _selectedCellName: String? = null
    val selectedCellName: String? get() = _selectedCellName

    /** Callback for when cell selection changes */
    var onCellSelectionChanged: ((cellId: String?, cellName: String?) -> Unit)? = null

    override fun onCreate(context: Context, intent: Intent, view: MapView) {
        context.setTheme(R.style.ATAKPluginTheme)
        super.onCreate(context, intent, view)

        pluginContext = context
        mapView = view
        Log.d(TAG, "HiveMapComponent onCreate")

        // Create track overlay for map markers
        trackOverlay = HiveTrackOverlay(view)
        Log.d(TAG, "Track overlay created")

        // Create cell overlay for cell boundaries (kept for cell metadata, but cell markers are secondary to platforms)
        cellOverlay = HiveCellOverlay(view)
        cellOverlay?.onCellSelectedListener = object : HiveCellOverlay.OnCellSelectedListener {
            override fun onCellSelected(cellId: String, cellName: String, centerLat: Double, centerLon: Double, radiusMeters: Double) {
                selectCell(cellId, cellName)
                zoomToCell(centerLat, centerLon, radiusMeters)
            }
        }
        Log.d(TAG, "Cell overlay created")

        // Create platform overlay for individual platform markers
        platformOverlay = HivePlatformOverlay(view)
        Log.d(TAG, "Platform overlay created")

        // Create self-position broadcaster for PLI
        selfPositionBroadcaster = SelfPositionBroadcaster(view)
        selfPositionBroadcaster?.onBroadcastCallback = { success, message ->
            _lastBroadcastStatus = message
            Log.d(TAG, "PLI broadcast: $success - $message")
        }
        Log.d(TAG, "Self-position broadcaster created")

        // Create dropdown receiver
        dropDownReceiver = HiveDropDownReceiver(view, context, this)

        // Register for show plugin intent
        val ddFilter = DocumentedIntentFilter()
        ddFilter.addAction(HiveDropDownReceiver.SHOW_PLUGIN)
        registerDropDownReceiver(dropDownReceiver, ddFilter)

        // Register BLE document sync callback to display peer locations on map
        registerBleDocumentCallback()

        // Register BLE peer event callback for emergency/ack events
        registerBlePeerEventCallback()

        // Register BLE marker callback for mesh-synced map markers
        registerBleMarkerCallback()

        // Register BLE chat callback for mesh chat messages
        registerBleChatCallback()

        // Update connection status based on HIVE node availability
        updateConnectionStatus()

        // Broadcast initial position to BLE mesh so the tablet's callsign is visible to watches
        // This sets hasPeripheral=true on the mesh so peers can resolve our callsign
        broadcastInitialPosition()

        // Start periodic refresh for map markers
        startPeriodicRefresh()

        Log.d(TAG, "HiveMapComponent initialized")
    }

    override fun onDestroyImpl(context: Context, view: MapView) {
        Log.d(TAG, "HiveMapComponent onDestroy")
        stopPeriodicRefresh()
        selfPositionBroadcaster?.stop()
        selfPositionBroadcaster = null
        trackOverlay?.dispose()
        trackOverlay = null
        cellOverlay?.dispose()
        cellOverlay = null
        platformOverlay?.dispose()
        platformOverlay = null
        super.onDestroyImpl(context, view)
    }

    /**
     * Register callback for BLE document sync to display peer locations.
     */
    private fun registerBleDocumentCallback() {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            if (bleManager != null) {
                bleManager.setDocumentSyncCallback { document ->
                    onBleDocumentSynced(document)
                }
                Log.i(TAG, "BLE document sync callback registered")
            } else {
                Log.w(TAG, "BLE manager not available for document callback")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error registering BLE document callback: ${e.message}", e)
        }
    }

    /**
     * Register callback for BLE peer events (emergency, ack, etc).
     */
    private fun registerBlePeerEventCallback() {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            if (bleManager != null) {
                bleManager.setPeerEventCallback { peer, eventType ->
                    onBlePeerEvent(peer.nodeId ?: 0L, eventType)
                }
                Log.i(TAG, "BLE peer event callback registered")
            } else {
                Log.w(TAG, "BLE manager not available for peer event callback")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error registering BLE peer event callback: ${e.message}", e)
        }
    }

    /**
     * Handle peer events from BLE mesh (emergency, cancellation).
     *
     * SOS lifecycle:
     * - EMERGENCY: Peer triggered SOS -> show red marker + alert
     * - NONE: Peer cancelled SOS -> clear emergency state
     * - ACK: (Deferred for MVP) Acknowledgment from other peer
     */
    private fun onBlePeerEvent(nodeId: Long, eventType: HiveEventType) {
        refreshHandler.post {
            val wasInEmergency = _activeEmergencies.containsKey(nodeId)

            when (eventType) {
                HiveEventType.EMERGENCY -> {
                    Log.w(TAG, "SOS EMERGENCY from peer ${String.format("%08X", nodeId)}")
                    Log.d(TAG, "Current platforms: ${_blePeerPlatforms.keys.map { String.format("%08X", it) }}")
                    _activeEmergencies[nodeId] = System.currentTimeMillis()

                    // Get platform info for alert callback
                    val platform = _blePeerPlatforms[nodeId]
                    Log.d(TAG, "Platform lookup for nodeId=$nodeId (${String.format("%08X", nodeId)}): ${if (platform != null) "FOUND ${platform.callsign}" else "NOT FOUND"}")
                    if (platform != null) {
                        // Update platform status to EMERGENCY and refresh overlay
                        val emergencyPlatform = platform.copy(status = HivePlatform.Status.EMERGENCY)
                        _blePeerPlatforms[nodeId] = emergencyPlatform
                        Log.i(TAG, "Updated ${platform.callsign} to EMERGENCY status")
                        updateBlePeerOverlay()

                        // Notify UI for alert display (only on new emergency)
                        if (!wasInEmergency) {
                            onEmergencyReceived?.invoke(nodeId, platform.callsign, platform.lat, platform.lon)
                        }
                    } else {
                        Log.w(TAG, "Cannot update emergency status - platform not yet synced")
                    }
                }
                HiveEventType.NONE -> {
                    // SOS cancellation - peer cleared their emergency
                    if (wasInEmergency) {
                        Log.i(TAG, "SOS CANCELLED by peer ${String.format("%08X", nodeId)}")
                        clearEmergencyState(nodeId)
                    }
                }
                HiveEventType.ACK -> {
                    // ACK handling deferred for MVP - just log it
                    Log.i(TAG, "ACK received from peer ${String.format("%08X", nodeId)}")
                }
                else -> {
                    Log.d(TAG, "Peer event: ${String.format("%08X", nodeId)} -> $eventType")
                }
            }
        }
    }

    /**
     * Register callback for BLE marker sync events.
     */
    private fun registerBleMarkerCallback() {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            if (bleManager != null) {
                bleManager.setMarkerSyncCallback { peer, marker ->
                    onBleMarkerSynced(peer, marker)
                }
                Log.i(TAG, "BLE marker sync callback registered")
            } else {
                Log.w(TAG, "BLE manager not available for marker callback")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error registering BLE marker callback: ${e.message}", e)
        }
    }

    /**
     * Handle marker received from BLE mesh peer.
     * Creates/updates ATAK map markers for mesh-synced waypoints and POIs.
     */
    private fun onBleMarkerSynced(peer: HivePeer, marker: HiveMarker) {
        refreshHandler.post {
            // Check for duplicate/outdated marker
            val existing = _markerCache[marker.uid]
            if (existing != null && existing.marker.time >= marker.time) {
                Log.v(TAG, "Ignoring older marker: ${marker.uid} (existing=${existing.marker.time}, received=${marker.time})")
                return@post
            }

            // Cache the marker with source peer info
            _markerCache[marker.uid] = CachedMarker(
                marker = marker,
                sourcePeerName = peer.displayName()
            )
            Log.i(TAG, "Marker synced from ${peer.displayName()}: ${marker.uid} " +
                    "type=${marker.type} callsign=${marker.callsign} at (${marker.lat}, ${marker.lon})")

            // Create ATAK map marker
            createAtakMarkerFromHive(marker, peer)

            // Notify UI of new marker
            onMarkerReceived?.invoke(marker, peer)
        }
    }

    /**
     * Register callback for BLE chat messages.
     */
    private fun registerBleChatCallback() {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            if (bleManager != null) {
                bleManager.setChatSyncCallback { chat, fromPeer ->
                    onBleChatReceived(chat, fromPeer)
                }
                Log.i(TAG, "BLE chat callback registered")
            } else {
                Log.w(TAG, "BLE manager not available for chat callback")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error registering BLE chat callback: ${e.message}", e)
        }
    }

    /**
     * Handle chat message received from BLE mesh peer.
     * The CRDT handles storage and deduplication - we just notify the UI.
     */
    private fun onBleChatReceived(chat: HiveChat, fromPeer: HivePeer) {
        refreshHandler.post {
            Log.i(TAG, "Chat from ${fromPeer.displayName()}: '${chat.sender}' says '${chat.message}'")

            // Notify UI to refresh (CRDT handles storage)
            onChatReceived?.invoke(chat, fromPeer)
        }
    }

    /**
     * Send a chat message to all connected BLE mesh peers.
     * The CRDT handles storage - message will appear in chatMessages after sync.
     * @param message Message text (max 140 chars)
     */
    fun sendChat(message: String) {
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        if (bleManager == null) {
            Log.w(TAG, "Cannot send chat - BLE manager not available")
            return
        }

        val nodeId = bleManager.getNodeId()
        if (nodeId == null || nodeId == 0L) {
            Log.e(TAG, "Cannot send chat - mesh not initialized")
            return
        }

        if (!bleManager.isRunning.value) {
            Log.e(TAG, "Cannot send chat - mesh not running")
            return
        }

        // Get callsign from self marker
        val selfCallsign = mapView.selfMarker?.getMetaString("callsign", null)
            ?: mapView.selfMarker?.title
            ?: "ATAK"

        // Create and send chat - CRDT handles storage and sync
        val chat = HiveChat(
            sender = selfCallsign,
            message = message,
            timestamp = System.currentTimeMillis(),
            originNode = nodeId,
            isBroadcast = true,
            requiresAck = false
        )

        bleManager.sendChat(chat)
        Log.i(TAG, "Sent chat: '$selfCallsign' says '$message'")
    }

    /**
     * Send a canned message to all connected BLE mesh peers.
     * The message is encoded as a CannedMessageAckEvent and broadcast.
     *
     * @param messageType The type of canned message to send
     * @return The sent message document, or null if failed
     */
    fun sendCannedMessage(messageType: CannedMessageType): CannedMessageAckEventData? {
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        if (bleManager == null) {
            Log.w(TAG, "Cannot send canned message - BLE manager not available")
            return null
        }

        if (!bleManager.isRunning.value) {
            Log.e(TAG, "Cannot send canned message - mesh not running")
            return null
        }

        return bleManager.sendCannedMessage(messageType)
    }

    /**
     * Get all canned messages from the BLE manager.
     */
    fun getCannedMessages(): List<CannedMessageAckEventData> {
        return HivePluginLifecycle.getInstance()?.getHiveBleManager()?.getCannedMessages() ?: emptyList()
    }

    /**
     * Create an ATAK map marker from a HiveMarker.
     */
    private fun createAtakMarkerFromHive(marker: HiveMarker, sourcePeer: HivePeer) {
        try {
            val point = GeoPoint(marker.lat.toDouble(), marker.lon.toDouble(), marker.hae.toDouble())

            // Create marker using ATAK API
            val atakMarker = com.atakmap.android.maps.Marker(point, marker.uid)
            atakMarker.type = marker.type
            atakMarker.title = marker.callsign
            atakMarker.setMetaString("callsign", marker.callsign)
            atakMarker.setMetaString("hiveSource", sourcePeer.displayName())
            atakMarker.setMetaString("hiveMeshMarker", "true")
            atakMarker.setMetaLong("hiveTime", marker.time)

            // Add to map group
            val rootGroup = mapView.rootGroup
            val hiveGroup = rootGroup?.findMapGroup("HIVE Markers")
                ?: rootGroup?.addGroup("HIVE Markers")

            // Remove existing marker with same UID if present
            hiveGroup?.items?.filterIsInstance<com.atakmap.android.maps.Marker>()
                ?.find { it.uid == marker.uid }
                ?.let { hiveGroup.removeItem(it) }

            hiveGroup?.addItem(atakMarker)
            Log.d(TAG, "Created ATAK marker: ${marker.uid} (${marker.callsign}) at ${point}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create ATAK marker from HiveMarker: ${e.message}", e)
        }
    }

    /**
     * Clear emergency state for a peer (called on SOS cancellation or ACK).
     */
    private fun clearEmergencyState(nodeId: Long) {
        _activeEmergencies.remove(nodeId)

        // Reset platform status from EMERGENCY to OPERATIONAL
        val platform = _blePeerPlatforms[nodeId]
        if (platform != null && platform.status == HivePlatform.Status.EMERGENCY) {
            val normalPlatform = platform.copy(status = HivePlatform.Status.OPERATIONAL)
            _blePeerPlatforms[nodeId] = normalPlatform
            updateBlePeerOverlay()

            // Notify UI that emergency was cleared
            onEmergencyCleared?.invoke(nodeId)
        }
    }

    /**
     * Acknowledge an emergency - broadcasts ACK to all peers.
     * Also clears local emergency state for the specified peer.
     */
    fun acknowledgeEmergency(nodeId: Long) {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            bleManager?.acknowledgeEmergency()  // Broadcasts ACK to all peers
            Log.i(TAG, "Sent ACK broadcast for emergency from ${String.format("%08X", nodeId)}")

            // Clear local emergency state
            _activeEmergencies.remove(nodeId)
            val platform = _blePeerPlatforms[nodeId]
            if (platform != null && platform.status == HivePlatform.Status.EMERGENCY) {
                val normalPlatform = platform.copy(status = HivePlatform.Status.OPERATIONAL)
                _blePeerPlatforms[nodeId] = normalPlatform
                updateBlePeerOverlay()
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to send emergency ACK: ${e.message}", e)
        }
    }

    /**
     * Handle synced document from BLE mesh peer - create/update track marker.
     *
     * With hive-btle 0.0.10+ delta sync, documents may contain only changed fields.
     * We merge incoming data with cached state to reconstruct full peer information.
     */
    private fun onBleDocumentSynced(document: HiveDocument) {
        val nodeId = document.nodeId
        val peripheral = document.peripheral

        Log.d(TAG, "Document synced: nodeId=${String.format("%08X", nodeId)}, callsign=${peripheral?.callsign}")

        // Get or create cached state for this peer
        val cachedState = _peerStateCache.getOrPut(nodeId) { PeerState() }
        cachedState.lastSeen = System.currentTimeMillis()

        // Merge incoming data into cached state (delta sync: only non-null fields are updated)
        if (peripheral != null) {
            // Merge callsign if present - but ignore generic "ANDROID" callsign
            // (watches may send "ANDROID" during SOS mode, we want to keep original callsign)
            peripheral.callsign?.trim()?.takeIf {
                it.isNotBlank() && !it.equals("ANDROID", ignoreCase = true)
            }?.let {
                cachedState.callsign = it
            }

            // Merge location if present
            peripheral.location?.let { location ->
                cachedState.latitude = location.latitude
                cachedState.longitude = location.longitude
                cachedState.altitude = location.altitude
            }

            // Merge health data if present
            peripheral.health.let { health ->
                // Always update battery if it's in valid range (0-100)
                // batteryPercent=0 is valid and means device is nearly dead
                if (health.batteryPercent in 0..100) {
                    cachedState.batteryPercent = health.batteryPercent
                    // Record battery reading for drain rate calculation
                    cachedState.recordBatteryAndEstimate(health.batteryPercent, System.currentTimeMillis())
                }
                // Update heart rate - null or 0 means sensor not active (e.g., watch on charger)
                cachedState.heartRate = health.heartRate?.takeIf { it > 0 }
                // Activity level: 0 is valid, so always update if present in peripheral
                cachedState.activityLevel = health.activityLevel
            }
        }

        // Check event type from document - handles SOS state transitions
        // onPeerEvent callback only fires for non-NONE events, so we detect cancellation here
        val documentEventType = document.currentEventType()
        val wasInEmergency = _activeEmergencies.containsKey(nodeId)

        when (documentEventType) {
            HiveEventType.EMERGENCY -> {
                if (!wasInEmergency) {
                    Log.w(TAG, "SOS EMERGENCY detected in document from peer ${String.format("%08X", nodeId)}")
                    _activeEmergencies[nodeId] = System.currentTimeMillis()
                    // Notify UI of emergency (will be called again in onBlePeerEvent if that fires too)
                    onPlatformsChanged?.invoke()
                }
            }
            HiveEventType.NONE -> {
                if (wasInEmergency) {
                    Log.i(TAG, "SOS CANCELLED detected in document from peer ${String.format("%08X", nodeId)}")
                    _activeEmergencies.remove(nodeId)
                    // Notify UI that emergency cleared
                    onEmergencyCleared?.invoke(nodeId)
                    onPlatformsChanged?.invoke()
                }
            }
            else -> { /* ACK or other events handled elsewhere */ }
        }

        // Skip display if we still don't have any location (cached or incoming)
        val lat = cachedState.latitude
        val lon = cachedState.longitude
        if (lat == null || lon == null) {
            Log.d(TAG, "BLE peer ${String.format("%08X", nodeId)}: no location yet (delta sync pending)")
            return
        }

        // Resolve callsign with fallback chain:
        // 1. Cached state callsign (from this session's documents)
        // 2. BLE manager's persisted callsign cache (from previous sessions)
        // 3. Generated fallback name
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val callsign = cachedState.callsign
            ?: bleManager?.getCachedCallsign(nodeId)
            ?: "BLE-${String.format("%08X", nodeId).takeLast(4)}"

        // Get cell ID for organizational grouping (NATO phonetic: ALPHA, BRAVO, etc.)
        // Cell is separate from mesh - mesh is transport layer, cell is organizational unit
        val cellId = HivePluginLifecycle.getInstance()?.getCurrentCellId()
            ?: HivePluginLifecycle.DEFAULT_CELL_ID
        val meshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId()

        Log.i(TAG, "BLE document synced: nodeId=${String.format("%08X", nodeId)}, " +
                "callsign=$callsign, cell=$cellId, mesh=$meshId, location=($lat, $lon), " +
                "battery=${cachedState.batteryPercent}%, heartRate=${cachedState.heartRate}, " +
                "event=$documentEventType")

        // Determine status - check if peer has active emergency
        val status = if (_activeEmergencies.containsKey(nodeId)) {
            HivePlatform.Status.EMERGENCY
        } else {
            HivePlatform.Status.OPERATIONAL
        }

        // Create/update platform from merged cached state
        // Note: BLE peers are shown as platforms only (not tracks) - tracks are for detected entities
        val platform = HivePlatform(
            id = "ble-${String.format("%08X", nodeId)}",
            callsign = callsign,
            platformType = HivePlatform.PlatformType.SOLDIER,
            lat = lat.toDouble(),
            lon = lon.toDouble(),
            hae = cachedState.altitude?.toDouble()?.takeIf { it != 0.0 },
            cellId = cellId,
            status = status,
            batteryPercent = cachedState.batteryPercent ?: 0,
            batteryTimeRemainingMinutes = cachedState.computeTimeRemaining(),
            heartRate = cachedState.heartRate,
            lastUpdate = cachedState.lastSeen
        )

        // Update the BLE peer platform map and refresh overlay
        refreshHandler.post {
            _blePeerPlatforms[nodeId] = platform
            // Ensure cell exists for this cell ID
            ensureCellExists(cellId)
            updateBlePeerOverlay()
            // Notify UI of platform change so dropdown refreshes
            onPlatformsChanged?.invoke()
        }
    }

    /**
     * Ensure a cell exists for the given cell ID (NATO phonetic: ALPHA, BRAVO, etc.).
     * Creates a cell if one doesn't exist.
     * The tablet (full HIVE node) auto-assigns itself as cell leader.
     *
     * Cells are organizational units within the mesh - squads/teams working together.
     * The mesh provides connectivity; the cell provides command structure.
     */
    private fun ensureCellExists(cellId: String) {
        if (_cells.any { it.id == cellId }) return

        // Get self marker to set as leader (tablet running full HIVE is the leader)
        val selfMarker = mapView.selfMarker
        val selfUid = selfMarker?.uid
        val selfCallsign = selfMarker?.getMetaString("callsign", null)
            ?: selfMarker?.title
            ?: "Tablet"
        val selfPoint = selfMarker?.point

        // Create cell for this organizational unit
        val cell = HiveCell(
            id = cellId,
            name = cellId,  // NATO phonetic name (ALPHA, BRAVO, etc.)
            status = HiveCell.Status.ACTIVE,
            platformCount = _blePeerPlatforms.size + 1,  // +1 for tablet
            centerLat = selfPoint?.latitude ?: _blePeerPlatforms.values.firstOrNull()?.lat ?: 0.0,
            centerLon = selfPoint?.longitude ?: _blePeerPlatforms.values.firstOrNull()?.lon ?: 0.0,
            capabilities = listOf("BLE_MESH", "GATEWAY"),
            formationId = null,
            leaderId = selfUid,  // Tablet is cell leader
            lastUpdate = System.currentTimeMillis()
        )
        _cells.add(cell)

        // Add tablet itself as a platform in this cell
        if (selfUid != null && selfPoint != null) {
            val tabletPlatform = HivePlatform(
                id = selfUid,
                callsign = selfCallsign,
                platformType = HivePlatform.PlatformType.SOLDIER,
                lat = selfPoint.latitude,
                lon = selfPoint.longitude,
                hae = if (selfPoint.isAltitudeValid) selfPoint.altitude else null,
                cellId = cellId,
                status = HivePlatform.Status.OPERATIONAL,
                batteryPercent = null,
                lastUpdate = System.currentTimeMillis()
            )
            _blePeerPlatforms[selfUid.hashCode().toLong()] = tabletPlatform
        }

        Log.i(TAG, "Created cell: $cellId, leader=$selfCallsign ($selfUid)")
    }

    /**
     * Update the track overlay with BLE peer tracks.
     */
    private fun updateBlePeerOverlay() {
        // Use combined tracks/platforms (includes BLE peers via getters)
        trackOverlay?.updateTracks(tracks)
        platformOverlay?.updatePlatforms(platforms, _cells)
        Log.d(TAG, "Updated overlay with ${_tracks.size} FFI tracks, ${_blePeerPlatforms.size} BLE platforms")
    }

    /**
     * Start periodic refresh of track data and map markers
     */
    private fun startPeriodicRefresh() {
        if (isRefreshing) return
        isRefreshing = true
        Log.i(TAG, "Starting periodic refresh (${REFRESH_INTERVAL_MS}ms interval)")
        refreshHandler.post(refreshRunnable)
    }

    /**
     * Stop periodic refresh
     */
    private fun stopPeriodicRefresh() {
        isRefreshing = false
        refreshHandler.removeCallbacks(refreshRunnable)
        Log.i(TAG, "Stopped periodic refresh")
    }

    private val refreshRunnable = object : Runnable {
        override fun run() {
            if (!isRefreshing) return

            try {
                refreshData()

                // Clean up lost BLE platforms using hive-btle's connection state graph
                cleanupLostPlatforms()

                // Ensure cell exists for current cell assignment if BLE platforms exist
                if (_blePeerPlatforms.isNotEmpty()) {
                    val cellId = HivePluginLifecycle.getInstance()?.getCurrentCellId()
                        ?: HivePluginLifecycle.DEFAULT_CELL_ID
                    ensureCellExists(cellId)
                } else {
                    // No BLE peers - remove cell if empty
                    val cellId = HivePluginLifecycle.getInstance()?.getCurrentCellId()
                        ?: HivePluginLifecycle.DEFAULT_CELL_ID
                    removeCellIfEmpty(cellId)
                }

                // Update map overlays - use combined tracks and platforms (includes BLE peers)
                trackOverlay?.updateTracks(tracks)
                // Update cell bounding circles based on platform positions
                cellOverlay?.updateCellBounds(_cells, platforms)
                platformOverlay?.updatePlatforms(platforms, _cells)
            } catch (e: Exception) {
                Log.e(TAG, "Error in periodic refresh: ${e.message}", e)
            }

            // Schedule next refresh
            if (isRefreshing) {
                refreshHandler.postDelayed(this, REFRESH_INTERVAL_MS)
            }
        }
    }

    /**
     * Remove BLE peer platforms that are no longer known to the mesh.
     * Uses isPeerKnown() from hive-btle for accurate multi-hop awareness,
     * with fallback to staleness check for robustness.
     */
    private fun cleanupLostPlatforms() {
        val hiveMesh = HivePluginLifecycle.getInstance()?.getHiveBleManager()?.getMesh()
        val selfUidHash = mapView.selfMarker?.uid?.hashCode()?.toLong()
        val now = System.currentTimeMillis()
        val staleThresholdMs = 5 * 60 * 1000L // 5 minutes - same as overlay

        // Find platforms that are either:
        // 1. Not known to the mesh (isPeerKnown returns false), OR
        // 2. Stale (haven't sent a document in 5 minutes) as fallback
        val toRemove = _blePeerPlatforms.entries.filter { (nodeId, platform) ->
            if (nodeId == selfUidHash) return@filter false

            // Check if peer is known to mesh (direct or indirect)
            val isKnown = hiveMesh?.isPeerKnown(nodeId.toUInt()) ?: true
            val isStale = (now - platform.lastUpdate > staleThresholdMs)

            // Remove if not known to mesh AND stale (belt and suspenders)
            !isKnown && isStale
        }

        Log.d(TAG, "Cleanup check: ${_blePeerPlatforms.size} platforms, ${toRemove.size} to remove")

        if (toRemove.isNotEmpty()) {
            toRemove.forEach { (nodeId, platform) ->
                _blePeerPlatforms.remove(nodeId)
                // Also clear cached state for delta sync
                _peerStateCache.remove(nodeId)
                // Clear any emergency state
                _activeEmergencies.remove(nodeId)
                val staleSec = (now - platform.lastUpdate) / 1000
                Log.i(TAG, "Removed lost BLE peer: ${platform.callsign} " +
                        "(${String.format("%08X", nodeId)}, unknown to mesh, last seen ${staleSec}s ago)")
            }
        }
    }

    /**
     * Remove a cell if it has no platforms (except maybe the tablet itself).
     */
    private fun removeCellIfEmpty(cellId: String) {
        val cellPlatforms = platforms.filter { it.cellId == cellId }
        // Keep cell if tablet is still in it, otherwise remove
        val selfUid = mapView.selfMarker?.uid
        val hasOnlyTablet = cellPlatforms.size <= 1 && cellPlatforms.firstOrNull()?.id == selfUid

        if (cellPlatforms.isEmpty() || hasOnlyTablet) {
            _cells.removeAll { it.id == cellId }
            // Also remove tablet from BLE platforms if no other peers
            if (hasOnlyTablet) {
                val selfUidHash = selfUid?.hashCode()?.toLong()
                selfUidHash?.let { _blePeerPlatforms.remove(it) }
            }
            Log.i(TAG, "Removed empty cell: $cellId")
        }
    }

    /**
     * Update connection status based on HIVE node availability and peer count
     */
    private fun updateConnectionStatus() {
        val node = HivePluginLifecycle.getInstance()?.getHiveNodeJni()
        val ffiPeerCount = peerCount  // FFI mesh peers
        val blePeerCount = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            ?.peers?.value?.count { it.isConnected } ?: 0
        val totalPeerCount = ffiPeerCount + blePeerCount

        _connectionStatus = when {
            node == null -> ConnectionStatus.DISCONNECTED
            totalPeerCount > 0 -> ConnectionStatus.CONNECTED
            else -> ConnectionStatus.CONNECTING  // Node exists but no peers
        }
        Log.d(TAG, "Connection status: $_connectionStatus (ffi=$ffiPeerCount, ble=$blePeerCount)")
    }

    /**
     * Refresh data from HIVE network
     */
    fun refreshData() {
        Log.d(TAG, "Refreshing HIVE data")
        updateConnectionStatus()

        val lifecycle = HivePluginLifecycle.getInstance()
        val node = lifecycle?.getHiveNodeJni()
        if (node == null) {
            Log.d(TAG, "No HIVE FFI node - running BLE-only mode")
            // In BLE-only mode, don't clear cells/platforms - they're managed by BLE callbacks
            // Only clear FFI-sourced data (_platforms and _tracks from JSON), preserve BLE data
            _platforms.clear()
            _tracks.clear()
            // Don't clear _cells - synthetic BLE cell is preserved
            return  // Skip FFI calls, BLE data is managed by callbacks
        }

        // Fetch cells from HIVE sync (only when FFI node is available)
        try {
            val cellsJson = node.getCellsJson()
            Log.d(TAG, "Cells JSON: $cellsJson")
            // Preserve synthetic BLE cells when loading from FFI
            val bleCells = _cells.filter { it.capabilities.contains("BLE_MESH") }
            _cells.clear()
            _cells.addAll(parseCellsJson(cellsJson))
            // Re-add BLE cells that aren't duplicates
            bleCells.forEach { bleCell ->
                if (_cells.none { it.id == bleCell.id }) {
                    _cells.add(bleCell)
                }
            }
            Log.i(TAG, "Loaded ${_cells.size} cells from HIVE (+ ${bleCells.size} BLE cells)")
        } catch (e: Exception) {
            Log.e(TAG, "Error fetching cells: ${e.message}", e)
        }

        // Fetch tracks from HIVE sync
        try {
            val tracksJson = node.getTracksJson()
            Log.d(TAG, "Tracks JSON: $tracksJson")
            _tracks.clear()
            _tracks.addAll(parseTracksJson(tracksJson))
            Log.i(TAG, "Loaded ${_tracks.size} tracks from HIVE")
        } catch (e: Exception) {
            Log.e(TAG, "Error fetching tracks: ${e.message}", e)
        }

        // Fetch platforms from HIVE sync
        try {
            val platformsJson = node.getPlatformsJson()
            Log.d(TAG, "Platforms JSON: $platformsJson")
            _platforms.clear()
            _platforms.addAll(parsePlatformsJson(platformsJson))
            Log.i(TAG, "Loaded ${_platforms.size} platforms from HIVE")
        } catch (e: Exception) {
            Log.e(TAG, "Error fetching platforms: ${e.message}", e)
        }
    }

    /**
     * Parse cells JSON array into HiveCell objects
     */
    private fun parseCellsJson(json: String): List<HiveCell> {
        val cells = mutableListOf<HiveCell>()
        try {
            val arr = JSONArray(json)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                cells.add(parseCellObject(obj))
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error parsing cells JSON: ${e.message}")
        }
        return cells
    }

    /**
     * Parse a single cell JSON object
     */
    private fun parseCellObject(obj: JSONObject): HiveCell {
        val capabilitiesArr = obj.optJSONArray("capabilities")
        val capabilities = mutableListOf<String>()
        if (capabilitiesArr != null) {
            for (i in 0 until capabilitiesArr.length()) {
                capabilities.add(capabilitiesArr.getString(i))
            }
        }

        val statusStr = obj.optString("status", "OFFLINE").uppercase()
        val status = try {
            HiveCell.Status.valueOf(statusStr)
        } catch (e: Exception) {
            HiveCell.Status.OFFLINE
        }

        return HiveCell(
            id = obj.getString("id"),
            name = obj.getString("name"),
            status = status,
            platformCount = obj.optInt("platform_count", 0),
            centerLat = obj.optDouble("center_lat", 0.0),
            centerLon = obj.optDouble("center_lon", 0.0),
            capabilities = capabilities,
            formationId = obj.optString("formation_id", null).takeIf { it?.isNotEmpty() == true },
            leaderId = obj.optString("leader_id", null).takeIf { it?.isNotEmpty() == true },
            lastUpdate = obj.optLong("last_update", System.currentTimeMillis())
        )
    }

    /**
     * Parse tracks JSON array into HiveTrack objects
     */
    private fun parseTracksJson(json: String): List<HiveTrack> {
        val tracks = mutableListOf<HiveTrack>()
        try {
            val arr = JSONArray(json)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                tracks.add(parseTrackObject(obj))
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error parsing tracks JSON: ${e.message}")
        }
        return tracks
    }

    /**
     * Parse a single track JSON object
     */
    private fun parseTrackObject(obj: JSONObject): HiveTrack {
        val attributesObj = obj.optJSONObject("attributes")
        val attributes = mutableMapOf<String, String>()
        if (attributesObj != null) {
            val keys = attributesObj.keys()
            while (keys.hasNext()) {
                val key = keys.next()
                attributes[key] = attributesObj.optString(key, "")
            }
        }

        val categoryStr = obj.optString("category", "UNKNOWN").uppercase()
        val category = try {
            HiveTrack.Category.valueOf(categoryStr)
        } catch (e: Exception) {
            HiveTrack.Category.UNKNOWN
        }

        return HiveTrack(
            id = obj.getString("id"),
            sourcePlatform = obj.optString("source_platform", "unknown"),
            cellId = obj.optString("cell_id", null).takeIf { it?.isNotEmpty() == true },
            formationId = obj.optString("formation_id", null).takeIf { it?.isNotEmpty() == true },
            lat = obj.getDouble("lat"),
            lon = obj.getDouble("lon"),
            hae = if (obj.has("hae") && !obj.isNull("hae")) obj.getDouble("hae") else null,
            cep = if (obj.has("cep") && !obj.isNull("cep")) obj.getDouble("cep") else null,
            heading = if (obj.has("heading") && !obj.isNull("heading")) obj.getDouble("heading") else null,
            speed = if (obj.has("speed") && !obj.isNull("speed")) obj.getDouble("speed") else null,
            classification = obj.optString("classification", "a-u-G"),
            confidence = obj.optDouble("confidence", 0.5),
            category = category,
            attributes = attributes,
            createdAt = obj.optLong("created_at", System.currentTimeMillis()),
            lastUpdate = obj.optLong("last_update", System.currentTimeMillis())
        )
    }

    /**
     * Parse platforms JSON array into HivePlatform objects
     */
    private fun parsePlatformsJson(json: String): List<HivePlatform> {
        val platforms = mutableListOf<HivePlatform>()
        try {
            val arr = JSONArray(json)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                platforms.add(parsePlatformObject(obj))
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error parsing platforms JSON: ${e.message}")
        }
        return platforms
    }

    /**
     * Parse a single platform JSON object
     * Note: FFI uses "name" but Kotlin model uses "callsign"
     * Note: FFI uses "last_heartbeat" but Kotlin model uses "lastUpdate"
     */
    private fun parsePlatformObject(obj: JSONObject): HivePlatform {
        val capabilitiesArr = obj.optJSONArray("capabilities")
        val capabilities = mutableListOf<String>()
        if (capabilitiesArr != null) {
            for (i in 0 until capabilitiesArr.length()) {
                capabilities.add(capabilitiesArr.getString(i))
            }
        }

        val typeStr = obj.optString("platform_type", "UNKNOWN").uppercase()
        val platformType = try {
            HivePlatform.PlatformType.valueOf(typeStr)
        } catch (e: Exception) {
            HivePlatform.PlatformType.UNKNOWN
        }

        // Map FFI status to Kotlin status enum
        val statusStr = obj.optString("status", "READY").uppercase()
        val status = when (statusStr) {
            "READY" -> HivePlatform.Status.OPERATIONAL
            "ACTIVE" -> HivePlatform.Status.OPERATIONAL
            "DEGRADED" -> HivePlatform.Status.DEGRADED
            "OFFLINE" -> HivePlatform.Status.OFFLINE
            "LOADING" -> HivePlatform.Status.OPERATIONAL
            else -> HivePlatform.Status.OPERATIONAL
        }

        // FFI uses "name", Kotlin model uses "callsign"
        val callsign = obj.optString("name", obj.optString("callsign", "Unknown"))

        return HivePlatform(
            id = obj.getString("id"),
            callsign = callsign,
            platformType = platformType,
            lat = obj.getDouble("lat"),
            lon = obj.getDouble("lon"),
            hae = if (obj.has("hae")) obj.getDouble("hae") else null,
            heading = if (obj.has("heading")) obj.getDouble("heading") else null,
            speed = if (obj.has("speed")) obj.getDouble("speed") else null,
            cellId = obj.optString("cell_id", null).takeIf { it?.isNotEmpty() == true },
            capabilities = capabilities,
            status = status,
            // FFI uses "last_heartbeat"
            lastUpdate = obj.optLong("last_heartbeat", obj.optLong("last_update", System.currentTimeMillis()))
        )
    }

    /**
     * Select a cell for hierarchical navigation view.
     * @param cellId The cell ID to select, or null to clear selection
     * @param cellName The cell name for display
     */
    fun selectCell(cellId: String?, cellName: String? = null) {
        _selectedCellId = cellId
        _selectedCellName = cellName ?: cellId?.let { id ->
            _cells.find { it.id == id }?.name
        }
        Log.d(TAG, "Cell selected: $cellId ($cellName)")
        onCellSelectionChanged?.invoke(_selectedCellId, _selectedCellName)
    }

    /**
     * Clear the selected cell and return to all-cells view.
     */
    fun clearCellSelection() {
        selectCell(null, null)
    }

    /**
     * Get platforms filtered by the currently selected cell.
     * @return All platforms if no cell selected, otherwise only platforms in selected cell
     */
    fun getFilteredPlatforms(): List<HivePlatform> {
        val allPlatforms = platforms  // Uses the getter which includes BLE platforms
        val selectedId = _selectedCellId ?: return allPlatforms
        return allPlatforms.filter { it.cellId == selectedId }
    }

    /**
     * Zoom the map to show the specified cell bounds.
     * @param centerLat Center latitude
     * @param centerLon Center longitude
     * @param radiusMeters Radius in meters to show
     */
    fun zoomToCell(centerLat: Double, centerLon: Double, radiusMeters: Double) {
        try {
            val centerPoint = GeoPoint(centerLat, centerLon)
            // Calculate appropriate zoom scale based on radius
            // ATAK uses map scale where lower = more zoomed in
            // Roughly: scale = radiusMeters * 2 / screenWidthPixels * metersPerPixel
            // For simplicity, use a scale that shows ~2x the radius
            val zoomScale = (radiusMeters * 4.0).coerceIn(500.0, 100000.0)

            mapView.mapController.panTo(centerPoint, true)
            mapView.mapController.zoomTo(zoomScale, true)

            Log.i(TAG, "Zoomed to cell at ($centerLat, $centerLon) with radius ${radiusMeters}m")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to zoom to cell: ${e.message}", e)
        }
    }

    /**
     * Get the node manager - returns null in this simplified version
     */
    fun getNodeManager(): Any? = null

    /**
     * Get the number of track markers currently on the map
     */
    fun getMapMarkerCount(): Int = trackOverlay?.getMarkerCount() ?: 0

    /**
     * Get the number of cell visualizations currently on the map
     */
    fun getCellMarkerCount(): Int = cellOverlay?.getCellCount() ?: 0

    /**
     * Get the number of platform markers currently on the map
     */
    fun getPlatformMarkerCount(): Int = platformOverlay?.getMarkerCount() ?: 0

    /**
     * Force update of track markers on the map
     */
    fun updateMapMarkers() {
        trackOverlay?.updateTracks(_tracks)
        cellOverlay?.updateCellBounds(_cells, _platforms)
        platformOverlay?.updatePlatforms(_platforms, _cells)
    }

    /**
     * Enable or disable PLI (self-position) broadcasting to HIVE network.
     * @param enabled true to start broadcasting, false to stop
     */
    fun setPliBroadcastEnabled(enabled: Boolean) {
        _pliBroadcastEnabled = enabled
        if (enabled) {
            selfPositionBroadcaster?.start()
            Log.i(TAG, "PLI broadcast enabled")
        } else {
            selfPositionBroadcaster?.stop()
            _lastBroadcastStatus = "Disabled"
            Log.i(TAG, "PLI broadcast disabled")
        }
    }

    /**
     * Manually trigger a single PLI broadcast (for testing).
     */
    fun broadcastPliNow() {
        selfPositionBroadcaster?.broadcastNow()
    }

    /**
     * Broadcast initial position/callsign to BLE mesh so the tablet is visible to watches.
     * This sets hasPeripheral=true on the mesh so peers can resolve our callsign.
     *
     * Called once during initialization. For continuous updates, enable PLI broadcast.
     */
    private fun broadcastInitialPosition() {
        try {
            val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
            if (bleManager == null || !bleManager.isRunning.value) {
                Log.d(TAG, "BLE mesh not running - skipping initial position broadcast")
                return
            }

            val selfMarker = mapView.selfMarker
            val point = selfMarker?.point
            if (point == null) {
                Log.w(TAG, "No self marker position - will retry initial broadcast later")
                // Retry after a delay when self marker becomes available
                refreshHandler.postDelayed({
                    if (!_pliBroadcastEnabled) broadcastInitialPosition()
                }, 5000)
                return
            }

            val callsign = selfMarker.getMetaString("callsign", null)
                ?: selfMarker.title
                ?: "ATAK"

            bleManager.broadcastPosition(
                lat = point.latitude,
                lon = point.longitude,
                alt = if (point.isAltitudeValid) point.altitude else 0.0,
                callsign = callsign
            )

            Log.i(TAG, "Broadcast initial position: callsign=$callsign at (${point.latitude}, ${point.longitude})")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to broadcast initial position: ${e.message}", e)
        }
    }

    /**
     * Zoom the map to focus on a specific location.
     * @param lat Latitude
     * @param lon Longitude
     * @param zoomScale Zoom scale (lower = more zoomed in)
     */
    fun zoomToLocation(lat: Double, lon: Double, zoomScale: Double = 1000.0) {
        try {
            val point = GeoPoint(lat, lon)
            mapView.mapController.panTo(point, true)
            mapView.mapController.zoomTo(zoomScale, true)
            Log.i(TAG, "Zoomed to location ($lat, $lon) at scale $zoomScale")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to zoom to location: ${e.message}", e)
        }
    }

    /**
     * Zoom the map to focus on a specific marker by UID.
     * @param markerUid The marker UID to zoom to
     * @return true if marker was found and zoomed to, false otherwise
     */
    fun zoomToMarker(markerUid: String): Boolean {
        val cached = _markerCache[markerUid] ?: return false
        val marker = cached.marker
        zoomToLocation(marker.lat.toDouble(), marker.lon.toDouble())
        return true
    }

    /**
     * Connection status enumeration
     */
    enum class ConnectionStatus {
        DISCONNECTED,
        CONNECTING,
        CONNECTED,
        ERROR
    }
}
