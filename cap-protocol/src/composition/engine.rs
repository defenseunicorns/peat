//! Composition engine and rule registry
//!
//! This module provides the composition engine that applies multiple
//! composition rules to sets of capabilities.

use crate::composition::rules::{CompositionContext, CompositionResult, CompositionRule};
use crate::models::capability::Capability;
use crate::Result;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Composition engine that manages and applies composition rules
pub struct CompositionEngine {
    /// Registered composition rules
    rules: Vec<Arc<dyn CompositionRule>>,
}

impl CompositionEngine {
    /// Create a new composition engine
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Register a composition rule
    ///
    /// Rules are applied in registration order during composition.
    pub fn register_rule(&mut self, rule: Arc<dyn CompositionRule>) {
        debug!("Registering composition rule: {}", rule.name());
        self.rules.push(rule);
    }

    /// Get the number of registered rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Compose capabilities using all applicable rules
    ///
    /// Applies each registered rule to the input capabilities and
    /// aggregates all detected compositions.
    #[instrument(skip(self, capabilities, context))]
    pub async fn compose(
        &self,
        capabilities: &[Capability],
        context: &CompositionContext,
    ) -> Result<Vec<CompositionResult>> {
        debug!(
            "Composing {} capabilities with {} rules",
            capabilities.len(),
            self.rules.len()
        );

        let mut all_results = Vec::new();

        for rule in &self.rules {
            if rule.applies_to(capabilities) {
                debug!("Applying rule: {}", rule.name());
                let result = rule.compose(capabilities, context).await?;

                if result.has_compositions() {
                    debug!(
                        "Rule {} produced {} compositions",
                        rule.name(),
                        result.composed_capabilities.len()
                    );
                    all_results.push(result);
                }
            } else {
                debug!("Rule {} does not apply", rule.name());
            }
        }

        debug!("Total compositions: {}", all_results.len());
        Ok(all_results)
    }

    /// Compose capabilities and flatten into a single list
    ///
    /// This is a convenience method that applies all rules and returns
    /// all composed capabilities in a flat list.
    pub async fn compose_all(
        &self,
        capabilities: &[Capability],
        context: &CompositionContext,
    ) -> Result<Vec<Capability>> {
        let results = self.compose(capabilities, context).await?;

        let composed_capabilities: Vec<Capability> = results
            .into_iter()
            .flat_map(|r| r.composed_capabilities)
            .collect();

        Ok(composed_capabilities)
    }
}

impl Default for CompositionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::rules::CompositionRule;
    use crate::models::capability::{CapabilityExt, CapabilityType};
    use async_trait::async_trait;

    // Mock rule for testing
    struct MockRule {
        name: String,
        should_apply: bool,
        result_count: usize,
    }

    impl MockRule {
        fn new(name: &str, should_apply: bool, result_count: usize) -> Self {
            Self {
                name: name.to_string(),
                should_apply,
                result_count,
            }
        }
    }

    #[async_trait]
    impl CompositionRule for MockRule {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock rule for testing"
        }

        fn applies_to(&self, _capabilities: &[Capability]) -> bool {
            self.should_apply
        }

        async fn compose(
            &self,
            _capabilities: &[Capability],
            _context: &CompositionContext,
        ) -> Result<CompositionResult> {
            let composed: Vec<Capability> = (0..self.result_count)
                .map(|i| {
                    Capability::new(
                        format!("composed_{}", i),
                        format!("Composed {}", i),
                        CapabilityType::Emergent,
                        0.8,
                    )
                })
                .collect();

            Ok(CompositionResult::new(composed, 0.8))
        }
    }

    #[test]
    fn test_engine_creation() {
        let engine = CompositionEngine::new();
        assert_eq!(engine.rule_count(), 0);
    }

    #[test]
    fn test_register_rule() {
        let mut engine = CompositionEngine::new();
        let rule = Arc::new(MockRule::new("test_rule", true, 1));

        engine.register_rule(rule);
        assert_eq!(engine.rule_count(), 1);
    }

    #[test]
    fn test_register_multiple_rules() {
        let mut engine = CompositionEngine::new();

        engine.register_rule(Arc::new(MockRule::new("rule1", true, 1)));
        engine.register_rule(Arc::new(MockRule::new("rule2", true, 1)));
        engine.register_rule(Arc::new(MockRule::new("rule3", true, 1)));

        assert_eq!(engine.rule_count(), 3);
    }

    #[tokio::test]
    async fn test_compose_with_applicable_rule() {
        let mut engine = CompositionEngine::new();
        engine.register_rule(Arc::new(MockRule::new("applicable", true, 2)));

        let capabilities = vec![Capability::new(
            "sensor".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )];

        let context = CompositionContext::new(vec!["node1".to_string()]);
        let results = engine.compose(&capabilities, &context).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].composed_capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_compose_with_non_applicable_rule() {
        let mut engine = CompositionEngine::new();
        engine.register_rule(Arc::new(MockRule::new("not_applicable", false, 2)));

        let capabilities = vec![Capability::new(
            "sensor".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )];

        let context = CompositionContext::new(vec!["node1".to_string()]);
        let results = engine.compose(&capabilities, &context).await.unwrap();

        // Rule doesn't apply, so no results
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_compose_with_multiple_rules() {
        let mut engine = CompositionEngine::new();
        engine.register_rule(Arc::new(MockRule::new("rule1", true, 1)));
        engine.register_rule(Arc::new(MockRule::new("rule2", true, 2)));
        engine.register_rule(Arc::new(MockRule::new("rule3", false, 1))); // Won't apply

        let capabilities = vec![Capability::new(
            "sensor".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )];

        let context = CompositionContext::new(vec!["node1".to_string()]);
        let results = engine.compose(&capabilities, &context).await.unwrap();

        // Only 2 rules apply
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].composed_capabilities.len(), 1);
        assert_eq!(results[1].composed_capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_compose_all() {
        let mut engine = CompositionEngine::new();
        engine.register_rule(Arc::new(MockRule::new("rule1", true, 2)));
        engine.register_rule(Arc::new(MockRule::new("rule2", true, 3)));

        let capabilities = vec![Capability::new(
            "sensor".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )];

        let context = CompositionContext::new(vec!["node1".to_string()]);
        let composed = engine.compose_all(&capabilities, &context).await.unwrap();

        // Should flatten all results: 2 + 3 = 5
        assert_eq!(composed.len(), 5);
    }

    #[tokio::test]
    async fn test_compose_empty_capabilities() {
        let mut engine = CompositionEngine::new();
        engine.register_rule(Arc::new(MockRule::new("rule1", true, 1)));

        let capabilities = vec![];
        let context = CompositionContext::new(vec!["node1".to_string()]);
        let results = engine.compose(&capabilities, &context).await.unwrap();

        // Rule still applies even with empty input
        assert_eq!(results.len(), 1);
    }
}
