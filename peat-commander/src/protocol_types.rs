//! Protocol type mappings for PEAT Commander
//!
//! Maps game types (PieceType, DetectionMode) to peat-protocol types
//! (NodeConfig, Capability, Operator).

use peat_protocol::models::{
    AuthorityLevel, Capability, CapabilityExt, CapabilityType, Domain, DomainSet, HumanMachinePair,
    HumanMachinePairExt, NodeConfig, NodeConfigExt, Operator, OperatorExt, OperatorRank,
    SensorType,
};
use serde_json::json;

use crate::{DetectionMode, Piece, PieceType};

/// Extension trait for pieces with domain information
pub trait PieceDomain {
    /// Get the primary operating domain for this piece
    fn domain(&self) -> Domain;

    /// Get all domains this piece's sensors can detect
    fn detection_domains(&self) -> DomainSet;
}

/// Extension trait to convert game types to protocol types
pub trait PieceToProtocol {
    /// Convert a game Piece to a protocol NodeConfig
    fn to_node_config(&self) -> NodeConfig;

    /// Get the capabilities this piece provides
    fn to_capabilities(&self) -> Vec<Capability>;
}

impl PieceToProtocol for Piece {
    fn to_node_config(&self) -> NodeConfig {
        let platform_type = match self.piece_type {
            PieceType::Sensor(mode) => format!("sensor_{}", mode.name().to_lowercase()),
            PieceType::Scout => "scout".to_string(),
            PieceType::Striker => "striker".to_string(),
            PieceType::Support => "support".to_string(),
            PieceType::Authority => "command_post".to_string(),
            PieceType::Analyst => "analyst".to_string(),
        };

        let mut config = NodeConfig::new(platform_type);
        config.id = format!("piece_{}", self.id);

        // Add capabilities
        for cap in self.to_capabilities() {
            config.add_capability(cap);
        }

        // Authority pieces get an operator binding
        if matches!(self.piece_type, PieceType::Authority) {
            let operator = Operator::new(
                format!("operator_{}", self.id),
                "Commander".to_string(),
                OperatorRank::O3, // Captain
                AuthorityLevel::Commander,
                "11A".to_string(), // Infantry Officer
            );

            let binding = HumanMachinePair::one_to_one(operator, config.id.clone());
            config.set_operator_binding(Some(binding));
        }

        config
    }

    fn to_capabilities(&self) -> Vec<Capability> {
        match self.piece_type {
            PieceType::Sensor(mode) => {
                let sensor_type = mode.to_sensor_type();
                let detection_domains = sensor_type.detection_domains();
                let mut cap = Capability::new(
                    format!("sensor_{}_{}", self.id, mode.name().to_lowercase()),
                    format!("{} Sensor", mode.name()),
                    CapabilityType::Sensor,
                    0.9, // High confidence
                );
                cap.metadata_json = serde_json::to_string(&json!({
                    "sensor_type": mode.sensor_type(),
                    "range": mode.range(),
                    "detection_mode": mode.name(),
                    "detection_domains": detection_domains.to_vec().iter().map(|d| d.name().to_lowercase()).collect::<Vec<_>>()
                }))
                .unwrap_or_default();
                vec![cap]
            }
            PieceType::Scout => {
                let sensor = Capability::new(
                    format!("scout_sensor_{}", self.id),
                    "Scout Sensor".to_string(),
                    CapabilityType::Sensor,
                    0.7,
                );
                let mobility = Capability::new(
                    format!("scout_mobility_{}", self.id),
                    "High Mobility".to_string(),
                    CapabilityType::Mobility,
                    0.9,
                );
                vec![sensor, mobility]
            }
            PieceType::Striker => {
                let mut payload = Capability::new(
                    format!("striker_payload_{}", self.id),
                    "Strike Package".to_string(),
                    CapabilityType::Payload,
                    0.9,
                );
                payload.metadata_json = serde_json::to_string(&json!({
                    "strike_capable": true,
                    "munition_type": "precision"
                }))
                .unwrap_or_default();
                vec![payload]
            }
            PieceType::Support => {
                let comms = Capability::new(
                    format!("support_comms_{}", self.id),
                    "Tactical Radio".to_string(),
                    CapabilityType::Communication,
                    0.9,
                );
                vec![comms]
            }
            PieceType::Authority => {
                // Authority has communication (for relaying commands)
                let comms = Capability::new(
                    format!("authority_comms_{}", self.id),
                    "Command Radio".to_string(),
                    CapabilityType::Communication,
                    0.95,
                );
                // And compute (for decision-making support)
                let compute = Capability::new(
                    format!("authority_compute_{}", self.id),
                    "Command System".to_string(),
                    CapabilityType::Compute,
                    0.8,
                );
                vec![comms, compute]
            }
            PieceType::Analyst => {
                // Analyst has AI/ML capabilities: CLASSIFY, PREDICT, FUSE
                let mut classify = Capability::new(
                    format!("analyst_classify_{}", self.id),
                    "AI Classification".to_string(),
                    CapabilityType::Compute,
                    0.85,
                );
                classify.metadata_json = serde_json::to_string(&json!({
                    "ai_capability": "classify",
                    "model_type": "target_recognition"
                }))
                .unwrap_or_default();

                let mut predict = Capability::new(
                    format!("analyst_predict_{}", self.id),
                    "Predictive Analytics".to_string(),
                    CapabilityType::Compute,
                    0.75,
                );
                predict.metadata_json = serde_json::to_string(&json!({
                    "ai_capability": "predict",
                    "model_type": "trajectory_prediction"
                }))
                .unwrap_or_default();

                let mut fuse = Capability::new(
                    format!("analyst_fuse_{}", self.id),
                    "Data Fusion".to_string(),
                    CapabilityType::Compute,
                    0.9,
                );
                fuse.metadata_json = serde_json::to_string(&json!({
                    "ai_capability": "fuse",
                    "model_type": "multi_source_fusion"
                }))
                .unwrap_or_default();

                vec![classify, predict, fuse]
            }
        }
    }
}

impl PieceDomain for Piece {
    fn domain(&self) -> Domain {
        match self.piece_type {
            // Sensors operate based on detection mode
            PieceType::Sensor(mode) => match mode {
                DetectionMode::Acoustic => Domain::Subsurface, // Primarily underwater
                _ => Domain::Air,                              // Most sensors are airborne
            },
            PieceType::Scout => Domain::Air,   // Drones are airborne
            PieceType::Striker => Domain::Air, // Strike drones are airborne
            PieceType::Support => Domain::Surface, // Support is ground-based
            PieceType::Authority => Domain::Surface, // Command posts are ground-based
            PieceType::Analyst => Domain::Surface, // Analysts are ground-based (processing centers)
        }
    }

    fn detection_domains(&self) -> DomainSet {
        match self.piece_type {
            PieceType::Sensor(mode) => mode.to_sensor_type().detection_domains(),
            PieceType::Scout => {
                // Scouts have basic EO sensors
                SensorType::ElectroOptical.detection_domains()
            }
            PieceType::Striker => DomainSet::empty(), // Strikers don't detect
            PieceType::Support => DomainSet::empty(), // Support doesn't detect
            PieceType::Authority => DomainSet::empty(), // Authority doesn't detect
            PieceType::Analyst => DomainSet::empty(), // Analysts process, don't detect directly
        }
    }
}

/// Convert detection mode to sensor type string
impl DetectionMode {
    pub fn sensor_type(self) -> &'static str {
        match self {
            DetectionMode::EO => "electro_optical",
            DetectionMode::IR => "infrared",
            DetectionMode::Radar => "radar",
            DetectionMode::Acoustic => "acoustic",
            DetectionMode::SIGINT => "signals_intelligence",
        }
    }

    /// Convert to protocol SensorType
    pub fn to_sensor_type(self) -> SensorType {
        match self {
            DetectionMode::EO => SensorType::ElectroOptical,
            DetectionMode::IR => SensorType::Infrared,
            DetectionMode::Radar => SensorType::Radar,
            DetectionMode::Acoustic => SensorType::Acoustic,
            DetectionMode::SIGINT => SensorType::Sigint,
        }
    }
}

/// Calculate bonuses from protocol capabilities
///
/// This replaces the hardcoded bonus calculation with protocol-based composition
pub struct ProtocolBonusCalculator;

impl ProtocolBonusCalculator {
    /// Calculate detect bonus from capabilities
    pub fn detect_bonus(capabilities: &[Capability]) -> i32 {
        let sensor_count = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Sensor as i32)
            .count();

        // Base bonus for each sensor
        let base = (sensor_count * 3) as i32;

        // Bonus for radar sensors
        let radar_bonus = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Sensor as i32)
            .filter(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("detection_mode")
                            .and_then(|d| d.as_str())
                            .map(|s| s == "RAD")
                    })
                    .unwrap_or(false)
            })
            .count() as i32;

        base + radar_bonus
    }

    /// Calculate track bonus from capabilities
    pub fn track_bonus(capabilities: &[Capability]) -> i32 {
        // Tracking requires sensors
        let sensor_count = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Sensor as i32)
            .count();

        // 2 points per sensor (tracking is harder than detection)
        (sensor_count * 2) as i32
    }

    /// Calculate strike bonus from capabilities
    pub fn strike_bonus(capabilities: &[Capability]) -> i32 {
        let payload_count = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Payload as i32)
            .filter(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| v.get("strike_capable").and_then(|s| s.as_bool()))
                    .unwrap_or(false)
            })
            .count();

        (payload_count * 3) as i32
    }

    /// Calculate recon bonus from capabilities
    pub fn recon_bonus(capabilities: &[Capability]) -> i32 {
        let has_sensor = capabilities
            .iter()
            .any(|c| c.capability_type == CapabilityType::Sensor as i32);
        let has_mobility = capabilities
            .iter()
            .any(|c| c.capability_type == CapabilityType::Mobility as i32);

        if has_sensor && has_mobility {
            // Scout-like capability
            3
        } else if has_sensor {
            1
        } else {
            0
        }
    }

    /// Calculate relay bonus from capabilities
    pub fn relay_bonus(capabilities: &[Capability]) -> i32 {
        let comms_count = capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Communication as i32)
            .count();

        (comms_count * 3) as i32
    }

    /// Calculate authorize bonus from node configs (requires operator bindings)
    pub fn authorize_bonus(configs: &[NodeConfig]) -> i32 {
        use peat_protocol::composition::CompositionContext;

        let context = CompositionContext::new(vec![]).with_node_configs(configs.to_vec());

        context.authorization_bonus()
    }

    /// Calculate classify bonus from capabilities (AI target classification)
    pub fn classify_bonus(capabilities: &[Capability]) -> i32 {
        capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Compute as i32)
            .filter(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("ai_capability")
                            .and_then(|a| a.as_str())
                            .map(|s| s == "classify")
                    })
                    .unwrap_or(false)
            })
            .count() as i32
            * 3
    }

    /// Calculate predict bonus from capabilities (AI prediction/trajectory)
    pub fn predict_bonus(capabilities: &[Capability]) -> i32 {
        capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Compute as i32)
            .filter(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("ai_capability")
                            .and_then(|a| a.as_str())
                            .map(|s| s == "predict")
                    })
                    .unwrap_or(false)
            })
            .count() as i32
            * 2
    }

    /// Calculate fuse bonus from capabilities (multi-source data fusion)
    pub fn fuse_bonus(capabilities: &[Capability]) -> i32 {
        capabilities
            .iter()
            .filter(|c| c.capability_type == CapabilityType::Compute as i32)
            .filter(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("ai_capability")
                            .and_then(|a| a.as_str())
                            .map(|s| s == "fuse")
                    })
                    .unwrap_or(false)
            })
            .count() as i32
            * 3
    }
}

/// Emergent capability detection using the composition engine
pub struct EmergentCapabilityDetector;

/// Types of emergent capabilities that can be detected
#[derive(Debug, Clone, PartialEq)]
pub enum EmergentCapabilityType {
    /// ISR chain: Sensor + Compute + Communication
    IsrChain { confidence: f32 },
    /// Strike chain: ISR + Payload + BDA sensor (requires human approval)
    StrikeChain { confidence: f32 },
    /// Authorization coverage: Communication + Operator with authority
    AuthorizationCoverage { bonus: i32 },
    /// Multi-domain coverage: Sensors covering multiple domains (air/surface/subsurface)
    MultiDomainCoverage { domain_count: usize, bonus: i32 },
}

impl EmergentCapabilityDetector {
    /// Detect emergent capabilities from a set of protocol capabilities
    pub async fn detect(
        capabilities: &[Capability],
        node_configs: &[NodeConfig],
    ) -> Vec<EmergentCapabilityType> {
        use peat_protocol::composition::{
            emergent::{
                AuthorizationCoverageRule, IsrChainRule, MultiDomainCoverageRule, StrikeChainRule,
            },
            CompositionContext, CompositionRule,
        };

        let mut detected = Vec::new();
        let node_ids: Vec<String> = node_configs.iter().map(|c| c.id.clone()).collect();
        let context = CompositionContext::new(node_ids).with_node_configs(node_configs.to_vec());

        // Check for ISR Chain
        let isr_rule = IsrChainRule::default();
        if isr_rule.applies_to(capabilities) {
            if let Ok(result) = isr_rule.compose(capabilities, &context).await {
                if result.has_compositions() {
                    detected.push(EmergentCapabilityType::IsrChain {
                        confidence: result.confidence,
                    });
                }
            }
        }

        // Check for Strike Chain
        let strike_rule = StrikeChainRule::default();
        if strike_rule.applies_to(capabilities) {
            if let Ok(result) = strike_rule.compose(capabilities, &context).await {
                if result.has_compositions() {
                    detected.push(EmergentCapabilityType::StrikeChain {
                        confidence: result.confidence,
                    });
                }
            }
        }

        // Check for Authorization Coverage
        let auth_rule = AuthorizationCoverageRule::default();
        if auth_rule.applies_to(capabilities) {
            if let Ok(result) = auth_rule.compose(capabilities, &context).await {
                if result.has_compositions() {
                    detected.push(EmergentCapabilityType::AuthorizationCoverage {
                        bonus: context.authorization_bonus(),
                    });
                }
            }
        }

        // Check for Multi-Domain Coverage (dual domain first, then full spectrum)
        let dual_domain_rule = MultiDomainCoverageRule::dual_domain();
        if dual_domain_rule.applies_to(capabilities) {
            if let Ok(result) = dual_domain_rule.compose(capabilities, &context).await {
                if result.has_compositions() {
                    // Check if we have full spectrum coverage
                    let full_spectrum_rule = MultiDomainCoverageRule::full_spectrum();
                    let domain_count = if let Ok(full_result) =
                        full_spectrum_rule.compose(capabilities, &context).await
                    {
                        if full_result.has_compositions() {
                            3
                        } else {
                            2
                        }
                    } else {
                        2
                    };

                    // Bonus: +2 per domain covered
                    let bonus = (domain_count * 2) as i32;
                    detected.push(EmergentCapabilityType::MultiDomainCoverage {
                        domain_count,
                        bonus,
                    });
                }
            }
        }

        detected
    }

    /// Synchronous wrapper for detect (blocks on async)
    pub fn detect_sync(
        capabilities: &[Capability],
        node_configs: &[NodeConfig],
    ) -> Vec<EmergentCapabilityType> {
        // Use a simple blocking approach since we're in a game loop
        futures::executor::block_on(Self::detect(capabilities, node_configs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Team;

    #[test]
    fn test_sensor_piece_to_node_config() {
        let piece = Piece {
            id: 1,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 5,
            y: 5,
            fuel: 100,
            max_fuel: 100,
        };

        let config = piece.to_node_config();
        assert_eq!(config.platform_type, "sensor_rad");
        assert!(!config.capabilities.is_empty());
        assert!(!config.is_human_operated());
    }

    #[test]
    fn test_authority_piece_has_operator() {
        let piece = Piece {
            id: 2,
            piece_type: PieceType::Authority,
            team: Team::Blue,
            x: 3,
            y: 3,
            fuel: 100,
            max_fuel: 100,
        };

        let config = piece.to_node_config();
        assert_eq!(config.platform_type, "command_post");
        assert!(config.is_human_operated());

        // Should have Commander authority
        let primary_op = config.get_primary_operator().unwrap();
        assert_eq!(primary_op.authority_level, AuthorityLevel::Commander as i32);
    }

    #[test]
    fn test_striker_piece_to_capabilities() {
        let piece = Piece {
            id: 3,
            piece_type: PieceType::Striker,
            team: Team::Blue,
            x: 4,
            y: 4,
            fuel: 80,
            max_fuel: 100,
        };

        let caps = piece.to_capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].capability_type, CapabilityType::Payload as i32);

        // Check metadata
        let metadata: serde_json::Value = serde_json::from_str(&caps[0].metadata_json).unwrap();
        assert!(metadata["strike_capable"].as_bool().unwrap());
    }

    #[test]
    fn test_scout_piece_has_sensor_and_mobility() {
        let piece = Piece {
            id: 4,
            piece_type: PieceType::Scout,
            team: Team::Blue,
            x: 6,
            y: 6,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = piece.to_capabilities();
        assert_eq!(caps.len(), 2);

        let has_sensor = caps
            .iter()
            .any(|c| c.capability_type == CapabilityType::Sensor as i32);
        let has_mobility = caps
            .iter()
            .any(|c| c.capability_type == CapabilityType::Mobility as i32);

        assert!(has_sensor);
        assert!(has_mobility);
    }

    #[test]
    fn test_authorize_bonus_with_commander() {
        let piece = Piece {
            id: 5,
            piece_type: PieceType::Authority,
            team: Team::Blue,
            x: 2,
            y: 2,
            fuel: 100,
            max_fuel: 100,
        };

        let config = piece.to_node_config();
        let bonus = ProtocolBonusCalculator::authorize_bonus(&[config]);

        // Commander = 0.8 * 5 = 4
        assert_eq!(bonus, 4);
    }

    #[test]
    fn test_authorize_bonus_without_operator() {
        let piece = Piece {
            id: 6,
            piece_type: PieceType::Striker,
            team: Team::Blue,
            x: 2,
            y: 2,
            fuel: 100,
            max_fuel: 100,
        };

        let config = piece.to_node_config();
        let bonus = ProtocolBonusCalculator::authorize_bonus(&[config]);

        // No operator = 0
        assert_eq!(bonus, 0);
    }

    #[test]
    fn test_detect_bonus_calculation() {
        let sensor_piece = Piece {
            id: 7,
            piece_type: PieceType::Sensor(DetectionMode::EO),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = sensor_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::detect_bonus(&caps);

        // One sensor = 3
        assert_eq!(bonus, 3);
    }

    #[test]
    fn test_recon_bonus_with_scout() {
        let scout_piece = Piece {
            id: 8,
            piece_type: PieceType::Scout,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = scout_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::recon_bonus(&caps);

        // Scout has sensor + mobility = 3
        assert_eq!(bonus, 3);
    }

    #[test]
    fn test_strike_bonus_calculation() {
        let striker_piece = Piece {
            id: 9,
            piece_type: PieceType::Striker,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = striker_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::strike_bonus(&caps);

        // One striker = 3
        assert_eq!(bonus, 3);
    }

    #[test]
    fn test_piece_domain_sensor() {
        let radar_piece = Piece {
            id: 10,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let acoustic_piece = Piece {
            id: 11,
            piece_type: PieceType::Sensor(DetectionMode::Acoustic),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        // Radar is airborne
        assert_eq!(radar_piece.domain(), Domain::Air);
        // Acoustic is subsurface
        assert_eq!(acoustic_piece.domain(), Domain::Subsurface);
    }

    #[test]
    fn test_piece_detection_domains() {
        let radar_piece = Piece {
            id: 12,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let acoustic_piece = Piece {
            id: 13,
            piece_type: PieceType::Sensor(DetectionMode::Acoustic),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        // Radar detects air and surface
        let radar_domains = radar_piece.detection_domains();
        assert!(radar_domains.contains(Domain::Air));
        assert!(radar_domains.contains(Domain::Surface));
        assert!(!radar_domains.contains(Domain::Subsurface));

        // Acoustic detects all domains (sound propagates everywhere)
        let acoustic_domains = acoustic_piece.detection_domains();
        assert!(acoustic_domains.contains(Domain::Subsurface));
        assert!(acoustic_domains.contains(Domain::Surface));
        assert!(acoustic_domains.contains(Domain::Air));
    }

    #[test]
    fn test_sensor_capability_has_detection_domains() {
        let radar_piece = Piece {
            id: 14,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = radar_piece.to_capabilities();
        assert_eq!(caps.len(), 1);

        let metadata: serde_json::Value = serde_json::from_str(&caps[0].metadata_json).unwrap();
        let detection_domains = metadata["detection_domains"].as_array().unwrap();

        // Radar should have air and surface
        assert!(detection_domains.iter().any(|d| d.as_str() == Some("air")));
        assert!(detection_domains
            .iter()
            .any(|d| d.as_str() == Some("surface")));
    }

    #[test]
    fn test_detection_mode_to_sensor_type() {
        assert_eq!(
            DetectionMode::EO.to_sensor_type(),
            SensorType::ElectroOptical
        );
        assert_eq!(DetectionMode::IR.to_sensor_type(), SensorType::Infrared);
        assert_eq!(DetectionMode::Radar.to_sensor_type(), SensorType::Radar);
        assert_eq!(
            DetectionMode::Acoustic.to_sensor_type(),
            SensorType::Acoustic
        );
        assert_eq!(DetectionMode::SIGINT.to_sensor_type(), SensorType::Sigint);
    }

    #[tokio::test]
    async fn test_multi_domain_coverage_detection() {
        // Create sensors that cover multiple domains
        let radar_piece = Piece {
            id: 15,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let acoustic_piece = Piece {
            id: 16,
            piece_type: PieceType::Sensor(DetectionMode::Acoustic),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let configs = vec![
            radar_piece.to_node_config(),
            acoustic_piece.to_node_config(),
        ];
        let mut capabilities = Vec::new();
        capabilities.extend(radar_piece.to_capabilities());
        capabilities.extend(acoustic_piece.to_capabilities());

        let detected = EmergentCapabilityDetector::detect(&capabilities, &configs).await;

        // Should detect multi-domain coverage (radar: air+surface, acoustic: subsurface = 3 domains)
        let multi_domain = detected
            .iter()
            .find(|e| matches!(e, EmergentCapabilityType::MultiDomainCoverage { .. }));

        assert!(
            multi_domain.is_some(),
            "Should detect multi-domain coverage"
        );
        if let Some(EmergentCapabilityType::MultiDomainCoverage {
            domain_count,
            bonus,
        }) = multi_domain
        {
            assert_eq!(*domain_count, 3, "Should cover all 3 domains");
            assert_eq!(*bonus, 6, "Full spectrum bonus should be +6");
        }
    }

    #[tokio::test]
    async fn test_dual_domain_coverage_detection() {
        // Create sensors that cover only 2 domains (air + surface from radar)
        let radar_piece = Piece {
            id: 17,
            piece_type: PieceType::Sensor(DetectionMode::Radar),
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let configs = vec![radar_piece.to_node_config()];
        let capabilities = radar_piece.to_capabilities();

        let detected = EmergentCapabilityDetector::detect(&capabilities, &configs).await;

        // Should detect dual-domain coverage (radar: air+surface = 2 domains)
        let multi_domain = detected
            .iter()
            .find(|e| matches!(e, EmergentCapabilityType::MultiDomainCoverage { .. }));

        assert!(multi_domain.is_some(), "Should detect dual-domain coverage");
        if let Some(EmergentCapabilityType::MultiDomainCoverage {
            domain_count,
            bonus,
        }) = multi_domain
        {
            assert_eq!(*domain_count, 2, "Should cover 2 domains");
            assert_eq!(*bonus, 4, "Dual domain bonus should be +4");
        }
    }

    // =============================================================================
    // ANALYST CLASS TESTS
    // =============================================================================

    #[test]
    fn test_analyst_to_node_config() {
        let analyst_piece = Piece {
            id: 20,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 5,
            y: 5,
            fuel: 100,
            max_fuel: 100,
        };

        let config = analyst_piece.to_node_config();

        assert_eq!(config.id, "piece_20");
        assert_eq!(config.platform_type, "analyst");
    }

    #[test]
    fn test_analyst_to_capabilities() {
        let analyst_piece = Piece {
            id: 21,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = analyst_piece.to_capabilities();

        // Analyst should have 3 capabilities: classify, predict, fuse
        assert_eq!(caps.len(), 3);

        // All should be Compute type
        for cap in &caps {
            assert_eq!(cap.capability_type, CapabilityType::Compute as i32);
        }

        // Check that each AI capability is present
        let ai_caps: Vec<String> = caps
            .iter()
            .filter_map(|c| {
                serde_json::from_str::<serde_json::Value>(&c.metadata_json)
                    .ok()
                    .and_then(|v| {
                        v.get("ai_capability")
                            .and_then(|a| a.as_str())
                            .map(String::from)
                    })
            })
            .collect();

        assert!(ai_caps.contains(&"classify".to_string()));
        assert!(ai_caps.contains(&"predict".to_string()));
        assert!(ai_caps.contains(&"fuse".to_string()));
    }

    #[test]
    fn test_analyst_domain() {
        let analyst_piece = Piece {
            id: 22,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        // Analysts are ground-based (Surface domain)
        assert_eq!(analyst_piece.domain(), Domain::Surface);

        // Analysts don't detect directly
        let detection_domains = analyst_piece.detection_domains();
        assert!(detection_domains.is_empty());
    }

    #[test]
    fn test_classify_bonus_calculation() {
        let analyst_piece = Piece {
            id: 23,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = analyst_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::classify_bonus(&caps);

        // One analyst = 3 (one classify capability * 3)
        assert_eq!(bonus, 3);
    }

    #[test]
    fn test_predict_bonus_calculation() {
        let analyst_piece = Piece {
            id: 24,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = analyst_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::predict_bonus(&caps);

        // One analyst = 2 (one predict capability * 2)
        assert_eq!(bonus, 2);
    }

    #[test]
    fn test_fuse_bonus_calculation() {
        let analyst_piece = Piece {
            id: 25,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = analyst_piece.to_capabilities();
        let bonus = ProtocolBonusCalculator::fuse_bonus(&caps);

        // One analyst = 3 (one fuse capability * 3)
        assert_eq!(bonus, 3);
    }

    #[test]
    fn test_analyst_symbol() {
        let analyst_piece = Piece {
            id: 26,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        assert_eq!(analyst_piece.piece_type.symbol(), "An");
    }

    #[test]
    fn test_analyst_no_detect_bonus() {
        let analyst_piece = Piece {
            id: 27,
            piece_type: PieceType::Analyst,
            team: Team::Blue,
            x: 0,
            y: 0,
            fuel: 100,
            max_fuel: 100,
        };

        let caps = analyst_piece.to_capabilities();

        // Analyst should have no detect bonus (no sensors)
        let detect = ProtocolBonusCalculator::detect_bonus(&caps);
        assert_eq!(detect, 0);

        // And no strike bonus
        let strike = ProtocolBonusCalculator::strike_bonus(&caps);
        assert_eq!(strike, 0);
    }
}
