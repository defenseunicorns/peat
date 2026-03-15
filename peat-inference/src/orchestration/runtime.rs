//! Runtime Adapter - Issue #177 Phase 2 / ADR-026
//!
//! Provides the `RuntimeAdapter` trait for artifact-type-specific activation
//! and lifecycle management.
//!
//! ## Overview
//!
//! Different artifact types (ONNX models, containers, native binaries) require
//! different runtime handling. The `RuntimeAdapter` trait abstracts this:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  OrchestrationService                                            │
//! │  ┌─────────────────────────────────────────────────────────────┐│
//! │  │                    RuntimeAdapter trait                      ││
//! │  └─────────────────────────────────────────────────────────────┘│
//! │       ▲              ▲                ▲              ▲          │
//! │       │              │                │              │          │
//! │  ┌────┴────┐   ┌─────┴─────┐   ┌──────┴──────┐   ┌───┴───┐     │
//! │  │  Onnx   │   │ Container │   │   Process   │   │ Your  │     │
//! │  │ Adapter │   │  Adapter  │   │   Adapter   │   │Adapter│     │
//! │  └─────────┘   └───────────┘   └─────────────┘   └───────┘     │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::orchestration::runtime::{RuntimeAdapter, ArtifactType, InstanceId};
//!
//! // Implement for your runtime
//! struct MyAdapter { /* ... */ }
//!
//! #[async_trait]
//! impl RuntimeAdapter for MyAdapter {
//!     fn name(&self) -> &str { "my_adapter" }
//!     fn can_handle(&self, artifact_type: &ArtifactType) -> bool { /* ... */ }
//!     // ... implement other methods
//! }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;
use tokio::sync::broadcast;

// ============================================================================
// Errors
// ============================================================================

/// Errors from runtime adapter operations
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Instance not found: {0}")]
    InstanceNotFound(String),

    #[error("Activation failed: {0}")]
    ActivationFailed(String),

    #[error("Deactivation failed: {0}")]
    DeactivationFailed(String),

    #[error("Unsupported artifact type: {0}")]
    UnsupportedArtifact(String),

    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Result type for runtime operations
pub type RuntimeResult<T> = Result<T, RuntimeError>;

// ============================================================================
// Instance Types
// ============================================================================

/// Unique identifier for a running instance
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(pub String);

impl InstanceId {
    /// Create a new instance ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random instance ID
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// State of a running instance
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceState {
    /// Instance is starting up
    Starting,
    /// Instance is running normally
    Running,
    /// Instance is running but degraded
    Degraded { reason: String },
    /// Instance has stopped
    Stopped,
    /// Instance has failed
    Failed { error: String },
}

impl InstanceState {
    /// Check if the instance is healthy (Running or Starting)
    pub fn is_healthy(&self) -> bool {
        matches!(self, InstanceState::Running | InstanceState::Starting)
    }

    /// Check if the instance is operational (can process requests)
    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            InstanceState::Running | InstanceState::Degraded { .. }
        )
    }
}

/// Health status of a running instance
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Instance identifier
    pub instance_id: InstanceId,
    /// Current state
    pub state: InstanceState,
    /// Status timestamp
    pub timestamp: DateTime<Utc>,
    /// Additional details
    #[serde(default)]
    pub details: HashMap<String, serde_json::Value>,
}

impl HealthStatus {
    /// Create a new health status
    pub fn new(instance_id: InstanceId, state: InstanceState) -> Self {
        Self {
            instance_id,
            state,
            timestamp: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Add a detail to the status
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

/// Runtime metrics for an instance
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    /// Instance identifier
    pub instance_id: InstanceId,
    /// Metrics timestamp
    pub timestamp: DateTime<Utc>,
    /// CPU usage (0.0 - 1.0)
    pub cpu_usage: Option<f64>,
    /// Memory usage in bytes
    pub memory_bytes: Option<u64>,
    /// GPU memory usage in bytes
    pub gpu_memory_bytes: Option<u64>,
    /// Custom metrics
    #[serde(default)]
    pub custom: HashMap<String, f64>,
}

impl RuntimeMetrics {
    /// Create new runtime metrics
    pub fn new(instance_id: InstanceId) -> Self {
        Self {
            instance_id,
            timestamp: Utc::now(),
            cpu_usage: None,
            memory_bytes: None,
            gpu_memory_bytes: None,
            custom: HashMap::new(),
        }
    }

    /// Set CPU usage
    pub fn with_cpu(mut self, usage: f64) -> Self {
        self.cpu_usage = Some(usage);
        self
    }

    /// Set memory usage
    pub fn with_memory(mut self, bytes: u64) -> Self {
        self.memory_bytes = Some(bytes);
        self
    }

    /// Add custom metric
    pub fn with_custom(mut self, key: impl Into<String>, value: f64) -> Self {
        self.custom.insert(key.into(), value);
        self
    }
}

// ============================================================================
// Product and Anomaly Outputs
// ============================================================================

/// Priority for event routing
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPriority {
    /// Critical - always propagate immediately
    Critical,
    /// High priority
    High,
    /// Normal priority
    #[default]
    Normal,
    /// Low priority
    Low,
    /// Local only - don't propagate
    LocalOnly,
}

/// Hints for routing product outputs through Peat
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoutingHint {
    /// Propagate full payload
    pub propagate_full: bool,
    /// Propagate summary only
    pub propagate_summary: bool,
    /// Event priority
    pub priority: EventPriority,
    /// Time-to-live in seconds
    pub ttl_seconds: u32,
}

impl RoutingHint {
    /// Create routing hint for local-only events
    pub fn local_only() -> Self {
        Self {
            propagate_full: false,
            propagate_summary: false,
            priority: EventPriority::LocalOnly,
            ttl_seconds: 0,
        }
    }

    /// Create routing hint for full propagation
    pub fn propagate() -> Self {
        Self {
            propagate_full: true,
            propagate_summary: false,
            priority: EventPriority::Normal,
            ttl_seconds: 300,
        }
    }

    /// Create routing hint for critical events
    pub fn critical() -> Self {
        Self {
            propagate_full: true,
            propagate_summary: true,
            priority: EventPriority::Critical,
            ttl_seconds: 600,
        }
    }
}

/// Output product from software (detection, classification, etc.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductOutput {
    /// Source instance
    pub instance_id: InstanceId,
    /// Output timestamp
    pub timestamp: DateTime<Utc>,
    /// Product type (e.g., "detection", "classification")
    pub product_type: String,
    /// Product payload (application-defined)
    pub payload: serde_json::Value,
    /// Routing hints
    pub routing: RoutingHint,
}

impl ProductOutput {
    /// Create a new product output
    pub fn new(
        instance_id: InstanceId,
        product_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            instance_id,
            timestamp: Utc::now(),
            product_type: product_type.into(),
            payload,
            routing: RoutingHint::default(),
        }
    }

    /// Set routing hints
    pub fn with_routing(mut self, routing: RoutingHint) -> Self {
        self.routing = routing;
        self
    }
}

/// Severity of an anomaly
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    /// Informational
    Info,
    /// Warning - may need attention
    Warning,
    /// Error - requires attention
    Error,
    /// Critical - immediate action required
    Critical,
}

/// Anomaly detected by runtime
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyOutput {
    /// Source instance
    pub instance_id: InstanceId,
    /// Detection timestamp
    pub timestamp: DateTime<Utc>,
    /// Anomaly type
    pub anomaly_type: String,
    /// Severity
    pub severity: AnomalySeverity,
    /// Human-readable description
    pub description: String,
    /// Supporting evidence
    pub evidence: Option<serde_json::Value>,
}

impl AnomalyOutput {
    /// Create a new anomaly output
    pub fn new(
        instance_id: InstanceId,
        anomaly_type: impl Into<String>,
        severity: AnomalySeverity,
        description: impl Into<String>,
    ) -> Self {
        Self {
            instance_id,
            timestamp: Utc::now(),
            anomaly_type: anomaly_type.into(),
            severity,
            description: description.into(),
            evidence: None,
        }
    }

    /// Add evidence
    pub fn with_evidence(mut self, evidence: serde_json::Value) -> Self {
        self.evidence = Some(evidence);
        self
    }
}

// ============================================================================
// Artifact Types
// ============================================================================

/// Port mapping for containers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortMapping {
    /// Container port
    pub container_port: u16,
    /// Host port (None = auto-assign)
    pub host_port: Option<u16>,
    /// Protocol (tcp, udp)
    pub protocol: String,
}

/// Container runtime
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContainerRuntime {
    /// Docker
    Docker,
    /// Podman
    Podman,
    /// containerd
    Containerd,
}

/// Model input/output signature
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelSignature {
    /// Input tensor names and shapes
    pub inputs: HashMap<String, Vec<i64>>,
    /// Output tensor names and shapes
    pub outputs: HashMap<String, Vec<i64>>,
}

/// Artifact type - defines what kind of software artifact this is
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

impl ArtifactType {
    /// Create an ONNX model artifact type
    pub fn onnx(execution_providers: Vec<String>) -> Self {
        Self::OnnxModel {
            execution_providers,
            signature: None,
        }
    }

    /// Create an ONNX model artifact with CUDA provider
    pub fn onnx_cuda() -> Self {
        Self::onnx(vec![
            "CUDAExecutionProvider".into(),
            "CPUExecutionProvider".into(),
        ])
    }

    /// Create an ONNX model artifact with TensorRT provider
    pub fn onnx_tensorrt() -> Self {
        Self::onnx(vec![
            "TensorrtExecutionProvider".into(),
            "CUDAExecutionProvider".into(),
            "CPUExecutionProvider".into(),
        ])
    }

    /// Create a Docker container artifact type
    pub fn docker(env: HashMap<String, String>) -> Self {
        Self::Container {
            runtime: ContainerRuntime::Docker,
            ports: Vec::new(),
            env,
        }
    }

    /// Create a native binary artifact type
    pub fn native(arch: impl Into<String>, args: Vec<String>) -> Self {
        Self::NativeBinary {
            arch: arch.into(),
            args,
        }
    }

    /// Get a short type name
    pub fn type_name(&self) -> &str {
        match self {
            ArtifactType::OnnxModel { .. } => "onnx",
            ArtifactType::Container { .. } => "container",
            ArtifactType::NativeBinary { .. } => "native",
            ArtifactType::ConfigPackage { .. } => "config",
            ArtifactType::WasmModule { .. } => "wasm",
        }
    }
}

// ============================================================================
// RuntimeAdapter Trait
// ============================================================================

/// Runtime adapter trait
///
/// Implement this for each artifact type you want to support.
/// The orchestration service uses this trait to manage artifacts
/// without knowing the runtime-specific details.
///
/// # Example Implementations
///
/// - `SimulatedAdapter` - For testing (included)
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
    ) -> RuntimeResult<InstanceId>;

    /// Deactivate a running instance
    ///
    /// Should gracefully stop the instance and release resources.
    async fn deactivate(&self, instance_id: &InstanceId) -> RuntimeResult<()>;

    /// Get current health status
    async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus>;

    /// Get current metrics
    async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics>;

    /// Subscribe to product outputs from this instance
    ///
    /// Products are runtime-specific outputs (detections, classifications, etc.)
    /// The orchestration service will route these through Peat events.
    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<ProductOutput>>;

    /// Subscribe to anomaly outputs from this instance
    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<AnomalyOutput>>;

    /// Attempt hot-reload without full restart
    ///
    /// Returns Ok(true) if successful, Ok(false) if not supported.
    async fn hot_reload(
        &self,
        _instance_id: &InstanceId,
        _new_artifact_type: &ArtifactType,
        _new_config: &serde_json::Value,
        _new_path: PathBuf,
    ) -> RuntimeResult<bool> {
        // Default: not supported
        Ok(false)
    }

    /// List all instances managed by this adapter
    fn list_instances(&self) -> Vec<InstanceId>;
}

// ============================================================================
// Simulated Adapter (for testing)
// ============================================================================

use std::sync::Arc;
use tokio::sync::RwLock;

/// Record for a simulated instance
struct SimulatedInstance {
    artifact_type: ArtifactType,
    state: InstanceState,
    activated_at: DateTime<Utc>,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
}

/// Simulated runtime adapter for testing
///
/// This adapter doesn't actually run anything - it simulates the lifecycle
/// for testing purposes.
pub struct SimulatedAdapter {
    name: String,
    instances: Arc<RwLock<HashMap<InstanceId, SimulatedInstance>>>,
    supported_types: Vec<String>,
}

impl SimulatedAdapter {
    /// Create a new simulated adapter
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instances: Arc::new(RwLock::new(HashMap::new())),
            supported_types: vec!["onnx".into(), "native".into()],
        }
    }

    /// Set which artifact types this adapter supports
    pub fn with_supported_types(mut self, types: Vec<String>) -> Self {
        self.supported_types = types;
        self
    }

    /// Simulate publishing a product
    pub async fn publish_product(&self, instance_id: &InstanceId, product: ProductOutput) -> bool {
        let instances = self.instances.read().await;
        if let Some(instance) = instances.get(instance_id) {
            instance.product_tx.send(product).is_ok()
        } else {
            false
        }
    }

    /// Simulate publishing an anomaly
    pub async fn publish_anomaly(&self, instance_id: &InstanceId, anomaly: AnomalyOutput) -> bool {
        let instances = self.instances.read().await;
        if let Some(instance) = instances.get(instance_id) {
            instance.anomaly_tx.send(anomaly).is_ok()
        } else {
            false
        }
    }

    /// Set instance state (for testing state transitions)
    pub async fn set_state(&self, instance_id: &InstanceId, state: InstanceState) -> bool {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(instance_id) {
            instance.state = state;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl RuntimeAdapter for SimulatedAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn can_handle(&self, artifact_type: &ArtifactType) -> bool {
        self.supported_types
            .contains(&artifact_type.type_name().to_string())
    }

    async fn activate(
        &self,
        artifact_type: &ArtifactType,
        _config: &serde_json::Value,
        _local_path: PathBuf,
    ) -> RuntimeResult<InstanceId> {
        let instance_id = InstanceId::generate();
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);

        let instance = SimulatedInstance {
            artifact_type: artifact_type.clone(),
            state: InstanceState::Running,
            activated_at: Utc::now(),
            product_tx,
            anomaly_tx,
        };

        self.instances
            .write()
            .await
            .insert(instance_id.clone(), instance);

        Ok(instance_id)
    }

    async fn deactivate(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        self.instances
            .write()
            .await
            .remove(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(())
    }

    async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus> {
        let instances = self.instances.read().await;
        let instance = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        Ok(
            HealthStatus::new(instance_id.clone(), instance.state.clone()).with_detail(
                "artifact_type",
                serde_json::json!(instance.artifact_type.type_name()),
            ),
        )
    }

    async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics> {
        let instances = self.instances.read().await;
        if !instances.contains_key(instance_id) {
            return Err(RuntimeError::InstanceNotFound(instance_id.to_string()));
        }

        Ok(RuntimeMetrics::new(instance_id.clone())
            .with_cpu(0.1)
            .with_memory(100_000_000)
            .with_custom("simulated", 1.0))
    }

    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<ProductOutput>> {
        let instances = self.instances.read().await;
        let instance = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(instance.product_tx.subscribe())
    }

    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<AnomalyOutput>> {
        let instances = self.instances.read().await;
        let instance = instances
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(instance.anomaly_tx.subscribe())
    }

    fn list_instances(&self) -> Vec<InstanceId> {
        // Note: This is a sync method, so we can't use await
        // In production, you'd want a different design
        Vec::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_id() {
        let id1 = InstanceId::new("test-1");
        let id2 = InstanceId::generate();

        assert_eq!(id1.0, "test-1");
        assert!(!id2.0.is_empty());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_instance_state() {
        assert!(InstanceState::Running.is_healthy());
        assert!(InstanceState::Starting.is_healthy());
        assert!(!InstanceState::Stopped.is_healthy());

        assert!(InstanceState::Running.is_operational());
        assert!(InstanceState::Degraded {
            reason: "test".into()
        }
        .is_operational());
        assert!(!InstanceState::Stopped.is_operational());
    }

    #[test]
    fn test_health_status() {
        let status = HealthStatus::new(InstanceId::new("test"), InstanceState::Running)
            .with_detail("version", serde_json::json!("1.0.0"));

        assert!(status.state.is_healthy());
        assert!(status.details.contains_key("version"));
    }

    #[test]
    fn test_runtime_metrics() {
        let metrics = RuntimeMetrics::new(InstanceId::new("test"))
            .with_cpu(0.5)
            .with_memory(1024)
            .with_custom("inference_count", 100.0);

        assert_eq!(metrics.cpu_usage, Some(0.5));
        assert_eq!(metrics.memory_bytes, Some(1024));
        assert_eq!(metrics.custom.get("inference_count"), Some(&100.0));
    }

    #[test]
    fn test_artifact_type_onnx() {
        let artifact = ArtifactType::onnx_cuda();
        assert_eq!(artifact.type_name(), "onnx");

        if let ArtifactType::OnnxModel {
            execution_providers,
            ..
        } = artifact
        {
            assert!(execution_providers.contains(&"CUDAExecutionProvider".to_string()));
        } else {
            panic!("Expected OnnxModel");
        }
    }

    #[test]
    fn test_artifact_type_container() {
        let mut env = HashMap::new();
        env.insert("API_KEY".into(), "secret".into());

        let artifact = ArtifactType::docker(env);
        assert_eq!(artifact.type_name(), "container");
    }

    #[test]
    fn test_artifact_type_native() {
        let artifact = ArtifactType::native("aarch64", vec!["--config".into(), "test.yaml".into()]);
        assert_eq!(artifact.type_name(), "native");
    }

    #[test]
    fn test_routing_hint() {
        let local = RoutingHint::local_only();
        assert_eq!(local.priority, EventPriority::LocalOnly);
        assert!(!local.propagate_full);

        let critical = RoutingHint::critical();
        assert_eq!(critical.priority, EventPriority::Critical);
        assert!(critical.propagate_full);
    }

    #[test]
    fn test_product_output() {
        let product = ProductOutput::new(
            InstanceId::new("test"),
            "detection",
            serde_json::json!({"object": "person", "confidence": 0.95}),
        )
        .with_routing(RoutingHint::propagate());

        assert_eq!(product.product_type, "detection");
        assert!(product.routing.propagate_full);
    }

    #[test]
    fn test_anomaly_output() {
        let anomaly = AnomalyOutput::new(
            InstanceId::new("test"),
            "performance_degradation",
            AnomalySeverity::Warning,
            "FPS dropped below threshold",
        )
        .with_evidence(serde_json::json!({"current_fps": 15, "threshold": 30}));

        assert_eq!(anomaly.severity, AnomalySeverity::Warning);
        assert!(anomaly.evidence.is_some());
    }

    #[tokio::test]
    async fn test_simulated_adapter_lifecycle() {
        let adapter = SimulatedAdapter::new("test_adapter");

        // Activate
        let instance_id = adapter
            .activate(
                &ArtifactType::onnx_cuda(),
                &serde_json::json!({}),
                PathBuf::from("/tmp/model.onnx"),
            )
            .await
            .unwrap();

        // Health check
        let health = adapter.health(&instance_id).await.unwrap();
        assert!(health.state.is_healthy());

        // Metrics
        let metrics = adapter.metrics(&instance_id).await.unwrap();
        assert!(metrics.cpu_usage.is_some());

        // Deactivate
        adapter.deactivate(&instance_id).await.unwrap();

        // Should fail after deactivation
        assert!(adapter.health(&instance_id).await.is_err());
    }

    #[tokio::test]
    async fn test_simulated_adapter_products() {
        let adapter = SimulatedAdapter::new("test_adapter");

        let instance_id = adapter
            .activate(
                &ArtifactType::onnx_cuda(),
                &serde_json::json!({}),
                PathBuf::from("/tmp/model.onnx"),
            )
            .await
            .unwrap();

        // Subscribe to products
        let mut rx = adapter.subscribe_products(&instance_id).await.unwrap();

        // Publish a product
        let product = ProductOutput::new(
            instance_id.clone(),
            "detection",
            serde_json::json!({"test": true}),
        );
        adapter.publish_product(&instance_id, product).await;

        // Should receive the product
        let received = rx.try_recv().unwrap();
        assert_eq!(received.product_type, "detection");
    }

    #[tokio::test]
    async fn test_simulated_adapter_state_changes() {
        let adapter = SimulatedAdapter::new("test_adapter");

        let instance_id = adapter
            .activate(
                &ArtifactType::onnx_cuda(),
                &serde_json::json!({}),
                PathBuf::from("/tmp/model.onnx"),
            )
            .await
            .unwrap();

        // Initially running
        let health = adapter.health(&instance_id).await.unwrap();
        assert_eq!(health.state, InstanceState::Running);

        // Degrade
        adapter
            .set_state(
                &instance_id,
                InstanceState::Degraded {
                    reason: "High latency".into(),
                },
            )
            .await;

        let health = adapter.health(&instance_id).await.unwrap();
        assert!(matches!(health.state, InstanceState::Degraded { .. }));
        assert!(health.state.is_operational());
    }

    #[test]
    fn test_simulated_adapter_can_handle() {
        let adapter = SimulatedAdapter::new("test")
            .with_supported_types(vec!["onnx".into(), "container".into()]);

        assert!(adapter.can_handle(&ArtifactType::onnx_cuda()));
        assert!(adapter.can_handle(&ArtifactType::docker(HashMap::new())));
        assert!(!adapter.can_handle(&ArtifactType::native("x86_64", vec![])));
    }
}
