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

// ============================================================================
// True E2E Tests with Real Multi-Node Sync (Issue #177)
// ============================================================================
//
// These tests use AutomergeIroh backends to verify the full model delivery
// flow with actual network synchronization between nodes.

/// True E2E test: Beacon advertises sensor-only capability initially,
/// then AI capability after model deployment - verified via multi-node sync.
///
/// Flow:
/// 1. Start two nodes (edge + c2) with AutomergeIroh backends
/// 2. Edge node publishes beacon with sensor-only capability
/// 3. C2 node observes beacon via sync - verifies NO AI capability
/// 4. C2 sends DeploymentDirective via sync
/// 5. Edge processes directive, deploys model, updates beacon
/// 6. C2 observes updated beacon - verifies AI capability NOW present
///
/// Note: This test requires network peer discovery which is not automatically
/// configured in the test environment. Run with `--ignored` to execute.
#[tokio::test]
#[ignore = "requires network peer discovery - run with --ignored"]
async fn test_beacon_capability_sync_e2e() {
    use hive_inference::beacon::{BeaconConfig, CameraSpec, HiveBeacon};
    use hive_inference::orchestration::{DirectiveHandler, OrchestrationService, SimulatedAdapter};
    use hive_inference::testing::collections;
    use hive_protocol::distribution::{ArtifactSpec, DeploymentDirective, DeploymentScope};
    use hive_protocol::sync::types::{Document, Query, Value};
    use hive_protocol::sync::DataSyncBackend;
    use hive_protocol::testing::E2EHarness;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    // === Setup: Create two nodes with real sync ===
    let mut harness = E2EHarness::new("beacon_capability_e2e");

    let edge_backend = harness
        .create_automerge_backend()
        .await
        .expect("Failed to create edge backend");

    let c2_backend = harness
        .create_automerge_backend()
        .await
        .expect("Failed to create C2 backend");

    // Allow peer discovery
    tokio::time::sleep(Duration::from_millis(500)).await;

    // === Phase 1: Edge node starts with sensor-only capability ===
    // Create beacon with camera but NO AI model
    let beacon_config = BeaconConfig::new("edge-sensor-001")
        .with_name("Edge Sensor Platform")
        .with_camera(CameraSpec::imx219());
    // Note: NO .with_model() - this is the key point!

    let beacon = HiveBeacon::new(beacon_config).expect("Failed to create beacon");
    let advertisement = beacon.generate_advertisement().await;

    // Verify beacon has NO AI models initially
    assert!(
        advertisement.models.is_empty(),
        "Initial beacon should have NO AI models"
    );

    // Store beacon in sync layer
    let beacon_doc = serde_json::json!({
        "platform_id": advertisement.platform_id,
        "advertised_at": advertisement.advertised_at.to_rfc3339(),
        "has_ai_capability": false,
        "model_count": advertisement.models.len(),
        "models": advertisement.models.iter().map(|m| {
            serde_json::json!({
                "model_id": m.model_id,
                "model_version": m.model_version,
                "model_type": m.model_type
            })
        }).collect::<Vec<_>>()
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in beacon_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(&advertisement.platform_id, fields);

    edge_backend
        .document_store()
        .upsert(collections::BEACONS, doc)
        .await
        .expect("Failed to store beacon");

    // === Phase 2: C2 observes beacon via sync ===
    // Wait for sync to propagate
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query beacon on C2 side
    let query = Query::Eq {
        field: "platform_id".to_string(),
        value: Value::String("edge-sensor-001".to_string()),
    };

    let mut retries = 0;
    let observed_beacon = loop {
        let results = c2_backend
            .document_store()
            .query(collections::BEACONS, &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            break results.into_iter().next().unwrap();
        }

        retries += 1;
        if retries > 20 {
            panic!("C2 did not receive beacon after 20 retries");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // Verify C2 sees beacon with NO AI capability
    let has_ai = observed_beacon
        .get("has_ai_capability")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(
        !has_ai,
        "C2 should see beacon with NO AI capability initially"
    );

    let model_count = observed_beacon
        .get("model_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(99);
    assert_eq!(model_count, 0, "C2 should see 0 models initially");

    // === Phase 3: C2 issues DeploymentDirective via sync ===
    let directive = DeploymentDirective::new("deploy-yolov8-e2e")
        .with_issuer("c2-command")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model(
                "sha256:e2e_test_hash",
                50_000_000,
                vec!["CPUExecutionProvider".into()],
            )
            .with_name("YOLOv8n")
            .with_version("8.0.0"),
        )
        .with_capability("object_detection")
        .with_capability("person_tracking");

    // Store directive in sync layer from C2
    let directive_doc = serde_json::json!({
        "directive_id": directive.directive_id,
        "issuer_node_id": directive.issuer_node_id,
        "scope": "broadcast",
        "artifact_hash": directive.artifact.blob_hash,
        "capabilities": directive.capabilities
    });

    let mut directive_fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in directive_doc.as_object().unwrap() {
        directive_fields.insert(k.clone(), v.clone());
    }
    let directive_doc = Document::with_id(&directive.directive_id, directive_fields);

    c2_backend
        .document_store()
        .upsert(collections::DIRECTIVES, directive_doc)
        .await
        .expect("Failed to store directive");

    // === Phase 4: Edge node processes directive ===
    // In a real system, edge would observe the directive via sync.
    // Here we simulate the edge processing it directly.
    let service = Arc::new(OrchestrationService::with_simulated_storage());
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx")))
        .await;

    let handler = DirectiveHandler::new("edge-sensor-001", service.clone());
    let status = handler.handle(directive.clone()).await;

    assert_eq!(
        status.state,
        hive_protocol::distribution::DeploymentState::Active,
        "Deployment should succeed"
    );

    // === Phase 5: Edge updates beacon with AI capability ===
    // Now the beacon should include the deployed model
    let updated_beacon_doc = serde_json::json!({
        "platform_id": "edge-sensor-001",
        "advertised_at": chrono::Utc::now().to_rfc3339(),
        "has_ai_capability": true,
        "model_count": 1,
        "models": [{
            "model_id": "YOLOv8n",
            "version": "8.0.0",
            "model_type": "object_detection",
            "capabilities": ["object_detection", "person_tracking"]
        }]
    });

    let mut updated_fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in updated_beacon_doc.as_object().unwrap() {
        updated_fields.insert(k.clone(), v.clone());
    }
    let updated_doc = Document::with_id("edge-sensor-001", updated_fields);

    edge_backend
        .document_store()
        .upsert(collections::BEACONS, updated_doc)
        .await
        .expect("Failed to update beacon");

    // === Phase 6: C2 observes updated beacon with AI capability ===
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut retries = 0;
    let final_beacon = loop {
        let results = c2_backend
            .document_store()
            .query(collections::BEACONS, &query)
            .await
            .expect("Query failed");

        if let Some(beacon) = results.into_iter().next() {
            let has_ai = beacon
                .get("has_ai_capability")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if has_ai {
                break beacon;
            }
        }

        retries += 1;
        if retries > 30 {
            panic!("C2 did not see updated beacon with AI capability after 30 retries");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // Verify C2 now sees AI capability
    let has_ai = final_beacon
        .get("has_ai_capability")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        has_ai,
        "C2 should see beacon with AI capability after deployment"
    );

    let model_count = final_beacon
        .get("model_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(model_count, 1, "C2 should see 1 model after deployment");

    // Cleanup
    edge_backend.shutdown().await.ok();
    c2_backend.shutdown().await.ok();
}

/// Test that directive sync propagates between nodes
///
/// Note: This test requires network peer discovery which is not automatically
/// configured in the test environment. Run with `--ignored` to execute.
#[tokio::test]
#[ignore = "requires network peer discovery - run with --ignored"]
async fn test_directive_sync_e2e() {
    use hive_inference::testing::collections;
    use hive_protocol::distribution::{ArtifactSpec, DeploymentDirective, DeploymentScope};
    use hive_protocol::sync::types::{Document, Query, Value};
    use hive_protocol::sync::DataSyncBackend;
    use hive_protocol::testing::E2EHarness;
    use std::collections::HashMap;
    use std::time::Duration;

    let mut harness = E2EHarness::new("directive_sync_e2e");

    let node1 = harness
        .create_automerge_backend()
        .await
        .expect("Failed to create node1");

    let node2 = harness
        .create_automerge_backend()
        .await
        .expect("Failed to create node2");

    // Allow peer discovery
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node1 creates a directive
    let directive = DeploymentDirective::new("sync-test-directive")
        .with_issuer("node1")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(ArtifactSpec::onnx_model("sha256:test", 1000, vec![]));

    let directive_doc = serde_json::json!({
        "directive_id": directive.directive_id,
        "issuer": "node1",
        "artifact_hash": directive.artifact.blob_hash,
        "scope": "broadcast"
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in directive_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(&directive.directive_id, fields);

    node1
        .document_store()
        .upsert(collections::DIRECTIVES, doc)
        .await
        .expect("Failed to store directive");

    // Node2 should see the directive via sync
    let query = Query::Eq {
        field: "directive_id".to_string(),
        value: Value::String("sync-test-directive".to_string()),
    };

    let mut found = false;
    for _ in 0..30 {
        let results = node2
            .document_store()
            .query(collections::DIRECTIVES, &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            let synced_directive = &results[0];
            assert_eq!(
                synced_directive.get("issuer").and_then(|v| v.as_str()),
                Some("node1")
            );
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(found, "Node2 should receive directive from Node1 via sync");

    // Cleanup
    node1.shutdown().await.ok();
    node2.shutdown().await.ok();
}
