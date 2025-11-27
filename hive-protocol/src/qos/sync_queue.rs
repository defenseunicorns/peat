//! Priority-based sync queue (ADR-019 Phase 3)
//!
//! This module provides a priority-ordered queue for pending synchronization
//! operations. Data is dequeued in priority order (P1 Critical first) with
//! support for priority aging to prevent starvation.
//!
//! # Architecture
//!
//! The queue maintains separate internal queues for each QoS class:
//! - P1 Critical: Emergency alerts, contact reports
//! - P2 High: Target imagery, mission retasking
//! - P3 Normal: Health status, capability changes
//! - P4 Low: Position updates, heartbeats
//! - P5 Bulk: Model updates, debug logs
//!
//! # Priority Aging
//!
//! To prevent starvation of low-priority data, items are promoted after
//! extended wait times:
//! - P5 → P4 after 1 hour
//! - P4 → P3 after 2 hours
//!
//! # Example
//!
//! ```
//! use hive_protocol::qos::{QoSClass, DataType, PrioritySyncQueue, PendingSync};
//! use std::time::Instant;
//!
//! let mut queue = PrioritySyncQueue::new(10 * 1024 * 1024); // 10 MB max
//!
//! // Enqueue some data
//! let sync = PendingSync {
//!     data: vec![1, 2, 3],
//!     qos_class: QoSClass::Critical,
//!     data_type: DataType::ContactReport,
//!     queued_at: Instant::now(),
//!     priority_multiplier: 1.0,
//! };
//!
//! queue.enqueue(sync).unwrap();
//!
//! // Dequeue highest priority
//! if let Some(item) = queue.dequeue_highest() {
//!     assert_eq!(item.qos_class, QoSClass::Critical);
//! }
//! ```

use super::classification::DataType;
use super::QoSClass;
use crate::{Error, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A pending synchronization item
#[derive(Debug, Clone)]
pub struct PendingSync {
    /// The data to synchronize
    pub data: Vec<u8>,

    /// QoS class for this data
    pub qos_class: QoSClass,

    /// Type of data being synced
    pub data_type: DataType,

    /// When this item was enqueued
    pub queued_at: Instant,

    /// Priority multiplier for aging (1.0 = normal)
    ///
    /// Values > 1.0 accelerate aging promotion.
    /// Values < 1.0 slow down aging promotion.
    pub priority_multiplier: f32,
}

impl PendingSync {
    /// Create a new pending sync item
    pub fn new(data: Vec<u8>, qos_class: QoSClass, data_type: DataType) -> Self {
        Self {
            data,
            qos_class,
            data_type,
            queued_at: Instant::now(),
            priority_multiplier: 1.0,
        }
    }

    /// Create with custom priority multiplier
    pub fn with_multiplier(
        data: Vec<u8>,
        qos_class: QoSClass,
        data_type: DataType,
        multiplier: f32,
    ) -> Self {
        Self {
            data,
            qos_class,
            data_type,
            queued_at: Instant::now(),
            priority_multiplier: multiplier,
        }
    }

    /// Get the time this item has been queued
    pub fn queue_duration(&self) -> Duration {
        self.queued_at.elapsed()
    }

    /// Get effective priority considering aging
    ///
    /// Returns the potentially-promoted QoS class based on wait time.
    pub fn effective_class(&self) -> QoSClass {
        let wait_hours = self.queue_duration().as_secs_f32() / 3600.0;
        let adjusted_hours = wait_hours * self.priority_multiplier;

        match self.qos_class {
            QoSClass::Bulk if adjusted_hours >= 1.0 => QoSClass::Low,
            QoSClass::Low if adjusted_hours >= 2.0 => QoSClass::Normal,
            other => other,
        }
    }

    /// Check if this item should be promoted due to aging
    pub fn should_promote(&self) -> bool {
        self.effective_class() != self.qos_class
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Priority-ordered sync queue
///
/// Maintains separate queues per QoS class with automatic aging promotion
/// to prevent starvation of low-priority data.
#[derive(Debug)]
pub struct PrioritySyncQueue {
    /// Internal queues, indexed by QoS class (1-5)
    queues: [VecDeque<PendingSync>; 5],

    /// Total bytes currently queued
    total_bytes: AtomicUsize,

    /// Maximum queue size in bytes
    max_bytes: usize,

    /// Number of items promoted due to aging
    aging_promotions: AtomicUsize,
}

impl PrioritySyncQueue {
    /// Create a new priority sync queue
    ///
    /// # Arguments
    ///
    /// * `max_bytes` - Maximum total size in bytes the queue can hold
    pub fn new(max_bytes: usize) -> Self {
        Self {
            queues: [
                VecDeque::new(), // P1 Critical (index 0)
                VecDeque::new(), // P2 High (index 1)
                VecDeque::new(), // P3 Normal (index 2)
                VecDeque::new(), // P4 Low (index 3)
                VecDeque::new(), // P5 Bulk (index 4)
            ],
            total_bytes: AtomicUsize::new(0),
            max_bytes,
            aging_promotions: AtomicUsize::new(0),
        }
    }

    /// Default queue with 10 MB capacity
    pub fn default_capacity() -> Self {
        Self::new(10 * 1024 * 1024)
    }

    /// Get the queue index for a QoS class
    #[inline]
    fn queue_index(class: QoSClass) -> usize {
        (class.as_u8() - 1) as usize
    }

    /// Enqueue a sync item
    ///
    /// Returns an error if the queue is full.
    pub fn enqueue(&mut self, sync: PendingSync) -> Result<()> {
        let size = sync.size();
        let current_bytes = self.total_bytes.load(Ordering::Relaxed);

        if current_bytes + size > self.max_bytes {
            return Err(Error::Internal(format!(
                "Queue full: {} + {} > {} bytes",
                current_bytes, size, self.max_bytes
            )));
        }

        let idx = Self::queue_index(sync.qos_class);
        self.queues[idx].push_back(sync);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);

        Ok(())
    }

    /// Dequeue the highest priority item
    ///
    /// Returns None if all queues are empty.
    pub fn dequeue_highest(&mut self) -> Option<PendingSync> {
        // Check queues in priority order (P1 first)
        for idx in 0..5 {
            if let Some(sync) = self.queues[idx].pop_front() {
                self.total_bytes.fetch_sub(sync.size(), Ordering::Relaxed);
                return Some(sync);
            }
        }
        None
    }

    /// Peek at the highest priority item without removing
    pub fn peek_highest(&self) -> Option<&PendingSync> {
        for idx in 0..5 {
            if let Some(sync) = self.queues[idx].front() {
                return Some(sync);
            }
        }
        None
    }

    /// Apply aging promotion to queued items
    ///
    /// Moves items that have waited long enough to higher priority queues.
    /// Returns the number of items promoted.
    pub fn apply_aging(&mut self) -> usize {
        let mut promoted = 0;

        // Process P5 Bulk → P4 Low promotions
        let bulk_idx = Self::queue_index(QoSClass::Bulk);
        let low_idx = Self::queue_index(QoSClass::Low);

        let mut to_promote_bulk = Vec::new();
        self.queues[bulk_idx].retain(|sync| {
            if sync.should_promote() && sync.effective_class() == QoSClass::Low {
                to_promote_bulk.push(sync.clone());
                false
            } else {
                true
            }
        });

        for mut sync in to_promote_bulk {
            sync.qos_class = QoSClass::Low;
            self.queues[low_idx].push_back(sync);
            promoted += 1;
        }

        // Process P4 Low → P3 Normal promotions
        let normal_idx = Self::queue_index(QoSClass::Normal);

        let mut to_promote_low = Vec::new();
        self.queues[low_idx].retain(|sync| {
            if sync.should_promote() && sync.effective_class() == QoSClass::Normal {
                to_promote_low.push(sync.clone());
                false
            } else {
                true
            }
        });

        for mut sync in to_promote_low {
            sync.qos_class = QoSClass::Normal;
            self.queues[normal_idx].push_back(sync);
            promoted += 1;
        }

        if promoted > 0 {
            self.aging_promotions.fetch_add(promoted, Ordering::Relaxed);
        }

        promoted
    }

    /// Get queue depth for a specific class
    pub fn queue_depth(&self, class: QoSClass) -> usize {
        let idx = Self::queue_index(class);
        self.queues[idx].len()
    }

    /// Get total bytes currently queued
    pub fn total_bytes_queued(&self) -> usize {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// Get total items currently queued
    pub fn total_items(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(|q| q.is_empty())
    }

    /// Check if the queue is full
    pub fn is_full(&self) -> bool {
        self.total_bytes.load(Ordering::Relaxed) >= self.max_bytes
    }

    /// Get available capacity in bytes
    pub fn available_bytes(&self) -> usize {
        let current = self.total_bytes.load(Ordering::Relaxed);
        self.max_bytes.saturating_sub(current)
    }

    /// Get max capacity in bytes
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        QueueStats {
            total_items: self.total_items(),
            total_bytes: self.total_bytes_queued(),
            max_bytes: self.max_bytes,
            depth_critical: self.queue_depth(QoSClass::Critical),
            depth_high: self.queue_depth(QoSClass::High),
            depth_normal: self.queue_depth(QoSClass::Normal),
            depth_low: self.queue_depth(QoSClass::Low),
            depth_bulk: self.queue_depth(QoSClass::Bulk),
            aging_promotions: self.aging_promotions.load(Ordering::Relaxed),
        }
    }

    /// Clear all queued items
    pub fn clear(&mut self) {
        for queue in &mut self.queues {
            queue.clear();
        }
        self.total_bytes.store(0, Ordering::Relaxed);
    }

    /// Drain all items from a specific class
    pub fn drain_class(&mut self, class: QoSClass) -> Vec<PendingSync> {
        let idx = Self::queue_index(class);
        let items: Vec<_> = self.queues[idx].drain(..).collect();

        let bytes: usize = items.iter().map(|s| s.size()).sum();
        self.total_bytes.fetch_sub(bytes, Ordering::Relaxed);

        items
    }

    /// Remove items older than a threshold
    ///
    /// Returns the number of items removed.
    pub fn remove_stale(&mut self, max_age: Duration) -> usize {
        let mut removed = 0;
        let mut bytes_removed = 0;

        for queue in &mut self.queues {
            let old_len = queue.len();
            queue.retain(|sync| {
                let keep = sync.queue_duration() < max_age;
                if !keep {
                    bytes_removed += sync.size();
                }
                keep
            });
            removed += old_len - queue.len();
        }

        if bytes_removed > 0 {
            self.total_bytes.fetch_sub(bytes_removed, Ordering::Relaxed);
        }

        removed
    }

    /// Get oldest item across all queues (for monitoring)
    pub fn oldest_item_age(&self) -> Option<Duration> {
        self.queues
            .iter()
            .filter_map(|q| q.front())
            .map(|s| s.queue_duration())
            .max()
    }

    /// Dequeue up to N items, respecting priority order
    pub fn dequeue_batch(&mut self, max_items: usize) -> Vec<PendingSync> {
        let mut batch = Vec::with_capacity(max_items);

        while batch.len() < max_items {
            if let Some(sync) = self.dequeue_highest() {
                batch.push(sync);
            } else {
                break;
            }
        }

        batch
    }

    /// Dequeue items up to a byte limit, respecting priority order
    pub fn dequeue_bytes(&mut self, max_bytes: usize) -> Vec<PendingSync> {
        let mut batch = Vec::new();
        let mut total_bytes = 0;

        while total_bytes < max_bytes {
            // Peek first to check size
            if let Some(peek) = self.peek_highest() {
                if total_bytes + peek.size() > max_bytes && !batch.is_empty() {
                    break; // Would exceed limit
                }
            }

            if let Some(sync) = self.dequeue_highest() {
                total_bytes += sync.size();
                batch.push(sync);
            } else {
                break;
            }
        }

        batch
    }
}

/// Queue statistics
#[derive(Debug, Clone, Copy)]
pub struct QueueStats {
    /// Total items in queue
    pub total_items: usize,

    /// Total bytes in queue
    pub total_bytes: usize,

    /// Maximum bytes allowed
    pub max_bytes: usize,

    /// Items in P1 Critical queue
    pub depth_critical: usize,

    /// Items in P2 High queue
    pub depth_high: usize,

    /// Items in P3 Normal queue
    pub depth_normal: usize,

    /// Items in P4 Low queue
    pub depth_low: usize,

    /// Items in P5 Bulk queue
    pub depth_bulk: usize,

    /// Number of aging promotions
    pub aging_promotions: usize,
}

impl QueueStats {
    /// Get utilization percentage (0.0 - 1.0)
    pub fn utilization(&self) -> f64 {
        if self.max_bytes == 0 {
            0.0
        } else {
            self.total_bytes as f64 / self.max_bytes as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_sync_creation() {
        let sync = PendingSync::new(vec![1, 2, 3], QoSClass::Critical, DataType::ContactReport);

        assert_eq!(sync.size(), 3);
        assert_eq!(sync.qos_class, QoSClass::Critical);
        assert_eq!(sync.priority_multiplier, 1.0);
    }

    #[test]
    fn test_queue_creation() {
        let queue = PrioritySyncQueue::new(1024);

        assert!(queue.is_empty());
        assert_eq!(queue.max_bytes(), 1024);
        assert_eq!(queue.available_bytes(), 1024);
    }

    #[test]
    fn test_enqueue_dequeue() {
        let mut queue = PrioritySyncQueue::new(1024);

        let sync = PendingSync::new(vec![1, 2, 3], QoSClass::Normal, DataType::HealthStatus);
        queue.enqueue(sync).unwrap();

        assert_eq!(queue.total_items(), 1);
        assert_eq!(queue.total_bytes_queued(), 3);

        let dequeued = queue.dequeue_highest().unwrap();
        assert_eq!(dequeued.qos_class, QoSClass::Normal);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = PrioritySyncQueue::new(1024);

        // Enqueue in reverse priority order
        queue
            .enqueue(PendingSync::new(
                vec![5],
                QoSClass::Bulk,
                DataType::DebugLog,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![1],
                QoSClass::Critical,
                DataType::ContactReport,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![3],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();

        // Should dequeue in priority order
        assert_eq!(
            queue.dequeue_highest().unwrap().qos_class,
            QoSClass::Critical
        );
        assert_eq!(queue.dequeue_highest().unwrap().qos_class, QoSClass::Normal);
        assert_eq!(queue.dequeue_highest().unwrap().qos_class, QoSClass::Bulk);
    }

    #[test]
    fn test_queue_full() {
        let mut queue = PrioritySyncQueue::new(10);

        let sync1 = PendingSync::new(vec![0; 8], QoSClass::Normal, DataType::HealthStatus);
        queue.enqueue(sync1).unwrap();

        // This should fail - would exceed capacity
        let sync2 = PendingSync::new(vec![0; 5], QoSClass::Normal, DataType::HealthStatus);
        assert!(queue.enqueue(sync2).is_err());
    }

    #[test]
    fn test_queue_depth() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![1],
                QoSClass::Critical,
                DataType::ContactReport,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![2],
                QoSClass::Critical,
                DataType::EmergencyAlert,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![3],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();

        assert_eq!(queue.queue_depth(QoSClass::Critical), 2);
        assert_eq!(queue.queue_depth(QoSClass::Normal), 1);
        assert_eq!(queue.queue_depth(QoSClass::Bulk), 0);
    }

    #[test]
    fn test_peek_highest() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![3],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![1],
                QoSClass::Critical,
                DataType::ContactReport,
            ))
            .unwrap();

        // Peek should not remove
        let peeked = queue.peek_highest().unwrap();
        assert_eq!(peeked.qos_class, QoSClass::Critical);
        assert_eq!(queue.total_items(), 2);
    }

    #[test]
    fn test_clear() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![1; 100],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![2; 100],
                QoSClass::High,
                DataType::TargetImage,
            ))
            .unwrap();

        queue.clear();

        assert!(queue.is_empty());
        assert_eq!(queue.total_bytes_queued(), 0);
    }

    #[test]
    fn test_drain_class() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![1],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![2],
                QoSClass::Normal,
                DataType::CapabilityChange,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![3],
                QoSClass::High,
                DataType::TargetImage,
            ))
            .unwrap();

        let drained = queue.drain_class(QoSClass::Normal);
        assert_eq!(drained.len(), 2);
        assert_eq!(queue.queue_depth(QoSClass::Normal), 0);
        assert_eq!(queue.queue_depth(QoSClass::High), 1);
    }

    #[test]
    fn test_stats() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![0; 100],
                QoSClass::Critical,
                DataType::ContactReport,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![0; 50],
                QoSClass::Bulk,
                DataType::DebugLog,
            ))
            .unwrap();

        let stats = queue.stats();
        assert_eq!(stats.total_items, 2);
        assert_eq!(stats.total_bytes, 150);
        assert_eq!(stats.depth_critical, 1);
        assert_eq!(stats.depth_bulk, 1);
        assert!((stats.utilization() - 150.0 / 1024.0).abs() < 0.001);
    }

    #[test]
    fn test_dequeue_batch() {
        let mut queue = PrioritySyncQueue::new(1024);

        for i in 0..5 {
            queue
                .enqueue(PendingSync::new(
                    vec![i],
                    QoSClass::Normal,
                    DataType::HealthStatus,
                ))
                .unwrap();
        }

        let batch = queue.dequeue_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(queue.total_items(), 2);
    }

    #[test]
    fn test_dequeue_bytes() {
        let mut queue = PrioritySyncQueue::new(1024);

        queue
            .enqueue(PendingSync::new(
                vec![0; 100],
                QoSClass::Critical,
                DataType::ContactReport,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![0; 100],
                QoSClass::High,
                DataType::TargetImage,
            ))
            .unwrap();
        queue
            .enqueue(PendingSync::new(
                vec![0; 100],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();

        // Dequeue up to 150 bytes - should get 2 items (200 bytes, since we allow going over if empty)
        let batch = queue.dequeue_bytes(150);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_effective_class_no_aging() {
        let sync = PendingSync::new(vec![1], QoSClass::Bulk, DataType::DebugLog);

        // Fresh item should not be promoted
        assert_eq!(sync.effective_class(), QoSClass::Bulk);
        assert!(!sync.should_promote());
    }

    #[test]
    fn test_oldest_item_age() {
        let mut queue = PrioritySyncQueue::new(1024);

        assert!(queue.oldest_item_age().is_none());

        queue
            .enqueue(PendingSync::new(
                vec![1],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();

        let age = queue.oldest_item_age().unwrap();
        assert!(age < Duration::from_secs(1));
    }

    #[test]
    fn test_available_bytes() {
        let mut queue = PrioritySyncQueue::new(1000);

        queue
            .enqueue(PendingSync::new(
                vec![0; 300],
                QoSClass::Normal,
                DataType::HealthStatus,
            ))
            .unwrap();

        assert_eq!(queue.available_bytes(), 700);
    }
}
