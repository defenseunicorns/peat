//! Priority-based event queue for transmission scheduling (ADR-027)
//!
//! Implements a 4-level priority queue where:
//! - CRITICAL events always transmit immediately, preempting others
//! - HIGH/NORMAL/LOW share remaining bandwidth with weighted fair queuing

use peat_schema::event::v1::{EventPriority, PeatEvent};
use std::collections::VecDeque;

/// Number of priority levels
pub const PRIORITY_LEVELS: usize = 4;

/// Priority-based event queue for transmission scheduling
///
/// Events are organized into 4 priority levels:
/// - Level 0: CRITICAL - immediate, preempts all other traffic
/// - Level 1: HIGH - after CRITICAL
/// - Level 2: NORMAL - default priority
/// - Level 3: LOW - transmitted when bandwidth available
#[derive(Debug, Default)]
pub struct PriorityEventQueue {
    /// Queues for each priority level (CRITICAL=0, HIGH=1, NORMAL=2, LOW=3)
    queues: [VecDeque<PeatEvent>; PRIORITY_LEVELS],
}

impl PriorityEventQueue {
    /// Create a new empty priority queue
    pub fn new() -> Self {
        Self {
            queues: Default::default(),
        }
    }

    /// Push an event onto the appropriate priority queue
    pub fn push(&mut self, event: PeatEvent) {
        let priority = self.get_priority(&event);
        let level = priority_to_level(priority);
        self.queues[level].push_back(event);
    }

    /// Pop the highest-priority event
    ///
    /// CRITICAL events are always returned first. If no CRITICAL events,
    /// returns from HIGH, then NORMAL, then LOW.
    pub fn pop(&mut self) -> Option<PeatEvent> {
        for queue in &mut self.queues {
            if let Some(event) = queue.pop_front() {
                return Some(event);
            }
        }
        None
    }

    /// Pop all CRITICAL events (for immediate transmission)
    ///
    /// Returns events in FIFO order within CRITICAL priority.
    pub fn pop_critical(&mut self) -> Vec<PeatEvent> {
        self.queues[0].drain(..).collect()
    }

    /// Pop events from non-critical queues for weighted fair transmission
    ///
    /// Returns up to `max_events` events, weighted by priority:
    /// - HIGH: 50% of allocation
    /// - NORMAL: 35% of allocation
    /// - LOW: 15% of allocation
    ///
    /// Unused allocation rolls to lower priorities.
    pub fn pop_weighted(&mut self, max_events: usize) -> Vec<PeatEvent> {
        if max_events == 0 {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(max_events);

        // Calculate allocations (weighted fair queuing)
        let high_alloc = (max_events * 50) / 100;
        let normal_alloc = (max_events * 35) / 100;
        // LOW gets remainder

        // Pop from HIGH (level 1)
        let mut remaining = max_events;
        let high_count = self.pop_from_level(1, high_alloc.min(remaining), &mut result);
        remaining -= high_count;

        // Unused HIGH allocation rolls to NORMAL
        let normal_target = normal_alloc + (high_alloc - high_count);
        let normal_count = self.pop_from_level(2, normal_target.min(remaining), &mut result);
        remaining -= normal_count;

        // LOW gets everything remaining
        self.pop_from_level(3, remaining, &mut result);

        result
    }

    /// Check if there are any CRITICAL events pending
    pub fn has_critical(&self) -> bool {
        !self.queues[0].is_empty()
    }

    /// Get total count of events across all priorities
    pub fn len(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(|q| q.is_empty())
    }

    /// Get count of events at a specific priority level
    pub fn len_at_priority(&self, priority: EventPriority) -> usize {
        let level = priority_to_level(priority);
        self.queues[level].len()
    }

    /// Get total count by priority level (for metrics)
    pub fn counts(&self) -> [usize; PRIORITY_LEVELS] {
        [
            self.queues[0].len(),
            self.queues[1].len(),
            self.queues[2].len(),
            self.queues[3].len(),
        ]
    }

    /// Clear all events from the queue
    pub fn clear(&mut self) {
        for queue in &mut self.queues {
            queue.clear();
        }
    }

    // Internal helpers

    fn get_priority(&self, event: &PeatEvent) -> EventPriority {
        event
            .routing
            .as_ref()
            .map(|r| EventPriority::try_from(r.priority).unwrap_or(EventPriority::PriorityNormal))
            .unwrap_or(EventPriority::PriorityNormal)
    }

    fn pop_from_level(&mut self, level: usize, count: usize, result: &mut Vec<PeatEvent>) -> usize {
        let mut popped = 0;
        while popped < count {
            if let Some(event) = self.queues[level].pop_front() {
                result.push(event);
                popped += 1;
            } else {
                break;
            }
        }
        popped
    }
}

/// Convert EventPriority enum to queue level index
fn priority_to_level(priority: EventPriority) -> usize {
    match priority {
        EventPriority::PriorityCritical => 0,
        EventPriority::PriorityHigh => 1,
        EventPriority::PriorityNormal => 2,
        EventPriority::PriorityLow => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::event::v1::AggregationPolicy;

    fn make_event(id: &str, priority: EventPriority) -> PeatEvent {
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
            payload_value: vec![],
        }
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = PriorityEventQueue::new();

        // Add events in reverse priority order
        queue.push(make_event("low", EventPriority::PriorityLow));
        queue.push(make_event("normal", EventPriority::PriorityNormal));
        queue.push(make_event("high", EventPriority::PriorityHigh));
        queue.push(make_event("critical", EventPriority::PriorityCritical));

        // Should pop in priority order (CRITICAL first)
        assert_eq!(queue.pop().unwrap().event_id, "critical");
        assert_eq!(queue.pop().unwrap().event_id, "high");
        assert_eq!(queue.pop().unwrap().event_id, "normal");
        assert_eq!(queue.pop().unwrap().event_id, "low");
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_fifo_within_priority() {
        let mut queue = PriorityEventQueue::new();

        // Add multiple HIGH priority events
        queue.push(make_event("h1", EventPriority::PriorityHigh));
        queue.push(make_event("h2", EventPriority::PriorityHigh));
        queue.push(make_event("h3", EventPriority::PriorityHigh));

        // Should maintain FIFO order within priority
        assert_eq!(queue.pop().unwrap().event_id, "h1");
        assert_eq!(queue.pop().unwrap().event_id, "h2");
        assert_eq!(queue.pop().unwrap().event_id, "h3");
    }

    #[test]
    fn test_pop_critical() {
        let mut queue = PriorityEventQueue::new();

        queue.push(make_event("c1", EventPriority::PriorityCritical));
        queue.push(make_event("h1", EventPriority::PriorityHigh));
        queue.push(make_event("c2", EventPriority::PriorityCritical));

        let critical = queue.pop_critical();
        assert_eq!(critical.len(), 2);
        assert_eq!(critical[0].event_id, "c1");
        assert_eq!(critical[1].event_id, "c2");

        // HIGH should still be there
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop().unwrap().event_id, "h1");
    }

    #[test]
    fn test_pop_weighted() {
        let mut queue = PriorityEventQueue::new();

        // Add 10 events at each non-critical priority
        for i in 0..10 {
            queue.push(make_event(&format!("h{}", i), EventPriority::PriorityHigh));
            queue.push(make_event(
                &format!("n{}", i),
                EventPriority::PriorityNormal,
            ));
            queue.push(make_event(&format!("l{}", i), EventPriority::PriorityLow));
        }

        // Pop 10 events with weighted allocation
        // Expected: ~5 HIGH, ~3.5 NORMAL, ~1.5 LOW
        let events = queue.pop_weighted(10);
        assert_eq!(events.len(), 10);

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

        // Verify weighted distribution (approximate)
        assert!((4..=6).contains(&high_count), "high={}", high_count);
        assert!((2..=5).contains(&normal_count), "normal={}", normal_count);
        assert!(low_count <= 3, "low={}", low_count);
    }

    #[test]
    fn test_weighted_rollover() {
        let mut queue = PriorityEventQueue::new();

        // Only LOW priority events
        for i in 0..10 {
            queue.push(make_event(&format!("l{}", i), EventPriority::PriorityLow));
        }

        // Pop 5 events - should all come from LOW since HIGH/NORMAL are empty
        let events = queue.pop_weighted(5);
        assert_eq!(events.len(), 5);
        assert!(events.iter().all(|e| e.event_id.starts_with('l')));
    }

    #[test]
    fn test_has_critical() {
        let mut queue = PriorityEventQueue::new();
        assert!(!queue.has_critical());

        queue.push(make_event("h1", EventPriority::PriorityHigh));
        assert!(!queue.has_critical());

        queue.push(make_event("c1", EventPriority::PriorityCritical));
        assert!(queue.has_critical());

        queue.pop_critical();
        assert!(!queue.has_critical());
    }

    #[test]
    fn test_counts() {
        let mut queue = PriorityEventQueue::new();

        queue.push(make_event("c1", EventPriority::PriorityCritical));
        queue.push(make_event("h1", EventPriority::PriorityHigh));
        queue.push(make_event("h2", EventPriority::PriorityHigh));
        queue.push(make_event("n1", EventPriority::PriorityNormal));

        let counts = queue.counts();
        assert_eq!(counts, [1, 2, 1, 0]);
        assert_eq!(queue.len(), 4);

        assert_eq!(queue.len_at_priority(EventPriority::PriorityCritical), 1);
        assert_eq!(queue.len_at_priority(EventPriority::PriorityHigh), 2);
        assert_eq!(queue.len_at_priority(EventPriority::PriorityNormal), 1);
        assert_eq!(queue.len_at_priority(EventPriority::PriorityLow), 0);
    }

    #[test]
    fn test_default_priority_for_missing_routing() {
        let mut queue = PriorityEventQueue::new();

        // Event without routing should default to NORMAL
        let event = PeatEvent {
            event_id: "no-routing".to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: peat_schema::event::v1::EventClass::Product as i32,
            event_type: "test".to_string(),
            routing: None,
            payload_type_url: String::new(),
            payload_value: vec![],
        };

        queue.push(event);
        assert_eq!(queue.len_at_priority(EventPriority::PriorityNormal), 1);
    }
}
