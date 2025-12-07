//! Protocol type mappings for HIVE Commander
//!
//! Maps game types (PieceType, DetectionMode) to hive-protocol types
//! (NodeConfig, Capability, Operator).

use hive_protocol::models::{
    AuthorityLevel, Capability, CapabilityExt, CapabilityType, HumanMachinePair,
    HumanMachinePairExt, NodeConfig, NodeConfigExt, Operator, OperatorExt, OperatorRank,
};
use serde_json::json;

use crate::{DetectionMode, Piece, PieceType};

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
                let mut cap = Capability::new(
                    format!("sensor_{}_{}", self.id, mode.name().to_lowercase()),
                    format!("{} Sensor", mode.name()),
                    CapabilityType::Sensor,
                    0.9, // High confidence
                );
                cap.metadata_json = serde_json::to_string(&json!({
                    "sensor_type": mode.name().to_lowercase(),
                    "range": mode.range(),
                    "detection_mode": mode.name()
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
        use hive_protocol::composition::CompositionContext;

        let context = CompositionContext::new(vec![]).with_node_configs(configs.to_vec());

        context.authorization_bonus()
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
}

impl EmergentCapabilityDetector {
    /// Detect emergent capabilities from a set of protocol capabilities
    pub async fn detect(
        capabilities: &[Capability],
        node_configs: &[NodeConfig],
    ) -> Vec<EmergentCapabilityType> {
        use hive_protocol::composition::{
            emergent::{AuthorizationCoverageRule, IsrChainRule, StrikeChainRule},
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
}
