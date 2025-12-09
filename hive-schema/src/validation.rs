//! Schema validation utilities
//!
//! This module provides validation functions for HIVE Protocol messages to ensure:
//! - Confidence scores are within valid range (0.0 - 1.0)
//! - Required fields are present
//! - Semantic constraints are satisfied
//! - CRDT invariants are maintained

use crate::actuator::v1::{
    ActuatorCommand, ActuatorCommandType, ActuatorMount, ActuatorSpec, ActuatorStateUpdate,
    ActuatorStatus, ActuatorType, BarrierLimits, BarrierState, GripperLimits, GripperState,
    LinearLimits, LinearState, LockState, RotaryLimits, RotaryState, ValveLimits, ValveState,
    WinchLimits, WinchState,
};
use crate::capability::v1::{
    Capability, CapabilityAdvertisement, OperationalStatus, ResourceStatus,
};
use crate::cell::v1::{CellConfig, CellState};
use crate::command::v1::HierarchicalCommand;
use crate::model::v1::{
    DeploymentPolicy, DeploymentPriority, DeploymentState, ModelDeployment, ModelDeploymentStatus,
    ModelType,
};
use crate::node::v1::{NodeConfig, NodeState};
use crate::sensor::v1::{
    FieldOfView, GimbalLimits, GimbalState, SensorMountType, SensorOrientation, SensorSpec,
    SensorStateUpdate, SensorStatus,
};
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

// ============================================================================
// Model Deployment Validators (Issue #319)
// ============================================================================

/// Validate a ModelDeployment message
///
/// Validates:
/// - deployment_id is present
/// - model_id is present
/// - model_version is present
/// - model_type is specified (not unspecified)
/// - model_url is present and well-formed
/// - checksum_sha256 is present and valid length
/// - file_size_bytes is non-zero
/// - At least one target_platform is specified
/// - deployment_policy is specified
/// - priority is specified
/// - deployed_at timestamp is present
/// - deployed_by is present
pub fn validate_model_deployment(deployment: &ModelDeployment) -> ValidationResult<()> {
    // Check required string fields
    if deployment.deployment_id.is_empty() {
        return Err(ValidationError::MissingField("deployment_id".to_string()));
    }

    if deployment.model_id.is_empty() {
        return Err(ValidationError::MissingField("model_id".to_string()));
    }

    if deployment.model_version.is_empty() {
        return Err(ValidationError::MissingField("model_version".to_string()));
    }

    // Model type must be specified
    if deployment.model_type == ModelType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "model_type must be specified".to_string(),
        ));
    }

    // Model URL must be present
    if deployment.model_url.is_empty() {
        return Err(ValidationError::MissingField("model_url".to_string()));
    }

    // Validate URL scheme (basic check for https://, s3://, or similar)
    if !deployment.model_url.contains("://") {
        return Err(ValidationError::InvalidValue(
            "model_url must be a valid URL with scheme".to_string(),
        ));
    }

    // Checksum must be present and 64 characters (SHA256 hex)
    if deployment.checksum_sha256.is_empty() {
        return Err(ValidationError::MissingField("checksum_sha256".to_string()));
    }

    if deployment.checksum_sha256.len() != 64 {
        return Err(ValidationError::InvalidValue(format!(
            "checksum_sha256 must be 64 hex characters, got {}",
            deployment.checksum_sha256.len()
        )));
    }

    // Validate checksum is valid hex
    if !deployment
        .checksum_sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit())
    {
        return Err(ValidationError::InvalidValue(
            "checksum_sha256 must contain only hex characters".to_string(),
        ));
    }

    // File size must be non-zero
    if deployment.file_size_bytes == 0 {
        return Err(ValidationError::InvalidValue(
            "file_size_bytes must be non-zero".to_string(),
        ));
    }

    // At least one target platform is required
    if deployment.target_platforms.is_empty() {
        return Err(ValidationError::MissingField(
            "target_platforms (at least one required)".to_string(),
        ));
    }

    // Deployment policy must be specified
    if deployment.deployment_policy == DeploymentPolicy::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "deployment_policy must be specified".to_string(),
        ));
    }

    // Priority must be specified
    if deployment.priority == DeploymentPriority::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "priority must be specified".to_string(),
        ));
    }

    // deployed_at timestamp is required
    if deployment.deployed_at.is_none() {
        return Err(ValidationError::MissingField("deployed_at".to_string()));
    }

    // deployed_by is required
    if deployment.deployed_by.is_empty() {
        return Err(ValidationError::MissingField("deployed_by".to_string()));
    }

    Ok(())
}

/// Validate a ModelDeploymentStatus message
///
/// Validates:
/// - deployment_id is present
/// - platform_id is present
/// - state is specified (not unspecified)
/// - progress_percent is in valid range (0-100)
/// - updated_at timestamp is present
/// - If state is FAILED, error_message is present
/// - If state is COMPLETE or VERIFYING, downloaded_hash is present and valid
pub fn validate_model_deployment_status(status: &ModelDeploymentStatus) -> ValidationResult<()> {
    // Check required fields
    if status.deployment_id.is_empty() {
        return Err(ValidationError::MissingField("deployment_id".to_string()));
    }

    if status.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    // State must be specified
    if status.state == DeploymentState::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "state must be specified".to_string(),
        ));
    }

    // Progress must be 0-100
    if status.progress_percent > 100 {
        return Err(ValidationError::InvalidValue(format!(
            "progress_percent {} must be between 0 and 100",
            status.progress_percent
        )));
    }

    // updated_at is required
    if status.updated_at.is_none() {
        return Err(ValidationError::MissingField("updated_at".to_string()));
    }

    // If state is FAILED, error_message must be present
    if status.state == DeploymentState::Failed as i32 && status.error_message.is_empty() {
        return Err(ValidationError::MissingField(
            "error_message (required when state is FAILED)".to_string(),
        ));
    }

    // If state is COMPLETE or VERIFYING, downloaded_hash should be present
    if (status.state == DeploymentState::Complete as i32
        || status.state == DeploymentState::Verifying as i32)
        && status.downloaded_hash.is_empty()
    {
        return Err(ValidationError::MissingField(
            "downloaded_hash (required for COMPLETE or VERIFYING state)".to_string(),
        ));
    }

    // Validate downloaded_hash format if present
    if !status.downloaded_hash.is_empty() {
        if status.downloaded_hash.len() != 64 {
            return Err(ValidationError::InvalidValue(format!(
                "downloaded_hash must be 64 hex characters, got {}",
                status.downloaded_hash.len()
            )));
        }

        if !status
            .downloaded_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        {
            return Err(ValidationError::InvalidValue(
                "downloaded_hash must contain only hex characters".to_string(),
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Sensor Validators
// ============================================================================

/// Validate sensor orientation values
///
/// Validates:
/// - bearing_offset_deg is in range [0, 360)
/// - elevation_offset_deg is in range [-90, 90]
/// - roll_offset_deg is in range [-180, 180]
pub fn validate_sensor_orientation(orientation: &SensorOrientation) -> ValidationResult<()> {
    // Bearing should be [0, 360)
    if orientation.bearing_offset_deg < 0.0 || orientation.bearing_offset_deg >= 360.0 {
        return Err(ValidationError::InvalidValue(format!(
            "bearing_offset_deg {} must be in range [0, 360)",
            orientation.bearing_offset_deg
        )));
    }

    // Elevation should be [-90, 90]
    if orientation.elevation_offset_deg < -90.0 || orientation.elevation_offset_deg > 90.0 {
        return Err(ValidationError::InvalidValue(format!(
            "elevation_offset_deg {} must be in range [-90, 90]",
            orientation.elevation_offset_deg
        )));
    }

    // Roll should be [-180, 180]
    if orientation.roll_offset_deg < -180.0 || orientation.roll_offset_deg > 180.0 {
        return Err(ValidationError::InvalidValue(format!(
            "roll_offset_deg {} must be in range [-180, 180]",
            orientation.roll_offset_deg
        )));
    }

    Ok(())
}

/// Validate field of view values
///
/// Validates:
/// - horizontal_deg is positive and reasonable (< 360)
/// - vertical_deg is positive and reasonable (< 180)
/// - max_range_m is non-negative if specified
pub fn validate_field_of_view(fov: &FieldOfView) -> ValidationResult<()> {
    // Horizontal FOV must be positive and reasonable
    if fov.horizontal_deg <= 0.0 || fov.horizontal_deg >= 360.0 {
        return Err(ValidationError::InvalidValue(format!(
            "horizontal_deg {} must be in range (0, 360)",
            fov.horizontal_deg
        )));
    }

    // Vertical FOV must be positive and reasonable
    if fov.vertical_deg <= 0.0 || fov.vertical_deg >= 180.0 {
        return Err(ValidationError::InvalidValue(format!(
            "vertical_deg {} must be in range (0, 180)",
            fov.vertical_deg
        )));
    }

    // Max range must be non-negative
    if fov.max_range_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "max_range_m must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate gimbal limits
///
/// Validates:
/// - pan_min <= pan_max
/// - tilt_min <= tilt_max
/// - zoom_min <= zoom_max
/// - zoom values are positive
pub fn validate_gimbal_limits(limits: &GimbalLimits) -> ValidationResult<()> {
    if limits.pan_min_deg > limits.pan_max_deg {
        return Err(ValidationError::ConstraintViolation(
            "pan_min_deg must be <= pan_max_deg".to_string(),
        ));
    }

    if limits.tilt_min_deg > limits.tilt_max_deg {
        return Err(ValidationError::ConstraintViolation(
            "tilt_min_deg must be <= tilt_max_deg".to_string(),
        ));
    }

    if limits.roll_min_deg > limits.roll_max_deg {
        return Err(ValidationError::ConstraintViolation(
            "roll_min_deg must be <= roll_max_deg".to_string(),
        ));
    }

    if limits.zoom_min <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "zoom_min must be positive".to_string(),
        ));
    }

    if limits.zoom_max < limits.zoom_min {
        return Err(ValidationError::ConstraintViolation(
            "zoom_max must be >= zoom_min".to_string(),
        ));
    }

    Ok(())
}

/// Validate gimbal state against limits
///
/// Validates:
/// - pan_deg is within limits
/// - tilt_deg is within limits
/// - zoom is within limits
pub fn validate_gimbal_state(
    state: &GimbalState,
    limits: Option<&GimbalLimits>,
) -> ValidationResult<()> {
    // Zoom must be positive
    if state.zoom <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "zoom must be positive".to_string(),
        ));
    }

    // If limits are provided, validate state is within them
    if let Some(limits) = limits {
        if state.pan_deg < limits.pan_min_deg || state.pan_deg > limits.pan_max_deg {
            return Err(ValidationError::ConstraintViolation(format!(
                "pan_deg {} must be within limits [{}, {}]",
                state.pan_deg, limits.pan_min_deg, limits.pan_max_deg
            )));
        }

        if state.tilt_deg < limits.tilt_min_deg || state.tilt_deg > limits.tilt_max_deg {
            return Err(ValidationError::ConstraintViolation(format!(
                "tilt_deg {} must be within limits [{}, {}]",
                state.tilt_deg, limits.tilt_min_deg, limits.tilt_max_deg
            )));
        }

        if state.zoom < limits.zoom_min || state.zoom > limits.zoom_max {
            return Err(ValidationError::ConstraintViolation(format!(
                "zoom {} must be within limits [{}, {}]",
                state.zoom, limits.zoom_min, limits.zoom_max
            )));
        }
    }

    Ok(())
}

/// Validate a complete sensor specification
///
/// Validates:
/// - sensor_id is present
/// - name is present
/// - mount_type is specified
/// - base_orientation is valid
/// - field_of_view is present and valid
/// - For non-fixed mounts: gimbal_limits should be present
/// - For fixed mounts: gimbal_limits and current_state should be absent
pub fn validate_sensor_spec(spec: &SensorSpec) -> ValidationResult<()> {
    // Check required fields
    if spec.sensor_id.is_empty() {
        return Err(ValidationError::MissingField("sensor_id".to_string()));
    }

    if spec.name.is_empty() {
        return Err(ValidationError::MissingField("name".to_string()));
    }

    // Mount type must be specified
    if spec.mount_type == SensorMountType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "mount_type must be specified".to_string(),
        ));
    }

    // Validate base orientation if present
    if let Some(ref orientation) = spec.base_orientation {
        validate_sensor_orientation(orientation)?;
    }

    // Field of view is required
    let fov = spec
        .field_of_view
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("field_of_view".to_string()))?;
    validate_field_of_view(fov)?;

    // For articulated mounts, gimbal_limits should be present
    let is_fixed = spec.mount_type == SensorMountType::Fixed as i32;

    if !is_fixed {
        // PTZ, Gimbal, or Turret should have limits
        if spec.gimbal_limits.is_none() {
            return Err(ValidationError::MissingField(
                "gimbal_limits (required for non-fixed mount types)".to_string(),
            ));
        }

        // Validate gimbal limits
        if let Some(ref limits) = spec.gimbal_limits {
            validate_gimbal_limits(limits)?;
        }

        // Validate current state if present
        if let Some(ref state) = spec.current_state {
            validate_gimbal_state(state, spec.gimbal_limits.as_ref())?;
        }
    }

    // Resolution should be positive if specified
    if spec.resolution_width > 0 && spec.resolution_height == 0 {
        return Err(ValidationError::InvalidValue(
            "resolution_height must be positive when resolution_width is set".to_string(),
        ));
    }

    if spec.resolution_height > 0 && spec.resolution_width == 0 {
        return Err(ValidationError::InvalidValue(
            "resolution_width must be positive when resolution_height is set".to_string(),
        ));
    }

    // Frame rate should be non-negative
    if spec.frame_rate_fps < 0.0 {
        return Err(ValidationError::InvalidValue(
            "frame_rate_fps must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate a sensor state update message
///
/// Validates:
/// - platform_id is present
/// - sensor spec is valid
/// - status is specified
/// - timestamp is present
pub fn validate_sensor_state_update(update: &SensorStateUpdate) -> ValidationResult<()> {
    if update.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    // Sensor spec is required
    let sensor = update
        .sensor
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("sensor".to_string()))?;
    validate_sensor_spec(sensor)?;

    // Status must be specified
    if update.status == SensorStatus::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "status must be specified".to_string(),
        ));
    }

    // Timestamp is required
    if update.timestamp.is_none() {
        return Err(ValidationError::MissingField("timestamp".to_string()));
    }

    Ok(())
}

// ============================================================================
// Actuator Validators
// ============================================================================

/// Validate linear actuator limits
///
/// Validates:
/// - position_min <= position_max
/// - velocity_max is non-negative
/// - force_max is non-negative
pub fn validate_linear_limits(limits: &LinearLimits) -> ValidationResult<()> {
    if limits.position_min_m > limits.position_max_m {
        return Err(ValidationError::ConstraintViolation(
            "position_min_m must be <= position_max_m".to_string(),
        ));
    }

    if limits.velocity_max_mps < 0.0 {
        return Err(ValidationError::InvalidValue(
            "velocity_max_mps must be non-negative".to_string(),
        ));
    }

    if limits.force_max_n < 0.0 {
        return Err(ValidationError::InvalidValue(
            "force_max_n must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate rotary actuator limits
///
/// Validates:
/// - angle_min <= angle_max
/// - velocity_max is non-negative
/// - torque_max is non-negative
pub fn validate_rotary_limits(limits: &RotaryLimits) -> ValidationResult<()> {
    // For continuous rotation, min/max might be equal (e.g., both 0 for unlimited)
    if !limits.continuous_rotation && limits.angle_min_deg > limits.angle_max_deg {
        return Err(ValidationError::ConstraintViolation(
            "angle_min_deg must be <= angle_max_deg for non-continuous rotation".to_string(),
        ));
    }

    if limits.velocity_max_dps < 0.0 {
        return Err(ValidationError::InvalidValue(
            "velocity_max_dps must be non-negative".to_string(),
        ));
    }

    if limits.torque_max_nm < 0.0 {
        return Err(ValidationError::InvalidValue(
            "torque_max_nm must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate gripper limits
///
/// Validates:
/// - aperture_min <= aperture_max
/// - aperture values are non-negative
/// - grip_force_max is non-negative
/// - payload_max is non-negative
pub fn validate_gripper_limits(limits: &GripperLimits) -> ValidationResult<()> {
    if limits.aperture_min_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "aperture_min_m must be non-negative".to_string(),
        ));
    }

    if limits.aperture_min_m > limits.aperture_max_m {
        return Err(ValidationError::ConstraintViolation(
            "aperture_min_m must be <= aperture_max_m".to_string(),
        ));
    }

    if limits.grip_force_max_n < 0.0 {
        return Err(ValidationError::InvalidValue(
            "grip_force_max_n must be non-negative".to_string(),
        ));
    }

    if limits.payload_max_kg < 0.0 {
        return Err(ValidationError::InvalidValue(
            "payload_max_kg must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate valve limits
///
/// Validates:
/// - position values are in [0.0, 1.0]
/// - position_min <= position_max
/// - travel time is positive
pub fn validate_valve_limits(limits: &ValveLimits) -> ValidationResult<()> {
    if limits.position_min < 0.0 || limits.position_min > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position_min {} must be in range [0.0, 1.0]",
            limits.position_min
        )));
    }

    if limits.position_max < 0.0 || limits.position_max > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position_max {} must be in range [0.0, 1.0]",
            limits.position_max
        )));
    }

    if limits.position_min > limits.position_max {
        return Err(ValidationError::ConstraintViolation(
            "position_min must be <= position_max".to_string(),
        ));
    }

    if limits.full_travel_time_s <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "full_travel_time_s must be positive".to_string(),
        ));
    }

    Ok(())
}

/// Validate barrier/gate limits
///
/// Validates:
/// - position values are in [0.0, 1.0]
/// - cycle time is positive
/// - dimensions are non-negative
pub fn validate_barrier_limits(limits: &BarrierLimits) -> ValidationResult<()> {
    if limits.position_min < 0.0 || limits.position_min > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position_min {} must be in range [0.0, 1.0]",
            limits.position_min
        )));
    }

    if limits.position_max < 0.0 || limits.position_max > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position_max {} must be in range [0.0, 1.0]",
            limits.position_max
        )));
    }

    if limits.full_cycle_time_s <= 0.0 {
        return Err(ValidationError::InvalidValue(
            "full_cycle_time_s must be positive".to_string(),
        ));
    }

    if limits.clear_width_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "clear_width_m must be non-negative".to_string(),
        ));
    }

    if limits.clear_height_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "clear_height_m must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate winch limits
///
/// Validates:
/// - cable_min <= cable_max
/// - cable values are non-negative
/// - speed and tension are non-negative
pub fn validate_winch_limits(limits: &WinchLimits) -> ValidationResult<()> {
    if limits.cable_min_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "cable_min_m must be non-negative".to_string(),
        ));
    }

    if limits.cable_min_m > limits.cable_max_m {
        return Err(ValidationError::ConstraintViolation(
            "cable_min_m must be <= cable_max_m".to_string(),
        ));
    }

    if limits.line_speed_max_mps < 0.0 {
        return Err(ValidationError::InvalidValue(
            "line_speed_max_mps must be non-negative".to_string(),
        ));
    }

    if limits.tension_max_n < 0.0 {
        return Err(ValidationError::InvalidValue(
            "tension_max_n must be non-negative".to_string(),
        ));
    }

    if limits.payload_max_kg < 0.0 {
        return Err(ValidationError::InvalidValue(
            "payload_max_kg must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate linear actuator state against limits
pub fn validate_linear_state(
    state: &LinearState,
    limits: Option<&LinearLimits>,
) -> ValidationResult<()> {
    if let Some(limits) = limits {
        if state.position_m < limits.position_min_m || state.position_m > limits.position_max_m {
            return Err(ValidationError::ConstraintViolation(format!(
                "position_m {} must be within limits [{}, {}]",
                state.position_m, limits.position_min_m, limits.position_max_m
            )));
        }
    }
    Ok(())
}

/// Validate rotary actuator state against limits
pub fn validate_rotary_state(
    state: &RotaryState,
    limits: Option<&RotaryLimits>,
) -> ValidationResult<()> {
    if let Some(limits) = limits {
        if !limits.continuous_rotation
            && (state.angle_deg < limits.angle_min_deg || state.angle_deg > limits.angle_max_deg)
        {
            return Err(ValidationError::ConstraintViolation(format!(
                "angle_deg {} must be within limits [{}, {}]",
                state.angle_deg, limits.angle_min_deg, limits.angle_max_deg
            )));
        }
    }
    Ok(())
}

/// Validate gripper state against limits
pub fn validate_gripper_state(
    state: &GripperState,
    limits: Option<&GripperLimits>,
) -> ValidationResult<()> {
    if state.aperture_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "aperture_m must be non-negative".to_string(),
        ));
    }

    if let Some(limits) = limits {
        if state.aperture_m < limits.aperture_min_m || state.aperture_m > limits.aperture_max_m {
            return Err(ValidationError::ConstraintViolation(format!(
                "aperture_m {} must be within limits [{}, {}]",
                state.aperture_m, limits.aperture_min_m, limits.aperture_max_m
            )));
        }
    }
    Ok(())
}

/// Validate valve state
pub fn validate_valve_state(state: &ValveState) -> ValidationResult<()> {
    if state.position < 0.0 || state.position > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position {} must be in range [0.0, 1.0]",
            state.position
        )));
    }
    Ok(())
}

/// Validate barrier/gate state
pub fn validate_barrier_state(state: &BarrierState) -> ValidationResult<()> {
    if state.position < 0.0 || state.position > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "position {} must be in range [0.0, 1.0]",
            state.position
        )));
    }

    // Consistency checks
    if state.is_closed && state.position > 0.01 {
        return Err(ValidationError::ConstraintViolation(
            "is_closed should not be true when position > 0".to_string(),
        ));
    }

    if state.is_open && state.position < 0.99 {
        return Err(ValidationError::ConstraintViolation(
            "is_open should not be true when position < 1".to_string(),
        ));
    }

    Ok(())
}

/// Validate winch state against limits
pub fn validate_winch_state(
    state: &WinchState,
    limits: Option<&WinchLimits>,
) -> ValidationResult<()> {
    if state.cable_out_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "cable_out_m must be non-negative".to_string(),
        ));
    }

    if let Some(limits) = limits {
        if state.cable_out_m > limits.cable_max_m {
            return Err(ValidationError::ConstraintViolation(format!(
                "cable_out_m {} exceeds max {}",
                state.cable_out_m, limits.cable_max_m
            )));
        }
    }
    Ok(())
}

/// Validate lock state (basic validation - locks have minimal numeric constraints)
pub fn validate_lock_state(_state: &LockState) -> ValidationResult<()> {
    // Lock state is mostly boolean - no numeric constraints to validate
    // Could validate timestamps if needed
    Ok(())
}

/// Validate a complete actuator specification
///
/// Validates:
/// - actuator_id is present
/// - name is present
/// - actuator_type is specified
/// - mount is specified
/// - Type-specific limits are valid if present
/// - Type-specific state is valid if present
pub fn validate_actuator_spec(spec: &ActuatorSpec) -> ValidationResult<()> {
    // Check required fields
    if spec.actuator_id.is_empty() {
        return Err(ValidationError::MissingField("actuator_id".to_string()));
    }

    if spec.name.is_empty() {
        return Err(ValidationError::MissingField("name".to_string()));
    }

    // Type must be specified
    if spec.actuator_type == ActuatorType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "actuator_type must be specified".to_string(),
        ));
    }

    // Mount must be specified
    if spec.mount == ActuatorMount::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "mount must be specified".to_string(),
        ));
    }

    // Validate type-specific limits if present
    if let Some(ref limits) = spec.limits {
        use crate::actuator::v1::actuator_spec::Limits;
        match limits {
            Limits::LinearLimits(l) => validate_linear_limits(l)?,
            Limits::RotaryLimits(l) => validate_rotary_limits(l)?,
            Limits::GripperLimits(l) => validate_gripper_limits(l)?,
            Limits::ValveLimits(l) => validate_valve_limits(l)?,
            Limits::BarrierLimits(l) => validate_barrier_limits(l)?,
            Limits::WinchLimits(l) => validate_winch_limits(l)?,
        }
    }

    // Validate type-specific state if present
    if let Some(ref state) = spec.state {
        use crate::actuator::v1::actuator_spec::State;
        match state {
            State::LinearState(s) => {
                let limits = spec.limits.as_ref().and_then(|l| {
                    if let crate::actuator::v1::actuator_spec::Limits::LinearLimits(ll) = l {
                        Some(ll)
                    } else {
                        None
                    }
                });
                validate_linear_state(s, limits)?;
            }
            State::RotaryState(s) => {
                let limits = spec.limits.as_ref().and_then(|l| {
                    if let crate::actuator::v1::actuator_spec::Limits::RotaryLimits(rl) = l {
                        Some(rl)
                    } else {
                        None
                    }
                });
                validate_rotary_state(s, limits)?;
            }
            State::GripperState(s) => {
                let limits = spec.limits.as_ref().and_then(|l| {
                    if let crate::actuator::v1::actuator_spec::Limits::GripperLimits(gl) = l {
                        Some(gl)
                    } else {
                        None
                    }
                });
                validate_gripper_state(s, limits)?;
            }
            State::ValveState(s) => validate_valve_state(s)?,
            State::BarrierState(s) => validate_barrier_state(s)?,
            State::WinchState(s) => {
                let limits = spec.limits.as_ref().and_then(|l| {
                    if let crate::actuator::v1::actuator_spec::Limits::WinchLimits(wl) = l {
                        Some(wl)
                    } else {
                        None
                    }
                });
                validate_winch_state(s, limits)?;
            }
            State::LockState(s) => validate_lock_state(s)?,
        }
    }

    Ok(())
}

/// Validate an actuator state update message
///
/// Validates:
/// - platform_id is present
/// - actuator spec is valid
/// - status is specified
/// - timestamp is present
pub fn validate_actuator_state_update(update: &ActuatorStateUpdate) -> ValidationResult<()> {
    if update.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    // Actuator spec is required
    let actuator = update
        .actuator
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("actuator".to_string()))?;
    validate_actuator_spec(actuator)?;

    // Status must be specified
    if update.status == ActuatorStatus::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "status must be specified".to_string(),
        ));
    }

    // Timestamp is required
    if update.timestamp.is_none() {
        return Err(ValidationError::MissingField("timestamp".to_string()));
    }

    Ok(())
}

/// Validate an actuator command
///
/// Validates:
/// - command_id is present
/// - platform_id is present
/// - actuator_id is present
/// - command_type is specified
/// - issued_at is present
pub fn validate_actuator_command(cmd: &ActuatorCommand) -> ValidationResult<()> {
    if cmd.command_id.is_empty() {
        return Err(ValidationError::MissingField("command_id".to_string()));
    }

    if cmd.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    if cmd.actuator_id.is_empty() {
        return Err(ValidationError::MissingField("actuator_id".to_string()));
    }

    if cmd.command_type == ActuatorCommandType::ActuatorCommandUnspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "command_type must be specified".to_string(),
        ));
    }

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

    mod model_deployment_tests {
        use super::*;
        use crate::common::v1::Timestamp;

        fn valid_model_deployment() -> ModelDeployment {
            ModelDeployment {
                deployment_id: "deploy-2025-001".to_string(),
                model_id: "yolov8-poi-v2.1".to_string(),
                model_version: "2.1.0".to_string(),
                model_type: ModelType::Detector as i32,
                model_url: "https://models.example.com/yolov8-poi-v2.1.onnx".to_string(),
                checksum_sha256: "a".repeat(64), // Valid SHA256 hex
                file_size_bytes: 45_000_000,
                target_platforms: vec!["Alpha-3".to_string(), "Bravo-1".to_string()],
                deployment_policy: DeploymentPolicy::Rolling as i32,
                priority: DeploymentPriority::Normal as i32,
                deployed_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                deployed_by: "MLOps-Pipeline".to_string(),
                rollback_model_id: String::new(),
                metadata: None,
            }
        }

        #[test]
        fn test_valid_model_deployment() {
            let deployment = valid_model_deployment();
            assert!(validate_model_deployment(&deployment).is_ok());
        }

        #[test]
        fn test_missing_deployment_id() {
            let mut deployment = valid_model_deployment();
            deployment.deployment_id = String::new();
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "deployment_id"));
        }

        #[test]
        fn test_missing_model_id() {
            let mut deployment = valid_model_deployment();
            deployment.model_id = String::new();
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "model_id"));
        }

        #[test]
        fn test_unspecified_model_type() {
            let mut deployment = valid_model_deployment();
            deployment.model_type = ModelType::Unspecified as i32;
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_model_url() {
            let mut deployment = valid_model_deployment();
            deployment.model_url = "not-a-valid-url".to_string();
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_checksum_length() {
            let mut deployment = valid_model_deployment();
            deployment.checksum_sha256 = "abc123".to_string(); // Too short
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_checksum_chars() {
            let mut deployment = valid_model_deployment();
            deployment.checksum_sha256 = "g".repeat(64); // 'g' is not hex
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_zero_file_size() {
            let mut deployment = valid_model_deployment();
            deployment.file_size_bytes = 0;
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_empty_target_platforms() {
            let mut deployment = valid_model_deployment();
            deployment.target_platforms = vec![];
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(_)));
        }

        #[test]
        fn test_unspecified_deployment_policy() {
            let mut deployment = valid_model_deployment();
            deployment.deployment_policy = DeploymentPolicy::Unspecified as i32;
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_missing_deployed_at() {
            let mut deployment = valid_model_deployment();
            deployment.deployed_at = None;
            let err = validate_model_deployment(&deployment).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "deployed_at"));
        }
    }

    mod model_deployment_status_tests {
        use super::*;
        use crate::common::v1::Timestamp;

        fn valid_deployment_status() -> ModelDeploymentStatus {
            ModelDeploymentStatus {
                deployment_id: "deploy-2025-001".to_string(),
                platform_id: "Alpha-3".to_string(),
                state: DeploymentState::Downloading as i32,
                progress_percent: 45,
                error_message: String::new(),
                updated_at: Some(Timestamp {
                    seconds: 1702000100,
                    nanos: 0,
                }),
                downloaded_hash: String::new(),
                previous_version: "2.0.0".to_string(),
            }
        }

        #[test]
        fn test_valid_deployment_status() {
            let status = valid_deployment_status();
            assert!(validate_model_deployment_status(&status).is_ok());
        }

        #[test]
        fn test_missing_deployment_id() {
            let mut status = valid_deployment_status();
            status.deployment_id = String::new();
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "deployment_id"));
        }

        #[test]
        fn test_missing_platform_id() {
            let mut status = valid_deployment_status();
            status.platform_id = String::new();
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "platform_id"));
        }

        #[test]
        fn test_unspecified_state() {
            let mut status = valid_deployment_status();
            status.state = DeploymentState::Unspecified as i32;
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_progress_percent() {
            let mut status = valid_deployment_status();
            status.progress_percent = 150; // > 100
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_missing_updated_at() {
            let mut status = valid_deployment_status();
            status.updated_at = None;
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "updated_at"));
        }

        #[test]
        fn test_failed_state_requires_error_message() {
            let mut status = valid_deployment_status();
            status.state = DeploymentState::Failed as i32;
            status.error_message = String::new();
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(_)));
        }

        #[test]
        fn test_complete_state_requires_hash() {
            let mut status = valid_deployment_status();
            status.state = DeploymentState::Complete as i32;
            status.downloaded_hash = String::new();
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(_)));
        }

        #[test]
        fn test_valid_complete_status() {
            let mut status = valid_deployment_status();
            status.state = DeploymentState::Complete as i32;
            status.downloaded_hash = "a".repeat(64);
            status.progress_percent = 100;
            assert!(validate_model_deployment_status(&status).is_ok());
        }

        #[test]
        fn test_invalid_downloaded_hash_length() {
            let mut status = valid_deployment_status();
            status.state = DeploymentState::Complete as i32;
            status.downloaded_hash = "abc123".to_string(); // Too short
            let err = validate_model_deployment_status(&status).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }
    }

    mod sensor_spec_tests {
        use super::*;
        use crate::common::v1::Timestamp;
        use crate::sensor::v1::SensorModality;

        fn valid_fixed_sensor() -> SensorSpec {
            SensorSpec {
                sensor_id: "eo-main".to_string(),
                name: "Main EO Camera".to_string(),
                mount_type: SensorMountType::Fixed as i32,
                base_orientation: Some(SensorOrientation {
                    bearing_offset_deg: 0.0,   // Forward
                    elevation_offset_deg: 0.0, // Level
                    roll_offset_deg: 0.0,
                }),
                field_of_view: Some(FieldOfView {
                    horizontal_deg: 62.0,
                    vertical_deg: 48.0,
                    diagonal_deg: 0.0,
                    max_range_m: 500.0,
                }),
                modality: SensorModality::Eo as i32,
                resolution_width: 1920,
                resolution_height: 1080,
                frame_rate_fps: 30.0,
                gimbal_limits: None, // Fixed mount - no gimbal
                current_state: None,
                updated_at: None,
            }
        }

        fn valid_ptz_sensor() -> SensorSpec {
            SensorSpec {
                sensor_id: "ptz-tower".to_string(),
                name: "Tower PTZ Camera".to_string(),
                mount_type: SensorMountType::Ptz as i32,
                base_orientation: Some(SensorOrientation {
                    bearing_offset_deg: 0.0,
                    elevation_offset_deg: 0.0,
                    roll_offset_deg: 0.0,
                }),
                field_of_view: Some(FieldOfView {
                    horizontal_deg: 45.0,
                    vertical_deg: 35.0,
                    diagonal_deg: 0.0,
                    max_range_m: 1000.0,
                }),
                modality: SensorModality::Eo as i32,
                resolution_width: 3840,
                resolution_height: 2160,
                frame_rate_fps: 30.0,
                gimbal_limits: Some(GimbalLimits {
                    pan_min_deg: -180.0,
                    pan_max_deg: 180.0,
                    tilt_min_deg: -30.0,
                    tilt_max_deg: 90.0,
                    roll_min_deg: 0.0,
                    roll_max_deg: 0.0,
                    zoom_min: 1.0,
                    zoom_max: 30.0,
                    pan_rate_max: 45.0,
                    tilt_rate_max: 30.0,
                }),
                current_state: Some(GimbalState {
                    pan_deg: 45.0,
                    tilt_deg: 15.0,
                    roll_deg: 0.0,
                    zoom: 2.0,
                    tracking: false,
                    tracked_target_id: String::new(),
                }),
                updated_at: None,
            }
        }

        #[test]
        fn test_valid_fixed_sensor() {
            let sensor = valid_fixed_sensor();
            assert!(validate_sensor_spec(&sensor).is_ok());
        }

        #[test]
        fn test_valid_ptz_sensor() {
            let sensor = valid_ptz_sensor();
            assert!(validate_sensor_spec(&sensor).is_ok());
        }

        #[test]
        fn test_missing_sensor_id() {
            let mut sensor = valid_fixed_sensor();
            sensor.sensor_id = String::new();
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "sensor_id"));
        }

        #[test]
        fn test_missing_name() {
            let mut sensor = valid_fixed_sensor();
            sensor.name = String::new();
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "name"));
        }

        #[test]
        fn test_unspecified_mount_type() {
            let mut sensor = valid_fixed_sensor();
            sensor.mount_type = SensorMountType::Unspecified as i32;
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_missing_fov() {
            let mut sensor = valid_fixed_sensor();
            sensor.field_of_view = None;
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "field_of_view"));
        }

        #[test]
        fn test_ptz_without_gimbal_limits() {
            let mut sensor = valid_ptz_sensor();
            sensor.gimbal_limits = None;
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(_)));
        }

        #[test]
        fn test_invalid_bearing() {
            let mut sensor = valid_fixed_sensor();
            sensor.base_orientation = Some(SensorOrientation {
                bearing_offset_deg: 400.0, // Invalid
                elevation_offset_deg: 0.0,
                roll_offset_deg: 0.0,
            });
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_elevation() {
            let mut sensor = valid_fixed_sensor();
            sensor.base_orientation = Some(SensorOrientation {
                bearing_offset_deg: 0.0,
                elevation_offset_deg: 100.0, // Invalid
                roll_offset_deg: 0.0,
            });
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_horizontal_fov() {
            let mut sensor = valid_fixed_sensor();
            sensor.field_of_view = Some(FieldOfView {
                horizontal_deg: 0.0, // Invalid
                vertical_deg: 48.0,
                diagonal_deg: 0.0,
                max_range_m: 500.0,
            });
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_gimbal_pan_range() {
            let mut sensor = valid_ptz_sensor();
            sensor.gimbal_limits = Some(GimbalLimits {
                pan_min_deg: 100.0,
                pan_max_deg: -100.0, // Invalid: min > max
                tilt_min_deg: -30.0,
                tilt_max_deg: 90.0,
                roll_min_deg: 0.0,
                roll_max_deg: 0.0,
                zoom_min: 1.0,
                zoom_max: 30.0,
                pan_rate_max: 45.0,
                tilt_rate_max: 30.0,
            });
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_gimbal_state_outside_limits() {
            let mut sensor = valid_ptz_sensor();
            sensor.current_state = Some(GimbalState {
                pan_deg: 200.0, // Outside limits [-180, 180]
                tilt_deg: 15.0,
                roll_deg: 0.0,
                zoom: 2.0,
                tracking: false,
                tracked_target_id: String::new(),
            });
            let err = validate_sensor_spec(&sensor).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_valid_sensor_state_update() {
            let update = SensorStateUpdate {
                platform_id: "UGV-Alpha-1".to_string(),
                sensor: Some(valid_fixed_sensor()),
                status: SensorStatus::Operational as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            assert!(validate_sensor_state_update(&update).is_ok());
        }

        #[test]
        fn test_sensor_update_missing_platform_id() {
            let update = SensorStateUpdate {
                platform_id: String::new(),
                sensor: Some(valid_fixed_sensor()),
                status: SensorStatus::Operational as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            let err = validate_sensor_state_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "platform_id"));
        }

        #[test]
        fn test_sensor_update_unspecified_status() {
            let update = SensorStateUpdate {
                platform_id: "UGV-Alpha-1".to_string(),
                sensor: Some(valid_fixed_sensor()),
                status: SensorStatus::Unspecified as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            let err = validate_sensor_state_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }
    }

    mod actuator_spec_tests {
        use super::*;
        use crate::actuator::v1::{actuator_spec::Limits, actuator_spec::State, ActuatorDrive};
        use crate::common::v1::Timestamp;

        fn valid_barrier_actuator() -> ActuatorSpec {
            ActuatorSpec {
                actuator_id: "gate-main".to_string(),
                name: "Main Entry Gate".to_string(),
                actuator_type: ActuatorType::Barrier as i32,
                mount: ActuatorMount::Fixed as i32,
                drive: ActuatorDrive::Electric as i32,
                limits: Some(Limits::BarrierLimits(BarrierLimits {
                    position_min: 0.0,
                    position_max: 1.0,
                    full_cycle_time_s: 8.0,
                    clear_width_m: 4.0,
                    clear_height_m: 2.5,
                })),
                state: Some(State::BarrierState(BarrierState {
                    position: 0.0,
                    is_closed: true,
                    is_open: false,
                    obstruction_detected: false,
                })),
                updated_at: None,
                metadata_json: String::new(),
            }
        }

        fn valid_linear_actuator() -> ActuatorSpec {
            ActuatorSpec {
                actuator_id: "lift-main".to_string(),
                name: "Cargo Lift".to_string(),
                actuator_type: ActuatorType::Linear as i32,
                mount: ActuatorMount::Fixed as i32,
                drive: ActuatorDrive::Hydraulic as i32,
                limits: Some(Limits::LinearLimits(LinearLimits {
                    position_min_m: 0.0,
                    position_max_m: 3.0,
                    velocity_max_mps: 0.5,
                    acceleration_max: 0.2,
                    force_max_n: 50000.0,
                })),
                state: Some(State::LinearState(LinearState {
                    position_m: 1.5,
                    velocity_mps: 0.0,
                    force_n: 0.0,
                })),
                updated_at: None,
                metadata_json: String::new(),
            }
        }

        fn valid_winch_actuator() -> ActuatorSpec {
            ActuatorSpec {
                actuator_id: "crane-hoist".to_string(),
                name: "Container Crane Hoist".to_string(),
                actuator_type: ActuatorType::Winch as i32,
                mount: ActuatorMount::Fixed as i32,
                drive: ActuatorDrive::Electric as i32,
                limits: Some(Limits::WinchLimits(WinchLimits {
                    cable_min_m: 0.0,
                    cable_max_m: 50.0,
                    line_speed_max_mps: 2.0,
                    tension_max_n: 500000.0,
                    payload_max_kg: 40000.0,
                })),
                state: Some(State::WinchState(WinchState {
                    cable_out_m: 25.0,
                    line_speed_mps: 0.0,
                    tension_n: 150000.0,
                    payload_kg: 15000.0,
                })),
                updated_at: None,
                metadata_json: String::new(),
            }
        }

        #[test]
        fn test_valid_barrier_actuator() {
            let actuator = valid_barrier_actuator();
            assert!(validate_actuator_spec(&actuator).is_ok());
        }

        #[test]
        fn test_valid_linear_actuator() {
            let actuator = valid_linear_actuator();
            assert!(validate_actuator_spec(&actuator).is_ok());
        }

        #[test]
        fn test_valid_winch_actuator() {
            let actuator = valid_winch_actuator();
            assert!(validate_actuator_spec(&actuator).is_ok());
        }

        #[test]
        fn test_missing_actuator_id() {
            let mut actuator = valid_barrier_actuator();
            actuator.actuator_id = String::new();
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "actuator_id"));
        }

        #[test]
        fn test_missing_name() {
            let mut actuator = valid_barrier_actuator();
            actuator.name = String::new();
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "name"));
        }

        #[test]
        fn test_unspecified_actuator_type() {
            let mut actuator = valid_barrier_actuator();
            actuator.actuator_type = ActuatorType::Unspecified as i32;
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_unspecified_mount() {
            let mut actuator = valid_barrier_actuator();
            actuator.mount = ActuatorMount::Unspecified as i32;
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_invalid_barrier_position() {
            let mut actuator = valid_barrier_actuator();
            actuator.state = Some(State::BarrierState(BarrierState {
                position: 1.5, // Invalid: > 1.0
                is_closed: false,
                is_open: false,
                obstruction_detected: false,
            }));
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_barrier_state_consistency() {
            let mut actuator = valid_barrier_actuator();
            actuator.state = Some(State::BarrierState(BarrierState {
                position: 0.5,   // Half open
                is_closed: true, // Invalid: closed but position > 0
                is_open: false,
                obstruction_detected: false,
            }));
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_linear_state_outside_limits() {
            let mut actuator = valid_linear_actuator();
            actuator.state = Some(State::LinearState(LinearState {
                position_m: 5.0, // Outside limits [0, 3]
                velocity_mps: 0.0,
                force_n: 0.0,
            }));
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_invalid_linear_limits() {
            let mut actuator = valid_linear_actuator();
            actuator.limits = Some(Limits::LinearLimits(LinearLimits {
                position_min_m: 5.0, // Invalid: min > max
                position_max_m: 3.0,
                velocity_max_mps: 0.5,
                acceleration_max: 0.2,
                force_max_n: 50000.0,
            }));
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_winch_cable_exceeds_max() {
            let mut actuator = valid_winch_actuator();
            actuator.state = Some(State::WinchState(WinchState {
                cable_out_m: 100.0, // Exceeds max of 50m
                line_speed_mps: 0.0,
                tension_n: 0.0,
                payload_kg: 0.0,
            }));
            let err = validate_actuator_spec(&actuator).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }

        #[test]
        fn test_valid_actuator_state_update() {
            let update = ActuatorStateUpdate {
                platform_id: "PORT-GATE-1".to_string(),
                actuator: Some(valid_barrier_actuator()),
                status: ActuatorStatus::Operational as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            assert!(validate_actuator_state_update(&update).is_ok());
        }

        #[test]
        fn test_actuator_update_missing_platform_id() {
            let update = ActuatorStateUpdate {
                platform_id: String::new(),
                actuator: Some(valid_barrier_actuator()),
                status: ActuatorStatus::Operational as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            let err = validate_actuator_state_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "platform_id"));
        }

        #[test]
        fn test_actuator_update_unspecified_status() {
            let update = ActuatorStateUpdate {
                platform_id: "PORT-GATE-1".to_string(),
                actuator: Some(valid_barrier_actuator()),
                status: ActuatorStatus::Unspecified as i32,
                timestamp: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
            };
            let err = validate_actuator_state_update(&update).unwrap_err();
            assert!(matches!(err, ValidationError::InvalidValue(_)));
        }

        #[test]
        fn test_valid_actuator_command() {
            let cmd = ActuatorCommand {
                command_id: "CMD-001".to_string(),
                platform_id: "PORT-GATE-1".to_string(),
                actuator_id: "gate-main".to_string(),
                command_type: ActuatorCommandType::ActuatorCommandDisengage as i32,
                target_position: 1.0,
                target_velocity: 0.0,
                priority: 1,
                issued_by: "operator-1".to_string(),
                issued_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                expires_at: None,
            };
            assert!(validate_actuator_command(&cmd).is_ok());
        }

        #[test]
        fn test_command_missing_command_id() {
            let cmd = ActuatorCommand {
                command_id: String::new(),
                platform_id: "PORT-GATE-1".to_string(),
                actuator_id: "gate-main".to_string(),
                command_type: ActuatorCommandType::ActuatorCommandDisengage as i32,
                target_position: 1.0,
                target_velocity: 0.0,
                priority: 1,
                issued_by: "operator-1".to_string(),
                issued_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                expires_at: None,
            };
            let err = validate_actuator_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::MissingField(f) if f == "command_id"));
        }

        #[test]
        fn test_command_expires_before_issued() {
            let cmd = ActuatorCommand {
                command_id: "CMD-001".to_string(),
                platform_id: "PORT-GATE-1".to_string(),
                actuator_id: "gate-main".to_string(),
                command_type: ActuatorCommandType::ActuatorCommandDisengage as i32,
                target_position: 1.0,
                target_velocity: 0.0,
                priority: 1,
                issued_by: "operator-1".to_string(),
                issued_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                expires_at: Some(Timestamp {
                    seconds: 1701000000, // Before issued_at
                    nanos: 0,
                }),
            };
            let err = validate_actuator_command(&cmd).unwrap_err();
            assert!(matches!(err, ValidationError::ConstraintViolation(_)));
        }
    }
}
