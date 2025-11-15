//! Automerge sync protocol implementation
//!
//! This module provides the sync coordinator that manages Automerge document
//! synchronization over Iroh QUIC streams.
//!
//! # Phase 4 Implementation
//!
//! Implements the Automerge sync protocol (https://arxiv.org/abs/2012.00472) over
//! Iroh P2P connections to enable CRDT document synchronization.
//!
//! ## Sync Flow
//!
//! ```text
//! Node A                          Node B
//!   │                               │
//!   ├─ Document updated             │
//!   ├─ generate_sync_message() ────→│
//!   │                               ├─ receive_sync_message()
//!   │                               ├─ apply changes
//!   │                               ├─ generate_sync_message()
//!   │←────────────────────────────┤
//!   ├─ receive_sync_message()       │
//!   ├─ apply changes                │
//!   │                               │
//!   ├─ Synced! ✅                   ├─ Synced! ✅
//! ```
//!
//! ## Wire Format
//!
//! Sync messages are sent over Iroh bidirectional streams with length prefixing:
//! ```text
//! [4 bytes: message length (u32, big-endian)][N bytes: serialized sync::Message]
//! ```

#[cfg(feature = "automerge-backend")]
use super::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use crate::network::iroh_transport::IrohTransport;
#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use automerge::sync::{Message as SyncMessage, State as SyncState, SyncDoc};
#[cfg(feature = "automerge-backend")]
use automerge::Automerge;
#[cfg(feature = "automerge-backend")]
use iroh::endpoint::Connection;
#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
#[allow(unused_imports)] // Used in sync message send/receive methods
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Coordinator for Automerge document synchronization over Iroh
///
/// Manages sync state for each peer and coordinates message exchange.
#[cfg(feature = "automerge-backend")]
pub struct AutomergeSyncCoordinator {
    /// Reference to the AutomergeStore
    store: Arc<AutomergeStore>,
    /// Reference to the IrohTransport
    transport: Arc<IrohTransport>,
    /// Sync state for each peer (per document)
    /// Map: document_key -> peer_id -> SyncState
    peer_states: Arc<RwLock<HashMap<String, HashMap<EndpointId, SyncState>>>>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeSyncCoordinator {
    /// Create a new sync coordinator
    ///
    /// # Arguments
    ///
    /// * `store` - The AutomergeStore managing documents
    /// * `transport` - The IrohTransport for P2P connections
    pub fn new(store: Arc<AutomergeStore>, transport: Arc<IrohTransport>) -> Self {
        Self {
            store,
            transport,
            peer_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initiate sync for a document with a peer
    ///
    /// Generates an initial sync message and sends it to the peer.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "cells:cell-1")
    /// * `peer_id` - The EndpointId of the peer to sync with
    pub async fn initiate_sync(&self, doc_key: &str, peer_id: EndpointId) -> Result<()> {
        // Get the document
        let doc = self
            .store
            .get(doc_key)?
            .context("Document not found for sync")?;

        // Get or create sync state for this peer
        let mut sync_state = self.get_or_create_sync_state(doc_key, peer_id);

        // Generate initial sync message using SyncDoc trait
        let message = SyncDoc::generate_sync_message(&doc, &mut sync_state)
            .context("No initial sync message to send")?;

        // Store updated sync state
        self.update_sync_state(doc_key, peer_id, sync_state);

        // Send message to peer with document key
        self.send_sync_message_for_doc(peer_id, doc_key, &message)
            .await?;

        Ok(())
    }

    /// Receive and process a sync message from a peer
    ///
    /// Applies the changes to the document and generates a response message if needed.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier
    /// * `peer_id` - The EndpointId of the sending peer
    /// * `message` - The received sync message
    pub async fn receive_sync_message(
        &self,
        doc_key: &str,
        peer_id: EndpointId,
        message: SyncMessage,
    ) -> Result<()> {
        // Get the document (or create empty one if doesn't exist)
        let mut doc = self.store.get(doc_key)?.unwrap_or_else(Automerge::new);

        // Get or create sync state for this peer
        let mut sync_state = self.get_or_create_sync_state(doc_key, peer_id);

        // Apply the sync message using SyncDoc trait
        SyncDoc::receive_sync_message(&mut doc, &mut sync_state, message)?;

        // Save updated document
        self.store.put(doc_key, &doc)?;

        // Generate response message
        if let Some(response) = SyncDoc::generate_sync_message(&doc, &mut sync_state) {
            // Store updated sync state
            self.update_sync_state(doc_key, peer_id, sync_state);

            // Send response to peer with document key
            self.send_sync_message_for_doc(peer_id, doc_key, &response)
                .await?;
        } else {
            // Store sync state even if no response needed
            self.update_sync_state(doc_key, peer_id, sync_state);
        }

        Ok(())
    }

    /// Send a sync message to a peer over Iroh stream
    ///
    /// Wire format: [2 bytes: doc_key length][N bytes: doc_key UTF-8][4 bytes: message length][M bytes: encoded message]
    async fn send_sync_message_for_doc(
        &self,
        peer_id: EndpointId,
        doc_key: &str,
        message: &SyncMessage,
    ) -> Result<()> {
        // Get connection to peer
        let conn = self
            .transport
            .get_connection(&peer_id)
            .context("No connection to peer")?;

        // Open a bidirectional stream
        let (mut send, _recv) = conn
            .open_bi()
            .await
            .context("Failed to open bidirectional stream")?;

        // Encode doc_key as UTF-8 bytes
        let doc_key_bytes = doc_key.as_bytes();
        let doc_key_len = doc_key_bytes.len() as u16;

        // Write doc_key length prefix (2 bytes, big-endian)
        send.write_all(&doc_key_len.to_be_bytes())
            .await
            .context("Failed to write doc_key length")?;

        // Write doc_key
        send.write_all(doc_key_bytes)
            .await
            .context("Failed to write doc_key")?;

        // Encode the sync message (clone since encode() takes ownership)
        let encoded = message.clone().encode();

        // Write message length prefix (4 bytes, big-endian)
        let message_len = encoded.len() as u32;
        send.write_all(&message_len.to_be_bytes())
            .await
            .context("Failed to write message length")?;

        // Write the message
        send.write_all(&encoded)
            .await
            .context("Failed to write message")?;

        // Finish the stream
        send.finish().context("Failed to finish stream")?;

        Ok(())
    }

    /// Receive a sync message from a peer over Iroh stream
    ///
    /// Wire format: [2 bytes: doc_key length][N bytes: doc_key UTF-8][4 bytes: message length][M bytes: encoded message]
    async fn receive_sync_message_from_stream(
        &self,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(String, SyncMessage)> {
        // Read doc_key length prefix (2 bytes, big-endian)
        let mut doc_key_len_bytes = [0u8; 2];
        recv.read_exact(&mut doc_key_len_bytes)
            .await
            .context("Failed to read doc_key length")?;
        let doc_key_len = u16::from_be_bytes(doc_key_len_bytes) as usize;

        // Read doc_key
        let mut doc_key_bytes = vec![0u8; doc_key_len];
        recv.read_exact(&mut doc_key_bytes)
            .await
            .context("Failed to read doc_key")?;
        let doc_key =
            String::from_utf8(doc_key_bytes).context("Failed to parse doc_key as UTF-8")?;

        // Read message length prefix (4 bytes, big-endian)
        let mut message_len_bytes = [0u8; 4];
        recv.read_exact(&mut message_len_bytes)
            .await
            .context("Failed to read message length")?;
        let message_len = u32::from_be_bytes(message_len_bytes) as usize;

        // Read the message
        let mut buffer = vec![0u8; message_len];
        recv.read_exact(&mut buffer)
            .await
            .context("Failed to read message")?;

        // Decode the sync message
        let message = SyncMessage::decode(&buffer).context("Failed to decode sync message")?;

        Ok((doc_key, message))
    }

    /// Get or create sync state for a peer
    fn get_or_create_sync_state(&self, doc_key: &str, peer_id: EndpointId) -> SyncState {
        let mut states = self.peer_states.write().unwrap();
        states
            .entry(doc_key.to_string())
            .or_default()
            .entry(peer_id)
            .or_default()
            .clone()
    }

    /// Update sync state for a peer
    fn update_sync_state(&self, doc_key: &str, peer_id: EndpointId, state: SyncState) {
        let mut states = self.peer_states.write().unwrap();
        states
            .entry(doc_key.to_string())
            .or_default()
            .insert(peer_id, state);
    }

    /// Sync a specific document with a peer
    ///
    /// This initiates sync for a single document with a peer.
    /// Use this when a document has been created or modified.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "nodes:node-1")
    /// * `peer_id` - The EndpointId of the peer to sync with
    pub async fn sync_document_with_peer(&self, doc_key: &str, peer_id: EndpointId) -> Result<()> {
        self.initiate_sync(doc_key, peer_id).await
    }

    /// Sync a document with all connected peers
    ///
    /// This initiates sync for a single document with all currently connected peers.
    ///
    /// # Arguments
    ///
    /// * `doc_key` - The document identifier (e.g., "nodes:node-1")
    pub async fn sync_document_with_all_peers(&self, doc_key: &str) -> Result<()> {
        let peer_ids = self.transport.connected_peers();

        for peer_id in peer_ids {
            if let Err(e) = self.sync_document_with_peer(doc_key, peer_id).await {
                tracing::warn!("Failed to sync {} with peer {:?}: {}", doc_key, peer_id, e);
            }
        }

        Ok(())
    }

    /// Handle an incoming sync connection from a peer
    ///
    /// This is called when a peer initiates sync with us.
    pub async fn handle_incoming_sync(&self, conn: Connection) -> Result<()> {
        let peer_id = conn.remote_id();

        // Accept a bidirectional stream
        let (_send, recv) = conn
            .accept_bi()
            .await
            .context("Failed to accept bidirectional stream")?;

        // Receive the sync message (now includes doc_key in wire format)
        let (doc_key, message) = self.receive_sync_message_from_stream(recv).await?;

        // Process the message
        self.receive_sync_message(&doc_key, peer_id, message)
            .await?;

        Ok(())
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_coordinator() -> (AutomergeSyncCoordinator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        let transport = Arc::new(IrohTransport::new().await.unwrap());
        let coordinator = AutomergeSyncCoordinator::new(store, transport);
        (coordinator, temp_dir)
    }

    #[tokio::test]
    async fn test_coordinator_creation() {
        let (coordinator, _temp) = create_test_coordinator().await;
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_sync_state_management() {
        let (coordinator, _temp) = create_test_coordinator().await;
        let peer_id = coordinator.transport.endpoint_id();

        // Get or create sync state
        let state1 = coordinator.get_or_create_sync_state("doc1", peer_id);
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 1);

        // Update sync state
        coordinator.update_sync_state("doc1", peer_id, state1);

        // Get same state again
        let _state2 = coordinator.get_or_create_sync_state("doc1", peer_id);
        assert_eq!(coordinator.peer_states.read().unwrap().len(), 1);
    }
}
