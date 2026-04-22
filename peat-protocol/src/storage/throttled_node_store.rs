//! Throttled state updates for reduced write load
//!
//! This module provides a throttling wrapper around NodeStore to batch
//! state updates and reduce the frequency of writes to the backend at scale.

use crate::models::node::NodeState;
use crate::storage::node_store::NodeStore;
use crate::sync::DataSyncBackend;
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, instrument, warn};

/// Wrapper around NodeStore that throttles state updates
///
/// Batches pending updates and only syncs to backend after a configurable interval.
/// This significantly reduces write load when many nodes are updating frequently.
///
/// # Example
/// ```ignore
/// use peat_protocol::storage::{NodeStore, ThrottledNodeStore};
/// use peat_protocol::models::node::{NodeState, NodeStateExt};
/// use std::time::Duration;
/// use std::sync::Arc;
///
/// # async fn example() -> peat_protocol::Result<()> {
/// // Assuming you have a NodeStore backed by an AutomergeIrohBackend
/// # let backend: Arc<_> = unimplemented!();
/// # let store = NodeStore::new(backend).await?;
/// let throttled = ThrottledNodeStore::new(store, Duration::from_secs(5));
///
/// // Updates are queued, not immediately written
/// let state = NodeState::new((37.7, -122.4, 100.0));
/// throttled.update_state("node1", &state).await?;
///
/// // Force flush if needed
/// throttled.flush().await?;
/// # Ok(())
/// # }
/// ```
pub struct ThrottledNodeStore<B: DataSyncBackend> {
    /// Inner node store for actual backend operations
    inner: NodeStore<B>,
    /// Pending state updates (node_id -> state)
    pending_updates: Arc<Mutex<HashMap<String, NodeState>>>,
    /// Last time we synced to backend
    last_sync: Arc<Mutex<Instant>>,
    /// Minimum time between syncs
    sync_interval: Duration,
}

impl<B: DataSyncBackend> ThrottledNodeStore<B> {
    /// Create a new throttled store wrapper
    ///
    /// # Arguments
    /// * `store` - The underlying NodeStore to wrap
    /// * `sync_interval` - Minimum duration between backend syncs
    pub fn new(store: NodeStore<B>, sync_interval: Duration) -> Self {
        Self {
            inner: store,
            pending_updates: Arc::new(Mutex::new(HashMap::new())),
            last_sync: Arc::new(Mutex::new(Instant::now())),
            sync_interval,
        }
    }

    /// Queue a state update, syncing if interval has elapsed
    ///
    /// If the sync interval has elapsed, immediately flushes all pending updates.
    /// Otherwise, queues the update for the next flush.
    #[instrument(skip(self, state))]
    pub async fn update_state(&self, node_id: &str, state: &NodeState) -> Result<()> {
        debug!("Queueing state update for node: {}", node_id);

        // Add to pending updates
        {
            let mut pending = self.pending_updates.lock().await;
            pending.insert(node_id.to_string(), state.clone());
        }

        // Check if we should sync now
        let should_sync = {
            let last_sync = self.last_sync.lock().await;
            last_sync.elapsed() >= self.sync_interval
        };

        if should_sync {
            self.flush().await?;
        }

        Ok(())
    }

    /// Force flush all pending updates to the backend
    ///
    /// Writes all queued state updates to the underlying NodeStore and clears the queue.
    #[instrument(skip(self))]
    pub async fn flush(&self) -> Result<()> {
        let mut pending = self.pending_updates.lock().await;

        if pending.is_empty() {
            debug!("No pending updates to flush");
            return Ok(());
        }

        info!("Flushing {} pending state updates", pending.len());

        let mut errors = Vec::new();

        // Write all pending updates
        for (node_id, state) in pending.iter() {
            if let Err(e) = self.inner.store_state(node_id, state).await {
                warn!("Failed to store state for {}: {}", node_id, e);
                errors.push((node_id.clone(), e));
            }
        }

        // Clear pending updates
        pending.clear();

        // Update last sync time
        {
            let mut last_sync = self.last_sync.lock().await;
            *last_sync = Instant::now();
        }

        if !errors.is_empty() {
            return Err(Error::Internal(format!(
                "Failed to flush {} state updates",
                errors.len()
            )));
        }

        Ok(())
    }

    /// Get the number of pending updates
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending_updates.lock().await;
        pending.len()
    }

    /// Get statistics about the throttled store
    pub async fn stats(&self) -> ThrottleStats {
        let pending = self.pending_updates.lock().await;
        let last_sync = self.last_sync.lock().await;

        ThrottleStats {
            pending_updates: pending.len(),
            time_since_last_sync: last_sync.elapsed(),
            sync_interval: self.sync_interval,
            should_sync_now: last_sync.elapsed() >= self.sync_interval,
        }
    }

    /// Get a reference to the inner NodeStore for direct operations
    ///
    /// Use this for read operations that don't need throttling
    pub fn inner(&self) -> &NodeStore<B> {
        &self.inner
    }
}

/// Statistics about the throttled store
#[derive(Debug, Clone)]
pub struct ThrottleStats {
    /// Number of updates waiting to be flushed
    pub pending_updates: usize,
    /// Time since last sync to the backend
    pub time_since_last_sync: Duration,
    /// Configured sync interval
    pub sync_interval: Duration,
    /// Whether a sync should happen now
    pub should_sync_now: bool,
}
