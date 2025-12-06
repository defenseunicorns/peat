package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import com.atakmap.android.dropdown.DropDown.OnStateListener
import com.atakmap.android.dropdown.DropDownReceiver
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.revolveteam.atak.hive.model.HiveCell

/**
 * HIVE DropDown Receiver
 *
 * Manages the side panel UI for the HIVE plugin using traditional Android Views
 * to avoid lifecycle conflicts with ATAK's bundled androidx libraries.
 */
class HiveDropDownReceiver(
    mapView: MapView?,
    private val pluginContext: Context,
    private val mapComponent: HiveMapComponent
) : DropDownReceiver(mapView), OnStateListener {

    companion object {
        val TAG: String = HiveDropDownReceiver::class.java.simpleName
        const val SHOW_PLUGIN = "com.revolveteam.atak.hive.SHOW_PLUGIN"
    }

    override fun disposeImpl() {
        Log.d(TAG, "HiveDropDownReceiver disposed")
    }

    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return

        if (action == SHOW_PLUGIN) {
            Log.d(TAG, "Showing HIVE plugin dropdown")

            val view = createContentView()

            showDropDown(
                view,
                HALF_WIDTH, FULL_HEIGHT,
                FULL_WIDTH, HALF_HEIGHT,
                false, this
            )
        }
    }

    private fun createContentView(): View {
        val scrollView = ScrollView(pluginContext).apply {
            setBackgroundColor(Color.parseColor("#1a1a1a"))
        }

        val container = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
        }

        // Header
        val header = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val title = TextView(pluginContext).apply {
            text = "HIVE Manager"
            textSize = 20f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        header.addView(title)

        val statusColor = when (mapComponent.connectionStatus) {
            HiveMapComponent.ConnectionStatus.CONNECTED -> Color.parseColor("#4CAF50")
            HiveMapComponent.ConnectionStatus.CONNECTING -> Color.parseColor("#FFC107")
            else -> Color.parseColor("#F44336")
        }
        val status = TextView(pluginContext).apply {
            text = "${mapComponent.connectionStatus.name} (${mapComponent.peerCount} peers)"
            textSize = 12f
            setTextColor(statusColor)
        }
        header.addView(status)
        container.addView(header)

        // Spacer
        container.addView(createSpacer(24))

        // Cells section
        val cellsTitle = TextView(pluginContext).apply {
            text = "Active Cells"
            textSize = 16f
            setTextColor(Color.WHITE)
        }
        container.addView(cellsTitle)
        container.addView(createSpacer(12))

        if (mapComponent.cells.isEmpty()) {
            val noCells = TextView(pluginContext).apply {
                text = "No active cells"
                textSize = 14f
                setTextColor(Color.GRAY)
            }
            container.addView(noCells)
        } else {
            mapComponent.cells.forEach { cell ->
                container.addView(createCellCard(cell))
                container.addView(createSpacer(8))
            }
        }

        container.addView(createSpacer(24))

        // Plugin info section
        val infoTitle = TextView(pluginContext).apply {
            text = "Plugin Info"
            textSize = 16f
            setTextColor(Color.WHITE)
        }
        container.addView(infoTitle)
        container.addView(createSpacer(12))

        val infoCard = createInfoCard()
        container.addView(infoCard)

        container.addView(createSpacer(24))

        // Refresh button
        val refreshButton = Button(pluginContext).apply {
            text = "Refresh Data"
            setOnClickListener {
                mapComponent.refreshData()
                // Refresh the dropdown
                showDropDown(
                    createContentView(),
                    HALF_WIDTH, FULL_HEIGHT,
                    FULL_WIDTH, HALF_HEIGHT,
                    false, this@HiveDropDownReceiver
                )
            }
        }
        container.addView(refreshButton)

        scrollView.addView(container)
        return scrollView
    }

    private fun createSpacer(heightDp: Int): View {
        return View(pluginContext).apply {
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                (heightDp * pluginContext.resources.displayMetrics.density).toInt()
            )
        }
    }

    private fun createCellCard(cell: HiveCell): View {
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val name = TextView(pluginContext).apply {
            text = cell.name
            textSize = 14f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(name)

        val statusColor = when (cell.status) {
            HiveCell.Status.ACTIVE -> Color.parseColor("#4CAF50")
            HiveCell.Status.FORMING -> Color.parseColor("#FFC107")
            else -> Color.parseColor("#F44336")
        }
        val statusText = TextView(pluginContext).apply {
            text = cell.status.name
            textSize = 12f
            setTextColor(statusColor)
        }
        headerRow.addView(statusText)
        card.addView(headerRow)

        val platforms = TextView(pluginContext).apply {
            text = "${cell.platformCount} platforms"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(platforms)

        if (cell.capabilities.isNotEmpty()) {
            val caps = TextView(pluginContext).apply {
                text = cell.capabilities.joinToString(", ")
                textSize = 11f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(caps)
        }

        return card
    }

    private fun createInfoCard(): View {
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        val version = TextView(pluginContext).apply {
            text = "Version: 0.1.0"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(version)

        val ffiStatus = if (HivePluginLifecycle.getInstance()?.isHiveFfiAvailable() == true)
            "Available" else "Not loaded"
        val ffi = TextView(pluginContext).apply {
            text = "HIVE FFI: $ffiStatus"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(ffi)

        // Node ID (full)
        val nodeId = HivePluginLifecycle.getInstance()?.getNodeId() ?: "N/A"
        val nodeIdLabel = TextView(pluginContext).apply {
            text = "Node ID:"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(nodeIdLabel)

        val nodeIdText = TextView(pluginContext).apply {
            text = nodeId
            textSize = 10f
            setTextColor(Color.parseColor("#888888"))
            setTypeface(android.graphics.Typeface.MONOSPACE)
            // Allow text selection for copying
            setTextIsSelectable(true)
        }
        card.addView(nodeIdText)

        // Peer count
        val peerCount = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0
        val peersText = TextView(pluginContext).apply {
            text = "Connected Peers: $peerCount"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(peersText)

        // Show peer IDs if we have any
        if (peerCount > 0) {
            val peersJson = HivePluginLifecycle.getInstance()?.getConnectedPeers() ?: "[]"
            try {
                val peerIds = org.json.JSONArray(peersJson)
                for (i in 0 until peerIds.length()) {
                    val peerId = peerIds.getString(i)
                    val peerIdText = TextView(pluginContext).apply {
                        // Show first 16 chars of peer ID for readability
                        text = "  - ${peerId.take(16)}..."
                        textSize = 10f
                        setTextColor(Color.parseColor("#888888"))
                        setTypeface(android.graphics.Typeface.MONOSPACE)
                    }
                    card.addView(peerIdText)
                }
            } catch (e: Exception) {
                Log.w(TAG, "Failed to parse peer IDs JSON: ${e.message}")
            }
        }

        return card
    }

    override fun onDropDownSelectionRemoved() {}
    override fun onDropDownVisible(v: Boolean) {}
    override fun onDropDownSizeChanged(width: Double, height: Double) {}
    override fun onDropDownClose() {}
}
