//! Garbage Collection for tombstones and TTL-expired documents (ADR-034 Phase 3)
//!
//! This module provides automatic cleanup of:
//! - Expired tombstones based on collection-specific TTL policies
//! - Documents that exceed their implicit TTL in ImplicitTTL collections
//! - Resurrection detection when offline nodes reconnect with stale documents
//!
//! # Example
//!
//! ```rust,ignore
//! use hive_protocol::qos::{GarbageCollector, GcConfig};
//!
//! // Create GC with default config (5 minute interval)
//! let gc = GarbageCollector::new(store.clone(), GcConfig::default());
//!
//! // Run GC manually
//! let result = gc.run_gc().await?;
//! println!("Collected {} tombstones, {} documents",
//!     result.tombstones_collected, result.documents_collected);
//!
//! // Start periodic GC task
//! gc.start_periodic().await;
//! ```

use super::{DeletionPolicy, DeletionPolicyRegistry, Tombstone};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Policy for handling document resurrection after tombstone expiry
///
/// When an offline node reconnects and tries to sync a document that was
/// deleted (and whose tombstone has since expired), this policy determines
/// what happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ResurrectionPolicy {
    /// Allow resurrection - the document comes back
    /// Best for high-frequency data like beacons where supersession handles staleness
    Allow,

    /// Re-delete with a fresh tombstone (default)
    /// Prevents resurrection but creates a new tombstone that must sync
    #[default]
    ReDelete,

    /// Reject the sync entirely
    /// Most aggressive - document is dropped and peer notified
    Reject,
}

impl ResurrectionPolicy {
    /// Get the default resurrection policy for a collection
    pub fn default_for_collection(collection: &str) -> Self {
        match collection {
            // High-frequency position data - allow resurrection (will be superseded anyway)
            "beacons" | "platforms" | "tracks" => Self::Allow,

            // Critical data - re-delete to prevent stale resurrections
            "nodes" | "cells" | "commands" | "contact_reports" => Self::ReDelete,

            // Alerts - reject to prevent confusion from old alerts
            "alerts" => Self::Reject,

            // Default: re-delete for safety
            _ => Self::ReDelete,
        }
    }
}

/// Configuration for the garbage collector
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// How often to run garbage collection (default: 5 minutes)
    pub gc_interval: Duration,

    /// Maximum number of tombstones to process per GC run (default: 1000)
    /// Prevents GC from blocking for too long
    pub tombstone_batch_size: usize,

    /// Maximum number of documents to process per GC run (default: 500)
    pub document_batch_size: usize,

    /// Whether to log GC operations at debug level
    pub debug_logging: bool,

    /// Resurrection policies per collection (overrides defaults)
    pub resurrection_policies: HashMap<String, ResurrectionPolicy>,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            gc_interval: Duration::from_secs(300), // 5 minutes
            tombstone_batch_size: 1000,
            document_batch_size: 500,
            debug_logging: true,
            resurrection_policies: HashMap::new(),
        }
    }
}

impl GcConfig {
    /// Create a new GC config with a custom interval
    pub fn with_interval(interval: Duration) -> Self {
        Self {
            gc_interval: interval,
            ..Default::default()
        }
    }

    /// Set the resurrection policy for a specific collection
    pub fn set_resurrection_policy(&mut self, collection: &str, policy: ResurrectionPolicy) {
        self.resurrection_policies
            .insert(collection.to_string(), policy);
    }

    /// Get the resurrection policy for a collection
    pub fn resurrection_policy(&self, collection: &str) -> ResurrectionPolicy {
        self.resurrection_policies
            .get(collection)
            .copied()
            .unwrap_or_else(|| ResurrectionPolicy::default_for_collection(collection))
    }
}

/// Result of a garbage collection run
#[derive(Debug, Clone, Default)]
pub struct GcResult {
    /// Number of tombstones collected
    pub tombstones_collected: usize,

    /// Number of documents collected (from ImplicitTTL collections)
    pub documents_collected: usize,

    /// Number of resurrections detected
    pub resurrections_detected: usize,

    /// Number of resurrections that were re-deleted
    pub resurrections_redeleted: usize,

    /// Number of resurrections that were allowed
    pub resurrections_allowed: usize,

    /// Number of resurrections that were rejected
    pub resurrections_rejected: usize,

    /// Duration of the GC run
    pub duration: Duration,

    /// Any errors encountered (non-fatal)
    pub errors: Vec<String>,
}

impl GcResult {
    /// Check if any work was done
    pub fn had_work(&self) -> bool {
        self.tombstones_collected > 0
            || self.documents_collected > 0
            || self.resurrections_detected > 0
    }
}

/// Statistics for the garbage collector
#[derive(Debug, Clone, Default)]
pub struct GcStats {
    /// Total GC runs
    pub total_runs: u64,

    /// Total tombstones collected across all runs
    pub total_tombstones_collected: u64,

    /// Total documents collected across all runs
    pub total_documents_collected: u64,

    /// Total resurrections detected
    pub total_resurrections: u64,

    /// Last GC run time
    pub last_run: Option<SystemTime>,

    /// Last GC duration
    pub last_duration: Option<Duration>,
}

/// Garbage collector for tombstones and TTL-expired documents
///
/// The GarbageCollector periodically cleans up:
/// 1. Tombstones that have exceeded their TTL
/// 2. Documents in ImplicitTTL collections that have expired
///
/// It also handles resurrection detection when syncing with offline nodes.
pub struct GarbageCollector<S> {
    /// Reference to the document store
    store: Arc<S>,

    /// Deletion policy registry
    policy_registry: Arc<DeletionPolicyRegistry>,

    /// GC configuration
    config: GcConfig,

    /// Whether GC is currently running
    running: Arc<AtomicBool>,

    /// Statistics
    stats_tombstones: Arc<AtomicU64>,
    stats_documents: Arc<AtomicU64>,
    stats_resurrections: Arc<AtomicU64>,
    stats_runs: Arc<AtomicU64>,
}

/// Trait for stores that support garbage collection
pub trait GcStore: Send + Sync {
    /// Get all tombstones
    fn get_all_tombstones(&self) -> anyhow::Result<Vec<Tombstone>>;

    /// Remove a tombstone
    fn remove_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool>;

    /// Check if a tombstone exists
    fn has_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool>;

    /// Get document IDs in a collection that are older than the cutoff time
    fn get_expired_documents(
        &self,
        collection: &str,
        cutoff: SystemTime,
    ) -> anyhow::Result<Vec<String>>;

    /// Hard delete a document (permanent removal, no tombstone)
    fn hard_delete(&self, collection: &str, document_id: &str) -> anyhow::Result<()>;

    /// Get list of collections
    fn list_collections(&self) -> anyhow::Result<Vec<String>>;
}

impl<S: GcStore> GarbageCollector<S> {
    /// Create a new garbage collector
    pub fn new(store: Arc<S>, config: GcConfig) -> Self {
        Self {
            store,
            policy_registry: Arc::new(DeletionPolicyRegistry::new()),
            config,
            running: Arc::new(AtomicBool::new(false)),
            stats_tombstones: Arc::new(AtomicU64::new(0)),
            stats_documents: Arc::new(AtomicU64::new(0)),
            stats_resurrections: Arc::new(AtomicU64::new(0)),
            stats_runs: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create with custom policy registry
    pub fn with_policy_registry(
        store: Arc<S>,
        policy_registry: Arc<DeletionPolicyRegistry>,
        config: GcConfig,
    ) -> Self {
        Self {
            store,
            policy_registry,
            config,
            running: Arc::new(AtomicBool::new(false)),
            stats_tombstones: Arc::new(AtomicU64::new(0)),
            stats_documents: Arc::new(AtomicU64::new(0)),
            stats_resurrections: Arc::new(AtomicU64::new(0)),
            stats_runs: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Run garbage collection once
    pub fn run_gc(&self) -> anyhow::Result<GcResult> {
        let start = std::time::Instant::now();
        let mut result = GcResult::default();

        // Collect expired tombstones
        match self.collect_tombstones() {
            Ok(count) => {
                result.tombstones_collected = count;
                self.stats_tombstones
                    .fetch_add(count as u64, Ordering::Relaxed);
            }
            Err(e) => {
                result.errors.push(format!("Tombstone GC error: {}", e));
            }
        }

        // Collect expired documents from ImplicitTTL collections
        match self.collect_expired_documents() {
            Ok(count) => {
                result.documents_collected = count;
                self.stats_documents
                    .fetch_add(count as u64, Ordering::Relaxed);
            }
            Err(e) => {
                result.errors.push(format!("Document GC error: {}", e));
            }
        }

        result.duration = start.elapsed();
        self.stats_runs.fetch_add(1, Ordering::Relaxed);

        if self.config.debug_logging && result.had_work() {
            tracing::info!(
                "GC completed: {} tombstones, {} documents in {:?}",
                result.tombstones_collected,
                result.documents_collected,
                result.duration
            );
        }

        Ok(result)
    }

    /// Collect expired tombstones
    fn collect_tombstones(&self) -> anyhow::Result<usize> {
        let now = SystemTime::now();
        let mut collected = 0;

        let tombstones = self.store.get_all_tombstones()?;

        for tombstone in tombstones.iter().take(self.config.tombstone_batch_size) {
            let policy = self.policy_registry.get(&tombstone.collection);

            if let DeletionPolicy::Tombstone { tombstone_ttl, .. } = policy {
                if let Ok(age) = now.duration_since(tombstone.deleted_at) {
                    if age > tombstone_ttl
                        && self
                            .store
                            .remove_tombstone(&tombstone.collection, &tombstone.document_id)?
                    {
                        collected += 1;

                        if self.config.debug_logging {
                            tracing::debug!(
                                "GC: Removed expired tombstone for {}:{} (age: {:?}, ttl: {:?})",
                                tombstone.collection,
                                tombstone.document_id,
                                age,
                                tombstone_ttl
                            );
                        }
                    }
                }
            }
        }

        Ok(collected)
    }

    /// Collect expired documents from ImplicitTTL collections
    fn collect_expired_documents(&self) -> anyhow::Result<usize> {
        let now = SystemTime::now();
        let mut collected = 0;

        let collections = self.store.list_collections()?;

        for collection in collections {
            let policy = self.policy_registry.get(&collection);

            if let DeletionPolicy::ImplicitTTL { ttl, .. } = policy {
                let cutoff = now - ttl;

                let expired_ids = self.store.get_expired_documents(&collection, cutoff)?;

                for doc_id in expired_ids.iter().take(self.config.document_batch_size) {
                    if let Err(e) = self.store.hard_delete(&collection, doc_id) {
                        tracing::warn!(
                            "GC: Failed to hard delete {}:{}: {}",
                            collection,
                            doc_id,
                            e
                        );
                        continue;
                    }

                    collected += 1;

                    if self.config.debug_logging {
                        tracing::debug!(
                            "GC: Hard deleted expired document {}:{} (ttl: {:?})",
                            collection,
                            doc_id,
                            ttl
                        );
                    }
                }
            }
        }

        Ok(collected)
    }

    /// Check if a document is a resurrection (was deleted, tombstone expired, now being synced)
    ///
    /// Returns the resurrection policy action to take if this is a resurrection.
    pub fn check_resurrection(
        &self,
        collection: &str,
        document_id: &str,
        _document_timestamp: SystemTime,
    ) -> anyhow::Result<Option<ResurrectionPolicy>> {
        // First check if there's currently a tombstone (document is still "deleted")
        if self.store.has_tombstone(collection, document_id)? {
            // Tombstone exists, this is a normal sync conflict, not resurrection
            return Ok(None);
        }

        // No tombstone exists. This could mean:
        // 1. Document was never deleted (normal sync)
        // 2. Document was deleted but tombstone expired (resurrection)
        //
        // We can't distinguish these cases without additional metadata.
        // For now, we rely on the sync layer to detect resurrection by tracking
        // which documents we've seen deleted.
        //
        // The sync layer should call handle_resurrection() when it detects one.

        Ok(None)
    }

    /// Handle a resurrection according to policy
    pub fn handle_resurrection(
        &self,
        collection: &str,
        document_id: &str,
    ) -> anyhow::Result<ResurrectionPolicy> {
        let policy = self.config.resurrection_policy(collection);

        self.stats_resurrections.fetch_add(1, Ordering::Relaxed);

        tracing::info!(
            "Resurrection detected for {}:{}, policy: {:?}",
            collection,
            document_id,
            policy
        );

        Ok(policy)
    }

    /// Get current statistics
    pub fn stats(&self) -> GcStats {
        GcStats {
            total_runs: self.stats_runs.load(Ordering::Relaxed),
            total_tombstones_collected: self.stats_tombstones.load(Ordering::Relaxed),
            total_documents_collected: self.stats_documents.load(Ordering::Relaxed),
            total_resurrections: self.stats_resurrections.load(Ordering::Relaxed),
            last_run: None, // Would need additional tracking
            last_duration: None,
        }
    }

    /// Check if GC is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the GC interval
    pub fn interval(&self) -> Duration {
        self.config.gc_interval
    }

    /// Stop the GC task
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Start a periodic garbage collection task
///
/// This spawns a tokio task that runs GC at the configured interval.
/// Returns a JoinHandle that can be used to await or abort the task.
///
/// # Example
///
/// ```rust,ignore
/// use hive_protocol::qos::{GarbageCollector, GcConfig, start_periodic_gc};
/// use std::sync::Arc;
///
/// let gc = Arc::new(GarbageCollector::new(store.clone(), GcConfig::default()));
/// let handle = start_periodic_gc(gc.clone());
///
/// // Later, to stop:
/// gc.stop();
/// handle.abort();
/// ```
pub fn start_periodic_gc<S: GcStore + 'static>(
    gc: Arc<GarbageCollector<S>>,
) -> tokio::task::JoinHandle<()> {
    let interval = gc.interval();
    gc.running.store(true, Ordering::SeqCst);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);

        // Skip the first immediate tick
        ticker.tick().await;

        while gc.running.load(Ordering::SeqCst) {
            ticker.tick().await;

            if !gc.running.load(Ordering::SeqCst) {
                break;
            }

            match gc.run_gc() {
                Ok(result) => {
                    if result.had_work() {
                        tracing::info!(
                            "Periodic GC: collected {} tombstones, {} documents in {:?}",
                            result.tombstones_collected,
                            result.documents_collected,
                            result.duration
                        );
                    } else {
                        tracing::debug!("Periodic GC: no work to do");
                    }
                }
                Err(e) => {
                    tracing::error!("Periodic GC failed: {}", e);
                }
            }
        }

        tracing::info!("Periodic GC task stopped");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resurrection_policy_defaults() {
        assert_eq!(
            ResurrectionPolicy::default_for_collection("beacons"),
            ResurrectionPolicy::Allow
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("platforms"),
            ResurrectionPolicy::Allow
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("tracks"),
            ResurrectionPolicy::Allow
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("nodes"),
            ResurrectionPolicy::ReDelete
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("cells"),
            ResurrectionPolicy::ReDelete
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("alerts"),
            ResurrectionPolicy::Reject
        );
        assert_eq!(
            ResurrectionPolicy::default_for_collection("unknown"),
            ResurrectionPolicy::ReDelete
        );
    }

    #[test]
    fn test_gc_config_default() {
        let config = GcConfig::default();
        assert_eq!(config.gc_interval, Duration::from_secs(300));
        assert_eq!(config.tombstone_batch_size, 1000);
        assert_eq!(config.document_batch_size, 500);
        assert!(config.debug_logging);
    }

    #[test]
    fn test_gc_config_with_interval() {
        let config = GcConfig::with_interval(Duration::from_secs(60));
        assert_eq!(config.gc_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_gc_config_resurrection_policy_override() {
        let mut config = GcConfig::default();
        config.set_resurrection_policy("beacons", ResurrectionPolicy::ReDelete);

        // Override should take precedence
        assert_eq!(
            config.resurrection_policy("beacons"),
            ResurrectionPolicy::ReDelete
        );

        // Non-overridden should use default
        assert_eq!(
            config.resurrection_policy("tracks"),
            ResurrectionPolicy::Allow
        );
    }

    #[test]
    fn test_gc_result_had_work() {
        let empty = GcResult::default();
        assert!(!empty.had_work());

        let with_tombstones = GcResult {
            tombstones_collected: 1,
            ..Default::default()
        };
        assert!(with_tombstones.had_work());

        let with_documents = GcResult {
            documents_collected: 1,
            ..Default::default()
        };
        assert!(with_documents.had_work());

        let with_resurrections = GcResult {
            resurrections_detected: 1,
            ..Default::default()
        };
        assert!(with_resurrections.had_work());
    }

    // Mock store for testing
    struct MockGcStore {
        tombstones: Vec<Tombstone>,
        collections: Vec<String>,
    }

    impl MockGcStore {
        fn new() -> Self {
            Self {
                tombstones: Vec::new(),
                collections: Vec::new(),
            }
        }

        fn with_tombstones(tombstones: Vec<Tombstone>) -> Self {
            Self {
                tombstones,
                collections: Vec::new(),
            }
        }
    }

    impl GcStore for MockGcStore {
        fn get_all_tombstones(&self) -> anyhow::Result<Vec<Tombstone>> {
            Ok(self.tombstones.clone())
        }

        fn remove_tombstone(&self, _collection: &str, _document_id: &str) -> anyhow::Result<bool> {
            Ok(true)
        }

        fn has_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool> {
            Ok(self
                .tombstones
                .iter()
                .any(|t| t.collection == collection && t.document_id == document_id))
        }

        fn get_expired_documents(
            &self,
            _collection: &str,
            _cutoff: SystemTime,
        ) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn hard_delete(&self, _collection: &str, _document_id: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn list_collections(&self) -> anyhow::Result<Vec<String>> {
            Ok(self.collections.clone())
        }
    }

    #[test]
    fn test_gc_with_mock_store() {
        let store = Arc::new(MockGcStore::new());
        let gc = GarbageCollector::new(store, GcConfig::default());

        let result = gc.run_gc().unwrap();
        assert_eq!(result.tombstones_collected, 0);
        assert_eq!(result.documents_collected, 0);
        assert!(!result.had_work());
    }

    #[test]
    fn test_gc_stats() {
        let store = Arc::new(MockGcStore::new());
        let gc = GarbageCollector::new(store, GcConfig::default());

        let stats = gc.stats();
        assert_eq!(stats.total_runs, 0);
        assert_eq!(stats.total_tombstones_collected, 0);

        gc.run_gc().unwrap();

        let stats = gc.stats();
        assert_eq!(stats.total_runs, 1);
    }

    #[test]
    fn test_gc_collect_expired_tombstones() {
        // Create an expired tombstone (2 days old)
        let old_time = SystemTime::now() - Duration::from_secs(86400 * 2);
        let mut tombstone = Tombstone::new("node-1", "nodes", "test-node", 1);
        tombstone.deleted_at = old_time;

        let store = Arc::new(MockGcStore::with_tombstones(vec![tombstone]));
        let gc = GarbageCollector::new(store, GcConfig::default());

        let result = gc.run_gc().unwrap();
        // The tombstone should be collected because it's older than the 24hr TTL
        assert_eq!(result.tombstones_collected, 1);
    }
}
