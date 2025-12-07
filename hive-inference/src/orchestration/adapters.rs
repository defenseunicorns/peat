//! Example Runtime Adapters - Issue #177 Phase 2 / ADR-026
//!
//! This module provides example implementations of the `RuntimeAdapter` trait.
//! These are stub implementations that demonstrate the expected structure and
//! behavior without requiring external dependencies.
//!
//! ## Available Adapters
//!
//! - **`OnnxRuntimeAdapter`** - For ONNX model inference
//! - **`ContainerAdapter`** - For Docker/Podman containers
//! - **`ProcessAdapter`** - For native executables
//!
//! ## Production Usage
//!
//! For production use, you would implement these adapters with real dependencies:
//! - ONNX: Use the `ort` crate for ONNX Runtime
//! - Containers: Use the `bollard` crate for Docker API
//! - Processes: Use `tokio::process` for async process management
//!
//! These stub implementations can be used for:
//! - Testing orchestration logic
//! - Development without runtime dependencies
//! - Understanding the expected adapter behavior

use super::runtime::{
    AnomalyOutput, ArtifactType, HealthStatus, InstanceId, InstanceState, ProductOutput,
    RoutingHint, RuntimeAdapter, RuntimeError, RuntimeMetrics, RuntimeResult,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

// ============================================================================
// ONNX Runtime Adapter
// ============================================================================

/// Metrics tracked for ONNX inference sessions
#[derive(Debug, Default)]
struct OnnxMetrics {
    inference_count: u64,
    total_latency_ms: f64,
    last_inference: Option<chrono::DateTime<chrono::Utc>>,
}

/// Session record for a loaded ONNX model
struct OnnxSession {
    model_path: PathBuf,
    execution_providers: Vec<String>,
    state: InstanceState,
    loaded_at: chrono::DateTime<chrono::Utc>,
    metrics: OnnxMetrics,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
}

/// ONNX Runtime Adapter
///
/// This is a stub implementation that demonstrates the expected behavior
/// for an ONNX model adapter. In production, you would use the `ort` crate.
///
/// ## Example Usage
///
/// ```rust,ignore
/// use hive_inference::orchestration::adapters::OnnxRuntimeAdapter;
/// use hive_inference::orchestration::runtime::ArtifactType;
///
/// let adapter = OnnxRuntimeAdapter::new();
///
/// // Activate a model
/// let instance = adapter.activate(
///     &ArtifactType::onnx_cuda(),
///     &serde_json::json!({"batch_size": 1}),
///     PathBuf::from("/models/yolov8.onnx"),
/// ).await?;
///
/// // Run inference (in real impl)
/// // let outputs = adapter.infer(&instance, inputs).await?;
///
/// // Deactivate when done
/// adapter.deactivate(&instance).await?;
/// ```
pub struct OnnxRuntimeAdapter {
    sessions: Arc<RwLock<HashMap<InstanceId, OnnxSession>>>,
}

impl OnnxRuntimeAdapter {
    /// Create a new ONNX Runtime adapter
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Simulate running inference on a model
    ///
    /// In a real implementation, this would:
    /// 1. Get the ONNX session
    /// 2. Prepare input tensors
    /// 3. Run inference
    /// 4. Return output tensors
    pub async fn infer(
        &self,
        instance_id: &InstanceId,
        _inputs: serde_json::Value,
    ) -> RuntimeResult<serde_json::Value> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        // Simulate inference latency
        let start = std::time::Instant::now();

        // In real impl: run inference here
        // let outputs = session.run(inputs)?;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Update metrics
        session.metrics.inference_count += 1;
        session.metrics.total_latency_ms += latency_ms;
        session.metrics.last_inference = Some(Utc::now());

        // Simulated output
        Ok(serde_json::json!({
            "simulated": true,
            "inference_count": session.metrics.inference_count,
            "latency_ms": latency_ms
        }))
    }

    /// Publish a detection product
    ///
    /// Call this after processing inference outputs to publish results
    /// through the HIVE event system.
    pub async fn publish_detection(
        &self,
        instance_id: &InstanceId,
        detection: serde_json::Value,
    ) -> RuntimeResult<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        let product = ProductOutput::new(instance_id.clone(), "detection", detection)
            .with_routing(RoutingHint::propagate());

        let _ = session.product_tx.send(product);
        Ok(())
    }
}

impl Default for OnnxRuntimeAdapter {
    fn default() -> Self {
        Self::new()
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
        _config: &serde_json::Value,
        local_path: PathBuf,
    ) -> RuntimeResult<InstanceId> {
        let ArtifactType::OnnxModel {
            execution_providers,
            ..
        } = artifact_type
        else {
            return Err(RuntimeError::UnsupportedArtifact(
                "Expected ONNX model".into(),
            ));
        };

        info!(
            path = %local_path.display(),
            providers = ?execution_providers,
            "Loading ONNX model (stub)"
        );

        // In real impl: Load model with ort crate
        // let session = SessionBuilder::new(&env)?
        //     .with_execution_providers(providers)?
        //     .with_model_from_file(&local_path)?;

        let instance_id = InstanceId::generate();
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);

        let session = OnnxSession {
            model_path: local_path,
            execution_providers: execution_providers.clone(),
            state: InstanceState::Running,
            loaded_at: Utc::now(),
            metrics: OnnxMetrics::default(),
            product_tx,
            anomaly_tx,
        };

        self.sessions
            .write()
            .await
            .insert(instance_id.clone(), session);

        info!(instance_id = %instance_id, "ONNX model loaded");
        Ok(instance_id)
    }

    async fn deactivate(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        let session = self
            .sessions
            .write()
            .await
            .remove(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        info!(
            instance_id = %instance_id,
            inference_count = session.metrics.inference_count,
            "ONNX model unloaded"
        );

        Ok(())
    }

    async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        Ok(
            HealthStatus::new(instance_id.clone(), session.state.clone())
                .with_detail(
                    "model_path",
                    serde_json::json!(session.model_path.display().to_string()),
                )
                .with_detail(
                    "execution_providers",
                    serde_json::json!(session.execution_providers),
                )
                .with_detail(
                    "inference_count",
                    serde_json::json!(session.metrics.inference_count),
                ),
        )
    }

    async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        let avg_latency = if session.metrics.inference_count > 0 {
            session.metrics.total_latency_ms / session.metrics.inference_count as f64
        } else {
            0.0
        };

        Ok(RuntimeMetrics::new(instance_id.clone())
            .with_custom("inference_count", session.metrics.inference_count as f64)
            .with_custom("avg_latency_ms", avg_latency)
            .with_custom("total_latency_ms", session.metrics.total_latency_ms))
    }

    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<ProductOutput>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(session.product_tx.subscribe())
    }

    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<AnomalyOutput>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(session.anomaly_tx.subscribe())
    }

    async fn hot_reload(
        &self,
        instance_id: &InstanceId,
        new_artifact_type: &ArtifactType,
        _new_config: &serde_json::Value,
        new_path: PathBuf,
    ) -> RuntimeResult<bool> {
        // ONNX models can support hot-reload by loading the new model
        // and swapping the session pointer atomically
        let ArtifactType::OnnxModel {
            execution_providers,
            ..
        } = new_artifact_type
        else {
            return Ok(false);
        };

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        info!(
            instance_id = %instance_id,
            new_path = %new_path.display(),
            "Hot-reloading ONNX model (stub)"
        );

        // In real impl: Load new model, swap session, release old
        session.model_path = new_path;
        session.execution_providers = execution_providers.clone();
        session.loaded_at = Utc::now();

        Ok(true)
    }

    fn list_instances(&self) -> Vec<InstanceId> {
        // Note: sync method limitation - can't await
        Vec::new()
    }
}

// ============================================================================
// Container Adapter
// ============================================================================

/// Container state information
struct ContainerRecord {
    container_id: String,
    image_path: PathBuf,
    runtime: super::runtime::ContainerRuntime,
    env: HashMap<String, String>,
    state: InstanceState,
    started_at: chrono::DateTime<chrono::Utc>,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
}

/// Container Adapter
///
/// This is a stub implementation that demonstrates the expected behavior
/// for a container adapter. In production, you would use the `bollard` crate
/// for Docker or similar for other container runtimes.
///
/// ## Example Usage
///
/// ```rust,ignore
/// use hive_inference::orchestration::adapters::ContainerAdapter;
/// use hive_inference::orchestration::runtime::ArtifactType;
///
/// let adapter = ContainerAdapter::new();
///
/// // Activate a container
/// let instance = adapter.activate(
///     &ArtifactType::docker(env),
///     &serde_json::json!({}),
///     PathBuf::from("/images/my-service.tar"),
/// ).await?;
///
/// // Container is now running
///
/// // Deactivate when done
/// adapter.deactivate(&instance).await?;
/// ```
pub struct ContainerAdapter {
    containers: Arc<RwLock<HashMap<InstanceId, ContainerRecord>>>,
}

impl ContainerAdapter {
    /// Create a new container adapter
    pub fn new() -> Self {
        Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get container logs (stub)
    pub async fn logs(&self, instance_id: &InstanceId) -> RuntimeResult<String> {
        let containers = self.containers.read().await;
        if !containers.contains_key(instance_id) {
            return Err(RuntimeError::InstanceNotFound(instance_id.to_string()));
        }

        // In real impl: docker.logs(&container_id, options).await
        Ok(format!(
            "[{}] Container started successfully\n[{}] Running...",
            instance_id, instance_id
        ))
    }

    /// Execute a command in the container (stub)
    pub async fn exec(
        &self,
        instance_id: &InstanceId,
        command: Vec<String>,
    ) -> RuntimeResult<String> {
        let containers = self.containers.read().await;
        if !containers.contains_key(instance_id) {
            return Err(RuntimeError::InstanceNotFound(instance_id.to_string()));
        }

        debug!(
            instance_id = %instance_id,
            command = ?command,
            "Executing command in container (stub)"
        );

        // In real impl: docker.exec(&container_id, command).await
        Ok(format!("Executed: {:?}", command))
    }
}

impl Default for ContainerAdapter {
    fn default() -> Self {
        Self::new()
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
        _config: &serde_json::Value,
        local_path: PathBuf,
    ) -> RuntimeResult<InstanceId> {
        let ArtifactType::Container { runtime, env, .. } = artifact_type else {
            return Err(RuntimeError::UnsupportedArtifact(
                "Expected container".into(),
            ));
        };

        info!(
            path = %local_path.display(),
            runtime = ?runtime,
            "Starting container (stub)"
        );

        // In real impl:
        // 1. Load image from tarball: docker.import_image(...)
        // 2. Create container: docker.create_container(...)
        // 3. Start container: docker.start_container(...)

        let instance_id = InstanceId::generate();
        let container_id = format!("container-{}", uuid::Uuid::new_v4());
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);

        let record = ContainerRecord {
            container_id: container_id.clone(),
            image_path: local_path,
            runtime: runtime.clone(),
            env: env.clone(),
            state: InstanceState::Running,
            started_at: Utc::now(),
            product_tx,
            anomaly_tx,
        };

        self.containers
            .write()
            .await
            .insert(instance_id.clone(), record);

        info!(
            instance_id = %instance_id,
            container_id = %container_id,
            "Container started"
        );

        Ok(instance_id)
    }

    async fn deactivate(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        let record = self
            .containers
            .write()
            .await
            .remove(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        info!(
            instance_id = %instance_id,
            container_id = %record.container_id,
            "Stopping container (stub)"
        );

        // In real impl:
        // docker.stop_container(&record.container_id, None).await?;
        // docker.remove_container(&record.container_id, None).await?;

        Ok(())
    }

    async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus> {
        let containers = self.containers.read().await;
        let record = containers
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        // In real impl: docker.inspect_container(&record.container_id, None).await
        Ok(HealthStatus::new(instance_id.clone(), record.state.clone())
            .with_detail("container_id", serde_json::json!(record.container_id))
            .with_detail(
                "runtime",
                serde_json::json!(format!("{:?}", record.runtime)),
            )
            .with_detail(
                "uptime_secs",
                serde_json::json!((Utc::now() - record.started_at).num_seconds()),
            ))
    }

    async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics> {
        let containers = self.containers.read().await;
        let record = containers
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        // In real impl: docker.stats(&record.container_id, None).await
        Ok(RuntimeMetrics::new(instance_id.clone())
            .with_cpu(0.05) // Simulated
            .with_memory(256_000_000) // Simulated 256MB
            .with_custom(
                "uptime_secs",
                (Utc::now() - record.started_at).num_seconds() as f64,
            ))
    }

    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<ProductOutput>> {
        let containers = self.containers.read().await;
        let record = containers
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(record.product_tx.subscribe())
    }

    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<AnomalyOutput>> {
        let containers = self.containers.read().await;
        let record = containers
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(record.anomaly_tx.subscribe())
    }

    fn list_instances(&self) -> Vec<InstanceId> {
        Vec::new()
    }
}

// ============================================================================
// Process Adapter
// ============================================================================

/// Process record for a running native binary
struct ProcessRecord {
    executable_path: PathBuf,
    args: Vec<String>,
    state: InstanceState,
    started_at: chrono::DateTime<chrono::Utc>,
    pid: Option<u32>,
    product_tx: broadcast::Sender<ProductOutput>,
    anomaly_tx: broadcast::Sender<AnomalyOutput>,
}

/// Process Adapter
///
/// Manages native executables as child processes. This is a stub implementation;
/// in production you would use `tokio::process` for async process management.
///
/// ## Example Usage
///
/// ```rust,ignore
/// use hive_inference::orchestration::adapters::ProcessAdapter;
/// use hive_inference::orchestration::runtime::ArtifactType;
///
/// let adapter = ProcessAdapter::new();
///
/// // Activate a native binary
/// let instance = adapter.activate(
///     &ArtifactType::native("aarch64", vec!["--config".into(), "config.yaml".into()]),
///     &serde_json::json!({}),
///     PathBuf::from("/opt/bin/my-service"),
/// ).await?;
///
/// // Process is now running
///
/// // Deactivate when done
/// adapter.deactivate(&instance).await?;
/// ```
pub struct ProcessAdapter {
    processes: Arc<RwLock<HashMap<InstanceId, ProcessRecord>>>,
}

impl ProcessAdapter {
    /// Create a new process adapter
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Send a signal to the process (stub)
    pub async fn signal(&self, instance_id: &InstanceId, signal: i32) -> RuntimeResult<()> {
        let processes = self.processes.read().await;
        let record = processes
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        debug!(
            instance_id = %instance_id,
            pid = ?record.pid,
            signal = signal,
            "Sending signal to process (stub)"
        );

        // In real impl: nix::sys::signal::kill(Pid::from_raw(pid), Signal::try_from(signal)?)
        Ok(())
    }
}

impl Default for ProcessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeAdapter for ProcessAdapter {
    fn name(&self) -> &str {
        "process_runtime"
    }

    fn can_handle(&self, artifact_type: &ArtifactType) -> bool {
        matches!(artifact_type, ArtifactType::NativeBinary { .. })
    }

    async fn activate(
        &self,
        artifact_type: &ArtifactType,
        _config: &serde_json::Value,
        local_path: PathBuf,
    ) -> RuntimeResult<InstanceId> {
        let ArtifactType::NativeBinary { arch, args } = artifact_type else {
            return Err(RuntimeError::UnsupportedArtifact(
                "Expected native binary".into(),
            ));
        };

        // Verify architecture matches (in real impl)
        let current_arch = std::env::consts::ARCH;
        if arch != current_arch && arch != "any" {
            warn!(
                expected = %arch,
                actual = %current_arch,
                "Architecture mismatch (continuing in stub mode)"
            );
        }

        info!(
            path = %local_path.display(),
            args = ?args,
            "Starting process (stub)"
        );

        // In real impl:
        // let child = Command::new(&local_path)
        //     .args(args)
        //     .spawn()?;
        // let pid = child.id();

        let instance_id = InstanceId::generate();
        let simulated_pid = 10000 + rand::random::<u32>() % 50000;
        let (product_tx, _) = broadcast::channel(1024);
        let (anomaly_tx, _) = broadcast::channel(256);

        let record = ProcessRecord {
            executable_path: local_path,
            args: args.clone(),
            state: InstanceState::Running,
            started_at: Utc::now(),
            pid: Some(simulated_pid),
            product_tx,
            anomaly_tx,
        };

        self.processes
            .write()
            .await
            .insert(instance_id.clone(), record);

        info!(
            instance_id = %instance_id,
            pid = simulated_pid,
            "Process started"
        );

        Ok(instance_id)
    }

    async fn deactivate(&self, instance_id: &InstanceId) -> RuntimeResult<()> {
        let record = self
            .processes
            .write()
            .await
            .remove(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        info!(
            instance_id = %instance_id,
            pid = ?record.pid,
            "Stopping process (stub)"
        );

        // In real impl:
        // Send SIGTERM, wait, then SIGKILL if needed
        // child.kill().await?;

        Ok(())
    }

    async fn health(&self, instance_id: &InstanceId) -> RuntimeResult<HealthStatus> {
        let processes = self.processes.read().await;
        let record = processes
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        Ok(HealthStatus::new(instance_id.clone(), record.state.clone())
            .with_detail("pid", serde_json::json!(record.pid))
            .with_detail(
                "executable",
                serde_json::json!(record.executable_path.display().to_string()),
            )
            .with_detail("args", serde_json::json!(record.args)))
    }

    async fn metrics(&self, instance_id: &InstanceId) -> RuntimeResult<RuntimeMetrics> {
        let processes = self.processes.read().await;
        let record = processes
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;

        // In real impl: Read from /proc/{pid}/stat or use sysinfo crate
        Ok(RuntimeMetrics::new(instance_id.clone())
            .with_cpu(0.02) // Simulated
            .with_memory(50_000_000) // Simulated 50MB
            .with_custom("pid", record.pid.unwrap_or(0) as f64)
            .with_custom(
                "uptime_secs",
                (Utc::now() - record.started_at).num_seconds() as f64,
            ))
    }

    async fn subscribe_products(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<ProductOutput>> {
        let processes = self.processes.read().await;
        let record = processes
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(record.product_tx.subscribe())
    }

    async fn subscribe_anomalies(
        &self,
        instance_id: &InstanceId,
    ) -> RuntimeResult<broadcast::Receiver<AnomalyOutput>> {
        let processes = self.processes.read().await;
        let record = processes
            .get(instance_id)
            .ok_or_else(|| RuntimeError::InstanceNotFound(instance_id.to_string()))?;
        Ok(record.anomaly_tx.subscribe())
    }

    fn list_instances(&self) -> Vec<InstanceId> {
        Vec::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_onnx_adapter_lifecycle() {
        let adapter = OnnxRuntimeAdapter::new();

        assert!(adapter.can_handle(&ArtifactType::onnx_cuda()));
        assert!(!adapter.can_handle(&ArtifactType::docker(HashMap::new())));

        // Activate
        let instance_id = adapter
            .activate(
                &ArtifactType::onnx_cuda(),
                &serde_json::json!({}),
                PathBuf::from("/models/test.onnx"),
            )
            .await
            .unwrap();

        // Health check
        let health = adapter.health(&instance_id).await.unwrap();
        assert!(health.state.is_healthy());
        assert!(health.details.contains_key("execution_providers"));

        // Run inference
        let output = adapter
            .infer(&instance_id, serde_json::json!({"input": [1, 2, 3]}))
            .await
            .unwrap();
        assert!(output.get("simulated").is_some());

        // Check metrics updated
        let metrics = adapter.metrics(&instance_id).await.unwrap();
        assert_eq!(metrics.custom.get("inference_count"), Some(&1.0));

        // Deactivate
        adapter.deactivate(&instance_id).await.unwrap();
        assert!(adapter.health(&instance_id).await.is_err());
    }

    #[tokio::test]
    async fn test_onnx_adapter_hot_reload() {
        let adapter = OnnxRuntimeAdapter::new();

        let instance_id = adapter
            .activate(
                &ArtifactType::onnx_cuda(),
                &serde_json::json!({}),
                PathBuf::from("/models/v1.onnx"),
            )
            .await
            .unwrap();

        // Hot reload
        let result = adapter
            .hot_reload(
                &instance_id,
                &ArtifactType::onnx_tensorrt(),
                &serde_json::json!({}),
                PathBuf::from("/models/v2.onnx"),
            )
            .await
            .unwrap();

        assert!(result);

        // Verify new path
        let health = adapter.health(&instance_id).await.unwrap();
        let path = health.details.get("model_path").unwrap();
        assert!(path.as_str().unwrap().contains("v2.onnx"));
    }

    #[tokio::test]
    async fn test_container_adapter_lifecycle() {
        let adapter = ContainerAdapter::new();

        let mut env = HashMap::new();
        env.insert("API_KEY".into(), "test".into());

        assert!(adapter.can_handle(&ArtifactType::docker(env.clone())));
        assert!(!adapter.can_handle(&ArtifactType::onnx_cuda()));

        // Activate
        let instance_id = adapter
            .activate(
                &ArtifactType::docker(env),
                &serde_json::json!({}),
                PathBuf::from("/images/test.tar"),
            )
            .await
            .unwrap();

        // Health check
        let health = adapter.health(&instance_id).await.unwrap();
        assert!(health.state.is_healthy());
        assert!(health.details.contains_key("container_id"));

        // Get logs
        let logs = adapter.logs(&instance_id).await.unwrap();
        assert!(!logs.is_empty());

        // Exec command
        let output = adapter
            .exec(&instance_id, vec!["echo".into(), "hello".into()])
            .await
            .unwrap();
        assert!(output.contains("echo"));

        // Metrics
        let metrics = adapter.metrics(&instance_id).await.unwrap();
        assert!(metrics.cpu_usage.is_some());

        // Deactivate
        adapter.deactivate(&instance_id).await.unwrap();
        assert!(adapter.health(&instance_id).await.is_err());
    }

    #[tokio::test]
    async fn test_process_adapter_lifecycle() {
        let adapter = ProcessAdapter::new();

        assert!(adapter.can_handle(&ArtifactType::native("aarch64", vec![])));
        assert!(!adapter.can_handle(&ArtifactType::onnx_cuda()));

        // Activate
        let instance_id = adapter
            .activate(
                &ArtifactType::native("any", vec!["--config".into(), "test.yaml".into()]),
                &serde_json::json!({}),
                PathBuf::from("/opt/bin/test-service"),
            )
            .await
            .unwrap();

        // Health check
        let health = adapter.health(&instance_id).await.unwrap();
        assert!(health.state.is_healthy());
        assert!(health.details.contains_key("pid"));

        // Metrics
        let metrics = adapter.metrics(&instance_id).await.unwrap();
        assert!(metrics.custom.contains_key("pid"));

        // Signal (stub)
        adapter.signal(&instance_id, 15).await.unwrap(); // SIGTERM

        // Deactivate
        adapter.deactivate(&instance_id).await.unwrap();
        assert!(adapter.health(&instance_id).await.is_err());
    }

    #[test]
    fn test_adapter_names() {
        assert_eq!(OnnxRuntimeAdapter::new().name(), "onnx_runtime");
        assert_eq!(ContainerAdapter::new().name(), "container_runtime");
        assert_eq!(ProcessAdapter::new().name(), "process_runtime");
    }
}
