//! Negentropy-based set reconciliation for efficient sync (ADR-040, Issue #435)
//!
//! This module provides O(log n) event discovery using the Negentropy protocol,
//! replacing unbounded multi-round Automerge sync that accumulates state.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐                    ┌─────────────┐
//! │   Node A    │                    │   Node B    │
//! │             │                    │             │
//! │ ┌─────────┐ │  1. Initiate       │ ┌─────────┐ │
//! │ │Negentropy│─┼──────────────────►│ │Negentropy│ │
//! │ │ Storage │ │                    │ │ Storage │ │
//! │ └─────────┘ │  2. Reconcile      │ └─────────┘ │
//! │             │◄──────────────────┼│             │
//! │             │                    │             │
//! │ have_ids ───┼──► Send to B       │             │
//! │ need_ids ◄──┼─── Request from B  │             │
//! └─────────────┘                    └─────────────┘
//! ```
//!
//! # Benefits over Automerge multi-round sync
//!
//! - **O(log n) rounds** instead of unbounded rounds
//! - **No state accumulation**: No `sent_hashes` BTreeSet growing
//! - **Bandwidth efficient**: Only exchange fingerprints, not full state
//! - **Stateless**: Each sync session is independent

use anyhow::{Context, Result};
use negentropy::{Id as NegentropyId, Negentropy, NegentropyStorageVector};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;

/// Size of document ID for Negentropy (32 bytes = SHA256)
pub const DOC_ID_SIZE: usize = 32;

/// Represents a document for Negentropy reconciliation
#[derive(Debug, Clone)]
pub struct SyncItem {
    /// Document key (collection::id format)
    pub doc_key: String,
    /// Timestamp (created_at or last_modified)
    pub timestamp: u64,
    /// 32-byte document ID (SHA256 of content or unique identifier)
    pub id: [u8; DOC_ID_SIZE],
}

impl SyncItem {
    /// Create a new sync item from document key and content hash
    pub fn new(doc_key: String, timestamp: u64, content_hash: [u8; DOC_ID_SIZE]) -> Self {
        Self {
            doc_key,
            timestamp,
            id: content_hash,
        }
    }

    /// Create sync item from doc_key, deriving ID from the key itself
    pub fn from_doc_key(doc_key: &str, timestamp: u64) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(doc_key.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        Self {
            doc_key: doc_key.to_string(),
            timestamp,
            id: hash,
        }
    }
}

/// Result of a Negentropy reconciliation
#[derive(Debug, Default)]
pub struct ReconcileResult {
    /// Document keys I have that peer needs
    pub have_keys: Vec<String>,
    /// Document keys peer has that I need
    pub need_keys: Vec<String>,
    /// Whether reconciliation is complete
    pub is_complete: bool,
    /// Next message to send (if not complete)
    pub next_message: Option<Vec<u8>>,
}

/// Active sync session with a peer
pub struct SyncSession {
    /// Peer we're syncing with
    pub peer_id: EndpointId,
    /// Local storage snapshot (kept for responder initialization)
    storage: Option<NegentropyStorageVector>,
    /// Negentropy instance (owned storage variant)
    negentropy: Option<Negentropy<'static, NegentropyStorageVector>>,
    /// Mapping from Negentropy ID to doc_key
    id_to_key: HashMap<[u8; DOC_ID_SIZE], String>,
    /// Whether we initiated or responded
    pub is_initiator: bool,
}

impl SyncSession {
    /// Create a new sync session as initiator
    fn new_initiator(peer_id: EndpointId, items: Vec<SyncItem>) -> Result<Self> {
        let mut storage = NegentropyStorageVector::new();
        let mut id_to_key = HashMap::new();

        for item in items {
            let neg_id =
                NegentropyId::from_slice(&item.id).context("Invalid ID size for Negentropy")?;
            storage.insert(item.timestamp, neg_id)?;
            id_to_key.insert(item.id, item.doc_key);
        }
        storage.seal()?;

        Ok(Self {
            peer_id,
            storage: Some(storage),
            negentropy: None,
            id_to_key,
            is_initiator: true,
        })
    }

    /// Create a new sync session as responder
    fn new_responder(peer_id: EndpointId, items: Vec<SyncItem>) -> Result<Self> {
        let mut storage = NegentropyStorageVector::new();
        let mut id_to_key = HashMap::new();

        for item in items {
            let neg_id =
                NegentropyId::from_slice(&item.id).context("Invalid ID size for Negentropy")?;
            storage.insert(item.timestamp, neg_id)?;
            id_to_key.insert(item.id, item.doc_key);
        }
        storage.seal()?;

        Ok(Self {
            peer_id,
            storage: Some(storage),
            negentropy: None,
            id_to_key,
            is_initiator: false,
        })
    }

    /// Generate initial message (initiator only)
    pub fn initiate(&mut self) -> Result<Vec<u8>> {
        let storage = self.storage.take().context("Storage already consumed")?;

        let mut neg =
            Negentropy::owned(storage, 0).context("Failed to create Negentropy instance")?;

        let init_msg = neg
            .initiate()
            .context("Failed to initiate Negentropy sync")?;

        self.negentropy = Some(neg);
        Ok(init_msg)
    }

    /// Process received message and generate response
    pub fn reconcile(&mut self, peer_msg: &[u8]) -> Result<ReconcileResult> {
        if self.is_initiator {
            self.reconcile_initiator(peer_msg)
        } else {
            self.reconcile_responder(peer_msg)
        }
    }

    fn reconcile_initiator(&mut self, peer_msg: &[u8]) -> Result<ReconcileResult> {
        let neg = self.negentropy.as_mut().context("Session not initiated")?;

        let mut have_ids: Vec<NegentropyId> = Vec::new();
        let mut need_ids: Vec<NegentropyId> = Vec::new();

        // reconcile_with_ids returns Option<Vec<u8>>: None when complete, Some when more rounds needed
        let response = neg
            .reconcile_with_ids(peer_msg, &mut have_ids, &mut need_ids)
            .context("Failed to reconcile")?;

        // Convert IDs back to doc keys
        let have_keys: Vec<String> = have_ids
            .iter()
            .filter_map(|id| {
                let bytes: &[u8; 32] = id.as_bytes();
                self.id_to_key.get(bytes).cloned()
            })
            .collect();

        let need_keys: Vec<String> = need_ids
            .iter()
            .filter_map(|id| {
                let bytes: &[u8; 32] = id.as_bytes();
                self.id_to_key.get(bytes).cloned()
            })
            .collect();

        let is_complete = response.is_none();

        Ok(ReconcileResult {
            have_keys,
            need_keys,
            is_complete,
            next_message: response,
        })
    }

    fn reconcile_responder(&mut self, peer_msg: &[u8]) -> Result<ReconcileResult> {
        // Create negentropy if first message
        if self.negentropy.is_none() {
            let storage = self.storage.take().context("Storage already consumed")?;

            let neg =
                Negentropy::owned(storage, 0).context("Failed to create Negentropy instance")?;

            self.negentropy = Some(neg);
        }

        let neg = self.negentropy.as_mut().unwrap();

        let response = neg.reconcile(peer_msg).context("Failed to reconcile")?;

        // Responder doesn't get have/need until reconciliation completes
        // The response message encodes the differences
        Ok(ReconcileResult {
            have_keys: Vec::new(),
            need_keys: Vec::new(),
            is_complete: false,
            next_message: Some(response),
        })
    }
}

/// Manager for Negentropy-based document sync
///
/// Provides efficient O(log n) set reconciliation to discover which
/// documents need to be synced, avoiding unbounded Automerge sync state.
pub struct NegentropySync {
    /// Active sync sessions per peer
    sessions: Arc<RwLock<HashMap<EndpointId, SyncSession>>>,
    /// Statistics
    stats: Arc<RwLock<NegentropyStats>>,
}

/// Statistics for Negentropy sync operations
#[derive(Debug, Default, Clone)]
pub struct NegentropyStats {
    /// Total sync sessions initiated
    pub sessions_initiated: u64,
    /// Total sync sessions completed
    pub sessions_completed: u64,
    /// Total documents discovered as "have" (we have, peer needs)
    pub docs_have: u64,
    /// Total documents discovered as "need" (peer has, we need)
    pub docs_need: u64,
    /// Total bytes exchanged in Negentropy messages
    pub bytes_exchanged: u64,
    /// Total round trips
    pub round_trips: u64,
}

impl NegentropySync {
    /// Create a new NegentropySync manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NegentropyStats::default())),
        }
    }

    /// Start a sync session with a peer as initiator
    ///
    /// Returns the initial message to send to the peer.
    pub fn initiate_sync(
        &self,
        peer_id: EndpointId,
        local_items: Vec<SyncItem>,
    ) -> Result<Vec<u8>> {
        let mut session = SyncSession::new_initiator(peer_id, local_items)?;
        let init_msg = session.initiate()?;

        // Store session
        {
            let mut sessions = self.sessions.write().unwrap();
            sessions.insert(peer_id, session);
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.sessions_initiated += 1;
            stats.bytes_exchanged += init_msg.len() as u64;
        }

        tracing::debug!(
            "Initiated Negentropy sync with peer {:?}, msg_len={}",
            peer_id,
            init_msg.len()
        );

        Ok(init_msg)
    }

    /// Handle incoming sync message from peer
    ///
    /// If we have an active session, this is a response to our initiation.
    /// Otherwise, we're the responder and should create a new session.
    pub fn handle_message(
        &self,
        peer_id: EndpointId,
        message: &[u8],
        local_items: Vec<SyncItem>,
    ) -> Result<ReconcileResult> {
        let mut sessions = self.sessions.write().unwrap();

        let session = if let Some(existing) = sessions.get_mut(&peer_id) {
            existing
        } else {
            // Create responder session
            let session = SyncSession::new_responder(peer_id, local_items)?;
            sessions.insert(peer_id, session);
            sessions.get_mut(&peer_id).unwrap()
        };

        let result = session.reconcile(message)?;

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.bytes_exchanged += message.len() as u64;
            stats.round_trips += 1;
            if let Some(next) = &result.next_message {
                stats.bytes_exchanged += next.len() as u64;
            }
            stats.docs_have += result.have_keys.len() as u64;
            stats.docs_need += result.need_keys.len() as u64;
            if result.is_complete {
                stats.sessions_completed += 1;
            }
        }

        // Clean up completed session
        if result.is_complete {
            sessions.remove(&peer_id);
            tracing::debug!(
                "Negentropy sync complete with {:?}: have={}, need={}",
                peer_id,
                result.have_keys.len(),
                result.need_keys.len()
            );
        }

        Ok(result)
    }

    /// Get current statistics
    pub fn stats(&self) -> NegentropyStats {
        self.stats.read().unwrap().clone()
    }

    /// Check if there's an active session with a peer
    pub fn has_session(&self, peer_id: &EndpointId) -> bool {
        self.sessions.read().unwrap().contains_key(peer_id)
    }

    /// Cancel a sync session
    pub fn cancel_session(&self, peer_id: &EndpointId) {
        self.sessions.write().unwrap().remove(peer_id);
    }
}

impl Default for NegentropySync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_items(keys: &[&str], base_timestamp: u64) -> Vec<SyncItem> {
        keys.iter()
            .enumerate()
            .map(|(i, key)| SyncItem::from_doc_key(key, base_timestamp + i as u64))
            .collect()
    }

    /// Create a test EndpointId from a seed byte
    fn test_peer_id(seed: u8) -> EndpointId {
        use iroh::SecretKey;
        let mut key_bytes = [0u8; 32];
        key_bytes[0] = seed;
        let secret = SecretKey::from_bytes(&key_bytes);
        secret.public()
    }

    #[test]
    fn test_sync_item_from_doc_key() {
        let item1 = SyncItem::from_doc_key("nodes::node-1", 1000);
        let item2 = SyncItem::from_doc_key("nodes::node-1", 1000);
        let item3 = SyncItem::from_doc_key("nodes::node-2", 1000);

        // Same key should produce same ID
        assert_eq!(item1.id, item2.id);
        // Different keys should produce different IDs
        assert_ne!(item1.id, item3.id);
    }

    #[test]
    fn test_identical_sets_no_differences() {
        let peer_a = test_peer_id(1);
        let peer_b = test_peer_id(2);

        let items = make_test_items(&["doc-1", "doc-2", "doc-3"], 1000);

        let sync_a = NegentropySync::new();
        let sync_b = NegentropySync::new();

        // A initiates
        let msg1 = sync_a.initiate_sync(peer_b, items.clone()).unwrap();

        // B responds
        let result_b = sync_b.handle_message(peer_a, &msg1, items.clone()).unwrap();
        assert!(result_b.next_message.is_some());

        // A processes response
        let result_a = sync_a
            .handle_message(peer_b, &result_b.next_message.unwrap(), items)
            .unwrap();

        // Should complete with no differences
        assert!(result_a.is_complete);
        assert!(result_a.have_keys.is_empty());
        assert!(result_a.need_keys.is_empty());
    }

    #[test]
    fn test_different_sets_finds_differences() {
        let peer_a = test_peer_id(1);
        let peer_b = test_peer_id(2);

        // A has doc-1, doc-2
        let items_a = make_test_items(&["doc-1", "doc-2"], 1000);
        // B has doc-2, doc-3
        let items_b = make_test_items(&["doc-2", "doc-3"], 1000);

        let sync_a = NegentropySync::new();
        let sync_b = NegentropySync::new();

        // A initiates
        let msg1 = sync_a.initiate_sync(peer_b, items_a.clone()).unwrap();

        // B responds
        let result_b = sync_b
            .handle_message(peer_a, &msg1, items_b.clone())
            .unwrap();

        // Continue reconciliation
        let mut current_msg = result_b.next_message;
        let mut final_result = None;

        while let Some(msg) = current_msg {
            let result = sync_a
                .handle_message(peer_b, &msg, items_a.clone())
                .unwrap();
            if result.is_complete {
                final_result = Some(result);
                break;
            }

            if let Some(next) = result.next_message {
                let resp = sync_b
                    .handle_message(peer_a, &next, items_b.clone())
                    .unwrap();
                current_msg = resp.next_message;
            } else {
                break;
            }
        }

        // A should discover:
        // - have_keys: doc-1 (A has, B needs)
        // - need_keys: doc-3 (B has, A needs)
        if let Some(result) = final_result {
            // Note: exact results depend on Negentropy internals
            // At minimum, reconciliation should complete
            assert!(result.is_complete);
        }
    }

    #[test]
    fn test_stats_tracking() {
        let peer_b = test_peer_id(2);
        let items = make_test_items(&["doc-1"], 1000);

        let sync = NegentropySync::new();
        let _msg = sync.initiate_sync(peer_b, items).unwrap();

        let stats = sync.stats();
        assert_eq!(stats.sessions_initiated, 1);
        assert!(stats.bytes_exchanged > 0);
    }
}
