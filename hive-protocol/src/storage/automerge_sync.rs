//! Automerge sync protocol implementation
//!
//! This module provides the sync coordinator that manages Automerge document
//! synchronization over Iroh QUIC streams.
//!
//! # Phase 4 Implementation
//!
//! Implements the Automerge sync protocol (https://arxiv.org/abs/2012.00472) over
//! Iroh P2P connections to enable CRDT document synchronization.
//!
//! ## Sync Flow
//!
//! ```text
//! Node A                          Node B
//!   │                               │
//!   ├─ Document updated             │
//!   ├─ generate_sync_message() ────→│
//!   │                               ├─ receive_sync_message()
//!   │                               ├─ apply changes
//!   │                               ├─ generate_sync_message()
//!   │←────────────────────────────┤
//!   ├─ receive_sync_message()       │
//!   ├─ apply changes                │
//!   │                               │
//!   ├─ Synced! ✅                   ├─ Synced! ✅
//! ```
//!
//! ## Wire Format
//!
//! Sync messages are sent over Iroh bidirectional streams with length prefixing:
//! ```text
//! [4 bytes: message length (u32, big-endian)][N bytes: serialized sync::Message]
//! ```

#[cfg(feature = "automerge-backend")]
use super::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use super::flow_control::{FlowControlConfig, FlowControlStats, FlowController};
#[cfg(feature = "automerge-backend")]
use super::partition_detection::PartitionDetector;
#[cfg(feature = "automerge-backend")]
use super::sync_errors::{SyncError, SyncErrorHandler};
#[cfg(feature = "automerge-backend")]
use crate::network::iroh_transport::IrohTransport;
#[cfg(feature = "automerge-backend")]
use crate::qos::{SyncMode, SyncModeRegistry};
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use automerge::sync::{Message as SyncMessage, State as SyncState, SyncDoc};
#[cfg(feature = "automerge-backend")]
use automerge::Automerge;
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::Connection;
#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use std::time::SystemTime;
#[cfg(feature = "automerge-backend")]
#[allow(unused_imports)] // Used in sync message send/receive methods
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Wire format message type prefix (Issue #355)
///
/// Used to distinguish between delta-based sync messages and state snapshots.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncMessageType {
    /// Delta-based sync message (Automerge sync protocol)
    DeltaSync = 0x00,
    /// Full state snapshot (doc.save() bytes)
    StateSnapshot = 0x01,
}

/// Received sync payload (Issue #355)
///
/// Can be either a delta-based sync message or a state snapshot.
#[cfg(feature = "automerge-backend")]
#[derive(Debug)]
pub enum ReceivedSyncPayload {
    /// Delta-based sync message from Automerge protocol
    Delta(SyncMessage),
    /// Full document state snapshot (from LatestOnly mode)
    StateSnapshot(Vec<u8>),
}

/// Per-peer sync statistics
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Default)]
pub struct PeerSyncStats {
    /// Total bytes sent to this peer
    pub bytes_sent: u64,
    /// Total bytes received from this peer
    pub bytes_received: u64,
    /// Number of successful syncs
    pub sync_count: u64,
    /// Last successful sync timestamp
    pub last_sync: Option<SystemTime>,
    /// Number of sync failures
    pub failure_count: u64,
}

/// Coordinator for Automerge document synchronization over Iroh
///
/// Manages sync state for each peer and coordinates message exchange.
///
/// # Phase 4-5 Enhancements
///
/// - ✅ Per-peer sync state management
/// - ✅ Sync statistics tracking (bytes, counts, timestamps)
/// - ✅ Error handling with retry logic and circuit breaker (Phase 5)
/// - ✅ Partition detection with heartbeat mechanism (Phase 6.3)
/// - ✅ Flow control and backpressure (Issue #97)
/// - ✅ Sync modes: LatestOnly vs FullHistory (Issue #355)
#[cfg(feature = "automerge-backend")]
pub struct AutomergeSyncCoordinator {
    /// Reference to the AutomergeStore
    store: Arc<AutomergeStore>,
    /// Reference to the IrohTransport
    transport: Arc<IrohTransport>,
    /// Sync state for each peer (per document)
    /// Map: document_key -> peer_id -> SyncState
    peer_states: Arc<RwLock<HashMap<String, HashMap<EndpointId, SyncState>>>>,
    /// Per-peer sync statistics
    /// Map: peer_id -> PeerSyncStats
    peer_stats: Arc<RwLock<HashMap<EndpointId, PeerSyncStats>>>,
    /// Total bytes sent (across all peers)
    total_bytes_sent: Arc<AtomicU64>,
    /// Total bytes received (across all peers)
    total_bytes_received: Arc<AtomicU64>,
    /// Error handler with retry logic and circuit breaker
    error_handler: Arc<SyncErrorHandler>,
    /// Partition detector for heartbeat tracking
    partition_detector: Arc<PartitionDetector>,
    /// Flow controller for rate limiting and backpressure
    flow_controller: Arc<FlowController>,
    /// Sync mode registry for per-collection sync mode configuration (Issue #355)
    sync_mode_registry: Arc<SyncModeRegistry>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeSyncCoordinator {
    /// Create a new sync coordinator
    ///
    /// # Arguments
    ///
    /// * `store` - The AutomergeStore managing documents
    /// * `transport` - The IrohTransport for P2P connections
    pub fn new(store: Arc<AutomergeStore>, transport: Arc<IrohTransport>) -> Self {
        Self::with_flow_control(store, transport, FlowControlConfig::default())
    }

    /// Create a new sync coordinator with custom flow control configuration
    ///
    /// # Arguments
    ///
    /// * `store` - The AutomergeStore managing documents
    /// * `transport` - The IrohTransport for P2P connections
    /// * `flow_config` - Custom flow control configuration
    pub fn with_flow_control(
        store: Arc<AutomergeStore>,
        transport: Arc<IrohTransport>,
        flow_config: FlowControlConfig,
    ) -> Self {
        Self {
            store,
            transport,
            peer_states: Arc::new(RwLock::new(HashMap::new())),
            peer_stats: Arc::new(RwLock::new(HashMap::new())),
            total_bytes_sent: Arc::new(AtomicU64::new(0)),
            total_bytes_received: Arc::new(AtomicU64::new(0)),
            error_handler: Arc::new(SyncErrorHandler::new()),
            partition_detector: Arc::new(PartitionDetector::new()),
            flow_controller: Arc::new(FlowController::with_config(flow_config)),
            sync_mode_registry: Arc::new(SyncModeRegistry::with_defaults()),
        }
    }

    /// Create a new sync coordinator with custom sync mode registry
    ///
    /// # Arguments
    ///
    /// * `store` - The AutomergeStore managing documents
    /// * `transport` - The IrohTransport for P2P connections
    /// * `sync_mode_registry` - Custom sync mode configuration
    pub fn with_sync_modes(
        store: Arc<AutomergeStore>,
        transport: Arc<IrohTransport>,
        sync_mode_registry: Arc<SyncModeRegistry>,
    ) -> Self {
        Self {
            store,
            transport,
            peer_states: Arc::new(RwLock::new(HashMap::new())),
            peer_stats: Arc::new(RwLock::new(HashMap::new())),
            total_bytes_sent: Arc::new(AtomicU64::new(0)),
            total_bytes_received: Arc::new(AtomicU64::new(0)),
            error_handler: Arc::new(SyncErrorHandler::new()),
            partition_detector: Arc::new(PartitionDetector::new()),
            flow_controller: Arc::new(FlowController::with_config(FlowControlConfig::default())),
            sync_mode_registry,
        }
    }

    /// Get the sync mode registry for runtime configuration
    pub fn sync_mode_registry(&self) -> &Arc<SyncModeRegistry> {
        &self.sync_mode_registry
    }

    /// Extract collection name from document key
    ///
    /// Document keys are formatted as "collection:doc_id" (e.g., "beacons:beacon-1")
    fn collection_from_doc_key(doc_key: &str) -> &str {
        doc_key.split(':').next().unwrap_or(doc_key)
    }

    /// Get sync mode for a document key
    fn sync_mode_for_doc(&self, doc_key: &str) -> SyncMode {
        let collection = Self::collection_from_doc_key(doc_key);
        self.sync_mode_registry.get(collection)
    }

    /// Initiate sync for a document with a peer
    ///
    /// Generates an initial sync message and sends it to the peer.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "cells:cell-1")
    /// * `peer_id` - The EndpointId of the peer to sync with
    pub async fn initiate_sync(&self, doc_key: &str, peer_id: EndpointId) -> Result<()> {
        // Check circuit breaker before attempting sync
        if self.error_handler.is_circuit_open(&peer_id) {
            let err = SyncError::CircuitBreakerOpen;
            tracing::warn!("Sync blocked by circuit breaker for peer {:?}", peer_id);
            return Err(anyhow::anyhow!("{}", err));
        }

        // Check flow control (rate limit + cooldown)
        if let Err(flow_err) = self.flow_controller.check_sync_allowed(&peer_id, doc_key) {
            tracing::debug!(
                "Sync blocked by flow control for peer {:?}, doc {}: {}",
                peer_id,
                doc_key,
                flow_err
            );
            return Err(anyhow::anyhow!("{}", flow_err));
        }

        // Attempt sync operation
        let result = self.initiate_sync_inner(doc_key, peer_id).await;

        // Handle the result through error handler
        match &result {
            Ok(_) => {
                self.error_handler.record_success(&peer_id);
                // Record sync for cooldown tracking
                self.flow_controller.record_sync(&peer_id, doc_key);
                tracing::debug!("Sync initiated successfully with peer {:?}", peer_id);
            }
            Err(e) => {
                // Convert error to SyncError
                let sync_error =
                    if e.to_string().contains("connection") || e.to_string().contains("network") {
                        SyncError::Network(e.to_string())
                    } else if e.to_string().contains("document") || e.to_string().contains("CRDT") {
                        SyncError::Document(e.to_string())
                    } else {
                        SyncError::Protocol(e.to_string())
                    };

                // Process error through handler
                match self.error_handler.handle_error(&peer_id, sync_error) {
                    Ok(Some(retry_delay)) => {
                        tracing::warn!(
                            "Sync failed for peer {:?}, will retry after {:?}",
                            peer_id,
                            retry_delay
                        );
                    }
                    Ok(None) => {
                        tracing::error!("Sync failed for peer {:?}, max retries exceeded", peer_id);
                    }
                    Err(SyncError::CircuitBreakerOpen) => {
                        tracing::error!("Circuit breaker opened for peer {:?}", peer_id);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Error handling sync failure for peer {:?}: {}",
                            peer_id,
                            e
                        );
                    }
                }
            }
        }

        result
    }

    /// Inner sync method without error handling wrapper
    ///
    /// Checks the sync mode for the collection and uses either:
    /// - **FullHistory**: Delta-based sync via `generate_sync_message()`
    /// - **LatestOnly**: State-based sync via `doc.save()` (Issue #355)
    async fn initiate_sync_inner(&self, doc_key: &str, peer_id: EndpointId) -> Result<()> {
        tracing::debug!(
            "initiate_sync_inner: doc_key={}, peer={:?}",
            doc_key,
            peer_id
        );

        // Check sync mode for this collection (Issue #355)
        let sync_mode = self.sync_mode_for_doc(doc_key);
        tracing::debug!(
            "initiate_sync_inner: sync_mode={} for {}",
            sync_mode,
            doc_key
        );

        // Get the document
        let doc = self
            .store
            .get(doc_key)?
            .context("Document not found for sync")?;

        let doc_bytes = doc.save();
        tracing::debug!("initiate_sync_inner: got doc, len={}", doc_bytes.len());

        // Use appropriate sync method based on mode
        match sync_mode {
            SyncMode::LatestOnly => {
                // Issue #355: Send full document state instead of delta sync
                // This is much more efficient for high-frequency data like beacons
                tracing::debug!(
                    "initiate_sync_inner: using LatestOnly mode, sending {} bytes state snapshot",
                    doc_bytes.len()
                );
                self.send_state_snapshot(peer_id, doc_key, &doc_bytes)
                    .await?;
                tracing::debug!("initiate_sync_inner: state snapshot sent successfully");
                Ok(())
            }
            SyncMode::FullHistory | SyncMode::WindowedHistory { .. } => {
                // Traditional delta-based sync
                // WindowedHistory uses same path but receiver will filter (Phase 2)
                self.initiate_delta_sync(doc_key, peer_id, &doc).await
            }
        }
    }

    /// Initiate delta-based sync (FullHistory mode)
    ///
    /// Uses Automerge's sync protocol to exchange deltas.
    async fn initiate_delta_sync(
        &self,
        doc_key: &str,
        peer_id: EndpointId,
        doc: &Automerge,
    ) -> Result<()> {
        // Get or create sync state for this peer
        let mut sync_state = self.get_or_create_sync_state(doc_key, peer_id);

        // Generate initial sync message using SyncDoc trait
        // NOTE: generate_sync_message mutates sync_state internally to track which heads
        // have been "prepared for sending". We must only persist this state AFTER
        // successful send, otherwise retries will fail with "nothing to send".
        let message = match SyncDoc::generate_sync_message(doc, &mut sync_state) {
            Some(msg) => {
                tracing::debug!(
                    "initiate_delta_sync: generated sync message, encoded_len={}",
                    msg.clone().encode().len()
                );
                msg
            }
            None => {
                tracing::debug!("initiate_delta_sync: generate_sync_message returned None");
                return Err(anyhow::anyhow!("No initial sync message to send"));
            }
        };

        // Send message to peer with document key BEFORE updating sync state
        // This ensures that if send fails, we can retry with the same state
        tracing::debug!(
            "initiate_delta_sync: sending sync message to peer {:?}",
            peer_id
        );
        self.send_sync_message_for_doc(peer_id, doc_key, &message)
            .await?;
        tracing::debug!("initiate_delta_sync: sync message sent successfully");

        // Only update sync state AFTER successful send
        self.update_sync_state(doc_key, peer_id, sync_state);

        Ok(())
    }

    /// Send a state snapshot for LatestOnly sync mode (Issue #355)
    ///
    /// Instead of delta-based sync, sends the full document state.
    /// This is ~300× more efficient for high-frequency data after reconnection.
    ///
    /// # Wire Format
    ///
    /// ```text
    /// [2 bytes: doc_key_len][N bytes: doc_key][1 byte: msg_type=0x01][4 bytes: state_len][M bytes: state]
    /// ```
    async fn send_state_snapshot(
        &self,
        peer_id: EndpointId,
        doc_key: &str,
        state_bytes: &[u8],
    ) -> Result<()> {
        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open a bidirectional stream
        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        // Encode doc_key as UTF-8 bytes
        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        // Write doc_key length prefix (2 bytes, big-endian)
        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        // Write doc_key
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        // Write message type (1 byte) - StateSnapshot = 0x01
        send.write_all(&[SyncMessageType::StateSnapshot as u8])
            .await
            .context("Failed to write message type")?;

        // Write state length prefix (4 bytes, big-endian)
        let state_len = state_bytes.len() as u32;
        send.write_all(&state_len.to_be_bytes())
            .await
            .context("Failed to write state length")?;

        // Write the state bytes
        send.write_all(state_bytes)
            .await
            .context("Failed to write state bytes")?;

        // Finish the stream
        send.finish().context("Failed to finish stream")?;

        // Track statistics: bytes sent = doc_key overhead + type + state size
        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + state_bytes.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        // Update per-peer statistics
        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_sent += total_bytes as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Sent state snapshot for {} to {:?}: {} bytes",
            doc_key,
            peer_id,
            total_bytes
        );

        Ok(())
    }

    /// Receive and process a sync message from a peer
    ///
    /// Applies the changes to the document and generates a response message if needed.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier
    /// * `peer_id` - The EndpointId of the sending peer
    /// * `message` - The received sync message
    /// * `message_size` - Size of the received message in bytes (for statistics)
    pub async fn receive_sync_message(
        &self,
        doc_key: &str,
        peer_id: EndpointId,
        message: SyncMessage,
        message_size: usize,
    ) -> Result<()> {
        // Track statistics first
        self.total_bytes_received
            .fetch_add(message_size as u64, Ordering::Relaxed);

        // Update per-peer statistics
        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_received += message_size as u64;
        }

        tracing::debug!(
            "Received sync message for {} from {:?}: {} bytes",
            doc_key,
            peer_id,
            message_size
        );

        // Get the document (or create empty one if doesn't exist)
        let mut doc = self.store.get(doc_key)?.unwrap_or_else(Automerge::new);
        let doc_len_before = doc.save().len();

        // Get or create sync state for this peer
        let mut sync_state = self.get_or_create_sync_state(doc_key, peer_id);

        // Apply the sync message using SyncDoc trait
        SyncDoc::receive_sync_message(&mut doc, &mut sync_state, message)?;

        let doc_len_after = doc.save().len();
        tracing::debug!(
            "receive_sync_message: doc {} size changed from {} to {} bytes",
            doc_key,
            doc_len_before,
            doc_len_after
        );

        // Save updated document - this triggers change notification
        // The flow control cooldown (per peer+doc) will correctly prevent
        // syncing back to the peer that just sent us this document,
        // while still allowing sync to other peers and notifying observers.
        self.store.put(doc_key, &doc)?;

        // Generate response message
        if let Some(response) = SyncDoc::generate_sync_message(&doc, &mut sync_state) {
            // Store updated sync state
            self.update_sync_state(doc_key, peer_id, sync_state);

            // Send response to peer with document key
            self.send_sync_message_for_doc(peer_id, doc_key, &response)
                .await?;
        } else {
            // Store sync state even if no response needed
            self.update_sync_state(doc_key, peer_id, sync_state);
        }

        Ok(())
    }

    /// Send a sync message to a peer over Iroh stream
    ///
    /// Wire format (v2 with message type - Issue #355):
    /// ```text
    /// [2 bytes: doc_key_len][N bytes: doc_key][1 byte: msg_type=0x00][4 bytes: msg_len][M bytes: msg]
    /// ```
    async fn send_sync_message_for_doc(
        &self,
        peer_id: EndpointId,
        doc_key: &str,
        message: &SyncMessage,
    ) -> Result<()> {
        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open a bidirectional stream
        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        // Encode doc_key as UTF-8 bytes
        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        // Write doc_key length prefix (2 bytes, big-endian)
        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        // Write doc_key
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        // Write message type (1 byte) - DeltaSync = 0x00 (Issue #355)
        send.write_all(&[SyncMessageType::DeltaSync as u8])
            .await
            .context("Failed to write message type")?;

        // Encode the sync message (clone since encode() takes ownership)
        let encoded = message.clone().encode();

        // Write message length prefix (4 bytes, big-endian)
        let message_len = encoded.len() as u32;
        send.write_all(&message_len.to_be_bytes())
            .await
            .context("Failed to write message length")?;

        // Write the message
        send.write_all(&encoded)
            .await
            .context("Failed to write message")?;

        // Finish the stream
        send.finish().context("Failed to finish stream")?;

        // Track statistics: bytes sent = doc_key overhead + type + message size
        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + encoded.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        // Update per-peer statistics
        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_sent += total_bytes as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Sent delta sync message for {} to {:?}: {} bytes",
            doc_key,
            peer_id,
            total_bytes
        );

        Ok(())
    }

    /// Receive a sync payload from a peer over Iroh stream (Issue #355)
    ///
    /// Wire format (v2 with message type):
    /// ```text
    /// [2 bytes: doc_key_len][N bytes: doc_key][1 byte: msg_type][4 bytes: payload_len][M bytes: payload]
    /// ```
    ///
    /// Returns (doc_key, payload, total_bytes_received)
    async fn receive_sync_payload_from_stream(
        &self,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(String, ReceivedSyncPayload, usize)> {
        // Read doc_key length prefix (2 bytes, big-endian)
        let mut doc_key_len_bytes = [0u8; 2];
        recv.read_exact(&mut doc_key_len_bytes)
            .await
            .context("Failed to read doc_key length")?;
        let doc_key_len = u16::from_be_bytes(doc_key_len_bytes) as usize;

        // Read doc_key
        let mut doc_key_bytes = vec![0u8; doc_key_len];
        recv.read_exact(&mut doc_key_bytes)
            .await
            .context("Failed to read doc_key")?;
        let doc_key =
            String::from_utf8(doc_key_bytes).context("Failed to parse doc_key as UTF-8")?;

        // Read message type (1 byte) - Issue #355
        let mut msg_type_byte = [0u8; 1];
        recv.read_exact(&mut msg_type_byte)
            .await
            .context("Failed to read message type")?;

        // Read payload length prefix (4 bytes, big-endian)
        let mut payload_len_bytes = [0u8; 4];
        recv.read_exact(&mut payload_len_bytes)
            .await
            .context("Failed to read payload length")?;
        let payload_len = u32::from_be_bytes(payload_len_bytes) as usize;

        // Read the payload
        let mut buffer = vec![0u8; payload_len];
        recv.read_exact(&mut buffer)
            .await
            .context("Failed to read payload")?;

        // Calculate total bytes: doc_key overhead + type + payload size
        let total_bytes = 2 + doc_key_len + 1 + 4 + payload_len;

        // Parse based on message type
        let payload = match msg_type_byte[0] {
            0x00 => {
                // DeltaSync - decode as Automerge sync message
                let message =
                    SyncMessage::decode(&buffer).context("Failed to decode sync message")?;
                ReceivedSyncPayload::Delta(message)
            }
            0x01 => {
                // StateSnapshot - raw Automerge document bytes
                tracing::debug!(
                    "Received state snapshot for {}: {} bytes",
                    doc_key,
                    buffer.len()
                );
                ReceivedSyncPayload::StateSnapshot(buffer)
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unknown sync message type: 0x{:02x}",
                    other
                ));
            }
        };

        Ok((doc_key, payload, total_bytes))
    }

    /// Legacy receive function for backwards compatibility
    ///
    /// Calls the new payload receiver and extracts delta sync message.
    /// Returns error if a state snapshot is received (caller should use new API).
    async fn receive_sync_message_from_stream(
        &self,
        recv: iroh::endpoint::RecvStream,
    ) -> Result<(String, SyncMessage, usize)> {
        let (doc_key, payload, total_bytes) = self.receive_sync_payload_from_stream(recv).await?;

        match payload {
            ReceivedSyncPayload::Delta(message) => Ok((doc_key, message, total_bytes)),
            ReceivedSyncPayload::StateSnapshot(_) => Err(anyhow::anyhow!(
                "Received state snapshot but expected delta sync message for {}",
                doc_key
            )),
        }
    }

    /// Get or create sync state for a peer
    fn get_or_create_sync_state(&self, doc_key: &str, peer_id: EndpointId) -> SyncState {
        let mut states = self.peer_states.write().unwrap();
        states
            .entry(doc_key.to_string())
            .or_default()
            .entry(peer_id)
            .or_default()
            .clone()
    }

    /// Update sync state for a peer
    fn update_sync_state(&self, doc_key: &str, peer_id: EndpointId, state: SyncState) {
        let mut states = self.peer_states.write().unwrap();
        states
            .entry(doc_key.to_string())
            .or_default()
            .insert(peer_id, state);
    }

    /// Clear sync state for a document (for all peers)
    ///
    /// This should be called when a document is modified locally, to ensure
    /// the next sync attempt will generate a fresh sync message with the new
    /// document heads rather than thinking peers are already up-to-date.
    pub fn clear_sync_state_for_document(&self, doc_key: &str) {
        let mut states = self.peer_states.write().unwrap();
        if states.remove(doc_key).is_some() {
            tracing::debug!("Cleared sync state for document {}", doc_key);
        }
    }

    /// Clear all sync state for a peer (call on disconnect/reconnect)
    ///
    /// This removes sync state for a peer across ALL documents. Call this when:
    /// - A peer disconnects (to allow fresh sync on reconnect)
    /// - A peer reconnects (to ensure sync starts from scratch)
    ///
    /// Without this, reconnecting peers may fail to sync because the stale
    /// sync state thinks "I already sent those changes" even though the peer
    /// never received them.
    pub fn clear_peer_sync_state(&self, peer_id: EndpointId) {
        let mut states = self.peer_states.write().unwrap();
        let mut cleared_count = 0;
        for (_doc_key, peer_map) in states.iter_mut() {
            if peer_map.remove(&peer_id).is_some() {
                cleared_count += 1;
            }
        }
        if cleared_count > 0 {
            tracing::debug!(
                "Cleared sync state for peer {:?} ({} document(s))",
                peer_id,
                cleared_count
            );
        }
    }

    /// Sync a specific document with a peer
    ///
    /// This initiates sync for a single document with a peer.
    /// Use this when a document has been created or modified.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "nodes:node-1")
    /// * `peer_id` - The EndpointId of the peer to sync with
    pub async fn sync_document_with_peer(&self, doc_key: &str, peer_id: EndpointId) -> Result<()> {
        self.initiate_sync(doc_key, peer_id).await
    }

    /// Sync a document with all connected peers
    ///
    /// This initiates sync for a single document with all currently connected peers.
    /// Clears existing sync state first to ensure fresh sync messages are generated
    /// even if the document was recently synced but has been modified locally.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "nodes:node-1")
    pub async fn sync_document_with_all_peers(&self, doc_key: &str) -> Result<()> {
        let peer_ids = self.transport.connected_peers();
        tracing::info!(
            "sync_document_with_all_peers: syncing {} with {} peers",
            doc_key,
            peer_ids.len()
        );

        // Clear sync state to ensure we generate fresh sync messages
        // This is important after local document modifications
        self.clear_sync_state_for_document(doc_key);

        for peer_id in peer_ids {
            tracing::debug!("Syncing {} with peer {:?}", doc_key, peer_id);
            if let Err(e) = self.sync_document_with_peer(doc_key, peer_id).await {
                tracing::warn!("Failed to sync {} with peer {:?}: {}", doc_key, peer_id, e);
            }
        }

        Ok(())
    }

    /// Sync all existing documents with a newly connected peer (Issue #235)
    ///
    /// This is called when a new peer connection is established to ensure
    /// documents created before the peer connected are synchronized.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The EndpointId of the newly connected peer
    pub async fn sync_all_documents_with_peer(&self, peer_id: EndpointId) -> Result<()> {
        // Get all document keys from the store
        let all_docs = self.store.scan_prefix("")?;

        tracing::info!(
            "Syncing {} existing documents with new peer {:?}",
            all_docs.len(),
            peer_id
        );

        for (doc_key, _doc) in all_docs {
            if let Err(e) = self.sync_document_with_peer(&doc_key, peer_id).await {
                tracing::warn!(
                    "Failed to sync document {} with new peer {:?}: {}",
                    doc_key,
                    peer_id,
                    e
                );
            }
        }

        Ok(())
    }

    /// Handle an incoming sync connection from a peer
    ///
    /// This is called when a peer initiates sync with us.
    pub async fn handle_incoming_sync(&self, conn: Connection) -> Result<()> {
        let peer_id = conn.remote_id();

        // Accept a bidirectional stream
        let (_send, recv) = conn
            .accept_bi()
            .await
            .context("Failed to accept bidirectional stream")?;

        // Receive the sync message (now includes doc_key and size in wire format)
        let (doc_key, message, message_size) = self.receive_sync_message_from_stream(recv).await?;

        // Process the message with statistics tracking
        self.receive_sync_message(&doc_key, peer_id, message, message_size)
            .await?;

        Ok(())
    }

    /// Handle an incoming sync stream (when streams are accepted externally)
    ///
    /// This is a more efficient variant for continuous accept loops where
    /// streams are pre-accepted and passed in directly.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The EndpointId of the peer (for stats tracking)
    /// * `_send` - The send half of the bidirectional stream (unused for now)
    /// * `recv` - The receive half of the bidirectional stream
    pub async fn handle_incoming_sync_stream(
        &self,
        peer_id: EndpointId,
        _send: iroh::endpoint::SendStream,
        recv: iroh::endpoint::RecvStream,
    ) -> Result<()> {
        // Receive the sync payload (includes doc_key and message type in wire format)
        let (doc_key, payload, payload_size) = self.receive_sync_payload_from_stream(recv).await?;

        // Process based on payload type (Issue #355)
        match payload {
            ReceivedSyncPayload::Delta(message) => {
                // Traditional delta-based sync
                self.receive_sync_message(&doc_key, peer_id, message, payload_size)
                    .await?;
            }
            ReceivedSyncPayload::StateSnapshot(state_bytes) => {
                // LatestOnly mode: apply full state snapshot
                self.apply_state_snapshot(&doc_key, peer_id, state_bytes, payload_size)
                    .await?;
            }
        }

        Ok(())
    }

    /// Apply a state snapshot to a document (Issue #355)
    ///
    /// Used for LatestOnly sync mode. Replaces the local document with the
    /// received state, or merges if the document already exists.
    async fn apply_state_snapshot(
        &self,
        doc_key: &str,
        peer_id: EndpointId,
        state_bytes: Vec<u8>,
        payload_size: usize,
    ) -> Result<()> {
        // Track statistics first
        self.total_bytes_received
            .fetch_add(payload_size as u64, Ordering::Relaxed);

        // Update per-peer statistics
        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_received += payload_size as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Applying state snapshot for {} from {:?}: {} bytes",
            doc_key,
            peer_id,
            state_bytes.len()
        );

        // Load the received document
        let received_doc =
            Automerge::load(&state_bytes).context("Failed to load state snapshot")?;

        // Check if we have an existing document
        let mut received_doc = received_doc;
        match self.store.get(doc_key) {
            Ok(Some(mut existing_doc)) => {
                // Merge the received state into our existing document
                // This handles the case where both sides have made changes
                existing_doc
                    .merge(&mut received_doc)
                    .context("Failed to merge state snapshot")?;

                // Update the store (this triggers change notification via broadcast channel)
                self.store.put(doc_key, &existing_doc)?;

                tracing::debug!("Merged state snapshot into existing document {}", doc_key);
            }
            Ok(None) => {
                // No existing document, just store the received one
                self.store.put(doc_key, &received_doc)?;

                tracing::debug!("Stored new document {} from state snapshot", doc_key);
            }
            Err(e) => {
                tracing::warn!(
                    "Error checking existing document {}: {}, storing received state",
                    doc_key,
                    e
                );
                self.store.put(doc_key, &received_doc)?;
            }
        }

        Ok(())
    }

    /// Get total bytes sent across all peers
    pub fn total_bytes_sent(&self) -> u64 {
        self.total_bytes_sent.load(Ordering::Relaxed)
    }

    /// Get total bytes received across all peers
    pub fn total_bytes_received(&self) -> u64 {
        self.total_bytes_received.load(Ordering::Relaxed)
    }

    /// Get statistics for a specific peer
    pub fn peer_stats(&self, peer_id: &EndpointId) -> Option<PeerSyncStats> {
        self.peer_stats.read().unwrap().get(peer_id).cloned()
    }

    /// Get statistics for all peers
    pub fn all_peer_stats(&self) -> HashMap<EndpointId, PeerSyncStats> {
        self.peer_stats.read().unwrap().clone()
    }

    /// Get reference to the error handler for diagnostics
    pub fn error_handler(&self) -> &SyncErrorHandler {
        &self.error_handler
    }

    /// Get reference to the partition detector
    pub fn partition_detector(&self) -> &PartitionDetector {
        &self.partition_detector
    }

    /// Get reference to the flow controller
    pub fn flow_controller(&self) -> &FlowController {
        &self.flow_controller
    }

    /// Get flow control statistics
    pub fn flow_control_stats(&self) -> FlowControlStats {
        self.flow_controller.stats()
    }

    /// Send a heartbeat to a peer
    ///
    /// Sends a minimal heartbeat message to verify the peer is reachable.
    /// Wire format: [1 byte: 0x01 (heartbeat marker)][8 bytes: timestamp (u64, big-endian)]
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The EndpointId of the peer to send heartbeat to
    pub async fn send_heartbeat(&self, peer_id: EndpointId) -> Result<()> {
        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open a unidirectional stream (heartbeats don't need response)
        let mut send = conn
            .open_uni()
            .await
            .context("Failed to open unidirectional stream")?;

        // Write heartbeat marker (1 byte: 0x01)
        send.write_all(&[0x01])
            .await
            .context("Failed to write heartbeat marker")?;

        // Write timestamp (8 bytes, big-endian)
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        send.write_all(&timestamp.to_be_bytes())
            .await
            .context("Failed to write timestamp")?;

        // Finish the stream
        send.finish().context("Failed to finish stream")?;

        tracing::trace!("Sent heartbeat to peer {:?}", peer_id);

        Ok(())
    }

    /// Handle an incoming heartbeat from a peer
    ///
    /// Called when a peer sends a heartbeat. Records the heartbeat
    /// success in the partition detector.
    ///
    /// # Arguments
    ///
    /// * `conn` - The connection the heartbeat arrived on
    pub async fn handle_incoming_heartbeat(&self, conn: Connection) -> Result<()> {
        let peer_id = conn.remote_id();

        // Accept a unidirectional stream
        let mut recv = conn
            .accept_uni()
            .await
            .context("Failed to accept unidirectional stream")?;

        // Read heartbeat marker (1 byte: 0x01)
        let mut marker = [0u8; 1];
        recv.read_exact(&mut marker)
            .await
            .context("Failed to read heartbeat marker")?;

        if marker[0] != 0x01 {
            anyhow::bail!(
                "Invalid heartbeat marker: expected 0x01, got {:#x}",
                marker[0]
            );
        }

        // Read timestamp (8 bytes, big-endian)
        let mut timestamp_bytes = [0u8; 8];
        recv.read_exact(&mut timestamp_bytes)
            .await
            .context("Failed to read timestamp")?;
        let _timestamp = u64::from_be_bytes(timestamp_bytes);

        // Record heartbeat success in partition detector
        self.partition_detector.record_heartbeat_success(&peer_id);

        tracing::trace!("Received heartbeat from peer {:?}", peer_id);

        Ok(())
    }

    /// Handle an incoming heartbeat stream (when streams are accepted externally)
    ///
    /// This is a more efficient variant for continuous accept loops.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The EndpointId of the peer (for partition detection)
    /// * `recv` - The unidirectional receive stream
    pub async fn handle_incoming_heartbeat_stream(
        &self,
        peer_id: EndpointId,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<()> {
        // Read heartbeat marker (1 byte: 0x01)
        let mut marker = [0u8; 1];
        recv.read_exact(&mut marker)
            .await
            .context("Failed to read heartbeat marker")?;

        if marker[0] != 0x01 {
            anyhow::bail!(
                "Invalid heartbeat marker: expected 0x01, got {:#x}",
                marker[0]
            );
        }

        // Read timestamp (8 bytes, big-endian)
        let mut timestamp_bytes = [0u8; 8];
        recv.read_exact(&mut timestamp_bytes)
            .await
            .context("Failed to read timestamp")?;
        let _timestamp = u64::from_be_bytes(timestamp_bytes);

        // Record heartbeat success in partition detector
        self.partition_detector.record_heartbeat_success(&peer_id);

        tracing::trace!("Received heartbeat from peer {:?}", peer_id);

        Ok(())
    }

    /// Send heartbeats to all connected peers
    ///
    /// This is called periodically by the background heartbeat task.
    pub async fn send_heartbeats_to_all_peers(&self) -> Result<()> {
        let peer_ids = self.transport.connected_peers();

        for peer_id in peer_ids {
            // Register peer with partition detector if not already registered
            self.partition_detector.register_peer(peer_id);

            // Send heartbeat
            if let Err(e) = self.send_heartbeat(peer_id).await {
                tracing::debug!("Failed to send heartbeat to {:?}: {}", peer_id, e);
                // Record heartbeat failure - event already logged via tracing in partition_detector
                let _event = self.partition_detector.record_heartbeat_failure(&peer_id);
            }
        }

        Ok(())
    }

    /// Check all peers for partition timeouts
    ///
    /// This is called periodically to detect partitions based on elapsed time
    /// since last successful heartbeat.
    ///
    /// Returns partition events for newly detected partitions (events already logged via tracing).
    pub fn check_partition_timeouts(&self) -> Vec<crate::storage::PartitionEvent> {
        self.partition_detector.check_timeouts()
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_coordinator() -> (AutomergeSyncCoordinator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let coordinator = AutomergeSyncCoordinator::new(store, transport);
        (coordinator, temp_dir)
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let (coordinator, _temp) = create_test_coordinator().await;
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_sync_state_management() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let peer_id = coordinator.transport.endpoint_id();

        // Get or create sync state
        let state1 = coordinator.get_or_create_sync_state("doc1", peer_id);
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 1);

        // Update sync state
        coordinator.update_sync_state("doc1", peer_id, state1);

        // Get same state again
        let _state2 = coordinator.get_or_create_sync_state("doc1", peer_id);
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 1);
    }

    /// Diagnostic test for Issue #229 - sync message generation
    #[tokio::test]
    async fn test_sync_message_generation_diagnostic() {
        use automerge::sync::SyncDoc;
        use automerge::transaction::Transactable;

        let (coordinator, _temp) = create_test_coordinator().await;
        let peer_id = coordinator.transport.endpoint_id();

        // Step 1: Create a document with actual data (simulating collection upsert)
        let mut doc = Automerge::new();
        let data = vec![1, 2, 3, 4, 5]; // Simulate serialized JSON
        doc.transact(|tx| {
            tx.put(
                automerge::ROOT,
                "data",
                automerge::ScalarValue::Bytes(data.clone()),
            )?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .expect("Transaction should succeed");

        println!("Step 1: Created doc with heads: {:?}", doc.get_heads());
        assert!(
            !doc.get_heads().is_empty(),
            "Document should have change history"
        );

        // Step 2: Store the document
        let doc_key = "nodes:test-node-1";
        coordinator
            .store
            .put(doc_key, &doc)
            .expect("Store should succeed");
        println!("Step 2: Stored document with key: {}", doc_key);

        // Step 3: Load it back
        let loaded_doc = coordinator
            .store
            .get(doc_key)
            .expect("Get should succeed")
            .expect("Document should exist");

        println!(
            "Step 3: Loaded doc with heads: {:?}",
            loaded_doc.get_heads()
        );
        assert_eq!(
            doc.get_heads(),
            loaded_doc.get_heads(),
            "Loaded doc should have same heads"
        );

        // Step 4: Try to generate sync message with fresh SyncState
        let mut sync_state = coordinator.get_or_create_sync_state(doc_key, peer_id);
        println!("Step 4: Created fresh sync state");

        let message = SyncDoc::generate_sync_message(&loaded_doc, &mut sync_state);
        println!(
            "Step 5: generate_sync_message returned: {:?}",
            message.is_some()
        );

        assert!(
            message.is_some(),
            "Should generate sync message for document with changes and fresh sync state"
        );

        // Extra: Test with SyncState::new() directly
        let mut fresh_state = SyncState::new();
        let message2 = SyncDoc::generate_sync_message(&loaded_doc, &mut fresh_state);
        println!("Extra: SyncState::new() message: {:?}", message2.is_some());
        assert!(
            message2.is_some(),
            "SyncState::new() should also produce a message"
        );
    }
}
