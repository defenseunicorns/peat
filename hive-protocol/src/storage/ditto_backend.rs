//! Ditto backend adapter for trait abstraction
//!
//! This module provides an adapter between the StorageBackend/Collection traits
//! and the existing DittoStore implementation. It enables backend-agnostic business
//! logic while preserving all Ditto-specific functionality.
//!
//! # Architecture
//!
//! ```text
//! Business Logic (Coordinators)
//!         ↓
//! StorageBackend trait (backend-agnostic)
//!         ↓
//! DittoBackend (adapter) ← This module
//!         ↓
//! DittoStore (existing Ditto integration)
//!         ↓
//! Ditto SDK
//! ```
//!
//! # Data Format and CRDT Limitations
//!
//! **IMPORTANT**: This generic trait interface uses `Vec<u8>` which **DEFEATS Ditto's CRDT benefits**.
//!
//! The trait stores bytes as base64-encoded blobs, which means:
//! - ❌ No field-level merging (full blob replacement on conflicts)
//! - ❌ No delta sync (entire document sent on any change)
//! - ❌ No OR-Set/LWW-Register semantics
//!
//! **For CRDT benefits, use `DittoStore` methods directly** (not this trait):
//! - `upsert_squad_summary()` - Full JSON expansion with CRDT types
//! - `get_squad_summary()` - Type-safe retrieval
//! - See `ditto_store.rs` for type-specific methods
//!
//! **Conversion** (current base64 approach):
//! - `upsert(bytes)` → encode to base64 → store in JSON {"_id": ..., "data": base64}
//! - `get()` → retrieve JSON → decode base64 → return bytes
//!
//! **Future Work**: See E11.2_STORAGE_SERIALIZATION_ANALYSIS.md Option 1 for typed trait design.
//!
//! # Usage Examples
//!
//! ## ❌ DON'T: Use trait for CRDT-critical data
//!
//! ```ignore
//! // BAD: Generic trait defeats CRDT benefits
//! let backend = DittoBackend::new(store);
//! let collection = backend.collection("squad_summaries");
//! let bytes = summary.encode_to_vec(); // Protobuf bytes
//! collection.upsert("squad-1", bytes)?; // ❌ Stored as base64 blob
//! ```
//!
//! ## ✅ DO: Use DittoStore directly for CRDT benefits
//!
//! ```ignore
//! // GOOD: Type-specific methods use JSON expansion
//! let store = DittoStore::new(config)?;
//! store.upsert_squad_summary("squad-1", &summary).await?; // ✅ Full JSON expansion
//! ```
//!
//! ## When to use each approach:
//!
//! **Use `DittoStore` directly when:**
//! - Data requires CRDT conflict resolution (squad/platoon summaries)
//! - Delta sync is important for bandwidth efficiency
//! - Field-level merging is needed (member lists, positions)
//!
//! **Use trait interface when:**
//! - You need backend-agnostic code (can swap Ditto/Automerge/RocksDB)
//! - Testing with mock implementations
//! - CRDT benefits are not critical for the data type

use super::capabilities::{CrdtCapable, SyncCapable, SyncStats, TypedCollection};
use super::ditto_store::DittoStore;
use super::traits::{Collection as CollectionTrait, DocumentPredicate, StorageBackend};
use anyhow::{Context, Result};
use base64::Engine;
use prost::Message;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

/// Ditto backend adapter implementing StorageBackend trait
///
/// Wraps DittoStore to provide trait-based interface for backend-agnostic code.
pub struct DittoBackend {
    /// Underlying DittoStore instance
    store: Arc<DittoStore>,
}

impl DittoBackend {
    /// Create a new Ditto backend from an existing DittoStore
    ///
    /// # Arguments
    ///
    /// * `store` - Configured and initialized DittoStore instance
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ditto_store = DittoStore::from_env()?;
    /// let backend = DittoBackend::new(Arc::new(ditto_store));
    /// ```
    pub fn new(store: Arc<DittoStore>) -> Self {
        Self { store }
    }

    /// Get access to underlying DittoStore for Ditto-specific operations
    ///
    /// This provides an escape hatch for features not yet abstracted by the trait.
    pub fn ditto_store(&self) -> &DittoStore {
        &self.store
    }
}

impl StorageBackend for DittoBackend {
    fn collection(&self, name: &str) -> Arc<dyn CollectionTrait> {
        Arc::new(DittoCollection {
            name: name.to_string(),
            store: self.store.clone(),
        })
    }

    fn list_collections(&self) -> Vec<String> {
        // Ditto doesn't provide a direct way to list collections
        // Return known collections based on HIVE Protocol schema
        vec![
            "cells".to_string(),
            "nodes".to_string(),
            "capabilities".to_string(),
            "squad_summaries".to_string(),
            "platoon_summaries".to_string(),
            "commands".to_string(),
            "command_acks".to_string(),
        ]
    }

    fn flush(&self) -> Result<()> {
        // Ditto handles persistence automatically
        // No explicit flush needed
        Ok(())
    }

    fn close(self) -> Result<()> {
        // Stop Ditto sync
        self.store.stop_sync();
        Ok(())
    }
}

/// Ditto collection adapter implementing Collection trait
///
/// Provides byte-based CRUD operations on top of Ditto's JSON document model.
struct DittoCollection {
    name: String,
    store: Arc<DittoStore>,
}

impl DittoCollection {
    /// Convert raw bytes to Ditto JSON document
    ///
    /// Encodes bytes as base64 string and wraps in JSON with metadata.
    ///
    /// # Format
    ///
    /// ```json
    /// {
    ///   "_id": "doc-123",
    ///   "data": "base64-encoded-bytes...",
    ///   "type": "binary_document",
    ///   "collection": "cells"
    /// }
    /// ```
    fn bytes_to_json(&self, doc_id: &str, data: &[u8]) -> serde_json::Value {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        serde_json::json!({
            "_id": doc_id,
            "data": base64_data,
            "type": "binary_document",
            "collection": self.name,
        })
    }

    /// Extract raw bytes from Ditto JSON document
    ///
    /// Decodes base64 "data" field from JSON document.
    fn json_to_bytes(&self, json: &serde_json::Value) -> Result<Vec<u8>> {
        let base64_str = json
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'data' field in document"))?;

        base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .context("Failed to decode base64 data")
    }
}

impl CollectionTrait for DittoCollection {
    fn upsert(&self, doc_id: &str, data: Vec<u8>) -> Result<()> {
        let json_doc = self.bytes_to_json(doc_id, &data);

        // Use async runtime to call async upsert
        tokio::runtime::Handle::current()
            .block_on(self.store.upsert(&self.name, json_doc))
            .context("Failed to upsert document")?;

        Ok(())
    }

    fn get(&self, doc_id: &str) -> Result<Option<Vec<u8>>> {
        let where_clause = format!("_id == '{}'", doc_id);

        // Query for document
        let results = tokio::runtime::Handle::current()
            .block_on(self.store.query(&self.name, &where_clause))
            .context("Failed to query document")?;

        if results.is_empty() {
            return Ok(None);
        }

        // Extract bytes from first result
        let bytes = self.json_to_bytes(&results[0])?;
        Ok(Some(bytes))
    }

    fn delete(&self, doc_id: &str) -> Result<()> {
        // Use DQL DELETE statement
        let dql_query = format!("EVICT FROM {} WHERE _id == '{}'", self.name, doc_id);

        tokio::runtime::Handle::current()
            .block_on(async {
                self.store
                    .ditto()
                    .store()
                    .execute_v2(dql_query)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to delete document: {}", e))
            })
            .context("Failed to delete document")?;

        Ok(())
    }

    fn scan(&self) -> Result<Vec<(String, Vec<u8>)>> {
        // Query all documents in collection
        let where_clause = "type == 'binary_document'";

        let results = tokio::runtime::Handle::current()
            .block_on(self.store.query(&self.name, where_clause))
            .context("Failed to scan collection")?;

        let mut documents = Vec::new();
        for json in results {
            let doc_id = json
                .get("_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing '_id' field"))?
                .to_string();

            let bytes = self.json_to_bytes(&json)?;
            documents.push((doc_id, bytes));
        }

        Ok(documents)
    }

    fn find(&self, predicate: DocumentPredicate) -> Result<Vec<(String, Vec<u8>)>> {
        // Scan all documents and filter with predicate
        let all_docs = self.scan()?;
        let filtered = all_docs
            .into_iter()
            .filter(|(_, bytes)| predicate(bytes))
            .collect();
        Ok(filtered)
    }

    fn query_geohash_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        // Query documents by geohash prefix
        // Assumes documents have a "geohash" field indexed by Ditto
        let where_clause = format!("geohash LIKE '{}%'", prefix);

        let results = tokio::runtime::Handle::current()
            .block_on(self.store.query(&self.name, &where_clause))
            .context("Failed to query by geohash")?;

        let mut documents = Vec::new();
        for json in results {
            let doc_id = json
                .get("_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing '_id' field"))?
                .to_string();

            let bytes = self.json_to_bytes(&json)?;
            documents.push((doc_id, bytes));
        }

        Ok(documents)
    }

    fn count(&self) -> Result<usize> {
        let docs = self.scan()?;
        Ok(docs.len())
    }
}

// ============================================================================
// CRDT Capability Implementation
// ============================================================================

/// Typed collection wrapper for Ditto with CRDT optimization
///
/// Converts protobuf messages to JSON for field-level CRDT merging.
struct DittoTypedCollection<M> {
    name: String,
    store: Arc<DittoStore>,
    _phantom: std::marker::PhantomData<M>,
}

impl<M> TypedCollection<M> for DittoTypedCollection<M>
where
    M: Message + Serialize + DeserializeOwned + Default + Clone,
{
    fn upsert(&self, doc_id: &str, message: &M) -> Result<()> {
        // Convert protobuf message to JSON (full expansion for CRDT)
        let mut json =
            serde_json::to_value(message).context("Failed to serialize message to JSON")?;

        // Add Ditto metadata
        json["_id"] = serde_json::Value::String(doc_id.to_string());
        json["type"] = serde_json::Value::String("typed_document".to_string());
        json["collection"] = serde_json::Value::String(self.name.clone());

        // Store with CRDT benefits (OR-Set for arrays, LWW-Register for scalars)
        tokio::runtime::Handle::current()
            .block_on(self.store.upsert(&self.name, json))
            .context("Failed to upsert typed document")?;

        Ok(())
    }

    fn get(&self, doc_id: &str) -> Result<Option<M>> {
        let where_clause = format!("_id == '{}'", doc_id);

        let results = tokio::runtime::Handle::current()
            .block_on(self.store.query(&self.name, &where_clause))
            .context("Failed to query typed document")?;

        if results.is_empty() {
            return Ok(None);
        }

        // JSON → Protobuf
        let message: M = serde_json::from_value(results[0].clone())
            .context("Failed to deserialize message from JSON")?;

        Ok(Some(message))
    }

    fn delete(&self, doc_id: &str) -> Result<()> {
        let dql_query = format!("EVICT FROM {} WHERE _id == '{}'", self.name, doc_id);

        tokio::runtime::Handle::current()
            .block_on(async {
                self.store
                    .ditto()
                    .store()
                    .execute_v2(dql_query)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to delete: {}", e))
            })
            .context("Failed to delete typed document")?;

        Ok(())
    }

    fn scan(&self) -> Result<Vec<(String, M)>> {
        let where_clause = "type == 'typed_document'";

        let results = tokio::runtime::Handle::current()
            .block_on(self.store.query(&self.name, where_clause))
            .context("Failed to scan typed collection")?;

        let mut documents = Vec::new();
        for json in results {
            let doc_id = json
                .get("_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing '_id' field"))?
                .to_string();

            let message: M =
                serde_json::from_value(json).context("Failed to deserialize message")?;

            documents.push((doc_id, message));
        }

        Ok(documents)
    }

    fn find(&self, predicate: Box<dyn Fn(&M) -> bool + Send>) -> Result<Vec<(String, M)>> {
        let all_docs = self.scan()?;
        let filtered = all_docs
            .into_iter()
            .filter(|(_, msg)| predicate(msg))
            .collect();
        Ok(filtered)
    }

    fn count(&self) -> Result<usize> {
        let docs = self.scan()?;
        Ok(docs.len())
    }
}

/// CrdtCapable implementation for DittoBackend
///
/// Enables field-level CRDT merging with OR-Set and LWW-Register semantics.
impl CrdtCapable for DittoBackend {
    fn typed_collection<M>(&self, name: &str) -> Arc<dyn TypedCollection<M>>
    where
        M: Message + Serialize + DeserializeOwned + Default + Clone + 'static,
    {
        Arc::new(DittoTypedCollection::<M> {
            name: name.to_string(),
            store: self.store.clone(),
            _phantom: std::marker::PhantomData,
        })
    }
}

/// SyncCapable implementation for DittoBackend
///
/// Ditto has built-in mesh networking that needs lifecycle management.
impl SyncCapable for DittoBackend {
    fn start_sync(&self) -> Result<()> {
        // Ditto sync is always active once configured
        // This is a no-op but maintains trait compatibility
        Ok(())
    }

    fn stop_sync(&self) -> Result<()> {
        // Ditto doesn't expose explicit stop sync API
        // Sync stops when Ditto instance is dropped
        Ok(())
    }

    fn sync_stats(&self) -> Result<SyncStats> {
        // Ditto doesn't expose detailed sync stats via public API
        // Return basic stats structure
        Ok(SyncStats {
            peer_count: 0, // Would require Ditto API enhancement
            bytes_sent: 0,
            bytes_received: 0,
            last_sync: None,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

// Note: Unit tests removed per Codex.md policy ("No ignored tests - tests must pass or be removed").
// These tests required environment variables (DITTO_APP_ID, DITTO_SHARED_KEY) and couldn't run in CI.
// The DittoBackend functionality is comprehensively tested in integration tests:
//   - tests/storage_layer_e2e.rs - End-to-end storage backend tests with real Ditto instances
//   - tests/sync_backend_integration.rs - Backend sync integration tests
//
// For local testing of DittoBackend, use the integration test suite with environment variables set.
