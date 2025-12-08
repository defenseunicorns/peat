//! M1 Vignette End-to-End Tests
//!
//! These tests validate the full M1 object tracking scenario across
//! distributed human-machine-AI teams using REAL multi-node sync
//! via AutomergeIroh backends.
//!
//! Note: M1TestHarness tests are disabled until E2EHarness is available
//! for automerge-backend in hive-protocol.

// M1TestHarness requires hive_protocol::testing::E2EHarness which is only
// available with the ditto-backend feature. These tests are disabled until
// an automerge-backend equivalent is available.

/*
use hive_inference::testing::M1TestHarness;
use std::time::Duration;

/// Test Phase 1: Team Initialization with REAL sync
#[tokio::test]
async fn test_phase1_initialization_real_sync() {
    let mut harness = M1TestHarness::new("e2e_phase1_init_real");
    harness.initialize().await.expect("Init should succeed");
    let duration = harness.phase1_initialization().await.expect("Phase 1 should succeed");
    assert!(duration < Duration::from_secs(30));
    harness.shutdown().await.expect("Shutdown should succeed");
}

// ... other M1TestHarness tests ...
*/

/// Test Team Fixture Creation (no sync needed)
#[tokio::test]
async fn test_team_fixtures() {
    use hive_inference::testing::TeamFixture;

    let alpha = TeamFixture::alpha();
    let bravo = TeamFixture::bravo();

    // Alpha should be UAV-based
    assert_eq!(alpha.name, "Alpha");
    assert_eq!(alpha.team.member_count(), 3);
    assert_eq!(alpha.network_id, "network-a");

    // Bravo should be UGV-based
    assert_eq!(bravo.name, "Bravo");
    assert_eq!(bravo.team.member_count(), 3);
    assert_eq!(bravo.network_id, "network-b");

    // Teams should have different platform IDs
    let alpha_ids = alpha.platform_ids();
    let bravo_ids = bravo.platform_ids();
    for id in &alpha_ids {
        assert!(
            !bravo_ids.contains(id),
            "Platform IDs should be unique across teams"
        );
    }
}

/// Test Simulated C2 (no sync needed)
#[tokio::test]
async fn test_simulated_c2() {
    use hive_inference::messages::{Position, Priority};
    use hive_inference::testing::SimulatedC2;

    let mut c2 = SimulatedC2::new("Test-C2");

    // Issue command
    let cmd = c2.issue_track_command("Test target", Priority::High, None);
    assert_eq!(c2.command_count(), 1);
    assert!(c2.get_command_timestamp(&cmd.command_id).is_some());

    // Receive tracks
    let track = hive_inference::messages::TrackUpdate::new(
        "TRACK-001",
        "person",
        0.85,
        Position::new(33.77, -84.39),
        "Alpha-2",
        "Alpha-3",
        "1.0.0",
    );
    c2.receive_track(track);

    assert_eq!(c2.track_count(), 1);
    assert!(c2.get_latest_track("TRACK-001").is_some());
}

/// Test Coordinator Fixture (no sync needed)
#[tokio::test]
async fn test_coordinator_fixture() {
    use hive_inference::testing::CoordinatorFixture;

    let mut coord = CoordinatorFixture::bridge("Test-Bridge", "net-a", "net-b");

    assert!(coord.is_bridge);
    assert!(coord.connects("net-a", "net-b"));
    assert!(!coord.connects("net-a", "net-c"));

    coord.register_team("Alpha");
    coord.register_team("Bravo");
    coord.register_team("Alpha"); // Duplicate should not add

    assert_eq!(coord.teams.len(), 2);
}

// ============================================================================
// Model Delivery E2E Tests (Issue #177)
// ============================================================================

/// Test the full model delivery flow:
/// 1. Edge node starts with only sensor capability (no AI)
/// 2. C2 issues DeploymentDirective
/// 3. Node fetches blob and activates model
/// 4. Node now advertises AI capability
#[tokio::test]
async fn test_model_delivery_e2e() {
    use hive_inference::orchestration::{DirectiveHandler, OrchestrationService, SimulatedAdapter};
    use hive_protocol::distribution::{
        ArtifactSpec, DeploymentDirective, DeploymentScope, DeploymentState,
    };
    use std::sync::Arc;

    // === Phase 1: Initial State ===
    // Edge node has only sensor capability, no AI model
    let service = Arc::new(OrchestrationService::with_simulated_storage());

    // Register a simulated adapter (in real deployment, this would be OnnxRuntimeAdapter)
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx_simulator")))
        .await;

    // Node has no active deployments initially
    let instances = service.list_instances().await;
    assert!(instances.is_empty(), "Node should start with no AI models");

    // === Phase 2: C2 Issues Deployment Directive ===
    let handler = DirectiveHandler::new("edge-node-001", service.clone());

    let directive = DeploymentDirective::new("deploy-yolov8-001")
        .with_issuer("c2-command")
        .with_formation("formation-alpha")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model(
                "sha256:abc123def456789",
                50_000_000, // 50MB model
                vec![
                    "CUDAExecutionProvider".into(),
                    "CPUExecutionProvider".into(),
                ],
            )
            .with_name("YOLOv8n")
            .with_version("8.0.0"),
        )
        .with_capability("object_detection")
        .with_capability("person_tracking");

    // === Phase 3: Node Handles Directive ===
    let status = handler.handle(directive).await;

    // Deployment should succeed
    assert_eq!(
        status.state,
        DeploymentState::Active,
        "Deployment should be active"
    );
    assert!(status.instance_id.is_some(), "Should have an instance ID");
    assert_eq!(status.node_id, "edge-node-001");
    assert_eq!(status.directive_id, "deploy-yolov8-001");

    // === Phase 4: Verify Node Now Has AI Capability ===
    let instances = service.list_instances().await;
    assert_eq!(instances.len(), 1, "Node should have 1 active deployment");

    let instance = &instances[0];
    assert_eq!(
        instance.request.capabilities,
        vec!["object_detection", "person_tracking"]
    );
    assert_eq!(instance.request.blob_hash, "sha256:abc123def456789");

    // Check health
    let health = service.health(&instance.instance_id).await;
    assert!(health.is_ok(), "Instance should be healthy");
    assert!(
        health.unwrap().state.is_healthy(),
        "Instance state should be healthy"
    );

    // === Phase 5: Status Lookup ===
    let lookup = handler.status("deploy-yolov8-001").await;
    assert!(lookup.is_some(), "Should find deployment by directive ID");
    assert_eq!(lookup.unwrap().state, DeploymentState::Active);

    // Lookup unknown directive
    let unknown = handler.status("unknown-directive").await;
    assert!(unknown.is_none(), "Should not find unknown directive");
}

/// Test model update flow (rolling update via directive)
#[tokio::test]
async fn test_model_update_e2e() {
    use hive_inference::orchestration::{DirectiveHandler, OrchestrationService, SimulatedAdapter};
    use hive_protocol::distribution::{
        ArtifactSpec, DeploymentDirective, DeploymentScope, DeploymentState,
    };
    use std::sync::Arc;

    let service = Arc::new(OrchestrationService::with_simulated_storage());
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx")))
        .await;
    let handler = DirectiveHandler::new("edge-node-002", service.clone());

    // Deploy v1.0.0
    let directive_v1 = DeploymentDirective::new("deploy-model-v1")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model("sha256:v1hash", 50_000_000, vec![])
                .with_name("detector")
                .with_version("1.0.0"),
        )
        .with_capability("detection");

    let status_v1 = handler.handle(directive_v1).await;
    assert_eq!(status_v1.state, DeploymentState::Active);

    // Deploy v2.0.0 (new deployment, old one stays)
    let directive_v2 = DeploymentDirective::new("deploy-model-v2")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model("sha256:v2hash", 55_000_000, vec![])
                .with_name("detector")
                .with_version("2.0.0"),
        )
        .with_capability("detection_v2");

    let status_v2 = handler.handle(directive_v2).await;
    assert_eq!(status_v2.state, DeploymentState::Active);

    // Both should be active (replace logic would be in OrchestrationService)
    let instances = service.list_instances().await;
    assert_eq!(instances.len(), 2, "Both versions should be active");

    // Undeploy old version
    let undeploy_status = handler.undeploy("deploy-model-v1").await;
    assert_ne!(undeploy_status.state, DeploymentState::Failed);

    let instances = service.list_instances().await;
    assert_eq!(instances.len(), 1, "Only v2 should remain");
    assert_eq!(instances[0].request.blob_hash, "sha256:v2hash");
}

/// Test deployment targeting (directive only processed by matching nodes)
#[tokio::test]
async fn test_deployment_targeting() {
    use hive_inference::orchestration::{DirectiveHandler, OrchestrationService, SimulatedAdapter};
    use hive_protocol::distribution::{
        ArtifactSpec, DeploymentDirective, DeploymentScope, DeploymentState,
    };
    use std::sync::Arc;

    let service = Arc::new(OrchestrationService::with_simulated_storage());
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx")))
        .await;

    // Handler for node-1
    let handler_node1 = DirectiveHandler::new("node-1", service.clone());

    // Directive targets only node-2
    let directive = DeploymentDirective::new("targeted-deploy")
        .with_scope(DeploymentScope::nodes(vec!["node-2".into()]))
        .with_artifact(ArtifactSpec::onnx_model("sha256:abc", 1000, vec![]));

    // node-1 should not process this
    let status = handler_node1.handle(directive).await;
    assert_eq!(
        status.state,
        DeploymentState::Pending,
        "Node-1 should not process directive for node-2"
    );

    // No instances should be created
    let instances = service.list_instances().await;
    assert!(instances.is_empty());
}

/// Test deployment failure handling
#[tokio::test]
async fn test_deployment_failure() {
    use hive_inference::orchestration::{DirectiveHandler, OrchestrationService};
    use hive_protocol::distribution::{
        ArtifactSpec, DeploymentDirective, DeploymentScope, DeploymentState,
    };
    use std::sync::Arc;

    // Service with NO adapters registered - deployments will fail
    let service = Arc::new(OrchestrationService::with_simulated_storage());
    let handler = DirectiveHandler::new("edge-node", service);

    let directive = DeploymentDirective::new("will-fail")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(ArtifactSpec::onnx_model("sha256:abc", 1000, vec![]));

    let status = handler.handle(directive).await;

    assert_eq!(status.state, DeploymentState::Failed);
    assert!(status.error_message.is_some());
    assert!(
        status.error_message.unwrap().contains("adapter"),
        "Error should mention missing adapter"
    );
}
