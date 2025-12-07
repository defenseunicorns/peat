//! Coordinator module - Aggregates team capabilities for C2 visibility
//!
//! The Coordinator is a node in the HIVE mesh that aggregates capabilities
//! from multiple teams and presents a formation-level view to C2.
//!
//! # Architecture Note
//!
//! The HIVE protocol uses CRDT-based document synchronization (Automerge/Iroh).
//! All nodes on the mesh automatically sync documents. The Coordinator doesn't
//! "bridge networks" - it simply reads team documents from the mesh and
//! aggregates their capabilities.
//!
//! Network connectivity, document sync, and mesh topology are handled by
//! the hive-protocol crate, not this application layer.

use hive_protocol::cell::{AggregatedCapability, CapabilityAggregator};
use hive_protocol::models::{CapabilityType, NodeConfig, NodeConfigExt, NodeState, NodeStateExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::platform::Platform;
use crate::team::Team;

// ============================================================================
// Formation Capability Summary (App-level view for C2)
// ============================================================================

/// Formation-level capability summary aggregating multiple teams
///
/// This is an application-level view presented to C2, summarizing
/// the capabilities of all teams in the formation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationCapabilitySummary {
    /// Formation identifier
    pub formation_id: String,
    /// Number of teams in formation
    pub team_count: usize,
    /// Total platforms across all teams
    pub total_platforms: usize,
    /// Total cameras across all teams
    pub total_cameras: usize,
    /// Total AI trackers across all teams
    pub total_trackers: usize,
    /// AI model versions in use
    pub tracker_versions: Vec<String>,
    /// Coverage sectors (team names/areas)
    pub coverage_sectors: Vec<String>,
    /// Overall readiness score (0.0-1.0)
    pub readiness_score: f32,
    /// Aggregated capability confidence by type
    pub capability_confidence: HashMap<String, f32>,
}

impl std::fmt::Display for FormationCapabilitySummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Formation {}: {} teams, {} platforms\n\
             Cameras: {}, Trackers: {} ({})\n\
             Coverage: {}\n\
             Readiness: {:.0}%",
            self.formation_id,
            self.team_count,
            self.total_platforms,
            self.total_cameras,
            self.total_trackers,
            self.tracker_versions.join(", "),
            self.coverage_sectors.join(" + "),
            self.readiness_score * 100.0
        )
    }
}

// ============================================================================
// Coordinator (App-level team aggregation)
// ============================================================================

/// A coordinator node that aggregates team capabilities for C2
///
/// The Coordinator reads team documents from the HIVE mesh and presents
/// a unified formation-level view. Network connectivity and document
/// synchronization are handled by the hive-protocol layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinator {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Teams in this formation (read from mesh documents)
    pub teams: Vec<Team>,
}

impl Coordinator {
    /// Create a new coordinator
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            teams: Vec::new(),
        }
    }

    /// Register a team with this coordinator
    ///
    /// In a real deployment, teams would be discovered via the HIVE mesh.
    /// This method is for local testing and demonstration.
    pub fn register_team(&mut self, team: Team) {
        self.teams.push(team);
    }

    /// Get the number of registered teams
    pub fn team_count(&self) -> usize {
        self.teams.len()
    }

    /// Get total platform count across all teams
    pub fn total_platforms(&self) -> usize {
        self.teams.iter().map(|t| t.member_count()).sum()
    }

    /// Get a team by name
    pub fn get_team(&self, name: &str) -> Option<&Team> {
        self.teams.iter().find(|t| t.name == name)
    }

    /// Get a mutable team by name
    pub fn get_team_mut(&mut self, name: &str) -> Option<&mut Team> {
        self.teams.iter_mut().find(|t| t.name == name)
    }

    /// Aggregate capabilities from all teams using hive-protocol
    pub fn get_aggregated_capabilities(
        &self,
    ) -> Result<HashMap<CapabilityType, AggregatedCapability>, hive_protocol::Error> {
        // Convert all team members to NodeConfig/NodeState for aggregation
        let mut all_members: Vec<(NodeConfig, NodeState)> = Vec::new();

        for team in &self.teams {
            for platform in &team.members {
                let (config, state) = platform_to_node(platform);
                all_members.push((config, state));
            }
        }

        CapabilityAggregator::aggregate_capabilities(&all_members)
    }

    /// Calculate formation readiness score
    pub fn readiness_score(&self) -> f32 {
        if let Ok(caps) = self.get_aggregated_capabilities() {
            CapabilityAggregator::calculate_readiness_score(&caps)
        } else {
            0.0
        }
    }

    /// Generate formation-level capability summary for C2
    pub fn get_formation_summary(&self) -> FormationCapabilitySummary {
        let mut total_cameras = 0;
        let mut total_trackers = 0;
        let mut tracker_versions: Vec<String> = Vec::new();
        let mut coverage_sectors: Vec<String> = Vec::new();

        for team in &self.teams {
            let team_summary = team.get_capability_summary();
            total_cameras += team_summary.camera_count;

            for model in &team_summary.ai_models {
                total_trackers += 1;
                let version_str = format!("v{}", model.version);
                if !tracker_versions.contains(&version_str) {
                    tracker_versions.push(version_str);
                }
            }

            // Use team name as sector identifier
            coverage_sectors.push(format!("Sector {}", team.name.replace(" Team", "")));
        }

        // Build capability confidence map
        let mut capability_confidence: HashMap<String, f32> = HashMap::new();
        if let Ok(caps) = self.get_aggregated_capabilities() {
            for (cap_type, agg_cap) in caps {
                capability_confidence.insert(format!("{:?}", cap_type), agg_cap.confidence);
            }
        }

        FormationCapabilitySummary {
            formation_id: self.id.to_string(),
            team_count: self.teams.len(),
            total_platforms: self.total_platforms(),
            total_cameras,
            total_trackers,
            tracker_versions,
            coverage_sectors,
            readiness_score: self.readiness_score(),
            capability_confidence,
        }
    }
}

// ============================================================================
// Helper: Platform to NodeConfig/NodeState conversion
// ============================================================================

/// Convert a Platform to hive-protocol NodeConfig and NodeState
fn platform_to_node(platform: &Platform) -> (NodeConfig, NodeState) {
    use crate::platform::AuthorityLevel;
    use hive_protocol::models::{
        AuthorityLevel as HiveAuthorityLevel, BindingType, HumanMachinePair, HumanMachinePairExt,
        Operator, OperatorExt, OperatorRank,
    };
    use hive_protocol::traits::Phase;

    let caps = platform.get_capabilities();

    fn convert_authority(auth: AuthorityLevel) -> HiveAuthorityLevel {
        match auth {
            AuthorityLevel::Observer => HiveAuthorityLevel::Observer,
            AuthorityLevel::Advisor => HiveAuthorityLevel::Advisor,
            AuthorityLevel::Supervisor => HiveAuthorityLevel::Supervisor,
            AuthorityLevel::Commander => HiveAuthorityLevel::Commander,
        }
    }

    fn authority_to_rank(auth: AuthorityLevel) -> OperatorRank {
        match auth {
            AuthorityLevel::Commander => OperatorRank::E7,
            AuthorityLevel::Supervisor => OperatorRank::E6,
            AuthorityLevel::Advisor => OperatorRank::E5,
            AuthorityLevel::Observer => OperatorRank::E4,
        }
    }

    match platform {
        Platform::Operator(op) => {
            let mut config = NodeConfig::new("Operator".to_string());
            config.id = op.id.clone();

            for cap in caps {
                config.add_capability(cap);
            }

            let operator = Operator::new(
                op.id.clone(),
                op.name.clone(),
                authority_to_rank(op.authority),
                convert_authority(op.authority),
                "11B".to_string(),
            );

            let binding =
                HumanMachinePair::new(vec![operator], vec![op.id.clone()], BindingType::OneToOne);
            config.operator_binding = Some(binding);

            let mut state = NodeState::new((
                op.position.map(|(lat, _)| lat).unwrap_or(0.0),
                op.position.map(|(_, lon)| lon).unwrap_or(0.0),
                0.0,
            ));
            state.update_phase(Phase::Cell);

            (config, state)
        }
        Platform::Vehicle(veh) => {
            let mut config = NodeConfig::new(format!("{:?}", veh.vehicle_type));
            config.id = veh.id.clone();
            config.comm_range_m = veh.comm_range_m as f32;
            config.max_speed_mps = veh.max_speed_mps as f32;

            for cap in caps {
                config.add_capability(cap);
            }

            let mut state = NodeState::new((
                veh.position.map(|(lat, _, _)| lat).unwrap_or(0.0),
                veh.position.map(|(_, lon, _)| lon).unwrap_or(0.0),
                veh.position.map(|(_, _, alt)| alt).unwrap_or(0.0),
            ));
            state.update_phase(Phase::Cell);

            (config, state)
        }
        Platform::AiModel(ai) => {
            let mut config = NodeConfig::new("AiModel".to_string());
            config.id = ai.id.clone();

            for cap in caps {
                config.add_capability(cap);
            }

            let mut state = NodeState::new((0.0, 0.0, 0.0));
            state.update_phase(Phase::Cell);

            (config, state)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        AiModelInfo, AiModelPlatform, AuthorityLevel, OperatorPlatform, SensorCapability,
        VehiclePlatform,
    };

    /// Create a standard M1 vignette team
    fn create_test_team(name: &str, is_uav: bool) -> Team {
        let mut team = Team::new(format!("{} Team", name));

        team.add_member(Platform::Operator(OperatorPlatform::new(
            format!("{}-1", name),
            format!("{}-1", name.to_uppercase()),
            AuthorityLevel::Commander,
        )));

        if is_uav {
            team.add_member(Platform::Vehicle(
                VehiclePlatform::new_uav(format!("{}-2", name))
                    .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0)),
            ));
        } else {
            team.add_member(Platform::Vehicle(
                VehiclePlatform::new_ugv(format!("{}-2", name))
                    .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
            ));
        }

        team.add_member(Platform::AiModel(AiModelPlatform::new(
            format!("{}-3", name),
            AiModelInfo::object_tracker("1.2.0"),
        )));

        team
    }

    #[test]
    fn test_coordinator_creation() {
        let mut coordinator = Coordinator::new("M1 Coordinator");
        assert_eq!(coordinator.name, "M1 Coordinator");
        assert_eq!(coordinator.team_count(), 0);

        let team = Team::new("Alpha Team");
        coordinator.register_team(team);

        assert_eq!(coordinator.team_count(), 1);
    }

    #[test]
    fn test_formation_capability_summary() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        let summary = coordinator.get_formation_summary();

        assert_eq!(summary.team_count, 2);
        assert_eq!(summary.total_platforms, 6); // 3 per team
        assert_eq!(summary.total_cameras, 2); // One per team
        assert_eq!(summary.total_trackers, 2); // One per team
        assert!(summary.tracker_versions.contains(&"v1.2.0".to_string()));
        assert_eq!(summary.coverage_sectors.len(), 2);

        // Print summary for verification
        println!("{}", summary);
    }

    #[test]
    fn test_aggregated_capabilities() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        let caps = coordinator.get_aggregated_capabilities().unwrap();

        // Should have multiple capability types aggregated
        assert!(caps.contains_key(&CapabilityType::Communication));
        assert!(caps.contains_key(&CapabilityType::Sensor));
        assert!(caps.contains_key(&CapabilityType::Compute));

        // Communication should have multiple contributors
        let comm_cap = caps.get(&CapabilityType::Communication).unwrap();
        assert!(comm_cap.contributor_count >= 2);
    }

    #[test]
    fn test_readiness_score() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        coordinator.register_team(alpha);

        let score = coordinator.readiness_score();
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_get_team() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        coordinator.register_team(alpha);

        assert!(coordinator.get_team("Alpha Team").is_some());
        assert!(coordinator.get_team("Bravo Team").is_none());
    }

    #[test]
    fn test_total_platforms() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        assert_eq!(coordinator.total_platforms(), 6);
    }

    #[test]
    fn test_formation_summary_display() {
        let summary = FormationCapabilitySummary {
            formation_id: "test-123".to_string(),
            team_count: 2,
            total_platforms: 6,
            total_cameras: 2,
            total_trackers: 2,
            tracker_versions: vec!["v1.2.0".to_string()],
            coverage_sectors: vec!["Sector Alpha".to_string(), "Sector Bravo".to_string()],
            readiness_score: 0.85,
            capability_confidence: HashMap::new(),
        };

        let display = format!("{}", summary);
        assert!(display.contains("test-123"));
        assert!(display.contains("2 teams"));
        assert!(display.contains("6 platforms"));
        assert!(display.contains("Cameras: 2"));
        assert!(display.contains("Trackers: 2"));
        assert!(display.contains("85%"));
    }
}
