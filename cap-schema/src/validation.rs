//! Schema validation utilities
//!
//! This module provides validation functions for CAP Protocol messages to ensure:
//! - Confidence scores are within valid range (0.0 - 1.0)
//! - Required fields are present
//! - Semantic constraints are satisfied
//! - CRDT invariants are maintained

use crate::capability::v1::Capability;
use crate::cell::v1::{CellConfig, CellState};
use crate::node::v1::{NodeConfig, NodeState};

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
}
