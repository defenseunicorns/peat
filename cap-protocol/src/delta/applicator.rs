//! Delta application system for applying incremental updates
//!
//! This module applies delta operations to CRDT state, implementing
//! idempotent application and validation logic.

use crate::delta::generator::{Delta, DeltaBatch, DeltaOp};
use crate::{Error, Result};
use std::collections::HashSet;

/// Result of applying a delta
#[derive(Debug, Clone, PartialEq)]
pub enum ApplicationResult {
    /// Delta was successfully applied
    Applied,

    /// Delta was already applied (idempotent - no-op)
    AlreadyApplied,

    /// Delta was rejected due to validation failure
    Rejected(String),

    /// Delta was skipped due to obsolescence or TTL expiration
    Skipped(String),
}

impl ApplicationResult {
    /// Check if delta was successfully applied
    pub fn is_applied(&self) -> bool {
        matches!(self, ApplicationResult::Applied)
    }

    /// Check if delta was rejected
    pub fn is_rejected(&self) -> bool {
        matches!(self, ApplicationResult::Rejected(_))
    }

    /// Check if delta was skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, ApplicationResult::Skipped(_))
    }

    /// Check if operation succeeded (either applied or idempotently skipped)
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ApplicationResult::Applied | ApplicationResult::AlreadyApplied
        )
    }
}

/// Tracks applied deltas to ensure idempotent application
#[derive(Debug, Clone)]
pub struct DeltaHistory {
    /// Set of applied delta identifiers (object_id:sequence)
    applied: HashSet<String>,
}

impl DeltaHistory {
    /// Create a new delta history
    pub fn new() -> Self {
        Self {
            applied: HashSet::new(),
        }
    }

    /// Check if delta has been applied
    pub fn is_applied(&self, object_id: &str, sequence: u64) -> bool {
        let key = format!("{}:{}", object_id, sequence);
        self.applied.contains(&key)
    }

    /// Mark delta as applied
    pub fn mark_applied(&mut self, object_id: &str, sequence: u64) {
        let key = format!("{}:{}", object_id, sequence);
        self.applied.insert(key);
    }

    /// Clear history for an object (when full state sync occurs)
    pub fn clear_object(&mut self, object_id: &str) {
        self.applied.retain(|key| !key.starts_with(object_id));
    }

    /// Clear all history
    pub fn clear_all(&mut self) {
        self.applied.clear();
    }

    /// Get count of applied deltas
    pub fn count(&self) -> usize {
        self.applied.len()
    }
}

impl Default for DeltaHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Delta applicator that applies delta operations to state
pub struct DeltaApplicator {
    /// History of applied deltas for idempotency
    history: DeltaHistory,
}

impl DeltaApplicator {
    /// Create a new delta applicator
    pub fn new() -> Self {
        Self {
            history: DeltaHistory::new(),
        }
    }

    /// Apply a single delta
    ///
    /// Returns ApplicationResult indicating whether the delta was applied.
    /// This method is idempotent - applying the same delta twice is safe.
    pub fn apply(&mut self, delta: &Delta) -> Result<ApplicationResult> {
        // Check if already applied (idempotency)
        if self.history.is_applied(&delta.object_id, delta.sequence) {
            return Ok(ApplicationResult::AlreadyApplied);
        }

        // Validate delta
        if let Err(e) = self.validate_delta(delta) {
            return Ok(ApplicationResult::Rejected(e.to_string()));
        }

        // Apply each operation
        // In production, this would interact with actual Ditto store
        for op in &delta.operations {
            self.apply_operation(&delta.object_id, &delta.collection, op)?;
        }

        // Mark as applied
        self.history.mark_applied(&delta.object_id, delta.sequence);

        Ok(ApplicationResult::Applied)
    }

    /// Apply a batch of deltas
    pub fn apply_batch(&mut self, batch: &DeltaBatch) -> Result<Vec<ApplicationResult>> {
        let mut results = Vec::new();

        for delta in &batch.deltas {
            let result = self.apply(delta)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Validate delta before application
    fn validate_delta(&self, delta: &Delta) -> Result<()> {
        // Check for empty operations
        if delta.operations.is_empty() {
            return Err(Error::storage_error(
                "Delta has no operations",
                "validate_delta",
                Some(delta.object_id.clone()),
            ));
        }

        // Validate each operation
        for op in &delta.operations {
            self.validate_operation(op)?;
        }

        Ok(())
    }

    /// Validate a single operation
    fn validate_operation(&self, op: &DeltaOp) -> Result<()> {
        match op {
            DeltaOp::LwwSet {
                field, timestamp, ..
            } => {
                if field.is_empty() {
                    return Err(Error::storage_error(
                        "LWW field name is empty",
                        "validate_operation",
                        None,
                    ));
                }
                if *timestamp == 0 {
                    return Err(Error::storage_error(
                        "LWW timestamp is zero",
                        "validate_operation",
                        None,
                    ));
                }
            }
            DeltaOp::GSetAdd { field, .. } => {
                if field.is_empty() {
                    return Err(Error::storage_error(
                        "GSet field name is empty",
                        "validate_operation",
                        None,
                    ));
                }
            }
            DeltaOp::OrSetAdd { field, tag, .. } => {
                if field.is_empty() || tag.is_empty() {
                    return Err(Error::storage_error(
                        "ORSet field or tag is empty",
                        "validate_operation",
                        None,
                    ));
                }
            }
            DeltaOp::OrSetRemove { field, tag } => {
                if field.is_empty() || tag.is_empty() {
                    return Err(Error::storage_error(
                        "ORSet field or tag is empty",
                        "validate_operation",
                        None,
                    ));
                }
            }
            DeltaOp::CounterIncrement { field, amount }
            | DeltaOp::CounterDecrement { field, amount } => {
                if field.is_empty() {
                    return Err(Error::storage_error(
                        "Counter field name is empty",
                        "validate_operation",
                        None,
                    ));
                }
                if *amount == 0 {
                    return Err(Error::storage_error(
                        "Counter amount is zero",
                        "validate_operation",
                        None,
                    ));
                }
            }
        }

        Ok(())
    }

    /// Apply a single operation
    ///
    /// In production, this would interact with the Ditto store to apply
    /// the operation to the actual CRDT state. For now, this is a placeholder.
    fn apply_operation(&self, _object_id: &str, _collection: &str, op: &DeltaOp) -> Result<()> {
        // Placeholder - in production would apply to Ditto store
        match op {
            DeltaOp::LwwSet {
                field,
                value,
                timestamp,
            } => {
                // Would call: ditto_store.update_field(object_id, field, value, timestamp)
                tracing::debug!(
                    "Applying LWW-Set: field={}, value={:?}, timestamp={}",
                    field,
                    value,
                    timestamp
                );
            }
            DeltaOp::GSetAdd { field, element } => {
                // Would call: ditto_store.add_to_set(object_id, field, element)
                tracing::debug!("Applying G-Set Add: field={}, element={:?}", field, element);
            }
            DeltaOp::OrSetAdd {
                field,
                element,
                tag,
            } => {
                // Would call: ditto_store.orset_add(object_id, field, element, tag)
                tracing::debug!(
                    "Applying OR-Set Add: field={}, element={:?}, tag={}",
                    field,
                    element,
                    tag
                );
            }
            DeltaOp::OrSetRemove { field, tag } => {
                // Would call: ditto_store.orset_remove(object_id, field, tag)
                tracing::debug!("Applying OR-Set Remove: field={}, tag={}", field, tag);
            }
            DeltaOp::CounterIncrement { field, amount } => {
                // Would call: ditto_store.increment_counter(object_id, field, amount)
                tracing::debug!(
                    "Applying Counter Increment: field={}, amount={}",
                    field,
                    amount
                );
            }
            DeltaOp::CounterDecrement { field, amount } => {
                // Would call: ditto_store.decrement_counter(object_id, field, amount)
                tracing::debug!(
                    "Applying Counter Decrement: field={}, amount={}",
                    field,
                    amount
                );
            }
        }

        Ok(())
    }

    /// Clear history for an object
    pub fn clear_object_history(&mut self, object_id: &str) {
        self.history.clear_object(object_id);
    }

    /// Clear all history
    pub fn clear_all_history(&mut self) {
        self.history.clear_all();
    }

    /// Get reference to history for inspection
    pub fn history(&self) -> &DeltaHistory {
        &self.history
    }
}

impl Default for DeltaApplicator {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for delta application
#[derive(Debug, Clone, Default)]
pub struct ApplicationStats {
    /// Number of deltas applied
    pub applied_count: usize,

    /// Number of deltas already applied (idempotent)
    pub already_applied_count: usize,

    /// Number of deltas rejected
    pub rejected_count: usize,

    /// Number of deltas skipped
    pub skipped_count: usize,

    /// Total operations applied
    pub operations_applied: usize,
}

impl ApplicationStats {
    /// Create statistics from application results
    pub fn from_results(results: &[ApplicationResult]) -> Self {
        let mut stats = Self::default();

        for result in results {
            match result {
                ApplicationResult::Applied => stats.applied_count += 1,
                ApplicationResult::AlreadyApplied => stats.already_applied_count += 1,
                ApplicationResult::Rejected(_) => stats.rejected_count += 1,
                ApplicationResult::Skipped(_) => stats.skipped_count += 1,
            }
        }

        stats
    }

    /// Total number of deltas processed
    pub fn total(&self) -> usize {
        self.applied_count + self.already_applied_count + self.rejected_count + self.skipped_count
    }

    /// Success rate (applied + already_applied) / total
    pub fn success_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 1.0;
        }
        (self.applied_count + self.already_applied_count) as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::time::SystemTime;

    fn create_test_delta(object_id: &str, sequence: u64, operations: Vec<DeltaOp>) -> Delta {
        Delta {
            object_id: object_id.to_string(),
            collection: "cells".to_string(),
            sequence,
            operations,
            generated_at: SystemTime::now(),
        }
    }

    #[test]
    fn test_apply_lww_delta() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 12345,
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);
    }

    #[test]
    fn test_idempotent_application() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 12345,
            }],
        );

        // First application
        let result1 = applicator.apply(&delta).unwrap();
        assert_eq!(result1, ApplicationResult::Applied);

        // Second application - should be idempotent
        let result2 = applicator.apply(&delta).unwrap();
        assert_eq!(result2, ApplicationResult::AlreadyApplied);
    }

    #[test]
    fn test_apply_gset_add() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::GSetAdd {
                field: "capabilities".to_string(),
                element: Value::String("sensor".to_string()),
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_applied());
    }

    #[test]
    fn test_apply_orset_operations() {
        let mut applicator = DeltaApplicator::new();

        // Add operation
        let add_delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetAdd {
                field: "members".to_string(),
                element: Value::String("node1".to_string()),
                tag: "add_123".to_string(),
            }],
        );

        let result = applicator.apply(&add_delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);

        // Remove operation
        let remove_delta = create_test_delta(
            "cell1",
            2,
            vec![DeltaOp::OrSetRemove {
                field: "members".to_string(),
                tag: "add_123".to_string(),
            }],
        );

        let result = applicator.apply(&remove_delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);
    }

    #[test]
    fn test_apply_counter_operations() {
        let mut applicator = DeltaApplicator::new();

        // Increment
        let inc_delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::CounterIncrement {
                field: "vote_count".to_string(),
                amount: 5,
            }],
        );

        let result = applicator.apply(&inc_delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);

        // Decrement
        let dec_delta = create_test_delta(
            "cell1",
            2,
            vec![DeltaOp::CounterDecrement {
                field: "vote_count".to_string(),
                amount: 2,
            }],
        );

        let result = applicator.apply(&dec_delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);
    }

    #[test]
    fn test_validate_empty_operations() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta("cell1", 1, vec![]);

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, ApplicationResult::Rejected(_)));
    }

    #[test]
    fn test_validate_empty_field_name() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "".to_string(),
                value: Value::String("test".to_string()),
                timestamp: 12345,
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_rejected());
    }

    #[test]
    fn test_validate_zero_timestamp() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 0,
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_rejected());
    }

    #[test]
    fn test_validate_empty_tag() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetAdd {
                field: "members".to_string(),
                element: Value::String("node1".to_string()),
                tag: "".to_string(),
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_rejected());
    }

    #[test]
    fn test_validate_zero_counter_amount() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::CounterIncrement {
                field: "count".to_string(),
                amount: 0,
            }],
        );

        let result = applicator.apply(&delta).unwrap();
        assert!(result.is_rejected());
    }

    #[test]
    fn test_apply_batch() {
        let mut applicator = DeltaApplicator::new();

        let mut batch = DeltaBatch::new();

        batch.add(create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 12345,
            }],
        ));

        batch.add(create_test_delta(
            "cell2",
            1,
            vec![DeltaOp::GSetAdd {
                field: "capabilities".to_string(),
                element: Value::String("sensor".to_string()),
            }],
        ));

        let results = applicator.apply_batch(&batch).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_applied()));
    }

    #[test]
    fn test_batch_with_duplicate() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 12345,
            }],
        );

        let mut batch = DeltaBatch::new();
        batch.add(delta.clone());
        batch.add(delta); // Duplicate

        let results = applicator.apply_batch(&batch).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ApplicationResult::Applied);
        assert_eq!(results[1], ApplicationResult::AlreadyApplied);
    }

    #[test]
    fn test_delta_history() {
        let mut history = DeltaHistory::new();

        assert!(!history.is_applied("cell1", 1));

        history.mark_applied("cell1", 1);
        assert!(history.is_applied("cell1", 1));
        assert_eq!(history.count(), 1);

        history.mark_applied("cell1", 2);
        history.mark_applied("cell2", 1);
        assert_eq!(history.count(), 3);

        history.clear_object("cell1");
        assert!(!history.is_applied("cell1", 1));
        assert!(history.is_applied("cell2", 1));
        assert_eq!(history.count(), 1);

        history.clear_all();
        assert_eq!(history.count(), 0);
    }

    #[test]
    fn test_application_stats() {
        let results = vec![
            ApplicationResult::Applied,
            ApplicationResult::Applied,
            ApplicationResult::AlreadyApplied,
            ApplicationResult::Rejected("error".to_string()),
            ApplicationResult::Skipped("obsolete".to_string()),
        ];

        let stats = ApplicationStats::from_results(&results);

        assert_eq!(stats.applied_count, 2);
        assert_eq!(stats.already_applied_count, 1);
        assert_eq!(stats.rejected_count, 1);
        assert_eq!(stats.skipped_count, 1);
        assert_eq!(stats.total(), 5);
        assert_eq!(stats.success_rate(), 0.6); // 3/5
    }

    #[test]
    fn test_stats_success_rate_perfect() {
        let results = vec![
            ApplicationResult::Applied,
            ApplicationResult::AlreadyApplied,
        ];

        let stats = ApplicationStats::from_results(&results);
        assert_eq!(stats.success_rate(), 1.0);
    }

    #[test]
    fn test_stats_empty() {
        let stats = ApplicationStats::from_results(&[]);
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.success_rate(), 1.0); // Empty is considered success
    }

    #[test]
    fn test_clear_object_history() {
        let mut applicator = DeltaApplicator::new();

        let delta1 = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node1".to_string()),
                timestamp: 12345,
            }],
        );

        let delta2 = create_test_delta(
            "cell2",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: Value::String("node2".to_string()),
                timestamp: 12345,
            }],
        );

        applicator.apply(&delta1).unwrap();
        applicator.apply(&delta2).unwrap();

        assert_eq!(applicator.history().count(), 2);

        applicator.clear_object_history("cell1");
        assert_eq!(applicator.history().count(), 1);

        // Can now reapply delta1
        let result = applicator.apply(&delta1).unwrap();
        assert_eq!(result, ApplicationResult::Applied);
    }

    #[test]
    fn test_multiple_operations_in_delta() {
        let mut applicator = DeltaApplicator::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![
                DeltaOp::LwwSet {
                    field: "leader_id".to_string(),
                    value: Value::String("node1".to_string()),
                    timestamp: 12345,
                },
                DeltaOp::GSetAdd {
                    field: "capabilities".to_string(),
                    element: Value::String("sensor".to_string()),
                },
                DeltaOp::CounterIncrement {
                    field: "member_count".to_string(),
                    amount: 1,
                },
            ],
        );

        let result = applicator.apply(&delta).unwrap();
        assert_eq!(result, ApplicationResult::Applied);
    }
}
