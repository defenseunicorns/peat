/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

/**
 * Listener interface for peer connection events.
 *
 * Implement this interface to receive notifications when peers
 * connect or disconnect from the PEAT network.
 */
interface PeerEventListener {
    /**
     * Called when a peer connects to the network.
     *
     * @param peerId The hex-encoded node ID of the connected peer
     */
    fun onPeerConnected(peerId: String)

    /**
     * Called when a peer disconnects from the network.
     *
     * @param peerId The hex-encoded node ID of the disconnected peer
     * @param reason The reason for disconnection
     */
    fun onPeerDisconnected(peerId: String, reason: String)
}
