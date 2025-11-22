//! Static hierarchy strategy with fixed role assignment
//!
//! This strategy uses pre-configured hierarchy levels and roles,
//! typically for well-defined organizational structures (e.g., military command).

use super::{HierarchyStrategy, NodeRole};
use crate::beacon::{GeographicBeacon, HierarchyLevel, NodeProfile};

/// Static hierarchy strategy
///
/// Assigns fixed hierarchy level and role from configuration.
/// No dynamic transitions are allowed.
///
/// # Use Cases
///
/// - Military command structures with defined roles
/// - Fixed infrastructure nodes (command posts, relay stations)
/// - Testing and validation with known topologies
///
/// # Example
///
/// ```ignore
/// let strategy = StaticHierarchyStrategy {
///     assigned_level: HierarchyLevel::Squad,
///     assigned_role: NodeRole::Leader,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct StaticHierarchyStrategy {
    /// Assigned hierarchy level (from organizational configuration)
    pub assigned_level: HierarchyLevel,

    /// Assigned role (from organizational configuration)
    pub assigned_role: NodeRole,
}

impl StaticHierarchyStrategy {
    /// Create a new static hierarchy strategy
    pub fn new(assigned_level: HierarchyLevel, assigned_role: NodeRole) -> Self {
        Self {
            assigned_level,
            assigned_role,
        }
    }
}

impl HierarchyStrategy for StaticHierarchyStrategy {
    fn determine_level(&self, _node_profile: &NodeProfile) -> HierarchyLevel {
        // Always return the assigned level
        self.assigned_level
    }

    fn determine_role(
        &self,
        _node_profile: &NodeProfile,
        _nearby_peers: &[GeographicBeacon],
    ) -> NodeRole {
        // Always return the assigned role
        self.assigned_role
    }

    fn can_transition(&self, _current_level: HierarchyLevel, _new_level: HierarchyLevel) -> bool {
        // Static strategy never allows transitions
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{NodeMobility, NodeResources};

    fn create_test_profile() -> NodeProfile {
        NodeProfile {
            mobility: NodeMobility::Static,
            resources: NodeResources {
                cpu_cores: 4,
                memory_mb: 2048,
                bandwidth_mbps: 100,
                cpu_usage_percent: 30,
                memory_usage_percent: 40,
                battery_percent: None,
            },
            can_parent: true,
            prefer_leaf: false,
            parent_priority: 100,
        }
    }

    #[test]
    fn test_static_strategy_returns_assigned_level() {
        let strategy = StaticHierarchyStrategy::new(HierarchyLevel::Platoon, NodeRole::Leader);
        let profile = create_test_profile();

        let level = strategy.determine_level(&profile);
        assert_eq!(level, HierarchyLevel::Platoon);
    }

    #[test]
    fn test_static_strategy_returns_assigned_role() {
        let strategy = StaticHierarchyStrategy::new(HierarchyLevel::Squad, NodeRole::Member);
        let profile = create_test_profile();

        let role = strategy.determine_role(&profile, &[]);
        assert_eq!(role, NodeRole::Member);
    }

    #[test]
    fn test_static_strategy_no_transitions() {
        let strategy = StaticHierarchyStrategy::new(HierarchyLevel::Squad, NodeRole::Leader);

        // Cannot transition to any level
        assert!(!strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon));
        assert!(!strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Company));
        assert!(!strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad));
    }
}
