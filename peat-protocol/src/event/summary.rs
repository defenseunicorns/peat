//! Summary Strategies for Event Aggregation (ADR-027 Phase 2)
//!
//! Different event types require different summarization strategies.
//! This module provides a trait and implementations for generating
//! summaries from aggregated events.
//!
//! ## Strategy Pattern
//!
//! ```text
//! Events → SummaryStrategy → Summary Payload (bytes)
//!              ↓
//!    ┌─────────┴─────────┐
//!    │  Detection: counts, histogram │
//!    │  Telemetry: min/max/avg      │
//!    │  Custom: user-defined        │
//!    └───────────────────────────────┘
//! ```

use peat_schema::event::v1::PeatEvent;
use std::collections::HashMap;
use std::fmt::Debug;

/// Strategy for summarizing events of a given type
///
/// Implementations should be stateless and thread-safe.
pub trait SummaryStrategy: Send + Sync + Debug {
    /// Event type this strategy handles (e.g., "detection", "telemetry")
    fn event_type(&self) -> &str;

    /// Generate summary payload from collected events
    ///
    /// Returns a byte vector containing the summarized data.
    /// The format is application-specific but should be consistent.
    fn summarize(&self, events: &[PeatEvent]) -> Vec<u8>;
}

/// Default summary strategy for events without a specific strategy
///
/// Generates a simple count-based summary.
#[derive(Debug)]
pub struct DefaultSummaryStrategy {
    event_type: String,
}

impl DefaultSummaryStrategy {
    /// Create a new default strategy for an event type
    pub fn new(event_type: &str) -> Self {
        Self {
            event_type: event_type.to_string(),
        }
    }
}

impl SummaryStrategy for DefaultSummaryStrategy {
    fn event_type(&self) -> &str {
        &self.event_type
    }

    fn summarize(&self, events: &[PeatEvent]) -> Vec<u8> {
        // Simple JSON summary with counts
        let summary = serde_json::json!({
            "event_type": self.event_type,
            "event_count": events.len(),
            "source_nodes": events.iter()
                .map(|e| e.source_node_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
        });

        serde_json::to_vec(&summary).unwrap_or_default()
    }
}

/// Detection event summary strategy
///
/// Generates summaries with:
/// - Counts by detection type
/// - Confidence histogram (10 buckets)
/// - Total detection count
#[derive(Debug, Default)]
pub struct DetectionSummaryStrategy;

impl DetectionSummaryStrategy {
    /// Create a new detection summary strategy
    pub fn new() -> Self {
        Self
    }
}

impl SummaryStrategy for DetectionSummaryStrategy {
    fn event_type(&self) -> &str {
        "detection"
    }

    fn summarize(&self, events: &[PeatEvent]) -> Vec<u8> {
        let mut counts_by_type: HashMap<String, u32> = HashMap::new();
        let mut confidence_histogram = [0u32; 10];
        let mut total_detections = 0u32;

        for event in events {
            total_detections += 1;

            // Parse event type for detection subtype
            let subtype = event
                .event_type
                .strip_prefix("detection.")
                .or_else(|| event.event_type.strip_prefix("product.detection."))
                .unwrap_or("unknown");

            *counts_by_type.entry(subtype.to_string()).or_default() += 1;

            // Try to extract confidence from payload if present
            if !event.payload_value.is_empty() {
                // Attempt to parse confidence from JSON payload
                if let Ok(payload) =
                    serde_json::from_slice::<serde_json::Value>(&event.payload_value)
                {
                    if let Some(conf) = payload.get("confidence").and_then(|v| v.as_f64()) {
                        let bucket = ((conf * 10.0).clamp(0.0, 9.0)) as usize;
                        confidence_histogram[bucket] += 1;
                    }
                }
            }
        }

        let summary = DetectionSummary {
            counts_by_type,
            confidence_histogram: confidence_histogram.to_vec(),
            total_detections,
        };

        serde_json::to_vec(&summary).unwrap_or_default()
    }
}

/// Summary data for detection events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectionSummary {
    /// Counts of detections by type
    pub counts_by_type: HashMap<String, u32>,

    /// Confidence histogram (10 buckets: 0.0-0.1, 0.1-0.2, ..., 0.9-1.0)
    pub confidence_histogram: Vec<u32>,

    /// Total number of detections
    pub total_detections: u32,
}

/// Telemetry event summary strategy
///
/// Generates summaries with:
/// - Min/max/avg for each metric
/// - Sample count
#[derive(Debug, Default)]
pub struct TelemetrySummaryStrategy;

impl TelemetrySummaryStrategy {
    /// Create a new telemetry summary strategy
    pub fn new() -> Self {
        Self
    }
}

impl SummaryStrategy for TelemetrySummaryStrategy {
    fn event_type(&self) -> &str {
        "telemetry"
    }

    fn summarize(&self, events: &[PeatEvent]) -> Vec<u8> {
        let mut metrics: HashMap<String, MetricStats> = HashMap::new();

        for event in events {
            // Try to parse metrics from payload
            if !event.payload_value.is_empty() {
                if let Ok(payload) =
                    serde_json::from_slice::<serde_json::Value>(&event.payload_value)
                {
                    // Look for metrics in the payload
                    if let Some(obj) = payload.as_object() {
                        for (key, value) in obj {
                            if let Some(v) = value.as_f64() {
                                let stats = metrics.entry(key.clone()).or_default();
                                stats.update(v);
                            }
                        }
                    }
                }
            }

            // Also track by event type (e.g., "telemetry.cpu" -> "cpu")
            let metric_name = event
                .event_type
                .strip_prefix("telemetry.")
                .unwrap_or(&event.event_type);

            // Track at least the count for this metric type
            let stats = metrics.entry(metric_name.to_string()).or_default();
            if stats.count == 0 {
                stats.count = 1;
            } else {
                stats.count += 1;
            }
        }

        let summary = TelemetrySummary {
            metrics: metrics
                .into_iter()
                .map(|(k, v)| (k, v.finalize()))
                .collect(),
            sample_count: events.len() as u32,
        };

        serde_json::to_vec(&summary).unwrap_or_default()
    }
}

/// Summary data for telemetry events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelemetrySummary {
    /// Statistics for each metric
    pub metrics: HashMap<String, MetricSummaryStats>,

    /// Total number of samples
    pub sample_count: u32,
}

/// Statistics for a single metric
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MetricStats {
    min: f64,
    max: f64,
    sum: f64,
    count: u32,
}

impl MetricStats {
    /// Update stats with a new value
    pub fn update(&mut self, value: f64) {
        if self.count == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.sum += value;
        self.count += 1;
    }

    /// Finalize into a summary stats structure
    pub fn finalize(&self) -> MetricSummaryStats {
        MetricSummaryStats {
            min: self.min,
            max: self.max,
            avg: if self.count > 0 {
                self.sum / self.count as f64
            } else {
                0.0
            },
            count: self.count,
        }
    }
}

/// Final statistics for a metric
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricSummaryStats {
    /// Minimum value
    pub min: f64,

    /// Maximum value
    pub max: f64,

    /// Average value
    pub avg: f64,

    /// Sample count
    pub count: u32,
}

/// Anomaly event summary strategy
///
/// Generates summaries with:
/// - Counts by severity
/// - List of unique anomaly types
/// - Total anomaly count
#[derive(Debug, Default)]
pub struct AnomalySummaryStrategy;

impl AnomalySummaryStrategy {
    /// Create a new anomaly summary strategy
    pub fn new() -> Self {
        Self
    }
}

impl SummaryStrategy for AnomalySummaryStrategy {
    fn event_type(&self) -> &str {
        "anomaly"
    }

    fn summarize(&self, events: &[PeatEvent]) -> Vec<u8> {
        let mut counts_by_severity: HashMap<String, u32> = HashMap::new();
        let mut anomaly_types: std::collections::HashSet<String> = std::collections::HashSet::new();

        for event in events {
            // Extract severity from priority
            let severity = if let Some(routing) = &event.routing {
                match routing.priority {
                    0 => "critical",
                    1 => "high",
                    2 => "normal",
                    3 => "low",
                    _ => "unknown",
                }
            } else {
                "unknown"
            };

            *counts_by_severity.entry(severity.to_string()).or_default() += 1;

            // Extract anomaly type
            let anomaly_type = event
                .event_type
                .strip_prefix("anomaly.")
                .unwrap_or(&event.event_type);
            anomaly_types.insert(anomaly_type.to_string());
        }

        let summary = AnomalySummary {
            counts_by_severity,
            anomaly_types: anomaly_types.into_iter().collect(),
            total_anomalies: events.len() as u32,
        };

        serde_json::to_vec(&summary).unwrap_or_default()
    }
}

/// Summary data for anomaly events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnomalySummary {
    /// Counts by severity level
    pub counts_by_severity: HashMap<String, u32>,

    /// Unique anomaly types observed
    pub anomaly_types: Vec<String>,

    /// Total anomaly count
    pub total_anomalies: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::common::v1::Timestamp;
    use peat_schema::event::v1::{AggregationPolicy, EventClass, EventPriority, PropagationMode};

    fn make_event(event_type: &str, payload: Option<serde_json::Value>) -> PeatEvent {
        PeatEvent {
            event_id: "test-1".to_string(),
            timestamp: Some(Timestamp {
                seconds: 0,
                nanos: 0,
            }),
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Product as i32,
            event_type: event_type.to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationSummary as i32,
                priority: EventPriority::PriorityNormal as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 1000,
            }),
            payload_type_url: String::new(),
            payload_value: payload
                .map(|p| serde_json::to_vec(&p).unwrap())
                .unwrap_or_default(),
        }
    }

    #[test]
    fn test_default_strategy() {
        let strategy = DefaultSummaryStrategy::new("test");
        assert_eq!(strategy.event_type(), "test");

        let events = vec![
            make_event("test.a", None),
            make_event("test.b", None),
            make_event("test.a", None),
        ];

        let summary_bytes = strategy.summarize(&events);
        let summary: serde_json::Value = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary["event_count"], 3);
        assert_eq!(summary["event_type"], "test");
    }

    #[test]
    fn test_detection_strategy_counts() {
        let strategy = DetectionSummaryStrategy::new();
        assert_eq!(strategy.event_type(), "detection");

        let events = vec![
            make_event("detection.vehicle", None),
            make_event("detection.person", None),
            make_event("detection.vehicle", None),
        ];

        let summary_bytes = strategy.summarize(&events);
        let summary: DetectionSummary = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary.total_detections, 3);
        assert_eq!(*summary.counts_by_type.get("vehicle").unwrap(), 2);
        assert_eq!(*summary.counts_by_type.get("person").unwrap(), 1);
    }

    #[test]
    fn test_detection_strategy_confidence() {
        let strategy = DetectionSummaryStrategy::new();

        let events = vec![
            make_event(
                "detection.vehicle",
                Some(serde_json::json!({"confidence": 0.95})),
            ),
            make_event(
                "detection.vehicle",
                Some(serde_json::json!({"confidence": 0.85})),
            ),
            make_event(
                "detection.vehicle",
                Some(serde_json::json!({"confidence": 0.35})),
            ),
        ];

        let summary_bytes = strategy.summarize(&events);
        let summary: DetectionSummary = serde_json::from_slice(&summary_bytes).unwrap();

        // Bucket 9 (0.9-1.0): 1 event with 0.95
        // Bucket 8 (0.8-0.9): 1 event with 0.85
        // Bucket 3 (0.3-0.4): 1 event with 0.35
        assert_eq!(summary.confidence_histogram[9], 1);
        assert_eq!(summary.confidence_histogram[8], 1);
        assert_eq!(summary.confidence_histogram[3], 1);
    }

    #[test]
    fn test_telemetry_strategy() {
        let strategy = TelemetrySummaryStrategy::new();
        assert_eq!(strategy.event_type(), "telemetry");

        let events = vec![
            make_event(
                "telemetry.cpu",
                Some(serde_json::json!({"cpu_percent": 50.0, "memory_mb": 1024.0})),
            ),
            make_event(
                "telemetry.cpu",
                Some(serde_json::json!({"cpu_percent": 75.0, "memory_mb": 2048.0})),
            ),
        ];

        let summary_bytes = strategy.summarize(&events);
        let summary: TelemetrySummary = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary.sample_count, 2);

        let cpu = summary.metrics.get("cpu_percent").unwrap();
        assert_eq!(cpu.min, 50.0);
        assert_eq!(cpu.max, 75.0);
        assert!((cpu.avg - 62.5).abs() < 0.01);
        assert_eq!(cpu.count, 2);

        let mem = summary.metrics.get("memory_mb").unwrap();
        assert_eq!(mem.min, 1024.0);
        assert_eq!(mem.max, 2048.0);
    }

    #[test]
    fn test_anomaly_strategy() {
        let strategy = AnomalySummaryStrategy::new();
        assert_eq!(strategy.event_type(), "anomaly");

        let events = vec![
            {
                let mut e = make_event("anomaly.intrusion", None);
                e.routing.as_mut().unwrap().priority = EventPriority::PriorityCritical as i32;
                e
            },
            {
                let mut e = make_event("anomaly.network_spike", None);
                e.routing.as_mut().unwrap().priority = EventPriority::PriorityHigh as i32;
                e
            },
            {
                let mut e = make_event("anomaly.intrusion", None);
                e.routing.as_mut().unwrap().priority = EventPriority::PriorityCritical as i32;
                e
            },
        ];

        let summary_bytes = strategy.summarize(&events);
        let summary: AnomalySummary = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary.total_anomalies, 3);
        assert_eq!(*summary.counts_by_severity.get("critical").unwrap(), 2);
        assert_eq!(*summary.counts_by_severity.get("high").unwrap(), 1);
        assert!(summary.anomaly_types.contains(&"intrusion".to_string()));
        assert!(summary.anomaly_types.contains(&"network_spike".to_string()));
    }

    #[test]
    fn test_empty_events() {
        let strategy = DetectionSummaryStrategy::new();
        let summary_bytes = strategy.summarize(&[]);
        let summary: DetectionSummary = serde_json::from_slice(&summary_bytes).unwrap();

        assert_eq!(summary.total_detections, 0);
        assert!(summary.counts_by_type.is_empty());
    }
}
