//! Additive composition rules
//!
//! This module implements composition rules for capabilities that combine
//! additively, such as:
//! - Coverage area (sum of sensor ranges)
//! - Lift capacity (sum of payload weights)
//! - Sensor count (total number of sensors)
//! - Communication bandwidth (aggregate throughput)

use crate::composition::rules::{CompositionContext, CompositionResult, CompositionRule};
use crate::models::capability::{Capability, CapabilityExt, CapabilityType};
use crate::Result;
use async_trait::async_trait;
use serde_json::json;

/// Rule for composing additive sensor coverage
///
/// Combines multiple sensors to calculate total coverage area.
/// Metadata should include "coverage_area" in square meters.
pub struct SensorCoverageRule {
    /// Minimum number of sensors required for composition
    min_sensors: usize,
}

impl SensorCoverageRule {
    /// Create a new sensor coverage rule
    pub fn new(min_sensors: usize) -> Self {
        Self { min_sensors }
    }
}

impl Default for SensorCoverageRule {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl CompositionRule for SensorCoverageRule {
    fn name(&self) -> &str {
        "sensor_coverage"
    }

    fn description(&self) -> &str {
        "Composes additive sensor coverage from multiple sensor capabilities"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let sensor_count = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Sensor)
            .count();

        sensor_count >= self.min_sensors
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let sensors: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Sensor)
            .collect();

        if sensors.len() < self.min_sensors {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Sum coverage areas from metadata (parse from JSON string)
        let total_coverage: f64 = sensors
            .iter()
            .filter_map(|cap| {
                serde_json::from_str::<serde_json::Value>(&cap.metadata_json)
                    .ok()
                    .and_then(|v| v.get("coverage_area").and_then(|v| v.as_f64()))
            })
            .sum();

        // Average confidence across sensors
        let avg_confidence: f32 =
            sensors.iter().map(|c| c.confidence).sum::<f32>() / sensors.len() as f32;

        let mut composed = Capability::new(
            format!("composed_sensor_coverage_{}", uuid::Uuid::new_v4()),
            "Aggregate Sensor Coverage".to_string(),
            CapabilityType::Emergent,
            avg_confidence,
        );
        composed.metadata_json = json!({
            "coverage_area": total_coverage,
            "sensor_count": sensors.len(),
            "composition_type": "additive"
        })
        .to_string();

        let contributor_ids: Vec<String> = sensors.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], avg_confidence)
            .with_contributors(contributor_ids))
    }
}

/// Rule for composing additive payload capacity
///
/// Combines multiple payload capabilities to calculate total lift capacity.
/// Metadata should include "lift_capacity" in kilograms.
pub struct PayloadCapacityRule {
    /// Minimum number of payload capabilities required
    min_payloads: usize,
}

impl PayloadCapacityRule {
    /// Create a new payload capacity rule
    pub fn new(min_payloads: usize) -> Self {
        Self { min_payloads }
    }
}

impl Default for PayloadCapacityRule {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl CompositionRule for PayloadCapacityRule {
    fn name(&self) -> &str {
        "payload_capacity"
    }

    fn description(&self) -> &str {
        "Composes additive payload capacity from multiple payload capabilities"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let payload_count = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Payload)
            .count();

        payload_count >= self.min_payloads
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let payloads: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Payload)
            .collect();

        if payloads.len() < self.min_payloads {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Sum lift capacities from metadata
        let total_capacity: f64 = payloads
            .iter()
            .filter_map(|cap| {
                serde_json::from_str::<serde_json::Value>(&cap.metadata_json)
                    .ok()
                    .and_then(|v| v.get("lift_capacity").and_then(|v| v.as_f64()))
            })
            .sum();

        // Average confidence across payloads
        let avg_confidence: f32 =
            payloads.iter().map(|c| c.confidence).sum::<f32>() / payloads.len() as f32;

        let mut composed = Capability::new(
            format!("composed_payload_capacity_{}", uuid::Uuid::new_v4()),
            "Aggregate Payload Capacity".to_string(),
            CapabilityType::Emergent,
            avg_confidence,
        );
        composed.metadata_json = json!({
            "lift_capacity": total_capacity,
            "payload_count": payloads.len(),
            "composition_type": "additive"
        })
        .to_string();

        let contributor_ids: Vec<String> = payloads.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], avg_confidence)
            .with_contributors(contributor_ids))
    }
}

/// Rule for composing additive communication bandwidth
///
/// Combines multiple communication capabilities to calculate total bandwidth.
/// Metadata should include "bandwidth" in Mbps.
pub struct CommunicationBandwidthRule {
    /// Minimum number of communication capabilities required
    min_comms: usize,
}

impl CommunicationBandwidthRule {
    /// Create a new communication bandwidth rule
    pub fn new(min_comms: usize) -> Self {
        Self { min_comms }
    }
}

impl Default for CommunicationBandwidthRule {
    fn default() -> Self {
        Self::new(2)
    }
}

#[async_trait]
impl CompositionRule for CommunicationBandwidthRule {
    fn name(&self) -> &str {
        "communication_bandwidth"
    }

    fn description(&self) -> &str {
        "Composes additive communication bandwidth from multiple communication capabilities"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let comm_count = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Communication)
            .count();

        comm_count >= self.min_comms
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        let comms: Vec<&Capability> = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Communication)
            .collect();

        if comms.len() < self.min_comms {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Sum bandwidth from metadata
        let total_bandwidth: f64 = comms
            .iter()
            .filter_map(|cap| {
                serde_json::from_str::<serde_json::Value>(&cap.metadata_json)
                    .ok()
                    .and_then(|v| v.get("bandwidth").and_then(|v| v.as_f64()))
            })
            .sum();

        // Average confidence across communication capabilities
        let avg_confidence: f32 =
            comms.iter().map(|c| c.confidence).sum::<f32>() / comms.len() as f32;

        let mut composed = Capability::new(
            format!("composed_comm_bandwidth_{}", uuid::Uuid::new_v4()),
            "Aggregate Communication Bandwidth".to_string(),
            CapabilityType::Emergent,
            avg_confidence,
        );
        composed.metadata_json = json!({
            "bandwidth": total_bandwidth,
            "comm_count": comms.len(),
            "composition_type": "additive"
        })
        .to_string();

        let contributor_ids: Vec<String> = comms.iter().map(|c| c.id.clone()).collect();

        Ok(CompositionResult::new(vec![composed], avg_confidence)
            .with_contributors(contributor_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_sensor_coverage_composition() {
        let rule = SensorCoverageRule::default();

        let mut sensor1 = Capability::new(
            "sensor1".to_string(),
            "Camera 1".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        sensor1.metadata_json = json!({"coverage_area": 100.0}).to_string();

        let mut sensor2 = Capability::new(
            "sensor2".to_string(),
            "Camera 2".to_string(),
            CapabilityType::Sensor,
            0.8,
        );
        sensor2.metadata_json = json!({"coverage_area": 150.0}).to_string();

        let caps = vec![sensor1, sensor2];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        assert!(result.has_compositions());
        assert_eq!(result.composed_capabilities.len(), 1);

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.get_capability_type(), CapabilityType::Emergent);
        let metadata: serde_json::Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert_eq!(metadata["coverage_area"].as_f64().unwrap(), 250.0);
        assert_eq!(metadata["sensor_count"].as_u64().unwrap(), 2);
        assert_eq!(result.contributing_capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_sensor_coverage_insufficient_sensors() {
        let rule = SensorCoverageRule::default();

        let mut sensor1 = Capability::new(
            "sensor1".to_string(),
            "Camera 1".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        sensor1.metadata_json = json!({"coverage_area": 100.0}).to_string();

        let caps = vec![sensor1];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        assert!(!result.has_compositions());
    }

    #[tokio::test]
    async fn test_payload_capacity_composition() {
        let rule = PayloadCapacityRule::default();

        let mut payload1 = Capability::new(
            "payload1".to_string(),
            "Drone 1".to_string(),
            CapabilityType::Payload,
            0.95,
        );
        payload1.metadata_json = json!({"lift_capacity": 5.0}).to_string();

        let mut payload2 = Capability::new(
            "payload2".to_string(),
            "Drone 2".to_string(),
            CapabilityType::Payload,
            0.85,
        );
        payload2.metadata_json = json!({"lift_capacity": 7.0}).to_string();

        let caps = vec![payload1, payload2];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        assert!(result.has_compositions());
        assert_eq!(result.composed_capabilities.len(), 1);

        let composed = &result.composed_capabilities[0];
        let metadata: serde_json::Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert_eq!(metadata["lift_capacity"].as_f64().unwrap(), 12.0);
        assert_eq!(metadata["payload_count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_communication_bandwidth_composition() {
        let rule = CommunicationBandwidthRule::default();

        let mut comm1 = Capability::new(
            "comm1".to_string(),
            "Radio 1".to_string(),
            CapabilityType::Communication,
            0.9,
        );
        comm1.metadata_json = json!({"bandwidth": 10.0}).to_string();

        let mut comm2 = Capability::new(
            "comm2".to_string(),
            "Radio 2".to_string(),
            CapabilityType::Communication,
            0.85,
        );
        comm2.metadata_json = json!({"bandwidth": 15.0}).to_string();

        let mut comm3 = Capability::new(
            "comm3".to_string(),
            "Satellite".to_string(),
            CapabilityType::Communication,
            0.95,
        );
        comm3.metadata_json = json!({"bandwidth": 50.0}).to_string();

        let caps = vec![comm1, comm2, comm3];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        assert!(result.has_compositions());
        let composed = &result.composed_capabilities[0];
        let metadata: serde_json::Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert_eq!(metadata["bandwidth"].as_f64().unwrap(), 75.0);
        assert_eq!(metadata["comm_count"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_applies_to_checks() {
        let sensor_rule = SensorCoverageRule::default();
        let payload_rule = PayloadCapacityRule::default();
        let comm_rule = CommunicationBandwidthRule::default();

        let sensor = Capability::new(
            "s1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        let payload = Capability::new(
            "p1".to_string(),
            "Payload".to_string(),
            CapabilityType::Payload,
            0.9,
        );

        let caps = vec![sensor.clone(), payload.clone()];

        // Sensor rule shouldn't apply (only 1 sensor)
        assert!(!sensor_rule.applies_to(&caps));

        // With 2 sensors, should apply
        assert!(sensor_rule.applies_to(&[sensor.clone(), sensor]));

        // Payload rule should apply (2 payloads)
        assert!(payload_rule.applies_to(&[payload.clone(), payload]));

        // Comm rule shouldn't apply (no comm capabilities)
        assert!(!comm_rule.applies_to(&caps));
    }

    #[tokio::test]
    async fn test_confidence_averaging() {
        let rule = SensorCoverageRule::default();

        let mut sensor1 = Capability::new(
            "sensor1".to_string(),
            "High Confidence Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        sensor1.metadata_json = json!({"coverage_area": 100.0}).to_string();

        let mut sensor2 = Capability::new(
            "sensor2".to_string(),
            "Low Confidence Sensor".to_string(),
            CapabilityType::Sensor,
            0.5,
        );
        sensor2.metadata_json = json!({"coverage_area": 100.0}).to_string();

        let caps = vec![sensor1, sensor2];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        let composed = &result.composed_capabilities[0];
        // Average of 0.9 and 0.5 = 0.7
        assert_eq!(composed.confidence, 0.7);
        assert_eq!(result.confidence, 0.7);
    }
}
