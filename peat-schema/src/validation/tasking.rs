//! Tasking validators (AI/ML Detection Tasks)
//!
//! Validates DetectionTask messages and related configuration for PEAT Protocol.

use super::{ValidationError, ValidationResult};
use crate::tasking::v1::{
    BatchingConfig, ChipoutConfig, DetectionFilter, DetectionTask, ProductDelivery, TaskControl,
    TaskControlAction, TaskPriority, TaskState, TaskStatistics, TaskStatus, TrackReportMode,
    TrackReportingConfig,
};

/// Validate a DetectionTask message
///
/// Validates:
/// - task_id is present
/// - name is present
/// - filter parameters are valid
/// - product delivery configuration is valid
/// - schedule is valid
pub fn validate_detection_task(task: &DetectionTask) -> ValidationResult<()> {
    // Check required fields
    if task.task_id.is_empty() {
        return Err(ValidationError::MissingField("task_id".to_string()));
    }

    if task.name.is_empty() {
        return Err(ValidationError::MissingField("name".to_string()));
    }

    // Priority must be specified
    if task.priority == TaskPriority::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "priority must be specified".to_string(),
        ));
    }

    // Timestamp is required
    if task.issued_at.is_none() {
        return Err(ValidationError::MissingField("issued_at".to_string()));
    }

    // issued_by is required
    if task.issued_by.is_empty() {
        return Err(ValidationError::MissingField("issued_by".to_string()));
    }

    // Validate filter if present
    if let Some(ref filter) = task.filter {
        validate_detection_filter(filter)?;
    }

    // Validate product delivery if present
    if let Some(ref delivery) = task.product_delivery {
        validate_product_delivery(delivery)?;
    }

    Ok(())
}

/// Validate DetectionFilter parameters
pub fn validate_detection_filter(filter: &DetectionFilter) -> ValidationResult<()> {
    // min_confidence must be in valid range
    if filter.min_confidence < 0.0 || filter.min_confidence > 1.0 {
        return Err(ValidationError::InvalidConfidence(filter.min_confidence));
    }

    // min_report_interval must be non-negative
    if filter.min_report_interval_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "min_report_interval_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate ProductDelivery configuration
pub fn validate_product_delivery(delivery: &ProductDelivery) -> ValidationResult<()> {
    // Validate chipout config if present
    if let Some(ref chipout) = delivery.chipout_config {
        validate_chipout_config(chipout)?;
    }

    // Validate track reporting config if present
    if let Some(ref track_reporting) = delivery.track_reporting {
        validate_track_reporting_config(track_reporting)?;
    }

    // Validate batching config if present
    if let Some(ref batching) = delivery.batching {
        validate_batching_config(batching)?;
    }

    Ok(())
}

/// Validate ChipoutConfig
pub fn validate_chipout_config(config: &ChipoutConfig) -> ValidationResult<()> {
    // JPEG quality should be in range 1-100
    if config.jpeg_quality > 100 {
        return Err(ValidationError::InvalidValue(format!(
            "jpeg_quality {} must be between 1 and 100",
            config.jpeg_quality
        )));
    }

    // Padding percent should be reasonable (0-100%)
    if config.padding_percent < 0.0 || config.padding_percent > 1.0 {
        return Err(ValidationError::InvalidValue(format!(
            "padding_percent {} must be between 0.0 and 1.0",
            config.padding_percent
        )));
    }

    // Full frame quality should also be valid
    if config.full_frame_jpeg_quality > 100 {
        return Err(ValidationError::InvalidValue(format!(
            "full_frame_jpeg_quality {} must be between 1 and 100",
            config.full_frame_jpeg_quality
        )));
    }

    Ok(())
}

/// Validate TrackReportingConfig
pub fn validate_track_reporting_config(config: &TrackReportingConfig) -> ValidationResult<()> {
    // Mode must be specified
    if config.mode == TrackReportMode::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "track reporting mode must be specified".to_string(),
        ));
    }

    // Position change threshold must be non-negative
    if config.min_position_change_m < 0.0 {
        return Err(ValidationError::InvalidValue(
            "min_position_change_m must be non-negative".to_string(),
        ));
    }

    // Confidence change threshold must be valid
    if config.min_confidence_change < 0.0 || config.min_confidence_change > 1.0 {
        return Err(ValidationError::InvalidConfidence(
            config.min_confidence_change,
        ));
    }

    // Max report interval must be non-negative
    if config.max_report_interval_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "max_report_interval_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate BatchingConfig
pub fn validate_batching_config(config: &BatchingConfig) -> ValidationResult<()> {
    // Max batch delay must be non-negative
    if config.max_batch_delay_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "max_batch_delay_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate TaskStatus message
pub fn validate_task_status(status: &TaskStatus) -> ValidationResult<()> {
    // task_id is required
    if status.task_id.is_empty() {
        return Err(ValidationError::MissingField("task_id".to_string()));
    }

    // platform_id is required
    if status.platform_id.is_empty() {
        return Err(ValidationError::MissingField("platform_id".to_string()));
    }

    // State must be specified
    if status.state == TaskState::Unspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "task state must be specified".to_string(),
        ));
    }

    // Timestamp is required
    if status.updated_at.is_none() {
        return Err(ValidationError::MissingField("updated_at".to_string()));
    }

    // Validate statistics if present
    if let Some(ref stats) = status.statistics {
        validate_task_statistics(stats)?;
    }

    Ok(())
}

/// Validate TaskStatistics
pub fn validate_task_statistics(stats: &TaskStatistics) -> ValidationResult<()> {
    // reported_detections should not exceed total_detections
    if stats.reported_detections > stats.total_detections {
        return Err(ValidationError::ConstraintViolation(
            "reported_detections cannot exceed total_detections".to_string(),
        ));
    }

    // Average values must be non-negative
    if stats.avg_inference_time_ms < 0.0 {
        return Err(ValidationError::InvalidValue(
            "avg_inference_time_ms must be non-negative".to_string(),
        ));
    }

    if stats.avg_fps < 0.0 {
        return Err(ValidationError::InvalidValue(
            "avg_fps must be non-negative".to_string(),
        ));
    }

    if stats.uptime_s < 0.0 {
        return Err(ValidationError::InvalidValue(
            "uptime_s must be non-negative".to_string(),
        ));
    }

    Ok(())
}

/// Validate TaskControl message
pub fn validate_task_control(control: &TaskControl) -> ValidationResult<()> {
    // task_id is required
    if control.task_id.is_empty() {
        return Err(ValidationError::MissingField("task_id".to_string()));
    }

    // Action must be specified
    if control.action == TaskControlAction::TaskControlUnspecified as i32 {
        return Err(ValidationError::InvalidValue(
            "task control action must be specified".to_string(),
        ));
    }

    // issued_by is required
    if control.issued_by.is_empty() {
        return Err(ValidationError::MissingField("issued_by".to_string()));
    }

    // Timestamp is required
    if control.issued_at.is_none() {
        return Err(ValidationError::MissingField("issued_at".to_string()));
    }

    // UPDATE action requires updated_task
    if control.action == TaskControlAction::TaskControlUpdate as i32 {
        if control.updated_task.is_none() {
            return Err(ValidationError::MissingField(
                "updated_task (required for UPDATE action)".to_string(),
            ));
        }
        // Validate the updated task
        if let Some(ref task) = control.updated_task {
            validate_detection_task(task)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::v1::Timestamp;

    fn valid_detection_task() -> DetectionTask {
        DetectionTask {
            task_id: "TASK-001".to_string(),
            name: "Maritime Detection".to_string(),
            description: "Detect boats in harbor area".to_string(),
            target_classes: vec!["boat".to_string(), "person".to_string()],
            filter: Some(DetectionFilter {
                min_confidence: 0.7,
                priority_classes: vec!["boat".to_string()],
                ignore_classes: vec![],
                min_bbox_area: 100,
                max_detections_per_frame: 10,
                min_report_interval_s: 1.0,
            }),
            product_delivery: None,
            area_of_interest: None,
            schedule: None,
            priority: TaskPriority::Normal as i32,
            issued_by: "C2-WebTAK".to_string(),
            issued_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
            target_platforms: vec![],
        }
    }

    #[test]
    fn test_valid_detection_task() {
        let task = valid_detection_task();
        assert!(validate_detection_task(&task).is_ok());
    }

    #[test]
    fn test_missing_task_id() {
        let mut task = valid_detection_task();
        task.task_id = String::new();
        let err = validate_detection_task(&task).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "task_id"));
    }

    #[test]
    fn test_missing_name() {
        let mut task = valid_detection_task();
        task.name = String::new();
        let err = validate_detection_task(&task).unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(f) if f == "name"));
    }

    #[test]
    fn test_unspecified_priority() {
        let mut task = valid_detection_task();
        task.priority = TaskPriority::Unspecified as i32;
        let err = validate_detection_task(&task).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidValue(_)));
    }

    #[test]
    fn test_invalid_confidence_filter() {
        let mut task = valid_detection_task();
        task.filter = Some(DetectionFilter {
            min_confidence: 1.5, // Invalid
            ..Default::default()
        });
        let err = validate_detection_task(&task).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidConfidence(_)));
    }

    #[test]
    fn test_valid_task_status() {
        let status = TaskStatus {
            task_id: "TASK-001".to_string(),
            platform_id: "Alpha-3".to_string(),
            state: TaskState::Active as i32,
            statistics: Some(TaskStatistics {
                frames_processed: 1000,
                total_detections: 50,
                reported_detections: 45,
                tracks_created: 10,
                tracks_active: 5,
                chipouts_generated: 20,
                products_sent: 65,
                avg_inference_time_ms: 25.0,
                avg_fps: 30.0,
                uptime_s: 3600.0,
            }),
            error_message: String::new(),
            updated_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
        };
        assert!(validate_task_status(&status).is_ok());
    }

    #[test]
    fn test_invalid_statistics() {
        let status = TaskStatus {
            task_id: "TASK-001".to_string(),
            platform_id: "Alpha-3".to_string(),
            state: TaskState::Active as i32,
            statistics: Some(TaskStatistics {
                frames_processed: 1000,
                total_detections: 50,
                reported_detections: 100, // Invalid: exceeds total
                ..Default::default()
            }),
            error_message: String::new(),
            updated_at: Some(Timestamp {
                seconds: 1702000000,
                nanos: 0,
            }),
        };
        let err = validate_task_status(&status).unwrap_err();
        assert!(matches!(err, ValidationError::ConstraintViolation(_)));
    }
}
