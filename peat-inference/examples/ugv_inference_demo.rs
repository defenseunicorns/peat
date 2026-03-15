//! UGV + Inference Integration Demo
//!
//! Demonstrates the full M1 vignette scenario where:
//! 1. UGV patrols an area while running YOLOv8 inference
//! 2. When a person is detected, the UGV pursues the target
//! 3. Track updates and UGV position are published to Peat
//!
//! Run with: cargo run --example ugv_inference_demo
//!
//! This example demonstrates issue #331 requirements:
//! - UGV client runs alongside inference pipeline
//! - Detection events trigger UGV behavior
//! - UGV position correlates with camera FOV

use peat_inference::{
    inference::{
        BoundingBox, GroundTruthObject, InferencePipeline, PipelineConfig, SimulatedDetector,
        SimulatedDetectorConfig, SimulatedTracker, TrackerConfig, VideoFrame,
    },
    MissionCommand, PatrolPattern, UgvClient, UgvConfig,
};
use std::time::Duration;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== Peat UGV + Inference Integration Demo ===");
    info!("Demonstrating M1 vignette: detection-triggered UGV pursuit\n");

    // =========================================================================
    // Setup: Create UGV and Inference Pipeline
    // =========================================================================

    // UGV Configuration - patrol area in Atlanta
    let patrol_waypoints = vec![
        (33.7749, -84.3958), // Start/base
        (33.7755, -84.3950), // NE corner
        (33.7760, -84.3955), // N point
        (33.7758, -84.3962), // NW corner
    ];

    let ugv_config = UgvConfig::new("UGV-Alpha-1")
        .with_position(33.7749, -84.3958)
        .with_base(33.7749, -84.3958)
        .with_waypoints(patrol_waypoints.clone())
        .with_speed(3.0); // 3 m/s patrol speed

    let mut ugv = UgvClient::new(ugv_config);

    // Inference Pipeline Configuration
    // Reference position matches UGV start for realistic coordinate mapping
    let pipeline_config = PipelineConfig {
        platform_id: ugv.platform_id().to_string(),
        model_id: "YOLOv8n".to_string(),
        min_confidence: 0.5,
        confirmed_only: true,
        reference_position: Some((33.7749, -84.3958)),
        meters_per_pixel: 0.05,
        camera_bearing: 0.0, // Forward-facing camera
        camera_hfov: 62.0,   // Typical webcam FOV
    };

    // Simulated detector with realistic latency
    let detector_config = SimulatedDetectorConfig {
        latency: Duration::from_millis(30), // ~30ms inference
        recall: 0.9,
        false_positive_rate: 0.05,
        ..Default::default()
    };

    // Tracker config tuned for vehicles
    let tracker_config = TrackerConfig {
        min_hits: 3,        // Require 3 frames to confirm
        max_age: 30,        // Lose track after 30 missed frames
        iou_threshold: 0.3, // IoU for matching
        ..Default::default()
    };

    let detector = SimulatedDetector::new(detector_config);
    let tracker = SimulatedTracker::new(tracker_config);
    let pipeline = InferencePipeline::new(detector, tracker, pipeline_config);

    pipeline.initialize().await?;
    info!("Inference pipeline initialized\n");

    // Target classes for UGV to pursue
    let target_classes = vec!["person".to_string()];

    // =========================================================================
    // Phase 1: Patrol (10 seconds)
    // =========================================================================

    info!("=== Phase 1: UGV Patrol ===");
    ugv.handle_mission(MissionCommand::SearchArea {
        boundary: patrol_waypoints,
        patrol_pattern: PatrolPattern::Sequential,
    });

    let update_interval = Duration::from_millis(100);
    let mut frame_id = 0u64;
    let mut elapsed = Duration::ZERO;

    // Patrol for 10 seconds with no detections
    while elapsed < Duration::from_secs(10) {
        // Update UGV position
        ugv.update(update_interval);

        // Process a frame (no ground truth = no detections)
        let frame = VideoFrame::simulated(frame_id, 1920, 1080);
        let output = pipeline.process(&frame).await?;

        // Log every 2 seconds
        if elapsed.as_secs() % 2 == 0 && elapsed.subsec_millis() < 100 {
            let (lat, lon) = ugv.position();
            info!(
                "[{:>5.1}s] {} | Pos: ({:.5}, {:.5}) | Detections: {}",
                elapsed.as_secs_f32(),
                ugv.state(),
                lat,
                lon,
                output.track_updates.len()
            );
        }

        frame_id += 1;
        elapsed += update_interval;
    }

    // =========================================================================
    // Phase 2: Person Detected - UGV Pursuit (15 seconds)
    // =========================================================================

    info!("\n=== Phase 2: Person Detected! ===");

    // Add a "person" ground truth for the detector
    // Position them ahead of UGV patrol route
    {
        let mut detector = pipeline.detector.lock().await;
        let person = GroundTruthObject::new(
            1,
            BoundingBox::new(0.6, 0.5, 0.08, 0.25), // Right side of frame
            "person",
            0,
        );
        detector.add_ground_truth(person);
        info!("Person ground truth added to simulation");
    }

    // Continue simulation - detect and pursue
    let pursuit_start = elapsed;
    while elapsed < pursuit_start + Duration::from_secs(15) {
        // Update UGV position
        ugv.update(update_interval);

        // Process frame - now should detect person
        let frame = VideoFrame::simulated(frame_id, 1920, 1080);
        let output = pipeline.process(&frame).await?;

        // Feed detections to UGV
        for track_update in &output.track_updates {
            if ugv.handle_detection(track_update, &target_classes) {
                info!(
                    "  -> UGV pursuing {} at ({:.5}, {:.5})",
                    track_update.classification,
                    track_update.position.lat,
                    track_update.position.lon
                );
            }
        }

        // Log every 2 seconds
        if (elapsed - pursuit_start).as_secs() % 2 == 0
            && (elapsed - pursuit_start).subsec_millis() < 100
        {
            let (lat, lon) = ugv.position();
            info!(
                "[{:>5.1}s] {} | Pos: ({:.5}, {:.5}) | Tracks: {}",
                elapsed.as_secs_f32(),
                ugv.state(),
                lat,
                lon,
                output.tracks.len()
            );
        }

        frame_id += 1;
        elapsed += update_interval;
    }

    // =========================================================================
    // Phase 3: Target Lost - Return to Base (5 seconds)
    // =========================================================================

    info!("\n=== Phase 3: Target Lost - RTB ===");

    // Remove ground truth (person left scene)
    {
        let mut detector = pipeline.detector.lock().await;
        detector.set_ground_truth(vec![]);
        info!("Person left scene - target lost");
    }

    // Issue abort/RTB command
    ugv.handle_mission(MissionCommand::Abort);

    let rtb_start = elapsed;
    while elapsed < rtb_start + Duration::from_secs(5) {
        ugv.update(update_interval);

        // Still process frames but no detections
        let frame = VideoFrame::simulated(frame_id, 1920, 1080);
        let _output = pipeline.process(&frame).await?;

        if (elapsed - rtb_start).as_secs() % 2 == 0 && (elapsed - rtb_start).subsec_millis() < 100 {
            let (lat, lon) = ugv.position();
            info!(
                "[{:>5.1}s] {} | Pos: ({:.5}, {:.5}) | Heading: {:.1}°",
                elapsed.as_secs_f32(),
                ugv.state(),
                lat,
                lon,
                ugv.heading()
            );
        }

        frame_id += 1;
        elapsed += update_interval;
    }

    // =========================================================================
    // Summary
    // =========================================================================

    info!("\n=== Demo Complete ===");

    // Pipeline statistics
    let summary = pipeline.metrics_summary().await;
    let tracker_stats = pipeline.tracker_stats().await;

    info!("Pipeline Statistics:");
    info!("  Total frames: {}", summary.total_frames);
    info!("  Avg FPS: {:.1}", summary.avg_fps);
    info!(
        "  Total tracks created: {}",
        tracker_stats.total_tracks_created
    );

    info!("\nUGV Final State:");
    let (lat, lon) = ugv.position();
    info!("  Position: ({:.5}, {:.5})", lat, lon);
    info!("  State: {}", ugv.state());
    info!("  Battery: {:.1}%", ugv.battery_level() * 100.0);

    // Output sample track update JSON
    info!("\n=== Sample UGV TrackUpdate (for Peat) ===");
    let ugv_track = ugv.get_position_update();
    println!("{}", serde_json::to_string_pretty(&ugv_track)?);

    Ok(())
}
