//! Automerge document storage with redb persistence
//!
//! This module provides persistent storage for Automerge CRDT documents using redb,
//! a pure Rust embedded database. This replaces the previous RocksDB implementation
//! to eliminate C/C++ build dependencies and align with Iroh's storage layer.

#[cfg(feature = "automerge-backend")]
use crate::storage::traits::{Collection, DocumentPredicate};
#[cfg(feature = "automerge-backend")]
use automerge::{transaction::Transactable, Automerge, ReadDoc};
#[cfg(feature = "automerge-backend")]
use lru::LruCache;
#[cfg(feature = "automerge-backend")]
use redb::{Database, ReadableTableMetadata, TableDefinition};
#[cfg(feature = "automerge-backend")]
use std::num::NonZeroUsize;
#[cfg(feature = "automerge-backend")]
use std::path::Path;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use tokio::sync::broadcast;

#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};

/// Table definition for document storage
/// Key: document key as string bytes
/// Value: serialized Automerge document bytes
#[cfg(feature = "automerge-backend")]
const DOCUMENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("documents");

/// Storage layer for Automerge documents with redb persistence
///
/// # Change Notifications (Phase 6.3)
///
/// The store emits change notifications when documents are modified via `put()`.
/// Subscribers can listen for these notifications to trigger automatic sync.
#[cfg(feature = "automerge-backend")]
pub struct AutomergeStore {
    db: Arc<Database>,
    cache: Arc<RwLock<LruCache<String, Automerge>>>,
    /// Broadcast channel for notifying of document changes (Phase 6.3)
    /// Multiple subscribers can receive notifications (sync coordinator + observers)
    change_tx: broadcast::Sender<String>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeStore {
    /// Open or create storage at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // Ensure the directory exists
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // redb stores in a single file, append .redb extension if it's a directory path
        let db_path = if path.is_dir() || !path.exists() {
            std::fs::create_dir_all(path).ok();
            path.join("automerge.redb")
        } else {
            path.to_path_buf()
        };

        // Check for corrupted (0-byte) database file and remove it
        // This can happen on Android if the previous initialization was interrupted
        if db_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&db_path) {
                if metadata.len() == 0 {
                    tracing::warn!("Removing corrupted 0-byte redb database at {:?}", db_path);
                    std::fs::remove_file(&db_path).ok();
                }
            }
        }

        let db = Database::create(&db_path).context("Failed to open redb database")?;

        // Initialize the table (redb requires this on first use)
        {
            let write_txn = db
                .begin_write()
                .context("Failed to begin write transaction")?;
            // Creating the table if it doesn't exist
            let _ = write_txn.open_table(DOCUMENTS_TABLE);
            write_txn
                .commit()
                .context("Failed to commit table creation")?;
        }

        let cache = LruCache::new(NonZeroUsize::new(1000).unwrap());

        // Create broadcast channel for change notifications
        // Capacity of 1024 should be sufficient for most workloads
        let (change_tx, _) = broadcast::channel(1024);

        Ok(Self {
            db: Arc::new(db),
            cache: Arc::new(RwLock::new(cache)),
            change_tx,
        })
    }

    /// Save an Automerge document
    ///
    /// # Change Notifications (Phase 6.3)
    ///
    /// This method emits a change notification after successfully persisting the document.
    /// Subscribers will receive the document key to trigger automatic sync.
    pub fn put(&self, key: &str, doc: &Automerge) -> Result<()> {
        let bytes = doc.save();

        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .context("Failed to open documents table")?;
            table
                .insert(key.as_bytes(), bytes.as_slice())
                .context("Failed to insert document")?;
        }
        write_txn.commit().context("Failed to commit write")?;

        self.cache
            .write()
            .unwrap()
            .put(key.to_string(), doc.clone());

        // Notify subscribers of the change (Phase 6.3)
        // Ignore send errors - if no one is listening, that's fine
        let _ = self.change_tx.send(key.to_string());

        Ok(())
    }

    /// Load an Automerge document
    pub fn get(&self, key: &str) -> Result<Option<Automerge>> {
        {
            let mut cache = self.cache.write().unwrap();
            if let Some(doc) = cache.get(key) {
                return Ok(Some(doc.clone()));
            }
        }

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .context("Failed to open documents table")?;

        match table.get(key.as_bytes())? {
            Some(value) => {
                let bytes = value.value();
                let doc = Automerge::load(bytes).context("Failed to load Automerge document")?;

                self.cache
                    .write()
                    .unwrap()
                    .put(key.to_string(), doc.clone());

                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Delete a document
    pub fn delete(&self, key: &str) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(DOCUMENTS_TABLE)
                .context("Failed to open documents table")?;
            table.remove(key.as_bytes())?;
        }
        write_txn.commit().context("Failed to commit delete")?;

        self.cache.write().unwrap().pop(key);
        Ok(())
    }

    /// Scan documents with prefix
    pub fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Automerge)>> {
        let mut results = Vec::new();

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .context("Failed to open documents table")?;

        // Use range to scan from prefix onwards
        let prefix_bytes = prefix.as_bytes();
        for entry in table.range(prefix_bytes..)? {
            let (key, value) = entry?;
            let key_bytes = key.value();

            // Stop if we've passed the prefix
            if !key_bytes.starts_with(prefix_bytes) {
                break;
            }

            let key_str = String::from_utf8_lossy(key_bytes).to_string();
            let doc = Automerge::load(value.value())?;
            results.push((key_str, doc));
        }

        Ok(results)
    }

    /// Count total documents
    pub fn count(&self) -> usize {
        let read_txn = match self.db.begin_read() {
            Ok(txn) => txn,
            Err(_) => return 0,
        };
        let table = match read_txn.open_table(DOCUMENTS_TABLE) {
            Ok(t) => t,
            Err(_) => return 0,
        };

        table.len().unwrap_or(0) as usize
    }

    /// Subscribe to document change notifications (Phase 6.3)
    ///
    /// Returns a receiver that will receive document keys whenever documents are modified.
    /// Multiple subscribers are supported - each gets their own receiver.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = AutomergeStore::open("./data")?;
    /// let mut rx = store.subscribe_to_changes();
    /// while let Ok(doc_key) = rx.recv().await {
    ///     println!("Document changed: {}", doc_key);
    /// }
    /// ```
    pub fn subscribe_to_changes(&self) -> broadcast::Receiver<String> {
        self.change_tx.subscribe()
    }

    /// Get a collection handle for a specific namespace
    pub fn collection(self: &Arc<Self>, name: &str) -> Arc<dyn Collection> {
        Arc::new(AutomergeCollection {
            store: Arc::clone(self),
            prefix: format!("{}:", name),
        })
    }
}

/// Collection implementation for AutomergeStore
///
/// Wraps AutomergeStore and provides Collection trait implementation.
/// Uses key prefixing to namespace collections (e.g., "cells:cell-1", "nodes:node-1").
#[cfg(feature = "automerge-backend")]
pub struct AutomergeCollection {
    store: Arc<AutomergeStore>,
    prefix: String,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeCollection {
    fn prefixed_key(&self, doc_id: &str) -> String {
        format!("{}{}", self.prefix, doc_id)
    }

    fn strip_prefix<'b>(&self, key: &'b str) -> Option<&'b str> {
        key.strip_prefix(&self.prefix)
    }
}

#[cfg(feature = "automerge-backend")]
impl Collection for AutomergeCollection {
    fn upsert(&self, doc_id: &str, data: Vec<u8>) -> Result<()> {
        // Convert raw bytes to Automerge document
        // For now, we store bytes directly in an Automerge document
        // TODO: In Phase 2, this will convert protobuf → JSON → Automerge
        let mut doc = Automerge::new();
        match doc.transact(|tx| {
            tx.put(
                automerge::ROOT,
                "data",
                automerge::ScalarValue::Bytes(data.clone()),
            )?;
            Ok::<(), automerge::AutomergeError>(())
        }) {
            Ok(_) => self.store.put(&self.prefixed_key(doc_id), &doc),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to create Automerge document: {:?}",
                e
            )),
        }
    }

    fn get(&self, doc_id: &str) -> Result<Option<Vec<u8>>> {
        match self.store.get(&self.prefixed_key(doc_id))? {
            Some(doc) => {
                // Extract bytes from Automerge document
                if let Ok(Some((automerge::Value::Scalar(scalar), _))) =
                    doc.get(automerge::ROOT, "data")
                {
                    if let automerge::ScalarValue::Bytes(bytes) = scalar.as_ref() {
                        return Ok(Some(bytes.to_vec()));
                    }
                }
                Ok(None)
            }
            None => Ok(None),
        }
    }

    fn delete(&self, doc_id: &str) -> Result<()> {
        self.store.delete(&self.prefixed_key(doc_id))
    }

    fn scan(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let docs = self.store.scan_prefix(&self.prefix)?;
        let mut results = Vec::new();

        for (key, doc) in docs {
            if let Some(doc_id) = self.strip_prefix(&key) {
                if let Ok(Some((automerge::Value::Scalar(scalar), _))) =
                    doc.get(automerge::ROOT, "data")
                {
                    if let automerge::ScalarValue::Bytes(bytes) = scalar.as_ref() {
                        results.push((doc_id.to_string(), bytes.to_vec()));
                    }
                }
            }
        }

        Ok(results)
    }

    fn find(&self, predicate: DocumentPredicate) -> Result<Vec<(String, Vec<u8>)>> {
        let all_docs = self.scan()?;
        Ok(all_docs
            .into_iter()
            .filter(|(_, bytes)| predicate(bytes))
            .collect())
    }

    fn query_geohash_prefix(&self, geohash_prefix: &str) -> Result<Vec<(String, Vec<u8>)>> {
        // For AutomergeStore, geohash queries require the geohash to be in the key
        // This is a simplified implementation - in Phase 2 we'll add proper indexing
        let all_docs = self.scan()?;
        Ok(all_docs
            .into_iter()
            .filter(|(id, _)| id.starts_with(geohash_prefix))
            .collect())
    }

    fn count(&self) -> Result<usize> {
        Ok(self.scan()?.len())
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (Arc<AutomergeStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        (store, temp_dir)
    }

    #[test]
    fn test_collection_upsert_and_get() {
        let (store, _temp) = create_test_store();
        let collection = store.collection("test");

        let data = b"test data".to_vec();
        collection.upsert("doc1", data.clone()).unwrap();

        let retrieved = collection.get("doc1").unwrap().unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_collection_scan() {
        let (store, _temp) = create_test_store();
        let collection = store.collection("test");

        collection.upsert("doc1", b"data1".to_vec()).unwrap();
        collection.upsert("doc2", b"data2".to_vec()).unwrap();

        let results = collection.scan().unwrap();
        assert_eq!(results.len(), 2);

        let ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&"doc1".to_string()));
        assert!(ids.contains(&"doc2".to_string()));
    }

    #[test]
    fn test_collection_delete() {
        let (store, _temp) = create_test_store();
        let collection = store.collection("test");

        collection.upsert("doc1", b"data1".to_vec()).unwrap();
        assert!(collection.get("doc1").unwrap().is_some());

        collection.delete("doc1").unwrap();
        assert!(collection.get("doc1").unwrap().is_none());
    }

    #[test]
    fn test_collection_count() {
        let (store, _temp) = create_test_store();
        let collection = store.collection("test");

        assert_eq!(collection.count().unwrap(), 0);

        collection.upsert("doc1", b"data1".to_vec()).unwrap();
        collection.upsert("doc2", b"data2".to_vec()).unwrap();

        assert_eq!(collection.count().unwrap(), 2);
    }

    #[test]
    fn test_collection_find_with_predicate() {
        let (store, _temp) = create_test_store();
        let collection = store.collection("test");

        collection.upsert("doc1", b"hello".to_vec()).unwrap();
        collection.upsert("doc2", b"world".to_vec()).unwrap();
        collection.upsert("doc3", b"hello world".to_vec()).unwrap();

        let results = collection
            .find(Box::new(|bytes| {
                String::from_utf8_lossy(bytes).contains("hello")
            }))
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_collection_namespace_isolation() {
        let (store, _temp) = create_test_store();
        let collection1 = store.collection("coll1");
        let collection2 = store.collection("coll2");

        collection1.upsert("doc1", b"data1".to_vec()).unwrap();
        collection2.upsert("doc1", b"data2".to_vec()).unwrap();

        let data1 = collection1.get("doc1").unwrap().unwrap();
        let data2 = collection2.get("doc1").unwrap().unwrap();

        assert_eq!(data1, b"data1");
        assert_eq!(data2, b"data2");
        assert_ne!(data1, data2);
    }

    #[test]
    fn test_direct_put_and_get() {
        let (store, _temp) = create_test_store();

        let mut doc = Automerge::new();
        doc.transact::<_, _, automerge::AutomergeError>(|tx| {
            tx.put(automerge::ROOT, "key", "value")?;
            Ok(())
        })
        .unwrap();

        store.put("test-doc", &doc).unwrap();

        let loaded = store.get("test-doc").unwrap().unwrap();
        let value: String = loaded
            .get(automerge::ROOT, "key")
            .unwrap()
            .unwrap()
            .0
            .to_string();
        assert!(value.contains("value"));
    }

    #[test]
    fn test_scan_prefix() {
        let (store, _temp) = create_test_store();

        let mut doc1 = Automerge::new();
        doc1.transact::<_, _, automerge::AutomergeError>(|tx| {
            tx.put(automerge::ROOT, "n", "1")?;
            Ok(())
        })
        .unwrap();

        let mut doc2 = Automerge::new();
        doc2.transact::<_, _, automerge::AutomergeError>(|tx| {
            tx.put(automerge::ROOT, "n", "2")?;
            Ok(())
        })
        .unwrap();

        store.put("prefix:a", &doc1).unwrap();
        store.put("prefix:b", &doc2).unwrap();
        store.put("other:c", &doc1).unwrap();

        let results = store.scan_prefix("prefix:").unwrap();
        assert_eq!(results.len(), 2);
    }
}
