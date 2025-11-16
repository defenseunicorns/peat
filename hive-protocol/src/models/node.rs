//! Node state data structures
//!
//! This module defines platform data models with CRDT operations:
//! - Static capabilities: G-Set (grow-only set) - capabilities can only be added
//! - Dynamic state: LWW-Register (last-write-wins) - state updates with timestamps
//! - Fuel counter: PN-Counter (positive-negative counter) - increments/decrements

use crate::models::{Capability, CapabilityExt, HumanMachinePairExt, Operator};
use crate::traits::Phase;
use uuid::Uuid;

// Re-export protobuf types
pub use hive_schema::node::v1::{HealthStatus, NodeConfig, NodeState};

// Extension trait for NodeConfig helper methods
pub trait NodeConfigExt {
    /// Create a new node configuration (autonomous, no operator)
    fn new(platform_type: String) -> Self;

    /// Create a new platform with operator binding
    fn with_operator(
        platform_type: String,
        operator_binding: hive_schema::node::v1::HumanMachinePair,
    ) -> Self;

    /// Add a capability (G-Set operation - monotonic add only)
    fn add_capability(&mut self, capability: Capability);

    /// Check if platform has a specific capability type
    fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool;

    /// Get all capabilities of a specific type
    fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability>;

    /// Check if platform has an operator binding
    fn has_operator(&self) -> bool;

    /// Check if platform is human-operated (has at least one operator)
    fn is_human_operated(&self) -> bool;

    /// Get the primary operator (highest-ranking) if any
    fn get_primary_operator(&self) -> Option<&Operator>;

    /// Get the operator binding
    fn get_operator_binding(&self) -> Option<&hive_schema::node::v1::HumanMachinePair>;

    /// Set or update the operator binding
    fn set_operator_binding(&mut self, binding: Option<hive_schema::node::v1::HumanMachinePair>);

    /// Check if platform is autonomous (no operators)
    fn is_autonomous(&self) -> bool;
}

impl NodeConfigExt for NodeConfig {
    fn new(platform_type: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            platform_type,
            capabilities: Vec::new(),
            comm_range_m: 1000.0,
            max_speed_mps: 10.0,
            operator_binding: None,
            created_at: None,
        }
    }

    fn with_operator(
        platform_type: String,
        operator_binding: hive_schema::node::v1::HumanMachinePair,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            platform_type,
            capabilities: Vec::new(),
            comm_range_m: 1000.0,
            max_speed_mps: 10.0,
            operator_binding: Some(operator_binding),
            created_at: None,
        }
    }

    fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists (by ID)
        if !self.capabilities.iter().any(|c| c.id == capability.id) {
            self.capabilities.push(capability);
        }
    }

    fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.get_capability_type() == capability_type)
    }

    fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability> {
        self.capabilities
            .iter()
            .filter(|c| c.get_capability_type() == capability_type)
            .collect()
    }

    fn has_operator(&self) -> bool {
        self.operator_binding.is_some()
    }

    fn is_human_operated(&self) -> bool {
        self.operator_binding
            .as_ref()
            .map(|binding| !binding.operators.is_empty())
            .unwrap_or(false)
    }

    fn get_primary_operator(&self) -> Option<&Operator> {
        self.operator_binding
            .as_ref()
            .and_then(|binding| binding.primary_operator())
    }

    fn get_operator_binding(&self) -> Option<&hive_schema::node::v1::HumanMachinePair> {
        self.operator_binding.as_ref()
    }

    fn set_operator_binding(&mut self, binding: Option<hive_schema::node::v1::HumanMachinePair>) {
        self.operator_binding = binding;
    }

    fn is_autonomous(&self) -> bool {
        !self.is_human_operated()
    }
}

// Extension trait for NodeState helper methods
pub trait NodeStateExt {
    /// Create a new node state at a given position
    fn new(position: (f64, f64, f64)) -> Self;

    /// Update the timestamp to current time
    fn update_timestamp(&mut self);

    /// Get position as tuple (lat, lon, alt)
    fn get_position(&self) -> (f64, f64, f64);

    /// Update position (LWW-Register operation)
    fn update_position(&mut self, position: (f64, f64, f64));

    /// Get health status
    fn get_health(&self) -> HealthStatus;

    /// Update health status (LWW-Register operation)
    fn update_health(&mut self, health: HealthStatus);

    /// Get phase
    fn get_phase(&self) -> Phase;

    /// Update phase (LWW-Register operation)
    fn update_phase(&mut self, phase: Phase);

    /// Assign to a cell (LWW-Register operation)
    fn assign_cell(&mut self, cell_id: String);

    /// Remove from cell (LWW-Register operation)
    fn leave_cell(&mut self);

    /// Assign to a zone (LWW-Register operation)
    fn assign_zone(&mut self, zone_id: String);

    /// Remove from zone (LWW-Register operation)
    fn leave_zone(&mut self);

    /// Consume fuel (PN-Counter decrement operation)
    fn consume_fuel(&mut self, minutes: u32);

    /// Replenish fuel (PN-Counter increment operation)
    fn replenish_fuel(&mut self, minutes: u32);

    /// Check if platform is operational
    fn is_operational(&self) -> bool;

    /// Check if platform needs refueling (below 25% capacity)
    fn needs_refuel(&self) -> bool;

    /// Merge with another state (LWW-Register merge)
    fn merge(&mut self, other: &NodeState);
}

impl NodeStateExt for NodeState {
    fn new(position: (f64, f64, f64)) -> Self {
        use hive_schema::common::v1::Position;

        Self {
            position: Some(Position {
                latitude: position.0,
                longitude: position.1,
                altitude: position.2,
            }),
            fuel_minutes: 120,
            health: HealthStatus::Nominal as i32,
            phase: hive_schema::node::v1::Phase::Discovery as i32,
            cell_id: None,
            zone_id: None,
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }

    fn update_timestamp(&mut self) {
        self.timestamp = Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        });
    }

    fn get_position(&self) -> (f64, f64, f64) {
        if let Some(ref pos) = self.position {
            (pos.latitude, pos.longitude, pos.altitude)
        } else {
            (0.0, 0.0, 0.0)
        }
    }

    fn update_position(&mut self, position: (f64, f64, f64)) {
        use hive_schema::common::v1::Position;
        self.position = Some(Position {
            latitude: position.0,
            longitude: position.1,
            altitude: position.2,
        });
        self.update_timestamp();
    }

    fn get_health(&self) -> HealthStatus {
        HealthStatus::try_from(self.health).unwrap_or(HealthStatus::Unspecified)
    }

    fn update_health(&mut self, health: HealthStatus) {
        self.health = health as i32;
        self.update_timestamp();
    }

    fn get_phase(&self) -> Phase {
        let proto_phase = hive_schema::node::v1::Phase::try_from(self.phase)
            .unwrap_or(hive_schema::node::v1::Phase::Unspecified);
        match proto_phase {
            hive_schema::node::v1::Phase::Discovery => Phase::Discovery,
            hive_schema::node::v1::Phase::Cell => Phase::Cell,
            hive_schema::node::v1::Phase::Hierarchy => Phase::Hierarchy,
            _ => Phase::Discovery,
        }
    }

    fn update_phase(&mut self, phase: Phase) {
        self.phase = phase as i32;
        self.update_timestamp();
    }

    fn assign_cell(&mut self, cell_id: String) {
        self.cell_id = Some(cell_id);
        self.update_timestamp();
    }

    fn leave_cell(&mut self) {
        self.cell_id = None;
        self.update_timestamp();
    }

    fn assign_zone(&mut self, zone_id: String) {
        self.zone_id = Some(zone_id);
        self.update_timestamp();
    }

    fn leave_zone(&mut self) {
        self.zone_id = None;
        self.update_timestamp();
    }

    fn consume_fuel(&mut self, minutes: u32) {
        self.fuel_minutes = self.fuel_minutes.saturating_sub(minutes);
        self.update_timestamp();
    }

    fn replenish_fuel(&mut self, minutes: u32) {
        self.fuel_minutes = self.fuel_minutes.saturating_add(minutes);
        self.update_timestamp();
    }

    fn is_operational(&self) -> bool {
        self.get_health() != HealthStatus::Failed && self.fuel_minutes > 0
    }

    fn needs_refuel(&self) -> bool {
        self.fuel_minutes < 30 // Assuming 120 minutes is full capacity
    }

    fn merge(&mut self, other: &NodeState) {
        let self_ts = self.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
        let other_ts = other.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        if other_ts > self_ts {
            self.position = other.position;
            self.health = other.health;
            self.phase = other.phase;
            self.cell_id = other.cell_id.clone();
            self.zone_id = other.zone_id.clone();
            self.fuel_minutes = other.fuel_minutes;
            self.timestamp = other.timestamp;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthorityLevel, BindingType, CapabilityType, HumanMachinePair};

    #[test]
    fn test_platform_config_add_capability() {
        let mut config = NodeConfig::new("UAV".to_string());

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
        let mut config = NodeConfig::new("UAV".to_string());
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
        let mut state = NodeState::new((37.7, -122.4, 100.0));
        let initial_timestamp = state.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        // Update position
        std::thread::sleep(std::time::Duration::from_secs(1));
        state.update_position((37.8, -122.5, 150.0));
        assert!(state.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0) > initial_timestamp);
        assert_eq!(state.get_position(), (37.8, -122.5, 150.0));

        // Update health
        state.update_health(HealthStatus::Degraded);
        assert_eq!(state.get_health(), HealthStatus::Degraded);

        // Update phase
        state.update_phase(Phase::Cell);
        assert_eq!(state.get_phase(), Phase::Cell);

        // Cell assignment
        state.assign_cell("cell_1".to_string());
        assert_eq!(state.cell_id, Some("cell_1".to_string()));

        state.leave_cell();
        assert_eq!(state.cell_id, None);

        // Zone assignment
        state.assign_zone("zone_1".to_string());
        assert_eq!(state.zone_id, Some("zone_1".to_string()));

        state.leave_zone();
        assert_eq!(state.zone_id, None);
    }

    #[test]
    fn test_platform_state_fuel_counter() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));
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
        let mut state = NodeState::new((0.0, 0.0, 0.0));
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
        let mut state = NodeState::new((0.0, 0.0, 0.0));
        assert!(!state.needs_refuel());

        state.consume_fuel(100);
        assert!(state.needs_refuel());
    }

    #[test]
    fn test_platform_state_merge_lww() {
        let mut state1 = NodeState::new((37.7, -122.4, 100.0));
        let mut state2 = state1.clone();

        // State2 has a later update
        std::thread::sleep(std::time::Duration::from_secs(1));
        state2.update_position((37.8, -122.5, 150.0));
        state2.update_health(HealthStatus::Degraded);

        // Merge state2 into state1 - state2 wins due to later timestamp
        state1.merge(&state2);

        assert_eq!(state1.get_position(), (37.8, -122.5, 150.0));
        assert_eq!(state1.get_health(), HealthStatus::Degraded);
        assert_eq!(
            state1.timestamp.as_ref().map(|t| t.seconds),
            state2.timestamp.as_ref().map(|t| t.seconds)
        );
    }

    #[test]
    fn test_platform_state_merge_older_ignored() {
        let mut state1 = NodeState::new((37.7, -122.4, 100.0));
        std::thread::sleep(std::time::Duration::from_secs(1));
        state1.update_position((37.8, -122.5, 150.0));

        let state2 = NodeState::new((38.0, -123.0, 200.0));

        // Merge older state2 into state1 - state1 should remain unchanged
        let original_pos = state1.get_position();
        state1.merge(&state2);

        assert_eq!(state1.get_position(), original_pos);
    }

    #[test]
    fn test_platform_config_autonomous() {
        let config = NodeConfig::new("UAV".to_string());

        assert!(!config.has_operator());
        assert!(!config.is_human_operated());
        assert!(config.is_autonomous());
        assert!(config.get_primary_operator().is_none());
        assert!(config.get_operator_binding().is_none());
    }

    #[test]
    fn test_platform_config_with_operator() {
        use crate::models::{OperatorExt, OperatorRank};

        let operator = Operator::new(
            "op_1".to_string(),
            "SSG Smith".to_string(),
            OperatorRank::E6,
            AuthorityLevel::Commander,
            "11B".to_string(), // Infantry
        );

        let binding = HumanMachinePair::new(
            vec![operator],
            vec!["node_1".to_string()],
            BindingType::OneToOne,
        );

        let config = NodeConfig::with_operator("Soldier System".to_string(), binding);

        assert!(config.has_operator());
        assert!(config.is_human_operated());
        assert!(!config.is_autonomous());

        let primary = config.get_primary_operator().unwrap();
        assert_eq!(primary.rank, OperatorRank::E6 as i32);
        assert_eq!(primary.name, "SSG Smith");
    }

    #[test]
    fn test_platform_config_set_operator_binding() {
        use crate::models::{OperatorExt, OperatorRank};

        let mut config = NodeConfig::new("Robot".to_string());
        assert!(config.is_autonomous());

        // Add operator binding
        let operator = Operator::new(
            "op_1".to_string(),
            "PFC Jones".to_string(),
            OperatorRank::E3,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        let binding = HumanMachinePair::new(
            vec![operator],
            vec![config.id.clone()],
            BindingType::OneToOne,
        );

        config.set_operator_binding(Some(binding));
        assert!(config.is_human_operated());
        assert!(!config.is_autonomous());

        // Remove operator binding
        config.set_operator_binding(None);
        assert!(config.is_autonomous());
    }

    #[test]
    fn test_platform_config_multiple_operators() {
        use crate::models::{OperatorExt, OperatorRank};

        // Command vehicle with multiple operators
        let commander = Operator::new(
            "op_1".to_string(),
            "CPT Williams".to_string(),
            OperatorRank::O3,
            AuthorityLevel::Commander,
            "11A".to_string(), // Infantry Officer
        );

        let nco = Operator::new(
            "op_2".to_string(),
            "SFC Davis".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        let rto = Operator::new(
            "op_3".to_string(),
            "SPC Brown".to_string(),
            OperatorRank::E4,
            AuthorityLevel::Advisor,
            "25U".to_string(), // Signal
        );

        let binding = HumanMachinePair::new(
            vec![commander, nco, rto],
            vec!["command_vehicle_1".to_string()],
            BindingType::ManyToOne,
        );

        let config = NodeConfig::with_operator("Command Vehicle".to_string(), binding);

        assert!(config.is_human_operated());

        // Primary operator should be highest rank (O3)
        let primary = config.get_primary_operator().unwrap();
        assert_eq!(primary.rank, OperatorRank::O3 as i32);
        assert_eq!(primary.name, "CPT Williams");

        let binding = config.get_operator_binding().unwrap();
        assert_eq!(binding.operators.len(), 3);
        assert_eq!(binding.binding_type, BindingType::ManyToOne as i32);
    }

    #[test]
    fn test_platform_config_swarm_operator() {
        use crate::models::{OperatorExt, OperatorRank};

        // One operator controlling multiple platforms
        let operator = Operator::new(
            "op_1".to_string(),
            "SSG Martinez".to_string(),
            OperatorRank::E6,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        let platform_ids = vec![
            "robot_1".to_string(),
            "robot_2".to_string(),
            "robot_3".to_string(),
            "robot_4".to_string(),
        ];

        let binding =
            HumanMachinePair::new(vec![operator], platform_ids.clone(), BindingType::OneToMany);

        let config = NodeConfig::with_operator("Swarm Control Station".to_string(), binding);

        assert!(config.is_human_operated());

        let binding = config.get_operator_binding().unwrap();
        assert_eq!(binding.binding_type, BindingType::OneToMany as i32);
        assert_eq!(binding.platform_ids.len(), 4);
        assert_eq!(binding.operators.len(), 1);
    }

    #[test]
    fn test_node_config_get_capabilities_by_type_multiple() {
        let mut config = NodeConfig::new("Multi-sensor platform".to_string());

        // Add multiple sensors
        for i in 1..=3 {
            config.add_capability(Capability::new(
                format!("sensor_{}", i),
                format!("Sensor {}", i),
                CapabilityType::Sensor,
                0.9,
            ));
        }

        // Add compute capability
        config.add_capability(Capability::new(
            "compute_1".to_string(),
            "Edge Compute".to_string(),
            CapabilityType::Compute,
            0.8,
        ));

        let sensors = config.get_capabilities_by_type(CapabilityType::Sensor);
        assert_eq!(sensors.len(), 3);

        let compute = config.get_capabilities_by_type(CapabilityType::Compute);
        assert_eq!(compute.len(), 1);

        let mobility = config.get_capabilities_by_type(CapabilityType::Mobility);
        assert_eq!(mobility.len(), 0);
    }

    #[test]
    fn test_node_state_health_transitions() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));

        // Test all health status transitions
        for health in [
            HealthStatus::Nominal,
            HealthStatus::Degraded,
            HealthStatus::Critical,
            HealthStatus::Failed,
        ] {
            state.update_health(health);
            assert_eq!(state.get_health(), health);
        }
    }

    #[test]
    fn test_node_state_phase_transitions() {
        use crate::traits::Phase;

        let mut state = NodeState::new((0.0, 0.0, 0.0));

        // Test all phase transitions
        for phase in [Phase::Discovery, Phase::Cell, Phase::Hierarchy] {
            state.update_phase(phase);
            assert_eq!(state.get_phase(), phase);
        }
    }

    #[test]
    fn test_node_state_fuel_edge_cases() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));

        // Test saturating add
        state.replenish_fuel(u32::MAX);
        assert!(state.fuel_minutes > 0);

        // Consume all fuel
        state.consume_fuel(u32::MAX);
        assert_eq!(state.fuel_minutes, 0);
        assert!(!state.is_operational());
    }

    #[test]
    fn test_node_state_position_updates() {
        let mut state = NodeState::new((37.7, -122.4, 100.0));

        let pos1 = state.get_position();
        assert_eq!(pos1, (37.7, -122.4, 100.0));

        // Update multiple times
        state.update_position((38.0, -123.0, 200.0));
        assert_eq!(state.get_position(), (38.0, -123.0, 200.0));

        state.update_position((39.0, -124.0, 300.0));
        assert_eq!(state.get_position(), (39.0, -124.0, 300.0));
    }

    #[test]
    fn test_node_state_merge_with_equal_timestamps() {
        let mut state1 = NodeState::new((37.7, -122.4, 100.0));
        let state2 = state1.clone();

        // Same timestamp - state1 should remain unchanged
        let original_pos = state1.get_position();
        state1.merge(&state2);
        assert_eq!(state1.get_position(), original_pos);
    }

    #[test]
    fn test_node_state_cell_and_zone_assignments() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));

        assert!(state.cell_id.is_none());
        assert!(state.zone_id.is_none());

        // Assign both
        state.assign_cell("cell_1".to_string());
        state.assign_zone("zone_1".to_string());

        assert_eq!(state.cell_id, Some("cell_1".to_string()));
        assert_eq!(state.zone_id, Some("zone_1".to_string()));

        // Leave cell but keep zone
        state.leave_cell();
        assert!(state.cell_id.is_none());
        assert_eq!(state.zone_id, Some("zone_1".to_string()));

        // Leave zone
        state.leave_zone();
        assert!(state.zone_id.is_none());
    }

    #[test]
    fn test_node_state_needs_refuel_threshold() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));
        assert_eq!(state.fuel_minutes, 120);
        assert!(!state.needs_refuel());

        // At threshold (30)
        state.consume_fuel(90);
        assert_eq!(state.fuel_minutes, 30);
        assert!(!state.needs_refuel());

        // Below threshold
        state.consume_fuel(1);
        assert_eq!(state.fuel_minutes, 29);
        assert!(state.needs_refuel());
    }

    #[test]
    fn test_node_config_empty_binding() {
        // Empty operators but binding exists
        let binding =
            HumanMachinePair::new(vec![], vec!["node_1".to_string()], BindingType::Unspecified);

        let config = NodeConfig::with_operator("Test".to_string(), binding);

        // Has binding but no operators
        assert!(config.has_operator());
        assert!(!config.is_human_operated());
        assert!(config.is_autonomous());
        assert!(config.get_primary_operator().is_none());
    }

    #[test]
    fn test_node_state_get_position_no_position() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));
        state.position = None;

        // Should return zeros when position is None
        assert_eq!(state.get_position(), (0.0, 0.0, 0.0));
    }

    #[test]
    fn test_node_state_invalid_health_defaults_to_unspecified() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));

        // Set invalid health value
        state.health = 999;

        assert_eq!(state.get_health(), HealthStatus::Unspecified);
    }

    #[test]
    fn test_node_state_invalid_phase_defaults_to_discovery() {
        let mut state = NodeState::new((0.0, 0.0, 0.0));

        // Set invalid phase value
        state.phase = 999;

        assert_eq!(state.get_phase(), crate::traits::Phase::Discovery);
    }
}
