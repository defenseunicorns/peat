use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

use crate::error::{RegistryError, Result};

const CHECKPOINT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("checkpoints");

/// State of a partial blob transfer for resume.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialBlob {
    pub digest: String,
    pub repository: String,
    pub offset: u64,
    pub total_size: u64,
}

/// Persistent checkpoint for a transfer operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferCheckpoint {
    pub checkpoint_id: String,
    pub intent_id: String,
    pub source_id: String,
    pub target_id: String,
    pub completed_blobs: HashSet<String>,
    pub completed_manifests: HashSet<String>,
    pub partial_blob: Option<PartialBlob>,
    pub bytes_transferred: u64,
    pub bytes_total: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TransferCheckpoint {
    pub fn new(intent_id: &str, source_id: &str, target_id: &str, bytes_total: u64) -> Self {
        let now = Utc::now();
        Self {
            checkpoint_id: uuid::Uuid::new_v4().to_string(),
            intent_id: intent_id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            completed_blobs: HashSet::new(),
            completed_manifests: HashSet::new(),
            partial_blob: None,
            bytes_transferred: 0,
            bytes_total,
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark a blob as completed and accumulate bytes.
    pub fn mark_blob_completed(&mut self, digest: &str, size: u64) {
        self.completed_blobs.insert(digest.to_string());
        self.bytes_transferred += size;
        self.partial_blob = None;
        self.updated_at = Utc::now();
    }

    /// Mark a manifest as completed and accumulate bytes.
    pub fn mark_manifest_completed(&mut self, digest: &str, size: u64) {
        self.completed_manifests.insert(digest.to_string());
        self.bytes_transferred += size;
        self.updated_at = Utc::now();
    }

    /// Update partial blob progress (for resume).
    pub fn update_partial_blob(&mut self, partial: PartialBlob) {
        self.partial_blob = Some(partial);
        self.updated_at = Utc::now();
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.bytes_total == 0 {
            return 1.0;
        }
        self.bytes_transferred as f64 / self.bytes_total as f64
    }
}

/// Persistent checkpoint store backed by redb.
pub struct CheckpointStore {
    db: Arc<Database>,
}

impl CheckpointStore {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path)
            .map_err(|e| RegistryError::Checkpoint(format!("Failed to open checkpoint db: {e}")))?;

        // Ensure table exists
        let write_txn = db
            .begin_write()
            .map_err(|e| RegistryError::Checkpoint(format!("begin_write: {e}")))?;
        {
            let _table = write_txn
                .open_table(CHECKPOINT_TABLE)
                .map_err(|e| RegistryError::Checkpoint(format!("open_table: {e}")))?;
        }
        write_txn
            .commit()
            .map_err(|e| RegistryError::Checkpoint(format!("commit: {e}")))?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Save or update a checkpoint.
    pub fn save(&self, checkpoint: &TransferCheckpoint) -> Result<()> {
        let data = serde_json::to_vec(checkpoint)?;
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| RegistryError::Checkpoint(format!("begin_write: {e}")))?;
        {
            let mut table = write_txn
                .open_table(CHECKPOINT_TABLE)
                .map_err(|e| RegistryError::Checkpoint(format!("open_table: {e}")))?;
            table
                .insert(checkpoint.checkpoint_id.as_str(), data.as_slice())
                .map_err(|e| RegistryError::Checkpoint(format!("insert: {e}")))?;
        }
        write_txn
            .commit()
            .map_err(|e| RegistryError::Checkpoint(format!("commit: {e}")))?;

        debug!(
            checkpoint_id = %checkpoint.checkpoint_id,
            progress = checkpoint.progress_fraction(),
            "checkpoint saved"
        );
        Ok(())
    }

    /// Load a checkpoint by ID.
    pub fn load(&self, checkpoint_id: &str) -> Result<Option<TransferCheckpoint>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| RegistryError::Checkpoint(format!("begin_read: {e}")))?;
        let table = read_txn
            .open_table(CHECKPOINT_TABLE)
            .map_err(|e| RegistryError::Checkpoint(format!("open_table: {e}")))?;

        match table.get(checkpoint_id) {
            Ok(Some(data)) => {
                let checkpoint: TransferCheckpoint = serde_json::from_slice(data.value())?;
                Ok(Some(checkpoint))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(RegistryError::Checkpoint(format!("get: {e}"))),
        }
    }

    /// Delete a checkpoint.
    pub fn delete(&self, checkpoint_id: &str) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| RegistryError::Checkpoint(format!("begin_write: {e}")))?;
        {
            let mut table = write_txn
                .open_table(CHECKPOINT_TABLE)
                .map_err(|e| RegistryError::Checkpoint(format!("open_table: {e}")))?;
            table
                .remove(checkpoint_id)
                .map_err(|e| RegistryError::Checkpoint(format!("remove: {e}")))?;
        }
        write_txn
            .commit()
            .map_err(|e| RegistryError::Checkpoint(format!("commit: {e}")))?;
        Ok(())
    }

    /// List all active checkpoints.
    pub fn list_active(&self) -> Result<Vec<TransferCheckpoint>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| RegistryError::Checkpoint(format!("begin_read: {e}")))?;
        let table = read_txn
            .open_table(CHECKPOINT_TABLE)
            .map_err(|e| RegistryError::Checkpoint(format!("open_table: {e}")))?;

        let mut checkpoints = Vec::new();
        let iter = table
            .iter()
            .map_err(|e| RegistryError::Checkpoint(format!("iter: {e}")))?;

        for entry in iter {
            let (_, value) = entry.map_err(|e| RegistryError::Checkpoint(format!("entry: {e}")))?;
            let checkpoint: TransferCheckpoint = serde_json::from_slice(value.value())?;
            checkpoints.push(checkpoint);
        }

        Ok(checkpoints)
    }

    /// Find a checkpoint for a specific intent + source + target combination.
    pub fn find_for_transfer(
        &self,
        intent_id: &str,
        source_id: &str,
        target_id: &str,
    ) -> Result<Option<TransferCheckpoint>> {
        let all = self.list_active()?;
        Ok(all.into_iter().find(|cp| {
            cp.intent_id == intent_id && cp.source_id == source_id && cp.target_id == target_id
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_save_load_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("checkpoints.redb");
        let store = CheckpointStore::open(&db_path).unwrap();

        let mut cp = TransferCheckpoint::new("intent-1", "source-1", "target-1", 10000);
        let cp_id = cp.checkpoint_id.clone();

        store.save(&cp).unwrap();

        let loaded = store.load(&cp_id).unwrap().unwrap();
        assert_eq!(loaded.intent_id, "intent-1");
        assert_eq!(loaded.bytes_total, 10000);
        assert_eq!(loaded.bytes_transferred, 0);

        // Update with completed blob
        cp.mark_blob_completed("sha256:abc", 5000);
        store.save(&cp).unwrap();

        let loaded2 = store.load(&cp_id).unwrap().unwrap();
        assert_eq!(loaded2.bytes_transferred, 5000);
        assert!(loaded2.completed_blobs.contains("sha256:abc"));
    }

    #[test]
    fn test_checkpoint_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("checkpoints.redb");
        let store = CheckpointStore::open(&db_path).unwrap();

        let cp = TransferCheckpoint::new("intent-1", "source-1", "target-1", 5000);
        let cp_id = cp.checkpoint_id.clone();

        store.save(&cp).unwrap();
        assert!(store.load(&cp_id).unwrap().is_some());

        store.delete(&cp_id).unwrap();
        assert!(store.load(&cp_id).unwrap().is_none());
    }

    #[test]
    fn test_checkpoint_list_active() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("checkpoints.redb");
        let store = CheckpointStore::open(&db_path).unwrap();

        let cp1 = TransferCheckpoint::new("intent-1", "src", "tgt-a", 1000);
        let cp2 = TransferCheckpoint::new("intent-1", "src", "tgt-b", 2000);

        store.save(&cp1).unwrap();
        store.save(&cp2).unwrap();

        let active = store.list_active().unwrap();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_checkpoint_find_for_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("checkpoints.redb");
        let store = CheckpointStore::open(&db_path).unwrap();

        let cp = TransferCheckpoint::new("intent-x", "src-a", "tgt-b", 1000);
        store.save(&cp).unwrap();

        let found = store
            .find_for_transfer("intent-x", "src-a", "tgt-b")
            .unwrap();
        assert!(found.is_some());

        let not_found = store
            .find_for_transfer("intent-x", "src-a", "tgt-c")
            .unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_checkpoint_partial_blob() {
        let mut cp = TransferCheckpoint::new("i", "s", "t", 10000);
        assert!(cp.partial_blob.is_none());

        cp.update_partial_blob(PartialBlob {
            digest: "sha256:big".to_string(),
            repository: "myrepo".to_string(),
            offset: 4096,
            total_size: 8192,
        });
        assert_eq!(cp.partial_blob.as_ref().unwrap().offset, 4096);

        cp.mark_blob_completed("sha256:big", 8192);
        assert!(cp.partial_blob.is_none());
        assert_eq!(cp.bytes_transferred, 8192);
    }

    #[test]
    fn test_checkpoint_progress_fraction() {
        let mut cp = TransferCheckpoint::new("i", "s", "t", 1000);
        assert!((cp.progress_fraction() - 0.0).abs() < f64::EPSILON);

        cp.mark_blob_completed("sha256:a", 500);
        assert!((cp.progress_fraction() - 0.5).abs() < f64::EPSILON);

        cp.mark_blob_completed("sha256:b", 500);
        assert!((cp.progress_fraction() - 1.0).abs() < f64::EPSILON);

        // Edge case: zero total
        let cp_zero = TransferCheckpoint::new("i", "s", "t", 0);
        assert!((cp_zero.progress_fraction() - 1.0).abs() < f64::EPSILON);
    }
}
