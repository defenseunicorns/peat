package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import com.atakmap.android.dropdown.DropDownMapComponent
import com.atakmap.android.ipc.AtakBroadcast.DocumentedIntentFilter
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.revolveteam.atak.hive.model.HiveCell
import com.revolveteam.atak.hive.model.HivePlatform
import com.revolveteam.atak.hive.model.HiveTrack
import com.revolveteam.atak.hive.overlay.HiveCellOverlay
import com.revolveteam.atak.hive.overlay.HiveTrackOverlay
import org.json.JSONArray
import org.json.JSONObject

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
    val platforms: List<HivePlatform> get() = _platforms.toList()

    private val _tracks = mutableListOf<HiveTrack>()
    val tracks: List<HiveTrack> get() = _tracks.toList()

    private var _connectionStatus = ConnectionStatus.DISCONNECTED
    val connectionStatus: ConnectionStatus get() = _connectionStatus

    val peerCount: Int get() = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0

    override fun onCreate(context: Context, intent: Intent, view: MapView) {
        context.setTheme(R.style.ATAKPluginTheme)
        super.onCreate(context, intent, view)

        pluginContext = context
        mapView = view
        Log.d(TAG, "HiveMapComponent onCreate")

        // Create track overlay for map markers
        trackOverlay = HiveTrackOverlay(view)
        Log.d(TAG, "Track overlay created")

        // Create cell overlay for cell boundaries
        cellOverlay = HiveCellOverlay(view)
        Log.d(TAG, "Cell overlay created")

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

        // Update connection status based on HIVE node availability
        updateConnectionStatus()

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
        super.onDestroyImpl(context, view)
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
                // Update map overlays
                trackOverlay?.updateTracks(_tracks)
                cellOverlay?.updateCells(_cells)
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
     * Update connection status based on HIVE node availability and peer count
     */
    private fun updateConnectionStatus() {
        val node = HivePluginLifecycle.getInstance()?.getHiveNodeJni()
        val currentPeerCount = peerCount  // Use the property which gets live peer count
        _connectionStatus = when {
            node == null -> ConnectionStatus.DISCONNECTED
            currentPeerCount > 0 -> ConnectionStatus.CONNECTED
            else -> ConnectionStatus.CONNECTING  // Node exists but no peers
        }
        Log.d(TAG, "Connection status: $_connectionStatus (peers: $currentPeerCount)")
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
            Log.w(TAG, "No HIVE node available - lifecycle=$lifecycle, node=$node")
            _cells.clear()
            _platforms.clear()
            _tracks.clear()
            return
        }

        // Fetch cells from HIVE sync
        try {
            val cellsJson = node.getCellsJson()
            Log.d(TAG, "Cells JSON: $cellsJson")
            _cells.clear()
            _cells.addAll(parseCellsJson(cellsJson))
            Log.i(TAG, "Loaded ${_cells.size} cells from HIVE")
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
     * Select a cell
     */
    fun selectCell(cellId: String) {
        Log.d(TAG, "Cell selected: $cellId")
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
     * Force update of track markers on the map
     */
    fun updateMapMarkers() {
        trackOverlay?.updateTracks(_tracks)
        cellOverlay?.updateCells(_cells)
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
     * Connection status enumeration
     */
    enum class ConnectionStatus {
        DISCONNECTED,
        CONNECTING,
        CONNECTED,
        ERROR
    }
}
