package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import com.atakmap.android.dropdown.DropDownMapComponent
import com.atakmap.android.ipc.AtakBroadcast.DocumentedIntentFilter
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.revolveteam.atak.hive.model.HiveCell
import com.revolveteam.atak.hive.model.HivePlatform
import com.revolveteam.atak.hive.model.HiveTrack

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

    val peerCount: Int get() = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0

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

        // Update connection status based on HIVE node availability
        updateConnectionStatus()

        Log.d(TAG, "HiveMapComponent initialized")
    }

    override fun onDestroyImpl(context: Context, view: MapView) {
        Log.d(TAG, "HiveMapComponent onDestroy")
        super.onDestroyImpl(context, view)
    }

    /**
     * Update connection status based on HIVE node availability
     */
    private fun updateConnectionStatus() {
        _connectionStatus = if (HivePluginLifecycle.getInstance()?.getHiveNodeJni() != null) {
            ConnectionStatus.CONNECTED
        } else {
            ConnectionStatus.DISCONNECTED
        }
        Log.d(TAG, "Connection status: $_connectionStatus")
    }

    /**
     * Refresh data from HIVE network
     */
    fun refreshData() {
        Log.d(TAG, "Refreshing HIVE data")
        updateConnectionStatus()
        // TODO: Fetch real cells/platforms/tracks from HIVE sync when automerge is fixed
        _cells.clear()
        _platforms.clear()
        _tracks.clear()
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
