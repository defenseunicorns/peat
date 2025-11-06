//! Delta generation system for efficient state synchronization
//!
//! This module generates compact delta messages from tracked changes,
//! reducing bandwidth by transmitting only modifications instead of full state.

use crate::delta::change_tracker::{ChangeTracker, FieldChangeSet};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Delta operation types matching CRDT semantics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum DeltaOp {
    /// Last-Write-Wins Register: Set a field value with timestamp
    LwwSet {
        field: String,
        value: serde_json::Value,
        timestamp: u64,
    },

    /// Grow-Only Set: Add an element to a set
    GSetAdd {
        field: String,
        element: serde_json::Value,
    },

    /// Observed-Remove Set: Add element with unique tag
    OrSetAdd {
        field: String,
        element: serde_json::Value,
        tag: String, // Unique tag for this add operation
    },

    /// Observed-Remove Set: Remove element by tag
    OrSetRemove { field: String, tag: String },

    /// PN-Counter: Increment counter
    CounterIncrement { field: String, amount: i64 },

    /// PN-Counter: Decrement counter
    CounterDecrement { field: String, amount: i64 },
}

/// A batch of delta operations for a single object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Object identifier
    pub object_id: String,

    /// Collection name (e.g., "cells", "node_configs")
    pub collection: String,

    /// Sequence number for ordering
    pub sequence: u64,

    /// Operations in this delta
    pub operations: Vec<DeltaOp>,

    /// Generation timestamp
    pub generated_at: SystemTime,
}

impl Delta {
    /// Get serialized size in bytes
    pub fn size_bytes(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }

    /// Check if delta is empty
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

/// Batch of deltas for multiple objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaBatch {
    /// Deltas in this batch
    pub deltas: Vec<Delta>,

    /// Batch creation timestamp
    pub created_at: SystemTime,
}

impl DeltaBatch {
    /// Create empty batch
    pub fn new() -> Self {
        Self {
            deltas: Vec::new(),
            created_at: SystemTime::now(),
        }
    }

    /// Add delta to batch
    pub fn add(&mut self, delta: Delta) {
        if !delta.is_empty() {
            self.deltas.push(delta);
        }
    }

    /// Get total serialized size
    pub fn size_bytes(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    /// Compress batch using gzip
    ///
    /// Note: Requires flate2 dependency. Add to Cargo.toml and uncomment to use.
    /// ```toml
    /// flate2 = "1.0"
    /// ```
    #[allow(dead_code)]
    pub fn compress(&self) -> Result<Vec<u8>, std::io::Error> {
        // Commented out until flate2 is added as a dependency
        // use flate2::write::GzEncoder;
        // use flate2::Compression;
        // use std::io::Write;
        //
        // let json = serde_json::to_vec(self)?;
        // let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        // encoder.write_all(&json)?;
        // encoder.finish()

        // Placeholder - returns uncompressed JSON for now
        serde_json::to_vec(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

impl Default for DeltaBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Delta generator that converts tracked changes to delta operations
pub struct DeltaGenerator {
    change_tracker: ChangeTracker,
}

impl DeltaGenerator {
    /// Create a new delta generator
    pub fn new(change_tracker: ChangeTracker) -> Self {
        Self { change_tracker }
    }

    /// Generate deltas for all tracked changes
    ///
    /// This retrieves all pending changes from the tracker and generates
    /// delta operations. The changes remain in the tracker until explicitly cleared.
    pub fn generate_all(&self, collection: &str) -> DeltaBatch {
        let all_changes = self.change_tracker.get_all_changes();
        self.generate_from_changes(collection, all_changes)
    }

    /// Generate deltas for specific object
    pub fn generate_for_object(&self, collection: &str, object_id: &str) -> Option<Delta> {
        let changes = self.change_tracker.get_changes(object_id)?;
        Some(self.create_delta(collection, object_id, &changes))
    }

    /// Generate deltas from coalesced changes (within time window)
    pub fn generate_coalesced(&self, collection: &str, max_age_ms: u64) -> DeltaBatch {
        let coalesced = self.change_tracker.coalesce_changes(max_age_ms);
        self.generate_from_changes(collection, coalesced)
    }

    /// Generate delta batch from change map
    fn generate_from_changes(
        &self,
        collection: &str,
        changes: HashMap<String, FieldChangeSet>,
    ) -> DeltaBatch {
        let mut batch = DeltaBatch::new();

        for (object_id, field_set) in changes {
            let delta = self.create_delta(collection, &object_id, &field_set);
            batch.add(delta);
        }

        batch
    }

    /// Create delta from field change set
    ///
    /// This is a placeholder that creates generic LWW operations.
    /// In production, this would inspect the actual state objects to determine
    /// the appropriate CRDT operation type for each field.
    fn create_delta(&self, collection: &str, object_id: &str, changes: &FieldChangeSet) -> Delta {
        let mut operations = Vec::new();

        // For now, treat all fields as LWW registers
        // In production, we'd determine operation type based on field semantics:
        // - members -> OR-Set operations
        // - capabilities -> G-Set operations
        // - leader_id -> LWW-Register
        // - counters -> PN-Counter operations

        for field in &changes.dirty_fields {
            operations.push(DeltaOp::LwwSet {
                field: field.clone(),
                value: serde_json::Value::Null, // Placeholder - would fetch actual value
                timestamp: changes
                    .last_modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
        }

        Delta {
            object_id: object_id.to_string(),
            collection: collection.to_string(),
            sequence: changes.sequence,
            operations,
            generated_at: SystemTime::now(),
        }
    }

    /// Clear changes after delta generation (after successful transmission)
    pub fn clear_changes(&self, object_id: &str) {
        self.change_tracker.clear_changes(object_id);
    }

    /// Clear all changes
    pub fn clear_all(&self) {
        self.change_tracker.clear_all();
    }
}

/// Statistics for measuring delta efficiency
#[derive(Debug, Clone)]
pub struct DeltaStats {
    /// Size of delta in bytes
    pub delta_size: usize,

    /// Size of full state in bytes (for comparison)
    pub full_state_size: usize,

    /// Number of operations in delta
    pub operation_count: usize,

    /// Compression ratio (if compressed)
    pub compression_ratio: Option<f64>,
}

impl DeltaStats {
    /// Calculate size ratio (delta / full state)
    pub fn size_ratio(&self) -> f64 {
        if self.full_state_size == 0 {
            return 0.0;
        }
        self.delta_size as f64 / self.full_state_size as f64
    }

    /// Calculate size reduction percentage
    pub fn size_reduction_percent(&self) -> f64 {
        (1.0 - self.size_ratio()) * 100.0
    }

    /// Check if meets <5% target from E7 success criteria
    pub fn meets_target(&self) -> bool {
        self.size_ratio() < 0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_op_serialization() {
        let op = DeltaOp::LwwSet {
            field: "leader_id".to_string(),
            value: serde_json::json!("node1"),
            timestamp: 12345,
        };

        let json = serde_json::to_string(&op).unwrap();
        let deserialized: DeltaOp = serde_json::from_str(&json).unwrap();

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_gset_add_serialization() {
        let op = DeltaOp::GSetAdd {
            field: "capabilities".to_string(),
            element: serde_json::json!({"id": "cap1", "type": "Sensor"}),
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("GSetAdd"));
        assert!(json.contains("capabilities"));
    }

    #[test]
    fn test_orset_operations() {
        let add_op = DeltaOp::OrSetAdd {
            field: "members".to_string(),
            element: serde_json::json!("node1"),
            tag: "add_123".to_string(),
        };

        let remove_op = DeltaOp::OrSetRemove {
            field: "members".to_string(),
            tag: "add_123".to_string(),
        };

        let add_json = serde_json::to_string(&add_op).unwrap();
        let remove_json = serde_json::to_string(&remove_op).unwrap();

        assert!(add_json.contains("OrSetAdd"));
        assert!(remove_json.contains("OrSetRemove"));
    }

    #[test]
    fn test_counter_operations() {
        let inc = DeltaOp::CounterIncrement {
            field: "vote_count".to_string(),
            amount: 5,
        };

        let dec = DeltaOp::CounterDecrement {
            field: "vote_count".to_string(),
            amount: 2,
        };

        let inc_json = serde_json::to_string(&inc).unwrap();
        let dec_json = serde_json::to_string(&dec).unwrap();

        assert!(inc_json.contains("CounterIncrement"));
        assert!(dec_json.contains("CounterDecrement"));
    }

    #[test]
    fn test_delta_creation() {
        let delta = Delta {
            object_id: "cell1".to_string(),
            collection: "cells".to_string(),
            sequence: 42,
            operations: vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
            generated_at: SystemTime::now(),
        };

        assert_eq!(delta.object_id, "cell1");
        assert_eq!(delta.operations.len(), 1);
        assert!(!delta.is_empty());
        assert!(delta.size_bytes() > 0);
    }

    #[test]
    fn test_empty_delta() {
        let delta = Delta {
            object_id: "cell1".to_string(),
            collection: "cells".to_string(),
            sequence: 1,
            operations: vec![],
            generated_at: SystemTime::now(),
        };

        assert!(delta.is_empty());
    }

    #[test]
    fn test_delta_batch() {
        let mut batch = DeltaBatch::new();
        assert!(batch.is_empty());

        let delta1 = Delta {
            object_id: "cell1".to_string(),
            collection: "cells".to_string(),
            sequence: 1,
            operations: vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
            generated_at: SystemTime::now(),
        };

        batch.add(delta1);
        assert!(!batch.is_empty());
        assert_eq!(batch.deltas.len(), 1);
        assert!(batch.size_bytes() > 0);
    }

    #[test]
    fn test_batch_skips_empty_deltas() {
        let mut batch = DeltaBatch::new();

        let empty_delta = Delta {
            object_id: "cell1".to_string(),
            collection: "cells".to_string(),
            sequence: 1,
            operations: vec![],
            generated_at: SystemTime::now(),
        };

        batch.add(empty_delta);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_delta_generator_integration() {
        let tracker = ChangeTracker::new();
        let generator = DeltaGenerator::new(tracker.clone());

        // Mark some fields as changed
        tracker.mark_changed("cell1", "leader_id");
        tracker.mark_changed("cell1", "members");
        tracker.mark_changed("cell2", "capabilities");

        // Generate deltas
        let batch = generator.generate_all("cells");

        assert!(!batch.is_empty());
        assert_eq!(batch.deltas.len(), 2); // Two objects

        // Check cell1 delta
        let cell1_delta = batch
            .deltas
            .iter()
            .find(|d| d.object_id == "cell1")
            .unwrap();
        assert_eq!(cell1_delta.operations.len(), 2); // Two fields

        // Check cell2 delta
        let cell2_delta = batch
            .deltas
            .iter()
            .find(|d| d.object_id == "cell2")
            .unwrap();
        assert_eq!(cell2_delta.operations.len(), 1); // One field
    }

    #[test]
    fn test_generate_for_specific_object() {
        let tracker = ChangeTracker::new();
        let generator = DeltaGenerator::new(tracker.clone());

        tracker.mark_changed("cell1", "leader_id");
        tracker.mark_changed("cell2", "members");

        let delta = generator.generate_for_object("cells", "cell1").unwrap();

        assert_eq!(delta.object_id, "cell1");
        assert_eq!(delta.operations.len(), 1);
    }

    #[test]
    fn test_generate_for_nonexistent_object() {
        let tracker = ChangeTracker::new();
        let generator = DeltaGenerator::new(tracker.clone());

        let delta = generator.generate_for_object("cells", "nonexistent");
        assert!(delta.is_none());
    }

    #[test]
    fn test_coalesced_generation() {
        let tracker = ChangeTracker::new();
        let generator = DeltaGenerator::new(tracker.clone());

        // Mark changes with time gaps
        tracker.mark_changed("cell1", "leader_id");

        std::thread::sleep(std::time::Duration::from_millis(50));

        tracker.mark_changed("cell2", "members");

        // Generate with 100ms window - should get both
        let batch = generator.generate_coalesced("cells", 100);
        assert_eq!(batch.deltas.len(), 2);

        // Generate with 30ms window - should only get cell2
        let recent_batch = generator.generate_coalesced("cells", 30);
        assert_eq!(recent_batch.deltas.len(), 1);
        assert_eq!(recent_batch.deltas[0].object_id, "cell2");
    }

    #[test]
    fn test_clear_after_generation() {
        let tracker = ChangeTracker::new();
        let generator = DeltaGenerator::new(tracker.clone());

        tracker.mark_changed("cell1", "leader_id");
        assert!(tracker.has_changes("cell1"));

        // Generate delta
        let _delta = generator.generate_for_object("cells", "cell1").unwrap();

        // Clear after generation
        generator.clear_changes("cell1");
        assert!(!tracker.has_changes("cell1"));
    }

    #[test]
    fn test_delta_stats_calculation() {
        let stats = DeltaStats {
            delta_size: 100,
            full_state_size: 5000,
            operation_count: 3,
            compression_ratio: None,
        };

        assert_eq!(stats.size_ratio(), 0.02); // 2%
        assert_eq!(stats.size_reduction_percent(), 98.0);
        assert!(stats.meets_target()); // <5%
    }

    #[test]
    fn test_delta_stats_threshold() {
        let good_stats = DeltaStats {
            delta_size: 200,
            full_state_size: 5000,
            operation_count: 2,
            compression_ratio: None,
        };
        assert!(good_stats.meets_target()); // 4% < 5%

        let bad_stats = DeltaStats {
            delta_size: 300,
            full_state_size: 5000,
            operation_count: 5,
            compression_ratio: None,
        };
        assert!(!bad_stats.meets_target()); // 6% > 5%
    }

    #[test]
    fn test_stats_with_compression() {
        let stats = DeltaStats {
            delta_size: 50, // Compressed size
            full_state_size: 5000,
            operation_count: 3,
            compression_ratio: Some(2.0), // 2x compression
        };

        assert_eq!(stats.size_ratio(), 0.01); // 1%
        assert!(stats.meets_target());
    }

    #[test]
    fn test_batch_serialization() {
        let mut batch = DeltaBatch::new();

        batch.add(Delta {
            object_id: "cell1".to_string(),
            collection: "cells".to_string(),
            sequence: 1,
            operations: vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
            generated_at: SystemTime::now(),
        });

        // Serialize and deserialize
        let json = serde_json::to_string(&batch).unwrap();
        let deserialized: DeltaBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.deltas.len(), batch.deltas.len());
        assert_eq!(deserialized.deltas[0].object_id, "cell1");
    }
}
