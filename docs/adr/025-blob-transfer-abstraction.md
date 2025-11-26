# ADR-025: Blob Transfer Abstraction Layer

**Status**: Proposed
**Date**: 2025-11-25
**Authors**: Codex, Kit Plummer
**Relates to**: ADR-005 (DataSync Abstraction), ADR-013 (Distributed Software/AI Operations), ADR-022 (Edge MLOps), ADR-018 (AI Model Capability Advertisement)

## Context

### The File Transfer Gap

HIVE Protocol has comprehensive architecture for distributed AI model operations (ADR-013, ADR-022) and capability advertisement (ADR-018), but lacks the foundational primitive: **backend-agnostic binary file transfer through the mesh**.

**Current State:**
- `StorageBackend` and `Collection` traits handle document CRUD (`storage/traits.rs`)
- `DittoBackend` wraps Ditto document sync
- `AutomergeBackend` wraps Automerge document sync
- **No blob/attachment support** - large binaries cannot be transferred

**The Problem:**
```
ADR-013 says: "Distribute 500MB AI model to 64 platforms"
ADR-022 says: "ONNX models with multiple variants (FP32/FP16/INT8)"

But we have no API to actually transfer these files!
```

### Why Documents Aren't Enough

CRDT document sync (Ditto/Automerge) is optimized for:
- Small JSON-like documents (KB to low MB)
- Field-level conflict resolution
- Frequent incremental updates

Binary blobs need different handling:
- Large files (100MB-10GB AI models)
- Content-addressed deduplication
- Chunked transfer with resumption
- Progress tracking
- No CRDT merging (blobs are immutable)

Both Ditto and Iroh recognize this distinction:
- **Ditto**: Separate "Attachment" API with content-addressable storage
- **Iroh**: `iroh-blobs` crate distinct from `iroh-docs`

### Use Cases Requiring File Transfer

| Use Case | File Types | Typical Size | Delivery Pattern |
|----------|-----------|--------------|------------------|
| AI Model Distribution | ONNX, TFLite | 100MB-2GB | Hierarchical cascade |
| Software Updates | Binaries, containers | 10MB-500MB | Targeted delivery |
| Configuration Packages | Encrypted bundles | 1KB-10MB | Formation-wide |
| Training Data Upload | Images, telemetry | 1MB-100MB | Edge → C2 |
| Maps/Threat Libraries | Geospatial data | 50MB-1GB | Sector-based |
| Firmware Updates | Binary images | 5MB-100MB | Platform-specific |

### Backend Capabilities

#### Ditto Attachments

Ditto provides a mature attachment API:

```rust
// Create attachment
let attachment = ditto.store().new_attachment(&file_path, metadata)?;

// Reference in document
ditto.store().execute_v2((
    "INSERT INTO models (model_attachment ATTACHMENT) VALUES (:doc)",
    json!({ "doc": { "_id": model_id, "model_attachment": attachment } }),
)).await?;

// Fetch with progress
let fetcher = ditto.store().fetch_attachment(token, |event| match event {
    DittoAttachmentFetchEvent::Completed { attachment } => { /* ... */ }
    DittoAttachmentFetchEvent::Progress { downloaded_bytes, total_bytes } => { /* ... */ }
    DittoAttachmentFetchEvent::Deleted => { /* ... */ }
})?;
```

**Features:**
- Content-addressable storage (hash-based IDs)
- Automatic deduplication (same blob stored once)
- Separate sync protocol optimized for large transfers
- Resilient resumption on interrupted transfers
- Progress callbacks
- 10-minute garbage collection for unreferenced blobs

#### iroh-blobs

Iroh provides `iroh-blobs` for content-addressed blob transfer:

```rust
// Create blob from data
let hash = blobs.add_bytes(data).await?;

// Download blob by hash
let content = blobs.read_to_bytes(hash).await?;

// Stream large blob
let mut reader = blobs.read(hash).await?;
while let Some(chunk) = reader.next().await {
    process_chunk(chunk?);
}
```

**Features:**
- BLAKE3 verified streaming (16KiB chunk groups)
- Content-addressed by hash
- Verified range requests (fetch portions)
- Size proofs (verify size before download)
- No central server required

### Gap Analysis

| Feature | Ditto | iroh-blobs | HIVE (Current) |
|---------|-------|------------|----------------|
| Content-addressed storage | Yes | Yes | No |
| Progress tracking | Yes | Yes | No |
| Resumable transfers | Yes | Yes | No |
| Deduplication | Yes | Yes | No |
| Backend-agnostic API | N/A | N/A | **Missing** |
| Targeted delivery | Via queries | Manual | **Missing** |
| Formation distribution | Via sync | Manual | **Missing** |

## Decision

### Core Principle: Separate Blob and Document Sync

Introduce a `BlobStore` trait parallel to `StorageBackend`, acknowledging that blob transfer has fundamentally different semantics than document sync:

```
┌─────────────────────────────────────────────────────────────┐
│                    HIVE Protocol Layer                       │
│                                                              │
│  ┌─────────────────┐           ┌─────────────────────────┐  │
│  │ ModelDistribution│           │  SoftwareDistribution   │  │
│  │ (ADR-022)        │           │  (ADR-013)              │  │
│  └────────┬────────┘           └───────────┬─────────────┘  │
│           │                                 │                │
│           └──────────┬──────────────────────┘                │
│                      ▼                                       │
│           ┌─────────────────────┐                           │
│           │  FileDistribution   │  ← Targeted delivery      │
│           │  (This ADR)         │    Progress monitoring    │
│           └──────────┬──────────┘                           │
│                      ▼                                       │
│  ┌───────────────────────────────────────────────────────┐  │
│  │               BlobStore Trait                          │  │
│  │   create_blob() / fetch_blob() / blob_exists()        │  │
│  └───────────────────┬───────────────────────────────────┘  │
└──────────────────────┼──────────────────────────────────────┘
                       │
        ┌──────────────┴──────────────┐
        ▼                              ▼
┌───────────────────┐         ┌───────────────────┐
│  DittoBlobStore   │         │  IrohBlobStore    │
│  (Attachments)    │         │  (iroh-blobs)     │
└───────────────────┘         └───────────────────┘
```

### Layer 1: BlobStore Trait

Backend-agnostic interface for content-addressed blob storage:

```rust
//! Blob storage trait for large binary transfers
//!
//! This trait abstracts over content-addressed blob storage backends
//! (Ditto Attachments, iroh-blobs) enabling backend-agnostic file transfer.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::Result;

/// Content-addressed blob identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// Token referencing a blob with metadata
#[derive(Clone, Debug)]
pub struct BlobToken {
    /// Content hash (sha256 for Ditto, blake3 for Iroh)
    pub hash: BlobHash,
    /// Size in bytes
    pub size_bytes: u64,
    /// User-defined metadata
    pub metadata: BlobMetadata,
}

/// Metadata attached to blobs
#[derive(Clone, Debug, Default)]
pub struct BlobMetadata {
    /// Human-readable name
    pub name: Option<String>,
    /// MIME type
    pub content_type: Option<String>,
    /// Custom key-value pairs
    pub custom: HashMap<String, String>,
}

/// Progress updates during blob operations
#[derive(Clone, Debug)]
pub enum BlobProgress {
    /// Transfer started
    Started { total_bytes: u64 },
    /// Transfer in progress
    Downloading { downloaded_bytes: u64, total_bytes: u64 },
    /// Transfer complete
    Completed { local_path: PathBuf },
    /// Transfer failed
    Failed { error: String },
}

/// Handle to a locally available blob
pub struct BlobHandle {
    /// Token identifying the blob
    pub token: BlobToken,
    /// Local filesystem path
    pub path: PathBuf,
}

/// Content-addressed blob storage trait
///
/// Abstracts over backend-specific blob storage (Ditto Attachments, iroh-blobs).
/// All blobs are content-addressed: the hash of the content serves as the ID.
///
/// # Thread Safety
///
/// All methods are safe to call from multiple threads.
///
/// # Example
///
/// ```ignore
/// // Create blob from file
/// let token = blob_store.create_blob(Path::new("/models/yolov8.onnx"), metadata).await?;
/// println!("Created blob: {}", token.hash.as_hex());
///
/// // Fetch blob with progress
/// let handle = blob_store.fetch_blob(&token, |progress| {
///     if let BlobProgress::Downloading { downloaded, total } = progress {
///         println!("Progress: {}/{} bytes", downloaded, total);
///     }
/// }).await?;
/// println!("Blob available at: {}", handle.path.display());
/// ```
#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// Create a blob from a file
    ///
    /// Reads the file, computes content hash, and stores in blob storage.
    /// Returns a token that can be used to fetch the blob later.
    ///
    /// # Arguments
    /// * `path` - Path to source file
    /// * `metadata` - User-defined metadata to attach
    ///
    /// # Returns
    /// Token identifying the blob (content hash + size + metadata)
    async fn create_blob(
        &self,
        path: &Path,
        metadata: BlobMetadata,
    ) -> Result<BlobToken>;

    /// Create a blob from bytes
    ///
    /// Useful for generating blobs programmatically without writing to disk.
    ///
    /// # Arguments
    /// * `data` - Raw blob content
    /// * `metadata` - User-defined metadata to attach
    async fn create_blob_from_bytes(
        &self,
        data: &[u8],
        metadata: BlobMetadata,
    ) -> Result<BlobToken>;

    /// Fetch a blob with progress tracking
    ///
    /// If the blob exists locally, returns immediately.
    /// Otherwise, fetches from mesh peers (via backend-specific protocol).
    ///
    /// # Arguments
    /// * `token` - Token identifying the blob to fetch
    /// * `progress` - Callback invoked with progress updates
    ///
    /// # Returns
    /// Handle providing local path to blob content
    async fn fetch_blob<F>(
        &self,
        token: &BlobToken,
        progress: F,
    ) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static;

    /// Check if blob exists locally
    ///
    /// Returns true if the blob is available locally without network fetch.
    fn blob_exists_locally(&self, hash: &BlobHash) -> bool;

    /// Get blob info without fetching content
    ///
    /// Returns metadata about a known blob, or None if unknown.
    fn blob_info(&self, hash: &BlobHash) -> Option<BlobToken>;

    /// Delete a local blob
    ///
    /// Removes the blob from local storage. Does not affect other peers.
    /// If blob is referenced by documents, garbage collection may recreate it.
    async fn delete_blob(&self, hash: &BlobHash) -> Result<()>;

    /// List all locally available blobs
    fn list_local_blobs(&self) -> Vec<BlobToken>;

    /// Get total size of local blob storage
    fn local_storage_bytes(&self) -> u64;
}
```

### Layer 2: Document-Blob Integration

Blobs are referenced from CRDT documents via tokens:

```rust
/// Store a blob reference in a document
///
/// This creates the connection between CRDT-synced documents and
/// content-addressed blob storage.
pub trait BlobDocumentIntegration {
    /// Store blob token in document field
    ///
    /// The token is stored as a structured value in the document.
    /// When the document syncs to other nodes, they see the token
    /// and can fetch the blob content on demand.
    async fn store_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        token: &BlobToken,
    ) -> Result<()>;

    /// Retrieve blob token from document field
    async fn get_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<Option<BlobToken>>;
}
```

**Schema Example (AI Model Registry):**
```json
{
  "_id": "target_recognition:4.2.1",
  "model_id": "target_recognition",
  "version": "4.2.1",
  "variants": {
    "fp32_cuda": {
      "blob_token": {
        "hash": "sha256:a7f8b3c4d5e6f7...",
        "size_bytes": 487326720,
        "metadata": {
          "name": "target_recognition_fp32.onnx",
          "content_type": "application/onnx"
        }
      },
      "precision": "float32",
      "execution_providers": ["CUDAExecutionProvider"]
    },
    "int8_cpu": {
      "blob_token": {
        "hash": "sha256:b8c9d4e5f6a7b8...",
        "size_bytes": 121831680,
        "metadata": {
          "name": "target_recognition_int8.onnx",
          "content_type": "application/onnx"
        }
      },
      "precision": "int8",
      "execution_providers": ["CPUExecutionProvider"]
    }
  },
  "provenance": {
    "signed_by": "ml_ops_team",
    "signature": "ed25519:..."
  }
}
```

### Layer 3: FileDistribution API

Higher-level API for targeted delivery and progress monitoring:

```rust
/// Priority levels for file distribution
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferPriority {
    /// ROE updates, safety-critical fixes - immediate transfer
    Critical,
    /// Operational model updates - next available window
    High,
    /// Routine updates - best effort
    Normal,
    /// Non-urgent - defer to low-bandwidth periods
    Low,
}

/// Target scope for file distribution
#[derive(Clone, Debug)]
pub enum DistributionScope {
    /// All connected nodes
    AllNodes,
    /// Specific formation (cell, platoon, company)
    Formation { formation_id: String },
    /// Specific nodes by ID
    Nodes { node_ids: Vec<String> },
    /// Nodes with specific capabilities
    Capable {
        /// Minimum GPU memory (for model deployment)
        min_gpu_gb: Option<f64>,
        /// Required CPU architecture
        cpu_arch: Option<String>,
    },
}

/// Status of a single node in distribution
#[derive(Clone, Debug)]
pub struct NodeTransferStatus {
    pub node_id: String,
    pub status: TransferState,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferState {
    Pending,
    Connecting,
    Transferring,
    Completed,
    Failed,
}

/// Handle to track distribution operation
#[derive(Clone, Debug)]
pub struct DistributionHandle {
    pub distribution_id: String,
    pub blob_hash: BlobHash,
    pub scope: DistributionScope,
    pub priority: TransferPriority,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// Overall distribution status
#[derive(Clone, Debug)]
pub struct DistributionStatus {
    pub handle: DistributionHandle,
    pub total_targets: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub node_statuses: HashMap<String, NodeTransferStatus>,
}

/// File distribution service for targeted delivery
#[async_trait::async_trait]
pub trait FileDistribution: Send + Sync {
    /// Distribute blob to target nodes
    ///
    /// Initiates distribution of a blob to nodes matching the scope.
    /// Returns a handle for tracking progress.
    ///
    /// # Distribution Behavior by Backend
    ///
    /// **Ditto**: Creates document with blob reference in collection watched by
    /// target nodes. Blob syncs via attachment protocol.
    ///
    /// **Iroh**: Connects to target nodes and pushes blob directly.
    async fn distribute(
        &self,
        blob_token: &BlobToken,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<DistributionHandle>;

    /// Get current distribution status
    async fn status(&self, handle: &DistributionHandle) -> Result<DistributionStatus>;

    /// Cancel an in-progress distribution
    async fn cancel(&self, handle: &DistributionHandle) -> Result<()>;

    /// Wait for distribution to complete (or fail)
    async fn wait_for_completion(
        &self,
        handle: &DistributionHandle,
        timeout: std::time::Duration,
    ) -> Result<DistributionStatus>;

    /// Subscribe to distribution progress updates
    async fn subscribe_progress(
        &self,
        handle: &DistributionHandle,
    ) -> tokio::sync::broadcast::Receiver<DistributionStatus>;
}
```

### Layer 4: ModelDistribution (ADR-022 Integration)

Specialized API for AI model distribution:

```rust
/// Model distribution building on FileDistribution
///
/// Provides model-specific features like:
/// - Variant selection based on hardware capabilities
/// - Differential updates (delta between versions)
/// - Convergence monitoring
/// - Automatic rollback
#[async_trait::async_trait]
pub trait ModelDistribution: Send + Sync {
    /// Distribute model to eligible platforms
    ///
    /// Selects appropriate variant for each target based on hardware.
    async fn distribute_model(
        &self,
        model_id: &str,
        version: &str,
        scope: DistributionScope,
        priority: TransferPriority,
    ) -> Result<ModelDistributionHandle>;

    /// Distribute differential update
    ///
    /// Computes delta between versions and distributes only changed chunks.
    /// Requires platforms to already have from_version.
    async fn distribute_model_delta(
        &self,
        model_id: &str,
        from_version: &str,
        to_version: &str,
        scope: DistributionScope,
    ) -> Result<ModelDistributionHandle>;

    /// Get convergence status
    ///
    /// Shows how many platforms have received the model and are operational.
    async fn convergence_status(
        &self,
        model_id: &str,
        target_version: &str,
    ) -> Result<ModelConvergenceStatus>;

    /// Rollback to previous version
    ///
    /// Initiates distribution of previous version to platforms that received
    /// a problematic update.
    async fn rollback(
        &self,
        model_id: &str,
        to_version: &str,
        scope: DistributionScope,
    ) -> Result<ModelDistributionHandle>;
}

/// Model convergence status across formation
#[derive(Clone, Debug)]
pub struct ModelConvergenceStatus {
    pub model_id: String,
    pub target_version: String,
    pub total_platforms: usize,
    pub converged: usize,        // Have correct version AND operational
    pub in_progress: usize,      // Currently receiving
    pub pending: usize,          // Not yet started
    pub failed: usize,           // Transfer or deployment failed
    pub version_distribution: HashMap<String, usize>,  // version -> count
    pub blockers: Vec<ConvergenceBlocker>,
    pub estimated_completion: Option<std::time::Duration>,
}

#[derive(Clone, Debug)]
pub struct ConvergenceBlocker {
    pub node_id: String,
    pub reason: BlockerReason,
    pub since: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
pub enum BlockerReason {
    NetworkPartition,
    InsufficientStorage,
    InsufficientGpuMemory,
    TransferFailed { error: String },
    DeploymentFailed { error: String },
}
```

### DittoBlobStore Implementation

```rust
use ditto::prelude::*;
use std::sync::Arc;

pub struct DittoBlobStore {
    ditto: Arc<Ditto>,
}

impl DittoBlobStore {
    pub fn new(ditto: Arc<Ditto>) -> Self {
        Self { ditto }
    }
}

#[async_trait::async_trait]
impl BlobStore for DittoBlobStore {
    async fn create_blob(
        &self,
        path: &Path,
        metadata: BlobMetadata,
    ) -> Result<BlobToken> {
        // Convert metadata to Ditto format
        let ditto_metadata: HashMap<String, String> = metadata.into();

        // Create attachment via Ditto API
        let attachment = self.ditto.store()
            .new_attachment(path, ditto_metadata)
            .map_err(|e| anyhow::anyhow!("Failed to create attachment: {}", e))?;

        // Extract token info
        let token = BlobToken {
            hash: BlobHash::from_hex(&attachment.id()),
            size_bytes: attachment.len() as u64,
            metadata,
        };

        Ok(token)
    }

    async fn fetch_blob<F>(
        &self,
        token: &BlobToken,
        mut progress: F,
    ) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static,
    {
        // Convert token to Ditto format
        let ditto_token = self.token_to_ditto(token)?;

        let (tx, rx) = tokio::sync::oneshot::channel();

        // Fetch with progress callback
        let _fetcher = self.ditto.store().fetch_attachment(
            ditto_token,
            move |event| {
                match event {
                    DittoAttachmentFetchEvent::Progress {
                        downloaded_bytes,
                        total_bytes
                    } => {
                        progress(BlobProgress::Downloading {
                            downloaded_bytes: downloaded_bytes as u64,
                            total_bytes: total_bytes as u64,
                        });
                    }
                    DittoAttachmentFetchEvent::Completed { attachment } => {
                        let path = attachment.path().to_path_buf();
                        progress(BlobProgress::Completed { local_path: path.clone() });
                        let _ = tx.send(Ok(path));
                    }
                    DittoAttachmentFetchEvent::Deleted => {
                        progress(BlobProgress::Failed {
                            error: "Attachment deleted".to_string()
                        });
                        let _ = tx.send(Err(anyhow::anyhow!("Attachment deleted")));
                    }
                }
            },
        )?;

        // Wait for completion
        let path = rx.await??;

        Ok(BlobHandle {
            token: token.clone(),
            path,
        })
    }

    fn blob_exists_locally(&self, hash: &BlobHash) -> bool {
        // Query local blob store
        // Ditto doesn't expose direct API for this, use filesystem check
        self.get_blob_path(hash).map_or(false, |p| p.exists())
    }

    // ... remaining methods
}
```

### IrohBlobStore Implementation

```rust
use iroh_blobs::store::Store;
use iroh_blobs::Hash;

pub struct IrohBlobStore {
    store: Store,
}

#[async_trait::async_trait]
impl BlobStore for IrohBlobStore {
    async fn create_blob(
        &self,
        path: &Path,
        metadata: BlobMetadata,
    ) -> Result<BlobToken> {
        // Read file and add to store
        let data = tokio::fs::read(path).await?;
        let hash = self.store.add_bytes(data).await?;

        // Store metadata separately (iroh-blobs doesn't have native metadata)
        self.store_metadata(&hash, &metadata).await?;

        let size = self.store.get_size(&hash).await?;

        Ok(BlobToken {
            hash: BlobHash::from_hex(&hash.to_hex()),
            size_bytes: size,
            metadata,
        })
    }

    async fn fetch_blob<F>(
        &self,
        token: &BlobToken,
        mut progress: F,
    ) -> Result<BlobHandle>
    where
        F: FnMut(BlobProgress) + Send + 'static,
    {
        let hash = Hash::from_hex(&token.hash.0)?;
        let total = token.size_bytes;

        progress(BlobProgress::Started { total_bytes: total });

        // iroh-blobs verified streaming
        let mut reader = self.store.read(hash).await?;
        let temp_path = self.temp_path_for(&token.hash);
        let mut file = tokio::fs::File::create(&temp_path).await?;
        let mut downloaded = 0u64;

        while let Some(chunk) = reader.next().await {
            let chunk = chunk?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
            downloaded += chunk.len() as u64;
            progress(BlobProgress::Downloading {
                downloaded_bytes: downloaded,
                total_bytes: total,
            });
        }

        // Move to final location
        let final_path = self.blob_path_for(&token.hash);
        tokio::fs::rename(&temp_path, &final_path).await?;

        progress(BlobProgress::Completed { local_path: final_path.clone() });

        Ok(BlobHandle {
            token: token.clone(),
            path: final_path,
        })
    }

    // ... remaining methods
}
```

## Implementation Phases

### Phase 1: BlobStore Trait (Week 1-2)

**Deliverables:**
- `storage/blob_traits.rs` - Core trait definitions
- `storage/ditto_blob_store.rs` - Ditto implementation
- Unit tests with mock backend
- Integration test with real Ditto

**Success Criteria:**
- Create blob from file
- Fetch blob with progress
- Verify content hash matches

### Phase 2: Document Integration (Week 3)

**Deliverables:**
- `BlobDocumentIntegration` trait
- Ditto implementation storing tokens in documents
- Model registry schema with blob references

**Success Criteria:**
- Store blob token in document
- Document syncs to peer
- Peer fetches blob via token

### Phase 3: FileDistribution API (Week 4-5)

**Deliverables:**
- `FileDistribution` trait
- Ditto implementation using collection subscriptions
- Distribution status tracking

**Success Criteria:**
- Distribute blob to formation
- Track progress per-node
- Cancel in-progress distribution

### Phase 4: ModelDistribution (Week 6-7)

**Deliverables:**
- `ModelDistribution` trait
- Integration with ADR-022 model registry
- Convergence monitoring

**Success Criteria:**
- Distribute ONNX model to platforms
- Select variants based on hardware
- Track convergence status

### Phase 5: iroh-blobs Backend (Week 8)

**Deliverables:**
- `storage/iroh_blob_store.rs`
- Integration with existing Automerge backend

**Success Criteria:**
- Feature parity with Ditto implementation
- Backend-agnostic tests pass with both

## Consequences

### Positive

**Enables AI Model Distribution:**
- Foundation for ADR-013 differential propagation
- Foundation for ADR-022 edge MLOps
- Content-addressed deduplication saves bandwidth

**Backend Agnostic:**
- Same API for Ditto and Iroh backends
- Easy to add new backends (S3, IPFS, etc.)
- Testable with mock implementations

**Progress Visibility:**
- Operators see real-time transfer progress
- Convergence monitoring for deployments
- Blocker identification

**Resilient:**
- Resumable transfers on disconnection
- Content verification ensures integrity
- Automatic retry on failure

### Negative

**Additional Complexity:**
- New trait hierarchy to maintain
- Two sync protocols (documents + blobs)
- Metadata stored separately for iroh-blobs

**Storage Overhead:**
- Blobs not garbage collected immediately
- Need retention policies
- Deduplication requires index

**Ditto-Specific Limitations:**
- 1MB limit on HTTP API (20MB planned)
- Must use SDK for larger files
- Attachment API quirks

### Mitigations

**Complexity:**
- Comprehensive documentation
- Reference implementations
- Integration tests

**Storage:**
- Implement retention policies (ADR-016 TTL)
- Monitor storage usage
- Automatic cleanup

**Ditto Limits:**
- Use SDK for large files
- Chunk very large models (>1GB)
- Monitor Ditto roadmap for increases

## Integration Points

### ADR-005 (DataSync Abstraction)
- `BlobStore` parallels `StorageBackend`
- Both use backend factory pattern
- Can share configuration

### ADR-013 (Distributed Software/AI Ops)
- `ModelDistribution` implements concepts from ADR-013
- Differential propagation uses blob chunking
- Convergence monitoring tracks deployment

### ADR-022 (Edge MLOps)
- Model registry stores blob tokens
- `HiveMLRuntime` uses `fetch_blob()` to load models
- Variant selection uses blob metadata

### ADR-018 (AI Model Capability Advertisement)
- Capability advertisement includes model hash
- Nodes advertise which blobs they have locally
- Enables capability-based model routing

### ADR-006 (Security)
- Blob provenance via signatures on tokens
- Authorization for distribution operations
- Encryption of blob content (future)

## References

### Ditto
- [Working with Attachments](https://docs.ditto.live/sdk/latest/crud/working-with-attachments)
- [Attachment Data Type](https://docs.ditto.live/data-types/attachment)

### Iroh
- [iroh-blobs GitHub](https://github.com/n0-computer/iroh-blobs)
- [iroh-blobs Docs](https://docs.rs/iroh-blobs/latest/iroh_blobs/)
- [Blobs Protocol](https://www.iroh.computer/docs/protocols/blobs)

### Related ADRs
- ADR-005: DataSync Abstraction Layer
- ADR-013: Distributed Software and AI Operations
- ADR-018: AI Model Capability Advertisement
- ADR-022: Edge MLOps Architecture

## Open Questions

### General Blob Transfer

1. **Chunk Size for Large Models**: Should we chunk models >1GB into multiple blobs, or rely on backend chunking?

2. **Metadata Storage for Iroh**: iroh-blobs doesn't support metadata natively. Store in separate collection or embed in filename?

3. **Garbage Collection Policy**: How long to retain blobs after last reference removed? TTL-based (ADR-016)?

4. **Encryption**: Should blob content be encrypted at rest? In transit? Defer to Phase 2?

5. **Bandwidth Throttling**: How to implement transfer priority? Backend-specific or application-level?

### ModelDistribution API Design (Phase 4)

These questions must be answered before implementing Phase 4:

6. **Model Selection Strategy**: How should nodes select which model variant to fetch?
   - Device capability matching (GPU type, memory, CPU arch)?
   - Should we support fallback chains (try CUDA → TensorRT → CPU)?
   - Automatic selection vs explicit operator control?

7. **Push vs Pull Distribution**:
   - **Push model**: Operator deploys model, system pushes to capable nodes
   - **Pull model**: Node requests capability, fetches appropriate model on demand
   - **Hybrid**: Proactive distribution based on mission profile + reactive fetch?

8. **Model Lifecycle Management**:
   - Hot-swapping models during operation without service interruption?
   - Version management and rollback to previous versions?
   - Model validation before activation (hash verification, format check)?

9. **Integration Hooks**: What callbacks should the API provide?
   - Pre-load callbacks (validate model, allocate GPU memory)?
   - Post-load callbacks (register with inference engine, run warmup)?
   - Failure handlers (fallback to CPU, alert operator)?

10. **API Abstraction Depth**:
    - Thin wrapper over FileDistribution (minimal model-specific logic)?
    - Rich model-specific API (variant selection, convergence tracking)?
    - How much should be in hive-protocol vs inference runtime integration?

11. **NodeCapabilities Schema**: What capabilities should nodes advertise?
    ```rust
    pub struct NodeCapabilities {
        pub gpu_memory_gb: Option<f64>,
        pub gpu_type: Option<String>,  // "nvidia", "amd", "apple"
        pub cpu_arch: String,          // "x86_64", "aarch64"
        pub available_storage_mb: u64,
        pub execution_providers: Vec<String>,  // "CUDAExecutionProvider", "CPUExecutionProvider"
    }
    ```
    - How to discover these capabilities at runtime?
    - How to handle capability changes (storage fills up)?

---

**This ADR establishes the foundational file transfer abstraction enabling AI model distribution and software operations across the HIVE mesh network.**
