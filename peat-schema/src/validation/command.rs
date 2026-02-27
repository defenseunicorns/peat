//! HierarchicalCommand validators
//!
//! Validates command messages (MissionTask) for PEAT Protocol.

use super::{ValidationError, ValidationResult};
use crate::command::v1::HierarchicalCommand;

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
                crate::command::v1::hierarchical_command::CommandType::MissionOrder(MissionOrder {
                    mission_type: MissionType::Isr as i32,
                    mission_id: "ISR-001".to_string(),
                    description: "Conduct ISR in sector Alpha".to_string(),
                    objective_location: None,
                    start_time: None,
                    end_time: None,
                    roe: None,
                }),
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
