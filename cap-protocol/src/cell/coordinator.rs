//! Cell Formation Coordinator (E4.5)
//!
//! Coordinates cell formation completion by integrating role assignment (E4.3) and
//! capability aggregation (E4.4). Detects when formation is complete and manages
//! phase transitions with human approval workflow following ADR-004.
//!
//! # Formation Completion Criteria
//!
//! A cell formation is considered complete when:
//! 1. Minimum squad size met (configurable, default 3)
//! 2. Leader elected and confirmed
//! 3. All members have assigned roles
//! 4. Minimum capability coverage achieved (Communication + Sensor required)
//! 5. Cell readiness score above threshold (default 0.7)
//! 6. Human approval obtained (if required by authority levels)
//!
//! # Phase Transition Workflow
//!
//! SquadFormation -> OperationalReady (with human approval if needed):
//! - Check formation completion criteria
//! - Calculate squad readiness score
//! - Request human approval if any mission-critical capabilities lack DirectControl authority
//! - Transition to OperationalReady once approved

use crate::cell::{AggregatedCapability, CapabilityAggregator};
use crate::models::{CellRole, NodeConfig, NodeState};
use crate::traits::Phase;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cell formation status
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

/// Cell formation coordinator
pub struct CellCoordinator {
    /// Cell ID
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

impl CellCoordinator {
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
    /// * `members` - Cell members (config, state, role)
    /// * `leader_id` - Optional ID of elected leader
    ///
    /// # Returns
    /// True if formation meets all criteria
    pub fn check_formation_complete(
        &mut self,
        members: &[(NodeConfig, NodeState, Option<CellRole>)],
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
        let members_for_agg: Vec<(NodeConfig, NodeState)> = members
            .iter()
            .map(|(c, s, _)| (c.clone(), s.clone()))
            .collect();

        let capabilities = CapabilityAggregator::aggregate_capabilities(&members_for_agg)?;

        // Check required capabilities
        let gaps = CapabilityAggregator::identify_gaps(&capabilities, &self.required_capabilities);
        if !gaps.is_empty() {
            self.status =
                FormationStatus::Failed(format!("Missing required capabilities: {:?}", gaps));
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
        capabilities.values().any(|cap| cap.requires_oversight)
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
        role: Option<CellRole>,
        operator: Option<Operator>,
    ) -> (NodeConfig, NodeState, Option<CellRole>) {
        let mut config = NodeConfig::new("Test".to_string());
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

        let state = NodeState::new((0.0, 0.0, 0.0));

        (config, state, role)
    }

    #[test]
    fn test_coordinator_creation() {
        let coord = CellCoordinator::new("cell1".to_string());
        assert_eq!(coord.squad_id, "cell1");
        assert_eq!(coord.status, FormationStatus::Forming);
        assert_eq!(coord.min_size, 3);
        assert!(!coord.human_approved);
    }

    #[test]
    fn test_insufficient_members() {
        let mut coord = CellCoordinator::new("cell1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication, CapabilityType::Sensor],
                Some(CellRole::Leader),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
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
        let mut coord = CellCoordinator::new("cell1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(CellRole::Follower),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
                None,
            ),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(CellRole::Compute),
                None,
            ),
        ];

        let complete = coord.check_formation_complete(&members, None).unwrap();

        assert!(!complete);
        assert_eq!(coord.status, FormationStatus::Forming); // Not failed, just incomplete
    }

    #[test]
    fn test_unassigned_roles() {
        let mut coord = CellCoordinator::new("cell1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(CellRole::Leader),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
                None,
            ),
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
        let mut coord = CellCoordinator::new("cell1".to_string());

        let members = vec![
            create_test_member(
                "p1",
                vec![CapabilityType::Communication],
                Some(CellRole::Leader),
                None,
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Compute],
                Some(CellRole::Compute),
                None,
            ), // Missing Sensor
            create_test_member(
                "p3",
                vec![CapabilityType::Mobility],
                Some(CellRole::Follower),
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
        let mut coord = CellCoordinator::new("cell1".to_string());

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
                Some(CellRole::Leader),
                Some(operator),
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
                None,
            ),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(CellRole::Compute),
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
        let mut coord = CellCoordinator::new("cell1".to_string());

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
                Some(CellRole::Leader),
                Some(operator1),
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
                None,
            ),
            create_test_member(
                "p3",
                vec![CapabilityType::Payload], // Oversight-required with low authority
                Some(CellRole::Follower),
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
        let mut coord = CellCoordinator::new("cell1".to_string());
        coord.status = FormationStatus::AwaitingApproval;

        coord.approve_formation().unwrap();

        assert_eq!(coord.status, FormationStatus::Ready);
        assert!(coord.human_approved);
        assert!(coord.formation_complete.is_some());
    }

    #[test]
    fn test_human_rejection() {
        let mut coord = CellCoordinator::new("cell1".to_string());
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
        let mut coord = CellCoordinator::new("cell1".to_string());
        coord.status = FormationStatus::Ready;

        assert!(coord.can_transition_to_hierarchical());

        let phase = coord.get_hierarchical_phase().unwrap();
        assert_eq!(phase, Phase::Hierarchical);
    }

    #[test]
    fn test_cannot_transition_when_not_ready() {
        let coord = CellCoordinator::new("cell1".to_string());

        assert!(!coord.can_transition_to_hierarchical());
        assert!(coord.get_hierarchical_phase().is_err());
    }

    #[test]
    fn test_formation_duration() {
        let coord = CellCoordinator::new("cell1".to_string());

        std::thread::sleep(std::time::Duration::from_secs(1));

        let duration = coord.formation_duration();
        assert!(duration >= 1);
    }

    #[test]
    fn test_reset_formation() {
        let mut coord = CellCoordinator::new("cell1".to_string());
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
        let mut coord = CellCoordinator::new("cell1".to_string());

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
            Some(CellRole::Leader),
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
        let mut coord = CellCoordinator::new("cell1".to_string());
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
                Some(CellRole::Leader),
                Some(operator),
            ),
            create_test_member(
                "p2",
                vec![CapabilityType::Sensor],
                Some(CellRole::Sensor),
                None,
            ),
            create_test_member(
                "p3",
                vec![CapabilityType::Compute],
                Some(CellRole::Compute),
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
        let mut coord = CellCoordinator::new("cell1".to_string());

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
        let mut coord = CellCoordinator::new("cell1".to_string());
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
