//! Relay ingest: converts [`HiveEvent`] into typed [`ViewerEvent`] for frontend consumption.
//!
//! The ingest module inspects the `event_type` field of incoming `HiveEvent` messages
//! and converts recognized capability lifecycle events into strongly-typed `ViewerEvent`
//! variants. Unrecognized event types are ignored (returns `None`).

use super::protocol::ViewerEvent;
use hive_schema::event::v1::HiveEvent;

/// Event type prefix/identifier for capability degradation events.
const EVENT_TYPE_CAPABILITY_DEGRADATION: &str = "capability_degradation";

/// Event type prefix/identifier for logistical events.
const EVENT_TYPE_LOGISTICAL_EVENT: &str = "logistical_event";

/// Attempt to convert a [`HiveEvent`] into a [`ViewerEvent`].
///
/// Returns `Some(ViewerEvent)` if the event's `event_type` field matches a known
/// capability lifecycle event type. Returns `None` for unrecognized event types.
///
/// The conversion extracts structured fields from the event's JSON payload.
/// The `event_type` field is the discriminator — this is the same field used
/// throughout the HiveEvent system for routing and classification.
///
/// # Errors
///
/// Returns `Err` if the payload is present but contains malformed JSON.
pub fn try_into_viewer_event(event: &HiveEvent) -> Result<Option<ViewerEvent>, IngestError> {
    // Match on event_type field to detect capability lifecycle events
    let event_type = event.event_type.as_str();

    if event_type == EVENT_TYPE_CAPABILITY_DEGRADATION
        || event_type.ends_with(".capability_degradation")
    {
        let payload = parse_json_payload(&event.payload_value)?;
        return Ok(Some(ViewerEvent::CapabilityDegradation {
            node_id: payload
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&event.source_node_id)
                .to_string(),
            capability_type: payload
                .get("capability_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            confidence_before: payload
                .get("confidence_before")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            confidence_after: payload
                .get("confidence_after")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            cause: payload
                .get("cause")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            decay_rate_per_hour: payload
                .get("decay_rate_per_hour")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
        }));
    }

    if event_type == EVENT_TYPE_LOGISTICAL_EVENT || event_type.ends_with(".logistical_event") {
        let payload = parse_json_payload(&event.payload_value)?;
        return Ok(Some(ViewerEvent::LogisticalEvent {
            node_id: payload
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&event.source_node_id)
                .to_string(),
            event_subtype: payload
                .get("event_subtype")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            capability_sustained: payload
                .get("capability_sustained")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            eta_restore: payload.get("eta_restore").and_then(|v| v.as_f64()),
            details: payload
                .get("details")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }));
    }

    Ok(None)
}

/// Parse a JSON payload from raw bytes.
///
/// Empty payloads return an empty JSON object.
fn parse_json_payload(bytes: &[u8]) -> Result<serde_json::Value, IngestError> {
    if bytes.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_slice(bytes).map_err(|e| IngestError::MalformedPayload {
        message: e.to_string(),
    })
}

/// Errors that can occur during event ingest.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// The event payload contained malformed JSON.
    #[error("malformed event payload: {message}")]
    MalformedPayload { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_schema::event::v1::{AggregationPolicy, EventClass, EventPriority, PropagationMode};

    fn make_hive_event(event_type: &str, payload_json: &str) -> HiveEvent {
        HiveEvent {
            event_id: "evt-test-1".to_string(),
            timestamp: None,
            source_node_id: "node-1".to_string(),
            source_formation_id: "squad-1".to_string(),
            source_instance_id: None,
            event_class: EventClass::Product as i32,
            event_type: event_type.to_string(),
            routing: Some(AggregationPolicy {
                propagation: PropagationMode::PropagationFull as i32,
                priority: EventPriority::PriorityNormal as i32,
                ttl_seconds: 300,
                aggregation_window_ms: 0,
            }),
            payload_type_url: String::new(),
            payload_value: payload_json.as_bytes().to_vec(),
        }
    }

    #[test]
    fn test_ingest_capability_degradation() {
        let event = make_hive_event(
            "capability_degradation",
            r#"{
                "node_id": "crane-2",
                "capability_type": "payload",
                "confidence_before": 0.85,
                "confidence_after": 0.72,
                "cause": "hydraulic_wear",
                "decay_rate_per_hour": 0.03
            }"#,
        );

        let result = try_into_viewer_event(&event).unwrap().unwrap();
        match result {
            ViewerEvent::CapabilityDegradation {
                node_id,
                capability_type,
                confidence_before,
                confidence_after,
                cause,
                decay_rate_per_hour,
            } => {
                assert_eq!(node_id, "crane-2");
                assert_eq!(capability_type, "payload");
                assert!((confidence_before - 0.85).abs() < 0.001);
                assert!((confidence_after - 0.72).abs() < 0.001);
                assert_eq!(cause, "hydraulic_wear");
                assert!((decay_rate_per_hour - 0.03).abs() < 0.001);
            }
            _ => panic!("Expected CapabilityDegradation"),
        }
    }

    #[test]
    fn test_ingest_logistical_event() {
        let event = make_hive_event(
            "logistical_event",
            r#"{
                "node_id": "crane-2",
                "event_subtype": "maintenance_scheduled",
                "capability_sustained": "hydraulic_lift",
                "eta_restore": 1200.0,
                "details": "Scheduled hydraulic fluid replacement"
            }"#,
        );

        let result = try_into_viewer_event(&event).unwrap().unwrap();
        match result {
            ViewerEvent::LogisticalEvent {
                node_id,
                event_subtype,
                capability_sustained,
                eta_restore,
                details,
            } => {
                assert_eq!(node_id, "crane-2");
                assert_eq!(event_subtype, "maintenance_scheduled");
                assert_eq!(capability_sustained, "hydraulic_lift");
                assert!((eta_restore.unwrap() - 1200.0).abs() < 0.001);
                assert_eq!(details, "Scheduled hydraulic fluid replacement");
            }
            _ => panic!("Expected LogisticalEvent"),
        }
    }

    #[test]
    fn test_ingest_prefixed_event_type() {
        let event = make_hive_event(
            "product.capability_degradation",
            r#"{"node_id": "uav-1", "capability_type": "sensor", "confidence_before": 1.0, "confidence_after": 0.8, "cause": "battery_drain", "decay_rate_per_hour": 0.05}"#,
        );

        let result = try_into_viewer_event(&event).unwrap();
        assert!(result.is_some());
        assert!(matches!(
            result.unwrap(),
            ViewerEvent::CapabilityDegradation { .. }
        ));
    }

    #[test]
    fn test_ingest_unknown_event_type_returns_none() {
        let event = make_hive_event("telemetry.cpu_usage", r#"{"cpu": 0.75}"#);
        let result = try_into_viewer_event(&event).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_ingest_empty_payload_uses_defaults() {
        let event = make_hive_event("capability_degradation", "");
        let result = try_into_viewer_event(&event).unwrap().unwrap();
        match result {
            ViewerEvent::CapabilityDegradation {
                node_id,
                capability_type,
                ..
            } => {
                // Falls back to source_node_id when node_id not in payload
                assert_eq!(node_id, "node-1");
                assert_eq!(capability_type, "");
            }
            _ => panic!("Expected CapabilityDegradation"),
        }
    }

    #[test]
    fn test_ingest_malformed_payload_returns_error() {
        let event = make_hive_event("capability_degradation", "not valid json{{{");
        let result = try_into_viewer_event(&event);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("malformed"));
    }

    #[test]
    fn test_ingest_logistical_event_no_eta() {
        let event = make_hive_event(
            "logistical_event",
            r#"{
                "node_id": "node-5",
                "event_subtype": "resupply_delivered",
                "capability_sustained": "power",
                "details": "Battery pack delivered"
            }"#,
        );

        let result = try_into_viewer_event(&event).unwrap().unwrap();
        match result {
            ViewerEvent::LogisticalEvent { eta_restore, .. } => {
                assert!(eta_restore.is_none());
            }
            _ => panic!("Expected LogisticalEvent"),
        }
    }
}
