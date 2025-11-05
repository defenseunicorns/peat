//! Constraint-based composition rules
//!
//! This module implements composition rules for constraint-based capabilities -
//! capabilities where team performance is limited by individual constraints.
//!
//! Examples:
//! - Team Speed: Limited by slowest member
//! - Communication Range: Depends on mesh topology
//! - Mission Duration: Limited by shortest endurance

use crate::composition::rules::{CompositionContext, CompositionResult, CompositionRule};
use crate::models::capability::{Capability, CapabilityType};
use crate::Result;
use async_trait::async_trait;
use serde_json::json;

/// Rule for determining team speed constraint
///
/// Team moves at the speed of the slowest member. This is critical for
/// coordinated operations where the team must stay together.
pub struct TeamSpeedConstraintRule {
    /// Minimum number of platforms for team movement
    min_platforms: usize,
}

impl TeamSpeedConstraintRule {
    /// Create a new team speed constraint rule
    pub fn new(min_platforms: usize) -> Self {
        Self { min_platforms }
    }
}

impl Default for TeamSpeedConstraintRule {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl CompositionRule for TeamSpeedConstraintRule {
    fn name(&self) -> &str {
        "team_speed_constraint"
    }

    fn description(&self) -> &str {
        "Determines team movement speed based on slowest member constraint"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let mobility_count = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Mobility
                    && c.metadata.get("max_speed").is_some()
            })
            .count();

        mobility_count >= self.min_platforms
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let mobility_caps: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Mobility
                    && c.metadata.get("max_speed").is_some()
            })
            .collect();

        if mobility_caps.len() < self.min_platforms {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Find minimum speed (slowest member)
        let speeds: Vec<f64> = mobility_caps
            .iter()
            .filter_map(|c| c.metadata.get("max_speed").and_then(|v| v.as_f64()))
            .collect();

        let team_speed = speeds.iter().cloned().fold(f64::INFINITY, f64::min);

        // Find slowest member
        let slowest = mobility_caps
            .iter()
            .min_by(|a, b| {
                let speed_a = a
                    .metadata
                    .get("max_speed")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let speed_b = b
                    .metadata
                    .get("max_speed")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                speed_a.partial_cmp(&speed_b).unwrap()
            })
            .unwrap();

        // Confidence is based on slowest member's confidence
        let team_confidence = slowest.confidence;

        let composed = Capability {
            id: format!("constraint_team_speed_{}", uuid::Uuid::new_v4()),
            name: "Team Speed".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: team_confidence,
            metadata: json!({
                "composition_type": "constraint",
                "pattern": "team_speed",
                "team_speed": team_speed,
                "platform_count": mobility_caps.len(),
                "limiting_platform": slowest.id,
                "individual_speeds": speeds,
                "description": "Team movement speed constrained by slowest member"
            }),
        };

        let contributor_ids: Vec<String> = mobility_caps.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], team_confidence)
            .with_contributors(contributor_ids))
    }
}

/// Rule for determining effective communication range
///
/// Communication range depends on whether the team has mesh networking.
/// - With mesh: Range is maximum (relay through intermediaries)
/// - Without mesh: Range is minimum (all must reach all)
pub struct CommunicationRangeConstraintRule {
    /// Minimum number of nodes for communication
    min_nodes: usize,
    /// Whether mesh networking is available
    has_mesh: bool,
}

impl CommunicationRangeConstraintRule {
    /// Create a new communication range constraint rule
    pub fn new(min_nodes: usize, has_mesh: bool) -> Self {
        Self {
            min_nodes,
            has_mesh,
        }
    }
}

impl Default for CommunicationRangeConstraintRule {
    fn default() -> Self {
        Self::new(2, false) // Default: no mesh, direct comms only
    }
}

#[async_trait]
impl CompositionRule for CommunicationRangeConstraintRule {
    fn name(&self) -> &str {
        "communication_range_constraint"
    }

    fn description(&self) -> &str {
        "Determines effective communication range based on mesh capability"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let comm_count = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Communication
                    && c.metadata.get("range").is_some()
            })
            .count();

        comm_count >= self.min_nodes
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let comm_caps: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Communication
                    && c.metadata.get("range").is_some()
            })
            .collect();

        if comm_caps.len() < self.min_nodes {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Get communication ranges
        let ranges: Vec<f64> = comm_caps
            .iter()
            .filter_map(|c| c.metadata.get("range").and_then(|v| v.as_f64()))
            .collect();

        // Determine effective range based on mesh capability
        let (effective_range, limiting_factor) = if self.has_mesh {
            // With mesh: can relay through intermediaries, use max range
            let max_range = ranges.iter().cloned().fold(0.0, f64::max);
            (max_range, "mesh_enabled".to_string())
        } else {
            // Without mesh: all must reach all, use min range
            let min_range = ranges.iter().cloned().fold(f64::INFINITY, f64::min);
            (min_range, "direct_comms_only".to_string())
        };

        // Find limiting node
        let limiting_node = if self.has_mesh {
            comm_caps
                .iter()
                .max_by(|a, b| {
                    let range_a = a
                        .metadata
                        .get("range")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let range_b = b
                        .metadata
                        .get("range")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    range_a.partial_cmp(&range_b).unwrap()
                })
                .unwrap()
        } else {
            comm_caps
                .iter()
                .min_by(|a, b| {
                    let range_a = a
                        .metadata
                        .get("range")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let range_b = b
                        .metadata
                        .get("range")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    range_a.partial_cmp(&range_b).unwrap()
                })
                .unwrap()
        };

        // Average confidence across communication capabilities
        let avg_confidence: f32 =
            comm_caps.iter().map(|c| c.confidence).sum::<f32>() / comm_caps.len() as f32;

        let composed = Capability {
            id: format!("constraint_comm_range_{}", uuid::Uuid::new_v4()),
            name: "Team Communication Range".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: avg_confidence,
            metadata: json!({
                "composition_type": "constraint",
                "pattern": "communication_range",
                "effective_range": effective_range,
                "mesh_enabled": self.has_mesh,
                "limiting_factor": limiting_factor,
                "limiting_node": limiting_node.id,
                "node_count": comm_caps.len(),
                "individual_ranges": ranges,
                "description": if self.has_mesh {
                    "Extended range through mesh networking"
                } else {
                    "Range constrained by weakest link"
                }
            }),
        };

        let contributor_ids: Vec<String> = comm_caps.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], avg_confidence)
            .with_contributors(contributor_ids))
    }
}

/// Rule for determining mission duration constraint
///
/// Mission duration is limited by the platform with shortest endurance.
/// Critical for planning operations that require the entire team.
pub struct MissionDurationConstraintRule {
    /// Minimum number of platforms
    min_platforms: usize,
}

impl MissionDurationConstraintRule {
    /// Create a new mission duration constraint rule
    pub fn new(min_platforms: usize) -> Self {
        Self { min_platforms }
    }
}

impl Default for MissionDurationConstraintRule {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl CompositionRule for MissionDurationConstraintRule {
    fn name(&self) -> &str {
        "mission_duration_constraint"
    }

    fn description(&self) -> &str {
        "Determines maximum mission duration based on shortest platform endurance"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let platforms_with_endurance = capabilities
            .iter()
            .filter(|c| c.metadata.get("endurance_minutes").is_some())
            .count();

        platforms_with_endurance >= self.min_platforms
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let platforms: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| c.metadata.get("endurance_minutes").is_some())
            .collect();

        if platforms.len() < self.min_platforms {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Get endurance values
        let endurances: Vec<f64> = platforms
            .iter()
            .filter_map(|c| c.metadata.get("endurance_minutes").and_then(|v| v.as_f64()))
            .collect();

        // Mission duration is limited by shortest endurance
        let mission_duration = endurances.iter().cloned().fold(f64::INFINITY, f64::min);

        // Find limiting platform
        let limiting_platform = platforms
            .iter()
            .min_by(|a, b| {
                let endurance_a = a
                    .metadata
                    .get("endurance_minutes")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let endurance_b = b
                    .metadata
                    .get("endurance_minutes")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                endurance_a.partial_cmp(&endurance_b).unwrap()
            })
            .unwrap();

        // Confidence based on limiting platform
        let mission_confidence = limiting_platform.confidence;

        let composed = Capability {
            id: format!("constraint_mission_duration_{}", uuid::Uuid::new_v4()),
            name: "Team Mission Duration".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: mission_confidence,
            metadata: json!({
                "composition_type": "constraint",
                "pattern": "mission_duration",
                "mission_duration_minutes": mission_duration,
                "platform_count": platforms.len(),
                "limiting_platform": limiting_platform.id,
                "individual_endurances": endurances,
                "description": "Mission duration constrained by shortest endurance"
            }),
        };

        let contributor_ids: Vec<String> = platforms.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], mission_confidence)
            .with_contributors(contributor_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_team_speed_constraint() {
        let rule = TeamSpeedConstraintRule::default();

        let fast_platform = Capability {
            id: "fast1".to_string(),
            name: "Fast Drone".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.9,
            metadata: json!({"max_speed": 20.0}), // 20 m/s
        };

        let slow_platform = Capability {
            id: "slow1".to_string(),
            name: "Slow Ground Vehicle".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.85,
            metadata: json!({"max_speed": 5.0}), // 5 m/s
        };

        let caps = vec![fast_platform, slow_platform];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Team Speed");
        // Team speed should be limited by slowest member (5.0 m/s)
        assert_eq!(composed.metadata["team_speed"].as_f64().unwrap(), 5.0);
        assert_eq!(
            composed.metadata["limiting_platform"].as_str().unwrap(),
            "slow1"
        );
        // Confidence should match slowest member
        assert_eq!(composed.confidence, 0.85);
    }

    #[tokio::test]
    async fn test_communication_range_without_mesh() {
        let rule = CommunicationRangeConstraintRule::new(2, false); // No mesh

        let long_range = Capability {
            id: "comm1".to_string(),
            name: "Long Range Radio".to_string(),
            capability_type: CapabilityType::Communication,
            confidence: 0.9,
            metadata: json!({"range": 1000.0}), // 1km
        };

        let short_range = Capability {
            id: "comm2".to_string(),
            name: "Short Range Radio".to_string(),
            capability_type: CapabilityType::Communication,
            confidence: 0.85,
            metadata: json!({"range": 200.0}), // 200m
        };

        let caps = vec![long_range, short_range];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // Without mesh, range is limited by weakest link (200m)
        assert_eq!(
            composed.metadata["effective_range"].as_f64().unwrap(),
            200.0
        );
        assert!(!composed.metadata["mesh_enabled"].as_bool().unwrap());
        assert_eq!(
            composed.metadata["limiting_node"].as_str().unwrap(),
            "comm2"
        );
    }

    #[tokio::test]
    async fn test_communication_range_with_mesh() {
        let rule = CommunicationRangeConstraintRule::new(2, true); // With mesh

        let long_range = Capability {
            id: "comm1".to_string(),
            name: "Long Range Radio".to_string(),
            capability_type: CapabilityType::Communication,
            confidence: 0.9,
            metadata: json!({"range": 1000.0}),
        };

        let short_range = Capability {
            id: "comm2".to_string(),
            name: "Short Range Radio".to_string(),
            capability_type: CapabilityType::Communication,
            confidence: 0.85,
            metadata: json!({"range": 200.0}),
        };

        let caps = vec![long_range, short_range];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // With mesh, can use maximum range (1000m)
        assert_eq!(
            composed.metadata["effective_range"].as_f64().unwrap(),
            1000.0
        );
        assert!(composed.metadata["mesh_enabled"].as_bool().unwrap());
        assert_eq!(
            composed.metadata["limiting_node"].as_str().unwrap(),
            "comm1"
        );
    }

    #[tokio::test]
    async fn test_mission_duration_constraint() {
        let rule = MissionDurationConstraintRule::default();

        let long_endurance = Capability {
            id: "platform1".to_string(),
            name: "Fixed-Wing UAV".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.95,
            metadata: json!({"endurance_minutes": 120.0}), // 2 hours
        };

        let short_endurance = Capability {
            id: "platform2".to_string(),
            name: "Quadcopter".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.8,
            metadata: json!({"endurance_minutes": 25.0}), // 25 minutes
        };

        let caps = vec![long_endurance, short_endurance];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Team Mission Duration");
        // Mission duration limited by shortest endurance (25 minutes)
        assert_eq!(
            composed.metadata["mission_duration_minutes"]
                .as_f64()
                .unwrap(),
            25.0
        );
        assert_eq!(
            composed.metadata["limiting_platform"].as_str().unwrap(),
            "platform2"
        );
        // Confidence matches limiting platform
        assert_eq!(composed.confidence, 0.8);
    }

    #[tokio::test]
    async fn test_constraint_rules_dont_apply_insufficient_platforms() {
        let speed_rule = TeamSpeedConstraintRule::default();
        let comm_rule = CommunicationRangeConstraintRule::default();
        let duration_rule = MissionDurationConstraintRule::default();

        // Single platform
        let single_platform = Capability {
            id: "platform1".to_string(),
            name: "Solo Platform".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.9,
            metadata: json!({"max_speed": 10.0, "endurance_minutes": 60.0}),
        };

        let caps = vec![single_platform];

        // All rules require at least 2 platforms
        assert!(!speed_rule.applies_to(&caps));
        assert!(!comm_rule.applies_to(&caps));
        assert!(!duration_rule.applies_to(&caps));
    }

    #[tokio::test]
    async fn test_team_speed_with_three_platforms() {
        let rule = TeamSpeedConstraintRule::default();

        let platforms: Vec<Capability> = vec![
            ("fast", 25.0, 0.95),
            ("medium", 15.0, 0.9),
            ("slow", 8.0, 0.85),
        ]
        .into_iter()
        .map(|(name, speed, confidence)| Capability {
            id: format!("platform_{}", name),
            name: name.to_string(),
            capability_type: CapabilityType::Mobility,
            confidence,
            metadata: json!({"max_speed": speed}),
        })
        .collect();

        let context = CompositionContext::new(vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
        ]);

        let result = rule.compose(&platforms, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // Team speed constrained by slowest (8.0)
        assert_eq!(composed.metadata["team_speed"].as_f64().unwrap(), 8.0);
        assert_eq!(composed.metadata["platform_count"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_constraint_metadata_accuracy() {
        let rule = TeamSpeedConstraintRule::default();

        let platform1 = Capability {
            id: "p1".to_string(),
            name: "Platform 1".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.9,
            metadata: json!({"max_speed": 12.5}),
        };

        let platform2 = Capability {
            id: "p2".to_string(),
            name: "Platform 2".to_string(),
            capability_type: CapabilityType::Mobility,
            confidence: 0.85,
            metadata: json!({"max_speed": 18.3}),
        };

        let caps = vec![platform1, platform2];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        let composed = &result.composed_capabilities[0];

        // Check that all individual speeds are recorded
        let individual_speeds = composed.metadata["individual_speeds"].as_array().unwrap();
        assert_eq!(individual_speeds.len(), 2);
        assert!(individual_speeds.contains(&json!(12.5)));
        assert!(individual_speeds.contains(&json!(18.3)));
    }
}
