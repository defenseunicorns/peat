//! Platform state data structures
//!
//! This module defines platform data models with CRDT operations:
//! - Static capabilities: G-Set (grow-only set) - capabilities can only be added
//! - Dynamic state: LWW-Register (last-write-wins) - state updates with timestamps
//! - Fuel counter: PN-Counter (positive-negative counter) - increments/decrements

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

    /// Add a capability (G-Set operation - monotonic add only)
    ///
    /// This implements a G-Set (Grow-only Set) CRDT where capabilities can only be added,
    /// never removed. This ensures eventual consistency across distributed platforms.
    pub fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists (by ID)
        if !self.capabilities.iter().any(|c| c.id == capability.id) {
            self.capabilities.push(capability);
        }
    }

    /// Check if platform has a specific capability type
    pub fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.capability_type == capability_type)
    }

    /// Get all capabilities of a specific type
    pub fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability> {
        self.capabilities
            .iter()
            .filter(|c| c.capability_type == capability_type)
            .collect()
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

    /// Update position (LWW-Register operation)
    ///
    /// This implements Last-Write-Wins semantics where the update with the latest
    /// timestamp wins. Concurrent updates are resolved by timestamp comparison.
    pub fn update_position(&mut self, position: (f64, f64, f64)) {
        self.position = position;
        self.update_timestamp();
    }

    /// Update health status (LWW-Register operation)
    pub fn update_health(&mut self, health: HealthStatus) {
        self.health = health;
        self.update_timestamp();
    }

    /// Update phase (LWW-Register operation)
    pub fn update_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.update_timestamp();
    }

    /// Assign to a squad (LWW-Register operation)
    pub fn assign_squad(&mut self, squad_id: String) {
        self.squad_id = Some(squad_id);
        self.update_timestamp();
    }

    /// Remove from squad (LWW-Register operation)
    pub fn leave_squad(&mut self) {
        self.squad_id = None;
        self.update_timestamp();
    }

    /// Consume fuel (PN-Counter decrement operation)
    ///
    /// This implements a PN-Counter (Positive-Negative Counter) CRDT where
    /// fuel can be both consumed (decrement) and replenished (increment).
    pub fn consume_fuel(&mut self, minutes: u32) {
        self.fuel_minutes = self.fuel_minutes.saturating_sub(minutes);
        self.update_timestamp();
    }

    /// Replenish fuel (PN-Counter increment operation)
    pub fn replenish_fuel(&mut self, minutes: u32) {
        self.fuel_minutes = self.fuel_minutes.saturating_add(minutes);
        self.update_timestamp();
    }

    /// Check if platform is operational
    pub fn is_operational(&self) -> bool {
        self.health != HealthStatus::Failed && self.fuel_minutes > 0
    }

    /// Check if platform needs refueling (below 25% capacity)
    pub fn needs_refuel(&self) -> bool {
        self.fuel_minutes < 30 // Assuming 120 minutes is full capacity
    }

    /// Merge with another state (LWW-Register merge)
    ///
    /// When receiving updates from other peers, merge based on timestamp.
    /// The state with the later timestamp wins for each field.
    pub fn merge(&mut self, other: &PlatformState) {
        if other.timestamp > self.timestamp {
            self.position = other.position;
            self.health = other.health;
            self.phase = other.phase;
            self.squad_id = other.squad_id.clone();
            self.fuel_minutes = other.fuel_minutes;
            self.timestamp = other.timestamp;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CapabilityType;

    #[test]
    fn test_platform_config_add_capability() {
        let mut config = PlatformConfig::new("UAV".to_string());

        let cap1 = Capability::new(
            "camera_1".to_string(),
            "HD Camera".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        let cap2 = Capability::new(
            "gps_1".to_string(),
            "GPS".to_string(),
            CapabilityType::Sensor,
            1.0,
        );

        config.add_capability(cap1.clone());
        config.add_capability(cap2);

        assert_eq!(config.capabilities.len(), 2);

        // Try to add duplicate - should not add
        config.add_capability(cap1);
        assert_eq!(config.capabilities.len(), 2);
    }

    #[test]
    fn test_platform_config_has_capability_type() {
        let mut config = PlatformConfig::new("UAV".to_string());
        assert!(!config.has_capability_type(CapabilityType::Sensor));

        config.add_capability(Capability::new(
            "camera".to_string(),
            "Camera".to_string(),
            CapabilityType::Sensor,
            0.9,
        ));

        assert!(config.has_capability_type(CapabilityType::Sensor));
        assert!(!config.has_capability_type(CapabilityType::Compute));
    }

    #[test]
    fn test_platform_state_lww_operations() {
        let mut state = PlatformState::new((37.7, -122.4, 100.0));
        let initial_timestamp = state.timestamp;

        // Update position
        std::thread::sleep(std::time::Duration::from_secs(1));
        state.update_position((37.8, -122.5, 150.0));
        assert!(state.timestamp > initial_timestamp);
        assert_eq!(state.position, (37.8, -122.5, 150.0));

        // Update health
        state.update_health(HealthStatus::Degraded);
        assert_eq!(state.health, HealthStatus::Degraded);

        // Update phase
        state.update_phase(Phase::Squad);
        assert_eq!(state.phase, Phase::Squad);

        // Squad assignment
        state.assign_squad("squad_1".to_string());
        assert_eq!(state.squad_id, Some("squad_1".to_string()));

        state.leave_squad();
        assert_eq!(state.squad_id, None);
    }

    #[test]
    fn test_platform_state_fuel_counter() {
        let mut state = PlatformState::new((0.0, 0.0, 0.0));
        assert_eq!(state.fuel_minutes, 120);

        // Consume fuel
        state.consume_fuel(30);
        assert_eq!(state.fuel_minutes, 90);

        // Replenish fuel
        state.replenish_fuel(20);
        assert_eq!(state.fuel_minutes, 110);

        // Consume more than available
        state.consume_fuel(200);
        assert_eq!(state.fuel_minutes, 0);

        // Replenish to max
        state.replenish_fuel(150);
        assert_eq!(state.fuel_minutes, 150);
    }

    #[test]
    fn test_platform_state_operational_checks() {
        let mut state = PlatformState::new((0.0, 0.0, 0.0));
        assert!(state.is_operational());

        // No fuel
        state.consume_fuel(120);
        assert!(!state.is_operational());

        state.replenish_fuel(50);
        assert!(state.is_operational());

        // Failed health
        state.update_health(HealthStatus::Failed);
        assert!(!state.is_operational());
    }

    #[test]
    fn test_platform_state_needs_refuel() {
        let mut state = PlatformState::new((0.0, 0.0, 0.0));
        assert!(!state.needs_refuel());

        state.consume_fuel(100);
        assert!(state.needs_refuel());
    }

    #[test]
    fn test_platform_state_merge_lww() {
        let mut state1 = PlatformState::new((37.7, -122.4, 100.0));
        let mut state2 = state1.clone();

        // State2 has a later update
        std::thread::sleep(std::time::Duration::from_secs(1));
        state2.update_position((37.8, -122.5, 150.0));
        state2.update_health(HealthStatus::Degraded);

        // Merge state2 into state1 - state2 wins due to later timestamp
        state1.merge(&state2);

        assert_eq!(state1.position, (37.8, -122.5, 150.0));
        assert_eq!(state1.health, HealthStatus::Degraded);
        assert_eq!(state1.timestamp, state2.timestamp);
    }

    #[test]
    fn test_platform_state_merge_older_ignored() {
        let mut state1 = PlatformState::new((37.7, -122.4, 100.0));
        std::thread::sleep(std::time::Duration::from_secs(1));
        state1.update_position((37.8, -122.5, 150.0));

        let state2 = PlatformState::new((38.0, -123.0, 200.0));

        // Merge older state2 into state1 - state1 should remain unchanged
        let original_pos = state1.position;
        state1.merge(&state2);

        assert_eq!(state1.position, original_pos);
    }
}
