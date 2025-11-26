# ADR-026: Reference Implementation - Software Orchestration

**Status**: Proposed  
**Date**: 2025-11-25  
**Authors**: Codex, Kit Plummer  
**Type**: Reference Implementation (not protocol specification)  
**Relates to**: ADR-012 (Schema), ADR-025 (Blob Transfer), ADR-018 (Capability Advertisement)

## Document Classification

> **⚠️ This is a REFERENCE IMPLEMENTATION guide, not a HIVE Protocol specification.**
>
> This ADR demonstrates how to build a software orchestration system on top of HIVE Protocol primitives. The patterns, traits, and implementations shown here are examples that users MAY adopt, adapt, or replace entirely.
>
> **HIVE Protocol primitives used:**
> - `BlobStore` / `BlobRef` (ADR-025)
> - `CapabilityAdvertisement` (ADR-012)
> - `HiveEvent` with `AggregationPolicy` (ADR-012)
> - `DeploymentDirective` (ADR-012)
>
> **Application-layer concerns defined here:**
> - `RuntimeAdapter` trait and implementations
> - `OrchestrationService` coordination logic
> - Product schemas (DetectionProduct, etc.)
> - Runtime-specific handling (ONNX, containers)

## Context

### Building on HIVE Primitives

HIVE Protocol provides foundational primitives for distributed coordination:

| HIVE Primitive | What It Does | ADR |
|----------------|--------------|-----|
| BlobRef / BlobStore | Content-addressed binary transfer | ADR-025 |
| CapabilityAdvertisement | Nodes advertise what they can do | ADR-012 |
| HiveEvent | Typed events with routing policies | ADR-012 |
| DeploymentDirective | Commands flow through hierarchy | ADR-012 |
| AggregationPolicy | Control event propagation | ADR-012 |

**These primitives are runtime-agnostic** - HIVE doesn't know or care if you're deploying ONNX models, Docker containers, or configuration files.

This reference implementation shows **one way** to build a software orchestration system that:
- Deploys multiple artifact types (models, containers, binaries)
- Reports health and status through HIVE events
- Produces outputs that flow through HIVE's event routing
- Uses HIVE's blob transfer for artifact distribution

### Why a Reference Implementation?

Different organizations will have different needs:
- Some may only deploy ONNX models
- Others may use Kubernetes and want to integrate with existing tooling
- Military users may have specific runtime requirements (MOSA, certifications)

By providing a reference implementation rather than mandating an approach, we:
1. **Demonstrate** how to use HIVE primitives effectively
2. **Provide** working code that can be adopted or adapted
3. **Avoid** constraining users to our specific choices
4. **Enable** innovation in the application layer

## Reference Architecture

### Layering

```
┌─────────────────────────────────────────────────────────────────┐
│  YOUR APPLICATION (you build this)                               │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ Custom orchestration logic, UIs, integrations               ││
│  └─────────────────────────────────────────────────────────────┘│
└──────────────────────────┬──────────────────────────────────────┘
                           │ may use
┌──────────────────────────┴──────────────────────────────────────┐
│  REFERENCE IMPLEMENTATION (this document)                        │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ OrchestrationService                                        ││
│  │ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐    ││
│  │ │OnnxAdapter│ │Container  │ │Process    │ │YourAdapter│    ││
│  │ │           │ │Adapter    │ │Adapter    │ │           │    ││
│  │ └───────────┘ └───────────┘ └───────────┘ └───────────┘    ││
│  └─────────────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ Application Schemas                                         ││
│  │ DetectionProduct, ClassificationProduct, ModelMetrics, etc. ││
│  └─────────────────────────────────────────────────────────────┘│
└──────────────────────────┬──────────────────────────────────────┘
                           │ uses
═══════════════════════════╪═══════════════════════════════════════
         HIVE PROTOCOL BOUNDARY
═══════════════════════════╪═══════════════════════════════════════
                           │
┌──────────────────────────┴──────────────────────────────────────┐
│  HIVE PROTOCOL LAYER                                             │
│  BlobStore, CapabilityAdvertisement, HiveEvent, DeploymentDir.  │
└─────────────────────────────────────────────────────────────────┘
```

### Core Concepts

**Artifact**: A deployable unit (blob) with type information
```rust
struct Artifact {
    blob_ref: BlobRef,           // HIVE primitive (ADR-025)
    artifact_type: ArtifactType, // Application-defined
    config: serde_json::Value,   // Runtime-specific config
}
```

**RuntimeAdapter**: Handles artifact-type-specific activation
```rust
trait RuntimeAdapter {
    fn activate(&self, artifact: &Artifact, local_path: PathBuf) -> Result<InstanceId>;
    fn deactivate(&self, instance_id: &InstanceId) -> Result<()>;
    fn health(&self, instance_id: &InstanceId) -> Result<HealthStatus>;
    // ...
}
```

**OrchestrationService**: Coordinates deployment lifecycle
```rust
struct OrchestrationService {
    blob_store: Arc<dyn BlobStore>,        // HIVE primitive
    hive_events: Arc<dyn HiveEventPublisher>, // HIVE primitive  
    adapters: Vec<Arc<dyn RuntimeAdapter>>, // Application-defined
}
```

## Reference Implementation

### Artifact Types

Define the types of artifacts your system supports:

```rust
/// Artifact type - extend as needed for your use case
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ArtifactType {
    /// ONNX model for inference
    #[serde(rename = "onnx_model")]
    OnnxModel {
        /// Execution providers in preference order
        execution_providers: Vec<String>,
        /// Model input/output signature (optional)
        signature: Option<ModelSignature>,
    },
    
    /// OCI container image
    #[serde(rename = "container")]
    Container {
        /// Container runtime
        runtime: ContainerRuntime,
        /// Exposed ports
        ports: Vec<PortMapping>,
        /// Environment variables
        env: HashMap<String, String>,
    },
    
    /// Native executable
    #[serde(rename = "native_binary")]
    NativeBinary {
        /// Target architecture (x86_64, aarch64)
        arch: String,
        /// Command-line arguments
        args: Vec<String>,
    },
    
    /// Configuration package (files to deploy)
    #[serde(rename = "config_package")]
    ConfigPackage {
        /// Target directory for extraction
        target_path: PathBuf,
    },
    
    /// WebAssembly module
    #[serde(rename = "wasm_module")]
    WasmModule {
        /// WASI capabilities required
        wasi_capabilities: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContainerRuntime {
    Docker,
    Podman,
    Containerd,
}
```

### RuntimeAdapter Trait

The core abstraction for runtime-specific behavior:

```rust
use async_trait::async_trait;
use tokio::sync::broadcast;

/// Unique identifier for a running instance
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstanceId(pub String);

/// Health status of a running instance
#[derive(Clone, Debug)]
pub struct HealthStatus {
    pub instance_id: InstanceId,
    pub state: InstanceState,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub details: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstanceState {
    Starting,
    Running,
    Degraded { reason: String },
    Stopped,
    Failed { error: String },
}

/// Runtime metrics
#[derive(Clone, Debug)]
pub struct RuntimeMetrics {
    pub instance_id: InstanceId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub cpu_usage: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub gpu_memory_bytes: Option<u64>,
    pub custom: HashMap<String, f64>,
}

/// Output product from software (detection, classification, etc.)
/// 
/// This wraps a HiveEvent payload with instance context.
/// The actual payload is application-defined.
#[derive(Clone, Debug)]
pub struct ProductOutput {
    pub instance_id: InstanceId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub product_type: String,
    pub payload: serde_json::Value,
    pub routing: RoutingHint,
}

/// Hint for how this product should be routed through HIVE
#[derive(Clone, Debug, Default)]
pub struct RoutingHint {
    /// Map to HiveEvent.AggregationPolicy
    pub propagate_full: bool,
    pub propagate_summary: bool,
    pub priority: EventPriority,
    pub ttl_seconds: u32,
}

#[derive(Clone, Debug, Default)]
pub enum EventPriority {
    Critical,
    High,
    #[default]
    Normal,
    Low,
    LocalOnly,
}

/// Anomaly detected by the runtime
#[derive(Clone, Debug)]
pub struct AnomalyOutput {
    pub instance_id: InstanceId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub anomaly_type: String,
    pub severity: AnomalySeverity,
    pub description: String,
    pub evidence: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
pub enum AnomalySeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Runtime adapter trait
///
/// Implement this for each artifact type you want to support.
/// The orchestration service uses this trait to manage artifacts
/// without knowing the runtime-specific details.
///
/// # Example Implementations
///
/// - `OnnxRuntimeAdapter` - Loads ONNX models via ort crate
/// - `ContainerAdapter` - Manages Docker/Podman containers
/// - `ProcessAdapter` - Runs native binaries as child processes
#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    /// Human-readable name for this adapter
    fn name(&self) -> &str;
    
    /// Check if this adapter can handle the given artifact type
    fn can_handle(&self, artifact_type: &ArtifactType) -> bool;
    
    /// Activate an artifact
    ///
    /// The blob has already been fetched to `local_path`.
    /// This method should load/start the artifact and return an instance ID.
    async fn activate(
        &self,
        artifact_type: &ArtifactType,
        config: &serde_json::Value,
        local_path: PathBuf,
    ) -> Result<InstanceId>;
    
    /// Deactivate a running instance
    ///
    /// Should gracefully stop the instance and release resources.
    async fn deactivate(&self, instance_id: &InstanceId) -> Result<()>;
    
    /// Get current health status
    async fn health(&self, instance_id: &InstanceId) -> Result<HealthStatus>;
    
    /// Get current metrics
    async fn metrics(&self, instance_id: &InstanceId) -> Result<RuntimeMetrics>;
    
    /// Subscribe to product outputs from this instance
    ///
    /// Products are runtime-specific outputs (detections, classifications, etc.)
    /// The orchestration service will route these through HIVE events.
    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<ProductOutput>>;
    
    /// Subscribe to anomaly outputs from this instance
    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<AnomalyOutput>>;
    
    /// Attempt hot-reload without full restart
    ///
    /// Returns Ok(true) if successful, Ok(false) if not supported.
    async fn hot_reload(
        &self,
        instance_id: &InstanceId,
        new_artifact_type: &ArtifactType,
        new_config: &serde_json::Value,
        new_path: PathBuf,
    ) -> Result<bool> {
        // Default: not supported
        Ok(false)
    }
    
    /// List all instances managed by this adapter
    fn list_instances(&self) -> Vec<InstanceId>;
}
```

### Example: OnnxRuntimeAdapter

```rust
use ort::{Environment, Session, SessionBuilder};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct OnnxRuntimeAdapter {
    env: Arc<Environment>,
    sessions: RwLock<HashMap<InstanceId, OnnxSession>>,
}

struct OnnxSession {
    session: Session,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
    metrics: Arc<RwLock<InferenceMetrics>>,
}

#[derive(Default)]
struct InferenceMetrics {
    inference_count: u64,
    total_latency_ms: f64,
    last_inference: Option<chrono::DateTime<chrono::Utc>>,
}

impl OnnxRuntimeAdapter {
    pub fn new() -> Result<Self> {
        let env = Environment::builder()
            .with_name("hive_onnx")
            .build()?;
        
        Ok(Self {
            env: Arc::new(env),
            sessions: RwLock::new(HashMap::new()),
        })
    }
    
    /// Run inference on a loaded model
    ///
    /// This is called by application code, not by the orchestration service.
    /// Products are published to subscribers automatically.
    pub async fn infer(
        &self,
        instance_id: &InstanceId,
        inputs: HashMap<String, ndarray::ArrayD<f32>>,
    ) -> Result<HashMap<String, ndarray::ArrayD<f32>>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        let start = std::time::Instant::now();
        
        // Run inference
        let outputs = session.session.run(inputs)?;
        
        let latency = start.elapsed().as_secs_f64() * 1000.0;
        
        // Update metrics
        {
            let mut metrics = session.metrics.write().await;
            metrics.inference_count += 1;
            metrics.total_latency_ms += latency;
            metrics.last_inference = Some(chrono::Utc::now());
        }
        
        Ok(outputs)
    }
    
    /// Publish a product (e.g., detection result)
    ///
    /// Call this after processing inference outputs.
    pub async fn publish_product(
        &self,
        instance_id: &InstanceId,
        product: ProductOutput,
    ) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        let _ = session.product_tx.send(product);
        Ok(())
    }
}

#[async_trait]
impl RuntimeAdapter for OnnxRuntimeAdapter {
    fn name(&self) -> &str {
        "onnx_runtime"
    }
    
    fn can_handle(&self, artifact_type: &ArtifactType) -> bool {
        matches!(artifact_type, ArtifactType::OnnxModel { .. })
    }
    
    async fn activate(
        &self,
        artifact_type: &ArtifactType,
        config: &serde_json::Value,
        local_path: PathBuf,
    ) -> Result<InstanceId> {
        let ArtifactType::OnnxModel { execution_providers, .. } = artifact_type else {
            anyhow::bail!("Not an ONNX model");
        };
        
        // Build session with requested execution providers
        let mut builder = SessionBuilder::new(&self.env)?;
        
        for provider in execution_providers {
            builder = match provider.as_str() {
                "CUDAExecutionProvider" => builder.with_execution_providers([
                    ort::CUDAExecutionProvider::default().build()
                ])?,
                "TensorRTExecutionProvider" => builder.with_execution_providers([
                    ort::TensorRTExecutionProvider::default().build()
                ])?,
                _ => builder,
            };
        }
        
        let session = builder.with_model_from_file(&local_path)?;
        
        let instance_id = InstanceId(uuid::Uuid::new_v4().to_string());
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);
        
        let onnx_session = OnnxSession {
            session,
            product_tx,
            anomaly_tx,
            metrics: Arc::new(RwLock::new(InferenceMetrics::default())),
        };
        
        self.sessions.write().await.insert(instance_id.clone(), onnx_session);
        
        Ok(instance_id)
    }
    
    async fn deactivate(&self, instance_id: &InstanceId) -> Result<()> {
        self.sessions.write().await.remove(instance_id);
        Ok(())
    }
    
    async fn health(&self, instance_id: &InstanceId) -> Result<HealthStatus> {
        let sessions = self.sessions.read().await;
        let _session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        Ok(HealthStatus {
            instance_id: instance_id.clone(),
            state: InstanceState::Running,
            timestamp: chrono::Utc::now(),
            details: HashMap::new(),
        })
    }
    
    async fn metrics(&self, instance_id: &InstanceId) -> Result<RuntimeMetrics> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        let metrics = session.metrics.read().await;
        let avg_latency = if metrics.inference_count > 0 {
            metrics.total_latency_ms / metrics.inference_count as f64
        } else {
            0.0
        };
        
        Ok(RuntimeMetrics {
            instance_id: instance_id.clone(),
            timestamp: chrono::Utc::now(),
            cpu_usage: None, // Would need system monitoring
            memory_bytes: None,
            gpu_memory_bytes: None,
            custom: [
                ("inference_count".into(), metrics.inference_count as f64),
                ("avg_latency_ms".into(), avg_latency),
            ].into(),
        })
    }
    
    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<ProductOutput>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        Ok(session.product_tx.subscribe())
    }
    
    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<AnomalyOutput>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        Ok(session.anomaly_tx.subscribe())
    }
    
    fn list_instances(&self) -> Vec<InstanceId> {
        // Note: sync access, may need redesign for production
        vec![]
    }
}
```

### Example: ContainerAdapter

```rust
use bollard::Docker;

pub struct ContainerAdapter {
    docker: Docker,
    containers: RwLock<HashMap<InstanceId, ContainerRecord>>,
}

struct ContainerRecord {
    container_id: String,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
}

impl ContainerAdapter {
    pub async fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        
        Ok(Self {
            docker,
            containers: RwLock::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl RuntimeAdapter for ContainerAdapter {
    fn name(&self) -> &str {
        "container_runtime"
    }
    
    fn can_handle(&self, artifact_type: &ArtifactType) -> bool {
        matches!(artifact_type, ArtifactType::Container { .. })
    }
    
    async fn activate(
        &self,
        artifact_type: &ArtifactType,
        config: &serde_json::Value,
        local_path: PathBuf,
    ) -> Result<InstanceId> {
        let ArtifactType::Container { runtime, ports, env } = artifact_type else {
            anyhow::bail!("Not a container");
        };
        
        // Load image from tarball
        let image_data = tokio::fs::read(&local_path).await?;
        self.docker.import_image(
            bollard::image::ImportImageOptions { quiet: true },
            image_data.into(),
            None,
        ).await?;
        
        // Create container
        let container = self.docker.create_container::<&str, &str>(
            None,
            bollard::container::Config {
                image: Some("imported-image"), // Would extract from tarball
                env: Some(env.iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
                ..Default::default()
            },
        ).await?;
        
        // Start container
        self.docker.start_container::<&str>(&container.id, None).await?;
        
        let instance_id = InstanceId(container.id.clone());
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);
        
        self.containers.write().await.insert(instance_id.clone(), ContainerRecord {
            container_id: container.id,
            product_tx,
            anomaly_tx,
        });
        
        // Start log monitoring (products could be extracted from logs)
        // self.start_log_monitor(&instance_id).await?;
        
        Ok(instance_id)
    }
    
    async fn deactivate(&self, instance_id: &InstanceId) -> Result<()> {
        let containers = self.containers.read().await;
        if let Some(record) = containers.get(instance_id) {
            self.docker.stop_container(&record.container_id, None).await?;
            self.docker.remove_container(&record.container_id, None).await?;
        }
        drop(containers);
        
        self.containers.write().await.remove(instance_id);
        Ok(())
    }
    
    async fn health(&self, instance_id: &InstanceId) -> Result<HealthStatus> {
        let containers = self.containers.read().await;
        let record = containers.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        let inspect = self.docker.inspect_container(&record.container_id, None).await?;
        
        let state = match inspect.state.and_then(|s| s.status) {
            Some(bollard::container::ContainerStateStatusEnum::RUNNING) => InstanceState::Running,
            Some(bollard::container::ContainerStateStatusEnum::EXITED) => InstanceState::Stopped,
            _ => InstanceState::Failed { error: "Unknown state".into() },
        };
        
        Ok(HealthStatus {
            instance_id: instance_id.clone(),
            state,
            timestamp: chrono::Utc::now(),
            details: [
                ("container_id".into(), serde_json::json!(record.container_id)),
            ].into(),
        })
    }
    
    async fn metrics(&self, instance_id: &InstanceId) -> Result<RuntimeMetrics> {
        let containers = self.containers.read().await;
        let record = containers.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        // Get container stats
        let stats = self.docker.stats(&record.container_id, None).next().await;
        
        Ok(RuntimeMetrics {
            instance_id: instance_id.clone(),
            timestamp: chrono::Utc::now(),
            cpu_usage: None, // Extract from stats
            memory_bytes: None,
            gpu_memory_bytes: None,
            custom: HashMap::new(),
        })
    }
    
    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<ProductOutput>> {
        let containers = self.containers.read().await;
        let record = containers.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        Ok(record.product_tx.subscribe())
    }
    
    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> Result<broadcast::Receiver<AnomalyOutput>> {
        let containers = self.containers.read().await;
        let record = containers.get(instance_id)
            .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
        
        Ok(record.anomaly_tx.subscribe())
    }
    
    fn list_instances(&self) -> Vec<InstanceId> {
        vec![]
    }
}
```

### OrchestrationService

Coordinates deployment using HIVE primitives:

```rust
use crate::storage::{BlobStore, BlobRef};
use crate::protocol::{HiveEventPublisher, CapabilityPublisher};

/// Orchestration service coordinating software lifecycle
///
/// This is the main entry point for deploying and managing software.
/// It uses HIVE primitives (BlobStore, Events) and delegates runtime
/// specifics to RuntimeAdapter implementations.
pub struct OrchestrationService {
    /// HIVE blob store for artifact retrieval
    blob_store: Arc<dyn BlobStore>,
    
    /// HIVE event publisher for products/anomalies
    event_publisher: Arc<dyn HiveEventPublisher>,
    
    /// HIVE capability publisher for status
    capability_publisher: Arc<dyn CapabilityPublisher>,
    
    /// Available runtime adapters
    adapters: Vec<Arc<dyn RuntimeAdapter>>,
    
    /// Active instances
    instances: RwLock<HashMap<InstanceId, InstanceRecord>>,
}

struct InstanceRecord {
    artifact_type: ArtifactType,
    blob_ref: BlobRef,
    adapter: Arc<dyn RuntimeAdapter>,
    local_path: PathBuf,
    activated_at: chrono::DateTime<chrono::Utc>,
    capabilities: Vec<String>,
}

impl OrchestrationService {
    pub fn new(
        blob_store: Arc<dyn BlobStore>,
        event_publisher: Arc<dyn HiveEventPublisher>,
        capability_publisher: Arc<dyn CapabilityPublisher>,
    ) -> Self {
        Self {
            blob_store,
            event_publisher,
            capability_publisher,
            adapters: Vec::new(),
            instances: RwLock::new(HashMap::new()),
        }
    }
    
    /// Register a runtime adapter
    pub fn register_adapter(&mut self, adapter: Arc<dyn RuntimeAdapter>) {
        self.adapters.push(adapter);
    }
    
    /// Deploy an artifact locally
    ///
    /// 1. Fetches blob via BlobStore (HIVE primitive)
    /// 2. Selects appropriate RuntimeAdapter
    /// 3. Activates via adapter
    /// 4. Subscribes to products/anomalies
    /// 5. Publishes capability advertisement (HIVE primitive)
    pub async fn deploy(
        &self,
        blob_ref: BlobRef,
        artifact_type: ArtifactType,
        config: serde_json::Value,
        capabilities: Vec<String>,
    ) -> Result<InstanceId> {
        // Step 1: Fetch blob (HIVE primitive)
        let local_blob = self.blob_store.fetch(&blob_ref, |progress| {
            // Could emit progress events here
            tracing::debug!(?progress, "Blob fetch progress");
        }).await?;
        
        // Step 2: Find adapter
        let adapter = self.adapters
            .iter()
            .find(|a| a.can_handle(&artifact_type))
            .ok_or_else(|| anyhow::anyhow!(
                "No adapter for artifact type: {:?}",
                artifact_type
            ))?
            .clone();
        
        // Step 3: Activate
        let instance_id = adapter
            .activate(&artifact_type, &config, local_blob.path.clone())
            .await?;
        
        // Step 4: Subscribe to outputs
        self.start_monitoring(&instance_id, &adapter).await?;
        
        // Step 5: Publish capability (HIVE primitive)
        self.publish_capability(&instance_id, &capabilities, InstanceState::Running).await?;
        
        // Record instance
        {
            let mut instances = self.instances.write().await;
            instances.insert(instance_id.clone(), InstanceRecord {
                artifact_type,
                blob_ref,
                adapter,
                local_path: local_blob.path,
                activated_at: chrono::Utc::now(),
                capabilities,
            });
        }
        
        Ok(instance_id)
    }
    
    /// Undeploy an instance
    pub async fn undeploy(&self, instance_id: &InstanceId) -> Result<()> {
        let record = {
            let mut instances = self.instances.write().await;
            instances.remove(instance_id)
        };
        
        if let Some(record) = record {
            record.adapter.deactivate(instance_id).await?;
            self.publish_capability(instance_id, &record.capabilities, InstanceState::Stopped).await?;
        }
        
        Ok(())
    }
    
    /// Get health of all instances
    pub async fn health_all(&self) -> HashMap<InstanceId, HealthStatus> {
        let instances = self.instances.read().await;
        let mut results = HashMap::new();
        
        for (id, record) in instances.iter() {
            if let Ok(health) = record.adapter.health(id).await {
                results.insert(id.clone(), health);
            }
        }
        
        results
    }
    
    async fn start_monitoring(
        &self,
        instance_id: &InstanceId,
        adapter: &Arc<dyn RuntimeAdapter>,
    ) -> Result<()> {
        // Subscribe to products and route through HIVE events
        let mut products = adapter.subscribe_products(instance_id).await?;
        let event_pub = self.event_publisher.clone();
        let iid = instance_id.clone();
        
        tokio::spawn(async move {
            while let Ok(product) = products.recv().await {
                // Convert ProductOutput to HiveEvent and publish
                let hive_event = HiveEvent {
                    event_class: EventClass::Product,
                    event_type: product.product_type.clone(),
                    source_instance_id: Some(iid.0.clone()),
                    payload: product.payload,
                    routing: AggregationPolicy {
                        propagate_full: product.routing.propagate_full,
                        propagate_summary: product.routing.propagate_summary,
                        priority: match product.routing.priority {
                            EventPriority::Critical => hive::EventPriority::Critical,
                            EventPriority::High => hive::EventPriority::High,
                            EventPriority::Normal => hive::EventPriority::Normal,
                            EventPriority::Low => hive::EventPriority::Low,
                            EventPriority::LocalOnly => hive::EventPriority::LocalOnly,
                        },
                        ttl_seconds: product.routing.ttl_seconds,
                    },
                    ..Default::default()
                };
                
                let _ = event_pub.publish(hive_event).await;
            }
        });
        
        // Subscribe to anomalies (always propagate)
        let mut anomalies = adapter.subscribe_anomalies(instance_id).await?;
        let event_pub = self.event_publisher.clone();
        let iid = instance_id.clone();
        
        tokio::spawn(async move {
            while let Ok(anomaly) = anomalies.recv().await {
                let priority = match anomaly.severity {
                    AnomalySeverity::Critical => hive::EventPriority::Critical,
                    AnomalySeverity::Error => hive::EventPriority::High,
                    _ => hive::EventPriority::Normal,
                };
                
                let hive_event = HiveEvent {
                    event_class: EventClass::Anomaly,
                    event_type: anomaly.anomaly_type.clone(),
                    source_instance_id: Some(iid.0.clone()),
                    payload: serde_json::json!({
                        "severity": format!("{:?}", anomaly.severity),
                        "description": anomaly.description,
                        "evidence": anomaly.evidence,
                    }),
                    routing: AggregationPolicy {
                        propagate_full: true,
                        priority,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                
                let _ = event_pub.publish(hive_event).await;
            }
        });
        
        Ok(())
    }
    
    async fn publish_capability(
        &self,
        instance_id: &InstanceId,
        capabilities: &[String],
        state: InstanceState,
    ) -> Result<()> {
        // Build HIVE CapabilityAdvertisement
        let advertisement = CapabilityAdvertisement {
            capabilities: capabilities.iter().map(|c| Capability {
                capability_type: "software".into(),
                capability_id: c.clone(),
                state: match &state {
                    InstanceState::Running => CapabilityState::Available,
                    InstanceState::Degraded { .. } => CapabilityState::Degraded,
                    _ => CapabilityState::Offline,
                },
                ..Default::default()
            }).collect(),
            ..Default::default()
        };
        
        self.capability_publisher.publish(advertisement).await
    }
}
```

## Application Schemas

Define your own product schemas that flow through `HiveEvent.payload`:

```rust
/// Detection product (example application schema)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectionProduct {
    pub object_type: String,
    pub confidence: f32,
    pub bounding_box: Option<BoundingBox>,
    pub geo_location: Option<GeoLocation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: Option<f64>,
    pub accuracy_m: Option<f64>,
}

/// Classification product (example application schema)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassificationProduct {
    pub results: Vec<ClassificationResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub class_id: String,
    pub class_name: String,
    pub confidence: f32,
}

/// Route planning product (example application schema)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutePlanProduct {
    pub waypoints: Vec<Waypoint>,
    pub total_distance_m: f64,
    pub estimated_duration_s: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Waypoint {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: Option<f64>,
    pub speed_mps: Option<f64>,
}
```

## Integration with HIVE Protocol

### Using HIVE Primitives

```rust
// Example: Full deployment flow using HIVE primitives

async fn deploy_model(
    orchestration: &OrchestrationService,
    hive_storage: &dyn StorageBackend,  // HIVE document storage
    blob_store: &dyn BlobStore,          // HIVE blob storage (ADR-025)
) -> Result<()> {
    // 1. Model blob already stored (maybe by C2 node)
    let model_ref = BlobRef {
        hash: BlobHash::sha256("abc123..."),
        size_bytes: 500_000_000,
        metadata: [("name".into(), "YOLOv8".into())].into(),
    };
    
    // 2. Deploy locally
    let instance_id = orchestration.deploy(
        model_ref,
        ArtifactType::OnnxModel {
            execution_providers: vec!["CUDAExecutionProvider".into()],
            signature: None,
        },
        serde_json::json!({}),
        vec!["target_recognition".into()],
    ).await?;
    
    // 3. Capability is automatically advertised via HIVE
    // 4. Products/anomalies automatically flow via HiveEvent
    
    Ok(())
}
```

### Receiving Deployment Commands

```rust
// Example: Handling DeploymentDirective from HIVE

async fn handle_deployment_directive(
    directive: DeploymentDirective,  // HIVE protocol message
    orchestration: &OrchestrationService,
) -> Result<()> {
    // Extract application-specific config from directive
    let artifact_type: ArtifactType = serde_json::from_value(
        directive.config.get("artifact_type").cloned().unwrap_or_default()
    )?;
    
    let capabilities: Vec<String> = serde_json::from_value(
        directive.config.get("capabilities").cloned().unwrap_or_default()
    )?;
    
    // Deploy using orchestration service
    orchestration.deploy(
        directive.artifact,  // BlobRef from HIVE
        artifact_type,
        directive.config,
        capabilities,
    ).await?;
    
    Ok(())
}
```

## Extending This Reference

### Adding a New Runtime

1. Implement `RuntimeAdapter` trait
2. Register with `OrchestrationService`
3. Define `ArtifactType` variant

### Adding New Product Types

1. Define Rust struct with Serialize/Deserialize
2. Publish via `ProductOutput` with appropriate `product_type`
3. Consumers parse based on `product_type` field

### Custom Orchestration Logic

Fork `OrchestrationService` or wrap it:
- Add priority scheduling
- Implement rollback logic
- Add resource-based scheduling
- Integrate with external systems (K8s, Nomad)

## What This Reference Does NOT Cover

- **Distributed orchestration**: Multi-node deployment coordination
- **Scheduling**: Resource-aware placement decisions  
- **Service mesh**: Inter-service communication
- **Secrets management**: Credential injection
- **Persistent storage**: Volume management for containers

These are valid extensions you may need - this reference focuses on the core lifecycle.

## References

### HIVE Protocol ADRs
- ADR-012: Schema Definition (CapabilityAdvertisement, HiveEvent)
- ADR-025: Blob Transfer Protocol (BlobStore, BlobRef)

### Runtime Technologies
- [ONNX Runtime](https://onnxruntime.ai/)
- [Bollard (Docker API)](https://docs.rs/bollard/)
- [containerd](https://containerd.io/)

### Related Patterns
- [Sidecar Pattern](https://docs.microsoft.com/en-us/azure/architecture/patterns/sidecar)
- [Kubernetes Operators](https://kubernetes.io/docs/concepts/extend-kubernetes/operator/)

---

**This reference implementation demonstrates how to build software orchestration on HIVE Protocol primitives. Adopt, adapt, or replace as needed for your use case.**
