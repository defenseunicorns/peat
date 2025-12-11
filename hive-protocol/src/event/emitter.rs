//! Event emission with routing policy enforcement (ADR-027)
//!
//! The EventEmitter is responsible for:
//! 1. Accepting events from application code
//! 2. Enforcing propagation policies
//! 3. Queueing events by priority for transmission

use super::priority_queue::PriorityEventQueue;
use crate::Result;
use hive_schema::common::v1::Timestamp;
use hive_schema::event::v1::{
    AggregationPolicy, EventClass, EventPriority, HiveEvent, PropagationMode,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Event emitter for a single node
///
/// Handles event creation, policy enforcement, and priority queuing.
/// Events with `PropagationMode::Local` are stored locally and not queued
/// for transmission.
#[derive(Debug)]
pub struct EventEmitter {
    /// Node identifier
    node_id: String,

    /// Formation this node belongs to
    formation_id: String,

    /// Outbound priority queue for events to transmit
    outbound_queue: Arc<RwLock<PriorityEventQueue>>,

    /// Local event storage for non-propagating events (keyed by event_id)
    local_store: Arc<RwLock<HashMap<String, HiveEvent>>>,

    /// Counter for generating unique event IDs
    event_counter: Arc<RwLock<u64>>,
}

impl EventEmitter {
    /// Create a new event emitter for a node
    pub fn new(node_id: String, formation_id: String) -> Self {
        Self {
            node_id,
            formation_id,
            outbound_queue: Arc::new(RwLock::new(PriorityEventQueue::new())),
            local_store: Arc::new(RwLock::new(HashMap::new())),
            event_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Emit an event with automatic routing based on policy
    ///
    /// Events are routed according to their `AggregationPolicy`:
    /// - `PropagationMode::Local`: Stored locally, not transmitted
    /// - `PropagationMode::Query`: Stored locally for query access
    /// - `PropagationMode::Summary` or `Full`: Queued for transmission
    pub fn emit(&self, event: HiveEvent) -> Result<()> {
        // Validate event has required fields
        if event.event_id.is_empty() {
            return Err(crate::Error::EventOp {
                message: "Event must have an event_id".to_string(),
                operation: "emit".to_string(),
                source: None,
            });
        }

        let routing = event.routing.as_ref();

        // Check propagation mode
        let propagation = routing
            .map(|r| {
                PropagationMode::try_from(r.propagation).unwrap_or(PropagationMode::PropagationFull)
            })
            .unwrap_or(PropagationMode::PropagationFull);

        match propagation {
            PropagationMode::PropagationLocal => {
                // Store locally only, do not queue for transmission
                self.store_local(event)?;
            }
            PropagationMode::PropagationQuery => {
                // Store locally for query access, do not transmit
                self.store_local(event)?;
            }
            PropagationMode::PropagationSummary | PropagationMode::PropagationFull => {
                // Queue for transmission to parent echelon
                let mut queue = self.outbound_queue.write().unwrap();
                queue.push(event);
            }
        }

        Ok(())
    }

    /// Create and emit an event with the given parameters
    ///
    /// Automatically generates event_id and timestamp.
    pub fn emit_new(
        &self,
        event_class: EventClass,
        event_type: String,
        routing: AggregationPolicy,
        payload_type_url: String,
        payload_value: Vec<u8>,
        source_instance_id: Option<String>,
    ) -> Result<String> {
        let event_id = self.generate_event_id();

        let event = HiveEvent {
            event_id: event_id.clone(),
            timestamp: Some(current_timestamp()),
            source_node_id: self.node_id.clone(),
            source_formation_id: self.formation_id.clone(),
            source_instance_id,
            event_class: event_class as i32,
            event_type,
            routing: Some(routing),
            payload_type_url,
            payload_value,
        };

        self.emit(event)?;
        Ok(event_id)
    }

    /// Emit a product event (output from software processing)
    ///
    /// Products are the primary output events from software instances.
    pub fn emit_product(
        &self,
        product_type: &str,
        payload: Vec<u8>,
        propagation: PropagationMode,
        priority: EventPriority,
    ) -> Result<String> {
        let routing = AggregationPolicy {
            propagation: propagation as i32,
            priority: priority as i32,
            ttl_seconds: 300,
            aggregation_window_ms: if propagation == PropagationMode::PropagationSummary {
                1000
            } else {
                0
            },
        };

        self.emit_new(
            EventClass::Product,
            format!("product.{}", product_type),
            routing,
            format!("type.hive/product.{}", product_type),
            payload,
            None,
        )
    }

    /// Emit a telemetry event (metrics, health, diagnostics)
    ///
    /// Telemetry defaults to Query propagation (stored locally, queryable)
    pub fn emit_telemetry(&self, metric_name: &str, payload: Vec<u8>) -> Result<String> {
        let routing = AggregationPolicy {
            propagation: PropagationMode::PropagationQuery as i32,
            priority: EventPriority::PriorityLow as i32,
            ttl_seconds: 3600, // 1 hour TTL for telemetry
            aggregation_window_ms: 0,
        };

        self.emit_new(
            EventClass::Telemetry,
            format!("telemetry.{}", metric_name),
            routing,
            format!("type.hive/telemetry.{}", metric_name),
            payload,
            None,
        )
    }

    /// Emit an anomaly event (unusual patterns, alerts)
    ///
    /// Anomalies default to Full propagation with High priority
    pub fn emit_anomaly(&self, anomaly_type: &str, payload: Vec<u8>) -> Result<String> {
        let routing = AggregationPolicy {
            propagation: PropagationMode::PropagationFull as i32,
            priority: EventPriority::PriorityHigh as i32,
            ttl_seconds: 600, // 10 minutes
            aggregation_window_ms: 0,
        };

        self.emit_new(
            EventClass::Anomaly,
            format!("anomaly.{}", anomaly_type),
            routing,
            format!("type.hive/anomaly.{}", anomaly_type),
            payload,
            None,
        )
    }

    /// Emit a critical event (immediate attention required)
    ///
    /// Critical events have CRITICAL priority and Full propagation
    pub fn emit_critical(&self, event_type: &str, payload: Vec<u8>) -> Result<String> {
        let routing = AggregationPolicy {
            propagation: PropagationMode::PropagationFull as i32,
            priority: EventPriority::PriorityCritical as i32,
            ttl_seconds: 300,
            aggregation_window_ms: 0,
        };

        self.emit_new(
            EventClass::Anomaly,
            format!("critical.{}", event_type),
            routing,
            format!("type.hive/critical.{}", event_type),
            payload,
            None,
        )
    }

    /// Pop all critical events for immediate transmission
    pub fn pop_critical(&self) -> Vec<HiveEvent> {
        let mut queue = self.outbound_queue.write().unwrap();
        queue.pop_critical()
    }

    /// Pop events for transmission using weighted fair queuing
    pub fn pop_events(&self, max_events: usize) -> Vec<HiveEvent> {
        let mut queue = self.outbound_queue.write().unwrap();

        // Always drain critical first
        let mut events = queue.pop_critical();

        // Then weighted fair queue for the rest
        let remaining = max_events.saturating_sub(events.len());
        if remaining > 0 {
            events.extend(queue.pop_weighted(remaining));
        }

        events
    }

    /// Check if there are critical events pending
    pub fn has_critical(&self) -> bool {
        let queue = self.outbound_queue.read().unwrap();
        queue.has_critical()
    }

    /// Get count of pending outbound events
    pub fn pending_count(&self) -> usize {
        let queue = self.outbound_queue.read().unwrap();
        queue.len()
    }

    /// Get count of locally stored events
    pub fn local_count(&self) -> usize {
        let store = self.local_store.read().unwrap();
        store.len()
    }

    /// Query locally stored events by event type
    pub fn query_local(&self, event_type: Option<&str>) -> Vec<HiveEvent> {
        let store = self.local_store.read().unwrap();
        store
            .values()
            .filter(|e| event_type.is_none() || Some(e.event_type.as_str()) == event_type)
            .cloned()
            .collect()
    }

    /// Get a specific locally stored event by ID
    pub fn get_local(&self, event_id: &str) -> Option<HiveEvent> {
        let store = self.local_store.read().unwrap();
        store.get(event_id).cloned()
    }

    /// Get the node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get the formation ID
    pub fn formation_id(&self) -> &str {
        &self.formation_id
    }

    // Internal helpers

    fn store_local(&self, event: HiveEvent) -> Result<()> {
        let mut store = self.local_store.write().unwrap();
        store.insert(event.event_id.clone(), event);
        Ok(())
    }

    fn generate_event_id(&self) -> String {
        let mut counter = self.event_counter.write().unwrap();
        *counter += 1;
        format!("{}-{}", self.node_id, *counter)
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

    #[test]
    fn test_emit_event_full_propagation() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let event = HiveEvent {
            event_id: "evt-1".to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Product as i32,
            event_type: "detection".to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationFull as i32,
                priority: EventPriority::PriorityNormal as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: vec![],
        };

        emitter.emit(event).unwrap();

        assert_eq!(emitter.pending_count(), 1);
        assert_eq!(emitter.local_count(), 0);
    }

    #[test]
    fn test_emit_event_local_propagation() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let event = HiveEvent {
            event_id: "evt-1".to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Telemetry as i32,
            event_type: "debug".to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationLocal as i32,
                priority: EventPriority::PriorityLow as i32,
                ttl_seconds: 60,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: vec![],
        };

        emitter.emit(event).unwrap();

        assert_eq!(emitter.pending_count(), 0);
        assert_eq!(emitter.local_count(), 1);
    }

    #[test]
    fn test_emit_event_query_propagation() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let event = HiveEvent {
            event_id: "evt-1".to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Telemetry as i32,
            event_type: "metrics".to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationQuery as i32,
                priority: EventPriority::PriorityLow as i32,
                ttl_seconds: 3600,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: vec![],
        };

        emitter.emit(event).unwrap();

        // Query mode stores locally, doesn't transmit
        assert_eq!(emitter.pending_count(), 0);
        assert_eq!(emitter.local_count(), 1);

        // Should be queryable
        let local = emitter.query_local(Some("metrics"));
        assert_eq!(local.len(), 1);
    }

    #[test]
    fn test_emit_new_generates_id() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let routing = AggregationPolicy {
            propagation: PropagationMode::PropagationFull as i32,
            priority: EventPriority::PriorityNormal as i32,
            ttl_seconds: 300,
            aggregation_window_ms: 0,
        };

        let event_id = emitter
            .emit_new(
                EventClass::Product,
                "test".to_string(),
                routing,
                String::new(),
                vec![],
                None,
            )
            .unwrap();

        assert!(event_id.starts_with("node-1-"));
        assert_eq!(emitter.pending_count(), 1);
    }

    #[test]
    fn test_emit_product() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let event_id = emitter
            .emit_product(
                "output_v1",
                vec![1, 2, 3],
                PropagationMode::PropagationSummary,
                EventPriority::PriorityNormal,
            )
            .unwrap();

        assert!(!event_id.is_empty());
        assert_eq!(emitter.pending_count(), 1);
    }

    #[test]
    fn test_emit_telemetry_stored_locally() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        emitter.emit_telemetry("cpu_usage", vec![42]).unwrap();

        // Telemetry defaults to QUERY mode - stored locally
        assert_eq!(emitter.pending_count(), 0);
        assert_eq!(emitter.local_count(), 1);
    }

    #[test]
    fn test_emit_critical() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        emitter.emit_critical("urgent_condition", vec![]).unwrap();

        assert!(emitter.has_critical());

        let critical = emitter.pop_critical();
        assert_eq!(critical.len(), 1);
        assert!(critical[0].event_type.starts_with("critical."));
    }

    #[test]
    fn test_pop_events_critical_first() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        // Add normal event first
        emitter
            .emit_product(
                "normal_output",
                vec![],
                PropagationMode::PropagationFull,
                EventPriority::PriorityNormal,
            )
            .unwrap();

        // Add critical event second
        emitter
            .emit_critical("immediate_attention", vec![])
            .unwrap();

        // Pop should return critical first
        let events = emitter.pop_events(10);
        assert_eq!(events.len(), 2);
        assert!(events[0].event_type.starts_with("critical."));
    }

    #[test]
    fn test_emit_without_event_id_fails() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        let event = HiveEvent {
            event_id: String::new(), // Empty ID should fail
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Product as i32,
            event_type: "test".to_string(),
            routing: None,
            payload_type_url: String::new(),
            payload_value: vec![],
        };

        let result = emitter.emit(event);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_local_by_type() {
        let emitter = EventEmitter::new("node-1".to_string(), "squad-1".to_string());

        // Emit different telemetry types
        emitter.emit_telemetry("cpu", vec![]).unwrap();
        emitter.emit_telemetry("memory", vec![]).unwrap();
        emitter.emit_telemetry("cpu", vec![]).unwrap();

        // Query all
        let all = emitter.query_local(None);
        assert_eq!(all.len(), 3);

        // Query by type
        let cpu = emitter.query_local(Some("telemetry.cpu"));
        assert_eq!(cpu.len(), 2);

        let memory = emitter.query_local(Some("telemetry.memory"));
        assert_eq!(memory.len(), 1);
    }
}
