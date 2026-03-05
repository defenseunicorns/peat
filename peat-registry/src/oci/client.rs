use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use oci_client::client::{BlobResponse, ClientConfig};
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use tracing::{debug, instrument};

use crate::error::{RegistryError, Result};
use crate::oci::auth;
use crate::types::{ManifestInfo, ReferrerInfo, RegistryTarget, TagPage};

/// Trait abstracting OCI registry operations for testability.
#[async_trait]
pub trait RegistryClient: Send + Sync {
    /// Check whether a blob exists in the registry.
    async fn blob_exists(&self, repo: &str, digest: &str) -> Result<bool>;

    /// Pull a complete blob by digest.
    async fn pull_blob(&self, repo: &str, digest: &str) -> Result<Bytes>;

    /// Pull a blob range starting at offset. Returns (data, total_size).
    async fn pull_blob_range(
        &self,
        repo: &str,
        digest: &str,
        offset: u64,
        len: Option<u64>,
    ) -> Result<(Bytes, u64)>;

    /// Push a blob and return the canonical digest.
    async fn push_blob(&self, repo: &str, data: Bytes, digest: &str) -> Result<String>;

    /// Mount a blob from source_repo into target_repo (cross-repo mount).
    async fn mount_blob(&self, target_repo: &str, source_repo: &str, digest: &str) -> Result<()>;

    /// Pull a manifest by reference (tag or digest). Returns parsed info.
    async fn pull_manifest(&self, repo: &str, reference: &str) -> Result<ManifestInfo>;

    /// Push a raw manifest and return the digest.
    async fn push_manifest(
        &self,
        repo: &str,
        reference: &str,
        content: Bytes,
        media_type: &str,
    ) -> Result<String>;

    /// Get the digest of a manifest by reference (HEAD request).
    async fn manifest_digest(&self, repo: &str, reference: &str) -> Result<String>;

    /// List tags with optional pagination.
    async fn list_tags(
        &self,
        repo: &str,
        page_size: Option<usize>,
        last: Option<&str>,
    ) -> Result<TagPage>;

    /// List referrers for a subject digest (OCI 1.1 Referrers API).
    async fn list_referrers(
        &self,
        repo: &str,
        digest: &str,
        artifact_type: Option<&str>,
    ) -> Result<Vec<ReferrerInfo>>;
}

/// OCI registry client wrapping `oci_client::Client`.
pub struct OciRegistryClient {
    client: oci_client::Client,
    target: RegistryTarget,
}

impl OciRegistryClient {
    pub fn new(target: RegistryTarget) -> Self {
        let protocol = if target.endpoint.starts_with("http://") {
            oci_client::client::ClientProtocol::Http
        } else {
            oci_client::client::ClientProtocol::Https
        };

        let config = ClientConfig {
            protocol,
            ..Default::default()
        };
        let client = oci_client::Client::new(config);
        Self { client, target }
    }

    fn oci_auth(&self) -> oci_client::secrets::RegistryAuth {
        auth::to_oci_auth(&self.target.auth)
    }

    fn reference(&self, repo: &str, reference: Option<&str>) -> Result<oci_client::Reference> {
        auth::parse_reference(&self.target.endpoint, repo, reference)
    }

    fn make_descriptor(digest: &str) -> OciDescriptor {
        OciDescriptor {
            digest: digest.to_string(),
            ..Default::default()
        }
    }
}

#[async_trait]
impl RegistryClient for OciRegistryClient {
    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn blob_exists(&self, repo: &str, digest: &str) -> Result<bool> {
        let reference = self.reference(repo, None)?;
        self.client
            .blob_exists(&reference, digest)
            .await
            .map_err(|e| RegistryError::Oci(format!("blob_exists failed: {e}")))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn pull_blob(&self, repo: &str, digest: &str) -> Result<Bytes> {
        let reference = self.reference(repo, None)?;
        let desc = Self::make_descriptor(digest);
        let mut buf = Vec::new();
        self.client
            .pull_blob(&reference, &desc, &mut buf)
            .await
            .map_err(|e| RegistryError::Oci(format!("pull_blob failed: {e}")))?;
        Ok(Bytes::from(buf))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn pull_blob_range(
        &self,
        repo: &str,
        digest: &str,
        offset: u64,
        len: Option<u64>,
    ) -> Result<(Bytes, u64)> {
        let reference = self.reference(repo, None)?;
        let desc = Self::make_descriptor(digest);

        let response = self
            .client
            .pull_blob_stream_partial(&reference, &desc, offset, len)
            .await
            .map_err(|e| RegistryError::Oci(format!("pull_blob_range failed: {e}")))?;

        // Extract the SizedStream from BlobResponse
        let sized_stream = match response {
            BlobResponse::Full(s) => s,
            BlobResponse::Partial(s) => s,
        };
        let total_size = sized_stream.content_length.unwrap_or(0);
        let mut data = Vec::new();
        let mut stream = sized_stream;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| RegistryError::Oci(format!("stream chunk error: {e}")))?;
            data.extend_from_slice(&chunk);
        }
        Ok((Bytes::from(data), total_size))
    }

    #[instrument(skip(self, data), fields(endpoint = %self.target.endpoint, size = data.len()))]
    async fn push_blob(&self, repo: &str, data: Bytes, digest: &str) -> Result<String> {
        let reference = self.reference(repo, None)?;
        debug!(repo, digest, "pushing blob");
        self.client
            .push_blob(&reference, data, digest)
            .await
            .map_err(|e| RegistryError::Oci(format!("push_blob failed: {e}")))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn mount_blob(&self, target_repo: &str, source_repo: &str, digest: &str) -> Result<()> {
        let target_ref = self.reference(target_repo, None)?;
        let source_ref = self.reference(source_repo, None)?;
        self.client
            .mount_blob(&target_ref, &source_ref, digest)
            .await
            .map_err(|e| RegistryError::Oci(format!("mount_blob failed: {e}")))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn pull_manifest(&self, repo: &str, reference: &str) -> Result<ManifestInfo> {
        let img_ref = self.reference(repo, Some(reference))?;
        let oci_auth = self.oci_auth();

        let (manifest_bytes, digest) = self
            .client
            .pull_manifest_raw(
                &img_ref,
                &oci_auth,
                &[
                    "application/vnd.oci.image.manifest.v1+json",
                    "application/vnd.docker.distribution.manifest.v2+json",
                    "application/vnd.oci.image.index.v1+json",
                ],
            )
            .await
            .map_err(|e| RegistryError::Oci(format!("pull_manifest failed: {e}")))?;

        // Parse to extract layer digests using oci-client's own types
        let size = manifest_bytes.len() as u64;
        let mut layer_digests = Vec::new();
        let mut config_digest = None;
        let media_type;

        if let Ok(manifest) = serde_json::from_slice::<OciImageManifest>(&manifest_bytes) {
            media_type = manifest
                .media_type
                .as_deref()
                .unwrap_or("application/vnd.oci.image.manifest.v1+json")
                .to_string();
            for layer in &manifest.layers {
                layer_digests.push((layer.digest.clone(), layer.size as u64));
            }
            config_digest = Some((manifest.config.digest.clone(), manifest.config.size as u64));
        } else {
            media_type = "application/vnd.oci.image.index.v1+json".to_string();
        }

        Ok(ManifestInfo {
            content: manifest_bytes,
            media_type,
            digest,
            size,
            layer_digests,
            config_digest,
        })
    }

    #[instrument(skip(self, content), fields(endpoint = %self.target.endpoint, size = content.len()))]
    async fn push_manifest(
        &self,
        repo: &str,
        reference: &str,
        content: Bytes,
        media_type: &str,
    ) -> Result<String> {
        let img_ref = self.reference(repo, Some(reference))?;
        let header_value = http::header::HeaderValue::from_str(media_type)
            .map_err(|e| RegistryError::Oci(format!("invalid media type: {e}")))?;
        self.client
            .push_manifest_raw(&img_ref, content, header_value)
            .await
            .map_err(|e| RegistryError::Oci(format!("push_manifest failed: {e}")))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn manifest_digest(&self, repo: &str, reference: &str) -> Result<String> {
        let img_ref = self.reference(repo, Some(reference))?;
        let oci_auth = self.oci_auth();
        self.client
            .fetch_manifest_digest(&img_ref, &oci_auth)
            .await
            .map_err(|e| RegistryError::Oci(format!("manifest_digest failed: {e}")))
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn list_tags(
        &self,
        repo: &str,
        page_size: Option<usize>,
        last: Option<&str>,
    ) -> Result<TagPage> {
        let img_ref = self.reference(repo, None)?;
        let oci_auth = self.oci_auth();
        let response = self
            .client
            .list_tags(&img_ref, &oci_auth, page_size, last)
            .await
            .map_err(|e| RegistryError::Oci(format!("list_tags failed: {e}")))?;
        Ok(TagPage {
            tags: response.tags,
        })
    }

    #[instrument(skip(self), fields(endpoint = %self.target.endpoint))]
    async fn list_referrers(
        &self,
        repo: &str,
        digest: &str,
        artifact_type: Option<&str>,
    ) -> Result<Vec<ReferrerInfo>> {
        let img_ref = self.reference(repo, Some(digest))?;
        let index = self
            .client
            .pull_referrers(&img_ref, artifact_type)
            .await
            .map_err(|e| RegistryError::Oci(format!("pull_referrers failed: {e}")))?;

        let referrers = index
            .manifests
            .iter()
            .map(|entry| ReferrerInfo {
                digest: entry.digest.clone(),
                artifact_type: entry
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get("org.opencontainers.image.title"))
                    .cloned()
                    .unwrap_or_default(),
                size: entry.size as u64,
            })
            .collect();

        Ok(referrers)
    }
}
