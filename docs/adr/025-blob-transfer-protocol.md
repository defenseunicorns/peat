# ADR-025: Blob Transfer Protocol

**Status**: Proposed (Revision 2)  
**Date**: 2025-11-25  
**Authors**: Claude, Kit Plummer  
**Relates to**: ADR-005 (DataSync Abstraction), ADR-012 (Schema Definition), ADR-007 (Automerge Sync)

## Context

### The File Transfer Gap

PEAT Protocol provides CRDT-based document synchronization for coordination state (capabilities, commands, events). However, many coordination scenarios require transferring large binary artifacts:

| Use Case | Examples | Size Range |
|----------|----------|------------|
| AI/ML Models | ONNX, TFLite, PyTorch | 50MB - 2GB |
| Software Updates | Binaries, containers, firmware | 10MB - 500MB |
| Configuration | Encrypted bundles, ROE packages | 1KB - 10MB |
| Geospatial Data | Maps, threat libraries, imagery | 50MB - 1GB |
| Training Data | Operational samples (edge → C2) | 1MB - 100MB |

**Current State:**
- `StorageBackend` trait handles document CRUD (ADR-005)
- Documents optimized for small, frequently-updated state (KB to low MB)
- No backend-agnostic mechanism for large binary transfer

**Why Documents Aren't Enough:**

CRDT document sync is optimized for:
- Small JSON-like documents with field-level merging
- Frequent incremental updates
- Conflict resolution semantics

Binary blobs need different handling:
- Large immutable files (no CRDT merging)
- Content-addressed deduplication
- Chunked transfer with resumption
- Progress tracking

Both Ditto and Iroh recognize this distinction with separate APIs:
- **Ditto**: Attachment API (content-addressable, separate sync protocol)
- **Iroh**: `iroh-blobs` crate (BLAKE3 verified streaming)

### Scope Clarification

**This ADR defines PEAT Protocol primitives for blob transfer.**

It does NOT define:
- How applications use blobs (model loading, container execution)
- Distribution orchestration (which nodes get which blobs)
- Application-specific metadata schemas

Those concerns belong to applications built on PEAT (see ADR-026 Reference Implementation).

```
┌─────────────────────────────────────────────────────────────────┐
│  APPLICATION LAYER (out of scope)                                │
│  - ModelDistribution, OrchestrationService                      │
│  - RuntimeAdapters, application-specific schemas                │
└──────────────────────────┬──────────────────────────────────────┘
                           │ uses
═══════════════════════════╪═══════════════════════════════════════
         PEAT PROTOCOL (this ADR)
═══════════════════════════╪═══════════════════════════════════════
                           │
┌──────────────────────────┴──────────────────────────────────────┐
│  BLOB TRANSFER PROTOCOL                                          │
│  - BlobReference schema (content-addressed identifier)          │
│  - BlobStore trait (backend abstraction)                        │
│  - Transfer status tracking                                      │
│  - Integration with document layer (references)                 │
└─────────────────────────────────────────────────────────────────┘
```

## Decision

### Core Principle: Content-Addressed Blob References

Blobs are identified by content hash, enabling:
- **Deduplication**: Same content stored once regardless of name
- **Integrity verification**: Hash validates content correctness
- **Location independence**: Any peer with the hash can provide content

### Schema: BlobReference

The fundamental unit of blob identification in PEAT:

```protobuf
syntax = "proto3";

package peat.blob.v1;

import "google/protobuf/timestamp.proto";

// Content-addressed blob reference
// This is the PEAT protocol's way of identifying binary artifacts
message BlobReference {
  // Content hash (hex-encoded)
  string hash = 1;
  
  // Hash algorithm used ("sha256", "blake3")
  string hash_algorithm = 2;
  
  // Size in bytes (required for transfer planning)
  uint64 size_bytes = 3;
  
  // Optional: application-defined metadata
  // PEAT treats this as opaque - applications define semantics
  map<string, string> metadata = 10;
}

// Transfer status for a blob to/from a specific node
message BlobTransferStatus {
  // Blob being transferred
  string blob_hash = 1;
  
  // Node involved in transfer
  string node_id = 2;
  
  // Current state
  TransferState state = 3;
  
  // Progress
  uint64 bytes_transferred = 4;
  uint64 total_bytes = 5;
  
  // Timing
  google.protobuf.Timestamp started_at = 6;
  google.protobuf.Timestamp updated_at = 7;
  google.protobuf.Timestamp completed_at = 8;
  
  // Error details (if failed)
  string error_message = 9;
}

enum TransferState {
  TRANSFER_STATE_UNSPECIFIED = 0;
  TRANSFER_STATE_PENDING = 1;      // Queued, not started
  TRANSFER_STATE_CONNECTING = 2;   // Establishing connection
  TRANSFER_STATE_TRANSFERRING = 3; // Active transfer
  TRANSFER_STATE_VERIFYING = 4;    // Verifying hash
  TRANSFER_STATE_COMPLETED = 5;    // Successfully transferred
  TRANSFER_STATE_FAILED = 6;       // Transfer failed
  TRANSFER_STATE_CANCELLED = 7;    // Cancelled by request
}

// Blob availability advertisement
// Nodes advertise which blobs they have locally available
message BlobAvailability {
  string node_id = 1;
  google.protobuf.Timestamp advertised_at = 2;
  
  // Blobs available on this node
  repeated BlobSummary available_blobs = 3;
  
  // Total blob storage used
  uint64 storage_used_bytes = 4;
  
  // Storage capacity
  uint64 storage_capacity_bytes = 5;
}

message BlobSummary {
  string hash = 1;
  uint64 size_bytes = 2;
  google.protobuf.Timestamp acquired_at = 3;
  
  // Last time this blob was accessed (for LRU eviction)
  google.protobuf.Timestamp last_accessed_at = 4;
}
```

### Trait: BlobStore

Backend-agnostic interface for blob operations:

```rust
//! Blob storage trait for content-addressed binary transfer
//!
//! This trait abstracts over backend-specific blob storage implementations
//! (Ditto Attachments, iroh-blobs, etc.) providing a unified interface
//! for the PEAT protocol layer.
//!
//! # Design Principles
//!
//! 1. **Content-Addressed**: Blobs identified by hash, not name
//! 2. **Backend-Agnostic**: Same API regardless of underlying storage
//! 3. **Progress-Aware**: All transfers report progress for monitoring
//! 4. **Resumable**: Interrupted transfers can resume from last position
//!
//! # Non-Goals
//!
//! This trait does NOT handle:
//! - Distribution orchestration (which nodes get which blobs)
//! - Application-specific blob semantics (models, containers, etc.)
//! - Priority scheduling across multiple transfers
//!
//! Those concerns belong to application-layer code built on this primitive.

use std::path::{Path, PathBuf};
use anyhow::Result;
use async_trait::async_trait;

/// Content-addressed blob identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlobHash {
    /// Hex-encoded hash value
    pub hash: String,
    /// Algorithm used ("sha256", "blake3")
    pub algorithm: String,
}

impl BlobHash {
    pub fn sha256(hex: &str) -> Self {
        Self { hash: hex.to_string(), algorithm: "sha256".to_string() }
    }
    
    pub fn blake3(hex: &str) -> Self {
        Self { hash: hex.to_string(), algorithm: "blake3".to_string() }
    }
}

/// Reference to a blob with size and optional metadata
#[derive(Clone, Debug)]
pub struct BlobRef {
    /// Content hash
    pub hash: BlobHash,
    /// Size in bytes
    pub size_bytes: u64,
    /// Application-defined metadata (opaque to BlobStore)
    pub metadata: std::collections::HashMap<String, String>,
}

impl BlobRef {
    /// Create a minimal reference (hash + size only)
    pub fn new(hash: BlobHash, size_bytes: u64) -> Self {
        Self { hash, size_bytes, metadata: Default::default() }
    }
    
    /// Create reference with metadata
    pub fn with_metadata(
        hash: BlobHash, 
        size_bytes: u64,
        metadata: std::collections::HashMap<String, String>,
    ) -> Self {
        Self { hash, size_bytes, metadata }
    }
}

/// Progress updates during blob operations
#[derive(Clone, Debug)]
pub enum TransferProgress {
    /// Transfer started
    Started { total_bytes: u64 },
    /// Transfer in progress
    Progress { 
        bytes_transferred: u64, 
        total_bytes: u64,
        bytes_per_second: Option<f64>,
    },
    /// Verifying content hash
    Verifying,
    /// Transfer complete, blob available locally
    Completed { local_path: PathBuf },
    /// Transfer failed
    Failed { error: String, resumable: bool },
}

/// Handle to a locally available blob
pub struct LocalBlob {
    /// Reference to the blob
    pub blob_ref: BlobRef,
    /// Path to content on local filesystem
    pub path: PathBuf,
}

/// Content-addressed blob storage interface
///
/// # Implementations
///
/// - `DittoBlobStore`: Uses Ditto Attachments API
/// - `IrohBlobStore`: Uses iroh-blobs with BLAKE3
/// - `FilesystemBlobStore`: Simple local storage (testing)
///
/// # Thread Safety
///
/// All methods are safe to call from multiple threads.
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store a blob from a local file
    ///
    /// Reads the file, computes content hash, and stores in blob storage.
    /// The blob becomes available to other nodes via the mesh.
    ///
    /// # Arguments
    /// * `path` - Path to source file
    /// * `metadata` - Application-defined metadata (stored with blob)
    ///
    /// # Returns
    /// Reference to the stored blob (hash + size + metadata)
    async fn store_file(
        &self,
        path: &Path,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef>;
    
    /// Store a blob from bytes
    ///
    /// Useful for programmatically generated content.
    async fn store_bytes(
        &self,
        data: &[u8],
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef>;
    
    /// Fetch a blob, reporting progress
    ///
    /// If blob exists locally, returns immediately.
    /// Otherwise, fetches from mesh peers via backend-specific protocol.
    ///
    /// # Arguments
    /// * `blob_ref` - Reference to blob to fetch
    /// * `progress` - Callback for progress updates
    ///
    /// # Returns
    /// Handle providing local filesystem path to blob content
    async fn fetch<F>(
        &self,
        blob_ref: &BlobRef,
        progress: F,
    ) -> Result<LocalBlob>
    where
        F: FnMut(TransferProgress) + Send + 'static;
    
    /// Check if blob exists locally (no network fetch)
    fn exists_locally(&self, hash: &BlobHash) -> bool;
    
    /// Get blob info without fetching
    ///
    /// Returns metadata about a known blob, or None if unknown.
    fn get_info(&self, hash: &BlobHash) -> Option<BlobRef>;
    
    /// Delete a blob from local storage
    ///
    /// Does not affect other peers. If blob is referenced by documents
    /// in active sync, it may be re-fetched automatically.
    async fn delete(&self, hash: &BlobHash) -> Result<()>;
    
    /// List all locally available blobs
    fn list_local(&self) -> Vec<BlobRef>;
    
    /// Get total local storage used by blobs
    fn storage_used(&self) -> u64;
}
```

### Document-Blob Integration

Blobs are referenced from CRDT documents, creating the bridge between document sync and blob transfer:

```rust
/// Extension trait for storing blob references in documents
///
/// This enables the pattern:
/// 1. Store blob via BlobStore
/// 2. Store BlobRef in document field
/// 3. Document syncs to peers via CRDT
/// 4. Peers see BlobRef and fetch blob on-demand
pub trait BlobDocumentBridge {
    /// Store a blob reference in a document field
    ///
    /// The BlobRef is serialized into the document. When the document
    /// syncs to other nodes, they can extract the BlobRef and fetch
    /// the blob content via BlobStore.
    async fn set_blob_ref(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        blob_ref: &BlobRef,
    ) -> Result<()>;
    
    /// Get a blob reference from a document field
    async fn get_blob_ref(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<Option<BlobRef>>;
}
```

**Usage Pattern:**

```rust
// Node A: Store blob and reference it in a document
let model_ref = blob_store.store_file(
    Path::new("/models/yolov8.onnx"),
    [("name".into(), "YOLOv8 Target Recognition".into())].into(),
).await?;

storage.set_blob_ref(
    "model_registry",
    "target_recognition:4.2.1",
    "model_blob",
    &model_ref,
).await?;

// --- Document syncs via CRDT ---

// Node B: See document, fetch blob
let model_ref = storage.get_blob_ref(
    "model_registry",
    "target_recognition:4.2.1", 
    "model_blob",
).await?.expect("model should exist");

let local_model = blob_store.fetch(&model_ref, |progress| {
    if let TransferProgress::Progress { bytes_transferred, total_bytes, .. } = progress {
        println!("Downloading: {}/{}", bytes_transferred, total_bytes);
    }
}).await?;

// Use model at local_model.path
```

### Backend Implementations

#### DittoBlobStore

```rust
use ditto::prelude::*;
use std::sync::Arc;

/// Ditto Attachments implementation of BlobStore
pub struct DittoBlobStore {
    ditto: Arc<Ditto>,
}

impl DittoBlobStore {
    pub fn new(ditto: Arc<Ditto>) -> Self {
        Self { ditto }
    }
    
    fn attachment_to_blob_ref(&self, attachment: &DittoAttachment) -> BlobRef {
        BlobRef {
            hash: BlobHash::sha256(&attachment.id()),
            size_bytes: attachment.len() as u64,
            metadata: attachment.metadata().clone(),
        }
    }
}

#[async_trait]
impl BlobStore for DittoBlobStore {
    async fn store_file(
        &self,
        path: &Path,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef> {
        let attachment = self.ditto.store()
            .new_attachment(path, metadata)
            .map_err(|e| anyhow::anyhow!("Ditto attachment error: {}", e))?;
        
        Ok(self.attachment_to_blob_ref(&attachment))
    }
    
    async fn store_bytes(
        &self,
        data: &[u8],
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef> {
        // Ditto requires file path, so write to temp file
        let temp_path = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        tokio::fs::write(&temp_path, data).await?;
        
        let result = self.store_file(&temp_path, metadata).await;
        let _ = tokio::fs::remove_file(&temp_path).await;
        
        result
    }
    
    async fn fetch<F>(
        &self,
        blob_ref: &BlobRef,
        mut progress: F,
    ) -> Result<LocalBlob>
    where
        F: FnMut(TransferProgress) + Send + 'static,
    {
        // Get attachment token from hash
        let token = DittoAttachmentToken::from_id(&blob_ref.hash.hash)?;
        
        progress(TransferProgress::Started { total_bytes: blob_ref.size_bytes });
        
        let fetcher = self.ditto.store().fetch_attachment(token, move |event| {
            match event {
                DittoAttachmentFetchEvent::Progress { downloaded_bytes, total_bytes } => {
                    progress(TransferProgress::Progress {
                        bytes_transferred: downloaded_bytes,
                        total_bytes,
                        bytes_per_second: None, // Ditto doesn't provide this
                    });
                }
                DittoAttachmentFetchEvent::Completed { attachment } => {
                    // Attachment available - get path
                    progress(TransferProgress::Completed { 
                        local_path: attachment.path().to_path_buf() 
                    });
                }
                DittoAttachmentFetchEvent::Deleted => {
                    progress(TransferProgress::Failed {
                        error: "Attachment deleted during fetch".into(),
                        resumable: false,
                    });
                }
            }
        })?;
        
        // Wait for completion
        let attachment = fetcher.await?;
        
        Ok(LocalBlob {
            blob_ref: blob_ref.clone(),
            path: attachment.path().to_path_buf(),
        })
    }
    
    fn exists_locally(&self, hash: &BlobHash) -> bool {
        // Check Ditto's local attachment storage
        self.ditto.store()
            .attachment_exists(&hash.hash)
            .unwrap_or(false)
    }
    
    fn get_info(&self, hash: &BlobHash) -> Option<BlobRef> {
        self.ditto.store()
            .get_attachment_info(&hash.hash)
            .ok()
            .map(|info| BlobRef {
                hash: hash.clone(),
                size_bytes: info.len() as u64,
                metadata: info.metadata().clone(),
            })
    }
    
    async fn delete(&self, hash: &BlobHash) -> Result<()> {
        // Ditto handles GC automatically, but we can hint
        // that we no longer need this attachment
        Ok(())
    }
    
    fn list_local(&self) -> Vec<BlobRef> {
        self.ditto.store()
            .list_attachments()
            .unwrap_or_default()
            .into_iter()
            .map(|a| self.attachment_to_blob_ref(&a))
            .collect()
    }
    
    fn storage_used(&self) -> u64 {
        self.ditto.store()
            .attachments_storage_used()
            .unwrap_or(0)
    }
}
```

#### IrohBlobStore

```rust
use iroh_blobs::store::Store;
use iroh_blobs::Hash;

/// iroh-blobs implementation of BlobStore
pub struct IrohBlobStore {
    store: Store,
    local_path: PathBuf,
}

impl IrohBlobStore {
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        let store = Store::load(&data_dir).await?;
        Ok(Self { store, local_path: data_dir })
    }
}

#[async_trait]
impl BlobStore for IrohBlobStore {
    async fn store_file(
        &self,
        path: &Path,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef> {
        // iroh-blobs uses BLAKE3
        let outcome = self.store.import_file(path).await?;
        
        // iroh-blobs doesn't store metadata natively
        // Store in sidecar file or separate collection
        let hash = outcome.hash;
        let size = outcome.size;
        
        // Store metadata in sidecar (simple approach)
        if !metadata.is_empty() {
            let meta_path = self.local_path
                .join("metadata")
                .join(hash.to_hex().to_string());
            tokio::fs::create_dir_all(meta_path.parent().unwrap()).await?;
            let meta_json = serde_json::to_string(&metadata)?;
            tokio::fs::write(&meta_path, meta_json).await?;
        }
        
        Ok(BlobRef {
            hash: BlobHash::blake3(&hash.to_hex()),
            size_bytes: size,
            metadata,
        })
    }
    
    async fn store_bytes(
        &self,
        data: &[u8],
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<BlobRef> {
        let outcome = self.store.import_bytes(data.to_vec().into()).await?;
        
        Ok(BlobRef {
            hash: BlobHash::blake3(&outcome.hash.to_hex()),
            size_bytes: outcome.size,
            metadata,
        })
    }
    
    async fn fetch<F>(
        &self,
        blob_ref: &BlobRef,
        mut progress: F,
    ) -> Result<LocalBlob>
    where
        F: FnMut(TransferProgress) + Send + 'static,
    {
        let hash = Hash::from_hex(&blob_ref.hash.hash)?;
        
        progress(TransferProgress::Started { total_bytes: blob_ref.size_bytes });
        
        // iroh-blobs provides verified streaming
        let mut reader = self.store.export(hash).await?;
        let local_path = self.local_path.join("blobs").join(hash.to_hex().to_string());
        tokio::fs::create_dir_all(local_path.parent().unwrap()).await?;
        
        let mut file = tokio::fs::File::create(&local_path).await?;
        let mut transferred = 0u64;
        let start = std::time::Instant::now();
        
        while let Some(chunk) = reader.next().await {
            let chunk = chunk?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
            transferred += chunk.len() as u64;
            
            let elapsed = start.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 { transferred as f64 / elapsed } else { 0.0 };
            
            progress(TransferProgress::Progress {
                bytes_transferred: transferred,
                total_bytes: blob_ref.size_bytes,
                bytes_per_second: Some(rate),
            });
        }
        
        progress(TransferProgress::Verifying);
        // iroh-blobs verifies during streaming, so we're good
        
        progress(TransferProgress::Completed { local_path: local_path.clone() });
        
        Ok(LocalBlob {
            blob_ref: blob_ref.clone(),
            path: local_path,
        })
    }
    
    fn exists_locally(&self, hash: &BlobHash) -> bool {
        let Ok(h) = Hash::from_hex(&hash.hash) else { return false };
        self.store.contains(&h)
    }
    
    fn get_info(&self, hash: &BlobHash) -> Option<BlobRef> {
        let h = Hash::from_hex(&hash.hash).ok()?;
        let entry = self.store.get(&h)?;
        
        // Load metadata from sidecar
        let meta_path = self.local_path
            .join("metadata")
            .join(hash.hash.clone());
        let metadata = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        
        Some(BlobRef {
            hash: hash.clone(),
            size_bytes: entry.size(),
            metadata,
        })
    }
    
    async fn delete(&self, hash: &BlobHash) -> Result<()> {
        let h = Hash::from_hex(&hash.hash)?;
        self.store.delete(&h).await?;
        
        // Remove metadata sidecar
        let meta_path = self.local_path
            .join("metadata")
            .join(&hash.hash);
        let _ = tokio::fs::remove_file(&meta_path).await;
        
        Ok(())
    }
    
    fn list_local(&self) -> Vec<BlobRef> {
        self.store.list()
            .filter_map(|entry| {
                let hash = BlobHash::blake3(&entry.hash().to_hex());
                self.get_info(&hash)
            })
            .collect()
    }
    
    fn storage_used(&self) -> u64 {
        self.store.list()
            .map(|e| e.size())
            .sum()
    }
}
```

## Implementation Phases

### Phase 1: Core Trait & Schema (Week 1)
- `BlobRef`, `BlobHash` types
- `BlobStore` trait definition
- Protobuf schema (`peat.blob.v1`)
- Unit tests with mock implementation

### Phase 2: Ditto Backend (Week 2)
- `DittoBlobStore` implementation
- Integration with existing Ditto infrastructure
- E2E test: store on Node A, fetch on Node B

### Phase 3: Document Bridge (Week 3)
- `BlobDocumentBridge` trait
- Ditto implementation using attachment references
- Test: document sync triggers blob availability

### Phase 4: Iroh Backend (Week 4)
- `IrohBlobStore` implementation
- Metadata sidecar handling
- Backend-agnostic test suite passes with both

## Consequences

### Positive

**Protocol-Level Primitive:**
- Clean abstraction usable by any application
- Doesn't prescribe application semantics
- Enables diverse use cases (models, containers, configs)

**Backend Agnostic:**
- Same API for Ditto and Iroh
- Easy to add new backends
- Testable with mock implementations

**Content-Addressed:**
- Automatic deduplication
- Integrity verification built-in
- Location-independent references

### Negative

**Two Sync Protocols:**
- Documents sync via CRDT
- Blobs sync via backend-specific protocol
- Coordination complexity

**Metadata Handling:**
- iroh-blobs lacks native metadata
- Sidecar files add complexity
- Consistency challenges

### Mitigations

**Sync Coordination:**
- Document-blob bridge ensures references sync before fetches
- Progress tracking for visibility
- Clear documentation of sync semantics

**Metadata:**
- Abstract behind BlobRef
- Backend-specific storage strategies
- Consider promoting to iroh-blobs upstream

## What This ADR Does NOT Cover

The following are explicitly out of scope for this ADR:

1. **Distribution Orchestration**: How to distribute blobs to multiple nodes (see ADR-026 Reference Implementation)

2. **Application Semantics**: What blobs mean (models, containers, etc.) - applications define this

3. **Priority Scheduling**: Which transfers happen first - application-layer concern

4. **Lifecycle Management**: When to delete blobs, versioning - application-layer concern

5. **Security**: Encryption, access control - see ADR-006, may warrant separate blob security ADR

## References

### Ditto
- [Working with Attachments](https://docs.ditto.live/sdk/latest/crud/working-with-attachments)
- [Attachment Data Type](https://docs.ditto.live/data-types/attachment)

### Iroh
- [iroh-blobs GitHub](https://github.com/n0-computer/iroh-blobs)
- [iroh-blobs Docs](https://docs.rs/iroh-blobs/latest/iroh_blobs/)

### Related ADRs
- ADR-005: DataSync Abstraction Layer
- ADR-007: Automerge-Based Sync Engine
- ADR-012: Schema Definition and Protocol Extensibility
- ADR-026: Reference Implementation - Software Orchestration

---

**This ADR establishes the PEAT Protocol primitive for content-addressed blob transfer, enabling applications to distribute large binary artifacts through the mesh network.**
