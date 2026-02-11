//! Blob storage trait abstraction (ADR-025)
//!
//! This module defines traits for content-addressed blob storage, enabling
//! backend-agnostic file transfer through the mesh network.
//!
//! # Design Philosophy
//!
//! - **Separate from documents**: Blobs use different sync protocols optimized for large binaries
//! - **Content-addressed**: Blobs identified by cryptographic hash (SHA256 or BLAKE3)
//! - **Progress tracking**: Long transfers provide progress callbacks
//! - **Resumable**: Interrupted transfers can continue where they left off
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::storage::{BlobStore, BlobMetadata};
//! use std::path::Path;
//!
//! // Create blob from file
//! let metadata = BlobMetadata {
//!     name: Some("model.onnx".to_string()),
//!     content_type: Some("application/onnx".to_string()),
//!     ..Default::default()
//! };
//! let token = blob_store.create_blob(Path::new("/models/yolov8.onnx"), metadata).await?;
//! println!("Created blob: {}", token.hash.as_hex());
//!
//! // Fetch blob with progress
//! let handle = blob_store.fetch_blob(&token, |progress| {
//!     if let BlobProgress::Downloading { downloaded_bytes, total_bytes } = progress {
//!         println!("Progress: {}/{} bytes", downloaded_bytes, total_bytes);
//!     }
//! }).await?;
//! println!("Blob available at: {}", handle.path.display());
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ============================================================================
// Core Types
// ============================================================================

/// Content-addressed blob identifier
///
/// Blobs are identified by their cryptographic hash:
/// - Ditto uses SHA256
/// - iroh-blobs uses BLAKE3
///
/// The hash string format is backend-specific but always represents
/// the content hash of the blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobHash(pub String);

impl BlobHash {
    /// Create from hex string (sha256 or blake3)
    pub fn from_hex(hex: &str) -> Self {
        Self(hex.to_string())
    }

    /// Get hex representation
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show first 16 chars for readability
        if self.0.len() > 16 {
            write!(f, "{}...", &self.0[..16])
        } else {
            write!(f, "{}", self.0)
        }
    }
}

/// Token referencing a blob with metadata
///
/// A BlobToken uniquely identifies a blob and contains all information
/// needed to fetch it from the mesh. Tokens are serializable and can
/// be stored in CRDT documents to reference blob content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobToken {
    /// Content hash (sha256 for Ditto, blake3 for Iroh)
    pub hash: BlobHash,
    /// Size in bytes (known at creation time)
    pub size_bytes: u64,
    /// User-defined metadata
    pub metadata: BlobMetadata,
}

impl BlobToken {
    /// Create a new blob token
    pub fn new(hash: BlobHash, size_bytes: u64, metadata: BlobMetadata) -> Self {
        Self {
            hash,
            size_bytes,
            metadata,
        }
    }

    /// Check if this is a small blob (< 1MB)
    pub fn is_small(&self) -> bool {
        self.size_bytes < 1024 * 1024
    }

    /// Check if this is a large blob (> 100MB)
    pub fn is_large(&self) -> bool {
        self.size_bytes > 100 * 1024 * 1024
    }
}

/// Metadata attached to blobs
///
/// Metadata travels with the blob token and is available before
/// fetching the blob content. Use this for display names, MIME types,
/// and application-specific key-value pairs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlobMetadata {
    /// Human-readable name (e.g., "yolov8_fp16.onnx")
    pub name: Option<String>,
    /// MIME type (e.g., "application/onnx", "application/octet-stream")
    pub content_type: Option<String>,
    /// Custom key-value pairs for application-specific data
    ///
    /// Examples:
    /// - "model_id" -> "target_recognition"
    /// - "version" -> "4.2.1"
    /// - "precision" -> "fp16"
    pub custom: HashMap<String, String>,
}

impl BlobMetadata {
    /// Create metadata with just a name
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    /// Create metadata with name and content type
    pub fn with_name_and_type(name: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            content_type: Some(content_type.into()),
            custom: HashMap::new(),
        }
    }

    /// Add a custom metadata field
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.insert(key.into(), value.into());
        self
    }
}

/// Progress updates during blob operations
///
/// Callbacks receive these updates during long-running blob transfers.
/// Use for progress bars, logging, and timeout detection.
#[derive(Clone, Debug)]
pub enum BlobProgress {
    /// Transfer started, total size known
    Started {
        /// Total bytes to transfer
        total_bytes: u64,
    },
    /// Transfer in progress
    Downloading {
        /// Bytes downloaded so far
        downloaded_bytes: u64,
        /// Total bytes to download
        total_bytes: u64,
    },
    /// Transfer complete, blob available locally
    Completed {
        /// Local filesystem path to blob content
        local_path: PathBuf,
    },
    /// Transfer failed
    Failed {
        /// Error description
        error: String,
    },
}

impl BlobProgress {
    /// Get progress percentage (0.0 to 100.0)
    pub fn percentage(&self) -> Option<f64> {
        match self {
            BlobProgress::Started { .. } => Some(0.0),
            BlobProgress::Downloading {
                downloaded_bytes,
                total_bytes,
            } => {
                if *total_bytes == 0 {
                    Some(100.0)
                } else {
                    Some(*downloaded_bytes as f64 / *total_bytes as f64 * 100.0)
                }
            }
            BlobProgress::Completed { .. } => Some(100.0),
            BlobProgress::Failed { .. } => None,
        }
    }

    /// Check if transfer is complete
    pub fn is_complete(&self) -> bool {
        matches!(self, BlobProgress::Completed { .. })
    }

    /// Check if transfer failed
    pub fn is_failed(&self) -> bool {
        matches!(self, BlobProgress::Failed { .. })
    }
}

/// Handle to a locally available blob
///
/// Returned after successfully fetching a blob. Provides access to
/// the local file path where the blob content is stored.
#[derive(Debug)]
pub struct BlobHandle {
    /// Token identifying the blob
    pub token: BlobToken,
    /// Local filesystem path to blob content
    pub path: PathBuf,
}

impl BlobHandle {
    /// Create a new blob handle
    pub fn new(token: BlobToken, path: PathBuf) -> Self {
        Self { token, path }
    }

    /// Read the blob content into memory
    ///
    /// # Warning
    ///
    /// Only use for small blobs! For large blobs, stream from the path directly.
    pub async fn read_to_vec(&self) -> Result<Vec<u8>> {
        tokio::fs::read(&self.path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read blob at {:?}: {}", self.path, e))
    }

    /// Get the blob size in bytes
    pub fn size(&self) -> u64 {
        self.token.size_bytes
    }
}

// ============================================================================
// BlobStore Trait
// ============================================================================

/// Content-addressed blob storage trait
///
/// Abstracts over backend-specific blob storage (Ditto Attachments, iroh-blobs).
/// All blobs are content-addressed: the hash of the content serves as the ID.
///
/// # Thread Safety
///
/// All methods are safe to call from multiple threads. Implementations use
/// appropriate synchronization internally.
///
/// # Backend Differences
///
/// | Feature | Ditto | iroh-blobs |
/// |---------|-------|------------|
/// | Hash Algorithm | SHA256 | BLAKE3 |
/// | Metadata Storage | Native | External |
/// | Sync Protocol | Attachment protocol | iroh-blobs protocol |
/// | Garbage Collection | 10-minute TTL | Manual |
///
/// # Example
///
/// ```ignore
/// // Create blob from file
/// let token = blob_store.create_blob(
///     Path::new("/models/yolov8.onnx"),
///     BlobMetadata::with_name("yolov8.onnx")
/// ).await?;
///
/// // Share token with other nodes via CRDT document
/// doc.set("model_blob", &token)?;
///
/// // Other node fetches blob
/// let handle = blob_store.fetch_blob(&token, |p| println!("{:?}", p)).await?;
/// ```
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// Create a blob from a file
    ///
    /// Reads the file, computes content hash, and stores in blob storage.
    /// Returns a token that can be used to fetch the blob later.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to source file (must exist and be readable)
    /// * `metadata` - User-defined metadata to attach
    ///
    /// # Returns
    ///
    /// Token identifying the blob (content hash + size + metadata)
    ///
    /// # Errors
    ///
    /// - File not found or not readable
    /// - Backend storage failure
    async fn create_blob(&self, path: &Path, metadata: BlobMetadata) -> Result<BlobToken>;

    /// Create a blob from bytes
    ///
    /// Useful for generating blobs programmatically without writing to disk first.
    ///
    /// # Arguments
    ///
    /// * `data` - Raw blob content
    /// * `metadata` - User-defined metadata to attach
    ///
    /// # Returns
    ///
    /// Token identifying the blob
    async fn create_blob_from_bytes(
        &self,
        data: &[u8],
        metadata: BlobMetadata,
    ) -> Result<BlobToken>;

    /// Fetch a blob with progress tracking
    ///
    /// If the blob exists locally, returns immediately with the local path.
    /// Otherwise, fetches from mesh peers via the backend's sync protocol.
    ///
    /// # Arguments
    ///
    /// * `token` - Token identifying the blob to fetch
    /// * `progress` - Callback invoked with progress updates
    ///
    /// # Returns
    ///
    /// Handle providing local path to blob content
    ///
    /// # Errors
    ///
    /// - Blob not found on any peer
    /// - Network failure
    /// - Timeout
    async fn fetch_blob<F>(&self, token: &BlobToken, progress: F) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static;

    /// Check if blob exists locally
    ///
    /// Returns true if the blob is available locally without network fetch.
    /// Use this to avoid unnecessary network requests.
    fn blob_exists_locally(&self, hash: &BlobHash) -> bool;

    /// Get blob info without fetching content
    ///
    /// Returns metadata about a known blob, or None if unknown.
    /// Does not trigger network fetch.
    fn blob_info(&self, hash: &BlobHash) -> Option<BlobToken>;

    /// Delete a local blob
    ///
    /// Removes the blob from local storage. Does not affect other peers.
    ///
    /// # Warning
    ///
    /// If the blob is referenced by documents, garbage collection may recreate it
    /// when those documents sync. Use with caution.
    async fn delete_blob(&self, hash: &BlobHash) -> Result<()>;

    /// List all locally available blobs
    ///
    /// Returns tokens for all blobs stored locally. Does not include
    /// blobs available only on remote peers.
    fn list_local_blobs(&self) -> Vec<BlobToken>;

    /// Get total size of local blob storage in bytes
    fn local_storage_bytes(&self) -> u64;
}

// ============================================================================
// BlobStoreExt - Extension Trait
// ============================================================================

/// Extension methods for BlobStore
///
/// Provides convenience methods built on top of the core BlobStore trait.
#[async_trait::async_trait]
pub trait BlobStoreExt: BlobStore {
    /// Fetch blob without progress callback
    ///
    /// Convenience method when progress tracking isn't needed.
    async fn fetch_blob_simple(&self, token: &BlobToken) -> Result<BlobHandle> {
        self.fetch_blob(token, |_| {}).await
    }

    /// Ensure blob is available locally, fetching if needed
    ///
    /// Returns the local path if already present, otherwise fetches.
    async fn ensure_local(&self, token: &BlobToken) -> Result<PathBuf> {
        if self.blob_exists_locally(&token.hash) {
            if let Some(info) = self.blob_info(&token.hash) {
                // Blob exists locally, but we need the path
                // This is a limitation - we'd need the handle
                // For now, just fetch (which should be instant if local)
                let handle = self
                    .fetch_blob_simple(&BlobToken {
                        hash: info.hash,
                        size_bytes: info.size_bytes,
                        metadata: token.metadata.clone(),
                    })
                    .await?;
                return Ok(handle.path);
            }
        }
        let handle = self.fetch_blob_simple(token).await?;
        Ok(handle.path)
    }

    /// Get storage usage summary
    fn storage_summary(&self) -> BlobStorageSummary {
        let blobs = self.list_local_blobs();
        BlobStorageSummary {
            blob_count: blobs.len(),
            total_bytes: self.local_storage_bytes(),
            largest_blob: blobs.iter().map(|t| t.size_bytes).max(),
        }
    }
}

/// Storage usage summary
#[derive(Debug, Clone)]
pub struct BlobStorageSummary {
    /// Number of blobs stored locally
    pub blob_count: usize,
    /// Total bytes used
    pub total_bytes: u64,
    /// Size of largest blob (if any)
    pub largest_blob: Option<u64>,
}

// Blanket implementation of BlobStoreExt for all BlobStore implementations
impl<T: BlobStore + ?Sized> BlobStoreExt for T {}

// ============================================================================
// Type Aliases
// ============================================================================

/// Arc-wrapped BlobStore for shared ownership
pub type SharedBlobStore = Arc<dyn BlobStore>;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_hash_display() {
        let hash = BlobHash::from_hex("a7f8b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0");
        assert_eq!(format!("{}", hash), "a7f8b3c4d5e6f7a8...");

        let short_hash = BlobHash::from_hex("abc123");
        assert_eq!(format!("{}", short_hash), "abc123");
    }

    #[test]
    fn test_blob_token_size_classification() {
        let small = BlobToken::new(
            BlobHash::from_hex("abc"),
            500 * 1024, // 500KB
            BlobMetadata::default(),
        );
        assert!(small.is_small());
        assert!(!small.is_large());

        let large = BlobToken::new(
            BlobHash::from_hex("def"),
            200 * 1024 * 1024, // 200MB
            BlobMetadata::default(),
        );
        assert!(!large.is_small());
        assert!(large.is_large());
    }

    #[test]
    fn test_blob_metadata_builder() {
        let meta = BlobMetadata::with_name("model.onnx")
            .with_custom("version", "1.0")
            .with_custom("precision", "fp16");

        assert_eq!(meta.name, Some("model.onnx".to_string()));
        assert_eq!(meta.custom.get("version"), Some(&"1.0".to_string()));
        assert_eq!(meta.custom.get("precision"), Some(&"fp16".to_string()));
    }

    #[test]
    fn test_blob_progress_percentage() {
        let started = BlobProgress::Started { total_bytes: 1000 };
        assert_eq!(started.percentage(), Some(0.0));

        let downloading = BlobProgress::Downloading {
            downloaded_bytes: 500,
            total_bytes: 1000,
        };
        assert_eq!(downloading.percentage(), Some(50.0));

        let completed = BlobProgress::Completed {
            local_path: PathBuf::from("/tmp/blob"),
        };
        assert_eq!(completed.percentage(), Some(100.0));

        let failed = BlobProgress::Failed {
            error: "oops".to_string(),
        };
        assert_eq!(failed.percentage(), None);
    }

    #[test]
    fn test_blob_token_serialization() {
        let token = BlobToken::new(
            BlobHash::from_hex("a7f8b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0"),
            1024 * 1024,
            BlobMetadata::with_name_and_type("model.onnx", "application/onnx"),
        );

        let json = serde_json::to_string(&token).unwrap();
        let parsed: BlobToken = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.hash, token.hash);
        assert_eq!(parsed.size_bytes, token.size_bytes);
        assert_eq!(parsed.metadata.name, token.metadata.name);
    }
}
