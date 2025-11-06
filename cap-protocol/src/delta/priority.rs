//! Delta prioritization and TTL management
//!
//! This module assigns priorities to deltas and manages time-to-live (TTL)
//! for obsolescence detection.

use crate::delta::generator::{Delta, DeltaBatch, DeltaOp};
use std::time::{Duration, SystemTime};

/// Priority levels for delta messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// P0: Critical - Capability loss (highest priority)
    Critical = 0,

    /// P1: High - Cell membership changes
    High = 1,

    /// P2: Medium - Leader election, state updates
    Medium = 2,

    /// P3: Low - Capability additions, metadata
    Low = 3,
}

impl Priority {
    /// Get priority as numeric value for comparison
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Check if this priority is higher than another
    pub fn is_higher_than(&self, other: &Priority) -> bool {
        self < other // Lower numeric value = higher priority
    }
}

/// Delta with priority and TTL information
#[derive(Debug, Clone)]
pub struct PrioritizedDelta {
    /// The delta message
    pub delta: Delta,

    /// Assigned priority
    pub priority: Priority,

    /// Time-to-live (expiration time)
    pub expires_at: SystemTime,

    /// TTL duration for reference
    pub ttl: Duration,
}

impl PrioritizedDelta {
    /// Check if delta has expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Check if delta is about to expire (within threshold)
    pub fn is_expiring_soon(&self, threshold: Duration) -> bool {
        match self.expires_at.duration_since(SystemTime::now()) {
            Ok(remaining) => remaining < threshold,
            Err(_) => true, // Already expired
        }
    }

    /// Get remaining TTL
    pub fn remaining_ttl(&self) -> Option<Duration> {
        self.expires_at.duration_since(SystemTime::now()).ok()
    }
}

/// Priority classifier that assigns priorities based on delta content
#[derive(Debug, Clone)]
pub struct PriorityClassifier {
    /// Default TTL for each priority level
    ttl_by_priority: [Duration; 4],
}

impl PriorityClassifier {
    /// Create a new priority classifier with default TTLs
    pub fn new() -> Self {
        Self {
            ttl_by_priority: [
                Duration::from_secs(30),  // Critical: 30s
                Duration::from_secs(60),  // High: 1min
                Duration::from_secs(300), // Medium: 5min
                Duration::from_secs(600), // Low: 10min
            ],
        }
    }

    /// Create classifier with custom TTLs
    pub fn with_ttls(critical: Duration, high: Duration, medium: Duration, low: Duration) -> Self {
        Self {
            ttl_by_priority: [critical, high, medium, low],
        }
    }

    /// Assign priority to a delta based on its operations
    pub fn classify(&self, delta: &Delta) -> Priority {
        let mut max_priority = Priority::Low;

        for op in &delta.operations {
            let op_priority = self.classify_operation(op);
            if op_priority.is_higher_than(&max_priority) {
                max_priority = op_priority;
            }
        }

        max_priority
    }

    /// Classify a single operation
    fn classify_operation(&self, op: &DeltaOp) -> Priority {
        match op {
            // Capability removals = Critical (capability loss)
            DeltaOp::OrSetRemove { field, .. } if field == "capabilities" => Priority::Critical,

            // Member removals = High (cell membership changes)
            DeltaOp::OrSetRemove { field, .. } if field == "members" => Priority::High,

            // Member additions = High (cell membership changes)
            DeltaOp::OrSetAdd { field, .. } if field == "members" => Priority::High,

            // Leader changes = Medium (leadership updates)
            DeltaOp::LwwSet { field, .. } if field == "leader_id" => Priority::Medium,

            // Capability additions = Low (new capabilities)
            DeltaOp::GSetAdd { field, .. } if field == "capabilities" => Priority::Low,
            DeltaOp::OrSetAdd { field, .. } if field == "capabilities" => Priority::Low,

            // All other operations = Low
            _ => Priority::Low,
        }
    }

    /// Create prioritized delta with TTL
    pub fn prioritize(&self, delta: Delta) -> PrioritizedDelta {
        let priority = self.classify(&delta);
        let ttl = self.ttl_by_priority[priority as usize];
        let expires_at = SystemTime::now() + ttl;

        PrioritizedDelta {
            delta,
            priority,
            expires_at,
            ttl,
        }
    }

    /// Prioritize a batch of deltas
    pub fn prioritize_batch(&self, batch: &DeltaBatch) -> Vec<PrioritizedDelta> {
        batch
            .deltas
            .iter()
            .cloned()
            .map(|delta| self.prioritize(delta))
            .collect()
    }

    /// Filter out expired deltas from a batch
    pub fn filter_expired(&self, deltas: Vec<PrioritizedDelta>) -> Vec<PrioritizedDelta> {
        deltas.into_iter().filter(|d| !d.is_expired()).collect()
    }

    /// Sort deltas by priority (highest first)
    pub fn sort_by_priority(&self, deltas: &mut [PrioritizedDelta]) {
        deltas.sort_by(|a, b| a.priority.cmp(&b.priority));
    }
}

impl Default for PriorityClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Priority queue for managing deltas
#[derive(Debug)]
pub struct DeltaQueue {
    /// Queued deltas sorted by priority
    deltas: Vec<PrioritizedDelta>,

    /// Classifier for new deltas
    classifier: PriorityClassifier,
}

impl DeltaQueue {
    /// Create a new delta queue
    pub fn new() -> Self {
        Self {
            deltas: Vec::new(),
            classifier: PriorityClassifier::new(),
        }
    }

    /// Create queue with custom classifier
    pub fn with_classifier(classifier: PriorityClassifier) -> Self {
        Self {
            deltas: Vec::new(),
            classifier,
        }
    }

    /// Enqueue a delta
    pub fn enqueue(&mut self, delta: Delta) {
        let prioritized = self.classifier.prioritize(delta);
        self.deltas.push(prioritized);
        self.deltas.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    /// Enqueue a batch of deltas
    pub fn enqueue_batch(&mut self, batch: &DeltaBatch) {
        for delta in &batch.deltas {
            self.enqueue(delta.clone());
        }
    }

    /// Dequeue the highest priority delta
    pub fn dequeue(&mut self) -> Option<PrioritizedDelta> {
        if self.deltas.is_empty() {
            return None;
        }
        Some(self.deltas.remove(0))
    }

    /// Peek at the highest priority delta without removing it
    pub fn peek(&self) -> Option<&PrioritizedDelta> {
        self.deltas.first()
    }

    /// Remove expired deltas
    pub fn remove_expired(&mut self) -> usize {
        let before = self.deltas.len();
        self.deltas.retain(|d| !d.is_expired());
        before - self.deltas.len()
    }

    /// Get count of queued deltas
    pub fn len(&self) -> usize {
        self.deltas.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    /// Get count of deltas by priority
    pub fn count_by_priority(&self) -> [usize; 4] {
        let mut counts = [0; 4];
        for delta in &self.deltas {
            counts[delta.priority as usize] += 1;
        }
        counts
    }
}

impl Default for DeltaQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);

        assert!(Priority::Critical.is_higher_than(&Priority::Low));
        assert!(!Priority::Low.is_higher_than(&Priority::Critical));
    }

    #[test]
    fn test_priority_as_u8() {
        assert_eq!(Priority::Critical.as_u8(), 0);
        assert_eq!(Priority::High.as_u8(), 1);
        assert_eq!(Priority::Medium.as_u8(), 2);
        assert_eq!(Priority::Low.as_u8(), 3);
    }

    #[test]
    fn test_classify_capability_loss() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetRemove {
                field: "capabilities".to_string(),
                tag: "cap_123".to_string(),
            }],
        );

        assert_eq!(classifier.classify(&delta), Priority::Critical);
    }

    #[test]
    fn test_classify_member_removal() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetRemove {
                field: "members".to_string(),
                tag: "node_123".to_string(),
            }],
        );

        assert_eq!(classifier.classify(&delta), Priority::High);
    }

    #[test]
    fn test_classify_member_addition() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetAdd {
                field: "members".to_string(),
                element: serde_json::json!("node1"),
                tag: "add_123".to_string(),
            }],
        );

        assert_eq!(classifier.classify(&delta), Priority::High);
    }

    #[test]
    fn test_classify_leader_change() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
        );

        assert_eq!(classifier.classify(&delta), Priority::Medium);
    }

    #[test]
    fn test_classify_capability_addition() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::GSetAdd {
                field: "capabilities".to_string(),
                element: serde_json::json!({"type": "Sensor"}),
            }],
        );

        assert_eq!(classifier.classify(&delta), Priority::Low);
    }

    #[test]
    fn test_classify_multiple_operations() {
        let classifier = PriorityClassifier::new();

        // Mixed operations - highest priority wins
        let delta = create_test_delta(
            "cell1",
            1,
            vec![
                DeltaOp::GSetAdd {
                    field: "capabilities".to_string(),
                    element: serde_json::json!({"type": "Sensor"}),
                }, // Low
                DeltaOp::LwwSet {
                    field: "leader_id".to_string(),
                    value: serde_json::json!("node1"),
                    timestamp: 12345,
                }, // Medium
                DeltaOp::OrSetRemove {
                    field: "members".to_string(),
                    tag: "node_123".to_string(),
                }, // High
            ],
        );

        assert_eq!(classifier.classify(&delta), Priority::High);
    }

    #[test]
    fn test_prioritize_delta() {
        let classifier = PriorityClassifier::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetRemove {
                field: "capabilities".to_string(),
                tag: "cap_123".to_string(),
            }],
        );

        let prioritized = classifier.prioritize(delta);

        assert_eq!(prioritized.priority, Priority::Critical);
        assert!(!prioritized.is_expired());
        assert!(prioritized.remaining_ttl().is_some());
    }

    #[test]
    fn test_ttl_expiration() {
        let classifier = PriorityClassifier::with_ttls(
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(10),
        );

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
        );

        let prioritized = classifier.prioritize(delta);
        assert!(!prioritized.is_expired());

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(15));

        assert!(prioritized.is_expired());
        assert!(prioritized.remaining_ttl().is_none());
    }

    #[test]
    fn test_expiring_soon() {
        let classifier = PriorityClassifier::with_ttls(
            Duration::from_millis(100),
            Duration::from_millis(100),
            Duration::from_millis(100),
            Duration::from_millis(100),
        );

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
        );

        let prioritized = classifier.prioritize(delta);

        // Should be expiring soon with 200ms threshold
        assert!(prioritized.is_expiring_soon(Duration::from_millis(200)));

        // Should not be expiring soon with 10ms threshold
        assert!(!prioritized.is_expiring_soon(Duration::from_millis(10)));
    }

    #[test]
    fn test_delta_queue_enqueue_dequeue() {
        let mut queue = DeltaQueue::new();

        let delta1 = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::GSetAdd {
                field: "capabilities".to_string(),
                element: serde_json::json!({"type": "Sensor"}),
            }],
        ); // Low priority

        let delta2 = create_test_delta(
            "cell1",
            2,
            vec![DeltaOp::OrSetRemove {
                field: "capabilities".to_string(),
                tag: "cap_123".to_string(),
            }],
        ); // Critical priority

        queue.enqueue(delta1);
        queue.enqueue(delta2);

        assert_eq!(queue.len(), 2);

        // Should dequeue Critical first
        let first = queue.dequeue().unwrap();
        assert_eq!(first.priority, Priority::Critical);

        // Then Low
        let second = queue.dequeue().unwrap();
        assert_eq!(second.priority, Priority::Low);

        assert!(queue.is_empty());
    }

    #[test]
    fn test_delta_queue_peek() {
        let mut queue = DeltaQueue::new();

        let delta = create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::OrSetRemove {
                field: "capabilities".to_string(),
                tag: "cap_123".to_string(),
            }],
        );

        queue.enqueue(delta);

        let peeked = queue.peek().unwrap();
        assert_eq!(peeked.priority, Priority::Critical);
        assert_eq!(queue.len(), 1); // Still there

        let dequeued = queue.dequeue().unwrap();
        assert_eq!(dequeued.priority, Priority::Critical);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_delta_queue_remove_expired() {
        let classifier = PriorityClassifier::with_ttls(
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(10),
        );

        let mut queue = DeltaQueue::with_classifier(classifier);

        for i in 0..5 {
            let delta = create_test_delta(
                &format!("cell{}", i),
                1,
                vec![DeltaOp::LwwSet {
                    field: "leader_id".to_string(),
                    value: serde_json::json!("node1"),
                    timestamp: 12345,
                }],
            );
            queue.enqueue(delta);
        }

        assert_eq!(queue.len(), 5);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(15));

        let removed = queue.remove_expired();
        assert_eq!(removed, 5);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_count_by_priority() {
        let mut queue = DeltaQueue::new();

        // Add 2 critical, 3 high, 1 medium, 2 low
        for _ in 0..2 {
            queue.enqueue(create_test_delta(
                "cell1",
                1,
                vec![DeltaOp::OrSetRemove {
                    field: "capabilities".to_string(),
                    tag: "cap_123".to_string(),
                }],
            ));
        }

        for _ in 0..3 {
            queue.enqueue(create_test_delta(
                "cell1",
                1,
                vec![DeltaOp::OrSetRemove {
                    field: "members".to_string(),
                    tag: "node_123".to_string(),
                }],
            ));
        }

        queue.enqueue(create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
        ));

        for _ in 0..2 {
            queue.enqueue(create_test_delta(
                "cell1",
                1,
                vec![DeltaOp::GSetAdd {
                    field: "capabilities".to_string(),
                    element: serde_json::json!({"type": "Sensor"}),
                }],
            ));
        }

        let counts = queue.count_by_priority();
        assert_eq!(counts[Priority::Critical as usize], 2);
        assert_eq!(counts[Priority::High as usize], 3);
        assert_eq!(counts[Priority::Medium as usize], 1);
        assert_eq!(counts[Priority::Low as usize], 2);
    }

    #[test]
    fn test_enqueue_batch() {
        let mut queue = DeltaQueue::new();
        let mut batch = DeltaBatch::new();

        batch.add(create_test_delta(
            "cell1",
            1,
            vec![DeltaOp::LwwSet {
                field: "leader_id".to_string(),
                value: serde_json::json!("node1"),
                timestamp: 12345,
            }],
        ));

        batch.add(create_test_delta(
            "cell2",
            1,
            vec![DeltaOp::GSetAdd {
                field: "capabilities".to_string(),
                element: serde_json::json!({"type": "Sensor"}),
            }],
        ));

        queue.enqueue_batch(&batch);
        assert_eq!(queue.len(), 2);
    }
}
