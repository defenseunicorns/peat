//! Cell Capability Aggregation
//!
//! This module implements capability aggregation across squad members following ADR-004
//! human-machine teaming principles. It collects individual platform capabilities and
//! composes them into emergent cell-level capabilities with human authority integration.
//!
//! # Key Concepts
//!
//! - **Capability Collection**: Gathers capabilities from all squad members
//! - **Emergent Capabilities**: Squad-level capabilities that emerge from member composition
//! - **Human Authority**: Integrates operator authority levels into capability confidence
//! - **Confidence Aggregation**: Combines individual confidence scores with authority weights
//!
//! # Human-Machine Integration
//!
//! Following ADR-004, capability aggregation factors in:
//! - Operator authority levels (Monitoring, Conditional, Full)
//! - Human oversight requirements for critical capabilities
//! - Hybrid confidence scoring (technical capability + human authority)

use crate::models::{
    AuthorityLevel, CapabilityExt, CapabilityType, HumanMachinePair, NodeConfig, NodeState,
};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aggregated cell-level capability with human authority integration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedCapability {
    /// Capability type
    pub capability_type: CapabilityType,
    /// Aggregated confidence score (0.0-1.0)
    pub confidence: f32,
    /// Number of nodes contributing this capability
    pub contributor_count: usize,
    /// Contributing platform IDs
    pub contributors: Vec<String>,
    /// Highest authority level among contributors
    pub max_authority: Option<AuthorityLevel>,
    /// Requires human oversight for mission-critical operations
    pub requires_oversight: bool,
}

impl AggregatedCapability {
    /// Create a new aggregated capability
    pub fn new(
        capability_type: CapabilityType,
        confidence: f32,
        contributors: Vec<String>,
        max_authority: Option<AuthorityLevel>,
    ) -> Self {
        let contributor_count = contributors.len();
        let requires_oversight = Self::check_oversight_requirement(capability_type, max_authority);

        Self {
            capability_type,
            confidence,
            contributor_count,
            contributors,
            max_authority,
            requires_oversight,
        }
    }

    /// Check if this capability type requires human oversight
    fn check_oversight_requirement(
        capability_type: CapabilityType,
        max_authority: Option<AuthorityLevel>,
    ) -> bool {
        // Mission-critical capabilities require oversight unless DirectControl authority present
        match capability_type {
            CapabilityType::Payload => {
                // Weapons require DirectControl authority or oversight
                !matches!(max_authority, Some(AuthorityLevel::DirectControl))
            }
            CapabilityType::Communication => {
                // Critical comms require at least Commander authority
                matches!(max_authority, None | Some(AuthorityLevel::Observer))
            }
            _ => false, // Other capabilities don't require oversight by default
        }
    }

    /// Check if this capability is mission-ready (high confidence and appropriate authority)
    pub fn is_mission_ready(&self) -> bool {
        let confidence_threshold = if self.requires_oversight { 0.8 } else { 0.7 };

        self.confidence >= confidence_threshold
            && self.contributor_count > 0
            && (!self.requires_oversight || self.max_authority.is_some())
    }

    /// Get effective confidence factoring in authority and oversight
    pub fn effective_confidence(&self) -> f32 {
        let mut confidence = self.confidence;

        // Reduce confidence if oversight required but no high authority present
        if self.requires_oversight {
            match self.max_authority {
                Some(AuthorityLevel::DirectControl) => confidence *= 1.0, // No penalty
                Some(AuthorityLevel::Commander) => confidence *= 0.85,    // Slight penalty
                Some(AuthorityLevel::Supervisor) => confidence *= 0.7,    // Moderate penalty
                Some(AuthorityLevel::Advisor) => confidence *= 0.6,       // Significant penalty
                Some(AuthorityLevel::Observer) => confidence *= 0.6,      // Significant penalty
                None => confidence *= 0.5, // Major penalty for autonomous-only
            }
        }

        confidence.min(1.0)
    }
}

/// Cell capability aggregator
pub struct CapabilityAggregator;

impl CapabilityAggregator {
    /// Aggregate capabilities from a list of squad members
    ///
    /// # Arguments
    /// * `members` - List of (NodeConfig, NodeState) tuples for each squad member
    ///
    /// # Returns
    /// HashMap of CapabilityType to AggregatedCapability
    pub fn aggregate_capabilities(
        members: &[(NodeConfig, NodeState)],
    ) -> Result<HashMap<CapabilityType, AggregatedCapability>> {
        let mut capability_map: HashMap<
            CapabilityType,
            Vec<(String, f32, Option<AuthorityLevel>)>,
        > = HashMap::new();

        // Collect capabilities from all members
        for (config, state) in members {
            // Skip if platform is not operational
            if !state.is_operational() {
                continue;
            }

            // Get authority level from operator binding
            let authority = config
                .operator_binding
                .as_ref()
                .and_then(Self::get_max_authority);

            // Add each capability to the map
            for cap in &config.capabilities {
                capability_map
                    .entry(cap.get_capability_type())
                    .or_default()
                    .push((config.id.clone(), cap.confidence, authority));
            }
        }

        // Aggregate each capability type
        let mut aggregated = HashMap::new();
        for (cap_type, contributors) in capability_map {
            let agg_cap = Self::aggregate_capability_type(cap_type, contributors)?;
            aggregated.insert(cap_type, agg_cap);
        }

        Ok(aggregated)
    }

    /// Aggregate a single capability type from multiple contributors
    fn aggregate_capability_type(
        capability_type: CapabilityType,
        contributors: Vec<(String, f32, Option<AuthorityLevel>)>,
    ) -> Result<AggregatedCapability> {
        if contributors.is_empty() {
            return Err(Error::config_error(
                "Cannot aggregate capability with no contributors",
                None,
            ));
        }

        // Calculate aggregated confidence
        // Strategy: Take weighted average with redundancy bonus
        let avg_confidence: f32 =
            contributors.iter().map(|(_, conf, _)| conf).sum::<f32>() / contributors.len() as f32;

        // Redundancy bonus: more contributors = higher confidence
        let redundancy_bonus = match contributors.len() {
            1 => 0.0,
            2 => 0.05,
            3..=4 => 0.10,
            _ => 0.15, // Cap at 0.15 bonus for 5+ contributors
        };

        let base_confidence = (avg_confidence + redundancy_bonus).min(1.0);

        // Authority bonus: higher authority increases confidence
        let max_authority = contributors.iter().filter_map(|(_, _, auth)| *auth).max();

        let authority_bonus = match max_authority {
            Some(AuthorityLevel::DirectControl) => 0.10,
            Some(AuthorityLevel::Commander) => 0.05,
            Some(AuthorityLevel::Supervisor) => 0.03,
            Some(AuthorityLevel::Advisor) => 0.01,
            Some(AuthorityLevel::Observer) => 0.0,
            None => 0.0,
        };

        let final_confidence = (base_confidence + authority_bonus).min(1.0);

        let contributor_ids: Vec<String> = contributors.into_iter().map(|(id, _, _)| id).collect();

        Ok(AggregatedCapability::new(
            capability_type,
            final_confidence,
            contributor_ids,
            max_authority,
        ))
    }

    /// Get the maximum authority level from a human-machine pair
    fn get_max_authority(binding: &HumanMachinePair) -> Option<AuthorityLevel> {
        binding.operators.iter().map(|op| op.authority).max()
    }

    /// Calculate squad readiness score based on aggregated capabilities
    ///
    /// Returns a score from 0.0-1.0 indicating overall squad capability readiness
    pub fn calculate_readiness_score(
        capabilities: &HashMap<CapabilityType, AggregatedCapability>,
    ) -> f32 {
        if capabilities.is_empty() {
            return 0.0;
        }

        // Weight different capability types
        let weights: HashMap<CapabilityType, f32> = [
            (CapabilityType::Communication, 0.30), // Critical for coordination
            (CapabilityType::Sensor, 0.25),        // Important for awareness
            (CapabilityType::Compute, 0.20),       // Important for processing
            (CapabilityType::Payload, 0.15),       // Important for mission execution
            (CapabilityType::Mobility, 0.10),      // Important for positioning
        ]
        .into_iter()
        .collect();

        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for (cap_type, agg_cap) in capabilities {
            let weight = weights.get(cap_type).copied().unwrap_or(0.05);
            let score = agg_cap.effective_confidence();
            total_score += score * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            total_score / total_weight
        } else {
            0.0
        }
    }

    /// Identify capability gaps in the squad
    ///
    /// Returns a list of missing or weak capability types
    pub fn identify_gaps(
        capabilities: &HashMap<CapabilityType, AggregatedCapability>,
        required_capabilities: &[CapabilityType],
    ) -> Vec<CapabilityType> {
        let mut gaps = Vec::new();

        for &cap_type in required_capabilities {
            match capabilities.get(&cap_type) {
                None => gaps.push(cap_type), // Missing entirely
                Some(agg_cap) if !agg_cap.is_mission_ready() => gaps.push(cap_type), // Present but weak
                _ => {}                                                              // Adequate
            }
        }

        gaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capability, HealthStatus, Operator, OperatorRank};

    fn create_test_platform(
        id: &str,
        capabilities: Vec<(CapabilityType, f32)>,
        operator: Option<Operator>,
    ) -> (NodeConfig, NodeState) {
        let mut config = NodeConfig::new("Test".to_string());
        config.id = id.to_string();

        for (cap_type, confidence) in capabilities {
            config.add_capability(Capability::new(
                format!("{}_{:?}", id, cap_type),
                format!("{:?}", cap_type),
                cap_type,
                confidence,
            ));
        }

        if let Some(op) = operator {
            let binding = HumanMachinePair::new(
                vec![op],
                vec![id.to_string()],
                crate::models::BindingType::OneToOne,
            );
            config.operator_binding = Some(binding);
        }

        let state = NodeState::new((0.0, 0.0, 0.0));

        (config, state)
    }

    #[test]
    fn test_aggregate_single_platform() {
        let platform = create_test_platform(
            "p1",
            vec![
                (CapabilityType::Sensor, 0.8),
                (CapabilityType::Communication, 0.9),
            ],
            None,
        );

        let result = CapabilityAggregator::aggregate_capabilities(&[platform]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&CapabilityType::Sensor));
        assert!(result.contains_key(&CapabilityType::Communication));

        let sensor_cap = result.get(&CapabilityType::Sensor).unwrap();
        assert_eq!(sensor_cap.contributor_count, 1);
        assert_eq!(sensor_cap.confidence, 0.8); // No redundancy bonus for single contributor
    }

    #[test]
    fn test_aggregate_multiple_platforms_redundancy() {
        let p1 = create_test_platform("p1", vec![(CapabilityType::Sensor, 0.7)], None);
        let p2 = create_test_platform("p2", vec![(CapabilityType::Sensor, 0.8)], None);
        let p3 = create_test_platform("p3", vec![(CapabilityType::Sensor, 0.75)], None);

        let result = CapabilityAggregator::aggregate_capabilities(&[p1, p2, p3]).unwrap();

        let sensor_cap = result.get(&CapabilityType::Sensor).unwrap();
        assert_eq!(sensor_cap.contributor_count, 3);

        // Average: (0.7 + 0.8 + 0.75) / 3 = 0.75
        // Redundancy bonus for 3 contributors: 0.10
        // Expected: 0.85
        assert!((sensor_cap.confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_authority_integration() {
        let operator = Operator::new(
            "op1".to_string(),
            "John Doe".to_string(),
            OperatorRank::E5,
            AuthorityLevel::DirectControl,
            "19D".to_string(),
        );

        let p1 = create_test_platform("p1", vec![(CapabilityType::Payload, 0.7)], Some(operator));

        let result = CapabilityAggregator::aggregate_capabilities(&[p1]).unwrap();

        let payload_cap = result.get(&CapabilityType::Payload).unwrap();
        assert_eq!(
            payload_cap.max_authority,
            Some(AuthorityLevel::DirectControl)
        );

        // Base: 0.7, Authority bonus: 0.10 = 0.80
        assert!((payload_cap.confidence - 0.80).abs() < 0.01);
    }

    #[test]
    fn test_oversight_requirements() {
        // Payload capability without operator - requires oversight
        let p1 = create_test_platform("p1", vec![(CapabilityType::Payload, 0.9)], None);
        let result = CapabilityAggregator::aggregate_capabilities(&[p1]).unwrap();
        let payload_cap = result.get(&CapabilityType::Payload).unwrap();
        assert!(payload_cap.requires_oversight);

        // Payload capability with DirectControl authority - no oversight required
        let operator = Operator::new(
            "op1".to_string(),
            "Jane Smith".to_string(),
            OperatorRank::E6,
            AuthorityLevel::DirectControl,
            "11B".to_string(),
        );
        let p2 = create_test_platform("p2", vec![(CapabilityType::Payload, 0.9)], Some(operator));
        let result2 = CapabilityAggregator::aggregate_capabilities(&[p2]).unwrap();
        let payload_cap2 = result2.get(&CapabilityType::Payload).unwrap();
        assert!(!payload_cap2.requires_oversight);
    }

    #[test]
    fn test_mission_readiness() {
        let operator = Operator::new(
            "op1".to_string(),
            "Bob Johnson".to_string(),
            OperatorRank::E5,
            AuthorityLevel::DirectControl,
            "11B".to_string(),
        );

        let p1 = create_test_platform("p1", vec![(CapabilityType::Payload, 0.85)], Some(operator));
        let result = CapabilityAggregator::aggregate_capabilities(&[p1]).unwrap();
        let payload_cap = result.get(&CapabilityType::Payload).unwrap();

        // High confidence + DirectControl authority = mission ready
        assert!(payload_cap.is_mission_ready());
    }

    #[test]
    fn test_effective_confidence_with_authority() {
        // Observer authority on Payload capability - reduced confidence
        let operator = Operator::new(
            "op1".to_string(),
            "Alice Brown".to_string(),
            OperatorRank::E4,
            AuthorityLevel::Observer,
            "11B".to_string(),
        );

        let p1 = create_test_platform("p1", vec![(CapabilityType::Payload, 0.9)], Some(operator));
        let result = CapabilityAggregator::aggregate_capabilities(&[p1]).unwrap();
        let payload_cap = result.get(&CapabilityType::Payload).unwrap();

        // Base: 0.9, but reduced by 0.6x for Observer authority on oversight-required capability
        let effective = payload_cap.effective_confidence();
        assert!(effective < 0.9);
        assert!((effective - 0.54).abs() < 0.01); // 0.9 * 0.6
    }

    #[test]
    fn test_readiness_score() {
        let operator = Operator::new(
            "op1".to_string(),
            "Charlie Davis".to_string(),
            OperatorRank::E5,
            AuthorityLevel::DirectControl,
            "11B".to_string(),
        );

        let p1 = create_test_platform(
            "p1",
            vec![
                (CapabilityType::Communication, 0.9),
                (CapabilityType::Sensor, 0.8),
            ],
            Some(operator.clone()),
        );

        let p2 = create_test_platform(
            "p2",
            vec![
                (CapabilityType::Compute, 0.85),
                (CapabilityType::Payload, 0.8),
            ],
            Some(operator),
        );

        let capabilities = CapabilityAggregator::aggregate_capabilities(&[p1, p2]).unwrap();
        let score = CapabilityAggregator::calculate_readiness_score(&capabilities);

        // Should be high with good capabilities across multiple types
        assert!(score > 0.7);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_identify_gaps() {
        let p1 = create_test_platform(
            "p1",
            vec![
                (CapabilityType::Sensor, 0.8),
                (CapabilityType::Communication, 0.9),
            ],
            None,
        );

        let capabilities = CapabilityAggregator::aggregate_capabilities(&[p1]).unwrap();

        let required = vec![
            CapabilityType::Sensor,
            CapabilityType::Communication,
            CapabilityType::Payload,
            CapabilityType::Compute,
        ];

        let gaps = CapabilityAggregator::identify_gaps(&capabilities, &required);

        // Communication without operator requires oversight and is not mission-ready
        // Payload and Compute are missing entirely
        assert_eq!(gaps.len(), 3);
        assert!(gaps.contains(&CapabilityType::Communication));
        assert!(gaps.contains(&CapabilityType::Payload));
        assert!(gaps.contains(&CapabilityType::Compute));
    }

    #[test]
    fn test_skip_non_operational_platforms() {
        let mut platform = create_test_platform("p1", vec![(CapabilityType::Sensor, 0.9)], None);

        // Set platform to degraded state
        platform.1.health = HealthStatus::Degraded;

        let result = CapabilityAggregator::aggregate_capabilities(&[platform]).unwrap();

        // Should still include degraded nodes (they're operational)
        assert_eq!(result.len(), 1);

        // Critical nodes are still operational (only Failed is non-operational)
        let mut platform2 = create_test_platform("p2", vec![(CapabilityType::Sensor, 0.9)], None);
        platform2.1.health = HealthStatus::Critical;

        let result2 = CapabilityAggregator::aggregate_capabilities(&[platform2]).unwrap();

        // Critical is still operational
        assert_eq!(result2.len(), 1);

        // Now test with Failed status (truly non-operational)
        let mut platform3 = create_test_platform("p3", vec![(CapabilityType::Sensor, 0.9)], None);
        platform3.1.health = HealthStatus::Failed;

        let result3 = CapabilityAggregator::aggregate_capabilities(&[platform3]).unwrap();

        // Should exclude failed platforms
        assert_eq!(result3.len(), 0);
    }

    #[test]
    fn test_empty_squad_aggregation() {
        // Edge case: Empty squad should return empty capabilities
        let result = CapabilityAggregator::aggregate_capabilities(&[]).unwrap();
        assert_eq!(result.len(), 0);

        // Readiness score for empty squad should be 0
        let readiness = CapabilityAggregator::calculate_readiness_score(&result);
        assert_eq!(readiness, 0.0);

        // Gap identification should show all required capabilities as missing
        let required = vec![CapabilityType::Communication, CapabilityType::Sensor];
        let gaps = CapabilityAggregator::identify_gaps(&result, &required);
        assert_eq!(gaps.len(), 2);
        assert!(gaps.contains(&CapabilityType::Communication));
        assert!(gaps.contains(&CapabilityType::Sensor));
    }

    #[test]
    fn test_all_platforms_non_operational() {
        // Edge case: All nodes failed/non-operational
        let mut platform1 = create_test_platform("p1", vec![(CapabilityType::Sensor, 0.9)], None);
        platform1.1.health = HealthStatus::Failed;

        let mut platform2 =
            create_test_platform("p2", vec![(CapabilityType::Communication, 0.8)], None);
        platform2.1.health = HealthStatus::Failed;

        let result = CapabilityAggregator::aggregate_capabilities(&[platform1, platform2]).unwrap();

        // All nodes excluded, should be empty
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_zero_confidence_capability() {
        // Edge case: Capability with 0.0 confidence
        let platform = create_test_platform("p1", vec![(CapabilityType::Sensor, 0.0)], None);

        let result = CapabilityAggregator::aggregate_capabilities(&[platform]).unwrap();

        // Should still aggregate, but with low confidence
        assert_eq!(result.len(), 1);
        let sensor_cap = result.get(&CapabilityType::Sensor).unwrap();
        assert!(sensor_cap.confidence < 0.1);
    }
}
