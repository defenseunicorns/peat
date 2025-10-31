//! Human operator and human-machine binding models
//!
//! This module defines the relationship between human operators and platforms,
//! supporting multiple teaming patterns and authority models.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Human operator of a platform or squad
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Operator {
    /// Unique operator identifier
    pub id: String,
    /// Operator name
    pub name: String,
    /// Military rank
    pub rank: OperatorRank,
    /// Authority level in current context
    pub authority: AuthorityLevel,
    /// Military Occupational Specialty (MOS) or role
    pub mos: String,
    /// Cognitive load score (0.0-1.0, higher = more loaded)
    /// Updated by platform based on task complexity, workload
    pub cognitive_load: f32,
    /// Fatigue level (0.0-1.0, higher = more fatigued)
    /// Updated by physiological sensors or self-report
    pub fatigue: f32,
}

impl Operator {
    /// Create a new operator with default cognitive state
    pub fn new(
        id: String,
        name: String,
        rank: OperatorRank,
        authority: AuthorityLevel,
        mos: String,
    ) -> Self {
        Self {
            id,
            name,
            rank,
            authority,
            mos,
            cognitive_load: 0.0,
            fatigue: 0.0,
        }
    }

    /// Update cognitive load (clamped to 0.0-1.0)
    pub fn update_cognitive_load(&mut self, load: f32) {
        self.cognitive_load = load.clamp(0.0, 1.0);
    }

    /// Update fatigue (clamped to 0.0-1.0)
    pub fn update_fatigue(&mut self, fatigue: f32) {
        self.fatigue = fatigue.clamp(0.0, 1.0);
    }

    /// Check if operator is overloaded (cognitive load > threshold)
    pub fn is_overloaded(&self, threshold: f32) -> bool {
        self.cognitive_load > threshold
    }

    /// Check if operator is fatigued (fatigue > threshold)
    pub fn is_fatigued(&self, threshold: f32) -> bool {
        self.fatigue > threshold
    }

    /// Get operator effectiveness score (0.0-1.0)
    /// Considers cognitive load and fatigue
    pub fn effectiveness(&self) -> f32 {
        let cognitive_factor = 1.0 - self.cognitive_load;
        let fatigue_factor = 1.0 - self.fatigue;
        (cognitive_factor * 0.6 + fatigue_factor * 0.4).clamp(0.0, 1.0)
    }
}

/// Military rank hierarchy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperatorRank {
    // Enlisted ranks (E-1 through E-9)
    E1,
    E2,
    E3,
    E4,
    E5,
    E6,
    E7, // Typical squad leader rank
    E8,
    E9,

    // Warrant Officers (W-1 through W-5)
    W1,
    W2,
    W3,
    W4,
    W5,

    // Commissioned Officers (O-1 through O-10)
    O1,
    O2,
    O3, // Platoon leader typical rank
    O4,
    O5,
    O6,
    O7,
    O8,
    O9,
    O10,

    // Civilian equivalent (for coalition/allied forces)
    /// Civilian with equivalent authority level (0-10)
    Civilian(u8),
}

impl OperatorRank {
    /// Convert rank to numeric score (0.0-1.0) for leadership scoring
    pub fn to_score(self) -> f64 {
        match self {
            Self::E1 => 0.10,
            Self::E2 => 0.15,
            Self::E3 => 0.20,
            Self::E4 => 0.30,
            Self::E5 => 0.40,
            Self::E6 => 0.50,
            Self::E7 => 0.60, // Cell leader typical
            Self::E8 => 0.70,
            Self::E9 => 0.80,
            Self::W1 => 0.70,
            Self::W2 => 0.75,
            Self::W3 => 0.80,
            Self::W4 => 0.85,
            Self::W5 => 0.90,
            Self::O1 => 0.85,
            Self::O2 => 0.90,
            Self::O3 => 0.95, // Platoon leader
            Self::O4 => 0.97,
            Self::O5 => 0.98,
            Self::O6 => 0.99,
            Self::O7 => 0.995,
            Self::O8 => 0.997,
            Self::O9 => 0.999,
            Self::O10 => 1.0,
            Self::Civilian(level) => (level as f64) / 10.0,
        }
    }

    /// Get human-readable rank name
    pub fn name(&self) -> &'static str {
        match self {
            Self::E1 => "Private (E-1)",
            Self::E2 => "Private (E-2)",
            Self::E3 => "Private First Class",
            Self::E4 => "Corporal/Specialist",
            Self::E5 => "Sergeant",
            Self::E6 => "Staff Sergeant",
            Self::E7 => "Sergeant First Class",
            Self::E8 => "Master Sergeant",
            Self::E9 => "Sergeant Major",
            Self::W1 => "Warrant Officer 1",
            Self::W2 => "Chief Warrant Officer 2",
            Self::W3 => "Chief Warrant Officer 3",
            Self::W4 => "Chief Warrant Officer 4",
            Self::W5 => "Chief Warrant Officer 5",
            Self::O1 => "Second Lieutenant",
            Self::O2 => "First Lieutenant",
            Self::O3 => "Captain",
            Self::O4 => "Major",
            Self::O5 => "Lieutenant Colonel",
            Self::O6 => "Colonel",
            Self::O7 => "Brigadier General",
            Self::O8 => "Major General",
            Self::O9 => "Lieutenant General",
            Self::O10 => "General",
            Self::Civilian(_) => "Civilian",
        }
    }
}

impl fmt::Display for OperatorRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Authority level in human-machine teaming
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuthorityLevel {
    /// Human observes but cannot override (display only)
    Observer,
    /// Human can provide recommendations that machine may consider
    Advisor,
    /// Human provides high-level intent, machine executes
    Supervisor,
    /// Human approves machine recommendations before execution
    Commander,
    /// Human has full control, machine is tool/assistant
    DirectControl,
}

impl AuthorityLevel {
    /// Convert authority to numeric score (0.0-1.0) for leadership scoring
    pub fn to_score(self) -> f64 {
        match self {
            Self::Observer => 0.1,
            Self::Advisor => 0.3,
            Self::Supervisor => 0.5,
            Self::Commander => 0.8,
            Self::DirectControl => 1.0,
        }
    }

    /// Check if this authority level can override machine decisions
    pub fn can_override(&self) -> bool {
        matches!(
            self,
            Self::Supervisor | Self::Commander | Self::DirectControl
        )
    }

    /// Check if this authority level requires human approval for actions
    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::Commander | Self::DirectControl)
    }
}

impl fmt::Display for AuthorityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Observer => write!(f, "Observer"),
            Self::Advisor => write!(f, "Advisor"),
            Self::Supervisor => write!(f, "Supervisor"),
            Self::Commander => write!(f, "Commander"),
            Self::DirectControl => write!(f, "Direct Control"),
        }
    }
}

/// Human-machine binding representing the relationship between operators and platforms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HumanMachinePair {
    /// Operator(s) - can be empty for autonomous platforms
    pub operators: Vec<Operator>,
    /// Node ID(s) bound to this pairing
    pub platform_ids: Vec<String>,
    /// Type of binding relationship
    pub binding_type: BindingType,
    /// Primary operator ID (for multi-operator scenarios)
    pub primary_operator_id: Option<String>,
}

impl HumanMachinePair {
    /// Create a new human-machine pair
    pub fn new(
        operators: Vec<Operator>,
        platform_ids: Vec<String>,
        binding_type: BindingType,
    ) -> Self {
        // Default primary operator to highest-ranking operator
        let primary_operator_id = operators
            .iter()
            .max_by(|a, b| a.rank.cmp(&b.rank))
            .map(|op| op.id.clone());

        Self {
            operators,
            platform_ids,
            binding_type,
            primary_operator_id,
        }
    }

    /// Create an autonomous (no human) binding
    pub fn autonomous(platform_id: String) -> Self {
        Self {
            operators: Vec::new(),
            platform_ids: vec![platform_id],
            binding_type: BindingType::Autonomous,
            primary_operator_id: None,
        }
    }

    /// Create a one-to-one human-platform pair
    pub fn one_to_one(operator: Operator, platform_id: String) -> Self {
        Self::new(vec![operator], vec![platform_id], BindingType::OneToOne)
    }

    /// Check if this is an autonomous platform (no operators)
    pub fn is_autonomous(&self) -> bool {
        self.operators.is_empty()
    }

    /// Get the primary operator (highest rank or explicitly set)
    pub fn primary_operator(&self) -> Option<&Operator> {
        if let Some(ref primary_id) = self.primary_operator_id {
            self.operators.iter().find(|op| &op.id == primary_id)
        } else {
            // Return highest-ranking operator
            self.operators.iter().max_by(|a, b| a.rank.cmp(&b.rank))
        }
    }

    /// Get highest rank among operators
    pub fn max_rank(&self) -> Option<OperatorRank> {
        self.operators.iter().map(|op| op.rank).max()
    }

    /// Get highest authority level among operators
    pub fn max_authority(&self) -> Option<AuthorityLevel> {
        self.operators.iter().map(|op| op.authority).max()
    }

    /// Check if any operator is overloaded
    pub fn has_overloaded_operator(&self, threshold: f32) -> bool {
        self.operators.iter().any(|op| op.is_overloaded(threshold))
    }

    /// Get average operator effectiveness across all operators
    pub fn avg_effectiveness(&self) -> f32 {
        if self.operators.is_empty() {
            return 1.0; // Autonomous nodes are always "effective"
        }

        let sum: f32 = self.operators.iter().map(|op| op.effectiveness()).sum();
        sum / self.operators.len() as f32
    }
}

/// Type of human-machine binding relationship
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BindingType {
    /// One human operating one platform (traditional)
    OneToOne,
    /// One human controlling multiple nodes (swarm operator)
    OneToMany,
    /// Multiple humans sharing one platform (command vehicle)
    ManyToOne,
    /// Complex teaming relationships (platoon/company level)
    ManyToMany,
    /// No human operator (autonomous platform)
    Autonomous,
}

impl fmt::Display for BindingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneToOne => write!(f, "1:1 (One Human : One Platform)"),
            Self::OneToMany => write!(f, "1:N (One Human : Multiple Platforms)"),
            Self::ManyToOne => write!(f, "N:1 (Multiple Humans : One Platform)"),
            Self::ManyToMany => write!(f, "N:M (Complex Teaming)"),
            Self::Autonomous => write!(f, "Autonomous (No Human)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_creation() {
        let op = Operator::new(
            "op1".to_string(),
            "John Doe".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        assert_eq!(op.id, "op1");
        assert_eq!(op.rank, OperatorRank::E7);
        assert_eq!(op.cognitive_load, 0.0);
        assert_eq!(op.fatigue, 0.0);
    }

    #[test]
    fn test_operator_cognitive_load() {
        let mut op = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        op.update_cognitive_load(0.8);
        assert_eq!(op.cognitive_load, 0.8);
        assert!(op.is_overloaded(0.7));
        assert!(!op.is_overloaded(0.9));

        // Test clamping
        op.update_cognitive_load(1.5);
        assert_eq!(op.cognitive_load, 1.0);
    }

    #[test]
    fn test_operator_effectiveness() {
        let mut op = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        // Fresh operator
        assert_eq!(op.effectiveness(), 1.0);

        // High cognitive load, low fatigue
        op.update_cognitive_load(0.8);
        op.update_fatigue(0.2);
        let eff = op.effectiveness();
        assert!(eff > 0.0 && eff < 1.0);

        // High both
        op.update_cognitive_load(0.9);
        op.update_fatigue(0.9);
        assert!(op.effectiveness() < 0.2);
    }

    #[test]
    fn test_rank_ordering() {
        assert!(OperatorRank::E7 > OperatorRank::E5);
        assert!(OperatorRank::O3 > OperatorRank::E9);
        assert!(OperatorRank::W5 > OperatorRank::W1);
    }

    #[test]
    fn test_rank_to_score() {
        assert_eq!(OperatorRank::E1.to_score(), 0.10);
        assert_eq!(OperatorRank::E7.to_score(), 0.60);
        assert_eq!(OperatorRank::O3.to_score(), 0.95);
        assert_eq!(OperatorRank::O10.to_score(), 1.0);
    }

    #[test]
    fn test_authority_level_ordering() {
        assert!(AuthorityLevel::Commander > AuthorityLevel::Supervisor);
        assert!(AuthorityLevel::DirectControl > AuthorityLevel::Observer);
    }

    #[test]
    fn test_authority_can_override() {
        assert!(!AuthorityLevel::Observer.can_override());
        assert!(!AuthorityLevel::Advisor.can_override());
        assert!(AuthorityLevel::Supervisor.can_override());
        assert!(AuthorityLevel::Commander.can_override());
        assert!(AuthorityLevel::DirectControl.can_override());
    }

    #[test]
    fn test_human_machine_pair_autonomous() {
        let pair = HumanMachinePair::autonomous("platform_1".to_string());
        assert!(pair.is_autonomous());
        assert_eq!(pair.operators.len(), 0);
        assert_eq!(pair.binding_type, BindingType::Autonomous);
        assert_eq!(pair.avg_effectiveness(), 1.0);
    }

    #[test]
    fn test_human_machine_pair_one_to_one() {
        let op = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let pair = HumanMachinePair::one_to_one(op, "platform_1".to_string());

        assert!(!pair.is_autonomous());
        assert_eq!(pair.operators.len(), 1);
        assert_eq!(pair.binding_type, BindingType::OneToOne);
        assert_eq!(pair.max_rank(), Some(OperatorRank::E5));
    }

    #[test]
    fn test_human_machine_pair_primary_operator() {
        let op1 = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );
        let op2 = Operator::new(
            "op2".to_string(),
            "Jane".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let pair = HumanMachinePair::new(
            vec![op1, op2],
            vec!["platform_1".to_string()],
            BindingType::ManyToOne,
        );

        // Should return highest-ranking operator
        let primary = pair.primary_operator().unwrap();
        assert_eq!(primary.rank, OperatorRank::E7);
        assert_eq!(primary.name, "Jane");
    }

    #[test]
    fn test_human_machine_pair_max_authority() {
        let op1 = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );
        let op2 = Operator::new(
            "op2".to_string(),
            "Jane".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let pair = HumanMachinePair::new(
            vec![op1, op2],
            vec!["platform_1".to_string()],
            BindingType::ManyToOne,
        );

        assert_eq!(pair.max_authority(), Some(AuthorityLevel::Commander));
    }

    #[test]
    fn test_human_machine_pair_overloaded_check() {
        let mut op1 = Operator::new(
            "op1".to_string(),
            "John".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );
        op1.update_cognitive_load(0.9);

        let op2 = Operator::new(
            "op2".to_string(),
            "Jane".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );

        let pair = HumanMachinePair::new(
            vec![op1, op2],
            vec!["platform_1".to_string()],
            BindingType::ManyToOne,
        );

        assert!(pair.has_overloaded_operator(0.8));
        assert!(!pair.has_overloaded_operator(0.95));
    }
}
