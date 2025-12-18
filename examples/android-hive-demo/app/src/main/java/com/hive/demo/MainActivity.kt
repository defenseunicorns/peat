package com.hive.demo

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.hive.btle.DiscoveredDevice
import com.hive.btle.HiveBtle
import java.util.concurrent.ConcurrentHashMap

/**
 * Demo activity for HIVE BLE mesh connectivity.
 *
 * This app demonstrates:
 * 1. Scanning for HIVE BLE nodes (e.g., M5Stack Core2 devices)
 * 2. Connecting to discovered nodes
 * 3. Advertising as a HIVE node
 * 4. CRDT sync data exchange
 */
class MainActivity : AppCompatActivity() {

    companion object {
        private const val TAG = "HiveDemo"
        private const val NODE_ID = 0x12345678L // Demo node ID
    }

    private lateinit var hiveBtle: HiveBtle
    private lateinit var deviceAdapter: DeviceAdapter
    private val discoveredDevices = ConcurrentHashMap<String, DiscoveredDevice>()

    // UI elements
    private lateinit var statusText: TextView
    private lateinit var scanButton: Button
    private lateinit var advertiseButton: Button
    private lateinit var deviceList: RecyclerView

    // Permission launcher
    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { permissions ->
        val allGranted = permissions.all { it.value }
        if (allGranted) {
            Log.i(TAG, "All permissions granted")
            initializeHiveBtle()
        } else {
            Log.e(TAG, "Permissions denied: ${permissions.filter { !it.value }.keys}")
            Toast.makeText(this, "Bluetooth permissions required", Toast.LENGTH_LONG).show()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        // Initialize UI
        statusText = findViewById(R.id.statusText)
        scanButton = findViewById(R.id.scanButton)
        advertiseButton = findViewById(R.id.advertiseButton)
        deviceList = findViewById(R.id.deviceList)

        // Setup RecyclerView
        deviceAdapter = DeviceAdapter(
            onDeviceClick = { device -> connectToDevice(device) }
        )
        deviceList.layoutManager = LinearLayoutManager(this)
        deviceList.adapter = deviceAdapter

        // Setup button listeners
        scanButton.setOnClickListener { toggleScan() }
        advertiseButton.setOnClickListener { toggleAdvertise() }

        // Check permissions
        if (hasAllPermissions()) {
            initializeHiveBtle()
        } else {
            requestPermissions()
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        if (::hiveBtle.isInitialized) {
            hiveBtle.shutdown()
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

    private fun initializeHiveBtle() {
        try {
            hiveBtle = HiveBtle(applicationContext, NODE_ID)
            hiveBtle.init()
            updateStatus("Ready - Node ID: ${String.format("%08X", NODE_ID)}")
            Log.i(TAG, "HIVE BLE initialized")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize HIVE BLE", e)
            updateStatus("Error: ${e.message}")
            Toast.makeText(this, "Failed to initialize: ${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    private fun toggleScan() {
        if (!::hiveBtle.isInitialized) {
            Toast.makeText(this, "Not initialized", Toast.LENGTH_SHORT).show()
            return
        }

        if (hiveBtle.isScanning()) {
            hiveBtle.stopScan()
            scanButton.text = "Start Scan"
            updateStatus("Scan stopped")
        } else {
            discoveredDevices.clear()
            deviceAdapter.updateDevices(emptyList())

            hiveBtle.startScan { device ->
                runOnUiThread {
                    onDeviceDiscovered(device)
                }
            }
            scanButton.text = "Stop Scan"
            updateStatus("Scanning for HIVE devices...")
        }
    }

    private fun toggleAdvertise() {
        if (!::hiveBtle.isInitialized) {
            Toast.makeText(this, "Not initialized", Toast.LENGTH_SHORT).show()
            return
        }

        if (hiveBtle.isAdvertising()) {
            hiveBtle.stopAdvertising()
            advertiseButton.text = "Start Advertise"
            updateStatus("Advertising stopped")
        } else {
            hiveBtle.startAdvertising()
            advertiseButton.text = "Stop Advertise"
            updateStatus("Advertising as HIVE-${String.format("%08X", NODE_ID)}")
        }
    }

    private fun onDeviceDiscovered(device: DiscoveredDevice) {
        Log.d(TAG, "Discovered: ${device.address} (${device.name}) RSSI=${device.rssi}")
        discoveredDevices[device.address] = device
        deviceAdapter.updateDevices(discoveredDevices.values.toList().sortedByDescending { it.rssi })
    }

    private fun connectToDevice(device: DiscoveredDevice) {
        Log.i(TAG, "Connecting to ${device.address}")
        updateStatus("Connecting to ${device.name.ifEmpty { device.address }}...")

        try {
            val connection = hiveBtle.connect(device.address)
            if (connection != null) {
                updateStatus("Connected to ${device.address}")
                Toast.makeText(this, "Connected!", Toast.LENGTH_SHORT).show()
            } else {
                updateStatus("Connection failed")
                Toast.makeText(this, "Connection failed", Toast.LENGTH_SHORT).show()
            }
        } catch (e: Exception) {
            Log.e(TAG, "Connection error", e)
            updateStatus("Error: ${e.message}")
        }
    }

    private fun updateStatus(text: String) {
        runOnUiThread {
            statusText.text = text
        }
    }

    /**
     * RecyclerView adapter for discovered devices
     */
    inner class DeviceAdapter(
        private val onDeviceClick: (DiscoveredDevice) -> Unit
    ) : RecyclerView.Adapter<DeviceAdapter.ViewHolder>() {

        private var devices: List<DiscoveredDevice> = emptyList()

        fun updateDevices(newDevices: List<DiscoveredDevice>) {
            devices = newDevices
            notifyDataSetChanged()
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
            val view = LayoutInflater.from(parent.context)
                .inflate(R.layout.item_device, parent, false)
            return ViewHolder(view)
        }

        override fun onBindViewHolder(holder: ViewHolder, position: Int) {
            val device = devices[position]
            holder.bind(device)
        }

        override fun getItemCount() = devices.size

        inner class ViewHolder(view: View) : RecyclerView.ViewHolder(view) {
            private val nameText: TextView = view.findViewById(R.id.deviceName)
            private val addressText: TextView = view.findViewById(R.id.deviceAddress)
            private val rssiText: TextView = view.findViewById(R.id.deviceRssi)

            fun bind(device: DiscoveredDevice) {
                nameText.text = device.name.ifEmpty { "Unknown" }
                addressText.text = device.address
                rssiText.text = "${device.rssi} dBm"

                // Highlight HIVE devices
                if (device.name.startsWith("HIVE-") || device.nodeId != null) {
                    nameText.setTextColor(ContextCompat.getColor(itemView.context, android.R.color.holo_green_dark))
                } else {
                    nameText.setTextColor(ContextCompat.getColor(itemView.context, android.R.color.white))
                }

                itemView.setOnClickListener { onDeviceClick(device) }
            }
        }
    }
}
