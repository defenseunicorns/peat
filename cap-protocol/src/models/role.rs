//! Squad role model and scoring
//!
//! Defines tactical roles that platforms can fill within a squad, with scoring
//! algorithms that consider both platform capabilities and human operator specialties.

use crate::models::{CapabilityType, Operator, PlatformConfig, PlatformState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tactical role that a platform can fill within a squad
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SquadRole {
    /// Squad leader - elected leader, coordinates squad operations
    Leader,
    /// Primary sensor/scout - long-range detection and reconnaissance
    Sensor,
    /// Compute node - processes sensor data, runs analysis
    Compute,
    /// Communications relay - extends network range
    Relay,
    /// Strike platform - primary weapons capability
    Strike,
    /// Support platform - logistics, medical, maintenance
    Support,
    /// General follower - no specialized role
    Follower,
}

impl SquadRole {
    /// Get all assignable roles (excludes Leader, which is elected)
    pub fn assignable_roles() -> Vec<Self> {
        vec![
            Self::Sensor,
            Self::Compute,
            Self::Relay,
            Self::Strike,
            Self::Support,
            Self::Follower,
        ]
    }

    /// Get human-readable description of role
    pub fn description(&self) -> &'static str {
        match self {
            Self::Leader => "Squad leader - coordinates operations and makes tactical decisions",
            Self::Sensor => "Sensor/scout - provides long-range detection and reconnaissance",
            Self::Compute => "Compute node - processes sensor data and runs analysis algorithms",
            Self::Relay => "Communications relay - extends network range and connectivity",
            Self::Strike => "Strike platform - engages targets with weapons systems",
            Self::Support => {
                "Support platform - provides logistics, medical, or maintenance support"
            }
            Self::Follower => "General squad member - performs assigned tasks",
        }
    }

    /// Get required capabilities for this role
    pub fn required_capabilities(&self) -> Vec<CapabilityType> {
        match self {
            Self::Leader => vec![CapabilityType::Communication],
            Self::Sensor => vec![CapabilityType::Sensor],
            Self::Compute => vec![CapabilityType::Compute],
            Self::Relay => vec![CapabilityType::Communication],
            Self::Strike => vec![CapabilityType::Payload],
            Self::Support => vec![],
            Self::Follower => vec![],
        }
    }

    /// Get preferred capabilities for this role (not required but improve scoring)
    pub fn preferred_capabilities(&self) -> Vec<CapabilityType> {
        match self {
            Self::Leader => vec![CapabilityType::Compute, CapabilityType::Sensor],
            Self::Sensor => vec![CapabilityType::Communication],
            Self::Compute => vec![CapabilityType::Communication],
            Self::Relay => vec![CapabilityType::Sensor],
            Self::Strike => vec![CapabilityType::Sensor, CapabilityType::Compute],
            Self::Support => vec![CapabilityType::Mobility],
            Self::Follower => vec![],
        }
    }

    /// Get relevant MOS codes for this role (Military Occupational Specialty)
    pub fn relevant_mos(&self) -> Vec<&'static str> {
        match self {
            Self::Leader => vec!["11B", "11C", "19D"], // Infantry, Indirect Fire, Cavalry Scout
            Self::Sensor => vec!["19D", "35M", "35N"], // Cavalry Scout, Human Intel, Signals Intel
            Self::Compute => vec!["35F", "35N", "17C"], // Intel Analyst, Signals Intel, Cyber
            Self::Relay => vec!["25U", "25B", "25Q"], // Signal Support, IT Specialist, Multichannel
            Self::Strike => vec!["11B", "11C", "19K"], // Infantry, Indirect Fire, Armor
            Self::Support => vec!["68W", "88M", "91B"], // Medic, Transport, Mechanic
            Self::Follower => vec![],
        }
    }
}

/// Role assignment for a platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Platform ID
    pub platform_id: String,
    /// Assigned role
    pub role: SquadRole,
    /// Score for this assignment (0.0-1.0)
    pub score: f64,
    /// Whether this is the platform's primary role choice
    pub is_primary_choice: bool,
}

impl RoleAssignment {
    /// Create a new role assignment
    pub fn new(platform_id: String, role: SquadRole, score: f64, is_primary_choice: bool) -> Self {
        Self {
            platform_id,
            role,
            score,
            is_primary_choice,
        }
    }
}

/// Role scorer - calculates how well a platform fits a role
pub struct RoleScorer;

impl RoleScorer {
    /// Score a platform for a specific role
    ///
    /// Scoring considers:
    /// - Platform capabilities (required and preferred)
    /// - Human operator MOS (if present)
    /// - Platform health and readiness
    ///
    /// Returns score 0.0-1.0, or None if platform cannot fill role
    pub fn score_platform_for_role(
        config: &PlatformConfig,
        state: &PlatformState,
        role: SquadRole,
    ) -> Option<f64> {
        let operator = config.get_primary_operator();
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        // Check required capabilities (blocking)
        for required_cap_type in role.required_capabilities() {
            let has_required = config
                .capabilities
                .iter()
                .any(|c| c.capability_type == required_cap_type);

            if !has_required {
                return None; // Cannot fill this role
            }
        }

        // Score required capabilities (30% weight)
        let required_score = Self::score_required_capabilities(config, &role);
        score += required_score * 0.3;
        weight_sum += 0.3;

        // Score preferred capabilities (20% weight)
        let preferred_score = Self::score_preferred_capabilities(config, &role);
        score += preferred_score * 0.2;
        weight_sum += 0.2;

        // Score operator MOS match (30% weight if operator present)
        if let Some(op) = operator {
            let mos_score = Self::score_operator_mos(op, &role);
            score += mos_score * 0.3;
            weight_sum += 0.3;
        }

        // Score platform health (20% weight)
        let health_score = Self::score_platform_health(state);
        score += health_score * 0.2;
        weight_sum += 0.2;

        // Normalize if we didn't use all weights (no operator case)
        if weight_sum < 1.0 {
            score /= weight_sum;
        }

        Some(score.clamp(0.0, 1.0))
    }

    /// Score required capabilities
    fn score_required_capabilities(config: &PlatformConfig, role: &SquadRole) -> f64 {
        let required = role.required_capabilities();
        if required.is_empty() {
            return 1.0;
        }

        let mut total_score = 0.0;
        for req_type in &required {
            let best_capability = config
                .capabilities
                .iter()
                .filter(|c| c.capability_type == *req_type)
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let Some(cap) = best_capability {
                total_score += cap.confidence as f64;
            }
        }

        total_score / required.len() as f64
    }

    /// Score preferred capabilities
    fn score_preferred_capabilities(config: &PlatformConfig, role: &SquadRole) -> f64 {
        let preferred = role.preferred_capabilities();
        if preferred.is_empty() {
            return 1.0;
        }

        let mut total_score = 0.0;
        let mut count = 0;

        for pref_type in preferred {
            if let Some(best_cap) = config
                .capabilities
                .iter()
                .filter(|c| c.capability_type == pref_type)
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                total_score += best_cap.confidence as f64;
                count += 1;
            }
        }

        if count > 0 {
            total_score / count as f64
        } else {
            0.5 // Neutral score if no preferred capabilities
        }
    }

    /// Score operator MOS match
    fn score_operator_mos(operator: &Operator, role: &SquadRole) -> f64 {
        let relevant_mos = role.relevant_mos();
        if relevant_mos.is_empty() {
            return 0.5; // Neutral score for roles with no MOS preference
        }

        if relevant_mos.contains(&operator.mos.as_str()) {
            0.9 // High score for matching MOS
        } else {
            0.3 // Low score for non-matching MOS
        }
    }

    /// Score platform health
    fn score_platform_health(state: &PlatformState) -> f64 {
        match state.health {
            crate::models::HealthStatus::Nominal => 1.0,
            crate::models::HealthStatus::Degraded => 0.6,
            crate::models::HealthStatus::Critical => 0.3,
            crate::models::HealthStatus::Failed => 0.0,
        }
    }

    /// Get all role scores for a platform
    pub fn score_all_roles(
        config: &PlatformConfig,
        state: &PlatformState,
    ) -> HashMap<SquadRole, f64> {
        let mut scores = HashMap::new();

        for role in SquadRole::assignable_roles() {
            if let Some(score) = Self::score_platform_for_role(config, state, role) {
                scores.insert(role, score);
            }
        }

        scores
    }

    /// Get the best role for a platform
    pub fn best_role_for_platform(
        config: &PlatformConfig,
        state: &PlatformState,
    ) -> Option<(SquadRole, f64)> {
        Self::score_all_roles(config, state)
            .into_iter()
            .max_by(|(_, score_a), (_, score_b)| {
                score_a
                    .partial_cmp(score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AuthorityLevel, BindingType, Capability, HumanMachinePair, OperatorRank, PlatformConfig,
    };

    fn create_test_platform_with_capabilities(
        caps: Vec<Capability>,
    ) -> (PlatformConfig, PlatformState) {
        let mut config = PlatformConfig::new("test_platform".to_string());
        for cap in caps {
            config.add_capability(cap);
        }
        let state = PlatformState::new((0.0, 0.0, 0.0));
        (config, state)
    }

    fn create_test_operator(mos: &str, rank: OperatorRank) -> Operator {
        Operator::new(
            "op_1".to_string(),
            "Test Operator".to_string(),
            rank,
            AuthorityLevel::Commander,
            mos.to_string(),
        )
    }

    #[test]
    fn test_role_required_capabilities() {
        assert_eq!(
            SquadRole::Sensor.required_capabilities(),
            vec![CapabilityType::Sensor]
        );
        assert_eq!(
            SquadRole::Strike.required_capabilities(),
            vec![CapabilityType::Payload]
        );
        assert!(SquadRole::Follower.required_capabilities().is_empty());
    }

    #[test]
    fn test_role_relevant_mos() {
        let sensor_mos = SquadRole::Sensor.relevant_mos();
        assert!(sensor_mos.contains(&"19D")); // Cavalry Scout

        let relay_mos = SquadRole::Relay.relevant_mos();
        assert!(relay_mos.contains(&"25U")); // Signal Support
    }

    #[test]
    fn test_score_platform_without_required_capability() {
        // Platform without sensing capability cannot be sensor
        let (config, state) = create_test_platform_with_capabilities(vec![Capability::new(
            "cpu_1".to_string(),
            "CPU".to_string(),
            CapabilityType::Compute,
            0.8,
        )]);

        let score = RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor);
        assert!(score.is_none());
    }

    #[test]
    fn test_score_platform_with_required_capability() {
        // Platform with sensing capability can be sensor
        let (config, state) = create_test_platform_with_capabilities(vec![Capability::new(
            "radar_1".to_string(),
            "Radar".to_string(),
            CapabilityType::Sensor,
            0.9,
        )]);

        let score = RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor);
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn test_score_with_operator_mos_match() {
        let (mut config, state) = create_test_platform_with_capabilities(vec![Capability::new(
            "camera_1".to_string(),
            "Camera".to_string(),
            CapabilityType::Sensor,
            0.8,
        )]);

        let operator = create_test_operator("19D", OperatorRank::E5); // Cavalry Scout
        let binding = crate::models::HumanMachinePair::new(
            vec![operator],
            vec![config.id.clone()],
            crate::models::BindingType::OneToOne,
        );
        config.set_operator_binding(Some(binding));

        let score_with_match =
            RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor).unwrap();

        // Create platform without operator for comparison
        let (config_no_op, state_no_op) =
            create_test_platform_with_capabilities(vec![Capability::new(
                "camera_2".to_string(),
                "Camera".to_string(),
                CapabilityType::Sensor,
                0.8,
            )]);

        let score_without_operator =
            RoleScorer::score_platform_for_role(&config_no_op, &state_no_op, SquadRole::Sensor)
                .unwrap();

        // Score with matching MOS should be higher
        assert!(score_with_match > score_without_operator);
    }

    #[test]
    fn test_score_with_operator_mos_mismatch() {
        let (mut config, state) = create_test_platform_with_capabilities(vec![Capability::new(
            "camera_1".to_string(),
            "Camera".to_string(),
            CapabilityType::Sensor,
            0.8,
        )]);

        let operator = create_test_operator("68W", OperatorRank::E4); // Medic (not sensor MOS)
        let binding = crate::models::HumanMachinePair::new(
            vec![operator],
            vec![config.id.clone()],
            crate::models::BindingType::OneToOne,
        );
        config.set_operator_binding(Some(binding));

        let score_with_mismatch =
            RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor).unwrap();

        // Score should still be valid but not boosted
        assert!(score_with_mismatch > 0.0);
        assert!(score_with_mismatch < 1.0);
    }

    #[test]
    fn test_score_all_roles() {
        let (config, state) = create_test_platform_with_capabilities(vec![
            Capability::new(
                "camera_1".to_string(),
                "Camera".to_string(),
                CapabilityType::Sensor,
                0.9,
            ),
            Capability::new(
                "radio_1".to_string(),
                "Radio".to_string(),
                CapabilityType::Communication,
                0.7,
            ),
        ]);

        let scores = RoleScorer::score_all_roles(&config, &state);

        // Should have scores for roles it can fill
        assert!(scores.contains_key(&SquadRole::Sensor));
        assert!(scores.contains_key(&SquadRole::Relay));
        assert!(scores.contains_key(&SquadRole::Follower));

        // Should not have scores for roles requiring capabilities it doesn't have
        assert!(!scores.contains_key(&SquadRole::Strike));
        assert!(!scores.contains_key(&SquadRole::Compute));
    }

    #[test]
    fn test_best_role_for_platform() {
        let mut config = PlatformConfig::new("test_platform".to_string());
        config.add_capability(Capability::new(
            "radar_1".to_string(),
            "Radar".to_string(),
            CapabilityType::Sensor,
            0.95,
        ));
        config.add_capability(Capability::new(
            "radio_1".to_string(),
            "Radio".to_string(),
            CapabilityType::Communication,
            0.5,
        ));

        // Add operator with Sensor-relevant MOS to boost Sensor score
        let operator = create_test_operator("19D", OperatorRank::E4); // Cavalry Scout
        let platform_id = config.id.clone();
        config.operator_binding = Some(HumanMachinePair::new(
            vec![operator],
            vec![platform_id],
            BindingType::OneToOne,
        ));

        let state = PlatformState::new((0.0, 0.0, 0.0));

        let (best_role, score) = RoleScorer::best_role_for_platform(&config, &state).unwrap();

        // Best role should be Sensor due to high sensing capability + matching MOS
        assert_eq!(best_role, SquadRole::Sensor);
        assert!(score > 0.5);
    }

    #[test]
    fn test_role_assignment_creation() {
        let assignment =
            RoleAssignment::new("platform_1".to_string(), SquadRole::Sensor, 0.85, true);

        assert_eq!(assignment.platform_id, "platform_1");
        assert_eq!(assignment.role, SquadRole::Sensor);
        assert_eq!(assignment.score, 0.85);
        assert!(assignment.is_primary_choice);
    }

    #[test]
    fn test_assignable_roles() {
        let roles = SquadRole::assignable_roles();

        // Leader is not assignable (it's elected)
        assert!(!roles.contains(&SquadRole::Leader));

        // All other roles should be assignable
        assert!(roles.contains(&SquadRole::Sensor));
        assert!(roles.contains(&SquadRole::Compute));
        assert!(roles.contains(&SquadRole::Relay));
        assert!(roles.contains(&SquadRole::Strike));
        assert!(roles.contains(&SquadRole::Support));
        assert!(roles.contains(&SquadRole::Follower));
    }

    #[test]
    fn test_degraded_platform_role_scoring() {
        // Edge case: Degraded platform should have lower score than nominal
        let (config_nominal, state_nominal) =
            create_test_platform_with_capabilities(vec![Capability::new(
                "sensor_1".to_string(),
                "Sensor".to_string(),
                CapabilityType::Sensor,
                0.9,
            )]);

        let (config_degraded, mut state_degraded) =
            create_test_platform_with_capabilities(vec![Capability::new(
                "sensor_2".to_string(),
                "Sensor".to_string(),
                CapabilityType::Sensor,
                0.9,
            )]);
        state_degraded.health = crate::models::HealthStatus::Degraded;

        let score_nominal =
            RoleScorer::score_platform_for_role(&config_nominal, &state_nominal, SquadRole::Sensor)
                .unwrap();
        let score_degraded = RoleScorer::score_platform_for_role(
            &config_degraded,
            &state_degraded,
            SquadRole::Sensor,
        )
        .unwrap();

        // Degraded platform should score lower
        assert!(score_degraded < score_nominal);
        // But should still be viable (>0.4)
        assert!(score_degraded > 0.4);
    }

    #[test]
    fn test_critical_platform_role_scoring() {
        // Edge case: Critical health platform
        let (config, mut state) = create_test_platform_with_capabilities(vec![Capability::new(
            "sensor_1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )]);
        state.health = crate::models::HealthStatus::Critical;

        let score =
            RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor).unwrap();

        // Critical health (0.3) weighs 20%, so contributes 0.06
        // High capability (0.9) weighs 30%, so contributes 0.27
        // Total score should be around 0.5-0.6 range
        assert!(score > 0.0);
        assert!(score < 0.7); // Less than nominal but still viable
    }

    #[test]
    fn test_failed_platform_role_scoring() {
        // Edge case: Failed platform
        let (config, mut state) = create_test_platform_with_capabilities(vec![Capability::new(
            "sensor_1".to_string(),
            "Sensor".to_string(),
            CapabilityType::Sensor,
            0.9,
        )]);
        state.health = crate::models::HealthStatus::Failed;

        let score =
            RoleScorer::score_platform_for_role(&config, &state, SquadRole::Sensor).unwrap();

        // Failed health (0.0) weighs 20%, so contributes 0.0
        // High capability (0.9) weighs 30%, so contributes 0.27
        // Total score should be around 0.4-0.5 (capability only)
        assert!(score > 0.2);
        assert!(score < 0.6); // Significantly reduced but not zero
    }
}
