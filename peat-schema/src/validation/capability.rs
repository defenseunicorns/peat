//! CapabilityAdvertisement validators
//!
//! Validates capability advertisement messages for Peat Protocol.

use super::{ValidationError, ValidationResult};
use crate::capability::v1::{CapabilityAdvertisement, OperationalStatus, ResourceStatus};
use crate::validation::core::validate_capability;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::v1::{Capability, CapabilityType};
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
