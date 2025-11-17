//! TTL Manager for automatic document expiration
//!
//! This module provides automatic cleanup of expired documents based on Time-To-Live (TTL)
//! configuration. It is particularly critical for beacon documents which must expire after
//! 30 seconds to prevent stale position data from affecting cell formation.
//!
//! # Architecture
//!
//! ```text
//! Collection API
//!     ↓
//! upsert_with_ttl(key, data, ttl)
//!     ↓
//! TtlManager::set_ttl(key, ttl)
//!     ↓
//! BTreeMap<Instant, Vec<String>>
//!     ↓
//! Background cleanup task (every 10s)
//!     ↓
//! AutomergeStore::delete()
//! ```
//!
//! # Two-Layer TTL Strategy (ADR-002)
//!
//! **Ditto Layer**: Automatic eviction via Ditto SDK
//! **Memory Layer**: Janitor cleanup for Automerge+Iroh (this module)
//!
//! # Usage Example
//!
//! ```ignore
//! let ttl_manager = TtlManager::new(store.clone(), config.clone());
//! ttl_manager.start_background_cleanup();
//!
//! // Set TTL for a beacon document
//! ttl_manager.set_ttl("beacons", "node-123", Duration::from_secs(30))?;
//!
//! // Document will be automatically deleted after 30 seconds
//! ```

use super::automerge_store::AutomergeStore;
use super::ttl::TtlConfig;
use anyhow::Result;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

/// TTL Manager for automatic document expiration
///
/// Tracks document expiry times and runs background cleanup task to remove
/// expired documents from the store.
pub struct TtlManager {
    /// Underlying Automerge store for document operations
    store: Arc<AutomergeStore>,

    /// TTL configuration
    config: TtlConfig,

    /// Expiry tracking: Instant → Vec<key>
    ///
    /// BTreeMap ensures efficient range queries for expired documents.
    /// Each expiry time maps to all document keys that expire at that time.
    /// Keys are in the format "collection/doc_id" (e.g., "beacons/node-123").
    expiry_map: Arc<RwLock<BTreeMap<Instant, Vec<String>>>>,

    /// Background cleanup task handle
    cleanup_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl TtlManager {
    /// Create a new TTL Manager
    ///
    /// # Arguments
    ///
    /// * `store` - AutomergeStore for document deletion
    /// * `config` - TTL configuration (beacon_ttl, position_ttl, etc.)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = AutomergeStore::new(iroh_transport.clone());
    /// let config = TtlConfig::tactical(); // 30s beacon TTL
    /// let ttl_manager = TtlManager::new(store, config);
    /// ```
    pub fn new(store: Arc<AutomergeStore>, config: TtlConfig) -> Self {
        Self {
            store,
            config,
            expiry_map: Arc::new(RwLock::new(BTreeMap::new())),
            cleanup_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Schedule a document for expiration
    ///
    /// # Arguments
    ///
    /// * `key` - Full document key in format "collection/doc_id" (e.g., "beacons/node-123")
    /// * `ttl` - Time until expiration
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Beacon expires in 30 seconds
    /// ttl_manager.set_ttl("beacons/node-123", Duration::from_secs(30))?;
    /// ```
    pub fn set_ttl(&self, key: &str, ttl: Duration) -> Result<()> {
        let expiry_time = Instant::now() + ttl;

        let mut expiry_map = self.expiry_map.write().unwrap();
        expiry_map
            .entry(expiry_time)
            .or_default()
            .push(key.to_string());

        Ok(())
    }

    /// Remove all expired documents
    ///
    /// This method is called by the background cleanup task every 10 seconds.
    /// It finds all documents with expiry times <= now and deletes them.
    ///
    /// # Returns
    ///
    /// Number of documents cleaned up
    pub fn cleanup_expired(&self) -> Result<usize> {
        let now = Instant::now();
        let mut count = 0;

        // Get all expired entries (expiry_time <= now)
        let expired_keys = {
            let mut expiry_map = self.expiry_map.write().unwrap();

            // Split at first entry > now, take everything before
            let split_key = expiry_map
                .range(..=now)
                .next_back()
                .map(|(k, _)| *k)
                .unwrap_or(now);

            // Collect and remove all expired entries
            let expired: Vec<_> = expiry_map
                .range(..=split_key)
                .flat_map(|(_, keys)| keys.clone())
                .collect();

            // Remove from map
            expiry_map.retain(|&expiry_time, _| expiry_time > now);

            expired
        };

        // Delete each expired document
        for key in expired_keys {
            self.store.delete(&key)?;
            count += 1;
        }

        Ok(count)
    }

    /// Start background cleanup task
    ///
    /// Spawns a tokio task that runs cleanup_expired() every 10 seconds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ttl_manager = TtlManager::new(store, config);
    /// ttl_manager.start_background_cleanup();
    /// ```
    pub fn start_background_cleanup(&self) {
        let expiry_map = self.expiry_map.clone();
        let store = self.store.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                // Run cleanup
                let now = Instant::now();
                let expired_docs = {
                    let mut expiry_map = expiry_map.write().unwrap();

                    // Get all expired entries
                    let split_key = expiry_map
                        .range(..=now)
                        .next_back()
                        .map(|(k, _)| *k)
                        .unwrap_or(now);

                    let expired: Vec<_> = expiry_map
                        .range(..=split_key)
                        .flat_map(|(_, docs)| docs.clone())
                        .collect();

                    // Remove from map
                    expiry_map.retain(|&expiry_time, _| expiry_time > now);

                    expired
                };

                // Delete expired documents
                for key in expired_docs {
                    if let Err(e) = store.delete(&key) {
                        eprintln!("TTL cleanup failed for {}: {}", key, e);
                    }
                }
            }
        });

        *self.cleanup_task.write().unwrap() = Some(handle);
    }

    /// Stop background cleanup task
    pub fn stop_background_cleanup(&self) {
        if let Some(handle) = self.cleanup_task.write().unwrap().take() {
            handle.abort();
        }
    }

    /// Get TTL configuration
    pub fn config(&self) -> &TtlConfig {
        &self.config
    }

    /// Get count of documents scheduled for expiration
    pub fn pending_count(&self) -> usize {
        self.expiry_map
            .read()
            .unwrap()
            .values()
            .map(|docs| docs.len())
            .sum()
    }
}

impl Drop for TtlManager {
    fn drop(&mut self) {
        self.stop_background_cleanup();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::Automerge;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_set_ttl() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = Arc::new(AutomergeStore::open(temp_dir.path())?);
        let config = TtlConfig::tactical();
        let ttl_manager = TtlManager::new(store, config);

        // Set TTL for a beacon
        ttl_manager.set_ttl("beacons/node-123", Duration::from_secs(30))?;

        // Verify it's tracked
        assert_eq!(ttl_manager.pending_count(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_cleanup_expired() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = Arc::new(AutomergeStore::open(temp_dir.path())?);
        let config = TtlConfig::tactical();
        let ttl_manager = TtlManager::new(store.clone(), config);

        // Insert a test document
        let doc = Automerge::new();
        store.put("beacons/node-123", &doc)?;

        // Set very short TTL (100ms)
        ttl_manager.set_ttl("beacons/node-123", Duration::from_millis(100))?;

        // Wait for expiration
        sleep(Duration::from_millis(150)).await;

        // Run cleanup
        let count = ttl_manager.cleanup_expired()?;
        assert_eq!(count, 1);

        // Verify document is gone
        let result = store.get("beacons/node-123")?;
        assert!(result.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_background_cleanup() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = Arc::new(AutomergeStore::open(temp_dir.path())?);
        let config = TtlConfig::tactical();
        let ttl_manager = TtlManager::new(store.clone(), config);

        // Insert test document
        let doc = Automerge::new();
        store.put("beacons/node-456", &doc)?;

        // Start background cleanup
        ttl_manager.start_background_cleanup();

        // Set very short TTL (1 second)
        ttl_manager.set_ttl("beacons/node-456", Duration::from_secs(1))?;

        // Wait for background cleanup (runs every 10s, but document expires in 1s)
        // We need to wait at least 11 seconds for cleanup to run
        sleep(Duration::from_secs(11)).await;

        // Verify document is gone
        let result = store.get("beacons/node-456")?;
        assert!(result.is_none());

        ttl_manager.stop_background_cleanup();

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_documents_same_expiry() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = Arc::new(AutomergeStore::open(temp_dir.path())?);
        let config = TtlConfig::tactical();
        let ttl_manager = TtlManager::new(store.clone(), config);

        // Insert multiple documents
        for i in 0..5 {
            let doc = Automerge::new();
            store.put(&format!("beacons/node-{}", i), &doc)?;
            ttl_manager.set_ttl(&format!("beacons/node-{}", i), Duration::from_millis(100))?;
        }

        assert_eq!(ttl_manager.pending_count(), 5);

        // Wait for expiration
        sleep(Duration::from_millis(150)).await;

        // Run cleanup
        let count = ttl_manager.cleanup_expired()?;
        assert_eq!(count, 5);

        // Verify all documents are gone
        for i in 0..5 {
            let result = store.get(&format!("beacons/node-{}", i))?;
            assert!(result.is_none());
        }

        Ok(())
    }
}
