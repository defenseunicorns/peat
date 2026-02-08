//! Event protocol types for the HIVE Operational Viewer.
//!
//! All data flows as JSON over WebSocket. The protocol is domain-agnostic —
//! any HIVE simulation that emits these event types can be visualized.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Server → Client messages sent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewerEvent {
    /// Full state snapshot sent on client connect.
    StateSnapshot {
        documents: HashMap<String, serde_json::Value>,
        events: Vec<HiveEvent>,
        sim_clock: Option<SimClock>,
    },

    /// One OODA cycle completed by an agent.
    OodaCycle(OodaCycleEvent),

    /// A HIVE document was updated.
    DocumentUpdate(DocumentUpdateEvent),

    /// A HIVE event was emitted (capability change, contention, etc.).
    HiveEvent(HiveEvent),

    /// Simulation clock tick.
    SimClock(SimClock),
}

/// OODA cycle metrics from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OodaCycleEvent {
    pub node_id: String,
    pub cycle: u32,
    pub sim_time: String,
    pub action: String,
    pub success: bool,
    pub contention_retry: bool,
    #[serde(default)]
    pub observe_ms: f64,
    #[serde(default)]
    pub decide_ms: f64,
    #[serde(default)]
    pub act_ms: f64,
    /// Extra fields from the METRICS JSON that don't map to known fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Document update event — a field changed in a HIVE document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUpdateEvent {
    pub collection: String,
    pub doc_id: String,
    pub fields: serde_json::Value,
}

/// HIVE event (capability change, contention, escalation, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiveEvent {
    pub event_type: String,
    pub source: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default)]
    pub timestamp: Option<String>,
}

fn default_priority() -> String {
    "ROUTINE".to_string()
}

/// Simulation clock state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimClock {
    pub sim_time: String,
    pub real_elapsed_ms: f64,
}

/// Ingest event — parsed from stdin/TCP input lines.
/// The relay server parses raw JSON lines into these, then converts
/// to ViewerEvent for broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestLine {
    /// The raw JSON object from the simulation.
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

impl IngestLine {
    /// Try to classify this JSON line as a ViewerEvent.
    /// Returns None if the line doesn't match any known event type.
    pub fn classify(&self) -> Option<ViewerEvent> {
        // Check for METRICS type (OODA cycle output from Python sim)
        // The sim emits {"type": "METRICS", "event": "ooda_cycle", ...}
        if self.fields.get("type").and_then(|v| v.as_str()) == Some("METRICS") {
            return self.as_ooda_cycle();
        }

        // Check for event_type field (HIVE events)
        if self.fields.contains_key("event_type") {
            return self.as_hive_event();
        }

        // Check for document update markers
        if self.fields.contains_key("collection") && self.fields.contains_key("doc_id") {
            return self.as_document_update();
        }

        None
    }

    fn as_ooda_cycle(&self) -> Option<ViewerEvent> {
        let node_id = self.fields.get("node_id")?.as_str()?.to_string();
        let cycle = self.fields.get("cycle")?.as_u64()? as u32;
        let sim_time = self
            .fields
            .get("sim_time")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let action = self
            .fields
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let success = self
            .fields
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let contention_retry = self
            .fields
            .get("contention_retry")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let observe_ms = self
            .fields
            .get("observe_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let decide_ms = self
            .fields
            .get("decide_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let act_ms = self
            .fields
            .get("act_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Collect extra fields not already mapped
        let known_keys: &[&str] = &[
            "type",
            "event",
            "node_id",
            "cycle",
            "sim_time",
            "action",
            "success",
            "contention_retry",
            "observe_ms",
            "decide_ms",
            "act_ms",
        ];
        let extra: HashMap<String, serde_json::Value> = self
            .fields
            .iter()
            .filter(|(k, _)| !known_keys.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Some(ViewerEvent::OodaCycle(OodaCycleEvent {
            node_id,
            cycle,
            sim_time,
            action,
            success,
            contention_retry,
            observe_ms,
            decide_ms,
            act_ms,
            extra,
        }))
    }

    fn as_hive_event(&self) -> Option<ViewerEvent> {
        let event_type = self.fields.get("event_type")?.as_str()?.to_string();
        let source = self
            .fields
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let priority = self
            .fields
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("ROUTINE")
            .to_string();
        let details = self
            .fields
            .get("details")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let timestamp = self
            .fields
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(ViewerEvent::HiveEvent(HiveEvent {
            event_type,
            source,
            priority,
            details,
            timestamp,
        }))
    }

    fn as_document_update(&self) -> Option<ViewerEvent> {
        let collection = self.fields.get("collection")?.as_str()?.to_string();
        let doc_id = self.fields.get("doc_id")?.as_str()?.to_string();
        let fields = self
            .fields
            .get("fields")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Some(ViewerEvent::DocumentUpdate(DocumentUpdateEvent {
            collection,
            doc_id,
            fields,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_metrics_line() {
        let json = r#"{"type":"METRICS","event":"ooda_cycle","node_id":"crane-1","cycle":1,"sim_time":"T+00:00","action":"complete_container_move","success":true,"contention_retry":false,"observe_ms":1.2,"decide_ms":0.5,"act_ms":0.8,"total_ms":2.5,"timestamp_us":123456}"#;
        let line: IngestLine = serde_json::from_str(json).unwrap();
        let event = line.classify().expect("should classify as OODA cycle");
        match event {
            ViewerEvent::OodaCycle(e) => {
                assert_eq!(e.node_id, "crane-1");
                assert_eq!(e.cycle, 1);
                assert_eq!(e.action, "complete_container_move");
                assert!(e.success);
            }
            _ => panic!("expected OodaCycle"),
        }
    }

    #[test]
    fn classify_hive_event() {
        let json = r#"{"event_type":"CAPABILITY_GAP","source":"hold-agg-3","priority":"HIGH","details":{"gaps":["hazmat"]}}"#;
        let line: IngestLine = serde_json::from_str(json).unwrap();
        let event = line.classify().expect("should classify as HIVE event");
        match event {
            ViewerEvent::HiveEvent(e) => {
                assert_eq!(e.event_type, "CAPABILITY_GAP");
                assert_eq!(e.source, "hold-agg-3");
            }
            _ => panic!("expected HiveEvent"),
        }
    }

    #[test]
    fn unrecognized_line_returns_none() {
        let json = r#"{"random":"data","foo":42}"#;
        let line: IngestLine = serde_json::from_str(json).unwrap();
        assert!(line.classify().is_none());
    }
}
