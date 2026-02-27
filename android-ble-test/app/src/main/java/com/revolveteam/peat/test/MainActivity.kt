/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.peat.test

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.widget.Button
import android.widget.ScrollView
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : AppCompatActivity() {

    companion object {
        private const val PERMISSION_REQUEST_CODE = 1001
    }

    private lateinit var btnRun: Button
    private lateinit var tvLog: TextView
    private lateinit var scrollView: ScrollView
    private val logBuilder = SpannableStringBuilder()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        btnRun = findViewById(R.id.btnRunTest)
        tvLog = findViewById(R.id.tvLog)
        scrollView = findViewById(R.id.scrollView)

        btnRun.setOnClickListener {
            if (checkPermissions()) {
                runTest()
            } else {
                requestPermissions()
            }
        }

        appendLog("PEAT BLE Test App ready.", false)
        appendLog("Ensure Pi is running: ~/ble_responder --mesh-id FUNCTEST --callsign PI-RESP", false)

        val quicNodeId = intent.getStringExtra("quic_node_id")
        val quicAddr = intent.getStringExtra("quic_address")
        if (!quicNodeId.isNullOrEmpty()) {
            appendLog("Dual-transport mode: QUIC peer=${quicNodeId.take(16)}..., addr=${quicAddr ?: "mDNS"}", false)
        }

        // Auto-run when launched with --ez auto_run true (for CI automation)
        val autoRun = intent.getBooleanExtra("auto_run", false)
        if (autoRun) {
            appendLog("Auto-run mode enabled, starting test...", false)
            if (checkPermissions()) {
                runTest()
            } else {
                requestPermissions()
            }
        } else {
            appendLog("Tap 'Run Test' to begin.", false)
        }
    }

    private fun checkPermissions(): Boolean {
        val perms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Android 12+: BLE scan/connect only (neverForLocation in manifest)
            listOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT
            )
        } else {
            listOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        return perms.all {
            ContextCompat.checkSelfPermission(this, it) == PackageManager.PERMISSION_GRANTED
        }
    }

    private fun requestPermissions() {
        val perms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT
            )
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        ActivityCompat.requestPermissions(this, perms, PERMISSION_REQUEST_CODE)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == PERMISSION_REQUEST_CODE) {
            if (grantResults.all { it == PackageManager.PERMISSION_GRANTED }) {
                runTest()
            } else {
                appendLog("ERROR: BLE permissions denied", true)
            }
        }
    }

    private fun runTest() {
        logBuilder.clear()
        tvLog.text = ""
        btnRun.isEnabled = false

        // Shared log callback for both test suites
        val logCb = { message: String, isError: Boolean ->
            runOnUiThread {
                appendLog(message, isError)
            }
        }

        // Accept QUIC peer info from intent extras (for dual-transport test)
        val quicNodeId = intent.getStringExtra("quic_node_id")
        val quicAddress = intent.getStringExtra("quic_address")

        lifecycleScope.launch {
            try {
                // --- UniFFI manager tests (pure logic, no BLE) ---
                val uniffiTests = UniFFIManagerTests()
                uniffiTests.setLogCallback(logCb)

                val uniffiResults = withContext(Dispatchers.IO) {
                    uniffiTests.runAll()
                }

                // --- BLE functional tests ---
                val runner = TestRunner(applicationContext, quicNodeId, quicAddress)
                runner.setLogCallback(logCb)

                val bleResults = withContext(Dispatchers.IO) {
                    runner.runAll()
                }

                // --- Combined summary ---
                withContext(Dispatchers.Main) {
                    val uniffiPassed = uniffiResults.count { it.passed }
                    val uniffiTotal = uniffiResults.size
                    val blePassed = bleResults.count { it.passed }
                    val bleTotal = bleResults.size
                    val totalPassed = uniffiPassed + blePassed
                    val totalAll = uniffiTotal + bleTotal
                    val allPassed = totalPassed == totalAll

                    appendLog("", false)
                    if (allPassed) {
                        appendLog("ALL TESTS PASSED ($totalPassed/$totalAll)", false, Color.GREEN)
                    } else {
                        appendLog("TESTS FAILED ($totalPassed/$totalAll passed)", true)
                        uniffiResults.filter { !it.passed }.forEach {
                            appendLog("  FAILED: Phase ${it.phase} - ${it.name}: ${it.detail}", true)
                        }
                        bleResults.filter { !it.passed }.forEach {
                            appendLog("  FAILED: Phase ${it.phase} - ${it.name}: ${it.detail}", true)
                        }
                    }
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) {
                    appendLog("FATAL: ${e.message}", true)
                }
            } finally {
                withContext(Dispatchers.Main) {
                    btnRun.isEnabled = true
                }
            }
        }
    }

    private fun appendLog(message: String, isError: Boolean, color: Int? = null) {
        val displayColor = color ?: if (isError) Color.RED else {
            when {
                message.contains("PASS") -> Color.parseColor("#4CAF50")
                message.contains("FAIL") -> Color.RED
                message.contains("====") -> Color.parseColor("#FFD600")
                else -> Color.WHITE
            }
        }

        val start = logBuilder.length
        logBuilder.append(message)
        logBuilder.append("\n")
        logBuilder.setSpan(
            ForegroundColorSpan(displayColor),
            start,
            logBuilder.length,
            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
        )

        tvLog.text = logBuilder
        scrollView.post { scrollView.fullScroll(ScrollView.FOCUS_DOWN) }
    }
}
