//! Orchestration Service - Issue #177 Phase 3 / ADR-026
//!
//! The `OrchestrationService` coordinates software deployment lifecycle:
//! - Fetches blobs via Peat BlobStore
//! - Selects appropriate RuntimeAdapter for each artifact type
//! - Manages instance lifecycle (deploy, health, undeploy)
//! - Routes products/anomalies through Peat events
//! - Publishes capability advertisements
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::orchestration::{
//!     OrchestrationService, OnnxRuntimeAdapter, ContainerAdapter,
//!     ArtifactType, SimulatedAdapter,
//! };
//!
//! // Create service with simulated blob store (for testing)
//! let service = OrchestrationService::with_simulated_storage();
//!
//! // Register adapters
//! service.register_adapter(Arc::new(OnnxRuntimeAdapter::new()));
//! service.register_adapter(Arc::new(ContainerAdapter::new()));
//!
//! // Deploy an artifact
//! let instance_id = service.deploy(DeploymentRequest {
//!     blob_hash: "sha256:abc123...".into(),
//!     artifact_type: ArtifactType::onnx_cuda(),
//!     config: serde_json::json!({}),
//!     capabilities: vec!["object_detection".into()],
//! }).await?;
//!
//! // Check health
//! let health = service.health(&instance_id).await?;
//!
//! // Undeploy when done
//! service.undeploy(&instance_id).await?;
//! ```

use super::runtime::{
    AnomalyOutput, ArtifactType, HealthStatus, InstanceId, InstanceState, ProductOutput,
    RuntimeAdapter, RuntimeError, RuntimeMetrics, RuntimeResult,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Request to deploy an artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRequest {
    /// Blob hash for the artifact (content-addressed)
    pub blob_hash: String,
    /// Type of artifact being deployed
    pub artifact_type: ArtifactType,
    /// Runtime-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,
    /// Capabilities this deployment provides
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Optional deployment ID (generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
}

/// Record of a deployed instance
#[derive(Debug, Clone)]
pub struct InstanceRecord {
    /// Instance identifier
    pub instance_id: InstanceId,
    /// Original deployment request
    pub request: DeploymentRequest,
    /// Name of the adapter managing this instance
    pub adapter_name: String,
    /// Local path where blob was stored
    pub local_path: PathBuf,
    /// When the instance was activated
    pub activated_at: DateTime<Utc>,
    /// Current known state
    pub last_known_state: InstanceState,
}

/// Deployment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentStatus {
    /// Deployment in progress (fetching blob)
    Fetching,
    /// Activating via adapter
    Activating,
    /// Running successfully
    Running,
    /// Deployment failed
    Failed { reason: String },
    /// Stopped/undeployed
    Stopped,
}

/// Result of a deployment operation
#[derive(Debug, Clone)]
pub struct DeploymentResult {
    /// Instance ID (if successful)
    pub instance_id: Option<InstanceId>,
    /// Deployment status
    pub status: DeploymentStatus,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Duration of deployment in milliseconds
    pub duration_ms: u64,
}

/// Trait for blob storage backend
///
/// Abstracts the Peat BlobStore for dependency injection.
/// In production, use the actual Peat BlobStore implementation.
#[async_trait::async_trait]
pub trait BlobStorage: Send + Sync {
    /// Fetch a blob by hash and return its local path
    async fn fetch(&self, blob_hash: &str) -> RuntimeResult<PathBuf>;

    /// Check if a blob is locally available
    async fn has_local(&self, blob_hash: &str) -> bool;
}

/// Trait for event publishing
///
/// Abstracts Peat event publishing for dependency injection.
#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish a product event
    async fn publish_product(&self, product: ProductOutput) -> RuntimeResult<()>;

    /// Publish an anomaly event
    async fn publish_anomaly(&self, anomaly: AnomalyOutput) -> RuntimeResult<()>;
}

/// Trait for capability advertisement
#[async_trait::async_trait]
pub trait CapabilityPublisher: Send + Sync {
    /// Advertise capabilities for an instance
    async fn advertise(
        &self,
        instance_id: &InstanceId,
        capabilities: &[String],
        state: &InstanceState,
    ) -> RuntimeResult<()>;

    /// Remove capability advertisement
    async fn remove(&self, instance_id: &InstanceId) -> RuntimeResult<()>;
}

/// Simulated blob storage for testing
pub struct SimulatedBlobStorage {
    blobs: RwLock<HashMap<String, PathBuf>>,
    base_path: PathBuf,
}

impl SimulatedBlobStorage {
    /// Create simulated storage with a base path
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            blobs: RwLock::new(HashMap::new()),
            base_path,
        }
    }

    /// Pre-populate a blob for testing
    pub async fn add_blob(&self, hash: &str, path: PathBuf) {
        self.blobs.write().await.insert(hash.to_string(), path);
    }
}

#[async_trait::async_trait]
impl BlobStorage for SimulatedBlobStorage {
    async fn fetch(&self, blob_hash: &str) -> RuntimeResult<PathBuf> {
        let blobs = self.blobs.read().await;
        if let Some(path) = blobs.get(blob_hash) {
            return Ok(path.clone());
        }

        // Simulate blob fetch by returning a path based on hash
        let simulated_path = self.base_path.join(format!("{}.blob", blob_hash));
        Ok(simulated_path)
    }

    async fn has_local(&self, blob_hash: &str) -> bool {
        self.blobs.read().await.contains_key(blob_hash)
    }
}

/// Simulated event publisher for testing
pub struct SimulatedEventPublisher {
    products: RwLock<Vec<ProductOutput>>,
    anomalies: RwLock<Vec<AnomalyOutput>>,
}

impl SimulatedEventPublisher {
    pub fn new() -> Self {
        Self {
            products: RwLock::new(Vec::new()),
            anomalies: RwLock::new(Vec::new()),
        }
    }

    /// Get all published products (for testing)
    pub async fn get_products(&self) -> Vec<ProductOutput> {
        self.products.read().await.clone()
    }

    /// Get all published anomalies (for testing)
    pub async fn get_anomalies(&self) -> Vec<AnomalyOutput> {
        self.anomalies.read().await.clone()
    }
}

impl Default for SimulatedEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EventPublisher for SimulatedEventPublisher {
    async fn publish_product(&self, product: ProductOutput) -> RuntimeResult<()> {
        debug!(
            instance_id = %product.instance_id,
            product_type = %product.product_type,
            "Publishing product (simulated)"
        );
        self.products.write().await.push(product);
        Ok(())
    }

    async fn publish_anomaly(&self, anomaly: AnomalyOutput) -> RuntimeResult<()> {
        warn!(
            instance_id = %anomaly.instance_id,
            anomaly_type = %anomaly.anomaly_type,
            severity = ?anomaly.severity,
            "Publishing anomaly (simulated)"
        );
        self.anomalies.write().await.push(anomaly);
        Ok(())
    }
}

/// Simulated capability publisher for testing
pub struct SimulatedCapabilityPublisher {
    capabilities: RwLock<HashMap<String, (Vec<String>, InstanceState)>>,
}

impl SimulatedCapabilityPublisher {
    pub fn new() -> Self {
        Self {
            capabilities: RwLock::new(HashMap::new()),
        }
    }

    /// Get advertised capabilities (for testing)
    pub async fn get_capabilities(&self) -> HashMap<String, (Vec<String>, InstanceState)> {
        self.capabilities.read().await.clone()
    }
}

impl Default for SimulatedCapabilityPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CapabilityPublisher for SimulatedCapabilityPublisher {
    async fn advertise(
        &self,
        instance_id: &InstanceId,
        capabilities: &[String],
        state: &InstanceState,
    ) -> RuntimeResult<()> {
        info!(
            instance_id = %instance_id,
            capabilities = ?capabilities,
            state = ?state,
            "Advertising capabilities (simulated)"
        );
        self.capabilities.write().await.insert(
            instance_id.to_string(),
            (capabilities.to_vec(), state.clone()),
        );
        Ok(())
    }

    async fn remove(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        info!(instance_id = %instance_id, "Removing capabilities (simulated)");
        self.capabilities
            .write()
            .await
            .remove(&instance_id.to_string());
        Ok(())
    }
}

/// Orchestration Service
///
/// Coordinates software deployment lifecycle on a Peat node:
/// - Manages runtime adapters for different artifact types
/// - Fetches blobs and activates instances
/// - Monitors health and publishes events
/// - Advertises capabilities to the Peat network
pub struct OrchestrationService {
    /// Blob storage backend
    blob_storage: Arc<dyn BlobStorage>,
    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,
    /// Capability publisher
    capability_publisher: Arc<dyn CapabilityPublisher>,
    /// Registered runtime adapters
    adapters: RwLock<Vec<Arc<dyn RuntimeAdapter>>>,
    /// Active instances
    instances: RwLock<HashMap<InstanceId, InstanceRecord>>,
    /// Monitoring tasks (instance_id -> task handle)
    monitors: RwLock<HashMap<InstanceId, tokio::task::JoinHandle<()>>>,
}

impl OrchestrationService {
    /// Create a new orchestration service
    pub fn new(
        blob_storage: Arc<dyn BlobStorage>,
        event_publisher: Arc<dyn EventPublisher>,
        capability_publisher: Arc<dyn CapabilityPublisher>,
    ) -> Self {
        Self {
            blob_storage,
            event_publisher,
            capability_publisher,
            adapters: RwLock::new(Vec::new()),
            instances: RwLock::new(HashMap::new()),
            monitors: RwLock::new(HashMap::new()),
        }
    }

    /// Create with simulated backends (for testing)
    pub fn with_simulated_storage() -> Self {
        Self::new(
            Arc::new(SimulatedBlobStorage::new(PathBuf::from("/tmp/peat-blobs"))),
            Arc::new(SimulatedEventPublisher::new()),
            Arc::new(SimulatedCapabilityPublisher::new()),
        )
    }

    /// Register a runtime adapter
    pub async fn register_adapter(&self, adapter: Arc<dyn RuntimeAdapter>) {
        info!(adapter = adapter.name(), "Registering runtime adapter");
        self.adapters.write().await.push(adapter);
    }

    /// List registered adapters
    pub async fn list_adapters(&self) -> Vec<String> {
        self.adapters
            .read()
            .await
            .iter()
            .map(|a| a.name().to_string())
            .collect()
    }

    /// Find an adapter that can handle the given artifact type
    async fn find_adapter(&self, artifact_type: &ArtifactType) -> Option<Arc<dyn RuntimeAdapter>> {
        let adapters = self.adapters.read().await;
        adapters
            .iter()
            .find(|a| a.can_handle(artifact_type))
            .cloned()
    }

    /// Deploy an artifact
    ///
    /// 1. Fetches the blob from storage
    /// 2. Finds an appropriate adapter
    /// 3. Activates the artifact
    /// 4. Starts monitoring for products/anomalies
    /// 5. Advertises capabilities
    pub async fn deploy(&self, request: DeploymentRequest) -> RuntimeResult<DeploymentResult> {
        let start = std::time::Instant::now();

        info!(
            blob_hash = %request.blob_hash,
            artifact_type = ?request.artifact_type,
            capabilities = ?request.capabilities,
            "Starting deployment"
        );

        // Find adapter
        let adapter = self
            .find_adapter(&request.artifact_type)
            .await
            .ok_or_else(|| {
                RuntimeError::UnsupportedArtifact(format!(
                    "No adapter for artifact type: {:?}",
                    request.artifact_type
                ))
            })?;

        // Fetch blob
        let local_path = match self.blob_storage.fetch(&request.blob_hash).await {
            Ok(path) => path,
            Err(e) => {
                error!(error = %e, "Failed to fetch blob");
                return Ok(DeploymentResult {
                    instance_id: None,
                    status: DeploymentStatus::Failed {
                        reason: format!("Blob fetch failed: {}", e),
                    },
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // Activate via adapter
        let instance_id = match adapter
            .activate(&request.artifact_type, &request.config, local_path.clone())
            .await
        {
            Ok(id) => id,
            Err(e) => {
                error!(error = %e, "Failed to activate artifact");
                return Ok(DeploymentResult {
                    instance_id: None,
                    status: DeploymentStatus::Failed {
                        reason: format!("Activation failed: {}", e),
                    },
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        // Record instance
        let record = InstanceRecord {
            instance_id: instance_id.clone(),
            request: request.clone(),
            adapter_name: adapter.name().to_string(),
            local_path,
            activated_at: Utc::now(),
            last_known_state: InstanceState::Running,
        };
        self.instances
            .write()
            .await
            .insert(instance_id.clone(), record);

        // Start monitoring
        self.start_monitoring(&instance_id, adapter.clone()).await?;

        // Advertise capabilities
        self.capability_publisher
            .advertise(&instance_id, &request.capabilities, &InstanceState::Running)
            .await?;

        info!(
            instance_id = %instance_id,
            adapter = adapter.name(),
            duration_ms = start.elapsed().as_millis(),
            "Deployment completed"
        );

        Ok(DeploymentResult {
            instance_id: Some(instance_id),
            status: DeploymentStatus::Running,
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Start monitoring an instance for products and anomalies
    async fn start_monitoring(
        &self,
        instance_id: &InstanceId,
        adapter: Arc<dyn RuntimeAdapter>,
    ) -> RuntimeResult<()> {
        // Subscribe to products
        let mut products = adapter.subscribe_products(instance_id).await?;
        let event_pub = self.event_publisher.clone();
        let iid = instance_id.clone();

        let product_handle = tokio::spawn(async move {
            while let Ok(product) = products.recv().await {
                if let Err(e) = event_pub.publish_product(product).await {
                    error!(error = %e, "Failed to publish product");
                }
            }
            debug!(instance_id = %iid, "Product monitoring ended");
        });

        // Subscribe to anomalies
        let mut anomalies = adapter.subscribe_anomalies(instance_id).await?;
        let event_pub = self.event_publisher.clone();
        let iid = instance_id.clone();

        let anomaly_handle = tokio::spawn(async move {
            while let Ok(anomaly) = anomalies.recv().await {
                if let Err(e) = event_pub.publish_anomaly(anomaly).await {
                    error!(error = %e, "Failed to publish anomaly");
                }
            }
            debug!(instance_id = %iid, "Anomaly monitoring ended");
        });

        // Combine handles (we just track one for now)
        let combined_handle = tokio::spawn(async move {
            let _ = tokio::join!(product_handle, anomaly_handle);
        });

        self.monitors
            .write()
            .await
            .insert(instance_id.clone(), combined_handle);

        Ok(())
    }

    /// Undeploy an instance
    pub async fn undeploy(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        info!(instance_id = %instance_id, "Undeploying instance");

        // Get record
        let record = self
            .instances
            .write()
            .await
            .remove(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        // Stop monitoring
        if let Some(handle) = self.monitors.write().await.remove(instance_id) {
            handle.abort();
        }

        // Find adapter and deactivate
        if let Some(adapter) = self.find_adapter(&record.request.artifact_type).await {
            adapter.deactivate(instance_id).await?;
        }

        // Remove capability advertisement
        self.capability_publisher.remove(instance_id).await?;

        info!(
            instance_id = %instance_id,
            uptime_secs = (Utc::now() - record.activated_at).num_seconds(),
            "Instance undeployed"
        );

        Ok(())
    }

    /// Get health status of an instance
    pub async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus> {
        let instances = self.instances.read().await;
        let record = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        if let Some(adapter) = self.find_adapter(&record.request.artifact_type).await {
            adapter.health(instance_id).await
        } else {
            Err(RuntimeError::UnsupportedArtifact(format!(
                "No adapter for: {}",
                record.adapter_name
            )))
        }
    }

    /// Get metrics for an instance
    pub async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics> {
        let instances = self.instances.read().await;
        let record = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        if let Some(adapter) = self.find_adapter(&record.request.artifact_type).await {
            adapter.metrics(instance_id).await
        } else {
            Err(RuntimeError::UnsupportedArtifact(format!(
                "No adapter for: {}",
                record.adapter_name
            )))
        }
    }

    /// Get health of all instances
    pub async fn health_all(&self) -> HashMap<InstanceId, HealthStatus> {
        let instances = self.instances.read().await;
        let mut results = HashMap::new();

        for (id, record) in instances.iter() {
            if let Some(adapter) = self.find_adapter(&record.request.artifact_type).await {
                if let Ok(health) = adapter.health(id).await {
                    results.insert(id.clone(), health);
                }
            }
        }

        results
    }

    /// List all active instances
    pub async fn list_instances(&self) -> Vec<InstanceRecord> {
        self.instances.read().await.values().cloned().collect()
    }

    /// Get instance record by ID
    pub async fn get_instance(&self, instance_id: &InstanceId) -> Option<InstanceRecord> {
        self.instances.read().await.get(instance_id).cloned()
    }

    /// Attempt hot-reload of an instance
    pub async fn hot_reload(
        &self,
        instance_id: &InstanceId,
        new_blob_hash: &str,
        new_artifact_type: Option<ArtifactType>,
        new_config: Option<serde_json::Value>,
    ) -> RuntimeResult<bool> {
        let instances = self.instances.read().await;
        let record = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        let artifact_type = new_artifact_type.unwrap_or(record.request.artifact_type.clone());
        let config = new_config.unwrap_or(record.request.config.clone());

        drop(instances);

        // Fetch new blob
        let new_path = self.blob_storage.fetch(new_blob_hash).await?;

        // Find adapter
        let adapter = self
            .find_adapter(&artifact_type)
            .await
            .ok_or_else(|| RuntimeError::UnsupportedArtifact("No adapter found".into()))?;

        // Attempt hot reload
        let success = adapter
            .hot_reload(instance_id, &artifact_type, &config, new_path.clone())
            .await?;

        if success {
            // Update record
            let mut instances = self.instances.write().await;
            if let Some(record) = instances.get_mut(instance_id) {
                record.request.blob_hash = new_blob_hash.to_string();
                record.request.artifact_type = artifact_type;
                record.request.config = config;
                record.local_path = new_path;
            }
            info!(instance_id = %instance_id, "Hot reload successful");
        } else {
            info!(instance_id = %instance_id, "Hot reload not supported, full redeploy needed");
        }

        Ok(success)
    }

    /// Shutdown all instances
    pub async fn shutdown(&self) -> RuntimeResult<()> {
        info!("Shutting down orchestration service");

        let instance_ids: Vec<InstanceId> = self.instances.read().await.keys().cloned().collect();

        for id in instance_ids {
            if let Err(e) = self.undeploy(&id).await {
                error!(instance_id = %id, error = %e, "Failed to undeploy during shutdown");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::runtime::SimulatedAdapter;

    #[tokio::test]
    async fn test_service_creation() {
        let service = OrchestrationService::with_simulated_storage();
        assert!(service.list_adapters().await.is_empty());
    }

    #[tokio::test]
    async fn test_register_adapter() {
        let service = OrchestrationService::with_simulated_storage();
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("test")))
            .await;

        let adapters = service.list_adapters().await;
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0], "test");
    }

    #[tokio::test]
    async fn test_deploy_with_simulated_adapter() {
        let service = OrchestrationService::with_simulated_storage();
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let result = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test123".into(),
                artifact_type: ArtifactType::onnx_cuda(),
                config: serde_json::json!({}),
                capabilities: vec!["object_detection".into()],
                deployment_id: None,
            })
            .await
            .unwrap();

        assert!(result.instance_id.is_some());
        assert_eq!(result.status, DeploymentStatus::Running);

        // Check instance is tracked
        let instances = service.list_instances().await;
        assert_eq!(instances.len(), 1);

        // Check health
        let health = service
            .health(result.instance_id.as_ref().unwrap())
            .await
            .unwrap();
        assert!(health.state.is_healthy());

        // Undeploy
        service
            .undeploy(result.instance_id.as_ref().unwrap())
            .await
            .unwrap();
        assert!(service.list_instances().await.is_empty());
    }

    #[tokio::test]
    async fn test_deploy_no_adapter() {
        let service = OrchestrationService::with_simulated_storage();
        // Don't register any adapters

        let result = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test123".into(),
                artifact_type: ArtifactType::onnx_cuda(),
                config: serde_json::json!({}),
                capabilities: vec![],
                deployment_id: None,
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_all() {
        let service = OrchestrationService::with_simulated_storage();
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        // Deploy two instances
        let result1 = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test1".into(),
                artifact_type: ArtifactType::onnx_cuda(),
                config: serde_json::json!({}),
                capabilities: vec!["cap1".into()],
                deployment_id: None,
            })
            .await
            .unwrap();

        let result2 = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test2".into(),
                artifact_type: ArtifactType::onnx_cuda(),
                config: serde_json::json!({}),
                capabilities: vec!["cap2".into()],
                deployment_id: None,
            })
            .await
            .unwrap();

        // Check health of all
        let health_all = service.health_all().await;
        assert_eq!(health_all.len(), 2);
        assert!(health_all.contains_key(result1.instance_id.as_ref().unwrap()));
        assert!(health_all.contains_key(result2.instance_id.as_ref().unwrap()));

        // Shutdown
        service.shutdown().await.unwrap();
        assert!(service.list_instances().await.is_empty());
    }

    #[tokio::test]
    async fn test_get_instance() {
        let service = OrchestrationService::with_simulated_storage();
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let result = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test123".into(),
                artifact_type: ArtifactType::onnx_tensorrt(),
                config: serde_json::json!({"batch_size": 4}),
                capabilities: vec!["detection".into(), "tracking".into()],
                deployment_id: Some("deployment-001".into()),
            })
            .await
            .unwrap();

        let instance_id = result.instance_id.unwrap();
        let record = service.get_instance(&instance_id).await.unwrap();

        assert_eq!(record.request.blob_hash, "sha256:test123");
        assert_eq!(record.request.capabilities.len(), 2);
        assert_eq!(
            record.request.deployment_id,
            Some("deployment-001".to_string())
        );
    }

    #[tokio::test]
    async fn test_metrics() {
        let service = OrchestrationService::with_simulated_storage();
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let result = service
            .deploy(DeploymentRequest {
                blob_hash: "sha256:test123".into(),
                artifact_type: ArtifactType::onnx_cuda(),
                config: serde_json::json!({}),
                capabilities: vec![],
                deployment_id: None,
            })
            .await
            .unwrap();

        let instance_id = result.instance_id.unwrap();
        let metrics = service.metrics(&instance_id).await.unwrap();

        assert_eq!(metrics.instance_id, instance_id);
    }
}
