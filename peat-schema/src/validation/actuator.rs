//! Actuator validators
//!
//! Validates ActuatorSpec, ActuatorStateUpdate, and ActuatorCommand messages for Peat Protocol.

use super::{ValidationError, ValidationResult};
use crate::actuator::v1::{
    ActuatorCommand, ActuatorCommandType, ActuatorMount, ActuatorSpec, ActuatorStateUpdate,
    ActuatorStatus, ActuatorType, BarrierLimits, BarrierState, GripperLimits, GripperState,
    LinearLimits, LinearState, LockState, RotaryLimits, RotaryState, ValveLimits, ValveState,
    WinchLimits, WinchState,
};

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
