//! Persistence abstraction for HIVE documents
//!
//! This module provides traits for persisting HIVE mesh state across restarts.
//! Platform implementations can use platform-specific storage backends:
//!
//! - **ESP32**: NVS (Non-Volatile Storage)
//! - **iOS/macOS**: Keychain or UserDefaults
//! - **Android**: SharedPreferences or EncryptedSharedPreferences
//! - **Linux**: File-based or SQLite
//!
//! ## Usage
//!
//! ```rust,no_run
//! use hive_btle::persistence::{DocumentStore, MemoryStore};
//! use hive_btle::document::HiveDocument;
//! use hive_btle::NodeId;
//!
//! // Use the in-memory store for testing
//! let mut store = MemoryStore::new();
//!
//! // Save a document
//! let doc = HiveDocument::new(NodeId::new(0x12345678));
//! store.save(&doc).unwrap();
//!
//! // Load it back
//! let loaded = store.load().unwrap();
//! assert!(loaded.is_some());
//! ```

use crate::document::HiveDocument;
use crate::error::Result;

#[cfg(feature = "std")]
use std::sync::{Arc, RwLock};

/// Trait for persisting HIVE documents
///
/// Implementations of this trait provide durable storage for mesh state,
/// allowing nodes to recover their document after restarts.
///
/// ## Implementation Notes
///
/// - `save()` should be called after significant state changes (new peers, emergencies)
/// - `load()` should be called during mesh initialization
/// - Implementations should handle concurrent access safely
/// - Consider encryption for sensitive deployment scenarios
pub trait DocumentStore: Send + Sync {
    /// Save the current document to persistent storage
    ///
    /// This should serialize the document and write it to durable storage.
    /// Implementations should handle errors gracefully and return appropriate
    /// error types.
    fn save(&mut self, doc: &HiveDocument) -> Result<()>;

    /// Load a previously saved document
    ///
    /// Returns `Ok(Some(doc))` if a document was found, `Ok(None)` if no
    /// document exists (first run), or `Err` if loading failed.
    fn load(&self) -> Result<Option<HiveDocument>>;

    /// Clear any stored document
    ///
    /// Use this for factory reset or when leaving a mesh.
    fn clear(&mut self) -> Result<()>;

    /// Check if a document is stored
    fn has_document(&self) -> bool {
        self.load().ok().flatten().is_some()
    }
}

/// In-memory document store for testing
///
/// This store keeps the document in memory only - it will be lost on restart.
/// Useful for unit tests and development.
#[cfg(feature = "std")]
#[derive(Default)]
pub struct MemoryStore {
    document: RwLock<Option<HiveDocument>>,
}

#[cfg(feature = "std")]
impl MemoryStore {
    /// Create a new empty memory store
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a memory store pre-populated with a document
    pub fn with_document(doc: HiveDocument) -> Self {
        Self {
            document: RwLock::new(Some(doc)),
        }
    }
}

#[cfg(feature = "std")]
impl DocumentStore for MemoryStore {
    fn save(&mut self, doc: &HiveDocument) -> Result<()> {
        let mut stored = self.document.write().unwrap();
        *stored = Some(doc.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<HiveDocument>> {
        let stored = self.document.read().unwrap();
        Ok(stored.clone())
    }

    fn clear(&mut self) -> Result<()> {
        let mut stored = self.document.write().unwrap();
        *stored = None;
        Ok(())
    }
}

/// File-based document store
///
/// Stores the document as a binary file on the filesystem.
/// Suitable for Linux desktop/server deployments.
#[cfg(feature = "std")]
pub struct FileStore {
    path: std::path::PathBuf,
}

#[cfg(feature = "std")]
impl FileStore {
    /// Create a new file store at the given path
    pub fn new<P: Into<std::path::PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    /// Get the storage path
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(feature = "std")]
impl DocumentStore for FileStore {
    fn save(&mut self, doc: &HiveDocument) -> Result<()> {
        let data = doc.encode();
        std::fs::write(&self.path, data).map_err(|e| {
            crate::error::BleError::NotSupported(format!("Failed to write document: {}", e))
        })?;
        Ok(())
    }

    fn load(&self) -> Result<Option<HiveDocument>> {
        match std::fs::read(&self.path) {
            Ok(data) => Ok(HiveDocument::decode(&data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(crate::error::BleError::NotSupported(format!(
                "Failed to read document: {}",
                e
            ))),
        }
    }

    fn clear(&mut self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(crate::error::BleError::NotSupported(format!(
                "Failed to clear document: {}",
                e
            ))),
        }
    }
}

/// Wrapper to make a DocumentStore shareable across threads
#[cfg(feature = "std")]
pub struct SharedStore<S: DocumentStore> {
    inner: Arc<RwLock<S>>,
}

#[cfg(feature = "std")]
impl<S: DocumentStore> SharedStore<S> {
    /// Wrap a store for shared access
    pub fn new(store: S) -> Self {
        Self {
            inner: Arc::new(RwLock::new(store)),
        }
    }
}

#[cfg(feature = "std")]
impl<S: DocumentStore> Clone for SharedStore<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(feature = "std")]
impl<S: DocumentStore> DocumentStore for SharedStore<S> {
    fn save(&mut self, doc: &HiveDocument) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.save(doc)
    }

    fn load(&self) -> Result<Option<HiveDocument>> {
        let inner = self.inner.read().unwrap();
        inner.load()
    }

    fn clear(&mut self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[test]
    fn test_memory_store() {
        let mut store = MemoryStore::new();

        // Initially empty
        assert!(store.load().unwrap().is_none());
        assert!(!store.has_document());

        // Save a document
        let doc = HiveDocument::new(NodeId::new(0x12345678));
        store.save(&doc).unwrap();

        // Load it back
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.node_id.as_u32(), 0x12345678);
        assert!(store.has_document());

        // Clear it
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn test_file_store() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("hive_test_doc.bin");

        // Clean up from any previous test
        let _ = std::fs::remove_file(&path);

        let mut store = FileStore::new(&path);

        // Initially empty
        assert!(store.load().unwrap().is_none());

        // Save a document
        let mut doc = HiveDocument::new(NodeId::new(0xAABBCCDD));
        doc.increment_counter();
        store.save(&doc).unwrap();

        // Load it back
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.node_id.as_u32(), 0xAABBCCDD);
        assert_eq!(loaded.counter.value(), 1);

        // Clear it
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    #[test]
    fn test_shared_store() {
        let store = MemoryStore::new();
        let mut shared = SharedStore::new(store);

        let doc = HiveDocument::new(NodeId::new(0x11111111));
        shared.save(&doc).unwrap();

        // Clone and read from the clone
        let shared2 = shared.clone();
        let loaded = shared2.load().unwrap().unwrap();
        assert_eq!(loaded.node_id.as_u32(), 0x11111111);
    }
}
