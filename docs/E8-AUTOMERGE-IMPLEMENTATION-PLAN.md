# E8 Automerge Backend Implementation Plan

**Status**: In Progress
**Date**: 2025-11-05
**Goal**: Implement AutomergeBackend as the first CRDT backend for E8 evaluation (ADR-007)

## Overview

This plan details the implementation of `AutomergeBackend` that implements our existing `DataSyncBackend` trait system. This backend will use the Automerge CRDT library for state synchronization, providing an alternative to Ditto.

## Prerequisites

✅ **Completed**:
- Abstraction layer traits defined (DocumentStore, PeerDiscovery, SyncEngine, DataSyncBackend)
- DittoBackend reference implementation working
- CellStore<B: DataSyncBackend> and NodeStore<B: DataSyncBackend> refactored
- Integration tests for sync backend abstraction
- All existing tests passing

## Phase 1: Automerge Integration Setup

### Task 1.1: Add Automerge Dependency

**File**: `cap-protocol/Cargo.toml`

```toml
[dependencies]
# Existing dependencies...

# CRDT Backend - Automerge (E8 Evaluation)
automerge = "0.7.1"  # Stable version for production use
```

**Verification**:
```bash
cargo check -p cap-protocol
```

### Task 1.2: Create AutomergeBackend Module

**File**: `cap-protocol/src/sync/automerge.rs`

**Initial Structure**:
```rust
//! Automerge-based implementation of DataSyncBackend
//!
//! This module provides a CRDT backend using the Automerge library,
//! enabling eventual consistency without requiring Ditto SDK.

use automerge::{Automerge, ReadDoc, transaction::Transactable, sync};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;

use crate::sync::traits::*;
use crate::sync::types::*;
use crate::error::{Error, Result};

/// Automerge-based backend for CRDT synchronization
pub struct AutomergeBackend {
    /// Automerge documents indexed by collection + document ID
    documents: Arc<Mutex<HashMap<String, Automerge>>>,

    /// Sync states for each peer connection
    sync_states: Arc<Mutex<HashMap<String, sync::State>>>,

    /// Peer connection callbacks
    peer_callbacks: PeerCallbacks,

    /// Configuration
    config: Arc<Mutex<Option<BackendConfig>>>,

    /// Initialized flag
    initialized: Arc<Mutex<bool>>,
}

impl AutomergeBackend {
    /// Create new AutomergeBackend
    pub fn new() -> Self {
        Self {
            documents: Arc::new(Mutex::new(HashMap::new())),
            sync_states: Arc::new(Mutex::new(HashMap::new())),
            peer_callbacks: Arc::new(Mutex::new(Vec::new())),
            config: Arc::new(Mutex::new(None)),
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Helper: Get document key
    fn doc_key(collection: &str, doc_id: &DocumentId) -> String {
        format!("{}:{}", collection, doc_id)
    }

    /// Helper: Convert Automerge document to our Document type
    fn automerge_to_document(
        doc: &Automerge,
        doc_id: DocumentId,
    ) -> Result<Document> {
        // Export to JSON
        let json = automerge::export(doc)
            .map_err(|e| Error::Internal(format!("Failed to export Automerge doc: {}", e)))?;

        // Convert JSON to fields HashMap
        let fields = json.as_object()
            .ok_or_else(|| Error::Internal("Document root must be object".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(Document {
            id: Some(doc_id),
            fields,
            updated_at: std::time::SystemTime::now(),
        })
    }

    /// Helper: Apply Document fields to Automerge doc
    fn apply_document_to_automerge(
        doc: &mut Automerge,
        document: &Document,
    ) -> Result<()> {
        doc.transact(|tx| {
            let root = tx.root();

            for (key, value) in &document.fields {
                // Convert serde_json::Value to Automerge scalar
                match value {
                    serde_json::Value::String(s) => {
                        tx.put(root, key, s.clone())?;
                    }
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            tx.put(root, key, i)?;
                        } else if let Some(f) = n.as_f64() {
                            tx.put(root, key, f)?;
                        }
                    }
                    serde_json::Value::Bool(b) => {
                        tx.put(root, key, *b)?;
                    }
                    serde_json::Value::Null => {
                        // Skip null values or represent as None
                    }
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                        // For complex types, serialize as JSON string
                        let json_str = serde_json::to_string(value)?;
                        tx.put(root, key, json_str)?;
                    }
                }
            }

            Ok(())
        }).map_err(|e| Error::Internal(format!("Transaction failed: {}", e)))
    }
}

impl Default for AutomergeBackend {
    fn default() -> Self {
        Self::new()
    }
}
```

**Verification**:
```bash
cargo check -p cap-protocol
```

### Task 1.3: Update Module Exports

**File**: `cap-protocol/src/sync/mod.rs`

Add:
```rust
pub mod automerge;
```

## Phase 2: Implement DocumentStore Trait

### Task 2.1: Implement Core CRUD Operations

**File**: `cap-protocol/src/sync/automerge.rs`

```rust
#[async_trait]
impl DocumentStore for AutomergeBackend {
    async fn upsert(&self, collection: &str, mut document: Document) -> Result<DocumentId> {
        // Generate ID if not present
        let doc_id = document.id.clone().unwrap_or_else(|| {
            uuid::Uuid::new_v4().to_string()
        });

        let key = Self::doc_key(collection, &doc_id);
        let mut docs = self.documents.lock().unwrap();

        if let Some(existing_doc) = docs.get_mut(&key) {
            // Update existing document
            Self::apply_document_to_automerge(existing_doc, &document)?;
        } else {
            // Create new document
            let mut automerge_doc = Automerge::new();
            Self::apply_document_to_automerge(&mut automerge_doc, &document)?;
            docs.insert(key, automerge_doc);
        }

        document.id = Some(doc_id.clone());
        Ok(doc_id)
    }

    async fn query(&self, collection: &str, query: &Query) -> Result<Vec<Document>> {
        let docs = self.documents.lock().unwrap();
        let mut results = Vec::new();

        // Iterate all documents in collection
        for (key, automerge_doc) in docs.iter() {
            if !key.starts_with(&format!("{}:", collection)) {
                continue;
            }

            // Extract document ID from key
            let doc_id = key.split(':').nth(1).unwrap_or("").to_string();

            // Convert to our Document type
            let document = Self::automerge_to_document(automerge_doc, doc_id)?;

            // Apply query filter
            if self.matches_query(&document, query)? {
                results.push(document);
            }
        }

        Ok(results)
    }

    async fn remove(&self, collection: &str, doc_id: &DocumentId) -> Result<()> {
        let key = Self::doc_key(collection, doc_id);
        let mut docs = self.documents.lock().unwrap();

        docs.remove(&key)
            .ok_or_else(|| Error::NotFound(format!("Document {} not found", doc_id)))?;

        Ok(())
    }

    async fn get(&self, collection: &str, doc_id: &DocumentId) -> Result<Option<Document>> {
        let key = Self::doc_key(collection, doc_id);
        let docs = self.documents.lock().unwrap();

        if let Some(automerge_doc) = docs.get(&key) {
            let document = Self::automerge_to_document(automerge_doc, doc_id.clone())?;
            Ok(Some(document))
        } else {
            Ok(None)
        }
    }

    async fn count(&self, collection: &str, query: &Query) -> Result<usize> {
        let results = self.query(collection, query).await?;
        Ok(results.len())
    }
}

impl AutomergeBackend {
    /// Helper: Check if document matches query
    fn matches_query(&self, document: &Document, query: &Query) -> Result<bool> {
        match query {
            Query::All => Ok(true),

            Query::Eq { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(doc_value == value)
                } else {
                    Ok(false)
                }
            }

            Query::Lt { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(self.compare_values(doc_value, value)? < 0)
                } else {
                    Ok(false)
                }
            }

            Query::Gt { field, value } => {
                if let Some(doc_value) = document.fields.get(field) {
                    Ok(self.compare_values(doc_value, value)? > 0)
                } else {
                    Ok(false)
                }
            }

            Query::And(queries) => {
                for q in queries {
                    if !self.matches_query(document, q)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }

            Query::Or(queries) => {
                for q in queries {
                    if self.matches_query(document, q)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }

            Query::Custom(_) => {
                // Custom queries not supported in initial implementation
                Err(Error::Unsupported("Custom queries not yet supported".into()))
            }
        }
    }

    /// Helper: Compare two JSON values
    fn compare_values(&self, a: &serde_json::Value, b: &serde_json::Value) -> Result<i8> {
        use serde_json::Value;

        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                if let (Some(a_f), Some(b_f)) = (a_num.as_f64(), b_num.as_f64()) {
                    if a_f < b_f { Ok(-1) }
                    else if a_f > b_f { Ok(1) }
                    else { Ok(0) }
                } else {
                    Err(Error::Internal("Number comparison failed".into()))
                }
            }
            (Value::String(a_str), Value::String(b_str)) => {
                Ok(a_str.cmp(b_str) as i8)
            }
            _ => Err(Error::Internal("Unsupported value comparison".into()))
        }
    }
}
```

## Phase 3: Implement SyncEngine Trait

### Task 3.1: Implement Automerge Sync Protocol

**Key Insight**: Automerge provides a built-in sync protocol via `automerge::sync` module.

```rust
#[async_trait]
impl SyncEngine for AutomergeBackend {
    async fn start_sync(&self) -> Result<()> {
        // For Automerge, sync is pull-based via generate/receive_sync_message
        // This method indicates we're ready to sync
        Ok(())
    }

    async fn stop_sync(&self) -> Result<()> {
        // Clean up sync states
        self.sync_states.lock().unwrap().clear();
        Ok(())
    }

    async fn subscribe(&self, collection: &str, query: &Query) -> Result<SyncSubscription> {
        // Create subscription handle
        // For Automerge, subscriptions are logical - we track interest
        Ok(SyncSubscription::new(
            collection.to_string(),
            Box::new(AutomergeSubscriptionHandle {
                collection: collection.to_string(),
                query: query.clone(),
            })
        ))
    }

    async fn unsubscribe(&self, _subscription: SyncSubscription) -> Result<()> {
        // Logical cleanup
        Ok(())
    }
}

/// Subscription handle for Automerge
struct AutomergeSubscriptionHandle {
    collection: String,
    query: Query,
}
```

### Task 3.2: Implement Sync Message Generation/Receipt

Add helper methods:

```rust
impl AutomergeBackend {
    /// Generate sync message for a document
    pub fn generate_sync_message(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        peer_id: &str,
    ) -> Result<Vec<u8>> {
        let key = Self::doc_key(collection, doc_id);
        let docs = self.documents.lock().unwrap();

        let automerge_doc = docs.get(&key)
            .ok_or_else(|| Error::NotFound(format!("Document {} not found", doc_id)))?;

        // Get or create sync state for this peer
        let mut sync_states = self.sync_states.lock().unwrap();
        let sync_state = sync_states
            .entry(format!("{}:{}", peer_id, key))
            .or_insert_with(|| sync::State::new());

        // Generate sync message
        let message = automerge_doc.generate_sync_message(sync_state)
            .map_err(|e| Error::Internal(format!("Sync message generation failed: {}", e)))?;

        // Encode message
        message.encode()
            .map_err(|e| Error::Internal(format!("Message encoding failed: {}", e)))
    }

    /// Receive and apply sync message
    pub fn receive_sync_message(
        &self,
        collection: &str,
        doc_id: &DocumentId,
        peer_id: &str,
        message: &[u8],
    ) -> Result<()> {
        let key = Self::doc_key(collection, doc_id);
        let mut docs = self.documents.lock().unwrap();

        let automerge_doc = docs.get_mut(&key)
            .ok_or_else(|| Error::NotFound(format!("Document {} not found", doc_id)))?;

        // Decode message
        let sync_message = sync::Message::decode(message)
            .map_err(|e| Error::Internal(format!("Message decode failed: {}", e)))?;

        // Get sync state
        let mut sync_states = self.sync_states.lock().unwrap();
        let sync_state = sync_states
            .entry(format!("{}:{}", peer_id, key))
            .or_insert_with(|| sync::State::new());

        // Apply sync message
        automerge_doc.receive_sync_message(sync_state, sync_message)
            .map_err(|e| Error::Internal(format!("Sync message apply failed: {}", e)))?;

        Ok(())
    }
}
```

## Phase 4: Implement PeerDiscovery Trait

### Task 4.1: Minimal PeerDiscovery Implementation

For initial implementation, use manual peer configuration (like DittoBackend):

```rust
#[async_trait]
impl PeerDiscovery for AutomergeBackend {
    async fn start_discovery(&self, _config: DiscoveryConfig) -> Result<()> {
        // Manual peer discovery only for now
        Ok(())
    }

    async fn stop_discovery(&self) -> Result<()> {
        Ok(())
    }

    async fn discovered_peers(&self) -> Result<Vec<PeerInfo>> {
        // Return empty - manual configuration required
        Ok(Vec::new())
    }

    fn register_peer_callback(&self, callback: PeerCallback) {
        self.peer_callbacks.lock().unwrap().push(callback);
    }
}
```

## Phase 5: Implement DataSyncBackend Trait

### Task 5.1: Backend Lifecycle

```rust
#[async_trait]
impl DataSyncBackend for AutomergeBackend {
    async fn initialize(&self, config: BackendConfig) -> Result<()> {
        let mut initialized = self.initialized.lock().unwrap();
        if *initialized {
            return Err(Error::Internal("Already initialized".into()));
        }

        *self.config.lock().unwrap() = Some(config);
        *initialized = true;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.stop_sync().await?;
        self.documents.lock().unwrap().clear();
        self.sync_states.lock().unwrap().clear();
        *self.initialized.lock().unwrap() = false;

        Ok(())
    }

    fn document_store(&self) -> Arc<dyn DocumentStore> {
        Arc::new(self.clone())
    }

    fn peer_discovery(&self) -> Arc<dyn PeerDiscovery> {
        Arc::new(self.clone())
    }

    fn sync_engine(&self) -> Arc<dyn SyncEngine> {
        Arc::new(self.clone())
    }

    async fn is_ready(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    fn backend_info(&self) -> BackendInfo {
        BackendInfo {
            name: "Automerge".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            features: vec![
                "CRDT".to_string(),
                "Eventual Consistency".to_string(),
                "Columnar Encoding".to_string(),
            ],
        }
    }
}

// Implement Clone for Arc wrapping
impl Clone for AutomergeBackend {
    fn clone(&self) -> Self {
        Self {
            documents: Arc::clone(&self.documents),
            sync_states: Arc::clone(&self.sync_states),
            peer_callbacks: Arc::clone(&self.peer_callbacks),
            config: Arc::clone(&self.config),
            initialized: Arc::clone(&self.initialized),
        }
    }
}
```

## Phase 6: Testing Strategy

### Task 6.1: Unit Tests

**File**: `cap-protocol/src/sync/automerge.rs` (bottom of file)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_automerge_backend_initialization() {
        let backend = AutomergeBackend::new();
        assert!(!backend.is_ready().await);

        let config = BackendConfig::default();
        backend.initialize(config).await.unwrap();
        assert!(backend.is_ready().await);
    }

    #[tokio::test]
    async fn test_document_upsert() {
        let backend = AutomergeBackend::new();
        backend.initialize(BackendConfig::default()).await.unwrap();

        let mut fields = HashMap::new();
        fields.insert("name".to_string(), serde_json::json!("test"));
        fields.insert("value".to_string(), serde_json::json!(42));

        let doc = Document::new(fields);
        let doc_id = backend.document_store()
            .upsert("test_collection", doc)
            .await
            .unwrap();

        assert!(!doc_id.is_empty());
    }

    #[tokio::test]
    async fn test_document_query() {
        let backend = AutomergeBackend::new();
        backend.initialize(BackendConfig::default()).await.unwrap();

        // Insert test document
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), serde_json::json!("active"));
        let doc = Document::new(fields);
        backend.document_store()
            .upsert("test_collection", doc)
            .await
            .unwrap();

        // Query
        let query = Query::Eq {
            field: "status".to_string(),
            value: serde_json::json!("active"),
        };

        let results = backend.document_store()
            .query("test_collection", &query)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_sync_message_generation() {
        let backend = AutomergeBackend::new();
        backend.initialize(BackendConfig::default()).await.unwrap();

        // Create document
        let mut fields = HashMap::new();
        fields.insert("data".to_string(), serde_json::json!("test"));
        let doc = Document::new(fields);
        let doc_id = backend.document_store()
            .upsert("sync_test", doc)
            .await
            .unwrap();

        // Generate sync message
        let message = backend.generate_sync_message("sync_test", &doc_id, "peer1").unwrap();
        assert!(!message.is_empty());
    }
}
```

### Task 6.2: Integration Tests

**File**: `cap-protocol/tests/automerge_backend_integration.rs`

```rust
use cap_protocol::sync::automerge::AutomergeBackend;
use cap_protocol::sync::types::*;
use cap_protocol::sync::traits::*;
use std::collections::HashMap;

#[tokio::test]
async fn test_automerge_two_peer_sync() {
    // Create two backends (simulating two peers)
    let backend1 = AutomergeBackend::new();
    let backend2 = AutomergeBackend::new();

    backend1.initialize(BackendConfig::default()).await.unwrap();
    backend2.initialize(BackendConfig::default()).await.unwrap();

    // Backend1: Create and insert document
    let mut fields = HashMap::new();
    fields.insert("cell_id".to_string(), serde_json::json!("cell_1"));
    fields.insert("leader".to_string(), serde_json::json!("node_1"));

    let doc = Document::new(fields);
    let doc_id = backend1.document_store()
        .upsert("cells", doc)
        .await
        .unwrap();

    // Sync: Backend1 → Backend2
    let sync_msg = backend1.generate_sync_message("cells", &doc_id, "peer2").unwrap();

    // Backend2: Create empty document first
    let empty_doc = Document::new(HashMap::new());
    backend2.document_store()
        .upsert("cells", empty_doc)
        .await
        .unwrap();

    // Backend2: Receive sync message
    backend2.receive_sync_message("cells", &doc_id, "peer1", &sync_msg).unwrap();

    // Verify sync
    let synced_doc = backend2.document_store()
        .get("cells", &doc_id)
        .await
        .unwrap()
        .expect("Document should be synced");

    assert_eq!(
        synced_doc.fields.get("cell_id").unwrap(),
        &serde_json::json!("cell_1")
    );
}
```

## Phase 7: Benchmarking Setup

### Task 7.1: Create CAP-Specific Benchmarks

**File**: `cap-protocol/benches/automerge_benchmarks.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use cap_protocol::sync::automerge::AutomergeBackend;
use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::sync::traits::*;
use cap_protocol::sync::types::*;

fn benchmark_position_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("Position Updates");

    // Benchmark Automerge
    group.bench_function("automerge_100_updates", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let backend = AutomergeBackend::new();
        rt.block_on(backend.initialize(BackendConfig::default())).unwrap();

        b.iter(|| {
            rt.block_on(async {
                for i in 0..100 {
                    let mut fields = HashMap::new();
                    fields.insert("lat".to_string(), serde_json::json!(37.7749 + i as f64 * 0.001));
                    fields.insert("lon".to_string(), serde_json::json!(-122.4194));

                    let doc = Document::new(fields);
                    backend.document_store()
                        .upsert("positions", doc)
                        .await
                        .unwrap();
                }
            });
        });
    });

    // Benchmark Ditto (for comparison)
    // TODO: Add Ditto comparison

    group.finish();
}

fn benchmark_delta_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("Delta Size");

    group.bench_function("automerge_delta_100_updates", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let backend = AutomergeBackend::new();
        rt.block_on(backend.initialize(BackendConfig::default())).unwrap();

        b.iter(|| {
            rt.block_on(async {
                // Create initial doc
                let mut fields = HashMap::new();
                fields.insert("value".to_string(), serde_json::json!(0));
                let doc = Document::new(fields);
                let doc_id = backend.document_store()
                    .upsert("test", doc)
                    .await
                    .unwrap();

                // Make 100 updates
                for i in 1..=100 {
                    let mut fields = HashMap::new();
                    fields.insert("value".to_string(), serde_json::json!(i));
                    let mut doc = Document::new(fields);
                    doc.id = Some(doc_id.clone());
                    backend.document_store()
                        .upsert("test", doc)
                        .await
                        .unwrap();
                }

                // Measure sync message size
                let message = backend.generate_sync_message("test", &doc_id, "peer1").unwrap();
                black_box(message.len())
            });
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_position_updates, benchmark_delta_size);
criterion_main!(benches);
```

## Phase 8: Validation with Existing Tests

### Task 8.1: Update E2E Tests to Support Automerge

Modify `cap-protocol/tests/baseline_ditto_bandwidth_e2e.rs` to add Automerge variant:

```rust
#[tokio::test]
async fn test_automerge_cell_formation_bandwidth() {
    // Similar structure to Ditto test but using AutomergeBackend
    let backend1 = Arc::new(AutomergeBackend::new());
    let backend2 = Arc::new(AutomergeBackend::new());

    backend1.initialize(BackendConfig::default()).await.unwrap();
    backend2.initialize(BackendConfig::default()).await.unwrap();

    let cell_store1: CellStore<AutomergeBackend> =
        CellStore::new(backend1.clone()).await.unwrap();
    let cell_store2: CellStore<AutomergeBackend> =
        CellStore::new(backend2.clone()).await.unwrap();

    // ... rest of test
}
```

## Success Criteria

✅ **Phase 1**: Automerge dependency added, module structure created
✅ **Phase 2**: DocumentStore trait fully implemented, unit tests pass
✅ **Phase 3**: SyncEngine trait implemented, sync messages work
✅ **Phase 4**: PeerDiscovery minimal implementation
✅ **Phase 5**: DataSyncBackend trait complete, lifecycle methods work
✅ **Phase 6**: All unit and integration tests pass
✅ **Phase 7**: Benchmarks running, baseline metrics captured
✅ **Phase 8**: E2E tests pass with AutomergeBackend

## Next Steps After Automerge Implementation

1. **Loro Backend Implementation** - Follow similar pattern
2. **Comparative Benchmarking** - Run both against same scenarios
3. **Decision Matrix Population** - Fill in ADR-007 decision table
4. **Backend Selection** - Choose based on data
5. **Remove Non-Selected Backend** - Clean up codebase

## References

- [Automerge Rust API Docs](https://docs.rs/automerge/latest/automerge/)
- [Automerge 2.0 Blog Post](https://automerge.org/blog/2023/11/06/automerge-2/)
- ADR-007: CRDT Backend Evaluation Framework
- ADR-005: Data Sync Abstraction Layer
- Existing DittoBackend implementation (reference)

---

**Status**: Ready to begin implementation
**Next Task**: Add automerge dependency to Cargo.toml
