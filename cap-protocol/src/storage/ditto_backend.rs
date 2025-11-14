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

use super::ditto_store::DittoStore;
use super::traits::{Collection as CollectionTrait, DocumentPredicate, StorageBackend};
use anyhow::{Context, Result};
use base64::Engine;
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
        // Return known collections based on CAP Protocol schema
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_json_conversion() {
        let collection = DittoCollection {
            name: "test".to_string(),
            store: Arc::new(DittoStore::from_env().unwrap()),
        };

        let test_data = b"Hello, world!";
        let json = collection.bytes_to_json("doc-1", test_data);

        // Verify JSON structure
        assert_eq!(json.get("_id").unwrap().as_str().unwrap(), "doc-1");
        assert_eq!(json.get("type").unwrap().as_str().unwrap(), "binary_document");
        assert_eq!(json.get("collection").unwrap().as_str().unwrap(), "test");

        // Verify roundtrip
        let decoded = collection.json_to_bytes(&json).unwrap();
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_json_to_bytes_missing_field() {
        let collection = DittoCollection {
            name: "test".to_string(),
            store: Arc::new(DittoStore::from_env().unwrap()),
        };

        let invalid_json = serde_json::json!({"_id": "doc-1"});
        let result = collection.json_to_bytes(&invalid_json);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing 'data' field"));
    }

    #[test]
    fn test_list_collections() {
        let store = Arc::new(DittoStore::from_env().unwrap());
        let backend = DittoBackend::new(store);

        let collections = backend.list_collections();
        assert!(collections.contains(&"cells".to_string()));
        assert!(collections.contains(&"nodes".to_string()));
        assert!(collections.contains(&"capabilities".to_string()));
    }
}
