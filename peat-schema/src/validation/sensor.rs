//! Sensor validators
//!
//! Validates SensorSpec and SensorStateUpdate messages for PEAT Protocol.

use super::{ValidationError, ValidationResult};
use crate::sensor::v1::{
    FieldOfView, GimbalLimits, GimbalState, SensorMountType, SensorOrientation, SensorSpec,
    SensorStateUpdate, SensorStatus,
};

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

#[cfg(test)]
mod tests {
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
