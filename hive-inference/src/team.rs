//! Team (Cell) module - Human-machine-AI cells
//!
//! Teams aggregate capabilities from member platforms and participate in leader election.
//! This module integrates with hive-protocol's cell module for:
//! - Formation management via CellCoordinator
//! - Leader election with human operator preference
//! - Capability aggregation using CapabilityAggregator
//!
//! # Success Criteria (M1 Vignette)
//! - Teams form and advertise capabilities within 30 seconds
//! - Human operator elected as team leader (authority weight)
//! - Team capability summary: "1 camera, 1 object tracker v1.2.0, precision 0.91"

use hive_protocol::cell::election_policy::{
    ElectionContext, ElectionPolicyConfig, LeadershipPolicy,
};
use hive_protocol::cell::{
    AggregatedCapability, CapabilityAggregator, CellCoordinator, FormationStatus,
};
use hive_protocol::models::{
    AuthorityLevel as HiveAuthorityLevel, BindingType, Capability, CapabilityType, CellRole,
    HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt, NodeState, NodeStateExt,
    Operator, OperatorExt, OperatorRank, RoleScorer,
};
use hive_protocol::traits::Phase;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::platform::{AuthorityLevel, Platform, PlatformType};

// ============================================================================
// Platform to NodeConfig/NodeState Conversion
// ============================================================================

/// Convert local AuthorityLevel to hive-protocol AuthorityLevel
fn convert_authority(auth: AuthorityLevel) -> HiveAuthorityLevel {
    match auth {
        AuthorityLevel::Observer => HiveAuthorityLevel::Observer,
        AuthorityLevel::Advisor => HiveAuthorityLevel::Advisor,
        AuthorityLevel::Supervisor => HiveAuthorityLevel::Supervisor,
        AuthorityLevel::Commander => HiveAuthorityLevel::Commander,
    }
}

/// Map local AuthorityLevel to an appropriate OperatorRank
/// Higher authority = higher rank for leader election scoring
fn authority_to_rank(auth: AuthorityLevel) -> OperatorRank {
    match auth {
        AuthorityLevel::Commander => OperatorRank::E7, // Sergeant First Class
        AuthorityLevel::Supervisor => OperatorRank::E6, // Staff Sergeant
        AuthorityLevel::Advisor => OperatorRank::E5,   // Sergeant
        AuthorityLevel::Observer => OperatorRank::E4,  // Specialist
    }
}

/// Convert a Platform to hive-protocol NodeConfig and NodeState
fn platform_to_node(platform: &Platform) -> (NodeConfig, NodeState) {
    let caps = platform.get_capabilities();

    match platform {
        Platform::Operator(op) => {
            let mut config = NodeConfig::new("Operator".to_string());
            config.id = op.id.clone();

            // Add capabilities
            for cap in caps {
                config.add_capability(cap);
            }

            // Create operator binding for human operators
            // Map authority level to rank for leader election scoring
            let operator = Operator::new(
                op.id.clone(),
                op.name.clone(),
                authority_to_rank(op.authority), // Map authority to appropriate rank
                convert_authority(op.authority),
                "11B".to_string(), // Default MOS (Infantry)
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
// Team Capability Summary
// ============================================================================

/// Summary of aggregated team capabilities for advertising
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamCapabilitySummary {
    /// Number of cameras/sensors
    pub camera_count: usize,
    /// AI model info (model ID, version, precision)
    pub ai_models: Vec<AiModelSummary>,
    /// Communication capability present
    pub has_communication: bool,
    /// Human-in-the-loop present
    pub has_hitl: bool,
    /// Overall readiness score (0.0-1.0)
    pub readiness_score: f32,
}

/// Summary of an AI model's capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelSummary {
    pub model_id: String,
    pub version: String,
    pub precision: f64,
}

impl std::fmt::Display for TeamCapabilitySummary {
    /// Generate a human-readable summary string
    /// Example: "1 camera, 1 object tracker v1.2.0, precision 0.91"
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();

        if self.camera_count > 0 {
            parts.push(format!(
                "{} camera{}",
                self.camera_count,
                if self.camera_count > 1 { "s" } else { "" }
            ));
        }

        for model in &self.ai_models {
            parts.push(format!(
                "{} v{}, precision {:.2}",
                model.model_id, model.version, model.precision
            ));
        }

        if parts.is_empty() {
            write!(f, "No capabilities")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

// ============================================================================
// Team Formation Manager
// ============================================================================

/// Team formation manager that wraps hive-protocol's CellCoordinator
pub struct TeamFormation {
    /// Team ID
    pub team_id: String,
    /// Cell coordinator for formation status
    coordinator: CellCoordinator,
    /// Election policy configuration
    election_policy: ElectionPolicyConfig,
    /// Formation start time
    formation_start: Instant,
    /// Members as (NodeConfig, NodeState, Role) tuples
    members: Vec<(NodeConfig, NodeState, Option<CellRole>)>,
    /// Current leader ID
    leader_id: Option<String>,
}

impl TeamFormation {
    /// Create a new team formation manager
    pub fn new(team_id: impl Into<String>) -> Self {
        let team_id = team_id.into();

        // Configure election policy to prefer human operators
        let election_policy = ElectionPolicyConfig {
            default_policy: LeadershipPolicy::RankDominant, // Human rank wins
            allow_autonomous_leaders: false,                // Require human leadership
            min_leader_rank: Some(OperatorRank::E4),        // Minimum Specialist
            ..Default::default()
        };

        let mut coordinator = CellCoordinator::new(team_id.clone());
        // M1 vignette: Teams have 3 members (operator + vehicle + AI)
        coordinator.min_size = 3;
        coordinator.min_readiness = 0.7;

        Self {
            team_id,
            coordinator,
            election_policy,
            formation_start: Instant::now(),
            members: Vec::new(),
            leader_id: None,
        }
    }

    /// Add a platform to the formation
    pub fn add_platform(&mut self, platform: &Platform) {
        let (config, state) = platform_to_node(platform);
        self.members.push((config, state, None));
    }

    /// Check if the election policy allows this platform to be leader
    fn is_qualified_leader(&self, config: &NodeConfig) -> bool {
        if let Some(op) = config.get_primary_operator() {
            self.election_policy.is_qualified_leader(op)
        } else {
            // Non-human platforms can only lead if autonomous leaders are allowed
            self.election_policy.allows_autonomous_leader()
        }
    }

    /// Calculate leadership score for a platform
    fn calculate_leadership_score(&self, config: &NodeConfig) -> f64 {
        let context =
            ElectionContext::new(self.election_policy.default_policy.clone(), Phase::Cell);

        let (authority_weight, technical_weight) =
            self.election_policy.default_policy.get_weights(&context);

        // Calculate authority score (from operator rank)
        let authority_score = if let Some(op) = config.get_primary_operator() {
            // Higher rank = higher score
            let rank = OperatorRank::try_from(op.rank).unwrap_or(OperatorRank::E1);
            match rank {
                OperatorRank::E9 | OperatorRank::O10 => 1.0,
                OperatorRank::E8 | OperatorRank::O9 => 0.95,
                OperatorRank::E7 | OperatorRank::O8 => 0.9,
                OperatorRank::E6 | OperatorRank::O7 => 0.85,
                OperatorRank::E5 | OperatorRank::O6 => 0.8,
                OperatorRank::E4 | OperatorRank::O5 => 0.7,
                OperatorRank::E3 | OperatorRank::O4 => 0.6,
                OperatorRank::E2 | OperatorRank::O3 => 0.5,
                OperatorRank::E1 | OperatorRank::O2 => 0.4,
                _ => 0.3,
            }
        } else {
            0.0 // Non-human platforms get 0 authority score
        };

        // Calculate technical score (from capabilities)
        let technical_score = {
            let mut score = 0.0;

            // Communication capability (25% of technical)
            if config.has_capability_type(CapabilityType::Communication) {
                score += 0.25;
            }

            // Compute capability (30% of technical)
            if config.has_capability_type(CapabilityType::Compute) {
                let compute_confidence = config
                    .get_capabilities_by_type(CapabilityType::Compute)
                    .first()
                    .map(|c| c.confidence as f64)
                    .unwrap_or(0.0);
                score += 0.30 * compute_confidence;
            }

            // Sensor capability (20% of technical)
            if config.has_capability_type(CapabilityType::Sensor) {
                score += 0.20;
            }

            // Mobility capability (15% of technical)
            if config.has_capability_type(CapabilityType::Mobility) {
                score += 0.15;
            }

            score.min(1.0)
        };

        // Weighted combination
        authority_score * authority_weight + technical_score * technical_weight
    }

    /// Elect a leader based on the election policy
    pub fn elect_leader(&mut self) -> Option<String> {
        // Score each candidate and find the best
        let mut best_leader: Option<(String, f64)> = None;

        for (config, _, _) in &self.members {
            // Check if qualified
            if !self.is_qualified_leader(config) {
                continue;
            }

            let score = self.calculate_leadership_score(config);

            if best_leader.is_none() || score > best_leader.as_ref().unwrap().1 {
                best_leader = Some((config.id.clone(), score));
            } else if (score - best_leader.as_ref().unwrap().1).abs() < 0.001 {
                // Tie-break by ID (deterministic)
                if config.id < best_leader.as_ref().unwrap().0 {
                    best_leader = Some((config.id.clone(), score));
                }
            }
        }

        // Set leader role
        if let Some((ref leader_id, _)) = best_leader {
            self.leader_id = Some(leader_id.clone());

            // Update member roles
            for (config, _, role) in &mut self.members {
                if &config.id == leader_id {
                    *role = Some(CellRole::Leader);
                }
            }
        }

        self.leader_id.clone()
    }

    /// Assign roles to non-leader members based on their capabilities
    pub fn assign_roles(&mut self) {
        for (config, state, role) in &mut self.members {
            // Skip if already assigned (leader)
            if role.is_some() {
                continue;
            }

            // Use RoleScorer to find best role
            if let Some((best_role, _score)) = RoleScorer::best_role_for_platform(config, state) {
                *role = Some(best_role);
            } else {
                *role = Some(CellRole::Follower);
            }
        }
    }

    /// Check if formation is complete
    pub fn check_formation_complete(&mut self) -> Result<bool, hive_protocol::Error> {
        self.coordinator
            .check_formation_complete(&self.members, self.leader_id.as_deref())
    }

    /// Get formation status
    pub fn status(&self) -> &FormationStatus {
        &self.coordinator.status
    }

    /// Get formation duration
    pub fn formation_duration(&self) -> Duration {
        self.formation_start.elapsed()
    }

    /// Check if formation completed within target time (30 seconds for M1)
    pub fn met_timing_requirement(&self, target_secs: u64) -> bool {
        self.formation_start.elapsed().as_secs() <= target_secs
    }

    /// Get the elected leader ID
    pub fn leader(&self) -> Option<&str> {
        self.leader_id.as_deref()
    }

    /// Get aggregated capabilities
    pub fn get_aggregated_capabilities(
        &self,
    ) -> Result<HashMap<CapabilityType, AggregatedCapability>, hive_protocol::Error> {
        let members_for_agg: Vec<(NodeConfig, NodeState)> = self
            .members
            .iter()
            .map(|(c, s, _)| (c.clone(), s.clone()))
            .collect();

        CapabilityAggregator::aggregate_capabilities(&members_for_agg)
    }

    /// Calculate team readiness score
    pub fn readiness_score(&self) -> f32 {
        if let Ok(caps) = self.get_aggregated_capabilities() {
            CapabilityAggregator::calculate_readiness_score(&caps)
        } else {
            0.0
        }
    }
}

// ============================================================================
// Team Struct (Enhanced)
// ============================================================================

/// A team (cell) consisting of human-machine-AI members
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Member platforms
    pub members: Vec<Platform>,
    /// Current leader (if elected)
    pub leader_id: Option<String>,
    /// Assigned roles for each member
    #[serde(default)]
    pub member_roles: HashMap<String, CellRole>,
    /// Formation status
    #[serde(default)]
    pub formation_status: TeamFormationStatus,
    /// Formation completed timestamp (for timing verification)
    pub formation_completed_at: Option<std::time::SystemTime>,
}

/// Team formation status for serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TeamFormationStatus {
    /// Formation not started
    #[default]
    NotStarted,
    /// Formation in progress
    Forming,
    /// Awaiting human approval
    AwaitingApproval,
    /// Formation complete and ready
    Ready,
    /// Formation failed
    Failed,
}

impl From<&FormationStatus> for TeamFormationStatus {
    fn from(status: &FormationStatus) -> Self {
        match status {
            FormationStatus::Forming => TeamFormationStatus::Forming,
            FormationStatus::AwaitingApproval => TeamFormationStatus::AwaitingApproval,
            FormationStatus::Ready => TeamFormationStatus::Ready,
            FormationStatus::Failed(_) => TeamFormationStatus::Failed,
        }
    }
}

impl Team {
    /// Create a new team
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            members: Vec::new(),
            leader_id: None,
            member_roles: HashMap::new(),
            formation_status: TeamFormationStatus::NotStarted,
            formation_completed_at: None,
        }
    }

    /// Add a platform to the team
    pub fn add_member(&mut self, platform: Platform) {
        self.members.push(platform);
    }

    /// Get the number of members
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Get raw aggregated capabilities from all team members (flat list)
    pub fn get_aggregated_capabilities(&self) -> Vec<Capability> {
        self.members
            .iter()
            .flat_map(|m| m.get_capabilities())
            .collect()
    }

    /// Get a member by ID
    pub fn get_member(&self, id: &str) -> Option<&Platform> {
        self.members.iter().find(|m| m.id() == id)
    }

    /// Get a mutable reference to a member by ID
    pub fn get_member_mut(&mut self, id: &str) -> Option<&mut Platform> {
        self.members.iter_mut().find(|m| m.id() == id)
    }

    /// Form the team using hive-protocol's cell formation
    ///
    /// This performs leader election (preferring human operators),
    /// role assignment, and capability aggregation.
    ///
    /// Returns true if formation succeeded within 30 seconds.
    pub fn form_team(&mut self) -> Result<bool, hive_protocol::Error> {
        self.formation_status = TeamFormationStatus::Forming;

        let mut formation = TeamFormation::new(self.id.to_string());

        // Add all members to formation
        for member in &self.members {
            formation.add_platform(member);
        }

        // Elect leader (prefers human operators via RankDominant policy)
        if let Some(leader_id) = formation.elect_leader() {
            self.leader_id = Some(leader_id);
        }

        // Assign roles to other members
        formation.assign_roles();

        // Store assigned roles
        for (config, _, role) in &formation.members {
            if let Some(role) = role {
                self.member_roles.insert(config.id.clone(), *role);
            }
        }

        // Check formation complete
        let complete = formation.check_formation_complete()?;

        if complete {
            self.formation_status = TeamFormationStatus::Ready;
            self.formation_completed_at = Some(std::time::SystemTime::now());
        } else {
            self.formation_status = formation.status().into();
        }

        // Check if we met the 30-second requirement
        Ok(complete && formation.met_timing_requirement(30))
    }

    /// Get the leader platform
    pub fn get_leader(&self) -> Option<&Platform> {
        self.leader_id.as_ref().and_then(|id| self.get_member(id))
    }

    /// Check if the leader is a human operator
    pub fn has_human_leader(&self) -> bool {
        if let Some(leader) = self.get_leader() {
            matches!(leader.platform_type(), PlatformType::Operator)
        } else {
            false
        }
    }

    /// Get the assigned role for a member
    pub fn get_member_role(&self, member_id: &str) -> Option<CellRole> {
        self.member_roles.get(member_id).copied()
    }

    /// Generate a capability summary for advertising
    pub fn get_capability_summary(&self) -> TeamCapabilitySummary {
        let mut camera_count = 0;
        let mut ai_models = Vec::new();
        let mut has_communication = false;
        let mut has_hitl = false;

        for member in &self.members {
            match member {
                Platform::Operator(_) => {
                    has_hitl = true;
                    has_communication = true; // Operators have TAK comms
                }
                Platform::Vehicle(veh) => {
                    camera_count += veh.sensors.len();
                    has_communication = true; // Vehicles have mesh comms
                }
                Platform::AiModel(ai) => {
                    ai_models.push(AiModelSummary {
                        model_id: ai.model.model_id.clone(),
                        version: ai.model.version.clone(),
                        precision: ai.model.precision,
                    });
                }
            }
        }

        // Calculate readiness score using hive-protocol
        let readiness_score = {
            let mut formation = TeamFormation::new("temp");
            for member in &self.members {
                formation.add_platform(member);
            }
            formation.readiness_score()
        };

        TeamCapabilitySummary {
            camera_count,
            ai_models,
            has_communication,
            has_hitl,
            readiness_score,
        }
    }

    /// Check if the team meets M1 vignette success criteria
    pub fn meets_m1_criteria(&self) -> M1CriteriaResult {
        let summary = self.get_capability_summary();
        let has_human_leader = self.has_human_leader();
        let formed_in_time = self.formation_status == TeamFormationStatus::Ready;

        M1CriteriaResult {
            has_camera: summary.camera_count > 0,
            has_object_tracker: !summary.ai_models.is_empty(),
            has_human_leader,
            formed_in_time,
            capability_summary: summary.to_string(),
            all_passed: summary.camera_count > 0
                && !summary.ai_models.is_empty()
                && has_human_leader
                && formed_in_time,
        }
    }
}

/// Result of M1 vignette criteria check
#[derive(Debug, Clone)]
pub struct M1CriteriaResult {
    /// Has at least one camera
    pub has_camera: bool,
    /// Has at least one object tracker
    pub has_object_tracker: bool,
    /// Human operator is the team leader
    pub has_human_leader: bool,
    /// Team formed within 30 seconds
    pub formed_in_time: bool,
    /// Capability summary string
    pub capability_summary: String,
    /// All criteria passed
    pub all_passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        AiModelInfo, AiModelPlatform, AuthorityLevel, OperatorPlatform, SensorCapability,
        VehiclePlatform,
    };

    /// Create a standard M1 vignette team (operator + vehicle + AI)
    fn create_m1_team() -> Team {
        let mut team = Team::new("Alpha Team");

        // Human operator with Commander authority
        team.add_member(Platform::Operator(OperatorPlatform::new(
            "Alpha-1",
            "ALPHA-1",
            AuthorityLevel::Commander,
        )));

        // UGV with camera sensor
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_ugv("Alpha-2")
                .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
        ));

        // AI model (object tracker)
        team.add_member(Platform::AiModel(AiModelPlatform::new(
            "Alpha-3",
            AiModelInfo::object_tracker("1.2.0"),
        )));

        team
    }

    #[test]
    fn test_team_creation() {
        let mut team = Team::new("Alpha Team");
        assert_eq!(team.name, "Alpha Team");
        assert_eq!(team.member_count(), 0);
        assert_eq!(team.formation_status, TeamFormationStatus::NotStarted);

        let operator = Platform::Operator(OperatorPlatform::new(
            "Alpha-1",
            "ALPHA-1",
            AuthorityLevel::Commander,
        ));
        let uav = Platform::Vehicle(VehiclePlatform::new_uav("Alpha-2"));

        team.add_member(operator);
        team.add_member(uav);

        assert_eq!(team.member_count(), 2);
    }

    #[test]
    fn test_team_capability_aggregation() {
        let mut team = Team::new("Test Team");

        // Add operator (2 capabilities: HITL + Communication)
        team.add_member(Platform::Operator(OperatorPlatform::new(
            "Op1",
            "OP1",
            AuthorityLevel::Commander,
        )));

        // Add vehicle with sensor (3 capabilities: Mobility + Sensor + Communication)
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_ugv("UGV1")
                .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
        ));

        // Add AI model (1 capability: Compute)
        team.add_member(Platform::AiModel(AiModelPlatform::new(
            "AI1",
            AiModelInfo::object_tracker("1.0.0"),
        )));

        let caps = team.get_aggregated_capabilities();
        assert_eq!(caps.len(), 6); // 2 + 3 + 1
    }

    #[test]
    fn test_team_get_member() {
        let mut team = Team::new("Test Team");

        let op = Platform::Operator(OperatorPlatform::new(
            "FindMe",
            "FINDME",
            AuthorityLevel::Supervisor,
        ));
        let op_id = op.id().to_string();

        team.add_member(op);

        assert!(team.get_member(&op_id).is_some());
        assert!(team.get_member("nonexistent").is_none());
    }

    // ========================================================================
    // New tests for Issue #4: Team formation with leader election
    // ========================================================================

    #[test]
    fn test_team_formation_elects_human_leader() {
        let mut team = create_m1_team();
        let operator_id = team.members[0].id().to_string();

        // Form the team
        let result = team.form_team();
        assert!(result.is_ok());

        // Verify human operator was elected as leader
        assert!(team.has_human_leader());
        assert_eq!(team.leader_id, Some(operator_id));

        // Leader should have Leader role
        if let Some(leader_id) = &team.leader_id {
            assert_eq!(team.get_member_role(leader_id), Some(CellRole::Leader));
        }
    }

    #[test]
    fn test_team_formation_assigns_roles() {
        let mut team = create_m1_team();

        team.form_team().unwrap();

        // All members should have roles assigned
        for member in &team.members {
            let role = team.get_member_role(member.id());
            assert!(role.is_some(), "Member {} should have a role", member.id());
        }

        // Vehicle should be Sensor role (has sensor capability)
        let vehicle = &team.members[1];
        let vehicle_role = team.get_member_role(vehicle.id());
        assert!(
            matches!(
                vehicle_role,
                Some(CellRole::Sensor) | Some(CellRole::Relay) | Some(CellRole::Follower)
            ),
            "Vehicle should have Sensor, Relay, or Follower role"
        );
    }

    #[test]
    fn test_team_formation_prefers_human_over_ai() {
        let mut team = Team::new("Mixed Team");

        // Add AI first
        team.add_member(Platform::AiModel(AiModelPlatform::new(
            "AI-1",
            AiModelInfo::object_tracker("1.0.0"),
        )));

        // Add vehicle second
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_uav("UAV-1")
                .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0)),
        ));

        // Add human last (should still be elected leader due to policy)
        team.add_member(Platform::Operator(OperatorPlatform::new(
            "Human-1",
            "H1",
            AuthorityLevel::Commander,
        )));

        team.form_team().unwrap();

        // Human should be leader despite being added last
        assert!(team.has_human_leader());
    }

    #[test]
    fn test_team_capability_summary() {
        let mut team = create_m1_team();
        team.form_team().unwrap();

        let summary = team.get_capability_summary();

        assert_eq!(summary.camera_count, 1);
        assert_eq!(summary.ai_models.len(), 1);
        assert!(summary.has_communication);
        assert!(summary.has_hitl);
        assert!(summary.readiness_score > 0.0);

        // Check summary string matches expected format
        let summary_str = summary.to_string();
        assert!(summary_str.contains("1 camera"));
        assert!(summary_str.contains("object_tracker"));
        assert!(summary_str.contains("v1.2.0"));
        assert!(summary_str.contains("precision 0.91"));
    }

    #[test]
    fn test_team_meets_m1_criteria() {
        let mut team = create_m1_team();
        team.form_team().unwrap();

        let criteria = team.meets_m1_criteria();

        assert!(criteria.has_camera, "Should have camera");
        assert!(criteria.has_object_tracker, "Should have object tracker");
        assert!(criteria.has_human_leader, "Should have human leader");
        assert!(criteria.formed_in_time, "Should form in time");
        assert!(criteria.all_passed, "All criteria should pass");

        println!("Capability summary: {}", criteria.capability_summary);
    }

    #[test]
    fn test_team_formation_status_transitions() {
        let mut team = create_m1_team();

        assert_eq!(team.formation_status, TeamFormationStatus::NotStarted);

        team.form_team().unwrap();

        // Formation should complete successfully
        assert_eq!(team.formation_status, TeamFormationStatus::Ready);
        assert!(team.formation_completed_at.is_some());
    }

    #[test]
    fn test_team_formation_without_human_fails_leader_election() {
        let mut team = Team::new("Autonomous Team");

        // Only add non-human members
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_ugv("UGV-1")
                .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
        ));
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_uav("UAV-1")
                .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0)),
        ));
        team.add_member(Platform::AiModel(AiModelPlatform::new(
            "AI-1",
            AiModelInfo::object_tracker("1.0.0"),
        )));

        team.form_team().unwrap();

        // Should not have a human leader (policy prevents autonomous leaders)
        assert!(!team.has_human_leader());
        assert!(team.leader_id.is_none());
    }

    #[test]
    fn test_team_formation_manager_directly() {
        let mut formation = TeamFormation::new("test-team");

        // Add M1 vignette members
        let operator = Platform::Operator(OperatorPlatform::new(
            "Op1",
            "OP1",
            AuthorityLevel::Commander,
        ));
        let vehicle = Platform::Vehicle(
            VehiclePlatform::new_ugv("UGV1")
                .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
        );
        let ai = Platform::AiModel(AiModelPlatform::new(
            "AI1",
            AiModelInfo::object_tracker("1.0.0"),
        ));

        formation.add_platform(&operator);
        formation.add_platform(&vehicle);
        formation.add_platform(&ai);

        // Elect leader
        let leader = formation.elect_leader();
        assert!(leader.is_some());

        // Assign roles
        formation.assign_roles();

        // Check readiness
        let readiness = formation.readiness_score();
        assert!(
            readiness > 0.5,
            "Readiness should be > 0.5, got {}",
            readiness
        );

        // Verify timing
        assert!(formation.met_timing_requirement(30));
    }

    #[test]
    fn test_authority_level_affects_leader_election() {
        // Test that higher authority levels win leadership
        let mut team = Team::new("Authority Test");

        // Add Observer-level operator first
        team.add_member(Platform::Operator(OperatorPlatform::new(
            "Observer-1",
            "OBS1",
            AuthorityLevel::Observer,
        )));

        // Add vehicle
        team.add_member(Platform::Vehicle(
            VehiclePlatform::new_ugv("UGV1")
                .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
        ));

        // Add Commander-level operator second
        team.add_member(Platform::Operator(OperatorPlatform::new(
            "Commander-1",
            "CMD1",
            AuthorityLevel::Commander,
        )));

        team.form_team().unwrap();

        // Commander should be elected despite being added later
        if let Some(Platform::Operator(op)) = team.get_leader() {
            assert_eq!(
                op.authority,
                AuthorityLevel::Commander,
                "Commander should be elected as leader"
            );
        }
    }

    #[test]
    fn test_platform_to_node_conversion() {
        let operator = Platform::Operator(OperatorPlatform::new(
            "Op1",
            "OP1",
            AuthorityLevel::Commander,
        ));

        let (config, _state) = platform_to_node(&operator);

        assert_eq!(config.platform_type, "Operator");
        assert!(config.operator_binding.is_some());
        assert!(config.is_human_operated());

        // Check capabilities were converted
        assert!(!config.capabilities.is_empty());
    }

    #[test]
    fn test_capability_summary_string_format() {
        let summary = TeamCapabilitySummary {
            camera_count: 1,
            ai_models: vec![AiModelSummary {
                model_id: "object_tracker".to_string(),
                version: "1.2.0".to_string(),
                precision: 0.91,
            }],
            has_communication: true,
            has_hitl: true,
            readiness_score: 0.85,
        };

        let str = summary.to_string();
        // Expected: "1 camera, object_tracker v1.2.0, precision 0.91"
        assert!(str.contains("1 camera"));
        assert!(str.contains("object_tracker v1.2.0"));
        assert!(str.contains("precision 0.91"));
    }
}
