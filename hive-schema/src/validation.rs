//! Schema validation utilities
//!
//! This module provides validation functions for HIVE Protocol messages to ensure:
//! - Confidence scores are within valid range (0.0 - 1.0)
//! - Required fields are present
//! - Semantic constraints are satisfied
//! - CRDT invariants are maintained

use crate::capability::v1::{
    Capability, CapabilityAdvertisement, OperationalStatus, ResourceStatus,
};
use crate::cell::v1::{CellConfig, CellState};
use crate::command::v1::HierarchicalCommand;
use crate::node::v1::{NodeConfig, NodeState};
use crate::track::v1::{Track, TrackPosition, TrackUpdate, UpdateType};

/// Validation error
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Invalid confidence score: {0} (must be between 0.0 and 1.0)")]
    InvalidConfidence(f32),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field value: {0}")]
    InvalidValue(String),

    #[error("Semantic constraint violated: {0}")]
    ConstraintViolation(String),
}

pub type ValidationResult<T> = Result<T, ValidationError>;

/// Validate a capability message
pub fn validate_capability(cap: &Capability) -> ValidationResult<()> {
    // Check confidence is in valid range
    if cap.confidence < 0.0 || cap.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(cap.confidence));
    }

    // Check required fields
    if cap.id.is_empty() {
        return Err(ValidationError::MissingField("id".to_string()));
    }

    if cap.name.is_empty() {
        return Err(ValidationError::MissingField("name".to_string()));
    }

    Ok(())
}

/// Validate a node configuration
pub fn validate_node_config(config: &NodeConfig) -> ValidationResult<()> {
    // Check required fields
    if config.id.is_empty() {
        return Err(ValidationError::MissingField("id".to_string()));
    }

    if config.platform_type.is_empty() {
        return Err(ValidationError::MissingField("platform_type".to_string()));
    }

    // Validate all capabilities
    for cap in &config.capabilities {
        validate_capability(cap)?;
    }

    // Check communication range is positive
    if config.comm_range_m <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "comm_range_m must be positive".to_string(),
        ));
    }

    // Check max speed is positive
    if config.max_speed_mps <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "max_speed_mps must be positive".to_string(),
        ));
    }

    Ok(())
}

/// Validate a node state
pub fn validate_node_state(state: &NodeState) -> ValidationResult<()> {
    // Check position has valid coordinates
    if let Some(pos) = &state.position {
        if pos.latitude < -90.0 || pos.latitude > 90.0 {
            return Err(ValidationError::InvalidValue(
                "latitude must be between -90 and 90".to_string(),
            ));
        }
        if pos.longitude < -180.0 || pos.longitude > 180.0 {
            return Err(ValidationError::InvalidValue(
                "longitude must be between -180 and 180".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate a cell configuration
pub fn validate_cell_config(config: &CellConfig) -> ValidationResult<()> {
    // Check required fields
    if config.id.is_empty() {
        return Err(ValidationError::MissingField("id".to_string()));
    }

    // Check max_size > min_size
    if config.max_size < config.min_size {
        return Err(ValidationError::ConstraintViolation(
            "max_size must be >= min_size".to_string(),
        ));
    }

    // Check minimum size is at least 2
    if config.min_size < 2 {
        return Err(ValidationError::ConstraintViolation(
            "min_size must be at least 2".to_string(),
        ));
    }

    Ok(())
}

/// Validate a cell state
pub fn validate_cell_state(state: &CellState) -> ValidationResult<()> {
    // Validate config
    if let Some(config) = &state.config {
        validate_cell_config(config)?;

        // Check member count constraints
        let member_count = state.members.len();
        if member_count > config.max_size as usize {
            return Err(ValidationError::ConstraintViolation(format!(
                "member count ({}) exceeds max_size ({})",
                member_count, config.max_size
            )));
        }
    }

    // Validate all capabilities
    for cap in &state.capabilities {
        validate_capability(cap)?;
    }

    // If leader_id is set, it must be in members list
    if let Some(leader_id) = &state.leader_id {
        if !state.members.contains(leader_id) {
            return Err(ValidationError::ConstraintViolation(
                "leader_id must be in members list".to_string(),
            ));
        }
    }

    Ok(())
}

// ============================================================================
// HIVE Protocol Message Validators (Issue #288)
// ============================================================================

/// Validate a CapabilityAdvertisement message
///
/// Validates:
/// - platform_id is present
/// - advertised_at timestamp is present
/// - All capabilities pass validation
/// - Resource status values are in valid range (0.0 - 1.0)
/// - Operational status is valid (not unspecified)
pub fn validate_capability_advertisement(ad: &CapabilityAdvertisement) -> ValidationResult<()> {
    // Check required fields
    if ad.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    if ad.advertised_at.is_none() {
        return Err(ValidationError::MissingField("advertised_at".to_string()));
    }

    // Validate all capabilities
    for cap in &ad.capabilities {
        validate_capability(cap)?;
    }

    // Validate resource status if present
    if let Some(resources) = &ad.resources {
        validate_resource_status(resources)?;
    }

    // Check operational status is specified
    if ad.operational_status == OperationalStatus::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "operational_status must be specified".to_string(),
        ));
    }

    Ok(())
}

/// Validate resource status values
fn validate_resource_status(resources: &ResourceStatus) -> ValidationResult<()> {
    // All utilization values should be 0.0 - 1.0
    if resources.compute_utilization < 0.0 || resources.compute_utilization > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "compute_utilization {} must be between 0.0 and 1.0",
            resources.compute_utilization
        )));
    }

    if resources.memory_utilization < 0.0 || resources.memory_utilization > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "memory_utilization {} must be between 0.0 and 1.0",
            resources.memory_utilization
        )));
    }

    if resources.power_level < 0.0 || resources.power_level > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "power_level {} must be between 0.0 and 1.0",
            resources.power_level
        )));
    }

    if resources.storage_utilization < 0.0 || resources.storage_utilization > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "storage_utilization {} must be between 0.0 and 1.0",
            resources.storage_utilization
        )));
    }

    if resources.bandwidth_utilization < 0.0 || resources.bandwidth_utilization > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "bandwidth_utilization {} must be between 0.0 and 1.0",
            resources.bandwidth_utilization
        )));
    }

    Ok(())
}

/// Validate a TrackUpdate message
///
/// Validates:
/// - update_type is specified (not unspecified)
/// - track is present and valid
/// - timestamp is present
pub fn validate_track_update(update: &TrackUpdate) -> ValidationResult<()> {
    // Check update_type is specified
    if update.update_type == UpdateType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "update_type must be specified".to_string(),
        ));
    }

    // Track is required
    let track = update
        .track
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("track".to_string()))?;

    validate_track(track)?;

    // Timestamp is required
    if update.timestamp.is_none() {
        return Err(ValidationError::MissingField("timestamp".to_string()));
    }

    // For merge operations, previous_track_id is required
    if update.update_type == UpdateType::Merge as i32 && update.previous_track_id.is_empty() {
        return Err(ValidationError::MissingField(
            "previous_track_id (required for MERGE updates)".to_string(),
        ));
    }

    Ok(())
}

/// Validate a Track message
///
/// Validates:
/// - track_id is present
/// - confidence is in valid range (0.0 - 1.0)
/// - position is present and valid
/// - source is present
pub fn validate_track(track: &Track) -> ValidationResult<()> {
    // Check required fields
    if track.track_id.is_empty() {
        return Err(ValidationError::MissingField("track_id".to_string()));
    }

    // Confidence must be in valid range
    if track.confidence < 0.0 || track.confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(track.confidence));
    }

    // Position is required
    let position = track
        .position
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("position".to_string()))?;

    validate_track_position(position)?;

    // Source is required
    let source = track
        .source
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("source".to_string()))?;

    if source.platform_id.is_empty() {
        return Err(ValidationError::MissingField(
            "source.platform_id".to_string(),
        ));
    }

    Ok(())
}

/// Validate a TrackPosition
fn validate_track_position(pos: &TrackPosition) -> ValidationResult<()> {
    // Latitude must be -90 to 90
    if pos.latitude < -90.0 || pos.latitude > 90.0 {
        return Err(ValidationError::InvalidValue(format!(
            "latitude {} must be between -90 and 90",
            pos.latitude
        )));
    }

    // Longitude must be -180 to 180
    if pos.longitude < -180.0 || pos.longitude > 180.0 {
        return Err(ValidationError::InvalidValue(format!(
            "longitude {} must be between -180 and 180",
            pos.longitude
        )));
    }

    // CEP must be non-negative
    if pos.cep_m < 0.0 {
        return Err(ValidationError::InvalidValue(format!(
            "cep_m {} must be non-negative",
            pos.cep_m
        )));
    }

    Ok(())
}

/// Validate a HierarchicalCommand (MissionTask)
///
/// Validates:
/// - command_id is present
/// - originator_id is present
/// - target is present and valid
/// - issued_at timestamp is present
/// - If expires_at is set, it must be after issued_at
pub fn validate_hierarchical_command(cmd: &HierarchicalCommand) -> ValidationResult<()> {
    // Check required fields
    if cmd.command_id.is_empty() {
        return Err(ValidationError::MissingField("command_id".to_string()));
    }

    if cmd.originator_id.is_empty() {
        return Err(ValidationError::MissingField("originator_id".to_string()));
    }

    // Target is required
    let target = cmd
        .target
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("target".to_string()))?;

    // Target must have at least one target_id (unless broadcast)
    if target.target_ids.is_empty() && target.scope != 4 {
        // 4 = BROADCAST
        return Err(ValidationError::MissingField(
            "target.target_ids (required for non-broadcast commands)".to_string(),
        ));
    }

    // issued_at is required
    if cmd.issued_at.is_none() {
        return Err(ValidationError::MissingField("issued_at".to_string()));
    }

    // If expires_at is set, validate it comes after issued_at
    if let (Some(issued), Some(expires)) = (&cmd.issued_at, &cmd.expires_at) {
        if expires.seconds < issued.seconds
            || (expires.seconds == issued.seconds && expires.nanos < issued.nanos)
        {
            return Err(ValidationError::ConstraintViolation(
                "expires_at must be after issued_at".to_string(),
            ));
        }
    }

    // Command type must be specified
    if cmd.command_type.is_none() {
        return Err(ValidationError::MissingField("command_type".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::v1::CapabilityType;

    #[test]
    fn test_validate_capability_success() {
        let cap = Capability {
            id: "cap-1".to_string(),
            name: "Camera".to_string(),
            capability_type: CapabilityType::Sensor as i32,
            confidence: 0.9,
            metadata_json: String::new(),
            registered_at: None,
        };

        assert!(validate_capability(&cap).is_ok());
    }

    #[test]
    fn test_validate_capability_invalid_confidence() {
        let cap = Capability {
            id: "cap-1".to_string(),
            name: "Camera".to_string(),
            capability_type: CapabilityType::Sensor as i32,
            confidence: 1.5, // Invalid
            metadata_json: String::new(),
            registered_at: None,
        };

        assert!(validate_capability(&cap).is_err());
    }

    #[test]
    fn test_validate_capability_missing_id() {
        let cap = Capability {
            id: String::new(), // Missing
            name: "Camera".to_string(),
            capability_type: CapabilityType::Sensor as i32,
            confidence: 0.9,
            metadata_json: String::new(),
            registered_at: None,
        };

        assert!(validate_capability(&cap).is_err());
    }

    #[test]
    fn test_validate_cell_config_invalid_sizes() {
        let config = CellConfig {
            id: "cell-1".to_string(),
            max_size: 2,
            min_size: 5, // Invalid: min > max
            created_at: None,
        };

        assert!(validate_cell_config(&config).is_err());
    }

    // =========================================================================
    // HIVE Protocol Message Validator Tests (Issue #288)
    // =========================================================================

    mod capability_advertisement_tests {
        use super::*;
        use crate::common::v1::Timestamp;

        fn valid_capability_advertisement() -> CapabilityAdvertisement {
            CapabilityAdvertisement {
                platform_id: "Alpha-3".to_string(),
                advertised_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                capabilities: vec![Capability {
                    id: "ai-model-1".to_string(),
                    name: "Object Detector".to_string(),
                    capability_type: CapabilityType::Compute as i32,
                    confidence: 0.95,
                    metadata_json: r#"{"model_type": "detector"}"#.to_string(),
                    registered_at: None,
                }],
                resources: Some(ResourceStatus {
                    compute_utilization: 0.65,
                    memory_utilization: 0.5,
                    power_level: 0.9,
                    storage_utilization: 0.3,
                    bandwidth_utilization: 0.1,
                    extra_json: String::new(),
                }),
                operational_status: OperationalStatus::Ready as i32,
            }
        }

        #[test]
        fn test_valid_capability_advertisement() {
            let ad = valid_capability_advertisement();
            assert!(validate_capability_advertisement(&ad).is_ok());
        }

        #[test]
        fn test_missing_platform_id() {
            let mut ad = valid_capability_advertisement();
            ad.platform_id = String::new();
            let err = validate_capability_advertisement(&ad).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "platform_id"));
        }

        #[test]
        fn test_missing_advertised_at() {
            let mut ad = valid_capability_advertisement();
            ad.advertised_at = None;
            let err = validate_capability_advertisement(&ad).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "advertised_at"));
        }

        #[test]
        fn test_invalid_resource_utilization() {
            let mut ad = valid_capability_advertisement();
            ad.resources = Some(ResourceStatus {
                compute_utilization: 1.5, // Invalid
                memory_utilization: 0.5,
                power_level: 0.9,
                storage_utilization: 0.3,
                bandwidth_utilization: 0.1,
                extra_json: String::new(),
            });
            let err = validate_capability_advertisement(&ad).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_unspecified_operational_status() {
            let mut ad = valid_capability_advertisement();
            ad.operational_status = OperationalStatus::Unspecified as i32;
            let err = validate_capability_advertisement(&ad).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }
    }

    mod track_update_tests {
        use super::*;
        use crate::common::v1::Timestamp;
        use crate::track::v1::{SourceType, TrackSource, TrackState};

        fn valid_track() -> Track {
            Track {
                track_id: "TRK-001".to_string(),
                classification: "person".to_string(),
                confidence: 0.92,
                position: Some(TrackPosition {
                    latitude: 38.8977,
                    longitude: -77.0365,
                    altitude: 10.0,
                    cep_m: 5.0,
                    vertical_error_m: 2.0,
                }),
                velocity: None,
                state: TrackState::Confirmed as i32,
                source: Some(TrackSource {
                    platform_id: "Alpha-3".to_string(),
                    sensor_id: "camera-1".to_string(),
                    model_version: "1.2.0".to_string(),
                    source_type: SourceType::AiModel as i32,
                }),
                attributes_json: r#"{"color": "red"}"#.to_string(),
                first_seen: None,
                last_seen: None,
                observation_count: 5,
            }
        }

        fn valid_track_update() -> TrackUpdate {
            TrackUpdate {
                update_type: UpdateType::New as i32,
                track: Some(valid_track()),
                previous_track_id: String::new(),
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            }
        }

        #[test]
        fn test_valid_track_update() {
            let update = valid_track_update();
            assert!(validate_track_update(&update).is_ok());
        }

        #[test]
        fn test_missing_track() {
            let mut update = valid_track_update();
            update.track = None;
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "track"));
        }

        #[test]
        fn test_missing_timestamp() {
            let mut update = valid_track_update();
            update.timestamp = None;
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "timestamp"));
        }

        #[test]
        fn test_unspecified_update_type() {
            let mut update = valid_track_update();
            update.update_type = UpdateType::Unspecified as i32;
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_merge_without_previous_track_id() {
            let mut update = valid_track_update();
            update.update_type = UpdateType::Merge as i32;
            update.previous_track_id = String::new();
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(_)));
        }

        #[test]
        fn test_invalid_latitude() {
            let mut update = valid_track_update();
            if let Some(ref mut track) = update.track {
                if let Some(ref mut pos) = track.position {
                    pos.latitude = 100.0; // Invalid
                }
            }
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_confidence() {
            let mut update = valid_track_update();
            if let Some(ref mut track) = update.track {
                track.confidence = -0.5; // Invalid
            }
            let err = validate_track_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidConfidence(_)));
        }
    }

    mod hierarchical_command_tests {
        use super::*;
        use crate::command::v1::{
            command_target::Scope, mission_order::MissionType, CommandTarget, MissionOrder,
        };
        use crate::common::v1::Timestamp;

        fn valid_command() -> HierarchicalCommand {
            HierarchicalCommand {
                command_id: "CMD-001".to_string(),
                originator_id: "HQ-1".to_string(),
                target: Some(CommandTarget {
                    scope: Scope::Squad as i32,
                    target_ids: vec!["Alpha".to_string()],
                }),
                command_type: Some(
                    crate::command::v1::hierarchical_command::CommandType::MissionOrder(
                        MissionOrder {
                            mission_type: MissionType::Isr as i32,
                            mission_id: "ISR-001".to_string(),
                            description: "Conduct ISR in sector Alpha".to_string(),
                            objective_location: None,
                            start_time: None,
                            end_time: None,
                            roe: None,
                        },
                    ),
                ),
                priority: 1,
                buffer_policy: 1,
                conflict_policy: 1,
                acknowledgment_policy: 2,
                leader_change_policy: 1,
                issued_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                expires_at: None,
                version: 1,
            }
        }

        #[test]
        fn test_valid_command() {
            let cmd = valid_command();
            assert!(validate_hierarchical_command(&cmd).is_ok());
        }

        #[test]
        fn test_missing_command_id() {
            let mut cmd = valid_command();
            cmd.command_id = String::new();
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "command_id"));
        }

        #[test]
        fn test_missing_originator_id() {
            let mut cmd = valid_command();
            cmd.originator_id = String::new();
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "originator_id"));
        }

        #[test]
        fn test_missing_target() {
            let mut cmd = valid_command();
            cmd.target = None;
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "target"));
        }

        #[test]
        fn test_missing_issued_at() {
            let mut cmd = valid_command();
            cmd.issued_at = None;
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "issued_at"));
        }

        #[test]
        fn test_expires_before_issued() {
            let mut cmd = valid_command();
            cmd.issued_at = Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            });
            cmd.expires_at = Some(Timestamp {
                seconds: 1701000000, // Before issued
                nanos: 0,
            });
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_missing_command_type() {
            let mut cmd = valid_command();
            cmd.command_type = None;
            let err = validate_hierarchical_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "command_type"));
        }
    }
}
