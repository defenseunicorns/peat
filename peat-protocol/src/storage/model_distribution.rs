//! Model distribution API (ADR-025 Phase 4, ADR-022 Integration)
//!
//! Model-specific distribution API integrating with Edge MLOps architecture.
//! Builds on `FileDistribution` to provide model-aware distribution with
//! variant selection, convergence tracking, and rollback capabilities.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 ModelDistribution Trait                      │
//! │  distribute_model() / convergence_status() / rollback()     │
//! └──────────────────────────┬──────────────────────────────────┘
//!                            │
//!            ┌───────────────┴───────────────┐
//!            ▼                               ▼
//! ┌──────────────────────┐       ┌──────────────────────┐
//! │  FileDistribution    │       │  ModelRegistry       │
//! │  (blob transfer)     │       │  (version tracking)  │
//! └──────────────────────┘       └──────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use peat_protocol::storage::{
//!     ModelDistribution, DistributionScope, TransferPriority,
//! };
//!
//! // Distribute model to capable nodes
//! let handle = distribution.distribute_model(
//!     "target_recognition",
//!     "4.2.1",
//!     DistributionScope::Capable {
//!         min_gpu_gb: Some(4.0),
//!         cpu_arch: Some("aarch64".into()),
//!         min_storage_mb: Some(500),
//!     },
//!     TransferPriority::High,
//! ).await?;
//!
//! // Check convergence status
//! let status = distribution.convergence_status(
//!     "target_recognition",
//!     "4.2.1",
//! ).await?;
//!
//! println!("Converged: {}/{}", status.converged, status.total_platforms);
//! ```

use super::file_distribution::{DistributionHandle, DistributionScope, TransferPriority};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;

// ============================================================================
// Types
// ============================================================================

/// Handle to track a model distribution operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelDistributionHandle {
    /// Model identifier
    pub model_id: String,
    /// Target version
    pub version: String,
    /// Selected variant for this distribution
    pub variant_id: String,
    /// Underlying file distribution handle
    pub distribution_handle: DistributionHandle,
    /// When distribution was initiated
    pub initiated_at: DateTime<Utc>,
}

/// Status of model convergence across the formation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelConvergenceStatus {
    /// Model identifier
    pub model_id: String,
    /// Target version we're converging to
    pub target_version: String,
    /// Total number of target platforms
    pub total_platforms: usize,
    /// Platforms that have target version AND it's operational
    pub converged: usize,
    /// Platforms currently receiving/deploying the model
    pub in_progress: usize,
    /// Platforms not yet started
    pub pending: usize,
    /// Platforms where distribution/deployment failed
    pub failed: usize,
    /// Distribution of versions across platforms (version -> count)
    pub version_distribution: HashMap<String, usize>,
    /// What's blocking convergence on specific nodes
    pub blockers: Vec<ConvergenceBlocker>,
    /// Estimated time to full convergence (if calculable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_completion: Option<Duration>,
}

impl ModelConvergenceStatus {
    /// Create new convergence status
    pub fn new(model_id: &str, target_version: &str, total_platforms: usize) -> Self {
        Self {
            model_id: model_id.to_string(),
            target_version: target_version.to_string(),
            total_platforms,
            converged: 0,
            in_progress: 0,
            pending: total_platforms,
            failed: 0,
            version_distribution: HashMap::new(),
            blockers: Vec::new(),
            estimated_completion: None,
        }
    }

    /// Check if convergence is complete (all platforms converged or failed)
    pub fn is_complete(&self) -> bool {
        self.converged + self.failed >= self.total_platforms
    }

    /// Check if convergence succeeded (all platforms have target version)
    pub fn is_success(&self) -> bool {
        self.converged >= self.total_platforms && self.failed == 0
    }

    /// Calculate convergence progress (0.0 to 1.0)
    pub fn convergence_progress(&self) -> f64 {
        if self.total_platforms == 0 {
            return 1.0;
        }
        self.converged as f64 / self.total_platforms as f64
    }
}

/// What's blocking convergence on a specific node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConvergenceBlocker {
    /// Node that's blocked
    pub node_id: String,
    /// Why it's blocked
    pub reason: BlockerReason,
    /// When the block was first detected
    pub since: DateTime<Utc>,
    /// Additional context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ConvergenceBlocker {
    /// Create a new blocker
    pub fn new(node_id: &str, reason: BlockerReason) -> Self {
        Self {
            node_id: node_id.to_string(),
            reason,
            since: Utc::now(),
            details: None,
        }
    }

    /// Add details to the blocker
    pub fn with_details(mut self, details: &str) -> Self {
        self.details = Some(details.to_string());
        self
    }
}

/// Reason a node is blocked from convergence
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockerReason {
    /// Node is network partitioned
    NetworkPartition,
    /// Insufficient storage space
    InsufficientStorage,
    /// Insufficient GPU memory for model
    InsufficientGpuMemory,
    /// File transfer failed
    TransferFailed,
    /// Model deployment/loading failed
    DeploymentFailed,
    /// Node doesn't meet capability requirements
    IncompatibleCapabilities,
    /// Node is currently busy with another operation
    NodeBusy,
    /// Unknown/other reason
    Unknown,
}

impl std::fmt::Display for BlockerReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkPartition => write!(f, "Network partition"),
            Self::InsufficientStorage => write!(f, "Insufficient storage"),
            Self::InsufficientGpuMemory => write!(f, "Insufficient GPU memory"),
            Self::TransferFailed => write!(f, "Transfer failed"),
            Self::DeploymentFailed => write!(f, "Deployment failed"),
            Self::IncompatibleCapabilities => write!(f, "Incompatible capabilities"),
            Self::NodeBusy => write!(f, "Node busy"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Node's model deployment status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeModelStatus {
    /// Node identifier
    pub node_id: String,
    /// Currently deployed model version (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    /// Deployed variant ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    /// Model operational status
    pub operational_status: ModelOperationalStatus,
    /// Last status update time
    pub last_updated: DateTime<Utc>,
}

/// Operational status of a deployed model
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelOperationalStatus {
    /// No model deployed
    #[default]
    NotDeployed,
    /// Model is being downloaded
    Downloading,
    /// Model is being loaded into runtime
    Loading,
    /// Model is operational and serving inference
    Operational,
    /// Model is loaded but degraded (e.g., high latency)
    Degraded,
    /// Model failed to load or crashed
    Failed,
}

/// Variant selection criteria for model distribution
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VariantSelector {
    /// Preferred precision (e.g., "float16", "int8")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_precision: Option<String>,
    /// Required execution providers
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required_providers: Vec<String>,
    /// Maximum model size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size_bytes: Option<u64>,
}

// ============================================================================
// ModelDistribution Trait
// ============================================================================

/// Model distribution service for AI/ML model deployment
///
/// Provides model-specific distribution with variant selection,
/// convergence tracking, and rollback capabilities.
#[async_trait::async_trait]
pub trait ModelDistribution: Send + Sync {
    /// Distribute a model version to target platforms
    ///
    /// Selects appropriate variant based on target capabilities and initiates
    /// distribution. Variant selection considers GPU memory, CPU architecture,
    /// and available execution providers.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Model identifier (e.g., "target_recognition")
    /// * `version` - Semantic version (e.g., "4.2.1")
    /// * `scope` - Target platforms (all, formation, specific nodes, capable)
    /// * `priority` - Transfer priority
    ///
    /// # Returns
    ///
    /// Handle for tracking distribution progress
    async fn distribute_model(
        &self,
        model_id: &str,
        version: &str,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<ModelDistributionHandle>;

    /// Distribute a model with explicit variant selection
    ///
    /// Use when automatic variant selection is not desired.
    async fn distribute_model_variant(
        &self,
        model_id: &str,
        version: &str,
        variant_id: &str,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<ModelDistributionHandle>;

    /// Distribute model delta (differential update)
    ///
    /// For large models, only transfer changed chunks between versions.
    /// Requires target platforms to have `from_version` locally.
    ///
    /// # Note
    ///
    /// Delta updates use content-defined chunking to minimize transfer size.
    /// If target doesn't have `from_version`, falls back to full distribution.
    async fn distribute_model_delta(
        &self,
        model_id: &str,
        from_version: &str,
        to_version: &str,
        scope: DistributionScope,
    ) -> Result<ModelDistributionHandle>;

    /// Get convergence status for a model version
    ///
    /// Returns detailed status of how many platforms have converged to
    /// the target version, what's blocking others, and estimated completion.
    async fn convergence_status(
        &self,
        model_id: &str,
        target_version: &str,
    ) -> Result<ModelConvergenceStatus>;

    /// Initiate rollback to a previous version
    ///
    /// Distributes the previous version to all platforms that have the
    /// current (problematic) version.
    async fn rollback(
        &self,
        model_id: &str,
        to_version: &str,
        scope: DistributionScope,
    ) -> Result<ModelDistributionHandle>;

    /// Get model status on a specific node
    async fn node_model_status(
        &self,
        model_id: &str,
        node_id: &str,
    ) -> Result<Option<NodeModelStatus>>;

    /// List all nodes with a specific model version
    async fn nodes_with_version(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<Vec<NodeModelStatus>>;

    /// Cancel an in-progress model distribution
    async fn cancel(&self, handle: &ModelDistributionHandle) -> Result<()>;

    /// Subscribe to convergence status updates
    async fn subscribe_convergence(
        &self,
        model_id: &str,
        target_version: &str,
    ) -> Result<tokio::sync::broadcast::Receiver<ModelConvergenceStatus>>;
}

// ============================================================================
// In-Memory Model Registry (for tracking deployed versions)
// ============================================================================

/// Tracks which model versions are deployed on which nodes
#[derive(Debug, Default)]
pub struct ModelDeploymentTracker {
    /// Node model statuses: node_id -> model_id -> status
    node_statuses: RwLock<HashMap<String, HashMap<String, NodeModelStatus>>>,
    /// Active distributions: distribution_id -> handle
    active_distributions: RwLock<HashMap<String, ModelDistributionHandle>>,
    /// Convergence status channels: (model_id, version) -> broadcast sender
    #[allow(dead_code)] // For future subscribe_convergence implementation
    convergence_channels:
        RwLock<HashMap<(String, String), tokio::sync::broadcast::Sender<ModelConvergenceStatus>>>,
}

impl ModelDeploymentTracker {
    /// Create a new deployment tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Update node's model status
    pub async fn update_node_status(&self, status: NodeModelStatus) {
        let mut statuses = self.node_statuses.write().await;
        let node_models = statuses.entry(status.node_id.clone()).or_default();

        // Extract model_id from status if we can infer it
        if let Some(ref version) = status.current_version {
            // We need a model_id - for now use a placeholder approach
            // In real usage, the status would include the model_id
            node_models.insert(version.clone(), status);
        }
    }

    /// Get status for a specific node and model
    pub async fn get_node_status(&self, model_id: &str, node_id: &str) -> Option<NodeModelStatus> {
        let statuses = self.node_statuses.read().await;
        statuses
            .get(node_id)
            .and_then(|models| models.get(model_id))
            .cloned()
    }

    /// Get all nodes with a specific model version
    pub async fn get_nodes_with_version(
        &self,
        model_id: &str,
        version: &str,
    ) -> Vec<NodeModelStatus> {
        let statuses = self.node_statuses.read().await;
        statuses
            .values()
            .filter_map(|models| models.get(model_id))
            .filter(|status| status.current_version.as_deref() == Some(version))
            .cloned()
            .collect()
    }

    /// Register an active distribution
    pub async fn register_distribution(&self, handle: ModelDistributionHandle) {
        let mut distributions = self.active_distributions.write().await;
        distributions.insert(handle.distribution_handle.distribution_id.clone(), handle);
    }

    /// Get active distribution by ID
    pub async fn get_distribution(&self, distribution_id: &str) -> Option<ModelDistributionHandle> {
        let distributions = self.active_distributions.read().await;
        distributions.get(distribution_id).cloned()
    }

    /// Remove completed distribution
    pub async fn complete_distribution(&self, distribution_id: &str) {
        let mut distributions = self.active_distributions.write().await;
        distributions.remove(distribution_id);
    }

    /// Calculate convergence status for a model version
    pub async fn calculate_convergence(
        &self,
        model_id: &str,
        target_version: &str,
        total_platforms: usize,
    ) -> ModelConvergenceStatus {
        let statuses = self.node_statuses.read().await;

        let mut status = ModelConvergenceStatus::new(model_id, target_version, total_platforms);
        let mut version_counts: HashMap<String, usize> = HashMap::new();

        for (node_id, models) in statuses.iter() {
            if let Some(node_status) = models.get(model_id) {
                if let Some(ref version) = node_status.current_version {
                    *version_counts.entry(version.clone()).or_default() += 1;

                    if version == target_version {
                        match node_status.operational_status {
                            ModelOperationalStatus::Operational => {
                                status.converged += 1;
                                status.pending = status.pending.saturating_sub(1);
                            }
                            ModelOperationalStatus::Downloading
                            | ModelOperationalStatus::Loading => {
                                status.in_progress += 1;
                                status.pending = status.pending.saturating_sub(1);
                            }
                            ModelOperationalStatus::Failed => {
                                status.failed += 1;
                                status.pending = status.pending.saturating_sub(1);
                                status.blockers.push(ConvergenceBlocker::new(
                                    node_id,
                                    BlockerReason::DeploymentFailed,
                                ));
                            }
                            ModelOperationalStatus::Degraded => {
                                // Count as converged but note degradation
                                status.converged += 1;
                                status.pending = status.pending.saturating_sub(1);
                            }
                            ModelOperationalStatus::NotDeployed => {
                                // Still pending
                            }
                        }
                    }
                }
            }
        }

        status.version_distribution = version_counts;
        status
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convergence_status_creation() {
        let status = ModelConvergenceStatus::new("target_recognition", "4.2.1", 10);

        assert_eq!(status.model_id, "target_recognition");
        assert_eq!(status.target_version, "4.2.1");
        assert_eq!(status.total_platforms, 10);
        assert_eq!(status.converged, 0);
        assert_eq!(status.pending, 10);
        assert!(!status.is_complete());
        assert!(!status.is_success());
        assert_eq!(status.convergence_progress(), 0.0);
    }

    #[test]
    fn test_convergence_progress() {
        let mut status = ModelConvergenceStatus::new("model", "1.0", 10);
        status.converged = 5;
        status.pending = 5;

        assert_eq!(status.convergence_progress(), 0.5);
        assert!(!status.is_complete());

        status.converged = 10;
        status.pending = 0;
        assert_eq!(status.convergence_progress(), 1.0);
        assert!(status.is_complete());
        assert!(status.is_success());
    }

    #[test]
    fn test_convergence_with_failures() {
        let mut status = ModelConvergenceStatus::new("model", "1.0", 10);
        status.converged = 8;
        status.failed = 2;
        status.pending = 0;

        assert!(status.is_complete());
        assert!(!status.is_success()); // Not success because of failures
    }

    #[test]
    fn test_blocker_creation() {
        let blocker = ConvergenceBlocker::new("node-1", BlockerReason::InsufficientGpuMemory)
            .with_details("Required 8GB, available 4GB");

        assert_eq!(blocker.node_id, "node-1");
        assert_eq!(blocker.reason, BlockerReason::InsufficientGpuMemory);
        assert_eq!(
            blocker.details,
            Some("Required 8GB, available 4GB".to_string())
        );
    }

    #[test]
    fn test_blocker_reason_display() {
        assert_eq!(
            format!("{}", BlockerReason::NetworkPartition),
            "Network partition"
        );
        assert_eq!(
            format!("{}", BlockerReason::InsufficientStorage),
            "Insufficient storage"
        );
        assert_eq!(
            format!("{}", BlockerReason::TransferFailed),
            "Transfer failed"
        );
    }

    #[test]
    fn test_node_model_status() {
        let status = NodeModelStatus {
            node_id: "node-1".to_string(),
            current_version: Some("4.2.1".to_string()),
            variant_id: Some("fp16-cuda".to_string()),
            operational_status: ModelOperationalStatus::Operational,
            last_updated: Utc::now(),
        };

        assert_eq!(status.current_version, Some("4.2.1".to_string()));
        assert_eq!(
            status.operational_status,
            ModelOperationalStatus::Operational
        );
    }

    #[tokio::test]
    async fn test_deployment_tracker() {
        let tracker = ModelDeploymentTracker::new();

        // No status initially
        let status = tracker.get_node_status("model-1", "node-1").await;
        assert!(status.is_none());

        // Empty nodes list
        let nodes = tracker.get_nodes_with_version("model-1", "1.0").await;
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_model_distribution_handle() {
        use super::super::blob_traits::BlobHash;

        let handle = ModelDistributionHandle {
            model_id: "target_recognition".to_string(),
            version: "4.2.1".to_string(),
            variant_id: "fp16-cuda".to_string(),
            distribution_handle: DistributionHandle::new(
                BlobHash::from_hex("abc123"),
                DistributionScope::AllNodes,
                TransferPriority::High,
            ),
            initiated_at: Utc::now(),
        };

        assert_eq!(handle.model_id, "target_recognition");
        assert_eq!(handle.version, "4.2.1");
        assert_eq!(handle.variant_id, "fp16-cuda");
    }
}
