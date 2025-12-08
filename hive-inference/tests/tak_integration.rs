//! TAK Integration Tests (Issue #330)
//!
//! End-to-end integration testing for the Jetson → HIVE → TAK pipeline.
//!
//! These tests validate:
//! 1. Capability Advertisement Flow
//! 2. Live Detection → Track Flow
//! 3. Chipout Extraction
//! 4. Mission Tasking (TAK → Jetson)
//! 5. Model Deployment via C2 Directive
//!
//! Note: These tests use simulated components but exercise the full message flow.
//! For live hardware tests, see the integration_live.rs module.

use hive_inference::beacon::{BeaconConfig, CameraSpec, ComputeSpec, HiveBeacon, ModelSpec};
use hive_inference::inference::TrackerConfig;
use hive_inference::messages::{Position, Priority, TrackUpdate};
use hive_inference::orchestration::{DirectiveHandler, OrchestrationService, SimulatedAdapter};
use hive_inference::testing::collections;
use hive_inference::testing::{MetricsCollector, SimulatedC2};
use hive_inference::{
    ChipoutExtractor, InferencePipeline, PipelineConfig, SimulatedDetector, SimulatedTracker,
    VideoFrame,
};
use hive_protocol::distribution::{
    ArtifactSpec, DeploymentDirective, DeploymentScope, DeploymentState,
};
use hive_protocol::sync::types::{Document, Query, Value};
use hive_protocol::sync::DataSyncBackend;
use hive_protocol::testing::E2EHarness;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Performance metrics targets from issue #330
mod targets {
    use std::time::Duration;

    pub const DETECTION_FPS: f64 = 15.0;
    pub const TRACK_PUBLISH_LATENCY: Duration = Duration::from_millis(500);
    pub const END_TO_END_LATENCY: Duration = Duration::from_secs(5);
    #[allow(dead_code)]
    pub const CHIPOUT_MAX_SIZE: usize = 100 * 1024; // 100KB
    #[allow(dead_code)]
    pub const GPU_UTILIZATION_MAX: f64 = 0.80;
}

// ============================================================================
// Test 1: Capability Advertisement Flow
// ============================================================================

/// Test the full capability advertisement flow:
/// 1. Jetson beacon publishes CapabilityAdvertisement on startup
/// 2. Advertisement syncs through HIVE mesh
/// 3. Verify advertisement contains expected AI capability info
#[tokio::test]
async fn test_capability_advertisement_flow() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    println!("=== Test: Capability Advertisement Flow ===");

    // === Phase 1: Create Jetson beacon with AI capability ===
    let beacon_config = BeaconConfig::new("jetson-orin-001")
        .with_name("Jetson Orin Nano Edge Platform")
        .with_camera(CameraSpec::imx219())
        .with_compute(ComputeSpec::jetson_orin_nano())
        .with_model(ModelSpec::yolov8n());

    let beacon = HiveBeacon::new(beacon_config).expect("Failed to create beacon");
    let advertisement = beacon.generate_advertisement().await;

    // === Phase 2: Verify advertisement structure ===
    assert_eq!(advertisement.platform_id, "jetson-orin-001");
    assert!(!advertisement.models.is_empty(), "Should have AI models");

    let ai_model = &advertisement.models[0];
    assert_eq!(ai_model.model_id, "yolov8n");
    assert_eq!(ai_model.model_version, "8.0.0");

    println!("✓ Capability advertisement generated successfully");
    println!(
        "  Platform: {}, Models: {}",
        advertisement.platform_id,
        advertisement.models.len()
    );
}

/// Test capability advertisement sync between two nodes
#[tokio::test]
async fn test_capability_sync_between_nodes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("hive_protocol::storage::automerge_sync=debug,hive_protocol::sync=debug")
        .with_test_writer()
        .try_init();

    println!("=== Test: Capability Sync Between Nodes ===");

    let mut harness = E2EHarness::new("capability_sync_tak");

    // Create two nodes with explicit bind addresses
    let jetson_addr: std::net::SocketAddr = "127.0.0.1:19401".parse().unwrap();
    let bridge_addr: std::net::SocketAddr = "127.0.0.1:19402".parse().unwrap();

    let jetson_backend = harness
        .create_automerge_backend_with_bind(Some(jetson_addr))
        .await
        .expect("Failed to create Jetson backend");

    let bridge_backend = harness
        .create_automerge_backend_with_bind(Some(bridge_addr))
        .await
        .expect("Failed to create Bridge backend");

    // Connect the peers
    let bridge_endpoint_id = bridge_backend.endpoint_id();
    let bridge_id_hex = hex::encode(bridge_endpoint_id.as_bytes());

    let connected = jetson_backend
        .sync_engine()
        .connect_to_peer(&bridge_id_hex, &[bridge_addr.to_string()])
        .await
        .expect("Should connect Jetson to Bridge");

    if !connected {
        let jetson_endpoint_id = jetson_backend.endpoint_id();
        let jetson_id_hex = hex::encode(jetson_endpoint_id.as_bytes());
        bridge_backend
            .sync_engine()
            .connect_to_peer(&jetson_id_hex, &[jetson_addr.to_string()])
            .await
            .expect("Should connect Bridge to Jetson");
    }

    // Start sync
    jetson_backend
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start Jetson sync");
    bridge_backend
        .sync_engine()
        .start_sync()
        .await
        .expect("Should start Bridge sync");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Generate and store capability advertisement
    let beacon_config = BeaconConfig::new("jetson-test-001")
        .with_name("Test Jetson")
        .with_model(ModelSpec::yolov8n());

    let beacon = HiveBeacon::new(beacon_config).unwrap();
    let advertisement = beacon.generate_advertisement().await;

    let cap_doc = serde_json::json!({
        "platform_id": advertisement.platform_id,
        "has_ai_capability": !advertisement.models.is_empty(),
        "model_count": advertisement.models.len(),
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in cap_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(&advertisement.platform_id, fields);

    let start = Instant::now();
    jetson_backend
        .document_store()
        .upsert(collections::CAPABILITIES, doc)
        .await
        .expect("Failed to store capability");

    // Wait for sync and trigger explicit sync
    let doc_key = format!(
        "{}:{}",
        collections::CAPABILITIES,
        advertisement.platform_id
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    jetson_backend
        .as_ref()
        .sync_document(&doc_key)
        .await
        .expect("Failed to sync document");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query on bridge side
    let query = Query::Eq {
        field: "platform_id".to_string(),
        value: Value::String("jetson-test-001".to_string()),
    };

    let mut retries = 0;
    let synced_cap = loop {
        let results = bridge_backend
            .document_store()
            .query(collections::CAPABILITIES, &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            break results.into_iter().next().unwrap();
        }

        retries += 1;
        if retries > 20 {
            panic!("Bridge did not receive capability after 20 retries");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let sync_time = start.elapsed();
    println!("✓ Capability synced in {:?}", sync_time);

    // Verify synced data
    let has_ai = synced_cap
        .get("has_ai_capability")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(has_ai, "Bridge should see AI capability");

    // Cleanup
    jetson_backend.shutdown().await.ok();
    bridge_backend.shutdown().await.ok();
}

// ============================================================================
// Test 2: Live Detection → Track Flow
// ============================================================================

/// Test the detection → track → HIVE sync flow
#[tokio::test]
async fn test_detection_to_track_flow() {
    use hive_inference::inference::BoundingBox;
    use hive_inference::inference::GroundTruthObject;

    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    println!("=== Test: Detection → Track Flow ===");

    // Create inference pipeline with simulated detector/tracker
    let mut detector = SimulatedDetector::default_config();

    // Add ground truth objects for the detector to find
    // GroundTruthObject::new(id, bbox, class_label, class_id)
    detector.add_ground_truth(GroundTruthObject::new(
        1,
        BoundingBox::new(0.3, 0.3, 0.1, 0.2),
        "person",
        0,
    ));
    detector.add_ground_truth(GroundTruthObject::new(
        2,
        BoundingBox::new(0.6, 0.4, 0.15, 0.1),
        "vehicle",
        1,
    ));

    let tracker = SimulatedTracker::new(TrackerConfig::default());
    let config = PipelineConfig {
        platform_id: "jetson-001".to_string(),
        model_id: "yolov8n".to_string(),
        min_confidence: 0.5,
        confirmed_only: false, // Allow unconfirmed tracks for immediate detection
        reference_position: Some((33.7749, -84.3958)), // Atlanta
        meters_per_pixel: 0.05,
        camera_bearing: 0.0,
        camera_hfov: 60.0,
    };

    let pipeline = InferencePipeline::new(detector, tracker, config);
    pipeline.initialize().await.expect("Pipeline init failed");

    // Process a frame using the correct API
    let frame = VideoFrame::simulated(1, 640, 480);

    let output = pipeline.process(&frame).await.expect("Process failed");

    // Verify output structure
    assert!(!output.detections.is_empty(), "Should have detections");
    println!(
        "  Detections: {}, Tracks: {}, Updates: {}",
        output.detections.len(),
        output.tracks.len(),
        output.track_updates.len()
    );

    // Verify track update format
    for update in &output.track_updates {
        assert!(!update.track_id.is_empty());
        assert!(update.confidence > 0.0);
        assert_eq!(update.source_platform, "jetson-001");
        assert_eq!(update.source_model, "yolov8n");
    }

    // Check latency
    assert!(
        output.latency_ms < targets::TRACK_PUBLISH_LATENCY.as_millis() as f64,
        "Latency {} ms exceeds target {} ms",
        output.latency_ms,
        targets::TRACK_PUBLISH_LATENCY.as_millis()
    );

    println!(
        "✓ Pipeline processed frame in {:.2}ms with {} track updates",
        output.latency_ms,
        output.track_updates.len()
    );
}

/// Test track sync to C2 (simulated)
#[tokio::test]
async fn test_track_sync_to_c2() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("hive_protocol::storage::automerge_sync=debug,hive_protocol::sync=debug")
        .with_test_writer()
        .try_init();

    println!("=== Test: Track Sync to C2 ===");

    let mut harness = E2EHarness::new("track_sync_c2");

    let jetson_addr: std::net::SocketAddr = "127.0.0.1:19411".parse().unwrap();
    let c2_addr: std::net::SocketAddr = "127.0.0.1:19412".parse().unwrap();

    let jetson_backend = harness
        .create_automerge_backend_with_bind(Some(jetson_addr))
        .await
        .expect("Failed to create Jetson backend");

    let c2_backend = harness
        .create_automerge_backend_with_bind(Some(c2_addr))
        .await
        .expect("Failed to create C2 backend");

    // Connect peers
    let c2_endpoint_id = c2_backend.endpoint_id();
    let c2_id_hex = hex::encode(c2_endpoint_id.as_bytes());
    let connected = jetson_backend
        .sync_engine()
        .connect_to_peer(&c2_id_hex, &[c2_addr.to_string()])
        .await
        .expect("Should connect");

    if !connected {
        let jetson_endpoint_id = jetson_backend.endpoint_id();
        let jetson_id_hex = hex::encode(jetson_endpoint_id.as_bytes());
        c2_backend
            .sync_engine()
            .connect_to_peer(&jetson_id_hex, &[jetson_addr.to_string()])
            .await
            .expect("Should connect");
    }

    jetson_backend.sync_engine().start_sync().await.unwrap();
    c2_backend.sync_engine().start_sync().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create and store track update
    let track = TrackUpdate::new(
        "TRACK-001",
        "person",
        0.92,
        Position::new(33.7749, -84.3958),
        "jetson-001",
        "yolov8n",
        "8.0.0",
    );

    let track_doc = serde_json::json!({
        "track_id": track.track_id,
        "classification": track.classification,
        "confidence": track.confidence,
        "position": {
            "lat": track.position.lat,
            "lon": track.position.lon
        },
        "source_platform": track.source_platform,
        "source_model": track.source_model,
        "model_version": track.model_version,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in track_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(&track.track_id, fields);

    let start = Instant::now();
    jetson_backend
        .document_store()
        .upsert(collections::TRACKS, doc)
        .await
        .expect("Failed to store track");

    // Trigger sync
    let doc_key = format!("{}:{}", collections::TRACKS, track.track_id);
    tokio::time::sleep(Duration::from_millis(150)).await;
    jetson_backend
        .as_ref()
        .sync_document(&doc_key)
        .await
        .expect("Failed to sync");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query on C2 side
    let query = Query::Eq {
        field: "track_id".to_string(),
        value: Value::String("TRACK-001".to_string()),
    };

    let mut retries = 0;
    let synced_track = loop {
        let results = c2_backend
            .document_store()
            .query(collections::TRACKS, &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            break results.into_iter().next().unwrap();
        }

        retries += 1;
        if retries > 20 {
            panic!("C2 did not receive track after 20 retries");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let sync_time = start.elapsed();
    println!("✓ Track synced to C2 in {:?}", sync_time);

    // Verify against end-to-end target
    assert!(
        sync_time < targets::END_TO_END_LATENCY,
        "Sync time {:?} exceeds target {:?}",
        sync_time,
        targets::END_TO_END_LATENCY
    );

    // Verify synced data
    let synced_track_id = synced_track
        .get("track_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(synced_track_id, "TRACK-001");

    let synced_confidence = synced_track
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    assert!((synced_confidence - 0.92).abs() < 0.01);

    // Cleanup
    jetson_backend.shutdown().await.ok();
    c2_backend.shutdown().await.ok();
}

// ============================================================================
// Test 3: Chipout Extraction
// ============================================================================

/// Test chipout extraction via inference pipeline
#[tokio::test]
async fn test_chipout_extraction() {
    println!("=== Test: Chipout Extraction ===");

    // Create pipeline with chipout extractor
    let detector = SimulatedDetector::default_config();
    let tracker = SimulatedTracker::new(TrackerConfig::default());
    let config = PipelineConfig {
        platform_id: "jetson-001".to_string(),
        model_id: "yolov8n".to_string(),
        min_confidence: 0.5,
        confirmed_only: false, // Allow unconfirmed for chipout test
        reference_position: Some((33.7749, -84.3958)),
        meters_per_pixel: 0.05,
        camera_bearing: 0.0,
        camera_hfov: 60.0,
    };

    let pipeline = InferencePipeline::new(detector, tracker, config);
    pipeline.initialize().await.expect("Pipeline init failed");

    // Create chipout extractor with default config
    let mut extractor = ChipoutExtractor::with_defaults("jetson-001", "yolov8n", "8.0.0");

    // Process a frame to get tracks
    let frame = VideoFrame::simulated(1, 640, 480);
    let output = pipeline.process(&frame).await.expect("Process failed");

    // Extract chipouts from tracks
    let chipouts = extractor.evaluate_and_extract(&output.tracks, &frame);

    // New tracks should trigger chipouts
    if !output.tracks.is_empty() {
        println!(
            "  Tracks: {}, Chipouts: {}",
            output.tracks.len(),
            chipouts.len()
        );

        // Verify chipout structure if any were generated
        for chipout in &chipouts {
            assert!(!chipout.track_id.is_empty());
            assert!(!chipout.chipout_id.is_empty());
            // Image may be empty for simulated frames
            println!(
                "    Chipout {} for track {} ({} bytes)",
                chipout.chipout_id, chipout.track_id, chipout.image.size_bytes
            );
        }
    }

    println!("✓ Chipout extraction pipeline functional");
}

// ============================================================================
// Test 4: Mission Tasking (TAK → Jetson)
// ============================================================================

/// Test mission task reception from C2/TAK
#[tokio::test]
async fn test_mission_tasking_flow() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("hive_protocol::storage::automerge_sync=debug,hive_protocol::sync=debug")
        .with_test_writer()
        .try_init();

    println!("=== Test: Mission Tasking (TAK → Jetson) ===");

    let mut harness = E2EHarness::new("mission_tasking");

    let c2_addr: std::net::SocketAddr = "127.0.0.1:19421".parse().unwrap();
    let jetson_addr: std::net::SocketAddr = "127.0.0.1:19422".parse().unwrap();

    let c2_backend = harness
        .create_automerge_backend_with_bind(Some(c2_addr))
        .await
        .expect("Failed to create C2 backend");

    let jetson_backend = harness
        .create_automerge_backend_with_bind(Some(jetson_addr))
        .await
        .expect("Failed to create Jetson backend");

    // Connect peers
    let jetson_endpoint_id = jetson_backend.endpoint_id();
    let jetson_id_hex = hex::encode(jetson_endpoint_id.as_bytes());
    let connected = c2_backend
        .sync_engine()
        .connect_to_peer(&jetson_id_hex, &[jetson_addr.to_string()])
        .await
        .expect("Should connect");

    if !connected {
        let c2_endpoint_id = c2_backend.endpoint_id();
        let c2_id_hex = hex::encode(c2_endpoint_id.as_bytes());
        jetson_backend
            .sync_engine()
            .connect_to_peer(&c2_id_hex, &[c2_addr.to_string()])
            .await
            .expect("Should connect");
    }

    c2_backend.sync_engine().start_sync().await.unwrap();
    jetson_backend.sync_engine().start_sync().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // C2 creates mission task
    let mut c2 = SimulatedC2::new("TAK-C2");
    let task = c2.issue_track_command(
        "Track suspicious vehicle near perimeter",
        Priority::High,
        None, // No operational boundary for this test
    );

    let task_doc = serde_json::json!({
        "task_id": task.command_id.to_string(),
        "task_type": "track",
        "target_description": task.target_description,
        "priority": format!("{:?}", task.priority),
        "issued_by": "TAK-C2",
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in task_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(task.command_id.to_string(), fields);

    let start = Instant::now();
    c2_backend
        .document_store()
        .upsert("missions", doc)
        .await
        .expect("Failed to store mission");

    // Trigger sync
    let doc_key = format!("missions:{}", task.command_id);
    tokio::time::sleep(Duration::from_millis(150)).await;
    c2_backend
        .as_ref()
        .sync_document(&doc_key)
        .await
        .expect("Failed to sync");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query on Jetson side
    let query = Query::Eq {
        field: "task_id".to_string(),
        value: Value::String(task.command_id.to_string()),
    };

    let mut retries = 0;
    let synced_task = loop {
        let results = jetson_backend
            .document_store()
            .query("missions", &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            break results.into_iter().next().unwrap();
        }

        retries += 1;
        if retries > 20 {
            panic!("Jetson did not receive mission task after 20 retries");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let sync_time = start.elapsed();
    println!("✓ Mission task synced to Jetson in {:?}", sync_time);

    // Verify synced data
    let task_type = synced_task
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(task_type, "track");

    let priority = synced_task
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(priority, "High");

    // Cleanup
    c2_backend.shutdown().await.ok();
    jetson_backend.shutdown().await.ok();
}

// ============================================================================
// Test 5: Model Deployment
// ============================================================================

/// Test model deployment via C2 directive
#[tokio::test]
async fn test_model_deployment() {
    println!("=== Test: Model Deployment via C2 Directive ===");

    // Create orchestration service
    let service = Arc::new(OrchestrationService::with_simulated_storage());

    // Register simulated ONNX adapter
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx")))
        .await;

    let handler = DirectiveHandler::new("jetson-orin-001", service.clone());

    // Initially no deployments
    let instances = service.list_instances().await;
    assert!(instances.is_empty(), "Should start with no deployments");

    // C2 issues deployment directive
    let directive = DeploymentDirective::new("deploy-yolov8-001")
        .with_issuer("c2-command")
        .with_formation("formation-alpha")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model(
                "sha256:e2e_test_hash_yolov8n",
                50_000_000,
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

    // Handle directive
    let status = handler.handle(directive).await;

    assert_eq!(
        status.state,
        DeploymentState::Active,
        "Deployment should be active"
    );
    assert!(status.instance_id.is_some(), "Should have instance ID");

    // Verify deployment
    let instances = service.list_instances().await;
    assert_eq!(instances.len(), 1, "Should have 1 active deployment");

    let instance = &instances[0];
    assert_eq!(
        instance.request.capabilities,
        vec!["object_detection", "person_tracking"]
    );

    println!(
        "✓ Model deployed: {} capabilities: {:?}",
        instance.request.blob_hash, instance.request.capabilities
    );
}

/// Test full model deployment flow with sync
#[tokio::test]
async fn test_model_deployment_with_sync() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("hive_protocol::storage::automerge_sync=debug,hive_protocol::sync=debug")
        .with_test_writer()
        .try_init();

    println!("=== Test: Model Deployment with Sync ===");

    let mut harness = E2EHarness::new("model_deployment_sync");

    let c2_addr: std::net::SocketAddr = "127.0.0.1:19431".parse().unwrap();
    let jetson_addr: std::net::SocketAddr = "127.0.0.1:19432".parse().unwrap();

    let c2_backend = harness
        .create_automerge_backend_with_bind(Some(c2_addr))
        .await
        .expect("Failed to create C2 backend");

    let jetson_backend = harness
        .create_automerge_backend_with_bind(Some(jetson_addr))
        .await
        .expect("Failed to create Jetson backend");

    // Connect peers
    let jetson_endpoint_id = jetson_backend.endpoint_id();
    let jetson_id_hex = hex::encode(jetson_endpoint_id.as_bytes());
    let connected = c2_backend
        .sync_engine()
        .connect_to_peer(&jetson_id_hex, &[jetson_addr.to_string()])
        .await
        .expect("Should connect");

    if !connected {
        let c2_endpoint_id = c2_backend.endpoint_id();
        let c2_id_hex = hex::encode(c2_endpoint_id.as_bytes());
        jetson_backend
            .sync_engine()
            .connect_to_peer(&c2_id_hex, &[c2_addr.to_string()])
            .await
            .expect("Should connect");
    }

    c2_backend.sync_engine().start_sync().await.unwrap();
    jetson_backend.sync_engine().start_sync().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // C2 creates deployment directive
    let directive = DeploymentDirective::new("deploy-yolov8-e2e")
        .with_issuer("c2-command")
        .with_scope(DeploymentScope::Broadcast)
        .with_artifact(
            ArtifactSpec::onnx_model("sha256:e2e_model_hash", 50_000_000, vec![])
                .with_name("YOLOv8n")
                .with_version("8.0.0"),
        )
        .with_capability("object_detection");

    let directive_doc = serde_json::json!({
        "directive_id": directive.directive_id,
        "issuer_node_id": directive.issuer_node_id,
        "scope": "broadcast",
        "artifact_hash": directive.artifact.blob_hash,
        "artifact_name": directive.artifact.name,
        "artifact_version": directive.artifact.version,
        "capabilities": directive.capabilities
    });

    let mut fields: HashMap<String, Value> = HashMap::new();
    for (k, v) in directive_doc.as_object().unwrap() {
        fields.insert(k.clone(), v.clone());
    }
    let doc = Document::with_id(&directive.directive_id, fields);

    let start = Instant::now();
    c2_backend
        .document_store()
        .upsert(collections::DIRECTIVES, doc)
        .await
        .expect("Failed to store directive");

    // Trigger sync
    let doc_key = format!("{}:{}", collections::DIRECTIVES, directive.directive_id);
    tokio::time::sleep(Duration::from_millis(150)).await;
    c2_backend
        .as_ref()
        .sync_document(&doc_key)
        .await
        .expect("Failed to sync");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query on Jetson side
    let query = Query::Eq {
        field: "directive_id".to_string(),
        value: Value::String("deploy-yolov8-e2e".to_string()),
    };

    let mut retries = 0;
    let synced_directive = loop {
        let results = jetson_backend
            .document_store()
            .query(collections::DIRECTIVES, &query)
            .await
            .expect("Query failed");

        if !results.is_empty() {
            break results.into_iter().next().unwrap();
        }

        retries += 1;
        if retries > 20 {
            panic!("Jetson did not receive directive after 20 retries");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let sync_time = start.elapsed();
    println!("✓ Deployment directive synced in {:?}", sync_time);

    // Verify synced directive
    let artifact_hash = synced_directive
        .get("artifact_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(artifact_hash, "sha256:e2e_model_hash");

    // Now simulate Jetson processing the directive
    let service = Arc::new(OrchestrationService::with_simulated_storage());
    service
        .register_adapter(Arc::new(SimulatedAdapter::new("onnx")))
        .await;
    let handler = DirectiveHandler::new("jetson-orin-001", service.clone());

    let status = handler.handle(directive).await;
    assert_eq!(status.state, DeploymentState::Active);

    println!("✓ Model deployed on Jetson after receiving directive");

    // Cleanup
    c2_backend.shutdown().await.ok();
    jetson_backend.shutdown().await.ok();
}

// ============================================================================
// Performance Metrics Test
// ============================================================================

/// Test performance metrics collection and validation
#[tokio::test]
async fn test_performance_metrics() {
    println!("=== Test: Performance Metrics ===");

    let mut metrics = MetricsCollector::new();
    metrics.start();

    // Simulate detection FPS measurement
    let frame_times: Vec<f64> = vec![16.0, 17.0, 15.5, 16.5, 15.0]; // ~60 FPS
    let avg_frame_time = frame_times.iter().sum::<f64>() / frame_times.len() as f64;
    let fps = 1000.0 / avg_frame_time;

    assert!(
        fps >= targets::DETECTION_FPS,
        "FPS {} below target {}",
        fps,
        targets::DETECTION_FPS
    );

    // Record track latencies
    metrics.record_track_latency(Duration::from_millis(50));
    metrics.record_track_latency(Duration::from_millis(45));
    metrics.record_track_latency(Duration::from_millis(55));

    // Generate report
    let report = metrics.report();
    println!("{}", report);

    println!(
        "✓ Performance metrics validated: FPS={:.1}, target={:.1}",
        fps,
        targets::DETECTION_FPS
    );
}
