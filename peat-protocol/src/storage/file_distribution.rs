//! File distribution API (ADR-025 Phase 3)
//!
//! Higher-level API for targeted file delivery and progress monitoring.
//! Builds on `BlobStore` and `BlobDocumentIntegration` to provide
//! formation-aware file distribution with status tracking.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │         FileDistribution Trait          │
//! │  distribute() / status() / cancel()     │
//! └──────────────────┬──────────────────────┘
//!                    │
//!                    ▼
//!            ┌──────────────────┐
//!            │IrohFileDistrib.  │
//!            │ (Direct push)    │
//!            └──────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use peat_protocol::storage::{
//!     FileDistribution, IrohFileDistribution,
//!     DistributionScope, TransferPriority,
//! };
//!
//! // Distribute AI model to all nodes in a formation
//! let handle = distribution.distribute(
//!     &model_token,
//!     DistributionScope::Formation { formation_id: "alpha-squad".into() },
//!     TransferPriority::High,
//! ).await?;
//!
//! // Wait for completion with timeout
//! let status = distribution.wait_for_completion(
//!     &handle,
//!     Duration::from_secs(300),
//! ).await?;
//!
//! println!("Completed: {}/{}", status.completed, status.total_targets);
//! ```

use super::blob_traits::{BlobHash, BlobToken};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
#[cfg(feature = "automerge-backend")]
use tokio::sync::RwLock;
#[cfg(feature = "automerge-backend")]
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// Priority levels for file distribution
///
/// Higher priority transfers are scheduled first and may preempt lower priority
/// transfers when bandwidth is limited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransferPriority {
    /// ROE updates, safety-critical fixes - immediate transfer
    Critical,
    /// Operational model updates - next available window
    High,
    /// Routine updates - best effort
    #[default]
    Normal,
    /// Non-urgent - defer to low-bandwidth periods
    Low,
}

impl TransferPriority {
    /// Get numeric priority (higher = more urgent)
    pub fn as_numeric(&self) -> u8 {
        match self {
            Self::Critical => 4,
            Self::High => 3,
            Self::Normal => 2,
            Self::Low => 1,
        }
    }
}

/// Target scope for file distribution
///
/// Determines which nodes receive the distributed file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum DistributionScope {
    /// All connected nodes in the mesh
    #[default]
    AllNodes,

    /// Specific formation (cell, platoon, company)
    Formation {
        /// Formation identifier (e.g., "alpha-squad", "1st-platoon")
        formation_id: String,
    },

    /// Specific nodes by ID
    Nodes {
        /// List of target node IDs
        node_ids: Vec<String>,
    },

    /// Nodes with specific hardware capabilities
    Capable {
        /// Minimum GPU memory in GB (for model deployment)
        #[serde(skip_serializing_if = "Option::is_none")]
        min_gpu_gb: Option<f64>,

        /// Required CPU architecture (e.g., "x86_64", "aarch64")
        #[serde(skip_serializing_if = "Option::is_none")]
        cpu_arch: Option<String>,

        /// Minimum available storage in MB
        #[serde(skip_serializing_if = "Option::is_none")]
        min_storage_mb: Option<u64>,
    },
}

/// State of a transfer to a single node
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferState {
    /// Transfer not yet started
    #[default]
    Pending,
    /// Establishing connection to node
    Connecting,
    /// Actively transferring data
    Transferring,
    /// Transfer completed successfully
    Completed,
    /// Transfer failed
    Failed,
}

/// Status of transfer to a single node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeTransferStatus {
    /// Node identifier
    pub node_id: String,
    /// Current transfer state
    pub status: TransferState,
    /// Bytes transferred so far
    pub progress_bytes: u64,
    /// Total bytes to transfer
    pub total_bytes: u64,
    /// When transfer started (if started)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When transfer completed (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl NodeTransferStatus {
    /// Create new pending status for a node
    pub fn new(node_id: String, total_bytes: u64) -> Self {
        Self {
            node_id,
            status: TransferState::Pending,
            progress_bytes: 0,
            total_bytes,
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    /// Calculate progress percentage (0.0 to 1.0)
    pub fn progress_fraction(&self) -> f64 {
        if self.total_bytes == 0 {
            return 1.0;
        }
        self.progress_bytes as f64 / self.total_bytes as f64
    }
}

/// Handle to track a distribution operation
///
/// Returned from `distribute()` and used to query status, cancel, or wait.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistributionHandle {
    /// Unique distribution ID
    pub distribution_id: String,
    /// Hash of the blob being distributed
    pub blob_hash: BlobHash,
    /// Target scope
    pub scope: DistributionScope,
    /// Transfer priority
    pub priority: TransferPriority,
    /// When distribution was initiated
    pub started_at: DateTime<Utc>,
}

impl DistributionHandle {
    /// Create a new distribution handle
    pub fn new(blob_hash: BlobHash, scope: DistributionScope, priority: TransferPriority) -> Self {
        Self {
            distribution_id: Uuid::new_v4().to_string(),
            blob_hash,
            scope,
            priority,
            started_at: Utc::now(),
        }
    }
}

/// Overall distribution status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistributionStatus {
    /// The distribution handle
    pub handle: DistributionHandle,
    /// Total number of target nodes
    pub total_targets: usize,
    /// Number completed successfully
    pub completed: usize,
    /// Number currently in progress
    pub in_progress: usize,
    /// Number failed
    pub failed: usize,
    /// Per-node status
    pub node_statuses: HashMap<String, NodeTransferStatus>,
}

impl DistributionStatus {
    /// Create initial status for a distribution
    pub fn new(handle: DistributionHandle, target_nodes: Vec<String>, total_bytes: u64) -> Self {
        let node_statuses: HashMap<String, NodeTransferStatus> = target_nodes
            .into_iter()
            .map(|id| (id.clone(), NodeTransferStatus::new(id, total_bytes)))
            .collect();

        let total_targets = node_statuses.len();

        Self {
            handle,
            total_targets,
            completed: 0,
            in_progress: 0,
            failed: 0,
            node_statuses,
        }
    }

    /// Check if distribution is complete (all nodes done or failed)
    pub fn is_complete(&self) -> bool {
        self.completed + self.failed >= self.total_targets
    }

    /// Check if distribution succeeded (all targets completed)
    pub fn is_success(&self) -> bool {
        self.completed >= self.total_targets && self.failed == 0
    }

    /// Calculate overall progress fraction
    pub fn overall_progress(&self) -> f64 {
        if self.total_targets == 0 {
            return 1.0;
        }
        let total_bytes: u64 = self.node_statuses.values().map(|s| s.total_bytes).sum();
        let progress_bytes: u64 = self.node_statuses.values().map(|s| s.progress_bytes).sum();
        if total_bytes == 0 {
            return 1.0;
        }
        progress_bytes as f64 / total_bytes as f64
    }

    /// Recalculate counts from node statuses
    pub fn recalculate_counts(&mut self) {
        self.completed = 0;
        self.in_progress = 0;
        self.failed = 0;

        for status in self.node_statuses.values() {
            match status.status {
                TransferState::Completed => self.completed += 1,
                TransferState::Failed => self.failed += 1,
                TransferState::Transferring | TransferState::Connecting => self.in_progress += 1,
                TransferState::Pending => {}
            }
        }
    }
}

// ============================================================================
// FileDistribution Trait
// ============================================================================

/// File distribution service for targeted delivery
///
/// Provides higher-level API for distributing blobs to specific targets
/// with progress tracking and status monitoring.
#[async_trait::async_trait]
pub trait FileDistribution: Send + Sync {
    /// Distribute blob to target nodes
    ///
    /// Initiates distribution of a blob to nodes matching the scope.
    /// Returns a handle for tracking progress.
    ///
    /// # Distribution Behavior by Backend
    ///
    /// **Ditto**: Creates document with blob reference in a distribution
    /// collection. Target nodes subscribe to this collection and fetch
    /// the blob via attachment protocol when they see the reference.
    ///
    /// **Iroh**: Connects directly to target nodes and pushes blob.
    ///
    /// # Arguments
    ///
    /// * `blob_token` - Token identifying the blob to distribute
    /// * `scope` - Target scope (all nodes, formation, specific nodes, capable)
    /// * `priority` - Transfer priority level
    ///
    /// # Returns
    ///
    /// Handle for tracking distribution progress
    async fn distribute(
        &self,
        blob_token: &BlobToken,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<DistributionHandle>;

    /// Get current distribution status
    ///
    /// Returns the current status of all transfers in a distribution.
    async fn status(&self, handle: &DistributionHandle) -> Result<DistributionStatus>;

    /// Cancel an in-progress distribution
    ///
    /// Stops any pending or in-progress transfers. Completed transfers
    /// are not rolled back.
    async fn cancel(&self, handle: &DistributionHandle) -> Result<()>;

    /// Wait for distribution to complete (or fail)
    ///
    /// Blocks until all targets complete or the timeout expires.
    ///
    /// # Arguments
    ///
    /// * `handle` - Distribution handle
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    ///
    /// Final distribution status, or error if timeout or other failure
    async fn wait_for_completion(
        &self,
        handle: &DistributionHandle,
        timeout: Duration,
    ) -> Result<DistributionStatus>;

    /// Subscribe to distribution progress updates
    ///
    /// Returns a broadcast receiver that emits status updates as
    /// transfers progress.
    async fn subscribe_progress(
        &self,
        handle: &DistributionHandle,
    ) -> Result<broadcast::Receiver<DistributionStatus>>;
}

// ============================================================================
// IrohFileDistribution Implementation (Issue #379, ADR-025)
// ============================================================================

#[cfg(feature = "automerge-backend")]
use super::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use super::iroh_blob_store::NetworkedIrohBlobStore;

/// Distribution collection for Iroh backend
#[cfg(feature = "automerge-backend")]
const IROH_DISTRIBUTION_COLLECTION: &str = "file_distributions";

/// Iroh-based file distribution service
///
/// Distributes files/models using NetworkedIrohBlobStore with:
/// - Blob tokens stored in Automerge documents for discovery
/// - Direct P2P transfer via iroh-blobs protocol
/// - Status tracking via distribution documents
///
/// # Architecture
///
/// ```text
/// IrohFileDistribution
///     ├─ NetworkedIrohBlobStore (P2P blob transfer)
///     └─ AutomergeStore (distribution metadata sync)
///
/// Distribution Flow:
/// 1. Commander calls distribute(token, scope)
/// 2. Distribution document created in Automerge with blob token
/// 3. Document syncs to target nodes via CRDT sync
/// 4. Target nodes see distribution doc, fetch blob via iroh-blobs
/// 5. Target nodes update their status in distribution doc
/// ```
#[cfg(feature = "automerge-backend")]
pub struct IrohFileDistribution {
    /// Blob store for P2P file transfer
    blob_store: Arc<NetworkedIrohBlobStore>,
    /// Document store for distribution metadata
    document_store: Arc<AutomergeStore>,
    /// Active distributions (distribution_id -> status)
    distributions: RwLock<HashMap<String, DistributionStatus>>,
    /// Progress broadcast channels per distribution
    progress_channels: RwLock<HashMap<String, broadcast::Sender<DistributionStatus>>>,
}

#[cfg(feature = "automerge-backend")]
impl IrohFileDistribution {
    /// Create a new Iroh file distribution service
    pub fn new(
        blob_store: Arc<NetworkedIrohBlobStore>,
        document_store: Arc<AutomergeStore>,
    ) -> Self {
        Self {
            blob_store,
            document_store,
            distributions: RwLock::new(HashMap::new()),
            progress_channels: RwLock::new(HashMap::new()),
        }
    }

    /// Get the blob store reference
    pub fn blob_store(&self) -> &Arc<NetworkedIrohBlobStore> {
        &self.blob_store
    }

    /// Get the document store reference
    pub fn document_store(&self) -> &Arc<AutomergeStore> {
        &self.document_store
    }

    /// Resolve target nodes from scope
    ///
    /// For now, returns known peers from the blob store.
    /// In the future, could query node capabilities from Automerge documents.
    async fn resolve_targets(&self, scope: &DistributionScope) -> Vec<String> {
        match scope {
            DistributionScope::AllNodes => {
                // Return all known peers
                self.blob_store
                    .known_peers()
                    .await
                    .iter()
                    .map(|p| p.fmt_short().to_string())
                    .collect()
            }
            DistributionScope::Nodes { node_ids } => {
                // Return specified nodes (if they're known peers)
                let known_peers: Vec<String> = self
                    .blob_store
                    .known_peers()
                    .await
                    .iter()
                    .map(|p| p.fmt_short().to_string())
                    .collect();

                node_ids
                    .iter()
                    .filter(|id| known_peers.contains(id))
                    .cloned()
                    .collect()
            }
            DistributionScope::Formation { formation_id } => {
                // TODO: Query formation membership from Automerge documents
                // For now, return all known peers (formation filtering not yet implemented)
                warn!(
                    formation_id = %formation_id,
                    "Formation-based distribution not yet implemented, distributing to all peers"
                );
                self.blob_store
                    .known_peers()
                    .await
                    .iter()
                    .map(|p| p.fmt_short().to_string())
                    .collect()
            }
            DistributionScope::Capable { .. } => {
                // TODO: Query node capabilities from Automerge documents
                // For now, return all known peers (capability filtering not yet implemented)
                warn!(
                    "Capability-based distribution not yet implemented, distributing to all peers"
                );
                self.blob_store
                    .known_peers()
                    .await
                    .iter()
                    .map(|p| p.fmt_short().to_string())
                    .collect()
            }
        }
    }

    /// Store distribution metadata as Automerge document
    #[allow(unused_imports)]
    async fn store_distribution_document(
        &self,
        handle: &DistributionHandle,
        blob_token: &BlobToken,
        target_nodes: &[String],
    ) -> Result<()> {
        use super::traits::Collection;

        let doc_id = &handle.distribution_id;

        // Create distribution document
        let distribution_doc = serde_json::json!({
            "distribution_id": handle.distribution_id,
            "blob_hash": blob_token.hash.as_hex(),
            "blob_size": blob_token.size_bytes,
            "blob_metadata": blob_token.metadata,
            "scope": handle.scope,
            "priority": handle.priority,
            "target_nodes": target_nodes,
            "started_at": handle.started_at.to_rfc3339(),
            "status": "distributing"
        });

        // Serialize to bytes for storage
        let bytes = serde_json::to_vec(&distribution_doc)
            .map_err(|e| anyhow::anyhow!("Failed to serialize distribution doc: {}", e))?;

        // Store in Automerge via Collection trait - this will sync to peers via CRDT
        let collection = self.document_store.collection(IROH_DISTRIBUTION_COLLECTION);
        collection.upsert(doc_id, bytes)?;

        debug!(
            distribution_id = %handle.distribution_id,
            blob_hash = %blob_token.hash,
            target_count = target_nodes.len(),
            "Stored distribution document in Automerge"
        );

        Ok(())
    }

    /// Broadcast progress update to subscribers
    #[allow(dead_code)]
    async fn broadcast_progress(&self, distribution_id: &str, status: &DistributionStatus) {
        let channels = self.progress_channels.read().await;
        if let Some(sender) = channels.get(distribution_id) {
            // Ignore send errors (no subscribers)
            let _ = sender.send(status.clone());
        }
    }
}

#[cfg(feature = "automerge-backend")]
#[async_trait::async_trait]
impl FileDistribution for IrohFileDistribution {
    async fn distribute(
        &self,
        blob_token: &BlobToken,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<DistributionHandle> {
        info!(
            blob_hash = %blob_token.hash,
            blob_size = blob_token.size_bytes,
            scope = ?scope,
            priority = ?priority,
            "Starting file distribution"
        );

        // Create distribution handle
        let handle = DistributionHandle::new(blob_token.hash.clone(), scope.clone(), priority);

        // Resolve target nodes
        let target_nodes = self.resolve_targets(&scope).await;

        if target_nodes.is_empty() {
            warn!("No target nodes found for distribution scope");
        }

        // Create initial status
        let status =
            DistributionStatus::new(handle.clone(), target_nodes.clone(), blob_token.size_bytes);

        // Store distribution document (syncs to peers via Automerge)
        self.store_distribution_document(&handle, blob_token, &target_nodes)
            .await?;

        // Store status locally
        {
            let mut distributions = self.distributions.write().await;
            distributions.insert(handle.distribution_id.clone(), status.clone());
        }

        // Create progress channel
        {
            let (tx, _rx) = broadcast::channel(16);
            let mut channels = self.progress_channels.write().await;
            channels.insert(handle.distribution_id.clone(), tx);
        }

        info!(
            distribution_id = %handle.distribution_id,
            target_count = target_nodes.len(),
            "Distribution initiated - document synced to peers"
        );

        // Note: Actual blob transfer happens when target nodes:
        // 1. Receive the distribution document via Automerge sync
        // 2. See they are a target node
        // 3. Fetch the blob via NetworkedIrohBlobStore::fetch_blob()
        // 4. Update their status (not yet implemented - would require observer pattern)

        Ok(handle)
    }

    async fn status(&self, handle: &DistributionHandle) -> Result<DistributionStatus> {
        let distributions = self.distributions.read().await;
        distributions
            .get(&handle.distribution_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Distribution not found: {}", handle.distribution_id))
    }

    async fn cancel(&self, handle: &DistributionHandle) -> Result<()> {
        info!(
            distribution_id = %handle.distribution_id,
            "Cancelling distribution"
        );

        // Update status to cancelled
        {
            let mut distributions = self.distributions.write().await;
            if let Some(status) = distributions.get_mut(&handle.distribution_id) {
                // Mark all pending/in-progress as failed
                for node_status in status.node_statuses.values_mut() {
                    if node_status.status != TransferState::Completed {
                        node_status.status = TransferState::Failed;
                        node_status.error = Some("Distribution cancelled".to_string());
                    }
                }
                status.recalculate_counts();
            }
        }

        // Update distribution document
        #[allow(unused_imports)]
        use super::traits::Collection;

        let cancel_update = serde_json::json!({
            "status": "cancelled",
            "cancelled_at": Utc::now().to_rfc3339()
        });

        let bytes = serde_json::to_vec(&cancel_update)
            .map_err(|e| anyhow::anyhow!("Failed to serialize cancel update: {}", e))?;

        let collection = self.document_store.collection(IROH_DISTRIBUTION_COLLECTION);
        collection.upsert(&handle.distribution_id, bytes)?;

        Ok(())
    }

    async fn wait_for_completion(
        &self,
        handle: &DistributionHandle,
        timeout: Duration,
    ) -> Result<DistributionStatus> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(500);

        loop {
            let status = self.status(handle).await?;

            if status.is_complete() {
                return Ok(status);
            }

            if start.elapsed() >= timeout {
                return Err(anyhow::anyhow!("Distribution timeout after {:?}", timeout));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn subscribe_progress(
        &self,
        handle: &DistributionHandle,
    ) -> Result<broadcast::Receiver<DistributionStatus>> {
        let channels = self.progress_channels.read().await;
        channels
            .get(&handle.distribution_id)
            .map(|sender| sender.subscribe())
            .ok_or_else(|| anyhow::anyhow!("Distribution not found: {}", handle.distribution_id))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_priority_ordering() {
        assert!(TransferPriority::Critical.as_numeric() > TransferPriority::High.as_numeric());
        assert!(TransferPriority::High.as_numeric() > TransferPriority::Normal.as_numeric());
        assert!(TransferPriority::Normal.as_numeric() > TransferPriority::Low.as_numeric());
    }

    #[test]
    fn test_distribution_handle_creation() {
        let hash = BlobHash::from_hex("abc123");
        let scope = DistributionScope::AllNodes;
        let priority = TransferPriority::High;

        let handle = DistributionHandle::new(hash.clone(), scope, priority);

        assert!(!handle.distribution_id.is_empty());
        assert_eq!(handle.blob_hash, hash);
        assert_eq!(handle.priority, TransferPriority::High);
    }

    #[test]
    fn test_node_transfer_status() {
        let mut status = NodeTransferStatus::new("node-1".to_string(), 1000);

        assert_eq!(status.status, TransferState::Pending);
        assert_eq!(status.progress_fraction(), 0.0);

        status.progress_bytes = 500;
        status.status = TransferState::Transferring;
        assert_eq!(status.progress_fraction(), 0.5);

        status.progress_bytes = 1000;
        status.status = TransferState::Completed;
        assert_eq!(status.progress_fraction(), 1.0);
    }

    #[test]
    fn test_distribution_status() {
        let hash = BlobHash::from_hex("abc123");
        let handle =
            DistributionHandle::new(hash, DistributionScope::AllNodes, TransferPriority::Normal);
        let targets = vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ];

        let mut status = DistributionStatus::new(handle, targets, 1000);

        assert_eq!(status.total_targets, 3);
        assert_eq!(status.completed, 0);
        assert!(!status.is_complete());

        // Simulate completion
        if let Some(node_status) = status.node_statuses.get_mut("node-1") {
            node_status.status = TransferState::Completed;
            node_status.progress_bytes = 1000;
        }
        if let Some(node_status) = status.node_statuses.get_mut("node-2") {
            node_status.status = TransferState::Completed;
            node_status.progress_bytes = 1000;
        }
        if let Some(node_status) = status.node_statuses.get_mut("node-3") {
            node_status.status = TransferState::Failed;
            node_status.error = Some("Connection lost".to_string());
        }

        status.recalculate_counts();

        assert_eq!(status.completed, 2);
        assert_eq!(status.failed, 1);
        assert!(status.is_complete());
        assert!(!status.is_success());
    }

    #[test]
    fn test_distribution_scope_serialization() {
        let scope = DistributionScope::Capable {
            min_gpu_gb: Some(4.0),
            cpu_arch: Some("x86_64".to_string()),
            min_storage_mb: Some(1024),
        };

        let json = serde_json::to_string(&scope).unwrap();
        let restored: DistributionScope = serde_json::from_str(&json).unwrap();

        match restored {
            DistributionScope::Capable {
                min_gpu_gb,
                cpu_arch,
                min_storage_mb,
            } => {
                assert_eq!(min_gpu_gb, Some(4.0));
                assert_eq!(cpu_arch, Some("x86_64".to_string()));
                assert_eq!(min_storage_mb, Some(1024));
            }
            _ => panic!("Wrong variant"),
        }
    }
}
