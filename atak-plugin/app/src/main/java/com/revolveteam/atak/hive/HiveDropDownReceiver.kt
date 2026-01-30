/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import com.atakmap.android.dropdown.DropDown.OnStateListener
import com.atakmap.android.dropdown.DropDownReceiver
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log
import com.revolveteam.atak.hive.model.HiveCell
import com.revolveteam.atak.hive.model.HivePlatform
import com.revolveteam.atak.hive.model.HiveRole
import com.revolveteam.atak.hive.model.HiveTrack
import com.revolveteam.atak.hive.model.CommsQuality
import com.revolveteam.hive.HiveChat
import com.revolveteam.hive.HiveMarker
import com.revolveteam.hive.HivePeer as BlePeer
import uniffi.hive_lite_android.CannedMessageType
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * HIVE DropDown Receiver
 *
 * Manages the side panel UI for the HIVE plugin using traditional Android Views
 * to avoid lifecycle conflicts with ATAK's bundled androidx libraries.
 *
 * Implements PeerEventListener to receive peer connect/disconnect events and
 * auto-update the UI.
 */
class HiveDropDownReceiver(
    mapView: MapView?,
    private val pluginContext: Context,
    private val mapComponent: HiveMapComponent
) : DropDownReceiver(mapView), OnStateListener, PeerEventListener {

    companion object {
        val TAG: String = HiveDropDownReceiver::class.java.simpleName
        const val SHOW_PLUGIN = "com.revolveteam.atak.hive.SHOW_PLUGIN"
    }

    private val handler = Handler(Looper.getMainLooper())
    private var currentScrollView: ScrollView? = null
    private var isDropDownVisible = false

    // Platform detail view state - store ID to allow refresh with updated data
    private var selectedPlatformId: String? = null

    // Marker detail view state - store UID to allow refresh with updated data
    private var selectedMarkerUid: String? = null

    // User's role in the hierarchy (for PoC, using default role)
    private var userRole: HiveRole = HiveRole.defaultRole()

    // Periodic refresh for BLE mesh updates
    private var bleRefreshRunnable: Runnable? = null
    private val BLE_REFRESH_INTERVAL = 2000L // 2 seconds

    init {
        // Register for peer events
        PeerEventManager.addListener(this)

        // Register for cell selection changes
        mapComponent.onCellSelectionChanged = { _, _ ->
            refreshContentOnMainThread()
        }

        // Register for platform changes (including SOS state changes)
        mapComponent.onPlatformsChanged = {
            refreshContentOnMainThread()
        }

        // Register for emergency cleared events
        mapComponent.onEmergencyCleared = { _ ->
            refreshContentOnMainThread()
        }

        // Register for marker received events
        mapComponent.onMarkerReceived = { _, _ ->
            refreshContentOnMainThread()
        }

        // Register for chat received events
        mapComponent.onChatReceived = { _, _ ->
            refreshContentOnMainThread()
        }

        // Register for BLE mesh updates
        HivePluginLifecycle.getInstance()?.getHiveBleManager()?.let { bleManager ->
            bleManager.connectedPeerCount.observe { _ ->
                refreshContentOnMainThread()
            }
        }
    }

    override fun disposeImpl() {
        PeerEventManager.removeListener(this)
        Log.d(TAG, "HiveDropDownReceiver disposed")
    }

    private var bleObserverRegistered = false

    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return

        if (action == SHOW_PLUGIN) {
            Log.d(TAG, "Showing HIVE plugin dropdown")

            // Register BLE observer if not already done
            if (!bleObserverRegistered) {
                HivePluginLifecycle.getInstance()?.getHiveBleManager()?.let { bleManager ->
                    bleManager.connectedPeerCount.observe { _ ->
                        refreshContentOnMainThread()
                    }
                    bleManager.peers.observe { _ ->
                        refreshContentOnMainThread()
                    }
                    bleObserverRegistered = true
                    Log.d(TAG, "Registered BLE mesh observer")
                }
            }

            val view = createContentView()
            currentScrollView = view as ScrollView

            showDropDown(
                view,
                HALF_WIDTH, FULL_HEIGHT,
                FULL_WIDTH, HALF_HEIGHT,
                false, this
            )
        }
    }

    // PeerEventListener implementation
    override fun onPeerConnected(peerId: String) {
        Log.i(TAG, "Peer connected: $peerId - refreshing UI")
        refreshContentOnMainThread()
    }

    override fun onPeerDisconnected(peerId: String, reason: String) {
        Log.i(TAG, "Peer disconnected: $peerId ($reason) - refreshing UI")
        refreshContentOnMainThread()
    }

    private fun refreshContentOnMainThread() {
        handler.post {
            if (isDropDownVisible) {
                refreshContent()
            }
        }
    }

    private fun refreshContent() {
        currentScrollView?.let { scrollView ->
            // Remember scroll position
            val scrollY = scrollView.scrollY

            // Rebuild content
            scrollView.removeAllViews()
            val container = buildContentContainer()
            scrollView.addView(container)

            // Restore scroll position
            scrollView.post { scrollView.scrollTo(0, scrollY) }
        }
    }

    private fun createContentView(): View {
        val scrollView = ScrollView(pluginContext).apply {
            setBackgroundColor(Color.parseColor("#1a1a1a"))
        }

        val container = buildContentContainer()
        scrollView.addView(container)
        return scrollView
    }

    private fun buildContentContainer(): LinearLayout {
        // If a marker is selected, show marker detail view
        selectedMarkerUid?.let { markerUid ->
            val cachedMarker = mapComponent.markers[markerUid]
            if (cachedMarker != null) {
                return buildMarkerDetailView(cachedMarker)
            } else {
                // Marker no longer exists, clear selection
                selectedMarkerUid = null
            }
        }

        // If a platform is selected, look up fresh data and show detail view
        selectedPlatformId?.let { platformId ->
            val platform = mapComponent.platforms.find { it.id == platformId }
            if (platform != null) {
                return buildPlatformDetailView(platform)
            } else {
                // Platform no longer exists, clear selection
                selectedPlatformId = null
            }
        }

        val selectedCellId = mapComponent.selectedCellId
        val selectedCellName = mapComponent.selectedCellName

        Log.d(TAG, "Building content - cells: ${mapComponent.cells.size}, tracks: ${mapComponent.tracks.size}, platforms: ${mapComponent.platforms.size}, selectedCell: $selectedCellId")

        val container = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
        }

        // Header with optional back button for cell-filtered view
        val header = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        // Back button when viewing a specific cell
        if (selectedCellId != null) {
            val backButton = Button(pluginContext).apply {
                text = "←"
                textSize = 16f
                setTextColor(Color.WHITE)
                setBackgroundColor(Color.parseColor("#444444"))
                setPadding(24, 8, 24, 8)
                setOnClickListener {
                    mapComponent.clearCellSelection()
                }
            }
            header.addView(backButton)
            header.addView(createHorizontalSpacer(16))
        }

        // Title with mesh name subtitle
        val titleContainer = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        val title = TextView(pluginContext).apply {
            text = if (selectedCellId != null) "Cell: $selectedCellName" else "HIVE Manager"
            textSize = 20f
            setTextColor(Color.WHITE)
        }
        titleContainer.addView(title)

        // Show mesh name as subtitle
        val meshName = HivePluginLifecycle.getInstance()?.getCurrentMeshId() ?: "Unknown"
        val meshSubtitle = TextView(pluginContext).apply {
            text = "Mesh: $meshName"
            textSize = 11f
            setTextColor(Color.parseColor("#888888"))
        }
        titleContainer.addView(meshSubtitle)
        header.addView(titleContainer)

        val statusColor = when (mapComponent.connectionStatus) {
            HiveMapComponent.ConnectionStatus.CONNECTED -> Color.parseColor("#4CAF50")
            HiveMapComponent.ConnectionStatus.CONNECTING -> Color.parseColor("#FFC107")
            else -> Color.parseColor("#F44336")
        }
        // Combined peer count (mesh + lite)
        val meshPeers = mapComponent.peerCount
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val litePeers = if (bleManager?.isRunning?.value == true) {
            bleManager.peers.value?.count { it.isConnected } ?: 0
        } else 0
        val totalPeers = meshPeers + litePeers
        val status = TextView(pluginContext).apply {
            text = "${mapComponent.connectionStatus.name} ($totalPeers peers)"
            textSize = 12f
            setTextColor(statusColor)
        }
        header.addView(status)
        container.addView(header)

        // Role indicator (only in main view)
        if (selectedCellId == null) {
            val currentCell = HivePluginLifecycle.getInstance()?.getCurrentCellId() ?: "ALPHA"
            val roleRow = TextView(pluginContext).apply {
                text = "Role: ${userRole.toDisplayString()} • Cell $currentCell"
                textSize = 11f
                setTextColor(Color.parseColor("#666666"))
            }
            container.addView(roleRow)
        }

        // Spacer
        container.addView(createSpacer(24))

        // PLI Broadcast section (only in main view)
        if (selectedCellId == null) {
            val pliSection = createPliBroadcastSection()
            container.addView(pliSection)
            container.addView(createSpacer(24))
        }


        // Squad Leader Summary (only in main view for leaders)
        if (selectedCellId == null && userRole.isLeader) {
            val summarySection = createSquadLeaderSummary()
            container.addView(summarySection)
            container.addView(createSpacer(24))
        }

        // Cells section (only in main view, not when viewing a specific cell)
        if (selectedCellId == null) {
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
        }

        // Tracks section (only in main view)
        if (selectedCellId == null) {
            val mapMarkerCount = mapComponent.getMapMarkerCount()
            val tracksTitle = TextView(pluginContext).apply {
                text = "Tracks (${mapComponent.tracks.size}) • Map: $mapMarkerCount"
                textSize = 16f
                setTextColor(Color.WHITE)
            }
            container.addView(tracksTitle)
            container.addView(createSpacer(12))

            if (mapComponent.tracks.isEmpty()) {
                val noTracks = TextView(pluginContext).apply {
                    text = "No tracks"
                    textSize = 14f
                    setTextColor(Color.GRAY)
                }
                container.addView(noTracks)
            } else {
                mapComponent.tracks.forEach { track ->
                    container.addView(createTrackCard(track))
                    container.addView(createSpacer(8))
                }
            }

            container.addView(createSpacer(24))
        }

        // HIVE Markers section (only in main view)
        if (selectedCellId == null) {
            val meshMarkers = mapComponent.markers.values.toList()
            val markersTitle = TextView(pluginContext).apply {
                text = "HIVE Markers (${meshMarkers.size})"
                textSize = 16f
                setTextColor(Color.WHITE)
            }
            container.addView(markersTitle)
            container.addView(createSpacer(12))

            if (meshMarkers.isEmpty()) {
                val noMarkers = TextView(pluginContext).apply {
                    text = "No mesh markers received"
                    textSize = 14f
                    setTextColor(Color.GRAY)
                }
                container.addView(noMarkers)
            } else {
                // Sort by received time, most recent first
                meshMarkers.sortedByDescending { it.receivedAt }.forEach { cachedMarker ->
                    container.addView(createMarkerCard(cachedMarker))
                    container.addView(createSpacer(8))
                }
            }

            container.addView(createSpacer(24))
        }

        // Canned Messages section (only in cell detail view)
        if (selectedCellId != null) {
            val chatMessages = mapComponent.chatMessages
            val chatTitle = TextView(pluginContext).apply {
                text = "Quick Messages"
                textSize = 16f
                setTextColor(Color.WHITE)
            }
            container.addView(chatTitle)
            container.addView(createSpacer(12))

            // Canned message buttons organized by category
            container.addView(createCannedMessageSection())
            container.addView(createSpacer(16))

            // Recent messages section
            val recentTitle = TextView(pluginContext).apply {
                text = "Recent (${chatMessages.size})"
                textSize = 14f
                setTextColor(Color.parseColor("#888888"))
            }
            container.addView(recentTitle)
            container.addView(createSpacer(8))

            // Chat messages (most recent at top)
            if (chatMessages.isEmpty()) {
                val noChat = TextView(pluginContext).apply {
                    text = "No messages yet"
                    textSize = 14f
                    setTextColor(Color.GRAY)
                }
                container.addView(noChat)
            } else {
                // Show most recent first (reversed)
                chatMessages.reversed().take(10).forEach { cachedChat ->
                    container.addView(createChatMessageCard(cachedChat))
                    container.addView(createSpacer(4))
                }
            }

            container.addView(createSpacer(24))
        }

        // Platforms section - filtered when cell is selected
        val filteredPlatforms = mapComponent.getFilteredPlatforms()
        val platformsTitle = TextView(pluginContext).apply {
            text = if (selectedCellId != null) {
                "Platforms in Cell (${filteredPlatforms.size})"
            } else {
                "Platforms (${mapComponent.platforms.size})"
            }
            textSize = 16f
            setTextColor(Color.WHITE)
        }
        container.addView(platformsTitle)
        container.addView(createSpacer(12))

        if (filteredPlatforms.isEmpty()) {
            val noPlatforms = TextView(pluginContext).apply {
                text = if (selectedCellId != null) "No platforms in this cell" else "No platforms"
                textSize = 14f
                setTextColor(Color.GRAY)
            }
            container.addView(noPlatforms)
        } else {
            filteredPlatforms.forEach { platform ->
                container.addView(createPlatformCard(platform))
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

        return container
    }

    private fun createHorizontalSpacer(widthDp: Int): View {
        return View(pluginContext).apply {
            layoutParams = LinearLayout.LayoutParams(
                (widthDp * pluginContext.resources.displayMetrics.density).toInt(),
                LinearLayout.LayoutParams.MATCH_PARENT
            )
        }
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
        // Get actual platforms in this cell
        val cellPlatforms = mapComponent.platforms.filter { it.cellId == cell.id }
        val emergencyPlatforms = cellPlatforms.filter { it.status == HivePlatform.Status.EMERGENCY }
        val hasEmergency = emergencyPlatforms.isNotEmpty()

        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            // Red border/background tint for cells with SOS
            setBackgroundColor(if (hasEmergency) Color.parseColor("#4d2d2d") else Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
            // Make card tappable to view cell details + chat
            isClickable = true
            isFocusable = true
            setOnClickListener {
                mapComponent.selectCell(cell.id, cell.name)
                refreshContent()
            }
        }

        // SOS Alert banner at top of cell card
        if (hasEmergency) {
            val sosBanner = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(0, 0, 0, 8)
            }
            val sosIcon = TextView(pluginContext).apply {
                text = "⚠ SOS"
                textSize = 12f
                setTextColor(Color.parseColor("#FF0000"))
                setTypeface(null, android.graphics.Typeface.BOLD)
            }
            sosBanner.addView(sosIcon)
            sosBanner.addView(createHorizontalSpacer(8))
            val sosNames = TextView(pluginContext).apply {
                text = emergencyPlatforms.joinToString(", ") { it.callsign }
                textSize = 11f
                setTextColor(Color.parseColor("#FF6666"))
            }
            sosBanner.addView(sosNames)
            card.addView(sosBanner)
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

        // Get actual platforms in this cell (for display)
        val displayPlatforms = mapComponent.platforms.filter { it.cellId == cell.id }

        val platformsHeader = TextView(pluginContext).apply {
            text = "${displayPlatforms.size} platforms"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(platformsHeader)

        // List platform names with SOS indicator
        if (displayPlatforms.isNotEmpty()) {
            displayPlatforms.forEach { platform ->
                val isEmergency = platform.status == HivePlatform.Status.EMERGENCY
                val platformRow = TextView(pluginContext).apply {
                    text = if (isEmergency) {
                        "  ⚠ ${platform.callsign} - SOS"
                    } else {
                        "  • ${platform.callsign} (${platform.platformType.name})"
                    }
                    textSize = 11f
                    setTextColor(if (isEmergency) Color.parseColor("#FF0000") else Color.parseColor("#AAAAAA"))
                    if (isEmergency) setTypeface(null, android.graphics.Typeface.BOLD)
                }
                card.addView(platformRow)
            }
        }

        // Tap hint
        val tapHint = TextView(pluginContext).apply {
            text = "Tap for chat + details →"
            textSize = 10f
            setTextColor(Color.parseColor("#666666"))
            gravity = Gravity.END
            setPadding(0, 8, 0, 0)
        }
        card.addView(tapHint)

        return card
    }

    private fun createTrackCard(track: HiveTrack): View {
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val id = TextView(pluginContext).apply {
            text = "Track: ${track.id.takeLast(8)}"
            textSize = 14f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(id)

        // Color based on classification (hostile = red, friendly = green, unknown = yellow)
        val classColor = when {
            track.classification.contains("-h-") -> Color.parseColor("#F44336")  // Hostile
            track.classification.contains("-f-") -> Color.parseColor("#4CAF50")  // Friendly
            else -> Color.parseColor("#FFC107")  // Unknown
        }
        val classText = TextView(pluginContext).apply {
            text = track.category.name
            textSize = 12f
            setTextColor(classColor)
        }
        headerRow.addView(classText)
        card.addView(headerRow)

        val location = TextView(pluginContext).apply {
            text = String.format("%.4f, %.4f", track.lat, track.lon)
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(location)

        val confidence = TextView(pluginContext).apply {
            text = "Confidence: ${(track.confidence * 100).toInt()}%"
            textSize = 11f
            setTextColor(Color.parseColor("#888888"))
        }
        card.addView(confidence)

        return card
    }

    private fun createMarkerCard(cachedMarker: HiveMapComponent.CachedMarker): View {
        val marker = cachedMarker.marker

        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
            // Make card clickable
            isClickable = true
            isFocusable = true
            setOnClickListener {
                selectedMarkerUid = marker.uid
                refreshContent()
            }
        }

        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val callsign = TextView(pluginContext).apply {
            text = marker.callsign.ifEmpty { "Marker" }
            textSize = 14f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(callsign)

        // Color based on CoT type
        val typeColor = when {
            marker.type.contains("-h-") -> Color.parseColor("#F44336")  // Hostile
            marker.type.contains("-f-") -> Color.parseColor("#4CAF50")  // Friendly
            marker.type.contains("-n-") -> Color.parseColor("#2196F3")  // Neutral
            else -> Color.parseColor("#FFC107")  // Unknown
        }
        val typeText = TextView(pluginContext).apply {
            text = getMarkerTypeLabel(marker.type)
            textSize = 12f
            setTextColor(typeColor)
        }
        headerRow.addView(typeText)
        card.addView(headerRow)

        val location = TextView(pluginContext).apply {
            text = String.format("%.4f, %.4f", marker.lat, marker.lon)
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(location)

        val sourceRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val sourceLabel = TextView(pluginContext).apply {
            text = "From: ${cachedMarker.sourcePeerName}"
            textSize = 11f
            setTextColor(Color.parseColor("#888888"))
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        sourceRow.addView(sourceLabel)

        val ageText = TextView(pluginContext).apply {
            val ageSec = (System.currentTimeMillis() - cachedMarker.receivedAt) / 1000
            text = when {
                ageSec < 60 -> "${ageSec}s ago"
                ageSec < 3600 -> "${ageSec / 60}m ago"
                else -> "${ageSec / 3600}h ago"
            }
            textSize = 10f
            setTextColor(Color.parseColor("#666666"))
        }
        sourceRow.addView(ageText)
        card.addView(sourceRow)

        // Tap hint
        val tapHint = TextView(pluginContext).apply {
            text = "Tap for details →"
            textSize = 10f
            setTextColor(Color.parseColor("#666666"))
            gravity = Gravity.END
        }
        card.addView(tapHint)

        return card
    }

    /**
     * Get a human-readable label for CoT marker type.
     */
    private fun getMarkerTypeLabel(cotType: String): String {
        return when {
            cotType.startsWith("a-f-G") -> "Friendly Ground"
            cotType.startsWith("a-f-A") -> "Friendly Air"
            cotType.startsWith("a-f-S") -> "Friendly Sea"
            cotType.startsWith("a-h-G") -> "Hostile Ground"
            cotType.startsWith("a-h-A") -> "Hostile Air"
            cotType.startsWith("a-h-S") -> "Hostile Sea"
            cotType.startsWith("a-n-G") -> "Neutral Ground"
            cotType.startsWith("a-u-G") -> "Unknown Ground"
            cotType.startsWith("b-m-p-w") -> "Waypoint"
            cotType.startsWith("b-m-p-c") -> "Checkpoint"
            cotType.startsWith("b-m-r") -> "Route"
            else -> cotType.take(10)
        }
    }

    private fun createChatMessageCard(cachedChat: HiveMapComponent.CachedChat): View {
        val chat = cachedChat.chat
        val isSelf = cachedChat.isSelf

        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(if (isSelf) Color.parseColor("#1a3d1a") else Color.parseColor("#2d2d2d"))
            setPadding(16, 12, 16, 12)
        }

        // Header row: sender + time
        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val senderText = TextView(pluginContext).apply {
            text = chat.sender
            textSize = 12f
            setTextColor(if (isSelf) Color.parseColor("#8BC34A") else Color.parseColor("#64B5F6"))
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(senderText)

        val ageSec = (System.currentTimeMillis() - chat.timestamp) / 1000
        val timeText = TextView(pluginContext).apply {
            text = when {
                ageSec < 60 -> "${ageSec}s"
                ageSec < 3600 -> "${ageSec / 60}m"
                else -> "${ageSec / 3600}h"
            }
            textSize = 10f
            setTextColor(Color.parseColor("#666666"))
        }
        headerRow.addView(timeText)
        card.addView(headerRow)

        // Message text
        val msgTextView = TextView(pluginContext).apply {
            text = chat.message
            textSize = 14f
            setTextColor(Color.WHITE)
        }
        card.addView(msgTextView)

        // ACK status row (for sent messages)
        if (isSelf && cachedChat.acks.isNotEmpty()) {
            val ackRow = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.END or Gravity.CENTER_VERTICAL
                setPadding(0, 4, 0, 0)
            }

            val ackNames = cachedChat.acks.joinToString(", ") { it.sender }
            val ackText = TextView(pluginContext).apply {
                text = "ACK: $ackNames"
                textSize = 10f
                setTextColor(Color.parseColor("#4CAF50"))  // Green for ACKs
            }
            ackRow.addView(ackText)
            card.addView(ackRow)
        }

        return card
    }

    /**
     * Canned message definition with display text and color
     */
    data class CannedMessageDef(
        val messageType: CannedMessageType,
        val label: String,
        val color: String
    )

    /**
     * Create the canned message selector with 2 compact rows
     */
    private fun createCannedMessageSection(): View {
        val section = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
        }

        // Row 1: Status/Movement (routine ops) - green/blue
        val row1Messages = listOf(
            CannedMessageDef(CannedMessageType.CHECK_IN, "CHECK IN", "#4CAF50"),
            CannedMessageDef(CannedMessageType.ALL_CLEAR, "ALL CLEAR", "#4CAF50"),
            CannedMessageDef(CannedMessageType.MOVING, "MOVING", "#2196F3"),
            CannedMessageDef(CannedMessageType.HOLDING, "HOLDING", "#2196F3"),
            CannedMessageDef(CannedMessageType.ON_STATION, "ON STATION", "#4CAF50"),
            CannedMessageDef(CannedMessageType.RETURNING, "RTB", "#2196F3")
        )
        section.addView(createCannedMessageRow(row1Messages))
        section.addView(createSpacer(6))

        // Row 2: Requests/Alerts (urgent) - orange/red
        val row2Messages = listOf(
            CannedMessageDef(CannedMessageType.NEED_SUPPORT, "SUPPORT", "#FF9800"),
            CannedMessageDef(CannedMessageType.NEED_MEDIC, "MEDIC", "#FF9800"),
            CannedMessageDef(CannedMessageType.NEED_EXTRACT, "EXTRACT", "#FF9800"),
            CannedMessageDef(CannedMessageType.CONTACT, "CONTACT", "#F44336"),
            CannedMessageDef(CannedMessageType.UNDER_FIRE, "UNDER FIRE", "#F44336"),
            CannedMessageDef(CannedMessageType.ALERT, "ALERT", "#F44336")
        )
        section.addView(createCannedMessageRow(row2Messages))

        // Recent canned messages with ACK counts
        val cannedMessages = mapComponent.getCannedMessages()
        if (cannedMessages.isNotEmpty()) {
            section.addView(createSpacer(12))
            val recentLabel = TextView(pluginContext).apply {
                text = "Recent Messages"
                textSize = 12f
                setTextColor(Color.parseColor("#888888"))
            }
            section.addView(recentLabel)
            section.addView(createSpacer(4))

            // Show last 5 messages
            cannedMessages.take(5).forEach { msg ->
                section.addView(createCannedMessageCard(msg))
                section.addView(createSpacer(4))
            }
        }

        return section
    }

    /**
     * Resolve a node ID to a callsign using the platform list.
     * Falls back to hex format if no platform is found.
     */
    private fun resolveNodeIdToCallsign(nodeId: Long): String {
        val hexId = String.format("%08X", nodeId)

        // Check if this is our own node
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val myNodeId = bleManager?.getNodeId()
        if (myNodeId != null && myNodeId == nodeId) {
            return mapComponent.selfCallsign
        }

        // Look up platform by ID format "ble-XXXXXXXX"
        val platform = mapComponent.platforms.find { it.id == "ble-$hexId" }
        return platform?.callsign ?: hexId
    }

    /**
     * Create a thread-style card showing a canned message with who ACK'd
     */
    private fun createCannedMessageCard(msg: uniffi.hive_lite_android.CannedMessageAckEventData): View {
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(12, 10, 12, 10)
        }

        // Header row: Message type + timestamp
        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        // Message type badge with color based on category
        val msgColor = when {
            msg.message.name.contains("EMERGENCY") || msg.message.name.contains("ALERT") ||
            msg.message.name.contains("CONTACT") || msg.message.name.contains("UNDER_FIRE") -> "#F44336"
            msg.message.name.contains("NEED_") -> "#FF9800"
            msg.message.name.contains("MOVING") || msg.message.name.contains("HOLDING") ||
            msg.message.name.contains("RETURNING") -> "#2196F3"
            else -> "#4CAF50"
        }

        val typeLabel = TextView(pluginContext).apply {
            text = msg.message.name.replace("_", " ")
            textSize = 12f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor(msgColor))
            setPadding(8, 4, 8, 4)
        }
        headerRow.addView(typeLabel)

        // Spacer
        headerRow.addView(View(pluginContext).apply {
            layoutParams = LinearLayout.LayoutParams(0, 1, 1f)
        })

        // Timestamp
        val timeStr = SimpleDateFormat("HH:mm:ss", Locale.US).format(Date(msg.timestamp.toLong()))
        val timeLabel = TextView(pluginContext).apply {
            text = timeStr
            textSize = 10f
            setTextColor(Color.parseColor("#888888"))
        }
        headerRow.addView(timeLabel)

        card.addView(headerRow)

        // From row - show callsign instead of hex ID
        val fromCallsign = resolveNodeIdToCallsign(msg.sourceNode.toLong())
        val fromRow = TextView(pluginContext).apply {
            text = "FROM: $fromCallsign"
            textSize = 10f
            setTextColor(Color.parseColor("#AAAAAA"))
            setPadding(0, 4, 0, 0)
        }
        card.addView(fromRow)

        // ACKs section - show callsigns instead of hex IDs
        if (msg.acks.isNotEmpty()) {
            val ackLabel = TextView(pluginContext).apply {
                // Build list of ACKers using callsigns
                val ackCallsigns = msg.acks
                    .map { ack -> resolveNodeIdToCallsign(ack.nodeId.toLong()) }
                    .joinToString(", ")
                text = "ACK: $ackCallsigns"
                textSize = 10f
                setTextColor(Color.parseColor("#4CAF50"))
                setPadding(0, 2, 0, 0)
            }
            card.addView(ackLabel)
        }

        return card
    }

    /**
     * Create a row of evenly-sized canned message buttons that fill the width
     */
    private fun createCannedMessageRow(messages: List<CannedMessageDef>): View {
        val row = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT
            )
        }

        messages.forEachIndexed { index, msg ->
            if (index > 0) {
                // Small gap between buttons
                row.addView(View(pluginContext).apply {
                    layoutParams = LinearLayout.LayoutParams(4, 1)
                })
            }

            val button = Button(pluginContext).apply {
                text = msg.label
                textSize = 9f
                setTextColor(Color.WHITE)
                setBackgroundColor(Color.parseColor(msg.color))
                setPadding(4, 8, 4, 8)
                minWidth = 0
                minHeight = 0
                minimumWidth = 0
                minimumHeight = 0
                // Equal weight for all buttons to fill width evenly
                layoutParams = LinearLayout.LayoutParams(
                    0,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                    1f  // weight
                )
                setOnClickListener {
                    mapComponent.sendCannedMessage(msg.messageType)
                    refreshContent()
                }
            }
            row.addView(button)
        }

        return row
    }

    private fun createPlatformCard(platform: HivePlatform): View {
        val isEmergency = platform.status == HivePlatform.Status.EMERGENCY

        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            // Red background for emergency platforms
            setBackgroundColor(if (isEmergency) Color.parseColor("#5d1a1a") else Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
            // Make card clickable
            isClickable = true
            isFocusable = true
            setOnClickListener {
                selectedPlatformId = platform.id
                refreshContent()
            }
        }

        // SOS Banner for emergency platforms
        if (isEmergency) {
            val sosBanner = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                setBackgroundColor(Color.parseColor("#D32F2F"))
                setPadding(12, 8, 12, 8)
                gravity = Gravity.CENTER_VERTICAL
            }
            val sosIcon = TextView(pluginContext).apply {
                text = "⚠ SOS EMERGENCY"
                textSize = 12f
                setTextColor(Color.WHITE)
                setTypeface(null, android.graphics.Typeface.BOLD)
            }
            sosBanner.addView(sosIcon)
            card.addView(sosBanner)
            card.addView(createSpacer(8))
        }

        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val name = TextView(pluginContext).apply {
            text = platform.callsign
            textSize = 14f
            setTextColor(if (isEmergency) Color.WHITE else Color.WHITE)
            setTypeface(null, if (isEmergency) android.graphics.Typeface.BOLD else android.graphics.Typeface.NORMAL)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(name)

        val statusColor = when (platform.status) {
            HivePlatform.Status.OPERATIONAL -> Color.parseColor("#4CAF50")
            HivePlatform.Status.DEGRADED -> Color.parseColor("#FFC107")
            HivePlatform.Status.EMERGENCY -> Color.parseColor("#FF0000")  // Bright red for SOS
            else -> Color.parseColor("#F44336")
        }
        val statusText = TextView(pluginContext).apply {
            text = platform.status.name
            textSize = 12f
            setTextColor(statusColor)
            setTypeface(null, if (isEmergency) android.graphics.Typeface.BOLD else android.graphics.Typeface.NORMAL)
        }
        headerRow.addView(statusText)
        card.addView(headerRow)

        val typeInfo = TextView(pluginContext).apply {
            text = "${platform.platformType.name} • ${String.format("%.4f, %.4f", platform.lat, platform.lon)}"
            textSize = 12f
            setTextColor(if (isEmergency) Color.parseColor("#CCCCCC") else Color.GRAY)
        }
        card.addView(typeInfo)

        if (platform.capabilities.isNotEmpty()) {
            val caps = TextView(pluginContext).apply {
                text = platform.capabilities.joinToString(", ")
                textSize = 11f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(caps)
        }

        // Tap hint
        val tapHint = TextView(pluginContext).apply {
            text = if (isEmergency) "Tap for SOS details →" else "Tap for details →"
            textSize = 10f
            setTextColor(if (isEmergency) Color.parseColor("#FF6666") else Color.parseColor("#666666"))
            gravity = Gravity.END
        }
        card.addView(tapHint)

        return card
    }

    /**
     * Build the platform detail view showing comprehensive platform information
     */
    private fun buildPlatformDetailView(platform: HivePlatform): LinearLayout {
        val container = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
        }

        // Header with back button
        val header = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val backButton = Button(pluginContext).apply {
            text = "←"
            textSize = 16f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#444444"))
            setPadding(24, 8, 24, 8)
            setOnClickListener {
                selectedPlatformId = null
                refreshContent()
            }
        }
        header.addView(backButton)
        header.addView(createHorizontalSpacer(16))

        val title = TextView(pluginContext).apply {
            text = platform.callsign
            textSize = 20f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        header.addView(title)

        val statusColor = when (platform.status) {
            HivePlatform.Status.OPERATIONAL -> Color.parseColor("#4CAF50")
            HivePlatform.Status.DEGRADED -> Color.parseColor("#FFC107")
            HivePlatform.Status.EMERGENCY -> Color.parseColor("#FF0000")  // Bright red for SOS
            else -> Color.parseColor("#F44336")
        }
        val statusBadge = TextView(pluginContext).apply {
            text = platform.status.name
            textSize = 12f
            setTextColor(statusColor)
        }
        header.addView(statusBadge)
        container.addView(header)

        // Emergency SOS Banner - displays when peer has triggered SOS
        // Note: ACK button deferred for MVP - SOS clears when peer cancels it
        if (platform.status == HivePlatform.Status.EMERGENCY) {
            container.addView(createSpacer(16))
            val emergencyBanner = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                setBackgroundColor(Color.parseColor("#D32F2F"))  // Red background
                setPadding(24, 16, 24, 16)
                gravity = Gravity.CENTER_VERTICAL
            }

            val alertIcon = TextView(pluginContext).apply {
                text = "⚠"
                textSize = 24f
                setTextColor(Color.WHITE)
            }
            emergencyBanner.addView(alertIcon)
            emergencyBanner.addView(createHorizontalSpacer(12))

            val alertText = TextView(pluginContext).apply {
                text = "SOS EMERGENCY ACTIVE"
                textSize = 16f
                setTextColor(Color.WHITE)
                setTypeface(null, android.graphics.Typeface.BOLD)
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            emergencyBanner.addView(alertText)

            container.addView(emergencyBanner)
        }

        // Subtitle: Platform type
        val subtitle = TextView(pluginContext).apply {
            text = "${platform.platformType.name} • ${platform.getStalenessString()}"
            textSize = 14f
            setTextColor(Color.GRAY)
        }
        container.addView(subtitle)
        container.addView(createSpacer(24))

        // Position Section
        container.addView(createSectionTitle("Position"))
        container.addView(createSpacer(8))

        val positionCard = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        positionCard.addView(createDetailRow("Latitude", String.format("%.6f° N", platform.lat)))
        positionCard.addView(createDetailRow("Longitude", String.format("%.6f° W", kotlin.math.abs(platform.lon))))

        platform.hae?.let {
            val accuracy = platform.positionAccuracy?.let { acc -> " (±${acc.toInt()}m)" } ?: ""
            positionCard.addView(createDetailRow("Altitude", "${it.toInt()}m HAE$accuracy"))
        }

        container.addView(positionCard)
        container.addView(createSpacer(16))

        // Motion Section (if available)
        if (platform.heading != null || platform.speed != null) {
            container.addView(createSectionTitle("Motion"))
            container.addView(createSpacer(8))

            val motionCard = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.VERTICAL
                setBackgroundColor(Color.parseColor("#2d2d2d"))
                setPadding(24, 16, 24, 16)
            }

            platform.heading?.let {
                motionCard.addView(createDetailRow("Heading", "${it.toInt()}°"))
            }
            platform.speed?.let {
                motionCard.addView(createDetailRow("Speed", String.format("%.1f m/s", it)))
            }
            platform.course?.let {
                motionCard.addView(createDetailRow("Course", "${it.toInt()}°"))
            }
            platform.verticalSpeed?.let {
                motionCard.addView(createDetailRow("Vertical Speed", String.format("%.1f m/s", it)))
            }

            container.addView(motionCard)
            container.addView(createSpacer(16))
        }

        // Status Section
        container.addView(createSectionTitle("Status"))
        container.addView(createSpacer(8))

        val statusCard = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        // Battery with progress bar
        platform.batteryPercent?.let { battery ->
            val batteryRow = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
            }
            val batteryLabel = TextView(pluginContext).apply {
                text = "Battery: $battery%"
                textSize = 12f
                setTextColor(Color.WHITE)
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            batteryRow.addView(batteryLabel)

            // Simple text-based progress bar
            val progressText = TextView(pluginContext).apply {
                val filled = (battery / 10)
                val empty = 10 - filled
                text = "█".repeat(filled) + "░".repeat(empty)
                textSize = 10f
                val batteryColor = when {
                    battery > 50 -> Color.parseColor("#4CAF50")
                    battery > 20 -> Color.parseColor("#FFC107")
                    else -> Color.parseColor("#F44336")
                }
                setTextColor(batteryColor)
            }
            batteryRow.addView(progressText)
            statusCard.addView(batteryRow)
        } ?: run {
            statusCard.addView(createDetailRow("Battery", "N/A"))
        }

        // Comms quality
        val commsText = platform.commsQuality?.name ?: "UNKNOWN"
        val commsColor = when (platform.commsQuality) {
            CommsQuality.EXCELLENT, CommsQuality.GOOD -> Color.parseColor("#4CAF50")
            CommsQuality.DEGRADED -> Color.parseColor("#FFC107")
            CommsQuality.POOR, CommsQuality.LOST -> Color.parseColor("#F44336")
            null -> Color.GRAY
        }
        statusCard.addView(createDetailRow("Comms", commsText, commsColor))

        // Sensors
        platform.sensorStatus?.forEach { (sensor, status) ->
            val sensorColor = when (status) {
                com.revolveteam.atak.hive.model.SensorStatus.ACTIVE -> Color.parseColor("#4CAF50")
                com.revolveteam.atak.hive.model.SensorStatus.IDLE -> Color.parseColor("#2196F3")
                com.revolveteam.atak.hive.model.SensorStatus.DEGRADED -> Color.parseColor("#FFC107")
                com.revolveteam.atak.hive.model.SensorStatus.OFFLINE -> Color.parseColor("#F44336")
                com.revolveteam.atak.hive.model.SensorStatus.UNKNOWN -> Color.GRAY
            }
            statusCard.addView(createDetailRow(sensor, status.name, sensorColor))
        }

        container.addView(statusCard)
        container.addView(createSpacer(16))

        // Mission Section (if available)
        if (platform.currentTask != null || platform.missionId != null) {
            container.addView(createSectionTitle("Mission"))
            container.addView(createSpacer(8))

            val missionCard = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.VERTICAL
                setBackgroundColor(Color.parseColor("#2d2d2d"))
                setPadding(24, 16, 24, 16)
            }

            platform.currentTask?.let {
                missionCard.addView(createDetailRow("Task", it))
            }
            platform.missionId?.let {
                missionCard.addView(createDetailRow("Mission ID", it))
            }

            container.addView(missionCard)
            container.addView(createSpacer(16))
        }

        // Capabilities Section
        if (platform.capabilities.isNotEmpty()) {
            container.addView(createSectionTitle("Capabilities"))
            container.addView(createSpacer(8))

            val capsCard = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.VERTICAL
                setBackgroundColor(Color.parseColor("#2d2d2d"))
                setPadding(24, 16, 24, 16)
            }

            val capsText = TextView(pluginContext).apply {
                text = platform.capabilities.joinToString(" • ")
                textSize = 12f
                setTextColor(Color.parseColor("#888888"))
            }
            capsCard.addView(capsText)

            container.addView(capsCard)
            container.addView(createSpacer(16))
        }

        // Action Buttons
        container.addView(createSpacer(8))
        val buttonRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }

        val focusButton = Button(pluginContext).apply {
            text = "Focus on Map"
            textSize = 12f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#2196F3"))
            setPadding(32, 16, 32, 16)
            setOnClickListener {
                Log.d(TAG, "Focus on platform: ${platform.callsign} at ${platform.lat}, ${platform.lon}")
                mapComponent.zoomToLocation(platform.lat, platform.lon)
                // Close dropdown to show map
                closeDropDown()
            }
        }
        buttonRow.addView(focusButton)

        container.addView(buttonRow)

        return container
    }

    /**
     * Build the marker detail view showing comprehensive marker information
     */
    private fun buildMarkerDetailView(cachedMarker: HiveMapComponent.CachedMarker): LinearLayout {
        val marker = cachedMarker.marker

        val container = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 32, 32, 32)
        }

        // Header with back button
        val header = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val backButton = Button(pluginContext).apply {
            text = "←"
            textSize = 16f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#444444"))
            setPadding(24, 8, 24, 8)
            setOnClickListener {
                selectedMarkerUid = null
                refreshContent()
            }
        }
        header.addView(backButton)
        header.addView(createHorizontalSpacer(16))

        val title = TextView(pluginContext).apply {
            text = marker.callsign.ifEmpty { "Marker" }
            textSize = 20f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        header.addView(title)

        // Type badge with color
        val typeColor = when {
            marker.type.contains("-h-") -> Color.parseColor("#F44336")  // Hostile
            marker.type.contains("-f-") -> Color.parseColor("#4CAF50")  // Friendly
            marker.type.contains("-n-") -> Color.parseColor("#2196F3")  // Neutral
            else -> Color.parseColor("#FFC107")  // Unknown
        }
        val typeBadge = TextView(pluginContext).apply {
            text = getMarkerTypeLabel(marker.type)
            textSize = 12f
            setTextColor(typeColor)
        }
        header.addView(typeBadge)
        container.addView(header)

        // Subtitle: Source peer and age
        val ageSec = (System.currentTimeMillis() - cachedMarker.receivedAt) / 1000
        val ageStr = when {
            ageSec < 60 -> "${ageSec}s ago"
            ageSec < 3600 -> "${ageSec / 60}m ago"
            else -> "${ageSec / 3600}h ago"
        }
        val subtitle = TextView(pluginContext).apply {
            text = "From ${cachedMarker.sourcePeerName} • $ageStr"
            textSize = 14f
            setTextColor(Color.GRAY)
        }
        container.addView(subtitle)
        container.addView(createSpacer(24))

        // Position Section
        container.addView(createSectionTitle("Position"))
        container.addView(createSpacer(8))

        val positionCard = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        positionCard.addView(createDetailRow("Latitude", String.format("%.6f°", marker.lat)))
        positionCard.addView(createDetailRow("Longitude", String.format("%.6f°", marker.lon)))
        if (marker.hae != 0f) {
            positionCard.addView(createDetailRow("Altitude", "${marker.hae.toInt()}m HAE"))
        }

        container.addView(positionCard)
        container.addView(createSpacer(16))

        // Details Section
        container.addView(createSectionTitle("Details"))
        container.addView(createSpacer(8))

        val detailsCard = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        detailsCard.addView(createDetailRow("CoT Type", marker.type))
        detailsCard.addView(createDetailRow("UID", marker.uid.takeLast(12) + "..."))

        // Format marker timestamp
        val dateFormat = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.getDefault())
        val markerTimeStr = dateFormat.format(Date(marker.time))
        detailsCard.addView(createDetailRow("Marker Time", markerTimeStr))

        // Received time
        val receivedTimeStr = dateFormat.format(Date(cachedMarker.receivedAt))
        detailsCard.addView(createDetailRow("Received", receivedTimeStr))

        container.addView(detailsCard)
        container.addView(createSpacer(16))

        // Action Buttons
        container.addView(createSpacer(8))
        val buttonRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
        }

        val focusButton = Button(pluginContext).apply {
            text = "Focus on Map"
            textSize = 12f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#2196F3"))
            setPadding(32, 16, 32, 16)
            setOnClickListener {
                Log.d(TAG, "Focus on marker: ${marker.callsign} at ${marker.lat}, ${marker.lon}")
                mapComponent.zoomToMarker(marker.uid)
                // Close dropdown to show map
                closeDropDown()
            }
        }
        buttonRow.addView(focusButton)

        container.addView(buttonRow)

        return container
    }

    private fun createSectionTitle(title: String): TextView {
        return TextView(pluginContext).apply {
            text = title
            textSize = 14f
            setTextColor(Color.WHITE)
        }
    }

    private fun createDetailRow(label: String, value: String, valueColor: Int = Color.GRAY): LinearLayout {
        return LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, 4, 0, 4)

            val labelView = TextView(pluginContext).apply {
                text = "$label:"
                textSize = 12f
                setTextColor(Color.parseColor("#888888"))
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            addView(labelView)

            val valueView = TextView(pluginContext).apply {
                text = value
                textSize = 12f
                setTextColor(valueColor)
            }
            addView(valueView)
        }
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
            setTextIsSelectable(true)
        }
        card.addView(nodeIdText)

        card.addView(createSpacer(12))

        // Unified peer list header with multi-hop support
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val hiveMesh = bleManager?.getMesh()
        val blePeers = if (bleManager?.isRunning?.value == true) {
            bleManager.peers.value ?: emptyList()
        } else emptyList<BlePeer>()
        val meshPeerCount = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0
        val directBlePeers = blePeers.filter { it.isConnected }
        val indirectBlePeers = if (hiveMesh != null) {
            hiveMesh.getIndirectPeers()
        } else emptyList()
        val totalPeers = meshPeerCount + directBlePeers.size + indirectBlePeers.size

        val peersHeader = TextView(pluginContext).apply {
            text = "Connected Peers ($totalPeers)"
            textSize = 14f
            setTextColor(Color.WHITE)
        }
        card.addView(peersHeader)

        // Show full state counts summary if BLE mesh is running
        if (hiveMesh != null) {
            val stateCounts = hiveMesh.getFullStateCounts()
            if (stateCounts != null) {
                val directCount = stateCounts.direct.connected + stateCounts.direct.degraded
                val indirectCount = stateCounts.oneHop + stateCounts.twoHop + stateCounts.threeHop
                val summaryParts = mutableListOf<String>()
                summaryParts.add("Direct: $directCount")
                if (stateCounts.oneHop > 0u) summaryParts.add("1-hop: ${stateCounts.oneHop}")
                if (stateCounts.twoHop > 0u) summaryParts.add("2-hop: ${stateCounts.twoHop}")
                if (stateCounts.threeHop > 0u) summaryParts.add("3-hop: ${stateCounts.threeHop}")
                val summaryText = TextView(pluginContext).apply {
                    text = summaryParts.joinToString(" • ")
                    textSize = 10f
                    setTextColor(Color.parseColor("#888888"))
                }
                card.addView(summaryText)
            }
        }
        card.addView(createSpacer(8))

        // Mesh peers (full HIVE)
        if (meshPeerCount > 0) {
            val peersJson = HivePluginLifecycle.getInstance()?.getConnectedPeers() ?: "[]"
            try {
                val peerIds = org.json.JSONArray(peersJson)
                for (i in 0 until peerIds.length()) {
                    val peerId = peerIds.getString(i)
                    val peerRow = LinearLayout(pluginContext).apply {
                        orientation = LinearLayout.HORIZONTAL
                        gravity = Gravity.CENTER_VERTICAL
                        setPadding(0, 4, 0, 4)
                    }

                    val indicator = TextView(pluginContext).apply {
                        text = "●"
                        textSize = 12f
                        setTextColor(Color.parseColor("#4CAF50"))
                    }
                    peerRow.addView(indicator)
                    peerRow.addView(createHorizontalSpacer(8))

                    val peerIdView = TextView(pluginContext).apply {
                        text = peerId.take(16) + "..."
                        textSize = 11f
                        setTextColor(Color.WHITE)
                        setTypeface(android.graphics.Typeface.MONOSPACE)
                        layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                    }
                    peerRow.addView(peerIdView)

                    val transportLabel = TextView(pluginContext).apply {
                        text = "mesh"
                        textSize = 10f
                        setTextColor(Color.parseColor("#2196F3"))
                    }
                    peerRow.addView(transportLabel)

                    card.addView(peerRow)
                }
            } catch (e: Exception) {
                Log.w(TAG, "Failed to parse peer IDs JSON: ${e.message}")
            }
        }

        // BLE direct peers (HIVE-lite, degree 0)
        directBlePeers.forEach { peer ->
            val peerRow = LinearLayout(pluginContext).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(0, 4, 0, 4)
            }

            val indicator = TextView(pluginContext).apply {
                text = "●"
                textSize = 12f
                setTextColor(Color.parseColor("#4CAF50"))
            }
            peerRow.addView(indicator)
            peerRow.addView(createHorizontalSpacer(8))

            val peerName = TextView(pluginContext).apply {
                text = peer.displayName()
                textSize = 11f
                setTextColor(Color.WHITE)
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            }
            peerRow.addView(peerName)

            val rssiText = TextView(pluginContext).apply {
                text = "${peer.rssi}dBm"
                textSize = 9f
                val rssiColor = when {
                    peer.rssi > -60 -> Color.parseColor("#4CAF50")
                    peer.rssi > -80 -> Color.parseColor("#FFC107")
                    else -> Color.parseColor("#F44336")
                }
                setTextColor(rssiColor)
            }
            peerRow.addView(rssiText)
            peerRow.addView(createHorizontalSpacer(8))

            val hopLabel = TextView(pluginContext).apply {
                text = "direct"
                textSize = 9f
                setTextColor(Color.parseColor("#4CAF50"))
            }
            peerRow.addView(hopLabel)

            card.addView(peerRow)
        }

        // BLE indirect peers (multi-hop, degree 1-3)
        if (indirectBlePeers.isNotEmpty()) {
            card.addView(createSpacer(8))
            val indirectHeader = TextView(pluginContext).apply {
                text = "Multi-hop Peers (${indirectBlePeers.size})"
                textSize = 11f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(indirectHeader)

            indirectBlePeers.forEach { peer ->
                val peerRow = LinearLayout(pluginContext).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, 4, 0, 4)
                }

                val indicator = TextView(pluginContext).apply {
                    text = "◐"  // Half-filled circle for indirect
                    textSize = 12f
                    setTextColor(Color.parseColor("#2196F3"))
                }
                peerRow.addView(indicator)
                peerRow.addView(createHorizontalSpacer(8))

                // IndirectPeer has nodeId and degree properties
                val peerName = TextView(pluginContext).apply {
                    text = String.format("%08X", peer.nodeId)
                    textSize = 11f
                    setTextColor(Color.WHITE)
                    setTypeface(android.graphics.Typeface.MONOSPACE)
                    layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                }
                peerRow.addView(peerName)

                // Use minHops from IndirectPeer directly
                val hops = peer.minHops
                val hopText = when (hops.toInt()) {
                    1 -> "1-hop"
                    2 -> "2-hop"
                    3 -> "3-hop"
                    else -> "${hops}-hop"
                }
                val hopColor = when (hops.toInt()) {
                    1 -> Color.parseColor("#4CAF50")  // Green - close
                    2 -> Color.parseColor("#FFC107")  // Yellow - medium
                    3 -> Color.parseColor("#FF9800")  // Orange - far
                    else -> Color.parseColor("#2196F3")  // Blue - unknown
                }
                val hopLabel = TextView(pluginContext).apply {
                    text = hopText
                    textSize = 9f
                    setTextColor(hopColor)
                }
                peerRow.addView(hopLabel)

                card.addView(peerRow)
            }
        }

        // Show discovered (not connected) BLE peers
        val discoveredBle = blePeers.filter { !it.isConnected }
        if (discoveredBle.isNotEmpty()) {
            card.addView(createSpacer(8))
            val discoveredHeader = TextView(pluginContext).apply {
                text = "Discovered (${discoveredBle.size})"
                textSize = 11f
                setTextColor(Color.parseColor("#666666"))
            }
            card.addView(discoveredHeader)

            discoveredBle.forEach { peer ->
                val peerRow = LinearLayout(pluginContext).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, 2, 0, 2)
                }

                val indicator = TextView(pluginContext).apply {
                    text = "○"
                    textSize = 12f
                    setTextColor(Color.GRAY)
                }
                peerRow.addView(indicator)
                peerRow.addView(createHorizontalSpacer(8))

                val peerName = TextView(pluginContext).apply {
                    text = peer.displayName()
                    textSize = 10f
                    setTextColor(Color.GRAY)
                    layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                }
                peerRow.addView(peerName)

                val rssiText = TextView(pluginContext).apply {
                    text = "${peer.rssi}dBm"
                    textSize = 9f
                    setTextColor(Color.parseColor("#666666"))
                }
                peerRow.addView(rssiText)

                card.addView(peerRow)
            }
        }

        // No peers message
        if (totalPeers == 0 && discoveredBle.isEmpty()) {
            val noPeers = TextView(pluginContext).apply {
                text = "No peers connected"
                textSize = 11f
                setTextColor(Color.GRAY)
            }
            card.addView(noPeers)
        }

        // BLE controls (compact)
        card.addView(createSpacer(12))
        val bleControlRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val bleLabel = TextView(pluginContext).apply {
            text = "BLE scan: "
            textSize = 11f
            setTextColor(Color.GRAY)
        }
        bleControlRow.addView(bleLabel)

        val isRunning = bleManager?.isRunning?.value ?: false
        val bleToggle = Button(pluginContext).apply {
            text = if (isRunning) "Stop" else "Start"
            textSize = 10f
            setBackgroundColor(if (isRunning) Color.parseColor("#F44336") else Color.parseColor("#4CAF50"))
            setTextColor(Color.WHITE)
            setPadding(24, 8, 24, 8)
            setOnClickListener {
                if (isRunning) {
                    HivePluginLifecycle.getInstance()?.stopBleMesh()
                } else {
                    HivePluginLifecycle.getInstance()?.startBleMesh()
                }
                handler.postDelayed({ refreshContent() }, 100)
            }
        }
        bleControlRow.addView(bleToggle)

        // Restart button - stops and restarts BLE mesh to force reconnection
        if (isRunning) {
            bleControlRow.addView(createHorizontalSpacer(8))
            val restartBtn = Button(pluginContext).apply {
                text = "Restart"
                textSize = 10f
                setBackgroundColor(Color.parseColor("#2196F3"))
                setTextColor(Color.WHITE)
                setPadding(24, 8, 24, 8)
                setOnClickListener {
                    HivePluginLifecycle.getInstance()?.stopBleMesh()
                    handler.postDelayed({
                        HivePluginLifecycle.getInstance()?.startBleMesh()
                        handler.postDelayed({ refreshContent() }, 500)
                    }, 500)
                }
            }
            bleControlRow.addView(restartBtn)
        }

        if (bleManager != null && !bleManager.hasPermissions()) {
            bleControlRow.addView(createHorizontalSpacer(8))
            val permWarning = TextView(pluginContext).apply {
                text = "⚠ permissions"
                textSize = 10f
                setTextColor(Color.parseColor("#FFC107"))
            }
            bleControlRow.addView(permWarning)
        }

        card.addView(bleControlRow)

        return card
    }

    private fun createPliBroadcastSection(): View {
        val section = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
        }

        // Title
        val title = TextView(pluginContext).apply {
            text = "PLI Broadcast"
            textSize = 16f
            setTextColor(Color.WHITE)
        }
        section.addView(title)
        section.addView(createSpacer(12))

        // Card with toggle and status
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        // Description
        val description = TextView(pluginContext).apply {
            text = "Share your position with HIVE network peers"
            textSize = 12f
            setTextColor(Color.GRAY)
        }
        card.addView(description)
        card.addView(createSpacer(12))

        // Toggle button row
        val buttonRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val isEnabled = mapComponent.pliBroadcastEnabled
        val toggleButton = Button(pluginContext).apply {
            text = if (isEnabled) "Stop Broadcasting" else "Start Broadcasting"
            setBackgroundColor(if (isEnabled) Color.parseColor("#F44336") else Color.parseColor("#4CAF50"))
            setTextColor(Color.WHITE)
            textSize = 12f
            setPadding(32, 16, 32, 16)

            setOnClickListener {
                val newState = !mapComponent.pliBroadcastEnabled
                mapComponent.setPliBroadcastEnabled(newState)
                // Refresh the UI to show updated state
                handler.postDelayed({ refreshContent() }, 100)
            }
        }
        buttonRow.addView(toggleButton)
        card.addView(buttonRow)
        card.addView(createSpacer(12))

        // Status indicator
        val statusColor = if (isEnabled) Color.parseColor("#4CAF50") else Color.GRAY
        val statusText = TextView(pluginContext).apply {
            text = "Status: ${mapComponent.lastBroadcastStatus}"
            textSize = 11f
            setTextColor(statusColor)
        }
        card.addView(statusText)

        section.addView(card)
        return section
    }

    /**
     * Create the BLE mesh section showing WearTAK peer connectivity.
     *
     * Note: This is currently a separate transport for WearTAK devices.
     * BLE transport unification with hive-ffi is in progress (ADR-039, #558).
     * PLI does not automatically sync over BLE yet - requires Android adapter integration.
     */
    private fun createBleMeshSection(): View {
        val section = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
        }

        // Title
        val title = TextView(pluginContext).apply {
            text = "WearTAK BLE Sync"
            textSize = 16f
            setTextColor(Color.WHITE)
        }
        section.addView(title)

        // Clarification note
        val note = TextView(pluginContext).apply {
            text = "Direct BLE connection to WearTAK watches"
            textSize = 11f
            setTextColor(Color.parseColor("#888888"))
        }
        section.addView(note)
        section.addView(createSpacer(12))

        // Card with status and peers
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val isRunning = bleManager?.isRunning?.value ?: false
        val blePeers = bleManager?.peers?.value ?: emptyList<BlePeer>()
        val connectedCount = blePeers.count { it.isConnected }

        Log.i(TAG, "BLE UI: bleManager=${System.identityHashCode(bleManager)}, isRunning=$isRunning, " +
                "peers=${blePeers.size}, connected=$connectedCount, " +
                "_peers=${bleManager?.let { System.identityHashCode(it.peers) }}")
        blePeers.forEach { peer ->
            Log.d(TAG, "  BLE peer: ${peer.displayName()} connected=${peer.isConnected}")
        }

        // Status row
        val statusRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val statusColor = if (isRunning) Color.parseColor("#4CAF50") else Color.parseColor("#F44336")
        val statusIndicator = TextView(pluginContext).apply {
            text = "●"
            textSize = 14f
            setTextColor(statusColor)
        }
        statusRow.addView(statusIndicator)
        statusRow.addView(createHorizontalSpacer(8))

        val statusText = TextView(pluginContext).apply {
            text = if (isRunning) {
                "Active • $connectedCount connected, ${blePeers.size} discovered"
            } else {
                "Inactive"
            }
            textSize = 12f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        statusRow.addView(statusText)
        card.addView(statusRow)

        // Mesh ID configuration row
        card.addView(createSpacer(12))
        val meshIdRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val meshIdLabel = TextView(pluginContext).apply {
            text = "Mesh ID: "
            textSize = 12f
            setTextColor(Color.WHITE)
        }
        meshIdRow.addView(meshIdLabel)

        val currentMeshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId() ?: HivePluginLifecycle.DEFAULT_MESH_ID
        val meshIdInput = android.widget.EditText(pluginContext).apply {
            setText(currentMeshId)
            textSize = 12f
            setTextColor(Color.WHITE)
            setBackgroundColor(Color.parseColor("#3d3d3d"))
            setPadding(16, 8, 16, 8)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
            setSingleLine(true)
        }
        meshIdRow.addView(meshIdInput)
        meshIdRow.addView(createHorizontalSpacer(8))

        val applyButton = Button(pluginContext).apply {
            text = "Apply"
            textSize = 10f
            setBackgroundColor(Color.parseColor("#2196F3"))
            setTextColor(Color.WHITE)
            setPadding(16, 8, 16, 8)

            setOnClickListener {
                val newMeshId = meshIdInput.text.toString().trim()
                if (newMeshId.isNotEmpty()) {
                    HivePluginLifecycle.getInstance()?.setMeshId(pluginContext, newMeshId)
                    handler.postDelayed({ refreshContent() }, 200)
                }
            }
        }
        meshIdRow.addView(applyButton)
        card.addView(meshIdRow)

        // Toggle button
        card.addView(createSpacer(12))
        val toggleButton = Button(pluginContext).apply {
            text = if (isRunning) "Stop BLE Mesh" else "Start BLE Mesh"
            setBackgroundColor(if (isRunning) Color.parseColor("#F44336") else Color.parseColor("#4CAF50"))
            setTextColor(Color.WHITE)
            textSize = 12f
            setPadding(32, 16, 32, 16)

            setOnClickListener {
                if (isRunning) {
                    HivePluginLifecycle.getInstance()?.stopBleMesh()
                } else {
                    HivePluginLifecycle.getInstance()?.startBleMesh()
                }
                handler.postDelayed({ refreshContent() }, 100)
            }
        }
        card.addView(toggleButton)

        // Note about PLI sync and unified transport migration status
        if (isRunning) {
            card.addView(createSpacer(8))
            val pliNote = TextView(pluginContext).apply {
                // ADR-039, #558: Unified transport configured but using fallback BLE manager
                // until Android BLE adapter integration in hive-btle is complete
                text = "Unified transport: pending Android adapter (#558)"
                textSize = 10f
                setTextColor(Color.parseColor("#666666"))
            }
            card.addView(pliNote)
        }

        // Peer list (if running and has peers)
        if (isRunning && blePeers.isNotEmpty()) {
            card.addView(createSpacer(16))

            val peersTitle = TextView(pluginContext).apply {
                text = "Discovered Peers"
                textSize = 12f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(peersTitle)
            card.addView(createSpacer(8))

            blePeers.forEach { peer ->
                val peerRow = LinearLayout(pluginContext).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, 4, 0, 4)
                }

                val connIndicator = TextView(pluginContext).apply {
                    text = if (peer.isConnected) "●" else "○"
                    textSize = 12f
                    setTextColor(if (peer.isConnected) Color.parseColor("#4CAF50") else Color.GRAY)
                }
                peerRow.addView(connIndicator)
                peerRow.addView(createHorizontalSpacer(8))

                val peerName = TextView(pluginContext).apply {
                    text = peer.displayName()
                    textSize = 11f
                    setTextColor(Color.WHITE)
                    layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                }
                peerRow.addView(peerName)

                val rssiText = TextView(pluginContext).apply {
                    text = "${peer.rssi} dBm"
                    textSize = 10f
                    val rssiColor = when {
                        peer.rssi > -60 -> Color.parseColor("#4CAF50")  // Strong
                        peer.rssi > -80 -> Color.parseColor("#FFC107")  // Medium
                        else -> Color.parseColor("#F44336")             // Weak
                    }
                    setTextColor(rssiColor)
                }
                peerRow.addView(rssiText)

                card.addView(peerRow)
            }
        }

        // Permission warning if needed
        if (bleManager != null && !bleManager.hasPermissions()) {
            card.addView(createSpacer(12))
            val permWarning = TextView(pluginContext).apply {
                text = "⚠ BLE permissions required. Grant in Android Settings."
                textSize = 11f
                setTextColor(Color.parseColor("#FFC107"))
            }
            card.addView(permWarning)
        }

        section.addView(card)
        return section
    }

    /**
     * Create the squad leader summary view showing aggregated squad information
     */
    private fun createSquadLeaderSummary(): View {
        val section = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
        }

        // Title with role indicator
        val titleRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        // Use current cell name for the summary title
        val cellName = HivePluginLifecycle.getInstance()?.getCurrentCellId() ?: "Squad"
        val title = TextView(pluginContext).apply {
            text = "$cellName Summary"
            textSize = 16f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        titleRow.addView(title)

        val roleIndicator = TextView(pluginContext).apply {
            text = userRole.toDisplayString()
            textSize = 11f
            setTextColor(Color.parseColor("#2196F3"))
        }
        titleRow.addView(roleIndicator)
        section.addView(titleRow)
        section.addView(createSpacer(12))

        // Summary card
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
        }

        // Get platforms in current mesh/cell (mesh_id == cell_id)
        val meshId = HivePluginLifecycle.getInstance()?.getCurrentMeshId()
        val unitPlatforms = mapComponent.platforms.filter { platform ->
            platform.cellId == meshId || meshId.isNullOrEmpty()
        }

        // Platform count by status
        val operational = unitPlatforms.count { it.status == HivePlatform.Status.OPERATIONAL }
        val degraded = unitPlatforms.count { it.status == HivePlatform.Status.DEGRADED }
        val offline = unitPlatforms.count {
            it.status == HivePlatform.Status.OFFLINE ||
            it.status == HivePlatform.Status.LOST_COMMS
        }

        val platformsHeader = TextView(pluginContext).apply {
            text = "Platforms: ${unitPlatforms.size} total"
            textSize = 13f
            setTextColor(Color.WHITE)
        }
        card.addView(platformsHeader)

        // Status breakdown
        val statusRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, 8, 0, 8)
        }

        if (operational > 0) {
            val opText = TextView(pluginContext).apply {
                text = "● $operational Operational  "
                textSize = 11f
                setTextColor(Color.parseColor("#4CAF50"))
            }
            statusRow.addView(opText)
        }
        if (degraded > 0) {
            val degText = TextView(pluginContext).apply {
                text = "● $degraded Degraded  "
                textSize = 11f
                setTextColor(Color.parseColor("#FFC107"))
            }
            statusRow.addView(degText)
        }
        if (offline > 0) {
            val offText = TextView(pluginContext).apply {
                text = "● $offline Offline"
                textSize = 11f
                setTextColor(Color.parseColor("#F44336"))
            }
            statusRow.addView(offText)
        }
        card.addView(statusRow)
        card.addView(createSpacer(8))

        // Aggregated capabilities
        val allCaps = unitPlatforms.flatMap { it.capabilities }.groupingBy { it }.eachCount()
        if (allCaps.isNotEmpty()) {
            val capsTitle = TextView(pluginContext).apply {
                text = "Capabilities"
                textSize = 12f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(capsTitle)

            val capsText = TextView(pluginContext).apply {
                text = allCaps.entries.joinToString("  ") { (cap, count) -> "✓ $cap ($count)" }
                textSize = 11f
                setTextColor(Color.parseColor("#4CAF50"))
            }
            card.addView(capsText)
            card.addView(createSpacer(8))
        }

        // Geographic spread (if platforms have positions)
        if (unitPlatforms.size >= 2) {
            val lats = unitPlatforms.map { it.lat }
            val lons = unitPlatforms.map { it.lon }
            val latSpread = (lats.maxOrNull()!! - lats.minOrNull()!!) * 111_000 // meters
            val lonSpread = (lons.maxOrNull()!! - lons.minOrNull()!!) * 111_000 * kotlin.math.cos(Math.toRadians(lats.average()))

            val spreadText = TextView(pluginContext).apply {
                text = "Spread: ${latSpread.toInt()}m × ${lonSpread.toInt()}m"
                textSize = 11f
                setTextColor(Color.parseColor("#888888"))
            }
            card.addView(spreadText)
            card.addView(createSpacer(8))
        }

        // Comms health
        val commsKnown = unitPlatforms.filter { it.commsQuality != null }
        if (commsKnown.isNotEmpty()) {
            val goodComms = commsKnown.count {
                it.commsQuality == CommsQuality.EXCELLENT || it.commsQuality == CommsQuality.GOOD
            }
            val healthPercent = (goodComms * 100) / commsKnown.size
            val healthColor = when {
                healthPercent >= 80 -> Color.parseColor("#4CAF50")
                healthPercent >= 50 -> Color.parseColor("#FFC107")
                else -> Color.parseColor("#F44336")
            }
            val healthText = TextView(pluginContext).apply {
                text = "Comms Health: $healthPercent%"
                textSize = 11f
                setTextColor(healthColor)
            }
            card.addView(healthText)
        }

        section.addView(card)
        return section
    }

    override fun onDropDownSelectionRemoved() {}
    override fun onDropDownVisible(v: Boolean) {
        isDropDownVisible = v
        Log.d(TAG, "DropDown visible: $v")
    }
    override fun onDropDownSizeChanged(width: Double, height: Double) {}
    override fun onDropDownClose() {
        isDropDownVisible = false
    }
}
