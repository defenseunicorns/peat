//! Redundant composition rules
//!
//! This module implements composition rules for redundant capabilities - capabilities
//! that benefit from redundancy to improve reliability, coverage, or availability.
//!
//! Examples:
//! - Detection Reliability: Multiple sensors increase detection confidence
//! - Continuous Coverage: Overlapping sensor fields ensure no gaps
//! - Fault Tolerance: Redundant systems provide backup capability

use crate::composition::rules::{CompositionContext, CompositionResult, CompositionRule};
use crate::models::capability::{Capability, CapabilityType};
use crate::Result;
use async_trait::async_trait;
use serde_json::json;

/// Rule for improving detection reliability through redundant sensors
///
/// Multiple sensors of the same type increase detection confidence through redundancy.
/// Uses probabilistic model: P(detection) = 1 - ∏(1 - P(detection_i))
pub struct DetectionReliabilityRule {
    /// Minimum number of redundant sensors required
    min_sensors: usize,
    /// Confidence threshold for individual sensors
    min_confidence: f32,
}

impl DetectionReliabilityRule {
    /// Create a new detection reliability rule
    pub fn new(min_sensors: usize, min_confidence: f32) -> Self {
        Self {
            min_sensors,
            min_confidence,
        }
    }

    /// Calculate combined detection probability from redundant sensors
    ///
    /// Uses probability formula: P(any detects) = 1 - ∏(1 - P(sensor_i detects))
    fn combined_confidence(&self, confidences: &[f32]) -> f32 {
        let failure_prob: f32 = confidences.iter().map(|c| 1.0 - c).product();
        1.0 - failure_prob
    }
}

impl Default for DetectionReliabilityRule {
    fn default() -> Self {
        Self::new(2, 0.6)
    }
}

#[async_trait]
impl CompositionRule for DetectionReliabilityRule {
    fn name(&self) -> &str {
        "detection_reliability"
    }

    fn description(&self) -> &str {
        "Improves detection confidence through redundant sensor coverage"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let qualified_sensors = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Sensor && c.confidence >= self.min_confidence
            })
            .count();

        qualified_sensors >= self.min_sensors
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let sensors: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Sensor && c.confidence >= self.min_confidence
            })
            .collect();

        if sensors.len() < self.min_sensors {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Calculate combined detection probability
        let confidences: Vec<f32> = sensors.iter().map(|s| s.confidence).collect();
        let combined_confidence = self.combined_confidence(&confidences);

        // Calculate coverage overlap (if metadata available)
        let total_coverage: f64 = sensors
            .iter()
            .filter_map(|cap| cap.metadata.get("coverage_area").and_then(|v| v.as_f64()))
            .sum();

        let composed = Capability {
            id: format!("redundant_detection_{}", uuid::Uuid::new_v4()),
            name: "Redundant Detection".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: combined_confidence,
            metadata: json!({
                "composition_type": "redundant",
                "pattern": "detection_reliability",
                "sensor_count": sensors.len(),
                "coverage_area": total_coverage,
                "individual_confidences": confidences,
                "reliability_improvement": combined_confidence - confidences.iter().cloned().fold(0.0, f32::max),
                "description": "Improved detection through sensor redundancy"
            }),
        };

        let contributor_ids: Vec<String> = sensors.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], combined_confidence)
            .with_contributors(contributor_ids))
    }
}

/// Rule for ensuring continuous coverage through temporal overlap
///
/// Multiple platforms with overlapping coverage windows ensure continuous monitoring
/// of an area without gaps.
pub struct ContinuousCoverageRule {
    /// Minimum number of platforms for continuous coverage
    min_platforms: usize,
    /// Minimum overlap percentage required (0.0 - 1.0)
    #[allow(dead_code)]
    min_overlap: f32,
}

impl ContinuousCoverageRule {
    /// Create a new continuous coverage rule
    pub fn new(min_platforms: usize, min_overlap: f32) -> Self {
        Self {
            min_platforms,
            min_overlap: min_overlap.clamp(0.0, 1.0),
        }
    }
}

impl Default for ContinuousCoverageRule {
    fn default() -> Self {
        Self::new(2, 0.3) // 30% overlap recommended
    }
}

#[async_trait]
impl CompositionRule for ContinuousCoverageRule {
    fn name(&self) -> &str {
        "continuous_coverage"
    }

    fn description(&self) -> &str {
        "Ensures continuous area coverage through temporal overlap of multiple platforms"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let qualified_sensors = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Sensor
                    && c.metadata.get("coverage_area").is_some()
            })
            .count();

        qualified_sensors >= self.min_platforms
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let sensors: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Sensor
                    && c.metadata.get("coverage_area").is_some()
            })
            .collect();

        if sensors.len() < self.min_platforms {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Calculate total coverage area
        let total_coverage: f64 = sensors
            .iter()
            .filter_map(|cap| cap.metadata.get("coverage_area").and_then(|v| v.as_f64()))
            .sum();

        // Estimate overlap (simplified - assumes some redundancy)
        let overlap_factor = if sensors.len() > 1 {
            // Rough estimate: more sensors = more overlap
            0.2 + (sensors.len() as f32 - 1.0) * 0.1
        } else {
            0.0
        };

        // Confidence based on number of platforms and individual confidences
        let avg_confidence: f32 =
            sensors.iter().map(|s| s.confidence).sum::<f32>() / sensors.len() as f32;

        // Boost confidence for continuous coverage
        let continuous_confidence = (avg_confidence + overlap_factor * 0.2).min(1.0);

        let composed = Capability {
            id: format!("continuous_coverage_{}", uuid::Uuid::new_v4()),
            name: "Continuous Coverage".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: continuous_confidence,
            metadata: json!({
                "composition_type": "redundant",
                "pattern": "continuous_coverage",
                "platform_count": sensors.len(),
                "total_coverage_area": total_coverage,
                "estimated_overlap": overlap_factor,
                "coverage_redundancy": (sensors.len() as f32 * overlap_factor),
                "cell_id": context.cell_id,
                "description": "Continuous monitoring through overlapping coverage"
            }),
        };

        let contributor_ids: Vec<String> = sensors.iter().map(|c| c.id.clone()).collect();

        Ok(
            CompositionResult::new(vec![composed], continuous_confidence)
                .with_contributors(contributor_ids),
        )
    }
}

/// Rule for fault-tolerant capability through redundant systems
///
/// Multiple identical capabilities provide backup and fault tolerance.
/// System remains operational even if some components fail.
pub struct FaultToleranceRule {
    /// Minimum number of redundant capabilities required
    min_redundancy: usize,
    /// Capability type to provide fault tolerance for
    capability_type: CapabilityType,
}

impl FaultToleranceRule {
    /// Create a new fault tolerance rule
    pub fn new(min_redundancy: usize, capability_type: CapabilityType) -> Self {
        Self {
            min_redundancy,
            capability_type,
        }
    }

    /// Calculate system reliability with N redundant components
    ///
    /// Assumes independent failures: R(system) = 1 - ∏(1 - R(component_i))
    fn system_reliability(&self, component_confidences: &[f32]) -> f32 {
        let failure_prob: f32 = component_confidences.iter().map(|c| 1.0 - c).product();
        1.0 - failure_prob
    }
}

impl Default for FaultToleranceRule {
    fn default() -> Self {
        Self::new(3, CapabilityType::Communication) // Triple redundant comms
    }
}

#[async_trait]
impl CompositionRule for FaultToleranceRule {
    fn name(&self) -> &str {
        "fault_tolerance"
    }

    fn description(&self) -> &str {
        "Provides fault-tolerant capability through redundant systems"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let redundant_count = capabilities
            .iter()
            .filter(|c| c.capability_type == self.capability_type)
            .count();

        redundant_count >= self.min_redundancy
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let redundant_caps: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| c.capability_type == self.capability_type)
            .collect();

        if redundant_caps.len() < self.min_redundancy {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Calculate system reliability
        let confidences: Vec<f32> = redundant_caps.iter().map(|c| c.confidence).collect();
        let system_reliability = self.system_reliability(&confidences);

        let composed = Capability {
            id: format!(
                "fault_tolerant_{:?}_{}",
                self.capability_type,
                uuid::Uuid::new_v4()
            ),
            name: format!("Fault-Tolerant {:?}", self.capability_type),
            capability_type: CapabilityType::Emergent,
            confidence: system_reliability,
            metadata: json!({
                "composition_type": "redundant",
                "pattern": "fault_tolerance",
                "base_capability_type": format!("{:?}", self.capability_type),
                "redundancy_level": redundant_caps.len(),
                "system_reliability": system_reliability,
                "individual_reliabilities": confidences,
                "description": format!("Fault-tolerant {:?} with {}-way redundancy",
                    self.capability_type, redundant_caps.len())
            }),
        };

        let contributor_ids: Vec<String> = redundant_caps.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], system_reliability)
            .with_contributors(contributor_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_detection_reliability_two_sensors() {
        let rule = DetectionReliabilityRule::default();

        let sensor1 = Capability {
            id: "sensor1".to_string(),
            name: "Camera 1".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.7,
            metadata: json!({"coverage_area": 100.0}),
        };

        let sensor2 = Capability {
            id: "sensor2".to_string(),
            name: "Camera 2".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.7,
            metadata: json!({"coverage_area": 100.0}),
        };

        let caps = vec![sensor1, sensor2];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // P(detect) = 1 - (1-0.7)(1-0.7) = 1 - 0.09 = 0.91
        assert!((composed.confidence - 0.91).abs() < 0.01);
        assert_eq!(composed.metadata["sensor_count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_detection_reliability_three_sensors() {
        let rule = DetectionReliabilityRule::new(3, 0.6);

        let sensors: Vec<Capability> = (0..3)
            .map(|i| Capability {
                id: format!("sensor{}", i),
                name: format!("Sensor {}", i),
                capability_type: CapabilityType::Sensor,
                confidence: 0.7,
                metadata: json!({}),
            })
            .collect();

        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&sensors, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // P(detect) = 1 - (1-0.7)^3 = 1 - 0.027 = 0.973
        assert!((composed.confidence - 0.973).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_detection_reliability_insufficient_sensors() {
        let rule = DetectionReliabilityRule::default();

        let sensor1 = Capability::new(
            "sensor1".to_string(),
            "Single Sensor".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        let caps = vec![sensor1];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(!result.has_compositions());
    }

    #[tokio::test]
    async fn test_continuous_coverage() {
        let rule = ContinuousCoverageRule::default();

        let sensor1 = Capability {
            id: "sensor1".to_string(),
            name: "Platform 1".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.85,
            metadata: json!({"coverage_area": 200.0}),
        };

        let sensor2 = Capability {
            id: "sensor2".to_string(),
            name: "Platform 2".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.8,
            metadata: json!({"coverage_area": 200.0}),
        };

        let caps = vec![sensor1, sensor2];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()])
            .with_cell_id("cell_alpha".to_string());

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Continuous Coverage");
        assert_eq!(
            composed.metadata["total_coverage_area"].as_f64().unwrap(),
            400.0
        );
        assert!(composed.confidence > 0.8); // Should be boosted by redundancy
        assert_eq!(result.contributing_capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_fault_tolerance_communication() {
        let rule = FaultToleranceRule::default(); // 3-way redundant comms

        let comms: Vec<Capability> = (0..3)
            .map(|i| Capability {
                id: format!("comm{}", i),
                name: format!("Radio {}", i),
                capability_type: CapabilityType::Communication,
                confidence: 0.8,
                metadata: json!({"bandwidth": 10.0}),
            })
            .collect();

        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&comms));

        let result = rule.compose(&comms, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // System reliability = 1 - (1-0.8)^3 = 1 - 0.008 = 0.992
        assert!((composed.confidence - 0.992).abs() < 0.01);
        assert_eq!(composed.metadata["redundancy_level"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_fault_tolerance_insufficient_redundancy() {
        let rule = FaultToleranceRule::new(4, CapabilityType::Compute);

        let compute_caps: Vec<Capability> = (0..3)
            .map(|i| {
                Capability::new(
                    format!("compute{}", i),
                    format!("Compute {}", i),
                    CapabilityType::Compute,
                    0.9,
                )
            })
            .collect();

        let context = CompositionContext::new(vec!["node1".to_string()]);

        // Only 3 but needs 4
        assert!(!rule.applies_to(&compute_caps));

        let result = rule.compose(&compute_caps, &context).await.unwrap();
        assert!(!result.has_compositions());
    }

    #[tokio::test]
    async fn test_redundancy_improves_low_confidence_sensors() {
        let rule = DetectionReliabilityRule::new(2, 0.5);

        // Two sensors with low individual confidence
        let sensor1 = Capability::new(
            "sensor1".to_string(),
            "Weak Sensor 1".to_string(),
            CapabilityType::Sensor,
            0.6,
        );

        let sensor2 = Capability::new(
            "sensor2".to_string(),
            "Weak Sensor 2".to_string(),
            CapabilityType::Sensor,
            0.6,
        );

        let caps = vec![sensor1, sensor2];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        // P(detect) = 1 - (1-0.6)^2 = 1 - 0.16 = 0.84
        // Redundancy significantly improves reliability!
        assert!((composed.confidence - 0.84).abs() < 0.01);
        assert!(composed.confidence > 0.6); // Better than individual sensors
    }

    #[tokio::test]
    async fn test_combined_confidence_calculation() {
        let rule = DetectionReliabilityRule::default();

        // Test with different confidence levels
        let confidences = vec![0.7, 0.8, 0.9];
        let combined = rule.combined_confidence(&confidences);

        // P(any detects) = 1 - (1-0.7)(1-0.8)(1-0.9) = 1 - 0.006 = 0.994
        assert!((combined - 0.994).abs() < 0.01);
    }
}
