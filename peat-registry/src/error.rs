use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("OCI registry error: {0}")]
    Oci(String),

    #[error("Authentication failed for registry {registry}: {reason}")]
    Auth { registry: String, reason: String },

    #[error("Blob not found: {digest} in {repository}")]
    BlobNotFound { repository: String, digest: String },

    #[error("Manifest not found: {reference} in {repository}")]
    ManifestNotFound {
        repository: String,
        reference: String,
    },

    #[error("Transfer failed for {digest}: {reason}")]
    Transfer { digest: String, reason: String },

    #[error("Transfer interrupted: {0}")]
    TransferInterrupted(String),

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),

    #[error("Budget exhausted on edge {edge}")]
    BudgetExhausted { edge: String },

    #[error("Wave {wave} not active (gate threshold not met)")]
    WaveNotActive { wave: u32 },

    #[error("Referrer gate failed for {digest}: missing artifact types {missing:?}")]
    ReferrerGateFailed {
        digest: String,
        missing: Vec<String>,
    },

    #[error("Topology error: {0}")]
    Topology(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Convergence error: {0}")]
    Convergence(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, RegistryError>;
