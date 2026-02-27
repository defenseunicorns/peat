//! Composition rule trait and implementations
//!
//! This module defines the core abstraction for capability composition rules.
//! Rules analyze sets of capabilities and detect composed/emergent capabilities.

use crate::models::capability::Capability;
use crate::Result;
use async_trait::async_trait;

use crate::models::{AuthorityLevel, NodeConfig};

/// Context for rule execution
///
/// Provides additional information that rules may need for composition decisions.
#[derive(Debug, Clone)]
pub struct CompositionContext {
    /// Node IDs contributing capabilities
    pub node_ids: Vec<String>,

    /// Cell or squad ID if composing within a cell
    pub cell_id: Option<String>,

    /// Timestamp of composition (for temporal analysis)
    pub timestamp: std::time::SystemTime,

    /// Node configurations (for operator/authority checks)
    pub node_configs: Vec<NodeConfig>,
}

impl CompositionContext {
    /// Create a new composition context
    pub fn new(node_ids: Vec<String>) -> Self {
        Self {
            node_ids,
            cell_id: None,
            timestamp: std::time::SystemTime::now(),
            node_configs: Vec::new(),
        }
    }

    /// Set the cell ID for this composition
    pub fn with_cell_id(mut self, cell_id: String) -> Self {
        self.cell_id = Some(cell_id);
        self
    }

    /// Add node configurations for operator/authority checks
    pub fn with_node_configs(mut self, configs: Vec<NodeConfig>) -> Self {
        self.node_configs = configs;
        self
    }

    /// Get the maximum authority level among all operators in the context
    pub fn max_authority(&self) -> Option<AuthorityLevel> {
        use crate::models::HumanMachinePairExt;

        self.node_configs
            .iter()
            .filter_map(|config| config.operator_binding.as_ref())
            .filter_map(|binding| binding.max_authority())
            .max()
    }

    /// Check if any node has an operator with Commander authority
    pub fn has_commander(&self) -> bool {
        self.max_authority() == Some(AuthorityLevel::Commander)
    }

    /// Get authorization bonus (0-5 scale) based on max authority
    pub fn authorization_bonus(&self) -> i32 {
        use crate::models::AuthorityLevelExt;

        match self.max_authority() {
            Some(auth) => (auth.to_score() * 5.0).round() as i32,
            None => 0,
        }
    }
}

impl Default for CompositionContext {
    fn default() -> Self {
        Self {
            node_ids: Vec::new(),
            cell_id: None,
            timestamp: std::time::SystemTime::now(),
            node_configs: Vec::new(),
        }
    }
}

/// Result of applying a composition rule
#[derive(Debug, Clone)]
pub struct CompositionResult {
    /// Composed capabilities detected by the rule
    pub composed_capabilities: Vec<Capability>,

    /// Confidence in the composition (0.0 - 1.0)
    pub confidence: f32,

    /// Input capabilities that contributed to this composition
    pub contributing_capabilities: Vec<String>, // capability IDs
}

impl CompositionResult {
    /// Create a new composition result
    pub fn new(composed_capabilities: Vec<Capability>, confidence: f32) -> Self {
        Self {
            composed_capabilities,
            confidence,
            contributing_capabilities: Vec::new(),
        }
    }

    /// Add contributing capability IDs
    pub fn with_contributors(mut self, capability_ids: Vec<String>) -> Self {
        self.contributing_capabilities = capability_ids;
        self
    }

    /// Check if any capabilities were composed
    pub fn has_compositions(&self) -> bool {
        !self.composed_capabilities.is_empty()
    }
}

/// Trait for capability composition rules
///
/// Composition rules analyze a set of capabilities and detect:
/// - Additive compositions (summed capabilities)
/// - Emergent compositions (new capabilities from combinations)
/// - Redundant compositions (reliability from redundancy)
/// - Constraint compositions (team limits from individual constraints)
#[async_trait]
pub trait CompositionRule: Send + Sync {
    /// Human-readable name for this rule
    fn name(&self) -> &str;

    /// Description of what this rule detects
    fn description(&self) -> &str;

    /// Check if this rule applies to the given set of capabilities
    ///
    /// Rules should return true if they can meaningfully compose
    /// any of the provided capabilities.
    fn applies_to(&self, capabilities: &[Capability]) -> bool;

    /// Apply the composition rule to a set of capabilities
    ///
    /// Returns composed capabilities with confidence scores and
    /// references to contributing capabilities.
    async fn compose(
        &self,
        capabilities: &[Capability],
        context: &CompositionContext,
    ) -> Result<CompositionResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::capability::{CapabilityExt, CapabilityType};

    #[test]
    fn test_composition_context_creation() {
        let node_ids = vec!["node1".to_string(), "node2".to_string()];
        let ctx = CompositionContext::new(node_ids.clone());

        assert_eq!(ctx.node_ids, node_ids);
        assert_eq!(ctx.cell_id, None);
    }

    #[test]
    fn test_composition_context_with_cell() {
        let ctx = CompositionContext::new(vec!["node1".to_string()])
            .with_cell_id("cell_alpha".to_string());

        assert_eq!(ctx.cell_id, Some("cell_alpha".to_string()));
    }

    #[test]
    fn test_composition_result_creation() {
        let capability = Capability::new(
            "test".to_string(),
            "Test Capability".to_string(),
            CapabilityType::Emergent,
            0.9,
        );

        let result = CompositionResult::new(vec![capability], 0.8);

        assert_eq!(result.composed_capabilities.len(), 1);
        assert_eq!(result.confidence, 0.8);
        assert!(result.has_compositions());
    }

    #[test]
    fn test_composition_result_with_contributors() {
        let capability = Capability::new(
            "emergent".to_string(),
            "Emergent".to_string(),
            CapabilityType::Emergent,
            0.9,
        );

        let result = CompositionResult::new(vec![capability], 0.8)
            .with_contributors(vec!["cap1".to_string(), "cap2".to_string()]);

        assert_eq!(result.contributing_capabilities.len(), 2);
        assert!(result
            .contributing_capabilities
            .contains(&"cap1".to_string()));
    }

    #[test]
    fn test_empty_composition_result() {
        let result = CompositionResult::new(vec![], 0.0);

        assert!(!result.has_compositions());
        assert_eq!(result.composed_capabilities.len(), 0);
    }

    #[test]
    fn test_composition_context_with_node_configs() {
        use crate::models::{NodeConfig, NodeConfigExt};

        let config = NodeConfig::new("UAV".to_string());
        let ctx =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        assert_eq!(ctx.node_configs.len(), 1);
    }

    #[test]
    fn test_composition_context_max_authority_none() {
        let ctx = CompositionContext::new(vec!["node1".to_string()]);

        assert!(ctx.max_authority().is_none());
        assert!(!ctx.has_commander());
        assert_eq!(ctx.authorization_bonus(), 0);
    }

    #[test]
    fn test_composition_context_max_authority_with_commander() {
        use crate::models::{
            HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt, Operator,
            OperatorExt, OperatorRank,
        };

        let operator = Operator::new(
            "op1".to_string(),
            "CPT Smith".to_string(),
            OperatorRank::O3,
            AuthorityLevel::Commander,
            "11A".to_string(),
        );

        let binding = HumanMachinePair::one_to_one(operator, "node1".to_string());
        let config = NodeConfig::with_operator("Command Post".to_string(), binding);

        let ctx =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        assert_eq!(ctx.max_authority(), Some(AuthorityLevel::Commander));
        assert!(ctx.has_commander());
        assert_eq!(ctx.authorization_bonus(), 4); // 0.8 * 5 = 4
    }

    #[test]
    fn test_composition_context_authorization_bonus_levels() {
        use crate::models::{
            HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt, Operator,
            OperatorExt, OperatorRank,
        };

        // Test with Supervisor authority (0.5 * 5 = 2.5 rounds to 2 or 3)
        let operator = Operator::new(
            "op1".to_string(),
            "SGT Jones".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        let binding = HumanMachinePair::one_to_one(operator, "node1".to_string());
        let config = NodeConfig::with_operator("Control Station".to_string(), binding);

        let ctx =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        assert_eq!(ctx.max_authority(), Some(AuthorityLevel::Supervisor));
        assert!(!ctx.has_commander());
        // 0.5 * 5 = 2.5, rounds to 2 or 3
        let bonus = ctx.authorization_bonus();
        assert!((2..=3).contains(&bonus));
    }

    #[test]
    fn test_composition_context_default() {
        let ctx = CompositionContext::default();

        assert!(ctx.node_ids.is_empty());
        assert!(ctx.cell_id.is_none());
        assert!(ctx.node_configs.is_empty());
    }
}
