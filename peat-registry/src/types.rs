use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// OCI registry endpoint with tier and auth configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistryTarget {
    pub id: String,
    pub endpoint: String,
    pub tier: RegistryTier,
    pub auth: RegistryAuth,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Topology tier — determines default sync direction and priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RegistryTier {
    /// Tier 0 — enterprise data center, source of truth.
    Enterprise = 0,
    /// Tier 1 — regional hub, caches from enterprise.
    Regional = 1,
    /// Tier 2 — tactical node, intermittent connectivity.
    Tactical = 2,
    /// Tier 3 — edge device, minimal storage.
    Edge = 3,
}

/// Authentication credentials for an OCI registry.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RegistryAuth {
    Anonymous,
    Basic { username: String, password: String },
    Bearer { token: String },
}

impl std::fmt::Debug for RegistryAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryAuth::Anonymous => f.debug_struct("Anonymous").finish(),
            RegistryAuth::Basic { username, .. } => f
                .debug_struct("Basic")
                .field("username", username)
                .field("password", &"[REDACTED]")
                .finish(),
            RegistryAuth::Bearer { .. } => f
                .debug_struct("Bearer")
                .field("token", &"[REDACTED]")
                .finish(),
        }
    }
}

impl RegistryAuth {
    /// Convert to oci_client::secrets::RegistryAuth.
    pub fn to_oci_auth(&self) -> oci_client::secrets::RegistryAuth {
        match self {
            RegistryAuth::Anonymous => oci_client::secrets::RegistryAuth::Anonymous,
            RegistryAuth::Basic { username, password } => {
                oci_client::secrets::RegistryAuth::Basic(username.clone(), password.clone())
            }
            RegistryAuth::Bearer { token } => {
                oci_client::secrets::RegistryAuth::Bearer(token.clone())
            }
        }
    }
}

/// Set of content digests and tags for a collection of repositories.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DigestSet {
    /// Manifest digests → size in bytes.
    pub manifests: HashMap<String, u64>,
    /// Blob (layer/config) digests → size in bytes.
    pub blobs: HashMap<String, u64>,
    /// Tag → manifest digest mappings.
    pub tags: HashMap<String, String>,
    /// Subject digest → list of referrer manifest digests.
    pub referrers: HashMap<String, Vec<String>>,
}

impl DigestSet {
    pub fn total_bytes(&self) -> u64 {
        let manifest_bytes: u64 = self.manifests.values().sum();
        let blob_bytes: u64 = self.blobs.values().sum();
        manifest_bytes + blob_bytes
    }

    pub fn total_items(&self) -> usize {
        self.manifests.len() + self.blobs.len()
    }
}

/// Delta between a source and target registry's content.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DigestDelta {
    /// Manifest digests missing from target → size.
    pub missing_manifests: HashMap<String, u64>,
    /// Blob digests missing from target → size.
    pub missing_blobs: HashMap<String, u64>,
    /// Tags missing from target (tag → digest).
    pub missing_tags: HashMap<String, String>,
    /// Referrer digests missing from target (subject → referrer digests).
    pub missing_referrers: HashMap<String, Vec<String>>,
    /// Total bytes to transfer.
    pub total_transfer_bytes: u64,
}

impl DigestDelta {
    pub fn is_empty(&self) -> bool {
        self.missing_manifests.is_empty()
            && self.missing_blobs.is_empty()
            && self.missing_tags.is_empty()
            && self.missing_referrers.is_empty()
    }

    pub fn total_items(&self) -> usize {
        self.missing_manifests.len() + self.missing_blobs.len()
    }
}

/// Declarative sync request from operator or formation leader.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncIntent {
    pub intent_id: String,
    pub source: String,
    pub targets: Vec<String>,
    pub repositories: Vec<String>,
    pub policy_class: DdilPolicyClass,
    pub priority: SyncPriority,
    pub wave: u32,
    #[serde(default)]
    pub require_referrers: Vec<String>,
    #[serde(default)]
    pub pin_digests: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Maps to existing QoSClass and TransferPriority in peat-protocol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DdilPolicyClass {
    /// ROE, safety-critical images — preempts all.
    MissionCritical,
    /// Operational workloads — bounded concurrency.
    #[default]
    MissionSupport,
    /// Non-urgent, fill bandwidth — preemptable.
    Background,
}

impl DdilPolicyClass {
    pub fn params(&self) -> PolicyParams {
        match self {
            DdilPolicyClass::MissionCritical => PolicyParams {
                max_concurrency: 8,
                max_retries: 10,
                retry_backoff_ms: 500,
                bandwidth_reservation_pct: 80,
                preemptable: false,
            },
            DdilPolicyClass::MissionSupport => PolicyParams {
                max_concurrency: 4,
                max_retries: 5,
                retry_backoff_ms: 2000,
                bandwidth_reservation_pct: 50,
                preemptable: false,
            },
            DdilPolicyClass::Background => PolicyParams {
                max_concurrency: 2,
                max_retries: 3,
                retry_backoff_ms: 5000,
                bandwidth_reservation_pct: 20,
                preemptable: true,
            },
        }
    }
}

/// Parameters derived from a DDIL policy class.
#[derive(Clone, Debug)]
pub struct PolicyParams {
    pub max_concurrency: usize,
    pub max_retries: u32,
    pub retry_backoff_ms: u64,
    pub bandwidth_reservation_pct: u32,
    pub preemptable: bool,
}

/// Sync priority — determines ordering within a policy class.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SyncPriority {
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Per-target convergence state for a sync intent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetConvergenceState {
    pub target_id: String,
    pub intent_id: String,
    pub status: ConvergenceStatus,
    pub remaining_delta: Option<DigestDelta>,
    pub active_checkpoint: Option<String>,
    pub blockers: Vec<ConvergenceBlocker>,
    pub last_updated: DateTime<Utc>,
}

/// State machine for convergence tracking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConvergenceStatus {
    #[default]
    Unknown,
    Pending,
    InProgress,
    /// All content transferred but referrer gates not yet passed.
    ContentComplete,
    /// Fully converged — content + referrers verified.
    Converged,
    /// Was converged but source has new content.
    Drifted,
    Failed,
}

impl ConvergenceStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ConvergenceStatus::Converged | ConvergenceStatus::Failed
        )
    }
}

/// A blocker preventing convergence of a specific target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConvergenceBlocker {
    pub target_id: String,
    pub reason: ConvergenceBlockerReason,
    pub since: DateTime<Utc>,
    pub details: Option<String>,
}

/// Typed reason for convergence being blocked.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConvergenceBlockerReason {
    NetworkUnavailable,
    TransferFailed,
    ReferrerMissing,
    TargetRejected,
    BudgetExhausted,
    WaveNotActive,
}

/// Aggregated status across all targets for a sync intent.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncAggregatedStatus {
    pub intent_id: String,
    pub total_targets: usize,
    pub converged: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub failed: usize,
    pub drifted: usize,
}

/// Information about a pulled manifest.
#[derive(Clone, Debug)]
pub struct ManifestInfo {
    pub content: bytes::Bytes,
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    /// Layer digests referenced by this manifest.
    pub layer_digests: Vec<(String, u64)>,
    /// Config digest if present.
    pub config_digest: Option<(String, u64)>,
}

/// A page of tags from the registry.
#[derive(Clone, Debug)]
pub struct TagPage {
    pub tags: Vec<String>,
}

/// Information about a referrer (OCI 1.1 referrers API).
#[derive(Clone, Debug)]
pub struct ReferrerInfo {
    pub digest: String,
    pub artifact_type: String,
    pub size: u64,
}

/// Result of checking referrer gates.
#[derive(Clone, Debug)]
pub struct ReferrerGateResult {
    pub passed: bool,
    pub present_types: HashSet<String>,
    pub missing_types: Vec<String>,
}
