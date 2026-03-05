use tracing::{debug, warn};

use crate::error::Result;
use crate::oci::client::RegistryClient;
use crate::types::DigestSet;

/// Walk tags → manifests → layers → referrers to build a complete DigestSet.
pub async fn enumerate_digests(
    client: &dyn RegistryClient,
    repositories: &[String],
) -> Result<DigestSet> {
    let mut set = DigestSet::default();

    for repo in repositories {
        debug!(repo, "enumerating digests");

        // List all tags
        let mut last: Option<String> = None;
        loop {
            let page = client.list_tags(repo, Some(100), last.as_deref()).await?;

            if page.tags.is_empty() {
                break;
            }

            for tag in &page.tags {
                // Pull manifest for each tag
                match client.pull_manifest(repo, tag).await {
                    Ok(info) => {
                        let tag_key = format!("{}:{}", repo, tag);
                        set.tags.insert(tag_key, info.digest.clone());
                        set.manifests.insert(info.digest.clone(), info.size);

                        // Add config blob
                        if let Some((config_digest, config_size)) = &info.config_digest {
                            set.blobs.insert(config_digest.clone(), *config_size);
                        }

                        // Add layer blobs
                        for (layer_digest, layer_size) in &info.layer_digests {
                            set.blobs.insert(layer_digest.clone(), *layer_size);
                        }

                        // Enumerate referrers for this manifest
                        match client.list_referrers(repo, &info.digest, None).await {
                            Ok(referrers) => {
                                if !referrers.is_empty() {
                                    let referrer_digests: Vec<String> =
                                        referrers.iter().map(|r| r.digest.clone()).collect();
                                    set.referrers
                                        .insert(info.digest.clone(), referrer_digests.clone());

                                    // Also add referrer manifests to the set
                                    for referrer in &referrers {
                                        set.manifests
                                            .insert(referrer.digest.clone(), referrer.size);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!(repo, digest = %info.digest, "referrers not available: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(repo, tag, "failed to pull manifest: {e}");
                    }
                }
            }

            last = page.tags.last().cloned();

            // If we got fewer than page_size, we're done
            if page.tags.len() < 100 {
                break;
            }
        }
    }

    debug!(
        manifests = set.manifests.len(),
        blobs = set.blobs.len(),
        tags = set.tags.len(),
        referrers = set.referrers.len(),
        total_bytes = set.total_bytes(),
        "enumeration complete"
    );

    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_set_operations() {
        let mut set = DigestSet::default();
        assert_eq!(set.total_bytes(), 0);
        assert_eq!(set.total_items(), 0);

        set.manifests.insert("sha256:aaa".to_string(), 1024);
        set.blobs.insert("sha256:bbb".to_string(), 2048);
        set.blobs.insert("sha256:ccc".to_string(), 4096);
        set.tags
            .insert("myrepo:latest".to_string(), "sha256:aaa".to_string());

        assert_eq!(set.total_bytes(), 1024 + 2048 + 4096);
        assert_eq!(set.total_items(), 3);
    }

    #[test]
    fn test_digest_set_serialization_roundtrip() {
        let mut set = DigestSet::default();
        set.manifests.insert("sha256:abc123".to_string(), 512);
        set.blobs.insert("sha256:def456".to_string(), 1024);
        set.tags
            .insert("repo:v1".to_string(), "sha256:abc123".to_string());
        set.referrers.insert(
            "sha256:abc123".to_string(),
            vec!["sha256:sig001".to_string()],
        );

        let json = serde_json::to_string(&set).unwrap();
        let deserialized: DigestSet = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.manifests.len(), 1);
        assert_eq!(deserialized.blobs.len(), 1);
        assert_eq!(deserialized.tags.len(), 1);
        assert_eq!(deserialized.referrers.len(), 1);
    }
}
