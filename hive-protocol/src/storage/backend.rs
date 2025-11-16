//! Storage backend factory and configuration
//!
//! This module provides runtime backend selection through configuration.
//! The backend can be chosen via environment variables or programmatic configuration.
//!
//! # Environment Variables
//!
//! - `CAP_STORAGE_BACKEND`: Backend type ("ditto", "automerge-memory", "rocksdb")
//! - `CAP_DATA_PATH`: Data directory path for persistent backends
//! - Ditto-specific variables (loaded by DittoStore)
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::storage::{StorageConfig, create_storage_backend};
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
use anyhow::{anyhow, Context, Result};
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
    /// - `"rocksdb"`: RocksDB persistence (production target)
    pub backend: String,

    /// Data directory path for persistent backends
    ///
    /// Required for RocksDB, optional for others.
    /// Example: `/var/cap/data`, `./data`, `/tmp/cap-test`
    pub data_path: Option<PathBuf>,
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

        Ok(Self { backend, data_path })
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
    /// - RocksDB requires data_path
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
            "rocksdb" => {
                if self.data_path.is_none() {
                    return Err(anyhow!("RocksDB backend requires CAP_DATA_PATH to be set"));
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
        "ditto" => {
            // Import DittoStore (existing implementation)
            use crate::storage::ditto_store::DittoStore;

            // Create DittoStore from environment (loads config internally)
            let _ditto = DittoStore::from_env()
                .context("Failed to create Ditto backend from environment")?;

            // Wrap in Arc (we'll create DittoBackend trait wrapper in Day 2)
            // For now, this won't compile - placeholder for Day 2 work
            todo!("DittoBackend trait wrapper not yet implemented (Day 2)")
        }
        "automerge-memory" => {
            // Will implement in E11.2 Week 2 (after RocksDB)
            Err(anyhow!(
                "Automerge in-memory backend not yet implemented.\n\
                 This will be available in E11.2 Week 2.\n\
                 For now, use CAP_STORAGE_BACKEND=ditto"
            ))
        }
        "rocksdb" => {
            // Will implement in E11.2 Week 2
            Err(anyhow!(
                "RocksDB backend not yet implemented.\n\
                 This will be available in E11.2 Week 2.\n\
                 For now, use CAP_STORAGE_BACKEND=ditto"
            ))
        }
        other => Err(anyhow!(
            "Unknown storage backend: {}\n\
             Supported backends: ditto, automerge-memory, rocksdb",
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
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_automerge_memory() {
        let config = StorageConfig {
            backend: "automerge-memory".to_string(),
            data_path: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_rocksdb_requires_path() {
        let config = StorageConfig {
            backend: "rocksdb".to_string(),
            data_path: None,
        };
        assert!(config.validate().is_err());

        let config_with_path = StorageConfig {
            backend: "rocksdb".to_string(),
            data_path: Some(PathBuf::from("/var/cap/data")),
        };
        assert!(config_with_path.validate().is_ok());
    }

    #[test]
    fn test_storage_config_validation_unknown_backend() {
        let config = StorageConfig {
            backend: "unknown".to_string(),
            data_path: None,
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
    fn test_create_backend_automerge_not_implemented() {
        let config = StorageConfig {
            backend: "automerge-memory".to_string(),
            data_path: None,
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not yet implemented")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_create_backend_rocksdb_not_implemented() {
        let config = StorageConfig {
            backend: "rocksdb".to_string(),
            data_path: Some(PathBuf::from("/tmp/test")),
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("not yet implemented")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_create_backend_unknown() {
        let config = StorageConfig {
            backend: "unknown".to_string(),
            data_path: None,
        };
        let result = create_storage_backend(&config);
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("Unknown storage backend")),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }
}
