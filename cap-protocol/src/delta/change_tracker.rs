//! Change detection system for tracking state modifications
//!
//! This module provides change tracking for CRDT-based state objects,
//! enabling efficient delta generation by identifying modified fields.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Tracks changes to state objects for delta generation
#[derive(Debug, Clone)]
pub struct ChangeTracker {
    /// Map of object ID to changed fields
    changes: Arc<RwLock<HashMap<String, FieldChangeSet>>>,
}

/// Set of changed fields for a single object
#[derive(Debug, Clone)]
pub struct FieldChangeSet {
    /// Fields that have been modified
    pub dirty_fields: HashSet<String>,
    /// Timestamp of last change
    pub last_modified: SystemTime,
    /// Sequence number for ordering
    pub sequence: u64,
}

impl Default for FieldChangeSet {
    fn default() -> Self {
        Self {
            dirty_fields: HashSet::new(),
            last_modified: SystemTime::now(),
            sequence: 0,
        }
    }
}

impl ChangeTracker {
    /// Create a new change tracker
    pub fn new() -> Self {
        Self {
            changes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Mark a field as changed for an object
    pub fn mark_changed(&self, object_id: &str, field: &str) {
        let mut changes = self.changes.write().unwrap();
        let field_set = changes.entry(object_id.to_string()).or_default();

        field_set.dirty_fields.insert(field.to_string());
        field_set.last_modified = SystemTime::now();
        field_set.sequence += 1;
    }

    /// Mark multiple fields as changed for an object
    pub fn mark_fields_changed(&self, object_id: &str, fields: &[&str]) {
        let mut changes = self.changes.write().unwrap();
        let field_set = changes.entry(object_id.to_string()).or_default();

        for field in fields {
            field_set.dirty_fields.insert(field.to_string());
        }
        field_set.last_modified = SystemTime::now();
        field_set.sequence += 1;
    }

    /// Get changed fields for an object
    pub fn get_changes(&self, object_id: &str) -> Option<FieldChangeSet> {
        let changes = self.changes.read().unwrap();
        changes.get(object_id).cloned()
    }

    /// Get all changed objects
    pub fn get_all_changes(&self) -> HashMap<String, FieldChangeSet> {
        let changes = self.changes.read().unwrap();
        changes.clone()
    }

    /// Clear changes for an object (after delta is generated)
    pub fn clear_changes(&self, object_id: &str) {
        let mut changes = self.changes.write().unwrap();
        changes.remove(object_id);
    }

    /// Clear all tracked changes
    pub fn clear_all(&self) {
        let mut changes = self.changes.write().unwrap();
        changes.clear();
    }

    /// Check if an object has any pending changes
    pub fn has_changes(&self, object_id: &str) -> bool {
        let changes = self.changes.read().unwrap();
        changes
            .get(object_id)
            .is_some_and(|cs| !cs.dirty_fields.is_empty())
    }

    /// Get count of objects with pending changes
    pub fn pending_count(&self) -> usize {
        let changes = self.changes.read().unwrap();
        changes.len()
    }

    /// Coalesce changes: merge overlapping field changes
    ///
    /// This is useful when multiple rapid changes occur to the same object,
    /// allowing us to batch them into a single delta.
    pub fn coalesce_changes(&self, max_age_ms: u64) -> HashMap<String, FieldChangeSet> {
        let changes = self.changes.read().unwrap();
        let now = SystemTime::now();

        changes
            .iter()
            .filter_map(|(id, field_set)| {
                let age = now
                    .duration_since(field_set.last_modified)
                    .ok()?
                    .as_millis() as u64;

                if age <= max_age_ms {
                    Some((id.clone(), field_set.clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for ChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_mark_single_field() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");

        let changes = tracker.get_changes("node1").unwrap();
        assert!(changes.dirty_fields.contains("position"));
        assert_eq!(changes.dirty_fields.len(), 1);
    }

    #[test]
    fn test_mark_multiple_fields() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");
        tracker.mark_changed("node1", "velocity");
        tracker.mark_changed("node1", "fuel");

        let changes = tracker.get_changes("node1").unwrap();
        assert_eq!(changes.dirty_fields.len(), 3);
        assert!(changes.dirty_fields.contains("position"));
        assert!(changes.dirty_fields.contains("velocity"));
        assert!(changes.dirty_fields.contains("fuel"));
    }

    #[test]
    fn test_mark_fields_batch() {
        let tracker = ChangeTracker::new();
        tracker.mark_fields_changed("node1", &["position", "velocity", "fuel"]);

        let changes = tracker.get_changes("node1").unwrap();
        assert_eq!(changes.dirty_fields.len(), 3);
    }

    #[test]
    fn test_multiple_objects() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");
        tracker.mark_changed("node2", "fuel");

        assert!(tracker.has_changes("node1"));
        assert!(tracker.has_changes("node2"));
        assert!(!tracker.has_changes("node3"));
        assert_eq!(tracker.pending_count(), 2);
    }

    #[test]
    fn test_clear_changes() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");
        tracker.mark_changed("node2", "fuel");

        tracker.clear_changes("node1");

        assert!(!tracker.has_changes("node1"));
        assert!(tracker.has_changes("node2"));
        assert_eq!(tracker.pending_count(), 1);
    }

    #[test]
    fn test_clear_all() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");
        tracker.mark_changed("node2", "fuel");

        tracker.clear_all();

        assert_eq!(tracker.pending_count(), 0);
        assert!(!tracker.has_changes("node1"));
        assert!(!tracker.has_changes("node2"));
    }

    #[test]
    fn test_sequence_increments() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");

        let changes1 = tracker.get_changes("node1").unwrap();
        assert_eq!(changes1.sequence, 1);

        tracker.mark_changed("node1", "velocity");

        let changes2 = tracker.get_changes("node1").unwrap();
        assert_eq!(changes2.sequence, 2);
    }

    #[test]
    fn test_coalesce_changes() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");

        // Wait a bit
        thread::sleep(Duration::from_millis(50));

        tracker.mark_changed("node2", "fuel");

        // Coalesce with 100ms window - should get both
        let coalesced = tracker.coalesce_changes(100);
        assert_eq!(coalesced.len(), 2);

        // Coalesce with 30ms window - should only get node2
        let coalesced_recent = tracker.coalesce_changes(30);
        assert_eq!(coalesced_recent.len(), 1);
        assert!(coalesced_recent.contains_key("node2"));
    }

    #[test]
    fn test_get_all_changes() {
        let tracker = ChangeTracker::new();
        tracker.mark_changed("node1", "position");
        tracker.mark_changed("node2", "fuel");
        tracker.mark_changed("node3", "velocity");

        let all_changes = tracker.get_all_changes();
        assert_eq!(all_changes.len(), 3);
        assert!(all_changes.contains_key("node1"));
        assert!(all_changes.contains_key("node2"));
        assert!(all_changes.contains_key("node3"));
    }

    #[test]
    fn test_concurrent_access() {
        let tracker = Arc::new(ChangeTracker::new());
        let tracker1 = Arc::clone(&tracker);
        let tracker2 = Arc::clone(&tracker);

        let t1 = thread::spawn(move || {
            for i in 0..100 {
                tracker1.mark_changed(&format!("node{}", i), "field1");
            }
        });

        let t2 = thread::spawn(move || {
            for i in 0..100 {
                tracker2.mark_changed(&format!("node{}", i), "field2");
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Should have changes for all nodes
        assert_eq!(tracker.pending_count(), 100);
    }

    #[test]
    fn test_no_changes_initially() {
        let tracker = ChangeTracker::new();
        assert_eq!(tracker.pending_count(), 0);
        assert!(!tracker.has_changes("node1"));
        assert!(tracker.get_changes("node1").is_none());
    }
}
