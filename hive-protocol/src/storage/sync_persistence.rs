//! Sync state persistence with redb
//!
//! This module provides durable storage for Automerge sync state, enabling:
//! - Persist sync heads per peer
//! - Recovery on restart (resume from last position)
//! - Periodic checkpointing
//!
//! # Storage Schema
//!
//! Uses redb with key prefixes:
//! - `sync_state:{peer_id}:{doc_key}` → serialized automerge::sync::State
//! - `checkpoint:{timestamp}` → snapshot metadata
//! - `meta:last_checkpoint` → timestamp of last checkpoint

#[cfg(feature = "automerge-backend")]
use anyhow::{Context, Result};
#[cfg(feature = "automerge-backend")]
use automerge::sync::State as SyncState;
#[cfg(feature = "automerge-backend")]
use iroh::EndpointId;
#[cfg(feature = "automerge-backend")]
use redb::{Database, ReadableDatabase, TableDefinition};
#[cfg(feature = "automerge-backend")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::path::Path;
#[cfg(feature = "automerge-backend")]
use std::sync::Arc;
#[cfg(feature = "automerge-backend")]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Table for sync state storage
#[cfg(feature = "automerge-backend")]
const SYNC_STATE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("sync_states");

/// Table for checkpoint storage
#[cfg(feature = "automerge-backend")]
const CHECKPOINT_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("checkpoints");

/// Table for metadata
#[cfg(feature = "automerge-backend")]
const META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("meta");

/// Key prefixes for storage
#[cfg(feature = "automerge-backend")]
const SYNC_STATE_PREFIX: &str = "sync_state:";
#[cfg(feature = "automerge-backend")]
const CHECKPOINT_PREFIX: &str = "checkpoint:";
#[cfg(feature = "automerge-backend")]
const META_LAST_CHECKPOINT: &str = "last_checkpoint";

/// Serializable wrapper for automerge::sync::State
///
/// The Automerge SyncState isn't directly serializable, so we store
/// the encoded bytes along with metadata.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSyncState {
    /// Encoded sync state bytes
    pub state_bytes: Vec<u8>,
    /// Peer ID (hex encoded for serialization)
    pub peer_id_hex: String,
    /// Document key
    pub doc_key: String,
    /// Timestamp when state was saved
    pub saved_at: u64,
    /// Number of syncs completed
    pub sync_count: u64,
}

#[cfg(feature = "automerge-backend")]
impl PersistedSyncState {
    /// Create from SyncState and metadata
    pub fn from_sync_state(
        state: &SyncState,
        peer_id: &EndpointId,
        doc_key: &str,
        sync_count: u64,
    ) -> Self {
        Self {
            state_bytes: state.encode(),
            peer_id_hex: hex::encode(peer_id.as_bytes()),
            doc_key: doc_key.to_string(),
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            sync_count,
        }
    }

    /// Restore SyncState from persisted data
    pub fn to_sync_state(&self) -> Result<SyncState> {
        SyncState::decode(&self.state_bytes).context("Failed to decode sync state")
    }
}

/// Checkpoint metadata
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Timestamp of checkpoint
    pub timestamp: u64,
    /// Number of sync states saved
    pub state_count: usize,
    /// Total bytes stored
    pub total_bytes: usize,
    /// Peer IDs included
    pub peer_ids: Vec<String>,
}

/// Statistics about persisted sync state
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Default)]
pub struct PersistenceStats {
    /// Number of sync states stored
    pub state_count: usize,
    /// Total bytes used
    pub total_bytes: usize,
    /// Number of peers with stored state
    pub peer_count: usize,
    /// Last checkpoint timestamp
    pub last_checkpoint: Option<u64>,
    /// Number of checkpoints
    pub checkpoint_count: usize,
}

/// Sync state persistence manager
///
/// Provides durable storage for Automerge sync state to enable
/// fast recovery after restart without full resync.
#[cfg(feature = "automerge-backend")]
pub struct SyncStatePersistence {
    /// redb instance
    db: Arc<Database>,
    /// Checkpoint interval (how often to create checkpoints)
    checkpoint_interval: Duration,
}

#[cfg(feature = "automerge-backend")]
impl SyncStatePersistence {
    /// Open or create sync state storage at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // redb stores in a single file, append .redb extension if it's a directory path
        let db_path = if path.is_dir() || !path.exists() {
            std::fs::create_dir_all(path).ok();
            path.join("sync_state.redb")
        } else {
            path.to_path_buf()
        };

        let db = Database::create(&db_path).context("Failed to open sync state redb")?;

        // Initialize tables
        {
            let write_txn = db
                .begin_write()
                .context("Failed to begin write transaction")?;
            let _ = write_txn.open_table(SYNC_STATE_TABLE);
            let _ = write_txn.open_table(CHECKPOINT_TABLE);
            let _ = write_txn.open_table(META_TABLE);
            write_txn
                .commit()
                .context("Failed to commit table creation")?;
        }

        Ok(Self {
            db: Arc::new(db),
            checkpoint_interval: Duration::from_secs(60), // Default: checkpoint every 60 seconds
        })
    }

    /// Open with custom checkpoint interval
    pub fn open_with_interval(
        path: impl AsRef<Path>,
        checkpoint_interval: Duration,
    ) -> Result<Self> {
        let mut persistence = Self::open(path)?;
        persistence.checkpoint_interval = checkpoint_interval;
        Ok(persistence)
    }

    /// Build storage key for sync state
    fn sync_state_key(peer_id: &EndpointId, doc_key: &str) -> String {
        format!(
            "{}{}:{}",
            SYNC_STATE_PREFIX,
            hex::encode(peer_id.as_bytes()),
            doc_key
        )
    }

    /// Save sync state for a peer and document
    pub fn save_sync_state(
        &self,
        peer_id: &EndpointId,
        doc_key: &str,
        state: &SyncState,
        sync_count: u64,
    ) -> Result<()> {
        let key = Self::sync_state_key(peer_id, doc_key);
        let persisted = PersistedSyncState::from_sync_state(state, peer_id, doc_key, sync_count);

        let value = serde_json::to_vec(&persisted).context("Failed to serialize sync state")?;

        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(SYNC_STATE_TABLE)
                .context("Failed to open sync state table")?;
            table
                .insert(key.as_bytes(), value.as_slice())
                .context("Failed to write sync state")?;
        }
        write_txn.commit().context("Failed to commit write")?;

        tracing::trace!(
            "Saved sync state for peer {} doc {}: {} bytes",
            persisted.peer_id_hex,
            doc_key,
            value.len()
        );

        Ok(())
    }

    /// Load sync state for a peer and document
    pub fn load_sync_state(
        &self,
        peer_id: &EndpointId,
        doc_key: &str,
    ) -> Result<Option<(SyncState, u64)>> {
        let key = Self::sync_state_key(peer_id, doc_key);

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(SYNC_STATE_TABLE)
            .context("Failed to open sync state table")?;

        match table.get(key.as_bytes())? {
            Some(value) => {
                let bytes = value.value();
                let persisted: PersistedSyncState =
                    serde_json::from_slice(bytes).context("Failed to deserialize sync state")?;

                let state = persisted.to_sync_state()?;

                tracing::trace!(
                    "Loaded sync state for peer {} doc {}: sync_count={}",
                    persisted.peer_id_hex,
                    doc_key,
                    persisted.sync_count
                );

                Ok(Some((state, persisted.sync_count)))
            }
            None => Ok(None),
        }
    }

    /// Delete sync state for a peer and document
    pub fn delete_sync_state(&self, peer_id: &EndpointId, doc_key: &str) -> Result<()> {
        let key = Self::sync_state_key(peer_id, doc_key);

        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(SYNC_STATE_TABLE)
                .context("Failed to open sync state table")?;
            table.remove(key.as_bytes())?;
        }
        write_txn.commit().context("Failed to commit delete")?;

        Ok(())
    }

    /// Load all sync states for a peer
    pub fn load_all_for_peer(&self, peer_id: &EndpointId) -> Result<HashMap<String, SyncState>> {
        let prefix = format!("{}{}:", SYNC_STATE_PREFIX, hex::encode(peer_id.as_bytes()));
        let mut results = HashMap::new();

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(SYNC_STATE_TABLE)
            .context("Failed to open sync state table")?;

        for entry in table.range(prefix.as_bytes()..)? {
            let (key, value) = entry?;
            let key_bytes = key.value();
            let key_str = String::from_utf8_lossy(key_bytes);

            if !key_str.starts_with(&prefix) {
                break;
            }

            let persisted: PersistedSyncState = serde_json::from_slice(value.value())?;
            let state = persisted.to_sync_state()?;
            results.insert(persisted.doc_key.clone(), state);
        }

        Ok(results)
    }

    /// Load all sync states (for full recovery)
    pub fn load_all(&self) -> Result<HashMap<(EndpointId, String), SyncState>> {
        let mut results = HashMap::new();

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(SYNC_STATE_TABLE)
            .context("Failed to open sync state table")?;

        for entry in table.range(SYNC_STATE_PREFIX.as_bytes()..)? {
            let (key, value) = entry?;
            let key_bytes = key.value();
            let key_str = String::from_utf8_lossy(key_bytes);

            if !key_str.starts_with(SYNC_STATE_PREFIX) {
                break;
            }

            let persisted: PersistedSyncState = serde_json::from_slice(value.value())?;

            // Parse peer ID from hex
            let peer_id_bytes =
                hex::decode(&persisted.peer_id_hex).context("Invalid peer ID hex")?;
            if peer_id_bytes.len() != 32 {
                continue; // Skip invalid entries
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&peer_id_bytes);
            let public_key = iroh::PublicKey::from_bytes(&arr)
                .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;
            let peer_id: EndpointId = public_key;

            let state = persisted.to_sync_state()?;
            results.insert((peer_id, persisted.doc_key.clone()), state);
        }

        tracing::info!("Loaded {} sync states from persistence", results.len());

        Ok(results)
    }

    /// Create a checkpoint
    pub fn create_checkpoint(&self) -> Result<Checkpoint> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Count current states
        let mut state_count = 0;
        let mut total_bytes = 0;
        let mut peer_ids = std::collections::HashSet::new();

        {
            let read_txn = self
                .db
                .begin_read()
                .context("Failed to begin read transaction")?;
            let table = read_txn
                .open_table(SYNC_STATE_TABLE)
                .context("Failed to open sync state table")?;

            for entry in table.range(SYNC_STATE_PREFIX.as_bytes()..)? {
                let (key, value) = entry?;
                let key_bytes = key.value();
                let key_str = String::from_utf8_lossy(key_bytes);

                if !key_str.starts_with(SYNC_STATE_PREFIX) {
                    break;
                }

                state_count += 1;
                total_bytes += value.value().len();

                // Extract peer ID from key
                if let Some(rest) = key_str.strip_prefix(SYNC_STATE_PREFIX) {
                    if let Some(peer_id) = rest.split(':').next() {
                        peer_ids.insert(peer_id.to_string());
                    }
                }
            }
        }

        let checkpoint = Checkpoint {
            timestamp,
            state_count,
            total_bytes,
            peer_ids: peer_ids.into_iter().collect(),
        };

        // Save checkpoint
        let checkpoint_key = format!("{}{}", CHECKPOINT_PREFIX, timestamp);
        let checkpoint_bytes = serde_json::to_vec(&checkpoint)?;

        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(CHECKPOINT_TABLE)
                .context("Failed to open checkpoint table")?;
            table.insert(checkpoint_key.as_bytes(), checkpoint_bytes.as_slice())?;
        }
        {
            // Update last checkpoint timestamp
            let mut meta_table = write_txn
                .open_table(META_TABLE)
                .context("Failed to open meta table")?;
            meta_table.insert(
                META_LAST_CHECKPOINT.as_bytes(),
                &timestamp.to_be_bytes()[..],
            )?;
        }
        write_txn.commit().context("Failed to commit checkpoint")?;

        tracing::info!(
            "Created checkpoint: {} states, {} bytes, {} peers",
            state_count,
            total_bytes,
            checkpoint.peer_ids.len()
        );

        Ok(checkpoint)
    }

    /// Get the last checkpoint
    pub fn get_last_checkpoint(&self) -> Result<Option<Checkpoint>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        // Get last checkpoint timestamp
        let meta_table = read_txn
            .open_table(META_TABLE)
            .context("Failed to open meta table")?;

        let timestamp_bytes = match meta_table.get(META_LAST_CHECKPOINT.as_bytes())? {
            Some(value) => value.value().to_vec(),
            None => return Ok(None),
        };

        if timestamp_bytes.len() != 8 {
            return Ok(None);
        }

        let mut arr = [0u8; 8];
        arr.copy_from_slice(&timestamp_bytes);
        let timestamp = u64::from_be_bytes(arr);

        // Load checkpoint
        let checkpoint_key = format!("{}{}", CHECKPOINT_PREFIX, timestamp);
        let checkpoint_table = read_txn
            .open_table(CHECKPOINT_TABLE)
            .context("Failed to open checkpoint table")?;

        match checkpoint_table.get(checkpoint_key.as_bytes())? {
            Some(value) => {
                let checkpoint: Checkpoint = serde_json::from_slice(value.value())?;
                Ok(Some(checkpoint))
            }
            None => Ok(None),
        }
    }

    /// Get persistence statistics
    pub fn stats(&self) -> Result<PersistenceStats> {
        let mut stats = PersistenceStats::default();
        let mut peer_ids = std::collections::HashSet::new();

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        // Count sync states
        {
            let table = read_txn
                .open_table(SYNC_STATE_TABLE)
                .context("Failed to open sync state table")?;

            for entry in table.range(SYNC_STATE_PREFIX.as_bytes()..)? {
                let (key, value) = entry?;
                let key_bytes = key.value();
                let key_str = String::from_utf8_lossy(key_bytes);

                if !key_str.starts_with(SYNC_STATE_PREFIX) {
                    break;
                }

                stats.state_count += 1;
                stats.total_bytes += value.value().len();

                if let Some(rest) = key_str.strip_prefix(SYNC_STATE_PREFIX) {
                    if let Some(peer_id) = rest.split(':').next() {
                        peer_ids.insert(peer_id.to_string());
                    }
                }
            }
        }

        stats.peer_count = peer_ids.len();

        // Count checkpoints
        {
            let checkpoint_table = read_txn
                .open_table(CHECKPOINT_TABLE)
                .context("Failed to open checkpoint table")?;

            for entry in checkpoint_table.range(CHECKPOINT_PREFIX.as_bytes()..)? {
                let (key, _) = entry?;
                if !key.value().starts_with(CHECKPOINT_PREFIX.as_bytes()) {
                    break;
                }
                stats.checkpoint_count += 1;
            }
        }

        // Get last checkpoint timestamp
        if let Ok(Some(checkpoint)) = self.get_last_checkpoint() {
            stats.last_checkpoint = Some(checkpoint.timestamp);
        }

        Ok(stats)
    }

    /// Clean up old checkpoints, keeping only the last N
    pub fn cleanup_old_checkpoints(&self, keep_count: usize) -> Result<usize> {
        let mut checkpoints: Vec<u64> = Vec::new();

        {
            let read_txn = self
                .db
                .begin_read()
                .context("Failed to begin read transaction")?;
            let table = read_txn
                .open_table(CHECKPOINT_TABLE)
                .context("Failed to open checkpoint table")?;

            for entry in table.range(CHECKPOINT_PREFIX.as_bytes()..)? {
                let (key, _) = entry?;
                let key_bytes = key.value();
                let key_str = String::from_utf8_lossy(key_bytes);

                if !key_str.starts_with(CHECKPOINT_PREFIX) {
                    break;
                }

                if let Some(ts_str) = key_str.strip_prefix(CHECKPOINT_PREFIX) {
                    if let Ok(ts) = ts_str.parse::<u64>() {
                        checkpoints.push(ts);
                    }
                }
            }
        }

        // Sort descending (newest first)
        checkpoints.sort_by(|a, b| b.cmp(a));

        // Delete old ones
        let mut deleted = 0;
        let to_delete: Vec<_> = checkpoints.iter().skip(keep_count).cloned().collect();

        if !to_delete.is_empty() {
            let write_txn = self
                .db
                .begin_write()
                .context("Failed to begin write transaction")?;
            {
                let mut table = write_txn
                    .open_table(CHECKPOINT_TABLE)
                    .context("Failed to open checkpoint table")?;

                for ts in to_delete {
                    let key = format!("{}{}", CHECKPOINT_PREFIX, ts);
                    table.remove(key.as_bytes())?;
                    deleted += 1;
                }
            }
            write_txn.commit().context("Failed to commit cleanup")?;
        }

        if deleted > 0 {
            tracing::info!("Cleaned up {} old checkpoints", deleted);
        }

        Ok(deleted)
    }

    /// Delete all sync state for a peer (when peer is removed from mesh)
    pub fn delete_peer(&self, peer_id: &EndpointId) -> Result<usize> {
        let prefix = format!("{}{}:", SYNC_STATE_PREFIX, hex::encode(peer_id.as_bytes()));
        let mut keys_to_delete = Vec::new();

        // First, collect keys to delete
        {
            let read_txn = self
                .db
                .begin_read()
                .context("Failed to begin read transaction")?;
            let table = read_txn
                .open_table(SYNC_STATE_TABLE)
                .context("Failed to open sync state table")?;

            for entry in table.range(prefix.as_bytes()..)? {
                let (key, _) = entry?;
                let key_bytes = key.value();
                if !key_bytes.starts_with(prefix.as_bytes()) {
                    break;
                }
                keys_to_delete.push(key_bytes.to_vec());
            }
        }

        // Then delete them
        let deleted = keys_to_delete.len();
        if !keys_to_delete.is_empty() {
            let write_txn = self
                .db
                .begin_write()
                .context("Failed to begin write transaction")?;
            {
                let mut table = write_txn
                    .open_table(SYNC_STATE_TABLE)
                    .context("Failed to open sync state table")?;

                for key in keys_to_delete {
                    table.remove(key.as_slice())?;
                }
            }
            write_txn.commit().context("Failed to commit delete")?;
        }

        if deleted > 0 {
            tracing::info!(
                "Deleted {} sync states for peer {}",
                deleted,
                hex::encode(peer_id.as_bytes())
            );
        }

        Ok(deleted)
    }

    /// Get checkpoint interval
    pub fn checkpoint_interval(&self) -> Duration {
        self.checkpoint_interval
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_persistence() -> (SyncStatePersistence, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let persistence = SyncStatePersistence::open(temp_dir.path()).unwrap();
        (persistence, temp_dir)
    }

    fn create_test_peer_id() -> EndpointId {
        use iroh::SecretKey;
        let mut rng = rand::rng();
        SecretKey::generate(&mut rng).public()
    }

    #[test]
    fn test_save_and_load_sync_state() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();
        let state = SyncState::new();

        // Save
        persistence
            .save_sync_state(&peer_id, "doc1", &state, 5)
            .unwrap();

        // Load
        let (loaded_state, sync_count) = persistence
            .load_sync_state(&peer_id, "doc1")
            .unwrap()
            .expect("State should exist");

        assert_eq!(sync_count, 5);
        // Sync states should be functionally equivalent (both empty/initial)
        assert_eq!(loaded_state.encode(), state.encode());
    }

    #[test]
    fn test_load_nonexistent_state() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();

        let result = persistence
            .load_sync_state(&peer_id, "nonexistent")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_sync_state() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();
        let state = SyncState::new();

        persistence
            .save_sync_state(&peer_id, "doc1", &state, 1)
            .unwrap();
        assert!(persistence
            .load_sync_state(&peer_id, "doc1")
            .unwrap()
            .is_some());

        persistence.delete_sync_state(&peer_id, "doc1").unwrap();
        assert!(persistence
            .load_sync_state(&peer_id, "doc1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_load_all_for_peer() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();
        let peer_id2 = create_test_peer_id();
        let state = SyncState::new();

        // Save states for peer 1
        persistence
            .save_sync_state(&peer_id, "doc1", &state, 1)
            .unwrap();
        persistence
            .save_sync_state(&peer_id, "doc2", &state, 2)
            .unwrap();

        // Save state for peer 2
        persistence
            .save_sync_state(&peer_id2, "doc1", &state, 3)
            .unwrap();

        // Load all for peer 1
        let peer1_states = persistence.load_all_for_peer(&peer_id).unwrap();
        assert_eq!(peer1_states.len(), 2);
        assert!(peer1_states.contains_key("doc1"));
        assert!(peer1_states.contains_key("doc2"));

        // Load all for peer 2
        let peer2_states = persistence.load_all_for_peer(&peer_id2).unwrap();
        assert_eq!(peer2_states.len(), 1);
    }

    #[test]
    fn test_load_all() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id1 = create_test_peer_id();
        let peer_id2 = create_test_peer_id();
        let state = SyncState::new();

        persistence
            .save_sync_state(&peer_id1, "doc1", &state, 1)
            .unwrap();
        persistence
            .save_sync_state(&peer_id2, "doc2", &state, 2)
            .unwrap();

        let all_states = persistence.load_all().unwrap();
        assert_eq!(all_states.len(), 2);
    }

    #[test]
    fn test_checkpoint() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();
        let state = SyncState::new();

        // Save some states
        persistence
            .save_sync_state(&peer_id, "doc1", &state, 1)
            .unwrap();
        persistence
            .save_sync_state(&peer_id, "doc2", &state, 2)
            .unwrap();

        // Create checkpoint
        let checkpoint = persistence.create_checkpoint().unwrap();
        assert_eq!(checkpoint.state_count, 2);
        assert_eq!(checkpoint.peer_ids.len(), 1);

        // Get last checkpoint
        let loaded = persistence.get_last_checkpoint().unwrap().unwrap();
        assert_eq!(loaded.timestamp, checkpoint.timestamp);
        assert_eq!(loaded.state_count, 2);
    }

    #[test]
    fn test_stats() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id1 = create_test_peer_id();
        let peer_id2 = create_test_peer_id();
        let state = SyncState::new();

        persistence
            .save_sync_state(&peer_id1, "doc1", &state, 1)
            .unwrap();
        persistence
            .save_sync_state(&peer_id2, "doc2", &state, 2)
            .unwrap();

        let stats = persistence.stats().unwrap();
        assert_eq!(stats.state_count, 2);
        assert_eq!(stats.peer_count, 2);
        assert!(stats.total_bytes > 0);
    }

    #[test]
    fn test_cleanup_old_checkpoints() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id = create_test_peer_id();
        let state = SyncState::new();

        persistence
            .save_sync_state(&peer_id, "doc1", &state, 1)
            .unwrap();

        // Create multiple checkpoints
        for _ in 0..5 {
            persistence.create_checkpoint().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let stats_before = persistence.stats().unwrap();
        assert_eq!(stats_before.checkpoint_count, 5);

        // Keep only 2
        let deleted = persistence.cleanup_old_checkpoints(2).unwrap();
        assert_eq!(deleted, 3);

        let stats_after = persistence.stats().unwrap();
        assert_eq!(stats_after.checkpoint_count, 2);
    }

    #[test]
    fn test_delete_peer() {
        let (persistence, _temp) = create_test_persistence();
        let peer_id1 = create_test_peer_id();
        let peer_id2 = create_test_peer_id();
        let state = SyncState::new();

        // Save states for both peers
        persistence
            .save_sync_state(&peer_id1, "doc1", &state, 1)
            .unwrap();
        persistence
            .save_sync_state(&peer_id1, "doc2", &state, 2)
            .unwrap();
        persistence
            .save_sync_state(&peer_id2, "doc1", &state, 3)
            .unwrap();

        // Delete peer 1
        let deleted = persistence.delete_peer(&peer_id1).unwrap();
        assert_eq!(deleted, 2);

        // Verify peer 1 states are gone
        assert!(persistence
            .load_sync_state(&peer_id1, "doc1")
            .unwrap()
            .is_none());
        assert!(persistence
            .load_sync_state(&peer_id1, "doc2")
            .unwrap()
            .is_none());

        // Verify peer 2 state remains
        assert!(persistence
            .load_sync_state(&peer_id2, "doc1")
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_persisted_sync_state_roundtrip() {
        let peer_id = create_test_peer_id();
        let state = SyncState::new();

        let persisted = PersistedSyncState::from_sync_state(&state, &peer_id, "test_doc", 42);

        assert_eq!(persisted.doc_key, "test_doc");
        assert_eq!(persisted.sync_count, 42);
        assert!(!persisted.state_bytes.is_empty());

        let restored = persisted.to_sync_state().unwrap();
        assert_eq!(restored.encode(), state.encode());
    }
}
