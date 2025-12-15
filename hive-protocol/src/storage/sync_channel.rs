//! Persistent sync channels for efficient stream reuse (Issue #438 Phase 2)
//!
//! This module provides persistent bidirectional channels for sync operations,
//! eliminating the need to open a new QUIC stream for each sync batch.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────┐
//! │   SyncChannelManager│
//! │                     │
//! │  ┌───────────────┐  │
//! │  │ channels:     │  │
//! │  │  peer_1 → Ch1 │  │
//! │  │  peer_2 → Ch2 │  │
//! │  │  ...          │  │
//! │  └───────────────┘  │
//! └─────────────────────┘
//!          │
//!          ▼
//! ┌─────────────────────┐
//! │    SyncChannel      │
//! │                     │
//! │  ┌──────────────┐   │
//! │  │ send stream  │   │  ← Persistent, reused for all batches
//! │  ├──────────────┤   │
//! │  │ recv task    │   │  ← Background receiver
//! │  ├──────────────┤   │
//! │  │ state        │   │  ← Connected/Reconnecting/Closed
//! │  └──────────────┘   │
//! └─────────────────────┘
//! ```
//!
//! # Benefits
//!
//! - Eliminates stream-per-batch overhead
//! - Reduces QUIC handshake latency
//! - Enables message multiplexing
//! - Automatic reconnection on failure

#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::{RecvStream, SendStream};
#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use std::time::{Duration, Instant};
#[cfg(feature = "automerge-backend")]
use tokio::sync::Mutex;
#[cfg(feature = "automerge-backend")]
use tokio::task::JoinHandle;

#[cfg(feature = "automerge-backend")]
use crate::network::IrohTransport;
#[cfg(feature = "automerge-backend")]
use crate::storage::automerge_sync::{AutomergeSyncCoordinator, SyncBatch, SyncMessageType};

/// Channel state
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Channel is connected and ready for use
    Connected,
    /// Channel is attempting to reconnect
    Reconnecting,
    /// Channel is closed and cannot be used
    Closed,
}

/// Persistent sync channel to a peer
///
/// Maintains a single bidirectional QUIC stream for all sync operations
/// with automatic reconnection on failure.
#[cfg(feature = "automerge-backend")]
pub struct SyncChannel {
    /// Peer ID this channel connects to
    peer_id: EndpointId,
    /// Transport reference for reconnection
    transport: Arc<IrohTransport>,
    /// Outbound send stream (protected by mutex for single writer)
    send: Arc<Mutex<Option<SendStream>>>,
    /// Inbound receiver task handle
    recv_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Channel state
    state: Arc<RwLock<ChannelState>>,
    /// Reconnection attempts counter
    reconnect_attempts: AtomicU32,
    /// Last successful send time
    last_send: Arc<RwLock<Instant>>,
    /// Total bytes sent through this channel
    bytes_sent: AtomicU64,
    /// Total batches sent through this channel
    batches_sent: AtomicU64,
}

#[cfg(feature = "automerge-backend")]
impl SyncChannel {
    /// Maximum reconnection attempts before giving up
    const MAX_RECONNECT_ATTEMPTS: u32 = 3;
    /// Delay between reconnection attempts
    const RECONNECT_DELAY: Duration = Duration::from_millis(500);

    /// Create a new sync channel to a peer
    ///
    /// Opens a bidirectional stream and spawns a receiver task.
    pub async fn connect(
        transport: Arc<IrohTransport>,
        peer_id: EndpointId,
        coordinator: Arc<AutomergeSyncCoordinator>,
    ) -> Result<Self> {
        // Get connection to peer
        let conn = transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open bidirectional stream
        let (send, recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        let channel = Self {
            peer_id,
            transport,
            send: Arc::new(Mutex::new(Some(send))),
            recv_task: Arc::new(Mutex::new(None)),
            state: Arc::new(RwLock::new(ChannelState::Connected)),
            reconnect_attempts: AtomicU32::new(0),
            last_send: Arc::new(RwLock::new(Instant::now())),
            bytes_sent: AtomicU64::new(0),
            batches_sent: AtomicU64::new(0),
        };

        // Spawn receiver task
        channel.spawn_receiver(recv, coordinator).await;

        tracing::debug!("Sync channel connected to peer {:?}", peer_id);
        Ok(channel)
    }

    /// Spawn the inbound receiver task
    async fn spawn_receiver(&self, recv: RecvStream, coordinator: Arc<AutomergeSyncCoordinator>) {
        let peer_id = self.peer_id;
        let state = Arc::clone(&self.state);

        let task = tokio::spawn(async move {
            tracing::debug!("Sync channel receiver started for peer {:?}", peer_id);

            if let Err(e) = Self::receive_loop(recv, peer_id, coordinator).await {
                tracing::warn!(
                    "Sync channel receiver for peer {:?} ended with error: {}",
                    peer_id,
                    e
                );
            }

            // Mark channel as needing reconnection
            *state.write().unwrap() = ChannelState::Reconnecting;
            tracing::debug!("Sync channel receiver ended for peer {:?}", peer_id);
        });

        *self.recv_task.lock().await = Some(task);
    }

    /// Main receive loop - processes incoming batches
    async fn receive_loop(
        mut recv: RecvStream,
        peer_id: EndpointId,
        coordinator: Arc<AutomergeSyncCoordinator>,
    ) -> Result<()> {
        loop {
            // Read message type marker
            let mut marker = [0u8; 1];
            match recv.read_exact(&mut marker).await {
                Ok(_) => {}
                Err(e) => {
                    // Stream closed or error
                    return Err(anyhow::anyhow!("Stream read error: {}", e));
                }
            }

            // Check if it's a batch message (0x07)
            if marker[0] != SyncMessageType::SyncBatch as u8 {
                tracing::warn!(
                    "Unexpected message type on sync channel: 0x{:02x}",
                    marker[0]
                );
                continue;
            }

            // Read batch length (4 bytes, big-endian)
            let mut len_bytes = [0u8; 4];
            recv.read_exact(&mut len_bytes)
                .await
                .context("Failed to read batch length")?;
            let batch_len = u32::from_be_bytes(len_bytes) as usize;

            // Read batch data
            let mut batch_data = vec![0u8; batch_len];
            recv.read_exact(&mut batch_data)
                .await
                .context("Failed to read batch data")?;

            // Decode and process batch
            match SyncBatch::decode(&batch_data) {
                Ok(batch) => {
                    let total_bytes = 1 + 4 + batch_len; // marker + len + data
                    if let Err(e) = coordinator
                        .receive_batch_message(peer_id, batch, total_bytes)
                        .await
                    {
                        tracing::warn!("Failed to process batch from peer {:?}: {}", peer_id, e);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to decode batch from peer {:?}: {}", peer_id, e);
                }
            }
        }
    }

    /// Send a batch through this channel
    ///
    /// Wire format on persistent channel:
    /// ```text
    /// [1 byte: 0x07 (SyncBatch marker)][4 bytes: batch_len][batch_bytes...]
    /// ```
    pub async fn send(&self, batch: &SyncBatch) -> Result<()> {
        // Check channel state (read and release lock before any await)
        let needs_reconnect = {
            let state = *self.state.read().unwrap();
            match state {
                ChannelState::Closed => return Err(anyhow::anyhow!("Channel is closed")),
                ChannelState::Reconnecting => true,
                ChannelState::Connected => false,
            }
        };

        // Reconnect if needed (outside the lock)
        if needs_reconnect {
            self.reconnect().await?;
        }

        let batch_bytes = batch.encode();
        let mut send_guard = self.send.lock().await;

        let send = send_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No send stream available"))?;

        // Write message type marker
        send.write_all(&[SyncMessageType::SyncBatch as u8])
            .await
            .context("Failed to write batch marker")?;

        // Write batch length
        let batch_len = batch_bytes.len() as u32;
        send.write_all(&batch_len.to_be_bytes())
            .await
            .context("Failed to write batch length")?;

        // Write batch data
        send.write_all(&batch_bytes)
            .await
            .context("Failed to write batch data")?;

        // Update statistics
        let total_bytes = 1 + 4 + batch_bytes.len();
        self.bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);
        self.batches_sent.fetch_add(1, Ordering::Relaxed);
        *self.last_send.write().unwrap() = Instant::now();

        tracing::trace!(
            "Sent batch {} ({} entries, {} bytes) to peer {:?}",
            batch.batch_id,
            batch.len(),
            total_bytes,
            self.peer_id
        );

        Ok(())
    }

    /// Attempt to reconnect the channel
    pub async fn reconnect(&self) -> Result<()> {
        // Update state
        *self.state.write().unwrap() = ChannelState::Reconnecting;

        let attempts = self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
        if attempts >= Self::MAX_RECONNECT_ATTEMPTS {
            *self.state.write().unwrap() = ChannelState::Closed;
            return Err(anyhow::anyhow!(
                "Max reconnection attempts ({}) exceeded",
                Self::MAX_RECONNECT_ATTEMPTS
            ));
        }

        tracing::info!(
            "Attempting reconnection to peer {:?} (attempt {})",
            self.peer_id,
            attempts + 1
        );

        // Wait before reconnecting
        tokio::time::sleep(Self::RECONNECT_DELAY).await;

        // Get connection
        let conn = self
            .transport
            .get_connection(&self.peer_id)
            .context("No connection to peer for reconnection")?;

        // Open new stream
        let (send, _recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream for reconnection")?;

        // Update send stream
        *self.send.lock().await = Some(send);
        *self.state.write().unwrap() = ChannelState::Connected;
        self.reconnect_attempts.store(0, Ordering::Relaxed);

        tracing::info!("Reconnected sync channel to peer {:?}", self.peer_id);
        Ok(())
    }

    /// Check if channel is connected
    pub fn is_connected(&self) -> bool {
        *self.state.read().unwrap() == ChannelState::Connected
    }

    /// Get channel state
    pub fn state(&self) -> ChannelState {
        *self.state.read().unwrap()
    }

    /// Get peer ID
    pub fn peer_id(&self) -> EndpointId {
        self.peer_id
    }

    /// Get total bytes sent
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Get total batches sent
    pub fn batches_sent(&self) -> u64 {
        self.batches_sent.load(Ordering::Relaxed)
    }

    /// Close the channel
    pub async fn close(&self) {
        *self.state.write().unwrap() = ChannelState::Closed;

        // Cancel receiver task
        if let Some(task) = self.recv_task.lock().await.take() {
            task.abort();
        }

        // Close send stream
        if let Some(mut send) = self.send.lock().await.take() {
            let _ = send.finish();
        }

        tracing::debug!("Sync channel to peer {:?} closed", self.peer_id);
    }
}

/// Manager for sync channels to all peers
///
/// Automatically creates and manages persistent channels for each connected peer.
#[cfg(feature = "automerge-backend")]
pub struct SyncChannelManager {
    /// Active channels per peer
    channels: Arc<RwLock<HashMap<EndpointId, Arc<SyncChannel>>>>,
    /// Transport reference
    transport: Arc<IrohTransport>,
    /// Coordinator reference for processing received batches
    coordinator: Arc<AutomergeSyncCoordinator>,
    /// Whether the manager is active
    active: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(feature = "automerge-backend")]
impl SyncChannelManager {
    /// Create a new channel manager
    pub fn new(transport: Arc<IrohTransport>, coordinator: Arc<AutomergeSyncCoordinator>) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            transport,
            coordinator,
            active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    /// Get or create a channel to a peer
    pub async fn get_channel(&self, peer_id: EndpointId) -> Result<Arc<SyncChannel>> {
        // Check if we have an existing connected channel
        {
            let channels = self.channels.read().unwrap();
            if let Some(channel) = channels.get(&peer_id) {
                if channel.is_connected() {
                    return Ok(Arc::clone(channel));
                }
            }
        }

        // Need to create or reconnect
        let channel = SyncChannel::connect(
            Arc::clone(&self.transport),
            peer_id,
            Arc::clone(&self.coordinator),
        )
        .await?;

        let channel = Arc::new(channel);
        self.channels
            .write()
            .unwrap()
            .insert(peer_id, Arc::clone(&channel));

        Ok(channel)
    }

    /// Send batch to a peer through persistent channel
    pub async fn send_to_peer(&self, peer_id: EndpointId, batch: &SyncBatch) -> Result<()> {
        let channel = self.get_channel(peer_id).await?;
        channel.send(batch).await
    }

    /// Broadcast batch to all connected peers
    pub async fn broadcast(&self, batch: &SyncBatch) -> Result<()> {
        let peer_ids = self.transport.connected_peers();

        for peer_id in peer_ids {
            if let Err(e) = self.send_to_peer(peer_id, batch).await {
                tracing::warn!("Failed to send batch to peer {:?}: {}", peer_id, e);
            }
        }

        Ok(())
    }

    /// Remove channel for a peer (e.g., on disconnect)
    pub async fn remove_channel(&self, peer_id: &EndpointId) {
        // Extract channel from map first, then close outside the lock
        let channel = self.channels.write().unwrap().remove(peer_id);
        if let Some(channel) = channel {
            channel.close().await;
        }
    }

    /// Get number of active channels
    pub fn channel_count(&self) -> usize {
        self.channels.read().unwrap().len()
    }

    /// Get statistics for all channels
    pub fn stats(&self) -> ChannelManagerStats {
        let channels = self.channels.read().unwrap();
        let mut total_bytes = 0u64;
        let mut total_batches = 0u64;
        let mut connected = 0usize;

        for channel in channels.values() {
            total_bytes += channel.bytes_sent();
            total_batches += channel.batches_sent();
            if channel.is_connected() {
                connected += 1;
            }
        }

        ChannelManagerStats {
            total_channels: channels.len(),
            connected_channels: connected,
            total_bytes_sent: total_bytes,
            total_batches_sent: total_batches,
        }
    }

    /// Shutdown the manager and close all channels
    pub async fn shutdown(&self) {
        self.active.store(false, Ordering::Relaxed);

        let channels: Vec<Arc<SyncChannel>> = {
            let mut channels = self.channels.write().unwrap();
            channels.drain().map(|(_, c)| c).collect()
        };

        for channel in channels {
            channel.close().await;
        }

        tracing::debug!("SyncChannelManager shutdown complete");
    }
}

/// Statistics for channel manager
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Default)]
pub struct ChannelManagerStats {
    /// Total number of channels (active + reconnecting)
    pub total_channels: usize,
    /// Number of connected channels
    pub connected_channels: usize,
    /// Total bytes sent across all channels
    pub total_bytes_sent: u64,
    /// Total batches sent across all channels
    pub total_batches_sent: u64,
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;

    #[test]
    fn test_channel_state_enum() {
        assert_ne!(ChannelState::Connected, ChannelState::Reconnecting);
        assert_ne!(ChannelState::Reconnecting, ChannelState::Closed);
        assert_eq!(ChannelState::Connected, ChannelState::Connected);
    }

    #[test]
    fn test_channel_manager_stats_default() {
        let stats = ChannelManagerStats::default();
        assert_eq!(stats.total_channels, 0);
        assert_eq!(stats.connected_channels, 0);
        assert_eq!(stats.total_bytes_sent, 0);
        assert_eq!(stats.total_batches_sent, 0);
    }
}
