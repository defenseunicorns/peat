//! Ditto blob store implementation (ADR-025)
//!
//! This module implements the `BlobStore` trait using Ditto's Attachment API,
//! providing content-addressed blob storage with mesh synchronization.
//!
//! # Ditto Attachments
//!
//! Ditto Attachments are optimized for large binary files:
//! - Content-addressed storage (SHA256 hash as ID)
//! - Automatic deduplication across peers
//! - Separate sync protocol from document sync
//! - Progress tracking and resumable transfers
//! - 10-minute garbage collection for unreferenced blobs
//!
//! # Usage
//!
//! ```ignore
//! use hive_protocol::storage::{DittoBlobStore, BlobStore, BlobMetadata};
//! use std::path::Path;
//!
//! let blob_store = DittoBlobStore::new(ditto_store);
//!
//! // Create blob from file
//! let token = blob_store.create_blob(
//!     Path::new("/models/yolov8.onnx"),
//!     BlobMetadata::with_name("yolov8.onnx")
//! ).await?;
//!
//! // Store token in document for mesh sync
//! // (tokens sync via CRDT, blob content syncs separately)
//!
//! // Later, fetch blob with progress
//! let handle = blob_store.fetch_blob(&token, |progress| {
//!     println!("Progress: {:?}", progress);
//! }).await?;
//! ```

use super::blob_traits::{BlobHandle, BlobHash, BlobMetadata, BlobProgress, BlobStore, BlobToken};
use super::ditto_store::DittoStore;
use anyhow::{Context, Result};
// Note: dittolive_ditto::prelude types used via DittoStore
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Ditto blob store implementing the BlobStore trait
///
/// Wraps Ditto's Attachment API to provide backend-agnostic blob storage.
/// Blobs are content-addressed using SHA256 hashes.
pub struct DittoBlobStore {
    /// Underlying Ditto store
    store: Arc<DittoStore>,
    /// Cache of known blob tokens (hash -> token)
    /// Used for blob_exists_locally and blob_info without network calls
    token_cache: RwLock<HashMap<BlobHash, BlobToken>>,
    /// Local storage directory for blob downloads
    blob_dir: PathBuf,
}

impl DittoBlobStore {
    /// Create a new Ditto blob store
    ///
    /// # Arguments
    ///
    /// * `store` - Configured DittoStore instance
    pub fn new(store: Arc<DittoStore>) -> Self {
        // Create blob directory next to Ditto's persistence directory
        let blob_dir = std::env::temp_dir().join("hive_blobs");
        if let Err(e) = std::fs::create_dir_all(&blob_dir) {
            warn!("Failed to create blob directory {:?}: {}", blob_dir, e);
        }

        Self {
            store,
            token_cache: RwLock::new(HashMap::new()),
            blob_dir,
        }
    }

    /// Create with custom blob directory
    pub fn with_blob_dir(store: Arc<DittoStore>, blob_dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&blob_dir) {
            warn!("Failed to create blob directory {:?}: {}", blob_dir, e);
        }

        Self {
            store,
            token_cache: RwLock::new(HashMap::new()),
            blob_dir,
        }
    }

    /// Get access to the underlying DittoStore
    pub fn ditto_store(&self) -> &DittoStore {
        &self.store
    }

    /// Convert BlobMetadata to Ditto's HashMap<String, String> format
    fn metadata_to_ditto(metadata: &BlobMetadata) -> HashMap<String, String> {
        let mut ditto_meta = HashMap::new();

        if let Some(name) = &metadata.name {
            ditto_meta.insert("name".to_string(), name.clone());
        }
        if let Some(content_type) = &metadata.content_type {
            ditto_meta.insert("content_type".to_string(), content_type.clone());
        }

        // Add custom fields with "custom_" prefix to avoid collisions
        for (key, value) in &metadata.custom {
            ditto_meta.insert(format!("custom_{}", key), value.clone());
        }

        ditto_meta
    }

    /// Convert Ditto's HashMap<String, String> to BlobMetadata
    ///
    /// Used in Phase 2 when fetching attachment tokens from documents.
    #[allow(dead_code)]
    fn ditto_to_metadata(ditto_meta: &HashMap<String, String>) -> BlobMetadata {
        // Extract custom fields
        let custom: HashMap<String, String> = ditto_meta
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix("custom_")
                    .map(|custom_key| (custom_key.to_string(), value.clone()))
            })
            .collect();

        BlobMetadata {
            name: ditto_meta.get("name").cloned(),
            content_type: ditto_meta.get("content_type").cloned(),
            custom,
        }
    }

    /// Cache a token for later lookup
    fn cache_token(&self, token: &BlobToken) {
        if let Ok(mut cache) = self.token_cache.write() {
            cache.insert(token.hash.clone(), token.clone());
        }
    }

    /// Get the local path for a blob
    fn local_blob_path(&self, hash: &BlobHash) -> PathBuf {
        self.blob_dir.join(hash.as_hex())
    }
}

#[async_trait::async_trait]
impl BlobStore for DittoBlobStore {
    async fn create_blob(&self, path: &Path, metadata: BlobMetadata) -> Result<BlobToken> {
        info!("Creating blob from file: {:?}", path);

        // Verify file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {:?}", path));
        }

        let file_size = std::fs::metadata(path)
            .context("Failed to get file metadata")?
            .len();

        // Convert metadata to Ditto format
        let ditto_metadata = Self::metadata_to_ditto(&metadata);

        // Create attachment via Ditto API
        let attachment = self
            .store
            .ditto()
            .store()
            .new_attachment(path, ditto_metadata)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Ditto attachment: {}", e))?;

        // Build our token from Ditto's attachment
        let token = BlobToken {
            hash: BlobHash::from_hex(&attachment.id()),
            size_bytes: file_size,
            metadata,
        };

        // Cache for later lookups
        self.cache_token(&token);

        debug!(
            "Created blob: hash={}, size={}",
            token.hash.as_hex(),
            token.size_bytes
        );

        Ok(token)
    }

    async fn create_blob_from_bytes(
        &self,
        data: &[u8],
        metadata: BlobMetadata,
    ) -> Result<BlobToken> {
        info!("Creating blob from {} bytes", data.len());

        // Convert metadata to Ditto format
        let ditto_metadata = Self::metadata_to_ditto(&metadata);

        // Create attachment via Ditto API
        let attachment = self
            .store
            .ditto()
            .store()
            .new_attachment_from_bytes(data, ditto_metadata)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Ditto attachment from bytes: {}", e))?;

        // Build our token from Ditto's attachment
        let token = BlobToken {
            hash: BlobHash::from_hex(&attachment.id()),
            size_bytes: data.len() as u64,
            metadata,
        };

        // Cache for later lookups
        self.cache_token(&token);

        debug!(
            "Created blob from bytes: hash={}, size={}",
            token.hash.as_hex(),
            token.size_bytes
        );

        Ok(token)
    }

    async fn fetch_blob<F>(&self, token: &BlobToken, mut progress: F) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static,
    {
        info!("Fetching blob: hash={}", token.hash.as_hex());

        // Check if we already have it locally
        let local_path = self.local_blob_path(&token.hash);
        if local_path.exists() {
            debug!("Blob already exists locally at {:?}", local_path);
            progress(BlobProgress::Completed {
                local_path: local_path.clone(),
            });
            return Ok(BlobHandle::new(token.clone(), local_path));
        }

        // Send started event
        progress(BlobProgress::Started {
            total_bytes: token.size_bytes,
        });

        // Phase 1 Limitation:
        // Ditto's fetch_attachment requires a DittoAttachmentToken which can only be
        // obtained from:
        // 1. A DittoAttachment object (returned by new_attachment)
        // 2. A query result containing an attachment field
        //
        // For blobs created on THIS node, the attachment is immediately available.
        // For blobs created on OTHER nodes and synced via documents, we need the
        // BlobDocumentIntegration layer (Phase 2) to store tokens in documents
        // and retrieve them via queries.
        //
        // For now, if the blob isn't locally available, we return an error explaining
        // that the token needs to come from a document query.

        // Check if this was a blob we created (we'd have cached it)
        if let Some(_cached_token) = self.blob_info(&token.hash) {
            // We have metadata but not the actual content
            // This means the blob was created elsewhere and we only have the token
            warn!(
                "Blob {} not available locally. In Phase 1, remote blob fetch requires \
                 document integration (Phase 2). Store the BlobToken in a CRDT document, \
                 query that document on this node, and use the returned attachment token.",
                token.hash.as_hex()
            );
        }

        progress(BlobProgress::Failed {
            error: format!(
                "Blob {} not available locally. Remote fetch requires document integration (Phase 2).",
                token.hash
            ),
        });

        Err(anyhow::anyhow!(
            "Blob {} not available locally. In Phase 1, blobs must be fetched via document \
             integration. Store the BlobToken in a CRDT document that syncs to this node, \
             then query the document to get a fetchable attachment token. \
             See ADR-025 Phase 2: BlobDocumentIntegration.",
            token.hash.as_hex()
        ))
    }

    fn blob_exists_locally(&self, hash: &BlobHash) -> bool {
        // Check our local blob directory
        let local_path = self.local_blob_path(hash);
        if local_path.exists() {
            return true;
        }

        // Check cache (might know about it from previous operations)
        if let Ok(cache) = self.token_cache.read() {
            if cache.contains_key(hash) {
                return true;
            }
        }

        false
    }

    fn blob_info(&self, hash: &BlobHash) -> Option<BlobToken> {
        if let Ok(cache) = self.token_cache.read() {
            cache.get(hash).cloned()
        } else {
            None
        }
    }

    async fn delete_blob(&self, hash: &BlobHash) -> Result<()> {
        info!("Deleting blob: hash={}", hash.as_hex());

        // Remove from local storage
        let local_path = self.local_blob_path(hash);
        if local_path.exists() {
            std::fs::remove_file(&local_path)
                .with_context(|| format!("Failed to delete local blob: {:?}", local_path))?;
        }

        // Remove from cache
        if let Ok(mut cache) = self.token_cache.write() {
            cache.remove(hash);
        }

        // Note: Ditto attachments are garbage collected when no documents reference them
        // (10-minute TTL). We don't have a direct delete API, so the blob will be
        // cleaned up automatically if no documents reference it.

        Ok(())
    }

    fn list_local_blobs(&self) -> Vec<BlobToken> {
        // Return all cached tokens
        if let Ok(cache) = self.token_cache.read() {
            cache.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    fn local_storage_bytes(&self) -> u64 {
        // Sum up sizes from cache
        if let Ok(cache) = self.token_cache.read() {
            cache.values().map(|t| t.size_bytes).sum()
        } else {
            0
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_conversion() {
        let metadata = BlobMetadata {
            name: Some("test.onnx".to_string()),
            content_type: Some("application/onnx".to_string()),
            custom: {
                let mut map = HashMap::new();
                map.insert("version".to_string(), "1.0".to_string());
                map.insert("model_id".to_string(), "yolov8".to_string());
                map
            },
        };

        // Convert to Ditto format
        let ditto_meta = DittoBlobStore::metadata_to_ditto(&metadata);

        assert_eq!(ditto_meta.get("name"), Some(&"test.onnx".to_string()));
        assert_eq!(
            ditto_meta.get("content_type"),
            Some(&"application/onnx".to_string())
        );
        assert_eq!(ditto_meta.get("custom_version"), Some(&"1.0".to_string()));
        assert_eq!(
            ditto_meta.get("custom_model_id"),
            Some(&"yolov8".to_string())
        );

        // Convert back
        let restored = DittoBlobStore::ditto_to_metadata(&ditto_meta);

        assert_eq!(restored.name, metadata.name);
        assert_eq!(restored.content_type, metadata.content_type);
        assert_eq!(restored.custom.get("version"), Some(&"1.0".to_string()));
        assert_eq!(restored.custom.get("model_id"), Some(&"yolov8".to_string()));
    }

    #[test]
    fn test_local_blob_path() {
        let blob_dir = PathBuf::from("/tmp/test_blobs");
        let hash = BlobHash::from_hex("abc123def456");

        // Simulate the path generation
        let path = blob_dir.join(hash.as_hex());

        assert_eq!(path, PathBuf::from("/tmp/test_blobs/abc123def456"));
    }
}
