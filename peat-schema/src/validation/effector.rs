//! Effector validators
//!
//! Validates EffectorSpec, EffectorStateUpdate, and EffectorCommand messages for Peat Protocol.

use super::{ValidationError, ValidationResult};
use crate::effector::v1::{
    AmmunitionStatus, Authorization, AuthorizationLevel, EffectorCategory, EffectorCommand,
    EffectorCommandType, EffectorSpec, EffectorStateUpdate, EffectorStatus, EffectorType,
    FiringSolution, SafetyInterlocks, TargetDesignation,
};

/// Validate safety interlocks
///
/// Validates the safety interlock structure is present and properly formed.
/// This is a basic validation - actual safety verification requires
/// hardware-level checks beyond schema validation.
pub fn validate_safety_interlocks(_interlocks: &SafetyInterlocks) -> ValidationResult<()> {
    // Safety interlocks are boolean flags - no numeric constraints
    // The semantic validation (are all required interlocks true for firing?) is
    // application-level logic, not schema validation
    Ok(())
}

/// Validate ammunition status
///
/// Validates:
/// - rounds_ready <= rounds_total
/// - magazine_capacity > 0 if magazines are used
/// - reload_time_remaining is non-negative
pub fn validate_ammunition_status(status: &AmmunitionStatus) -> ValidationResult<()> {
    if status.rounds_ready > status.rounds_total {
        return Err(ValidationError::ConstraintViolation(
            "rounds_ready cannot exceed rounds_total".to_string(),
        ));
    }

    if status.magazine_capacity == 0 && status.magazines_available > 0 {
        return Err(ValidationError::InvalidValue(
            "magazine_capacity must be > 0 if magazines_available > 0".to_string(),
        ));
    }

    if status.reload_time_remaining_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "reload_time_remaining_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate firing solution
///
/// Validates:
/// - quality is in [0.0, 1.0]
/// - hit_probability is in [0.0, 1.0]
/// - time_to_impact is non-negative
pub fn validate_firing_solution(solution: &FiringSolution) -> ValidationResult<()> {
    if solution.quality < 0.0 || solution.quality > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "quality {} must be in range [0.0, 1.0]",
            solution.quality
        )));
    }

    if solution.hit_probability < 0.0 || solution.hit_probability > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "hit_probability {} must be in range [0.0, 1.0]",
            solution.hit_probability
        )));
    }

    if solution.time_to_impact_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "time_to_impact_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate target designation
///
/// Validates:
/// - target_track_id is present
/// - range is non-negative
pub fn validate_target_designation(target: &TargetDesignation) -> ValidationResult<()> {
    if target.target_track_id.is_empty() {
        return Err(ValidationError::MissingField("target_track_id".to_string()));
    }

    if target.range_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "range_m must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate authorization record
///
/// Validates:
/// - authorization_id is present
/// - authorized_by is present
/// - level is specified
/// - authorized_at is present
pub fn validate_authorization(auth: &Authorization) -> ValidationResult<()> {
    if auth.authorization_id.is_empty() {
        return Err(ValidationError::MissingField(
            "authorization_id".to_string(),
        ));
    }

    if auth.authorized_by.is_empty() {
        return Err(ValidationError::MissingField("authorized_by".to_string()));
    }

    if auth.level == AuthorizationLevel::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "authorization level must be specified".to_string(),
        ));
    }

    if auth.authorized_at.is_none() {
        return Err(ValidationError::MissingField("authorized_at".to_string()));
    }

    // If expires_at is set, it must be after authorized_at
    if let (Some(authorized), Some(expires)) = (&auth.authorized_at, &auth.expires_at) {
        if expires.seconds < authorized.seconds
            || (expires.seconds == authorized.seconds && expires.nanos < authorized.nanos)
        {
            return Err(ValidationError::ConstraintViolation(
                "expires_at must be after authorized_at".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate a complete effector specification
///
/// Validates:
/// - effector_id is present
/// - name is present
/// - effector_type is specified
/// - category is specified
/// - max_range >= min_range
/// - Type-specific capacity is valid if present
/// - Safety interlocks are present
pub fn validate_effector_spec(spec: &EffectorSpec) -> ValidationResult<()> {
    // Check required fields
    if spec.effector_id.is_empty() {
        return Err(ValidationError::MissingField("effector_id".to_string()));
    }

    if spec.name.is_empty() {
        return Err(ValidationError::MissingField("name".to_string()));
    }

    // Type must be specified
    if spec.effector_type == EffectorType::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "effector_type must be specified".to_string(),
        ));
    }

    // Category must be specified
    if spec.category == EffectorCategory::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "category must be specified".to_string(),
        ));
    }

    // Range constraints
    if spec.min_range_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "min_range_m must be non-negative".to_string(),
        ));
    }

    if spec.max_range_m < spec.min_range_m {
        return Err(ValidationError::ConstraintViolation(
            "max_range_m must be >= min_range_m".to_string(),
        ));
    }

    // Validate safety interlocks if present
    if let Some(ref interlocks) = spec.interlocks {
        validate_safety_interlocks(interlocks)?;
    }

    // Validate type-specific capacity
    if let Some(ref capacity) = spec.capacity {
        use crate::effector::v1::effector_spec::Capacity;
        match capacity {
            Capacity::Ammunition(ammo) => validate_ammunition_status(ammo)?,
            Capacity::Energy(energy) => {
                // Energy capacity validation
                if energy.charge_level < 0.0 || energy.charge_level > 1.0 {
                    return Err(ValidationError::InvalidValue(format!(
                        "charge_level {} must be in range [0.0, 1.0]",
                        energy.charge_level
                    )));
                }
                if energy.thermal_level < 0.0 || energy.thermal_level > 1.0 {
                    return Err(ValidationError::InvalidValue(format!(
                        "thermal_level {} must be in range [0.0, 1.0]",
                        energy.thermal_level
                    )));
                }
            }
            Capacity::Dispenser(dispenser) => {
                // Dispenser capacity validation
                if dispenser.units_remaining > dispenser.total_capacity {
                    return Err(ValidationError::ConstraintViolation(
                        "units_remaining cannot exceed total_capacity".to_string(),
                    ));
                }
            }
        }
    }

    // Validate current target if present
    if let Some(ref target) = spec.current_target {
        validate_target_designation(target)?;
    }

    // Validate firing solution if present
    if let Some(ref solution) = spec.firing_solution {
        validate_firing_solution(solution)?;
    }

    // Validate authorization if present
    if let Some(ref auth) = spec.current_authorization {
        validate_authorization(auth)?;
    }

    Ok(())
}

/// Validate an effector state update message
///
/// Validates:
/// - platform_id is present
/// - effector spec is valid
/// - status is specified
/// - timestamp is present
pub fn validate_effector_state_update(update: &EffectorStateUpdate) -> ValidationResult<()> {
    if update.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    // Effector spec is required
    let effector = update
        .effector
        .as_ref()
        .ok_or_else(|| ValidationError::MissingField("effector".to_string()))?;
    validate_effector_spec(effector)?;

    // Status must be specified
    if update.status == EffectorStatus::Unspecified as i32 {
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

/// Validate an effector command
///
/// Validates:
/// - command_id is present
/// - platform_id is present
/// - effector_id is present
/// - command_type is specified
/// - issued_at is present
/// - ARM/ENGAGE commands require authorization
pub fn validate_effector_command(cmd: &EffectorCommand) -> ValidationResult<()> {
    if cmd.command_id.is_empty() {
        return Err(ValidationError::MissingField("command_id".to_string()));
    }

    if cmd.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    if cmd.effector_id.is_empty() {
        return Err(ValidationError::MissingField("effector_id".to_string()));
    }

    if cmd.command_type == EffectorCommandType::EffectorCommandUnspecified as i32 {
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

    // ARM and ENGAGE commands require authorization
    let requires_auth = cmd.command_type == EffectorCommandType::EffectorCommandArm as i32
        || cmd.command_type == EffectorCommandType::EffectorCommandEngage as i32;

    if requires_auth && cmd.authorization.is_none() {
        return Err(ValidationError::MissingField(
            "authorization (required for ARM/ENGAGE commands)".to_string(),
        ));
    }

    // Validate authorization if present
    if let Some(ref auth) = cmd.authorization {
        validate_authorization(auth)?;
    }

    // ENGAGE commands require target designation
    if cmd.command_type == EffectorCommandType::EffectorCommandEngage as i32 {
        if cmd.target.is_none() {
            return Err(ValidationError::MissingField(
                "target (required for ENGAGE command)".to_string(),
            ));
        }
        if let Some(ref target) = cmd.target {
            validate_target_designation(target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::v1::Timestamp;
    use crate::effector::v1::{
        effector_spec::Capacity, DispenserCapacity, EnergyCapacity, EngagementState, RoeStatus,
        SafetyState,
    };

    fn valid_kinetic_effector() -> EffectorSpec {
        EffectorSpec {
            effector_id: "m240-coax".to_string(),
            name: "M240 Coaxial Machine Gun".to_string(),
            effector_type: EffectorType::Kinetic as i32,
            category: EffectorCategory::Lethal as i32,
            effector_class: "7.62x51mm NATO".to_string(),
            max_range_m: 1800.0,
            min_range_m: 0.0,
            rate_of_fire: 650.0,
            safety_state: SafetyState::Safe as i32,
            interlocks: Some(SafetyInterlocks {
                master_arm_enabled: false,
                firing_circuit_ready: true,
                muzzle_clear: true,
                feed_ready: true,
                thermal_ok: true,
                authorization_valid: false,
                roe_compliant: true,
                engagement_zone_valid: false,
                friendly_clear: true,
                human_confirmed: false,
            }),
            capacity: Some(Capacity::Ammunition(AmmunitionStatus {
                rounds_ready: 200,
                rounds_total: 1000,
                magazine_capacity: 200,
                magazines_available: 5,
                ammunition_type: "7.62 NATO Ball".to_string(),
                reloading: false,
                reload_time_remaining_s: 0.0,
                malfunction: false,
                malfunction_detail: String::new(),
            })),
            engagement_state: EngagementState::Idle as i32,
            current_target: None,
            firing_solution: None,
            required_authorization: AuthorizationLevel::Commander as i32,
            current_authorization: None,
            roe_status: Some(RoeStatus {
                roe_id: "ROE-ALPHA-3".to_string(),
                roe_description: "Weapons tight".to_string(),
                weapons_status: "TIGHT".to_string(),
                engagement_authorized: false,
                denial_reason: String::new(),
                updated_at: None,
            }),
            mount_actuator_id: "turret-main".to_string(),
            updated_at: None,
            metadata_json: String::new(),
        }
    }

    fn valid_countermeasure_effector() -> EffectorSpec {
        EffectorSpec {
            effector_id: "smoke-l".to_string(),
            name: "Left Smoke Dispenser".to_string(),
            effector_type: EffectorType::Obscurant as i32,
            category: EffectorCategory::Defensive as i32,
            effector_class: "M18 Smoke".to_string(),
            max_range_m: 50.0,
            min_range_m: 5.0,
            rate_of_fire: 0.0,
            safety_state: SafetyState::Safe as i32,
            interlocks: None,
            capacity: Some(Capacity::Dispenser(DispenserCapacity {
                units_remaining: 4,
                total_capacity: 4,
                unit_type: "M18 Smoke Grenade".to_string(),
                ready: true,
            })),
            engagement_state: EngagementState::Idle as i32,
            current_target: None,
            firing_solution: None,
            required_authorization: AuthorizationLevel::Operator as i32,
            current_authorization: None,
            roe_status: None,
            mount_actuator_id: String::new(),
            updated_at: None,
            metadata_json: String::new(),
        }
    }

    #[test]
    fn test_valid_kinetic_effector() {
        let effector = valid_kinetic_effector();
        assert!(validate_effector_spec(&effector).is_ok());
    }

    #[test]
    fn test_valid_countermeasure_effector() {
        let effector = valid_countermeasure_effector();
        assert!(validate_effector_spec(&effector).is_ok());
    }

    #[test]
    fn test_missing_effector_id() {
        let mut effector = valid_kinetic_effector();
        effector.effector_id = String::new();
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "effector_id"));
    }

    #[test]
    fn test_missing_name() {
        let mut effector = valid_kinetic_effector();
        effector.name = String::new();
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "name"));
    }

    #[test]
    fn test_unspecified_effector_type() {
        let mut effector = valid_kinetic_effector();
        effector.effector_type = EffectorType::Unspecified as i32;
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_unspecified_category() {
        let mut effector = valid_kinetic_effector();
        effector.category = EffectorCategory::Unspecified as i32;
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_invalid_range_constraint() {
        let mut effector = valid_kinetic_effector();
        effector.min_range_m = 1000.0;
        effector.max_range_m = 500.0; // max < min
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }

    #[test]
    fn test_invalid_ammunition_rounds() {
        let mut effector = valid_kinetic_effector();
        effector.capacity = Some(Capacity::Ammunition(AmmunitionStatus {
            rounds_ready: 500, // > rounds_total
            rounds_total: 200,
            magazine_capacity: 100,
            magazines_available: 2,
            ammunition_type: "7.62 NATO".to_string(),
            reloading: false,
            reload_time_remaining_s: 0.0,
            malfunction: false,
            malfunction_detail: String::new(),
        }));
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }

    #[test]
    fn test_invalid_energy_charge_level() {
        let mut effector = valid_kinetic_effector();
        effector.capacity = Some(Capacity::Energy(EnergyCapacity {
            charge_level: 1.5, // > 1.0
            max_capacity: 1000.0,
            power_available_kw: 50.0,
            thermal_level: 0.3,
            charging: false,
            charge_time_remaining_s: 0.0,
            shots_remaining: 10,
        }));
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_invalid_dispenser_units() {
        let mut effector = valid_countermeasure_effector();
        effector.capacity = Some(Capacity::Dispenser(DispenserCapacity {
            units_remaining: 10, // > total_capacity
            total_capacity: 4,
            unit_type: "M18 Smoke".to_string(),
            ready: true,
        }));
        let err = validate_effector_spec(&effector).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }

    #[test]
    fn test_valid_effector_state_update() {
        let update = EffectorStateUpdate {
            platform_id: "IFV-Alpha-1".to_string(),
            effector: Some(valid_kinetic_effector()),
            status: EffectorStatus::Operational as i32,
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
        };
        assert!(validate_effector_state_update(&update).is_ok());
    }

    #[test]
    fn test_effector_update_missing_platform_id() {
        let update = EffectorStateUpdate {
            platform_id: String::new(),
            effector: Some(valid_kinetic_effector()),
            status: EffectorStatus::Operational as i32,
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
        };
        let err = validate_effector_state_update(&update).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "platform_id"));
    }

    #[test]
    fn test_effector_update_unspecified_status() {
        let update = EffectorStateUpdate {
            platform_id: "IFV-Alpha-1".to_string(),
            effector: Some(valid_kinetic_effector()),
            status: EffectorStatus::Unspecified as i32,
            timestamp: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
        };
        let err = validate_effector_state_update(&update).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_valid_safe_command() {
        let cmd = EffectorCommand {
            command_id: "CMD-001".to_string(),
            platform_id: "IFV-Alpha-1".to_string(),
            effector_id: "m240-coax".to_string(),
            command_type: EffectorCommandType::EffectorCommandSafe as i32,
            target: None,
            authorization: None, // Not required for SAFE command
            rounds_authorized: 0,
            issued_by: "operator-1".to_string(),
            priority: 1,
            issued_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            expires_at: None,
        };
        assert!(validate_effector_command(&cmd).is_ok());
    }

    #[test]
    fn test_arm_command_requires_authorization() {
        let cmd = EffectorCommand {
            command_id: "CMD-001".to_string(),
            platform_id: "IFV-Alpha-1".to_string(),
            effector_id: "m240-coax".to_string(),
            command_type: EffectorCommandType::EffectorCommandArm as i32,
            target: None,
            authorization: None, // Missing - required for ARM
            rounds_authorized: 0,
            issued_by: "operator-1".to_string(),
            priority: 1,
            issued_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            expires_at: None,
        };
        let err = validate_effector_command(&cmd).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(_)));
    }

    #[test]
    fn test_engage_command_requires_target() {
        let cmd = EffectorCommand {
            command_id: "CMD-001".to_string(),
            platform_id: "IFV-Alpha-1".to_string(),
            effector_id: "m240-coax".to_string(),
            command_type: EffectorCommandType::EffectorCommandEngage as i32,
            target: None, // Missing - required for ENGAGE
            authorization: Some(Authorization {
                authorization_id: "AUTH-001".to_string(),
                authorized_by: "commander-1".to_string(),
                level: AuthorizationLevel::Commander as i32,
                authorized_at: Some(Timestamp {
                    seconds: 1702000000,
                    nanos: 0,
                }),
                expires_at: None,
                authorized_target_classes: vec!["hostile_vehicle".to_string()],
                engagement_zone_id: String::new(),
                roe_reference: "ROE-ALPHA-3".to_string(),
                special_instructions: String::new(),
            }),
            rounds_authorized: 50,
            issued_by: "operator-1".to_string(),
            priority: 1,
            issued_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            expires_at: None,
        };
        let err = validate_effector_command(&cmd).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(_)));
    }

    #[test]
    fn test_effector_command_expires_before_issued() {
        let cmd = EffectorCommand {
            command_id: "CMD-001".to_string(),
            platform_id: "IFV-Alpha-1".to_string(),
            effector_id: "m240-coax".to_string(),
            command_type: EffectorCommandType::EffectorCommandSafe as i32,
            target: None,
            authorization: None,
            rounds_authorized: 0,
            issued_by: "operator-1".to_string(),
            priority: 1,
            issued_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            expires_at: Some(Timestamp {
                seconds: 1701000000, // Before issued_at
                nanos: 0,
            }),
        };
        let err = validate_effector_command(&cmd).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }
}
