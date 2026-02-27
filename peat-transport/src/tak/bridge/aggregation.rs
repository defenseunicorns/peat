//! Message aggregation for PEAT-TAK bridge

use std::collections::HashMap;
use std::sync::RwLock;

use super::config::AggregationPolicy;
use super::PeatMessage;

/// Aggregator for batching messages based on policy
#[derive(Debug)]
pub struct Aggregator {
    /// Aggregation policy
    policy: AggregationPolicy,
    /// Pending messages by source
    pending: RwLock<HashMap<String, PeatMessage>>,
}

impl Aggregator {
    /// Create a new aggregator with the given policy
    pub fn new(policy: AggregationPolicy) -> Self {
        Self {
            policy,
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Add a message to the aggregator
    ///
    /// For time-windowed policies, this stores the latest message per source.
    /// Older messages for the same source are replaced.
    pub fn add(&self, message: PeatMessage) {
        let key = message.source_node().to_string();
        let mut pending = self.pending.write().unwrap();
        pending.insert(key, message);
    }

    /// Flush all pending messages
    ///
    /// Returns messages that should be published.
    pub fn flush(&self) -> Vec<PeatMessage> {
        let mut pending = self.pending.write().unwrap();
        pending.drain().map(|(_, v)| v).collect()
    }

    /// Get the number of pending messages
    pub fn pending_count(&self) -> usize {
        self.pending.read().unwrap().len()
    }

    /// Get the window duration for time-windowed policies
    pub fn window_duration_secs(&self) -> Option<u64> {
        match &self.policy {
            AggregationPolicy::TimeWindowed { window_secs } => Some(*window_secs),
            _ => None,
        }
    }

    /// Check if this aggregator uses time-based batching
    pub fn is_time_windowed(&self) -> bool {
        matches!(self.policy, AggregationPolicy::TimeWindowed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_protocol::cot::{Position, TrackUpdate};

    fn make_track(id: &str, platform: &str) -> PeatMessage {
        PeatMessage::Track(TrackUpdate {
            track_id: id.to_string(),
            source_platform: platform.to_string(),
            source_model: "test-model".to_string(),
            model_version: "1.0".to_string(),
            cell_id: Some("cell-1".to_string()),
            formation_id: None,
            timestamp: chrono::Utc::now(),
            position: Position::new(34.0, -118.0),
            velocity: None,
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.9,
            attributes: Default::default(),
        })
    }

    #[test]
    fn test_aggregator_stores_by_source() {
        let agg = Aggregator::new(AggregationPolicy::time_windowed(5));

        agg.add(make_track("t1", "node-1"));
        agg.add(make_track("t2", "node-2"));
        agg.add(make_track("t3", "node-1")); // Replaces t1

        assert_eq!(agg.pending_count(), 2);

        let flushed = agg.flush();
        assert_eq!(flushed.len(), 2);
        assert_eq!(agg.pending_count(), 0);
    }

    #[test]
    fn test_aggregator_replaces_same_source() {
        let agg = Aggregator::new(AggregationPolicy::time_windowed(5));

        agg.add(make_track("t1", "node-1"));
        agg.add(make_track("t2", "node-1")); // Replaces t1

        let flushed = agg.flush();
        assert_eq!(flushed.len(), 1);

        // Should be t2 (the later one)
        if let PeatMessage::Track(track) = &flushed[0] {
            assert_eq!(track.track_id, "t2");
        } else {
            panic!("Expected track message");
        }
    }

    #[test]
    fn test_window_duration() {
        let agg = Aggregator::new(AggregationPolicy::time_windowed(10));
        assert_eq!(agg.window_duration_secs(), Some(10));

        let agg2 = Aggregator::new(AggregationPolicy::FullFidelity);
        assert_eq!(agg2.window_duration_secs(), None);
    }
}
