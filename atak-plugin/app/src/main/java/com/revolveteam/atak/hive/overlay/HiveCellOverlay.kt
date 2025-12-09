package com.revolveteam.atak.hive.overlay

import android.graphics.Color
import com.atakmap.android.maps.MapGroup
import com.atakmap.android.maps.MapView
import com.atakmap.android.maps.Marker
import com.atakmap.coremap.log.Log
import com.atakmap.coremap.maps.coords.GeoPoint
import com.revolveteam.atak.hive.model.HiveCell

/**
 * Manages HIVE cell visualizations on the ATAK map.
 *
 * Cells are displayed as:
 * - Center marker with cell name label and platform count
 * - Color based on status (active=green, degraded=yellow, offline=red)
 *
 * Note: Circle boundaries require ATAK Drawing tools which may not be available
 * in all ATAK versions. For now, cells are shown as labeled markers.
 */
class HiveCellOverlay(private val mapView: MapView) {

    companion object {
        private const val TAG = "HiveCellOverlay"
        private const val GROUP_NAME = "HIVE Cells"
    }

    private var mapGroup: MapGroup? = null
    private val cellMarkers = mutableMapOf<String, Marker>()

    init {
        initMapGroup()
    }

    private fun initMapGroup() {
        val rootGroup = mapView.rootGroup
        if (rootGroup == null) {
            Log.e(TAG, "Root map group is null")
            return
        }

        // Find or create HIVE Cells group
        mapGroup = rootGroup.findMapGroup(GROUP_NAME)
        if (mapGroup == null) {
            mapGroup = rootGroup.addGroup(GROUP_NAME)
            Log.i(TAG, "Created HIVE Cells map group")
        } else {
            Log.d(TAG, "Found existing HIVE Cells map group")
        }
    }

    /**
     * Update all cell visualizations from the provided list.
     * Adds new cells, updates existing ones, removes old ones.
     */
    fun updateCells(cells: List<HiveCell>) {
        val group = mapGroup ?: run {
            Log.w(TAG, "Map group not initialized")
            return
        }

        val currentCellIds = cells.map { it.id }.toSet()

        // Remove visualizations for cells no longer in the list
        val toRemove = cellMarkers.keys.filter { cellId ->
            cellId !in currentCellIds
        }
        toRemove.forEach { cellId ->
            removeCell(cellId)
        }

        // Add or update visualizations for current cells
        cells.forEach { cell ->
            if (cellMarkers.containsKey(cell.id)) {
                updateCell(cell)
            } else {
                createCell(cell)
            }
        }

        Log.d(TAG, "Updated ${cellMarkers.size} cell visualizations")
    }

    /**
     * Create visualizations for a new cell.
     */
    private fun createCell(cell: HiveCell) {
        val group = mapGroup ?: return

        try {
            val uid = cell.toCotUid()
            val centerPoint = GeoPoint(cell.centerLat, cell.centerLon)

            // Create center marker with cell name
            val marker = Marker(centerPoint, uid)
            marker.type = cell.toCotType()
            marker.title = "${cell.name} (${cell.platformCount} platforms)"

            // Add metadata
            marker.setMetaString("hiveCellId", cell.id)
            marker.setMetaInteger("platformCount", cell.platformCount)
            marker.setMetaString("status", cell.status.name)
            cell.leaderId?.let { marker.setMetaString("leaderId", it) }
            cell.formationId?.let { marker.setMetaString("formationId", it) }
            marker.setMetaString("capabilities", cell.capabilities.joinToString(", "))

            // Style based on status
            val statusColor = getStatusColor(cell.status)
            marker.setMetaInteger("color", statusColor)

            group.addItem(marker)
            cellMarkers[cell.id] = marker

            Log.d(TAG, "Created cell visualization: ${cell.id} (${cell.name})")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create cell visualization ${cell.id}: ${e.message}", e)
        }
    }

    /**
     * Update an existing cell's visualizations.
     */
    private fun updateCell(cell: HiveCell) {
        try {
            // Update marker
            cellMarkers[cell.id]?.let { marker ->
                val centerPoint = GeoPoint(cell.centerLat, cell.centerLon)
                marker.point = centerPoint
                marker.title = "${cell.name} (${cell.platformCount} platforms)"

                val statusColor = getStatusColor(cell.status)
                marker.setMetaInteger("color", statusColor)
                marker.setMetaInteger("platformCount", cell.platformCount)
                marker.setMetaString("status", cell.status.name)
                marker.setMetaString("capabilities", cell.capabilities.joinToString(", "))
            }

            Log.v(TAG, "Updated cell: ${cell.id}")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to update cell ${cell.id}: ${e.message}", e)
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
     * Remove all visualizations for a cell.
     */
    private fun removeCell(cellId: String) {
        // Remove marker
        cellMarkers.remove(cellId)?.let { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
        }

        Log.d(TAG, "Removed cell visualization: $cellId")
    }

    /**
     * Remove all cell visualizations.
     */
    fun clearAll() {
        cellMarkers.values.forEach { marker ->
            mapGroup?.removeItem(marker)
            marker.dispose()
        }
        cellMarkers.clear()

        Log.i(TAG, "Cleared all cell visualizations")
    }

    /**
     * Get the number of active cell visualizations.
     */
    fun getCellCount(): Int = cellMarkers.size

    /**
     * Dispose of the overlay and clean up resources.
     */
    fun dispose() {
        clearAll()
        mapGroup?.let { group ->
            mapView.rootGroup?.removeGroup(group)
        }
        mapGroup = null
        Log.i(TAG, "HiveCellOverlay disposed")
    }
}
