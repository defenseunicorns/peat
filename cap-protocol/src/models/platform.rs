//! Platform state data structures

use crate::models::Capability;
use crate::traits::Phase;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Platform static configuration (immutable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Unique platform identifier
    pub id: String,
    /// Platform type (UAV, UGV, etc.)
    pub platform_type: String,
    /// Static capabilities
    pub capabilities: Vec<Capability>,
    /// Maximum communication range in meters
    pub comm_range_m: f32,
    /// Maximum speed in m/s
    pub max_speed_mps: f32,
}

impl PlatformConfig {
    /// Create a new platform configuration
    pub fn new(platform_type: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            platform_type,
            capabilities: Vec::new(),
            comm_range_m: 1000.0,
            max_speed_mps: 10.0,
        }
    }
}

/// Platform dynamic state (mutable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformState {
    /// Current position (lat, lon, alt in degrees/meters)
    pub position: (f64, f64, f64),
    /// Fuel remaining in minutes
    pub fuel_minutes: u32,
    /// Health status
    pub health: HealthStatus,
    /// Current phase
    pub phase: Phase,
    /// Assigned squad ID (if any)
    pub squad_id: Option<String>,
    /// Last update timestamp
    pub timestamp: u64,
}

impl PlatformState {
    /// Create a new platform state at a given position
    pub fn new(position: (f64, f64, f64)) -> Self {
        Self {
            position,
            fuel_minutes: 120,
            health: HealthStatus::Nominal,
            phase: Phase::Bootstrap,
            squad_id: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Update the timestamp to current time
    pub fn update_timestamp(&mut self) {
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

/// Health status enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Operating normally
    Nominal,
    /// Degraded but operational
    Degraded,
    /// Critical failure imminent
    Critical,
    /// Failed/offline
    Failed,
}
