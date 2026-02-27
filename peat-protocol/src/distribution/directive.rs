//! Deployment Directives - ADR-012 / ADR-026
//!
//! This module provides the `DeploymentDirective` type for pushing software
//! artifacts (models, containers, binaries) from C2 to edge nodes.
//!
//! ## Flow
//!
//! ```text
//! ┌─────────────┐                    ┌─────────────┐
//! │     C2      │                    │  Edge Node  │
//! │             │─DeploymentDirective│             │
//! │             │───────────────────▶│             │
//! │             │                    │ fetch blob  │
//! │             │                    │ activate    │
//! │             │◀───────────────────│             │
//! │             │  DeploymentStatus  │ advertise   │
//! └─────────────┘                    └─────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use peat_protocol::distribution::{
//!     DeploymentDirective, DeploymentScope, ArtifactSpec, DeploymentPriority,
//! };
//!
//! // Create directive for ONNX model deployment
//! let directive = DeploymentDirective::new("yolov8n-deploy-001")
//!     .with_artifact(ArtifactSpec::onnx_model(
//!         "sha256:abc123...",
//!         500_000_000,
//!         vec!["CUDAExecutionProvider".into()],
//!     ))
//!     .with_scope(DeploymentScope::formation("formation-alpha"))
//!     .with_capabilities(vec!["object_detection".into()])
//!     .with_priority(DeploymentPriority::High);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deployment directive - command to deploy software to nodes
///
/// This is the primary message type for C2 → Edge software deployment.
/// Nodes matching the scope will fetch the artifact and activate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDirective {
    /// Unique directive identifier
    pub directive_id: String,
    /// When the directive was issued
    pub issued_at: DateTime<Utc>,
    /// Node ID of the issuer
    pub issuer_node_id: String,
    /// Formation ID of the issuer (for hierarchy routing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_formation_id: Option<String>,
    /// Target scope for this deployment
    pub scope: DeploymentScope,
    /// Artifact specification
    pub artifact: ArtifactSpec,
    /// Capabilities this deployment provides
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Runtime-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,
    /// Deployment options
    #[serde(default)]
    pub options: DeploymentOptions,
}

impl DeploymentDirective {
    /// Create a new deployment directive
    pub fn new(directive_id: impl Into<String>) -> Self {
        Self {
            directive_id: directive_id.into(),
            issued_at: Utc::now(),
            issuer_node_id: String::new(),
            issuer_formation_id: None,
            scope: DeploymentScope::Broadcast,
            artifact: ArtifactSpec::default(),
            capabilities: Vec::new(),
            config: serde_json::Value::Null,
            options: DeploymentOptions::default(),
        }
    }

    /// Generate a unique directive ID
    pub fn generate() -> Self {
        Self::new(uuid::Uuid::new_v4().to_string())
    }

    /// Set the issuer node
    pub fn with_issuer(mut self, node_id: impl Into<String>) -> Self {
        self.issuer_node_id = node_id.into();
        self
    }

    /// Set the issuer formation
    pub fn with_formation(mut self, formation_id: impl Into<String>) -> Self {
        self.issuer_formation_id = Some(formation_id.into());
        self
    }

    /// Set the deployment scope
    pub fn with_scope(mut self, scope: DeploymentScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set the artifact
    pub fn with_artifact(mut self, artifact: ArtifactSpec) -> Self {
        self.artifact = artifact;
        self
    }

    /// Add capabilities
    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// Set runtime config
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Set deployment options
    pub fn with_options(mut self, options: DeploymentOptions) -> Self {
        self.options = options;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: DeploymentPriority) -> Self {
        self.options.priority = priority;
        self
    }

    /// Check if this directive targets a specific node
    pub fn targets_node(&self, node_id: &str) -> bool {
        match &self.scope {
            DeploymentScope::Broadcast => true,
            DeploymentScope::Formation(fid) => {
                // Would need formation membership lookup
                // For now, return true if same formation
                self.issuer_formation_id.as_deref() == Some(fid)
            }
            DeploymentScope::Nodes(node_ids) => node_ids.iter().any(|n| n == node_id),
            DeploymentScope::Capability(filter) => {
                // Would need capability matching
                // For now, assume match if no specific requirements
                filter.required_capabilities.is_empty()
            }
        }
    }
}

/// Scope for deployment targeting
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeploymentScope {
    /// Broadcast to all capable nodes
    Broadcast,
    /// Target a specific formation
    Formation(String),
    /// Target specific nodes by ID
    Nodes(Vec<String>),
    /// Target nodes matching capability filter
    Capability(CapabilityFilter),
}

impl DeploymentScope {
    /// Create scope for a specific formation
    pub fn formation(formation_id: impl Into<String>) -> Self {
        Self::Formation(formation_id.into())
    }

    /// Create scope for specific nodes
    pub fn nodes(node_ids: Vec<String>) -> Self {
        Self::Nodes(node_ids)
    }

    /// Create scope for capability-based targeting
    pub fn with_capabilities(capabilities: Vec<String>) -> Self {
        Self::Capability(CapabilityFilter {
            required_capabilities: capabilities,
            ..Default::default()
        })
    }
}

/// Filter for capability-based deployment targeting
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityFilter {
    /// Minimum GPU memory in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_gpu_memory_mb: Option<u64>,
    /// Minimum system memory in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_memory_mb: Option<u64>,
    /// Minimum storage in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_storage_mb: Option<u64>,
    /// Required capabilities (e.g., ["cuda", "tensorrt"])
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// Custom filters
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, String>,
}

/// Artifact specification for deployment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactSpec {
    /// Blob hash (content-addressed)
    pub blob_hash: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Artifact type
    pub artifact_type: ArtifactType,
    /// SHA256 hash for verification (if different from blob hash)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Version string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ArtifactSpec {
    /// Create ONNX model artifact spec
    pub fn onnx_model(
        blob_hash: impl Into<String>,
        size_bytes: u64,
        execution_providers: Vec<String>,
    ) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            size_bytes,
            artifact_type: ArtifactType::OnnxModel {
                execution_providers,
            },
            sha256: None,
            name: None,
            version: None,
        }
    }

    /// Create container artifact spec
    pub fn container(
        blob_hash: impl Into<String>,
        size_bytes: u64,
        runtime: ContainerRuntime,
    ) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            size_bytes,
            artifact_type: ArtifactType::Container {
                runtime,
                ports: Vec::new(),
                env: HashMap::new(),
            },
            sha256: None,
            name: None,
            version: None,
        }
    }

    /// Create native binary artifact spec
    pub fn native_binary(
        blob_hash: impl Into<String>,
        size_bytes: u64,
        arch: impl Into<String>,
    ) -> Self {
        Self {
            blob_hash: blob_hash.into(),
            size_bytes,
            artifact_type: ArtifactType::NativeBinary {
                arch: arch.into(),
                args: Vec::new(),
            },
            sha256: None,
            name: None,
            version: None,
        }
    }

    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set SHA256 hash
    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.sha256 = Some(sha256.into());
        self
    }
}

/// Artifact type (mirrors peat-inference ArtifactType for protocol layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArtifactType {
    /// ONNX model for inference
    OnnxModel {
        /// Execution providers in preference order
        #[serde(default)]
        execution_providers: Vec<String>,
    },
    /// Container image
    Container {
        /// Container runtime
        runtime: ContainerRuntime,
        /// Port mappings
        #[serde(default)]
        ports: Vec<PortMapping>,
        /// Environment variables
        #[serde(default)]
        env: HashMap<String, String>,
    },
    /// Native executable
    NativeBinary {
        /// Target architecture
        arch: String,
        /// Command-line arguments
        #[serde(default)]
        args: Vec<String>,
    },
    /// Configuration package
    ConfigPackage {
        /// Target extraction path
        target_path: String,
    },
    /// WebAssembly module
    WasmModule {
        /// WASI capabilities
        #[serde(default)]
        wasi_capabilities: Vec<String>,
    },
}

impl Default for ArtifactType {
    fn default() -> Self {
        Self::OnnxModel {
            execution_providers: Vec::new(),
        }
    }
}

/// Container runtime
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntime {
    #[default]
    Docker,
    Podman,
    Containerd,
}

/// Port mapping for containers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Container port
    pub container_port: u16,
    /// Host port
    pub host_port: u16,
    /// Protocol (tcp/udp)
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

/// Deployment options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentOptions {
    /// Priority level
    #[serde(default)]
    pub priority: DeploymentPriority,
    /// Timeout in seconds (0 = no timeout)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    /// Replace existing deployment with same capabilities
    #[serde(default)]
    pub replace_existing: bool,
    /// Rollback threshold (percentage of nodes that must succeed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_threshold_percent: Option<u32>,
    /// Auto-activate after download
    #[serde(default = "default_true")]
    pub auto_activate: bool,
}

fn default_timeout() -> u32 {
    300 // 5 minutes
}

fn default_true() -> bool {
    true
}

impl Default for DeploymentOptions {
    fn default() -> Self {
        Self {
            priority: DeploymentPriority::Normal,
            timeout_seconds: default_timeout(),
            replace_existing: false,
            rollback_threshold_percent: None,
            auto_activate: true,
        }
    }
}

/// Deployment priority
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentPriority {
    /// Critical - interrupt other operations
    Critical,
    /// High - process soon
    High,
    /// Normal - standard processing
    #[default]
    Normal,
    /// Low - process when idle
    Low,
}

/// Deployment status report from a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStatus {
    /// Directive ID this status is for
    pub directive_id: String,
    /// Reporting node ID
    pub node_id: String,
    /// When this status was reported
    pub reported_at: DateTime<Utc>,
    /// Current state
    pub state: DeploymentState,
    /// Progress percentage (0-100)
    pub progress_percent: u8,
    /// Error message (if state is Failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Instance ID (if state is Active)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

impl DeploymentStatus {
    /// Create a new status report
    pub fn new(directive_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            directive_id: directive_id.into(),
            node_id: node_id.into(),
            reported_at: Utc::now(),
            state: DeploymentState::Pending,
            progress_percent: 0,
            error_message: None,
            instance_id: None,
        }
    }

    /// Set state to downloading
    pub fn downloading(mut self, progress: u8) -> Self {
        self.state = DeploymentState::Downloading;
        self.progress_percent = progress.min(99);
        self
    }

    /// Set state to activating
    pub fn activating(mut self) -> Self {
        self.state = DeploymentState::Activating;
        self.progress_percent = 100;
        self
    }

    /// Set state to active
    pub fn active(mut self, instance_id: impl Into<String>) -> Self {
        self.state = DeploymentState::Active;
        self.progress_percent = 100;
        self.instance_id = Some(instance_id.into());
        self
    }

    /// Set state to failed
    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.state = DeploymentState::Failed;
        self.error_message = Some(error.into());
        self
    }

    /// Set state to rolled back
    pub fn rolled_back(mut self) -> Self {
        self.state = DeploymentState::RolledBack;
        self
    }
}

/// Deployment state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    /// Directive received, waiting to process
    Pending,
    /// Downloading artifact from blob store
    Downloading,
    /// Activating artifact via runtime adapter
    Activating,
    /// Artifact is active and running
    Active,
    /// Deployment failed
    Failed,
    /// Deployment was rolled back
    RolledBack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directive_creation() {
        let directive = DeploymentDirective::generate()
            .with_issuer("c2-node-1")
            .with_formation("formation-alpha")
            .with_artifact(ArtifactSpec::onnx_model(
                "sha256:abc123",
                500_000_000,
                vec!["CUDAExecutionProvider".into()],
            ))
            .with_capability("object_detection")
            .with_priority(DeploymentPriority::High);

        assert!(!directive.directive_id.is_empty());
        assert_eq!(directive.issuer_node_id, "c2-node-1");
        assert_eq!(directive.capabilities, vec!["object_detection"]);
        assert_eq!(directive.options.priority, DeploymentPriority::High);
    }

    #[test]
    fn test_scope_targeting() {
        // Broadcast targets everyone
        let directive = DeploymentDirective::generate();
        assert!(directive.targets_node("any-node"));

        // Node list targets specific nodes
        let directive = DeploymentDirective::generate().with_scope(DeploymentScope::nodes(vec![
            "node-1".into(),
            "node-2".into(),
        ]));
        assert!(directive.targets_node("node-1"));
        assert!(!directive.targets_node("node-3"));
    }

    #[test]
    fn test_artifact_spec() {
        let spec = ArtifactSpec::onnx_model("sha256:abc", 1000, vec!["CUDA".into()])
            .with_name("YOLOv8n")
            .with_version("1.0.0");

        assert_eq!(spec.blob_hash, "sha256:abc");
        assert_eq!(spec.name, Some("YOLOv8n".to_string()));
        assert!(matches!(spec.artifact_type, ArtifactType::OnnxModel { .. }));
    }

    #[test]
    fn test_deployment_status_transitions() {
        let status = DeploymentStatus::new("directive-1", "node-1");
        assert_eq!(status.state, DeploymentState::Pending);

        let status = status.downloading(50);
        assert_eq!(status.state, DeploymentState::Downloading);
        assert_eq!(status.progress_percent, 50);

        let status = status.activating();
        assert_eq!(status.state, DeploymentState::Activating);

        let status = status.active("instance-123");
        assert_eq!(status.state, DeploymentState::Active);
        assert_eq!(status.instance_id, Some("instance-123".to_string()));
    }

    #[test]
    fn test_serialization() {
        let directive = DeploymentDirective::generate().with_artifact(ArtifactSpec::container(
            "sha256:def456",
            100_000_000,
            ContainerRuntime::Docker,
        ));

        let json = serde_json::to_string_pretty(&directive).unwrap();
        let parsed: DeploymentDirective = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.directive_id, directive.directive_id);
        assert!(matches!(
            parsed.artifact.artifact_type,
            ArtifactType::Container { .. }
        ));
    }
}
