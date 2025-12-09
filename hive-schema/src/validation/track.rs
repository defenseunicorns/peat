//! Track validators
//!
//! Validates Track and TrackUpdate messages for HIVE Protocol.

use super::{ValidationError, ValidationResult};
use crate::track::v1::{Track, TrackPosition, TrackUpdate, UpdateType};

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

#[cfg(test)]
mod tests {
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
