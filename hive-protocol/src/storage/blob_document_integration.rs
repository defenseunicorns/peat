//! Document-Blob Integration (ADR-025 Phase 2)
//!
//! This module connects blob storage with CRDT document sync, enabling:
//! - Store blob tokens in documents for mesh synchronization
//! - Retrieve blob tokens from synced documents
//! - Automatic blob fetching when documents sync
//!
//! # How It Works
//!
//! 1. **Create Blob**: Use `BlobStore::create_blob()` to add content
//! 2. **Store Reference**: Use `store_blob_reference()` to put token in document
//! 3. **Sync**: Document syncs to peers via CRDT (Ditto/Automerge)
//! 4. **Fetch**: Peer retrieves token, uses `fetch_blob()` to download content
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::storage::{
//!     BlobDocumentIntegration, BlobStore, BlobMetadata, DittoBlobStore
//! };
//!
//! // Create blob
//! let blob_store = DittoBlobStore::new(ditto_store);
//! let token = blob_store.create_blob(
//!     Path::new("/models/target_recognition.onnx"),
//!     BlobMetadata::with_name("target_recognition.onnx"),
//! ).await?;
//!
//! // Store in model registry document
//! let integration = DittoBlobDocumentIntegration::new(ditto_store, blob_store);
//! integration.store_blob_reference(
//!     "model_registry",
//!     "target_recognition:4.2.1",
//!     "model_blob",
//!     &token,
//! ).await?;
//!
//! // On another node after sync...
//! if let Some(token) = integration.get_blob_reference(
//!     "model_registry",
//!     "target_recognition:4.2.1",
//!     "model_blob",
//! ).await? {
//!     let handle = blob_store.fetch_blob(&token, |p| println!("{:?}", p)).await?;
//!     println!("Model available at: {}", handle.path().display());
//! }
//! ```

#[cfg(feature = "ditto-backend")]
use super::blob_traits::BlobStore;
use super::blob_traits::{BlobHash, BlobMetadata, BlobProgress, BlobToken};
#[cfg(feature = "ditto-backend")]
use super::ditto_store::DittoStore;
#[cfg(feature = "ditto-backend")]
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[cfg(feature = "ditto-backend")]
use serde_json::json;
use std::collections::HashMap;
#[cfg(feature = "ditto-backend")]
use std::sync::Arc;
#[cfg(feature = "ditto-backend")]
use tracing::{debug, info, warn};

/// Serializable blob reference for storage in documents
///
/// This struct is stored as a JSON field in CRDT documents.
/// When the document syncs, the blob reference syncs too.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobReference {
    /// Content hash (content-addressed ID)
    pub hash: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Blob metadata
    pub metadata: BlobReferenceMetadata,
}

/// Metadata within a blob reference
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BlobReferenceMetadata {
    /// Human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Custom key-value pairs
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, String>,
}

impl From<&BlobToken> for BlobReference {
    fn from(token: &BlobToken) -> Self {
        Self {
            hash: token.hash.as_hex().to_string(),
            size_bytes: token.size_bytes,
            metadata: BlobReferenceMetadata {
                name: token.metadata.name.clone(),
                content_type: token.metadata.content_type.clone(),
                custom: token.metadata.custom.clone(),
            },
        }
    }
}

impl From<BlobReference> for BlobToken {
    fn from(reference: BlobReference) -> Self {
        Self {
            hash: BlobHash::from_hex(&reference.hash),
            size_bytes: reference.size_bytes,
            metadata: BlobMetadata {
                name: reference.metadata.name,
                content_type: reference.metadata.content_type,
                custom: reference.metadata.custom,
            },
        }
    }
}

/// Trait for integrating blob storage with CRDT documents
///
/// This trait enables storing blob tokens in documents that sync
/// via CRDT, allowing blobs to be discovered through document queries.
#[async_trait::async_trait]
pub trait BlobDocumentIntegration: Send + Sync {
    /// Store a blob token reference in a document field
    ///
    /// The token is serialized and stored in the specified field.
    /// When the document syncs to other nodes, they can retrieve
    /// the token and fetch the blob content.
    ///
    /// # Arguments
    /// * `collection` - Collection name
    /// * `doc_id` - Document ID
    /// * `field` - Field name to store the token in
    /// * `token` - Blob token to store
    ///
    /// # Note
    /// If the document doesn't exist, it will be created.
    /// If the field already exists, it will be overwritten.
    async fn store_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        token: &BlobToken,
    ) -> Result<()>;

    /// Retrieve a blob token from a document field
    ///
    /// Reads the specified field from the document and deserializes
    /// it as a blob token.
    ///
    /// # Arguments
    /// * `collection` - Collection name
    /// * `doc_id` - Document ID
    /// * `field` - Field name containing the token
    ///
    /// # Returns
    /// `Some(token)` if the field exists and is valid, `None` otherwise
    async fn get_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<Option<BlobToken>>;

    /// Remove a blob reference from a document
    ///
    /// Sets the field to null, indicating no blob is referenced.
    /// The blob itself is NOT deleted - just the reference.
    async fn remove_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<()>;

    /// List all blob references in a document
    ///
    /// Scans the document for fields containing blob references.
    /// Useful for discovering all blobs associated with a document.
    ///
    /// # Returns
    /// Map of field_name -> BlobToken for all blob reference fields
    async fn list_blob_references(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<HashMap<String, BlobToken>>;

    /// Store blob reference and fetch it locally
    ///
    /// Convenience method that stores the reference and immediately
    /// fetches the blob to ensure it's available locally.
    async fn store_and_fetch<F>(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        token: &BlobToken,
        progress: F,
    ) -> Result<std::path::PathBuf>
    where
        F: FnMut(BlobProgress) + Send + 'static;
}

/// Ditto implementation of blob-document integration
///
/// Stores blob tokens as JSON in Ditto documents. When documents
/// sync via Ditto's CRDT protocol, the blob references sync too.
///
/// # Blob Fetching
///
/// After retrieving a token from a synced document, use
/// `DittoBlobStore::fetch_blob()` to download the actual content.
/// Ditto's Attachment API handles the blob transfer automatically.
#[cfg(feature = "ditto-backend")]
pub struct DittoBlobDocumentIntegration<B: BlobStore> {
    store: Arc<DittoStore>,
    blob_store: Arc<B>,
}

#[cfg(feature = "ditto-backend")]
impl<B: BlobStore> DittoBlobDocumentIntegration<B> {
    /// Create new integration layer
    ///
    /// # Arguments
    /// * `store` - DittoStore for document operations
    /// * `blob_store` - BlobStore for blob operations
    pub fn new(store: Arc<DittoStore>, blob_store: Arc<B>) -> Self {
        Self { store, blob_store }
    }

    /// Access the underlying DittoStore
    pub fn ditto_store(&self) -> &DittoStore {
        &self.store
    }

    /// Access the underlying BlobStore
    pub fn blob_store(&self) -> &B {
        &self.blob_store
    }
}

#[cfg(feature = "ditto-backend")]
#[async_trait::async_trait]
impl<B: BlobStore + 'static> BlobDocumentIntegration for DittoBlobDocumentIntegration<B> {
    async fn store_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        token: &BlobToken,
    ) -> Result<()> {
        info!(
            "Storing blob reference: collection={}, doc_id={}, field={}, hash={}",
            collection,
            doc_id,
            field,
            token.hash.as_hex()
        );

        // Convert token to serializable reference
        let reference = BlobReference::from(token);

        // Use Ditto's DQL to upsert the document with blob reference
        // The blob reference is stored as a nested JSON object
        let query = format!(
            "INSERT INTO {} DOCUMENTS (:doc) ON ID CONFLICT DO UPDATE",
            collection
        );

        let doc = json!({
            "_id": doc_id,
            field: reference,
        });

        self.store
            .ditto()
            .store()
            .execute_v2((query, json!({ "doc": doc })))
            .await
            .with_context(|| {
                format!(
                    "Failed to store blob reference in {}.{}.{}",
                    collection, doc_id, field
                )
            })?;

        debug!(
            "Stored blob reference {} in {}.{}.{}",
            token.hash.as_hex(),
            collection,
            doc_id,
            field
        );

        Ok(())
    }

    async fn get_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<Option<BlobToken>> {
        debug!(
            "Getting blob reference: collection={}, doc_id={}, field={}",
            collection, doc_id, field
        );

        // Query the specific document
        let query = format!("SELECT * FROM {} WHERE _id = :id", collection);

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((query, json!({ "id": doc_id })))
            .await
            .with_context(|| format!("Failed to query {}.{}", collection, doc_id))?;

        // Parse results
        let documents: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                serde_json::from_str(&item.json_string()).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        if documents.is_empty() {
            debug!("Document {}.{} not found", collection, doc_id);
            return Ok(None);
        }

        // Extract the field value
        let value = &documents[0];

        // Try to extract the blob reference field
        if let Some(blob_ref_value) = value.get(field) {
            // Check if it's null or missing
            if blob_ref_value.is_null() {
                return Ok(None);
            }

            // Try to deserialize as BlobReference
            match serde_json::from_value::<BlobReference>(blob_ref_value.clone()) {
                Ok(reference) => {
                    let token = BlobToken::from(reference);
                    debug!(
                        "Found blob reference {} in {}.{}.{}",
                        token.hash.as_hex(),
                        collection,
                        doc_id,
                        field
                    );
                    Ok(Some(token))
                }
                Err(e) => {
                    warn!(
                        "Field {}.{}.{} is not a valid blob reference: {}",
                        collection, doc_id, field, e
                    );
                    Ok(None)
                }
            }
        } else {
            debug!(
                "Field {} not found in document {}.{}",
                field, collection, doc_id
            );
            Ok(None)
        }
    }

    async fn remove_blob_reference(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
    ) -> Result<()> {
        info!(
            "Removing blob reference: collection={}, doc_id={}, field={}",
            collection, doc_id, field
        );

        // Use UPDATE to set the field to null
        let query = format!("UPDATE {} SET {} = NULL WHERE _id = :id", collection, field);

        self.store
            .ditto()
            .store()
            .execute_v2((query, json!({ "id": doc_id })))
            .await
            .with_context(|| {
                format!(
                    "Failed to remove blob reference from {}.{}.{}",
                    collection, doc_id, field
                )
            })?;

        debug!(
            "Removed blob reference from {}.{}.{}",
            collection, doc_id, field
        );

        Ok(())
    }

    async fn list_blob_references(
        &self,
        collection: &str,
        doc_id: &str,
    ) -> Result<HashMap<String, BlobToken>> {
        debug!(
            "Listing blob references: collection={}, doc_id={}",
            collection, doc_id
        );

        let mut references = HashMap::new();

        // Query the document
        let query = format!("SELECT * FROM {} WHERE _id = :id", collection);

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((query, json!({ "id": doc_id })))
            .await
            .with_context(|| format!("Failed to query {}.{}", collection, doc_id))?;

        // Parse results
        let documents: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                serde_json::from_str(&item.json_string()).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        if documents.is_empty() {
            return Ok(references);
        }

        let value = &documents[0];

        // Iterate over all fields looking for blob references
        if let Some(obj) = value.as_object() {
            for (field, field_value) in obj {
                // Skip system fields
                if field.starts_with('_') {
                    continue;
                }

                // Try to parse as blob reference
                if let Ok(reference) = serde_json::from_value::<BlobReference>(field_value.clone())
                {
                    references.insert(field.clone(), BlobToken::from(reference));
                }
            }
        }

        debug!(
            "Found {} blob references in {}.{}",
            references.len(),
            collection,
            doc_id
        );

        Ok(references)
    }

    async fn store_and_fetch<F>(
        &self,
        collection: &str,
        doc_id: &str,
        field: &str,
        token: &BlobToken,
        progress: F,
    ) -> Result<std::path::PathBuf>
    where
        F: FnMut(BlobProgress) + Send + 'static,
    {
        // Store the reference first
        self.store_blob_reference(collection, doc_id, field, token)
            .await?;

        // Then fetch the blob locally
        let handle = self.blob_store.fetch_blob(token, progress).await?;

        Ok(handle.path.clone())
    }
}

// ============================================================================
// Model Registry Helper Types (for ADR-022 integration)
// ============================================================================

/// Model variant blob reference with execution requirements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelVariantBlob {
    /// Blob reference for this variant
    pub blob: BlobReference,
    /// Precision (e.g., "float32", "float16", "int8")
    pub precision: String,
    /// Supported execution providers (e.g., ["CUDAExecutionProvider", "CPUExecutionProvider"])
    pub execution_providers: Vec<String>,
    /// Minimum GPU memory required in GB (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_gpu_memory_gb: Option<f64>,
}

/// Model registry document with blob references
///
/// This schema matches ADR-022 model registry format with blob tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRegistryDocument {
    /// Model identifier (e.g., "target_recognition")
    pub model_id: String,
    /// Semantic version (e.g., "4.2.1")
    pub version: String,
    /// Available model variants keyed by variant ID
    pub variants: HashMap<String, ModelVariantBlob>,
    /// Model provenance information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ModelProvenance>,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Model provenance and signing information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelProvenance {
    /// Entity that signed the model
    pub signed_by: String,
    /// Cryptographic signature
    pub signature: String,
    /// Timestamp of signing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_at: Option<String>,
}

impl ModelRegistryDocument {
    /// Create a new model registry document
    pub fn new(model_id: &str, version: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
            version: version.to_string(),
            variants: HashMap::new(),
            provenance: None,
            description: None,
        }
    }

    /// Add a model variant with blob reference
    pub fn add_variant(
        &mut self,
        variant_id: &str,
        token: &BlobToken,
        precision: &str,
        execution_providers: Vec<String>,
        min_gpu_memory_gb: Option<f64>,
    ) {
        self.variants.insert(
            variant_id.to_string(),
            ModelVariantBlob {
                blob: BlobReference::from(token),
                precision: precision.to_string(),
                execution_providers,
                min_gpu_memory_gb,
            },
        );
    }

    /// Get document ID (model_id:version)
    pub fn doc_id(&self) -> String {
        format!("{}:{}", self.model_id, self.version)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_reference_serialization() {
        let token = BlobToken {
            hash: BlobHash::from_hex("abc123def456"),
            size_bytes: 1024 * 1024,
            metadata: BlobMetadata {
                name: Some("test.onnx".to_string()),
                content_type: Some("application/onnx".to_string()),
                custom: HashMap::new(),
            },
        };

        let reference = BlobReference::from(&token);
        let json = serde_json::to_string_pretty(&reference).unwrap();

        // Verify it serializes correctly
        assert!(json.contains("abc123def456"));
        assert!(json.contains("1048576"));
        assert!(json.contains("test.onnx"));

        // Verify round-trip
        let deserialized: BlobReference = serde_json::from_str(&json).unwrap();
        let token_back = BlobToken::from(deserialized);

        assert_eq!(token_back.hash.as_hex(), token.hash.as_hex());
        assert_eq!(token_back.size_bytes, token.size_bytes);
        assert_eq!(token_back.metadata.name, token.metadata.name);
    }

    #[test]
    fn test_model_registry_document() {
        let token = BlobToken {
            hash: BlobHash::from_hex("sha256:abc123"),
            size_bytes: 500_000_000,
            metadata: BlobMetadata::with_name("target_recognition_fp32.onnx"),
        };

        let mut doc = ModelRegistryDocument::new("target_recognition", "4.2.1");
        doc.add_variant(
            "fp32_cuda",
            &token,
            "float32",
            vec!["CUDAExecutionProvider".to_string()],
            Some(4.0),
        );

        assert_eq!(doc.doc_id(), "target_recognition:4.2.1");
        assert!(doc.variants.contains_key("fp32_cuda"));

        let json = serde_json::to_string_pretty(&doc).unwrap();
        assert!(json.contains("target_recognition"));
        assert!(json.contains("CUDAExecutionProvider"));
    }

    #[test]
    fn test_blob_reference_with_custom_metadata() {
        let mut custom = HashMap::new();
        custom.insert("training_date".to_string(), "2025-01-15".to_string());
        custom.insert("accuracy".to_string(), "0.95".to_string());

        let token = BlobToken {
            hash: BlobHash::from_hex("deadbeef"),
            size_bytes: 100,
            metadata: BlobMetadata {
                name: Some("model.onnx".to_string()),
                content_type: None,
                custom,
            },
        };

        let reference = BlobReference::from(&token);
        let json = serde_json::to_string(&reference).unwrap();

        // Custom fields should be present
        assert!(json.contains("training_date"));
        assert!(json.contains("accuracy"));

        // Round-trip should preserve custom fields
        let deserialized: BlobReference = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.metadata.custom.get("training_date"),
            Some(&"2025-01-15".to_string())
        );
    }
}
