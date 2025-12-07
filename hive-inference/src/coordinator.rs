//! Coordinator module - Aggregates team capabilities for C2 visibility
//!
//! The Coordinator is a node in the HIVE mesh that aggregates capabilities
//! from multiple teams and presents a formation-level view to C2.
//!
//! # Architecture Note
//!
//! The HIVE protocol uses CRDT-based document synchronization (Automerge/Iroh).
//! All nodes on the mesh automatically sync documents. The Coordinator doesn't
//! "bridge networks" - it simply reads team documents from the mesh and
//! aggregates their capabilities.
//!
//! Network connectivity, document sync, and mesh topology are handled by
//! the hive-protocol crate, not this application layer.

use hive_protocol::cell::{AggregatedCapability, CapabilityAggregator};
use hive_protocol::discovery::capability_query::{
    CapabilityQuery, CapabilityQueryEngine, CapabilityStats, QueryMatch,
};
use hive_protocol::models::{CapabilityType, NodeConfig, NodeConfigExt, NodeState, NodeStateExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::messages::ModelCapability;
use crate::platform::Platform;
use crate::registry::ModelQuery;
use crate::team::Team;

// ============================================================================
// Formation Capability Summary (App-level view for C2)
// ============================================================================

/// Formation-level capability summary aggregating multiple teams
///
/// This is an application-level view presented to C2, summarizing
/// the capabilities of all teams in the formation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationCapabilitySummary {
    /// Formation identifier
    pub formation_id: String,
    /// Number of teams in formation
    pub team_count: usize,
    /// Total platforms across all teams
    pub total_platforms: usize,
    /// Total cameras across all teams
    pub total_cameras: usize,
    /// Total AI trackers across all teams
    pub total_trackers: usize,
    /// AI model versions in use
    pub tracker_versions: Vec<String>,
    /// Coverage sectors (team names/areas)
    pub coverage_sectors: Vec<String>,
    /// Overall readiness score (0.0-1.0)
    pub readiness_score: f32,
    /// Aggregated capability confidence by type
    pub capability_confidence: HashMap<String, f32>,
}

impl std::fmt::Display for FormationCapabilitySummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Formation {}: {} teams, {} platforms\n\
             Cameras: {}, Trackers: {} ({})\n\
             Coverage: {}\n\
             Readiness: {:.0}%",
            self.formation_id,
            self.team_count,
            self.total_platforms,
            self.total_cameras,
            self.total_trackers,
            self.tracker_versions.join(", "),
            self.coverage_sectors.join(" + "),
            self.readiness_score * 100.0
        )
    }
}

// ============================================================================
// Model Inventory Summary (Formation-level model view)
// ============================================================================

/// Formation-level AI model inventory
///
/// Provides a consolidated view of all AI models across the formation,
/// supporting Issue #107 Phase 3: Hierarchical Aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInventorySummary {
    /// Formation identifier
    pub formation_id: String,
    /// Total model instances across formation
    pub total_models: usize,
    /// Operational model count (ready, active, or degraded)
    pub operational_models: usize,
    /// Models by type (e.g., "detector": 5, "tracker": 3)
    pub models_by_type: HashMap<String, usize>,
    /// Models by version (e.g., "object_tracker:1.3.0": 4)
    pub models_by_version: HashMap<String, usize>,
    /// Platforms with each model (model_id -> [platform_ids])
    pub model_platforms: HashMap<String, Vec<String>>,
    /// Average performance by model type
    pub avg_performance: HashMap<String, ModelPerformanceStats>,
    /// Degraded models (platform_id, model_id, reason)
    pub degraded_models: Vec<DegradedModelInfo>,
}

/// Aggregated performance statistics for a model type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPerformanceStats {
    /// Average precision across instances
    pub avg_precision: f64,
    /// Average recall across instances
    pub avg_recall: f64,
    /// Average FPS across instances
    pub avg_fps: f64,
    /// Average latency in ms
    pub avg_latency_ms: Option<f64>,
    /// Number of instances contributing to these stats
    pub instance_count: usize,
}

/// Information about a degraded model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedModelInfo {
    /// Platform hosting the degraded model
    pub platform_id: String,
    /// Team containing the platform
    pub team_name: String,
    /// Model identifier
    pub model_id: String,
    /// Model version
    pub model_version: String,
    /// Degradation reason if known
    pub reason: Option<String>,
}

/// Performance metrics tuple: (precision, recall, fps, latency_ms)
type PerformanceMetrics = (f64, f64, f64, Option<f64>);

/// Result of a model query across the formation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelQueryResult {
    /// Matching platforms with their model capabilities
    pub matches: Vec<PlatformModelMatch>,
    /// Total matches found
    pub total_matches: usize,
    /// Teams represented in results
    pub teams: Vec<String>,
}

/// A platform matching a model query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformModelMatch {
    /// Platform identifier
    pub platform_id: String,
    /// Platform name
    pub platform_name: String,
    /// Team name
    pub team_name: String,
    /// The matching model capability
    pub model: ModelCapability,
}

// ============================================================================
// Coordinator (App-level team aggregation)
// ============================================================================

/// A coordinator node that aggregates team capabilities for C2
///
/// The Coordinator reads team documents from the HIVE mesh and presents
/// a unified formation-level view. Network connectivity and document
/// synchronization are handled by the hive-protocol layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coordinator {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Teams in this formation (read from mesh documents)
    pub teams: Vec<Team>,
}

impl Coordinator {
    /// Create a new coordinator
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            teams: Vec::new(),
        }
    }

    /// Register a team with this coordinator
    ///
    /// In a real deployment, teams would be discovered via the HIVE mesh.
    /// This method is for local testing and demonstration.
    pub fn register_team(&mut self, team: Team) {
        self.teams.push(team);
    }

    /// Get the number of registered teams
    pub fn team_count(&self) -> usize {
        self.teams.len()
    }

    /// Get total platform count across all teams
    pub fn total_platforms(&self) -> usize {
        self.teams.iter().map(|t| t.member_count()).sum()
    }

    /// Get a team by name
    pub fn get_team(&self, name: &str) -> Option<&Team> {
        self.teams.iter().find(|t| t.name == name)
    }

    /// Get a mutable team by name
    pub fn get_team_mut(&mut self, name: &str) -> Option<&mut Team> {
        self.teams.iter_mut().find(|t| t.name == name)
    }

    /// Aggregate capabilities from all teams using hive-protocol
    pub fn get_aggregated_capabilities(
        &self,
    ) -> Result<HashMap<CapabilityType, AggregatedCapability>, hive_protocol::Error> {
        // Convert all team members to NodeConfig/NodeState for aggregation
        let mut all_members: Vec<(NodeConfig, NodeState)> = Vec::new();

        for team in &self.teams {
            for platform in &team.members {
                let (config, state) = platform_to_node(platform);
                all_members.push((config, state));
            }
        }

        CapabilityAggregator::aggregate_capabilities(&all_members)
    }

    /// Calculate formation readiness score
    pub fn readiness_score(&self) -> f32 {
        if let Ok(caps) = self.get_aggregated_capabilities() {
            CapabilityAggregator::calculate_readiness_score(&caps)
        } else {
            0.0
        }
    }

    /// Generate formation-level capability summary for C2
    pub fn get_formation_summary(&self) -> FormationCapabilitySummary {
        let mut total_cameras = 0;
        let mut total_trackers = 0;
        let mut tracker_versions: Vec<String> = Vec::new();
        let mut coverage_sectors: Vec<String> = Vec::new();

        for team in &self.teams {
            let team_summary = team.get_capability_summary();
            total_cameras += team_summary.camera_count;

            for model in &team_summary.ai_models {
                total_trackers += 1;
                let version_str = format!("v{}", model.version);
                if !tracker_versions.contains(&version_str) {
                    tracker_versions.push(version_str);
                }
            }

            // Use team name as sector identifier
            coverage_sectors.push(format!("Sector {}", team.name.replace(" Team", "")));
        }

        // Build capability confidence map
        let mut capability_confidence: HashMap<String, f32> = HashMap::new();
        if let Ok(caps) = self.get_aggregated_capabilities() {
            for (cap_type, agg_cap) in caps {
                capability_confidence.insert(format!("{:?}", cap_type), agg_cap.confidence);
            }
        }

        FormationCapabilitySummary {
            formation_id: self.id.to_string(),
            team_count: self.teams.len(),
            total_platforms: self.total_platforms(),
            total_cameras,
            total_trackers,
            tracker_versions,
            coverage_sectors,
            readiness_score: self.readiness_score(),
            capability_confidence,
        }
    }

    // ========================================================================
    // Model Query API (Issue #107 Phase 3)
    // ========================================================================

    /// Query models across the formation matching the given criteria
    ///
    /// Returns all platforms with models matching the query.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Find platforms with object_tracker >= v1.3.0 and precision >= 0.9
    /// let query = ModelQuery::new()
    ///     .with_model_id("object_tracker")
    ///     .with_min_version("1.3.0")
    ///     .with_min_precision(0.9)
    ///     .operational();
    /// let results = coordinator.query_models(&query);
    /// ```
    pub fn query_models(&self, query: &ModelQuery) -> ModelQueryResult {
        let mut matches = Vec::new();
        let mut teams_seen = Vec::new();

        for team in &self.teams {
            for platform in &team.members {
                if let Platform::AiModel(ai_platform) = platform {
                    let model_cap = ai_platform.to_model_capability();
                    if query.matches(&model_cap) {
                        matches.push(PlatformModelMatch {
                            platform_id: ai_platform.id.clone(),
                            platform_name: ai_platform.name.clone(),
                            team_name: team.name.clone(),
                            model: model_cap,
                        });

                        if !teams_seen.contains(&team.name) {
                            teams_seen.push(team.name.clone());
                        }
                    }
                }
            }
        }

        ModelQueryResult {
            total_matches: matches.len(),
            matches,
            teams: teams_seen,
        }
    }

    /// Get a complete model inventory for the formation
    ///
    /// Provides aggregated statistics about all AI models across teams.
    pub fn get_model_inventory(&self) -> ModelInventorySummary {
        let mut total_models = 0;
        let mut operational_models = 0;
        let mut models_by_type: HashMap<String, usize> = HashMap::new();
        let mut models_by_version: HashMap<String, usize> = HashMap::new();
        let mut model_platforms: HashMap<String, Vec<String>> = HashMap::new();
        let mut degraded_models = Vec::new();

        // Collect performance data for averaging
        let mut perf_by_type: HashMap<String, Vec<PerformanceMetrics>> = HashMap::new();

        for team in &self.teams {
            for platform in &team.members {
                if let Platform::AiModel(ai_platform) = platform {
                    let model_cap = ai_platform.to_model_capability();
                    total_models += 1;

                    // Count operational models
                    if model_cap.is_operational() {
                        operational_models += 1;
                    }

                    // Count by type
                    *models_by_type
                        .entry(model_cap.model_type.clone())
                        .or_insert(0) += 1;

                    // Count by version
                    let version_key = format!("{}:{}", model_cap.model_id, model_cap.model_version);
                    *models_by_version.entry(version_key).or_insert(0) += 1;

                    // Track platforms per model
                    model_platforms
                        .entry(model_cap.model_id.clone())
                        .or_default()
                        .push(ai_platform.id.clone());

                    // Collect performance data
                    perf_by_type
                        .entry(model_cap.model_type.clone())
                        .or_default()
                        .push((
                            model_cap.performance.precision,
                            model_cap.performance.recall,
                            model_cap.performance.fps,
                            model_cap.performance.latency_ms,
                        ));

                    // Track degraded models
                    if model_cap.degraded {
                        degraded_models.push(DegradedModelInfo {
                            platform_id: ai_platform.id.clone(),
                            team_name: team.name.clone(),
                            model_id: model_cap.model_id.clone(),
                            model_version: model_cap.model_version.clone(),
                            reason: model_cap.degradation_reason.clone(),
                        });
                    }
                }
            }
        }

        // Calculate average performance by type
        let mut avg_performance = HashMap::new();
        for (model_type, perfs) in perf_by_type {
            let count = perfs.len();
            if count > 0 {
                let sum_precision: f64 = perfs.iter().map(|(p, _, _, _)| p).sum();
                let sum_recall: f64 = perfs.iter().map(|(_, r, _, _)| r).sum();
                let sum_fps: f64 = perfs.iter().map(|(_, _, f, _)| f).sum();
                let latencies: Vec<f64> = perfs.iter().filter_map(|(_, _, _, l)| *l).collect();

                avg_performance.insert(
                    model_type,
                    ModelPerformanceStats {
                        avg_precision: sum_precision / count as f64,
                        avg_recall: sum_recall / count as f64,
                        avg_fps: sum_fps / count as f64,
                        avg_latency_ms: if latencies.is_empty() {
                            None
                        } else {
                            Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
                        },
                        instance_count: count,
                    },
                );
            }
        }

        ModelInventorySummary {
            formation_id: self.id.to_string(),
            total_models,
            operational_models,
            models_by_type,
            models_by_version,
            model_platforms,
            avg_performance,
            degraded_models,
        }
    }

    /// Find platforms that can run a specific model
    ///
    /// Useful for model deployment planning.
    pub fn find_platforms_with_model(&self, model_id: &str) -> Vec<PlatformModelMatch> {
        let query = ModelQuery::new().with_model_id(model_id);
        self.query_models(&query).matches
    }

    /// Get all operational models of a specific type
    pub fn get_operational_models_by_type(&self, model_type: &str) -> Vec<PlatformModelMatch> {
        let query = ModelQuery::new().with_model_type(model_type).operational();
        self.query_models(&query).matches
    }

    /// Count platforms meeting minimum model requirements
    ///
    /// Useful for capability assessment before tasking.
    pub fn count_capable_platforms(
        &self,
        model_id: &str,
        min_version: &str,
        min_precision: f64,
    ) -> usize {
        let query = ModelQuery::new()
            .with_model_id(model_id)
            .with_min_version(min_version)
            .with_min_precision(min_precision)
            .operational();
        self.query_models(&query).total_matches
    }

    // ========================================================================
    // Generic Capability Query API (hive-protocol integration)
    // ========================================================================

    /// Query platforms by generic capabilities using hive-protocol's CapabilityQuery
    ///
    /// This provides a generic query interface that works with any capability
    /// type defined in the hive-protocol schema.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use hive_protocol::discovery::CapabilityQuery;
    /// use hive_protocol::models::CapabilityType;
    ///
    /// // Find platforms with sensor AND communication capabilities
    /// let query = CapabilityQuery::builder()
    ///     .require_type(CapabilityType::Sensor)
    ///     .require_type(CapabilityType::Communication)
    ///     .min_confidence(0.8)
    ///     .build();
    ///
    /// let matches = coordinator.query_capabilities(&query);
    /// for m in &matches {
    ///     println!("Platform {} (score: {:.2})", m.entity.id, m.score);
    /// }
    /// ```
    pub fn query_capabilities(&self, query: &CapabilityQuery) -> Vec<QueryMatch<NodeConfig>> {
        let engine = CapabilityQueryEngine::new();

        // Collect all platforms as NodeConfigs
        let nodes: Vec<NodeConfig> = self
            .teams
            .iter()
            .flat_map(|team| {
                team.members.iter().map(|platform| {
                    let (config, _state) = platform_to_node(platform);
                    config
                })
            })
            .collect();

        engine.query_platforms(query, &nodes)
    }

    /// Get capability statistics across the formation
    ///
    /// Returns aggregated statistics about capability distribution.
    pub fn get_capability_stats(&self) -> HashMap<CapabilityType, CapabilityStats> {
        let engine = CapabilityQueryEngine::new();

        let nodes: Vec<NodeConfig> = self
            .teams
            .iter()
            .flat_map(|team| {
                team.members.iter().map(|platform| {
                    let (config, _state) = platform_to_node(platform);
                    config
                })
            })
            .collect();

        engine.platform_capability_stats(&nodes)
    }

    /// Find platforms with a specific capability type
    ///
    /// Convenience method for common single-type queries.
    pub fn find_platforms_with_capability(
        &self,
        cap_type: CapabilityType,
        min_confidence: f32,
    ) -> Vec<QueryMatch<NodeConfig>> {
        let query = CapabilityQuery::builder()
            .require_type(cap_type)
            .min_confidence(min_confidence)
            .build();

        self.query_capabilities(&query)
    }

    /// Find platforms with multiple required capabilities
    ///
    /// Returns platforms that have ALL specified capability types.
    pub fn find_platforms_with_capabilities(
        &self,
        cap_types: &[CapabilityType],
        min_confidence: f32,
    ) -> Vec<QueryMatch<NodeConfig>> {
        let mut builder = CapabilityQuery::builder().min_confidence(min_confidence);

        for cap_type in cap_types {
            builder = builder.require_type(*cap_type);
        }

        self.query_capabilities(&builder.build())
    }
}

// ============================================================================
// Helper: Platform to NodeConfig/NodeState conversion
// ============================================================================

/// Convert a Platform to hive-protocol NodeConfig and NodeState
fn platform_to_node(platform: &Platform) -> (NodeConfig, NodeState) {
    use crate::platform::AuthorityLevel;
    use hive_protocol::models::{
        AuthorityLevel as HiveAuthorityLevel, BindingType, HumanMachinePair, HumanMachinePairExt,
        Operator, OperatorExt, OperatorRank,
    };
    use hive_protocol::traits::Phase;

    let caps = platform.get_capabilities();

    fn convert_authority(auth: AuthorityLevel) -> HiveAuthorityLevel {
        match auth {
            AuthorityLevel::Observer => HiveAuthorityLevel::Observer,
            AuthorityLevel::Advisor => HiveAuthorityLevel::Advisor,
            AuthorityLevel::Supervisor => HiveAuthorityLevel::Supervisor,
            AuthorityLevel::Commander => HiveAuthorityLevel::Commander,
        }
    }

    fn authority_to_rank(auth: AuthorityLevel) -> OperatorRank {
        match auth {
            AuthorityLevel::Commander => OperatorRank::E7,
            AuthorityLevel::Supervisor => OperatorRank::E6,
            AuthorityLevel::Advisor => OperatorRank::E5,
            AuthorityLevel::Observer => OperatorRank::E4,
        }
    }

    match platform {
        Platform::Operator(op) => {
            let mut config = NodeConfig::new("Operator".to_string());
            config.id = op.id.clone();

            for cap in caps {
                config.add_capability(cap);
            }

            let operator = Operator::new(
                op.id.clone(),
                op.name.clone(),
                authority_to_rank(op.authority),
                convert_authority(op.authority),
                "11B".to_string(),
            );

            let binding =
                HumanMachinePair::new(vec![operator], vec![op.id.clone()], BindingType::OneToOne);
            config.operator_binding = Some(binding);

            let mut state = NodeState::new((
                op.position.map(|(lat, _)| lat).unwrap_or(0.0),
                op.position.map(|(_, lon)| lon).unwrap_or(0.0),
                0.0,
            ));
            state.update_phase(Phase::Cell);

            (config, state)
        }
        Platform::Vehicle(veh) => {
            let mut config = NodeConfig::new(format!("{:?}", veh.vehicle_type));
            config.id = veh.id.clone();
            config.comm_range_m = veh.comm_range_m as f32;
            config.max_speed_mps = veh.max_speed_mps as f32;

            for cap in caps {
                config.add_capability(cap);
            }

            let mut state = NodeState::new((
                veh.position.map(|(lat, _, _)| lat).unwrap_or(0.0),
                veh.position.map(|(_, lon, _)| lon).unwrap_or(0.0),
                veh.position.map(|(_, _, alt)| alt).unwrap_or(0.0),
            ));
            state.update_phase(Phase::Cell);

            (config, state)
        }
        Platform::AiModel(ai) => {
            let mut config = NodeConfig::new("AiModel".to_string());
            config.id = ai.id.clone();

            for cap in caps {
                config.add_capability(cap);
            }

            let mut state = NodeState::new((0.0, 0.0, 0.0));
            state.update_phase(Phase::Cell);

            (config, state)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        AiModelInfo, AiModelPlatform, AuthorityLevel, OperatorPlatform, SensorCapability,
        VehiclePlatform,
    };

    /// Create a standard M1 vignette team
    fn create_test_team(name: &str, is_uav: bool) -> Team {
        let mut team = Team::new(format!("{} Team", name));

        team.add_member(Platform::Operator(OperatorPlatform::new(
            format!("{}-1", name),
            format!("{}-1", name.to_uppercase()),
            AuthorityLevel::Commander,
        )));

        if is_uav {
            team.add_member(Platform::Vehicle(
                VehiclePlatform::new_uav(format!("{}-2", name))
                    .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0)),
            ));
        } else {
            team.add_member(Platform::Vehicle(
                VehiclePlatform::new_ugv(format!("{}-2", name))
                    .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0)),
            ));
        }

        team.add_member(Platform::AiModel(AiModelPlatform::new(
            format!("{}-3", name),
            AiModelInfo::object_tracker("1.2.0"),
        )));

        team
    }

    #[test]
    fn test_coordinator_creation() {
        let mut coordinator = Coordinator::new("M1 Coordinator");
        assert_eq!(coordinator.name, "M1 Coordinator");
        assert_eq!(coordinator.team_count(), 0);

        let team = Team::new("Alpha Team");
        coordinator.register_team(team);

        assert_eq!(coordinator.team_count(), 1);
    }

    #[test]
    fn test_formation_capability_summary() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        let summary = coordinator.get_formation_summary();

        assert_eq!(summary.team_count, 2);
        assert_eq!(summary.total_platforms, 6); // 3 per team
        assert_eq!(summary.total_cameras, 2); // One per team
        assert_eq!(summary.total_trackers, 2); // One per team
        assert!(summary.tracker_versions.contains(&"v1.2.0".to_string()));
        assert_eq!(summary.coverage_sectors.len(), 2);

        // Print summary for verification
        println!("{}", summary);
    }

    #[test]
    fn test_aggregated_capabilities() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        let caps = coordinator.get_aggregated_capabilities().unwrap();

        // Should have multiple capability types aggregated
        assert!(caps.contains_key(&CapabilityType::Communication));
        assert!(caps.contains_key(&CapabilityType::Sensor));
        assert!(caps.contains_key(&CapabilityType::Compute));

        // Communication should have multiple contributors
        let comm_cap = caps.get(&CapabilityType::Communication).unwrap();
        assert!(comm_cap.contributor_count >= 2);
    }

    #[test]
    fn test_readiness_score() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        coordinator.register_team(alpha);

        let score = coordinator.readiness_score();
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_get_team() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        coordinator.register_team(alpha);

        assert!(coordinator.get_team("Alpha Team").is_some());
        assert!(coordinator.get_team("Bravo Team").is_none());
    }

    #[test]
    fn test_total_platforms() {
        let mut coordinator = Coordinator::new("M1 Coordinator");

        let alpha = create_test_team("Alpha", true);
        let bravo = create_test_team("Bravo", false);

        coordinator.register_team(alpha);
        coordinator.register_team(bravo);

        assert_eq!(coordinator.total_platforms(), 6);
    }

    #[test]
    fn test_formation_summary_display() {
        let summary = FormationCapabilitySummary {
            formation_id: "test-123".to_string(),
            team_count: 2,
            total_platforms: 6,
            total_cameras: 2,
            total_trackers: 2,
            tracker_versions: vec!["v1.2.0".to_string()],
            coverage_sectors: vec!["Sector Alpha".to_string(), "Sector Bravo".to_string()],
            readiness_score: 0.85,
            capability_confidence: HashMap::new(),
        };

        let display = format!("{}", summary);
        assert!(display.contains("test-123"));
        assert!(display.contains("2 teams"));
        assert!(display.contains("6 platforms"));
        assert!(display.contains("Cameras: 2"));
        assert!(display.contains("Trackers: 2"));
        assert!(display.contains("85%"));
    }

    // ========================================================================
    // Model Query Tests (Issue #107 Phase 3)
    // ========================================================================

    #[test]
    fn test_query_models_by_id() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        let query = ModelQuery::new().with_model_id("object_tracker");
        let results = coordinator.query_models(&query);

        assert_eq!(results.total_matches, 2);
        assert_eq!(results.teams.len(), 2);
    }

    #[test]
    fn test_query_models_by_version() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        // Both teams have v1.2.0
        let query = ModelQuery::new()
            .with_model_id("object_tracker")
            .with_min_version("1.2.0");
        let results = coordinator.query_models(&query);
        assert_eq!(results.total_matches, 2);

        // Neither team has v2.0.0
        let query = ModelQuery::new()
            .with_model_id("object_tracker")
            .with_min_version("2.0.0");
        let results = coordinator.query_models(&query);
        assert_eq!(results.total_matches, 0);
    }

    #[test]
    fn test_query_models_by_type() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        let query = ModelQuery::new().with_model_type("detector_tracker");
        let results = coordinator.query_models(&query);
        assert_eq!(results.total_matches, 1);

        let query = ModelQuery::new().with_model_type("classifier");
        let results = coordinator.query_models(&query);
        assert_eq!(results.total_matches, 0);
    }

    #[test]
    fn test_get_model_inventory() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        let inventory = coordinator.get_model_inventory();

        assert_eq!(inventory.total_models, 2);
        assert!(inventory.models_by_type.contains_key("detector_tracker"));
        assert_eq!(inventory.models_by_type["detector_tracker"], 2);
        assert!(inventory
            .models_by_version
            .contains_key("object_tracker:1.2.0"));
        assert!(inventory.avg_performance.contains_key("detector_tracker"));
    }

    #[test]
    fn test_find_platforms_with_model() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        let matches = coordinator.find_platforms_with_model("object_tracker");
        assert_eq!(matches.len(), 2);

        let matches = coordinator.find_platforms_with_model("nonexistent");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_count_capable_platforms() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        // Models are in Loading state by default, so won't match operational query
        // Let's test the query still works
        let count = coordinator.count_capable_platforms("object_tracker", "1.0.0", 0.8);
        // Loading state is not operational, so count should be 0
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_operational_models_by_type() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Models start in Loading state
        let matches = coordinator.get_operational_models_by_type("detector_tracker");
        assert_eq!(matches.len(), 0); // Loading is not operational
    }

    #[test]
    fn test_model_query_result_structure() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        let query = ModelQuery::new().with_model_id("object_tracker");
        let results = coordinator.query_models(&query);

        assert_eq!(results.total_matches, 1);
        assert_eq!(results.matches.len(), 1);

        let m = &results.matches[0];
        assert_eq!(m.team_name, "Alpha Team");
        assert_eq!(m.model.model_id, "object_tracker");
        assert_eq!(m.model.model_version, "1.2.0");
    }

    // ========================================================================
    // Generic Capability Query Tests (hive-protocol integration)
    // ========================================================================

    #[test]
    fn test_query_capabilities_with_sensor() {
        use super::CapabilityQuery;

        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true)); // UAV with EO/IR sensor

        // Query for sensor capabilities
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .build();

        let matches = coordinator.query_capabilities(&query);

        // UAV has sensor capability
        assert!(!matches.is_empty());
        // Matches should be sorted by score
        if matches.len() > 1 {
            assert!(matches[0].score >= matches[1].score);
        }
    }

    #[test]
    fn test_query_capabilities_with_compute() {
        use super::CapabilityQuery;

        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Query for compute capabilities (AI models have compute)
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Compute)
            .build();

        let matches = coordinator.query_capabilities(&query);
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_query_capabilities_multiple_types() {
        use super::CapabilityQuery;

        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Query for platforms with both sensor AND communication
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .require_type(CapabilityType::Communication)
            .build();

        let matches = coordinator.query_capabilities(&query);
        // Vehicles have both sensor and communication
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_query_capabilities_with_confidence() {
        use super::CapabilityQuery;

        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Query with high confidence requirement
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .min_confidence(0.9)
            .build();

        let high_conf_matches = coordinator.query_capabilities(&query);

        // Query with low confidence requirement
        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .min_confidence(0.1)
            .build();

        let low_conf_matches = coordinator.query_capabilities(&query);

        // Low confidence should match at least as many as high confidence
        assert!(low_conf_matches.len() >= high_conf_matches.len());
    }

    #[test]
    fn test_get_capability_stats() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));
        coordinator.register_team(create_test_team("Bravo", false));

        let stats = coordinator.get_capability_stats();

        // Should have stats for multiple capability types
        assert!(!stats.is_empty());

        // Communication should be present (all platforms have it)
        if let Some(comm_stats) = stats.get(&CapabilityType::Communication) {
            assert!(comm_stats.count >= 2);
            assert!(comm_stats.avg_confidence > 0.0);
        }
    }

    #[test]
    fn test_find_platforms_with_capability() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Find platforms with sensor capability
        let matches = coordinator.find_platforms_with_capability(CapabilityType::Sensor, 0.5);
        assert!(!matches.is_empty());

        // Each match should have a score
        for m in &matches {
            assert!(m.score > 0.0);
            assert!(m.score <= 1.0);
        }
    }

    #[test]
    fn test_find_platforms_with_capabilities() {
        let mut coordinator = Coordinator::new("Test Coordinator");
        coordinator.register_team(create_test_team("Alpha", true));

        // Find platforms with both sensor and communication
        let cap_types = vec![CapabilityType::Sensor, CapabilityType::Communication];
        let matches = coordinator.find_platforms_with_capabilities(&cap_types, 0.5);

        // Should find vehicle platforms that have both
        for m in &matches {
            // Verify the platform has capabilities (it matched the query)
            assert!(m.score > 0.0);
        }
    }

    #[test]
    fn test_capability_query_empty_formation() {
        use super::CapabilityQuery;

        let coordinator = Coordinator::new("Empty Coordinator");

        let query = CapabilityQuery::builder()
            .require_type(CapabilityType::Sensor)
            .build();

        let matches = coordinator.query_capabilities(&query);
        assert!(matches.is_empty());

        let stats = coordinator.get_capability_stats();
        assert!(stats.is_empty());
    }
}
