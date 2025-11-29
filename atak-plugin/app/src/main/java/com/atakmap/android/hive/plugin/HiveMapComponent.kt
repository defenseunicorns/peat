package com.atakmap.android.hive.plugin

import android.content.Context
import android.content.Intent
import com.atakmap.android.dropdown.DropDownMapComponent
import com.atakmap.android.hive.plugin.model.HiveCell
import com.atakmap.android.hive.plugin.model.HivePlatform
import com.atakmap.android.hive.plugin.model.HiveTrack
import com.atakmap.android.ipc.AtakBroadcast.DocumentedIntentFilter
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log

/**
 * HIVE Map Component
 *
 * Main component for the HIVE plugin. Extends DropDownMapComponent
 * to integrate with ATAK's dropdown system.
 *
 * NOTE: This is a simplified version without coroutines/Flow to avoid
 * dependency conflicts with ATAK SDK 5.6 Preview's bundled libraries.
 * Full coroutines support will be added when testing on real hardware.
 */
class HiveMapComponent : DropDownMapComponent() {

    companion object {
        private const val TAG = "HiveMapComponent"
    }

    private lateinit var pluginContext: Context
    private var dropDownReceiver: HiveDropDownReceiver? = null

    // Simple state management without coroutines
    private val _cells = mutableListOf<HiveCell>()
    val cells: List<HiveCell> get() = _cells.toList()

    private val _platforms = mutableListOf<HivePlatform>()
    val platforms: List<HivePlatform> get() = _platforms.toList()

    private val _tracks = mutableListOf<HiveTrack>()
    val tracks: List<HiveTrack> get() = _tracks.toList()

    private var _connectionStatus = ConnectionStatus.DISCONNECTED
    val connectionStatus: ConnectionStatus get() = _connectionStatus

    private var _peerCount = 0
    val peerCount: Int get() = _peerCount

    override fun onCreate(context: Context, intent: Intent, view: MapView) {
        context.setTheme(R.style.ATAKPluginTheme)
        super.onCreate(context, intent, view)

        pluginContext = context
        Log.d(TAG, "HiveMapComponent onCreate")

        // Create dropdown receiver
        dropDownReceiver = HiveDropDownReceiver(view, context, this)

        // Register for show plugin intent
        val ddFilter = DocumentedIntentFilter()
        ddFilter.addAction(HiveDropDownReceiver.SHOW_PLUGIN)
        registerDropDownReceiver(dropDownReceiver, ddFilter)

        // Load mock data for now
        loadMockData()

        Log.d(TAG, "HiveMapComponent initialized")
    }

    override fun onDestroyImpl(context: Context, view: MapView) {
        Log.d(TAG, "HiveMapComponent onDestroy")
        super.onDestroyImpl(context, view)
    }

    /**
     * Load mock data for development/testing
     */
    private fun loadMockData() {
        Log.d(TAG, "Loading mock data")
        _connectionStatus = ConnectionStatus.CONNECTED
        _peerCount = 2

        _cells.clear()
        _cells.addAll(listOf(
            HiveCell(
                id = "alpha-team",
                name = "Alpha Team",
                status = HiveCell.Status.ACTIVE,
                platformCount = 4,
                centerLat = 38.8977,
                centerLon = -77.0365,
                capabilities = listOf("detection", "tracking")
            ),
            HiveCell(
                id = "bravo-team",
                name = "Bravo Team",
                status = HiveCell.Status.ACTIVE,
                platformCount = 3,
                centerLat = 38.9000,
                centerLon = -77.0400,
                capabilities = listOf("classification", "tracking")
            )
        ))
    }

    /**
     * Refresh data
     */
    fun refreshData() {
        Log.d(TAG, "Refreshing HIVE data")
        loadMockData()
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
     * Connection status enumeration
     */
    enum class ConnectionStatus {
        DISCONNECTED,
        CONNECTING,
        CONNECTED,
        ERROR
    }
}
