package com.revolveteam.atak.hive

import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.os.Handler
import android.os.Looper
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
import com.revolveteam.atak.hive.model.HivePlatform
import com.revolveteam.atak.hive.model.HiveRole
import com.revolveteam.atak.hive.model.HiveTrack
import com.revolveteam.atak.hive.model.CommsQuality
import com.revolveteam.hive.HivePeer as BlePeer

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

    // Platform detail view state
    private var selectedPlatform: HivePlatform? = null

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
        // If a platform is selected, show detail view
        selectedPlatform?.let { platform ->
            return buildPlatformDetailView(platform)
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

        val title = TextView(pluginContext).apply {
            text = if (selectedCellId != null) "Cell: $selectedCellName" else "HIVE Manager"
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
            val roleRow = TextView(pluginContext).apply {
                text = "Role: ${userRole.toDisplayString()} • ${userRole.unitName.ifEmpty { userRole.unitId }}"
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
            container.addView(createSpacer(8))

            val cellsHint = TextView(pluginContext).apply {
                text = "Tap a cell on the map to view its platforms"
                textSize = 11f
                setTextColor(Color.parseColor("#888888"))
            }
            container.addView(cellsHint)
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

    private fun createPlatformCard(platform: HivePlatform): View {
        val card = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#2d2d2d"))
            setPadding(24, 16, 24, 16)
            // Make card clickable
            isClickable = true
            isFocusable = true
            setOnClickListener {
                selectedPlatform = platform
                refreshContent()
            }
        }

        val headerRow = LinearLayout(pluginContext).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val name = TextView(pluginContext).apply {
            text = platform.callsign
            textSize = 14f
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        }
        headerRow.addView(name)

        val statusColor = when (platform.status) {
            HivePlatform.Status.OPERATIONAL -> Color.parseColor("#4CAF50")
            HivePlatform.Status.DEGRADED -> Color.parseColor("#FFC107")
            else -> Color.parseColor("#F44336")
        }
        val statusText = TextView(pluginContext).apply {
            text = platform.status.name
            textSize = 12f
            setTextColor(statusColor)
        }
        headerRow.addView(statusText)
        card.addView(headerRow)

        val typeInfo = TextView(pluginContext).apply {
            text = "${platform.platformType.name} • ${String.format("%.4f, %.4f", platform.lat, platform.lon)}"
            textSize = 12f
            setTextColor(Color.GRAY)
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
            text = "Tap for details →"
            textSize = 10f
            setTextColor(Color.parseColor("#666666"))
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
                selectedPlatform = null
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
            else -> Color.parseColor("#F44336")
        }
        val statusBadge = TextView(pluginContext).apply {
            text = platform.status.name
            textSize = 12f
            setTextColor(statusColor)
        }
        header.addView(statusBadge)
        container.addView(header)

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
                // TODO: Center map on platform location
                Log.d(TAG, "Focus on platform: ${platform.callsign} at ${platform.lat}, ${platform.lon}")
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

        // Unified peer list header
        val bleManager = HivePluginLifecycle.getInstance()?.getHiveBleManager()
        val blePeers = if (bleManager?.isRunning?.value == true) {
            bleManager.peers.value ?: emptyList()
        } else emptyList<BlePeer>()
        val meshPeerCount = HivePluginLifecycle.getInstance()?.getPeerCount() ?: 0
        val blePeerCount = blePeers.count { it.isConnected }
        val totalPeers = meshPeerCount + blePeerCount

        val peersHeader = TextView(pluginContext).apply {
            text = "Connected Peers ($totalPeers)"
            textSize = 14f
            setTextColor(Color.WHITE)
        }
        card.addView(peersHeader)
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

        // BLE peers (HIVE-lite)
        blePeers.filter { it.isConnected }.forEach { peer ->
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

            val transportLabel = TextView(pluginContext).apply {
                text = "lite"
                textSize = 10f
                setTextColor(Color.parseColor("#9C27B0"))
            }
            peerRow.addView(transportLabel)

            card.addView(peerRow)
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

        // Note about PLI sync
        if (isRunning) {
            card.addView(createSpacer(8))
            val pliNote = TextView(pluginContext).apply {
                text = "Note: PLI broadcast not yet synced over BLE"
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

        val title = TextView(pluginContext).apply {
            text = "${userRole.unitName.ifEmpty { "Squad" }} Summary"
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

        // Get platforms in user's unit
        val unitPlatforms = mapComponent.platforms.filter { platform ->
            platform.cellId == userRole.unitId || userRole.unitId.isEmpty()
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
