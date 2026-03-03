/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

import android.util.Log

/**
 * Stub PeerEventManager required by peat-ffi JNI_OnLoad.
 *
 * The native library looks up this class during initialization to cache
 * a global reference for peer event callbacks. This stub satisfies that
 * lookup without pulling in the full ATAK plugin dependency.
 */
object PeerEventManager {
    private const val TAG = "PeerEventManager"

    @JvmStatic
    fun notifyPeerConnected(peerId: String) {
        Log.i(TAG, "Peer connected: $peerId")
    }

    @JvmStatic
    fun notifyPeerDisconnected(peerId: String, reason: String) {
        Log.i(TAG, "Peer disconnected: $peerId ($reason)")
    }
}
