//! Peer selection algorithm for topology formation
//!
//! This module implements the logic for evaluating and selecting peers
//! based on node profiles, resources, geographic proximity, and hierarchy levels.

use crate::beacon::{GeoPosition, GeographicBeacon, HierarchyLevel, NodeMobility};

/// Candidate peer with selection score
#[derive(Debug, Clone)]
pub struct PeerCandidate {
    pub beacon: GeographicBeacon,
    pub score: f64,
}

/// Peer selection configuration
#[derive(Debug, Clone)]
pub struct SelectionConfig {
    /// Weight for mobility factor (0.0-1.0)
    pub mobility_weight: f64,
    /// Weight for resource availability (0.0-1.0)
    pub resource_weight: f64,
    /// Weight for battery level (0.0-1.0)
    pub battery_weight: f64,
    /// Weight for geographic proximity (0.0-1.0)
    pub proximity_weight: f64,
    /// Maximum distance in meters (None = unlimited)
    pub max_distance_meters: Option<f64>,
    /// Maximum number of linked peers per node (None = unlimited)
    pub max_children_per_parent: Option<usize>,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            mobility_weight: 0.3,
            resource_weight: 0.3,
            battery_weight: 0.2,
            proximity_weight: 0.2,
            max_distance_meters: Some(10_000.0), // 10km default
            max_children_per_parent: Some(10),   // 10 children max
        }
    }
}

impl SelectionConfig {
    /// Tactical configuration for short-range operations
    pub fn tactical() -> Self {
        Self {
            mobility_weight: 0.4,
            resource_weight: 0.25,
            battery_weight: 0.15,
            proximity_weight: 0.2,
            max_distance_meters: Some(2_000.0), // 2km
            max_children_per_parent: Some(5),
        }
    }

    /// Distributed configuration for wide-area operations
    pub fn distributed() -> Self {
        Self {
            mobility_weight: 0.2,
            resource_weight: 0.4,
            battery_weight: 0.2,
            proximity_weight: 0.2,
            max_distance_meters: None, // Unlimited
            max_children_per_parent: Some(15),
        }
    }
}

/// Peer selection algorithm
pub struct PeerSelector {
    config: SelectionConfig,
    own_position: GeoPosition,
    own_level: HierarchyLevel,
}

impl PeerSelector {
    /// Create a new peer selector
    pub fn new(
        config: SelectionConfig,
        own_position: GeoPosition,
        own_level: HierarchyLevel,
    ) -> Self {
        Self {
            config,
            own_position,
            own_level,
        }
    }

    /// Select the best peer from nearby beacons
    ///
    /// Returns None if no suitable peer found
    pub fn select_peer(&self, candidates: &[GeographicBeacon]) -> Option<PeerCandidate> {
        let mut scored: Vec<PeerCandidate> = candidates
            .iter()
            .filter(|beacon| self.is_valid_peer(beacon))
            .map(|beacon| PeerCandidate {
                beacon: beacon.clone(),
                score: self.score_candidate(beacon),
            })
            .collect();

        // Sort by score (highest first)
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        scored.into_iter().next()
    }

    /// Check if a beacon is a valid peer candidate
    fn is_valid_peer(&self, beacon: &GeographicBeacon) -> bool {
        // Must be at least one level higher in hierarchy
        if !beacon.hierarchy_level.can_be_parent_of(&self.own_level) {
            return false;
        }

        // Check distance constraints
        if let Some(max_dist) = self.config.max_distance_meters {
            let distance = self.own_position.distance_to(&beacon.position);
            if distance > max_dist {
                return false;
            }
        }

        // Check if node can accept linked peers
        if !beacon.can_parent {
            return false;
        }

        true
    }

    /// Score a candidate peer (higher is better)
    fn score_candidate(&self, beacon: &GeographicBeacon) -> f64 {
        let mut score = 0.0;

        // Mobility score: Static nodes preferred
        if let Some(mobility) = beacon.mobility {
            score += self.mobility_score(&mobility) * self.config.mobility_weight;
        } else {
            // Default score if mobility not specified
            score += 0.5 * self.config.mobility_weight;
        }

        // Resource and battery scores
        if let Some(ref resources) = beacon.resources {
            score += self.resource_score(resources) * self.config.resource_weight;
            score += self.battery_score(resources) * self.config.battery_weight;
        } else {
            // Default scores if no resources specified
            score += 0.5 * self.config.resource_weight;
            score += 0.5 * self.config.battery_weight;
        }

        // Proximity score: Closer is better
        score += self.proximity_score(beacon) * self.config.proximity_weight;

        score
    }

    /// Score based on mobility (0.0-1.0)
    /// Static = 1.0, SemiMobile = 0.6, Mobile = 0.3
    fn mobility_score(&self, mobility: &NodeMobility) -> f64 {
        match mobility {
            NodeMobility::Static => 1.0,
            NodeMobility::SemiMobile => 0.6,
            NodeMobility::Mobile => 0.3,
        }
    }

    /// Score based on resource availability (0.0-1.0)
    fn resource_score(&self, resources: &crate::beacon::NodeResources) -> f64 {
        // CPU utilization (lower is better)
        let cpu_score = 1.0 - (resources.cpu_usage_percent as f64 / 100.0);

        // Memory utilization (lower is better)
        let mem_score = 1.0 - (resources.memory_usage_percent as f64 / 100.0);

        // Bandwidth availability (higher is better, normalize to 0-1)
        let bandwidth_score = (resources.bandwidth_mbps as f64).min(100.0) / 100.0;

        // Average of all resource scores
        (cpu_score + mem_score + bandwidth_score) / 3.0
    }

    /// Score based on battery level (0.0-1.0)
    fn battery_score(&self, resources: &crate::beacon::NodeResources) -> f64 {
        if let Some(battery) = resources.battery_percent {
            battery as f64 / 100.0
        } else {
            1.0 // Assume AC powered = best
        }
    }

    /// Score based on proximity (0.0-1.0)
    /// Uses exponential decay with distance
    fn proximity_score(&self, beacon: &GeographicBeacon) -> f64 {
        let distance = self.own_position.distance_to(&beacon.position);

        // Exponential decay: score = e^(-distance / scale)
        // Scale factor = max_distance / 3 (so score ≈ 0.05 at max distance)
        let scale = self
            .config
            .max_distance_meters
            .unwrap_or(10_000.0)
            .max(1000.0)
            / 3.0;

        (-distance / scale).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_best_peer_prefers_static_nodes() {
        let selector = PeerSelector::new(
            SelectionConfig::default(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        let static_peer = create_test_beacon(
            "static",
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            50, // 50% resource usage
        );

        let mobile_peer = create_test_beacon(
            "mobile",
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Platoon,
            NodeMobility::Mobile,
            30, // 30% resource usage (better resources)
        );

        let result = selector.select_peer(&[static_peer, mobile_peer]);

        assert!(result.is_some());
        let winner = result.unwrap();
        assert_eq!(winner.beacon.node_id, "static");
    }

    #[test]
    fn test_select_peer_respects_hierarchy() {
        let selector = PeerSelector::new(
            SelectionConfig::default(),
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        // Platoon can be selected peer of Squad  (Platoon > Squad in hierarchy)
        let valid_peer = create_test_beacon(
            "valid",
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            50,
        );

        // Platform cannot be selected peer of Squad (Platform < Squad in hierarchy)
        let invalid_peer = create_test_beacon(
            "invalid",
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Platform,
            NodeMobility::Static,
            30,
        );

        let result = selector.select_peer(&[valid_peer.clone(), invalid_peer]);

        assert!(result.is_some());
        let winner = result.unwrap();
        assert_eq!(winner.beacon.node_id, "valid");
    }

    #[test]
    fn test_select_peer_prefers_closer_nodes() {
        let selector = PeerSelector::new(
            SelectionConfig {
                proximity_weight: 0.9, // Heavy weight on proximity
                mobility_weight: 0.1,
                resource_weight: 0.0,
                battery_weight: 0.0,
                ..Default::default()
            },
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        let nearby = create_test_beacon(
            "nearby",
            GeoPosition::new(37.7750, -122.4195), // ~100m away
            HierarchyLevel::Platoon,
            NodeMobility::Mobile,
            70,
        );

        let far = create_test_beacon(
            "far",
            GeoPosition::new(37.8000, -122.4400), // ~2.5km away
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            30,
        );

        let result = selector.select_peer(&[nearby, far]);

        assert!(result.is_some());
        let winner = result.unwrap();
        assert_eq!(winner.beacon.node_id, "nearby");
    }

    #[test]
    fn test_select_peer_respects_distance_limit() {
        let selector = PeerSelector::new(
            SelectionConfig {
                max_distance_meters: Some(1_000.0), // 1km limit
                ..Default::default()
            },
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        let too_far = create_test_beacon(
            "far",
            GeoPosition::new(37.8000, -122.4400), // ~2.5km away
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            30,
        );

        let result = selector.select_peer(&[too_far]);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_peer_prefers_better_resources() {
        let selector = PeerSelector::new(
            SelectionConfig {
                resource_weight: 0.9, // Heavy weight on resources
                mobility_weight: 0.1,
                proximity_weight: 0.0,
                battery_weight: 0.0,
                ..Default::default()
            },
            GeoPosition::new(37.7749, -122.4194),
            HierarchyLevel::Squad,
        );

        let low_resources = create_test_beacon(
            "low",
            GeoPosition::new(37.7750, -122.4195),
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            90, // 90% resource usage
        );

        let high_resources = create_test_beacon(
            "high",
            GeoPosition::new(37.7751, -122.4196),
            HierarchyLevel::Platoon,
            NodeMobility::Static,
            20, // 20% resource usage
        );

        let result = selector.select_peer(&[low_resources, high_resources]);

        assert!(result.is_some());
        let winner = result.unwrap();
        assert_eq!(winner.beacon.node_id, "high");
    }

    fn create_test_beacon(
        node_id: &str,
        position: GeoPosition,
        level: HierarchyLevel,
        mobility: NodeMobility,
        resource_usage: u8,
    ) -> GeographicBeacon {
        let resources = crate::beacon::NodeResources {
            cpu_cores: 4,
            memory_mb: 8192,
            bandwidth_mbps: 100,
            cpu_usage_percent: resource_usage,
            memory_usage_percent: resource_usage,
            battery_percent: Some(80),
        };

        let mut beacon = GeographicBeacon::new(node_id.to_string(), position, level);
        beacon.mobility = Some(mobility);
        beacon.resources = Some(resources);
        beacon.can_parent = true;
        beacon.parent_priority = 100;
        beacon
    }
}
