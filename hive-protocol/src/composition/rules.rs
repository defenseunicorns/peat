//! Composition rule trait and implementations
//!
//! This module defines the core abstraction for capability composition rules.
//! Rules analyze sets of capabilities and detect composed/emergent capabilities.

use crate::models::capability::Capability;
use crate::Result;
use async_trait::async_trait;

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
}

impl CompositionContext {
    /// Create a new composition context
    pub fn new(node_ids: Vec<String>) -> Self {
        Self {
            node_ids,
            cell_id: None,
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Set the cell ID for this composition
    pub fn with_cell_id(mut self, cell_id: String) -> Self {
        self.cell_id = Some(cell_id);
        self
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
}
