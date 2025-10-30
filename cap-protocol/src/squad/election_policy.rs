//! Leadership election policy configuration
//!
//! This module defines tunable policies for hybrid human-machine leadership election.
//! Policies can be loaded from configuration files, environment variables, or C2 directives.

use crate::models::{Operator, OperatorRank};
use crate::traits::Phase;
use serde::{Deserialize, Serialize};
use std::env;

/// Leadership election policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionPolicyConfig {
    /// Default policy to use
    pub default_policy: LeadershipPolicy,
    /// Minimum rank required for squad leader (None = no minimum)
    pub min_leader_rank: Option<OperatorRank>,
    /// Whether autonomous platforms can be leaders
    pub allow_autonomous_leaders: bool,
    /// Cognitive load threshold for leadership disqualification (0.0-1.0)
    pub max_cognitive_load: f32,
    /// Fatigue threshold for leadership disqualification (0.0-1.0)
    pub max_fatigue: f32,
}

impl Default for ElectionPolicyConfig {
    fn default() -> Self {
        Self {
            default_policy: LeadershipPolicy::Hybrid {
                authority_weight: 0.6,
                technical_weight: 0.4,
            },
            min_leader_rank: Some(OperatorRank::E5), // Team leader minimum
            allow_autonomous_leaders: false,          // Require human leadership by default
            max_cognitive_load: 0.85,
            max_fatigue: 0.75,
        }
    }
}

impl ElectionPolicyConfig {
    /// Load configuration from environment variables
    ///
    /// Supported environment variables:
    /// - CAP_ELECTION_POLICY: "rank_dominant", "technical_dominant", or "hybrid"
    /// - CAP_AUTHORITY_WEIGHT: float 0.0-1.0 (for hybrid policy)
    /// - CAP_ALLOW_AUTONOMOUS_LEADERS: "true" or "false"
    /// - CAP_MIN_LEADER_RANK: "E5", "E7", "O3", etc.
    pub fn load_from_env() -> Self {
        let mut config = Self::default();

        // Load policy type
        if let Ok(policy_str) = env::var("CAP_ELECTION_POLICY") {
            config.default_policy = match policy_str.to_lowercase().as_str() {
                "rank_dominant" => LeadershipPolicy::RankDominant,
                "technical_dominant" => LeadershipPolicy::TechnicalDominant,
                "hybrid" => {
                    let authority_weight = env::var("CAP_AUTHORITY_WEIGHT")
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.6);
                    LeadershipPolicy::Hybrid {
                        authority_weight,
                        technical_weight: 1.0 - authority_weight,
                    }
                }
                "contextual" => LeadershipPolicy::Contextual,
                _ => config.default_policy, // Keep default on invalid value
            };
        }

        // Load autonomous leader flag
        if let Ok(allow_str) = env::var("CAP_ALLOW_AUTONOMOUS_LEADERS") {
            config.allow_autonomous_leaders = allow_str.to_lowercase() == "true";
        }

        // Load minimum rank
        if let Ok(rank_str) = env::var("CAP_MIN_LEADER_RANK") {
            config.min_leader_rank = parse_rank_string(&rank_str);
        }

        // Load cognitive load threshold
        if let Ok(load_str) = env::var("CAP_MAX_COGNITIVE_LOAD") {
            if let Ok(load) = load_str.parse::<f32>() {
                config.max_cognitive_load = load.clamp(0.0, 1.0);
            }
        }

        // Load fatigue threshold
        if let Ok(fatigue_str) = env::var("CAP_MAX_FATIGUE") {
            if let Ok(fatigue) = fatigue_str.parse::<f32>() {
                config.max_fatigue = fatigue.clamp(0.0, 1.0);
            }
        }

        config
    }

    /// Check if an operator is qualified to be squad leader
    pub fn is_qualified_leader(&self, operator: &Operator) -> bool {
        // Check cognitive load
        if operator.cognitive_load > self.max_cognitive_load {
            return false;
        }

        // Check fatigue
        if operator.fatigue > self.max_fatigue {
            return false;
        }

        // Check minimum rank
        if let Some(min_rank) = &self.min_leader_rank {
            if operator.rank < *min_rank {
                return false;
            }
        }

        true
    }

    /// Check if autonomous platforms are allowed to be leaders
    pub fn allows_autonomous_leader(&self) -> bool {
        self.allow_autonomous_leaders
    }
}

/// Leadership policy determines how authority and technical capability are weighted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LeadershipPolicy {
    /// Rank always wins - highest-ranking human becomes leader
    RankDominant,
    /// Technical capability always wins - best sensors/comms becomes leader
    TechnicalDominant,
    /// Weighted hybrid - configurable balance between authority and technical
    Hybrid {
        authority_weight: f64,
        technical_weight: f64,
    },
    /// Dynamic based on context (mission phase, casualties, etc.)
    Contextual,
}

impl LeadershipPolicy {
    /// Get the weights for authority and technical scores
    ///
    /// Returns (authority_weight, technical_weight)
    pub fn get_weights(&self, context: &ElectionContext) -> (f64, f64) {
        match self {
            Self::RankDominant => (1.0, 0.0),
            Self::TechnicalDominant => (0.0, 1.0),
            Self::Hybrid {
                authority_weight,
                technical_weight,
            } => (*authority_weight, *technical_weight),
            Self::Contextual => Self::compute_contextual_weights(context),
        }
    }

    /// Compute weights based on context
    fn compute_contextual_weights(context: &ElectionContext) -> (f64, f64) {
        // Adjust weights based on mission phase (authority_required flag can be checked by caller if needed)
        match context.mission_phase {
            Phase::Bootstrap => (0.7, 0.3),      // Planning phase - authority matters more
            Phase::Squad => (0.6, 0.4),          // Squad ops - balanced
            Phase::Hierarchical => (0.8, 0.2),   // Hierarchical - authority critical
        }
    }
}

/// Context for leadership election
#[derive(Debug, Clone)]
pub struct ElectionContext {
    /// Active policy
    pub policy: LeadershipPolicy,
    /// Current mission phase
    pub mission_phase: Phase,
    /// Whether human authority is required for this election
    pub authority_required: bool,
    /// Number of casualties (affects contextual policy)
    pub casualty_count: usize,
}

impl Default for ElectionContext {
    fn default() -> Self {
        Self {
            policy: LeadershipPolicy::Hybrid {
                authority_weight: 0.6,
                technical_weight: 0.4,
            },
            mission_phase: Phase::Squad,
            authority_required: false,
            casualty_count: 0,
        }
    }
}

impl ElectionContext {
    /// Create a new election context with a specific policy
    pub fn new(policy: LeadershipPolicy, mission_phase: Phase) -> Self {
        Self {
            policy,
            mission_phase,
            authority_required: false,
            casualty_count: 0,
        }
    }

    /// Set whether authority is required
    pub fn with_authority_required(mut self, required: bool) -> Self {
        self.authority_required = required;
        self
    }

    /// Set casualty count (affects contextual policy)
    pub fn with_casualties(mut self, count: usize) -> Self {
        self.casualty_count = count;
        self
    }
}

/// Parse a rank string like "E5", "E7", "O3" into OperatorRank
fn parse_rank_string(s: &str) -> Option<OperatorRank> {
    let s = s.trim().to_uppercase();

    if let Some(stripped) = s.strip_prefix('E') {
        match stripped {
            "1" => Some(OperatorRank::E1),
            "2" => Some(OperatorRank::E2),
            "3" => Some(OperatorRank::E3),
            "4" => Some(OperatorRank::E4),
            "5" => Some(OperatorRank::E5),
            "6" => Some(OperatorRank::E6),
            "7" => Some(OperatorRank::E7),
            "8" => Some(OperatorRank::E8),
            "9" => Some(OperatorRank::E9),
            _ => None,
        }
    } else if let Some(stripped) = s.strip_prefix('W') {
        match stripped {
            "1" => Some(OperatorRank::W1),
            "2" => Some(OperatorRank::W2),
            "3" => Some(OperatorRank::W3),
            "4" => Some(OperatorRank::W4),
            "5" => Some(OperatorRank::W5),
            _ => None,
        }
    } else if let Some(stripped) = s.strip_prefix('O') {
        match stripped {
            "1" => Some(OperatorRank::O1),
            "2" => Some(OperatorRank::O2),
            "3" => Some(OperatorRank::O3),
            "4" => Some(OperatorRank::O4),
            "5" => Some(OperatorRank::O5),
            "6" => Some(OperatorRank::O6),
            "7" => Some(OperatorRank::O7),
            "8" => Some(OperatorRank::O8),
            "9" => Some(OperatorRank::O9),
            "10" => Some(OperatorRank::O10),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AuthorityLevel;

    #[test]
    fn test_default_config() {
        let config = ElectionPolicyConfig::default();

        assert!(matches!(
            config.default_policy,
            LeadershipPolicy::Hybrid { .. }
        ));
        assert_eq!(config.min_leader_rank, Some(OperatorRank::E5));
        assert!(!config.allow_autonomous_leaders);
        assert_eq!(config.max_cognitive_load, 0.85);
        assert_eq!(config.max_fatigue, 0.75);
    }

    #[test]
    fn test_load_from_env() {
        // Set environment variables
        env::set_var("CAP_ELECTION_POLICY", "rank_dominant");
        env::set_var("CAP_ALLOW_AUTONOMOUS_LEADERS", "true");
        env::set_var("CAP_MIN_LEADER_RANK", "E7");

        let config = ElectionPolicyConfig::load_from_env();

        assert_eq!(config.default_policy, LeadershipPolicy::RankDominant);
        assert!(config.allow_autonomous_leaders);
        assert_eq!(config.min_leader_rank, Some(OperatorRank::E7));

        // Clean up
        env::remove_var("CAP_ELECTION_POLICY");
        env::remove_var("CAP_ALLOW_AUTONOMOUS_LEADERS");
        env::remove_var("CAP_MIN_LEADER_RANK");
    }

    #[test]
    fn test_is_qualified_leader() {
        let config = ElectionPolicyConfig::default();

        // Qualified E-7 with low cognitive load and fatigue
        let qualified = Operator::new(
            "op_1".to_string(),
            "SFC Smith".to_string(),
            OperatorRank::E7,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );
        assert!(config.is_qualified_leader(&qualified));

        // Disqualified - rank too low (E-4 < E-5 minimum)
        let low_rank = Operator::new(
            "op_2".to_string(),
            "SPC Jones".to_string(),
            OperatorRank::E4,
            AuthorityLevel::Commander,
            "11B".to_string(),
        );
        assert!(!config.is_qualified_leader(&low_rank));

        // Disqualified - high cognitive load
        let mut overloaded = qualified.clone();
        overloaded.cognitive_load = 0.95;
        assert!(!config.is_qualified_leader(&overloaded));

        // Disqualified - high fatigue
        let mut fatigued = qualified.clone();
        fatigued.fatigue = 0.90;
        assert!(!config.is_qualified_leader(&fatigued));
    }

    #[test]
    fn test_leadership_policy_weights() {
        let context = ElectionContext::default();

        // Rank dominant
        let policy = LeadershipPolicy::RankDominant;
        assert_eq!(policy.get_weights(&context), (1.0, 0.0));

        // Technical dominant
        let policy = LeadershipPolicy::TechnicalDominant;
        assert_eq!(policy.get_weights(&context), (0.0, 1.0));

        // Hybrid
        let policy = LeadershipPolicy::Hybrid {
            authority_weight: 0.7,
            technical_weight: 0.3,
        };
        assert_eq!(policy.get_weights(&context), (0.7, 0.3));
    }

    #[test]
    fn test_contextual_policy_weights() {
        let policy = LeadershipPolicy::Contextual;

        // Bootstrap phase - authority matters more
        let context = ElectionContext::new(policy.clone(), Phase::Bootstrap);
        let (auth, _tech) = policy.get_weights(&context);
        assert_eq!(auth, 0.7);

        // Squad phase - balanced
        let context = ElectionContext::new(policy.clone(), Phase::Squad);
        let (auth, _tech) = policy.get_weights(&context);
        assert_eq!(auth, 0.6);

        // Hierarchical phase - authority critical
        let context = ElectionContext::new(policy.clone(), Phase::Hierarchical);
        let (auth, _tech) = policy.get_weights(&context);
        assert_eq!(auth, 0.8);
    }

    #[test]
    fn test_contextual_policy_with_authority_required() {
        let policy = LeadershipPolicy::Contextual;
        let context = ElectionContext::new(policy.clone(), Phase::Squad).with_authority_required(true);

        let (auth, _tech) = policy.get_weights(&context);
        // Should keep authority weight at phase level when required
        assert_eq!(auth, 0.6); // Same as base squad weight (already high enough)
    }

    #[test]
    fn test_parse_rank_string() {
        assert_eq!(parse_rank_string("E5"), Some(OperatorRank::E5));
        assert_eq!(parse_rank_string("e7"), Some(OperatorRank::E7));
        assert_eq!(parse_rank_string("W3"), Some(OperatorRank::W3));
        assert_eq!(parse_rank_string("O3"), Some(OperatorRank::O3));
        assert_eq!(parse_rank_string("O10"), Some(OperatorRank::O10));
        assert_eq!(parse_rank_string("invalid"), None);
        assert_eq!(parse_rank_string("E99"), None);
    }

    #[test]
    fn test_election_context_builder() {
        let context = ElectionContext::new(
            LeadershipPolicy::Hybrid {
                authority_weight: 0.6,
                technical_weight: 0.4,
            },
            Phase::Hierarchical,
        )
        .with_authority_required(true)
        .with_casualties(2);

        assert_eq!(context.mission_phase, Phase::Hierarchical);
        assert!(context.authority_required);
        assert_eq!(context.casualty_count, 2);
    }
}
