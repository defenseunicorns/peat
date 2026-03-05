use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::{RegistryTarget, SyncIntent};

/// Top-level configuration loaded from TOML.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Local data directory for checkpoints and state.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Sync loop interval in seconds.
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,

    /// Drift detection interval in seconds.
    #[serde(default = "default_drift_interval")]
    pub drift_interval_secs: u64,

    /// Registry targets in the topology.
    #[serde(default)]
    pub targets: Vec<RegistryTarget>,

    /// Topology edges (parent-child relationships).
    #[serde(default)]
    pub edges: Vec<EdgeConfig>,

    /// Sync intents.
    #[serde(default)]
    pub intents: Vec<SyncIntent>,

    /// Transfer engine settings.
    #[serde(default)]
    pub transfer: TransferConfig,

    /// Wave controller settings.
    #[serde(default)]
    pub wave: WaveConfig,
}

/// An edge in the registry topology graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub parent_id: String,
    pub child_id: String,
    #[serde(default = "default_preference")]
    pub preference: u32,
    #[serde(default)]
    pub max_fanout: Option<usize>,
    #[serde(default)]
    pub bandwidth_budget_bytes_per_hour: Option<u64>,
}

/// Transfer engine configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferConfig {
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            max_concurrency: default_max_concurrency(),
            chunk_size: default_chunk_size(),
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
        }
    }
}

/// Wave controller configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaveConfig {
    /// Fraction of wave N targets that must converge before wave N+1 starts.
    #[serde(default = "default_gate_threshold")]
    pub gate_threshold: f64,
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self {
            gate_threshold: default_gate_threshold(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/data/peat-registry")
}
fn default_sync_interval() -> u64 {
    60
}
fn default_drift_interval() -> u64 {
    300
}
fn default_preference() -> u32 {
    1
}
fn default_max_concurrency() -> usize {
    4
}
fn default_chunk_size() -> usize {
    1024 * 1024 // 1 MiB
}
fn default_max_retries() -> u32 {
    5
}
fn default_retry_backoff_ms() -> u64 {
    2000
}
fn default_gate_threshold() -> f64 {
    0.8
}

impl RegistryConfig {
    pub fn load(path: &std::path::Path) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(crate::error::RegistryError::Io)?;
        toml::from_str(&content).map_err(|e| {
            crate::error::RegistryError::Config(format!("Failed to parse config: {e}"))
        })
    }
}
