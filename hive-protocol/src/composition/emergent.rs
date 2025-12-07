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
use crate::models::CapabilityExt;
use crate::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

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
            c.get_capability_type() == CapabilityType::Sensor && c.confidence >= self.min_confidence
        });

        let has_compute = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Compute
                && c.confidence >= self.min_confidence
        });

        let has_comms = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Communication
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
            .filter(|c| c.get_capability_type() == CapabilityType::Sensor)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        let best_compute = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Compute)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        let best_comms = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Communication)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        // Check if we have all required components
        if let (Some(sensor), Some(compute), Some(comms)) = (best_sensor, best_compute, best_comms)
        {
            // Emergent capability confidence is minimum of components (weakest link)
            let chain_confidence = sensor
                .confidence
                .min(compute.confidence)
                .min(comms.confidence);

            let mut composed = Capability::new(
                format!("emergent_isr_chain_{}", uuid::Uuid::new_v4()),
                "ISR Chain".to_string(),
                CapabilityType::Emergent,
                chain_confidence,
            );
            composed.metadata_json = serde_json::to_string(&json!({
                "composition_type": "emergent",
                "pattern": "isr_chain",
                "components": {
                    "sensor": sensor.id,
                    "compute": compute.id,
                    "communication": comms.id
                },
                "description": "Complete intelligence gathering capability"
            }))
            .unwrap_or_default();

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
            c.get_capability_type() == CapabilityType::Sensor
                && c.confidence >= self.min_confidence
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("sensor_type")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "camera")
                    })
                    .unwrap_or(false)
        });

        // Look for lidar sensor
        let has_lidar = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Sensor
                && c.confidence >= self.min_confidence
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("sensor_type")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "lidar")
                    })
                    .unwrap_or(false)
        });

        let has_compute = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Compute
                && c.confidence >= self.min_confidence
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
            c.get_capability_type() == CapabilityType::Sensor
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("sensor_type")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "camera")
                    })
                    .unwrap_or(false)
        });

        let lidar = capabilities.iter().find(|c| {
            c.get_capability_type() == CapabilityType::Sensor
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("sensor_type")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "lidar")
                    })
                    .unwrap_or(false)
        });

        let compute = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Compute)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        if let (Some(camera), Some(lidar), Some(compute)) = (camera, lidar, compute) {
            // Confidence is minimum of all components
            let mapping_confidence = camera
                .confidence
                .min(lidar.confidence)
                .min(compute.confidence);

            let mut composed = Capability::new(
                format!("emergent_3d_mapping_{}", uuid::Uuid::new_v4()),
                "3D Mapping".to_string(),
                CapabilityType::Emergent,
                mapping_confidence,
            );
            composed.metadata_json = serde_json::to_string(&json!({
                "composition_type": "emergent",
                "pattern": "3d_mapping",
                "components": {
                    "camera": camera.id,
                    "lidar": lidar.id,
                    "compute": compute.id
                },
                "description": "Real-time 3D environment mapping"
            }))
            .unwrap_or_default();

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
            c.get_capability_type() == CapabilityType::Emergent
                && c.confidence >= self.min_confidence
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .map(|v| {
                        let is_isr_pattern =
                            v.get("pattern").and_then(|p| p.as_str()) == Some("isr_chain");
                        let is_isr_capable = v
                            .get("isr_capable")
                            .and_then(|i| i.as_bool())
                            .unwrap_or(false);
                        is_isr_pattern || is_isr_capable
                    })
                    .unwrap_or(false)
        });

        // Look for strike/payload capability
        let has_strike = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Payload
                && c.confidence >= self.min_confidence
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| v.get("strike_capable").and_then(|s| s.as_bool()))
                    .unwrap_or(false)
        });

        // Look for BDA capability (sensor for assessment)
        let has_bda = capabilities.iter().any(|c| {
            c.get_capability_type() == CapabilityType::Sensor && c.confidence >= self.min_confidence
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
            c.get_capability_type() == CapabilityType::Emergent
                && serde_json::from_str::<Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("pattern")
                            .and_then(|p| p.as_str())
                            .map(|s| s == "isr_chain")
                    })
                    .unwrap_or(false)
        });

        // Find strike capability
        let strike = capabilities
            .iter()
            .filter(|c| {
                c.get_capability_type() == CapabilityType::Payload
                    && serde_json::from_str::<Value>(&c.metadata_json)
                        .ok()
                        .and_then(|v| v.get("strike_capable").and_then(|s| s.as_bool()))
                        .unwrap_or(false)
            })
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        // Find BDA sensor
        let bda = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Sensor)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        if let (Some(isr), Some(strike), Some(bda)) = (isr, strike, bda) {
            // Confidence is minimum of all components (critical for strike operations)
            let chain_confidence = isr.confidence.min(strike.confidence).min(bda.confidence);

            let mut composed = Capability::new(
                format!("emergent_strike_chain_{}", uuid::Uuid::new_v4()),
                "Strike Chain".to_string(),
                CapabilityType::Emergent,
                chain_confidence,
            );
            composed.metadata_json = serde_json::to_string(&json!({
                "composition_type": "emergent",
                "pattern": "strike_chain",
                "components": {
                    "isr": isr.id,
                    "strike": strike.id,
                    "bda": bda.id
                },
                "description": "Complete targeting cycle with assessment",
                "requires_human_approval": true // Safety critical
            }))
            .unwrap_or_default();

            let contributors = vec![isr.id.clone(), strike.id.clone(), bda.id.clone()];

            return Ok(CompositionResult::new(vec![composed], chain_confidence)
                .with_contributors(contributors));
        }

        Ok(CompositionResult::new(vec![], 0.0))
    }
}

/// Rule for detecting authorization coverage capability
///
/// Checks if the composition has human-in-the-loop authorization coverage.
/// Requires: Communication capability + Node with bound Operator having sufficient authority
/// Emergent capability: Authorization/command coverage for the party
pub struct AuthorizationCoverageRule {
    /// Minimum authority level required
    min_authority: crate::models::AuthorityLevel,
}

impl AuthorizationCoverageRule {
    /// Create a new authorization coverage rule
    pub fn new(min_authority: crate::models::AuthorityLevel) -> Self {
        Self { min_authority }
    }

    /// Create rule requiring Commander authority (for lethal/critical actions)
    pub fn commander_required() -> Self {
        Self::new(crate::models::AuthorityLevel::Commander)
    }

    /// Create rule requiring Supervisor authority (for general override)
    pub fn supervisor_required() -> Self {
        Self::new(crate::models::AuthorityLevel::Supervisor)
    }
}

impl Default for AuthorizationCoverageRule {
    fn default() -> Self {
        Self::commander_required()
    }
}

#[async_trait]
impl CompositionRule for AuthorizationCoverageRule {
    fn name(&self) -> &str {
        "authorization_coverage"
    }

    fn description(&self) -> &str {
        "Detects authorization coverage from communication + human operator with sufficient authority"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        // Requires at least Communication capability
        // The actual authority check happens in compose() using context.node_configs
        capabilities
            .iter()
            .any(|c| c.get_capability_type() == CapabilityType::Communication)
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        context: &CompositionContext,
    ) -> Result<CompositionResult> {
        use crate::models::{AuthorityLevelExt, HumanMachinePairExt};

        // Find best communication capability
        let best_comms = capabilities
            .iter()
            .filter(|c| c.get_capability_type() == CapabilityType::Communication)
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());

        // Check for operator with sufficient authority
        let max_authority = context.max_authority();

        let has_sufficient_authority = max_authority
            .map(|auth| auth >= self.min_authority)
            .unwrap_or(false);

        if let (Some(comms), true) = (best_comms, has_sufficient_authority) {
            let authority = max_authority.unwrap();
            let auth_score = authority.to_score() as f32;

            // Confidence is combination of comms quality and authority level
            let coverage_confidence = (comms.confidence * 0.4 + auth_score * 0.6).min(1.0);

            let mut composed = Capability::new(
                format!("emergent_auth_coverage_{}", uuid::Uuid::new_v4()),
                "Authorization Coverage".to_string(),
                CapabilityType::Emergent,
                coverage_confidence,
            );

            // Find the node with the authorizing operator
            let authorizing_node = context
                .node_configs
                .iter()
                .find(|config| {
                    config
                        .operator_binding
                        .as_ref()
                        .and_then(|b| b.max_authority())
                        .map(|a| a >= self.min_authority)
                        .unwrap_or(false)
                })
                .map(|c| c.id.clone());

            composed.metadata_json = serde_json::to_string(&json!({
                "composition_type": "emergent",
                "pattern": "authorization_coverage",
                "components": {
                    "communication": comms.id,
                    "authorizing_node": authorizing_node,
                },
                "authority_level": format!("{:?}", authority),
                "authorization_bonus": context.authorization_bonus(),
                "can_authorize_strike": authority == crate::models::AuthorityLevel::Commander,
                "description": "Human-in-the-loop authorization capability"
            }))
            .unwrap_or_default();

            let contributors = vec![comms.id.clone()];

            return Ok(CompositionResult::new(vec![composed], coverage_confidence)
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

        let mut sensor = Capability::new(
            "sensor1".to_string(),
            "EO Camera".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        sensor.metadata_json =
            serde_json::to_string(&json!({"sensor_type": "camera"})).unwrap_or_default();

        let compute = Capability::new(
            "compute1".to_string(),
            "Edge Compute".to_string(),
            CapabilityType::Compute,
            0.85,
        );

        let mut comms = Capability::new(
            "comms1".to_string(),
            "Tactical Radio".to_string(),
            CapabilityType::Communication,
            0.8,
        );
        comms.metadata_json =
            serde_json::to_string(&json!({"bandwidth": 10.0})).unwrap_or_default();

        let caps = vec![sensor, compute, comms];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());
        assert_eq!(result.composed_capabilities.len(), 1);

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.get_capability_type(), CapabilityType::Emergent);
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

        let mut camera = Capability::new(
            "camera1".to_string(),
            "RGB Camera".to_string(),
            CapabilityType::Sensor,
            0.95,
        );
        camera.metadata_json =
            serde_json::to_string(&json!({"sensor_type": "camera"})).unwrap_or_default();

        let mut lidar = Capability::new(
            "lidar1".to_string(),
            "3D Lidar".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        lidar.metadata_json =
            serde_json::to_string(&json!({"sensor_type": "lidar"})).unwrap_or_default();

        let compute = Capability::new(
            "compute1".to_string(),
            "GPU Compute".to_string(),
            CapabilityType::Compute,
            0.85,
        );

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
        let mut isr = Capability::new(
            "isr1".to_string(),
            "ISR Chain".to_string(),
            CapabilityType::Emergent,
            0.9,
        );
        isr.metadata_json = serde_json::to_string(&json!({
            "pattern": "isr_chain"
        }))
        .unwrap_or_default();

        let mut strike = Capability::new(
            "strike1".to_string(),
            "Precision Munition".to_string(),
            CapabilityType::Payload,
            0.95,
        );
        strike.metadata_json =
            serde_json::to_string(&json!({"strike_capable": true})).unwrap_or_default();

        let bda_sensor = Capability::new(
            "bda1".to_string(),
            "BDA Camera".to_string(),
            CapabilityType::Sensor,
            0.85,
        );

        let caps = vec![isr, strike, bda_sensor];
        let context = CompositionContext::new(vec!["node1".to_string(), "node2".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Strike Chain");
        assert_eq!(composed.confidence, 0.85); // Min of all components
        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert!(metadata["requires_human_approval"].as_bool().unwrap());
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

    #[tokio::test]
    async fn test_authorization_coverage_with_commander() {
        use crate::models::{
            AuthorityLevel, HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt,
            Operator, OperatorExt, OperatorRank,
        };

        let rule = AuthorizationCoverageRule::default();

        // Create communication capability
        let comms = Capability::new(
            "radio1".to_string(),
            "Tactical Radio".to_string(),
            CapabilityType::Communication,
            0.9,
        );

        let caps = vec![comms];

        // Create a node with a Commander-level operator
        let operator = Operator::new(
            "op1".to_string(),
            "CPT Smith".to_string(),
            OperatorRank::O3,
            AuthorityLevel::Commander,
            "11A".to_string(),
        );

        let binding = HumanMachinePair::one_to_one(operator, "node1".to_string());
        let config = NodeConfig::with_operator("Command Post".to_string(), binding);

        let context =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        assert!(rule.applies_to(&caps));
        assert!(context.has_commander());
        assert_eq!(context.authorization_bonus(), 4); // Commander = 0.8 * 5 = 4

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Authorization Coverage");

        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert!(metadata["can_authorize_strike"].as_bool().unwrap());
        assert_eq!(metadata["authorization_bonus"].as_i64().unwrap(), 4);
    }

    #[tokio::test]
    async fn test_authorization_coverage_without_operator() {
        let rule = AuthorizationCoverageRule::default();

        let comms = Capability::new(
            "radio1".to_string(),
            "Autonomous Radio".to_string(),
            CapabilityType::Communication,
            0.9,
        );

        let caps = vec![comms];

        // No operator binding
        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&caps));
        assert!(!context.has_commander());
        assert_eq!(context.authorization_bonus(), 0);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(!result.has_compositions()); // No authorization without human
    }

    #[tokio::test]
    async fn test_authorization_coverage_supervisor_level() {
        use crate::models::{
            AuthorityLevel, HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt,
            Operator, OperatorExt, OperatorRank,
        };

        // Use supervisor-level rule
        let rule = AuthorizationCoverageRule::supervisor_required();

        let comms = Capability::new(
            "radio1".to_string(),
            "Radio".to_string(),
            CapabilityType::Communication,
            0.85,
        );

        let caps = vec![comms];

        // Create a node with Supervisor authority (not Commander)
        let operator = Operator::new(
            "op1".to_string(),
            "SGT Jones".to_string(),
            OperatorRank::E5,
            AuthorityLevel::Supervisor,
            "11B".to_string(),
        );

        let binding = HumanMachinePair::one_to_one(operator, "node1".to_string());
        let config = NodeConfig::with_operator("Control Station".to_string(), binding);

        let context =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        // Supervisor level rule should find coverage
        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        // Supervisor can't authorize strikes
        assert!(!metadata["can_authorize_strike"].as_bool().unwrap());
        // Supervisor = 0.5 * 5 = 2.5 rounds to 2 or 3
        assert!(metadata["authorization_bonus"].as_i64().unwrap() >= 2);
    }

    #[tokio::test]
    async fn test_authorization_coverage_insufficient_authority() {
        use crate::models::{
            AuthorityLevel, HumanMachinePair, HumanMachinePairExt, NodeConfig, NodeConfigExt,
            Operator, OperatorExt, OperatorRank,
        };

        // Commander-level rule (default)
        let rule = AuthorizationCoverageRule::default();

        let comms = Capability::new(
            "radio1".to_string(),
            "Radio".to_string(),
            CapabilityType::Communication,
            0.9,
        );

        let caps = vec![comms];

        // Only Advisor authority - not sufficient for Commander requirement
        let operator = Operator::new(
            "op1".to_string(),
            "SPC Brown".to_string(),
            OperatorRank::E4,
            AuthorityLevel::Advisor,
            "11B".to_string(),
        );

        let binding = HumanMachinePair::one_to_one(operator, "node1".to_string());
        let config = NodeConfig::with_operator("Observation Post".to_string(), binding);

        let context =
            CompositionContext::new(vec!["node1".to_string()]).with_node_configs(vec![config]);

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(!result.has_compositions()); // Advisor can't satisfy Commander requirement
    }
}
