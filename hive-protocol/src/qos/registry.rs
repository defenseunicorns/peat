//! QoS Registry for policy lookups (ADR-019)
//!
//! Provides a centralized registry for QoS policy management, enabling
//! custom policy overrides and runtime configuration.

use super::classification::DataType;
use super::context_manager::ContextManager;
use super::{QoSClass, QoSPolicy};
use std::collections::HashMap;

/// QoS policy registry with customizable per-data-type policies
///
/// The registry maintains a mapping from data types to QoS policies,
/// allowing for runtime customization while providing sensible defaults.
#[derive(Debug, Clone)]
pub struct QoSRegistry {
    /// Custom policy overrides (data type -> policy)
    overrides: HashMap<DataType, QoSPolicy>,
}

impl Default for QoSRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl QoSRegistry {
    /// Create an empty registry (uses default policies for all data types)
    pub fn new() -> Self {
        Self {
            overrides: HashMap::new(),
        }
    }

    /// Create a registry with default military operational policies
    ///
    /// This is the recommended starting point for tactical operations.
    /// All data types use their default policies from `DataType::default_policy()`.
    pub fn default_military() -> Self {
        Self::new()
    }

    /// Create a registry optimized for low-bandwidth conditions
    ///
    /// Reduces max sizes and increases latency tolerances.
    pub fn low_bandwidth() -> Self {
        let mut registry = Self::new();

        // Reduce image/media sizes for low bandwidth
        registry.override_policy(
            DataType::TargetImage,
            QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(30_000), // 30s instead of 5s
                max_size_bytes: Some(2 * 1024 * 1024), // 2MB instead of 10MB
                ttl_seconds: Some(7200),
                retention_priority: 4,
                preemptable: true,
            },
        );

        registry.override_policy(
            DataType::AudioIntercept,
            QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(30_000),
                max_size_bytes: Some(1024 * 1024), // 1MB instead of 5MB
                ttl_seconds: Some(7200),
                retention_priority: 4,
                preemptable: true,
            },
        );

        // Disable bulk transfers
        registry.override_policy(
            DataType::ModelUpdate,
            QoSPolicy {
                base_class: QoSClass::Bulk,
                max_latency_ms: None,
                max_size_bytes: Some(50 * 1024 * 1024), // 50MB instead of 500MB
                ttl_seconds: Some(86400),
                retention_priority: 1,
                preemptable: true,
            },
        );

        registry
    }

    /// Create a registry optimized for high-priority operations
    ///
    /// Tighter latency requirements, more bandwidth for critical data.
    pub fn high_priority() -> Self {
        let mut registry = Self::new();

        // Tighter latency for contact reports
        registry.override_policy(
            DataType::ContactReport,
            QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(250), // 250ms instead of 500ms
                max_size_bytes: Some(64 * 1024),
                ttl_seconds: None,
                retention_priority: 5,
                preemptable: false,
            },
        );

        // Promote mission retasking to critical
        registry.override_policy(
            DataType::MissionRetasking,
            QoSPolicy {
                base_class: QoSClass::Critical,
                max_latency_ms: Some(500),
                max_size_bytes: Some(64 * 1024),
                ttl_seconds: Some(7200),
                retention_priority: 5,
                preemptable: false,
            },
        );

        registry
    }

    /// Get the QoS policy for a data type
    ///
    /// Returns the custom policy if one exists, otherwise the default.
    pub fn get_policy(&self, data_type: DataType) -> QoSPolicy {
        self.overrides
            .get(&data_type)
            .cloned()
            .unwrap_or_else(|| data_type.default_policy())
    }

    /// Get the QoS class for a data type
    pub fn classify(&self, data_type: DataType) -> QoSClass {
        self.get_policy(data_type).base_class
    }

    /// Get the effective policy for a data type considering the current mission context
    ///
    /// This applies context-aware adjustments to the base policy, enabling
    /// dynamic priority changes based on mission phase.
    pub fn get_effective_policy(
        &self,
        data_type: DataType,
        context_manager: &ContextManager,
    ) -> QoSPolicy {
        let base = self.get_policy(data_type.clone());
        context_manager.adjust_policy(&base, &data_type)
    }

    /// Get the effective QoS class for a data type in the current context
    ///
    /// This is a convenience method that combines registry lookup with context adjustment.
    pub fn classify_with_context(
        &self,
        data_type: DataType,
        context_manager: &ContextManager,
    ) -> QoSClass {
        self.get_effective_policy(data_type, context_manager)
            .base_class
    }

    /// Override the policy for a specific data type
    pub fn override_policy(&mut self, data_type: DataType, policy: QoSPolicy) {
        self.overrides.insert(data_type, policy);
    }

    /// Remove a custom policy override, reverting to default
    pub fn clear_override(&mut self, data_type: &DataType) {
        self.overrides.remove(data_type);
    }

    /// Clear all custom policy overrides
    pub fn clear_all_overrides(&mut self) {
        self.overrides.clear();
    }

    /// Check if a data type has a custom policy override
    pub fn has_override(&self, data_type: &DataType) -> bool {
        self.overrides.contains_key(data_type)
    }

    /// Get the number of custom policy overrides
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }

    /// Get all data types with custom overrides
    pub fn overridden_types(&self) -> impl Iterator<Item = &DataType> {
        self.overrides.keys()
    }

    /// Check if a message with given characteristics meets the policy requirements
    pub fn meets_requirements(
        &self,
        data_type: DataType,
        latency_ms: Option<u64>,
        size_bytes: Option<usize>,
    ) -> bool {
        let policy = self.get_policy(data_type);

        // Check latency constraint
        if let (Some(max), Some(actual)) = (policy.max_latency_ms, latency_ms) {
            if actual > max {
                return false;
            }
        }

        // Check size constraint
        if let (Some(max), Some(actual)) = (policy.max_size_bytes, size_bytes) {
            if actual > max {
                return false;
            }
        }

        true
    }

    /// Get all policies grouped by QoS class
    pub fn policies_by_class(&self) -> HashMap<QoSClass, Vec<(DataType, QoSPolicy)>> {
        let mut result: HashMap<QoSClass, Vec<(DataType, QoSPolicy)>> = HashMap::new();

        for dt in DataType::all_predefined() {
            let policy = self.get_policy(dt.clone());
            result
                .entry(policy.base_class)
                .or_default()
                .push((dt.clone(), policy));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_default() {
        let registry = QoSRegistry::default();

        // Should use default policies
        let policy = registry.get_policy(DataType::ContactReport);
        assert_eq!(policy.base_class, QoSClass::Critical);
        assert_eq!(policy.max_latency_ms, Some(500));
    }

    #[test]
    fn test_registry_classify() {
        let registry = QoSRegistry::default_military();

        assert_eq!(
            registry.classify(DataType::ContactReport),
            QoSClass::Critical
        );
        assert_eq!(registry.classify(DataType::TargetImage), QoSClass::High);
        assert_eq!(registry.classify(DataType::HealthStatus), QoSClass::Normal);
        assert_eq!(registry.classify(DataType::PositionUpdate), QoSClass::Low);
        assert_eq!(registry.classify(DataType::DebugLog), QoSClass::Bulk);
    }

    #[test]
    fn test_registry_override_policy() {
        let mut registry = QoSRegistry::new();

        // Override position update to be higher priority
        registry.override_policy(
            DataType::PositionUpdate,
            QoSPolicy {
                base_class: QoSClass::Normal,
                max_latency_ms: Some(30_000),
                max_size_bytes: Some(2048),
                ttl_seconds: Some(3600),
                retention_priority: 3,
                preemptable: true,
            },
        );

        // Check override applied
        let policy = registry.get_policy(DataType::PositionUpdate);
        assert_eq!(policy.base_class, QoSClass::Normal);
        assert_eq!(policy.max_latency_ms, Some(30_000));

        // Other types unchanged
        assert_eq!(
            registry.get_policy(DataType::ContactReport).base_class,
            QoSClass::Critical
        );
    }

    #[test]
    fn test_registry_clear_override() {
        let mut registry = QoSRegistry::new();

        registry.override_policy(DataType::PositionUpdate, QoSPolicy::high());

        assert!(registry.has_override(&DataType::PositionUpdate));
        assert_eq!(registry.override_count(), 1);

        registry.clear_override(&DataType::PositionUpdate);

        assert!(!registry.has_override(&DataType::PositionUpdate));
        assert_eq!(registry.override_count(), 0);

        // Should revert to default
        let policy = registry.get_policy(DataType::PositionUpdate);
        assert_eq!(policy.base_class, QoSClass::Low);
    }

    #[test]
    fn test_low_bandwidth_registry() {
        let registry = QoSRegistry::low_bandwidth();

        // Check reduced sizes
        let image_policy = registry.get_policy(DataType::TargetImage);
        assert_eq!(image_policy.max_size_bytes, Some(2 * 1024 * 1024));
        assert_eq!(image_policy.max_latency_ms, Some(30_000));

        let model_policy = registry.get_policy(DataType::ModelUpdate);
        assert_eq!(model_policy.max_size_bytes, Some(50 * 1024 * 1024));
    }

    #[test]
    fn test_high_priority_registry() {
        let registry = QoSRegistry::high_priority();

        // Contact report has tighter latency
        let contact_policy = registry.get_policy(DataType::ContactReport);
        assert_eq!(contact_policy.max_latency_ms, Some(250));

        // Mission retasking promoted to critical
        let retask_policy = registry.get_policy(DataType::MissionRetasking);
        assert_eq!(retask_policy.base_class, QoSClass::Critical);
    }

    #[test]
    fn test_meets_requirements() {
        let registry = QoSRegistry::default_military();

        // Contact report: max 500ms latency, 32KB size
        assert!(registry.meets_requirements(DataType::ContactReport, Some(400), Some(20_000)));
        assert!(!registry.meets_requirements(DataType::ContactReport, Some(600), Some(20_000)));
        assert!(!registry.meets_requirements(DataType::ContactReport, Some(400), Some(50_000)));

        // Bulk data has no latency constraint
        assert!(registry.meets_requirements(DataType::DebugLog, Some(1_000_000), None));
    }

    #[test]
    fn test_policies_by_class() {
        let registry = QoSRegistry::default_military();
        let by_class = registry.policies_by_class();

        // Should have entries for all 5 classes
        assert_eq!(by_class.len(), 5);

        // Critical should have 4 types
        let critical = by_class.get(&QoSClass::Critical).unwrap();
        assert_eq!(critical.len(), 4);
    }

    #[test]
    fn test_overridden_types() {
        let mut registry = QoSRegistry::new();
        registry.override_policy(DataType::HealthStatus, QoSPolicy::critical());
        registry.override_policy(DataType::Heartbeat, QoSPolicy::high());

        let overridden: Vec<_> = registry.overridden_types().collect();
        assert_eq!(overridden.len(), 2);
        assert!(overridden.contains(&&DataType::HealthStatus));
        assert!(overridden.contains(&&DataType::Heartbeat));
    }

    #[test]
    fn test_get_effective_policy_standby() {
        use super::super::context::MissionContext;

        let registry = QoSRegistry::default_military();
        let ctx_manager = ContextManager::with_context(MissionContext::Standby);

        // In standby, no adjustments - effective policy matches base
        let effective = registry.get_effective_policy(DataType::TargetImage, &ctx_manager);
        let base = registry.get_policy(DataType::TargetImage);

        assert_eq!(effective.base_class, base.base_class);
        assert_eq!(effective.max_latency_ms, base.max_latency_ms);
    }

    #[test]
    fn test_get_effective_policy_execution() {
        use super::super::context::MissionContext;

        let registry = QoSRegistry::default_military();
        let ctx_manager = ContextManager::with_context(MissionContext::Execution);

        // In execution, target images are elevated to P1
        let effective = registry.get_effective_policy(DataType::TargetImage, &ctx_manager);

        assert_eq!(effective.base_class, QoSClass::Critical);
    }

    #[test]
    fn test_get_effective_policy_emergency() {
        use super::super::context::MissionContext;

        let registry = QoSRegistry::default_military();
        let ctx_manager = ContextManager::with_context(MissionContext::Emergency);

        // In emergency, health status is elevated to critical
        let effective = registry.get_effective_policy(DataType::HealthStatus, &ctx_manager);

        assert_eq!(effective.base_class, QoSClass::Critical);
    }

    #[test]
    fn test_classify_with_context() {
        use super::super::context::MissionContext;

        let registry = QoSRegistry::default_military();
        let ctx_manager = ContextManager::with_context(MissionContext::Execution);

        // TargetImage: P2 base → P1 in execution
        assert_eq!(
            registry.classify_with_context(DataType::TargetImage, &ctx_manager),
            QoSClass::Critical
        );

        // ContactReport: P1 base → P1 (no change, already at max)
        assert_eq!(
            registry.classify_with_context(DataType::ContactReport, &ctx_manager),
            QoSClass::Critical
        );

        // DebugLog: P5 base → P5 (unchanged in execution)
        assert_eq!(
            registry.classify_with_context(DataType::DebugLog, &ctx_manager),
            QoSClass::Bulk
        );
    }

    #[test]
    fn test_effective_policy_with_override() {
        use super::super::context::MissionContext;

        let mut registry = QoSRegistry::new();
        let ctx_manager = ContextManager::with_context(MissionContext::Execution);

        // Override PositionUpdate to be High priority
        registry.override_policy(
            DataType::PositionUpdate,
            QoSPolicy {
                base_class: QoSClass::High,
                max_latency_ms: Some(1000),
                max_size_bytes: Some(1024),
                ttl_seconds: Some(60),
                retention_priority: 4,
                preemptable: true,
            },
        );

        // Now get effective policy - should apply context adjustment to our override
        let effective = registry.get_effective_policy(DataType::PositionUpdate, &ctx_manager);

        // Our override set it to High (P2), and execution profile elevates position
        // updates, so we should see the context-adjusted result
        assert!(effective.base_class <= QoSClass::High);
    }
}
