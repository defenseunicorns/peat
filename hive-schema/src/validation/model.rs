//! Model deployment validators
//!
//! Validates ModelDeployment and ModelDeploymentStatus messages for HIVE Protocol.

use super::{ValidationError, ValidationResult};
use crate::model::v1::{
    DeploymentPolicy, DeploymentPriority, DeploymentState, ModelDeployment, ModelDeploymentStatus,
    ModelType,
};

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

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_valid_deployment_status() {
        let status = valid_deployment_status();
        assert!(validate_model_deployment_status(&status).is_ok());
    }

    #[test]
    fn test_status_missing_deployment_id() {
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
