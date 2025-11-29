//! HIVE FFI - Foreign Function Interface for Kotlin/Swift
//!
//! This crate provides UniFFI bindings to expose HIVE functionality
//! to Kotlin (Android/ATAK) and Swift (iOS) applications.
//!
//! Uses proc-macro only UniFFI approach (no UDL file).

use std::collections::HashMap;

use hive_protocol::cot::{
    CotEncoder, Position as CotPosition, TrackUpdate, Velocity as CotVelocity,
};

// Setup UniFFI scaffolding
uniffi::setup_scaffolding!();

/// Get the HIVE library version
#[uniffi::export]
pub fn hive_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Geographic position for FFI
#[derive(Debug, Clone, uniffi::Record)]
pub struct Position {
    /// Latitude in degrees (WGS84)
    pub lat: f64,
    /// Longitude in degrees (WGS84)
    pub lon: f64,
    /// Height Above Ellipsoid in meters (optional)
    pub hae: Option<f64>,
}

/// Velocity vector for FFI
#[derive(Debug, Clone, uniffi::Record)]
pub struct Velocity {
    /// Bearing in degrees (0 = North, clockwise)
    pub bearing: f64,
    /// Speed in meters per second
    pub speed_mps: f64,
}

/// Track data for CoT encoding
#[derive(Debug, Clone, uniffi::Record)]
pub struct TrackData {
    /// Unique track identifier
    pub track_id: String,
    /// Source platform ID
    pub source_platform: String,
    /// Geographic position
    pub position: Position,
    /// Optional velocity
    pub velocity: Option<Velocity>,
    /// MIL-STD-2525 classification (e.g., "a-f-G-U-C")
    pub classification: String,
    /// Detection confidence (0.0 - 1.0)
    pub confidence: f64,
    /// Optional cell ID (for squad-level tracks)
    pub cell_id: Option<String>,
    /// Optional formation ID
    pub formation_id: Option<String>,
}

/// FFI Error type
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HiveError {
    #[error("Encoding error: {msg}")]
    EncodingError { msg: String },
    #[error("Invalid input: {msg}")]
    InvalidInput { msg: String },
}

/// Encode a track to CoT XML string
#[uniffi::export]
pub fn encode_track_to_cot(track: TrackData) -> Result<String, HiveError> {
    // Validate input
    if track.track_id.is_empty() {
        return Err(HiveError::InvalidInput {
            msg: "track_id cannot be empty".to_string(),
        });
    }

    // Convert FFI types to internal types
    let position = CotPosition {
        lat: track.position.lat,
        lon: track.position.lon,
        cep_m: None,
        hae: track.position.hae,
    };

    let velocity = track.velocity.map(|v| CotVelocity {
        bearing: v.bearing,
        speed_mps: v.speed_mps,
    });

    let track_update = TrackUpdate {
        track_id: track.track_id,
        source_platform: track.source_platform,
        source_model: "hive-ffi".to_string(),
        model_version: hive_version(),
        cell_id: track.cell_id,
        formation_id: track.formation_id,
        timestamp: chrono::Utc::now(),
        position,
        velocity,
        classification: track.classification,
        confidence: track.confidence,
        attributes: HashMap::new(),
    };

    // Encode to CoT
    let encoder = CotEncoder::new();
    let event = encoder
        .track_update_to_event(&track_update)
        .map_err(|e| HiveError::EncodingError { msg: e.to_string() })?;

    event
        .to_xml()
        .map_err(|e| HiveError::EncodingError { msg: e.to_string() })
}

/// Create a position from coordinates
#[uniffi::export]
pub fn create_position(lat: f64, lon: f64, hae: Option<f64>) -> Position {
    Position { lat, lon, hae }
}

/// Create a velocity from bearing and speed
#[uniffi::export]
pub fn create_velocity(bearing: f64, speed_mps: f64) -> Velocity {
    Velocity { bearing, speed_mps }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hive_version() {
        let version = hive_version();
        assert!(!version.is_empty());
        assert!(version.contains('.'));
    }

    #[test]
    fn test_encode_track() {
        let track = TrackData {
            track_id: "track-001".to_string(),
            source_platform: "platform-1".to_string(),
            position: Position {
                lat: 34.0522,
                lon: -118.2437,
                hae: Some(100.0),
            },
            velocity: Some(Velocity {
                bearing: 90.0,
                speed_mps: 10.0,
            }),
            classification: "a-f-G-U-C".to_string(),
            confidence: 0.95,
            cell_id: Some("cell-1".to_string()),
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_ok());

        let xml = result.unwrap();
        assert!(xml.contains("<event"));
        assert!(xml.contains("track-001"));
    }

    #[test]
    fn test_encode_minimal_track() {
        let track = TrackData {
            track_id: "t1".to_string(),
            source_platform: "p1".to_string(),
            position: Position {
                lat: 0.0,
                lon: 0.0,
                hae: None,
            },
            velocity: None,
            classification: "a-u-G".to_string(),
            confidence: 0.5,
            cell_id: None,
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_track_id() {
        let track = TrackData {
            track_id: "".to_string(), // Empty - should fail
            source_platform: "p1".to_string(),
            position: Position {
                lat: 0.0,
                lon: 0.0,
                hae: None,
            },
            velocity: None,
            classification: "a-u-G".to_string(),
            confidence: 0.5,
            cell_id: None,
            formation_id: None,
        };

        let result = encode_track_to_cot(track);
        assert!(result.is_err());
    }

    #[test]
    fn test_helper_functions() {
        let pos = create_position(34.0, -118.0, Some(50.0));
        assert_eq!(pos.lat, 34.0);
        assert_eq!(pos.lon, -118.0);
        assert_eq!(pos.hae, Some(50.0));

        let vel = create_velocity(45.0, 15.0);
        assert_eq!(vel.bearing, 45.0);
        assert_eq!(vel.speed_mps, 15.0);
    }
}
