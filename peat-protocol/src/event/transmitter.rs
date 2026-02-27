//! Event Transmitter with Bandwidth Control (ADR-027 Phase 4)
//!
//! Implements weighted fair queuing and bandwidth allocation for priority-based
//! event transmission.
//!
//! ## Bandwidth Allocation
//!
//! ```text
//! Total Bandwidth
//!     ├── CRITICAL (reserved, preempts all)
//!     └── Remaining
//!         ├── HIGH    (50%)
//!         ├── NORMAL  (35%)
//!         └── LOW     (15%)
//! ```
//!
//! ## Token Bucket Rate Limiting
//!
//! Each priority level uses a token bucket to enforce bandwidth limits.
//! Tokens are replenished at a configurable rate.

use peat_schema::event::v1::{EventPriority, PeatEvent};
use std::collections::VecDeque;
use std::time::Instant;

/// Bandwidth allocation configuration
#[derive(Debug, Clone, Copy)]
pub struct BandwidthAllocation {
    /// Reserved bandwidth for CRITICAL events (bytes/second)
    pub critical_reserved_bps: u64,

    /// Minimum bandwidth for HIGH priority (bytes/second)
    pub high_min_bps: u64,

    /// Minimum bandwidth for NORMAL priority (bytes/second)
    pub normal_min_bps: u64,

    /// Minimum bandwidth for LOW priority (bytes/second)
    pub low_min_bps: u64,

    /// Total available bandwidth (bytes/second)
    pub total_available_bps: u64,
}

impl Default for BandwidthAllocation {
    fn default() -> Self {
        // Default: 1 Mbps total, with standard 50/35/15 split
        let total = 1_000_000; // 1 Mbps
        Self {
            critical_reserved_bps: total / 10, // 10% reserved for critical
            high_min_bps: (total * 9 / 10) * 50 / 100, // 50% of remaining
            normal_min_bps: (total * 9 / 10) * 35 / 100, // 35% of remaining
            low_min_bps: (total * 9 / 10) * 15 / 100, // 15% of remaining
            total_available_bps: total,
        }
    }
}

impl BandwidthAllocation {
    /// Create a new bandwidth allocation
    pub fn new(total_bps: u64) -> Self {
        let non_critical = total_bps * 90 / 100; // 90% for non-critical
        Self {
            critical_reserved_bps: total_bps / 10,
            high_min_bps: non_critical * 50 / 100,
            normal_min_bps: non_critical * 35 / 100,
            low_min_bps: non_critical * 15 / 100,
            total_available_bps: total_bps,
        }
    }

    /// Create with custom percentages
    pub fn with_percentages(
        total_bps: u64,
        critical_pct: u8,
        high_pct: u8,
        normal_pct: u8,
        low_pct: u8,
    ) -> Self {
        assert!(
            critical_pct + high_pct + normal_pct + low_pct <= 100,
            "Percentages must sum to <= 100"
        );
        Self {
            critical_reserved_bps: total_bps * critical_pct as u64 / 100,
            high_min_bps: total_bps * high_pct as u64 / 100,
            normal_min_bps: total_bps * normal_pct as u64 / 100,
            low_min_bps: total_bps * low_pct as u64 / 100,
            total_available_bps: total_bps,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Current tokens available
    tokens: f64,

    /// Maximum tokens (bucket size)
    capacity: f64,

    /// Token refill rate (tokens per second)
    rate: f64,

    /// Last refill timestamp
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(capacity: f64, rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume tokens
    ///
    /// Returns true if tokens were consumed, false if insufficient tokens.
    fn try_consume(&mut self, count: f64) -> bool {
        self.refill();
        if self.tokens >= count {
            self.tokens -= count;
            true
        } else {
            false
        }
    }

    /// Get current token count
    fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity);
        self.last_refill = now;
    }
}

/// Queue overflow policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Reject incoming event if queue full
    RejectNew,
    /// Remove oldest event in queue
    RemoveOldest,
    /// Remove lowest priority event
    RemoveLowestPriority,
}

/// Event transmitter with bandwidth control
///
/// Manages event queues with:
/// - Weighted fair queuing for priority levels
/// - Token bucket rate limiting
/// - Queue overflow handling
pub struct EventTransmitter {
    /// Priority queues for events
    queues: [VecDeque<PeatEvent>; 4],

    /// Maximum queue sizes per priority
    max_queue_sizes: [usize; 4],

    /// Token buckets for rate limiting
    buckets: [TokenBucket; 4],

    /// Bandwidth allocation (stored for reference/debugging)
    #[allow(dead_code)]
    allocation: BandwidthAllocation,

    /// Queue overflow policy
    overflow_policy: OverflowPolicy,

    /// Statistics
    stats: TransmitterStats,
}

/// Transmission statistics
#[derive(Debug, Default, Clone)]
pub struct TransmitterStats {
    /// Events transmitted by priority
    pub transmitted: [u64; 4],

    /// Events dropped by priority
    pub dropped: [u64; 4],

    /// Bytes transmitted by priority
    pub bytes_transmitted: [u64; 4],
}

impl EventTransmitter {
    /// Create a new event transmitter
    pub fn new(allocation: BandwidthAllocation) -> Self {
        // Token bucket capacity = 1 second worth of bandwidth
        let critical_bucket = TokenBucket::new(
            allocation.critical_reserved_bps as f64,
            allocation.critical_reserved_bps as f64,
        );
        let high_bucket = TokenBucket::new(
            allocation.high_min_bps as f64,
            allocation.high_min_bps as f64,
        );
        let normal_bucket = TokenBucket::new(
            allocation.normal_min_bps as f64,
            allocation.normal_min_bps as f64,
        );
        let low_bucket =
            TokenBucket::new(allocation.low_min_bps as f64, allocation.low_min_bps as f64);

        Self {
            queues: Default::default(),
            max_queue_sizes: [100, 1000, 1000, 1000], // Default limits
            buckets: [critical_bucket, high_bucket, normal_bucket, low_bucket],
            allocation,
            overflow_policy: OverflowPolicy::RemoveLowestPriority,
            stats: TransmitterStats::default(),
        }
    }

    /// Create with default allocation
    pub fn with_defaults() -> Self {
        Self::new(BandwidthAllocation::default())
    }

    /// Set maximum queue size for a priority
    pub fn set_max_queue_size(&mut self, priority: EventPriority, size: usize) {
        self.max_queue_sizes[priority_to_level(priority)] = size;
    }

    /// Set overflow policy
    pub fn set_overflow_policy(&mut self, policy: OverflowPolicy) {
        self.overflow_policy = policy;
    }

    /// Enqueue an event for transmission
    ///
    /// Returns true if event was accepted, false if dropped due to overflow.
    pub fn enqueue(&mut self, event: PeatEvent) -> bool {
        let level = self.get_level(&event);

        // Check for overflow
        if self.queues[level].len() >= self.max_queue_sizes[level] {
            match self.overflow_policy {
                OverflowPolicy::RejectNew => {
                    self.stats.dropped[level] += 1;
                    return false;
                }
                OverflowPolicy::RemoveOldest => {
                    self.queues[level].pop_front();
                    self.stats.dropped[level] += 1;
                }
                OverflowPolicy::RemoveLowestPriority => {
                    // Try to drop from LOW, then NORMAL, then HIGH
                    let dropped = self.drop_lowest_priority();
                    if !dropped {
                        // Can't drop anything, drop incoming
                        self.stats.dropped[level] += 1;
                        return false;
                    }
                }
            }
        }

        self.queues[level].push_back(event);
        true
    }

    /// Transmit events within bandwidth limits
    ///
    /// Returns events ready for transmission, respecting bandwidth allocation.
    pub fn transmit(&mut self, max_events: usize) -> Vec<PeatEvent> {
        let mut result = Vec::with_capacity(max_events);
        let mut remaining = max_events;

        // CRITICAL: Always transmit all pending (preempt)
        while remaining > 0 {
            if let Some(event) = self.queues[0].front() {
                let size = estimate_event_size(event);
                if self.buckets[0].try_consume(size as f64) {
                    let event = self.queues[0].pop_front().unwrap();
                    self.stats.transmitted[0] += 1;
                    self.stats.bytes_transmitted[0] += size as u64;
                    result.push(event);
                    remaining -= 1;
                } else {
                    break; // No more critical bandwidth
                }
            } else {
                break; // No more critical events
            }
        }

        if remaining == 0 {
            return result;
        }

        // Weighted fair queuing for non-critical
        // Calculate allocations based on remaining capacity
        let high_alloc = (remaining * 50) / 100;
        let normal_alloc = (remaining * 35) / 100;
        // LOW gets remainder

        // HIGH
        let mut high_remaining = high_alloc;
        while high_remaining > 0 {
            if let Some(event) = self.queues[1].front() {
                let size = estimate_event_size(event);
                if self.buckets[1].try_consume(size as f64) {
                    let event = self.queues[1].pop_front().unwrap();
                    self.stats.transmitted[1] += 1;
                    self.stats.bytes_transmitted[1] += size as u64;
                    result.push(event);
                    high_remaining -= 1;
                    remaining -= 1;
                } else {
                    break; // No more high bandwidth
                }
            } else {
                break;
            }
        }
        // Unused high allocation rolls over
        let high_unused = high_alloc - (high_alloc - high_remaining);

        // NORMAL
        let mut normal_remaining = normal_alloc + high_unused;
        while normal_remaining > 0 && remaining > 0 {
            if let Some(event) = self.queues[2].front() {
                let size = estimate_event_size(event);
                if self.buckets[2].try_consume(size as f64) {
                    let event = self.queues[2].pop_front().unwrap();
                    self.stats.transmitted[2] += 1;
                    self.stats.bytes_transmitted[2] += size as u64;
                    result.push(event);
                    normal_remaining -= 1;
                    remaining -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // LOW gets everything remaining
        while remaining > 0 {
            if let Some(event) = self.queues[3].front() {
                let size = estimate_event_size(event);
                if self.buckets[3].try_consume(size as f64) {
                    let event = self.queues[3].pop_front().unwrap();
                    self.stats.transmitted[3] += 1;
                    self.stats.bytes_transmitted[3] += size as u64;
                    result.push(event);
                    remaining -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        result
    }

    /// Transmit all critical events immediately (preempt)
    pub fn transmit_critical(&mut self) -> Vec<PeatEvent> {
        let mut result = Vec::new();

        while let Some(event) = self.queues[0].front() {
            let size = estimate_event_size(event);
            if self.buckets[0].try_consume(size as f64) {
                let event = self.queues[0].pop_front().unwrap();
                self.stats.transmitted[0] += 1;
                self.stats.bytes_transmitted[0] += size as u64;
                result.push(event);
            } else {
                break;
            }
        }

        result
    }

    /// Check if there are critical events pending
    pub fn has_critical(&self) -> bool {
        !self.queues[0].is_empty()
    }

    /// Get queue lengths
    pub fn queue_lengths(&self) -> [usize; 4] {
        [
            self.queues[0].len(),
            self.queues[1].len(),
            self.queues[2].len(),
            self.queues[3].len(),
        ]
    }

    /// Get total queued events
    pub fn total_queued(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Get transmission statistics
    pub fn stats(&self) -> &TransmitterStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = TransmitterStats::default();
    }

    /// Get available bandwidth tokens per priority
    pub fn available_bandwidth(&mut self) -> [f64; 4] {
        [
            self.buckets[0].available(),
            self.buckets[1].available(),
            self.buckets[2].available(),
            self.buckets[3].available(),
        ]
    }

    // Internal helpers

    fn get_level(&self, event: &PeatEvent) -> usize {
        let priority = event
            .routing
            .as_ref()
            .map(|r| EventPriority::try_from(r.priority).unwrap_or(EventPriority::PriorityNormal))
            .unwrap_or(EventPriority::PriorityNormal);
        priority_to_level(priority)
    }

    fn drop_lowest_priority(&mut self) -> bool {
        // Try LOW first
        if !self.queues[3].is_empty() {
            self.queues[3].pop_front();
            self.stats.dropped[3] += 1;
            return true;
        }
        // Then NORMAL
        if !self.queues[2].is_empty() {
            self.queues[2].pop_front();
            self.stats.dropped[2] += 1;
            return true;
        }
        // Then HIGH
        if !self.queues[1].is_empty() {
            self.queues[1].pop_front();
            self.stats.dropped[1] += 1;
            return true;
        }
        // Never drop CRITICAL
        false
    }
}

/// Convert EventPriority to queue level index
fn priority_to_level(priority: EventPriority) -> usize {
    match priority {
        EventPriority::PriorityCritical => 0,
        EventPriority::PriorityHigh => 1,
        EventPriority::PriorityNormal => 2,
        EventPriority::PriorityLow => 3,
    }
}

/// Estimate event size in bytes
fn estimate_event_size(event: &PeatEvent) -> usize {
    // Approximate size: base overhead + payload
    let base_overhead = 200; // Headers, routing, metadata
    base_overhead + event.payload_value.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::event::v1::AggregationPolicy;

    fn make_event(id: &str, priority: EventPriority, payload_size: usize) -> PeatEvent {
        PeatEvent {
            event_id: id.to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: peat_schema::event::v1::EventClass::Product as i32,
            event_type: "test".to_string(),
            routing: Some(AggregationPolicy {
                propagation: peat_schema::event::v1::PropagationMode::PropagationFull as i32,
                priority: priority as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: vec![0u8; payload_size],
        }
    }

    #[test]
    fn test_bandwidth_allocation_default() {
        let alloc = BandwidthAllocation::default();
        assert_eq!(alloc.total_available_bps, 1_000_000);
        assert!(alloc.critical_reserved_bps > 0);
        assert!(alloc.high_min_bps > 0);
        assert!(alloc.normal_min_bps > 0);
        assert!(alloc.low_min_bps > 0);
    }

    #[test]
    fn test_bandwidth_allocation_custom() {
        let alloc = BandwidthAllocation::with_percentages(1_000_000, 10, 45, 30, 15);
        assert_eq!(alloc.critical_reserved_bps, 100_000);
        assert_eq!(alloc.high_min_bps, 450_000);
        assert_eq!(alloc.normal_min_bps, 300_000);
        assert_eq!(alloc.low_min_bps, 150_000);
    }

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(1000.0, 100.0); // 1000 capacity, 100/sec refill

        // Initial consumption
        assert!(bucket.try_consume(500.0));
        // Tokens should be around 500 (with small variance due to time elapsed)
        assert!(bucket.tokens >= 499.0 && bucket.tokens <= 501.0);

        // Try to consume more than available (600 > ~500)
        assert!(!bucket.try_consume(600.0));
        // Tokens should still be around 500
        assert!(bucket.tokens >= 499.0 && bucket.tokens <= 501.0);

        // Consume most of remaining
        assert!(bucket.try_consume(400.0));
        // Should have around 100 tokens left
        assert!(bucket.tokens >= 99.0 && bucket.tokens <= 110.0);
    }

    #[test]
    fn test_transmitter_enqueue() {
        let mut tx = EventTransmitter::with_defaults();

        let event = make_event("e1", EventPriority::PriorityNormal, 100);
        assert!(tx.enqueue(event));

        assert_eq!(tx.queue_lengths()[2], 1); // NORMAL is level 2
    }

    #[test]
    fn test_transmitter_critical_preemption() {
        let mut tx = EventTransmitter::with_defaults();

        // Add events of different priorities
        tx.enqueue(make_event("low", EventPriority::PriorityLow, 100));
        tx.enqueue(make_event("normal", EventPriority::PriorityNormal, 100));
        tx.enqueue(make_event("high", EventPriority::PriorityHigh, 100));
        tx.enqueue(make_event("critical", EventPriority::PriorityCritical, 100));

        // Transmit should return critical first
        let events = tx.transmit(4);
        assert!(!events.is_empty());
        assert_eq!(events[0].event_id, "critical");
    }

    #[test]
    fn test_transmitter_has_critical() {
        let mut tx = EventTransmitter::with_defaults();

        assert!(!tx.has_critical());

        tx.enqueue(make_event("normal", EventPriority::PriorityNormal, 100));
        assert!(!tx.has_critical());

        tx.enqueue(make_event("critical", EventPriority::PriorityCritical, 100));
        assert!(tx.has_critical());

        tx.transmit_critical();
        assert!(!tx.has_critical());
    }

    #[test]
    fn test_transmitter_overflow_drop_incoming() {
        let mut tx = EventTransmitter::with_defaults();
        tx.set_max_queue_size(EventPriority::PriorityNormal, 2);
        tx.set_overflow_policy(OverflowPolicy::RejectNew);

        assert!(tx.enqueue(make_event("e1", EventPriority::PriorityNormal, 100)));
        assert!(tx.enqueue(make_event("e2", EventPriority::PriorityNormal, 100)));
        assert!(!tx.enqueue(make_event("e3", EventPriority::PriorityNormal, 100)));

        assert_eq!(tx.queue_lengths()[2], 2);
        assert_eq!(tx.stats.dropped[2], 1);
    }

    #[test]
    fn test_transmitter_overflow_drop_oldest() {
        let mut tx = EventTransmitter::with_defaults();
        tx.set_max_queue_size(EventPriority::PriorityNormal, 2);
        tx.set_overflow_policy(OverflowPolicy::RemoveOldest);

        tx.enqueue(make_event("e1", EventPriority::PriorityNormal, 100));
        tx.enqueue(make_event("e2", EventPriority::PriorityNormal, 100));
        tx.enqueue(make_event("e3", EventPriority::PriorityNormal, 100));

        assert_eq!(tx.queue_lengths()[2], 2);
        assert_eq!(tx.stats.dropped[2], 1);

        // e1 should be dropped, e2 and e3 remain
        let events = tx.transmit(10);
        assert!(events.iter().any(|e| e.event_id == "e2"));
        assert!(events.iter().any(|e| e.event_id == "e3"));
    }

    #[test]
    fn test_transmitter_overflow_drop_lowest() {
        let mut tx = EventTransmitter::with_defaults();
        tx.set_max_queue_size(EventPriority::PriorityHigh, 2);
        tx.set_overflow_policy(OverflowPolicy::RemoveLowestPriority);

        // Fill LOW queue first
        tx.enqueue(make_event("low1", EventPriority::PriorityLow, 100));
        tx.enqueue(make_event("low2", EventPriority::PriorityLow, 100));

        // Fill HIGH queue
        tx.enqueue(make_event("high1", EventPriority::PriorityHigh, 100));
        tx.enqueue(make_event("high2", EventPriority::PriorityHigh, 100));

        // Overflow HIGH - should drop LOW
        tx.enqueue(make_event("high3", EventPriority::PriorityHigh, 100));

        assert_eq!(tx.queue_lengths()[1], 3); // HIGH queue has 3
        assert_eq!(tx.queue_lengths()[3], 1); // LOW queue lost one
        assert_eq!(tx.stats.dropped[3], 1);
    }

    #[test]
    fn test_transmitter_stats() {
        let mut tx = EventTransmitter::with_defaults();

        tx.enqueue(make_event("c1", EventPriority::PriorityCritical, 100));
        tx.enqueue(make_event("h1", EventPriority::PriorityHigh, 200));

        tx.transmit(10);

        let stats = tx.stats();
        assert_eq!(stats.transmitted[0], 1); // CRITICAL
        assert_eq!(stats.transmitted[1], 1); // HIGH
        assert!(stats.bytes_transmitted[0] > 0);
        assert!(stats.bytes_transmitted[1] > 0);
    }

    #[test]
    fn test_transmitter_weighted_distribution() {
        let mut tx = EventTransmitter::with_defaults();

        // Add many events at each priority
        for i in 0..20 {
            tx.enqueue(make_event(
                &format!("h{}", i),
                EventPriority::PriorityHigh,
                50,
            ));
            tx.enqueue(make_event(
                &format!("n{}", i),
                EventPriority::PriorityNormal,
                50,
            ));
            tx.enqueue(make_event(
                &format!("l{}", i),
                EventPriority::PriorityLow,
                50,
            ));
        }

        // Transmit some events
        let events = tx.transmit(10);

        // Count by priority
        let high_count = events
            .iter()
            .filter(|e| e.event_id.starts_with('h'))
            .count();
        let normal_count = events
            .iter()
            .filter(|e| e.event_id.starts_with('n'))
            .count();
        let low_count = events
            .iter()
            .filter(|e| e.event_id.starts_with('l'))
            .count();

        // Should roughly follow 50/35/15 distribution (with some variance)
        assert!(high_count >= 3, "high_count={}", high_count);
        assert!(normal_count >= 2, "normal_count={}", normal_count);
        assert!(high_count >= low_count, "high >= low");
    }
}
