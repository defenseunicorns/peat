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
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
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

/// Table definition for tombstone storage (ADR-034 Phase 2)
/// Key: "collection:document_id" as string bytes
/// Value: serialized Tombstone bytes (JSON)
#[cfg(feature = "automerge-backend")]
const TOMBSTONES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tombstones");

/// Storage layer for Automerge documents with redb persistence
///
/// # Change Notifications (Phase 6.3)
///
/// The store emits change notifications when documents are modified via `put()`.
/// Subscribers can listen for these notifications to trigger automatic sync.
///
/// # In-Memory Mode
///
/// For high-throughput testing, the store can operate in pure in-memory mode
/// where all documents are stored only in the LRU cache (no disk persistence).
/// Enable via `AutomergeStore::in_memory()` constructor.
#[cfg(feature = "automerge-backend")]
pub struct AutomergeStore {
    /// Database handle - None when running in memory-only mode
    db: Option<Arc<Database>>,
    cache: Arc<RwLock<LruCache<String, Automerge>>>,
    /// Broadcast channel for sync coordinator - used to trigger P2P sync
    /// Only notified for local puts (not synced documents)
    change_tx: broadcast::Sender<String>,
    /// Broadcast channel for observers - used for hierarchical aggregation (Issue #377)
    /// Notified for ALL document changes (local and synced) so observers can react
    observer_tx: broadcast::Sender<String>,
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

        // Initialize the tables (redb requires this on first use)
        {
            let write_txn = db
                .begin_write()
                .context("Failed to begin write transaction")?;
            // Creating the tables if they don't exist
            let _ = write_txn.open_table(DOCUMENTS_TABLE);
            let _ = write_txn.open_table(TOMBSTONES_TABLE); // ADR-034 Phase 2
            write_txn
                .commit()
                .context("Failed to commit table creation")?;
        }

        let cache = LruCache::new(NonZeroUsize::new(1000).unwrap());

        // Create broadcast channel for sync coordinator notifications
        // Issue #346: Increased from 1024 to 8192 to reduce lagging under high load.
        // When this channel lags, the auto_sync_task must do a full resync which is expensive.
        // A larger buffer trades memory (8KB per doc_key) for reduced resync frequency.
        let (change_tx, _) = broadcast::channel(8192);

        // Create broadcast channel for observer notifications (Issue #377)
        // This channel notifies ALL document changes including synced documents
        // so hierarchical aggregation can react to remotely synced platoon summaries
        let (observer_tx, _) = broadcast::channel(8192);

        Ok(Self {
            db: Some(Arc::new(db)),
            cache: Arc::new(RwLock::new(cache)),
            change_tx,
            observer_tx,
        })
    }

    /// Create an in-memory store (no disk persistence)
    ///
    /// Documents are stored only in the LRU cache. This mode is useful for
    /// high-throughput testing where persistence is not required.
    ///
    /// Note: Cache size is 10,000 documents in memory mode (vs 1,000 for disk mode)
    /// to accommodate larger working sets.
    pub fn in_memory() -> Self {
        // Larger cache for in-memory mode since we have no disk backing
        let cache = LruCache::new(NonZeroUsize::new(10000).unwrap());
        let (change_tx, _) = broadcast::channel(8192);
        let (observer_tx, _) = broadcast::channel(8192);

        tracing::info!("AutomergeStore: Running in MEMORY-ONLY mode (no disk persistence)");

        Self {
            db: None,
            cache: Arc::new(RwLock::new(cache)),
            change_tx,
            observer_tx,
        }
    }

    /// Check if the store is running in memory-only mode
    pub fn is_in_memory(&self) -> bool {
        self.db.is_none()
    }

    /// Save an Automerge document
    ///
    /// # Change Notifications (Phase 6.3)
    ///
    /// This method emits a change notification after successfully persisting the document.
    /// Subscribers will receive the document key to trigger automatic sync.
    pub fn put(&self, key: &str, doc: &Automerge) -> Result<()> {
        self.put_inner(key, doc, true)
    }

    /// Save an Automerge document without emitting change notification (Issue #346)
    ///
    /// Use this method when storing documents received via sync to avoid
    /// triggering a sync-back that would be blocked by cooldown and waste resources.
    /// The sending peer already has this document, so syncing back is unnecessary.
    pub fn put_without_notify(&self, key: &str, doc: &Automerge) -> Result<()> {
        self.put_inner(key, doc, false)
    }

    /// Internal put implementation
    fn put_inner(&self, key: &str, doc: &Automerge, notify: bool) -> Result<()> {
        // Only persist to disk if we have a database (not in-memory mode)
        if let Some(ref db) = self.db {
            let bytes = doc.save();

            let write_txn = db
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
        }

        self.cache
            .write()
            .unwrap()
            .put(key.to_string(), doc.clone());

        // Always notify observers for ALL document changes (Issue #377)
        // This enables hierarchical aggregation to react to remotely synced docs
        let _ = self.observer_tx.send(key.to_string());

        // Notify sync coordinator only for local changes (Phase 6.3, Issue #346)
        // Skip notification for documents received via sync to avoid sync-back loops
        if notify {
            // Ignore send errors - if no one is listening, that's fine
            let _ = self.change_tx.send(key.to_string());
        }

        Ok(())
    }

    /// Load an Automerge document
    pub fn get(&self, key: &str) -> Result<Option<Automerge>> {
        // Always check cache first
        {
            let mut cache = self.cache.write().unwrap();
            if let Some(doc) = cache.get(key) {
                return Ok(Some(doc.clone()));
            }
        }

        // In memory-only mode, cache miss means document doesn't exist
        let Some(ref db) = self.db else {
            return Ok(None);
        };

        let read_txn = db
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
        // Only delete from disk if we have a database
        if let Some(ref db) = self.db {
            let write_txn = db
                .begin_write()
                .context("Failed to begin write transaction")?;
            {
                let mut table = write_txn
                    .open_table(DOCUMENTS_TABLE)
                    .context("Failed to open documents table")?;
                table.remove(key.as_bytes())?;
            }
            write_txn.commit().context("Failed to commit delete")?;
        }

        self.cache.write().unwrap().pop(key);
        Ok(())
    }

    /// Scan documents with prefix
    pub fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Automerge)>> {
        // In memory-only mode, scan the cache
        if self.db.is_none() {
            let cache = self.cache.read().unwrap();
            let results: Vec<(String, Automerge)> = cache
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            return Ok(results);
        }

        let mut results = Vec::new();

        let read_txn = self
            .db
            .as_ref()
            .unwrap()
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
        // In memory-only mode, count cache entries
        let Some(ref db) = self.db else {
            return self.cache.read().unwrap().len();
        };

        let read_txn = match db.begin_read() {
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

    /// Subscribe to observer notifications (Issue #377)
    ///
    /// Returns a receiver that receives document keys for ALL changes, including
    /// documents received via sync. Use this for hierarchical aggregation where
    /// you need to react to remotely synced documents (e.g., company commander
    /// reacting to platoon summaries synced from platoon leaders).
    ///
    /// Unlike `subscribe_to_changes()` which only fires for local puts,
    /// this fires for ALL document changes.
    pub fn subscribe_to_observer_changes(&self) -> broadcast::Receiver<String> {
        self.observer_tx.subscribe()
    }

    /// Get a collection handle for a specific namespace
    pub fn collection(self: &Arc<Self>, name: &str) -> Arc<dyn Collection> {
        Arc::new(AutomergeCollection {
            store: Arc::clone(self),
            prefix: format!("{}:", name),
        })
    }

    // === Tombstone storage methods (ADR-034 Phase 2) ===

    /// Store a tombstone
    ///
    /// Tombstones are stored with key format "collection:document_id"
    /// In memory-only mode, this is a no-op (tombstones aren't needed without persistence)
    pub fn put_tombstone(&self, tombstone: &crate::qos::Tombstone) -> Result<()> {
        let Some(ref db) = self.db else {
            return Ok(()); // No-op in memory mode
        };

        let key = format!("{}:{}", tombstone.collection, tombstone.document_id);
        let bytes = serde_json::to_vec(tombstone).context("Failed to serialize tombstone")?;

        let write_txn = db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(TOMBSTONES_TABLE)
                .context("Failed to open tombstones table")?;
            table
                .insert(key.as_bytes(), bytes.as_slice())
                .context("Failed to insert tombstone")?;
        }
        write_txn
            .commit()
            .context("Failed to commit tombstone write")?;

        tracing::debug!(
            "Stored tombstone for document {} in collection {}",
            tombstone.document_id,
            tombstone.collection
        );

        Ok(())
    }

    /// Get a tombstone by collection and document ID
    pub fn get_tombstone(
        &self,
        collection: &str,
        document_id: &str,
    ) -> Result<Option<crate::qos::Tombstone>> {
        let Some(ref db) = self.db else {
            return Ok(None); // No tombstones in memory mode
        };

        let key = format!("{}:{}", collection, document_id);

        let read_txn = db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(TOMBSTONES_TABLE)
            .context("Failed to open tombstones table")?;

        match table.get(key.as_bytes())? {
            Some(value) => {
                let bytes = value.value();
                let tombstone: crate::qos::Tombstone =
                    serde_json::from_slice(bytes).context("Failed to deserialize tombstone")?;
                Ok(Some(tombstone))
            }
            None => Ok(None),
        }
    }

    /// Get all tombstones for a collection
    pub fn get_tombstones_for_collection(
        &self,
        collection: &str,
    ) -> Result<Vec<crate::qos::Tombstone>> {
        let Some(ref db) = self.db else {
            return Ok(Vec::new()); // No tombstones in memory mode
        };

        let prefix = format!("{}:", collection);
        let mut tombstones = Vec::new();

        let read_txn = db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(TOMBSTONES_TABLE)
            .context("Failed to open tombstones table")?;

        // Iterate all entries and filter by prefix
        for entry in table.iter()? {
            let (key, value) = entry?;
            let key_str = String::from_utf8_lossy(key.value());
            if key_str.starts_with(&prefix) {
                let tombstone: crate::qos::Tombstone = serde_json::from_slice(value.value())
                    .context("Failed to deserialize tombstone")?;
                tombstones.push(tombstone);
            }
        }

        Ok(tombstones)
    }

    /// Get all tombstones across all collections
    pub fn get_all_tombstones(&self) -> Result<Vec<crate::qos::Tombstone>> {
        let Some(ref db) = self.db else {
            return Ok(Vec::new()); // No tombstones in memory mode
        };

        let mut tombstones = Vec::new();

        let read_txn = db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(TOMBSTONES_TABLE)
            .context("Failed to open tombstones table")?;

        for entry in table.iter()? {
            let (_key, value) = entry?;
            let tombstone: crate::qos::Tombstone =
                serde_json::from_slice(value.value()).context("Failed to deserialize tombstone")?;
            tombstones.push(tombstone);
        }

        Ok(tombstones)
    }

    /// Remove a tombstone
    pub fn remove_tombstone(&self, collection: &str, document_id: &str) -> Result<bool> {
        let Some(ref db) = self.db else {
            return Ok(false); // No tombstones in memory mode
        };

        let key = format!("{}:{}", collection, document_id);

        let write_txn = db
            .begin_write()
            .context("Failed to begin write transaction")?;
        let existed = {
            let mut table = write_txn
                .open_table(TOMBSTONES_TABLE)
                .context("Failed to open tombstones table")?;
            let result = table.remove(key.as_bytes())?;
            result.is_some()
        };
        write_txn
            .commit()
            .context("Failed to commit tombstone removal")?;

        if existed {
            tracing::debug!(
                "Removed tombstone for document {} in collection {}",
                document_id,
                collection
            );
        }

        Ok(existed)
    }

    /// Check if a tombstone exists
    pub fn has_tombstone(&self, collection: &str, document_id: &str) -> Result<bool> {
        Ok(self.get_tombstone(collection, document_id)?.is_some())
    }

    // === Garbage Collection support methods (ADR-034 Phase 3) ===

    /// Get list of all collections (by scanning document key prefixes)
    pub fn list_collections(&self) -> Result<Vec<String>> {
        let mut collections = std::collections::HashSet::new();

        // In memory-only mode, scan the cache
        if self.db.is_none() {
            let cache = self.cache.read().unwrap();
            for key in cache.iter().map(|(k, _)| k) {
                if let Some(colon_pos) = key.find(':') {
                    let collection = &key[..colon_pos];
                    collections.insert(collection.to_string());
                }
            }
            return Ok(collections.into_iter().collect());
        }

        let read_txn = self
            .db
            .as_ref()
            .unwrap()
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .context("Failed to open documents table")?;

        for result in table.iter().context("Failed to iterate documents")? {
            let (key, _) = result.context("Failed to read document entry")?;
            let key_str =
                std::str::from_utf8(key.value()).context("Invalid UTF-8 in document key")?;

            // Keys are formatted as "collection:document_id"
            if let Some(colon_pos) = key_str.find(':') {
                let collection = &key_str[..colon_pos];
                collections.insert(collection.to_string());
            }
        }

        Ok(collections.into_iter().collect())
    }

    /// Get documents in a collection that were created before the cutoff time
    ///
    /// This checks the _created_at field stored in the Automerge document.
    /// Used for ImplicitTTL garbage collection.
    pub fn get_expired_documents(
        &self,
        collection: &str,
        cutoff: std::time::SystemTime,
    ) -> Result<Vec<String>> {
        let prefix = format!("{}:", collection);
        let docs = self.scan_prefix(&prefix)?;
        let mut expired = Vec::new();

        let cutoff_ms = cutoff
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for (key, doc) in docs {
            // Extract _created_at timestamp if present
            if let Ok(Some((automerge::Value::Scalar(scalar), _))) =
                doc.get(automerge::ROOT, "_created_at")
            {
                if let automerge::ScalarValue::Uint(created_at) = scalar.as_ref() {
                    if *created_at < cutoff_ms {
                        // Document is older than cutoff
                        if let Some(doc_id) = key.strip_prefix(&prefix) {
                            expired.push(doc_id.to_string());
                        }
                    }
                }
            }
        }

        Ok(expired)
    }

    /// Hard delete a document (permanent removal, no tombstone created)
    ///
    /// Used by garbage collection for ImplicitTTL collections where
    /// tombstones are not needed.
    pub fn hard_delete(&self, collection: &str, document_id: &str) -> Result<()> {
        let key = format!("{}:{}", collection, document_id);
        self.delete(&key)?;
        tracing::debug!(
            "Hard deleted document {} from collection {}",
            document_id,
            collection
        );
        Ok(())
    }

    // === Document Compaction (Issue #401 - Memory Blowout Fix) ===

    /// Compact a document by discarding CRDT history
    ///
    /// Automerge documents accumulate operation history with every change.
    /// This method replaces the document with a forked copy that contains
    /// only the current state, freeing memory used by historical operations.
    ///
    /// # When to use
    ///
    /// - After many updates to high-frequency documents (beacons, node_states)
    /// - When memory pressure is detected
    /// - Periodically for long-running simulations
    ///
    /// # Returns
    ///
    /// - `Ok(Some(old_size, new_size))` - Document was compacted, returns sizes before/after
    /// - `Ok(None)` - Document not found
    /// - `Err(_)` - Compaction failed
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = AutomergeStore::open("./data")?;
    /// if let Some((old, new)) = store.compact("node_states:soldier-1")? {
    ///     tracing::info!("Compacted {} -> {} bytes ({}% reduction)", old, new, 100 - (new * 100 / old));
    /// }
    /// ```
    pub fn compact(&self, key: &str) -> Result<Option<(usize, usize)>> {
        let doc = match self.get(key)? {
            Some(d) => d,
            None => return Ok(None),
        };

        let old_size = doc.save().len();

        // Fork creates a new document with current state but no history
        let compacted = doc.fork();
        let new_size = compacted.save().len();

        // Save the compacted document (without triggering sync notification)
        self.put_without_notify(key, &compacted)?;

        tracing::debug!(
            "Compacted document {}: {} -> {} bytes ({:.1}% reduction)",
            key,
            old_size,
            new_size,
            if old_size > 0 {
                100.0 - (new_size as f64 * 100.0 / old_size as f64)
            } else {
                0.0
            }
        );

        Ok(Some((old_size, new_size)))
    }

    /// Compact all documents with a given prefix
    ///
    /// Useful for batch-compacting all documents in a collection.
    ///
    /// # Returns
    ///
    /// `(documents_compacted, total_bytes_before, total_bytes_after)`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = AutomergeStore::open("./data")?;
    /// let (count, before, after) = store.compact_prefix("node_states:")?;
    /// tracing::info!("Compacted {} documents: {} -> {} bytes", count, before, after);
    /// ```
    pub fn compact_prefix(&self, prefix: &str) -> Result<(usize, usize, usize)> {
        let docs = self.scan_prefix(prefix)?;
        let mut count = 0;
        let mut total_before = 0;
        let mut total_after = 0;

        for (key, _) in docs {
            if let Some((before, after)) = self.compact(&key)? {
                count += 1;
                total_before += before;
                total_after += after;
            }
        }

        if count > 0 {
            tracing::info!(
                "Compacted {} documents with prefix '{}': {} -> {} bytes ({:.1}% reduction)",
                count,
                prefix,
                total_before,
                total_after,
                if total_before > 0 {
                    100.0 - (total_after as f64 * 100.0 / total_before as f64)
                } else {
                    0.0
                }
            );
        }

        Ok((count, total_before, total_after))
    }

    /// Compact all documents in the store
    ///
    /// # Returns
    ///
    /// `(documents_compacted, total_bytes_before, total_bytes_after)`
    pub fn compact_all(&self) -> Result<(usize, usize, usize)> {
        // In memory-only mode, iterate cache
        if self.db.is_none() {
            let keys: Vec<String> = {
                let cache = self.cache.read().unwrap();
                cache.iter().map(|(k, _)| k.clone()).collect()
            };

            let mut count = 0;
            let mut total_before = 0;
            let mut total_after = 0;

            for key in keys {
                if let Some((before, after)) = self.compact(&key)? {
                    count += 1;
                    total_before += before;
                    total_after += after;
                }
            }

            return Ok((count, total_before, total_after));
        }

        // With disk persistence, scan all documents
        let read_txn = self
            .db
            .as_ref()
            .unwrap()
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(DOCUMENTS_TABLE)
            .context("Failed to open documents table")?;

        let keys: Vec<String> = table
            .iter()?
            .filter_map(|entry| {
                entry
                    .ok()
                    .map(|(k, _)| String::from_utf8_lossy(k.value()).to_string())
            })
            .collect();

        drop(table);
        drop(read_txn);

        let mut count = 0;
        let mut total_before = 0;
        let mut total_after = 0;

        for key in keys {
            if let Some((before, after)) = self.compact(&key)? {
                count += 1;
                total_before += before;
                total_after += after;
            }
        }

        if count > 0 {
            tracing::info!(
                "Compacted {} documents: {} -> {} bytes ({:.1}% reduction)",
                count,
                total_before,
                total_after,
                if total_before > 0 {
                    100.0 - (total_after as f64 * 100.0 / total_before as f64)
                } else {
                    0.0
                }
            );
        }

        Ok((count, total_before, total_after))
    }

    /// Get the serialized size of a document (for monitoring)
    pub fn document_size(&self, key: &str) -> Result<Option<usize>> {
        match self.get(key)? {
            Some(doc) => Ok(Some(doc.save().len())),
            None => Ok(None),
        }
    }
}

// === GcStore trait implementation for AutomergeStore (ADR-034 Phase 3) ===

#[cfg(feature = "automerge-backend")]
impl crate::qos::GcStore for AutomergeStore {
    fn get_all_tombstones(&self) -> anyhow::Result<Vec<crate::qos::Tombstone>> {
        self.get_all_tombstones()
    }

    fn remove_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool> {
        self.remove_tombstone(collection, document_id)
    }

    fn has_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool> {
        self.has_tombstone(collection, document_id)
    }

    fn get_expired_documents(
        &self,
        collection: &str,
        cutoff: std::time::SystemTime,
    ) -> anyhow::Result<Vec<String>> {
        self.get_expired_documents(collection, cutoff)
    }

    fn hard_delete(&self, collection: &str, document_id: &str) -> anyhow::Result<()> {
        self.hard_delete(collection, document_id)
    }

    fn list_collections(&self) -> anyhow::Result<Vec<String>> {
        self.list_collections()
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
        // Get existing document or create a new one
        // This is critical for CRDT sync: we must UPDATE the existing document
        // rather than replacing it with a new one. If we create a new document,
        // Automerge will see it as a conflicting branch and may pick the wrong
        // value during merge.
        let key = self.prefixed_key(doc_id);
        let mut doc = match self.store.get(&key)? {
            Some(existing) => {
                // Fork the existing document to update it
                existing.fork()
            }
            None => {
                // No existing doc, create a new one
                Automerge::new()
            }
        };

        match doc.transact(|tx| {
            tx.put(
                automerge::ROOT,
                "data",
                automerge::ScalarValue::Bytes(data.clone()),
            )?;
            Ok::<(), automerge::AutomergeError>(())
        }) {
            Ok(_) => self.store.put(&key, &doc),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to update Automerge document: {:?}",
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
        tracing::debug!(
            "AutomergeCollection.scan: prefix={}, found {} docs",
            self.prefix,
            docs.len()
        );
        let mut results = Vec::new();

        for (key, doc) in docs {
            tracing::debug!(
                "AutomergeCollection.scan: processing key={}, doc_len={}",
                key,
                doc.save().len()
            );
            if let Some(doc_id) = self.strip_prefix(&key) {
                match doc.get(automerge::ROOT, "data") {
                    Ok(Some((automerge::Value::Scalar(scalar), _))) => {
                        if let automerge::ScalarValue::Bytes(bytes) = scalar.as_ref() {
                            tracing::debug!(
                                "AutomergeCollection.scan: found data bytes, doc_id={}, len={}",
                                doc_id,
                                bytes.len()
                            );
                            results.push((doc_id.to_string(), bytes.to_vec()));
                        } else {
                            tracing::debug!(
                                "AutomergeCollection.scan: data is not Bytes, doc_id={}",
                                doc_id
                            );
                        }
                    }
                    Ok(Some((value, _))) => {
                        tracing::debug!(
                            "AutomergeCollection.scan: data is not Scalar, doc_id={}, value_type={:?}",
                            doc_id,
                            value
                        );
                    }
                    Ok(None) => {
                        tracing::debug!(
                            "AutomergeCollection.scan: no 'data' field, doc_id={}",
                            doc_id
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            "AutomergeCollection.scan: error getting 'data', doc_id={}, err={}",
                            doc_id,
                            e
                        );
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

    // === Compaction Tests (Issue #401 - Memory Blowout Fix) ===

    #[test]
    fn test_compact_document() {
        let (store, _temp) = create_test_store();

        // Create a document and update it many times to build up history
        let mut doc = Automerge::new();
        for i in 0..100 {
            doc.transact::<_, _, automerge::AutomergeError>(|tx| {
                tx.put(automerge::ROOT, "counter", i as i64)?;
                Ok(())
            })
            .unwrap();
        }

        store.put("test-doc", &doc).unwrap();
        let size_before = store.document_size("test-doc").unwrap().unwrap();

        // Compact the document
        let result = store.compact("test-doc").unwrap();
        assert!(result.is_some());
        let (old_size, new_size) = result.unwrap();

        assert_eq!(old_size, size_before);
        assert!(
            new_size <= old_size,
            "Compaction should reduce or maintain size"
        );

        // Verify the document still has the correct value
        let loaded = store.get("test-doc").unwrap().unwrap();
        let value = loaded.get(automerge::ROOT, "counter").unwrap().unwrap();
        assert_eq!(value.0.to_i64(), Some(99));
    }

    #[test]
    fn test_compact_nonexistent_document() {
        let (store, _temp) = create_test_store();
        let result = store.compact("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_compact_prefix() {
        let (store, _temp) = create_test_store();

        // Create multiple documents with history
        for doc_num in 0..5 {
            let mut doc = Automerge::new();
            for i in 0..50 {
                doc.transact::<_, _, automerge::AutomergeError>(|tx| {
                    tx.put(automerge::ROOT, "counter", i as i64)?;
                    Ok(())
                })
                .unwrap();
            }
            store.put(&format!("test:{}", doc_num), &doc).unwrap();
        }

        // Add a document with different prefix
        let mut other_doc = Automerge::new();
        other_doc
            .transact::<_, _, automerge::AutomergeError>(|tx| {
                tx.put(automerge::ROOT, "other", "value")?;
                Ok(())
            })
            .unwrap();
        store.put("other:1", &other_doc).unwrap();

        // Compact only "test:" prefix
        let (count, before, after) = store.compact_prefix("test:").unwrap();
        assert_eq!(count, 5);
        assert!(before > 0);
        assert!(after <= before);

        // Verify other document was not affected
        let other = store.get("other:1").unwrap().unwrap();
        let value = other.get(automerge::ROOT, "other").unwrap().unwrap();
        assert!(value.0.to_str().unwrap().contains("value"));
    }

    #[test]
    fn test_compact_all() {
        let (store, _temp) = create_test_store();

        // Create documents with history
        for prefix in &["a", "b", "c"] {
            let mut doc = Automerge::new();
            for i in 0..30 {
                doc.transact::<_, _, automerge::AutomergeError>(|tx| {
                    tx.put(automerge::ROOT, "counter", i as i64)?;
                    Ok(())
                })
                .unwrap();
            }
            store.put(&format!("{}:doc", prefix), &doc).unwrap();
        }

        let (count, before, after) = store.compact_all().unwrap();
        assert_eq!(count, 3);
        assert!(before > 0);
        assert!(after <= before);
    }

    #[test]
    fn test_compact_in_memory_store() {
        let store = Arc::new(AutomergeStore::in_memory());

        // Create a document and update it many times
        let mut doc = Automerge::new();
        for i in 0..100 {
            doc.transact::<_, _, automerge::AutomergeError>(|tx| {
                tx.put(automerge::ROOT, "counter", i as i64)?;
                Ok(())
            })
            .unwrap();
        }

        store.put("test-doc", &doc).unwrap();

        // Compact should work in memory mode too
        let result = store.compact("test-doc").unwrap();
        assert!(result.is_some());

        // Verify value is preserved
        let loaded = store.get("test-doc").unwrap().unwrap();
        let value = loaded.get(automerge::ROOT, "counter").unwrap().unwrap();
        assert_eq!(value.0.to_i64(), Some(99));
    }
}
