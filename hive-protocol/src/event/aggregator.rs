//! Echelon Aggregation for Event Routing (ADR-027 Phase 2)
//!
//! The EchelonAggregator processes incoming events from subordinates and applies
//! aggregation policies at echelon boundaries (squad → platoon → company).
//!
//! ## Aggregation Flow
//!
//! ```text
//! Subordinate Events → EchelonAggregator → Parent Echelon
//!                           ↓
//!                    ┌──────┴──────┐
//!                    │ PropagationMode │
//!                    └──────┬──────┘
//!        ┌─────────────┬────┴───────┬─────────────┐
//!        ↓             ↓            ↓             ↓
//!      Full         Summary       Query         Local
//!   (passthrough)  (aggregate)  (store only)  (ignored)
//! ```

use super::priority_queue::PriorityEventQueue;
use super::summary::{DefaultSummaryStrategy, SummaryStrategy};
use crate::Result;
use hive_schema::common::v1::Timestamp;
use hive_schema::event::v1::{
    AggregationPolicy, EventClass, EventPriority, EventSummary, HiveEvent, PropagationMode,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

/// Echelon type for aggregation context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchelonType {
    /// Squad level (8-12 platforms)
    Squad,
    /// Platoon level (3-4 squads)
    Platoon,
    /// Company level (3-4 platoons)
    Company,
}

impl std::fmt::Display for EchelonType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EchelonType::Squad => write!(f, "squad"),
            EchelonType::Platoon => write!(f, "platoon"),
            EchelonType::Company => write!(f, "company"),
        }
    }
}

/// Aggregation window for collecting events before summarization
#[derive(Debug)]
pub struct AggregationWindow {
    /// Event class being aggregated
    event_class: EventClass,

    /// Event type identifier
    event_type: String,

    /// Window duration for aggregation
    window_duration: Duration,

    /// When this window started
    window_start: Instant,

    /// Events collected in this window
    events: Vec<HiveEvent>,

    /// Source nodes that contributed events
    source_nodes: HashSet<String>,
}

impl AggregationWindow {
    /// Create a new aggregation window
    pub fn new(event_class: EventClass, event_type: &str, window_duration: Duration) -> Self {
        Self {
            event_class,
            event_type: event_type.to_string(),
            window_duration,
            window_start: Instant::now(),
            events: Vec::new(),
            source_nodes: HashSet::new(),
        }
    }

    /// Add an event to this window
    pub fn add(&mut self, event: HiveEvent) {
        self.source_nodes.insert(event.source_node_id.clone());
        self.events.push(event);
    }

    /// Check if the window should be flushed (time expired)
    pub fn should_flush(&self) -> bool {
        self.window_start.elapsed() >= self.window_duration
    }

    /// Get the number of events in this window
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get events in this window
    pub fn events(&self) -> &[HiveEvent] {
        &self.events
    }

    /// Get source node IDs
    pub fn source_nodes(&self) -> &HashSet<String> {
        &self.source_nodes
    }

    /// Get the event class
    pub fn event_class(&self) -> EventClass {
        self.event_class
    }

    /// Get the event type
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Get window start time
    pub fn window_start(&self) -> Instant {
        self.window_start
    }

    /// Reset the window for a new aggregation period
    pub fn reset(&mut self) {
        self.window_start = Instant::now();
        self.events.clear();
        self.source_nodes.clear();
    }
}

/// Key for identifying aggregation windows
type WindowKey = (i32, String); // (event_class as i32, event_type)

/// Event aggregator at echelon boundary
///
/// Processes incoming events from subordinates and applies aggregation policies.
/// Events are routed based on their `PropagationMode`:
/// - `Full`: Passed through immediately to the parent
/// - `Summary`: Collected in aggregation windows and summarized
/// - `Query`: Stored locally for query access
/// - `Local`: Ignored (should not reach aggregator)
#[derive(Debug)]
pub struct EchelonAggregator {
    /// Unique identifier for this echelon
    echelon_id: String,

    /// Type of echelon (Squad, Platoon, Company)
    echelon_type: EchelonType,

    /// Aggregation windows by (event_class, event_type)
    windows: Arc<RwLock<HashMap<WindowKey, AggregationWindow>>>,

    /// Events to forward without aggregation (passthrough)
    passthrough_queue: Arc<RwLock<PriorityEventQueue>>,

    /// Local event storage for Query mode events
    queryable_store: Arc<RwLock<HashMap<String, HiveEvent>>>,

    /// Summary strategies by event type
    summary_strategies: Arc<RwLock<HashMap<String, Box<dyn SummaryStrategy>>>>,

    /// Generated summaries ready for transmission
    summary_queue: Arc<RwLock<Vec<HiveEvent>>>,

    /// Counter for generating unique summary IDs
    summary_counter: Arc<RwLock<u64>>,

    /// Default aggregation window duration (used when not specified in policy)
    default_window_duration: Duration,
}

impl EchelonAggregator {
    /// Create a new echelon aggregator
    pub fn new(echelon_id: String, echelon_type: EchelonType) -> Self {
        let mut strategies: HashMap<String, Box<dyn SummaryStrategy>> = HashMap::new();

        // Register default strategies
        strategies.insert(
            "detection".to_string(),
            Box::new(DefaultSummaryStrategy::new("detection")),
        );
        strategies.insert(
            "telemetry".to_string(),
            Box::new(DefaultSummaryStrategy::new("telemetry")),
        );

        Self {
            echelon_id,
            echelon_type,
            windows: Arc::new(RwLock::new(HashMap::new())),
            passthrough_queue: Arc::new(RwLock::new(PriorityEventQueue::new())),
            queryable_store: Arc::new(RwLock::new(HashMap::new())),
            summary_strategies: Arc::new(RwLock::new(strategies)),
            summary_queue: Arc::new(RwLock::new(Vec::new())),
            summary_counter: Arc::new(RwLock::new(0)),
            default_window_duration: Duration::from_secs(1),
        }
    }

    /// Set the default aggregation window duration
    pub fn with_default_window_duration(mut self, duration: Duration) -> Self {
        self.default_window_duration = duration;
        self
    }

    /// Register a custom summary strategy for an event type
    pub fn register_strategy(&self, strategy: Box<dyn SummaryStrategy>) {
        let mut strategies = self.summary_strategies.write().unwrap();
        strategies.insert(strategy.event_type().to_string(), strategy);
    }

    /// Process an incoming event from a subordinate
    ///
    /// Routes the event based on its `PropagationMode`:
    /// - `Full`: Added to passthrough queue for immediate forwarding
    /// - `Summary`: Added to aggregation window
    /// - `Query`: Stored locally
    /// - `Local`: Ignored
    pub fn receive(&self, event: HiveEvent) -> Result<()> {
        let routing = event.routing.as_ref();

        let propagation = routing
            .map(|r| {
                PropagationMode::try_from(r.propagation).unwrap_or(PropagationMode::PropagationFull)
            })
            .unwrap_or(PropagationMode::PropagationFull);

        match propagation {
            PropagationMode::PropagationFull => {
                // Forward immediately without aggregation
                let priority = routing
                    .map(|r| {
                        EventPriority::try_from(r.priority).unwrap_or(EventPriority::PriorityNormal)
                    })
                    .unwrap_or(EventPriority::PriorityNormal);

                let mut queue = self.passthrough_queue.write().unwrap();
                queue.push(event);
                let _ = priority; // Used implicitly via event.routing in push
            }

            PropagationMode::PropagationSummary => {
                // Add to aggregation window
                let key = (event.event_class, event.event_type.clone());
                let window_duration = routing
                    .map(|r| {
                        if r.aggregation_window_ms > 0 {
                            Duration::from_millis(r.aggregation_window_ms as u64)
                        } else {
                            self.default_window_duration
                        }
                    })
                    .unwrap_or(self.default_window_duration);

                let event_class =
                    EventClass::try_from(event.event_class).unwrap_or(EventClass::Unspecified);

                let mut windows = self.windows.write().unwrap();
                let window = windows.entry(key).or_insert_with(|| {
                    AggregationWindow::new(event_class, &event.event_type, window_duration)
                });

                window.add(event);
            }

            PropagationMode::PropagationQuery => {
                // Store locally for query access
                let mut store = self.queryable_store.write().unwrap();
                store.insert(event.event_id.clone(), event);
            }

            PropagationMode::PropagationLocal => {
                // Should not reach aggregator, but handle gracefully by ignoring
            }
        }

        Ok(())
    }

    /// Flush all windows that have expired and generate summaries
    ///
    /// Returns the number of summaries generated.
    pub fn flush_expired_windows(&self) -> usize {
        let mut windows = self.windows.write().unwrap();
        let mut summaries_generated = 0;

        let expired_keys: Vec<WindowKey> = windows
            .iter()
            .filter(|(_, w)| w.should_flush() && !w.events.is_empty())
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            if let Some(window) = windows.get_mut(&key) {
                if let Some(summary_event) = self.generate_summary(window) {
                    let mut queue = self.summary_queue.write().unwrap();
                    queue.push(summary_event);
                    summaries_generated += 1;
                }
                window.reset();
            }
        }

        summaries_generated
    }

    /// Force flush all windows regardless of expiry
    ///
    /// Useful for graceful shutdown or immediate summary generation.
    pub fn flush_all_windows(&self) -> usize {
        let mut windows = self.windows.write().unwrap();
        let mut summaries_generated = 0;

        let non_empty_keys: Vec<WindowKey> = windows
            .iter()
            .filter(|(_, w)| !w.events.is_empty())
            .map(|(k, _)| k.clone())
            .collect();

        for key in non_empty_keys {
            if let Some(window) = windows.get_mut(&key) {
                if let Some(summary_event) = self.generate_summary(window) {
                    let mut queue = self.summary_queue.write().unwrap();
                    queue.push(summary_event);
                    summaries_generated += 1;
                }
                window.reset();
            }
        }

        summaries_generated
    }

    /// Generate a summary event from an aggregation window
    fn generate_summary(&self, window: &AggregationWindow) -> Option<HiveEvent> {
        if window.events.is_empty() {
            return None;
        }

        let summary_id = self.generate_summary_id();
        let now = current_timestamp();

        // Get the appropriate strategy or use default
        let strategies = self.summary_strategies.read().unwrap();

        // Try to find a matching strategy by event type prefix
        let event_type_base = window
            .event_type
            .split('.')
            .next()
            .unwrap_or(&window.event_type);

        let summary_payload = if let Some(strategy) = strategies.get(event_type_base) {
            strategy.summarize(window.events())
        } else {
            // Use default strategy
            DefaultSummaryStrategy::new(&window.event_type).summarize(window.events())
        };

        // Create the EventSummary
        let event_summary = EventSummary {
            formation_id: self.echelon_id.clone(),
            window_start: Some(now), // We'd need to track actual start time
            window_end: Some(now),
            event_class: window.event_class as i32,
            event_type: window.event_type.clone(),
            event_count: window.event_count() as u32,
            source_node_ids: window.source_nodes().iter().cloned().collect(),
            summary_type_url: format!("type.hive/summary.{}", window.event_type),
            summary_value: summary_payload,
        };

        // Wrap summary in HiveEvent for transmission
        Some(HiveEvent {
            event_id: summary_id,
            timestamp: Some(now),
            source_node_id: self.echelon_id.clone(),
            source_formation_id: self.echelon_id.clone(),
            source_instance_id: None,
            event_class: window.event_class as i32,
            event_type: format!("{}_summary", window.event_type),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationFull as i32, // Summaries propagate fully
                priority: EventPriority::PriorityNormal as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload_type_url: format!("type.hive/event_summary.{}", window.event_type),
            payload_value: prost::Message::encode_to_vec(&event_summary),
        })
    }

    /// Pop all passthrough events (for transmission to parent)
    pub fn pop_passthrough(&self) -> Vec<HiveEvent> {
        let mut queue = self.passthrough_queue.write().unwrap();
        let mut events = Vec::new();

        // Pop critical first
        events.extend(queue.pop_critical());

        // Then weighted fair queue
        events.extend(queue.pop_weighted(100)); // Configurable batch size

        events
    }

    /// Pop all generated summaries (for transmission to parent)
    pub fn pop_summaries(&self) -> Vec<HiveEvent> {
        let mut queue = self.summary_queue.write().unwrap();
        queue.drain(..).collect()
    }

    /// Pop all events ready for transmission (passthrough + summaries)
    pub fn pop_all(&self) -> Vec<HiveEvent> {
        let mut events = self.pop_passthrough();
        events.extend(self.pop_summaries());
        events
    }

    /// Query locally stored events
    pub fn query_local(&self, event_type: Option<&str>) -> Vec<HiveEvent> {
        let store = self.queryable_store.read().unwrap();
        store
            .values()
            .filter(|e| event_type.is_none() || Some(e.event_type.as_str()) == event_type)
            .cloned()
            .collect()
    }

    /// Get a specific locally stored event by ID
    pub fn get_local(&self, event_id: &str) -> Option<HiveEvent> {
        let store = self.queryable_store.read().unwrap();
        store.get(event_id).cloned()
    }

    /// Get count of events in passthrough queue
    pub fn passthrough_count(&self) -> usize {
        let queue = self.passthrough_queue.read().unwrap();
        queue.len()
    }

    /// Get count of events in queryable store
    pub fn queryable_count(&self) -> usize {
        let store = self.queryable_store.read().unwrap();
        store.len()
    }

    /// Get count of pending summaries
    pub fn summary_count(&self) -> usize {
        let queue = self.summary_queue.read().unwrap();
        queue.len()
    }

    /// Get count of active aggregation windows
    pub fn window_count(&self) -> usize {
        let windows = self.windows.read().unwrap();
        windows.len()
    }

    /// Get the echelon ID
    pub fn echelon_id(&self) -> &str {
        &self.echelon_id
    }

    /// Get the echelon type
    pub fn echelon_type(&self) -> EchelonType {
        self.echelon_type
    }

    /// Clear expired events from queryable store based on TTL
    pub fn enforce_ttl(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let now_secs = now.as_secs();

        let mut store = self.queryable_store.write().unwrap();
        store.retain(|_, event| {
            if let Some(routing) = &event.routing {
                if routing.ttl_seconds > 0 {
                    if let Some(ts) = &event.timestamp {
                        let event_secs = ts.seconds;
                        let expiry = event_secs + routing.ttl_seconds as u64;
                        return now_secs < expiry;
                    }
                }
            }
            true // Keep events without TTL or timestamp
        });
    }

    fn generate_summary_id(&self) -> String {
        let mut counter = self.summary_counter.write().unwrap();
        *counter += 1;
        format!("{}-summary-{}", self.echelon_id, *counter)
    }
}

/// Get current timestamp
fn current_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: now.as_secs(),
        nanos: now.subsec_nanos(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(
        id: &str,
        event_type: &str,
        propagation: PropagationMode,
        priority: EventPriority,
    ) -> HiveEvent {
        HiveEvent {
            event_id: id.to_string(),
            timestamp: Some(current_timestamp()),
            source_node_id: format!("node-{}", id),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Product as i32,
            event_type: event_type.to_string(),
            routing: Some(AggregationPolicy {
                propagation: propagation as i32,
                priority: priority as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0, // Use default window duration from aggregator
            }),
            payload_type_url: String::new(),
            payload_value: vec![],
        }
    }

    #[test]
    fn test_aggregator_creation() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);
        assert_eq!(aggregator.echelon_id(), "squad-1");
        assert_eq!(aggregator.echelon_type(), EchelonType::Squad);
    }

    #[test]
    fn test_full_propagation_passthrough() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);

        let event = make_event(
            "evt-1",
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityNormal,
        );
        aggregator.receive(event).unwrap();

        assert_eq!(aggregator.passthrough_count(), 1);
        assert_eq!(aggregator.queryable_count(), 0);

        let events = aggregator.pop_passthrough();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "evt-1");
    }

    #[test]
    fn test_query_propagation_stored_locally() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);

        let event = make_event(
            "evt-1",
            "telemetry.cpu",
            PropagationMode::PropagationQuery,
            EventPriority::PriorityLow,
        );
        aggregator.receive(event).unwrap();

        assert_eq!(aggregator.passthrough_count(), 0);
        assert_eq!(aggregator.queryable_count(), 1);

        let local = aggregator.query_local(Some("telemetry.cpu"));
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].event_id, "evt-1");
    }

    #[test]
    fn test_summary_propagation_aggregated() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
            .with_default_window_duration(Duration::from_millis(50));

        // Add multiple events for aggregation
        for i in 0..5 {
            let event = make_event(
                &format!("evt-{}", i),
                "detection.vehicle",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            );
            aggregator.receive(event).unwrap();
        }

        assert_eq!(aggregator.window_count(), 1);
        assert_eq!(aggregator.passthrough_count(), 0);

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(100));

        let summaries = aggregator.flush_expired_windows();
        assert_eq!(summaries, 1);

        let summary_events = aggregator.pop_summaries();
        assert_eq!(summary_events.len(), 1);
        assert!(summary_events[0]
            .event_type
            .contains("detection.vehicle_summary"));
    }

    #[test]
    fn test_local_propagation_ignored() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);

        let event = make_event(
            "evt-1",
            "debug.trace",
            PropagationMode::PropagationLocal,
            EventPriority::PriorityLow,
        );
        aggregator.receive(event).unwrap();

        assert_eq!(aggregator.passthrough_count(), 0);
        assert_eq!(aggregator.queryable_count(), 0);
        assert_eq!(aggregator.window_count(), 0);
    }

    #[test]
    fn test_critical_events_passthrough() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);

        // Add critical event
        let event = make_event(
            "critical-1",
            "anomaly.urgent",
            PropagationMode::PropagationFull,
            EventPriority::PriorityCritical,
        );
        aggregator.receive(event).unwrap();

        // Add normal event
        let event = make_event(
            "normal-1",
            "detection",
            PropagationMode::PropagationFull,
            EventPriority::PriorityNormal,
        );
        aggregator.receive(event).unwrap();

        let events = aggregator.pop_passthrough();
        assert_eq!(events.len(), 2);
        // Critical should be first
        assert_eq!(events[0].event_id, "critical-1");
    }

    #[test]
    fn test_flush_all_windows() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
            .with_default_window_duration(Duration::from_secs(3600)); // Long window

        // Add events
        for i in 0..3 {
            let event = make_event(
                &format!("evt-{}", i),
                "detection",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            );
            aggregator.receive(event).unwrap();
        }

        // Force flush all (even though window hasn't expired)
        let summaries = aggregator.flush_all_windows();
        assert_eq!(summaries, 1);

        let summary_events = aggregator.pop_summaries();
        assert_eq!(summary_events.len(), 1);
    }

    #[test]
    fn test_multiple_event_types_separate_windows() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
            .with_default_window_duration(Duration::from_millis(50));

        // Add detection events
        aggregator
            .receive(make_event(
                "det-1",
                "detection.vehicle",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            ))
            .unwrap();

        // Add telemetry events
        aggregator
            .receive(make_event(
                "tel-1",
                "telemetry.cpu",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            ))
            .unwrap();

        assert_eq!(aggregator.window_count(), 2);

        std::thread::sleep(Duration::from_millis(100));
        let summaries = aggregator.flush_expired_windows();
        assert_eq!(summaries, 2);
    }

    #[test]
    fn test_pop_all_includes_passthrough_and_summaries() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
            .with_default_window_duration(Duration::from_millis(50));

        // Add passthrough event
        aggregator
            .receive(make_event(
                "pass-1",
                "anomaly",
                PropagationMode::PropagationFull,
                EventPriority::PriorityHigh,
            ))
            .unwrap();

        // Add summary event
        aggregator
            .receive(make_event(
                "sum-1",
                "detection",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            ))
            .unwrap();

        std::thread::sleep(Duration::from_millis(100));
        aggregator.flush_expired_windows();

        let all = aggregator.pop_all();
        assert_eq!(all.len(), 2); // 1 passthrough + 1 summary
    }

    #[test]
    fn test_source_nodes_tracked_in_window() {
        let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad)
            .with_default_window_duration(Duration::from_millis(50));

        // Add events from different nodes
        for i in 0..3 {
            let mut event = make_event(
                &format!("evt-{}", i),
                "detection",
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            );
            event.source_node_id = format!("node-{}", i);
            aggregator.receive(event).unwrap();
        }

        std::thread::sleep(Duration::from_millis(100));
        aggregator.flush_expired_windows();

        let summaries = aggregator.pop_summaries();
        assert_eq!(summaries.len(), 1);

        // Decode the summary and check source nodes
        let summary: EventSummary =
            prost::Message::decode(&summaries[0].payload_value[..]).unwrap();
        assert_eq!(summary.source_node_ids.len(), 3);
        assert_eq!(summary.event_count, 3);
    }
}
