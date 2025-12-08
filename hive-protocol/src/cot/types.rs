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

// =============================================================================
// Mission Task Types (Issue #318: TAK → HIVE direction)
// =============================================================================

/// Mission task type enumeration
///
/// Maps to CoT mission types as defined in CONTRACT_CORE_ATAK_TAK_BRIDGE.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionTaskType {
    /// Track a specific target (CoT: t-x-m-c-c → TRACK_TARGET)
    TrackTarget,
    /// Search an area for targets (CoT: t-x-m-c-s → SEARCH_AREA)
    SearchArea,
    /// Monitor a zone continuously
    MonitorZone,
    /// Abort current mission (CoT: t-x-m-c-a)
    Abort,
}

impl MissionTaskType {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TrackTarget => "TRACK_TARGET",
            Self::SearchArea => "SEARCH_AREA",
            Self::MonitorZone => "MONITOR_ZONE",
            Self::Abort => "ABORT",
        }
    }

    /// Parse from CoT type string
    pub fn from_cot_type(cot_type: &str) -> Option<Self> {
        match cot_type {
            "t-x-m-c-c" => Some(Self::TrackTarget),
            "t-x-m-c-s" => Some(Self::SearchArea),
            "t-x-m-c-a" => Some(Self::Abort),
            _ if cot_type.starts_with("t-x-m-c") => Some(Self::TrackTarget), // Default mission
            _ => None,
        }
    }
}

/// Mission priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionPriority {
    Critical,
    High,
    Normal,
    Low,
}

impl MissionPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Normal => "NORMAL",
            Self::Low => "LOW",
        }
    }
}

impl Default for MissionPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Target information for a mission
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionTarget {
    /// Description of the target
    pub description: String,
    /// Last known position
    pub last_known_position: Option<Position>,
}

/// Boundary definition for area missions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionBoundary {
    /// Boundary type
    pub boundary_type: BoundaryType,
    /// Polygon coordinates (for polygon type)
    pub coordinates: Vec<Position>,
    /// Radius in meters (for circle type)
    pub radius_m: Option<f64>,
}

/// Type of boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryType {
    Polygon,
    Circle,
}

/// Mission task from C2/TAK Server (Issue #318)
///
/// Represents a mission tasking command received from TAK Server.
/// This is the HIVE representation of CoT mission task events.
///
/// CoT Event Types handled:
/// - `t-x-m-c-c`: Track target command → TrackTarget
/// - `t-x-m-c-s`: Search area command → SearchArea
/// - `t-x-m-c-a`: Abort command → Abort
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionTask {
    /// Unique task identifier (from CoT event@uid)
    pub task_id: String,
    /// Mission type
    pub task_type: MissionTaskType,
    /// When the task was issued (from CoT event@time)
    pub issued_at: DateTime<Utc>,
    /// Who issued the task (CoT source UID or callsign)
    pub issued_by: String,
    /// When the task expires (from CoT event@stale)
    pub expires_at: DateTime<Utc>,
    /// Target information (for TRACK_TARGET missions)
    pub target: Option<MissionTarget>,
    /// Boundary/area (for SEARCH_AREA, MONITOR_ZONE missions)
    pub boundary: Option<MissionBoundary>,
    /// Task priority
    pub priority: MissionPriority,
    /// Objective location (from CoT point)
    pub objective_position: Option<Position>,
    /// Raw remarks from CoT (for additional context)
    pub remarks: Option<String>,
}

impl MissionTask {
    /// Create a new mission task
    pub fn new(
        task_id: String,
        task_type: MissionTaskType,
        issued_by: String,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            task_id,
            task_type,
            issued_at: Utc::now(),
            issued_by,
            expires_at,
            target: None,
            boundary: None,
            priority: MissionPriority::Normal,
            objective_position: None,
            remarks: None,
        }
    }

    /// Create from a CoT event (Issue #318)
    ///
    /// Converts a mission-type CoT event to HIVE MissionTask format.
    pub fn from_cot_event(event: &super::CotEvent) -> Result<Self, MissionTaskError> {
        let task_type = MissionTaskType::from_cot_type(event.cot_type.as_str()).ok_or(
            MissionTaskError::InvalidCotType(event.cot_type.as_str().to_string()),
        )?;

        let mut task = Self {
            task_id: event.uid.clone(),
            task_type,
            issued_at: event.time,
            issued_by: "TAK-Server".to_string(), // Could extract from CoT contact
            expires_at: event.stale,
            target: None,
            boundary: None,
            priority: MissionPriority::Normal,
            objective_position: Some(Position::with_altitude(
                event.point.lat,
                event.point.lon,
                event.point.hae,
                Some(event.point.ce),
            )),
            remarks: event.detail.remarks.clone(),
        };

        // Extract target description from remarks if present
        if let Some(ref remarks) = event.detail.remarks {
            task.target = Some(MissionTarget {
                description: remarks.clone(),
                last_known_position: task.objective_position.clone(),
            });
        }

        Ok(task)
    }

    /// Set target information
    pub fn with_target(mut self, target: MissionTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Set boundary
    pub fn with_boundary(mut self, boundary: MissionBoundary) -> Self {
        self.boundary = Some(boundary);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: MissionPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set objective position
    pub fn with_objective_position(mut self, position: Position) -> Self {
        self.objective_position = Some(position);
        self
    }

    /// Check if the mission is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if this is a mission-type CoT event
    pub fn is_mission_cot_type(cot_type: &str) -> bool {
        cot_type.starts_with("t-x-m-c")
    }

    /// Serialize to JSON for Automerge storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Errors that can occur when creating a MissionTask
#[derive(Debug, Clone, PartialEq)]
pub enum MissionTaskError {
    /// CoT type is not a mission type
    InvalidCotType(String),
    /// Missing required field
    MissingField(&'static str),
}

impl std::fmt::Display for MissionTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCotType(t) => write!(f, "Invalid CoT type for mission task: {}", t),
            Self::MissingField(field) => write!(f, "Missing required field: {}", field),
        }
    }
}

impl std::error::Error for MissionTaskError {}

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

    // =======================================================================
    // MissionTask Tests (Issue #318)
    // =======================================================================

    #[test]
    fn test_mission_task_type_from_cot() {
        assert_eq!(
            MissionTaskType::from_cot_type("t-x-m-c-c"),
            Some(MissionTaskType::TrackTarget)
        );
        assert_eq!(
            MissionTaskType::from_cot_type("t-x-m-c-s"),
            Some(MissionTaskType::SearchArea)
        );
        assert_eq!(
            MissionTaskType::from_cot_type("t-x-m-c-a"),
            Some(MissionTaskType::Abort)
        );
        // Generic mission defaults to TrackTarget
        assert_eq!(
            MissionTaskType::from_cot_type("t-x-m-c-z"),
            Some(MissionTaskType::TrackTarget)
        );
        // Non-mission types return None
        assert_eq!(MissionTaskType::from_cot_type("a-f-G-U-C"), None);
    }

    #[test]
    fn test_mission_task_type_as_str() {
        assert_eq!(MissionTaskType::TrackTarget.as_str(), "TRACK_TARGET");
        assert_eq!(MissionTaskType::SearchArea.as_str(), "SEARCH_AREA");
        assert_eq!(MissionTaskType::MonitorZone.as_str(), "MONITOR_ZONE");
        assert_eq!(MissionTaskType::Abort.as_str(), "ABORT");
    }

    #[test]
    fn test_mission_priority_default() {
        let priority = MissionPriority::default();
        assert_eq!(priority, MissionPriority::Normal);
    }

    #[test]
    fn test_mission_priority_as_str() {
        assert_eq!(MissionPriority::Critical.as_str(), "CRITICAL");
        assert_eq!(MissionPriority::High.as_str(), "HIGH");
        assert_eq!(MissionPriority::Normal.as_str(), "NORMAL");
        assert_eq!(MissionPriority::Low.as_str(), "LOW");
    }

    #[test]
    fn test_mission_task_new() {
        let expires = Utc::now() + chrono::Duration::hours(2);
        let task = MissionTask::new(
            "MISSION-001".to_string(),
            MissionTaskType::TrackTarget,
            "CMD-ALPHA".to_string(),
            expires,
        );

        assert_eq!(task.task_id, "MISSION-001");
        assert_eq!(task.task_type, MissionTaskType::TrackTarget);
        assert_eq!(task.issued_by, "CMD-ALPHA");
        assert_eq!(task.priority, MissionPriority::Normal);
        assert!(task.target.is_none());
        assert!(task.boundary.is_none());
    }

    #[test]
    fn test_mission_task_with_builder_methods() {
        let expires = Utc::now() + chrono::Duration::hours(1);
        let task = MissionTask::new(
            "MISSION-002".to_string(),
            MissionTaskType::SearchArea,
            "CMD-BRAVO".to_string(),
            expires,
        )
        .with_priority(MissionPriority::High)
        .with_objective_position(Position::new(33.7749, -84.3958))
        .with_target(MissionTarget {
            description: "Suspicious vehicle".to_string(),
            last_known_position: Some(Position::new(33.77, -84.39)),
        });

        assert_eq!(task.priority, MissionPriority::High);
        assert!(task.objective_position.is_some());
        assert!(task.target.is_some());
        assert_eq!(
            task.target.as_ref().unwrap().description,
            "Suspicious vehicle"
        );
    }

    #[test]
    fn test_mission_task_is_expired() {
        let past_expires = Utc::now() - chrono::Duration::hours(1);
        let task = MissionTask::new(
            "EXPIRED-001".to_string(),
            MissionTaskType::TrackTarget,
            "CMD".to_string(),
            past_expires,
        );
        assert!(task.is_expired());

        let future_expires = Utc::now() + chrono::Duration::hours(1);
        let active_task = MissionTask::new(
            "ACTIVE-001".to_string(),
            MissionTaskType::TrackTarget,
            "CMD".to_string(),
            future_expires,
        );
        assert!(!active_task.is_expired());
    }

    #[test]
    fn test_mission_task_is_mission_cot_type() {
        assert!(MissionTask::is_mission_cot_type("t-x-m-c-c"));
        assert!(MissionTask::is_mission_cot_type("t-x-m-c-s"));
        assert!(MissionTask::is_mission_cot_type("t-x-m-c-a"));
        assert!(!MissionTask::is_mission_cot_type("a-f-G-U-C"));
        assert!(!MissionTask::is_mission_cot_type("b-m-p-s-p-l"));
    }

    #[test]
    fn test_mission_task_json_roundtrip() {
        let expires = Utc::now() + chrono::Duration::hours(2);
        let task = MissionTask::new(
            "JSON-001".to_string(),
            MissionTaskType::SearchArea,
            "CMD".to_string(),
            expires,
        )
        .with_priority(MissionPriority::Critical)
        .with_objective_position(Position::new(33.7749, -84.3958));

        let json = task.to_json().expect("should serialize");
        let restored = MissionTask::from_json(&json).expect("should deserialize");

        assert_eq!(restored.task_id, task.task_id);
        assert_eq!(restored.task_type, task.task_type);
        assert_eq!(restored.priority, task.priority);
    }

    #[test]
    fn test_mission_task_error_display() {
        let err = MissionTaskError::InvalidCotType("a-f-G-U-C".to_string());
        assert!(err.to_string().contains("Invalid CoT type"));

        let err = MissionTaskError::MissingField("target");
        assert!(err.to_string().contains("Missing required field"));
    }

    #[test]
    fn test_mission_task_from_cot_event() {
        use crate::cot::{CotEvent, CotPoint, CotType};

        // Build a mission CoT event
        let event = CotEvent {
            version: "2.0".to_string(),
            uid: "MISSION-TAK-001".to_string(),
            cot_type: CotType::new("t-x-m-c-c"), // Track target
            time: Utc::now(),
            start: Utc::now(),
            stale: Utc::now() + chrono::Duration::hours(2),
            how: "m-g".to_string(),
            point: CotPoint {
                lat: 33.7749,
                lon: -84.3958,
                hae: 300.0,
                ce: 10.0,
                le: 10.0,
            },
            detail: crate::cot::CotDetail {
                contact_callsign: Some("CMD-001".to_string()),
                remarks: Some("Track suspicious vehicle in sector alpha".to_string()),
                ..Default::default()
            },
        };

        let task = MissionTask::from_cot_event(&event).expect("should convert");

        assert_eq!(task.task_id, "MISSION-TAK-001");
        assert_eq!(task.task_type, MissionTaskType::TrackTarget);
        assert!(task.target.is_some());
        assert_eq!(
            task.target.as_ref().unwrap().description,
            "Track suspicious vehicle in sector alpha"
        );
        assert!(task.objective_position.is_some());
        let pos = task.objective_position.as_ref().unwrap();
        assert_eq!(pos.lat, 33.7749);
        assert_eq!(pos.lon, -84.3958);
    }

    #[test]
    fn test_mission_task_from_cot_event_invalid_type() {
        use crate::cot::{CotEvent, CotPoint, CotType};

        // Build a non-mission CoT event
        let event = CotEvent {
            version: "2.0".to_string(),
            uid: "UNIT-001".to_string(),
            cot_type: CotType::new("a-f-G-U-C"), // Friendly ground unit, not a mission
            time: Utc::now(),
            start: Utc::now(),
            stale: Utc::now() + chrono::Duration::hours(1),
            how: "m-g".to_string(),
            point: CotPoint::new(0.0, 0.0),
            detail: Default::default(),
        };

        let result = MissionTask::from_cot_event(&event);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MissionTaskError::InvalidCotType(_)
        ));
    }

    /// End-to-end test: XML → CotEvent → MissionTask (Issue #318)
    #[test]
    fn test_mission_task_from_xml_end_to_end() {
        use crate::cot::CotEvent;

        // Realistic mission task CoT XML from TAK Server
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="TASK-20251208-001" type="t-x-m-c-c" time="2025-12-08T14:05:00Z"
                   start="2025-12-08T14:05:00Z" stale="2025-12-08T16:05:00Z" how="h-g-i-g-o">
                <point lat="33.7756" lon="-84.3963" hae="300" ce="50" le="50"/>
                <detail>
                    <contact callsign="CMD-ALPHA"/>
                    <remarks>Track suspicious vehicle in sector bravo, heading north on Main St</remarks>
                </detail>
            </event>"#;

        // Parse XML to CotEvent
        let event = CotEvent::from_xml(xml).expect("should parse XML");
        assert_eq!(event.uid, "TASK-20251208-001");
        assert_eq!(event.cot_type.as_str(), "t-x-m-c-c");

        // Convert to MissionTask
        let task = MissionTask::from_cot_event(&event).expect("should convert to MissionTask");

        // Verify conversion
        assert_eq!(task.task_id, "TASK-20251208-001");
        assert_eq!(task.task_type, MissionTaskType::TrackTarget);
        assert_eq!(task.issued_by, "TAK-Server");
        assert!(task.target.is_some());
        assert!(task
            .target
            .as_ref()
            .unwrap()
            .description
            .contains("suspicious vehicle"));
        assert!(task.objective_position.is_some());
        let pos = task.objective_position.as_ref().unwrap();
        assert!((pos.lat - 33.7756).abs() < 0.0001);
        assert!((pos.lon - (-84.3963)).abs() < 0.0001);
    }

    /// Test search area mission type
    #[test]
    fn test_mission_task_search_area_from_xml() {
        use crate::cot::CotEvent;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="SEARCH-001" type="t-x-m-c-s" time="2025-12-08T14:00:00Z"
                   start="2025-12-08T14:00:00Z" stale="2025-12-08T18:00:00Z" how="h-g-i-g-o">
                <point lat="33.80" lon="-84.40" hae="0" ce="500" le="500"/>
                <detail>
                    <remarks>Search grid sector 7 for missing hiker</remarks>
                </detail>
            </event>"#;

        let event = CotEvent::from_xml(xml).expect("should parse");
        let task = MissionTask::from_cot_event(&event).expect("should convert");

        assert_eq!(task.task_type, MissionTaskType::SearchArea);
        assert_eq!(task.task_id, "SEARCH-001");
    }

    /// Test abort mission type
    #[test]
    fn test_mission_task_abort_from_xml() {
        use crate::cot::CotEvent;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="ABORT-001" type="t-x-m-c-a" time="2025-12-08T14:00:00Z"
                   start="2025-12-08T14:00:00Z" stale="2025-12-08T14:30:00Z" how="h-g-i-g-o">
                <point lat="0" lon="0" hae="0" ce="999999" le="999999"/>
                <detail>
                    <remarks>Abort current mission - RTB immediately</remarks>
                </detail>
            </event>"#;

        let event = CotEvent::from_xml(xml).expect("should parse");
        let task = MissionTask::from_cot_event(&event).expect("should convert");

        assert_eq!(task.task_type, MissionTaskType::Abort);
        assert!(task.remarks.as_ref().unwrap().contains("RTB"));
    }
}
