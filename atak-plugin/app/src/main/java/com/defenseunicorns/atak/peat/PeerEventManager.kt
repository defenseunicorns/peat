/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

import android.util.Log
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Manages peer event listeners and dispatches events from the FFI layer.
 *
 * This is a singleton that bridges the JNI peer event callbacks to
 * Kotlin listeners. The FFI layer calls the static notify methods
 * which dispatch to all registered listeners.
 */
object PeerEventManager {
    private const val TAG = "PeerEventManager"

    private val listeners = CopyOnWriteArrayList<PeerEventListener>()

    /**
     * Register a listener for peer events.
     */
    fun addListener(listener: PeerEventListener) {
        listeners.add(listener)
        Log.d(TAG, "Added listener, total: ${listeners.size}")
    }

    /**
     * Unregister a listener.
     */
    fun removeListener(listener: PeerEventListener) {
        listeners.remove(listener)
        Log.d(TAG, "Removed listener, total: ${listeners.size}")
    }

    /**
     * Called from JNI when a peer connects.
     * This method is invoked by native code via JNI.
     */
    @JvmStatic
    fun notifyPeerConnected(peerId: String) {
        Log.i(TAG, "Peer connected event: $peerId")
        listeners.forEach { listener ->
            try {
                listener.onPeerConnected(peerId)
            } catch (e: Exception) {
                Log.e(TAG, "Error notifying listener of peer connect: ${e.message}", e)
            }
        }
    }

    /**
     * Called from JNI when a peer disconnects.
     * This method is invoked by native code via JNI.
     */
    @JvmStatic
    fun notifyPeerDisconnected(peerId: String, reason: String) {
        Log.i(TAG, "Peer disconnected event: $peerId ($reason)")
        listeners.forEach { listener ->
            try {
                listener.onPeerDisconnected(peerId, reason)
            } catch (e: Exception) {
                Log.e(TAG, "Error notifying listener of peer disconnect: ${e.message}", e)
            }
        }
    }
}
