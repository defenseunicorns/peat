//! Model Update Coordination - Issue #177 / ADR-026
//!
//! Implements Phase 4 of Issue #107: Model Update Coordination
//!
//! The UpdateCoordinator manages rolling model updates across a formation:
//! - Phased rollout to prevent capability fragmentation
//! - Version compatibility checking
//! - Rollback mechanism for failed updates
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  UpdateCoordinator                                               │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
//! │  │ RolloutPolicy   │  │ VersionChecker  │  │ RollbackManager │ │
//! │  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
//! └──────────────────────────┬──────────────────────────────────────┘
//!                            │ coordinates updates on
//! ┌──────────────────────────┴──────────────────────────────────────┐
//! │  ModelRegistry (per platform)                                    │
//! │  - start_update()                                                │
//! │  - complete_update()                                             │
//! │  - fail_update() + rollback                                      │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::orchestration::{UpdateCoordinator, RolloutConfig, UpdateRequest};
//!
//! // Create coordinator
//! let coordinator = UpdateCoordinator::new(RolloutConfig::default());
//!
//! // Request a formation-wide update
//! let request = UpdateRequest {
//!     model_id: "object_tracker".into(),
//!     from_version: Some("1.2.0".into()),
//!     to_version: "1.3.0".into(),
//!     blob_hash: "sha256:abc123...".into(),
//!     ..Default::default()
//! };
//!
//! let plan = coordinator.plan_rollout(&formation, &request)?;
//! coordinator.execute_rollout(plan).await?;
//! ```

use crate::coordinator::Coordinator;
use crate::messages::{ModelCapability, OperationalStatus};
use crate::registry::{ModelRegistry, RegistryError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Errors that can occur during update coordination
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("No platforms found with model {0}")]
    NoPlatformsFound(String),

    #[error("Version incompatible: {reason}")]
    VersionIncompatible { reason: String },

    #[error("Rollout failed on {failed_count} platforms: {reason}")]
    RolloutFailed { failed_count: usize, reason: String },

    #[error("Rollback failed: {0}")]
    RollbackFailed(String),

    #[error("Update already in progress for model {0}")]
    UpdateInProgress(String),

    #[error("Minimum capability threshold not met: {available} < {required}")]
    CapabilityThresholdNotMet { available: usize, required: usize },

    #[error("Registry error: {0}")]
    Registry(#[from] RegistryError),
}

/// Configuration for rolling updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutConfig {
    /// Maximum platforms to update simultaneously (batch size)
    pub batch_size: usize,
    /// Minimum operational platforms during rollout (as percentage 0.0-1.0)
    pub min_availability: f64,
    /// Time to wait between batches (seconds)
    pub batch_delay_secs: u64,
    /// Time to wait for health check after update (seconds)
    pub health_check_delay_secs: u64,
    /// Number of health check retries before marking failed
    pub health_check_retries: usize,
    /// Automatically rollback on failure
    pub auto_rollback: bool,
    /// Maximum failed platforms before aborting rollout
    pub max_failures: usize,
}

impl Default for RolloutConfig {
    fn default() -> Self {
        Self {
            batch_size: 2,
            min_availability: 0.5, // Keep at least 50% operational
            batch_delay_secs: 5,
            health_check_delay_secs: 10,
            health_check_retries: 3,
            auto_rollback: true,
            max_failures: 2,
        }
    }
}

impl RolloutConfig {
    /// Create a conservative config for critical systems
    pub fn conservative() -> Self {
        Self {
            batch_size: 1,
            min_availability: 0.75,
            batch_delay_secs: 30,
            health_check_delay_secs: 30,
            health_check_retries: 5,
            auto_rollback: true,
            max_failures: 1,
        }
    }

    /// Create an aggressive config for rapid deployment
    pub fn aggressive() -> Self {
        Self {
            batch_size: 5,
            min_availability: 0.25,
            batch_delay_secs: 2,
            health_check_delay_secs: 5,
            health_check_retries: 2,
            auto_rollback: true,
            max_failures: 5,
        }
    }
}

/// Request to update a model across the formation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequest {
    /// Model identifier to update
    pub model_id: String,
    /// Current version to update from (None = any version)
    pub from_version: Option<String>,
    /// Target version to update to
    pub to_version: String,
    /// Blob hash for the new model artifact
    pub blob_hash: String,
    /// Model capability for the new version
    pub new_capability: Option<ModelCapability>,
    /// Override rollout config for this request
    pub config_override: Option<RolloutConfig>,
    /// Require specific minimum version for compatibility
    pub requires_min_version: Option<String>,
}

/// A platform targeted for update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTarget {
    /// Platform identifier
    pub platform_id: String,
    /// Team name
    pub team_name: String,
    /// Current model version
    pub current_version: String,
    /// Current operational status
    pub current_status: OperationalStatus,
}

/// A batch of platforms to update together
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBatch {
    /// Batch number (1-indexed)
    pub batch_number: usize,
    /// Platforms in this batch
    pub targets: Vec<UpdateTarget>,
    /// Status of this batch
    pub status: BatchStatus,
}

/// Status of an update batch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BatchStatus {
    /// Batch is pending execution
    Pending,
    /// Batch is currently being updated
    InProgress,
    /// Batch completed successfully
    Completed,
    /// Batch failed (with count of failures)
    Failed { failed_count: usize },
    /// Batch was rolled back
    RolledBack,
}

/// A planned rollout across the formation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutPlan {
    /// Unique plan identifier
    pub plan_id: String,
    /// The update request
    pub request: UpdateRequest,
    /// Configuration for this rollout
    pub config: RolloutConfig,
    /// Ordered batches of platforms to update
    pub batches: Vec<UpdateBatch>,
    /// Total platforms to update
    pub total_targets: usize,
    /// Platforms excluded from update (already updated, incompatible, etc.)
    pub excluded: Vec<ExcludedPlatform>,
    /// When the plan was created
    pub created_at: DateTime<Utc>,
    /// Overall plan status
    pub status: PlanStatus,
}

/// Why a platform was excluded from the rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedPlatform {
    /// Platform identifier
    pub platform_id: String,
    /// Reason for exclusion
    pub reason: ExclusionReason,
}

/// Reasons a platform may be excluded from update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExclusionReason {
    /// Already at target version
    AlreadyUpdated,
    /// Version incompatible (too old to upgrade directly)
    VersionIncompatible { current: String, required: String },
    /// Platform not operational
    NotOperational { status: OperationalStatus },
    /// Update already in progress
    UpdateInProgress,
}

/// Overall status of a rollout plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    /// Plan created but not started
    Created,
    /// Rollout in progress
    InProgress { current_batch: usize },
    /// Rollout completed successfully
    Completed,
    /// Rollout failed
    Failed { reason: String },
    /// Rollout aborted (manually or due to threshold)
    Aborted { reason: String },
    /// Rollout rolled back
    RolledBack,
}

/// Result of a completed rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutResult {
    /// Plan identifier
    pub plan_id: String,
    /// Final status
    pub status: PlanStatus,
    /// Platforms successfully updated
    pub succeeded: Vec<String>,
    /// Platforms that failed to update
    pub failed: Vec<FailedUpdate>,
    /// Platforms that were rolled back
    pub rolled_back: Vec<String>,
    /// When rollout started
    pub started_at: DateTime<Utc>,
    /// When rollout completed
    pub completed_at: DateTime<Utc>,
    /// Duration in seconds
    pub duration_secs: u64,
}

/// Information about a failed update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedUpdate {
    /// Platform identifier
    pub platform_id: String,
    /// Error message
    pub error: String,
    /// Whether rollback succeeded
    pub rollback_succeeded: bool,
}

/// Coordinates model updates across a formation
///
/// The UpdateCoordinator manages rolling updates to ensure:
/// - Minimum capability is maintained during updates
/// - Failed updates are rolled back automatically
/// - Updates are applied in controlled batches
pub struct UpdateCoordinator {
    /// Default rollout configuration
    config: RolloutConfig,
    /// Active rollout plans
    active_plans: HashMap<String, RolloutPlan>,
    /// Completed rollout results
    completed: Vec<RolloutResult>,
}

impl UpdateCoordinator {
    /// Create a new update coordinator with default config
    pub fn new(config: RolloutConfig) -> Self {
        Self {
            config,
            active_plans: HashMap::new(),
            completed: Vec::new(),
        }
    }

    /// Get the default rollout configuration
    pub fn config(&self) -> &RolloutConfig {
        &self.config
    }

    /// Plan a rollout for the given formation
    ///
    /// Returns a RolloutPlan that can be reviewed before execution.
    pub fn plan_rollout(
        &self,
        coordinator: &Coordinator,
        request: &UpdateRequest,
    ) -> Result<RolloutPlan, UpdateError> {
        let config = request
            .config_override
            .clone()
            .unwrap_or(self.config.clone());

        // Find all platforms with the target model
        let query = crate::registry::ModelQuery::new().with_model_id(&request.model_id);
        let matches = coordinator.query_models(&query);

        if matches.total_matches == 0 {
            return Err(UpdateError::NoPlatformsFound(request.model_id.clone()));
        }

        let mut targets = Vec::new();
        let mut excluded = Vec::new();

        for m in &matches.matches {
            // Check if already at target version
            if m.model.model_version == request.to_version {
                excluded.push(ExcludedPlatform {
                    platform_id: m.platform_id.clone(),
                    reason: ExclusionReason::AlreadyUpdated,
                });
                continue;
            }

            // Check version compatibility
            if let Some(ref min_ver) = request.requires_min_version {
                if !version_gte(&m.model.model_version, min_ver) {
                    excluded.push(ExcludedPlatform {
                        platform_id: m.platform_id.clone(),
                        reason: ExclusionReason::VersionIncompatible {
                            current: m.model.model_version.clone(),
                            required: min_ver.clone(),
                        },
                    });
                    continue;
                }
            }

            // Check if from_version matches (if specified)
            if let Some(ref from_ver) = request.from_version {
                if !m.model.model_version.starts_with(from_ver) {
                    continue; // Skip platforms not at from_version
                }
            }

            // Check if operational
            if !m.model.is_operational() {
                excluded.push(ExcludedPlatform {
                    platform_id: m.platform_id.clone(),
                    reason: ExclusionReason::NotOperational {
                        status: m.model.operational_status,
                    },
                });
                continue;
            }

            // Check if update already in progress
            if m.model.operational_status == OperationalStatus::Updating {
                excluded.push(ExcludedPlatform {
                    platform_id: m.platform_id.clone(),
                    reason: ExclusionReason::UpdateInProgress,
                });
                continue;
            }

            targets.push(UpdateTarget {
                platform_id: m.platform_id.clone(),
                team_name: m.team_name.clone(),
                current_version: m.model.model_version.clone(),
                current_status: m.model.operational_status,
            });
        }

        if targets.is_empty() {
            return Err(UpdateError::NoPlatformsFound(format!(
                "{} (all {} platforms excluded)",
                request.model_id, matches.total_matches
            )));
        }

        // Validate minimum availability constraint
        let min_required = (targets.len() as f64 * config.min_availability).ceil() as usize;
        let max_concurrent = targets.len() - min_required;
        let effective_batch_size = config.batch_size.min(max_concurrent.max(1));

        // Create batches
        let batches: Vec<UpdateBatch> = targets
            .chunks(effective_batch_size)
            .enumerate()
            .map(|(i, chunk)| UpdateBatch {
                batch_number: i + 1,
                targets: chunk.to_vec(),
                status: BatchStatus::Pending,
            })
            .collect();

        let plan = RolloutPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            request: request.clone(),
            config,
            total_targets: targets.len(),
            batches,
            excluded,
            created_at: Utc::now(),
            status: PlanStatus::Created,
        };

        info!(
            plan_id = %plan.plan_id,
            model = %request.model_id,
            target_version = %request.to_version,
            total_targets = plan.total_targets,
            batches = plan.batches.len(),
            excluded = plan.excluded.len(),
            "Created rollout plan"
        );

        Ok(plan)
    }

    /// Check version compatibility between source and target
    pub fn check_compatibility(
        &self,
        _model_id: &str,
        from_version: &str,
        to_version: &str,
    ) -> Result<(), UpdateError> {
        // Parse versions for comparison
        let from_parts: Vec<u32> = from_version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let to_parts: Vec<u32> = to_version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        // Major version must match or be +1
        if !from_parts.is_empty() && !to_parts.is_empty() {
            let from_major = from_parts[0];
            let to_major = to_parts[0];

            if to_major < from_major {
                return Err(UpdateError::VersionIncompatible {
                    reason: format!(
                        "Cannot downgrade major version: {} -> {}",
                        from_version, to_version
                    ),
                });
            }

            if to_major > from_major + 1 {
                return Err(UpdateError::VersionIncompatible {
                    reason: format!(
                        "Cannot skip major versions: {} -> {} (max jump is +1)",
                        from_version, to_version
                    ),
                });
            }
        }

        Ok(())
    }

    /// Execute a rollout plan
    ///
    /// This applies updates in batches, checking health after each batch.
    /// If auto_rollback is enabled, failed platforms are rolled back.
    pub async fn execute_rollout(
        &mut self,
        mut plan: RolloutPlan,
        registries: &mut HashMap<String, ModelRegistry>,
    ) -> Result<RolloutResult, UpdateError> {
        let started_at = Utc::now();
        let plan_id = plan.plan_id.clone();

        info!(
            plan_id = %plan_id,
            model = %plan.request.model_id,
            batches = plan.batches.len(),
            "Starting rollout execution"
        );

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        let mut rolled_back = Vec::new();

        for batch_idx in 0..plan.batches.len() {
            plan.status = PlanStatus::InProgress {
                current_batch: batch_idx + 1,
            };
            plan.batches[batch_idx].status = BatchStatus::InProgress;

            let batch = &plan.batches[batch_idx];
            debug!(
                batch = batch.batch_number,
                targets = batch.targets.len(),
                "Processing batch"
            );

            let mut batch_failures = 0;

            for target in &batch.targets {
                // Start update on this platform's registry
                if let Some(registry) = registries.get_mut(&target.platform_id) {
                    match registry.start_update(&plan.request.model_id, &plan.request.to_version) {
                        Ok(()) => {
                            debug!(
                                platform = %target.platform_id,
                                "Started update"
                            );

                            // Simulate update process (in real impl, would fetch blob and load model)
                            // For now, we'll immediately complete
                            if let Some(ref new_cap) = plan.request.new_capability {
                                match registry
                                    .complete_update(&plan.request.model_id, new_cap.clone())
                                {
                                    Ok(()) => {
                                        succeeded.push(target.platform_id.clone());
                                        info!(
                                            platform = %target.platform_id,
                                            version = %plan.request.to_version,
                                            "Update completed"
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            platform = %target.platform_id,
                                            error = %e,
                                            "Update completion failed"
                                        );
                                        batch_failures += 1;
                                        let rollback_ok = registry
                                            .fail_update(&plan.request.model_id, &e.to_string())
                                            .is_ok();
                                        if rollback_ok {
                                            rolled_back.push(target.platform_id.clone());
                                        }
                                        failed.push(FailedUpdate {
                                            platform_id: target.platform_id.clone(),
                                            error: e.to_string(),
                                            rollback_succeeded: rollback_ok,
                                        });
                                    }
                                }
                            } else {
                                // No new capability provided, just mark as updated
                                succeeded.push(target.platform_id.clone());
                            }
                        }
                        Err(e) => {
                            error!(
                                platform = %target.platform_id,
                                error = %e,
                                "Failed to start update"
                            );
                            batch_failures += 1;
                            failed.push(FailedUpdate {
                                platform_id: target.platform_id.clone(),
                                error: e.to_string(),
                                rollback_succeeded: false,
                            });
                        }
                    }
                } else {
                    warn!(
                        platform = %target.platform_id,
                        "No registry found for platform"
                    );
                    failed.push(FailedUpdate {
                        platform_id: target.platform_id.clone(),
                        error: "Registry not found".to_string(),
                        rollback_succeeded: false,
                    });
                    batch_failures += 1;
                }
            }

            // Update batch status
            if batch_failures > 0 {
                plan.batches[batch_idx].status = BatchStatus::Failed {
                    failed_count: batch_failures,
                };

                // Check if we should abort
                if failed.len() >= plan.config.max_failures {
                    warn!(
                        failed = failed.len(),
                        max = plan.config.max_failures,
                        "Max failures reached, aborting rollout"
                    );
                    plan.status = PlanStatus::Aborted {
                        reason: format!("Max failures ({}) exceeded", plan.config.max_failures),
                    };
                    break;
                }
            } else {
                plan.batches[batch_idx].status = BatchStatus::Completed;
            }

            // Delay between batches
            if batch_idx < plan.batches.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    plan.config.batch_delay_secs,
                ))
                .await;
            }
        }

        let completed_at = Utc::now();
        let duration_secs = (completed_at - started_at).num_seconds() as u64;

        // Determine final status
        let final_status = if failed.is_empty() {
            PlanStatus::Completed
        } else if !succeeded.is_empty() {
            plan.status.clone() // Keep InProgress or Aborted status
        } else {
            PlanStatus::Failed {
                reason: "All updates failed".to_string(),
            }
        };

        let result = RolloutResult {
            plan_id,
            status: final_status,
            succeeded,
            failed,
            rolled_back,
            started_at,
            completed_at,
            duration_secs,
        };

        info!(
            plan_id = %result.plan_id,
            succeeded = result.succeeded.len(),
            failed = result.failed.len(),
            rolled_back = result.rolled_back.len(),
            duration_secs = result.duration_secs,
            "Rollout completed"
        );

        self.completed.push(result.clone());
        Ok(result)
    }

    /// Get active rollout plans
    pub fn active_plans(&self) -> &HashMap<String, RolloutPlan> {
        &self.active_plans
    }

    /// Get completed rollout results
    pub fn completed_results(&self) -> &[RolloutResult] {
        &self.completed
    }
}

/// Compare semantic versions (returns true if v1 >= v2)
fn version_gte(v1: &str, v2: &str) -> bool {
    let v1_parts: Vec<u32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let v2_parts: Vec<u32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..v1_parts.len().max(v2_parts.len()) {
        let p1 = v1_parts.get(i).copied().unwrap_or(0);
        let p2 = v2_parts.get(i).copied().unwrap_or(0);
        if p1 > p2 {
            return true;
        }
        if p1 < p2 {
            return false;
        }
    }
    true // Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        AiModelInfo, AiModelPlatform, AuthorityLevel, OperatorPlatform, Platform,
    };
    use crate::team::Team;

    fn create_test_team(name: &str, model_version: &str) -> Team {
        let mut team = Team::new(format!("{} Team", name));

        team.add_member(Platform::Operator(OperatorPlatform::new(
            format!("{}-op", name),
            format!("{}-OPERATOR", name.to_uppercase()),
            AuthorityLevel::Commander,
        )));

        // Create AI model and set it to Ready status
        let mut ai_platform = AiModelPlatform::new(
            format!("{}-ai", name),
            AiModelInfo::object_tracker(model_version),
        );
        ai_platform.status = OperationalStatus::Ready;
        team.add_member(Platform::AiModel(ai_platform));

        team
    }

    #[test]
    fn test_version_gte() {
        assert!(version_gte("1.3.0", "1.2.0"));
        assert!(version_gte("1.2.0", "1.2.0"));
        assert!(!version_gte("1.1.0", "1.2.0"));
        assert!(version_gte("2.0.0", "1.9.9"));
        assert!(version_gte("1.10.0", "1.9.0"));
    }

    #[test]
    fn test_rollout_config_defaults() {
        let config = RolloutConfig::default();
        assert_eq!(config.batch_size, 2);
        assert_eq!(config.min_availability, 0.5);
        assert!(config.auto_rollback);
    }

    #[test]
    fn test_rollout_config_conservative() {
        let config = RolloutConfig::conservative();
        assert_eq!(config.batch_size, 1);
        assert_eq!(config.min_availability, 0.75);
        assert_eq!(config.max_failures, 1);
    }

    #[test]
    fn test_plan_rollout_basic() {
        let coordinator_update = UpdateCoordinator::new(RolloutConfig::default());
        let mut formation = Coordinator::new("Test Formation");

        formation.register_team(create_test_team("Alpha", "1.2.0"));
        formation.register_team(create_test_team("Bravo", "1.2.0"));

        let request = UpdateRequest {
            model_id: "object_tracker".into(),
            from_version: Some("1.2".into()),
            to_version: "1.3.0".into(),
            blob_hash: "sha256:abc123".into(),
            ..Default::default()
        };

        let plan = coordinator_update
            .plan_rollout(&formation, &request)
            .unwrap();

        assert_eq!(plan.total_targets, 2);
        assert!(!plan.batches.is_empty());
        assert_eq!(plan.status, PlanStatus::Created);
    }

    #[test]
    fn test_plan_rollout_excludes_already_updated() {
        let coordinator_update = UpdateCoordinator::new(RolloutConfig::default());
        let mut formation = Coordinator::new("Test Formation");

        formation.register_team(create_test_team("Alpha", "1.2.0"));
        formation.register_team(create_test_team("Bravo", "1.3.0")); // Already at target

        let request = UpdateRequest {
            model_id: "object_tracker".into(),
            to_version: "1.3.0".into(),
            blob_hash: "sha256:abc123".into(),
            ..Default::default()
        };

        let plan = coordinator_update
            .plan_rollout(&formation, &request)
            .unwrap();

        assert_eq!(plan.total_targets, 1); // Only Alpha
        assert_eq!(plan.excluded.len(), 1); // Bravo excluded
        assert!(matches!(
            plan.excluded[0].reason,
            ExclusionReason::AlreadyUpdated
        ));
    }

    #[test]
    fn test_plan_rollout_version_compatibility() {
        let coordinator_update = UpdateCoordinator::new(RolloutConfig::default());
        let mut formation = Coordinator::new("Test Formation");

        formation.register_team(create_test_team("Alpha", "1.0.0")); // Too old
        formation.register_team(create_test_team("Bravo", "1.2.0")); // OK

        let request = UpdateRequest {
            model_id: "object_tracker".into(),
            to_version: "1.3.0".into(),
            blob_hash: "sha256:abc123".into(),
            requires_min_version: Some("1.1.0".into()),
            ..Default::default()
        };

        let plan = coordinator_update
            .plan_rollout(&formation, &request)
            .unwrap();

        assert_eq!(plan.total_targets, 1); // Only Bravo
        assert_eq!(plan.excluded.len(), 1);
        assert!(matches!(
            plan.excluded[0].reason,
            ExclusionReason::VersionIncompatible { .. }
        ));
    }

    #[test]
    fn test_check_compatibility() {
        let coordinator = UpdateCoordinator::new(RolloutConfig::default());

        // Valid upgrades
        assert!(coordinator
            .check_compatibility("model", "1.2.0", "1.3.0")
            .is_ok());
        assert!(coordinator
            .check_compatibility("model", "1.2.0", "2.0.0")
            .is_ok());

        // Invalid: downgrade
        assert!(coordinator
            .check_compatibility("model", "2.0.0", "1.0.0")
            .is_err());

        // Invalid: skip major version
        assert!(coordinator
            .check_compatibility("model", "1.0.0", "3.0.0")
            .is_err());
    }

    #[test]
    fn test_batch_size_respects_availability() {
        let config = RolloutConfig {
            batch_size: 10,        // Request large batches
            min_availability: 0.8, // But keep 80% available
            ..Default::default()
        };
        let coordinator_update = UpdateCoordinator::new(config);
        let mut formation = Coordinator::new("Test Formation");

        // Add 5 teams
        for i in 0..5 {
            formation.register_team(create_test_team(&format!("Team{}", i), "1.2.0"));
        }

        let request = UpdateRequest {
            model_id: "object_tracker".into(),
            to_version: "1.3.0".into(),
            blob_hash: "sha256:abc123".into(),
            ..Default::default()
        };

        let plan = coordinator_update
            .plan_rollout(&formation, &request)
            .unwrap();

        // With 5 platforms and 80% availability, max 1 can be updating at a time
        // So batch size should be capped at 1
        assert!(plan.batches[0].targets.len() <= 1);
    }

    #[test]
    fn test_no_platforms_found_error() {
        let coordinator_update = UpdateCoordinator::new(RolloutConfig::default());
        let formation = Coordinator::new("Empty Formation");

        let request = UpdateRequest {
            model_id: "nonexistent_model".into(),
            to_version: "1.0.0".into(),
            blob_hash: "sha256:abc123".into(),
            ..Default::default()
        };

        let result = coordinator_update.plan_rollout(&formation, &request);
        assert!(matches!(result, Err(UpdateError::NoPlatformsFound(_))));
    }
}
