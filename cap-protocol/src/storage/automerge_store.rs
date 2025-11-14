//! Automerge document storage with RocksDB persistence
//!
//! This module provides persistent storage for Automerge CRDT documents using RocksDB.

#[cfg(feature = "automerge-backend")]
use automerge::Automerge;
#[cfg(feature = "automerge-backend")]
use lru::LruCache;
#[cfg(feature = "automerge-backend")]
use rocksdb::{IteratorMode, Options, DB};
#[cfg(feature = "automerge-backend")]
use std::num::NonZeroUsize;
#[cfg(feature = "automerge-backend")]
use std::path::Path;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};

#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};

/// Storage layer for Automerge documents with RocksDB persistence
#[cfg(feature = "automerge-backend")]
pub struct AutomergeStore {
    db: Arc<DB>,
    cache: Arc<RwLock<LruCache<String, Automerge>>>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeStore {
    /// Open or create storage at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_open_files(512);
        opts.set_write_buffer_size(64 * 1024 * 1024);

        let db = DB::open(&opts, path).context("Failed to open RocksDB")?;
        let cache = LruCache::new(NonZeroUsize::new(1000).unwrap());

        Ok(Self {
            db: Arc::new(db),
            cache: Arc::new(RwLock::new(cache)),
        })
    }

    /// Save an Automerge document
    pub fn put(&self, key: &str, doc: &Automerge) -> Result<()> {
        let bytes = doc.save();
        self.db
            .put(key.as_bytes(), &bytes)
            .context("Failed to write to RocksDB")?;

        self.cache
            .write()
            .unwrap()
            .put(key.to_string(), doc.clone());

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

        match self.db.get(key.as_bytes())? {
            Some(bytes) => {
                let doc = Automerge::load(&bytes).context("Failed to load Automerge document")?;

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
        self.db.delete(key.as_bytes())?;
        self.cache.write().unwrap().pop(key);
        Ok(())
    }

    /// Scan documents with prefix
    pub fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Automerge)>> {
        let mut results = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(
            prefix.as_bytes(),
            rocksdb::Direction::Forward,
        ));

        for item in iter {
            let (key, value) = item?;
            if !key.starts_with(prefix.as_bytes()) {
                break;
            }

            let key_str = String::from_utf8_lossy(&key).to_string();
            let doc = Automerge::load(&value)?;
            results.push((key_str, doc));
        }

        Ok(results)
    }

    /// Count total documents
    pub fn count(&self) -> usize {
        self.db.iterator(IteratorMode::Start).count()
    }
}
