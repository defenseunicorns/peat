//! Sync message forwarding for multi-hop mesh networks
//!
//! This module provides the `SyncForwarder` which enables sync messages to
//! propagate through intermediate nodes in a mesh network, allowing full
//! data coverage with O(k) connections per node instead of O(n) connections.
//!
//! # Architecture
//!
//! When a node receives a sync batch from a peer, the forwarder determines
//! which other connected peers should also receive the batch based on:
//! - Sync direction (Upward, Downward, Lateral, Broadcast)
//! - TTL (time-to-live) hop count
//! - Deduplication to prevent infinite forwarding loops
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::storage::sync_forwarding::SyncForwarder;
//!
//! let forwarder = SyncForwarder::new(local_node_id);
//!
//! // When receiving a batch
//! if let Some(targets) = forwarder.forward_targets(&batch, source_peer, &connected_peers) {
//!     for target in targets {
//!         send_batch_to_peer(&batch, target).await;
//!     }
//!     forwarder.mark_forwarded(batch.batch_id);
//! }
//! ```

use crate::storage::automerge_sync::{SyncBatch, SyncDirection};
use iroh::EndpointId;
use lru::LruCache;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

/// Default capacity for the forwarded batch deduplication cache
const DEFAULT_DEDUP_CACHE_SIZE: usize = 1000;

/// Sync message forwarder for multi-hop mesh networks
///
/// The forwarder tracks which batches have been forwarded to prevent loops
/// and determines the appropriate forwarding targets based on sync direction.
pub struct SyncForwarder {
    /// Local node ID for filtering
    local_node_id: EndpointId,

    /// Parent node ID for upward forwarding (if known)
    parent_id: RwLock<Option<EndpointId>>,

    /// Child node IDs for downward forwarding
    children: RwLock<HashSet<EndpointId>>,

    /// Cache of forwarded batch IDs for deduplication
    forwarded_batches: Arc<RwLock<LruCache<u64, ()>>>,
}

impl SyncForwarder {
    /// Create a new forwarder
    pub fn new(local_node_id: EndpointId) -> Self {
        Self {
            local_node_id,
            parent_id: RwLock::new(None),
            children: RwLock::new(HashSet::new()),
            forwarded_batches: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_DEDUP_CACHE_SIZE).unwrap(),
            ))),
        }
    }

    /// Set the parent node for upward forwarding
    pub fn set_parent(&self, parent_id: Option<EndpointId>) {
        *self.parent_id.write().unwrap() = parent_id;
    }

    /// Add a child node for downward forwarding
    pub fn add_child(&self, child_id: EndpointId) {
        self.children.write().unwrap().insert(child_id);
    }

    /// Remove a child node
    pub fn remove_child(&self, child_id: &EndpointId) {
        self.children.write().unwrap().remove(child_id);
    }

    /// Get the parent node ID
    pub fn parent_id(&self) -> Option<EndpointId> {
        *self.parent_id.read().unwrap()
    }

    /// Get child node IDs
    pub fn children(&self) -> Vec<EndpointId> {
        self.children.read().unwrap().iter().copied().collect()
    }

    /// Check if a batch has already been forwarded
    pub fn was_forwarded(&self, batch_id: u64) -> bool {
        self.forwarded_batches.read().unwrap().contains(&batch_id)
    }

    /// Mark a batch as forwarded (for deduplication)
    pub fn mark_forwarded(&self, batch_id: u64) {
        self.forwarded_batches.write().unwrap().put(batch_id, ());
    }

    /// Determine forwarding targets for a received batch
    ///
    /// Returns None if:
    /// - Batch was already forwarded (duplicate)
    /// - Batch TTL is 0 (expired)
    ///
    /// Returns Some(empty vec) if no forwarding needed.
    /// Returns Some(targets) with the peers to forward to.
    pub fn forward_targets(
        &self,
        batch: &SyncBatch,
        source_peer: EndpointId,
        connected_peers: &[EndpointId],
    ) -> Option<Vec<EndpointId>> {
        // Check if already forwarded (dedup)
        if self.was_forwarded(batch.batch_id) {
            tracing::trace!(
                batch_id = batch.batch_id,
                "Batch already forwarded, skipping"
            );
            return None;
        }

        // Check TTL
        if batch.ttl == 0 {
            tracing::trace!(
                batch_id = batch.batch_id,
                "Batch TTL expired, not forwarding"
            );
            return None;
        }

        // Determine sync direction from batch entries
        // Use the most permissive direction if multiple entries have different
        // directions
        let direction = self.determine_batch_direction(batch);

        let mut targets = HashSet::new();

        match direction {
            SyncDirection::Upward => {
                // Forward to parent only
                if let Some(parent) = self.parent_id() {
                    if parent != source_peer && connected_peers.contains(&parent) {
                        targets.insert(parent);
                    }
                }
            }
            SyncDirection::Downward => {
                // Forward to children only
                for child in self.children() {
                    if child != source_peer && connected_peers.contains(&child) {
                        targets.insert(child);
                    }
                }
            }
            SyncDirection::Lateral => {
                // Forward to peers at same level (excluding source and parent/children)
                let parent = self.parent_id();
                let children = self.children();
                for peer in connected_peers {
                    if *peer != source_peer
                        && *peer != self.local_node_id
                        && Some(*peer) != parent
                        && !children.contains(peer)
                    {
                        targets.insert(*peer);
                    }
                }
            }
            SyncDirection::Broadcast => {
                // Forward to all connected peers except source
                for peer in connected_peers {
                    if *peer != source_peer && *peer != self.local_node_id {
                        targets.insert(*peer);
                    }
                }
            }
        }

        tracing::debug!(
            batch_id = batch.batch_id,
            direction = ?direction,
            ttl = batch.ttl,
            source = %hex::encode(source_peer.as_bytes()),
            target_count = targets.len(),
            "Determined forward targets"
        );

        Some(targets.into_iter().collect())
    }

    /// Determine the sync direction for a batch based on its entries
    fn determine_batch_direction(&self, batch: &SyncBatch) -> SyncDirection {
        let mut most_permissive = SyncDirection::Upward;

        for entry in &batch.entries {
            let dir = SyncDirection::from_doc_key(&entry.doc_key);
            // Broadcast is most permissive, then Lateral, then Downward, then Upward
            most_permissive = match (&most_permissive, &dir) {
                (_, SyncDirection::Broadcast) => SyncDirection::Broadcast,
                (SyncDirection::Broadcast, _) => SyncDirection::Broadcast,
                (_, SyncDirection::Lateral) => SyncDirection::Lateral,
                (SyncDirection::Lateral, _) => SyncDirection::Lateral,
                (_, SyncDirection::Downward) => SyncDirection::Downward,
                (SyncDirection::Downward, _) => SyncDirection::Downward,
                _ => SyncDirection::Upward,
            };

            // Short-circuit if we hit Broadcast
            if matches!(most_permissive, SyncDirection::Broadcast) {
                break;
            }
        }

        most_permissive
    }

    /// Prepare a batch for forwarding by decrementing TTL
    ///
    /// Returns a cloned batch with decremented TTL, or None if TTL would be 0.
    pub fn prepare_for_forward(&self, batch: &SyncBatch) -> Option<SyncBatch> {
        if batch.ttl == 0 {
            return None;
        }

        let mut forwarded = batch.clone();
        forwarded.ttl = batch.ttl.saturating_sub(1);
        Some(forwarded)
    }
}

/// Statistics for sync forwarding
#[derive(Debug, Clone, Default)]
pub struct ForwardingStats {
    /// Total batches received
    pub batches_received: u64,
    /// Batches forwarded to other peers
    pub batches_forwarded: u64,
    /// Batches dropped due to deduplication
    pub batches_deduplicated: u64,
    /// Batches dropped due to TTL expiry
    pub batches_ttl_expired: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::automerge_sync::{SyncEntry, SyncMessageType};

    fn create_test_peer_id() -> EndpointId {
        use iroh::SecretKey;
        let mut rng = rand::rng();
        SecretKey::generate(&mut rng).public()
    }

    fn test_endpoint_id(_n: u8) -> EndpointId {
        // Generate a valid ed25519 public key
        create_test_peer_id()
    }

    #[test]
    fn test_forwarder_new() {
        let local_id = test_endpoint_id(1);
        let forwarder = SyncForwarder::new(local_id);

        assert!(forwarder.parent_id().is_none());
        assert!(forwarder.children().is_empty());
    }

    #[test]
    fn test_set_parent_and_children() {
        let local_id = test_endpoint_id(1);
        let parent_id = test_endpoint_id(2);
        let child_id = test_endpoint_id(3);

        let forwarder = SyncForwarder::new(local_id);
        forwarder.set_parent(Some(parent_id));
        forwarder.add_child(child_id);

        assert_eq!(forwarder.parent_id(), Some(parent_id));
        assert!(forwarder.children().contains(&child_id));

        forwarder.remove_child(&child_id);
        assert!(!forwarder.children().contains(&child_id));
    }

    #[test]
    fn test_deduplication() {
        let local_id = test_endpoint_id(1);
        let forwarder = SyncForwarder::new(local_id);

        let batch_id = 12345;
        assert!(!forwarder.was_forwarded(batch_id));

        forwarder.mark_forwarded(batch_id);
        assert!(forwarder.was_forwarded(batch_id));
    }

    #[test]
    fn test_forward_targets_broadcast() {
        let local_id = test_endpoint_id(1);
        let source_id = test_endpoint_id(2);
        let peer_a = test_endpoint_id(3);
        let peer_b = test_endpoint_id(4);

        let forwarder = SyncForwarder::new(local_id);
        let connected = vec![source_id, peer_a, peer_b];

        // Create a broadcast batch (alerts)
        let mut batch = SyncBatch::with_id(1);
        batch.entries.push(SyncEntry::new(
            "alerts:alert-1".to_string(),
            SyncMessageType::DeltaSync,
            vec![1, 2, 3],
        ));

        let targets = forwarder
            .forward_targets(&batch, source_id, &connected)
            .unwrap();

        // Should forward to peer_a and peer_b, but not source
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&peer_a));
        assert!(targets.contains(&peer_b));
        assert!(!targets.contains(&source_id));
    }

    #[test]
    fn test_forward_targets_upward() {
        let local_id = test_endpoint_id(1);
        let parent_id = test_endpoint_id(2);
        let child_id = test_endpoint_id(3);
        let peer_id = test_endpoint_id(4);

        let forwarder = SyncForwarder::new(local_id);
        forwarder.set_parent(Some(parent_id));
        forwarder.add_child(child_id);

        let connected = vec![parent_id, child_id, peer_id];

        // Create an upward batch (nodes)
        let mut batch = SyncBatch::with_id(2);
        batch.entries.push(SyncEntry::new(
            "nodes:node-1".to_string(),
            SyncMessageType::DeltaSync,
            vec![1, 2, 3],
        ));

        let targets = forwarder
            .forward_targets(&batch, child_id, &connected)
            .unwrap();

        // Should forward to parent only
        assert_eq!(targets.len(), 1);
        assert!(targets.contains(&parent_id));
    }

    #[test]
    fn test_forward_targets_ttl_expired() {
        let local_id = test_endpoint_id(1);
        let source_id = test_endpoint_id(2);
        let peer_id = test_endpoint_id(3);

        let forwarder = SyncForwarder::new(local_id);
        let connected = vec![source_id, peer_id];

        // Create a batch with TTL = 0
        let mut batch = SyncBatch::with_id(3);
        batch.ttl = 0;
        batch.entries.push(SyncEntry::new(
            "alerts:alert-1".to_string(),
            SyncMessageType::DeltaSync,
            vec![1, 2, 3],
        ));

        let targets = forwarder.forward_targets(&batch, source_id, &connected);
        assert!(targets.is_none());
    }

    #[test]
    fn test_prepare_for_forward() {
        let local_id = test_endpoint_id(1);
        let forwarder = SyncForwarder::new(local_id);

        let mut batch = SyncBatch::with_id(4);
        batch.ttl = 3;

        let forwarded = forwarder.prepare_for_forward(&batch).unwrap();
        assert_eq!(forwarded.ttl, 2);

        // Original unchanged
        assert_eq!(batch.ttl, 3);
    }

    #[test]
    fn test_prepare_for_forward_ttl_zero() {
        let local_id = test_endpoint_id(1);
        let forwarder = SyncForwarder::new(local_id);

        let mut batch = SyncBatch::with_id(5);
        batch.ttl = 0;

        let forwarded = forwarder.prepare_for_forward(&batch);
        assert!(forwarded.is_none());
    }
}
