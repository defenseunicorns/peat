//! Dynamic hierarchy strategy with capability-based election
//!
//! This strategy dynamically assigns roles based on node capabilities,
//! resources, and proximity. Suitable for ad-hoc networks without
//! pre-defined organizational structure.

use super::{HierarchyStrategy, NodeRole};
use crate::beacon::{GeographicBeacon, HierarchyLevel, NodeMobility, NodeProfile};

/// Election configuration weights
#[derive(Debug, Clone)]
pub struct ElectionWeights {
    /// Weight for mobility preference (static nodes preferred)
    pub mobility: f64,

    /// Weight for resource availability
    pub resources: f64,

    /// Weight for battery level
    pub battery: f64,
}

impl Default for ElectionWeights {
    fn default() -> Self {
        Self {
            mobility: 0.4,
            resources: 0.4,
            battery: 0.2,
        }
    }
}

/// Election configuration for dynamic role assignment
#[derive(Debug, Clone)]
pub struct ElectionConfig {
    /// Weights for leadership score calculation
    pub priority_weights: ElectionWeights,

    /// Hysteresis factor to prevent role flapping (0.0-1.0)
    /// New candidate must score this much better to trigger role change
    pub hysteresis: f64,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            priority_weights: ElectionWeights::default(),
            hysteresis: 0.1, // 10% better required to change roles
        }
    }
}

/// Dynamic hierarchy strategy
///
/// Dynamically assigns roles based on node capabilities and resources.
/// Nodes with better capabilities (static, high resources, good battery)
/// are preferred for leadership roles.
///
/// # Use Cases
///
/// - Ad-hoc disaster response networks
/// - Mesh network extensions
/// - Testing dynamic topology formation
///
/// # Example
///
/// ```ignore
/// let strategy = DynamicHierarchyStrategy {
///     base_level: HierarchyLevel::Squad,
///     election_config: ElectionConfig::default(),
///     allow_level_transitions: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct DynamicHierarchyStrategy {
    /// Base hierarchy level (can be elevated if no higher-level peers found)
    pub base_level: HierarchyLevel,

    /// Election configuration for role determination
    pub election_config: ElectionConfig,

    /// Whether to allow automatic level transitions
    pub allow_level_transitions: bool,
}

impl DynamicHierarchyStrategy {
    /// Create a new dynamic hierarchy strategy
    pub fn new(
        base_level: HierarchyLevel,
        election_config: ElectionConfig,
        allow_level_transitions: bool,
    ) -> Self {
        Self {
            base_level,
            election_config,
            allow_level_transitions,
        }
    }

    /// Calculate leadership score for a node profile
    ///
    /// Higher score = more suitable for leadership
    fn calculate_leadership_score(&self, profile: &NodeProfile) -> f64 {
        let weights = &self.election_config.priority_weights;
        let mut score = 0.0;

        // Mobility score: Static > SemiMobile > Mobile
        let mobility_score = match profile.mobility {
            NodeMobility::Static => 1.0,
            NodeMobility::SemiMobile => 0.6,
            NodeMobility::Mobile => 0.3,
        };
        score += mobility_score * weights.mobility;

        // Resource score: Lower utilization is better
        let cpu_score = 1.0 - (profile.resources.cpu_usage_percent as f64 / 100.0);
        let mem_score = 1.0 - (profile.resources.memory_usage_percent as f64 / 100.0);
        let resource_score = (cpu_score + mem_score) / 2.0;
        score += resource_score * weights.resources;

        // Battery score: Higher battery is better (AC powered = 1.0)
        let battery_score = profile
            .resources
            .battery_percent
            .map(|b| b as f64 / 100.0)
            .unwrap_or(1.0);
        score += battery_score * weights.battery;

        // Boost score if node explicitly configured for parenting
        if profile.can_parent {
            score *= 1.1;
        }

        // Apply parent priority multiplier (0-255 range)
        score *= 1.0 + (profile.parent_priority as f64 / 255.0);

        score
    }

    /// Calculate leadership score from a beacon
    fn calculate_leadership_score_from_beacon(&self, beacon: &GeographicBeacon) -> f64 {
        let weights = &self.election_config.priority_weights;
        let mut score = 0.0;

        // Mobility score
        if let Some(mobility) = beacon.mobility {
            let mobility_score = match mobility {
                NodeMobility::Static => 1.0,
                NodeMobility::SemiMobile => 0.6,
                NodeMobility::Mobile => 0.3,
            };
            score += mobility_score * weights.mobility;
        } else {
            score += 0.5 * weights.mobility; // Default if not specified
        }

        // Resource score
        if let Some(ref resources) = beacon.resources {
            let cpu_score = 1.0 - (resources.cpu_usage_percent as f64 / 100.0);
            let mem_score = 1.0 - (resources.memory_usage_percent as f64 / 100.0);
            let resource_score = (cpu_score + mem_score) / 2.0;
            score += resource_score * weights.resources;

            let battery_score = resources
                .battery_percent
                .map(|b| b as f64 / 100.0)
                .unwrap_or(1.0);
            score += battery_score * weights.battery;
        } else {
            score += 0.5 * (weights.resources + weights.battery); // Default if not specified
        }

        // Apply can_parent and priority
        if beacon.can_parent {
            score *= 1.1;
        }
        score *= 1.0 + (beacon.parent_priority as f64 / 255.0);

        score
    }
}

impl HierarchyStrategy for DynamicHierarchyStrategy {
    fn determine_level(&self, _node_profile: &NodeProfile) -> HierarchyLevel {
        // For now, return base level
        // Future: Could promote to higher level if no higher-level peers found
        self.base_level
    }

    fn determine_role(
        &self,
        node_profile: &NodeProfile,
        nearby_peers: &[GeographicBeacon],
    ) -> NodeRole {
        // Filter to same-level peers
        let same_level_peers: Vec<&GeographicBeacon> = nearby_peers
            .iter()
            .filter(|b| b.hierarchy_level == self.base_level)
            .collect();

        if same_level_peers.is_empty() {
            // No peers at same level, standalone mode
            return NodeRole::Standalone;
        }

        // Calculate own leadership score
        let my_score = self.calculate_leadership_score(node_profile);

        // Calculate best peer score
        let best_peer_score = same_level_peers
            .iter()
            .map(|p| self.calculate_leadership_score_from_beacon(p))
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Apply hysteresis: we must be significantly better to become leader
        let threshold = best_peer_score * (1.0 + self.election_config.hysteresis);

        if my_score >= threshold {
            NodeRole::Leader
        } else {
            NodeRole::Member
        }
    }

    fn can_transition(&self, _current_level: HierarchyLevel, _new_level: HierarchyLevel) -> bool {
        // Allow transitions if configured
        self.allow_level_transitions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{GeoPosition, NodeResources};

    fn create_high_capability_profile() -> NodeProfile {
        NodeProfile {
            mobility: NodeMobility::Static,
            resources: NodeResources {
                cpu_cores: 8,
                memory_mb: 8192,
                bandwidth_mbps: 1000,
                cpu_usage_percent: 20,
                memory_usage_percent: 30,
                battery_percent: None, // AC powered
            },
            can_parent: true,
            prefer_leaf: false,
            parent_priority: 200,
        }
    }

    fn create_low_capability_profile() -> NodeProfile {
        NodeProfile {
            mobility: NodeMobility::Mobile,
            resources: NodeResources {
                cpu_cores: 2,
                memory_mb: 1024,
                bandwidth_mbps: 50,
                cpu_usage_percent: 70,
                memory_usage_percent: 80,
                battery_percent: Some(30),
            },
            can_parent: false,
            prefer_leaf: true,
            parent_priority: 50,
        }
    }

    #[test]
    fn test_leadership_score_prefers_high_capability() {
        let strategy =
            DynamicHierarchyStrategy::new(HierarchyLevel::Squad, ElectionConfig::default(), false);

        let high_cap = create_high_capability_profile();
        let low_cap = create_low_capability_profile();

        let high_score = strategy.calculate_leadership_score(&high_cap);
        let low_score = strategy.calculate_leadership_score(&low_cap);

        assert!(high_score > low_score);
    }

    #[test]
    fn test_role_determination_standalone_when_no_peers() {
        let strategy =
            DynamicHierarchyStrategy::new(HierarchyLevel::Squad, ElectionConfig::default(), false);

        let profile = create_high_capability_profile();
        let role = strategy.determine_role(&profile, &[]);

        assert_eq!(role, NodeRole::Standalone);
    }

    #[test]
    fn test_role_determination_leader_with_high_capability() {
        let strategy =
            DynamicHierarchyStrategy::new(HierarchyLevel::Squad, ElectionConfig::default(), false);

        let high_cap = create_high_capability_profile();

        // Create a lower-capability peer beacon
        let mut low_cap_beacon = GeographicBeacon::new(
            "peer1".to_string(),
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Squad,
        );
        low_cap_beacon.mobility = Some(NodeMobility::Mobile);
        low_cap_beacon.resources = Some(NodeResources {
            cpu_cores: 2,
            memory_mb: 1024,
            bandwidth_mbps: 50,
            cpu_usage_percent: 70,
            memory_usage_percent: 80,
            battery_percent: Some(30),
        });
        low_cap_beacon.can_parent = false;
        low_cap_beacon.parent_priority = 50;

        let role = strategy.determine_role(&high_cap, &[low_cap_beacon]);

        assert_eq!(role, NodeRole::Leader);
    }

    #[test]
    fn test_level_transitions_allowed_when_configured() {
        let strategy = DynamicHierarchyStrategy::new(
            HierarchyLevel::Squad,
            ElectionConfig::default(),
            true, // Allow transitions
        );

        assert!(strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon));
    }

    #[test]
    fn test_level_transitions_disabled_when_configured() {
        let strategy = DynamicHierarchyStrategy::new(
            HierarchyLevel::Squad,
            ElectionConfig::default(),
            false, // Disallow transitions
        );

        assert!(!strategy.can_transition(HierarchyLevel::Squad, HierarchyLevel::Platoon));
    }
}
