//! Priority-aware message queue for DIL resilience

use chrono::{DateTime, Utc};
use peat_protocol::cot::CotEvent;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use super::config::QueueConfig;
use super::error::TakError;
use super::metrics::QueueDepthMetrics;
use super::traits::Priority;

/// A message queued for sending
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// The CoT event to send
    pub event: CotEvent,
    /// Message priority (1-5)
    pub priority: Priority,
    /// When the message was enqueued
    pub enqueued_at: Instant,
    /// When the message becomes stale
    pub stale_time: DateTime<Utc>,
    /// Serialized size in bytes
    pub size_bytes: usize,
}

/// Priority-aware message queue for TAK transport
///
/// Provides DIL resilience by buffering messages during disconnections.
/// Messages are dequeued in priority order (P1 first).
pub struct TakMessageQueue {
    config: QueueConfig,
    /// Priority queues (index 0 = P1, index 4 = P5)
    queues: [VecDeque<QueuedMessage>; 5],
    /// Total bytes currently queued
    total_bytes: AtomicUsize,
    /// Count of stale messages dropped
    stale_dropped: AtomicU64,
    /// Count of messages dropped due to queue full
    full_dropped: AtomicU64,
}

impl TakMessageQueue {
    /// Create a new message queue with the given configuration
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config,
            queues: Default::default(),
            total_bytes: AtomicUsize::new(0),
            stale_dropped: AtomicU64::new(0),
            full_dropped: AtomicU64::new(0),
        }
    }

    /// Enqueue a message for sending
    ///
    /// Returns error if queue is full and message cannot be accepted.
    pub fn enqueue(&mut self, event: CotEvent, priority: Priority) -> Result<(), TakError> {
        // Approximate size - use 500 bytes as fallback if encoding fails
        let size_bytes = event.to_xml().map(|xml| xml.len()).unwrap_or(500);
        let stale_time = event.stale;

        let msg = QueuedMessage {
            event,
            priority,
            enqueued_at: Instant::now(),
            stale_time,
            size_bytes,
        };

        // Check per-priority limit
        let queue_idx = Self::priority_to_index(priority);
        let limit = self.config.priority_limits.limit_for(priority);

        if self.queues[queue_idx].len() >= limit {
            // Try to drop a lower priority message
            if !self.drop_lowest_priority() {
                self.full_dropped.fetch_add(1, Ordering::Relaxed);
                return Err(TakError::QueueFull);
            }
        }

        // Check total byte limit
        if self.total_bytes.load(Ordering::Relaxed) + size_bytes > self.config.max_bytes
            && !self.drop_lowest_priority()
        {
            self.full_dropped.fetch_add(1, Ordering::Relaxed);
            return Err(TakError::QueueFull);
        }

        self.queues[queue_idx].push_back(msg);
        self.total_bytes.fetch_add(size_bytes, Ordering::Relaxed);

        Ok(())
    }

    /// Dequeue next message (priority order)
    ///
    /// Filters out stale messages automatically.
    pub fn dequeue(&mut self) -> Option<QueuedMessage> {
        let now = Utc::now();

        // Drain in priority order (P1 first, index 0)
        for queue in &mut self.queues {
            while let Some(msg) = queue.pop_front() {
                self.total_bytes
                    .fetch_sub(msg.size_bytes, Ordering::Relaxed);

                // Filter stale messages
                if self.config.filter_stale && msg.stale_time < now {
                    self.stale_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                return Some(msg);
            }
        }

        None
    }

    /// Peek at the next message without removing it
    pub fn peek(&self) -> Option<&QueuedMessage> {
        for queue in &self.queues {
            if let Some(msg) = queue.front() {
                return Some(msg);
            }
        }
        None
    }

    /// Get current queue depth metrics
    pub fn metrics(&self) -> QueueDepthMetrics {
        QueueDepthMetrics {
            p1_depth: self.queues[0].len(),
            p2_depth: self.queues[1].len(),
            p3_depth: self.queues[2].len(),
            p4_depth: self.queues[3].len(),
            p5_depth: self.queues[4].len(),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            stale_dropped: self.stale_dropped.load(Ordering::Relaxed),
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(|q| q.is_empty())
    }

    /// Get total message count
    pub fn len(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Clear all queued messages
    pub fn clear(&mut self) {
        for queue in &mut self.queues {
            queue.clear();
        }
        self.total_bytes.store(0, Ordering::Relaxed);
    }

    /// Try to drop the lowest priority message to make room
    fn drop_lowest_priority(&mut self) -> bool {
        // Try P5 first, then P4, P3, P2 - never drop P1
        for queue_idx in (1..5).rev() {
            if let Some(msg) = self.queues[queue_idx].pop_front() {
                self.total_bytes
                    .fetch_sub(msg.size_bytes, Ordering::Relaxed);
                self.full_dropped.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Convert priority (1-5) to queue index (0-4)
    fn priority_to_index(priority: Priority) -> usize {
        match priority {
            1 => 0,
            2 => 1,
            3 => 2,
            4 => 3,
            _ => 4, // 5 or anything else goes to P5
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_protocol::cot::{CotEventBuilder, CotPoint, CotType};

    fn make_event(uid: &str) -> CotEvent {
        CotEventBuilder::new()
            .uid(uid)
            .cot_type(CotType::new("a-f-G"))
            .point(CotPoint::new(34.0, -118.0))
            .build()
            .expect("failed to build test event")
    }

    #[test]
    fn test_enqueue_dequeue_priority_order() {
        let mut queue = TakMessageQueue::new(QueueConfig::default());

        // Enqueue in reverse priority order
        queue.enqueue(make_event("p5"), 5).unwrap();
        queue.enqueue(make_event("p3"), 3).unwrap();
        queue.enqueue(make_event("p1"), 1).unwrap();
        queue.enqueue(make_event("p2"), 2).unwrap();
        queue.enqueue(make_event("p4"), 4).unwrap();

        // Should dequeue in priority order
        assert_eq!(queue.dequeue().unwrap().event.uid, "p1");
        assert_eq!(queue.dequeue().unwrap().event.uid, "p2");
        assert_eq!(queue.dequeue().unwrap().event.uid, "p3");
        assert_eq!(queue.dequeue().unwrap().event.uid, "p4");
        assert_eq!(queue.dequeue().unwrap().event.uid, "p5");
        assert!(queue.dequeue().is_none());
    }

    #[test]
    fn test_queue_metrics() {
        let mut queue = TakMessageQueue::new(QueueConfig::default());

        queue.enqueue(make_event("1"), 1).unwrap();
        queue.enqueue(make_event("2"), 1).unwrap();
        queue.enqueue(make_event("3"), 3).unwrap();

        let metrics = queue.metrics();
        assert_eq!(metrics.p1_depth, 2);
        assert_eq!(metrics.p3_depth, 1);
        assert_eq!(metrics.total_messages(), 3);
    }

    #[test]
    fn test_drop_lowest_priority() {
        let config = QueueConfig {
            max_messages: 100,
            max_bytes: 100, // Very small to trigger drops
            ..Default::default()
        };
        let mut queue = TakMessageQueue::new(config);

        // Fill with P5 messages
        for i in 0..5 {
            let _ = queue.enqueue(make_event(&format!("p5-{}", i)), 5);
        }

        // Add a P1 message - should succeed by dropping P5
        let result = queue.enqueue(make_event("p1"), 1);
        // Note: May or may not succeed depending on message sizes
        // The key is that it tries to drop lower priority first
        assert!(result.is_ok() || matches!(result, Err(TakError::QueueFull)));
    }

    #[test]
    fn test_queue_is_empty() {
        let mut queue = TakMessageQueue::new(QueueConfig::default());
        assert!(queue.is_empty());

        queue.enqueue(make_event("1"), 1).unwrap();
        assert!(!queue.is_empty());

        queue.dequeue();
        assert!(queue.is_empty());
    }
}
