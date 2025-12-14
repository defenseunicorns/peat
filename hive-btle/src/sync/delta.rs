//! Delta Encoder for HIVE-Lite Sync
//!
//! Tracks what state has been sent to each peer and only sends
//! the changes (deltas) since the last sync. This dramatically
//! reduces bandwidth over BLE.

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeMap, format, string::String, string::ToString, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

use super::crdt::{CrdtOperation, Timestamp};
use crate::NodeId;

/// Tracks the sync state with a specific peer
#[derive(Debug, Clone, Default)]
pub struct PeerSyncState {
    /// Last timestamp we synced each key
    #[cfg(feature = "std")]
    last_sent: HashMap<String, Timestamp>,
    #[cfg(not(feature = "std"))]
    last_sent: BTreeMap<String, Timestamp>,

    /// Last timestamp we received from this peer
    pub last_received_timestamp: Timestamp,

    /// Number of successful syncs
    pub sync_count: u32,

    /// Bytes sent to this peer
    pub bytes_sent: u64,

    /// Bytes received from this peer
    pub bytes_received: u64,
}

impl PeerSyncState {
    /// Create new peer sync state
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that we sent a key at a timestamp
    pub fn mark_sent(&mut self, key: &str, timestamp: Timestamp) {
        self.last_sent.insert(key.to_string(), timestamp);
    }

    /// Get the last sent timestamp for a key
    pub fn last_sent_timestamp(&self, key: &str) -> Option<Timestamp> {
        self.last_sent.get(key).copied()
    }

    /// Check if we need to send this key (has newer timestamp)
    pub fn needs_send(&self, key: &str, timestamp: Timestamp) -> bool {
        match self.last_sent.get(key) {
            Some(&last) => timestamp > last,
            None => true,
        }
    }

    /// Clear all tracking (for reconnection)
    pub fn reset(&mut self) {
        self.last_sent.clear();
    }
}

/// Manages delta encoding for all peers
#[derive(Debug)]
pub struct DeltaEncoder {
    /// Our node ID (for future use in operations)
    #[allow(dead_code)]
    node_id: NodeId,

    /// Sync state per peer
    #[cfg(feature = "std")]
    peers: HashMap<u32, PeerSyncState>,
    #[cfg(not(feature = "std"))]
    peers: BTreeMap<u32, PeerSyncState>,

    /// Current state (key -> (value_hash, timestamp))
    #[cfg(feature = "std")]
    current_state: HashMap<String, (u64, Timestamp)>,
    #[cfg(not(feature = "std"))]
    current_state: BTreeMap<String, (u64, Timestamp)>,
}

impl DeltaEncoder {
    /// Create a new delta encoder
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            #[cfg(feature = "std")]
            peers: HashMap::new(),
            #[cfg(not(feature = "std"))]
            peers: BTreeMap::new(),
            #[cfg(feature = "std")]
            current_state: HashMap::new(),
            #[cfg(not(feature = "std"))]
            current_state: BTreeMap::new(),
        }
    }

    /// Register a peer for delta tracking
    pub fn add_peer(&mut self, peer_id: &NodeId) {
        self.peers.entry(peer_id.as_u32()).or_default();
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, peer_id: &NodeId) {
        self.peers.remove(&peer_id.as_u32());
    }

    /// Get peer sync state
    pub fn get_peer_state(&self, peer_id: &NodeId) -> Option<&PeerSyncState> {
        self.peers.get(&peer_id.as_u32())
    }

    /// Get mutable peer sync state
    pub fn get_peer_state_mut(&mut self, peer_id: &NodeId) -> Option<&mut PeerSyncState> {
        self.peers.get_mut(&peer_id.as_u32())
    }

    /// Update our current state with an operation
    pub fn update_state(&mut self, key: &str, value_hash: u64, timestamp: Timestamp) {
        self.current_state
            .insert(key.to_string(), (value_hash, timestamp));
    }

    /// Filter operations to only those that need to be sent to a peer
    pub fn filter_for_peer(
        &self,
        peer_id: &NodeId,
        operations: &[CrdtOperation],
    ) -> Vec<CrdtOperation> {
        let peer_state = match self.peers.get(&peer_id.as_u32()) {
            Some(state) => state,
            None => return operations.to_vec(), // Unknown peer, send all
        };

        operations
            .iter()
            .filter(|op| {
                let (key, timestamp) = Self::operation_key_timestamp(op);
                peer_state.needs_send(&key, timestamp)
            })
            .cloned()
            .collect()
    }

    /// Mark operations as sent to a peer
    pub fn mark_sent(&mut self, peer_id: &NodeId, operations: &[CrdtOperation]) {
        let peer_state = match self.peers.get_mut(&peer_id.as_u32()) {
            Some(state) => state,
            None => return,
        };

        for op in operations {
            let (key, timestamp) = Self::operation_key_timestamp(op);
            peer_state.mark_sent(&key, timestamp);
        }
    }

    /// Record bytes sent to peer
    pub fn record_sent(&mut self, peer_id: &NodeId, bytes: usize) {
        if let Some(state) = self.peers.get_mut(&peer_id.as_u32()) {
            state.bytes_sent += bytes as u64;
            state.sync_count += 1;
        }
    }

    /// Record bytes received from peer
    pub fn record_received(&mut self, peer_id: &NodeId, bytes: usize, timestamp: Timestamp) {
        if let Some(state) = self.peers.get_mut(&peer_id.as_u32()) {
            state.bytes_received += bytes as u64;
            state.last_received_timestamp = timestamp;
        }
    }

    /// Reset sync state for a peer (e.g., on reconnection)
    pub fn reset_peer(&mut self, peer_id: &NodeId) {
        if let Some(state) = self.peers.get_mut(&peer_id.as_u32()) {
            state.reset();
        }
    }

    /// Get sync statistics
    pub fn stats(&self) -> DeltaStats {
        let mut total_sent = 0u64;
        let mut total_received = 0u64;
        let mut total_syncs = 0u32;

        for state in self.peers.values() {
            total_sent += state.bytes_sent;
            total_received += state.bytes_received;
            total_syncs += state.sync_count;
        }

        DeltaStats {
            peer_count: self.peers.len(),
            total_bytes_sent: total_sent,
            total_bytes_received: total_received,
            total_syncs,
            tracked_keys: self.current_state.len(),
        }
    }

    /// Extract key and timestamp from an operation
    fn operation_key_timestamp(op: &CrdtOperation) -> (String, Timestamp) {
        match op {
            CrdtOperation::UpdatePosition {
                node_id, timestamp, ..
            } => (format!("pos:{}", node_id), *timestamp),
            CrdtOperation::UpdateHealth {
                node_id, timestamp, ..
            } => (format!("health:{}", node_id), *timestamp),
            CrdtOperation::IncrementCounter {
                counter_id,
                node_id,
                ..
            } => {
                // Counters always need to be synced (they accumulate)
                (format!("counter:{}:{}", counter_id, node_id), u64::MAX)
            }
            CrdtOperation::UpdateRegister {
                key,
                timestamp,
                node_id,
                ..
            } => (format!("reg:{}:{}", key, node_id), *timestamp),
        }
    }
}

/// Statistics about delta encoding
#[derive(Debug, Clone, Default)]
pub struct DeltaStats {
    /// Number of peers being tracked
    pub peer_count: usize,
    /// Total bytes sent across all peers
    pub total_bytes_sent: u64,
    /// Total bytes received across all peers
    pub total_bytes_received: u64,
    /// Total number of sync operations
    pub total_syncs: u32,
    /// Number of keys being tracked
    pub tracked_keys: usize,
}

/// Vector clock for causality tracking
#[derive(Debug, Clone, Default)]
pub struct VectorClock {
    /// Per-node logical clocks
    #[cfg(feature = "std")]
    clocks: HashMap<u32, u64>,
    #[cfg(not(feature = "std"))]
    clocks: BTreeMap<u32, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the clock for a node
    pub fn increment(&mut self, node_id: &NodeId) -> u64 {
        let clock = self.clocks.entry(node_id.as_u32()).or_insert(0);
        *clock += 1;
        *clock
    }

    /// Get the clock value for a node
    pub fn get(&self, node_id: &NodeId) -> u64 {
        self.clocks.get(&node_id.as_u32()).copied().unwrap_or(0)
    }

    /// Update clock for a node (take max)
    pub fn update(&mut self, node_id: &NodeId, value: u64) {
        let clock = self.clocks.entry(node_id.as_u32()).or_insert(0);
        *clock = (*clock).max(value);
    }

    /// Merge with another vector clock (take component-wise max)
    pub fn merge(&mut self, other: &VectorClock) {
        for (&node_id, &value) in &other.clocks {
            let clock = self.clocks.entry(node_id).or_insert(0);
            *clock = (*clock).max(value);
        }
    }

    /// Check if this clock happens-before another
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut dominated = false;

        // All our clocks must be <= other's
        for (&node_id, &our_val) in &self.clocks {
            let their_val = other.clocks.get(&node_id).copied().unwrap_or(0);
            if our_val > their_val {
                return false;
            }
            if our_val < their_val {
                dominated = true;
            }
        }

        // Check for clocks they have that we don't
        for (&node_id, &their_val) in &other.clocks {
            if !self.clocks.contains_key(&node_id) && their_val > 0 {
                dominated = true;
            }
        }

        dominated
    }

    /// Check if clocks are concurrent (neither happens-before the other)
    pub fn concurrent_with(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self)
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.clocks.len() * 12);
        buf.extend_from_slice(&(self.clocks.len() as u32).to_le_bytes());
        for (&node_id, &value) in &self.clocks {
            buf.extend_from_slice(&node_id.to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
        }
        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + count * 12 {
            return None;
        }

        #[cfg(feature = "std")]
        let mut clocks = HashMap::with_capacity(count);
        #[cfg(not(feature = "std"))]
        let mut clocks = BTreeMap::new();

        let mut offset = 4;
        for _ in 0..count {
            let node_id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let value = u64::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);
            clocks.insert(node_id, value);
            offset += 12;
        }

        Some(Self { clocks })
    }
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
    fn test_peer_sync_state() {
        let mut state = PeerSyncState::new();

        assert!(state.needs_send("key1", 100));

        state.mark_sent("key1", 100);

        assert!(!state.needs_send("key1", 100));
        assert!(!state.needs_send("key1", 50));
        assert!(state.needs_send("key1", 101));
    }

    #[test]
    fn test_delta_encoder_filter() {
        let mut encoder = DeltaEncoder::new(NodeId::new(1));
        let peer = NodeId::new(2);

        encoder.add_peer(&peer);

        let ops = vec![make_position_op(1, 100), make_position_op(2, 200)];

        // First time, all ops should be sent
        let filtered = encoder.filter_for_peer(&peer, &ops);
        assert_eq!(filtered.len(), 2);

        // Mark as sent
        encoder.mark_sent(&peer, &filtered);

        // Same ops should not be sent again
        let filtered2 = encoder.filter_for_peer(&peer, &ops);
        assert_eq!(filtered2.len(), 0);

        // Newer ops should be sent
        let new_ops = vec![make_position_op(1, 101)];
        let filtered3 = encoder.filter_for_peer(&peer, &new_ops);
        assert_eq!(filtered3.len(), 1);
    }

    #[test]
    fn test_delta_encoder_stats() {
        let mut encoder = DeltaEncoder::new(NodeId::new(1));

        encoder.add_peer(&NodeId::new(2));
        encoder.add_peer(&NodeId::new(3));

        encoder.record_sent(&NodeId::new(2), 100);
        encoder.record_sent(&NodeId::new(3), 50);
        encoder.record_received(&NodeId::new(2), 75, 1000);

        let stats = encoder.stats();
        assert_eq!(stats.peer_count, 2);
        assert_eq!(stats.total_bytes_sent, 150);
        assert_eq!(stats.total_bytes_received, 75);
        assert_eq!(stats.total_syncs, 2);
    }

    #[test]
    fn test_vector_clock_increment() {
        let mut clock = VectorClock::new();
        let node = NodeId::new(1);

        assert_eq!(clock.get(&node), 0);

        clock.increment(&node);
        assert_eq!(clock.get(&node), 1);

        clock.increment(&node);
        assert_eq!(clock.get(&node), 2);
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();

        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        clock1.update(&node1, 5);
        clock1.update(&node2, 3);

        clock2.update(&node1, 3);
        clock2.update(&node2, 7);

        clock1.merge(&clock2);

        assert_eq!(clock1.get(&node1), 5); // max(5, 3)
        assert_eq!(clock1.get(&node2), 7); // max(3, 7)
    }

    #[test]
    fn test_vector_clock_happens_before() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();

        let node = NodeId::new(1);

        clock1.update(&node, 1);
        clock2.update(&node, 2);

        assert!(clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
    }

    #[test]
    fn test_vector_clock_concurrent() {
        let mut clock1 = VectorClock::new();
        let mut clock2 = VectorClock::new();

        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);

        clock1.update(&node1, 2);
        clock1.update(&node2, 1);

        clock2.update(&node1, 1);
        clock2.update(&node2, 2);

        // Neither dominates the other
        assert!(clock1.concurrent_with(&clock2));
    }

    #[test]
    fn test_vector_clock_encode_decode() {
        let mut clock = VectorClock::new();
        clock.update(&NodeId::new(1), 5);
        clock.update(&NodeId::new(2), 10);

        let encoded = clock.encode();
        let decoded = VectorClock::decode(&encoded).unwrap();

        assert_eq!(decoded.get(&NodeId::new(1)), 5);
        assert_eq!(decoded.get(&NodeId::new(2)), 10);
    }

    #[test]
    fn test_reset_peer() {
        let mut encoder = DeltaEncoder::new(NodeId::new(1));
        let peer = NodeId::new(2);

        encoder.add_peer(&peer);
        encoder.mark_sent(&peer, &[make_position_op(1, 100)]);

        // After reset, should need to send again
        encoder.reset_peer(&peer);

        let ops = vec![make_position_op(1, 100)];
        let filtered = encoder.filter_for_peer(&peer, &ops);
        assert_eq!(filtered.len(), 1);
    }
}
