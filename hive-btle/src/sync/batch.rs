//! Batch Accumulator for HIVE-Lite Sync
//!
//! Collects CRDT operations over a configurable time window or size limit,
//! then emits them as a batch for efficient BLE transmission.
//!
//! This reduces radio activity and power consumption on constrained devices.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::crdt::CrdtOperation;

/// Configuration for batch accumulation
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum time to wait before sending (milliseconds)
    pub max_wait_ms: u64,
    /// Maximum bytes to accumulate before sending
    pub max_bytes: usize,
    /// Maximum number of operations to accumulate
    pub max_operations: usize,
    /// Minimum time between syncs (milliseconds)
    pub min_interval_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_wait_ms: 5000,     // 5 seconds
            max_bytes: 512,        // Half a typical MTU-extended packet
            max_operations: 20,    // Reasonable batch size
            min_interval_ms: 1000, // At most 1 sync/second
        }
    }
}

impl BatchConfig {
    /// Create a config optimized for power efficiency
    pub fn low_power() -> Self {
        Self {
            max_wait_ms: 30_000, // 30 seconds
            max_bytes: 1024,
            max_operations: 50,
            min_interval_ms: 10_000, // At most 1 sync every 10 seconds
        }
    }

    /// Create a config for more responsive sync
    pub fn responsive() -> Self {
        Self {
            max_wait_ms: 1000, // 1 second
            max_bytes: 256,
            max_operations: 10,
            min_interval_ms: 500,
        }
    }
}

/// Batch of operations ready to send
#[derive(Debug, Clone)]
pub struct OperationBatch {
    /// Operations in this batch
    pub operations: Vec<CrdtOperation>,
    /// Total size in bytes
    pub total_bytes: usize,
    /// Timestamp when batch was created
    pub created_at: u64,
}

impl OperationBatch {
    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Get number of operations
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Encode all operations to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.total_bytes + 4);
        // Operation count
        buf.extend_from_slice(&(self.operations.len() as u16).to_le_bytes());
        // Encode each operation
        for op in &self.operations {
            let encoded = op.encode();
            buf.extend_from_slice(&(encoded.len() as u16).to_le_bytes());
            buf.extend_from_slice(&encoded);
        }
        buf
    }

    /// Decode operations from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }

        let op_count = u16::from_le_bytes([data[0], data[1]]) as usize;
        let mut operations = Vec::with_capacity(op_count);
        let mut offset = 2;
        let mut total_bytes = 0;

        for _ in 0..op_count {
            if offset + 2 > data.len() {
                return None;
            }
            let op_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if offset + op_len > data.len() {
                return None;
            }
            let op = CrdtOperation::decode(&data[offset..offset + op_len])?;
            total_bytes += op.size();
            operations.push(op);
            offset += op_len;
        }

        Some(Self {
            operations,
            total_bytes,
            created_at: 0,
        })
    }
}

/// Accumulates CRDT operations for batched transmission
#[derive(Debug)]
pub struct BatchAccumulator {
    /// Configuration
    config: BatchConfig,
    /// Pending operations
    pending: Vec<CrdtOperation>,
    /// Accumulated bytes
    bytes_accumulated: usize,
    /// Time of first pending operation
    first_pending_time: Option<u64>,
    /// Time of last sync
    last_sync_time: u64,
}

impl BatchAccumulator {
    /// Create a new accumulator with the given config
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            pending: Vec::new(),
            bytes_accumulated: 0,
            first_pending_time: None,
            last_sync_time: 0,
        }
    }

    /// Create with default config
    pub fn with_defaults() -> Self {
        Self::new(BatchConfig::default())
    }

    /// Add an operation to the batch
    ///
    /// Returns true if the operation was added, false if batch is full
    pub fn add(&mut self, op: CrdtOperation, current_time_ms: u64) -> bool {
        let op_size = op.size();

        // Check if we have room
        if self.bytes_accumulated + op_size > self.config.max_bytes && !self.pending.is_empty() {
            return false;
        }
        if self.pending.len() >= self.config.max_operations {
            return false;
        }

        // Record first pending time
        if self.first_pending_time.is_none() {
            self.first_pending_time = Some(current_time_ms);
        }

        self.pending.push(op);
        self.bytes_accumulated += op_size;
        true
    }

    /// Check if the batch should be flushed
    pub fn should_flush(&self, current_time_ms: u64) -> bool {
        if self.pending.is_empty() {
            return false;
        }

        // Check if minimum interval has passed
        if current_time_ms < self.last_sync_time + self.config.min_interval_ms {
            return false;
        }

        // Check size limits
        if self.bytes_accumulated >= self.config.max_bytes {
            return true;
        }
        if self.pending.len() >= self.config.max_operations {
            return true;
        }

        // Check time limit
        if let Some(first_time) = self.first_pending_time {
            if current_time_ms >= first_time + self.config.max_wait_ms {
                return true;
            }
        }

        false
    }

    /// Flush pending operations into a batch
    pub fn flush(&mut self, current_time_ms: u64) -> Option<OperationBatch> {
        if self.pending.is_empty() {
            return None;
        }

        let batch = OperationBatch {
            operations: core::mem::take(&mut self.pending),
            total_bytes: self.bytes_accumulated,
            created_at: current_time_ms,
        };

        self.bytes_accumulated = 0;
        self.first_pending_time = None;
        self.last_sync_time = current_time_ms;

        Some(batch)
    }

    /// Force flush regardless of timing constraints
    pub fn force_flush(&mut self, current_time_ms: u64) -> Option<OperationBatch> {
        self.flush(current_time_ms)
    }

    /// Get number of pending operations
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get accumulated bytes
    pub fn accumulated_bytes(&self) -> usize {
        self.bytes_accumulated
    }

    /// Check if there are pending operations
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Clear all pending operations without flushing
    pub fn clear(&mut self) {
        self.pending.clear();
        self.bytes_accumulated = 0;
        self.first_pending_time = None;
    }

    /// Get the config
    pub fn config(&self) -> &BatchConfig {
        &self.config
    }

    /// Update the config
    pub fn set_config(&mut self, config: BatchConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crdt::Position;
    use crate::NodeId;

    fn make_position_op(node_id: u32, timestamp: u64) -> CrdtOperation {
        CrdtOperation::UpdatePosition {
            node_id: NodeId::new(node_id),
            position: Position::new(37.0, -122.0),
            timestamp,
        }
    }

    #[test]
    fn test_batch_config_defaults() {
        let config = BatchConfig::default();
        assert_eq!(config.max_wait_ms, 5000);
        assert_eq!(config.max_bytes, 512);
    }

    #[test]
    fn test_accumulator_add() {
        let mut acc = BatchAccumulator::with_defaults();

        let op = make_position_op(1, 1000);
        assert!(acc.add(op, 1000));
        assert_eq!(acc.pending_count(), 1);
        assert!(acc.has_pending());
    }

    #[test]
    fn test_accumulator_max_operations() {
        let config = BatchConfig {
            max_operations: 2,
            ..Default::default()
        };
        let mut acc = BatchAccumulator::new(config);

        assert!(acc.add(make_position_op(1, 1000), 1000));
        assert!(acc.add(make_position_op(2, 1001), 1001));
        assert!(!acc.add(make_position_op(3, 1002), 1002)); // Should fail
        assert_eq!(acc.pending_count(), 2);
    }

    #[test]
    fn test_accumulator_flush() {
        let mut acc = BatchAccumulator::with_defaults();

        acc.add(make_position_op(1, 1000), 1000);
        acc.add(make_position_op(2, 1001), 1001);

        let batch = acc.flush(2000).unwrap();
        assert_eq!(batch.len(), 2);
        assert!(!acc.has_pending());
    }

    #[test]
    fn test_should_flush_time() {
        let config = BatchConfig {
            max_wait_ms: 100,
            min_interval_ms: 0,
            ..Default::default()
        };
        let mut acc = BatchAccumulator::new(config);

        acc.add(make_position_op(1, 1000), 1000);

        // Not enough time passed
        assert!(!acc.should_flush(1050));

        // Time limit reached
        assert!(acc.should_flush(1100));
    }

    #[test]
    fn test_should_flush_size() {
        let config = BatchConfig {
            max_bytes: 20, // Small limit (each position op is ~21 bytes)
            min_interval_ms: 0,
            ..Default::default()
        };
        let mut acc = BatchAccumulator::new(config);

        // First op (21 bytes) exceeds max_bytes (20), but allowed since batch is empty
        assert!(acc.add(make_position_op(1, 1000), 1000));
        // Should flush because we've exceeded max_bytes
        assert!(acc.should_flush(1000));
    }

    #[test]
    fn test_min_interval() {
        let config = BatchConfig {
            max_wait_ms: 100,
            min_interval_ms: 1000,
            ..Default::default()
        };
        let mut acc = BatchAccumulator::new(config);

        acc.add(make_position_op(1, 0), 0);
        acc.flush(0);

        acc.add(make_position_op(2, 100), 100);

        // Min interval not passed
        assert!(!acc.should_flush(500));

        // Min interval passed
        assert!(acc.should_flush(1100));
    }

    #[test]
    fn test_batch_encode_decode() {
        let batch = OperationBatch {
            operations: vec![make_position_op(1, 1000), make_position_op(2, 2000)],
            total_bytes: 100,
            created_at: 3000,
        };

        let encoded = batch.encode();
        let decoded = OperationBatch::decode(&encoded).unwrap();

        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_clear() {
        let mut acc = BatchAccumulator::with_defaults();

        acc.add(make_position_op(1, 1000), 1000);
        acc.add(make_position_op(2, 1001), 1001);

        acc.clear();
        assert!(!acc.has_pending());
        assert_eq!(acc.accumulated_bytes(), 0);
    }

    #[test]
    fn test_force_flush() {
        let config = BatchConfig {
            min_interval_ms: 10000, // Long interval
            ..Default::default()
        };
        let mut acc = BatchAccumulator::new(config);

        acc.add(make_position_op(1, 0), 0);
        acc.flush(0);

        acc.add(make_position_op(2, 100), 100);

        // Normal flush blocked by min_interval
        assert!(!acc.should_flush(100));

        // Force flush works anyway
        let batch = acc.force_flush(100);
        assert!(batch.is_some());
    }
}
