package com.revolveteam.atak.hive.overlay

import android.graphics.Color
import com.atakmap.android.maps.MapGroup
import com.atakmap.android.maps.MapItem
import com.atakmap.android.maps.MapView
import com.atakmap.android.maps.Marker
import com.atakmap.android.maps.PointMapItem
import com.atakmap.coremap.log.Log
import com.atakmap.coremap.maps.coords.GeoPoint
import com.revolveteam.atak.hive.model.HiveTrack

/**
 * Manages HIVE track markers on the ATAK map.
 *
 * Tracks are displayed as markers with:
 * - Icon based on classification (person, vehicle, etc.)
 * - Color based on affiliation (hostile=red, friendly=green, unknown=yellow)
 * - Label showing track ID and confidence
 * - Automatic removal when stale (no update > 5 minutes)
 */
class HiveTrackOverlay(private val mapView: MapView) {

    companion object {
        private const val TAG = "HiveTrackOverlay"
        private const val GROUP_NAME = "HIVE Tracks"
        private const val STALE_THRESHOLD_MS = 5 * 60 * 1000L // 5 minutes
    }

    private var mapGroup: MapGroup? = null
    private val trackMarkers = mutableMapOf<String, Marker>()

    init {
        initMapGroup()
    }

    private fun initMapGroup() {
        val rootGroup = mapView.rootGroup
        if (rootGroup == null) {
            Log.e(TAG, "Root map group is null")
            return
        }

        // Find or create HIVE Tracks group
        mapGroup = rootGroup.findMapGroup(GROUP_NAME)
        if (mapGroup == null) {
            mapGroup = rootGroup.addGroup(GROUP_NAME)
            Log.i(TAG, "Created HIVE Tracks map group")
        } else {
            Log.d(TAG, "Found existing HIVE Tracks map group")
        }
    }

    /**
     * Update all track markers from the provided list.
     * Adds new markers, updates existing ones, removes stale ones.
     */
    fun updateTracks(tracks: List<HiveTrack>) {
        val group = mapGroup ?: run {
            Log.w(TAG, "Map group not initialized")
            return
        }

        val currentTrackIds = tracks.map { it.id }.toSet()
        val now = System.currentTimeMillis()

        // Remove markers for tracks no longer in the list or stale
        val toRemove = trackMarkers.keys.filter { trackId ->
            trackId !in currentTrackIds
        }
        toRemove.forEach { trackId ->
            removeMarker(trackId)
        }

        // Add or update markers for current tracks
        tracks.forEach { track ->
            // Skip stale tracks
            if (track.isStale(STALE_THRESHOLD_MS)) {
                Log.d(TAG, "Skipping stale track: ${track.id}")
                removeMarker(track.id)
                return@forEach
            }

            val existingMarker = trackMarkers[track.id]
            if (existingMarker != null) {
                updateMarker(existingMarker, track)
            } else {
                createMarker(track)
            }
        }

        Log.d(TAG, "Updated ${trackMarkers.size} track markers")
    }

    /**
     * Create a new marker for a track.
     */
    private fun createMarker(track: HiveTrack) {
        val group = mapGroup ?: return

        try {
            val uid = track.toCotUid()
            val point = GeoPoint(track.lat, track.lon, track.hae ?: 0.0)

            val marker = Marker(point, uid)
            marker.type = track.toCotType()
            // Use callsign from attributes, fallback to track ID
            val callsign = track.attributes["callsign"] ?: track.id.takeLast(8)
            marker.title = callsign

            // Set marker style based on classification
            applyMarkerStyle(marker, track)

            // Add metadata for tap-to-view details
            marker.setMetaString("hiveTackId", track.id)
            marker.setMetaString("sourcePlatform", track.sourcePlatform)
            marker.setMetaDouble("confidence", track.confidence)
            marker.setMetaString("category", track.category.name)
            marker.setMetaString("classification", track.classification)

            // Add attributes as metadata
            track.attributes.forEach { (key, value) ->
                marker.setMetaString("attr_$key", value)
            }

            group.addItem(marker)
            trackMarkers[track.id] = marker
            Log.d(TAG, "Created marker for track: ${track.id}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create marker for track ${track.id}: ${e.message}", e)
        }
    }

    /**
     * Update an existing marker with new track data.
     */
    private fun updateMarker(marker: Marker, track: HiveTrack) {
        try {
            val point = GeoPoint(track.lat, track.lon, track.hae ?: 0.0)
            marker.point = point

            // Update title with callsign
            val callsign = track.attributes["callsign"] ?: track.id.takeLast(8)
            marker.title = callsign

            // Update style if classification changed
            applyMarkerStyle(marker, track)

            // Update metadata
            marker.setMetaDouble("confidence", track.confidence)
            marker.setMetaString("category", track.category.name)

            Log.v(TAG, "Updated marker for track: ${track.id}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to update marker for track ${track.id}: ${e.message}", e)
        }
    }

    /**
     * Apply visual style to marker based on track classification.
     */
    private fun applyMarkerStyle(marker: Marker, track: HiveTrack) {
        // Color based on affiliation in classification
        val color = when {
            track.classification.contains("-h-") -> Color.RED      // Hostile
            track.classification.contains("-f-") -> Color.GREEN    // Friendly
            track.classification.contains("-n-") -> Color.CYAN     // Neutral
            else -> Color.YELLOW                                    // Unknown
        }

        marker.setMetaInteger("color", color)

        // Icon type based on category
        val iconUri = when (track.category) {
            HiveTrack.Category.PERSON -> "asset://icons/person.png"
            HiveTrack.Category.VEHICLE -> "asset://icons/vehicle.png"
            HiveTrack.Category.AIRCRAFT -> "asset://icons/aircraft.png"
            HiveTrack.Category.VESSEL -> "asset://icons/vessel.png"
            else -> "asset://icons/unknown.png"
        }

        // Set track/course if available (store as metadata)
        track.heading?.let { heading ->
            marker.setMetaDouble("course", heading)
        }

        track.speed?.let { speed ->
            marker.setMetaDouble("speed", speed)
        }
    }

    /**
     * Remove a marker by track ID.
     */
    private fun removeMarker(trackId: String) {
        trackMarkers.remove(trackId)?.let { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
            Log.d(TAG, "Removed marker for track: $trackId")
        }
    }

    /**
     * Remove all track markers.
     */
    fun clearAll() {
        trackMarkers.values.forEach { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
        }
        trackMarkers.clear()
        Log.i(TAG, "Cleared all track markers")
    }

    /**
     * Get the number of active track markers.
     */
    fun getMarkerCount(): Int = trackMarkers.size

    /**
     * Dispose of the overlay and clean up resources.
     */
    fun dispose() {
        clearAll()
        mapGroup?.let { group ->
            mapView.rootGroup?.removeGroup(group)
        }
        mapGroup = null
        Log.i(TAG, "HiveTrackOverlay disposed")
    }
}
