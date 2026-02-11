//! Iroh blob store implementation (ADR-025)
//!
//! This module implements the `BlobStore` trait using iroh-blobs,
//! providing content-addressed blob storage with P2P mesh synchronization.
//!
//! # iroh-blobs Characteristics
//!
//! - Content-addressed storage using BLAKE3 hashes (32 bytes)
//! - Built on iroh's QUIC-based P2P networking
//! - Optimized for large file transfers with verified streaming
//! - No native metadata support (we use sidecar JSON files)
//!
//! # Usage
//!
//! ```ignore
//! use hive_mesh::storage::{IrohBlobStore, BlobStore, BlobMetadata};
//! use std::path::Path;
//!
//! let blob_store = IrohBlobStore::new_in_memory(blob_dir).await?;
//!
//! // Create blob from file
//! let token = blob_store.create_blob(
//!     Path::new("/models/yolov8.onnx"),
//!     BlobMetadata::with_name("yolov8.onnx")
//! ).await?;
//!
//! // Token can be shared via CRDT documents
//! // Other nodes can then fetch the blob
//! ```

use super::blob_traits::{BlobHandle, BlobHash, BlobMetadata, BlobProgress, BlobStore, BlobToken};
use anyhow::{Context, Result};
use iroh_blobs::{store::mem::MemStore, Hash};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// Sidecar metadata stored alongside blobs
///
/// Since iroh-blobs doesn't support native metadata, we store
/// metadata in JSON sidecar files: `{blob_dir}/{hash}.meta.json`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SidecarMetadata {
    /// Original BlobMetadata
    metadata: BlobMetadata,
    /// Size in bytes
    size_bytes: u64,
    /// Creation timestamp (Unix seconds)
    created_at: u64,
}

impl SidecarMetadata {
    fn new(metadata: BlobMetadata, size_bytes: u64) -> Self {
        Self {
            metadata,
            size_bytes,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

/// Iroh blob store implementing the BlobStore trait
///
/// Wraps iroh-blobs' in-memory store to provide backend-agnostic blob storage.
/// Blobs are content-addressed using BLAKE3 hashes.
///
/// # Metadata Storage
///
/// Since iroh-blobs doesn't support native metadata, we store metadata
/// in JSON sidecar files alongside the blob data.
pub struct IrohBlobStore {
    /// In-memory blob store from iroh-blobs
    store: MemStore,
    /// Cache of known blob tokens (hash -> token)
    token_cache: RwLock<HashMap<BlobHash, BlobToken>>,
    /// Directory for blob data exports and metadata sidecars
    blob_dir: PathBuf,
}

impl IrohBlobStore {
    /// Create a new Iroh blob store with in-memory storage
    ///
    /// # Arguments
    ///
    /// * `blob_dir` - Directory for exported blobs and metadata sidecars
    pub async fn new_in_memory(blob_dir: PathBuf) -> Result<Self> {
        // Create blob directory
        if let Err(e) = std::fs::create_dir_all(&blob_dir) {
            warn!("Failed to create blob directory {:?}: {}", blob_dir, e);
        }

        let store = MemStore::default();

        Ok(Self {
            store,
            token_cache: RwLock::new(HashMap::new()),
            blob_dir,
        })
    }

    /// Create with default temp directory
    pub async fn new_temp() -> Result<Self> {
        let blob_dir = std::env::temp_dir().join("hive_iroh_blobs");
        Self::new_in_memory(blob_dir).await
    }

    /// Get access to the underlying iroh-blobs store
    pub fn store(&self) -> &MemStore {
        &self.store
    }

    /// Get the blob directory path
    pub fn blob_dir(&self) -> &Path {
        &self.blob_dir
    }

    /// Convert iroh Hash to our BlobHash
    fn iroh_hash_to_blob_hash(hash: &Hash) -> BlobHash {
        BlobHash::from_hex(&hash.to_hex())
    }

    /// Convert our BlobHash to iroh Hash
    fn blob_hash_to_iroh_hash(hash: &BlobHash) -> Result<Hash> {
        Hash::from_str(hash.as_hex())
            .map_err(|e| anyhow::anyhow!("Invalid blob hash '{}': {}", hash.as_hex(), e))
    }

    /// Get the path for a blob's metadata sidecar file
    fn metadata_path(&self, hash: &BlobHash) -> PathBuf {
        self.blob_dir.join(format!("{}.meta.json", hash.as_hex()))
    }

    /// Get the local path for exported blob content
    fn local_blob_path(&self, hash: &BlobHash) -> PathBuf {
        self.blob_dir.join(hash.as_hex())
    }

    /// Save metadata sidecar file
    fn save_metadata(&self, hash: &BlobHash, metadata: &SidecarMetadata) -> Result<()> {
        let path = self.metadata_path(hash);
        let json = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write metadata to {:?}", path))?;
        Ok(())
    }

    /// Load metadata sidecar file
    fn load_metadata(&self, hash: &BlobHash) -> Option<SidecarMetadata> {
        let path = self.metadata_path(hash);
        if !path.exists() {
            return None;
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
    }

    /// Delete metadata sidecar file
    fn delete_metadata(&self, hash: &BlobHash) -> Result<()> {
        let path = self.metadata_path(hash);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to delete metadata at {:?}", path))?;
        }
        Ok(())
    }

    /// Cache a token for later lookup
    fn cache_token(&self, token: &BlobToken) {
        if let Ok(mut cache) = self.token_cache.write() {
            cache.insert(token.hash.clone(), token.clone());
        }
    }

    /// Export blob content to local filesystem
    async fn export_blob(&self, hash: &Hash) -> Result<PathBuf> {
        let blob_hash = Self::iroh_hash_to_blob_hash(hash);
        let local_path = self.local_blob_path(&blob_hash);

        // If already exported, return existing path
        if local_path.exists() {
            return Ok(local_path);
        }

        // Read blob content from store using get_bytes()
        let content = self
            .store
            .get_bytes(*hash)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get blob {}: {}", hash.to_hex(), e))?;

        // Write to local file
        std::fs::write(&local_path, &content)
            .with_context(|| format!("Failed to export blob to {:?}", local_path))?;

        Ok(local_path)
    }
}

#[async_trait::async_trait]
impl BlobStore for IrohBlobStore {
    async fn create_blob(&self, path: &Path, metadata: BlobMetadata) -> Result<BlobToken> {
        info!("Creating blob from file: {:?}", path);

        // Verify file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {:?}", path));
        }

        // Read file content
        let content = std::fs::read(path).with_context(|| format!("Failed to read {:?}", path))?;
        let size_bytes = content.len() as u64;

        // Add to iroh-blobs store using add_bytes()
        let tag = self.store.add_bytes(content).await?;
        let hash = tag.hash;

        // Build our token
        let token = BlobToken {
            hash: Self::iroh_hash_to_blob_hash(&hash),
            size_bytes,
            metadata: metadata.clone(),
        };

        // Save metadata sidecar
        let sidecar = SidecarMetadata::new(metadata, size_bytes);
        self.save_metadata(&token.hash, &sidecar)?;

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

        let size_bytes = data.len() as u64;

        // Add to iroh-blobs store using add_bytes()
        let tag = self.store.add_bytes(data.to_vec()).await?;
        let hash = tag.hash;

        // Build our token
        let token = BlobToken {
            hash: Self::iroh_hash_to_blob_hash(&hash),
            size_bytes,
            metadata: metadata.clone(),
        };

        // Save metadata sidecar
        let sidecar = SidecarMetadata::new(metadata, size_bytes);
        self.save_metadata(&token.hash, &sidecar)?;

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

        // Check if we already have it exported locally
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

        // Convert hash and check if in store using has()
        let iroh_hash = Self::blob_hash_to_iroh_hash(&token.hash)?;

        if self.store.has(iroh_hash).await? {
            // Blob is in our store, export it
            progress(BlobProgress::Downloading {
                downloaded_bytes: token.size_bytes / 2,
                total_bytes: token.size_bytes,
            });

            let export_path = self.export_blob(&iroh_hash).await?;

            progress(BlobProgress::Completed {
                local_path: export_path.clone(),
            });

            return Ok(BlobHandle::new(token.clone(), export_path));
        }

        // Blob not available locally
        // In Phase 1, remote fetch requires the P2P layer to be connected
        // and the blob to be announced. For now, return an error.
        progress(BlobProgress::Failed {
            error: format!(
                "Blob {} not available locally. Remote fetch requires P2P connectivity.",
                token.hash
            ),
        });

        Err(anyhow::anyhow!(
            "Blob {} not available locally. In Phase 1, ensure the blob is stored \
             on this node or connected via P2P to a node that has it.",
            token.hash.as_hex()
        ))
    }

    fn blob_exists_locally(&self, hash: &BlobHash) -> bool {
        // Check our local blob directory
        let local_path = self.local_blob_path(hash);
        if local_path.exists() {
            return true;
        }

        // Check cache
        if let Ok(cache) = self.token_cache.read() {
            if cache.contains_key(hash) {
                return true;
            }
        }

        // Check metadata sidecar (indicates we've seen this blob)
        self.metadata_path(hash).exists()
    }

    fn blob_info(&self, hash: &BlobHash) -> Option<BlobToken> {
        // Check cache first
        if let Ok(cache) = self.token_cache.read() {
            if let Some(token) = cache.get(hash) {
                return Some(token.clone());
            }
        }

        // Try loading from metadata sidecar
        if let Some(sidecar) = self.load_metadata(hash) {
            let token = BlobToken {
                hash: hash.clone(),
                size_bytes: sidecar.size_bytes,
                metadata: sidecar.metadata,
            };
            return Some(token);
        }

        None
    }

    async fn delete_blob(&self, hash: &BlobHash) -> Result<()> {
        info!("Deleting blob: hash={}", hash.as_hex());

        // Remove from local storage
        let local_path = self.local_blob_path(hash);
        if local_path.exists() {
            std::fs::remove_file(&local_path)
                .with_context(|| format!("Failed to delete local blob: {:?}", local_path))?;
        }

        // Remove metadata sidecar
        self.delete_metadata(hash)?;

        // Remove from cache
        if let Ok(mut cache) = self.token_cache.write() {
            cache.remove(hash);
        }

        // Note: We don't delete from the iroh-blobs store directly
        // as it may be shared or used for P2P sync. The in-memory store
        // will be garbage collected when no longer referenced.

        Ok(())
    }

    fn list_local_blobs(&self) -> Vec<BlobToken> {
        let mut tokens = Vec::new();

        // First, get tokens from cache
        if let Ok(cache) = self.token_cache.read() {
            tokens.extend(cache.values().cloned());
        }

        // Also scan metadata directory for any we might have missed
        if let Ok(entries) = std::fs::read_dir(&self.blob_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.ends_with(".meta.json") {
                        let hash_hex = filename.trim_end_matches(".meta.json");
                        let hash = BlobHash::from_hex(hash_hex);

                        // Skip if already in tokens
                        if tokens.iter().any(|t| t.hash == hash) {
                            continue;
                        }

                        // Load metadata and add to tokens
                        if let Some(sidecar) = self.load_metadata(&hash) {
                            tokens.push(BlobToken {
                                hash,
                                size_bytes: sidecar.size_bytes,
                                metadata: sidecar.metadata,
                            });
                        }
                    }
                }
            }
        }

        tokens
    }

    fn local_storage_bytes(&self) -> u64 {
        // Sum up sizes from cache (matches DittoBlobStore behavior)
        // This represents logical storage used by blobs we know about,
        // regardless of whether they've been exported to disk yet.
        if let Ok(cache) = self.token_cache.read() {
            if !cache.is_empty() {
                return cache.values().map(|t| t.size_bytes).sum();
            }
        }

        // Fallback: scan metadata sidecars for size info
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.blob_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.ends_with(".meta.json") {
                        let hash_hex = filename.trim_end_matches(".meta.json");
                        let hash = BlobHash::from_hex(hash_hex);
                        if let Some(sidecar) = self.load_metadata(&hash) {
                            total += sidecar.size_bytes;
                        }
                    }
                }
            }
        }

        total
    }
}

// ============================================================================
// NetworkedIrohBlobStore - P2P blob sync with iroh endpoint
// ============================================================================

use iroh::{Endpoint, EndpointId};
use iroh_blobs::BlobsProtocol;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Networked Iroh blob store with P2P sync capabilities
///
/// Extends `IrohBlobStore` with:
/// - iroh `Endpoint` for QUIC connections
/// - `BlobsProtocol` for serving blobs to peers
/// - `Downloader` for fetching from remote peers
///
/// # Architecture
///
/// ```text
/// NetworkedIrohBlobStore
///     ├─ IrohBlobStore (local storage)
///     ├─ Endpoint (QUIC transport)
///     ├─ BlobsProtocol (incoming requests)
///     └─ known_peers (for remote fetch)
/// ```
///
/// # Usage
///
/// ```ignore
/// use hive_mesh::storage::NetworkedIrohBlobStore;
///
/// // Create with networking
/// let store = NetworkedIrohBlobStore::new(blob_dir).await?;
///
/// // Add known peer for remote fetch
/// store.add_peer(peer_endpoint_id).await;
///
/// // Create blob locally (available to peers via BlobsProtocol)
/// let token = store.create_blob_from_bytes(data, metadata).await?;
///
/// // Fetch blob (tries local first, then remote peers)
/// let handle = store.fetch_blob(&token, |progress| {}).await?;
/// ```
pub struct NetworkedIrohBlobStore {
    /// Local blob store
    local_store: IrohBlobStore,
    /// Iroh endpoint for P2P connections
    endpoint: Endpoint,
    /// BlobsProtocol for serving blobs
    blobs_protocol: BlobsProtocol,
    /// Known peers that may have blobs
    known_peers: TokioRwLock<Vec<EndpointId>>,
}

impl NetworkedIrohBlobStore {
    /// Create a new networked blob store
    ///
    /// Binds an iroh endpoint and starts the BlobsProtocol for serving blobs.
    ///
    /// # Arguments
    ///
    /// * `blob_dir` - Directory for blob storage and metadata
    pub async fn new(blob_dir: PathBuf) -> Result<Arc<Self>> {
        // Create local store
        let local_store = IrohBlobStore::new_in_memory(blob_dir).await?;

        // Create endpoint
        let endpoint = Endpoint::builder()
            .alpns(vec![iroh_blobs::ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create iroh endpoint: {}", e))?;

        // Create BlobsProtocol with our store
        let blobs_protocol = BlobsProtocol::new(&local_store.store, None);

        let store = Arc::new(Self {
            local_store,
            endpoint,
            blobs_protocol,
            known_peers: TokioRwLock::new(Vec::new()),
        });

        // Note: In a full implementation, you'd spawn a Router here to handle
        // incoming blob requests. For E2E tests, we handle this explicitly.

        Ok(store)
    }

    /// Create with a specific bind address
    pub async fn bind(blob_dir: PathBuf, bind_addr: std::net::SocketAddr) -> Result<Arc<Self>> {
        let local_store = IrohBlobStore::new_in_memory(blob_dir).await?;

        let bind_addr_v4 = match bind_addr {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => {
                return Err(anyhow::anyhow!("Only IPv4 addresses supported"))
            }
        };

        let endpoint = Endpoint::builder()
            .alpns(vec![iroh_blobs::ALPN.to_vec()])
            .bind_addr_v4(bind_addr_v4)
            .bind()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create iroh endpoint: {}", e))?;

        let blobs_protocol = BlobsProtocol::new(&local_store.store, None);

        Ok(Arc::new(Self {
            local_store,
            endpoint,
            blobs_protocol,
            known_peers: TokioRwLock::new(Vec::new()),
        }))
    }

    /// Get the endpoint ID (public key) for this node
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Get a reference to the underlying endpoint
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Get a reference to the blobs protocol handler
    pub fn blobs_protocol(&self) -> &BlobsProtocol {
        &self.blobs_protocol
    }

    /// Get a reference to the local store
    pub fn local_store(&self) -> &IrohBlobStore {
        &self.local_store
    }

    /// Add a peer that may have blobs we want to fetch
    pub async fn add_peer(&self, peer_id: EndpointId) {
        let mut peers = self.known_peers.write().await;
        if !peers.contains(&peer_id) {
            peers.push(peer_id);
            debug!("Added peer {} for blob fetching", peer_id.fmt_short());
        }
    }

    /// Remove a peer from the known peers list
    pub async fn remove_peer(&self, peer_id: &EndpointId) {
        let mut peers = self.known_peers.write().await;
        peers.retain(|p| p != peer_id);
    }

    /// List known peers
    pub async fn known_peers(&self) -> Vec<EndpointId> {
        self.known_peers.read().await.clone()
    }

    /// Get the iroh-blobs Store API for advanced operations
    pub fn store_api(&self) -> &iroh_blobs::api::Store {
        &self.local_store.store
    }
}

#[async_trait::async_trait]
impl BlobStore for NetworkedIrohBlobStore {
    async fn create_blob(&self, path: &Path, metadata: BlobMetadata) -> Result<BlobToken> {
        // Delegate to local store - blob becomes available for P2P via BlobsProtocol
        self.local_store.create_blob(path, metadata).await
    }

    async fn create_blob_from_bytes(
        &self,
        data: &[u8],
        metadata: BlobMetadata,
    ) -> Result<BlobToken> {
        // Delegate to local store
        self.local_store
            .create_blob_from_bytes(data, metadata)
            .await
    }

    async fn fetch_blob<F>(&self, token: &BlobToken, mut progress: F) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static,
    {
        info!(
            "NetworkedIrohBlobStore: Fetching blob {}",
            token.hash.as_hex()
        );

        // First, try local fetch
        let local_path = self.local_store.local_blob_path(&token.hash);
        if local_path.exists() {
            debug!("Blob exists locally at {:?}", local_path);
            progress(BlobProgress::Completed {
                local_path: local_path.clone(),
            });
            return Ok(BlobHandle::new(token.clone(), local_path));
        }

        // Check if in local store (not yet exported)
        let iroh_hash = IrohBlobStore::blob_hash_to_iroh_hash(&token.hash)?;
        if self.local_store.store.has(iroh_hash).await? {
            debug!("Blob in local store, exporting");
            return self.local_store.fetch_blob(token, progress).await;
        }

        // Not available locally - try to fetch from known peers
        progress(BlobProgress::Started {
            total_bytes: token.size_bytes,
        });

        let peers = self.known_peers.read().await.clone();
        if peers.is_empty() {
            progress(BlobProgress::Failed {
                error: format!(
                    "Blob {} not available locally and no peers known",
                    token.hash
                ),
            });
            return Err(anyhow::anyhow!(
                "Blob {} not available locally and no peers configured for remote fetch",
                token.hash.as_hex()
            ));
        }

        info!(
            "Attempting to fetch blob {} from {} known peers",
            token.hash.as_hex(),
            peers.len()
        );

        // Use the downloader to fetch from peers
        let downloader = self.store_api().downloader(&self.endpoint);

        for peer_id in &peers {
            debug!(
                "Trying peer {} for blob {}",
                peer_id.fmt_short(),
                token.hash.as_hex()
            );

            progress(BlobProgress::Downloading {
                downloaded_bytes: 0,
                total_bytes: token.size_bytes,
            });

            // Try to download from this peer
            match downloader.download(iroh_hash, Some(*peer_id)).await {
                Ok(_) => {
                    info!(
                        "Successfully downloaded blob {} from peer {}",
                        token.hash.as_hex(),
                        peer_id.fmt_short()
                    );

                    // Export to local filesystem
                    let export_path = self.local_store.export_blob(&iroh_hash).await?;

                    // Save metadata sidecar
                    let sidecar = SidecarMetadata::new(token.metadata.clone(), token.size_bytes);
                    self.local_store.save_metadata(&token.hash, &sidecar)?;

                    // Cache token
                    self.local_store.cache_token(token);

                    progress(BlobProgress::Completed {
                        local_path: export_path.clone(),
                    });

                    return Ok(BlobHandle::new(token.clone(), export_path));
                }
                Err(e) => {
                    warn!(
                        "Failed to download from peer {}: {}",
                        peer_id.fmt_short(),
                        e
                    );
                    continue;
                }
            }
        }

        // All peers failed
        progress(BlobProgress::Failed {
            error: format!(
                "Failed to fetch blob {} from all {} peers",
                token.hash,
                peers.len()
            ),
        });

        Err(anyhow::anyhow!(
            "Failed to fetch blob {} from any of {} known peers",
            token.hash.as_hex(),
            peers.len()
        ))
    }

    fn blob_exists_locally(&self, hash: &BlobHash) -> bool {
        self.local_store.blob_exists_locally(hash)
    }

    fn blob_info(&self, hash: &BlobHash) -> Option<BlobToken> {
        self.local_store.blob_info(hash)
    }

    async fn delete_blob(&self, hash: &BlobHash) -> Result<()> {
        self.local_store.delete_blob(hash).await
    }

    fn list_local_blobs(&self) -> Vec<BlobToken> {
        self.local_store.list_local_blobs()
    }

    fn local_storage_bytes(&self) -> u64 {
        self.local_store.local_storage_bytes()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_store() -> (IrohBlobStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = IrohBlobStore::new_in_memory(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (store, temp_dir)
    }

    #[tokio::test]
    async fn test_create_blob_from_bytes() {
        let (store, _temp) = create_test_store().await;

        let data = b"Hello, iroh-blobs!";
        let metadata = BlobMetadata::with_name("test.txt");

        let token = store.create_blob_from_bytes(data, metadata).await.unwrap();

        assert_eq!(token.size_bytes, data.len() as u64);
        assert_eq!(token.metadata.name, Some("test.txt".to_string()));
        assert!(!token.hash.as_hex().is_empty());
    }

    #[tokio::test]
    async fn test_create_blob_from_file() {
        let (store, temp_dir) = create_test_store().await;

        // Create a test file
        let test_file = temp_dir.path().join("test_input.txt");
        std::fs::write(&test_file, "File content for testing").unwrap();

        let metadata = BlobMetadata::with_name_and_type("test_input.txt", "text/plain");
        let token = store.create_blob(&test_file, metadata).await.unwrap();

        assert_eq!(token.size_bytes, 24); // "File content for testing".len()
        assert_eq!(token.metadata.name, Some("test_input.txt".to_string()));
        assert_eq!(token.metadata.content_type, Some("text/plain".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_blob() {
        let (store, _temp) = create_test_store().await;

        // Create a blob first
        let data = b"Content to fetch";
        let metadata = BlobMetadata::with_name("fetch_test.bin");
        let token = store.create_blob_from_bytes(data, metadata).await.unwrap();

        // Fetch it back
        let handle = store.fetch_blob(&token, |_progress| {}).await.unwrap();

        assert!(handle.path.exists());
        let content = std::fs::read(&handle.path).unwrap();
        assert_eq!(content, data);
    }

    #[tokio::test]
    async fn test_blob_exists_locally() {
        let (store, _temp) = create_test_store().await;

        let data = b"Test data";
        let metadata = BlobMetadata::default();
        let token = store.create_blob_from_bytes(data, metadata).await.unwrap();

        assert!(store.blob_exists_locally(&token.hash));

        let unknown_hash =
            BlobHash::from_hex("0000000000000000000000000000000000000000000000000000000000000000");
        assert!(!store.blob_exists_locally(&unknown_hash));
    }

    #[tokio::test]
    async fn test_blob_info() {
        let (store, _temp) = create_test_store().await;

        let data = b"Info test";
        let metadata = BlobMetadata::with_name("info.dat").with_custom("version", "1.0");
        let token = store.create_blob_from_bytes(data, metadata).await.unwrap();

        let info = store.blob_info(&token.hash).unwrap();
        assert_eq!(info.size_bytes, token.size_bytes);
        assert_eq!(info.metadata.name, Some("info.dat".to_string()));
        assert_eq!(
            info.metadata.custom.get("version"),
            Some(&"1.0".to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_blob() {
        let (store, _temp) = create_test_store().await;

        let data = b"To be deleted";
        let metadata = BlobMetadata::default();
        let token = store.create_blob_from_bytes(data, metadata).await.unwrap();

        // Export blob to local file first
        let _ = store.fetch_blob(&token, |_| {}).await.unwrap();

        assert!(store.blob_exists_locally(&token.hash));

        store.delete_blob(&token.hash).await.unwrap();

        // Should no longer exist locally
        let local_path = store.local_blob_path(&token.hash);
        assert!(!local_path.exists());
        assert!(store.blob_info(&token.hash).is_none());
    }

    #[tokio::test]
    async fn test_list_local_blobs() {
        let (store, _temp) = create_test_store().await;

        // Create multiple blobs
        let token1 = store
            .create_blob_from_bytes(b"Blob 1", BlobMetadata::with_name("one.txt"))
            .await
            .unwrap();
        let token2 = store
            .create_blob_from_bytes(b"Blob 2", BlobMetadata::with_name("two.txt"))
            .await
            .unwrap();
        let token3 = store
            .create_blob_from_bytes(b"Blob 3", BlobMetadata::with_name("three.txt"))
            .await
            .unwrap();

        let blobs = store.list_local_blobs();
        assert_eq!(blobs.len(), 3);

        let hashes: Vec<_> = blobs.iter().map(|t| t.hash.clone()).collect();
        assert!(hashes.contains(&token1.hash));
        assert!(hashes.contains(&token2.hash));
        assert!(hashes.contains(&token3.hash));
    }

    #[tokio::test]
    async fn test_local_storage_bytes() {
        let (store, _temp) = create_test_store().await;

        // Initially zero
        assert_eq!(store.local_storage_bytes(), 0);

        // Create blobs (no need to export them - cache tracks sizes)
        let data1 = b"Small";
        let _token1 = store
            .create_blob_from_bytes(data1, BlobMetadata::default())
            .await
            .unwrap();

        let data2 = b"Larger blob content";
        let _token2 = store
            .create_blob_from_bytes(data2, BlobMetadata::default())
            .await
            .unwrap();

        let total = store.local_storage_bytes();
        assert_eq!(total, (data1.len() + data2.len()) as u64);
    }

    #[tokio::test]
    async fn test_metadata_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let blob_dir = temp_dir.path().to_path_buf();

        // Create a store and add a blob
        let store1 = IrohBlobStore::new_in_memory(blob_dir.clone())
            .await
            .unwrap();

        let data = b"Persistent metadata test";
        let metadata = BlobMetadata::with_name("persist.txt").with_custom("key", "value");
        let token = store1.create_blob_from_bytes(data, metadata).await.unwrap();

        // Create a NEW store pointing to the same directory
        let store2 = IrohBlobStore::new_in_memory(blob_dir).await.unwrap();

        // Metadata should be loadable from the sidecar file
        let info = store2.blob_info(&token.hash).unwrap();
        assert_eq!(info.metadata.name, Some("persist.txt".to_string()));
        assert_eq!(info.metadata.custom.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_sidecar_metadata_serialization() {
        let metadata = BlobMetadata::with_name("test.onnx")
            .with_custom("version", "1.0")
            .with_custom("model_id", "yolov8");

        let sidecar = SidecarMetadata::new(metadata, 1024);

        let json = serde_json::to_string(&sidecar).unwrap();
        let parsed: SidecarMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.size_bytes, 1024);
        assert_eq!(parsed.metadata.name, Some("test.onnx".to_string()));
        assert_eq!(
            parsed.metadata.custom.get("version"),
            Some(&"1.0".to_string())
        );
    }
}
