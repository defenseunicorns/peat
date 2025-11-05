//! Emergent composition rules
//!
//! This module implements composition rules for emergent capabilities - new
//! capabilities that arise from specific combinations of individual capabilities.
//!
//! Examples:
//! - ISR Chain: Sensor + Compute + Communication → Intelligence gathering
//! - 3D Mapping: Camera + Lidar + Compute → Detailed 3D maps
//! - Strike Chain: ISR + Strike + BDA → Complete targeting cycle

use crate::composition::rules::{CompositionContext, CompositionResult, CompositionRule};
use crate::models::capability::{Capability, CapabilityType};
use crate::Result;
use async_trait::async_trait;
use serde_json::json;

/// Rule for detecting ISR (Intelligence, Surveillance, Reconnaissance) chain capability
///
/// Requires: Sensor + Compute + Communication capabilities
/// Emergent capability: Complete ISR chain for intelligence gathering
pub struct IsrChainRule {
    /// Minimum confidence threshold for each component
    min_confidence: f32,
}

impl IsrChainRule {
    /// Create a new ISR chain rule
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

impl Default for IsrChainRule {
    fn default() -> Self {
        Self::new(0.7)
    }
}

#[async_trait]
impl CompositionRule for IsrChainRule {
    fn name(&self) -> &str {
        "isr_chain"
    }

    fn description(&self) -> &str {
        "Detects emergent ISR chain capability from sensor + compute + communication"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        let has_sensor = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Sensor && c.confidence >= self.min_confidence
        });

        let has_compute = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Compute && c.confidence >= self.min_confidence
        });

        let has_comms = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Communication
                && c.confidence >= self.min_confidence
        });

        has_sensor && has_compute && has_comms
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        // Find best capability of each required type
        let best_sensor = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Sensor)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        let best_compute = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Compute)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        let best_comms = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Communication)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        // Check if we have all required components
        if let (Some(sensor), Some(compute), Some(comms)) = (best_sensor, best_compute, best_comms)
        {
            // Emergent capability confidence is minimum of components (weakest link)
            let chain_confidence = sensor
                .confidence
                .min(compute.confidence)
                .min(comms.confidence);

            let composed = Capability {
                id: format!("emergent_isr_chain_{}", uuid::Uuid::new_v4()),
                name: "ISR Chain".to_string(),
                capability_type: CapabilityType::Emergent,
                confidence: chain_confidence,
                metadata: json!({
                    "composition_type": "emergent",
                    "pattern": "isr_chain",
                    "components": {
                        "sensor": sensor.id,
                        "compute": compute.id,
                        "communication": comms.id
                    },
                    "description": "Complete intelligence gathering capability"
                }),
            };

            let contributors = vec![sensor.id.clone(), compute.id.clone(), comms.id.clone()];

            return Ok(CompositionResult::new(vec![composed], chain_confidence)
                .with_contributors(contributors));
        }

        Ok(CompositionResult::new(vec![], 0.0))
    }
}

/// Rule for detecting 3D mapping capability
///
/// Requires: Camera + Lidar + Compute capabilities
/// Emergent capability: Detailed 3D environment mapping
pub struct Mapping3dRule {
    /// Minimum confidence threshold for each component
    min_confidence: f32,
}

impl Mapping3dRule {
    /// Create a new 3D mapping rule
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

impl Default for Mapping3dRule {
    fn default() -> Self {
        Self::new(0.7)
    }
}

#[async_trait]
impl CompositionRule for Mapping3dRule {
    fn name(&self) -> &str {
        "mapping_3d"
    }

    fn description(&self) -> &str {
        "Detects emergent 3D mapping capability from camera + lidar + compute"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        // Look for camera sensor
        let has_camera = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Sensor
                && c.confidence >= self.min_confidence
                && (c.metadata.get("sensor_type").and_then(|v| v.as_str()) == Some("camera"))
        });

        // Look for lidar sensor
        let has_lidar = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Sensor
                && c.confidence >= self.min_confidence
                && (c.metadata.get("sensor_type").and_then(|v| v.as_str()) == Some("lidar"))
        });

        let has_compute = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Compute && c.confidence >= self.min_confidence
        });

        has_camera && has_lidar && has_compute
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        // Find required capabilities
        let camera = capabilities.iter().find(|c| {
            c.capability_type == CapabilityType::Sensor
                && (c.metadata.get("sensor_type").and_then(|v| v.as_str()) == Some("camera"))
        });

        let lidar = capabilities.iter().find(|c| {
            c.capability_type == CapabilityType::Sensor
                && (c.metadata.get("sensor_type").and_then(|v| v.as_str()) == Some("lidar"))
        });

        let compute = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Compute)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        if let (Some(camera), Some(lidar), Some(compute)) = (camera, lidar, compute) {
            // Confidence is minimum of all components
            let mapping_confidence = camera
                .confidence
                .min(lidar.confidence)
                .min(compute.confidence);

            let composed = Capability {
                id: format!("emergent_3d_mapping_{}", uuid::Uuid::new_v4()),
                name: "3D Mapping".to_string(),
                capability_type: CapabilityType::Emergent,
                confidence: mapping_confidence,
                metadata: json!({
                    "composition_type": "emergent",
                    "pattern": "3d_mapping",
                    "components": {
                        "camera": camera.id,
                        "lidar": lidar.id,
                        "compute": compute.id
                    },
                    "description": "Real-time 3D environment mapping"
                }),
            };

            let contributors = vec![camera.id.clone(), lidar.id.clone(), compute.id.clone()];

            return Ok(CompositionResult::new(vec![composed], mapping_confidence)
                .with_contributors(contributors));
        }

        Ok(CompositionResult::new(vec![], 0.0))
    }
}

/// Rule for detecting strike chain capability
///
/// Requires: ISR + Strike + BDA (Battle Damage Assessment) capabilities
/// Emergent capability: Complete targeting and assessment cycle
pub struct StrikeChainRule {
    /// Minimum confidence threshold for each component
    min_confidence: f32,
}

impl StrikeChainRule {
    /// Create a new strike chain rule
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

impl Default for StrikeChainRule {
    fn default() -> Self {
        Self::new(0.8) // Higher threshold for lethal operations
    }
}

#[async_trait]
impl CompositionRule for StrikeChainRule {
    fn name(&self) -> &str {
        "strike_chain"
    }

    fn description(&self) -> &str {
        "Detects emergent strike chain capability from ISR + strike + BDA"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        // Look for ISR capability (could be emergent from another rule)
        let has_isr = capabilities.iter().any(|c| {
            (c.capability_type == CapabilityType::Emergent
                && (c.metadata.get("pattern").and_then(|v| v.as_str()) == Some("isr_chain"))
                || c.metadata
                    .get("isr_capable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false))
                && c.confidence >= self.min_confidence
        });

        // Look for strike/payload capability
        let has_strike = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Payload
                && c.confidence >= self.min_confidence
                && c.metadata
                    .get("strike_capable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
        });

        // Look for BDA capability (sensor for assessment)
        let has_bda = capabilities.iter().any(|c| {
            c.capability_type == CapabilityType::Sensor && c.confidence >= self.min_confidence
        });

        has_isr && has_strike && has_bda
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        // Find ISR capability
        let isr = capabilities.iter().find(|c| {
            c.capability_type == CapabilityType::Emergent
                && (c.metadata.get("pattern").and_then(|v| v.as_str()) == Some("isr_chain"))
        });

        // Find strike capability
        let strike = capabilities
            .iter()
            .filter(|c| {
                c.capability_type == CapabilityType::Payload
                    && c.metadata
                        .get("strike_capable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            })
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        // Find BDA sensor
        let bda = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Sensor)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        if let (Some(isr), Some(strike), Some(bda)) = (isr, strike, bda) {
            // Confidence is minimum of all components (critical for strike operations)
            let chain_confidence = isr.confidence.min(strike.confidence).min(bda.confidence);

            let composed = Capability {
                id: format!("emergent_strike_chain_{}", uuid::Uuid::new_v4()),
                name: "Strike Chain".to_string(),
                capability_type: CapabilityType::Emergent,
                confidence: chain_confidence,
                metadata: json!({
                    "composition_type": "emergent",
                    "pattern": "strike_chain",
                    "components": {
                        "isr": isr.id,
                        "strike": strike.id,
                        "bda": bda.id
                    },
                    "description": "Complete targeting cycle with assessment",
                    "requires_human_approval": true // Safety critical
                }),
            };

            let contributors = vec![isr.id.clone(), strike.id.clone(), bda.id.clone()];

            return Ok(CompositionResult::new(vec![composed], chain_confidence)
                .with_contributors(contributors));
        }

        Ok(CompositionResult::new(vec![], 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_isr_chain_detection() {
        let rule = IsrChainRule::default();

        let sensor = Capability {
            id: "sensor1".to_string(),
            name: "EO Camera".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.9,
            metadata: json!({"sensor_type": "camera"}),
        };

        let compute = Capability {
            id: "compute1".to_string(),
            name: "Edge Compute".to_string(),
            capability_type: CapabilityType::Compute,
            confidence: 0.85,
            metadata: json!({}),
        };

        let comms = Capability {
            id: "comms1".to_string(),
            name: "Tactical Radio".to_string(),
            capability_type: CapabilityType::Communication,
            confidence: 0.8,
            metadata: json!({"bandwidth": 10.0}),
        };

        let caps = vec![sensor, compute, comms];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());
        assert_eq!(result.composed_capabilities.len(), 1);

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.capability_type, CapabilityType::Emergent);
        assert_eq!(composed.name, "ISR Chain");
        // Confidence should be minimum of all components (0.8)
        assert_eq!(composed.confidence, 0.8);
        assert_eq!(result.contributing_capabilities.len(), 3);
    }

    #[tokio::test]
    async fn test_isr_chain_missing_component() {
        let rule = IsrChainRule::default();

        let sensor = Capability::new(
            "sensor1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        );

        let compute = Capability::new(
            "compute1".to_string(),
            "Compute".to_string(),
            CapabilityType::Compute,
            0.85,
        );

        // Missing communication capability
        let caps = vec![sensor, compute];

        assert!(!rule.applies_to(&caps));
    }

    #[tokio::test]
    async fn test_3d_mapping_detection() {
        let rule = Mapping3dRule::default();

        let camera = Capability {
            id: "camera1".to_string(),
            name: "RGB Camera".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.95,
            metadata: json!({"sensor_type": "camera"}),
        };

        let lidar = Capability {
            id: "lidar1".to_string(),
            name: "3D Lidar".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.9,
            metadata: json!({"sensor_type": "lidar"}),
        };

        let compute = Capability {
            id: "compute1".to_string(),
            name: "GPU Compute".to_string(),
            capability_type: CapabilityType::Compute,
            confidence: 0.85,
            metadata: json!({}),
        };

        let caps = vec![camera, lidar, compute];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "3D Mapping");
        assert_eq!(composed.confidence, 0.85); // Min of components
        assert_eq!(result.contributing_capabilities.len(), 3);
    }

    #[tokio::test]
    async fn test_strike_chain_detection() {
        let rule = StrikeChainRule::default();

        // Create an ISR capability (would come from IsrChainRule)
        let isr = Capability {
            id: "isr1".to_string(),
            name: "ISR Chain".to_string(),
            capability_type: CapabilityType::Emergent,
            confidence: 0.9,
            metadata: json!({
                "pattern": "isr_chain"
            }),
        };

        let strike = Capability {
            id: "strike1".to_string(),
            name: "Precision Munition".to_string(),
            capability_type: CapabilityType::Payload,
            confidence: 0.95,
            metadata: json!({"strike_capable": true}),
        };

        let bda_sensor = Capability {
            id: "bda1".to_string(),
            name: "BDA Camera".to_string(),
            capability_type: CapabilityType::Sensor,
            confidence: 0.85,
            metadata: json!({}),
        };

        let caps = vec![isr, strike, bda_sensor];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Strike Chain");
        assert_eq!(composed.confidence, 0.85); // Min of all components
        assert!(composed.metadata["requires_human_approval"]
            .as_bool()
            .unwrap());
        assert_eq!(result.contributing_capabilities.len(), 3);
    }

    #[tokio::test]
    async fn test_low_confidence_component_affects_emergent() {
        let rule = IsrChainRule::default();

        let sensor = Capability::new(
            "sensor1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.95,
        );

        let compute = Capability::new(
            "compute1".to_string(),
            "Compute".to_string(),
            CapabilityType::Compute,
            0.9,
        );

        // Low confidence comms - this should drag down the emergent capability
        let comms = Capability::new(
            "comms1".to_string(),
            "Comms".to_string(),
            CapabilityType::Communication,
            0.5,
        );

        let caps = vec![sensor, compute, comms];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        let result = rule.compose(&caps, &context).await.unwrap();

        // Emergent confidence should be limited by weakest link
        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.confidence, 0.5);
    }
}
