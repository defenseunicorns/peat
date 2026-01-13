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
import com.revolveteam.hive.HiveDocument
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
        private const val BLE_STALE_TIMEOUT_MS = 30_000L  // 30 seconds - remove platforms with no updates
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

    // BLE peer platforms for cell membership (nodeId -> platform)
    private val _blePeerPlatforms = mutableMapOf<Long, HivePlatform>()

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
     * Handle synced document from BLE mesh peer - create/update track marker.
     */
    private fun onBleDocumentSynced(document: HiveDocument) {
        // Access peripheral data directly (more compatible with different AAR versions)
        val peripheral = document.peripheral ?: return
        val location = peripheral.location ?: return  // No location, nothing to display

        val nodeId = document.nodeId
        // Get callsign, trim whitespace, fallback to node ID if empty/blank
        val rawCallsign = peripheral.callsign?.trim() ?: ""
        val callsign = rawCallsign.takeIf { it.isNotBlank() }
            ?: "BLE-${String.format("%08X", nodeId).takeLast(4)}"

        Log.d(TAG, "BLE peer callsign: raw='$rawCallsign', resolved='$callsign'")
        val battery = peripheral.health.batteryPercent
        val heartRate = peripheral.health.heartRate

        // Get mesh_id early for logging (mesh_id == cell_id mapping per ADR-041)
        val meshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId()

        Log.i(TAG, "BLE document synced: nodeId=${String.format("%08X", nodeId)}, " +
                "callsign=$callsign, cell=$meshId, location=(${location.latitude}, ${location.longitude}), " +
                "battery=$battery%, heartRate=$heartRate")

        // Create/update platform for cell membership, Squad Summary, and map display
        // Note: BLE peers are shown as platforms only (not tracks) - tracks are for detected entities
        val platform = HivePlatform(
            id = "ble-${String.format("%08X", nodeId)}",
            callsign = callsign,
            platformType = HivePlatform.PlatformType.SOLDIER,
            lat = location.latitude.toDouble(),
            lon = location.longitude.toDouble(),
            hae = location.altitude.toDouble().takeIf { it != 0.0 },
            cellId = meshId,
            status = HivePlatform.Status.OPERATIONAL,
            batteryPercent = battery,
            lastUpdate = System.currentTimeMillis()
        )

        // Update the BLE peer platform map and refresh overlay
        refreshHandler.post {
            _blePeerPlatforms[nodeId] = platform
            // Ensure cell exists for this mesh_id
            meshId?.let { ensureCellExists(it) }
            updateBlePeerOverlay()
        }
    }

    /**
     * Ensure a cell exists for the given cell_id (mesh_id).
     * Creates a synthetic cell if one doesn't exist.
     * The tablet (full HIVE node) auto-assigns itself as cell leader.
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

        // Create synthetic cell for BLE mesh
        val bleCell = HiveCell(
            id = cellId,
            name = cellId,  // Use mesh_id as name
            status = HiveCell.Status.ACTIVE,
            platformCount = _blePeerPlatforms.size + 1,  // +1 for tablet
            centerLat = selfPoint?.latitude ?: _blePeerPlatforms.values.firstOrNull()?.lat ?: 0.0,
            centerLon = selfPoint?.longitude ?: _blePeerPlatforms.values.firstOrNull()?.lon ?: 0.0,
            capabilities = listOf("BLE_MESH", "GATEWAY"),
            formationId = null,
            leaderId = selfUid,  // Tablet is cell leader
            lastUpdate = System.currentTimeMillis()
        )
        _cells.add(bleCell)

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

        Log.i(TAG, "Created synthetic cell for BLE mesh: $cellId, leader=$selfCallsign ($selfUid)")
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

                // Clean up stale BLE platforms (based on timestamp, not unreliable connection state)
                cleanupStalePlatforms()

                // Ensure cell exists for current mesh if BLE platforms exist
                if (_blePeerPlatforms.isNotEmpty()) {
                    val meshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId()
                    meshId?.let { ensureCellExists(it) }
                } else {
                    // No BLE peers - remove synthetic BLE cell
                    val meshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId()
                    meshId?.let { removeCellIfEmpty(it) }
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
     * Remove BLE peer platforms that are stale (no recent document sync).
     * Uses ONLY timestamp-based staleness since BLE disconnect detection is unreliable.
     */
    private fun cleanupStalePlatforms() {
        val now = System.currentTimeMillis()
        val selfUidHash = mapView.selfMarker?.uid?.hashCode()?.toLong()

        // Find BLE platforms that are stale (no update in timeout period)
        val toRemove = _blePeerPlatforms.entries.filter { (nodeId, platform) ->
            if (nodeId == selfUidHash) return@filter false  // Never remove tablet itself

            val ageMs = now - platform.lastUpdate
            val isStale = ageMs > BLE_STALE_TIMEOUT_MS

            if (isStale) {
                Log.d(TAG, "Platform ${platform.callsign} is stale (${ageMs / 1000}s since update)")
            }

            isStale
        }.map { it.key }

        if (toRemove.isNotEmpty()) {
            toRemove.forEach { nodeId ->
                val platform = _blePeerPlatforms.remove(nodeId)
                Log.i(TAG, "Removed stale BLE peer: ${platform?.callsign} (${String.format("%08X", nodeId)})")
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
     * Connection status enumeration
     */
    enum class ConnectionStatus {
        DISCONNECTED,
        CONNECTING,
        CONNECTED,
        ERROR
    }
}
