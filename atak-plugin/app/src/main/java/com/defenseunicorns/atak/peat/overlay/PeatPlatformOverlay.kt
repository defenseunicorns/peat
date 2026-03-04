/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.atak.peat.overlay

import android.graphics.Color
import com.atakmap.android.maps.MapGroup
import com.atakmap.android.maps.MapView
import com.atakmap.android.maps.Marker
import com.atakmap.coremap.log.Log
import com.atakmap.coremap.maps.coords.GeoPoint
import com.defenseunicorns.atak.peat.model.PeatCell
import com.defenseunicorns.atak.peat.model.PeatPlatform

/**
 * Manages PEAT platform markers on the ATAK map.
 *
 * Platforms are displayed as markers with:
 * - Callsign label including cell membership (e.g., "HAWK-1 [Atlanta]")
 * - Icon based on platform type (UAV, UGV, soldier, etc.)
 * - Color based on cell membership (platforms in same cell share color)
 * - Status indicator in metadata
 *
 * This provides visibility of individual assets with cell grouping context.
 */
class PeatPlatformOverlay(private val mapView: MapView) {

    companion object {
        private const val TAG = "PeatPlatformOverlay"
        private const val GROUP_NAME = "Peat Platforms"
        private const val STALE_THRESHOLD_MS = 5 * 60 * 1000L // 5 minutes

        // Emergency color - bright red for SOS alerts
        private val EMERGENCY_COLOR = Color.parseColor("#FF0000")

        // Cell colors for grouping (cycle through these)
        private val CELL_COLORS = listOf(
            Color.parseColor("#2196F3"),  // Blue
            Color.parseColor("#4CAF50"),  // Green
            Color.parseColor("#FF9800"),  // Orange
            Color.parseColor("#9C27B0"),  // Purple
            Color.parseColor("#00BCD4"),  // Cyan
            Color.parseColor("#E91E63"),  // Pink
            Color.parseColor("#FFEB3B"),  // Yellow
            Color.parseColor("#795548")   // Brown
        )
    }

    private var mapGroup: MapGroup? = null
    private val platformMarkers = mutableMapOf<String, Marker>()
    private val cellColorMap = mutableMapOf<String, Int>()
    private var nextColorIndex = 0

    init {
        initMapGroup()
    }

    private fun initMapGroup() {
        val rootGroup = mapView.rootGroup
        if (rootGroup == null) {
            Log.e(TAG, "Root map group is null")
            return
        }

        // Find or create PEAT Platforms group
        mapGroup = rootGroup.findMapGroup(GROUP_NAME)
        if (mapGroup == null) {
            mapGroup = rootGroup.addGroup(GROUP_NAME)
            Log.i(TAG, "Created PEAT Platforms map group")
        } else {
            Log.d(TAG, "Found existing PEAT Platforms map group")
        }
    }

    /**
     * Update all platform markers from the provided list.
     * Adds new markers, updates existing ones, removes old ones.
     *
     * @param platforms List of platforms to display
     * @param cells List of cells for name lookup
     */
    fun updatePlatforms(platforms: List<PeatPlatform>, cells: List<PeatCell>) {
        val group = mapGroup ?: run {
            Log.w(TAG, "Map group not initialized")
            return
        }

        // Build cell ID to name map
        val cellNameMap = cells.associate { it.id to it.name }

        val currentPlatformIds = platforms.map { it.id }.toSet()
        val now = System.currentTimeMillis()

        // Remove markers for platforms no longer in the list
        val toRemove = platformMarkers.keys.filter { platformId ->
            platformId !in currentPlatformIds
        }
        toRemove.forEach { platformId ->
            removeMarker(platformId)
        }

        // Add or update markers for current platforms
        platforms.forEach { platform ->
            // Skip stale platforms (no recent heartbeat)
            if (now - platform.lastUpdate > STALE_THRESHOLD_MS) {
                Log.d(TAG, "Skipping stale platform: ${platform.id}")
                removeMarker(platform.id)
                return@forEach
            }

            val existingMarker = platformMarkers[platform.id]
            if (existingMarker != null) {
                updateMarker(existingMarker, platform, cellNameMap)
            } else {
                createMarker(platform, cellNameMap)
            }
        }

        Log.d(TAG, "Updated ${platformMarkers.size} platform markers")
    }

    /**
     * Create a new marker for a platform.
     */
    private fun createMarker(platform: PeatPlatform, cellNameMap: Map<String, String>) {
        val group = mapGroup ?: return

        try {
            val uid = platform.toCotUid()
            val point = GeoPoint(platform.lat, platform.lon, platform.hae ?: 0.0)

            val marker = Marker(point, uid)
            marker.type = platform.toCotType()

            // Build title with cell name
            val cellName = platform.cellId?.let { cellNameMap[it] }
            marker.title = buildTitle(platform, cellName)

            // Apply cell-based color
            applyMarkerStyle(marker, platform)

            // Add metadata
            marker.setMetaString("peatPlatformId", platform.id)
            marker.setMetaString("callsign", platform.callsign)
            marker.setMetaString("platformType", platform.platformType.name)
            marker.setMetaString("status", platform.status.name)
            platform.cellId?.let { marker.setMetaString("cellId", it) }
            cellName?.let { marker.setMetaString("cellName", it) }
            marker.setMetaString("capabilities", platform.capabilities.joinToString(", "))
            marker.setMetaLong("lastUpdate", platform.lastUpdate)

            // Heading and speed
            platform.heading?.let { marker.setMetaDouble("course", it) }
            platform.speed?.let { marker.setMetaDouble("speed", it) }

            group.addItem(marker)
            platformMarkers[platform.id] = marker
            Log.d(TAG, "Created marker for platform: ${platform.callsign} (${platform.id})")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create marker for platform ${platform.id}: ${e.message}", e)
        }
    }

    /**
     * Update an existing marker with new platform data.
     */
    private fun updateMarker(marker: Marker, platform: PeatPlatform, cellNameMap: Map<String, String>) {
        try {
            val point = GeoPoint(platform.lat, platform.lon, platform.hae ?: 0.0)
            marker.point = point

            // Update title with cell name
            val cellName = platform.cellId?.let { cellNameMap[it] }
            marker.title = buildTitle(platform, cellName)

            // Update style
            applyMarkerStyle(marker, platform)

            // Update metadata
            marker.setMetaString("status", platform.status.name)
            marker.setMetaLong("lastUpdate", platform.lastUpdate)
            platform.heading?.let { marker.setMetaDouble("course", it) }
            platform.speed?.let { marker.setMetaDouble("speed", it) }

            Log.v(TAG, "Updated marker for platform: ${platform.callsign}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to update marker for platform ${platform.id}: ${e.message}", e)
        }
    }

    /**
     * Build marker title with callsign and optional cell name.
     */
    private fun buildTitle(platform: PeatPlatform, cellName: String?): String {
        val base = platform.callsign
        return if (cellName != null) {
            // Shorten cell name for display
            val shortCellName = cellName.split(" ").firstOrNull() ?: cellName
            "$base [$shortCellName]"
        } else {
            base
        }
    }

    /**
     * Apply visual style to marker based on platform and cell.
     */
    private fun applyMarkerStyle(marker: Marker, platform: PeatPlatform) {
        // EMERGENCY status overrides all other colors - bright red
        if (platform.status == PeatPlatform.Status.EMERGENCY) {
            marker.setColor(EMERGENCY_COLOR)
            marker.setMetaInteger("color", EMERGENCY_COLOR)
            // Update title to indicate emergency
            val currentTitle = marker.title ?: platform.callsign
            if (!currentTitle.contains("SOS")) {
                marker.title = "⚠ SOS: $currentTitle"
            }
            return
        }

        // Get color based on cell membership
        val color = if (platform.cellId != null) {
            getColorForCell(platform.cellId)
        } else {
            Color.GRAY // No cell = gray
        }

        // Dim the color if status is degraded/offline
        val finalColor = when (platform.status) {
            PeatPlatform.Status.DEGRADED,
            PeatPlatform.Status.LOST_COMMS,
            PeatPlatform.Status.LOW_POWER -> dimColor(color)
            PeatPlatform.Status.OFFLINE -> Color.DKGRAY
            else -> color
        }

        marker.setColor(finalColor)
        marker.setMetaInteger("color", finalColor)
    }

    /**
     * Get a consistent color for a cell ID.
     */
    private fun getColorForCell(cellId: String): Int {
        return cellColorMap.getOrPut(cellId) {
            val color = CELL_COLORS[nextColorIndex % CELL_COLORS.size]
            nextColorIndex++
            color
        }
    }

    /**
     * Dim a color by reducing brightness.
     */
    private fun dimColor(color: Int): Int {
        val r = (Color.red(color) * 0.6).toInt()
        val g = (Color.green(color) * 0.6).toInt()
        val b = (Color.blue(color) * 0.6).toInt()
        return Color.rgb(r, g, b)
    }

    /**
     * Remove a marker by platform ID.
     */
    private fun removeMarker(platformId: String) {
        platformMarkers.remove(platformId)?.let { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
            Log.d(TAG, "Removed marker for platform: $platformId")
        }
    }

    /**
     * Remove all platform markers.
     */
    fun clearAll() {
        platformMarkers.values.forEach { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
        }
        platformMarkers.clear()
        cellColorMap.clear()
        nextColorIndex = 0
        Log.i(TAG, "Cleared all platform markers")
    }

    /**
     * Get the number of active platform markers.
     */
    fun getMarkerCount(): Int = platformMarkers.size

    /**
     * Dispose of the overlay and clean up resources.
     */
    fun dispose() {
        clearAll()
        mapGroup?.let { group ->
            mapView.rootGroup?.removeGroup(group)
        }
        mapGroup = null
        Log.i(TAG, "PeatPlatformOverlay disposed")
    }
}
