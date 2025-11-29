package com.atakmap.android.hive.plugin

import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.compose.ui.unit.dp
import com.atakmap.android.dropdown.DropDown.OnStateListener
import com.atakmap.android.dropdown.DropDownReceiver
import com.atakmap.android.hive.plugin.model.HiveCell
import com.atakmap.android.hive.plugin.ui.theme.HivePluginTheme
import com.atakmap.android.maps.MapView
import com.atakmap.coremap.log.Log

// NOTE: This version does NOT use coroutines/Flow to avoid dependency conflicts
// with ATAK SDK 5.6 Preview's bundled libraries. Uses simple state polling instead.

/**
 * HIVE DropDown Receiver
 *
 * Manages the side panel UI for the HIVE plugin using Jetpack Compose.
 */
class HiveDropDownReceiver(
    mapView: MapView?,
    private val pluginContext: Context,
    private val mapComponent: HiveMapComponent
) : DropDownReceiver(mapView), OnStateListener {

    companion object {
        val TAG: String = HiveDropDownReceiver::class.java.simpleName
        const val SHOW_PLUGIN = "com.atakmap.android.hive.SHOW_PLUGIN"
    }

    /**
     * Context wrapper that provides the plugin context for resources
     * but the host context's application context.
     */
    private class ComposeContext(
        private val hostContext: Context,
        pluginContext: Context
    ) : ContextWrapper(pluginContext) {
        override fun getApplicationContext(): Context {
            return hostContext.applicationContext
        }
    }

    override fun disposeImpl() {
        Log.d(TAG, "HiveDropDownReceiver disposed")
    }

    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return

        if (action == SHOW_PLUGIN) {
            Log.d(TAG, "Showing HIVE plugin dropdown")

            val view = ComposeView(ComposeContext(mapView.context, pluginContext)).apply {
                setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnDetachedFromWindow)
                setContent {
                    HivePluginTheme {
                        HiveMainScreen(mapComponent = mapComponent)
                    }
                }
            }

            showDropDown(
                view,
                HALF_WIDTH, FULL_HEIGHT,
                FULL_WIDTH, HALF_HEIGHT,
                false, this
            )
        }
    }

    override fun onDropDownSelectionRemoved() {}
    override fun onDropDownVisible(v: Boolean) {}
    override fun onDropDownSizeChanged(width: Double, height: Double) {}
    override fun onDropDownClose() {}
}

/**
 * Main screen composable for HIVE plugin
 *
 * NOTE: Uses simple property reads instead of collectAsState() to avoid
 * kotlinx.coroutines dependency conflicts with ATAK SDK's bundled libraries.
 */
@Composable
fun HiveMainScreen(mapComponent: HiveMapComponent) {
    // Read simple properties directly (no coroutines)
    val cells = mapComponent.cells
    val connectionStatus = mapComponent.connectionStatus
    val peerCount = mapComponent.peerCount

    var selectedTab by remember { mutableIntStateOf(0) }
    val tabs = listOf("Cells", "Tracks", "Settings")

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        // Header with connection status and peer count
        HiveHeader(connectionStatus = connectionStatus, peerCount = peerCount)

        Spacer(modifier = Modifier.height(8.dp))

        // Tab row
        TabRow(selectedTabIndex = selectedTab) {
            tabs.forEachIndexed { index, title ->
                Tab(
                    selected = selectedTab == index,
                    onClick = { selectedTab = index },
                    text = { Text(title) }
                )
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        // Tab content
        when (selectedTab) {
            0 -> CellsTab(cells = cells, onCellClick = { mapComponent.selectCell(it.id) })
            1 -> TracksTab(mapComponent = mapComponent)
            2 -> SettingsTab(mapComponent = mapComponent)
        }
    }
}

@Composable
fun HiveHeader(connectionStatus: HiveMapComponent.ConnectionStatus, peerCount: Int = 0) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            text = "HIVE Manager",
            style = MaterialTheme.typography.headlineSmall
        )

        // Connection indicator with peer count
        val (statusText, statusColor) = when (connectionStatus) {
            HiveMapComponent.ConnectionStatus.CONNECTED ->
                "Connected ($peerCount peers)" to MaterialTheme.colorScheme.primary
            HiveMapComponent.ConnectionStatus.CONNECTING ->
                "Connecting..." to MaterialTheme.colorScheme.secondary
            HiveMapComponent.ConnectionStatus.DISCONNECTED ->
                "Disconnected" to MaterialTheme.colorScheme.error
            HiveMapComponent.ConnectionStatus.ERROR ->
                "Error" to MaterialTheme.colorScheme.error
        }

        Text(
            text = statusText,
            color = statusColor,
            style = MaterialTheme.typography.bodySmall
        )
    }
}

@Composable
fun CellsTab(cells: List<HiveCell>, onCellClick: (HiveCell) -> Unit) {
    if (cells.isEmpty()) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text("No active cells")
        }
    } else {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(cells) { cell ->
                CellCard(cell = cell, onClick = { onCellClick(cell) })
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CellCard(cell: HiveCell, onClick: () -> Unit) {
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth()
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp)
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = cell.name,
                    style = MaterialTheme.typography.titleMedium
                )
                StatusChip(status = cell.status)
            }

            Spacer(modifier = Modifier.height(4.dp))

            Text(
                text = "${cell.platformCount} platforms",
                style = MaterialTheme.typography.bodySmall
            )

            if (cell.capabilities.isNotEmpty()) {
                Spacer(modifier = Modifier.height(4.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    cell.capabilities.forEach { cap ->
                        AssistChip(
                            onClick = { },
                            label = { Text(cap, style = MaterialTheme.typography.labelSmall) }
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun StatusChip(status: HiveCell.Status) {
    val (text, color) = when (status) {
        HiveCell.Status.ACTIVE -> "Active" to MaterialTheme.colorScheme.primary
        HiveCell.Status.FORMING -> "Forming" to MaterialTheme.colorScheme.secondary
        HiveCell.Status.DEGRADED -> "Degraded" to MaterialTheme.colorScheme.tertiary
        HiveCell.Status.OFFLINE -> "Offline" to MaterialTheme.colorScheme.error
    }

    Surface(
        color = color.copy(alpha = 0.1f),
        shape = MaterialTheme.shapes.small
    ) {
        Text(
            text = text,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
            color = color,
            style = MaterialTheme.typography.labelSmall
        )
    }
}

@Composable
fun TracksTab(mapComponent: HiveMapComponent) {
    val tracks = mapComponent.tracks

    if (tracks.isEmpty()) {
        Box(
            modifier = Modifier.fillMaxSize(),
            contentAlignment = Alignment.Center
        ) {
            Text("No active tracks")
        }
    } else {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(tracks) { track ->
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text(text = track.id, style = MaterialTheme.typography.titleSmall)
                        Text(
                            text = "Classification: ${track.classification}",
                            style = MaterialTheme.typography.bodySmall
                        )
                        Text(
                            text = "Confidence: ${(track.confidence * 100).toInt()}%",
                            style = MaterialTheme.typography.bodySmall
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun SettingsTab(mapComponent: HiveMapComponent) {
    // Simplified settings without node manager (no coroutines dependency)

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(8.dp)
    ) {
        Text(
            text = "HIVE Settings",
            style = MaterialTheme.typography.titleMedium
        )

        Spacer(modifier = Modifier.height(16.dp))

        // Plugin info
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text(
                    text = "Plugin Info",
                    style = MaterialTheme.typography.titleSmall
                )
                Spacer(modifier = Modifier.height(8.dp))

                Text(
                    text = "Plugin Version: 0.1.0",
                    style = MaterialTheme.typography.bodySmall
                )

                val lifecycle = HivePluginLifecycle.getInstance()
                val ffiStatus = if (lifecycle?.isHiveFfiAvailable() == true) "Available" else "Not loaded"
                Text(
                    text = "HIVE FFI: $ffiStatus",
                    style = MaterialTheme.typography.bodySmall
                )
            }
        }

        Spacer(modifier = Modifier.height(8.dp))

        // Connection info
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text(
                    text = "Connection Info",
                    style = MaterialTheme.typography.titleSmall
                )
                Spacer(modifier = Modifier.height(8.dp))

                Text(
                    text = "Status: ${mapComponent.connectionStatus.name}",
                    style = MaterialTheme.typography.bodySmall
                )
                Text(
                    text = "Peers: ${mapComponent.peerCount}",
                    style = MaterialTheme.typography.bodySmall
                )
            }
        }

        Spacer(modifier = Modifier.height(16.dp))

        // Refresh button
        Button(
            onClick = { mapComponent.refreshData() },
            modifier = Modifier.fillMaxWidth()
        ) {
            Text("Refresh Data")
        }
    }
}

