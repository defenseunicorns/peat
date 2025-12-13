//! GATT Sync Protocol for HIVE-Lite
//!
//! Coordinates batching, delta encoding, and chunked transfer over GATT
//! characteristics for efficient BLE sync.

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeMap, collections::VecDeque, vec::Vec};
#[cfg(feature = "std")]
use std::collections::{HashMap, VecDeque};

use super::batch::{BatchAccumulator, BatchConfig, OperationBatch};
use super::crdt::CrdtOperation;
use super::delta::{DeltaEncoder, VectorClock};
use crate::NodeId;

/// Default MTU for BLE
pub const DEFAULT_MTU: usize = 23;

/// Maximum MTU for BLE 5.0+
pub const MAX_MTU: usize = 517;

/// Header size for chunks
pub const CHUNK_HEADER_SIZE: usize = 8;

/// Chunk header for multi-MTU messages
#[derive(Debug, Clone, Copy)]
pub struct ChunkHeader {
    /// Unique message ID
    pub message_id: u32,
    /// Index of this chunk (0-based)
    pub chunk_index: u16,
    /// Total number of chunks
    pub total_chunks: u16,
}

impl ChunkHeader {
    /// Encode header to bytes
    pub fn encode(&self) -> [u8; CHUNK_HEADER_SIZE] {
        let mut buf = [0u8; CHUNK_HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.message_id.to_le_bytes());
        buf[4..6].copy_from_slice(&self.chunk_index.to_le_bytes());
        buf[6..8].copy_from_slice(&self.total_chunks.to_le_bytes());
        buf
    }

    /// Decode header from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < CHUNK_HEADER_SIZE {
            return None;
        }
        Some(Self {
            message_id: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            chunk_index: u16::from_le_bytes([data[4], data[5]]),
            total_chunks: u16::from_le_bytes([data[6], data[7]]),
        })
    }
}

/// A chunk of data to send
#[derive(Debug, Clone)]
pub struct SyncChunk {
    /// Header
    pub header: ChunkHeader,
    /// Payload data
    pub payload: Vec<u8>,
}

impl SyncChunk {
    /// Encode chunk to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CHUNK_HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&self.header.encode());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode chunk from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        let header = ChunkHeader::decode(data)?;
        let payload = data[CHUNK_HEADER_SIZE..].to_vec();
        Some(Self { header, payload })
    }

    /// Get total encoded size
    pub fn encoded_size(&self) -> usize {
        CHUNK_HEADER_SIZE + self.payload.len()
    }
}

/// Reassembles chunks into complete messages
#[derive(Debug)]
pub struct ChunkReassembler {
    /// Partial messages being assembled
    #[cfg(feature = "std")]
    partials: HashMap<u32, PartialMessage>,
    #[cfg(not(feature = "std"))]
    partials: BTreeMap<u32, PartialMessage>,

    /// Maximum number of partial messages to track (for future use)
    #[allow(dead_code)]
    max_partials: usize,

    /// Timeout for partial messages (ms)
    partial_timeout_ms: u64,
}

/// A message being reassembled
#[derive(Debug, Clone)]
struct PartialMessage {
    /// Total expected chunks
    total_chunks: u16,
    /// Received chunks (index -> data)
    #[cfg(feature = "std")]
    chunks: HashMap<u16, Vec<u8>>,
    #[cfg(not(feature = "std"))]
    chunks: BTreeMap<u16, Vec<u8>>,
    /// Time first chunk was received
    started_at: u64,
}

impl ChunkReassembler {
    /// Create a new reassembler
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "std")]
            partials: HashMap::new(),
            #[cfg(not(feature = "std"))]
            partials: BTreeMap::new(),
            max_partials: 8,
            partial_timeout_ms: 30_000,
        }
    }

    /// Process a received chunk
    ///
    /// Returns the complete message if all chunks received
    pub fn process(&mut self, chunk: SyncChunk, current_time_ms: u64) -> Option<Vec<u8>> {
        let msg_id = chunk.header.message_id;

        // Single-chunk message
        if chunk.header.total_chunks == 1 {
            return Some(chunk.payload);
        }

        // Get or create partial
        let partial = self
            .partials
            .entry(msg_id)
            .or_insert_with(|| PartialMessage {
                total_chunks: chunk.header.total_chunks,
                #[cfg(feature = "std")]
                chunks: HashMap::new(),
                #[cfg(not(feature = "std"))]
                chunks: BTreeMap::new(),
                started_at: current_time_ms,
            });

        // Insert chunk
        partial
            .chunks
            .insert(chunk.header.chunk_index, chunk.payload);

        // Check if complete
        if partial.chunks.len() == partial.total_chunks as usize {
            let partial = self.partials.remove(&msg_id)?;

            // Reassemble in order
            let mut result = Vec::new();
            for i in 0..partial.total_chunks {
                if let Some(data) = partial.chunks.get(&i) {
                    result.extend_from_slice(data);
                } else {
                    // Missing chunk - shouldn't happen
                    return None;
                }
            }
            return Some(result);
        }

        None
    }

    /// Clean up timed-out partial messages
    pub fn cleanup(&mut self, current_time_ms: u64) {
        self.partials
            .retain(|_, partial| current_time_ms - partial.started_at < self.partial_timeout_ms);
    }

    /// Get number of messages being assembled
    pub fn pending_count(&self) -> usize {
        self.partials.len()
    }
}

impl Default for ChunkReassembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Split data into MTU-sized chunks
pub fn chunk_data(data: &[u8], mtu: usize, message_id: u32) -> Vec<SyncChunk> {
    let payload_size = mtu.saturating_sub(CHUNK_HEADER_SIZE);
    if payload_size == 0 {
        return Vec::new();
    }

    let total_chunks = (data.len() + payload_size - 1) / payload_size;
    let total_chunks = total_chunks.max(1) as u16;

    let mut chunks = Vec::with_capacity(total_chunks as usize);

    for (i, chunk_data) in data.chunks(payload_size).enumerate() {
        chunks.push(SyncChunk {
            header: ChunkHeader {
                message_id,
                chunk_index: i as u16,
                total_chunks,
            },
            payload: chunk_data.to_vec(),
        });
    }

    // Handle empty data
    if chunks.is_empty() {
        chunks.push(SyncChunk {
            header: ChunkHeader {
                message_id,
                chunk_index: 0,
                total_chunks: 1,
            },
            payload: Vec::new(),
        });
    }

    chunks
}

/// State of the sync protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    /// Idle, not syncing
    #[default]
    Idle,
    /// Sending data to peer
    Sending,
    /// Receiving data from peer
    Receiving,
    /// Waiting for acknowledgment
    WaitingAck,
}

/// Configuration for the sync protocol
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Negotiated MTU
    pub mtu: usize,
    /// Batch configuration
    pub batch: BatchConfig,
    /// Sync interval in milliseconds
    pub sync_interval_ms: u64,
    /// Enable delta encoding
    pub enable_delta: bool,
    /// Maximum retries per chunk
    pub max_retries: u8,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mtu: DEFAULT_MTU,
            batch: BatchConfig::default(),
            sync_interval_ms: 5000,
            enable_delta: true,
            max_retries: 3,
        }
    }
}

impl SyncConfig {
    /// Config for low-power operation
    pub fn low_power() -> Self {
        Self {
            mtu: DEFAULT_MTU,
            batch: BatchConfig::low_power(),
            sync_interval_ms: 30_000,
            enable_delta: true,
            max_retries: 2,
        }
    }

    /// Config for responsive operation
    pub fn responsive() -> Self {
        Self {
            mtu: MAX_MTU,
            batch: BatchConfig::responsive(),
            sync_interval_ms: 1000,
            enable_delta: true,
            max_retries: 3,
        }
    }
}

/// GATT Sync Protocol coordinator
///
/// Ties together batching, delta encoding, and chunked transfer
/// for efficient CRDT sync over BLE.
pub struct GattSyncProtocol {
    /// Our node ID
    node_id: NodeId,

    /// Configuration
    config: SyncConfig,

    /// Current state
    state: SyncState,

    /// Batch accumulator
    batch: BatchAccumulator,

    /// Delta encoder
    delta: DeltaEncoder,

    /// Vector clock
    vector_clock: VectorClock,

    /// Outgoing chunk queue
    tx_queue: VecDeque<SyncChunk>,

    /// Chunk reassembler for incoming data
    rx_reassembler: ChunkReassembler,

    /// Next message ID
    next_message_id: u32,

    /// Current time (set externally)
    current_time_ms: u64,

    /// Last sync time
    last_sync_time_ms: u64,
}

impl GattSyncProtocol {
    /// Create a new sync protocol instance
    pub fn new(node_id: NodeId, config: SyncConfig) -> Self {
        Self {
            node_id: node_id.clone(),
            batch: BatchAccumulator::new(config.batch.clone()),
            delta: DeltaEncoder::new(node_id),
            vector_clock: VectorClock::new(),
            config,
            state: SyncState::Idle,
            tx_queue: VecDeque::new(),
            rx_reassembler: ChunkReassembler::new(),
            next_message_id: 1,
            current_time_ms: 0,
            last_sync_time_ms: 0,
        }
    }

    /// Create with default config
    pub fn with_defaults(node_id: NodeId) -> Self {
        Self::new(node_id, SyncConfig::default())
    }

    /// Set the current time
    pub fn set_time(&mut self, time_ms: u64) {
        self.current_time_ms = time_ms;
    }

    /// Set the MTU (after negotiation)
    pub fn set_mtu(&mut self, mtu: usize) {
        self.config.mtu = mtu;
    }

    /// Get current state
    pub fn state(&self) -> SyncState {
        self.state
    }

    /// Get the vector clock
    pub fn vector_clock(&self) -> &VectorClock {
        &self.vector_clock
    }

    /// Add a peer for sync
    pub fn add_peer(&mut self, peer_id: &NodeId) {
        self.delta.add_peer(peer_id);
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, peer_id: &NodeId) {
        self.delta.remove_peer(peer_id);
    }

    /// Queue a CRDT operation for sync
    pub fn queue_operation(&mut self, op: CrdtOperation) -> bool {
        // Update vector clock
        self.vector_clock.increment(&self.node_id);

        // Add to batch
        self.batch.add(op, self.current_time_ms)
    }

    /// Check if we should sync now
    pub fn should_sync(&self) -> bool {
        self.batch.should_flush(self.current_time_ms)
    }

    /// Prepare a sync to a peer
    ///
    /// Returns the chunks to send
    pub fn prepare_sync(&mut self, peer_id: &NodeId) -> Vec<SyncChunk> {
        // Flush the batch
        let batch = match self.batch.flush(self.current_time_ms) {
            Some(b) => b,
            None => return Vec::new(),
        };

        // Filter with delta encoding
        let operations = if self.config.enable_delta {
            self.delta.filter_for_peer(peer_id, &batch.operations)
        } else {
            batch.operations.clone()
        };

        if operations.is_empty() {
            return Vec::new();
        }

        // Create a batch with filtered operations
        let filtered_batch = OperationBatch {
            operations: operations.clone(),
            total_bytes: operations.iter().map(|o| o.size()).sum(),
            created_at: batch.created_at,
        };

        // Encode the batch
        let encoded = filtered_batch.encode();

        // Chunk it
        let msg_id = self.next_message_id;
        self.next_message_id = self.next_message_id.wrapping_add(1);

        let chunks = chunk_data(&encoded, self.config.mtu, msg_id);

        // Mark as sent (after we have the filtered list)
        self.delta.mark_sent(peer_id, &operations);
        self.delta.record_sent(peer_id, encoded.len());

        self.state = SyncState::Sending;
        self.last_sync_time_ms = self.current_time_ms;

        chunks
    }

    /// Get next chunk to send (if any)
    pub fn next_tx_chunk(&mut self) -> Option<SyncChunk> {
        self.tx_queue.pop_front()
    }

    /// Queue chunks for sending
    pub fn queue_chunks(&mut self, chunks: Vec<SyncChunk>) {
        self.tx_queue.extend(chunks);
    }

    /// Check if there are chunks to send
    pub fn has_pending_tx(&self) -> bool {
        !self.tx_queue.is_empty()
    }

    /// Process a received chunk
    ///
    /// Returns decoded operations if message is complete
    pub fn process_received(
        &mut self,
        chunk: SyncChunk,
        peer_id: &NodeId,
    ) -> Option<Vec<CrdtOperation>> {
        self.state = SyncState::Receiving;

        // Reassemble
        let complete = self.rx_reassembler.process(chunk, self.current_time_ms)?;

        // Decode batch
        let batch = OperationBatch::decode(&complete)?;

        // Record stats
        self.delta
            .record_received(peer_id, complete.len(), self.current_time_ms);

        // Update vector clock with received operations
        for op in &batch.operations {
            let timestamp = match op {
                CrdtOperation::UpdatePosition { timestamp, .. } => *timestamp,
                CrdtOperation::UpdateHealth { timestamp, .. } => *timestamp,
                CrdtOperation::UpdateRegister { timestamp, .. } => *timestamp,
                CrdtOperation::IncrementCounter { .. } => 0,
            };
            if timestamp > 0 {
                self.vector_clock.update(peer_id, timestamp);
            }
        }

        self.state = SyncState::Idle;
        Some(batch.operations)
    }

    /// Acknowledge completion of send
    pub fn ack_send(&mut self) {
        if self.tx_queue.is_empty() {
            self.state = SyncState::Idle;
        }
    }

    /// Reset protocol state (e.g., on reconnection)
    pub fn reset(&mut self) {
        self.state = SyncState::Idle;
        self.tx_queue.clear();
        self.rx_reassembler = ChunkReassembler::new();
    }

    /// Reset sync state for a specific peer
    pub fn reset_peer(&mut self, peer_id: &NodeId) {
        self.delta.reset_peer(peer_id);
    }

    /// Run periodic maintenance
    pub fn tick(&mut self) {
        self.rx_reassembler.cleanup(self.current_time_ms);
    }

    /// Get sync statistics
    pub fn stats(&self) -> SyncStats {
        let delta_stats = self.delta.stats();
        SyncStats {
            bytes_sent: delta_stats.total_bytes_sent,
            bytes_received: delta_stats.total_bytes_received,
            syncs_completed: delta_stats.total_syncs,
            pending_operations: self.batch.pending_count(),
            pending_tx_chunks: self.tx_queue.len(),
            pending_rx_messages: self.rx_reassembler.pending_count(),
        }
    }
}

/// Sync statistics
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Number of completed syncs
    pub syncs_completed: u32,
    /// Pending operations in batch
    pub pending_operations: usize,
    /// Pending TX chunks
    pub pending_tx_chunks: usize,
    /// Pending RX messages being reassembled
    pub pending_rx_messages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crdt::Position;

    fn make_position_op(node_id: u32, timestamp: u64) -> CrdtOperation {
        CrdtOperation::UpdatePosition {
            node_id: NodeId::new(node_id),
            position: Position::new(37.0, -122.0),
            timestamp,
        }
    }

    #[test]
    fn test_chunk_header_encode_decode() {
        let header = ChunkHeader {
            message_id: 0x12345678,
            chunk_index: 5,
            total_chunks: 10,
        };

        let encoded = header.encode();
        let decoded = ChunkHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.message_id, 0x12345678);
        assert_eq!(decoded.chunk_index, 5);
        assert_eq!(decoded.total_chunks, 10);
    }

    #[test]
    fn test_chunk_data_single() {
        let data = vec![1, 2, 3, 4, 5];
        let chunks = chunk_data(&data, 100, 1);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].header.total_chunks, 1);
        assert_eq!(chunks[0].payload, data);
    }

    #[test]
    fn test_chunk_data_multiple() {
        let data = vec![0u8; 100];
        let mtu = 20; // 8 header + 12 payload
        let chunks = chunk_data(&data, mtu, 1);

        // 100 bytes / 12 per chunk = 9 chunks
        assert_eq!(chunks.len(), 9);
        assert_eq!(chunks[0].header.total_chunks, 9);

        // Verify payload sizes
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.header.chunk_index, i as u16);
            if i < 8 {
                assert_eq!(chunk.payload.len(), 12);
            } else {
                assert_eq!(chunk.payload.len(), 4); // Last chunk
            }
        }
    }

    #[test]
    fn test_chunk_reassembler_single() {
        let mut reassembler = ChunkReassembler::new();

        let chunk = SyncChunk {
            header: ChunkHeader {
                message_id: 1,
                chunk_index: 0,
                total_chunks: 1,
            },
            payload: vec![1, 2, 3],
        };

        let result = reassembler.process(chunk, 0).unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_chunk_reassembler_multiple() {
        let mut reassembler = ChunkReassembler::new();

        // Send chunks out of order
        let chunk2 = SyncChunk {
            header: ChunkHeader {
                message_id: 1,
                chunk_index: 1,
                total_chunks: 3,
            },
            payload: vec![4, 5, 6],
        };

        let chunk1 = SyncChunk {
            header: ChunkHeader {
                message_id: 1,
                chunk_index: 0,
                total_chunks: 3,
            },
            payload: vec![1, 2, 3],
        };

        let chunk3 = SyncChunk {
            header: ChunkHeader {
                message_id: 1,
                chunk_index: 2,
                total_chunks: 3,
            },
            payload: vec![7, 8, 9],
        };

        assert!(reassembler.process(chunk2, 0).is_none());
        assert!(reassembler.process(chunk1, 0).is_none());

        let result = reassembler.process(chunk3, 0).unwrap();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_sync_protocol_basic() {
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        let mut proto1 = GattSyncProtocol::with_defaults(node1.clone());
        proto1.add_peer(&node2);
        proto1.set_mtu(100);

        // Queue some operations
        proto1.queue_operation(make_position_op(1, 1000));
        proto1.queue_operation(make_position_op(1, 1001));

        // Force batch to be ready
        proto1.set_time(10000);

        // Prepare sync
        let chunks = proto1.prepare_sync(&node2);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_sync_protocol_round_trip() {
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        let mut proto1 = GattSyncProtocol::with_defaults(node1.clone());
        let mut proto2 = GattSyncProtocol::with_defaults(node2.clone());

        proto1.add_peer(&node2);
        proto2.add_peer(&node1);

        proto1.set_mtu(100);
        proto2.set_mtu(100);

        // Node 1 queues operation
        proto1.queue_operation(make_position_op(1, 1000));
        proto1.set_time(10000);

        // Node 1 prepares sync
        let chunks = proto1.prepare_sync(&node2);

        // Node 2 receives chunks
        let mut ops = None;
        for chunk in chunks {
            ops = proto2.process_received(chunk, &node1);
        }

        // Verify operation received
        let ops = ops.unwrap();
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn test_sync_config_profiles() {
        let low_power = SyncConfig::low_power();
        assert_eq!(low_power.sync_interval_ms, 30_000);

        let responsive = SyncConfig::responsive();
        assert_eq!(responsive.sync_interval_ms, 1000);
    }

    #[test]
    fn test_sync_stats() {
        let proto = GattSyncProtocol::with_defaults(NodeId::new(1));
        let stats = proto.stats();

        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.pending_operations, 0);
    }
}
