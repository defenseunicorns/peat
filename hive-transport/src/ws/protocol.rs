//! Viewer event protocol types for WebSocket relay
//!
//! Defines the `ViewerEvent` enum representing events sent to viewer frontends
//! over WebSocket connections. These events are converted from `HiveEvent` by
//! the ingest module.

use serde::{Deserialize, Serialize};

/// Events sent to viewer frontends over WebSocket.
///
/// Each variant is tagged with a `"type"` field in JSON serialization,
/// matching the `event_type` string used in [`HiveEvent`].
///
/// [`HiveEvent`]: hive_schema::event::v1::HiveEvent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ViewerEvent {
    /// A node's capability has degraded (confidence decreased).
    ///
    /// Emitted when equipment health declines, sensors degrade, or any
    /// capability's confidence score drops. The `confidence_before` and
    /// `confidence_after` fields use the canonical `[0.0, 1.0]` range
    /// from `capability.proto`.
    #[serde(rename = "capability_degradation")]
    CapabilityDegradation {
        /// Node whose capability degraded
        node_id: String,
        /// Type of capability affected (e.g., "sensor", "compute", "mobility")
        capability_type: String,
        /// Confidence score before degradation `[0.0, 1.0]`
        confidence_before: f32,
        /// Confidence score after degradation `[0.0, 1.0]`
        confidence_after: f32,
        /// Human-readable cause of degradation (e.g., "hydraulic_wear", "battery_drain")
        cause: String,
        /// Rate of confidence decay per hour (e.g., 0.05 means 5% per hour)
        decay_rate_per_hour: f32,
    },

    /// A logistical support event affecting capability sustainment.
    ///
    /// Represents maintenance, resupply, recertification, shift relief, and
    /// other logistical actions that sustain or restore capabilities.
    #[serde(rename = "logistical_event")]
    LogisticalEvent {
        /// Node involved in the logistical event
        node_id: String,
        /// Specific event subtype (e.g., "maintenance_scheduled", "resupply_delivered",
        /// "recertification_complete", "shift_relief_arrived")
        event_subtype: String,
        /// Capability being sustained or restored by this logistical action
        capability_sustained: String,
        /// Estimated time to restore capability (seconds), if applicable
        eta_restore: Option<f64>,
        /// Human-readable details about the logistical event
        details: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_degradation_serialization() {
        let event = ViewerEvent::CapabilityDegradation {
            node_id: "crane-2".to_string(),
            capability_type: "payload".to_string(),
            confidence_before: 0.85,
            confidence_after: 0.72,
            cause: "hydraulic_wear".to_string(),
            decay_rate_per_hour: 0.03,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"capability_degradation""#));
        assert!(json.contains(r#""node_id":"crane-2""#));
        assert!(json.contains(r#""confidence_before":0.85"#));
        assert!(json.contains(r#""confidence_after":0.72"#));

        let deserialized: ViewerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_logistical_event_serialization() {
        let event = ViewerEvent::LogisticalEvent {
            node_id: "crane-2".to_string(),
            event_subtype: "maintenance_scheduled".to_string(),
            capability_sustained: "hydraulic_lift".to_string(),
            eta_restore: Some(1200.0),
            details: "Scheduled hydraulic fluid replacement".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"logistical_event""#));
        assert!(json.contains(r#""event_subtype":"maintenance_scheduled""#));
        assert!(json.contains(r#""eta_restore":1200.0"#));

        let deserialized: ViewerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_logistical_event_no_eta() {
        let event = ViewerEvent::LogisticalEvent {
            node_id: "node-5".to_string(),
            event_subtype: "resupply_delivered".to_string(),
            capability_sustained: "power".to_string(),
            eta_restore: None,
            details: "Battery pack delivered and installed".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""eta_restore":null"#));

        let deserialized: ViewerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_deserialize_from_json_string() {
        let json = r#"{
            "type": "capability_degradation",
            "node_id": "uav-3",
            "capability_type": "sensor",
            "confidence_before": 1.0,
            "confidence_after": 0.6,
            "cause": "lens_contamination",
            "decay_rate_per_hour": 0.1
        }"#;

        let event: ViewerEvent = serde_json::from_str(json).unwrap();
        match event {
            ViewerEvent::CapabilityDegradation {
                node_id,
                capability_type,
                confidence_before,
                confidence_after,
                ..
            } => {
                assert_eq!(node_id, "uav-3");
                assert_eq!(capability_type, "sensor");
                assert!((confidence_before - 1.0).abs() < f32::EPSILON);
                assert!((confidence_after - 0.6).abs() < f32::EPSILON);
            }
            _ => panic!("Expected CapabilityDegradation"),
        }
    }
}
