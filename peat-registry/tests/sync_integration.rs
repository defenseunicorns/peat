//! Integration tests for peat-registry sync pipeline.
//!
//! Uses a MockRegistryClient backed by in-memory HashMaps to test the full
//! sync pipeline without a real OCI registry.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;

use peat_registry::convergence::referrers::check_referrer_gates;
use peat_registry::convergence::ConvergenceTracker;
use peat_registry::delta::{compute_delta, enumerate_digests};
use peat_registry::error::{RegistryError, Result};
use peat_registry::oci::RegistryClient;
use peat_registry::scheduler::wave::WaveController;
use peat_registry::topology::selector::select_source;
use peat_registry::topology::RegistryGraph;
use peat_registry::transfer::checkpoint::CheckpointStore;
use peat_registry::transfer::engine::TransferEngine;
use peat_registry::types::*;

// ---------- Mock Registry Client ----------

#[derive(Clone)]
struct MockRegistryClient {
    /// digest → (data, media_type)
    blobs: Arc<RwLock<HashMap<String, Bytes>>>,
    /// digest → (content, media_type, layers, config)
    manifests: Arc<RwLock<HashMap<String, MockManifest>>>,
    /// repo → list of tags
    tags: Arc<RwLock<HashMap<String, Vec<(String, String)>>>>,
    /// subject_digest → list of referrers
    referrers: Arc<RwLock<HashMap<String, Vec<ReferrerInfo>>>>,
}

#[derive(Clone)]
struct MockManifest {
    content: Bytes,
    media_type: String,
    layers: Vec<(String, u64)>,
    config: Option<(String, u64)>,
}

impl MockRegistryClient {
    fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(HashMap::new())),
            tags: Arc::new(RwLock::new(HashMap::new())),
            referrers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn add_blob(&self, digest: &str, data: &[u8]) {
        self.blobs
            .write()
            .unwrap()
            .insert(digest.to_string(), Bytes::from(data.to_vec()));
    }

    fn add_manifest(
        &self,
        digest: &str,
        content: &[u8],
        layers: Vec<(&str, u64)>,
        config: Option<(&str, u64)>,
    ) {
        let manifest = MockManifest {
            content: Bytes::from(content.to_vec()),
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            layers: layers
                .into_iter()
                .map(|(d, s)| (d.to_string(), s))
                .collect(),
            config: config.map(|(d, s)| (d.to_string(), s)),
        };
        self.manifests
            .write()
            .unwrap()
            .insert(digest.to_string(), manifest);
    }

    fn add_tag(&self, repo: &str, tag: &str, digest: &str) {
        self.tags
            .write()
            .unwrap()
            .entry(repo.to_string())
            .or_default()
            .push((tag.to_string(), digest.to_string()));
    }

    fn add_referrer(&self, subject_digest: &str, referrer: ReferrerInfo) {
        self.referrers
            .write()
            .unwrap()
            .entry(subject_digest.to_string())
            .or_default()
            .push(referrer);
    }
}

#[async_trait]
impl RegistryClient for MockRegistryClient {
    async fn blob_exists(&self, _repo: &str, digest: &str) -> Result<bool> {
        Ok(self.blobs.read().unwrap().contains_key(digest)
            || self.manifests.read().unwrap().contains_key(digest))
    }

    async fn pull_blob(&self, _repo: &str, digest: &str) -> Result<Bytes> {
        self.blobs
            .read()
            .unwrap()
            .get(digest)
            .cloned()
            .ok_or_else(|| RegistryError::BlobNotFound {
                repository: "mock".into(),
                digest: digest.into(),
            })
    }

    async fn pull_blob_range(
        &self,
        _repo: &str,
        digest: &str,
        offset: u64,
        _len: Option<u64>,
    ) -> Result<(Bytes, u64)> {
        let data = self
            .blobs
            .read()
            .unwrap()
            .get(digest)
            .cloned()
            .ok_or_else(|| RegistryError::BlobNotFound {
                repository: "mock".into(),
                digest: digest.into(),
            })?;
        let total = data.len() as u64;
        let sliced = data.slice(offset as usize..);
        Ok((sliced, total))
    }

    async fn push_blob(&self, _repo: &str, data: Bytes, digest: &str) -> Result<String> {
        self.blobs.write().unwrap().insert(digest.to_string(), data);
        Ok(digest.to_string())
    }

    async fn mount_blob(
        &self,
        _target_repo: &str,
        _source_repo: &str,
        _digest: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn pull_manifest(&self, _repo: &str, reference: &str) -> Result<ManifestInfo> {
        // Try direct digest lookup first
        if let Some(manifest) = self.manifests.read().unwrap().get(reference) {
            return Ok(ManifestInfo {
                content: manifest.content.clone(),
                media_type: manifest.media_type.clone(),
                digest: reference.to_string(),
                size: manifest.content.len() as u64,
                layer_digests: manifest.layers.clone(),
                config_digest: manifest.config.clone(),
            });
        }

        // Try tag lookup
        let tags = self.tags.read().unwrap();
        for tag_list in tags.values() {
            for (tag, digest) in tag_list {
                if tag == reference {
                    let manifests = self.manifests.read().unwrap();
                    if let Some(manifest) = manifests.get(digest) {
                        return Ok(ManifestInfo {
                            content: manifest.content.clone(),
                            media_type: manifest.media_type.clone(),
                            digest: digest.clone(),
                            size: manifest.content.len() as u64,
                            layer_digests: manifest.layers.clone(),
                            config_digest: manifest.config.clone(),
                        });
                    }
                }
            }
        }

        Err(RegistryError::ManifestNotFound {
            repository: "mock".into(),
            reference: reference.into(),
        })
    }

    async fn push_manifest(
        &self,
        _repo: &str,
        reference: &str,
        content: Bytes,
        media_type: &str,
    ) -> Result<String> {
        let manifest = MockManifest {
            content,
            media_type: media_type.to_string(),
            layers: vec![],
            config: None,
        };
        self.manifests
            .write()
            .unwrap()
            .insert(reference.to_string(), manifest);
        Ok(reference.to_string())
    }

    async fn manifest_digest(&self, _repo: &str, reference: &str) -> Result<String> {
        let tags = self.tags.read().unwrap();
        for tag_list in tags.values() {
            for (tag, digest) in tag_list {
                if tag == reference {
                    return Ok(digest.clone());
                }
            }
        }
        // Check if reference is itself a digest
        if self.manifests.read().unwrap().contains_key(reference) {
            return Ok(reference.to_string());
        }
        Err(RegistryError::ManifestNotFound {
            repository: "mock".into(),
            reference: reference.into(),
        })
    }

    async fn list_tags(
        &self,
        repo: &str,
        _page_size: Option<usize>,
        _last: Option<&str>,
    ) -> Result<TagPage> {
        let tags = self.tags.read().unwrap();
        let tag_list = tags
            .get(repo)
            .map(|t| t.iter().map(|(tag, _)| tag.clone()).collect())
            .unwrap_or_default();
        Ok(TagPage { tags: tag_list })
    }

    async fn list_referrers(
        &self,
        _repo: &str,
        digest: &str,
        _artifact_type: Option<&str>,
    ) -> Result<Vec<ReferrerInfo>> {
        Ok(self
            .referrers
            .read()
            .unwrap()
            .get(digest)
            .cloned()
            .unwrap_or_default())
    }
}

// ---------- Integration Tests ----------

#[tokio::test]
async fn test_full_sync_empty_target() {
    // Source has 1 manifest with 2 layer blobs and a config blob
    let source = MockRegistryClient::new();
    source.add_blob("sha256:layer1", &[1; 1024]);
    source.add_blob("sha256:layer2", &[2; 2048]);
    source.add_blob("sha256:config1", &[3; 256]);
    source.add_manifest(
        "sha256:manifest1",
        b"{}",
        vec![("sha256:layer1", 1024), ("sha256:layer2", 2048)],
        Some(("sha256:config1", 256)),
    );
    source.add_tag("myrepo", "v1.0", "sha256:manifest1");

    // Target is empty
    let target = MockRegistryClient::new();

    // Enumerate and compute delta
    let source_set = enumerate_digests(&source, &["myrepo".to_string()])
        .await
        .unwrap();
    assert_eq!(source_set.manifests.len(), 1);
    assert_eq!(source_set.blobs.len(), 3); // 2 layers + 1 config
    assert_eq!(source_set.tags.len(), 1);

    let delta = compute_delta(&source_set, &target, &["myrepo".to_string()])
        .await
        .unwrap();
    assert_eq!(delta.missing_manifests.len(), 1);
    assert_eq!(delta.missing_blobs.len(), 3);
    assert!(!delta.is_empty());

    // Execute transfer
    let dir = tempfile::tempdir().unwrap();
    let checkpoint_store =
        Arc::new(CheckpointStore::open(&dir.path().join("checkpoints.redb")).unwrap());

    let engine = TransferEngine::new(
        Arc::new(source.clone()),
        Arc::new(target.clone()),
        checkpoint_store,
        peat_registry::config::TransferConfig::default(),
    );

    let checkpoint = engine
        .execute(
            "intent-1",
            "source",
            "target",
            &delta,
            &["myrepo".to_string()],
        )
        .await
        .unwrap();

    // Verify all content transferred
    assert!(target.blob_exists("myrepo", "sha256:layer1").await.unwrap());
    assert!(target.blob_exists("myrepo", "sha256:layer2").await.unwrap());
    assert!(target
        .blob_exists("myrepo", "sha256:config1")
        .await
        .unwrap());
    assert!(target
        .blob_exists("myrepo", "sha256:manifest1")
        .await
        .unwrap());
    assert_eq!(checkpoint.completed_blobs.len(), 3);
    assert_eq!(checkpoint.completed_manifests.len(), 1);
}

#[tokio::test]
async fn test_incremental_sync_partial_target() {
    let source = MockRegistryClient::new();
    source.add_blob("sha256:layer1", &[1; 1024]);
    source.add_blob("sha256:layer2", &[2; 2048]);
    source.add_blob("sha256:config1", &[3; 256]);
    source.add_manifest(
        "sha256:manifest1",
        b"{}",
        vec![("sha256:layer1", 1024), ("sha256:layer2", 2048)],
        Some(("sha256:config1", 256)),
    );
    source.add_tag("myrepo", "latest", "sha256:manifest1");

    // Target already has layer1 and config
    let target = MockRegistryClient::new();
    target.add_blob("sha256:layer1", &[1; 1024]);
    target.add_blob("sha256:config1", &[3; 256]);

    let source_set = enumerate_digests(&source, &["myrepo".to_string()])
        .await
        .unwrap();
    let delta = compute_delta(&source_set, &target, &["myrepo".to_string()])
        .await
        .unwrap();

    // Only layer2 and manifest should be missing
    assert_eq!(delta.missing_blobs.len(), 1);
    assert!(delta.missing_blobs.contains_key("sha256:layer2"));
    assert_eq!(delta.missing_manifests.len(), 1);

    // Transfer only the missing items
    let dir = tempfile::tempdir().unwrap();
    let checkpoint_store =
        Arc::new(CheckpointStore::open(&dir.path().join("checkpoints.redb")).unwrap());

    let engine = TransferEngine::new(
        Arc::new(source),
        Arc::new(target.clone()),
        checkpoint_store,
        peat_registry::config::TransferConfig::default(),
    );

    let checkpoint = engine
        .execute(
            "intent-1",
            "source",
            "target",
            &delta,
            &["myrepo".to_string()],
        )
        .await
        .unwrap();

    assert!(target.blob_exists("myrepo", "sha256:layer2").await.unwrap());
    assert_eq!(checkpoint.completed_blobs.len(), 1); // Only transferred layer2
}

#[tokio::test]
async fn test_resume_from_checkpoint() {
    let source = MockRegistryClient::new();
    source.add_blob("sha256:b1", &[1; 500]);
    source.add_blob("sha256:b2", &[2; 500]);
    source.add_blob("sha256:b3", &[3; 500]);
    source.add_manifest("sha256:m1", b"{}", vec![], None);

    let target = MockRegistryClient::new();

    let dir = tempfile::tempdir().unwrap();
    let checkpoint_store =
        Arc::new(CheckpointStore::open(&dir.path().join("checkpoints.redb")).unwrap());

    // Simulate a previous partial transfer
    let mut existing_cp = peat_registry::transfer::checkpoint::TransferCheckpoint::new(
        "intent-r", "source", "target", 1500,
    );
    existing_cp.mark_blob_completed("sha256:b1", 500);
    checkpoint_store.save(&existing_cp).unwrap();

    // Also push b1 to target to simulate it was actually transferred
    target.add_blob("sha256:b1", &[1; 500]);

    let mut delta = DigestDelta::default();
    delta.missing_blobs.insert("sha256:b1".into(), 500);
    delta.missing_blobs.insert("sha256:b2".into(), 500);
    delta.missing_blobs.insert("sha256:b3".into(), 500);
    delta.missing_manifests.insert("sha256:m1".into(), 2);
    delta.total_transfer_bytes = 1502;

    let engine = TransferEngine::new(
        Arc::new(source),
        Arc::new(target.clone()),
        checkpoint_store,
        peat_registry::config::TransferConfig::default(),
    );

    let checkpoint = engine
        .execute(
            "intent-r",
            "source",
            "target",
            &delta,
            &["myrepo".to_string()],
        )
        .await
        .unwrap();

    // b1 was already done, so only b2, b3, m1 should be transferred
    assert!(target.blob_exists("myrepo", "sha256:b2").await.unwrap());
    assert!(target.blob_exists("myrepo", "sha256:b3").await.unwrap());
    assert!(target.blob_exists("myrepo", "sha256:m1").await.unwrap());
    assert_eq!(checkpoint.completed_blobs.len(), 3);
    assert_eq!(checkpoint.completed_manifests.len(), 1);
}

#[tokio::test]
async fn test_referrer_gate_blocks_convergence() {
    let target = MockRegistryClient::new();
    target.add_blob("sha256:manifest1", &[1; 100]);

    // Add a signature referrer but no provenance
    target.add_referrer(
        "sha256:manifest1",
        ReferrerInfo {
            digest: "sha256:sig1".into(),
            artifact_type: "application/vnd.cncf.notary.signature".into(),
            size: 50,
        },
    );

    let required_types = vec![
        "application/vnd.cncf.notary.signature".to_string(),
        "application/vnd.in-toto.provenance+json".to_string(),
    ];

    let result = check_referrer_gates(&target, "myrepo", "sha256:manifest1", &required_types)
        .await
        .unwrap();

    assert!(!result.passed);
    assert_eq!(result.missing_types.len(), 1);
    assert!(result
        .missing_types
        .contains(&"application/vnd.in-toto.provenance+json".to_string()));

    // Now add the provenance referrer
    target.add_referrer(
        "sha256:manifest1",
        ReferrerInfo {
            digest: "sha256:prov1".into(),
            artifact_type: "application/vnd.in-toto.provenance+json".into(),
            size: 200,
        },
    );

    let result2 = check_referrer_gates(&target, "myrepo", "sha256:manifest1", &required_types)
        .await
        .unwrap();

    assert!(result2.passed);
    assert!(result2.missing_types.is_empty());
}

#[tokio::test]
async fn test_wave_control_blocks_premature_sync() {
    use peat_registry::config::EdgeConfig;

    let targets = vec![
        RegistryTarget {
            id: "enterprise".into(),
            endpoint: "https://enterprise.example.com".into(),
            tier: RegistryTier::Enterprise,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
        RegistryTarget {
            id: "regional".into(),
            endpoint: "https://regional.example.com".into(),
            tier: RegistryTier::Regional,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
        RegistryTarget {
            id: "tactical".into(),
            endpoint: "https://tactical.example.com".into(),
            tier: RegistryTier::Tactical,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
    ];

    let edges = vec![
        EdgeConfig {
            parent_id: "enterprise".into(),
            child_id: "regional".into(),
            preference: 1,
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        },
        EdgeConfig {
            parent_id: "regional".into(),
            child_id: "tactical".into(),
            preference: 1,
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        },
    ];

    let graph = RegistryGraph::new(targets, &edges).unwrap();
    let wave_ctrl = WaveController::new(0.8);
    let tracker = ConvergenceTracker::new();

    // Wave 0 (enterprise) is always active
    assert!(wave_ctrl.is_wave_active(0, &graph.wave_assignments, &HashMap::new()));

    // Wave 1 (regional) should NOT be active — enterprise isn't converged yet
    let states = tracker.get_states_for_intent("intent-1");
    assert!(!wave_ctrl.is_wave_active(1, &graph.wave_assignments, &states));

    // Mark enterprise as converged
    tracker.update_status(
        "intent-1",
        "enterprise",
        ConvergenceStatus::Converged,
        None,
        None,
    );
    let states = tracker.get_states_for_intent("intent-1");
    assert!(wave_ctrl.is_wave_active(1, &graph.wave_assignments, &states));

    // Wave 2 (tactical) still shouldn't be active — regional isn't converged
    assert!(!wave_ctrl.is_wave_active(2, &graph.wave_assignments, &states));

    // Mark regional as converged
    tracker.update_status(
        "intent-1",
        "regional",
        ConvergenceStatus::Converged,
        None,
        None,
    );
    let states = tracker.get_states_for_intent("intent-1");
    assert!(wave_ctrl.is_wave_active(2, &graph.wave_assignments, &states));
}

#[tokio::test]
async fn test_source_selection_prefers_converged_nearest() {
    use peat_registry::config::EdgeConfig;

    let targets = vec![
        RegistryTarget {
            id: "root".into(),
            endpoint: "https://root.example.com".into(),
            tier: RegistryTier::Enterprise,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
        RegistryTarget {
            id: "mid".into(),
            endpoint: "https://mid.example.com".into(),
            tier: RegistryTier::Regional,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
        RegistryTarget {
            id: "leaf".into(),
            endpoint: "https://leaf.example.com".into(),
            tier: RegistryTier::Tactical,
            auth: RegistryAuth::Anonymous,
            metadata: Default::default(),
        },
    ];

    let edges = vec![
        EdgeConfig {
            parent_id: "root".into(),
            child_id: "leaf".into(),
            preference: 2, // lower preference (fallback)
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        },
        EdgeConfig {
            parent_id: "mid".into(),
            child_id: "leaf".into(),
            preference: 1, // preferred
            max_fanout: None,
            bandwidth_budget_bytes_per_hour: None,
        },
    ];

    let graph = RegistryGraph::new(targets, &edges).unwrap();

    // Both parents converged — should pick mid (preference 1)
    let mut states = HashMap::new();
    states.insert(
        "root".to_string(),
        TargetConvergenceState {
            target_id: "root".into(),
            intent_id: "test".into(),
            status: ConvergenceStatus::Converged,
            remaining_delta: None,
            active_checkpoint: None,
            blockers: vec![],
            last_updated: Utc::now(),
        },
    );
    states.insert(
        "mid".to_string(),
        TargetConvergenceState {
            target_id: "mid".into(),
            intent_id: "test".into(),
            status: ConvergenceStatus::Converged,
            remaining_delta: None,
            active_checkpoint: None,
            blockers: vec![],
            last_updated: Utc::now(),
        },
    );

    let source = select_source(&graph, "leaf", &states);
    assert_eq!(source.as_deref(), Some("mid"));

    // Only root converged, mid pending — should pick root
    states.get_mut("mid").unwrap().status = ConvergenceStatus::Pending;
    let source = select_source(&graph, "leaf", &states);
    assert_eq!(source.as_deref(), Some("root"));
}
