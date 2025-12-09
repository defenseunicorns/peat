package com.revolveteam.atak.hive.overlay

import android.graphics.Color
import com.atakmap.android.drawing.mapItems.DrawingCircle
import com.atakmap.android.maps.MapGroup
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.atakmap.coremap.maps.coords.GeoPoint
import com.atakmap.coremap.maps.coords.GeoPointMetaData
import com.revolveteam.atak.hive.model.HiveCell
import com.revolveteam.atak.hive.model.HivePlatform
import java.util.UUID
import kotlin.math.*

/**
 * Manages HIVE cell bounding circle visualizations on the ATAK map.
 *
 * Cells are displayed as dynamic bounding circles that encompass all platforms
 * belonging to the cell. The circle updates as platforms move.
 *
 * - Circle color based on cell status (active=green, degraded=yellow, offline=red)
 * - Circle is semi-transparent with a colored border
 * - Label shows cell name
 */
class HiveCellOverlay(private val mapView: MapView) {

    companion object {
        private const val TAG = "HiveCellOverlay"
        private const val GROUP_NAME = "HIVE Cells"
        private const val MIN_RADIUS_METERS = 100.0 // Minimum circle radius
        private const val PADDING_FACTOR = 1.2 // 20% padding around platforms
    }

    private var mapGroup: MapGroup? = null
    private val cellCircles = mutableMapOf<String, DrawingCircle>()

    init {
        initMapGroup()
    }

    private fun initMapGroup() {
        val rootGroup = mapView.rootGroup
        if (rootGroup == null) {
            Log.e(TAG, "Root map group is null")
            return
        }

        // Find or create HIVE Cells group under Drawing Objects
        var drawingGroup = rootGroup.findMapGroup("Drawing Objects")
        if (drawingGroup == null) {
            drawingGroup = rootGroup.addGroup("Drawing Objects")
            Log.i(TAG, "Created Drawing Objects group")
        }

        mapGroup = drawingGroup.findMapGroup(GROUP_NAME)
        if (mapGroup == null) {
            mapGroup = drawingGroup.addGroup(GROUP_NAME)
            Log.i(TAG, "Created HIVE Cells map group")
        } else {
            Log.d(TAG, "Found existing HIVE Cells map group")
        }
    }

    /**
     * Update cell bounding circles based on platform positions.
     * Groups platforms by cell and draws a bounding circle around each group.
     *
     * @param cells List of cells for metadata (name, status)
     * @param platforms List of platforms to calculate bounds from
     */
    fun updateCellBounds(cells: List<HiveCell>, platforms: List<HivePlatform>) {
        val group = mapGroup ?: run {
            Log.w(TAG, "Map group not initialized")
            return
        }

        // Group platforms by cell ID
        val platformsByCell = platforms.groupBy { it.cellId }

        // Build cell ID to cell map for metadata lookup
        val cellMap = cells.associateBy { it.id }

        val currentCellIds = platformsByCell.keys.filterNotNull().toSet()

        // Remove circles for cells with no platforms
        val toRemove = cellCircles.keys.filter { cellId ->
            cellId !in currentCellIds
        }
        toRemove.forEach { cellId ->
            removeCircle(cellId)
        }

        // Create or update circles for each cell with platforms
        platformsByCell.forEach { (cellId, cellPlatforms) ->
            if (cellId == null || cellPlatforms.isEmpty()) return@forEach

            val cell = cellMap[cellId]
            val existingCircle = cellCircles[cellId]

            if (existingCircle != null) {
                updateCircle(existingCircle, cellPlatforms, cell)
            } else {
                createCircle(cellId, cellPlatforms, cell)
            }
        }

        Log.d(TAG, "Updated ${cellCircles.size} cell bounding circles")
    }

    /**
     * Calculate the minimum bounding circle for a set of platforms.
     * Returns (centerLat, centerLon, radiusMeters)
     */
    private fun calculateBoundingCircle(platforms: List<HivePlatform>): Triple<Double, Double, Double> {
        if (platforms.isEmpty()) {
            return Triple(0.0, 0.0, MIN_RADIUS_METERS)
        }

        if (platforms.size == 1) {
            return Triple(platforms[0].lat, platforms[0].lon, MIN_RADIUS_METERS)
        }

        // Calculate centroid
        var sumLat = 0.0
        var sumLon = 0.0
        platforms.forEach { p ->
            sumLat += p.lat
            sumLon += p.lon
        }
        val centerLat = sumLat / platforms.size
        val centerLon = sumLon / platforms.size

        // Find maximum distance from center to any platform
        var maxDistance = 0.0
        platforms.forEach { p ->
            val distance = haversineDistance(centerLat, centerLon, p.lat, p.lon)
            if (distance > maxDistance) {
                maxDistance = distance
            }
        }

        // Apply padding and minimum radius
        val radius = max(maxDistance * PADDING_FACTOR, MIN_RADIUS_METERS)

        return Triple(centerLat, centerLon, radius)
    }

    /**
     * Calculate haversine distance in meters between two points.
     */
    private fun haversineDistance(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val R = 6371000.0 // Earth radius in meters

        val lat1Rad = Math.toRadians(lat1)
        val lat2Rad = Math.toRadians(lat2)
        val dLat = Math.toRadians(lat2 - lat1)
        val dLon = Math.toRadians(lon2 - lon1)

        val a = sin(dLat / 2).pow(2) + cos(lat1Rad) * cos(lat2Rad) * sin(dLon / 2).pow(2)
        val c = 2 * atan2(sqrt(a), sqrt(1 - a))

        return R * c
    }

    /**
     * Create a new bounding circle for a cell.
     */
    private fun createCircle(cellId: String, platforms: List<HivePlatform>, cell: HiveCell?) {
        val group = mapGroup ?: return

        try {
            val (centerLat, centerLon, radius) = calculateBoundingCircle(platforms)
            val uid = "HIVE-CELL-CIRCLE-$cellId"

            val circle = DrawingCircle(mapView, uid)
            circle.setCenterPoint(GeoPointMetaData.wrap(GeoPoint(centerLat, centerLon)))
            circle.setRadius(radius)

            // Style the circle
            val statusColor = cell?.let { getStatusColor(it.status) } ?: Color.GRAY
            circle.strokeColor = statusColor
            circle.fillColor = Color.argb(40, Color.red(statusColor), Color.green(statusColor), Color.blue(statusColor))
            circle.strokeWeight = 2.0

            // Set title/label
            val cellName = cell?.name ?: "Cell $cellId"
            circle.title = cellName
            circle.setMetaString("hiveCellId", cellId)
            circle.setMetaInteger("platformCount", platforms.size)

            group.addItem(circle)
            cellCircles[cellId] = circle

            Log.d(TAG, "Created bounding circle for cell: $cellId (${platforms.size} platforms, ${radius.toInt()}m radius)")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create bounding circle for cell $cellId: ${e.message}", e)
        }
    }

    /**
     * Update an existing bounding circle.
     */
    private fun updateCircle(circle: DrawingCircle, platforms: List<HivePlatform>, cell: HiveCell?) {
        try {
            val (centerLat, centerLon, radius) = calculateBoundingCircle(platforms)

            circle.setCenterPoint(GeoPointMetaData.wrap(GeoPoint(centerLat, centerLon)))
            circle.setRadius(radius)

            // Update style
            val statusColor = cell?.let { getStatusColor(it.status) } ?: Color.GRAY
            circle.strokeColor = statusColor
            circle.fillColor = Color.argb(40, Color.red(statusColor), Color.green(statusColor), Color.blue(statusColor))

            // Update metadata
            circle.setMetaInteger("platformCount", platforms.size)

            Log.v(TAG, "Updated bounding circle: ${platforms.size} platforms, ${radius.toInt()}m radius")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to update bounding circle: ${e.message}", e)
        }
    }

    /**
     * Get color for cell status.
     */
    private fun getStatusColor(status: HiveCell.Status): Int {
        return when (status) {
            HiveCell.Status.ACTIVE -> Color.parseColor("#4CAF50")   // Green
            HiveCell.Status.FORMING -> Color.parseColor("#2196F3")  // Blue
            HiveCell.Status.DEGRADED -> Color.parseColor("#FFC107") // Yellow/Amber
            HiveCell.Status.OFFLINE -> Color.parseColor("#F44336")  // Red
        }
    }

    /**
     * Remove a bounding circle.
     */
    private fun removeCircle(cellId: String) {
        cellCircles.remove(cellId)?.let { circle ->
            mapGroup?.removeItem(circle)
            circle.dispose()
            Log.d(TAG, "Removed bounding circle for cell: $cellId")
        }
    }

    /**
     * Legacy method for backward compatibility - calls updateCellBounds with empty platforms.
     */
    fun updateCells(cells: List<HiveCell>) {
        // This method is deprecated - use updateCellBounds instead
        Log.w(TAG, "updateCells called without platforms - circles will not be drawn")
    }

    /**
     * Remove all cell visualizations.
     */
    fun clearAll() {
        cellCircles.values.forEach { circle ->
            mapGroup?.removeItem(circle)
            circle.dispose()
        }
        cellCircles.clear()
        Log.i(TAG, "Cleared all cell bounding circles")
    }

    /**
     * Get the number of active cell visualizations.
     */
    fun getCellCount(): Int = cellCircles.size

    /**
     * Dispose of the overlay and clean up resources.
     */
    fun dispose() {
        clearAll()
        mapGroup?.let { group ->
            // Don't remove the Drawing Objects group as other components may use it
            mapView.rootGroup?.findMapGroup("Drawing Objects")?.removeGroup(group)
        }
        mapGroup = null
        Log.i(TAG, "HiveCellOverlay disposed")
    }
}
