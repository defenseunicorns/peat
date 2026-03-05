use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::TransferConfig;
use crate::error::{RegistryError, Result};
use crate::oci::client::RegistryClient;
use crate::transfer::checkpoint::{CheckpointStore, TransferCheckpoint};
use crate::types::DigestDelta;

/// Progress update emitted during transfer.
#[derive(Clone, Debug)]
pub struct TransferProgress {
    pub checkpoint_id: String,
    pub digest: String,
    pub bytes_transferred: u64,
    pub bytes_total: u64,
    pub item_type: TransferItemType,
}

#[derive(Clone, Debug)]
pub enum TransferItemType {
    Blob,
    Manifest,
}

/// Orchestrates blob/manifest transfer between source and target registries.
pub struct TransferEngine {
    source_client: Arc<dyn RegistryClient>,
    target_client: Arc<dyn RegistryClient>,
    checkpoint_store: Arc<CheckpointStore>,
    config: TransferConfig,
    progress_tx: broadcast::Sender<TransferProgress>,
}

impl TransferEngine {
    pub fn new(
        source_client: Arc<dyn RegistryClient>,
        target_client: Arc<dyn RegistryClient>,
        checkpoint_store: Arc<CheckpointStore>,
        config: TransferConfig,
    ) -> Self {
        let (progress_tx, _) = broadcast::channel(256);
        Self {
            source_client,
            target_client,
            checkpoint_store,
            config,
            progress_tx,
        }
    }

    /// Subscribe to transfer progress events.
    pub fn subscribe_progress(&self) -> broadcast::Receiver<TransferProgress> {
        self.progress_tx.subscribe()
    }

    /// Execute the transfer for a given delta, resuming from checkpoint if available.
    pub async fn execute(
        &self,
        intent_id: &str,
        source_id: &str,
        target_id: &str,
        delta: &DigestDelta,
        repositories: &[String],
    ) -> Result<TransferCheckpoint> {
        // Load or create checkpoint
        let mut checkpoint = self
            .checkpoint_store
            .find_for_transfer(intent_id, source_id, target_id)?
            .unwrap_or_else(|| {
                TransferCheckpoint::new(intent_id, source_id, target_id, delta.total_transfer_bytes)
            });

        info!(
            intent_id,
            source_id,
            target_id,
            missing_blobs = delta.missing_blobs.len(),
            missing_manifests = delta.missing_manifests.len(),
            total_bytes = delta.total_transfer_bytes,
            resumed_from = checkpoint.bytes_transferred,
            "starting transfer"
        );

        let repo = repositories
            .first()
            .map(|s| s.as_str())
            .unwrap_or("library");

        // Phase 1: Transfer blobs (skip completed, resume partial)
        for (digest, size) in &delta.missing_blobs {
            if checkpoint.completed_blobs.contains(digest) {
                debug!(digest, "blob already transferred, skipping");
                continue;
            }

            match self
                .transfer_blob(&mut checkpoint, repo, digest, *size)
                .await
            {
                Ok(()) => {
                    self.checkpoint_store.save(&checkpoint)?;
                }
                Err(e) => {
                    error!(digest, "blob transfer failed: {e}");
                    self.checkpoint_store.save(&checkpoint)?;
                    return Err(e);
                }
            }
        }

        // Phase 2: Transfer manifests (blobs must be present first)
        for (digest, size) in &delta.missing_manifests {
            if checkpoint.completed_manifests.contains(digest) {
                debug!(digest, "manifest already transferred, skipping");
                continue;
            }

            match self
                .transfer_manifest(&mut checkpoint, repo, digest, *size)
                .await
            {
                Ok(()) => {
                    self.checkpoint_store.save(&checkpoint)?;
                }
                Err(e) => {
                    error!(digest, "manifest transfer failed: {e}");
                    self.checkpoint_store.save(&checkpoint)?;
                    return Err(e);
                }
            }
        }

        // Phase 3: Publish tags (atomic visibility switch)
        for (tag_key, digest) in &delta.missing_tags {
            if let Some((repo_name, tag)) = tag_key.split_once(':') {
                // Pull the manifest content from source to push with tag
                match self.source_client.pull_manifest(repo_name, digest).await {
                    Ok(info) => {
                        if let Err(e) = self
                            .target_client
                            .push_manifest(repo_name, tag, info.content, &info.media_type)
                            .await
                        {
                            warn!(tag_key, "tag publish failed: {e}");
                        } else {
                            debug!(tag_key, digest, "tag published");
                        }
                    }
                    Err(e) => {
                        warn!(tag_key, "failed to pull manifest for tag: {e}");
                    }
                }
            }
        }

        info!(
            checkpoint_id = %checkpoint.checkpoint_id,
            bytes_transferred = checkpoint.bytes_transferred,
            "transfer complete"
        );

        Ok(checkpoint)
    }

    async fn transfer_blob(
        &self,
        checkpoint: &mut TransferCheckpoint,
        repo: &str,
        digest: &str,
        size: u64,
    ) -> Result<()> {
        let mut retries = 0;

        // Check for partial resume
        let start_offset = checkpoint
            .partial_blob
            .as_ref()
            .filter(|p| p.digest == digest)
            .map(|p| p.offset)
            .unwrap_or(0);

        loop {
            let result = if start_offset > 0 && retries == 0 {
                debug!(digest, offset = start_offset, "resuming partial blob");
                self.source_client
                    .pull_blob_range(repo, digest, start_offset, None)
                    .await
            } else {
                // Full pull
                self.source_client
                    .pull_blob(repo, digest)
                    .await
                    .map(|data| (data, size))
            };

            match result {
                Ok((data, _total_size)) => {
                    self.target_client.push_blob(repo, data, digest).await?;

                    checkpoint.mark_blob_completed(digest, size);

                    let _ = self.progress_tx.send(TransferProgress {
                        checkpoint_id: checkpoint.checkpoint_id.clone(),
                        digest: digest.to_string(),
                        bytes_transferred: checkpoint.bytes_transferred,
                        bytes_total: checkpoint.bytes_total,
                        item_type: TransferItemType::Blob,
                    });

                    return Ok(());
                }
                Err(e) => {
                    retries += 1;
                    if retries > self.config.max_retries {
                        return Err(RegistryError::Transfer {
                            digest: digest.to_string(),
                            reason: format!(
                                "max retries ({}) exceeded: {e}",
                                self.config.max_retries
                            ),
                        });
                    }
                    warn!(digest, retries, "blob transfer failed, retrying: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(
                        self.config.retry_backoff_ms * retries as u64,
                    ))
                    .await;
                }
            }
        }
    }

    async fn transfer_manifest(
        &self,
        checkpoint: &mut TransferCheckpoint,
        repo: &str,
        digest: &str,
        size: u64,
    ) -> Result<()> {
        let info = self.source_client.pull_manifest(repo, digest).await?;
        self.target_client
            .push_manifest(repo, digest, info.content, &info.media_type)
            .await?;

        checkpoint.mark_manifest_completed(digest, size);

        let _ = self.progress_tx.send(TransferProgress {
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            digest: digest.to_string(),
            bytes_transferred: checkpoint.bytes_transferred,
            bytes_total: checkpoint.bytes_total,
            item_type: TransferItemType::Manifest,
        });

        Ok(())
    }
}
