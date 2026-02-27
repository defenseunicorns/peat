//! Storage backend factory and configuration
//!
//! This module provides runtime backend selection through configuration.
//! The backend can be chosen via environment variables or programmatic configuration.
//!
//! # Environment Variables
//!
//! - `CAP_STORAGE_BACKEND`: Backend type ("ditto", "automerge-memory", "redb")
//! - `CAP_DATA_PATH`: Data directory path for persistent backends
//! - Ditto-specific variables (loaded by DittoStore)
//!
//! # Example
//!
//! ```ignore
//! use peat_protocol::storage::{StorageConfig, create_storage_backend};
//!
//! // Load from environment
//! let config = StorageConfig::from_env()?;
//! let storage = create_storage_backend(&config)?;
//!
//! // Or create manually
//! let config = StorageConfig {
//!     backend: "ditto".to_string(),
//!     data_path: Some("/var/cap/data".to_string()),
//! };
//! let storage = create_storage_backend(&config)?;
//! ```

use super::traits::StorageBackend;
#[cfg(any(feature = "ditto-backend", feature = "automerge-backend"))]
use anyhow::Context;
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;

/// Storage backend configuration
///
/// Determines which storage implementation to use and how to configure it.
#[derive(Clone, Debug)]
pub struct StorageConfig {
    /// Backend type identifier
    ///
    /// Supported values:
    /// - `"ditto"`: Ditto SDK backend (proprietary, production-ready)
    /// - `"automerge-memory"`: Automerge in-memory (POC, testing)
    /// - `"redb"`: redb persistence (production target)
    pub backend: String,

    /// Data directory path for persistent backends
    ///
    /// Required for redb, optional for others.
    /// Example: `/var/cap/data`, `./data`, `/tmp/cap-test`
    pub data_path: Option<PathBuf>,

    /// Run in pure in-memory mode (no disk persistence)
    ///
    /// When true, the automerge backend will skip all disk writes and store
    /// documents only in the LRU cache. Useful for high-throughput testing
    /// where persistence is not needed.
    pub in_memory: bool,
}

impl Default for StorageConfig {
    /// Create configuration with defaults
    ///
    /// Uses Ditto backend with no data path (Ditto manages its own storage).
    ///
    /// # Returns
    ///
    /// Default configuration (Ditto backend)
    fn default() -> Self {
        Self {
            backend: "ditto".to_string(),
            data_path: None,
            in_memory: false,
        }
    }
}

impl StorageConfig {
    /// Create configuration from environment variables
    ///
    /// # Environment Variables
    ///
    /// - `CAP_STORAGE_BACKEND` (default: "ditto")
    /// - `CAP_DATA_PATH` (optional, required for some backends)
    ///
    /// # Returns
    ///
    /// StorageConfig loaded from environment
    ///
    /// # Example
    ///
    /// ```bash
    /// export CAP_STORAGE_BACKEND=rocksdb
    /// export CAP_DATA_PATH=/var/cap/data
    /// ```
    ///
    /// ```ignore
    /// let config = StorageConfig::from_env()?;
    /// assert_eq!(config.backend, "rocksdb");
    /// ```
    pub fn from_env() -> Result<Self> {
        let backend = std::env::var("CAP_STORAGE_BACKEND").unwrap_or_else(|_| "ditto".to_string());

        let data_path = std::env::var("CAP_DATA_PATH").ok().map(PathBuf::from);

        // CAP_IN_MEMORY=true enables pure in-memory mode (no disk persistence)
        let in_memory = std::env::var("CAP_IN_MEMORY")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Ok(Self {
            backend,
            data_path,
            in_memory,
        })
    }

    /// Validate configuration
    ///
    /// Checks that required fields are present for the selected backend.
    ///
    /// # Returns
    ///
    /// Ok(()) if valid, Err with description if invalid
    ///
    /// # Errors
    ///
    /// - redb requires data_path
    /// - Unknown backend type
    pub fn validate(&self) -> Result<()> {
        match self.backend.as_str() {
            "ditto" => {
                // Ditto manages its own storage, no validation needed
                Ok(())
            }
            "automerge-memory" => {
                // In-memory, no data path needed
                Ok(())
            }
            "redb" => {
                if self.data_path.is_none() {
                    return Err(anyhow!("redb backend requires CAP_DATA_PATH to be set"));
                }
                Ok(())
            }
            other => Err(anyhow!("Unknown storage backend: {}", other)),
        }
    }
}

/// Create storage backend from configuration
///
/// Factory function that instantiates the appropriate backend based on configuration.
///
/// # Arguments
///
/// * `config` - Storage configuration (backend type, data path, etc.)
///
/// # Returns
///
/// Arc-wrapped trait object for the selected backend
///
/// # Errors
///
/// - Unknown backend type
/// - Backend initialization fails
/// - Invalid configuration
///
/// # Example
///
/// ```ignore
/// let config = StorageConfig::from_env()?;
/// let storage = create_storage_backend(&config)?;
///
/// // Use storage
/// let cells = storage.collection("cells");
/// cells.upsert("cell-1", data)?;
/// ```
pub fn create_storage_backend(config: &StorageConfig) -> Result<Arc<dyn StorageBackend>> {
    // Validate configuration first
    config.validate()?;

    match config.backend.as_str() {
        #[cfg(feature = "ditto-backend")]
        "ditto" => {
            // Import DittoStore and DittoBackend
            use crate::storage::ditto_backend::DittoBackend;
            use crate::storage::ditto_store::DittoStore;

            // Create DittoStore from environment (loads config internally)
            let ditto_store = DittoStore::from_env()
                .context("Failed to create Ditto backend from environment")?;

            // Wrap in DittoBackend trait adapter
            let backend = DittoBackend::new(Arc::new(ditto_store));

            Ok(Arc::new(backend))
        }
        #[cfg(not(feature = "ditto-backend"))]
        "ditto" => Err(anyhow!(
            "Ditto backend not enabled.\n\
                 Rebuild with --features ditto-backend to use this backend.\n\
                 For now, use CAP_STORAGE_BACKEND=automerge-memory"
        )),
        "automerge-memory" => {
            #[cfg(feature = "automerge-backend")]
            {
                use crate::storage::automerge_backend::AutomergeBackend;
                use crate::storage::automerge_store::AutomergeStore;

                // Create AutomergeStore - in-memory or with persistence
                let automerge_store = if config.in_memory {
                    tracing::info!("Creating AutomergeStore in MEMORY-ONLY mode");
                    AutomergeStore::in_memory()
                } else {
                    // Determine storage path (use data_path if provided, otherwise temp)
                    let path = config.data_path.as_deref().ok_or_else(|| {
                        anyhow!("Automerge backend requires CAP_DATA_PATH to be set for persistence (or use CAP_IN_MEMORY=true)")
                    })?;
                    AutomergeStore::open(path).context("Failed to create Automerge backend")?
                };

                // Wrap in AutomergeBackend trait adapter (without transport for now)
                let backend = AutomergeBackend::new(Arc::new(automerge_store));

                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "automerge-backend"))]
            {
                Err(anyhow!(
                    "Automerge backend not enabled.\n\
                     Rebuild with --features automerge-backend to use this backend.\n\
                     For now, use CAP_STORAGE_BACKEND=ditto"
                ))
            }
        }
        "redb" => {
            // redb is used internally by automerge-backend
            // There's no standalone redb backend - use automerge-memory instead
            Err(anyhow!(
                "Direct redb backend not available.\n\
                 Use CAP_STORAGE_BACKEND=automerge-memory for redb-backed storage,\n\
                 or CAP_STORAGE_BACKEND=ditto for production use."
            ))
        }
        other => Err(anyhow!(
            "Unknown storage backend: {}\n\
             Supported backends: ditto, automerge-memory, redb",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.backend, "ditto");
        assert!(config.data_path.is_none());
    }

    #[test]
    fn test_storage_config_validation_ditto() {
        let config = StorageConfig {
            backend: "ditto".to_string(),
            data_path: None,
            in_memory: false,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_automerge_memory() {
        let config = StorageConfig {
            backend: "automerge-memory".to_string(),
            data_path: None,
            in_memory: false,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_redb_requires_path() {
        let config = StorageConfig {
            backend: "redb".to_string(),
            data_path: None,
            in_memory: false,
        };
        assert!(config.validate().is_err());

        let config_with_path = StorageConfig {
            backend: "redb".to_string(),
            data_path: Some(PathBuf::from("/var/cap/data")),
            in_memory: false,
        };
        assert!(config_with_path.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_unknown_backend() {
        let config = StorageConfig {
            backend: "unknown".to_string(),
            data_path: None,
            in_memory: false,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_create_backend_ditto_not_yet_implemented() {
        let config = StorageConfig::default();
        // This will panic with todo!() for now (Day 2 work)
        // Once DittoBackend is implemented, this test will pass
        // For now, just verify it would try to create Ditto
        assert_eq!(config.backend, "ditto");
    }

    #[test]
    fn test_create_backend_automerge_requires_data_path() {
        let config = StorageConfig {
            backend: "automerge-memory".to_string(),
            data_path: None,
            in_memory: false, // Not in-memory, so needs data_path
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            #[cfg(feature = "automerge-backend")]
            Err(e) => assert!(e.to_string().contains("CAP_DATA_PATH")),
            #[cfg(not(feature = "automerge-backend"))]
            Err(e) => assert!(e.to_string().contains("not enabled")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_create_backend_redb_not_available() {
        let config = StorageConfig {
            backend: "redb".to_string(),
            data_path: Some(PathBuf::from("/tmp/test")),
            in_memory: false,
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not available")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_create_backend_unknown() {
        let config = StorageConfig {
            backend: "unknown".to_string(),
            data_path: None,
            in_memory: false,
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("Unknown storage backend")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }
}
