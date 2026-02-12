//! Hybrid hierarchy strategy combining static baseline with dynamic transitions
//!
//! This strategy starts with a static baseline assignment from configuration
//! but allows dynamic transitions based on node capabilities and network conditions.

use super::dynamic_strategy::{DynamicHierarchyStrategy, ElectionConfig};
use super::{HierarchyStrategy, NodeRole};
use crate::beacon::{GeographicBeacon, HierarchyLevel, NodeProfile};

/// Transition rules for hybrid strategy
#[derive(Debug, Clone)]
pub struct TransitionRules {
    /// Allow promotion to higher hierarchy levels
    pub allow_promotion: bool,

    /// Allow demotion to lower hierarchy levels
    pub allow_demotion: bool,

    /// Maximum number of levels that can be promoted
    pub max_promotion_levels: u8,

    /// Maximum number of levels that can be demoted
    pub max_demotion_levels: u8,

    /// Minimum number of same-level peers required before promotion
    /// (Prevents premature promotion when isolated)
    pub min_peers_for_promotion: usize,
}

impl Default for TransitionRules {
    fn default() -> Self {
        Self {
            allow_promotion: true,
            allow_demotion: true,
            max_promotion_levels: 1,
            max_demotion_levels: 1,
            min_peers_for_promotion: 0,
        }
    }
}

/// Hybrid hierarchy strategy
///
/// Combines static baseline with dynamic capability-based transitions.
/// Useful for networks that have organizational structure but need to
/// adapt to changing conditions (e.g., commander nodes going offline).
///
/// # Use Cases
///
/// - Military units with defined structure but adaptation needs
/// - IoT networks with preferred topologies but failure resilience
/// - Hybrid organizational/ad-hoc networks
///
/// # Example
///
/// ```ignore
/// let strategy = HybridHierarchyStrategy {
///     baseline_level: HierarchyLevel::Squad,
///     baseline_role: NodeRole::Member,
///     election_config: ElectionConfig::default(),
///     transition_rules: TransitionRules::default(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct HybridHierarchyStrategy {
    /// Baseline hierarchy level from configuration
    pub baseline_level: HierarchyLevel,

    /// Baseline role from configuration
    pub baseline_role: NodeRole,

    /// Election configuration for dynamic role/level determination
    pub election_config: ElectionConfig,

    /// Rules governing level transitions
    pub transition_rules: TransitionRules,
}

impl HybridHierarchyStrategy {
    /// Create a new hybrid hierarchy strategy
    pub fn new(
        baseline_level: HierarchyLevel,
        baseline_role: NodeRole,
        election_config: ElectionConfig,
        transition_rules: TransitionRules,
    ) -> Self {
        Self {
            baseline_level,
            baseline_role,
            election_config,
            transition_rules,
        }
    }

    /// Create a hybrid strategy with static baseline and dynamic role election
    pub fn with_static_level_dynamic_role(
        baseline_level: HierarchyLevel,
        election_config: ElectionConfig,
    ) -> Self {
        Self {
            baseline_level,
            baseline_role: NodeRole::Member, // Will be determined dynamically
            election_config,
            transition_rules: TransitionRules {
                allow_promotion: false,
                allow_demotion: false,
                max_promotion_levels: 0,
                max_demotion_levels: 0,
                min_peers_for_promotion: 0,
            },
        }
    }

    /// Create a hybrid strategy that allows promotion when no higher-level peers exist
    pub fn with_adaptive_promotion(
        baseline_level: HierarchyLevel,
        baseline_role: NodeRole,
        election_config: ElectionConfig,
    ) -> Self {
        Self {
            baseline_level,
            baseline_role,
            election_config,
            transition_rules: TransitionRules {
                allow_promotion: true,
                allow_demotion: false,
                max_promotion_levels: 1,
                max_demotion_levels: 0,
                min_peers_for_promotion: 2, // Need at least 2 peers before promoting
            },
        }
    }

    /// Determine if promotion should occur based on network conditions
    #[allow(dead_code)] // Reserved for future adaptive level transitions
    fn should_promote(
        &self,
        nearby_peers: &[GeographicBeacon],
        current_level: HierarchyLevel,
    ) -> bool {
        if !self.transition_rules.allow_promotion {
            return false;
        }

        // Check if there are any peers at a higher level
        let higher_level = current_level.parent();
        if higher_level.is_none() {
            return false; // Already at highest level
        }

        let has_higher_level_peers = nearby_peers
            .iter()
            .any(|b| b.hierarchy_level == higher_level.unwrap());

        if has_higher_level_peers {
            return false; // Higher level peers exist, no need to promote
        }

        // Check if we have enough same-level peers to justify promotion
        let same_level_peer_count = nearby_peers
            .iter()
            .filter(|b| b.hierarchy_level == current_level)
            .count();

        same_level_peer_count >= self.transition_rules.min_peers_for_promotion
    }

    /// Determine if demotion should occur based on network conditions
    #[allow(dead_code)] // Reserved for future adaptive level transitions
    fn should_demote(
        &self,
        nearby_peers: &[GeographicBeacon],
        current_level: HierarchyLevel,
    ) -> bool {
        if !self.transition_rules.allow_demotion {
            return false;
        }

        // Check if there are many peers at a higher level (indicating we should demote)
        let higher_level = current_level.parent();
        if let Some(parent_level) = higher_level {
            let higher_level_peer_count = nearby_peers
                .iter()
                .filter(|b| b.hierarchy_level == parent_level)
                .count();

            // Demote if there are 2+ higher level peers available
            higher_level_peer_count >= 2
        } else {
            false
        }
    }

    /// Use dynamic strategy for role determination
    fn dynamic_role_determination(
        &self,
        node_profile: &NodeProfile,
        nearby_peers: &[GeographicBeacon],
    ) -> NodeRole {
        let dynamic_strategy =
            DynamicHierarchyStrategy::new(self.baseline_level, self.election_config.clone(), false);
        dynamic_strategy.determine_role(node_profile, nearby_peers)
    }
}

impl HierarchyStrategy for HybridHierarchyStrategy {
    fn determine_level(&self, _node_profile: &NodeProfile) -> HierarchyLevel {
        // For now, return baseline level
        // Future: Could implement adaptive level adjustment based on network conditions
        self.baseline_level
    }

    fn determine_role(
        &self,
        node_profile: &NodeProfile,
        nearby_peers: &[GeographicBeacon],
    ) -> NodeRole {
        // Check if we should use dynamic role determination
        let same_level_peers: Vec<&GeographicBeacon> = nearby_peers
            .iter()
            .filter(|b| b.hierarchy_level == self.baseline_level)
            .collect();

        if same_level_peers.is_empty() {
            // No same-level peers, use baseline role
            return self.baseline_role;
        }

        // Use dynamic election for role determination
        self.dynamic_role_determination(node_profile, nearby_peers)
    }

    fn can_transition(&self, current_level: HierarchyLevel, new_level: HierarchyLevel) -> bool {
        if current_level == new_level {
            return true;
        }

        // Check if transition is within allowed limits
        let level_diff = (new_level as i8 - current_level as i8).unsigned_abs();

        if new_level > current_level {
            // Promotion
            self.transition_rules.allow_promotion
                && level_diff <= self.transition_rules.max_promotion_levels
        } else {
            // Demotion
            self.transition_rules.allow_demotion
                && level_diff <= self.transition_rules.max_demotion_levels
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{GeoPosition, NodeMobility, NodeResources};

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
    fn test_hybrid_strategy_returns_baseline_level() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules::default(),
        );
        let profile = create_test_profile();

        let level = strategy.determine_level(&profile);
        assert_eq!(level, HierarchyLevel::Platoon);
    }

    #[test]
    fn test_hybrid_strategy_uses_baseline_role_when_no_peers() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Squad,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules::default(),
        );
        let profile = create_test_profile();

        let role = strategy.determine_role(&profile, &[]);
        assert_eq!(role, NodeRole::Leader);
    }

    #[test]
    fn test_hybrid_strategy_uses_dynamic_role_with_peers() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
            TransitionRules::default(),
        );
        let profile = create_test_profile();

        // Create a lower-capability peer beacon
        let mut peer_beacon = GeographicBeacon::new(
            "peer1".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );
        peer_beacon.mobility = Some(NodeMobility::Mobile);
        peer_beacon.resources = Some(NodeResources {
            cpu_cores: 2,
            memory_mb: 1024,
            bandwidth_mbps: 50,
            cpu_usage_percent: 70,
            memory_usage_percent: 80,
            battery_percent: Some(30),
        });
        peer_beacon.can_parent = false;
        peer_beacon.parent_priority = 50;

        // With peers present, should use dynamic election (high capability node becomes Leader)
        let role = strategy.determine_role(&profile, &[peer_beacon]);
        assert_eq!(role, NodeRole::Leader);
    }

    #[test]
    fn test_static_level_dynamic_role_constructor() {
        let strategy = HybridHierarchyStrategy::with_static_level_dynamic_role(
            HierarchyLevel::Platoon,
            ElectionConfig::default(),
        );

        assert_eq!(strategy.baseline_level, HierarchyLevel::Platoon);
        assert!(!strategy.transition_rules.allow_promotion);
        assert!(!strategy.transition_rules.allow_demotion);
    }

    #[test]
    fn test_adaptive_promotion_constructor() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
        );

        assert_eq!(strategy.baseline_level, HierarchyLevel::Squad);
        assert!(strategy.transition_rules.allow_promotion);
        assert!(!strategy.transition_rules.allow_demotion);
        assert_eq!(strategy.transition_rules.max_promotion_levels, 1);
        assert_eq!(strategy.transition_rules.min_peers_for_promotion, 2);
    }

    #[test]
    fn test_transition_allowed_with_promotion_enabled() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: true,
                allow_demotion: false,
                max_promotion_levels: 1,
                max_demotion_levels: 0,
                min_peers_for_promotion: 0,
            },
        );

        // Can promote one level
        assert!(strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon));

        // Cannot promote two levels
        assert!(!strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Company));

        // Cannot demote
        assert!(!strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad));
    }

    #[test]
    fn test_transition_allowed_with_demotion_enabled() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: true,
                max_promotion_levels: 0,
                max_demotion_levels: 1,
                min_peers_for_promotion: 0,
            },
        );

        // Can demote one level
        assert!(strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad));

        // Cannot promote
        assert!(!strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Company));
    }

    #[test]
    fn test_no_transitions_when_disabled() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: false,
                max_promotion_levels: 0,
                max_demotion_levels: 0,
                min_peers_for_promotion: 0,
            },
        );

        // Cannot transition to any level
        assert!(!strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Company));
        assert!(!strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad));
    }

    #[test]
    fn test_same_level_transition_always_allowed() {
        // Even with all transitions disabled, same-level should be allowed
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: false,
                max_promotion_levels: 0,
                max_demotion_levels: 0,
                min_peers_for_promotion: 0,
            },
        );

        assert!(strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Squad));
        assert!(strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Platoon));
        assert!(strategy.can_transition(HierarchyLevel::Company, HierarchyLevel::Company));
    }

    #[test]
    fn test_should_promote_no_higher_peers_enough_same_level() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
        );

        // Create 2 squad-level peers (no higher-level peers)
        let peer1 = GeographicBeacon::new(
            "peer1".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );
        let peer2 = GeographicBeacon::new(
            "peer2".to_string(),
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Squad,
        );

        assert!(strategy.should_promote(&[peer1, peer2], HierarchyLevel::Squad));
    }

    #[test]
    fn test_should_not_promote_when_higher_level_peers_exist() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
        );

        // Include a platoon-level peer (higher level exists)
        let peer1 = GeographicBeacon::new(
            "peer1".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );
        let platoon_peer = GeographicBeacon::new(
            "platoon-peer".to_string(),
            GeoPosition::new(37.7752, -122.4197),
            HierarchyLevel::Platoon,
        );

        assert!(!strategy.should_promote(&[peer1, platoon_peer], HierarchyLevel::Squad));
    }

    #[test]
    fn test_should_not_promote_when_disabled() {
        let strategy = HybridHierarchyStrategy::with_static_level_dynamic_role(
            HierarchyLevel::Squad,
            ElectionConfig::default(),
        );

        let peer = GeographicBeacon::new(
            "peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );

        assert!(!strategy.should_promote(&[peer], HierarchyLevel::Squad));
    }

    #[test]
    fn test_should_not_promote_at_highest_level() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Company,
            NodeRole::Leader,
            ElectionConfig::default(),
        );

        let peer = GeographicBeacon::new(
            "peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );

        // Company has no parent level, can't promote
        assert!(!strategy.should_promote(&[peer], HierarchyLevel::Company));
    }

    #[test]
    fn test_should_not_promote_insufficient_same_level_peers() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Squad,
            NodeRole::Member,
            ElectionConfig::default(),
        );
        // min_peers_for_promotion is 2, only 1 same-level peer
        let peer = GeographicBeacon::new(
            "peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );

        assert!(!strategy.should_promote(&[peer], HierarchyLevel::Squad));
    }

    #[test]
    fn test_should_demote_when_multiple_higher_level_peers() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: true,
                max_promotion_levels: 0,
                max_demotion_levels: 1,
                min_peers_for_promotion: 0,
            },
        );

        // 2+ higher level peers (Company is parent of Platoon)
        let higher1 = GeographicBeacon::new(
            "company1".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );
        let higher2 = GeographicBeacon::new(
            "company2".to_string(),
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Company,
        );

        assert!(strategy.should_demote(&[higher1, higher2], HierarchyLevel::Platoon));
    }

    #[test]
    fn test_should_not_demote_when_disabled() {
        let strategy = HybridHierarchyStrategy::with_adaptive_promotion(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
        );
        // allow_demotion is false

        let higher = GeographicBeacon::new(
            "company".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );

        assert!(!strategy.should_demote(&[higher.clone(), higher], HierarchyLevel::Platoon));
    }

    #[test]
    fn test_should_not_demote_at_highest_level() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Company,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: true,
                max_promotion_levels: 0,
                max_demotion_levels: 1,
                min_peers_for_promotion: 0,
            },
        );

        // Company has no parent, so can't demote
        let peer = GeographicBeacon::new(
            "peer".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );

        assert!(!strategy.should_demote(&[peer], HierarchyLevel::Company));
    }

    #[test]
    fn test_should_not_demote_with_only_one_higher_peer() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: false,
                allow_demotion: true,
                max_promotion_levels: 0,
                max_demotion_levels: 1,
                min_peers_for_promotion: 0,
            },
        );

        // Only 1 higher level peer (need 2+)
        let higher = GeographicBeacon::new(
            "company".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Company,
        );

        assert!(!strategy.should_demote(&[higher], HierarchyLevel::Platoon));
    }

    #[test]
    fn test_transition_both_directions_enabled() {
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Platoon,
            NodeRole::Member,
            ElectionConfig::default(),
            TransitionRules {
                allow_promotion: true,
                allow_demotion: true,
                max_promotion_levels: 1,
                max_demotion_levels: 1,
                min_peers_for_promotion: 0,
            },
        );

        // Can promote one level
        assert!(strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Company));
        // Can demote one level
        assert!(strategy.can_transition(HierarchyLevel::Platoon, HierarchyLevel::Squad));
        // Can't jump two levels
        assert!(!strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Company));
    }

    #[test]
    fn test_transition_rules_default() {
        let rules = TransitionRules::default();
        assert!(rules.allow_promotion);
        assert!(rules.allow_demotion);
        assert_eq!(rules.max_promotion_levels, 1);
        assert_eq!(rules.max_demotion_levels, 1);
        assert_eq!(rules.min_peers_for_promotion, 0);
    }

    #[test]
    fn test_determine_level_ignores_profile() {
        // determine_level always returns baseline_level
        let strategy = HybridHierarchyStrategy::new(
            HierarchyLevel::Company,
            NodeRole::Leader,
            ElectionConfig::default(),
            TransitionRules::default(),
        );
        let profile = create_test_profile();
        assert_eq!(strategy.determine_level(&profile), HierarchyLevel::Company);
    }
}
