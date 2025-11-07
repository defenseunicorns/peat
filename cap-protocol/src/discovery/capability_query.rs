//! Capability-based queries for platform and squad discovery
//!
//! Implements the capability query system for finding nodes and squads
//! based on required capabilities during the bootstrap phase.
//!
//! # Architecture
//!
//! The capability query system allows C2 or nodes to discover other entities
//! based on capability requirements:
//!
//! ## Query Types
//!
//! - **Type-based**: Find entities with specific capability types (Sensor, Compute, etc.)
//! - **Confidence-based**: Filter by minimum confidence thresholds
//! - **Combination queries**: Require multiple capabilities (AND logic)
//! - **Ranked results**: Score and rank matches by relevance
//!
//! ## Use Cases
//!
//! - **Mission planning**: Find nodes with required sensor capabilities
//! - **Cell formation**: Form cells with complementary capabilities
//! - **Resource discovery**: Locate available compute/comms resources
//! - **Redundancy**: Find backup nodes with similar capabilities
//!
//! ## Example
//!
//! ```rust,ignore
//! // Find nodes with sensor AND communication capabilities
//! let query = CapabilityQuery::builder()
//!     .require_type(CapabilityType::Sensor)
//!     .require_type(CapabilityType::Communication)
//!     .min_confidence(0.8)
//!     .build();
//!
//! let matches = engine.query_platforms(&query, &nodes)?;
//! ```

use crate::models::{cell::CellState, node::NodeConfig, Capability, CapabilityExt, CapabilityType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Capability query for finding nodes or squads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityQuery {
    /// Required capability types (AND logic - all must be present)
    pub required_types: Vec<CapabilityType>,
    /// Optional capability types (OR logic - any can be present for bonus score)
    pub optional_types: Vec<CapabilityType>,
    /// Minimum confidence threshold (0.0 - 1.0)
    pub min_confidence: f32,
    /// Minimum number of total capabilities
    pub min_capability_count: Option<usize>,
    /// Maximum results to return
    pub limit: Option<usize>,
}

impl CapabilityQuery {
    /// Create a new query builder
    pub fn builder() -> CapabilityQueryBuilder {
        CapabilityQueryBuilder::new()
    }

    /// Check if a set of capabilities satisfies this query
    pub fn matches(&self, capabilities: &[Capability]) -> bool {
        // Check minimum capability count
        if let Some(min_count) = self.min_capability_count {
            if capabilities.len() < min_count {
                return false;
            }
        }

        // Check all required types are present with sufficient confidence
        for required_type in &self.required_types {
            let has_type = capabilities.iter().any(|cap| {
                cap.get_capability_type() == *required_type && cap.confidence >= self.min_confidence
            });

            if !has_type {
                return false;
            }
        }

        true
    }

    /// Calculate a relevance score for a set of capabilities (0.0 - 1.0)
    ///
    /// Score components:
    /// - Required types: 0.6 weight (normalized by count)
    /// - Optional types: 0.3 weight (normalized by count)
    /// - Average confidence: 0.1 weight
    pub fn score(&self, capabilities: &[Capability]) -> f32 {
        if capabilities.is_empty() {
            return 0.0;
        }

        let mut score = 0.0;

        // Required types score (60% weight)
        if !self.required_types.is_empty() {
            let required_score: f32 = self
                .required_types
                .iter()
                .map(|req_type| {
                    capabilities
                        .iter()
                        .filter(|cap| cap.get_capability_type() == *req_type)
                        .map(|cap| cap.confidence)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap_or(0.0)
                })
                .sum::<f32>()
                / self.required_types.len() as f32;

            score += required_score * 0.6;
        } else {
            // If no required types, give full weight
            score += 0.6;
        }

        // Optional types score (30% weight)
        if !self.optional_types.is_empty() {
            let optional_score: f32 = self
                .optional_types
                .iter()
                .map(|opt_type| {
                    capabilities
                        .iter()
                        .filter(|cap| cap.get_capability_type() == *opt_type)
                        .map(|cap| cap.confidence)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap_or(0.0)
                })
                .sum::<f32>()
                / self.optional_types.len() as f32;

            score += optional_score * 0.3;
        } else {
            score += 0.3;
        }

        // Average confidence score (10% weight)
        let avg_confidence: f32 =
            capabilities.iter().map(|cap| cap.confidence).sum::<f32>() / capabilities.len() as f32;
        score += avg_confidence * 0.1;

        score.clamp(0.0, 1.0)
    }
}

/// Builder for creating capability queries
#[derive(Debug, Default)]
pub struct CapabilityQueryBuilder {
    required_types: Vec<CapabilityType>,
    optional_types: Vec<CapabilityType>,
    min_confidence: f32,
    min_capability_count: Option<usize>,
    limit: Option<usize>,
}

impl CapabilityQueryBuilder {
    /// Create a new query builder
    pub fn new() -> Self {
        Self {
            min_confidence: 0.0,
            ..Default::default()
        }
    }

    /// Add a required capability type
    pub fn require_type(mut self, capability_type: CapabilityType) -> Self {
        self.required_types.push(capability_type);
        self
    }

    /// Add an optional capability type
    pub fn prefer_type(mut self, capability_type: CapabilityType) -> Self {
        self.optional_types.push(capability_type);
        self
    }

    /// Set minimum confidence threshold
    pub fn min_confidence(mut self, min_confidence: f32) -> Self {
        self.min_confidence = min_confidence.clamp(0.0, 1.0);
        self
    }

    /// Set minimum capability count
    pub fn min_capability_count(mut self, count: usize) -> Self {
        self.min_capability_count = Some(count);
        self
    }

    /// Set maximum results limit
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Build the query
    pub fn build(self) -> CapabilityQuery {
        CapabilityQuery {
            required_types: self.required_types,
            optional_types: self.optional_types,
            min_confidence: self.min_confidence,
            min_capability_count: self.min_capability_count,
            limit: self.limit,
        }
    }
}

/// Result of a capability query with score
#[derive(Debug, Clone)]
pub struct QueryMatch<T> {
    /// The matched entity (platform or squad)
    pub entity: T,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
}

/// Capability query engine for finding nodes and squads
pub struct CapabilityQueryEngine;

impl CapabilityQueryEngine {
    /// Create a new query engine
    pub fn new() -> Self {
        Self
    }

    /// Query nodes by capabilities
    pub fn query_platforms(
        &self,
        query: &CapabilityQuery,
        nodes: &[NodeConfig],
    ) -> Vec<QueryMatch<NodeConfig>> {
        let mut matches: Vec<QueryMatch<NodeConfig>> = nodes
            .iter()
            .filter(|node| query.matches(&node.capabilities))
            .map(|node| QueryMatch {
                score: query.score(&node.capabilities),
                entity: node.clone(),
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Apply limit if specified
        if let Some(limit) = query.limit {
            matches.truncate(limit);
        }

        matches
    }

    /// Query cells by capabilities
    pub fn query_squads(
        &self,
        query: &CapabilityQuery,
        squads: &[CellState],
    ) -> Vec<QueryMatch<CellState>> {
        let mut matches: Vec<QueryMatch<CellState>> = squads
            .iter()
            .filter(|squad| query.matches(&squad.capabilities))
            .map(|squad| QueryMatch {
                score: query.score(&squad.capabilities),
                entity: squad.clone(),
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Apply limit if specified
        if let Some(limit) = query.limit {
            matches.truncate(limit);
        }

        matches
    }

    /// Get capability statistics for a set of platforms
    pub fn platform_capability_stats(
        &self,
        nodes: &[NodeConfig],
    ) -> HashMap<CapabilityType, CapabilityStats> {
        let mut stats: HashMap<CapabilityType, Vec<f32>> = HashMap::new();

        for node in nodes {
            for capability in &node.capabilities {
                stats
                    .entry(capability.get_capability_type())
                    .or_default()
                    .push(capability.confidence);
            }
        }

        stats
            .into_iter()
            .map(|(cap_type, confidences)| {
                (cap_type, CapabilityStats::from_confidences(&confidences))
            })
            .collect()
    }
}

impl Default for CapabilityQueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistical summary of capability distribution
#[derive(Debug, Clone)]
pub struct CapabilityStats {
    /// Total count of this capability type
    pub count: usize,
    /// Average confidence
    pub avg_confidence: f32,
    /// Minimum confidence
    pub min_confidence: f32,
    /// Maximum confidence
    pub max_confidence: f32,
}

impl CapabilityStats {
    /// Calculate statistics from confidence values
    pub fn from_confidences(confidences: &[f32]) -> Self {
        let count = confidences.len();
        let sum: f32 = confidences.iter().sum();
        let avg_confidence = if count > 0 { sum / count as f32 } else { 0.0 };

        let min_confidence = confidences
            .iter()
            .copied()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let max_confidence = confidences
            .iter()
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        Self {
            count,
            avg_confidence,
            min_confidence,
            max_confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_capability(id: &str, cap_type: CapabilityType, confidence: f32) -> Capability {
        Capability::new(
            id.to_string(),
            format!("{:?} capability", cap_type),
            cap_type,
            confidence,
        )
    }

    fn create_test_platform(
        id: &str,
        platform_type: &str,
        capabilities: Vec<Capability>,
    ) -> NodeConfig {
        let mut platform = NodeConfig::new(platform_type.to_string());
        platform.id = id.to_string();
        for cap in capabilities {
            platform.add_capability(cap);
        }
        platform
    }

    #[test]
    fn test_query_builder() {
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .require_type(CapabilityType::Communication)
            .min_confidence(0.8)
            .limit(10)
            .build();

        assert_eq!(query.required_types.len(), 2);
        assert_eq!(query.min_confidence, 0.8);
        assert_eq!(query.limit, Some(10));
    }

    #[test]
    fn test_query_matches_required_types() {
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .require_type(CapabilityType::Communication)
            .min_confidence(0.7)
            .build();

        // Node with both required capabilities
        let caps1 = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.8),
        ];
        assert!(query.matches(&caps1));

        // Node missing one required capability
        let caps2 = vec![create_test_capability(
            "sensor1",
            CapabilityType::Sensor,
            0.9,
        )];
        assert!(!query.matches(&caps2));

        // Node with low confidence
        let caps3 = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.5),
        ];
        assert!(!query.matches(&caps3));
    }

    #[test]
    fn test_query_matches_min_capability_count() {
        let query = CapabilityQuery::builder().min_capability_count(3).build();

        let caps1 = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.8),
            create_test_capability("compute1", CapabilityType::Compute, 0.7),
        ];
        assert!(query.matches(&caps1));

        let caps2 = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.8),
        ];
        assert!(!query.matches(&caps2));
    }

    #[test]
    fn test_query_scoring() {
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .prefer_type(CapabilityType::Communication)
            .build();

        // Node with both required and optional
        let caps1 = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.8),
        ];
        let score1 = query.score(&caps1);

        // Node with only required
        let caps2 = vec![create_test_capability(
            "sensor1",
            CapabilityType::Sensor,
            0.9,
        )];
        let score2 = query.score(&caps2);

        // First platform should score higher
        assert!(score1 > score2);
        assert!(score1 <= 1.0);
        assert!(score2 > 0.0);
    }

    #[test]
    fn test_query_engine_platforms() {
        let engine = CapabilityQueryEngine::new();

        let nodes = vec![
            create_test_platform(
                "platform1",
                "UAV",
                vec![
                    create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
                    create_test_capability("comms1", CapabilityType::Communication, 0.8),
                ],
            ),
            create_test_platform(
                "platform2",
                "UAV",
                vec![create_test_capability(
                    "sensor2",
                    CapabilityType::Sensor,
                    0.7,
                )],
            ),
            create_test_platform(
                "platform3",
                "UAV",
                vec![
                    create_test_capability("sensor3", CapabilityType::Sensor, 0.95),
                    create_test_capability("comms3", CapabilityType::Communication, 0.9),
                    create_test_capability("compute3", CapabilityType::Compute, 0.85),
                ],
            ),
        ];

        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .prefer_type(CapabilityType::Communication)
            .min_confidence(0.7)
            .build();

        let matches = engine.query_platforms(&query, &nodes);

        // All nodes have sensor capability
        assert_eq!(matches.len(), 3);

        // platform3 should score highest (has all capabilities with high confidence)
        assert_eq!(matches[0].entity.id, "platform3");
        assert!(matches[0].score > matches[1].score);
    }

    #[test]
    fn test_query_engine_limit() {
        let engine = CapabilityQueryEngine::new();

        let nodes = vec![
            create_test_platform(
                "platform1",
                "UAV",
                vec![create_test_capability(
                    "sensor1",
                    CapabilityType::Sensor,
                    0.9,
                )],
            ),
            create_test_platform(
                "platform2",
                "UAV",
                vec![create_test_capability(
                    "sensor2",
                    CapabilityType::Sensor,
                    0.8,
                )],
            ),
            create_test_platform(
                "platform3",
                "UAV",
                vec![create_test_capability(
                    "sensor3",
                    CapabilityType::Sensor,
                    0.7,
                )],
            ),
        ];

        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .limit(2)
            .build();

        let matches = engine.query_platforms(&query, &nodes);

        assert_eq!(matches.len(), 2);
        // Should return top 2 by score
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn test_capability_stats() {
        let engine = CapabilityQueryEngine::new();

        let nodes = vec![
            create_test_platform(
                "platform1",
                "UAV",
                vec![
                    create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
                    create_test_capability("comms1", CapabilityType::Communication, 0.8),
                ],
            ),
            create_test_platform(
                "platform2",
                "UAV",
                vec![
                    create_test_capability("sensor2", CapabilityType::Sensor, 0.7),
                    create_test_capability("compute2", CapabilityType::Compute, 0.85),
                ],
            ),
        ];

        let stats = engine.platform_capability_stats(&nodes);

        assert_eq!(stats.len(), 3);
        assert_eq!(stats.get(&CapabilityType::Sensor).unwrap().count, 2);
        assert_eq!(stats.get(&CapabilityType::Communication).unwrap().count, 1);
        assert_eq!(stats.get(&CapabilityType::Compute).unwrap().count, 1);

        let sensor_stats = stats.get(&CapabilityType::Sensor).unwrap();
        assert_eq!(sensor_stats.min_confidence, 0.7);
        assert_eq!(sensor_stats.max_confidence, 0.9);
        assert!((sensor_stats.avg_confidence - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_empty_query() {
        let query = CapabilityQuery::builder().build();

        let caps = vec![
            create_test_capability("sensor1", CapabilityType::Sensor, 0.9),
            create_test_capability("comms1", CapabilityType::Communication, 0.8),
        ];

        // Empty query should match any platform
        assert!(query.matches(&caps));
        // Score should be non-zero
        assert!(query.score(&caps) > 0.0);
    }
}
