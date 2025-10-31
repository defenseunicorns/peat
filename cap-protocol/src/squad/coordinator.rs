//! Squad Formation Coordinator (E4.5)
//!
//! Coordinates squad formation completion by integrating role assignment (E4.3) and
//! capability aggregation (E4.4). Detects when formation is complete and manages
//! phase transitions with human approval workflow following ADR-004.
//!
//! # Formation Completion Criteria
//!
//! A squad formation is considered complete when:
//! 1. Minimum squad size met (configurable, default 3)
//! 2. Leader elected and confirmed
//! 3. All members have assigned roles
//! 4. Minimum capability coverage achieved (Communication + Sensor required)
//! 5. Squad readiness score above threshold (default 0.7)
//! 6. Human approval obtained (if required by authority levels)
//!
//! # Phase Transition Workflow
//!
//! SquadFormation -> OperationalReady (with human approval if needed):
//! - Check formation completion criteria
//! - Calculate squad readiness score
//! - Request human approval if any mission-critical capabilities lack DirectControl authority
//! - Transition to OperationalReady once approved

use crate::models::{PlatformConfig, PlatformState, SquadRole};
use crate::squad::{AggregatedCapability, CapabilityAggregator};
use crate::traits::Phase;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Squad formation status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormationStatus {
    /// Formation in progress
    Forming,
    /// Formation complete, awaiting human approval
    AwaitingApproval,
    /// Formation complete and approved, ready for operations
    Ready,
    /// Formation failed or incomplete
    Failed(String),
}

/// Squad formation coordinator
pub struct SquadCoordinator {
    /// Squad ID
    pub squad_id: String,
    /// Minimum squad size
    pub min_size: usize,
    /// Minimum readiness score (0.0-1.0)
    pub min_readiness: f32,
    /// Required capability types for formation
    pub required_capabilities: Vec<crate::models::CapabilityType>,
    /// Formation status
    pub status: FormationStatus,
    /// Human approval received
    pub human_approved: bool,
    /// Formation start timestamp
    pub formation_start: u64,
    /// Formation completion timestamp
    pub formation_complete: Option<u64>,
}

impl SquadCoordinator {
    /// Create a new squad coordinator
    pub fn new(squad_id: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            squad_id,
            min_size: 3,
            min_readiness: 0.7,
            required_capabilities: vec![
                crate::models::CapabilityType::Communication,
                crate::models::CapabilityType::Sensor,
            ],
            status: FormationStatus::Forming,
            human_approved: false,
            formation_start: now,
            formation_complete: None,
        }
    }

    /// Check if formation is complete
    ///
    /// # Arguments
    /// * `members` - Squad members (config, state, role)
    /// * `leader_id` - Optional ID of elected leader
    ///
    /// # Returns
    /// True if formation meets all criteria
    pub fn check_formation_complete(
        &mut self,
        members: &[(PlatformConfig, PlatformState, Option<SquadRole>)],
        leader_id: Option<&str>,
    ) -> Result<bool> {
        // Criterion 1: Minimum size
        if members.len() < self.min_size {
            self.status = FormationStatus::Failed(format!(
                "Insufficient members: {} < {}",
                members.len(),
                self.min_size
            ));
            return Ok(false);
        }

        // Criterion 2: Leader elected
        if leader_id.is_none() {
            return Ok(false); // Still forming, no failure
        }

        // Criterion 3: All members have roles
        let unassigned = members.iter().filter(|(_, _, role)| role.is_none()).count();
        if unassigned > 0 {
            return Ok(false); // Still assigning roles
        }

        // Criterion 4 & 5: Capability coverage and readiness
        let members_for_agg: Vec<(PlatformConfig, PlatformState)> = members
            .iter()
            .map(|(c, s, _)| (c.clone(), s.clone()))
            .collect();

        let capabilities = CapabilityAggregator::aggregate_capabilities(&members_for_agg)?;

        // Check required capabilities
        let gaps =
            CapabilityAggregator::identify_gaps(&capabilities, &self.required_capabilities);
        if !gaps.is_empty() {
            self.status = FormationStatus::Failed(format!(
                "Missing required capabilities: {:?}",
                gaps
            ));
            return Ok(false);
        }

        // Check readiness score
        let readiness = CapabilityAggregator::calculate_readiness_score(&capabilities);
        if readiness < self.min_readiness {
            self.status = FormationStatus::Failed(format!(
                "Insufficient readiness: {:.2} < {:.2}",
                readiness, self.min_readiness
            ));
            return Ok(false);
        }

        // Formation criteria met - check if human approval needed
        let needs_approval = self.needs_human_approval(&capabilities);

        if needs_approval && !self.human_approved {
            self.status = FormationStatus::AwaitingApproval;
            return Ok(false); // Complete but awaiting approval
        }

        // All criteria met
        self.status = FormationStatus::Ready;
        if self.formation_complete.is_none() {
            self.formation_complete = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }

        Ok(true)
    }

    /// Check if human approval is needed for formation
    ///
    /// Approval required if any oversight-required capabilities are present
    fn needs_human_approval(
        &self,
        capabilities: &HashMap<crate::models::CapabilityType, AggregatedCapability>,
    ) -> bool {
        capabilities
            .values()
            .any(|cap| cap.requires_oversight)
    }

    /// Approve formation (human operator decision)
    pub fn approve_formation(&mut self) -> Result<()> {
        if self.status != FormationStatus::AwaitingApproval {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "Ready".to_string(),
                reason: "Cannot approve formation not awaiting approval".to_string(),
            });
        }

        self.human_approved = true;
        self.status = FormationStatus::Ready;

        if self.formation_complete.is_none() {
            self.formation_complete = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }

        Ok(())
    }

    /// Reject formation (human operator decision)
    pub fn reject_formation(&mut self, reason: String) -> Result<()> {
        if self.status != FormationStatus::AwaitingApproval {
            return Err(Error::InvalidTransition {
                from: format!("{:?}", self.status),
                to: "Failed".to_string(),
                reason: "Cannot reject formation not awaiting approval".to_string(),
            });
        }

        self.status = FormationStatus::Failed(format!("Human rejected: {}", reason));
        Ok(())
    }

    /// Get formation duration in seconds
    pub fn formation_duration(&self) -> u64 {
        let end = self.formation_complete.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });

        end - self.formation_start
    }

    /// Check if squad can transition to Hierarchical phase
    pub fn can_transition_to_hierarchical(&self) -> bool {
        self.status == FormationStatus::Ready
    }

    /// Get transition to Hierarchical phase
    pub fn get_hierarchical_phase(&self) -> Result<Phase> {
        if !self.can_transition_to_hierarchical() {
            return Err(Error::InvalidTransition {
                from: "Squad".to_string(),
                to: "Hierarchical".to_string(),
                reason: format!("Cannot transition with status: {:?}", self.status),
            });
        }

        Ok(Phase::Hierarchical)
    }

    /// Reset formation (for retry scenarios)
    pub fn reset(&mut self) {
        self.status = FormationStatus::Forming;
        self.human_approved = false;
        self.formation_complete = None;
        self.formation_start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AuthorityLevel, Capability, CapabilityType, HumanMachinePair, Operator, OperatorRank,
    };

    fn create_test_member(
        id: &str,
        capabilities: Vec<CapabilityType>,
        role: Option<SquadRole>,
        operator: Option<Operator>,
    ) -> (PlatformConfig, PlatformState, Option<SquadRole>) {
        let mut config = PlatformConfig::new("Test".to_string());
        config.id = id.to_string();

        for cap_type in capabilities {
            config.add_capability(Capability::new(
                format!("{}_{:?}", id, cap_type),
                format!("{:?}", cap_type),
                cap_type,
                0.9,
            ));
        }

        if let Some(op) = operator {
            let binding = HumanMachinePair::new(
                vec![op],
                vec![id.to_string()],
                crate::models::BindingType::OneToOne,
            );
            config.operator_binding = Some(binding);
        }

        let state = PlatformState::new((0.0, 0.0, 0.0));

        (config, state, role)
    }

    #[test]
    fn test_coordinator_creation() {
        let coord = SquadCoordinator::new("squad1".to_string());
        assert_eq!(coord.squad_id, "squad1");
        assert_eq!(coord.status, FormationStatus::Forming);
        assert_eq!(coord.min_size, 3);
        assert!(!coord.human_approved);
    }

    #[test]
    fn test_insufficient_members() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication, CapabilityType::Sensor],
                Some(SquadRole::Leader),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(SquadRole::Sensor),
                None,
            ),
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(!complete);
        assert!(matches!(
            coord.status,
            FormationStatus::Failed(ref msg) if msg.contains("Insufficient members")
        ));
    }

    #[test]
    fn test_no_leader() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(SquadRole::Follower),
                None,
            ),
            create_test_member("p2", vec![CapabilityType::Sensor], Some(SquadRole::Sensor), None),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(SquadRole::Compute),
                None,
            ),
        ];

        let complete = coord.check_formation_complete(&members, None).unwrap();

        assert!(!complete);
        assert_eq!(coord.status, FormationStatus::Forming); // Not failed, just incomplete
    }

    #[test]
    fn test_unassigned_roles() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(SquadRole::Leader),
                None,
            ),
            create_test_member("p2", vec![CapabilityType::Sensor], Some(SquadRole::Sensor), None),
            create_test_member("p3", vec![CapabilityType::Compute], None, None), // No role
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(!complete);
        assert_eq!(coord.status, FormationStatus::Forming);
    }

    #[test]
    fn test_missing_required_capabilities() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(SquadRole::Leader),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Compute],
                Some(SquadRole::Compute),
                None,
            ), // Missing Sensor
            create_test_member(
                "p3",
                vec![CapabilityType::Mobility],
                Some(SquadRole::Follower),
                None,
            ),
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(!complete);
        assert!(matches!(
            coord.status,
            FormationStatus::Failed(ref msg) if msg.contains("Missing required capabilities")
        ));
    }

    #[test]
    fn test_formation_complete_no_approval_needed() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        // Create operator with Commander authority for Communication
        let operator = Operator::new(
            "op1".to_string(),
            "Test Operator".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication, CapabilityType::Sensor],
                Some(SquadRole::Leader),
                Some(operator),
            ),
            create_test_member("p2", vec![CapabilityType::Sensor], Some(SquadRole::Sensor), None),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(SquadRole::Compute),
                None,
            ),
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(complete);
        assert_eq!(coord.status, FormationStatus::Ready);
        assert!(coord.formation_complete.is_some());
    }

    #[test]
    fn test_formation_awaiting_approval() {
        let mut coord = SquadCoordinator::new("squad1".to_string());

        // Create operator with Commander authority for Communication (p1)
        let operator1 = Operator::new(
            "op1".to_string(),
            "Test Operator 1".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        // Create operator with Observer authority for Payload (p3) - requires oversight
        let operator3 = Operator::new(
            "op3".to_string(),
            "Test Operator 3".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Observer, // Observer on Payload requires oversight
            "11B".to_string(),
        );

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication, CapabilityType::Sensor],
                Some(SquadRole::Leader),
                Some(operator1),
            ),
            create_test_member("p2", vec![CapabilityType::Sensor], Some(SquadRole::Sensor), None),
            create_test_member(
                "p3",
                vec![CapabilityType::Payload], // Oversight-required with low authority
                Some(SquadRole::Follower),
                Some(operator3),
            ),
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(!complete); // Not complete until approved
        assert_eq!(coord.status, FormationStatus::AwaitingApproval);
    }

    #[test]
    fn test_human_approval_workflow() {
        let mut coord = SquadCoordinator::new("squad1".to_string());
        coord.status = FormationStatus::AwaitingApproval;

        coord.approve_formation().unwrap();

        assert_eq!(coord.status, FormationStatus::Ready);
        assert!(coord.human_approved);
        assert!(coord.formation_complete.is_some());
    }

    #[test]
    fn test_human_rejection() {
        let mut coord = SquadCoordinator::new("squad1".to_string());
        coord.status = FormationStatus::AwaitingApproval;

        coord
            .reject_formation("Insufficient capability coverage".to_string())
            .unwrap();

        assert!(matches!(
            coord.status,
            FormationStatus::Failed(ref msg) if msg.contains("Human rejected")
        ));
    }

    #[test]
    fn test_phase_transition() {
        let mut coord = SquadCoordinator::new("squad1".to_string());
        coord.status = FormationStatus::Ready;

        assert!(coord.can_transition_to_hierarchical());

        let phase = coord.get_hierarchical_phase().unwrap();
        assert_eq!(phase, Phase::Hierarchical);
    }

    #[test]
    fn test_cannot_transition_when_not_ready() {
        let coord = SquadCoordinator::new("squad1".to_string());

        assert!(!coord.can_transition_to_hierarchical());
        assert!(coord.get_hierarchical_phase().is_err());
    }

    #[test]
    fn test_formation_duration() {
        let coord = SquadCoordinator::new("squad1".to_string());

        std::thread::sleep(std::time::Duration::from_secs(1));

        let duration = coord.formation_duration();
        assert!(duration >= 1);
    }

    #[test]
    fn test_reset_formation() {
        let mut coord = SquadCoordinator::new("squad1".to_string());
        coord.status = FormationStatus::Ready;
        coord.human_approved = true;
        coord.formation_complete = Some(12345);

        coord.reset();

        assert_eq!(coord.status, FormationStatus::Forming);
        assert!(!coord.human_approved);
        assert!(coord.formation_complete.is_none());
    }

    #[test]
    fn test_single_member_squad() {
        // Edge case: Single member (< min_size)
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let operator = Operator::new(
            "op1".to_string(),
            "Test Operator".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let members = vec![create_test_member(
            "p1",
            vec![CapabilityType::Communication, CapabilityType::Sensor],
            Some(SquadRole::Leader),
            Some(operator),
        )];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        assert!(!complete);
        assert!(matches!(
            coord.status,
            FormationStatus::Failed(ref msg) if msg.contains("Insufficient members")
        ));
    }

    #[test]
    fn test_exact_minimum_size_squad() {
        // Boundary case: Exactly min_size (3) members
        let mut coord = SquadCoordinator::new("squad1".to_string());
        assert_eq!(coord.min_size, 3);

        let operator = Operator::new(
            "op1".to_string(),
            "Test Operator".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication, CapabilityType::Sensor],
                Some(SquadRole::Leader),
                Some(operator),
            ),
            create_test_member("p2", vec![CapabilityType::Sensor], Some(SquadRole::Sensor), None),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(SquadRole::Compute),
                None,
            ),
        ];

        let complete = coord
            .check_formation_complete(&members, Some("p1"))
            .unwrap();

        // Should succeed with exactly min_size
        assert!(complete);
        assert_eq!(coord.status, FormationStatus::Ready);
    }

    #[test]
    fn test_empty_squad_formation() {
        // Edge case: Zero members
        let mut coord = SquadCoordinator::new("squad1".to_string());

        let complete = coord.check_formation_complete(&[], None).unwrap();

        assert!(!complete);
        assert!(matches!(
            coord.status,
            FormationStatus::Failed(ref msg) if msg.contains("Insufficient members: 0 < 3")
        ));
    }

    #[test]
    fn test_approval_idempotency() {
        // Edge case: Multiple approval calls should be idempotent
        let mut coord = SquadCoordinator::new("squad1".to_string());
        coord.status = FormationStatus::AwaitingApproval;

        // First approval
        coord.approve_formation().unwrap();
        assert_eq!(coord.status, FormationStatus::Ready);
        assert!(coord.human_approved);

        // Second approval should fail (not awaiting approval anymore)
        let result = coord.approve_formation();
        assert!(result.is_err());
    }
}

/// End-to-end integration tests for full squad formation flow
#[cfg(test)]
mod e2e_integration_tests {
    use super::*;
    use crate::models::role::{RoleAllocator, RoleScorer};
    use crate::models::{HealthStatus, HumanMachinePair, OperatorRank};

    /// Test scenario configuration for E2E testing
    struct SquadFormationScenario {
        name: &'static str,
        squad_size: usize,
        include_operators: bool,
        authority_levels: Vec<Option<AuthorityLevel>>,
        health_statuses: Vec<HealthStatus>,
        expect_approval_required: bool,
        expect_success: bool,
        min_readiness: f32,
    }

    impl SquadFormationScenario {
        fn new_optimal() -> Self {
            Self {
                name: "Optimal: Full authority, all nominal",
                squad_size: 5,
                include_operators: true,
                authority_levels: vec![
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                ],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                ],
                expect_approval_required: false,
                expect_success: true,
                min_readiness: 0.7,
            }
        }

        fn new_mixed_authority() -> Self {
            Self {
                name: "Mixed Authority: Requires human oversight",
                squad_size: 4,
                include_operators: true,
                authority_levels: vec![
                    Some(AuthorityLevel::Commander),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::Observer),
                    Some(AuthorityLevel::Advisor),
                ],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                ],
                expect_approval_required: true,
                expect_success: true,
                min_readiness: 0.7,
            }
        }

        fn new_degraded_health() -> Self {
            Self {
                name: "Degraded Health: Mixed health statuses",
                squad_size: 4,
                include_operators: true,
                authority_levels: vec![
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                ],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Degraded,
                    HealthStatus::Nominal,
                    HealthStatus::Degraded,
                ],
                expect_approval_required: false,
                expect_success: true,
                min_readiness: 0.6, // Lower threshold for degraded scenario
            }
        }

        fn new_autonomous_only() -> Self {
            Self {
                name: "Autonomous Only: No operators",
                squad_size: 4,
                include_operators: false,
                authority_levels: vec![None, None, None, None],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                ],
                expect_approval_required: true, // Autonomous squads require oversight
                expect_success: true,
                min_readiness: 0.7,
            }
        }

        fn new_minimal_viable() -> Self {
            Self {
                name: "Minimal Viable: Exactly minimum size",
                squad_size: 3,
                include_operators: true,
                authority_levels: vec![
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                ],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                ],
                expect_approval_required: false,
                expect_success: true,
                min_readiness: 0.7,
            }
        }

        fn new_critical_platform() -> Self {
            Self {
                name: "Critical Platform: One critical health member",
                squad_size: 4,
                include_operators: true,
                authority_levels: vec![
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                    Some(AuthorityLevel::DirectControl),
                ],
                health_statuses: vec![
                    HealthStatus::Nominal,
                    HealthStatus::Critical,
                    HealthStatus::Nominal,
                    HealthStatus::Nominal,
                ],
                expect_approval_required: false,
                expect_success: true,
                min_readiness: 0.5, // Lower due to critical member
            }
        }
    }

    /// Helper to create a full platform with specified configuration
    fn create_full_platform(
        id: &str,
        capabilities: Vec<CapabilityType>,
        health: HealthStatus,
        operator_auth: Option<AuthorityLevel>,
    ) -> (PlatformConfig, PlatformState) {
        let mut config = PlatformConfig::new("Test".to_string());
        config.id = id.to_string();

        // Add capabilities
        for cap_type in capabilities {
            config.add_capability(Capability::new(
                format!("{}_{:?}", id, cap_type),
                format!("{:?}", cap_type),
                cap_type,
                0.9,
            ));
        }

        // Add operator if specified
        if let Some(auth) = operator_auth {
            let rank = match auth {
                AuthorityLevel::DirectControl => OperatorRank::E7,
                AuthorityLevel::Commander => OperatorRank::E5,
                AuthorityLevel::Observer => OperatorRank::E4,
                AuthorityLevel::Advisor => OperatorRank::E4,
            };

            let operator = Operator::new(
                format!("op_{}", id),
                format!("Operator {}", id),
                rank,
                auth,
                "11B".to_string(),
            );

            let binding = HumanMachinePair::new(
                vec![operator],
                vec![id.to_string()],
                crate::models::BindingType::OneToOne,
            );
            config.operator_binding = Some(binding);
        }

        let mut state = PlatformState::new((0.0, 0.0, 0.0));
        state.health = health;

        (config, state)
    }

    /// Run a complete E2E squad formation flow
    fn run_e2e_scenario(scenario: SquadFormationScenario) {
        println!("\n=== Running E2E Scenario: {} ===", scenario.name);

        // Step 1: Create platforms based on scenario
        let mut platforms: Vec<(PlatformConfig, PlatformState)> = Vec::new();

        let capability_distribution = vec![
            vec![CapabilityType::Communication, CapabilityType::Sensor],
            vec![CapabilityType::Sensor, CapabilityType::Compute],
            vec![CapabilityType::Payload, CapabilityType::Sensor],
            vec![CapabilityType::Communication, CapabilityType::Compute],
            vec![CapabilityType::Storage, CapabilityType::Sensor],
        ];

        for i in 0..scenario.squad_size {
            let id = format!("p{}", i + 1);
            let caps = capability_distribution[i % capability_distribution.len()].clone();
            let health = scenario.health_statuses[i].clone();
            let auth = if scenario.include_operators {
                scenario.authority_levels[i]
            } else {
                None
            };

            platforms.push(create_full_platform(&id, caps, health, auth));
        }

        println!(
            "Created {} platforms with health: {:?}",
            platforms.len(),
            scenario.health_statuses
        );

        // Step 2: Aggregate capabilities
        let aggregated = CapabilityAggregator::aggregate_capabilities(&platforms).unwrap();
        println!(
            "Aggregated {} capability types",
            aggregated.keys().len()
        );

        let readiness = CapabilityAggregator::calculate_readiness_score(&aggregated);
        println!("Squad readiness score: {:.2}", readiness);

        assert!(
            readiness >= scenario.min_readiness,
            "Readiness {} below minimum {}",
            readiness,
            scenario.min_readiness
        );

        // Step 3: Assign roles
        let roles = RoleAllocator::allocate_roles(&platforms).unwrap();
        println!("Assigned {} roles", roles.len());

        assert_eq!(
            roles.len(),
            platforms.len(),
            "All platforms should have assigned roles"
        );

        // Verify leader assignment
        let leader_id = roles
            .iter()
            .find(|(_, role)| **role == SquadRole::Leader)
            .map(|(id, _)| id.clone());
        assert!(
            leader_id.is_some(),
            "Squad should have an elected leader"
        );
        println!("Leader elected: {}", leader_id.as_ref().unwrap());

        // Step 4: Create squad coordinator
        let mut coord = SquadCoordinator::new("e2e_squad".to_string());
        coord.min_readiness = scenario.min_readiness;

        // Create member tuples
        let mut members: Vec<(PlatformConfig, PlatformState, Option<SquadRole>)> = Vec::new();
        for (config, state) in platforms {
            let role = roles.get(&config.id).cloned();
            members.push((config, state, role));
        }

        // Step 5: Check formation completion
        let complete = coord
            .check_formation_complete(&members, leader_id.as_deref())
            .unwrap();

        println!("Formation status: {:?}", coord.status);

        // Verify expectations
        if scenario.expect_approval_required {
            if scenario.expect_success {
                assert!(
                    matches!(coord.status, FormationStatus::AwaitingApproval),
                    "Expected AwaitingApproval status, got {:?}",
                    coord.status
                );
                assert!(
                    !complete,
                    "Formation should not be complete without approval"
                );

                // Step 6: Approve if awaiting
                coord.approve_formation().unwrap();
                assert_eq!(coord.status, FormationStatus::Ready);
                assert!(coord.human_approved);
                println!("Human approval granted, formation ready");
            }
        } else {
            if scenario.expect_success {
                assert!(
                    complete,
                    "Formation should be complete without approval required"
                );
                assert_eq!(coord.status, FormationStatus::Ready);
            }
        }

        // Step 7: Verify phase transition capability
        if coord.status == FormationStatus::Ready {
            assert!(coord.can_transition_to_hierarchical());
            let phase = coord.get_hierarchical_phase().unwrap();
            assert_eq!(phase, Phase::Hierarchical);
            println!("Phase transition to Hierarchical verified");
        }

        // Step 8: Verify formation duration tracking
        let duration = coord.formation_duration();
        assert!(duration >= 0, "Formation duration should be tracked");

        println!(
            "=== Scenario '{}' completed successfully ===\n",
            scenario.name
        );
    }

    #[test]
    fn test_e2e_optimal_squad_formation() {
        run_e2e_scenario(SquadFormationScenario::new_optimal());
    }

    #[test]
    fn test_e2e_mixed_authority_squad() {
        run_e2e_scenario(SquadFormationScenario::new_mixed_authority());
    }

    #[test]
    fn test_e2e_degraded_health_squad() {
        run_e2e_scenario(SquadFormationScenario::new_degraded_health());
    }

    #[test]
    fn test_e2e_autonomous_only_squad() {
        run_e2e_scenario(SquadFormationScenario::new_autonomous_only());
    }

    #[test]
    fn test_e2e_minimal_viable_squad() {
        run_e2e_scenario(SquadFormationScenario::new_minimal_viable());
    }

    #[test]
    fn test_e2e_critical_platform_squad() {
        run_e2e_scenario(SquadFormationScenario::new_critical_platform());
    }

    #[test]
    fn test_e2e_scenario_matrix() {
        // Run all scenarios in sequence to verify robustness
        println!("\n========================================");
        println!("Running Full E2E Scenario Matrix");
        println!("========================================");

        let scenarios = vec![
            SquadFormationScenario::new_optimal(),
            SquadFormationScenario::new_mixed_authority(),
            SquadFormationScenario::new_degraded_health(),
            SquadFormationScenario::new_autonomous_only(),
            SquadFormationScenario::new_minimal_viable(),
            SquadFormationScenario::new_critical_platform(),
        ];

        for scenario in scenarios {
            run_e2e_scenario(scenario);
        }

        println!("========================================");
        println!("All E2E scenarios passed!");
        println!("========================================\n");
    }
}
