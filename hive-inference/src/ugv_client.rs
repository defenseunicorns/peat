//! Simulated UGV Client for HIVE Demo
//!
//! Provides a simulated Unmanned Ground Vehicle that:
//! - Publishes position updates as TrackUpdate documents
//! - Responds to MissionTask commands (TRACK_TARGET, SEARCH_AREA, etc.)
//! - Advertises capabilities to the HIVE network
//! - Simulates movement patterns (waypoint patrol, pursuit, random walk)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::ugv_client::{UgvClient, UgvConfig, MovementMode};
//!
//! // Create UGV client
//! let config = UgvConfig::new("UGV-Alpha-1")
//!     .with_position(33.7749, -84.3958)
//!     .with_waypoints(vec![
//!         (33.7750, -84.3960),
//!         (33.7755, -84.3955),
//!     ]);
//!
//! let mut ugv = UgvClient::new(config);
//!
//! // Simulate movement
//! ugv.update(Duration::from_millis(100)).await?;
//!
//! // Get position as TrackUpdate
//! let track = ugv.get_position_update();
//! ```

use crate::messages::{OperationalStatus, Position, TrackUpdate, Velocity};
use crate::platform::SensorCapability;
use chrono::{DateTime, Utc};
use hive_protocol::models::{Capability, CapabilityExt, CapabilityType};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// UGV State Machine
// ============================================================================

/// State of the UGV
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UgvState {
    /// UGV is idle at a position
    Idle,
    /// UGV is moving to a waypoint
    Moving,
    /// UGV is patrolling an area
    Patrolling,
    /// UGV is tracking/pursuing a target
    Tracking,
    /// UGV is monitoring a zone (stationary with camera panning)
    Monitoring,
    /// UGV is returning to base
    ReturningToBase,
    /// UGV is in error state
    Error,
}

impl std::fmt::Display for UgvState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UgvState::Idle => write!(f, "IDLE"),
            UgvState::Moving => write!(f, "MOVING"),
            UgvState::Patrolling => write!(f, "PATROLLING"),
            UgvState::Tracking => write!(f, "TRACKING"),
            UgvState::Monitoring => write!(f, "MONITORING"),
            UgvState::ReturningToBase => write!(f, "RETURNING"),
            UgvState::Error => write!(f, "ERROR"),
        }
    }
}

// ============================================================================
// Movement Modes
// ============================================================================

/// Movement mode for simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MovementMode {
    /// Follow predefined GPS waypoints in order
    Waypoint {
        waypoints: Vec<(f64, f64)>,
        current_index: usize,
        loop_patrol: bool,
    },
    /// Move randomly within a geofenced area
    RandomWalk {
        center: (f64, f64),
        radius_m: f64,
        next_target: Option<(f64, f64)>,
    },
    /// Pursue a target position (follow detected tracks)
    Pursuit {
        target_position: (f64, f64),
        follow_distance_m: f64,
    },
    /// Hold position (for monitoring)
    Stationary,
}

// ============================================================================
// Mission Types (from Issue #331)
// ============================================================================

/// Mission command types for UGV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MissionCommand {
    /// Track a target at given position
    TrackTarget {
        target_id: String,
        last_known_position: (f64, f64),
    },
    /// Search/patrol an area defined by waypoints
    SearchArea {
        boundary: Vec<(f64, f64)>,
        patrol_pattern: PatrolPattern,
    },
    /// Monitor a zone from a fixed position
    MonitorZone { center: (f64, f64), radius_m: f64 },
    /// Abort current mission and return to base
    Abort,
    /// Move to specific waypoint
    MoveTo { position: (f64, f64) },
}

/// Patrol pattern for search missions
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PatrolPattern {
    /// Sequential waypoint traversal
    Sequential,
    /// Random waypoint selection
    Random,
    /// Lawn mower pattern
    LawnMower,
}

// ============================================================================
// UGV Configuration
// ============================================================================

/// Configuration for UGV client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UgvConfig {
    /// Platform identifier
    pub platform_id: String,
    /// Human-readable name
    pub name: String,
    /// Initial position (lat, lon)
    pub initial_position: (f64, f64),
    /// Base/home position for RTB
    pub base_position: (f64, f64),
    /// Maximum speed in m/s
    pub max_speed_mps: f64,
    /// Position update interval
    pub update_interval: Duration,
    /// Geofence boundary (if any)
    pub geofence: Option<Vec<(f64, f64)>>,
    /// Waypoints for patrol
    pub waypoints: Vec<(f64, f64)>,
    /// Camera sensor spec
    pub camera: Option<SensorCapability>,
}

impl UgvConfig {
    /// Create a new UGV configuration
    pub fn new(platform_id: impl Into<String>) -> Self {
        let id = platform_id.into();
        Self {
            platform_id: id.clone(),
            name: id,
            initial_position: (0.0, 0.0),
            base_position: (0.0, 0.0),
            max_speed_mps: 5.0, // 5 m/s typical UGV speed
            update_interval: Duration::from_millis(500),
            geofence: None,
            waypoints: Vec::new(),
            camera: Some(SensorCapability::camera("1920x1080", 60.0, 30.0)),
        }
    }

    /// Set initial position
    pub fn with_position(mut self, lat: f64, lon: f64) -> Self {
        self.initial_position = (lat, lon);
        self.base_position = (lat, lon);
        self
    }

    /// Set base position (may differ from initial)
    pub fn with_base(mut self, lat: f64, lon: f64) -> Self {
        self.base_position = (lat, lon);
        self
    }

    /// Add patrol waypoints
    pub fn with_waypoints(mut self, waypoints: Vec<(f64, f64)>) -> Self {
        self.waypoints = waypoints;
        self
    }

    /// Set maximum speed
    pub fn with_speed(mut self, speed_mps: f64) -> Self {
        self.max_speed_mps = speed_mps;
        self
    }

    /// Set geofence boundary
    pub fn with_geofence(mut self, boundary: Vec<(f64, f64)>) -> Self {
        self.geofence = Some(boundary);
        self
    }
}

// ============================================================================
// UGV Client
// ============================================================================

/// Simulated UGV Client
///
/// Implements the simulated UGV behavior for the M1 vignette demo.
pub struct UgvClient {
    /// Configuration
    config: UgvConfig,
    /// Current state
    state: UgvState,
    /// Current position (lat, lon)
    position: (f64, f64),
    /// Current heading in degrees (0 = North, 90 = East)
    heading: f64,
    /// Current speed in m/s
    speed: f64,
    /// Movement mode
    movement_mode: MovementMode,
    /// Current mission (if any)
    current_mission: Option<MissionCommand>,
    /// Battery level (0.0 - 1.0)
    battery_level: f64,
    /// Operational status
    status: OperationalStatus,
    /// Track ID for position updates
    track_id: String,
    /// Last update timestamp
    last_update: DateTime<Utc>,
    /// Position update counter
    update_count: u64,
}

impl UgvClient {
    /// Create a new UGV client
    pub fn new(config: UgvConfig) -> Self {
        let track_id = format!("UGV-{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        info!(
            "Creating UGV client '{}' at ({:.4}, {:.4})",
            config.platform_id, config.initial_position.0, config.initial_position.1
        );

        Self {
            position: config.initial_position,
            config,
            state: UgvState::Idle,
            heading: 0.0,
            speed: 0.0,
            movement_mode: MovementMode::Stationary,
            current_mission: None,
            battery_level: 1.0,
            status: OperationalStatus::Ready,
            track_id,
            last_update: Utc::now(),
            update_count: 0,
        }
    }

    /// Get the platform ID
    pub fn platform_id(&self) -> &str {
        &self.config.platform_id
    }

    /// Get current state
    pub fn state(&self) -> UgvState {
        self.state
    }

    /// Get current position
    pub fn position(&self) -> (f64, f64) {
        self.position
    }

    /// Get current heading
    pub fn heading(&self) -> f64 {
        self.heading
    }

    /// Get battery level
    pub fn battery_level(&self) -> f64 {
        self.battery_level
    }

    /// Handle a mission command
    pub fn handle_mission(&mut self, command: MissionCommand) {
        info!(
            "UGV '{}' received mission: {:?}",
            self.config.platform_id, command
        );

        match &command {
            MissionCommand::TrackTarget {
                target_id,
                last_known_position,
            } => {
                self.state = UgvState::Tracking;
                self.movement_mode = MovementMode::Pursuit {
                    target_position: *last_known_position,
                    follow_distance_m: 10.0,
                };
                self.speed = self.config.max_speed_mps;
                info!(
                    "UGV tracking target '{}' at ({:.4}, {:.4})",
                    target_id, last_known_position.0, last_known_position.1
                );
            }
            MissionCommand::SearchArea {
                boundary,
                patrol_pattern,
            } => {
                self.state = UgvState::Patrolling;
                self.movement_mode = MovementMode::Waypoint {
                    waypoints: boundary.clone(),
                    current_index: 0,
                    loop_patrol: true,
                };
                self.speed = self.config.max_speed_mps * 0.7; // Slower patrol speed
                info!(
                    "UGV patrolling area with {} waypoints, pattern: {:?}",
                    boundary.len(),
                    patrol_pattern
                );
            }
            MissionCommand::MonitorZone { center, radius_m } => {
                self.state = UgvState::Monitoring;
                self.movement_mode = MovementMode::Stationary;
                // Move to center first if not there
                let dist = haversine_distance(self.position, *center);
                if dist > 5.0 {
                    self.state = UgvState::Moving;
                    self.movement_mode = MovementMode::Pursuit {
                        target_position: *center,
                        follow_distance_m: 0.0,
                    };
                }
                info!(
                    "UGV monitoring zone at ({:.4}, {:.4}) radius {}m",
                    center.0, center.1, radius_m
                );
            }
            MissionCommand::Abort => {
                self.state = UgvState::ReturningToBase;
                self.movement_mode = MovementMode::Pursuit {
                    target_position: self.config.base_position,
                    follow_distance_m: 0.0,
                };
                self.speed = self.config.max_speed_mps;
                info!(
                    "UGV aborting mission, returning to base at ({:.4}, {:.4})",
                    self.config.base_position.0, self.config.base_position.1
                );
            }
            MissionCommand::MoveTo { position } => {
                self.state = UgvState::Moving;
                self.movement_mode = MovementMode::Pursuit {
                    target_position: *position,
                    follow_distance_m: 0.0,
                };
                self.speed = self.config.max_speed_mps;
                info!("UGV moving to ({:.4}, {:.4})", position.0, position.1);
            }
        }

        self.current_mission = Some(command);
    }

    /// Update target position for pursuit mode
    pub fn update_target_position(&mut self, position: (f64, f64)) {
        if let MovementMode::Pursuit {
            ref mut target_position,
            ..
        } = self.movement_mode
        {
            *target_position = position;
            debug!(
                "UGV target position updated to ({:.4}, {:.4})",
                position.0, position.1
            );
        }
    }

    /// Simulate one time step
    pub fn update(&mut self, dt: Duration) {
        let dt_secs = dt.as_secs_f64();

        // Drain battery slightly
        self.battery_level = (self.battery_level - dt_secs * 0.0001).max(0.0);
        if self.battery_level < 0.1 {
            self.status = OperationalStatus::Degraded;
            if self.battery_level < 0.05 {
                warn!("UGV '{}' battery critical!", self.config.platform_id);
            }
        }

        match &mut self.movement_mode {
            MovementMode::Stationary => {
                self.speed = 0.0;
            }
            MovementMode::Pursuit {
                target_position,
                follow_distance_m,
            } => {
                let dist = haversine_distance(self.position, *target_position);
                if dist > *follow_distance_m + 1.0 {
                    // Move toward target
                    self.heading = bearing(self.position, *target_position);
                    self.speed = self.config.max_speed_mps.min(dist / dt_secs);
                    self.move_forward(dt_secs);
                } else {
                    // Reached target
                    self.speed = 0.0;
                    if self.state == UgvState::Moving || self.state == UgvState::ReturningToBase {
                        self.state = UgvState::Idle;
                        self.movement_mode = MovementMode::Stationary;
                        info!("UGV '{}' reached destination", self.config.platform_id);
                    }
                }
            }
            MovementMode::Waypoint {
                waypoints,
                current_index,
                loop_patrol,
            } => {
                if waypoints.is_empty() {
                    self.speed = 0.0;
                    return;
                }

                let target = waypoints[*current_index];
                let dist = haversine_distance(self.position, target);

                if dist < 5.0 {
                    // Reached waypoint, go to next
                    *current_index += 1;
                    if *current_index >= waypoints.len() {
                        if *loop_patrol {
                            *current_index = 0;
                        } else {
                            self.state = UgvState::Idle;
                            self.movement_mode = MovementMode::Stationary;
                            self.speed = 0.0;
                            return;
                        }
                    }
                    debug!(
                        "UGV '{}' reached waypoint, moving to next ({})",
                        self.config.platform_id, *current_index
                    );
                }

                let target = waypoints[*current_index];
                self.heading = bearing(self.position, target);
                self.move_forward(dt_secs);
            }
            MovementMode::RandomWalk {
                center,
                radius_m,
                next_target,
            } => {
                // Pick a random target if none
                if next_target.is_none() {
                    let mut rng = rand::rng();
                    let angle = rng.random_range(0.0..2.0 * PI);
                    let dist = rng.random_range(0.0..*radius_m);
                    let new_target = offset_position(*center, angle.to_degrees(), dist);
                    *next_target = Some(new_target);
                }

                if let Some(target) = next_target {
                    let dist = haversine_distance(self.position, *target);
                    if dist < 5.0 {
                        // Reached target, pick new one
                        *next_target = None;
                    } else {
                        self.heading = bearing(self.position, *target);
                        self.move_forward(dt_secs);
                    }
                }
            }
        }

        self.last_update = Utc::now();
        self.update_count += 1;
    }

    /// Move forward based on current heading and speed
    fn move_forward(&mut self, dt_secs: f64) {
        let distance = self.speed * dt_secs;
        self.position = offset_position(self.position, self.heading, distance);
    }

    /// Generate a TrackUpdate for the UGV's current position
    pub fn get_position_update(&self) -> TrackUpdate {
        TrackUpdate {
            track_id: self.track_id.clone(),
            classification: "ugv".to_string(),
            confidence: 1.0, // Self-reported position is certain
            position: Position {
                lat: self.position.0,
                lon: self.position.1,
                cep_m: Some(1.0), // GPS accuracy
                hae: Some(0.0),   // Ground level
            },
            velocity: if self.speed > 0.1 {
                Some(Velocity {
                    bearing: self.heading,
                    speed_mps: self.speed,
                })
            } else {
                None
            },
            attributes: {
                let mut attrs = HashMap::new();
                attrs.insert(
                    "state".to_string(),
                    serde_json::json!(self.state.to_string()),
                );
                attrs.insert(
                    "battery_level".to_string(),
                    serde_json::json!(self.battery_level),
                );
                attrs.insert("platform_type".to_string(), serde_json::json!("UGV"));
                attrs
            },
            source_platform: self.config.platform_id.clone(),
            source_model: "self-report".to_string(),
            model_version: "1.0.0".to_string(),
            timestamp: Utc::now(),
            latest_chipout_id: None,
        }
    }

    /// Get capabilities as hive-protocol Capability objects
    pub fn get_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        // Mobility capability
        let mut mobility_cap = Capability::new(
            format!("{}-mobility", self.config.platform_id),
            "Ground Mobility".to_string(),
            CapabilityType::Mobility,
            self.battery_level as f32 * 0.95,
        );
        mobility_cap.metadata_json = serde_json::json!({
            "max_speed_mps": self.config.max_speed_mps,
            "vehicle_type": "UGV",
            "battery_level": self.battery_level
        })
        .to_string();
        caps.push(mobility_cap);

        // Navigation capability
        let nav_cap = Capability::new(
            format!("{}-nav", self.config.platform_id),
            "GPS Navigation".to_string(),
            CapabilityType::Sensor,
            0.98,
        );
        caps.push(nav_cap);

        // Camera if present
        if let Some(camera) = &self.config.camera {
            let mut cam_cap = Capability::new(
                format!("{}-camera", self.config.platform_id),
                "PTZ Camera".to_string(),
                CapabilityType::Sensor,
                0.95,
            );
            cam_cap.metadata_json = serde_json::json!({
                "resolution": camera.resolution,
                "fov_degrees": camera.fov_degrees,
                "frame_rate": camera.frame_rate
            })
            .to_string();
            caps.push(cam_cap);
        }

        caps
    }
}

// ============================================================================
// Geographic Utilities
// ============================================================================

/// Calculate haversine distance between two points in meters
fn haversine_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();
    let dlat = (p2.0 - p1.0).to_radians();
    let dlon = (p2.1 - p1.1).to_radians();

    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

/// Calculate bearing from p1 to p2 in degrees (0 = North, 90 = East)
fn bearing(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();
    let dlon = (p2.1 - p1.1).to_radians();

    let x = dlon.sin() * lat2.cos();
    let y = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();

    let bearing_rad = x.atan2(y);
    (bearing_rad.to_degrees() + 360.0) % 360.0
}

/// Offset a position by a bearing and distance
fn offset_position(start: (f64, f64), bearing_deg: f64, distance_m: f64) -> (f64, f64) {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1 = start.0.to_radians();
    let lon1 = start.1.to_radians();
    let bearing = bearing_deg.to_radians();
    let d = distance_m / EARTH_RADIUS_M;

    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * bearing.cos()).asin();
    let lon2 =
        lon1 + (bearing.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());

    (lat2.to_degrees(), lon2.to_degrees())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ugv_creation() {
        let config = UgvConfig::new("UGV-Test-1").with_position(33.7749, -84.3958);
        let ugv = UgvClient::new(config);

        assert_eq!(ugv.platform_id(), "UGV-Test-1");
        assert_eq!(ugv.state(), UgvState::Idle);
        assert!((ugv.position().0 - 33.7749).abs() < 0.0001);
        assert!((ugv.position().1 - (-84.3958)).abs() < 0.0001);
    }

    #[test]
    fn test_haversine_distance() {
        // Atlanta to nearby point (~111m per degree latitude)
        let p1 = (33.7749, -84.3958);
        let p2 = (33.7759, -84.3958); // ~0.001 degrees north

        let dist = haversine_distance(p1, p2);
        assert!(dist > 100.0 && dist < 120.0); // Should be ~111m
    }

    #[test]
    fn test_bearing_calculation() {
        let p1 = (33.7749, -84.3958);
        let p2 = (33.7759, -84.3958); // Due north

        let brg = bearing(p1, p2);
        assert!(!(5.0..=355.0).contains(&brg)); // Should be ~0 (North)
    }

    #[test]
    fn test_mission_track_target() {
        let config = UgvConfig::new("UGV-Test-1").with_position(33.7749, -84.3958);
        let mut ugv = UgvClient::new(config);

        ugv.handle_mission(MissionCommand::TrackTarget {
            target_id: "TRK-001".to_string(),
            last_known_position: (33.7760, -84.3950),
        });

        assert_eq!(ugv.state(), UgvState::Tracking);
        assert!(ugv.speed > 0.0);
    }

    #[test]
    fn test_mission_abort() {
        let config = UgvConfig::new("UGV-Test-1")
            .with_position(33.7749, -84.3958)
            .with_base(33.7740, -84.3960);
        let mut ugv = UgvClient::new(config);

        ugv.handle_mission(MissionCommand::Abort);

        assert_eq!(ugv.state(), UgvState::ReturningToBase);
    }

    #[test]
    fn test_position_update_generation() {
        let config = UgvConfig::new("UGV-Test-1").with_position(33.7749, -84.3958);
        let ugv = UgvClient::new(config);

        let track = ugv.get_position_update();

        assert_eq!(track.classification, "ugv");
        assert!((track.confidence - 1.0).abs() < 0.001);
        assert!((track.position.lat - 33.7749).abs() < 0.0001);
        assert_eq!(track.source_platform, "UGV-Test-1");
    }

    #[test]
    fn test_capabilities() {
        let config = UgvConfig::new("UGV-Test-1").with_position(33.7749, -84.3958);
        let ugv = UgvClient::new(config);

        let caps = ugv.get_capabilities();

        assert!(caps.len() >= 2); // At least mobility and nav
        assert!(caps.iter().any(|c| c.name == "Ground Mobility"));
        assert!(caps.iter().any(|c| c.name == "GPS Navigation"));
    }

    #[test]
    fn test_movement_simulation() {
        let config = UgvConfig::new("UGV-Test-1")
            .with_position(33.7749, -84.3958)
            .with_speed(10.0);
        let mut ugv = UgvClient::new(config);

        // Command to move somewhere
        ugv.handle_mission(MissionCommand::MoveTo {
            position: (33.7760, -84.3958),
        });

        // Simulate several updates
        for _ in 0..10 {
            ugv.update(Duration::from_millis(100));
        }

        // Should have moved from initial position
        let dist_from_start = haversine_distance((33.7749, -84.3958), ugv.position());
        assert!(dist_from_start > 5.0); // Should have moved at least 5m
    }

    #[test]
    fn test_waypoint_patrol() {
        let waypoints = vec![
            (33.7749, -84.3958),
            (33.7755, -84.3955),
            (33.7760, -84.3960),
        ];

        let config = UgvConfig::new("UGV-Test-1")
            .with_position(33.7749, -84.3958)
            .with_waypoints(waypoints.clone());
        let mut ugv = UgvClient::new(config);

        ugv.handle_mission(MissionCommand::SearchArea {
            boundary: waypoints,
            patrol_pattern: PatrolPattern::Sequential,
        });

        assert_eq!(ugv.state(), UgvState::Patrolling);
    }
}
