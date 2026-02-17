/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.hive.test

import android.content.Context
import android.util.Base64
import android.util.Log
import com.revolveteam.atak.hive.HiveJni
import org.json.JSONArray
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter

/**
 * Test orchestration for Pi-to-Android BLE functional test.
 *
 * When quicNodeId/quicAddress are provided, runs 11-phase dual-transport test
 * (BLE via rpi-ci + QUIC via rpi-ci2). Otherwise falls back to BLE-only 7-phase test.
 */
class TestRunner(
    private val context: Context,
    private val quicNodeId: String? = null,
    private val quicAddress: String? = null
) {

    companion object {
        private const val TAG = "HiveTest"
        // Well-known test key matching ble_responder's TEST_SECRET
        private val TEST_KEY = byteArrayOf(
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20
        )
    }

    private val isDualTransport: Boolean
        get() = !quicNodeId.isNullOrEmpty()

    data class PhaseResult(
        val phase: Int,
        val name: String,
        val passed: Boolean,
        val detail: String
    )

    fun interface LogCallback {
        fun onLog(message: String, isError: Boolean)
    }

    private var logCallback: LogCallback? = null
    private val results = mutableListOf<PhaseResult>()
    private var nodeHandle: Long = 0L
    private var bleClient: BleGattClient? = null
    private var discoveredDevice: BleGattClient.DiscoveredDevice? = null
    private var quicPlatformReceived = false

    fun setLogCallback(callback: LogCallback) {
        logCallback = callback
    }

    private fun log(msg: String) {
        Log.i(TAG, msg)
        logCallback?.onLog(msg, false)
    }

    private fun logError(msg: String) {
        Log.e(TAG, msg)
        logCallback?.onLog(msg, true)
    }

    private fun recordPhase(phase: Int, name: String, passed: Boolean, detail: String): Boolean {
        val status = if (passed) "PASS" else "FAIL"
        val result = PhaseResult(phase, name, passed, detail)
        results.add(result)
        log("Phase $phase: $name ${".".repeat(maxOf(1, 28 - name.length))} $status")
        if (detail.isNotEmpty()) log("  $detail")
        return passed
    }

    suspend fun runAll(): List<PhaseResult> {
        results.clear()
        val now = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm"))
        val buildInfo = "v${BuildConfig.VERSION_NAME} (${BuildConfig.GIT_BRANCH}@${BuildConfig.GIT_COMMIT})"

        log("================================================")
        if (isDualTransport) {
            log("HIVE Dual-Transport Test (BLE + QUIC)")
            log("  Run: $now  Build: $buildInfo")
            log("  Expected QUIC peer: ${quicNodeId?.take(16)}...")
            log("  Discovery: BLE advertisements + mDNS")
        } else {
            log("HIVE Pi-to-Android BLE Functional Test")
            log("  Run: $now  Build: $buildInfo")
        }
        log("================================================")

        try {
            if (!phase1JniInit()) return results
            if (!phase2CreateDualNode()) return results
            if (!phase3VerifyIroh()) return results
            if (!phase4BleDiscovery()) return results
            if (!phase5GattSync()) return results

            if (isDualTransport) {
                // Publish platform first so it's in local store before sync
                if (!phase6PublishPlatform()) return results
                // Connect to QUIC peer (sync will exchange data both ways)
                if (!phase7PeerDiscovery()) return results
                // Verify we received the Pi's platform
                if (!phase8QuicDataReceived()) return results
                // BLE state + verification (9-10)
                if (!phase9SignalBleState()) return results
                if (!phase10DualTransportVerified()) return results
                // Hold connection so remote peer can sync our data
                phase11HoldForSync()
            } else {
                // BLE-only flow (original phases 6-7)
                if (!phase6SignalBleState()) return results
                phase7VerifyDualTransport()
            }
        } catch (e: Throwable) {
            logError("Unexpected error: ${e.javaClass.simpleName}: ${e.message}")
        } finally {
            cleanupPhase()
        }

        // Summary
        val passed = results.count { it.passed }
        val total = results.size
        log("================================================")
        log("RESULT: $passed/$total PASSED")
        log("================================================")

        return results
    }

    // Phase 1: Load native library and init JNI
    private fun phase1JniInit(): Boolean {
        return try {
            HiveNativeLoader.init(context)
            HiveNativeLoader.loadLibrary("hive_ffi")
            val jniOk = HiveJni.initNatives()
            if (!jniOk) {
                return recordPhase(1, "JNI Init", false, "initNatives() returned false")
            }

            val version = HiveJni.hiveVersion()
            if (version.isEmpty()) {
                return recordPhase(1, "JNI Init", false, "hiveVersion() returned empty")
            }

            recordPhase(1, "JNI Init", true, "version=$version")
        } catch (e: Throwable) {
            recordPhase(1, "JNI Init", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 2: Create node with BLE transport enabled
    private fun phase2CreateDualNode(): Boolean {
        return try {
            val sharedKey = Base64.encodeToString(TEST_KEY, Base64.NO_WRAP)
            val storagePath = context.filesDir.absolutePath

            nodeHandle = HiveJni.createNodeWithConfigJni(
                "FUNCTEST",
                sharedKey,
                storagePath,
                true,  // enableBle
                "balanced"
            )

            if (nodeHandle == 0L) {
                return recordPhase(2, "Dual Node Created", false, "handle=0")
            }

            val syncOk = HiveJni.startSyncJni(nodeHandle)
            val nodeId = HiveJni.nodeIdJni(nodeHandle)

            if (nodeId.isEmpty()) {
                return recordPhase(2, "Dual Node Created", false, "nodeId empty")
            }

            recordPhase(2, "Dual Node Created", true,
                "Iroh + BLE, node=${nodeId.take(16)}..., sync=$syncOk")
        } catch (e: Throwable) {
            recordPhase(2, "Dual Node Created", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 3: Verify Iroh transport is active
    private fun phase3VerifyIroh(): Boolean {
        return try {
            val nodeId = HiveJni.nodeIdJni(nodeHandle)
            val isValid = nodeId.isNotEmpty() && nodeHandle != 0L

            recordPhase(3, "Iroh Active", isValid,
                "node: ${nodeId.take(16)}...")
        } catch (e: Throwable) {
            recordPhase(3, "Iroh Active", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 4: BLE scan for Pi responder
    private suspend fun phase4BleDiscovery(): Boolean {
        return try {
            val client = BleGattClient(context)
            bleClient = client

            val device = client.scan(meshId = "FUNCTEST", timeoutMs = 15_000)
            discoveredDevice = device

            recordPhase(4, "BLE Discovery", true,
                "${device.name}, ${device.rssi} dBm")
        } catch (e: Throwable) {
            recordPhase(4, "BLE Discovery", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 5: GATT connect + sync exchange
    private var peerNodeIdHex: String = ""
    private suspend fun phase5GattSync(): Boolean {
        return try {
            val client = bleClient
                ?: return recordPhase(5, "GATT Sync", false, "No BLE client")

            val device = discoveredDevice
                ?: return recordPhase(5, "GATT Sync", false, "No discovered device from phase 4")

            // Connect and discover services
            val (gatt, service) = client.connectAndDiscover(device.device)

            // Build a minimal sync document (callsign + counter)
            // Format matches HiveMesh::build_document() wire format
            val syncPayload = buildMinimalSyncPayload("ANDROID-TEST")

            // Perform full sync exchange
            val result = client.performSync(gatt, service, syncPayload)

            peerNodeIdHex = result.nodeInfo.nodeIdHex

            val passed = result.bytesRead > 0
            recordPhase(5, "BLE GATT Sync", passed,
                "${result.bytesWritten}B sent, ${result.bytesRead}B recv, " +
                "${result.latencyMs}ms, peer=0x${result.nodeInfo.nodeIdHex}")
        } catch (e: Throwable) {
            recordPhase(5, "BLE GATT Sync", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // ========================================================================
    // QUIC Phases (dual-transport mode only, phases 6-8)
    // ========================================================================

    // Phase 7: Discover QUIC peer — try mDNS first, fall back to direct connect, keep polling
    private fun phase7PeerDiscovery(): Boolean {
        return try {
            val expectedNodeId = quicNodeId

            var found = false
            var foundPeerId = ""
            var peers = 0
            var method = "mDNS"

            // Phase 1: mDNS discovery (15s) — Pi's deferred mDNS takes a few seconds
            log("  Trying mDNS discovery (15s)...")
            for (i in 1..30) {
                Thread.sleep(500)
                peers = HiveJni.peerCountJni(nodeHandle)
                if (peers > 0) {
                    found = checkForExpectedPeer(expectedNodeId)
                    if (found) { foundPeerId = expectedNodeId ?: "unknown"; break }
                }
                if (i % 10 == 0) {
                    log("  mDNS polling... ${i / 2}s, peers=$peers")
                }
            }

            // Phase 2: Try direct connect (non-blocking) then continue polling
            if (!found && !quicAddress.isNullOrEmpty() && !expectedNodeId.isNullOrEmpty()) {
                log("  mDNS not available, trying direct connect to $quicAddress...")
                method = "direct"
                // Fire and forget — connectPeer may fail but still trigger discovery
                try {
                    HiveJni.connectPeerJni(nodeHandle, expectedNodeId, quicAddress)
                } catch (_: Throwable) {}

                // Keep polling — retry direct connect every 5s if still disconnected
                log("  Polling for peer connection (25s)...")
                for (i in 1..50) {
                    Thread.sleep(500)
                    peers = HiveJni.peerCountJni(nodeHandle)
                    if (peers > 0) {
                        found = checkForExpectedPeer(expectedNodeId)
                        if (found) { foundPeerId = expectedNodeId; break }
                    }
                    if (i % 10 == 0) {
                        log("  Waiting for connection... ${i / 2}s, peers=$peers")
                        // Retry direct connect if still no peers
                        if (peers == 0) {
                            log("  Retrying direct connect to $quicAddress...")
                            try {
                                HiveJni.connectPeerJni(nodeHandle, expectedNodeId, quicAddress)
                            } catch (_: Throwable) {}
                        }
                    }
                }
            }

            recordPhase(7, "QUIC Peer Connect", found,
                if (found) "peer=${foundPeerId.take(16)}..., method=$method, iroh_peers=$peers"
                else "QUIC peer not reachable (peers=$peers)")
        } catch (e: Throwable) {
            recordPhase(7, "QUIC Peer Connect", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    private fun checkForExpectedPeer(expectedNodeId: String?): Boolean {
        val peersJson = HiveJni.connectedPeersJni(nodeHandle)
        return try {
            val arr = JSONArray(peersJson)
            for (j in 0 until arr.length()) {
                val peerId = arr.getString(j)
                if (expectedNodeId != null && peerId == expectedNodeId) return true
                if (expectedNodeId == null && peerId.isNotEmpty()) return true
            }
            false
        } catch (_: Throwable) { false }
    }

    // Phase 6: Publish our platform via QUIC (before connecting, so it's in local store for sync)
    private fun phase6PublishPlatform(): Boolean {
        return try {
            val platformJson = """
                {
                    "id": "android-dual-test",
                    "name": "ANDROID-DUAL",
                    "platform_type": "HANDHELD",
                    "lat": 33.749,
                    "lon": -84.388,
                    "hae": 0.0,
                    "status": "active",
                    "capabilities": ["PLI"],
                    "readiness": 1.0
                }
            """.trimIndent()

            val ok = HiveJni.publishPlatformJni(nodeHandle, platformJson)
            recordPhase(6, "Publish Platform", ok,
                if (ok) "android-dual-test published (pre-connect)" else "publishPlatformJni returned false")
        } catch (e: Throwable) {
            recordPhase(6, "Publish Platform", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 8: Poll for PI-QUIC platform from rpi-ci2 (up to 30s)
    private fun phase8QuicDataReceived(): Boolean {
        return try {
            var found = false
            var platformName = ""
            for (i in 1..60) {
                Thread.sleep(500)
                val json = HiveJni.getPlatformsJni(nodeHandle)
                try {
                    val arr = JSONArray(json)
                    for (j in 0 until arr.length()) {
                        val p = arr.getJSONObject(j)
                        val name = p.optString("name", "")
                        if (name == "PI-QUIC" || p.optString("id", "") == "pi-quic-test") {
                            found = true
                            platformName = name
                            break
                        }
                    }
                } catch (_: Throwable) {
                    // JSON parse error, keep polling
                }
                if (found) break
            }

            quicPlatformReceived = found
            recordPhase(8, "QUIC Data Received", found,
                if (found) "platform \"$platformName\" via QUIC/mDNS"
                else "PI-QUIC platform not received within 30s")
        } catch (e: Throwable) {
            recordPhase(8, "QUIC Data Received", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 9: Signal BLE state (dual-transport mode)
    private fun phase9SignalBleState(): Boolean {
        return try {
            HiveJni.bleSetStartedJni(nodeHandle, true)

            if (peerNodeIdHex.isNotEmpty()) {
                HiveJni.bleAddPeerJni(nodeHandle, peerNodeIdHex)
            }

            val available = HiveJni.bleIsAvailableJni(nodeHandle)
            val peerCount = HiveJni.blePeerCountJni(nodeHandle)

            val passed = available && (peerNodeIdHex.isEmpty() || peerCount >= 1)
            recordPhase(9, "BLE State Signaled", passed,
                "available=$available, peers=$peerCount")
        } catch (e: Throwable) {
            recordPhase(9, "BLE State Signaled", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 10: Verify both transports carried data
    private fun phase10DualTransportVerified(): Boolean {
        return try {
            val irohPeers = HiveJni.peerCountJni(nodeHandle)
            val blePeers = HiveJni.blePeerCountJni(nodeHandle)
            val bleAvailable = HiveJni.bleIsAvailableJni(nodeHandle)

            val passed = irohPeers >= 1 && blePeers >= 1 && quicPlatformReceived && bleAvailable
            recordPhase(10, "Dual Transport Verified", passed,
                "iroh=$irohPeers, ble=$blePeers, quic_data=${if (quicPlatformReceived) "OK" else "MISSING"}")
        } catch (e: Throwable) {
            recordPhase(10, "Dual Transport Verified", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // ========================================================================
    // BLE-only Phases (original 7-phase flow, phases 6-7)
    // ========================================================================

    // Phase 6 (BLE-only): Signal BLE state to Rust TransportManager
    private fun phase6SignalBleState(): Boolean {
        return try {
            HiveJni.bleSetStartedJni(nodeHandle, true)

            if (peerNodeIdHex.isNotEmpty()) {
                HiveJni.bleAddPeerJni(nodeHandle, peerNodeIdHex)
            }

            val available = HiveJni.bleIsAvailableJni(nodeHandle)
            val peerCount = HiveJni.blePeerCountJni(nodeHandle)

            val passed = available && (peerNodeIdHex.isEmpty() || peerCount >= 1)
            recordPhase(6, "BLE State Signaled", passed,
                "available=$available, peers=$peerCount")
        } catch (e: Throwable) {
            recordPhase(6, "BLE State Signaled", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 7 (BLE-only): Verify both transports active
    private fun phase7VerifyDualTransport(): Boolean {
        return try {
            val nodeId = HiveJni.nodeIdJni(nodeHandle)
            val irohActive = nodeId.isNotEmpty() && nodeHandle != 0L
            val bleAvailable = HiveJni.bleIsAvailableJni(nodeHandle)
            val blePeers = HiveJni.blePeerCountJni(nodeHandle)
            val totalPeers = HiveJni.peerCountJni(nodeHandle)

            val passed = irohActive && bleAvailable && blePeers >= 1
            recordPhase(7, "Dual Transport", passed,
                "iroh=${if (irohActive) "active" else "inactive"}, " +
                "ble=$blePeers peer(s), total=$totalPeers")
        } catch (e: Throwable) {
            recordPhase(7, "Dual Transport", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Phase 11: Hold connection open so the remote peer can sync our published data
    private fun phase11HoldForSync(): Boolean {
        return try {
            log("  Holding connection for remote peer sync (15s)...")
            // Stay connected so the Pi can receive our ANDROID-DUAL platform
            for (i in 1..15) {
                Thread.sleep(1000)
                if (i % 5 == 0) {
                    val peers = HiveJni.peerCountJni(nodeHandle)
                    log("  Sync hold... ${i}s, peers=$peers")
                }
            }
            recordPhase(11, "Sync Hold", true, "held connection 15s for remote sync")
        } catch (e: Throwable) {
            recordPhase(11, "Sync Hold", false, "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    // Cleanup (runs after all phases)
    private fun cleanupPhase() {
        val phaseNum = if (isDualTransport) 12 else 8
        log("Phase $phaseNum: Cleanup")
        try {
            bleClient?.disconnect()
            bleClient = null
        } catch (e: Throwable) {
            logError("  BLE disconnect error: ${e.javaClass.simpleName}: ${e.message}")
        }
        try {
            if (nodeHandle != 0L) {
                try { HiveJni.bleSetStartedJni(nodeHandle, false) } catch (_: Throwable) {}
                HiveJni.freeNodeJni(nodeHandle)
                nodeHandle = 0L
            }
        } catch (e: Throwable) {
            logError("  Node cleanup error: ${e.javaClass.simpleName}: ${e.message}")
        }
        log("  Cleanup complete")
    }

    /**
     * Build a minimal sync payload that the Pi ble_responder can parse.
     *
     * This produces a simplified handshake document — not the full CRDT wire
     * format (which requires the Rust side), but enough for the responder's
     * on_ble_data_received_anonymous to process.
     *
     * Wire format (unencrypted):
     *   [0..4]  node_id (BE u32) — our synthetic node ID
     *   [4]     counter (u8)
     *   [5]     flags (u8) — 0x00 = normal
     *   [6..]   callsign (UTF-8, null-terminated)
     */
    private fun buildMinimalSyncPayload(callsign: String): ByteArray {
        val nodeIdBytes = byteArrayOf(0x41, 0x4E, 0x44, 0x52) // "ANDR" as node ID
        val counter: Byte = 1
        val flags: Byte = 0
        val callsignBytes = callsign.toByteArray(Charsets.UTF_8)

        return nodeIdBytes + counter + flags + callsignBytes + 0x00.toByte()
    }
}
