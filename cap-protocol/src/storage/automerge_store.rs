//! Automerge-based storage adapter (ADR-011 E11.1 POC)
//!
//! This module provides a minimal Automerge storage implementation for proof-of-concept testing.
//! It replaces Ditto's CRDT backend with Automerge for:
//! - Licensing cost elimination
//! - Open-source GOTS compatibility
//! - Foundation for Iroh networking integration
//!
//! **POC Limitations** (to be addressed in later phases):
//! - In-memory storage only (no RocksDB persistence yet)
//! - Simple JSON-based protobuf conversion (inefficient)
//! - No Iroh networking (manual sync for testing)
//! - No TTL support
//! - No geohash indexing

#[cfg(feature = "automerge-backend")]
use automerge::sync::SyncDoc;
#[cfg(feature = "automerge-backend")]
use automerge::Automerge;

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

/// In-memory Automerge document store
///
/// This is a POC implementation that stores Automerge documents in memory.
/// Future phases will replace the HashMap with RocksDB for persistence.
///
/// # Example
///
/// ```ignore
/// use cap_protocol::storage::AutomergeStore;
///
/// let store = AutomergeStore::new();
/// let cells = store.collection("cells");
///
/// // Store an Automerge document
/// let mut doc = Automerge::new();
/// doc.transact(|tx| tx.put(ROOT, "id", "cell-1"))?;
/// cells.upsert("cell-1", doc)?;
///
/// // Retrieve it
/// let retrieved = cells.get("cell-1")?;
/// ```
#[cfg(feature = "automerge-backend")]
#[derive(Clone)]
pub struct AutomergeStore {
    /// Storage: "collection:doc_id" -> Automerge document
    documents: Arc<RwLock<HashMap<String, Automerge>>>,
    /// Known collection names
    collections: Arc<RwLock<HashSet<String>>>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeStore {
    /// Create a new in-memory Automerge store
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            collections: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Get or create a collection
    ///
    /// Collections group related documents (e.g., "cells", "nodes", "capabilities").
    /// This mirrors Ditto's collection model.
    pub fn collection(&self, name: &str) -> Collection {
        self.collections.write().unwrap().insert(name.to_string());
        Collection {
            name: name.to_string(),
            store: self.clone(),
        }
    }

    /// Get all collection names
    pub fn list_collections(&self) -> Vec<String> {
        self.collections.read().unwrap().iter().cloned().collect()
    }

    /// Total number of documents across all collections
    pub fn document_count(&self) -> usize {
        self.documents.read().unwrap().len()
    }

    // Hierarchical Summary Storage (E11.3)
    //
    // These methods provide hierarchical summary storage for Automerge backend,
    // matching the DittoStore API for Mode 3 testing.

    /// Store a SquadSummary in the squad_summaries collection
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier (used as document ID)
    /// * `summary` - SquadSummary protobuf message
    ///
    /// # Returns
    ///
    /// Document ID (same as squad_id)
    pub fn upsert_squad_summary(
        &self,
        squad_id: &str,
        summary: &cap_schema::hierarchy::v1::SquadSummary,
    ) -> Result<String> {
        use automerge::{transaction::Transactable, ROOT};
        use prost::Message;

        // Encode protobuf to bytes
        let bytes = summary.encode_to_vec();

        // Create Automerge document with binary data
        let mut doc = Automerge::new();
        doc.transact(|tx| {
            tx.put(ROOT, "_id", squad_id)?;
            tx.put(ROOT, "squad_id", summary.squad_id.as_str())?;
            tx.put(ROOT, "leader_id", summary.leader_id.as_str())?;
            tx.put(ROOT, "member_count", summary.member_count as i64)?;
            tx.put(ROOT, "type", "squad_summary")?;
            // Store binary protobuf data as bytes (Vec<u8> implements Into<ScalarValue>)
            tx.put(ROOT, "data", bytes.clone())?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .map_err(|e| anyhow::anyhow!("Automerge transaction failed: {:?}", e))?;

        // Store in squad_summaries collection
        self.collection("squad_summaries").upsert(squad_id, doc)?;

        Ok(squad_id.to_string())
    }

    /// Retrieve a SquadSummary from the squad_summaries collection
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    ///
    /// # Returns
    ///
    /// Some(SquadSummary) if found, None if not found
    pub fn get_squad_summary(
        &self,
        squad_id: &str,
    ) -> Result<Option<cap_schema::hierarchy::v1::SquadSummary>> {
        use automerge::ReadDoc;
        use prost::Message;

        let collection = self.collection("squad_summaries");
        let doc = collection.get(squad_id)?;

        match doc {
            None => Ok(None),
            Some(doc) => {
                // Extract binary data from Automerge document
                let data_bytes = doc
                    .get(automerge::ROOT, "data")
                    .ok()
                    .flatten()
                    .and_then(|(v, _)| v.to_bytes().map(|b| b.to_vec()))
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid data field"))?;

                // Decode protobuf
                let summary = cap_schema::hierarchy::v1::SquadSummary::decode(&data_bytes[..])?;
                Ok(Some(summary))
            }
        }
    }

    /// Store a PlatoonSummary in the platoon_summaries collection
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier (used as document ID)
    /// * `summary` - PlatoonSummary protobuf message
    ///
    /// # Returns
    ///
    /// Document ID (same as platoon_id)
    pub fn upsert_platoon_summary(
        &self,
        platoon_id: &str,
        summary: &cap_schema::hierarchy::v1::PlatoonSummary,
    ) -> Result<String> {
        use automerge::{transaction::Transactable, ROOT};
        use prost::Message;

        let bytes = summary.encode_to_vec();

        let mut doc = Automerge::new();
        doc.transact(|tx| {
            tx.put(ROOT, "_id", platoon_id)?;
            tx.put(ROOT, "platoon_id", summary.platoon_id.as_str())?;
            tx.put(ROOT, "leader_id", summary.leader_id.as_str())?;
            tx.put(ROOT, "squad_count", summary.squad_count as i64)?;
            tx.put(
                ROOT,
                "total_member_count",
                summary.total_member_count as i64,
            )?;
            tx.put(ROOT, "type", "platoon_summary")?;
            tx.put(ROOT, "data", bytes.clone())?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .map_err(|e| anyhow::anyhow!("Automerge transaction failed: {:?}", e))?;

        self.collection("platoon_summaries")
            .upsert(platoon_id, doc)?;

        Ok(platoon_id.to_string())
    }

    /// Retrieve a PlatoonSummary from the platoon_summaries collection
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier
    ///
    /// # Returns
    ///
    /// Some(PlatoonSummary) if found, None if not found
    pub fn get_platoon_summary(
        &self,
        platoon_id: &str,
    ) -> Result<Option<cap_schema::hierarchy::v1::PlatoonSummary>> {
        use automerge::ReadDoc;
        use prost::Message;

        let collection = self.collection("platoon_summaries");
        let doc = collection.get(platoon_id)?;

        match doc {
            None => Ok(None),
            Some(doc) => {
                let data_bytes = doc
                    .get(automerge::ROOT, "data")
                    .ok()
                    .flatten()
                    .and_then(|(v, _)| v.to_bytes().map(|b| b.to_vec()))
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid data field"))?;

                let summary = cap_schema::hierarchy::v1::PlatoonSummary::decode(&data_bytes[..])?;
                Ok(Some(summary))
            }
        }
    }
}

#[cfg(feature = "automerge-backend")]
impl Default for AutomergeStore {
    fn default() -> Self {
        Self::new()
    }
}

/// A collection of Automerge documents
///
/// Collections provide scoped document storage similar to Ditto's collection model.
/// Documents are keyed as "collection_name:doc_id" internally.
#[cfg(feature = "automerge-backend")]
#[derive(Clone)]
pub struct Collection {
    name: String,
    store: AutomergeStore,
}

#[cfg(feature = "automerge-backend")]
impl Collection {
    /// Create internal storage key from document ID
    fn make_key(&self, doc_id: &str) -> String {
        format!("{}:{}", self.name, doc_id)
    }

    /// Insert or update a document
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Unique document identifier within this collection
    /// * `doc` - Automerge document to store
    pub fn upsert(&self, doc_id: &str, doc: Automerge) -> Result<()> {
        let key = self.make_key(doc_id);
        self.store.documents.write().unwrap().insert(key, doc);
        Ok(())
    }

    /// Retrieve a document by ID
    ///
    /// Returns `None` if the document does not exist.
    pub fn get(&self, doc_id: &str) -> Result<Option<Automerge>> {
        let key = self.make_key(doc_id);
        Ok(self.store.documents.read().unwrap().get(&key).cloned())
    }

    /// Delete a document
    pub fn delete(&self, doc_id: &str) -> Result<()> {
        let key = self.make_key(doc_id);
        self.store.documents.write().unwrap().remove(&key);
        Ok(())
    }

    /// Get all documents in this collection
    ///
    /// Returns a vector of (doc_id, Automerge) pairs.
    pub fn all(&self) -> Vec<(String, Automerge)> {
        let prefix = format!("{}:", self.name);
        self.store
            .documents
            .read()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| {
                let doc_id = k.strip_prefix(&prefix).unwrap().to_string();
                (doc_id, v.clone())
            })
            .collect()
    }

    /// Count documents in this collection
    pub fn count(&self) -> usize {
        let prefix = format!("{}:", self.name);
        self.store
            .documents
            .read()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .count()
    }

    /// Check if a document exists
    pub fn exists(&self, doc_id: &str) -> bool {
        let key = self.make_key(doc_id);
        self.store.documents.read().unwrap().contains_key(&key)
    }
}

/// In-memory sync engine for POC testing
///
/// This provides basic document synchronization between two AutomergeStore instances
/// for testing purposes. In production, this will be replaced by Iroh networking.
///
/// # Example
///
/// ```ignore
/// let store1 = AutomergeStore::new();
/// let store2 = AutomergeStore::new();
/// let sync_engine = InMemorySyncEngine::new();
///
/// // Create document in store1
/// let mut doc1 = Automerge::new();
/// doc1.transact(|tx| tx.put(ROOT, "key", "value"))?;
/// store1.collection("test").upsert("doc1", doc1)?;
///
/// // Sync to store2
/// let mut doc1 = store1.collection("test").get("doc1")?.unwrap();
/// let mut doc2 = Automerge::new();
/// sync_engine.sync_documents(&mut doc1, &mut doc2)?;
/// store2.collection("test").upsert("doc1", doc2)?;
/// ```
#[cfg(feature = "automerge-backend")]
pub struct InMemorySyncEngine {
    peer1_state: Arc<RwLock<automerge::sync::State>>,
    peer2_state: Arc<RwLock<automerge::sync::State>>,
}

#[cfg(feature = "automerge-backend")]
impl InMemorySyncEngine {
    /// Create a new sync engine
    pub fn new() -> Self {
        Self {
            peer1_state: Arc::new(RwLock::new(automerge::sync::State::new())),
            peer2_state: Arc::new(RwLock::new(automerge::sync::State::new())),
        }
    }

    /// Synchronize two Automerge documents
    ///
    /// This performs a full sync between two documents, converging their state.
    /// Changes flow bidirectionally. This method iterates until both documents
    /// have fully converged (no more sync messages are generated).
    ///
    /// # Arguments
    ///
    /// * `doc1` - First document (will receive changes from doc2)
    /// * `doc2` - Second document (will receive changes from doc1)
    pub fn sync_documents(&self, doc1: &mut Automerge, doc2: &mut Automerge) -> Result<()> {
        let mut state1 = self.peer1_state.write().unwrap();
        let mut state2 = self.peer2_state.write().unwrap();

        // Sync requires multiple round-trips to converge
        // Keep exchanging messages until both sides have nothing new
        let max_iterations = 10; // Safety limit to prevent infinite loops
        for _ in 0..max_iterations {
            let mut has_changes = false;

            // doc1 -> doc2
            if let Some(msg) = doc1.generate_sync_message(&mut state1) {
                doc2.receive_sync_message(&mut state2, msg)
                    .context("Failed to apply sync message to doc2")?;
                has_changes = true;
            }

            // doc2 -> doc1
            if let Some(msg) = doc2.generate_sync_message(&mut state2) {
                doc1.receive_sync_message(&mut state1, msg)
                    .context("Failed to apply sync message to doc1")?;
                has_changes = true;
            }

            // If neither direction produced a message, we're done
            if !has_changes {
                break;
            }
        }

        Ok(())
    }

    /// Synchronize a document with an empty document (initial sync)
    ///
    /// This is a helper for one-way sync from an existing document to a new one.
    pub fn sync_to_empty(&self, source: &mut Automerge) -> Result<Automerge> {
        let mut target = Automerge::new();
        self.sync_documents(source, &mut target)?;
        Ok(target)
    }
}

#[cfg(feature = "automerge-backend")]
impl Default for InMemorySyncEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg(feature = "automerge-backend")]
mod tests {
    use super::*;
    use automerge::{transaction::Transactable, ReadDoc, ROOT};

    // Helper to get string value from document
    fn get_str(doc: &Automerge, key: &str) -> Option<String> {
        doc.get(ROOT, key)
            .ok()
            .flatten()
            .and_then(|(v, _)| v.to_str().map(|s| s.to_string()))
    }

    #[test]
    fn test_store_creation() {
        let store = AutomergeStore::new();
        assert_eq!(store.document_count(), 0);
        assert_eq!(store.list_collections().len(), 0);
    }

    #[test]
    fn test_collection_upsert_get() {
        let store = AutomergeStore::new();
        let collection = store.collection("test");

        // Create a simple Automerge document
        let mut doc = Automerge::new();
        doc.transact(|tx| {
            tx.put(ROOT, "key", "value")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        // Store it
        collection.upsert("doc1", doc.clone()).unwrap();

        // Retrieve it
        let retrieved = collection.get("doc1").unwrap().unwrap();

        // Verify content
        assert_eq!(get_str(&retrieved, "key"), Some("value".to_string()));
    }

    #[test]
    fn test_collection_delete() {
        let store = AutomergeStore::new();
        let collection = store.collection("test");

        let mut doc = Automerge::new();
        doc.transact(|tx| {
            tx.put(ROOT, "key", "value")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        collection.upsert("doc1", doc).unwrap();
        assert!(collection.exists("doc1"));

        collection.delete("doc1").unwrap();
        assert!(!collection.exists("doc1"));
    }

    #[test]
    fn test_collection_count() {
        let store = AutomergeStore::new();
        let collection = store.collection("test");

        assert_eq!(collection.count(), 0);

        let mut doc1 = Automerge::new();
        doc1.transact(|tx| {
            tx.put(ROOT, "id", "1")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();
        collection.upsert("doc1", doc1).unwrap();

        assert_eq!(collection.count(), 1);

        let mut doc2 = Automerge::new();
        doc2.transact(|tx| {
            tx.put(ROOT, "id", "2")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();
        collection.upsert("doc2", doc2).unwrap();

        assert_eq!(collection.count(), 2);
    }

    #[test]
    fn test_collection_all() {
        let store = AutomergeStore::new();
        let collection = store.collection("test");

        let mut doc1 = Automerge::new();
        doc1.transact(|tx| {
            tx.put(ROOT, "id", "1")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();
        collection.upsert("doc1", doc1).unwrap();

        let mut doc2 = Automerge::new();
        doc2.transact(|tx| {
            tx.put(ROOT, "id", "2")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();
        collection.upsert("doc2", doc2).unwrap();

        let all = collection.all();
        assert_eq!(all.len(), 2);

        let ids: Vec<String> = all.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&"doc1".to_string()));
        assert!(ids.contains(&"doc2".to_string()));
    }

    #[test]
    fn test_multiple_collections() {
        let store = AutomergeStore::new();

        let cells = store.collection("cells");
        let nodes = store.collection("nodes");

        let mut cell_doc = Automerge::new();
        cell_doc
            .transact(|tx| {
                tx.put(ROOT, "type", "cell")?;
                Ok::<(), automerge::AutomergeError>(())
            })
            .unwrap();
        cells.upsert("cell1", cell_doc).unwrap();

        let mut node_doc = Automerge::new();
        node_doc
            .transact(|tx| {
                tx.put(ROOT, "type", "node")?;
                Ok::<(), automerge::AutomergeError>(())
            })
            .unwrap();
        nodes.upsert("node1", node_doc).unwrap();

        assert_eq!(cells.count(), 1);
        assert_eq!(nodes.count(), 1);
        assert_eq!(store.document_count(), 2);

        let collections = store.list_collections();
        assert_eq!(collections.len(), 2);
        assert!(collections.contains(&"cells".to_string()));
        assert!(collections.contains(&"nodes".to_string()));
    }

    #[test]
    fn test_sync_two_documents() {
        let engine = InMemorySyncEngine::new();

        // Create first document with data
        let mut doc1 = Automerge::new();
        doc1.transact(|tx| {
            tx.put(ROOT, "foo", "bar")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        // Create empty second document
        let mut doc2 = Automerge::new();

        // Sync
        engine.sync_documents(&mut doc1, &mut doc2).unwrap();

        // Verify doc2 received the data
        assert_eq!(get_str(&doc2, "foo"), Some("bar".to_string()));
    }

    #[test]
    fn test_bidirectional_sync() {
        let engine = InMemorySyncEngine::new();

        // Doc1 has field 'a'
        let mut doc1 = Automerge::new();
        doc1.transact(|tx| {
            tx.put(ROOT, "a", "value_a")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        // Doc2 has field 'b'
        let mut doc2 = Automerge::new();
        doc2.transact(|tx| {
            tx.put(ROOT, "b", "value_b")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        // Sync both ways
        engine.sync_documents(&mut doc1, &mut doc2).unwrap();

        // Both documents should have both fields
        assert_eq!(get_str(&doc1, "a"), Some("value_a".to_string()));
        assert_eq!(get_str(&doc1, "b"), Some("value_b".to_string()));
        assert_eq!(get_str(&doc2, "a"), Some("value_a".to_string()));
        assert_eq!(get_str(&doc2, "b"), Some("value_b".to_string()));
    }

    #[test]
    fn test_sync_to_empty() {
        let engine = InMemorySyncEngine::new();

        let mut source = Automerge::new();
        source
            .transact(|tx| {
                tx.put(ROOT, "data", "test")?;
                Ok::<(), automerge::AutomergeError>(())
            })
            .unwrap();

        let target = engine.sync_to_empty(&mut source).unwrap();

        assert_eq!(get_str(&target, "data"), Some("test".to_string()));
    }

    #[test]
    fn test_crdt_convergence() {
        let engine = InMemorySyncEngine::new();

        // Two peers independently modify different fields
        let mut peer1 = Automerge::new();
        peer1
            .transact(|tx| {
                tx.put(ROOT, "field1", "peer1_value")?;
                Ok::<(), automerge::AutomergeError>(())
            })
            .unwrap();

        let mut peer2 = Automerge::new();
        peer2
            .transact(|tx| {
                tx.put(ROOT, "field2", "peer2_value")?;
                Ok::<(), automerge::AutomergeError>(())
            })
            .unwrap();

        // Sync
        engine.sync_documents(&mut peer1, &mut peer2).unwrap();

        // Both should converge to same state
        assert_eq!(get_str(&peer1, "field1"), Some("peer1_value".to_string()));
        assert_eq!(get_str(&peer1, "field2"), Some("peer2_value".to_string()));
        assert_eq!(get_str(&peer2, "field1"), Some("peer1_value".to_string()));
        assert_eq!(get_str(&peer2, "field2"), Some("peer2_value".to_string()));
    }

    #[test]
    fn test_squad_summary_storage() {
        use cap_schema::common::v1::{Position, Timestamp};
        use cap_schema::hierarchy::v1::{BoundingBox, SquadSummary};
        use cap_schema::node::v1::HealthStatus;

        let store = AutomergeStore::new();

        // Create test SquadSummary
        let squad_summary = SquadSummary {
            squad_id: "squad-alpha".to_string(),
            leader_id: "node-1".to_string(),
            member_ids: vec!["node-1".to_string(), "node-2".to_string()],
            member_count: 2,
            position_centroid: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 120.0,
            worst_health: HealthStatus::Nominal as i32,
            operational_count: 2,
            aggregated_capabilities: vec![],
            readiness_score: 0.95,
            bounding_box: Some(BoundingBox {
                southwest: Some(Position {
                    latitude: 37.7740,
                    longitude: -122.4203,
                    altitude: 90.0,
                }),
                northeast: Some(Position {
                    latitude: 37.7758,
                    longitude: -122.4185,
                    altitude: 110.0,
                }),
                max_altitude: 110.0,
                min_altitude: 90.0,
                radius_m: 500.0,
            }),
            aggregated_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        // Test upsert
        let doc_id = store
            .upsert_squad_summary("squad-alpha", &squad_summary)
            .unwrap();
        assert_eq!(doc_id, "squad-alpha");

        // Test retrieval
        let retrieved = store.get_squad_summary("squad-alpha").unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.squad_id, "squad-alpha");
        assert_eq!(retrieved.leader_id, "node-1");
        assert_eq!(retrieved.member_count, 2);
        assert_eq!(retrieved.operational_count, 2);
        assert!((retrieved.avg_fuel_minutes - 120.0).abs() < 0.001);

        // Test non-existent retrieval
        let not_found = store.get_squad_summary("squad-nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_platoon_summary_storage() {
        use cap_schema::common::v1::{Position, Timestamp};
        use cap_schema::hierarchy::v1::{BoundingBox, PlatoonSummary};
        use cap_schema::node::v1::HealthStatus;

        let store = AutomergeStore::new();

        // Create test PlatoonSummary
        let platoon_summary = PlatoonSummary {
            platoon_id: "platoon-1".to_string(),
            leader_id: "node-1".to_string(),
            squad_ids: vec!["squad-alpha".to_string(), "squad-bravo".to_string()],
            squad_count: 2,
            total_member_count: 16,
            position_centroid: Some(Position {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 110.0,
            worst_health: HealthStatus::Nominal as i32,
            operational_count: 14,
            aggregated_capabilities: vec![],
            readiness_score: 0.90,
            bounding_box: Some(BoundingBox {
                southwest: Some(Position {
                    latitude: 37.7700,
                    longitude: -122.4250,
                    altitude: 80.0,
                }),
                northeast: Some(Position {
                    latitude: 37.7800,
                    longitude: -122.4150,
                    altitude: 120.0,
                }),
                max_altitude: 120.0,
                min_altitude: 80.0,
                radius_m: 1000.0,
            }),
            aggregated_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        // Test upsert
        let doc_id = store
            .upsert_platoon_summary("platoon-1", &platoon_summary)
            .unwrap();
        assert_eq!(doc_id, "platoon-1");

        // Test retrieval
        let retrieved = store.get_platoon_summary("platoon-1").unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.platoon_id, "platoon-1");
        assert_eq!(retrieved.leader_id, "node-1");
        assert_eq!(retrieved.squad_count, 2);
        assert_eq!(retrieved.total_member_count, 16);
        assert_eq!(retrieved.operational_count, 14);
        assert!((retrieved.avg_fuel_minutes - 110.0).abs() < 0.001);

        // Test non-existent retrieval
        let not_found = store.get_platoon_summary("platoon-nonexistent").unwrap();
        assert!(not_found.is_none());
    }
}
