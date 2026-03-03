/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.peat.test

import android.util.Log
import uniffi.peat_btle.PeerLifetimeConfig
import uniffi.peat_btle.PeerLifetimeManager
import uniffi.peat_btle.ReconnectionConfig
import uniffi.peat_btle.ReconnectionManager
import uniffi.peat_btle.ReconnectionStatus

/**
 * Pure-logic tests for UniFFI-exported ReconnectionManager and PeerLifetimeManager.
 * No BLE hardware required — validates that the native .so loads on ARM64 Android
 * and the generated Kotlin bindings work end-to-end.
 */
class UniFFIManagerTests {

    companion object {
        private const val TAG = "PeatUniFFI"
    }

    data class PhaseResult(
        val phase: String,
        val name: String,
        val passed: Boolean,
        val detail: String
    )

    fun interface LogCallback {
        fun onLog(message: String, isError: Boolean)
    }

    private var logCallback: LogCallback? = null
    private val results = mutableListOf<PhaseResult>()

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

    private fun recordPhase(phase: String, name: String, passed: Boolean, detail: String): Boolean {
        val status = if (passed) "PASS" else "FAIL"
        val result = PhaseResult(phase, name, passed, detail)
        results.add(result)
        log("Phase $phase: $name ${".".repeat(maxOf(1, 28 - name.length))} $status")
        if (detail.isNotEmpty()) log("  $detail")
        return passed
    }

    fun runAll(): List<PhaseResult> {
        results.clear()

        log("================================================")
        log("UniFFI Manager Tests")
        log("================================================")

        phaseU1UniFFILoad()
        phaseU2ExponentialBackoff()
        phaseU3FlatDelayReset()
        phaseU4ConnectionSuccess()
        phaseU5StaleDetection()
        phaseU6ActivityReset()

        val passed = results.count { it.passed }
        val total = results.size
        log("================================================")
        log("UniFFI Tests: $passed/$total PASSED")
        log("================================================")

        return results
    }

    /**
     * U1: Load libpeat_btle.so and verify UniFFI scaffolding initializes.
     * Creating any UniFFI object triggers System.loadLibrary under the hood.
     */
    private fun phaseU1UniFFILoad(): Boolean {
        return try {
            val config = ReconnectionConfig(
                baseDelayMs = 1000UL,
                maxDelayMs = 60000UL,
                maxAttempts = 10U,
                checkIntervalMs = 5000UL,
                useFlatDelay = false,
                resetOnExhaustion = false
            )
            val mgr = ReconnectionManager(config)
            val count = mgr.trackedCount()
            mgr.close()
            recordPhase("U1", "UniFFI Load", count == 0U,
                "libpeat_btle.so loaded, trackedCount=$count")
        } catch (e: Throwable) {
            recordPhase("U1", "UniFFI Load", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * U2: Exponential backoff — verify delays increase and exhaustion after max_attempts.
     *
     * Config: base=100ms, max=1600ms, attempts=5, exponential.
     * Expected delays: ~100, ~200, ~400, ~800, then exhausted.
     */
    private fun phaseU2ExponentialBackoff(): Boolean {
        return try {
            val config = ReconnectionConfig(
                baseDelayMs = 100UL,
                maxDelayMs = 1600UL,
                maxAttempts = 5U,
                checkIntervalMs = 50UL,
                useFlatDelay = false,
                resetOnExhaustion = false
            )
            val mgr = ReconnectionManager(config)
            val addr = "AA:BB:CC:DD:EE:01"

            mgr.trackDisconnection(addr)
            if (!mgr.isTracked(addr)) {
                mgr.close()
                return recordPhase("U2", "Reconnect: Exp Backoff", false,
                    "peer not tracked after trackDisconnection")
            }

            // Record 5 attempts, collecting the delay after each
            val delays = mutableListOf<ULong>()
            for (i in 1..5) {
                mgr.recordAttempt(addr)
                val stats = mgr.getPeerStats(addr)
                if (stats != null) {
                    delays.add(stats.nextAttemptDelayMs)
                }
            }

            val status = mgr.getStatus(addr)
            val exhausted = status is ReconnectionStatus.Exhausted

            // Verify delays are non-decreasing (exponential backoff)
            val backoffDelays = delays.dropLast(1) // last may be sentinel
            val increasing = backoffDelays.size < 2 ||
                backoffDelays.zipWithNext().all { (a, b) -> b >= a }

            mgr.close()
            recordPhase("U2", "Reconnect: Exp Backoff", exhausted && increasing,
                "delays=${delays.map { it.toLong() }}ms, exhausted=$exhausted")
        } catch (e: Throwable) {
            recordPhase("U2", "Reconnect: Exp Backoff", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * U3: Flat delay + auto-reset on exhaustion.
     *
     * Config: base=100ms, flat delay, max_attempts=3, resetOnExhaustion=true.
     * All delays should be constant (100ms). After exhaustion, status resets to Ready.
     */
    private fun phaseU3FlatDelayReset(): Boolean {
        return try {
            val config = ReconnectionConfig(
                baseDelayMs = 100UL,
                maxDelayMs = 1600UL,
                maxAttempts = 3U,
                checkIntervalMs = 50UL,
                useFlatDelay = true,
                resetOnExhaustion = true
            )
            val mgr = ReconnectionManager(config)
            val addr = "AA:BB:CC:DD:EE:02"

            mgr.trackDisconnection(addr)

            // Record 3 attempts (all should use flat 100ms delay)
            val delays = mutableListOf<ULong>()
            for (i in 1..3) {
                mgr.recordAttempt(addr)
                val stats = mgr.getPeerStats(addr)
                if (stats != null) {
                    delays.add(stats.nextAttemptDelayMs)
                }
            }

            // With resetOnExhaustion, after exhausting attempts the peer should
            // auto-reset. Check that it's tracked and back to a usable state.
            val statusAfter = mgr.getStatus(addr)
            val isReady = statusAfter is ReconnectionStatus.Ready ||
                statusAfter is ReconnectionStatus.Waiting

            // Verify flat: all non-sentinel delays should be equal
            val nonSentinel = delays.filter { it in 1UL..10000UL }
            val allFlat = nonSentinel.size < 2 ||
                nonSentinel.all { it == nonSentinel.first() }

            mgr.close()
            recordPhase("U3", "Reconnect: Flat + Reset", isReady && allFlat,
                "delays=${delays.map { it.toLong() }}ms, flat=$allFlat, statusAfter=$statusAfter")
        } catch (e: Throwable) {
            recordPhase("U3", "Reconnect: Flat + Reset", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * U4: Connection success removes peer from tracking.
     */
    private fun phaseU4ConnectionSuccess(): Boolean {
        return try {
            val config = ReconnectionConfig(
                baseDelayMs = 100UL,
                maxDelayMs = 1600UL,
                maxAttempts = 5U,
                checkIntervalMs = 50UL,
                useFlatDelay = false,
                resetOnExhaustion = false
            )
            val mgr = ReconnectionManager(config)
            val addr = "AA:BB:CC:DD:EE:03"

            mgr.trackDisconnection(addr)
            mgr.recordAttempt(addr)

            if (!mgr.isTracked(addr)) {
                mgr.close()
                return recordPhase("U4", "Reconnect: Success", false,
                    "peer lost before onConnectionSuccess")
            }

            mgr.onConnectionSuccess(addr)
            val tracked = mgr.isTracked(addr)
            val status = mgr.getStatus(addr)

            mgr.close()
            recordPhase("U4", "Reconnect: Success", !tracked,
                "tracked=$tracked, statusAfter=$status")
        } catch (e: Throwable) {
            recordPhase("U4", "Reconnect: Success", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * U5: PeerLifetime stale detection.
     *
     * Config: disconnectedTimeout=2000ms, connectedTimeout=4000ms.
     * Register 2 peers — one disconnected, one connected.
     * After 2.5s: disconnected peer should be stale, connected should not.
     * After 4.5s total: both should be stale.
     */
    private fun phaseU5StaleDetection(): Boolean {
        return try {
            val config = PeerLifetimeConfig(
                disconnectedTimeoutMs = 2000UL,
                connectedTimeoutMs = 4000UL,
                cleanupIntervalMs = 500UL
            )
            val mgr = PeerLifetimeManager(config)

            val addrDisconn = "11:22:33:44:55:01"
            val addrConn = "11:22:33:44:55:02"

            mgr.onPeerActivity(addrDisconn, false)
            mgr.onPeerActivity(addrConn, true)

            if (mgr.trackedCount() != 2U) {
                mgr.close()
                return recordPhase("U5", "Lifetime: Stale Detect", false,
                    "expected 2 tracked, got ${mgr.trackedCount()}")
            }

            // Wait 2.5s — disconnected timeout (2s) should expire
            Thread.sleep(2500)

            val staleAfter2s = mgr.getStalePeerAddresses()
            val disconnStale = addrDisconn in staleAfter2s
            val connNotStale = addrConn !in staleAfter2s

            // Wait 2 more seconds (4.5s total) — connected timeout (4s) should expire
            Thread.sleep(2000)

            val staleAfter4s = mgr.getStalePeerAddresses()
            val bothStale = addrDisconn in staleAfter4s && addrConn in staleAfter4s

            mgr.close()
            val passed = disconnStale && connNotStale && bothStale
            recordPhase("U5", "Lifetime: Stale Detect", passed,
                "at 2.5s: disconn_stale=$disconnStale, conn_not_stale=$connNotStale; " +
                "at 4.5s: both_stale=$bothStale")
        } catch (e: Throwable) {
            recordPhase("U5", "Lifetime: Stale Detect", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }

    /**
     * U6: Activity reset — onPeerActivity resets the staleness timer.
     *
     * Config: disconnectedTimeout=2000ms.
     * Register peer, sleep 1.5s, call onPeerActivity again, sleep 1s → NOT stale.
     * Sleep 1.5s more (3s since last activity refresh, 2s+ since last activity) → stale.
     */
    private fun phaseU6ActivityReset(): Boolean {
        return try {
            val config = PeerLifetimeConfig(
                disconnectedTimeoutMs = 2000UL,
                connectedTimeoutMs = 10000UL,
                cleanupIntervalMs = 500UL
            )
            val mgr = PeerLifetimeManager(config)
            val addr = "11:22:33:44:55:03"

            mgr.onPeerActivity(addr, false)

            // Wait 1.5s (under the 2s timeout)
            Thread.sleep(1500)
            val staleBeforeRefresh = mgr.getStalePeerAddresses()
            val notStaleYet = addr !in staleBeforeRefresh

            // Refresh activity — resets the timer
            mgr.onPeerActivity(addr, false)

            // Wait 1s (1s since refresh, under 2s timeout)
            Thread.sleep(1000)
            val staleAfterRefresh = mgr.getStalePeerAddresses()
            val stillNotStale = addr !in staleAfterRefresh

            // Wait 1.5s more (2.5s since last refresh — should be stale now)
            Thread.sleep(1500)
            val staleFinal = mgr.getStalePeerAddresses()
            val nowStale = addr in staleFinal

            mgr.close()
            val passed = notStaleYet && stillNotStale && nowStale
            recordPhase("U6", "Lifetime: Activity Reset", passed,
                "at 1.5s=$notStaleYet, after_refresh+1s=$stillNotStale, after_refresh+2.5s=$nowStale")
        } catch (e: Throwable) {
            recordPhase("U6", "Lifetime: Activity Reset", false,
                "${e.javaClass.simpleName}: ${e.message}")
        }
    }
}
