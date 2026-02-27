package com.peat.demo

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.peat.btle.PeatBtle
import com.peat.btle.PeatEventType
import com.peat.btle.PeatMeshListener
import com.peat.btle.PeatPeer
import java.util.concurrent.ConcurrentHashMap

/**
 * Demo activity for PEAT BLE mesh connectivity.
 *
 * This app demonstrates:
 * 1. Starting a PEAT mesh network
 * 2. Automatic peer discovery and connection
 * 3. Sending/receiving events (Emergency, ACK)
 * 4. CRDT sync across the mesh
 */
class MainActivity : AppCompatActivity(), PeatMeshListener {

    companion object {
        private const val TAG = "PeatDemo"
    }

    private lateinit var peatBtle: PeatBtle
    private lateinit var peerAdapter: PeerAdapter

    // UI elements
    private lateinit var localNodeIdText: TextView
    private lateinit var statusText: TextView
    private lateinit var connectedCountText: TextView
    private lateinit var peerList: RecyclerView
    private lateinit var emergencyButton: Button
    private lateinit var ackButton: Button
    private lateinit var resetButton: Button
    private lateinit var ackStatusPanel: LinearLayout
    private lateinit var ackStatusText: TextView

    // Alert state
    private var alertActive = false
    private val pendingAcks = ConcurrentHashMap<Long, Boolean>() // nodeId -> has acked
    private var emergencySourceNodeId: Long? = null

    // Permission launcher
    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { permissions ->
        val allGranted = permissions.all { it.value }
        if (allGranted) {
            Log.i(TAG, "All permissions granted")
            initializePeatBtle()
        } else {
            Log.e(TAG, "Permissions denied: ${permissions.filter { !it.value }.keys}")
            Toast.makeText(this, "Bluetooth permissions required", Toast.LENGTH_LONG).show()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        // Initialize UI
        localNodeIdText = findViewById(R.id.localNodeIdText)
        statusText = findViewById(R.id.statusText)
        connectedCountText = findViewById(R.id.connectedDeviceText)
        peerList = findViewById(R.id.deviceList)
        emergencyButton = findViewById(R.id.emergencyButton)
        ackButton = findViewById(R.id.ackButton)
        resetButton = findViewById(R.id.resetButton)
        ackStatusPanel = findViewById(R.id.ackStatusPanel)
        ackStatusText = findViewById(R.id.ackStatusText)

        // Setup RecyclerView
        peerAdapter = PeerAdapter()
        peerList.layoutManager = LinearLayoutManager(this)
        peerList.adapter = peerAdapter

        // Setup button listeners
        emergencyButton.setOnClickListener { sendEmergency() }
        ackButton.setOnClickListener { sendAck() }
        resetButton.setOnClickListener { resetAlert() }

        // Check permissions
        if (hasAllPermissions()) {
            initializePeatBtle()
        } else {
            requestPermissions()
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        if (::peatBtle.isInitialized) {
            Log.i(TAG, "Shutting down PEAT mesh")
            peatBtle.shutdown()
        }
    }

    private fun hasAllPermissions(): Boolean {
        val permissions = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
                Manifest.permission.BLUETOOTH_ADVERTISE
            )
        } else {
            arrayOf(
                Manifest.permission.BLUETOOTH,
                Manifest.permission.BLUETOOTH_ADMIN,
                Manifest.permission.ACCESS_FINE_LOCATION
            )
        }
        return permissions.all {
            ContextCompat.checkSelfPermission(this, it) == PackageManager.PERMISSION_GRANTED
        }
    }

    private fun requestPermissions() {
        val permissions = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
                Manifest.permission.BLUETOOTH_ADVERTISE
            )
        } else {
            arrayOf(
                Manifest.permission.BLUETOOTH,
                Manifest.permission.BLUETOOTH_ADMIN,
                Manifest.permission.ACCESS_FINE_LOCATION
            )
        }
        permissionLauncher.launch(permissions)
    }

    private fun initializePeatBtle() {
        try {
            // Get mesh ID from environment (PEAT_APP_ID, PEAT_MESH_ID, or default "DEMO")
            val meshId = PeatBtle.getMeshIdFromEnvironment()
            peatBtle = PeatBtle(applicationContext, meshId = meshId) // nodeId auto-generated from adapter
            peatBtle.init()
            Log.i(TAG, "PEAT BLE initialized with nodeId: ${String.format("%08X", peatBtle.nodeId)}")

            // Update UI with our node ID and mesh ID
            localNodeIdText.text = "Node: ${PeatBtle.generateDeviceName(peatBtle.getMeshId(), peatBtle.nodeId)}"

            // Start the mesh - it handles everything automatically
            peatBtle.startMesh(this)
            updateStatus("Mesh active")

        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize PEAT BLE", e)
            updateStatus("Error: ${e.message}")
            Toast.makeText(this, "Failed to initialize: ${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    // ==================== PeatMeshListener Implementation ====================

    override fun onMeshUpdated(peers: List<PeatPeer>) {
        Log.d(TAG, "Mesh updated: ${peers.size} peers")
        for (peer in peers) {
            Log.d(TAG, "  Peer: ${peer.displayName()} connected=${peer.isConnected} rssi=${peer.rssi}")
        }
        runOnUiThread {
            peerAdapter.updatePeers(peers)

            val connectedCount = peers.count { it.isConnected }
            if (peers.isEmpty()) {
                connectedCountText.visibility = View.GONE
            } else {
                connectedCountText.text = "$connectedCount/${peers.size} peers connected"
                connectedCountText.visibility = View.VISIBLE
            }
        }
    }

    override fun onPeerEvent(peer: PeatPeer, eventType: PeatEventType) {
        Log.i(TAG, "Peer event: ${peer.displayName()} sent $eventType")

        runOnUiThread {
            when (eventType) {
                PeatEventType.EMERGENCY -> handleEmergencyReceived(peer)
                PeatEventType.ACK -> handleAckReceived(peer)
                PeatEventType.NEED_ASSIST -> {
                    Toast.makeText(this, "🆘 ${peer.displayName()} needs assistance", Toast.LENGTH_LONG).show()
                }
                PeatEventType.PING -> {
                    Toast.makeText(this, "📍 Ping from ${peer.displayName()}", Toast.LENGTH_SHORT).show()
                }
                else -> {}
            }
        }
    }

    // ==================== Event Handling ====================

    private fun handleEmergencyReceived(peer: PeatPeer) {
        alertActive = true
        emergencySourceNodeId = peer.nodeId

        // Initialize ACK tracking
        pendingAcks.clear()
        for (p in peatBtle.getPeers()) {
            pendingAcks[p.nodeId] = false
        }
        pendingAcks[peatBtle.nodeId] = false // We haven't acked yet
        pendingAcks[peer.nodeId] = true // Source has implicitly acked

        Toast.makeText(this, "🚨 EMERGENCY from ${peer.displayName()}!", Toast.LENGTH_LONG).show()
        updateStatus("⚠️ EMERGENCY - TAP ACK")
        updateAckStatusDisplay()

        // Vibrate
        vibrate()
    }

    private fun handleAckReceived(peer: PeatPeer) {
        pendingAcks[peer.nodeId] = true
        Toast.makeText(this, "✓ ACK from ${peer.displayName()}", Toast.LENGTH_SHORT).show()
        checkAllAcked()
    }

    private fun sendEmergency() {
        Log.i(TAG, ">>> SENDING EMERGENCY")
        alertActive = true
        emergencySourceNodeId = peatBtle.nodeId

        // Initialize ACK tracking
        pendingAcks.clear()
        for (peer in peatBtle.getPeers()) {
            pendingAcks[peer.nodeId] = false
        }
        pendingAcks[peatBtle.nodeId] = true // We sent it, so we're acked

        peatBtle.sendEvent(PeatEventType.EMERGENCY)
        Toast.makeText(this, "🚨 EMERGENCY SENT!", Toast.LENGTH_SHORT).show()
        updateAckStatusDisplay()
    }

    private fun sendAck() {
        Log.i(TAG, ">>> SENDING ACK")

        peatBtle.sendEvent(PeatEventType.ACK)
        Toast.makeText(this, "✓ ACK sent", Toast.LENGTH_SHORT).show()

        pendingAcks[peatBtle.nodeId] = true
        checkAllAcked()
    }

    private fun resetAlert() {
        Log.i(TAG, ">>> RESETTING ALERT")
        alertActive = false
        pendingAcks.clear()
        emergencySourceNodeId = null
        updateAckStatusDisplay()
        updateStatus("Mesh active")
        Toast.makeText(this, "Alert reset", Toast.LENGTH_SHORT).show()
    }

    private fun checkAllAcked() {
        if (pendingAcks.isNotEmpty() && pendingAcks.values.all { it }) {
            alertActive = false
            pendingAcks.clear()
            emergencySourceNodeId = null
            updateAckStatusDisplay()
            updateStatus("All peers acknowledged")
        } else {
            updateAckStatusDisplay()
        }
    }

    private fun updateAckStatusDisplay() {
        if (alertActive && pendingAcks.isNotEmpty()) {
            ackStatusPanel.visibility = View.VISIBLE

            val ackedNodes = pendingAcks.filter { it.value }.keys
            val notAckedNodes = pendingAcks.filter { !it.value }.keys

            val statusBuilder = StringBuilder()
            val meshId = if (::peatBtle.isInitialized) peatBtle.getMeshId() else PeatBtle.DEFAULT_MESH_ID
            if (ackedNodes.isNotEmpty()) {
                statusBuilder.append("✓ ACK'd: ")
                statusBuilder.append(ackedNodes.joinToString(", ") { PeatBtle.generateDeviceName(meshId, it) })
            }
            if (notAckedNodes.isNotEmpty()) {
                if (statusBuilder.isNotEmpty()) statusBuilder.append("\n")
                statusBuilder.append("⏳ Waiting: ")
                statusBuilder.append(notAckedNodes.joinToString(", ") { PeatBtle.generateDeviceName(meshId, it) })
            }

            ackStatusText.text = statusBuilder.toString()
        } else {
            ackStatusPanel.visibility = View.GONE
        }
    }

    private fun updateStatus(text: String) {
        statusText.text = text
    }

    private fun vibrate() {
        try {
            val vibrator = getSystemService(android.content.Context.VIBRATOR_SERVICE) as? android.os.Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator?.vibrate(android.os.VibrationEffect.createOneShot(500, android.os.VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator?.vibrate(500)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Vibration failed", e)
        }
    }

    // ==================== RecyclerView Adapter ====================

    inner class PeerAdapter : RecyclerView.Adapter<PeerAdapter.ViewHolder>() {

        private var peers: List<PeatPeer> = emptyList()

        fun updatePeers(newPeers: List<PeatPeer>) {
            peers = newPeers.sortedByDescending { it.rssi }
            notifyDataSetChanged()
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
            val view = LayoutInflater.from(parent.context)
                .inflate(R.layout.item_device, parent, false)
            return ViewHolder(view)
        }

        override fun onBindViewHolder(holder: ViewHolder, position: Int) {
            holder.bind(peers[position])
        }

        override fun getItemCount() = peers.size

        inner class ViewHolder(view: View) : RecyclerView.ViewHolder(view) {
            private val nameText: TextView = view.findViewById(R.id.deviceName)
            private val addressText: TextView = view.findViewById(R.id.deviceAddress)
            private val rssiText: TextView = view.findViewById(R.id.deviceRssi)

            fun bind(peer: PeatPeer) {
                nameText.text = peer.displayName()
                addressText.text = if (peer.isConnected) "● Connected" else "○ Discovered"
                rssiText.text = "${peer.rssi} dBm"

                // Color based on connection status
                val color = if (peer.isConnected) {
                    ContextCompat.getColor(itemView.context, android.R.color.holo_green_light)
                } else {
                    ContextCompat.getColor(itemView.context, android.R.color.darker_gray)
                }
                nameText.setTextColor(color)
            }
        }
    }
}
