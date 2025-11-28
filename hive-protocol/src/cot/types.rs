//! HIVE message types for TAK integration
//!
//! These types represent HIVE messages that can be translated to/from CoT format
//! for integration with TAK (Team Awareness Kit) systems.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Position with WGS84 coordinates and accuracy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Latitude in degrees (WGS84)
    pub lat: f64,
    /// Longitude in degrees (WGS84)
    pub lon: f64,
    /// Circular Error Probable in meters (accuracy)
    pub cep_m: Option<f64>,
    /// Height Above Ellipsoid in meters
    pub hae: Option<f64>,
}

impl Position {
    /// Create a new position
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            cep_m: None,
            hae: None,
        }
    }

    /// Create a position with accuracy
    pub fn with_accuracy(lat: f64, lon: f64, cep_m: f64) -> Self {
        Self {
            lat,
            lon,
            cep_m: Some(cep_m),
            hae: None,
        }
    }

    /// Create a position with full 3D coordinates
    pub fn with_altitude(lat: f64, lon: f64, hae: f64, cep_m: Option<f64>) -> Self {
        Self {
            lat,
            lon,
            cep_m,
            hae: Some(hae),
        }
    }
}

/// Velocity with bearing and speed
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Velocity {
    /// Bearing in degrees (0 = North, clockwise)
    pub bearing: f64,
    /// Speed in meters per second
    pub speed_mps: f64,
}

impl Velocity {
    /// Create a new velocity
    pub fn new(bearing: f64, speed_mps: f64) -> Self {
        Self { bearing, speed_mps }
    }

    /// Check if stationary (speed below threshold)
    pub fn is_stationary(&self, threshold_mps: f64) -> bool {
        self.speed_mps < threshold_mps
    }
}

/// Track update from a HIVE platform's sensor
///
/// Represents a detected entity (person, vehicle, etc.) being tracked by a HIVE sensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackUpdate {
    /// Unique track identifier
    pub track_id: String,
    /// Classification of tracked entity (person, vehicle, aircraft, etc.)
    pub classification: String,
    /// Detection confidence (0.0 - 1.0)
    pub confidence: f64,
    /// Current position
    pub position: Position,
    /// Current velocity (if available)
    pub velocity: Option<Velocity>,
    /// Custom attributes (key-value pairs)
    pub attributes: HashMap<String, serde_json::Value>,
    /// Platform that detected this track
    pub source_platform: String,
    /// AI model that generated detection
    pub source_model: String,
    /// Version of the AI model
    pub model_version: String,
    /// Timestamp of the update
    pub timestamp: DateTime<Utc>,
    /// Cell membership (if assigned)
    pub cell_id: Option<String>,
    /// Formation membership (if assigned)
    pub formation_id: Option<String>,
}

impl TrackUpdate {
    /// Create a new track update
    pub fn new(
        track_id: String,
        classification: String,
        confidence: f64,
        position: Position,
        source_platform: String,
        source_model: String,
        model_version: String,
    ) -> Self {
        Self {
            track_id,
            classification,
            confidence: confidence.clamp(0.0, 1.0),
            position,
            velocity: None,
            attributes: HashMap::new(),
            source_platform,
            source_model,
            model_version,
            timestamp: Utc::now(),
            cell_id: None,
            formation_id: None,
        }
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: &str, value: serde_json::Value) -> Self {
        self.attributes.insert(key.to_string(), value);
        self
    }

    /// Set velocity
    pub fn with_velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = Some(velocity);
        self
    }

    /// Set cell membership
    pub fn with_cell(mut self, cell_id: String) -> Self {
        self.cell_id = Some(cell_id);
        self
    }

    /// Set formation membership
    pub fn with_formation(mut self, formation_id: String) -> Self {
        self.formation_id = Some(formation_id);
        self
    }
}

/// Operational status of a platform or capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationalStatus {
    /// Platform is ready but not actively processing
    Ready,
    /// Platform is actively processing/sensing
    Active,
    /// Platform has reduced capability
    Degraded,
    /// Platform is offline
    Offline,
    /// Platform is loading/initializing
    Loading,
}

impl OperationalStatus {
    /// Convert to CoT-compatible string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Active => "ACTIVE",
            Self::Degraded => "DEGRADED",
            Self::Offline => "OFFLINE",
            Self::Loading => "LOADING",
        }
    }
}

/// Capability advertisement from a HIVE platform
///
/// Announces what a platform can do (sensor types, compute capabilities, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityAdvertisement {
    /// Platform identifier
    pub platform_id: String,
    /// Platform type (UGV, UAV, Soldier System, etc.)
    pub platform_type: String,
    /// Current position
    pub position: Position,
    /// Operational status
    pub status: OperationalStatus,
    /// Readiness level (0.0 - 1.0)
    pub readiness: f64,
    /// Capabilities offered by this platform
    pub capabilities: Vec<CapabilityInfo>,
    /// Cell membership (if assigned)
    pub cell_id: Option<String>,
    /// Formation membership (if assigned)
    pub formation_id: Option<String>,
    /// Timestamp of the advertisement
    pub timestamp: DateTime<Utc>,
}

/// Information about a single capability
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityInfo {
    /// Capability type (OBJECT_TRACKING, COMPUTE, COMMUNICATION, etc.)
    pub capability_type: String,
    /// Model or sensor name
    pub model_name: String,
    /// Version string
    pub version: String,
    /// Precision/confidence of this capability (0.0 - 1.0)
    pub precision: f64,
    /// Current status
    pub status: OperationalStatus,
}

impl CapabilityAdvertisement {
    /// Create a new capability advertisement
    pub fn new(
        platform_id: String,
        platform_type: String,
        position: Position,
        status: OperationalStatus,
        readiness: f64,
    ) -> Self {
        Self {
            platform_id,
            platform_type,
            position,
            status,
            readiness: readiness.clamp(0.0, 1.0),
            capabilities: Vec::new(),
            cell_id: None,
            formation_id: None,
            timestamp: Utc::now(),
        }
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: CapabilityInfo) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Set cell membership
    pub fn with_cell(mut self, cell_id: String) -> Self {
        self.cell_id = Some(cell_id);
        self
    }
}

/// State of a handoff operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffState {
    /// Handoff initiated, awaiting acceptance
    Initiated,
    /// Handoff accepted by receiving cell
    Accepted,
    /// Track custody transferred
    Transferred,
    /// Handoff completed successfully
    Completed,
    /// Handoff failed or rejected
    Failed,
}

impl HandoffState {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Initiated => "INITIATED",
            Self::Accepted => "ACCEPTED",
            Self::Transferred => "TRANSFERRED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }
}

/// Handoff message for track custody transfer between cells
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffMessage {
    /// Track being handed off
    pub track_id: String,
    /// Current track position
    pub position: Position,
    /// Source cell releasing the track
    pub source_cell: String,
    /// Target cell receiving the track
    pub target_cell: String,
    /// Current handoff state
    pub state: HandoffState,
    /// Reason for handoff (boundary crossing, capability match, etc.)
    pub reason: String,
    /// Priority level (1-5, with 1 being highest)
    pub priority: u8,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl HandoffMessage {
    /// Create a new handoff message
    pub fn new(
        track_id: String,
        position: Position,
        source_cell: String,
        target_cell: String,
        reason: String,
    ) -> Self {
        Self {
            track_id,
            position,
            source_cell,
            target_cell,
            state: HandoffState::Initiated,
            reason,
            priority: 3, // Default normal priority
            timestamp: Utc::now(),
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.clamp(1, 5);
        self
    }

    /// Update state
    pub fn with_state(mut self, state: HandoffState) -> Self {
        self.state = state;
        self
    }
}

/// Aggregated capability summary for a formation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationCapabilitySummary {
    /// Formation identifier
    pub formation_id: String,
    /// Formation name/callsign
    pub callsign: String,
    /// Center position of formation
    pub center_position: Position,
    /// Number of active platforms
    pub platform_count: u32,
    /// Number of cells in formation
    pub cell_count: u32,
    /// Aggregated capabilities
    pub capabilities: Vec<AggregatedCapability>,
    /// Overall formation readiness (0.0 - 1.0)
    pub readiness: f64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Aggregated capability across multiple platforms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedCapability {
    /// Capability type
    pub capability_type: String,
    /// Number of platforms with this capability
    pub count: u32,
    /// Average precision across platforms
    pub avg_precision: f64,
    /// Percentage of platforms that are active
    pub availability: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_creation() {
        let pos = Position::new(33.7749, -84.3958);
        assert_eq!(pos.lat, 33.7749);
        assert_eq!(pos.lon, -84.3958);
        assert!(pos.cep_m.is_none());
        assert!(pos.hae.is_none());
    }

    #[test]
    fn test_position_with_accuracy() {
        let pos = Position::with_accuracy(33.7749, -84.3958, 2.5);
        assert_eq!(pos.cep_m, Some(2.5));
    }

    #[test]
    fn test_position_with_altitude() {
        let pos = Position::with_altitude(33.7749, -84.3958, 100.0, Some(2.5));
        assert_eq!(pos.hae, Some(100.0));
        assert_eq!(pos.cep_m, Some(2.5));
    }

    #[test]
    fn test_velocity_stationary() {
        let moving = Velocity::new(45.0, 5.0);
        assert!(!moving.is_stationary(0.5));

        let stationary = Velocity::new(0.0, 0.1);
        assert!(stationary.is_stationary(0.5));
    }

    #[test]
    fn test_track_update_creation() {
        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "person".to_string(),
            0.89,
            Position::new(33.7749, -84.3958),
            "Alpha-2".to_string(),
            "object_tracker".to_string(),
            "1.3.0".to_string(),
        );

        assert_eq!(track.track_id, "TRACK-001");
        assert_eq!(track.confidence, 0.89);
    }

    #[test]
    fn test_track_update_confidence_clamped() {
        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "person".to_string(),
            1.5, // Should be clamped to 1.0
            Position::new(0.0, 0.0),
            "platform".to_string(),
            "model".to_string(),
            "1.0".to_string(),
        );

        assert_eq!(track.confidence, 1.0);
    }

    #[test]
    fn test_track_update_with_attributes() {
        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "person".to_string(),
            0.89,
            Position::new(0.0, 0.0),
            "platform".to_string(),
            "model".to_string(),
            "1.0".to_string(),
        )
        .with_attribute("jacket_color", serde_json::json!("blue"))
        .with_attribute("has_backpack", serde_json::json!(true));

        assert_eq!(track.attributes.len(), 2);
        assert_eq!(track.attributes["jacket_color"], "blue");
    }

    #[test]
    fn test_capability_advertisement() {
        let cap = CapabilityAdvertisement::new(
            "Alpha-3".to_string(),
            "UGV".to_string(),
            Position::new(33.7749, -84.3958),
            OperationalStatus::Active,
            0.91,
        )
        .with_capability(CapabilityInfo {
            capability_type: "OBJECT_TRACKING".to_string(),
            model_name: "object_tracker".to_string(),
            version: "1.3.0".to_string(),
            precision: 0.94,
            status: OperationalStatus::Active,
        });

        assert_eq!(cap.capabilities.len(), 1);
        assert_eq!(cap.status.as_str(), "ACTIVE");
    }

    #[test]
    fn test_handoff_message() {
        let handoff = HandoffMessage::new(
            "TRACK-001".to_string(),
            Position::new(33.78, -84.40),
            "Alpha-Team".to_string(),
            "Bravo-Team".to_string(),
            "boundary_crossing".to_string(),
        )
        .with_priority(2);

        assert_eq!(handoff.state, HandoffState::Initiated);
        assert_eq!(handoff.priority, 2);
    }

    #[test]
    fn test_handoff_priority_clamped() {
        let handoff = HandoffMessage::new(
            "TRACK-001".to_string(),
            Position::new(0.0, 0.0),
            "source".to_string(),
            "target".to_string(),
            "test".to_string(),
        )
        .with_priority(10); // Should be clamped to 5

        assert_eq!(handoff.priority, 5);
    }

    #[test]
    fn test_operational_status_strings() {
        assert_eq!(OperationalStatus::Ready.as_str(), "READY");
        assert_eq!(OperationalStatus::Active.as_str(), "ACTIVE");
        assert_eq!(OperationalStatus::Degraded.as_str(), "DEGRADED");
        assert_eq!(OperationalStatus::Offline.as_str(), "OFFLINE");
        assert_eq!(OperationalStatus::Loading.as_str(), "LOADING");
    }

    #[test]
    fn test_handoff_state_strings() {
        assert_eq!(HandoffState::Initiated.as_str(), "INITIATED");
        assert_eq!(HandoffState::Accepted.as_str(), "ACCEPTED");
        assert_eq!(HandoffState::Transferred.as_str(), "TRANSFERRED");
        assert_eq!(HandoffState::Completed.as_str(), "COMPLETED");
        assert_eq!(HandoffState::Failed.as_str(), "FAILED");
    }
}
