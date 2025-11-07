//! Human operator and human-machine binding models
//!
//! This module defines the relationship between human operators and platforms,
//! supporting multiple teaming patterns and authority models.

// Re-export protobuf types
pub use cap_schema::node::v1::{
    AuthorityLevel, BindingType, HumanMachinePair, Operator, OperatorRank,
};

// Extension trait for Operator helper methods
pub trait OperatorExt {
    /// Create a new operator with default cognitive state
    fn new(
        id: String,
        name: String,
        rank: OperatorRank,
        authority: AuthorityLevel,
        mos: String,
    ) -> Self;

    /// Update cognitive load from metadata (clamped to 0.0-1.0)
    fn update_cognitive_load(&mut self, load: f32);

    /// Update fatigue from metadata (clamped to 0.0-1.0)
    fn update_fatigue(&mut self, fatigue: f32);

    /// Get cognitive load from metadata
    fn cognitive_load(&self) -> f32;

    /// Get fatigue from metadata
    fn fatigue(&self) -> f32;

    /// Check if operator is overloaded (cognitive load > threshold)
    fn is_overloaded(&self, threshold: f32) -> bool;

    /// Check if operator is fatigued (fatigue > threshold)
    fn is_fatigued(&self, threshold: f32) -> bool;

    /// Get operator effectiveness score (0.0-1.0)
    /// Considers cognitive load and fatigue
    fn effectiveness(&self) -> f32;
}

impl OperatorExt for Operator {
    fn new(
        id: String,
        name: String,
        rank: OperatorRank,
        authority: AuthorityLevel,
        mos: String,
    ) -> Self {
        use serde_json::json;
        let metadata = json!({
            "cognitive_load": 0.0,
            "fatigue": 0.0,
        });

        Self {
            id,
            name,
            rank: rank as i32,
            authority_level: authority as i32,
            mos,
            metadata_json: metadata.to_string(),
        }
    }

    fn update_cognitive_load(&mut self, load: f32) {
        let load = load.clamp(0.0, 1.0);
        let mut metadata: serde_json::Value =
            serde_json::from_str(&self.metadata_json).unwrap_or(serde_json::json!({}));
        metadata["cognitive_load"] = serde_json::json!(load);
        self.metadata_json = metadata.to_string();
    }

    fn update_fatigue(&mut self, fatigue: f32) {
        let fatigue = fatigue.clamp(0.0, 1.0);
        let mut metadata: serde_json::Value =
            serde_json::from_str(&self.metadata_json).unwrap_or(serde_json::json!({}));
        metadata["fatigue"] = serde_json::json!(fatigue);
        self.metadata_json = metadata.to_string();
    }

    fn cognitive_load(&self) -> f32 {
        let metadata: serde_json::Value =
            serde_json::from_str(&self.metadata_json).unwrap_or(serde_json::json!({}));
        metadata["cognitive_load"].as_f64().unwrap_or(0.0) as f32
    }

    fn fatigue(&self) -> f32 {
        let metadata: serde_json::Value =
            serde_json::from_str(&self.metadata_json).unwrap_or(serde_json::json!({}));
        metadata["fatigue"].as_f64().unwrap_or(0.0) as f32
    }

    fn is_overloaded(&self, threshold: f32) -> bool {
        self.cognitive_load() > threshold
    }

    fn is_fatigued(&self, threshold: f32) -> bool {
        self.fatigue() > threshold
    }

    fn effectiveness(&self) -> f32 {
        let cognitive_factor = 1.0 - self.cognitive_load();
        let fatigue_factor = 1.0 - self.fatigue();
        (cognitive_factor * 0.6 + fatigue_factor * 0.4).clamp(0.0, 1.0)
    }
}

// Extension trait for OperatorRank helper methods
pub trait OperatorRankExt {
    /// Convert rank to numeric score (0.0-1.0) for leadership scoring
    fn to_score(self) -> f64;

    /// Get human-readable rank name
    fn name(self) -> &'static str;
}

impl OperatorRankExt for OperatorRank {
    fn to_score(self) -> f64 {
        match self {
            Self::Unspecified => 0.0,
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
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unspecified => "Unspecified",
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
        }
    }
}

// Extension trait for AuthorityLevel helper methods
pub trait AuthorityLevelExt {
    /// Convert authority to numeric score (0.0-1.0) for leadership scoring
    fn to_score(self) -> f64;

    /// Check if this authority level can override machine decisions
    fn can_override(self) -> bool;

    /// Check if this authority level requires human approval for actions
    fn requires_approval(self) -> bool;
}

impl AuthorityLevelExt for AuthorityLevel {
    fn to_score(self) -> f64 {
        match self {
            Self::Unspecified => 0.0,
            Self::Observer => 0.1,
            Self::Advisor => 0.3,
            Self::Supervisor => 0.5,
            Self::Commander => 0.8,
        }
    }

    fn can_override(self) -> bool {
        matches!(self, Self::Supervisor | Self::Commander)
    }

    fn requires_approval(self) -> bool {
        matches!(self, Self::Commander)
    }
}

// Extension trait for HumanMachinePair helper methods
pub trait HumanMachinePairExt {
    /// Create a new human-machine pair
    fn new(operators: Vec<Operator>, platform_ids: Vec<String>, binding_type: BindingType) -> Self;

    /// Create an autonomous (no human) binding
    fn autonomous(platform_id: String) -> Self;

    /// Create a one-to-one human-platform pair
    fn one_to_one(operator: Operator, platform_id: String) -> Self;

    /// Check if this is an autonomous platform (no operators)
    fn is_autonomous(&self) -> bool;

    /// Get the primary operator (highest rank)
    fn primary_operator(&self) -> Option<&Operator>;

    /// Get highest rank among operators
    fn max_rank(&self) -> Option<OperatorRank>;

    /// Get highest authority level among operators
    fn max_authority(&self) -> Option<AuthorityLevel>;

    /// Check if any operator is overloaded
    fn has_overloaded_operator(&self, threshold: f32) -> bool;

    /// Get average operator effectiveness across all operators
    fn avg_effectiveness(&self) -> f32;
}

impl HumanMachinePairExt for HumanMachinePair {
    fn new(operators: Vec<Operator>, platform_ids: Vec<String>, binding_type: BindingType) -> Self {
        Self {
            operators,
            platform_ids,
            binding_type: binding_type as i32,
            bound_at: None,
        }
    }

    fn autonomous(platform_id: String) -> Self {
        Self {
            operators: Vec::new(),
            platform_ids: vec![platform_id],
            binding_type: BindingType::Unspecified as i32,
            bound_at: None,
        }
    }

    fn one_to_one(operator: Operator, platform_id: String) -> Self {
        Self::new(vec![operator], vec![platform_id], BindingType::OneToOne)
    }

    fn is_autonomous(&self) -> bool {
        self.operators.is_empty()
    }

    fn primary_operator(&self) -> Option<&Operator> {
        // Return highest-ranking operator
        self.operators.iter().max_by(|a, b| a.rank.cmp(&b.rank))
    }

    fn max_rank(&self) -> Option<OperatorRank> {
        self.operators
            .iter()
            .map(|op| OperatorRank::try_from(op.rank).ok())
            .max()
            .flatten()
    }

    fn max_authority(&self) -> Option<AuthorityLevel> {
        self.operators
            .iter()
            .map(|op| AuthorityLevel::try_from(op.authority_level).ok())
            .max()
            .flatten()
    }

    fn has_overloaded_operator(&self, threshold: f32) -> bool {
        self.operators.iter().any(|op| op.is_overloaded(threshold))
    }

    fn avg_effectiveness(&self) -> f32 {
        if self.operators.is_empty() {
            return 1.0; // Autonomous nodes are always "effective"
        }

        let sum: f32 = self.operators.iter().map(|op| op.effectiveness()).sum();
        sum / self.operators.len() as f32
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
        assert_eq!(op.rank, OperatorRank::E7 as i32);
        assert_eq!(op.cognitive_load(), 0.0);
        assert_eq!(op.fatigue(), 0.0);
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
        assert_eq!(op.cognitive_load(), 0.8);
        assert!(op.is_overloaded(0.7));
        assert!(!op.is_overloaded(0.9));

        // Test clamping
        op.update_cognitive_load(1.5);
        assert_eq!(op.cognitive_load(), 1.0);
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
        assert!(AuthorityLevel::Commander > AuthorityLevel::Observer);
    }

    #[test]
    fn test_authority_can_override() {
        assert!(!AuthorityLevel::Observer.can_override());
        assert!(!AuthorityLevel::Advisor.can_override());
        assert!(AuthorityLevel::Supervisor.can_override());
        assert!(AuthorityLevel::Commander.can_override());
    }

    #[test]
    fn test_human_machine_pair_autonomous() {
        let pair = HumanMachinePair::autonomous("node_1".to_string());
        assert!(pair.is_autonomous());
        assert_eq!(pair.operators.len(), 0);
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

        let pair = HumanMachinePair::one_to_one(op, "node_1".to_string());

        assert!(!pair.is_autonomous());
        assert_eq!(pair.operators.len(), 1);
        assert_eq!(pair.binding_type, BindingType::OneToOne as i32);
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
            vec!["node_1".to_string()],
            BindingType::ManyToOne,
        );

        // Should return highest-ranking operator
        let primary = pair.primary_operator().unwrap();
        assert_eq!(primary.rank, OperatorRank::E7 as i32);
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
            vec!["node_1".to_string()],
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
            vec!["node_1".to_string()],
            BindingType::ManyToOne,
        );

        assert!(pair.has_overloaded_operator(0.8));
        assert!(!pair.has_overloaded_operator(0.95));
    }
}
