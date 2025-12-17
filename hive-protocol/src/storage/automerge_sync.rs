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
use super::negentropy_sync::{NegentropySync, SyncItem};
#[cfg(feature = "automerge-backend")]
use super::partition_detection::PartitionDetector;
#[cfg(feature = "automerge-backend")]
use super::sync_errors::{SyncError, SyncErrorHandler};
#[cfg(feature = "automerge-backend")]
use crate::hierarchy::HierarchicalRouter;
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
use std::sync::{Arc, RwLock, Weak};
#[cfg(feature = "automerge-backend")]
use std::time::SystemTime;
#[cfg(feature = "automerge-backend")]
#[allow(unused_imports)] // Used in sync message send/receive methods
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Wire format message type prefix (Issue #355, ADR-034)
///
/// Used to distinguish between delta-based sync messages, state snapshots,
/// and tombstone sync messages.
///
/// # Wire Format v3 (ADR-034 Phase 2)
///
/// ```text
/// 0x00 = DeltaSync (original Automerge protocol)
/// 0x01 = StateSnapshot (LatestOnly mode)
/// 0x02 = WindowedHistory (Phase 2)
/// 0x03 = Reserved
/// 0x04 = Tombstone (ADR-034)
/// 0x05 = TombstoneBatch (ADR-034)
/// 0x06 = TombstoneAck (ADR-034)
/// ```
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncMessageType {
    /// Delta-based sync message (Automerge sync protocol)
    DeltaSync = 0x00,
    /// Full state snapshot (doc.save() bytes)
    StateSnapshot = 0x01,
    /// Windowed history sync message (Phase 2)
    WindowedHistory = 0x02,
    /// Single tombstone message (ADR-034 Phase 2)
    Tombstone = 0x04,
    /// Batch of tombstones for initial exchange (ADR-034 Phase 2)
    TombstoneBatch = 0x05,
    /// Acknowledgement of received tombstones (ADR-034 Phase 2)
    TombstoneAck = 0x06,
    /// Batch of sync messages for multiple documents (Issue #438)
    SyncBatch = 0x07,
    /// Negentropy set reconciliation initiate (ADR-040, Issue #435)
    NegentropyInit = 0x08,
    /// Negentropy set reconciliation response (ADR-040, Issue #435)
    NegentropyResponse = 0x09,
    /// Negentropy reconciliation complete - request missing docs (ADR-040, Issue #435)
    NegentropyRequest = 0x0A,
}

/// Sync direction for hierarchical routing (Issue #438 Phase 3)
///
/// Determines how sync messages should be routed based on document type:
/// - Upward: Sync to parent/leader only (nodes, beacons, platforms)
/// - Downward: Sync from leader to members (commands)
/// - Lateral: Sync to peers at same level (cells)
/// - Broadcast: Sync to all connected peers (alerts, contact_reports)
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    /// Sync upward to parent/leader only
    /// Used for: nodes, beacons, platforms
    Upward,
    /// Sync downward from leader to members
    /// Used for: commands
    Downward,
    /// Sync laterally to peers at same hierarchy level
    /// Used for: cells
    Lateral,
    /// Broadcast to all connected peers (existing behavior)
    /// Used for: alerts, contact_reports, and unknown collections
    Broadcast,
}

#[cfg(feature = "automerge-backend")]
impl SyncDirection {
    /// Determine sync direction from document key
    ///
    /// Parses the collection name from the document key (format: "collection:id")
    /// and returns the appropriate sync direction.
    pub fn from_doc_key(doc_key: &str) -> Self {
        // Extract collection name from "collection:id" format
        let collection = doc_key.split(':').next().unwrap_or(doc_key);

        match collection {
            // Upward: State aggregation data flows up the hierarchy
            "nodes" | "beacons" | "platforms" | "summaries" => SyncDirection::Upward,
            // Downward: Commands flow down from coordinators to executors
            "commands" => SyncDirection::Downward,
            // Lateral: Cell state syncs between peers in same cell
            "cells" => SyncDirection::Lateral,
            // Broadcast: Alerts and reports go everywhere
            "alerts" | "contact_reports" | "events" => SyncDirection::Broadcast,
            // Default to broadcast for unknown collections (safe fallback)
            _ => SyncDirection::Broadcast,
        }
    }
}

/// A single document sync entry within a batch (Issue #438)
///
/// Represents one document's sync data within a `SyncBatch`. Can be a delta sync,
/// state snapshot, or tombstone.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub struct SyncEntry {
    /// Document key (e.g., "nodes:node-1")
    pub doc_key: String,
    /// Type of sync for this document
    pub sync_type: SyncMessageType,
    /// Payload bytes (encoded SyncMessage, state bytes, or tombstone)
    pub payload: Vec<u8>,
}

#[cfg(feature = "automerge-backend")]
impl SyncEntry {
    /// Create a new sync entry
    pub fn new(doc_key: String, sync_type: SyncMessageType, payload: Vec<u8>) -> Self {
        Self {
            doc_key,
            sync_type,
            payload,
        }
    }

    /// Encode to wire format
    ///
    /// Wire format:
    /// ```text
    /// [2 bytes: doc_key_len][doc_key][1 byte: sync_type][4 bytes: payload_len][payload]
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let doc_key_bytes = self.doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        let mut buf = Vec::with_capacity(2 + doc_key_bytes.len() + 1 + 4 + self.payload.len());
        buf.extend_from_slice(&doc_key_len.to_be_bytes());
        buf.extend_from_slice(doc_key_bytes);
        buf.push(self.sync_type as u8);
        buf.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode from wire format, returns (entry, bytes_consumed)
    pub fn decode(bytes: &[u8]) -> anyhow::Result<(Self, usize)> {
        use anyhow::Context;

        if bytes.len() < 7 {
            anyhow::bail!("SyncEntry too short: {} bytes", bytes.len());
        }

        let doc_key_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        let mut offset = 2;

        if bytes.len() < offset + doc_key_len + 1 + 4 {
            anyhow::bail!("SyncEntry truncated at doc_key");
        }

        let doc_key = String::from_utf8(bytes[offset..offset + doc_key_len].to_vec())
            .context("Invalid UTF-8 in doc_key")?;
        offset += doc_key_len;

        let sync_type = match bytes[offset] {
            0x00 => SyncMessageType::DeltaSync,
            0x01 => SyncMessageType::StateSnapshot,
            0x02 => SyncMessageType::WindowedHistory,
            0x04 => SyncMessageType::Tombstone,
            0x05 => SyncMessageType::TombstoneBatch,
            0x06 => SyncMessageType::TombstoneAck,
            0x07 => SyncMessageType::SyncBatch,
            other => anyhow::bail!("Unknown sync type: 0x{:02x}", other),
        };
        offset += 1;

        let payload_len = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + payload_len {
            anyhow::bail!("SyncEntry truncated at payload");
        }

        let payload = bytes[offset..offset + payload_len].to_vec();
        offset += payload_len;

        Ok((
            Self {
                doc_key,
                sync_type,
                payload,
            },
            offset,
        ))
    }
}

/// Batch of sync messages for multiple documents (Issue #438)
///
/// Enables sending multiple document syncs in a single QUIC stream,
/// reducing stream-per-message overhead from O(N×M) to O(N) where
/// N = peers and M = documents.
///
/// # Wire Format
///
/// ```text
/// [8 bytes: batch_id][8 bytes: created_at][1 byte: ttl][4 bytes: entry_count][entries...]
/// ```
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone)]
pub struct SyncBatch {
    /// Unique batch ID for tracking/acknowledgement
    pub batch_id: u64,
    /// Timestamp when batch was created (millis since UNIX epoch)
    pub created_at: u64,
    /// Time-to-live (hop count) for multi-hop forwarding
    /// Decremented at each hop; batch is dropped when TTL reaches 0
    pub ttl: u8,
    /// Entries in this batch
    pub entries: Vec<SyncEntry>,
}

/// Default TTL for sync batches (max 5 hops)
#[cfg(feature = "automerge-backend")]
pub const DEFAULT_SYNC_BATCH_TTL: u8 = 5;

#[cfg(feature = "automerge-backend")]
impl SyncBatch {
    /// Create a new empty batch
    pub fn new() -> Self {
        Self {
            batch_id: 0,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            ttl: DEFAULT_SYNC_BATCH_TTL,
            entries: Vec::new(),
        }
    }

    /// Create a batch with a specific ID
    pub fn with_id(batch_id: u64) -> Self {
        Self {
            batch_id,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            ttl: DEFAULT_SYNC_BATCH_TTL,
            entries: Vec::new(),
        }
    }

    /// Create a batch with entries (Issue #438 Phase 3)
    pub fn with_entries(entries: Vec<SyncEntry>) -> Self {
        Self {
            batch_id: 0, // Will be assigned when sent
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            ttl: DEFAULT_SYNC_BATCH_TTL,
            entries,
        }
    }

    /// Set TTL for this batch
    pub fn with_ttl(mut self, ttl: u8) -> Self {
        self.ttl = ttl;
        self
    }

    /// Decrement TTL and return true if batch should still be forwarded
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl > 0 {
            self.ttl -= 1;
            true
        } else {
            false
        }
    }

    /// Add a delta sync entry
    pub fn add_delta(&mut self, doc_key: &str, message: &automerge::sync::Message) {
        let payload = message.clone().encode();
        self.entries.push(SyncEntry::new(
            doc_key.to_string(),
            SyncMessageType::DeltaSync,
            payload,
        ));
    }

    /// Add a state snapshot entry
    pub fn add_snapshot(&mut self, doc_key: &str, state_bytes: Vec<u8>) {
        self.entries.push(SyncEntry::new(
            doc_key.to_string(),
            SyncMessageType::StateSnapshot,
            state_bytes,
        ));
    }

    /// Add a tombstone entry
    pub fn add_tombstone(&mut self, tombstone: &crate::qos::TombstoneSyncMessage) {
        self.entries.push(SyncEntry::new(
            format!(
                "tombstones:{}:{}",
                tombstone.tombstone.collection, tombstone.tombstone.document_id
            ),
            SyncMessageType::Tombstone,
            tombstone.encode(),
        ));
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get total payload size in bytes
    pub fn payload_size(&self) -> usize {
        self.entries.iter().map(|e| e.payload.len()).sum()
    }

    /// Encode to wire format
    /// Format: [8: batch_id][8: created_at][1: ttl][4: entry_count][entries...]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(21 + self.payload_size());
        buf.extend_from_slice(&self.batch_id.to_be_bytes());
        buf.extend_from_slice(&self.created_at.to_be_bytes());
        buf.push(self.ttl);
        buf.extend_from_slice(&(self.entries.len() as u32).to_be_bytes());
        for entry in &self.entries {
            buf.extend_from_slice(&entry.encode());
        }
        buf
    }

    /// Decode from wire format
    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < 21 {
            anyhow::bail!("SyncBatch too short: {} bytes", bytes.len());
        }

        let batch_id = u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let created_at = u64::from_be_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let ttl = bytes[16];
        let entry_count = u32::from_be_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]) as usize;

        let mut offset = 21;
        let mut entries = Vec::with_capacity(entry_count);

        for _ in 0..entry_count {
            let (entry, consumed) = SyncEntry::decode(&bytes[offset..])?;
            entries.push(entry);
            offset += consumed;
        }

        Ok(Self {
            batch_id,
            created_at,
            ttl,
            entries,
        })
    }
}

#[cfg(feature = "automerge-backend")]
impl Default for SyncBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Received sync payload (Issue #355, ADR-034, Issue #438)
///
/// Can be a delta-based sync message, state snapshot, tombstone, or batch of syncs.
#[cfg(feature = "automerge-backend")]
#[derive(Debug)]
pub enum ReceivedSyncPayload {
    /// Delta-based sync message from Automerge protocol
    Delta(SyncMessage),
    /// Full document state snapshot (from LatestOnly mode)
    StateSnapshot(Vec<u8>),
    /// Single tombstone (ADR-034 Phase 2)
    Tombstone(crate::qos::TombstoneSyncMessage),
    /// Batch of tombstones for initial exchange (ADR-034 Phase 2)
    TombstoneBatch(crate::qos::TombstoneBatch),
    /// Batch of sync messages for multiple documents (Issue #438)
    Batch(SyncBatch),
    /// Negentropy init message - initiates set reconciliation (ADR-040)
    NegentropyInit(Vec<u8>),
    /// Negentropy response message - reconciliation round (ADR-040)
    NegentropyResponse(Vec<u8>),
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
/// - ✅ Batch sync to reduce stream-per-message overhead (Issue #438)
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
    /// Next batch ID counter (Issue #438)
    next_batch_id: Arc<AtomicU64>,
    /// Optional hierarchical router for direction-based sync routing (Issue #438 Phase 3)
    hierarchical_router: Option<Arc<HierarchicalRouter>>,
    /// Optional channel manager for persistent stream sync (Issue #435)
    /// Uses Weak to avoid circular reference with SyncChannelManager
    channel_manager: Arc<RwLock<Option<Weak<super::sync_channel::SyncChannelManager>>>>,
    /// Negentropy set reconciliation for efficient document discovery (ADR-040, Issue #435)
    negentropy_sync: Arc<NegentropySync>,
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
            next_batch_id: Arc::new(AtomicU64::new(1)),
            hierarchical_router: None,
            channel_manager: Arc::new(RwLock::new(None)),
            negentropy_sync: Arc::new(NegentropySync::new()),
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
            next_batch_id: Arc::new(AtomicU64::new(1)),
            hierarchical_router: None,
            channel_manager: Arc::new(RwLock::new(None)),
            negentropy_sync: Arc::new(NegentropySync::new()),
        }
    }

    /// Set the channel manager for persistent stream sync (Issue #435)
    ///
    /// This enables routing all sync operations through persistent channels
    /// instead of opening new streams for each operation.
    pub fn set_channel_manager(&self, manager: Arc<super::sync_channel::SyncChannelManager>) {
        *self.channel_manager.write().unwrap() = Some(Arc::downgrade(&manager));
    }

    /// Get the channel manager if available
    fn get_channel_manager(&self) -> Option<Arc<super::sync_channel::SyncChannelManager>> {
        self.channel_manager
            .read()
            .unwrap()
            .as_ref()
            .and_then(|weak| weak.upgrade())
    }

    /// Create a new sync coordinator with hierarchical routing (Issue #438 Phase 3)
    ///
    /// When a hierarchical router is provided, sync operations will route messages
    /// based on document type direction:
    /// - Upward (nodes, beacons, platforms): Only sync to cell leader
    /// - Downward (commands): Only sync from leader to cell members
    /// - Lateral (cells): Sync to peers in same cell
    /// - Broadcast (alerts, contact_reports): Sync to all (existing behavior)
    ///
    /// # Arguments
    ///
    /// * `store` - The AutomergeStore managing documents
    /// * `transport` - The IrohTransport for P2P connections
    /// * `router` - HierarchicalRouter for direction-based routing
    pub fn with_hierarchical_router(
        store: Arc<AutomergeStore>,
        transport: Arc<IrohTransport>,
        router: Arc<HierarchicalRouter>,
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
            sync_mode_registry: Arc::new(SyncModeRegistry::with_defaults()),
            next_batch_id: Arc::new(AtomicU64::new(1)),
            hierarchical_router: Some(router),
            channel_manager: Arc::new(RwLock::new(None)),
            negentropy_sync: Arc::new(NegentropySync::new()),
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

    /// Send a state snapshot for LatestOnly sync mode (Issue #355, #435)
    ///
    /// Instead of delta-based sync, sends the full document state.
    /// Uses persistent channels when available.
    async fn send_state_snapshot(
        &self,
        peer_id: EndpointId,
        doc_key: &str,
        state_bytes: &[u8],
    ) -> Result<()> {
        // Issue #435: Use persistent channel if available
        if let Some(channel_manager) = self.get_channel_manager() {
            let total_bytes = channel_manager
                .send_state_snapshot(peer_id, doc_key, state_bytes.to_vec())
                .await?;

            self.total_bytes_sent
                .fetch_add(total_bytes as u64, Ordering::Relaxed);

            {
                let mut stats = self.peer_stats.write().unwrap();
                let peer_stat = stats.entry(peer_id).or_default();
                peer_stat.bytes_sent += total_bytes as u64;
                peer_stat.sync_count += 1;
                peer_stat.last_sync = Some(SystemTime::now());
            }

            tracing::debug!(
                "Sent state snapshot for {} to {:?} via persistent channel ({} bytes)",
                doc_key,
                peer_id,
                total_bytes
            );
            return Ok(());
        }

        // Fallback: Open a new stream (legacy path)
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;
        send.write_all(&[SyncMessageType::StateSnapshot as u8])
            .await
            .context("Failed to write message type")?;

        let state_len = state_bytes.len() as u32;
        send.write_all(&state_len.to_be_bytes())
            .await
            .context("Failed to write state length")?;
        send.write_all(state_bytes)
            .await
            .context("Failed to write state bytes")?;

        send.finish().context("Failed to finish stream")?;
        let _ = recv.stop(0u32.into());

        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + state_bytes.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

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
            // Sync converged - reset state to prevent memory accumulation (Issue #435)
            // Only preserve shared_heads (what both peers have), discard session data
            // like sent_hashes which accumulates unboundedly during sync rounds.
            let mut fresh_state = SyncState::new();
            fresh_state.shared_heads = sync_state.shared_heads;
            self.update_sync_state(doc_key, peer_id, fresh_state);
        }

        Ok(())
    }

    /// Send a sync message to a peer through persistent channel (Issue #435)
    ///
    /// Uses SyncChannelManager for persistent stream multiplexing instead of
    /// opening a new stream for each message.
    async fn send_sync_message_for_doc(
        &self,
        peer_id: EndpointId,
        doc_key: &str,
        message: &SyncMessage,
    ) -> Result<()> {
        // Issue #435: Use persistent channel if available
        if let Some(channel_manager) = self.get_channel_manager() {
            let total_bytes = channel_manager
                .send_delta_sync(peer_id, doc_key, message)
                .await?;

            // Track statistics
            self.total_bytes_sent
                .fetch_add(total_bytes as u64, Ordering::Relaxed);

            {
                let mut stats = self.peer_stats.write().unwrap();
                let peer_stat = stats.entry(peer_id).or_default();
                peer_stat.bytes_sent += total_bytes as u64;
                peer_stat.sync_count += 1;
                peer_stat.last_sync = Some(SystemTime::now());
            }

            tracing::debug!(
                "Sent delta sync for {} to peer {:?} via persistent channel ({} bytes)",
                doc_key,
                peer_id,
                total_bytes
            );
            return Ok(());
        }

        // Fallback: Open a new stream (legacy path)
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        send.write_all(&[SyncMessageType::DeltaSync as u8])
            .await
            .context("Failed to write message type")?;

        let encoded = message.clone().encode();

        let message_len = encoded.len() as u32;
        send.write_all(&message_len.to_be_bytes())
            .await
            .context("Failed to write message length")?;

        send.write_all(&encoded)
            .await
            .context("Failed to write message")?;

        send.finish().context("Failed to finish stream")?;
        let _ = recv.stop(0u32.into());

        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + encoded.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

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
            0x04 => {
                // Tombstone - single tombstone message (ADR-034 Phase 2)
                tracing::debug!(
                    "Received single tombstone for {}: {} bytes",
                    doc_key,
                    buffer.len()
                );
                let tombstone = crate::qos::TombstoneSyncMessage::decode(&buffer)
                    .map_err(|e| anyhow::anyhow!("Failed to decode tombstone: {}", e))?;
                ReceivedSyncPayload::Tombstone(tombstone)
            }
            0x05 => {
                // TombstoneBatch - batch of tombstones (ADR-034 Phase 2)
                tracing::debug!(
                    "Received tombstone batch for {}: {} bytes",
                    doc_key,
                    buffer.len()
                );
                let batch = crate::qos::TombstoneBatch::decode(&buffer)
                    .map_err(|e| anyhow::anyhow!("Failed to decode tombstone batch: {}", e))?;
                ReceivedSyncPayload::TombstoneBatch(batch)
            }
            0x06 => {
                // TombstoneAck - acknowledgement (ADR-034 Phase 2)
                // For now, just log and ignore - acks are informational
                tracing::debug!(
                    "Received tombstone ack for {}: {} bytes",
                    doc_key,
                    buffer.len()
                );
                // Return as a batch with no tombstones to indicate ack
                ReceivedSyncPayload::TombstoneBatch(crate::qos::TombstoneBatch::new())
            }
            0x07 => {
                // SyncBatch - batch of sync messages for multiple documents (Issue #438)
                tracing::debug!("Received sync batch: {} bytes", buffer.len());
                let batch = SyncBatch::decode(&buffer)
                    .map_err(|e| anyhow::anyhow!("Failed to decode sync batch: {}", e))?;
                ReceivedSyncPayload::Batch(batch)
            }
            0x08 => {
                // NegentropyInit - Negentropy set reconciliation init (ADR-040, Issue #435)
                tracing::debug!("Received Negentropy init from peer: {} bytes", buffer.len());
                ReceivedSyncPayload::NegentropyInit(buffer)
            }
            0x09 => {
                // NegentropyResponse - Negentropy reconciliation response (ADR-040, Issue #435)
                tracing::debug!(
                    "Received Negentropy response from peer: {} bytes",
                    buffer.len()
                );
                ReceivedSyncPayload::NegentropyResponse(buffer)
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
            ReceivedSyncPayload::Tombstone(_) | ReceivedSyncPayload::TombstoneBatch(_) => {
                Err(anyhow::anyhow!(
                    "Received tombstone but expected delta sync message for {}",
                    doc_key
                ))
            }
            ReceivedSyncPayload::Batch(_) => Err(anyhow::anyhow!(
                "Received sync batch but expected delta sync message for {}",
                doc_key
            )),
            ReceivedSyncPayload::NegentropyInit(_) | ReceivedSyncPayload::NegentropyResponse(_) => {
                Err(anyhow::anyhow!(
                    "Received Negentropy message but expected delta sync message for {}",
                    doc_key
                ))
            }
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

    // === Batch sync methods (Issue #438) ===
    //
    // These methods enable sending multiple document syncs in a single QUIC stream,
    // reducing stream-per-message overhead from O(N×M) to O(N).

    /// Generate the next batch ID
    fn next_batch_id(&self) -> u64 {
        self.next_batch_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Create a batch for multiple documents without sending
    ///
    /// This is useful for channel-based sync where the batch is queued.
    ///
    /// # Arguments
    ///
    /// * `doc_keys` - Document keys to include in the batch
    pub fn create_batch_for_documents(&self, doc_keys: &[&str]) -> Result<SyncBatch> {
        let mut batch = SyncBatch::with_id(self.next_batch_id());

        for doc_key in doc_keys {
            // Get the document
            let doc = match self.store.get(doc_key)? {
                Some(doc) => doc,
                None => {
                    tracing::debug!("Document {} not found, skipping in batch", doc_key);
                    continue;
                }
            };

            // Check sync mode for this collection
            let sync_mode = self.sync_mode_for_doc(doc_key);

            match sync_mode {
                crate::qos::SyncMode::LatestOnly => {
                    // State snapshot
                    let state_bytes = doc.save();
                    batch.add_snapshot(doc_key, state_bytes);
                }
                crate::qos::SyncMode::FullHistory
                | crate::qos::SyncMode::WindowedHistory { .. } => {
                    // Delta sync - need to generate message
                    // Note: For batch sync we use a fresh sync state since we're
                    // sending to potentially multiple peers
                    let mut sync_state = SyncState::new();
                    if let Some(message) =
                        automerge::sync::SyncDoc::generate_sync_message(&doc, &mut sync_state)
                    {
                        batch.add_delta(doc_key, &message);
                    }
                }
            }
        }

        Ok(batch)
    }

    /// Send a batch of sync messages to a peer (Issue #438)
    ///
    /// Opens a single QUIC stream and sends all entries in the batch.
    ///
    /// # Wire Format
    ///
    /// ```text
    /// [2 bytes: doc_key_len="batch"][5 bytes: "batch"][1 byte: msg_type=0x07]
    /// [4 bytes: batch_len][batch_bytes...]
    /// ```
    pub async fn send_batch_message(&self, peer_id: EndpointId, batch: &SyncBatch) -> Result<()> {
        if batch.is_empty() {
            tracing::debug!("Empty batch, nothing to send to {:?}", peer_id);
            return Ok(());
        }

        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open a bidirectional stream
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream for batch")?;

        // Use "batch" as the doc_key for batch messages
        let doc_key = "batch";
        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        // Encode the batch
        let batch_bytes = batch.encode();

        // Write doc_key length prefix (2 bytes, big-endian)
        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        // Write doc_key
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        // Write message type (1 byte) - SyncBatch = 0x07
        send.write_all(&[SyncMessageType::SyncBatch as u8])
            .await
            .context("Failed to write message type")?;

        // Write batch length prefix (4 bytes, big-endian)
        let batch_len = batch_bytes.len() as u32;
        send.write_all(&batch_len.to_be_bytes())
            .await
            .context("Failed to write batch length")?;

        // Write the batch bytes
        send.write_all(&batch_bytes)
            .await
            .context("Failed to write batch bytes")?;

        // Finish the stream
        send.finish().context("Failed to finish batch stream")?;

        // Issue #435: Explicitly stop recv stream to prevent resource accumulation
        let _ = recv.stop(0u32.into());

        // Track statistics
        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + batch_bytes.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        // Update per-peer statistics
        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_sent += total_bytes as u64;
            peer_stat.sync_count += batch.len() as u64;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Sent batch {} with {} entries ({} bytes) to {:?}",
            batch.batch_id,
            batch.len(),
            total_bytes,
            peer_id
        );

        Ok(())
    }

    /// Receive and process a batch of sync messages (Issue #438)
    ///
    /// Processes each entry in the batch, applying changes to local documents.
    pub async fn receive_batch_message(
        &self,
        peer_id: EndpointId,
        batch: SyncBatch,
        total_bytes: usize,
    ) -> Result<()> {
        // Track statistics first
        self.total_bytes_received
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_received += total_bytes as u64;
            peer_stat.sync_count += batch.len() as u64;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Received batch {} with {} entries ({} bytes) from {:?}",
            batch.batch_id,
            batch.len(),
            total_bytes,
            peer_id
        );

        // Process each entry in the batch
        for entry in batch.entries {
            match entry.sync_type {
                SyncMessageType::DeltaSync => {
                    // Decode and apply delta sync message
                    match SyncMessage::decode(&entry.payload) {
                        Ok(message) => {
                            if let Err(e) = self
                                .receive_sync_message(
                                    &entry.doc_key,
                                    peer_id,
                                    message,
                                    entry.payload.len(),
                                )
                                .await
                            {
                                tracing::warn!(
                                    "Failed to apply delta sync for {} from batch: {}",
                                    entry.doc_key,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to decode delta sync message for {}: {}",
                                entry.doc_key,
                                e
                            );
                        }
                    }
                }
                SyncMessageType::StateSnapshot => {
                    // Apply state snapshot
                    let payload_len = entry.payload.len();
                    if let Err(e) = self
                        .apply_state_snapshot(&entry.doc_key, peer_id, entry.payload, payload_len)
                        .await
                    {
                        tracing::warn!(
                            "Failed to apply state snapshot for {} from batch: {}",
                            entry.doc_key,
                            e
                        );
                    }
                }
                SyncMessageType::Tombstone => {
                    // Handle tombstone
                    match crate::qos::TombstoneSyncMessage::decode(&entry.payload) {
                        Ok(tombstone_msg) => {
                            if let Err(e) = self
                                .handle_incoming_tombstone(
                                    &entry.doc_key,
                                    peer_id,
                                    tombstone_msg,
                                    entry.payload.len(),
                                )
                                .await
                            {
                                tracing::warn!(
                                    "Failed to apply tombstone for {} from batch: {}",
                                    entry.doc_key,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to decode tombstone for {}: {}",
                                entry.doc_key,
                                e
                            );
                        }
                    }
                }
                other => {
                    tracing::warn!(
                        "Unexpected sync type {:?} in batch entry for {}",
                        other,
                        entry.doc_key
                    );
                }
            }
        }

        Ok(())
    }

    /// Sync multiple documents with a single peer using batch (Issue #438)
    ///
    /// Creates a batch containing all specified documents and sends it in a single stream.
    /// This reduces stream overhead from O(M) to O(1) for M documents.
    ///
    /// # Arguments
    ///
    /// * `doc_keys` - Document keys to sync
    /// * `peer_id` - Target peer
    pub async fn sync_documents_batch(&self, doc_keys: &[&str], peer_id: EndpointId) -> Result<()> {
        // Check circuit breaker before attempting sync
        if self.error_handler.is_circuit_open(&peer_id) {
            tracing::warn!(
                "Batch sync blocked by circuit breaker for peer {:?}",
                peer_id
            );
            return Err(anyhow::anyhow!("Circuit breaker open"));
        }

        // Clear sync state for all documents in batch
        for doc_key in doc_keys {
            self.clear_sync_state_for_document(doc_key);
        }

        // Create the batch
        let batch = self.create_batch_for_documents(doc_keys)?;

        if batch.is_empty() {
            tracing::debug!("No documents to sync in batch with {:?}", peer_id);
            return Ok(());
        }

        // Send the batch
        let result = self.send_batch_message(peer_id, &batch).await;

        // Handle the result through error handler
        match &result {
            Ok(_) => {
                self.error_handler.record_success(&peer_id);
                // Record sync for each document's cooldown tracking
                for doc_key in doc_keys {
                    self.flow_controller.record_sync(&peer_id, doc_key);
                }
                tracing::debug!(
                    "Batch sync of {} docs with peer {:?} succeeded",
                    batch.len(),
                    peer_id
                );
            }
            Err(e) => {
                let sync_error =
                    if e.to_string().contains("connection") || e.to_string().contains("network") {
                        SyncError::Network(e.to_string())
                    } else {
                        SyncError::Protocol(e.to_string())
                    };

                if let Err(SyncError::CircuitBreakerOpen) =
                    self.error_handler.handle_error(&peer_id, sync_error)
                {
                    tracing::error!("Circuit breaker opened for peer {:?}", peer_id);
                }
            }
        }

        result
    }

    /// Sync multiple documents with all connected peers using batch (Issue #438)
    ///
    /// Sends a batch to each connected peer in parallel.
    pub async fn sync_documents_batch_with_all_peers(&self, doc_keys: &[&str]) -> Result<()> {
        let peer_ids = self.transport.connected_peers();

        tracing::info!(
            "Batch syncing {} documents with {} peers",
            doc_keys.len(),
            peer_ids.len()
        );

        // Clear sync state for all documents
        for doc_key in doc_keys {
            self.clear_sync_state_for_document(doc_key);
        }

        // Send to each peer (could be parallelized with futures::join_all)
        for peer_id in peer_ids {
            if let Err(e) = self.sync_documents_batch(doc_keys, peer_id).await {
                tracing::warn!(
                    "Failed to batch sync {} docs with peer {:?}: {}",
                    doc_keys.len(),
                    peer_id,
                    e
                );
            }
        }

        Ok(())
    }

    /// Get sync targets for a given sync direction (Issue #438 Phase 3)
    ///
    /// Returns the appropriate peer IDs to sync with based on the direction
    /// and the hierarchical router configuration.
    ///
    /// If no hierarchical router is configured, returns all connected peers (broadcast).
    pub async fn get_sync_targets(&self, direction: SyncDirection) -> Vec<EndpointId> {
        // If no hierarchical router, fall back to broadcast
        let router = match &self.hierarchical_router {
            Some(r) => r,
            None => return self.transport.connected_peers(),
        };

        let all_peers = self.transport.connected_peers();

        match direction {
            SyncDirection::Broadcast => {
                // Broadcast to all connected peers
                all_peers
            }
            SyncDirection::Lateral => {
                // Sync to peers in same cell
                let valid = router.valid_targets().await;
                // Check if we have valid cell peers (non-zone targets)
                let has_cell_peers = valid.iter().any(|t| !t.starts_with("zone:"));
                if has_cell_peers {
                    // Return all connected peers that are valid cell targets
                    // TODO: Add peer_id -> node_id mapping to transport for precise filtering
                    all_peers
                } else {
                    // No lateral peers configured, broadcast to all
                    all_peers
                }
            }
            SyncDirection::Upward => {
                // Only sync to leader (if we're not the leader)
                // If we are the leader, sync to zone coordinator
                if router.is_leader().await {
                    // We're the leader - sync to zone (represented as the first zone target)
                    let valid = router.valid_targets().await;
                    let has_zone_targets = valid.iter().any(|t| t.starts_with("zone:"));
                    if has_zone_targets {
                        // Return empty for now - zone sync requires zone transport mapping
                        // TODO: Implement zone-level sync target resolution
                        tracing::debug!("Upward sync from leader - zone sync not yet implemented");
                        Vec::new()
                    } else {
                        // No zone targets, fallback to broadcast
                        all_peers
                    }
                } else {
                    // Not the leader - sync to cell peers (which includes leader)
                    // The leader will aggregate and sync upward
                    all_peers
                }
            }
            SyncDirection::Downward => {
                // Only leaders can send commands downward
                if router.is_leader().await {
                    // Sync to all cell members
                    all_peers
                } else {
                    // Non-leaders don't send commands downward - they receive
                    tracing::debug!(
                        "Non-leader ignoring downward sync (commands flow from leader)"
                    );
                    Vec::new()
                }
            }
        }
    }

    /// Sync batch with hierarchical routing (Issue #438 Phase 3)
    ///
    /// Splits the batch entries by their sync direction and routes each
    /// sub-batch to the appropriate peers.
    ///
    /// This replaces `sync_documents_batch_with_all_peers` when hierarchical
    /// routing is enabled.
    pub async fn sync_batch_with_hierarchical_routing(&self, batch: &SyncBatch) -> Result<()> {
        // If no hierarchical router, fall back to broadcast
        if self.hierarchical_router.is_none() {
            return self.broadcast_batch(batch).await;
        }

        // Group entries by direction
        let mut upward_entries = Vec::new();
        let mut downward_entries = Vec::new();
        let mut lateral_entries = Vec::new();
        let mut broadcast_entries = Vec::new();

        for entry in &batch.entries {
            let direction = SyncDirection::from_doc_key(&entry.doc_key);
            match direction {
                SyncDirection::Upward => upward_entries.push(entry.clone()),
                SyncDirection::Downward => downward_entries.push(entry.clone()),
                SyncDirection::Lateral => lateral_entries.push(entry.clone()),
                SyncDirection::Broadcast => broadcast_entries.push(entry.clone()),
            }
        }

        tracing::debug!(
            "Hierarchical routing: {} upward, {} downward, {} lateral, {} broadcast",
            upward_entries.len(),
            downward_entries.len(),
            lateral_entries.len(),
            broadcast_entries.len()
        );

        // Route each direction separately
        if !upward_entries.is_empty() {
            let upward_batch = SyncBatch::with_entries(upward_entries);
            let targets = self.get_sync_targets(SyncDirection::Upward).await;
            self.send_batch_to_peers(&upward_batch, &targets).await?;
        }

        if !downward_entries.is_empty() {
            let downward_batch = SyncBatch::with_entries(downward_entries);
            let targets = self.get_sync_targets(SyncDirection::Downward).await;
            self.send_batch_to_peers(&downward_batch, &targets).await?;
        }

        if !lateral_entries.is_empty() {
            let lateral_batch = SyncBatch::with_entries(lateral_entries);
            let targets = self.get_sync_targets(SyncDirection::Lateral).await;
            self.send_batch_to_peers(&lateral_batch, &targets).await?;
        }

        if !broadcast_entries.is_empty() {
            let broadcast_batch = SyncBatch::with_entries(broadcast_entries);
            let targets = self.get_sync_targets(SyncDirection::Broadcast).await;
            self.send_batch_to_peers(&broadcast_batch, &targets).await?;
        }

        Ok(())
    }

    /// Send batch to specific peers
    async fn send_batch_to_peers(&self, batch: &SyncBatch, peers: &[EndpointId]) -> Result<()> {
        if peers.is_empty() {
            tracing::trace!("No peers to send batch to");
            return Ok(());
        }

        tracing::debug!("Sending batch {} to {} peers", batch.batch_id, peers.len());

        for peer_id in peers {
            if let Err(e) = self.send_batch_message(*peer_id, batch).await {
                tracing::warn!("Failed to send batch to peer {:?}: {}", peer_id, e);
            }
        }

        Ok(())
    }

    /// Broadcast batch to all connected peers
    async fn broadcast_batch(&self, batch: &SyncBatch) -> Result<()> {
        let peers = self.transport.connected_peers();
        self.send_batch_to_peers(batch, &peers).await
    }

    /// Check if hierarchical routing is enabled
    pub fn has_hierarchical_routing(&self) -> bool {
        self.hierarchical_router.is_some()
    }

    /// Get hierarchical router reference (if configured)
    pub fn hierarchical_router(&self) -> Option<&Arc<HierarchicalRouter>> {
        self.hierarchical_router.as_ref()
    }

    // === Tombstone sync methods (ADR-034 Phase 2) ===

    /// Send all tombstones to a peer as a batch
    ///
    /// Called when connecting to a new peer to exchange deletion markers.
    /// This ensures the peer knows about all documents we've deleted.
    ///
    /// # Wire Format
    ///
    /// ```text
    /// [2 bytes: doc_key_len][N bytes: doc_key][1 byte: msg_type=0x05][4 bytes: batch_len][M bytes: batch]
    /// ```
    pub async fn send_tombstones_to_peer(&self, peer_id: EndpointId) -> Result<()> {
        // Get all tombstones from storage
        let tombstones = self.store.get_all_tombstones()?;

        if tombstones.is_empty() {
            tracing::debug!("No tombstones to send to peer {:?}", peer_id);
            return Ok(());
        }

        tracing::info!(
            "Sending {} tombstones to peer {:?}",
            tombstones.len(),
            peer_id
        );

        // Convert to TombstoneSyncMessages with direction
        let sync_messages: Vec<crate::qos::TombstoneSyncMessage> = tombstones
            .into_iter()
            .map(crate::qos::TombstoneSyncMessage::from_tombstone)
            .collect();

        // Create batch
        let batch = crate::qos::TombstoneBatch::with_messages(sync_messages);

        // Encode the batch
        let payload = batch.encode();

        tracing::debug!(
            "Encoded tombstone batch ({} bytes) for peer {:?}",
            payload.len(),
            peer_id
        );

        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer for tombstone exchange")?;

        // Open a bidirectional stream
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream for tombstone exchange")?;

        // Use "tombstones:batch" as the doc_key for tombstone batches
        let doc_key = "tombstones:batch";
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

        // Write message type (1 byte) - TombstoneBatch = 0x05
        send.write_all(&[SyncMessageType::TombstoneBatch as u8])
            .await
            .context("Failed to write message type")?;

        // Write payload length prefix (4 bytes, big-endian)
        let payload_len = payload.len() as u32;
        send.write_all(&payload_len.to_be_bytes())
            .await
            .context("Failed to write payload length")?;

        // Write the payload bytes
        send.write_all(&payload)
            .await
            .context("Failed to write tombstone batch payload")?;

        // Finish the stream
        send.finish().context("Failed to finish tombstone stream")?;

        // Issue #435: Explicitly stop recv stream to prevent resource accumulation
        let _ = recv.stop(0u32.into());

        // Track statistics: bytes sent = doc_key overhead + type + payload size
        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + payload.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_sent += total_bytes as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Successfully sent tombstone batch ({} bytes) to peer {:?}",
            total_bytes,
            peer_id
        );

        Ok(())
    }

    /// Exchange tombstones with a peer on connection
    ///
    /// Called when a new peer connection is established.
    /// Sends our tombstones and prepares to receive theirs.
    pub async fn sync_tombstones_with_peer(&self, peer_id: EndpointId) -> Result<()> {
        tracing::debug!("Initiating tombstone exchange with peer {:?}", peer_id);

        // Send our tombstones to the peer
        if let Err(e) = self.send_tombstones_to_peer(peer_id).await {
            tracing::warn!("Failed to send tombstones to peer {:?}: {}", peer_id, e);
            // Don't fail the whole sync just because tombstone exchange failed
        }

        Ok(())
    }

    /// Apply a tombstone received from a peer
    ///
    /// Stores the tombstone and optionally deletes the local document.
    pub async fn apply_tombstone(
        &self,
        tombstone: &crate::qos::Tombstone,
        peer_id: EndpointId,
    ) -> Result<bool> {
        // Check if we already have this tombstone
        if self
            .store
            .has_tombstone(&tombstone.collection, &tombstone.document_id)?
        {
            tracing::trace!(
                "Tombstone for {}:{} already exists, skipping",
                tombstone.collection,
                tombstone.document_id
            );
            return Ok(false);
        }

        // Store the tombstone
        self.store.put_tombstone(tombstone)?;

        // Delete the local document if it exists
        let doc_key = format!("{}:{}", tombstone.collection, tombstone.document_id);
        if self.store.get(&doc_key)?.is_some() {
            self.store.delete(&doc_key)?;
            tracing::info!(
                "Applied tombstone from peer {:?}: deleted document {}",
                peer_id,
                doc_key
            );
        }

        Ok(true)
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
    /// * `send` - The send half of the bidirectional stream (used for Negentropy responses)
    /// * `recv` - The receive half of the bidirectional stream
    pub async fn handle_incoming_sync_stream(
        &self,
        peer_id: EndpointId,
        mut send: iroh::endpoint::SendStream,
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
            ReceivedSyncPayload::Tombstone(tombstone_msg) => {
                // Tombstone sync (ADR-034 Phase 2)
                self.handle_incoming_tombstone(&doc_key, peer_id, tombstone_msg, payload_size)
                    .await?;
            }
            ReceivedSyncPayload::TombstoneBatch(batch) => {
                // Tombstone batch sync (ADR-034 Phase 2)
                self.handle_incoming_tombstone_batch(&doc_key, peer_id, batch, payload_size)
                    .await?;
            }
            ReceivedSyncPayload::Batch(batch) => {
                // Sync batch for multiple documents (Issue #438)
                self.receive_batch_message(peer_id, batch, payload_size)
                    .await?;
            }
            ReceivedSyncPayload::NegentropyInit(message) => {
                // Negentropy set reconciliation init (ADR-040, Issue #435)
                self.handle_negentropy_init(peer_id, message, &mut send)
                    .await?;
            }
            ReceivedSyncPayload::NegentropyResponse(message) => {
                // Negentropy reconciliation response (ADR-040, Issue #435)
                self.handle_negentropy_response(peer_id, message, &mut send)
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

    /// Handle an incoming tombstone message (ADR-034 Phase 2)
    ///
    /// Processes a single tombstone received from a peer. This applies the
    /// deletion locally and may propagate it further based on direction policy.
    async fn handle_incoming_tombstone(
        &self,
        _doc_key: &str,
        peer_id: EndpointId,
        tombstone_msg: crate::qos::TombstoneSyncMessage,
        payload_size: usize,
    ) -> Result<()> {
        // Track statistics
        self.total_bytes_received
            .fetch_add(payload_size as u64, Ordering::Relaxed);

        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_received += payload_size as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        tracing::debug!(
            "Received tombstone for document {} in collection {} from peer {:?}, direction: {:?}",
            tombstone_msg.tombstone.document_id,
            tombstone_msg.tombstone.collection,
            peer_id,
            tombstone_msg.direction
        );

        // Apply tombstone to local store
        let applied = self
            .apply_tombstone(&tombstone_msg.tombstone, peer_id)
            .await?;

        if applied {
            tracing::info!(
                "Applied tombstone for {}:{} from peer {:?}",
                tombstone_msg.tombstone.collection,
                tombstone_msg.tombstone.document_id,
                peer_id
            );

            // Propagate to other peers based on direction policy (ADR-034 Phase 2)
            self.propagate_tombstone_to_peers(&tombstone_msg, peer_id)
                .await;
        }

        Ok(())
    }

    /// Propagate a tombstone to other connected peers based on direction policy
    ///
    /// Implements ADR-034 direction-aware propagation:
    /// - SystemWide: propagate to ALL peers (for security-critical deletions)
    /// - Bidirectional: propagate to all peers (mesh topology)
    /// - UpOnly: propagate only to parent peers (requires hierarchy info)
    /// - DownOnly: propagate only to child peers (requires hierarchy info)
    ///
    /// Note: UpOnly/DownOnly require hierarchy context from the HiveMesh layer.
    /// At this transport level, we can't distinguish parent vs child peers,
    /// so we conservatively propagate bidirectionally for now.
    async fn propagate_tombstone_to_peers(
        &self,
        tombstone_msg: &crate::qos::TombstoneSyncMessage,
        source_peer_id: EndpointId,
    ) {
        use crate::qos::PropagationDirection;

        let direction = tombstone_msg.direction;

        // Get peers to propagate to (excluding source)
        let all_peers = self.transport.connected_peers();
        let target_peers: Vec<EndpointId> = all_peers
            .into_iter()
            .filter(|p| *p != source_peer_id)
            .collect();

        if target_peers.is_empty() {
            tracing::debug!(
                "No other peers to propagate tombstone {}:{} to",
                tombstone_msg.tombstone.collection,
                tombstone_msg.tombstone.document_id
            );
            return;
        }

        // Determine which peers to propagate to based on direction
        let peers_to_propagate: Vec<EndpointId> = match direction {
            PropagationDirection::SystemWide | PropagationDirection::Bidirectional => {
                // Propagate to all connected peers
                tracing::debug!(
                    "Propagating tombstone {}:{} to {} peers ({:?} mode)",
                    tombstone_msg.tombstone.collection,
                    tombstone_msg.tombstone.document_id,
                    target_peers.len(),
                    direction
                );
                target_peers
            }
            PropagationDirection::UpOnly | PropagationDirection::DownOnly => {
                // UpOnly/DownOnly requires hierarchy context from HiveMesh layer
                // At the transport layer, we don't know parent vs child relationships.
                // Conservative approach: log warning and skip propagation.
                // The HiveMesh layer should handle directional propagation.
                tracing::debug!(
                    "Tombstone {}:{} has {:?} propagation - skipping at transport layer (handled by HiveMesh)",
                    tombstone_msg.tombstone.collection,
                    tombstone_msg.tombstone.document_id,
                    direction
                );
                Vec::new()
            }
        };

        // Send tombstone to each target peer
        for peer_id in peers_to_propagate {
            if let Err(e) = self
                .send_single_tombstone_to_peer(peer_id, tombstone_msg)
                .await
            {
                tracing::warn!(
                    "Failed to propagate tombstone {}:{} to peer {:?}: {}",
                    tombstone_msg.tombstone.collection,
                    tombstone_msg.tombstone.document_id,
                    peer_id,
                    e
                );
            }
        }
    }

    /// Send a single tombstone to a peer
    ///
    /// # Wire Format
    ///
    /// ```text
    /// [2 bytes: doc_key_len][N bytes: doc_key][1 byte: msg_type=0x04][4 bytes: payload_len][M bytes: payload]
    /// ```
    async fn send_single_tombstone_to_peer(
        &self,
        peer_id: EndpointId,
        tombstone_msg: &crate::qos::TombstoneSyncMessage,
    ) -> Result<()> {
        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer for single tombstone")?;

        // Open a bidirectional stream
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream for single tombstone")?;

        // Use "tombstones:single" as the doc_key for single tombstones
        let doc_key = format!(
            "tombstones:{}:{}",
            tombstone_msg.tombstone.collection, tombstone_msg.tombstone.document_id
        );
        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        // Encode the tombstone
        let payload = tombstone_msg.encode();

        // Write doc_key length prefix (2 bytes, big-endian)
        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        // Write doc_key
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        // Write message type (1 byte) - Tombstone = 0x04
        send.write_all(&[SyncMessageType::Tombstone as u8])
            .await
            .context("Failed to write message type")?;

        // Write payload length prefix (4 bytes, big-endian)
        let payload_len = payload.len() as u32;
        send.write_all(&payload_len.to_be_bytes())
            .await
            .context("Failed to write payload length")?;

        // Write the payload bytes
        send.write_all(&payload)
            .await
            .context("Failed to write tombstone payload")?;

        // Finish the stream
        send.finish().context("Failed to finish tombstone stream")?;

        // Issue #435: Explicitly stop recv stream to prevent resource accumulation
        let _ = recv.stop(0u32.into());

        // Track statistics
        let total_bytes = 2 + doc_key_bytes.len() + 1 + 4 + payload.len();
        self.total_bytes_sent
            .fetch_add(total_bytes as u64, Ordering::Relaxed);

        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_sent += total_bytes as u64;
        }

        tracing::trace!(
            "Propagated tombstone {}:{} to peer {:?} ({} bytes)",
            tombstone_msg.tombstone.collection,
            tombstone_msg.tombstone.document_id,
            peer_id,
            total_bytes
        );

        Ok(())
    }

    /// Handle an incoming tombstone batch (ADR-034 Phase 2)
    ///
    /// Processes multiple tombstones received from a peer during initial sync
    /// or batch exchange. This is more efficient than sending individual tombstones.
    async fn handle_incoming_tombstone_batch(
        &self,
        _doc_key: &str,
        peer_id: EndpointId,
        batch: crate::qos::TombstoneBatch,
        payload_size: usize,
    ) -> Result<()> {
        // Track statistics
        self.total_bytes_received
            .fetch_add(payload_size as u64, Ordering::Relaxed);

        {
            let mut stats = self.peer_stats.write().unwrap();
            let peer_stat = stats.entry(peer_id).or_default();
            peer_stat.bytes_received += payload_size as u64;
            peer_stat.sync_count += 1;
            peer_stat.last_sync = Some(SystemTime::now());
        }

        let count = batch.tombstones.len();
        tracing::info!(
            "Received tombstone batch with {} tombstones from peer {:?}",
            count,
            peer_id
        );

        // Apply each tombstone to local store
        let mut applied_count = 0;
        for tombstone_msg in batch.tombstones {
            match self
                .apply_tombstone(&tombstone_msg.tombstone, peer_id)
                .await
            {
                Ok(true) => applied_count += 1,
                Ok(false) => {
                    // Tombstone already existed, skip
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to apply tombstone for {}:{}: {}",
                        tombstone_msg.tombstone.collection,
                        tombstone_msg.tombstone.document_id,
                        e
                    );
                }
            }
        }

        tracing::info!(
            "Applied {}/{} tombstones from peer {:?}",
            applied_count,
            count,
            peer_id
        );

        // TODO: Issue #367 - Propagate to other peers based on direction policies

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

    // ========================================================================
    // Negentropy Set Reconciliation (ADR-040, Issue #435)
    // ========================================================================

    /// Get local document inventory as SyncItems for Negentropy reconciliation
    ///
    /// Returns a list of all documents with their keys and timestamps,
    /// suitable for Negentropy set reconciliation.
    fn get_local_sync_items(&self) -> Vec<SyncItem> {
        // Get all documents by scanning with empty prefix
        let docs = self.store.scan_prefix("").unwrap_or_default();
        docs.into_iter()
            .map(|(key, _doc)| {
                // Use current timestamp - could be improved with actual doc timestamps
                let timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                SyncItem::from_doc_key(&key, timestamp)
            })
            .collect()
    }

    /// Initiate Negentropy sync with a peer
    ///
    /// This discovers which documents need to be synced using O(log n) rounds
    /// instead of syncing all documents blindly.
    ///
    /// # Returns
    ///
    /// The initial Negentropy message to send to the peer.
    pub fn initiate_negentropy_sync(&self, peer_id: EndpointId) -> Result<Vec<u8>> {
        let items = self.get_local_sync_items();
        tracing::debug!(
            "Initiating Negentropy sync with peer {:?}, local_docs={}",
            peer_id,
            items.len()
        );
        self.negentropy_sync.initiate_sync(peer_id, items)
    }

    /// Handle incoming Negentropy message from a peer
    ///
    /// Processes the Negentropy reconciliation message and returns:
    /// - Response message to send back (if reconciliation not complete)
    /// - List of document keys we have that peer needs (have_keys)
    /// - List of document keys peer has that we need (need_keys)
    pub fn handle_negentropy_message(
        &self,
        peer_id: EndpointId,
        message: &[u8],
    ) -> Result<super::negentropy_sync::ReconcileResult> {
        let items = self.get_local_sync_items();
        self.negentropy_sync.handle_message(peer_id, message, items)
    }

    /// Send Negentropy initiation message to peer
    ///
    /// Opens a bidirectional stream and sends the initial Negentropy message.
    pub async fn send_negentropy_init(&self, peer_id: EndpointId) -> Result<()> {
        let init_msg = self.initiate_negentropy_sync(peer_id)?;

        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        // Wire format: [2 bytes: doc_key_len][doc_key][1 byte: msg_type][4 bytes: len][payload]
        let doc_key = "_negentropy";
        let doc_key_bytes = doc_key.as_bytes();

        send.write_all(&(doc_key_bytes.len() as u16).to_be_bytes())
            .await?;
        send.write_all(doc_key_bytes).await?;
        send.write_all(&[SyncMessageType::NegentropyInit as u8])
            .await?;
        send.write_all(&(init_msg.len() as u32).to_be_bytes())
            .await?;
        send.write_all(&init_msg).await?;
        send.finish()?;

        // Close recv side - we don't need it for this message
        recv.stop(0u32.into())?;

        self.total_bytes_sent.fetch_add(
            2 + doc_key_bytes.len() as u64 + 1 + 4 + init_msg.len() as u64,
            Ordering::Relaxed,
        );

        tracing::debug!(
            "Sent Negentropy init to peer {:?}, msg_len={}",
            peer_id,
            init_msg.len()
        );

        Ok(())
    }

    /// Perform full Negentropy-based sync with a peer
    ///
    /// This is the main entry point for efficient sync:
    /// 1. Negentropy reconciliation to discover differences
    /// 2. Send documents we have that peer needs
    /// 3. Request documents peer has that we need
    pub async fn sync_with_peer_negentropy(&self, peer_id: EndpointId) -> Result<()> {
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Phase 1: Initiate Negentropy sync
        let init_msg = self.initiate_negentropy_sync(peer_id)?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        // Wire format: [2 bytes: doc_key_len][doc_key][1 byte: msg_type][4 bytes: len][payload]
        let doc_key = "_negentropy";
        let doc_key_bytes = doc_key.as_bytes();

        // Send init message
        send.write_all(&(doc_key_bytes.len() as u16).to_be_bytes())
            .await?;
        send.write_all(doc_key_bytes).await?;
        send.write_all(&[SyncMessageType::NegentropyInit as u8])
            .await?;
        send.write_all(&(init_msg.len() as u32).to_be_bytes())
            .await?;
        send.write_all(&init_msg).await?;

        self.total_bytes_sent.fetch_add(
            2 + doc_key_bytes.len() as u64 + 1 + 4 + init_msg.len() as u64,
            Ordering::Relaxed,
        );

        // Phase 2: Reconciliation loop
        let mut have_keys: Vec<String> = Vec::new();
        let mut need_keys: Vec<String> = Vec::new();

        loop {
            // Read response with doc_key prefix
            let mut doc_key_len_bytes = [0u8; 2];
            recv.read_exact(&mut doc_key_len_bytes).await?;
            let resp_doc_key_len = u16::from_be_bytes(doc_key_len_bytes) as usize;

            let mut resp_doc_key_bytes = vec![0u8; resp_doc_key_len];
            recv.read_exact(&mut resp_doc_key_bytes).await?;

            let mut type_buf = [0u8; 1];
            recv.read_exact(&mut type_buf).await?;

            let mut len_buf = [0u8; 4];
            recv.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;

            let mut payload = vec![0u8; len];
            recv.read_exact(&mut payload).await?;

            self.total_bytes_received.fetch_add(
                2 + resp_doc_key_len as u64 + 1 + 4 + len as u64,
                Ordering::Relaxed,
            );

            // Process response
            let result = self.handle_negentropy_message(peer_id, &payload)?;

            have_keys.extend(result.have_keys);
            need_keys.extend(result.need_keys);

            if result.is_complete {
                tracing::info!(
                    "Negentropy sync complete with {:?}: have={}, need={}",
                    peer_id,
                    have_keys.len(),
                    need_keys.len()
                );
                break;
            }

            // Send next message
            if let Some(next_msg) = result.next_message {
                send.write_all(&(doc_key_bytes.len() as u16).to_be_bytes())
                    .await?;
                send.write_all(doc_key_bytes).await?;
                send.write_all(&[SyncMessageType::NegentropyResponse as u8])
                    .await?;
                send.write_all(&(next_msg.len() as u32).to_be_bytes())
                    .await?;
                send.write_all(&next_msg).await?;

                self.total_bytes_sent.fetch_add(
                    2 + doc_key_bytes.len() as u64 + 1 + 4 + next_msg.len() as u64,
                    Ordering::Relaxed,
                );
            }
        }

        send.finish()?;

        // Phase 3: Sync documents based on discovery
        // Send documents we have that peer needs
        if !have_keys.is_empty() {
            tracing::debug!(
                "Sending {} documents to peer {:?}",
                have_keys.len(),
                peer_id
            );
            let doc_key_refs: Vec<&str> = have_keys.iter().map(|s| s.as_str()).collect();
            self.sync_documents_batch(&doc_key_refs, peer_id).await?;
        }

        // Request documents peer has that we need
        // (The peer will send these based on their have_keys from reconciliation)

        Ok(())
    }

    /// Get Negentropy sync statistics
    pub fn negentropy_stats(&self) -> super::negentropy_sync::NegentropyStats {
        self.negentropy_sync.stats()
    }

    /// Handle incoming Negentropy init message from a peer
    ///
    /// This is called when a peer initiates Negentropy sync with us.
    /// We process their init message and send back a response.
    async fn handle_negentropy_init(
        &self,
        peer_id: EndpointId,
        message: Vec<u8>,
        send: &mut iroh::endpoint::SendStream,
    ) -> Result<()> {
        tracing::debug!(
            "Handling Negentropy init from {:?}, msg_len={}",
            peer_id,
            message.len()
        );

        // Get our local documents to compare
        let items = self.get_local_sync_items();

        // Start a new session and process their init message
        // First initiate our side (to create session)
        let _init = self.negentropy_sync.initiate_sync(peer_id, items.clone())?;

        // Then handle their message
        let result = self
            .negentropy_sync
            .handle_message(peer_id, &message, items)?;

        // Send documents we have that peer needs
        if !result.have_keys.is_empty() {
            tracing::debug!(
                "Negentropy: we have {} docs peer {:?} needs",
                result.have_keys.len(),
                peer_id
            );
            // Will sync these after reconciliation completes
        }

        // Track documents peer has that we need
        if !result.need_keys.is_empty() {
            tracing::debug!(
                "Negentropy: peer {:?} has {} docs we need",
                peer_id,
                result.need_keys.len()
            );
        }

        // Send response if not complete
        if let Some(next_msg) = &result.next_message {
            // Wire format: [2 bytes: doc_key_len][doc_key][1 byte: msg_type][4 bytes: len][payload]
            // Use "_negentropy" as doc_key for Negentropy messages
            let doc_key = "_negentropy";
            let doc_key_bytes = doc_key.as_bytes();

            send.write_all(&(doc_key_bytes.len() as u16).to_be_bytes())
                .await?;
            send.write_all(doc_key_bytes).await?;
            send.write_all(&[SyncMessageType::NegentropyResponse as u8])
                .await?;
            send.write_all(&(next_msg.len() as u32).to_be_bytes())
                .await?;
            send.write_all(next_msg).await?;
            send.finish()?;

            self.total_bytes_sent.fetch_add(
                2 + doc_key_bytes.len() as u64 + 1 + 4 + next_msg.len() as u64,
                Ordering::Relaxed,
            );

            tracing::debug!(
                "Sent Negentropy response to {:?}, msg_len={}",
                peer_id,
                next_msg.len()
            );
        } else {
            // Reconciliation complete on first round
            tracing::info!(
                "Negentropy sync complete with {:?} on init (have={}, need={})",
                peer_id,
                result.have_keys.len(),
                result.need_keys.len()
            );

            // Send our documents that peer needs
            if !result.have_keys.is_empty() {
                let doc_key_refs: Vec<&str> = result.have_keys.iter().map(|s| s.as_str()).collect();
                self.sync_documents_batch(&doc_key_refs, peer_id).await?;
            }
        }

        Ok(())
    }

    /// Handle incoming Negentropy response message from a peer
    ///
    /// This is called during ongoing Negentropy reconciliation.
    async fn handle_negentropy_response(
        &self,
        peer_id: EndpointId,
        message: Vec<u8>,
        send: &mut iroh::endpoint::SendStream,
    ) -> Result<()> {
        tracing::debug!(
            "Handling Negentropy response from {:?}, msg_len={}",
            peer_id,
            message.len()
        );

        // Process the response
        let items = self.get_local_sync_items();
        let result = self
            .negentropy_sync
            .handle_message(peer_id, &message, items)?;

        if result.is_complete {
            tracing::info!(
                "Negentropy sync complete with {:?} (have={}, need={})",
                peer_id,
                result.have_keys.len(),
                result.need_keys.len()
            );

            // Send our documents that peer needs
            if !result.have_keys.is_empty() {
                let doc_key_refs: Vec<&str> = result.have_keys.iter().map(|s| s.as_str()).collect();
                self.sync_documents_batch(&doc_key_refs, peer_id).await?;
            }
        } else if let Some(next_msg) = &result.next_message {
            // Send next reconciliation message
            let doc_key = "_negentropy";
            let doc_key_bytes = doc_key.as_bytes();

            send.write_all(&(doc_key_bytes.len() as u16).to_be_bytes())
                .await?;
            send.write_all(doc_key_bytes).await?;
            send.write_all(&[SyncMessageType::NegentropyResponse as u8])
                .await?;
            send.write_all(&(next_msg.len() as u32).to_be_bytes())
                .await?;
            send.write_all(next_msg).await?;

            self.total_bytes_sent.fetch_add(
                2 + doc_key_bytes.len() as u64 + 1 + 4 + next_msg.len() as u64,
                Ordering::Relaxed,
            );

            tracing::debug!(
                "Sent next Negentropy message to {:?}, msg_len={}",
                peer_id,
                next_msg.len()
            );
        }

        Ok(())
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

    // === Batch sync tests (Issue #438) ===

    #[test]
    fn test_sync_entry_encode_decode() {
        // Test with DeltaSync type
        let entry = SyncEntry::new(
            "nodes:test-node-1".to_string(),
            SyncMessageType::DeltaSync,
            vec![1, 2, 3, 4, 5],
        );

        let encoded = entry.encode();

        // Verify wire format
        // [2 bytes: doc_key_len][doc_key][1 byte: sync_type][4 bytes: payload_len][payload]
        assert_eq!(u16::from_be_bytes([encoded[0], encoded[1]]), 17); // "nodes:test-node-1" len
        assert_eq!(&encoded[2..19], b"nodes:test-node-1");
        assert_eq!(encoded[19], 0x00); // DeltaSync
        assert_eq!(
            u32::from_be_bytes([encoded[20], encoded[21], encoded[22], encoded[23]]),
            5
        );
        assert_eq!(&encoded[24..], &[1, 2, 3, 4, 5]);

        // Decode and verify roundtrip
        let (decoded, consumed) = SyncEntry::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.doc_key, "nodes:test-node-1");
        assert_eq!(decoded.sync_type, SyncMessageType::DeltaSync);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_sync_entry_encode_decode_state_snapshot() {
        let entry = SyncEntry::new(
            "beacons:beacon-42".to_string(),
            SyncMessageType::StateSnapshot,
            vec![10, 20, 30, 40, 50, 60],
        );

        let encoded = entry.encode();
        let (decoded, _) = SyncEntry::decode(&encoded).unwrap();

        assert_eq!(decoded.doc_key, "beacons:beacon-42");
        assert_eq!(decoded.sync_type, SyncMessageType::StateSnapshot);
        assert_eq!(decoded.payload, vec![10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn test_sync_batch_empty() {
        let batch = SyncBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        assert_eq!(batch.payload_size(), 0);
    }

    #[test]
    fn test_sync_batch_encode_decode() {
        let mut batch = SyncBatch::with_id(12345);

        // Add entries
        batch.entries.push(SyncEntry::new(
            "nodes:node-1".to_string(),
            SyncMessageType::DeltaSync,
            vec![1, 2, 3],
        ));
        batch.entries.push(SyncEntry::new(
            "beacons:beacon-1".to_string(),
            SyncMessageType::StateSnapshot,
            vec![4, 5, 6, 7],
        ));

        assert_eq!(batch.len(), 2);
        assert_eq!(batch.payload_size(), 7); // 3 + 4

        let encoded = batch.encode();

        // Verify header
        // [8 bytes: batch_id][8 bytes: created_at][1 byte: ttl][4 bytes: entry_count][entries...]
        assert_eq!(u64::from_be_bytes(encoded[0..8].try_into().unwrap()), 12345);
        assert_eq!(encoded[16], DEFAULT_SYNC_BATCH_TTL); // TTL byte
        assert_eq!(u32::from_be_bytes(encoded[17..21].try_into().unwrap()), 2); // 2 entries

        // Decode and verify roundtrip
        let decoded = SyncBatch::decode(&encoded).unwrap();
        assert_eq!(decoded.batch_id, 12345);
        assert_eq!(decoded.entries.len(), 2);

        assert_eq!(decoded.entries[0].doc_key, "nodes:node-1");
        assert_eq!(decoded.entries[0].sync_type, SyncMessageType::DeltaSync);
        assert_eq!(decoded.entries[0].payload, vec![1, 2, 3]);

        assert_eq!(decoded.entries[1].doc_key, "beacons:beacon-1");
        assert_eq!(decoded.entries[1].sync_type, SyncMessageType::StateSnapshot);
        assert_eq!(decoded.entries[1].payload, vec![4, 5, 6, 7]);
    }

    #[test]
    fn test_sync_batch_with_many_entries() {
        let mut batch = SyncBatch::with_id(99999);

        // Add 100 entries
        for i in 0..100 {
            batch.entries.push(SyncEntry::new(
                format!("docs:doc-{}", i),
                SyncMessageType::DeltaSync,
                vec![i as u8; 10],
            ));
        }

        assert_eq!(batch.len(), 100);
        assert_eq!(batch.payload_size(), 1000); // 100 * 10

        let encoded = batch.encode();
        let decoded = SyncBatch::decode(&encoded).unwrap();

        assert_eq!(decoded.batch_id, 99999);
        assert_eq!(decoded.entries.len(), 100);

        // Verify a few entries
        assert_eq!(decoded.entries[0].doc_key, "docs:doc-0");
        assert_eq!(decoded.entries[0].payload, vec![0u8; 10]);

        assert_eq!(decoded.entries[50].doc_key, "docs:doc-50");
        assert_eq!(decoded.entries[50].payload, vec![50u8; 10]);

        assert_eq!(decoded.entries[99].doc_key, "docs:doc-99");
        assert_eq!(decoded.entries[99].payload, vec![99u8; 10]);
    }

    #[test]
    fn test_sync_entry_decode_error_too_short() {
        let result = SyncEntry::decode(&[0, 1, 2]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn test_sync_batch_decode_error_too_short() {
        let result = SyncBatch::decode(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    // === End batch sync tests ===

    // === SyncDirection tests (Issue #438 Phase 3) ===

    #[test]
    fn test_sync_direction_upward() {
        // Node-related collections should sync upward
        assert_eq!(
            SyncDirection::from_doc_key("nodes:node-1"),
            SyncDirection::Upward
        );
        assert_eq!(
            SyncDirection::from_doc_key("beacons:beacon-42"),
            SyncDirection::Upward
        );
        assert_eq!(
            SyncDirection::from_doc_key("platforms:platform-a"),
            SyncDirection::Upward
        );
        assert_eq!(
            SyncDirection::from_doc_key("summaries:cell-1"),
            SyncDirection::Upward
        );
    }

    #[test]
    fn test_sync_direction_downward() {
        // Commands flow from coordinators to executors
        assert_eq!(
            SyncDirection::from_doc_key("commands:cmd-123"),
            SyncDirection::Downward
        );
        assert_eq!(
            SyncDirection::from_doc_key("commands:urgent-456"),
            SyncDirection::Downward
        );
    }

    #[test]
    fn test_sync_direction_lateral() {
        // Cell state syncs between peers
        assert_eq!(
            SyncDirection::from_doc_key("cells:cell-alpha"),
            SyncDirection::Lateral
        );
        assert_eq!(
            SyncDirection::from_doc_key("cells:formation-1"),
            SyncDirection::Lateral
        );
    }

    #[test]
    fn test_sync_direction_broadcast() {
        // Alerts and reports go everywhere
        assert_eq!(
            SyncDirection::from_doc_key("alerts:alert-1"),
            SyncDirection::Broadcast
        );
        assert_eq!(
            SyncDirection::from_doc_key("contact_reports:cr-789"),
            SyncDirection::Broadcast
        );
        assert_eq!(
            SyncDirection::from_doc_key("events:event-1"),
            SyncDirection::Broadcast
        );
        // Unknown collections default to broadcast
        assert_eq!(
            SyncDirection::from_doc_key("unknown:item-1"),
            SyncDirection::Broadcast
        );
        assert_eq!(
            SyncDirection::from_doc_key("custom_collection:x"),
            SyncDirection::Broadcast
        );
    }

    #[test]
    fn test_sync_direction_edge_cases() {
        // Document key without colon - should use whole key as collection
        assert_eq!(SyncDirection::from_doc_key("nodes"), SyncDirection::Upward);
        assert_eq!(
            SyncDirection::from_doc_key("commands"),
            SyncDirection::Downward
        );
        // Empty key defaults to broadcast
        assert_eq!(SyncDirection::from_doc_key(""), SyncDirection::Broadcast);
    }

    #[test]
    fn test_sync_batch_with_entries() {
        let entries = vec![
            SyncEntry::new("nodes:n1".to_string(), SyncMessageType::DeltaSync, vec![1]),
            SyncEntry::new(
                "commands:c1".to_string(),
                SyncMessageType::StateSnapshot,
                vec![2],
            ),
        ];
        let batch = SyncBatch::with_entries(entries);
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.entries[0].doc_key, "nodes:n1");
        assert_eq!(batch.entries[1].doc_key, "commands:c1");
    }

    // === End SyncDirection tests ===

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
