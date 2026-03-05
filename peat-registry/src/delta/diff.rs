use tracing::{debug, instrument};

use crate::error::Result;
use crate::oci::client::RegistryClient;
use crate::types::{DigestDelta, DigestSet};

/// Compute the delta between a source DigestSet and a live target registry.
///
/// Performs HEAD requests against the target for each source blob/manifest
/// to determine what's missing.
#[instrument(skip(source_set, target_client))]
pub async fn compute_delta(
    source_set: &DigestSet,
    target_client: &dyn RegistryClient,
    repos: &[String],
) -> Result<DigestDelta> {
    let mut delta = DigestDelta::default();

    // Check manifests
    for (digest, size) in &source_set.manifests {
        let mut found = false;
        for repo in repos {
            match target_client.blob_exists(repo, digest).await {
                Ok(true) => {
                    found = true;
                    break;
                }
                Ok(false) => {}
                Err(e) => {
                    debug!(repo, digest, "manifest existence check failed: {e}");
                }
            }
        }
        if !found {
            delta.missing_manifests.insert(digest.clone(), *size);
            delta.total_transfer_bytes += size;
        }
    }

    // Check blobs
    for (digest, size) in &source_set.blobs {
        let mut found = false;
        for repo in repos {
            match target_client.blob_exists(repo, digest).await {
                Ok(true) => {
                    found = true;
                    break;
                }
                Ok(false) => {}
                Err(e) => {
                    debug!(repo, digest, "blob existence check failed: {e}");
                }
            }
        }
        if !found {
            delta.missing_blobs.insert(digest.clone(), *size);
            delta.total_transfer_bytes += size;
        }
    }

    // Check tags
    for (tag_key, digest) in &source_set.tags {
        // tag_key format: "repo:tag"
        if let Some((repo, tag)) = tag_key.split_once(':') {
            match target_client.manifest_digest(repo, tag).await {
                Ok(target_digest) if target_digest == *digest => {}
                _ => {
                    delta.missing_tags.insert(tag_key.clone(), digest.clone());
                }
            }
        }
    }

    // Check referrers
    for (subject, referrer_digests) in &source_set.referrers {
        let mut missing = Vec::new();
        for referrer_digest in referrer_digests {
            let mut found = false;
            for repo in repos {
                if let Ok(true) = target_client.blob_exists(repo, referrer_digest).await {
                    found = true;
                    break;
                }
            }
            if !found {
                missing.push(referrer_digest.clone());
            }
        }
        if !missing.is_empty() {
            delta.missing_referrers.insert(subject.clone(), missing);
        }
    }

    debug!(
        missing_manifests = delta.missing_manifests.len(),
        missing_blobs = delta.missing_blobs.len(),
        missing_tags = delta.missing_tags.len(),
        total_transfer_bytes = delta.total_transfer_bytes,
        "delta computed"
    );

    Ok(delta)
}

/// Compute delta by comparing two pre-enumerated DigestSets (offline).
pub fn compute_delta_from_sets(source: &DigestSet, target: &DigestSet) -> DigestDelta {
    let mut delta = DigestDelta::default();

    for (digest, size) in &source.manifests {
        if !target.manifests.contains_key(digest) {
            delta.missing_manifests.insert(digest.clone(), *size);
            delta.total_transfer_bytes += size;
        }
    }

    for (digest, size) in &source.blobs {
        if !target.blobs.contains_key(digest) {
            delta.missing_blobs.insert(digest.clone(), *size);
            delta.total_transfer_bytes += size;
        }
    }

    for (tag_key, digest) in &source.tags {
        match target.tags.get(tag_key) {
            Some(target_digest) if target_digest == digest => {}
            _ => {
                delta.missing_tags.insert(tag_key.clone(), digest.clone());
            }
        }
    }

    for (subject, referrer_digests) in &source.referrers {
        let target_referrers = target.referrers.get(subject);
        let missing: Vec<String> = referrer_digests
            .iter()
            .filter(|d| target_referrers.map(|tr| !tr.contains(d)).unwrap_or(true))
            .cloned()
            .collect();
        if !missing.is_empty() {
            delta.missing_referrers.insert(subject.clone(), missing);
        }
    }

    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_compute_delta_from_sets_empty() {
        let source = DigestSet::default();
        let target = DigestSet::default();
        let delta = compute_delta_from_sets(&source, &target);
        assert!(delta.is_empty());
        assert_eq!(delta.total_transfer_bytes, 0);
        assert_eq!(delta.total_items(), 0);
    }

    #[test]
    fn test_compute_delta_from_sets_all_missing() {
        let mut source = DigestSet::default();
        source.manifests.insert("sha256:m1".to_string(), 1000);
        source.blobs.insert("sha256:b1".to_string(), 5000);
        source.blobs.insert("sha256:b2".to_string(), 3000);
        source
            .tags
            .insert("repo:v1".to_string(), "sha256:m1".to_string());

        let target = DigestSet::default();
        let delta = compute_delta_from_sets(&source, &target);

        assert_eq!(delta.missing_manifests.len(), 1);
        assert_eq!(delta.missing_blobs.len(), 2);
        assert_eq!(delta.missing_tags.len(), 1);
        assert_eq!(delta.total_transfer_bytes, 9000);
        assert_eq!(delta.total_items(), 3);
        assert!(!delta.is_empty());
    }

    #[test]
    fn test_compute_delta_from_sets_partial_overlap() {
        let mut source = DigestSet::default();
        source.manifests.insert("sha256:m1".to_string(), 1000);
        source.blobs.insert("sha256:b1".to_string(), 5000);
        source.blobs.insert("sha256:b2".to_string(), 3000);

        let mut target = DigestSet::default();
        target.manifests.insert("sha256:m1".to_string(), 1000);
        target.blobs.insert("sha256:b1".to_string(), 5000);
        // b2 is missing

        let delta = compute_delta_from_sets(&source, &target);
        assert_eq!(delta.missing_manifests.len(), 0);
        assert_eq!(delta.missing_blobs.len(), 1);
        assert!(delta.missing_blobs.contains_key("sha256:b2"));
        assert_eq!(delta.total_transfer_bytes, 3000);
    }

    #[test]
    fn test_compute_delta_from_sets_tag_mismatch() {
        let mut source = DigestSet::default();
        source
            .tags
            .insert("repo:latest".to_string(), "sha256:new".to_string());

        let mut target = DigestSet::default();
        target
            .tags
            .insert("repo:latest".to_string(), "sha256:old".to_string());

        let delta = compute_delta_from_sets(&source, &target);
        assert_eq!(delta.missing_tags.len(), 1);
        assert_eq!(delta.missing_tags.get("repo:latest").unwrap(), "sha256:new");
    }

    #[test]
    fn test_compute_delta_from_sets_referrers() {
        let mut source = DigestSet::default();
        source.referrers.insert(
            "sha256:m1".to_string(),
            vec!["sha256:sig1".to_string(), "sha256:sig2".to_string()],
        );

        let mut target = DigestSet::default();
        target
            .referrers
            .insert("sha256:m1".to_string(), vec!["sha256:sig1".to_string()]);

        let delta = compute_delta_from_sets(&source, &target);
        assert_eq!(delta.missing_referrers.len(), 1);
        assert_eq!(
            delta.missing_referrers.get("sha256:m1").unwrap(),
            &vec!["sha256:sig2".to_string()]
        );
    }

    #[test]
    fn test_digest_delta_is_empty() {
        let delta = DigestDelta::default();
        assert!(delta.is_empty());

        let mut delta2 = DigestDelta::default();
        delta2.missing_tags.insert("a".into(), "b".into());
        assert!(!delta2.is_empty());
    }
}
