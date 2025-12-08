//! Deployment Directive Handler - Issue #177 E2E Integration
//!
//! This module bridges HIVE protocol's `DeploymentDirective` with the
//! `OrchestrationService`, handling the conversion and lifecycle management.
//!
//! ## Flow
//!
//! ```text
//! DeploymentDirective (hive-protocol)
//!         │
//!         ▼
//! DirectiveHandler.handle()
//!         │
//!         ├──▶ Convert to DeploymentRequest
//!         │
//!         ▼
//! OrchestrationService.deploy()
//!         │
//!         ▼
//! DeploymentStatus (hive-protocol)
//! ```

use super::runtime::ArtifactType as InferenceArtifactType;
use super::service::{DeploymentRequest, DeploymentStatus as ServiceStatus, OrchestrationService};
use hive_protocol::distribution::{
    ArtifactType as ProtocolArtifactType, ContainerRuntime as ProtocolContainerRuntime,
    DeploymentDirective, DeploymentStatus,
};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Handler for deployment directives
///
/// Bridges the protocol-layer `DeploymentDirective` to the
/// application-layer `OrchestrationService`.
pub struct DirectiveHandler {
    /// Node ID for status reporting
    node_id: String,
    /// Orchestration service
    service: Arc<OrchestrationService>,
}

impl DirectiveHandler {
    /// Create a new directive handler
    pub fn new(node_id: impl Into<String>, service: Arc<OrchestrationService>) -> Self {
        Self {
            node_id: node_id.into(),
            service,
        }
    }

    /// Handle a deployment directive
    ///
    /// Returns a deployment status indicating the result.
    pub async fn handle(&self, directive: DeploymentDirective) -> DeploymentStatus {
        let directive_id = directive.directive_id.clone();
        info!(
            directive_id = %directive_id,
            artifact_type = ?directive.artifact.artifact_type,
            "Handling deployment directive"
        );

        // Check if this directive targets us
        if !directive.targets_node(&self.node_id) {
            debug!(
                directive_id = %directive_id,
                node_id = %self.node_id,
                "Directive does not target this node"
            );
            return DeploymentStatus::new(&directive_id, &self.node_id);
        }

        // Convert protocol artifact type to inference artifact type
        let artifact_type = match convert_artifact_type(&directive.artifact.artifact_type) {
            Ok(at) => at,
            Err(e) => {
                error!(error = %e, "Failed to convert artifact type");
                return DeploymentStatus::new(&directive_id, &self.node_id)
                    .failed(format!("Unsupported artifact type: {}", e));
            }
        };

        // Create deployment request
        let request = DeploymentRequest {
            blob_hash: directive.artifact.blob_hash.clone(),
            artifact_type,
            config: directive.config.clone(),
            capabilities: directive.capabilities.clone(),
            deployment_id: Some(directive_id.clone()),
        };

        // Deploy via orchestration service
        match self.service.deploy(request).await {
            Ok(result) => match result.status {
                ServiceStatus::Running => {
                    let instance_id = result
                        .instance_id
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                    info!(
                        directive_id = %directive_id,
                        instance_id = %instance_id,
                        "Deployment succeeded"
                    );
                    DeploymentStatus::new(&directive_id, &self.node_id).active(instance_id)
                }
                ServiceStatus::Failed { reason } => {
                    error!(directive_id = %directive_id, reason = %reason, "Deployment failed");
                    DeploymentStatus::new(&directive_id, &self.node_id).failed(reason)
                }
                _ => DeploymentStatus::new(&directive_id, &self.node_id),
            },
            Err(e) => {
                error!(directive_id = %directive_id, error = %e, "Deployment error");
                DeploymentStatus::new(&directive_id, &self.node_id).failed(e.to_string())
            }
        }
    }

    /// Undeploy an instance by directive ID
    pub async fn undeploy(&self, directive_id: &str) -> DeploymentStatus {
        // Look up instance by directive ID
        let instances = self.service.list_instances().await;
        let instance = instances.iter().find(|i| {
            i.request
                .deployment_id
                .as_ref()
                .is_some_and(|id| id == directive_id)
        });

        match instance {
            Some(record) => {
                let instance_id = record.instance_id.clone();
                match self.service.undeploy(&instance_id).await {
                    Ok(()) => {
                        info!(directive_id = %directive_id, "Undeployment succeeded");
                        DeploymentStatus::new(directive_id, &self.node_id)
                    }
                    Err(e) => {
                        error!(directive_id = %directive_id, error = %e, "Undeployment failed");
                        DeploymentStatus::new(directive_id, &self.node_id).failed(e.to_string())
                    }
                }
            }
            None => DeploymentStatus::new(directive_id, &self.node_id).failed("Instance not found"),
        }
    }

    /// Get status of a deployment by directive ID
    pub async fn status(&self, directive_id: &str) -> Option<DeploymentStatus> {
        let instances = self.service.list_instances().await;
        let instance = instances.iter().find(|i| {
            i.request
                .deployment_id
                .as_ref()
                .is_some_and(|id| id == directive_id)
        });

        instance.map(|record| {
            DeploymentStatus::new(directive_id, &self.node_id)
                .active(record.instance_id.to_string())
        })
    }

    /// Get the node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

/// Convert protocol artifact type to inference artifact type
fn convert_artifact_type(
    protocol_type: &ProtocolArtifactType,
) -> Result<InferenceArtifactType, String> {
    match protocol_type {
        ProtocolArtifactType::OnnxModel {
            execution_providers,
        } => Ok(InferenceArtifactType::OnnxModel {
            execution_providers: execution_providers.clone(),
            signature: None,
        }),
        ProtocolArtifactType::Container {
            runtime,
            ports,
            env,
        } => {
            let container_runtime = match runtime {
                ProtocolContainerRuntime::Docker => super::runtime::ContainerRuntime::Docker,
                ProtocolContainerRuntime::Podman => super::runtime::ContainerRuntime::Podman,
                ProtocolContainerRuntime::Containerd => {
                    super::runtime::ContainerRuntime::Containerd
                }
            };
            Ok(InferenceArtifactType::Container {
                runtime: container_runtime,
                ports: ports
                    .iter()
                    .map(|p| super::runtime::PortMapping {
                        container_port: p.container_port,
                        host_port: Some(p.host_port),
                        protocol: p.protocol.clone(),
                    })
                    .collect(),
                env: env.clone(),
            })
        }
        ProtocolArtifactType::NativeBinary { arch, args } => {
            Ok(InferenceArtifactType::NativeBinary {
                arch: arch.clone(),
                args: args.clone(),
            })
        }
        ProtocolArtifactType::ConfigPackage { target_path } => {
            Ok(InferenceArtifactType::ConfigPackage {
                target_path: target_path.clone().into(),
            })
        }
        ProtocolArtifactType::WasmModule { wasi_capabilities } => {
            Ok(InferenceArtifactType::WasmModule {
                wasi_capabilities: wasi_capabilities.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_protocol::distribution::{ArtifactSpec, DeploymentScope, DeploymentState};

    #[tokio::test]
    async fn test_handler_creation() {
        let service = Arc::new(OrchestrationService::with_simulated_storage());
        let handler = DirectiveHandler::new("test-node", service);
        assert_eq!(handler.node_id(), "test-node");
    }

    #[tokio::test]
    async fn test_handle_directive_no_adapter() {
        let service = Arc::new(OrchestrationService::with_simulated_storage());
        // Don't register any adapters
        let handler = DirectiveHandler::new("test-node", service);

        let directive = DeploymentDirective::generate()
            .with_scope(DeploymentScope::Broadcast)
            .with_artifact(ArtifactSpec::onnx_model(
                "sha256:abc123",
                1000,
                vec!["CUDA".into()],
            ));

        let status = handler.handle(directive).await;
        assert_eq!(status.state, DeploymentState::Failed);
        assert!(status.error_message.is_some());
    }

    #[tokio::test]
    async fn test_handle_directive_with_adapter() {
        use super::super::runtime::SimulatedAdapter;

        let service = Arc::new(OrchestrationService::with_simulated_storage());
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let handler = DirectiveHandler::new("test-node", service);

        let directive = DeploymentDirective::generate()
            .with_scope(DeploymentScope::Broadcast)
            .with_artifact(ArtifactSpec::onnx_model(
                "sha256:abc123",
                1000,
                vec!["CUDA".into()],
            ))
            .with_capability("object_detection");

        let status = handler.handle(directive).await;
        assert_eq!(status.state, DeploymentState::Active);
        assert!(status.instance_id.is_some());
    }

    #[tokio::test]
    async fn test_directive_targeting() {
        use super::super::runtime::SimulatedAdapter;

        let service = Arc::new(OrchestrationService::with_simulated_storage());
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let handler = DirectiveHandler::new("node-1", service);

        // Directive targets different node
        let directive = DeploymentDirective::generate()
            .with_scope(DeploymentScope::nodes(vec!["node-2".into()]))
            .with_artifact(ArtifactSpec::onnx_model("sha256:abc", 1000, vec![]));

        let status = handler.handle(directive).await;
        // Should be pending (not processed)
        assert_eq!(status.state, DeploymentState::Pending);
    }

    #[tokio::test]
    async fn test_status_lookup() {
        use super::super::runtime::SimulatedAdapter;

        let service = Arc::new(OrchestrationService::with_simulated_storage());
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let handler = DirectiveHandler::new("test-node", service);

        let directive = DeploymentDirective::new("directive-001")
            .with_scope(DeploymentScope::Broadcast)
            .with_artifact(ArtifactSpec::onnx_model("sha256:abc", 1000, vec![]));

        // Deploy
        let status = handler.handle(directive).await;
        assert_eq!(status.state, DeploymentState::Active);

        // Check status
        let lookup = handler.status("directive-001").await;
        assert!(lookup.is_some());
        assert_eq!(lookup.unwrap().state, DeploymentState::Active);

        // Check unknown directive
        let unknown = handler.status("unknown").await;
        assert!(unknown.is_none());
    }

    #[tokio::test]
    async fn test_undeploy() {
        use super::super::runtime::SimulatedAdapter;

        let service = Arc::new(OrchestrationService::with_simulated_storage());
        service
            .register_adapter(Arc::new(SimulatedAdapter::new("simulated")))
            .await;

        let handler = DirectiveHandler::new("test-node", service);

        // Deploy first
        let directive = DeploymentDirective::new("directive-002")
            .with_scope(DeploymentScope::Broadcast)
            .with_artifact(ArtifactSpec::onnx_model("sha256:abc", 1000, vec![]));

        handler.handle(directive).await;

        // Undeploy
        let status = handler.undeploy("directive-002").await;
        assert_ne!(status.state, DeploymentState::Failed);

        // Verify gone
        let lookup = handler.status("directive-002").await;
        assert!(lookup.is_none());
    }

    #[test]
    fn test_convert_artifact_types() {
        // ONNX model
        let onnx = ProtocolArtifactType::OnnxModel {
            execution_providers: vec!["CUDA".into()],
        };
        let converted = convert_artifact_type(&onnx).unwrap();
        assert!(matches!(converted, InferenceArtifactType::OnnxModel { .. }));

        // Container
        let container = ProtocolArtifactType::Container {
            runtime: ProtocolContainerRuntime::Docker,
            ports: vec![],
            env: std::collections::HashMap::new(),
        };
        let converted = convert_artifact_type(&container).unwrap();
        assert!(matches!(converted, InferenceArtifactType::Container { .. }));

        // Native binary
        let binary = ProtocolArtifactType::NativeBinary {
            arch: "aarch64".into(),
            args: vec![],
        };
        let converted = convert_artifact_type(&binary).unwrap();
        assert!(matches!(
            converted,
            InferenceArtifactType::NativeBinary { .. }
        ));
    }
}
