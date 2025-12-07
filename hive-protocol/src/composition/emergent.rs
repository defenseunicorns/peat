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

/// Rule for detecting multi-domain coverage capability
///
/// Requires: Sensors or capabilities that cover multiple domains (Air, Surface, Subsurface)
/// Emergent capability: Multi-domain battlespace awareness
pub struct MultiDomainCoverageRule {
    /// Minimum number of domains required for the rule to apply
    min_domains: usize,
    /// Minimum confidence threshold for sensors
    min_confidence: f32,
}

impl MultiDomainCoverageRule {
    /// Create a new multi-domain coverage rule
    pub fn new(min_domains: usize, min_confidence: f32) -> Self {
        Self {
            min_domains: min_domains.max(2), // At least 2 domains for "multi"
            min_confidence,
        }
    }

    /// Create rule requiring all three domains
    pub fn full_spectrum() -> Self {
        Self::new(3, 0.7)
    }

    /// Create rule requiring any two domains
    pub fn dual_domain() -> Self {
        Self::new(2, 0.7)
    }

    /// Extract sensor type and infer domains
    fn get_sensor_domains(cap: &Capability) -> crate::models::DomainSet {
        use crate::models::{DomainSet, SensorType};

        if cap.get_capability_type() != CapabilityType::Sensor {
            return DomainSet::empty();
        }

        // Try to get sensor type from metadata
        let sensor_type = serde_json::from_str::<serde_json::Value>(&cap.metadata_json)
            .ok()
            .and_then(|v| {
                v.get("sensor_type").and_then(|s| s.as_str()).and_then(|s| {
                    match s.to_lowercase().as_str() {
                        "electro_optical" | "eo" | "camera" => Some(SensorType::ElectroOptical),
                        "infrared" | "ir" | "thermal" => Some(SensorType::Infrared),
                        "radar" | "rad" => Some(SensorType::Radar),
                        "sonar" | "son" => Some(SensorType::Sonar),
                        "acoustic" | "aco" => Some(SensorType::Acoustic),
                        "sigint" | "sig" | "signals_intelligence" => Some(SensorType::Sigint),
                        "mad" | "magnetic" => Some(SensorType::Mad),
                        _ => None,
                    }
                })
            });

        if let Some(st) = sensor_type {
            st.detection_domains()
        } else {
            // If sensor type not specified, check for explicit domains
            serde_json::from_str::<serde_json::Value>(&cap.metadata_json)
                .ok()
                .and_then(|v| {
                    v.get("detection_domains").and_then(|domains| {
                        if let Some(arr) = domains.as_array() {
                            let mut set = DomainSet::empty();
                            for d in arr {
                                if let Some(s) = d.as_str() {
                                    if let Some(domain) = crate::models::Domain::parse(s) {
                                        set.add(domain);
                                    }
                                }
                            }
                            Some(set)
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| {
                    // Default: assume surface + air for unknown sensors
                    DomainSet::from_domains(&[
                        crate::models::Domain::Surface,
                        crate::models::Domain::Air,
                    ])
                })
        }
    }
}

impl Default for MultiDomainCoverageRule {
    fn default() -> Self {
        Self::dual_domain()
    }
}

#[async_trait]
impl CompositionRule for MultiDomainCoverageRule {
    fn name(&self) -> &str {
        "multi_domain_coverage"
    }

    fn description(&self) -> &str {
        "Detects multi-domain coverage from sensors spanning air, surface, and/or subsurface"
    }

    fn applies_to(&self, capabilities: &[Capability]) -> bool {
        use crate::models::DomainSet;

        // Aggregate domains from all capabilities
        let mut covered = DomainSet::empty();

        for cap in capabilities {
            if cap.confidence < self.min_confidence {
                continue;
            }

            // Get domains this capability covers
            let domains = Self::get_sensor_domains(cap);
            covered = covered.union(&domains);
        }

        covered.count() >= self.min_domains
    }

    async fn compose(
        &self,
        capabilities: &[Capability],
        _context: &CompositionContext,
    ) -> Result<CompositionResult> {
        use crate::models::{Domain, DomainSet};

        let mut covered = DomainSet::empty();
        let mut contributors: Vec<String> = Vec::new();
        let mut domain_sensors: std::collections::HashMap<Domain, Vec<String>> =
            std::collections::HashMap::new();
        let mut min_confidence = 1.0f32;

        for cap in capabilities {
            if cap.confidence < self.min_confidence {
                continue;
            }

            let domains = Self::get_sensor_domains(cap);

            if !domains.is_empty() {
                contributors.push(cap.id.clone());
                min_confidence = min_confidence.min(cap.confidence);

                for domain in domains.iter() {
                    covered.add(domain);
                    domain_sensors
                        .entry(domain)
                        .or_default()
                        .push(cap.id.clone());
                }
            }
        }

        if covered.count() < self.min_domains {
            return Ok(CompositionResult::new(vec![], 0.0));
        }

        // Calculate composition bonus based on coverage
        let coverage_bonus = match covered.count() {
            3 => 3, // Full spectrum
            2 => 2, // Dual domain
            _ => 1, // Single domain (shouldn't happen but safe)
        };

        // Confidence is minimum of all contributors, boosted slightly by coverage
        let coverage_confidence = (min_confidence + (coverage_bonus as f32 * 0.05)).min(1.0);

        let coverage_name = match covered.count() {
            3 => "Full Spectrum Coverage",
            2 => "Dual-Domain Coverage",
            _ => "Domain Coverage",
        };

        let mut composed = Capability::new(
            format!("emergent_multi_domain_{}", uuid::Uuid::new_v4()),
            coverage_name.to_string(),
            CapabilityType::Emergent,
            coverage_confidence,
        );

        // Build domain coverage map for metadata
        let domain_coverage: serde_json::Map<String, serde_json::Value> = domain_sensors
            .iter()
            .map(|(domain, sensors)| {
                (
                    domain.name().to_lowercase(),
                    serde_json::Value::Array(
                        sensors
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                )
            })
            .collect();

        composed.metadata_json = serde_json::to_string(&json!({
            "composition_type": "emergent",
            "pattern": "multi_domain_coverage",
            "domains_covered": covered.to_vec().iter().map(|d| d.name()).collect::<Vec<_>>(),
            "domain_count": covered.count(),
            "coverage_bonus": coverage_bonus,
            "domain_sensors": domain_coverage,
            "is_full_spectrum": covered.count() == 3,
            "can_detect_subsurface": covered.contains(Domain::Subsurface),
            "description": format!("Multi-domain awareness across {} domains", covered.count())
        }))
        .unwrap_or_default();

        Ok(CompositionResult::new(vec![composed], coverage_confidence)
            .with_contributors(contributors))
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

    // MultiDomainCoverageRule tests

    #[tokio::test]
    async fn test_multi_domain_dual_domain_coverage() {
        let rule = MultiDomainCoverageRule::default(); // dual domain

        // Radar (air+surface) + Sonar (subsurface+surface)
        let mut radar = Capability::new(
            "radar1".to_string(),
            "Search Radar".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        radar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "radar"
        }))
        .unwrap();

        let mut sonar = Capability::new(
            "sonar1".to_string(),
            "Hull Sonar".to_string(),
            CapabilityType::Sensor,
            0.85,
        );
        sonar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "sonar"
        }))
        .unwrap();

        let caps = vec![radar, sonar];
        let context = CompositionContext::new(vec!["ship1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert!(composed.name.contains("Coverage"));

        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert!(metadata["domain_count"].as_i64().unwrap() >= 2);
        assert!(metadata["can_detect_subsurface"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_multi_domain_full_spectrum() {
        let rule = MultiDomainCoverageRule::full_spectrum();

        // Need sensors covering all three domains
        let mut radar = Capability::new(
            "radar1".to_string(),
            "Air Search Radar".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        radar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "radar"
        }))
        .unwrap();

        let mut sonar = Capability::new(
            "sonar1".to_string(),
            "Sonar".to_string(),
            CapabilityType::Sensor,
            0.85,
        );
        sonar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "sonar"
        }))
        .unwrap();

        // Together radar (air+surface) + sonar (subsurface+surface) = all 3 domains
        let caps = vec![radar, sonar];
        let context = CompositionContext::new(vec!["ship1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Full Spectrum Coverage");

        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert!(metadata["is_full_spectrum"].as_bool().unwrap());
        assert_eq!(metadata["domain_count"].as_i64().unwrap(), 3);
        assert_eq!(metadata["coverage_bonus"].as_i64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_multi_domain_insufficient_coverage() {
        let rule = MultiDomainCoverageRule::full_spectrum();

        // Only one sensor type = not full spectrum
        let mut radar = Capability::new(
            "radar1".to_string(),
            "Radar".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        radar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "radar"
        }))
        .unwrap();

        let caps = vec![radar];
        let _context = CompositionContext::new(vec!["node1".to_string()]);

        // Radar only covers air+surface, not subsurface
        assert!(!rule.applies_to(&caps));
    }

    #[tokio::test]
    async fn test_multi_domain_low_confidence_filtered() {
        let rule = MultiDomainCoverageRule::new(2, 0.8); // min 0.8 confidence

        let mut radar = Capability::new(
            "radar1".to_string(),
            "Radar".to_string(),
            CapabilityType::Sensor,
            0.9, // Good
        );
        radar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "radar"
        }))
        .unwrap();

        let mut sonar = Capability::new(
            "sonar1".to_string(),
            "Sonar".to_string(),
            CapabilityType::Sensor,
            0.5, // Too low - should be filtered
        );
        sonar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "sonar"
        }))
        .unwrap();

        let caps = vec![radar, sonar];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        // Sonar filtered due to low confidence, so only 2 domains from radar
        assert!(rule.applies_to(&caps)); // radar still covers air+surface = 2 domains

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        // Only radar should be a contributor
        assert_eq!(result.contributing_capabilities.len(), 1);
    }

    #[tokio::test]
    async fn test_multi_domain_explicit_domains_in_metadata() {
        let rule = MultiDomainCoverageRule::default();

        // Sensor with explicit domain specification
        let mut custom_sensor = Capability::new(
            "custom1".to_string(),
            "Custom Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        custom_sensor.metadata_json = serde_json::to_string(&json!({
            "detection_domains": ["subsurface", "surface", "air"]
        }))
        .unwrap();

        let caps = vec![custom_sensor];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        // Single sensor covering all domains
        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        assert_eq!(metadata["domain_count"].as_i64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_multi_domain_acoustic_covers_all() {
        let rule = MultiDomainCoverageRule::full_spectrum();

        // Acoustic sensor covers all domains
        let mut acoustic = Capability::new(
            "acoustic1".to_string(),
            "Acoustic Array".to_string(),
            CapabilityType::Sensor,
            0.85,
        );
        acoustic.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "acoustic"
        }))
        .unwrap();

        let caps = vec![acoustic];
        let context = CompositionContext::new(vec!["node1".to_string()]);

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        assert_eq!(composed.name, "Full Spectrum Coverage");
    }

    #[tokio::test]
    async fn test_multi_domain_non_sensors_ignored() {
        let rule = MultiDomainCoverageRule::default();

        // Non-sensor capabilities should be ignored
        let compute = Capability::new(
            "compute1".to_string(),
            "Compute".to_string(),
            CapabilityType::Compute,
            0.9,
        );

        let comms = Capability::new(
            "comms1".to_string(),
            "Radio".to_string(),
            CapabilityType::Communication,
            0.9,
        );

        let caps = vec![compute, comms];
        let _context = CompositionContext::new(vec!["node1".to_string()]);

        // No sensors = no domain coverage
        assert!(!rule.applies_to(&caps));
    }

    #[tokio::test]
    async fn test_multi_domain_mad_for_asw() {
        let rule = MultiDomainCoverageRule::default();

        // MAD operates from air/surface but detects subsurface
        let mut mad = Capability::new(
            "mad1".to_string(),
            "MAD Boom".to_string(),
            CapabilityType::Sensor,
            0.8,
        );
        mad.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "mad"
        }))
        .unwrap();

        let mut radar = Capability::new(
            "radar1".to_string(),
            "Surface Radar".to_string(),
            CapabilityType::Sensor,
            0.9,
        );
        radar.metadata_json = serde_json::to_string(&json!({
            "sensor_type": "radar"
        }))
        .unwrap();

        let caps = vec![mad, radar];
        let context = CompositionContext::new(vec!["p3c".to_string()]); // ASW aircraft

        assert!(rule.applies_to(&caps));

        let result = rule.compose(&caps, &context).await.unwrap();
        assert!(result.has_compositions());

        let composed = &result.composed_capabilities[0];
        let metadata: Value = serde_json::from_str(&composed.metadata_json).unwrap();
        // MAD detects subsurface, radar detects air+surface = 3 domains
        assert!(metadata["can_detect_subsurface"].as_bool().unwrap());
    }
}
