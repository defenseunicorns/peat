//! Grow-only Counter (G-Counter)
//!
//! A CRDT counter that can only be incremented, never decremented.
//! Useful for counting events like button presses, detections, etc.

use super::{CrdtError, LiteCrdt};
use heapless::FnvIndexMap;

/// Maximum number of nodes that can contribute to the counter
const MAX_NODES: usize = 16;

/// Grow-only Counter
///
/// Each node maintains its own count, and the total is the sum of all counts.
/// This ensures that concurrent increments from different nodes are all counted.
#[derive(Debug, Clone)]
pub struct GCounter {
    /// Map from node_id to that node's count
    counts: FnvIndexMap<u32, u64, MAX_NODES>,
    /// This node's ID (for local increments)
    local_node_id: u32,
}

impl GCounter {
    /// Create a new G-Counter for the given node
    pub fn new(local_node_id: u32) -> Self {
        Self {
            counts: FnvIndexMap::new(),
            local_node_id,
        }
    }

    /// Increment the counter by 1
    pub fn increment(&mut self) {
        self.increment_by(1);
    }

    /// Increment the counter by a specific amount
    pub fn increment_by(&mut self, amount: u64) {
        let current = self.counts.get(&self.local_node_id).copied().unwrap_or(0);
        // If map is full and this node isn't in it, we can't add
        if self.counts.insert(self.local_node_id, current + amount).is_err() {
            // Map full, try to update existing
            if let Some(v) = self.counts.get_mut(&self.local_node_id) {
                *v += amount;
            }
            // Otherwise silently fail (shouldn't happen if local node is already in map)
        }
    }

    /// Get the total count across all nodes
    pub fn count(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Get the count for a specific node
    pub fn count_for_node(&self, node_id: u32) -> u64 {
        self.counts.get(&node_id).copied().unwrap_or(0)
    }

    /// Get the number of contributing nodes
    pub fn num_nodes(&self) -> usize {
        self.counts.len()
    }
}

impl Default for GCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Operation for G-Counter (increment from a specific node)
#[derive(Debug, Clone)]
pub struct GCounterOp {
    pub node_id: u32,
    pub new_count: u64,
}

impl LiteCrdt for GCounter {
    type Op = GCounterOp;
    type Value = u64;

    fn apply(&mut self, op: &Self::Op) {
        // Only apply if it increases the count for that node
        let current = self.counts.get(&op.node_id).copied().unwrap_or(0);
        if op.new_count > current {
            let _ = self.counts.insert(op.node_id, op.new_count);
        }
    }

    fn merge(&mut self, other: &Self) {
        // Take the max count for each node
        for (&node_id, &count) in other.counts.iter() {
            let current = self.counts.get(&node_id).copied().unwrap_or(0);
            if count > current {
                let _ = self.counts.insert(node_id, count);
            }
        }
    }

    fn value(&self) -> Self::Value {
        self.count()
    }

    fn encode(&self, buf: &mut [u8]) -> Result<usize, CrdtError> {
        // Format: [local_node_id:4][num_entries:2][entries:N*12]
        // Each entry: [node_id:4][count:8]
        let num_entries = self.counts.len();
        let total_len = 4 + 2 + (num_entries * 12);

        if buf.len() < total_len {
            return Err(CrdtError::BufferTooSmall);
        }

        buf[0..4].copy_from_slice(&self.local_node_id.to_le_bytes());
        buf[4..6].copy_from_slice(&(num_entries as u16).to_le_bytes());

        let mut offset = 6;
        for (&node_id, &count) in self.counts.iter() {
            buf[offset..offset + 4].copy_from_slice(&node_id.to_le_bytes());
            buf[offset + 4..offset + 12].copy_from_slice(&count.to_le_bytes());
            offset += 12;
        }

        Ok(total_len)
    }

    fn decode(buf: &[u8]) -> Result<Self, CrdtError> {
        if buf.len() < 6 {
            return Err(CrdtError::InvalidData);
        }

        let local_node_id = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let num_entries = u16::from_le_bytes(buf[4..6].try_into().unwrap()) as usize;

        if buf.len() < 6 + (num_entries * 12) {
            return Err(CrdtError::InvalidData);
        }

        let mut counter = GCounter::new(local_node_id);
        let mut offset = 6;

        for _ in 0..num_entries {
            let node_id = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap());
            let count = u64::from_le_bytes(buf[offset + 4..offset + 12].try_into().unwrap());
            let _ = counter.counts.insert(node_id, count);
            offset += 12;
        }

        Ok(counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcounter_increment() {
        let mut counter = GCounter::new(1);
        assert_eq!(counter.count(), 0);

        counter.increment();
        assert_eq!(counter.count(), 1);

        counter.increment_by(5);
        assert_eq!(counter.count(), 6);
    }

    #[test]
    fn test_gcounter_merge() {
        let mut counter1 = GCounter::new(1);
        counter1.increment_by(10);

        let mut counter2 = GCounter::new(2);
        counter2.increment_by(20);

        counter1.merge(&counter2);
        assert_eq!(counter1.count(), 30); // 10 + 20
        assert_eq!(counter1.count_for_node(1), 10);
        assert_eq!(counter1.count_for_node(2), 20);
    }

    #[test]
    fn test_gcounter_merge_takes_max() {
        let mut counter1 = GCounter::new(1);
        counter1.increment_by(10);

        let mut counter2 = GCounter::new(1); // Same node ID
        counter2.increment_by(5); // Lower count

        counter1.merge(&counter2);
        assert_eq!(counter1.count(), 10); // Keeps the max (10)
    }

    #[test]
    fn test_gcounter_encode_decode() {
        let mut counter = GCounter::new(42);
        counter.increment_by(100);

        let mut buf = [0u8; 256];
        let len = counter.encode(&mut buf).unwrap();

        let decoded = GCounter::decode(&buf[..len]).unwrap();
        assert_eq!(decoded.count(), 100);
        assert_eq!(decoded.count_for_node(42), 100);
    }
}
