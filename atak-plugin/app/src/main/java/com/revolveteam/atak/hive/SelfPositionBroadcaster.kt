package com.revolveteam.atak.hive

import android.os.Handler
import android.os.Looper
import com.atakmap.android.maps.MapView
import com.atakmap.android.maps.PointMapItem
import com.atakmap.coremap.log.Log
import org.json.JSONArray
import org.json.JSONObject

/**
 * Broadcasts ATAK self-position (PLI) to the HIVE network.
 *
 * This class monitors the device's self-marker in ATAK and periodically
 * publishes the position as a platform to the HIVE mesh network. This allows
 * other HIVE nodes (including the test client) to see the ATAK user's position.
 *
 * @param mapView The ATAK MapView to get self-marker from
 */
class SelfPositionBroadcaster(private val mapView: MapView) {

    companion object {
        private const val TAG = "SelfPositionBroadcaster"
        private const val DEFAULT_BROADCAST_INTERVAL_MS = 5000L // 5 seconds
        private const val MIN_BROADCAST_INTERVAL_MS = 1000L    // 1 second minimum
    }

    private val handler = Handler(Looper.getMainLooper())
    private var isRunning = false
    private var broadcastIntervalMs = DEFAULT_BROADCAST_INTERVAL_MS

    // Callback for broadcast events
    var onBroadcastCallback: ((success: Boolean, message: String) -> Unit)? = null

    /**
     * Start broadcasting self-position to HIVE network.
     * Does nothing if already running.
     */
    fun start() {
        if (isRunning) {
            Log.d(TAG, "Already running")
            return
        }

        isRunning = true
        Log.i(TAG, "Starting self-position broadcast (interval: ${broadcastIntervalMs}ms)")
        handler.post(broadcastRunnable)
    }

    /**
     * Stop broadcasting self-position.
     */
    fun stop() {
        if (!isRunning) {
            Log.d(TAG, "Not running")
            return
        }

        isRunning = false
        handler.removeCallbacks(broadcastRunnable)
        Log.i(TAG, "Stopped self-position broadcast")
    }

    /**
     * Check if broadcasting is active.
     */
    fun isActive(): Boolean = isRunning

    /**
     * Set the broadcast interval in milliseconds.
     * @param intervalMs Interval between broadcasts (minimum 1000ms)
     */
    fun setBroadcastInterval(intervalMs: Long) {
        broadcastIntervalMs = maxOf(intervalMs, MIN_BROADCAST_INTERVAL_MS)
        Log.d(TAG, "Broadcast interval set to ${broadcastIntervalMs}ms")
    }

    private val broadcastRunnable = object : Runnable {
        override fun run() {
            if (!isRunning) return

            try {
                broadcastSelfPosition()
            } catch (e: Exception) {
                Log.e(TAG, "Error in broadcast: ${e.message}", e)
                onBroadcastCallback?.invoke(false, "Error: ${e.message}")
            }

            // Schedule next broadcast
            if (isRunning) {
                handler.postDelayed(this, broadcastIntervalMs)
            }
        }
    }

    /**
     * Broadcast current self-position to HIVE network.
     */
    private fun broadcastSelfPosition() {
        // Get the self-marker from ATAK
        val selfMarker = mapView.selfMarker
        if (selfMarker == null) {
            Log.w(TAG, "No self-marker available")
            onBroadcastCallback?.invoke(false, "No self-marker")
            return
        }

        // Extract position data
        val point = selfMarker.point
        if (point == null) {
            Log.w(TAG, "Self-marker has no position")
            onBroadcastCallback?.invoke(false, "No position")
            return
        }

        val uid = selfMarker.uid ?: "unknown-device"
        val callsign = getCallsign(selfMarker)
        val lat = point.latitude
        val lon = point.longitude
        val hae = if (point.isAltitudeValid) point.altitude else null

        // Get heading and speed if available (from track data)
        val heading = selfMarker.getMetaDouble("heading", Double.NaN).takeIf { !it.isNaN() }
        val speed = selfMarker.getMetaDouble("Speed", Double.NaN).takeIf { !it.isNaN() }

        Log.d(TAG, "Broadcasting PLI: uid=$uid, callsign=$callsign, lat=$lat, lon=$lon")

        // Build platform JSON
        val platformJson = JSONObject().apply {
            put("id", uid)
            put("name", callsign)
            put("platform_type", "SOLDIER")
            put("lat", lat)
            put("lon", lon)
            if (hae != null) put("hae", hae)
            if (heading != null) put("heading", heading)
            if (speed != null) put("speed", speed)
            put("status", "ACTIVE")
            put("capabilities", JSONArray().put("PLI"))
            put("readiness", 1.0)
            // cell_id is null for now - could be set if user joins a cell
        }

        // Get HIVE node and publish
        val node = HivePluginLifecycle.getInstance()?.getHiveNodeJni()
        if (node == null) {
            Log.w(TAG, "HIVE node not available")
            onBroadcastCallback?.invoke(false, "HIVE not connected")
            return
        }

        val success = node.publishPlatform(platformJson.toString())
        if (success) {
            Log.d(TAG, "PLI broadcast successful")
            onBroadcastCallback?.invoke(true, "Broadcast OK: $callsign @ ${String.format("%.4f, %.4f", lat, lon)}")
        } else {
            Log.w(TAG, "PLI broadcast failed")
            onBroadcastCallback?.invoke(false, "Broadcast failed")
        }
    }

    /**
     * Get the callsign from a map item, with fallbacks.
     */
    private fun getCallsign(item: PointMapItem): String {
        // Try various metadata keys that might contain the callsign
        val callsign = item.getMetaString("callsign", null)
            ?: item.getMetaString("title", null)
            ?: item.title
            ?: item.uid?.take(8)
            ?: "Unknown"

        return callsign
    }

    /**
     * Manually trigger a single broadcast (for testing).
     */
    fun broadcastNow() {
        try {
            broadcastSelfPosition()
        } catch (e: Exception) {
            Log.e(TAG, "Manual broadcast failed: ${e.message}", e)
            onBroadcastCallback?.invoke(false, "Error: ${e.message}")
        }
    }
}
