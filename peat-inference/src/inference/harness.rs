//! Test harness for inference pipeline validation
//!
//! Provides pre-built scenarios for testing the full inference pipeline:
//! - Single person walking across frame
//! - Multiple people with crossings
//! - Vehicle tracking
//! - Track handoff simulation
//! - Occlusion and reacquisition

use super::detector::{GroundTruthObject, SimulatedDetector, SimulatedDetectorConfig};
use super::pipeline::{InferencePipeline, PipelineConfig, PipelineOutput};
use super::tracker::{SimulatedTracker, TrackerConfig};
use super::types::{BoundingBox, VideoFrame};
use crate::messages::TrackUpdate;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

/// Scenario configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Scenario name
    pub name: String,
    /// Frame dimensions
    pub frame_width: u32,
    pub frame_height: u32,
    /// Number of frames to simulate
    pub num_frames: u64,
    /// Target FPS
    pub target_fps: f64,
    /// Reference geographic position
    pub reference_position: (f64, f64),
    /// Camera bearing in degrees
    pub camera_bearing: f64,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            frame_width: 1920,
            frame_height: 1080,
            num_frames: 300, // 10 seconds at 30 FPS
            target_fps: 30.0,
            reference_position: (33.7749, -84.3958), // Atlanta
            camera_bearing: 0.0,
        }
    }
}

/// Pre-built test scenarios
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Single person walking left to right
    SinglePersonWalking,
    /// Two people crossing paths
    TwoPeopleCrossing,
    /// Person stops, pauses, continues
    PersonWithPause,
    /// Multiple people and vehicles
    MixedTraffic,
    /// Track appears, gets occluded, reappears
    OcclusionRecovery,
    /// Track exits one team's view, enters another
    HandoffSimulation,
    /// High density scenario with many objects
    CrowdedScene,
    /// Fast moving vehicle
    VehicleTracking,
}

impl Scenario {
    /// Get scenario configuration
    pub fn config(&self) -> ScenarioConfig {
        match self {
            Scenario::SinglePersonWalking => ScenarioConfig {
                name: "single_person_walking".to_string(),
                num_frames: 150, // 5 seconds
                ..Default::default()
            },
            Scenario::TwoPeopleCrossing => ScenarioConfig {
                name: "two_people_crossing".to_string(),
                num_frames: 200,
                ..Default::default()
            },
            Scenario::PersonWithPause => ScenarioConfig {
                name: "person_with_pause".to_string(),
                num_frames: 300,
                ..Default::default()
            },
            Scenario::MixedTraffic => ScenarioConfig {
                name: "mixed_traffic".to_string(),
                num_frames: 300,
                ..Default::default()
            },
            Scenario::OcclusionRecovery => ScenarioConfig {
                name: "occlusion_recovery".to_string(),
                num_frames: 200,
                ..Default::default()
            },
            Scenario::HandoffSimulation => ScenarioConfig {
                name: "handoff_simulation".to_string(),
                num_frames: 300,
                ..Default::default()
            },
            Scenario::CrowdedScene => ScenarioConfig {
                name: "crowded_scene".to_string(),
                num_frames: 300,
                ..Default::default()
            },
            Scenario::VehicleTracking => ScenarioConfig {
                name: "vehicle_tracking".to_string(),
                num_frames: 150,
                target_fps: 30.0,
                ..Default::default()
            },
        }
    }

    /// Create ground truth objects for the scenario
    pub fn create_ground_truth(&self) -> Vec<GroundTruthObject> {
        match self {
            Scenario::SinglePersonWalking => {
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.0, 0.4, 0.05, 0.15), "person", 0)
                        .with_velocity(0.005, 0.0),
                ] // Walking right
            }

            Scenario::TwoPeopleCrossing => {
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.1, 0.3, 0.05, 0.15), "person", 0)
                        .with_velocity(0.004, 0.002), // Walking right and down
                    GroundTruthObject::new(2, BoundingBox::new(0.8, 0.5, 0.05, 0.15), "person", 0)
                        .with_velocity(-0.004, -0.001), // Walking left and up
                ]
            }

            Scenario::PersonWithPause => {
                // Person starts, pauses, then continues
                // Velocity will be modified during scenario run
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.1, 0.4, 0.05, 0.15), "person", 0)
                        .with_velocity(0.004, 0.0),
                ]
            }

            Scenario::MixedTraffic => {
                vec![
                    // Pedestrians
                    GroundTruthObject::new(1, BoundingBox::new(0.1, 0.6, 0.04, 0.12), "person", 0)
                        .with_velocity(0.003, 0.0),
                    GroundTruthObject::new(2, BoundingBox::new(0.3, 0.5, 0.04, 0.12), "person", 0)
                        .with_velocity(0.002, 0.001),
                    // Vehicle (larger, faster)
                    GroundTruthObject::new(3, BoundingBox::new(0.0, 0.3, 0.15, 0.08), "car", 2)
                        .with_velocity(0.015, 0.0),
                    // Bicycle
                    GroundTruthObject::new(4, BoundingBox::new(0.7, 0.55, 0.06, 0.1), "bicycle", 1)
                        .with_velocity(-0.008, 0.0),
                ]
            }

            Scenario::OcclusionRecovery => {
                // Person walks behind obstacle (simulated by toggling visibility)
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.1, 0.4, 0.05, 0.15), "person", 0)
                        .with_velocity(0.004, 0.0),
                ]
            }

            Scenario::HandoffSimulation => {
                // Person walks across entire frame (simulating moving between team coverage areas)
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.0, 0.4, 0.05, 0.15), "person", 0)
                        .with_velocity(0.003, 0.0),
                ]
            }

            Scenario::CrowdedScene => (0..10)
                .map(|i| {
                    let x = 0.1 + ((i % 5) as f32) * 0.15;
                    let y = 0.3 + ((i / 5) as f32) * 0.25;
                    let vx = ((i % 3) as f32 - 1.0) * 0.002;
                    let vy = ((i % 2) as f32 - 0.5) * 0.001;

                    GroundTruthObject::new(
                        i as u32 + 1,
                        BoundingBox::new(x, y, 0.04, 0.12),
                        "person",
                        0,
                    )
                    .with_velocity(vx, vy)
                })
                .collect(),

            Scenario::VehicleTracking => {
                vec![
                    GroundTruthObject::new(1, BoundingBox::new(0.0, 0.35, 0.12, 0.06), "car", 2)
                        .with_velocity(0.02, 0.0),
                ] // Fast moving vehicle
            }
        }
    }
}

/// Inference test harness
pub struct InferenceHarness {
    scenario: Scenario,
    config: ScenarioConfig,
    ground_truth: Vec<GroundTruthObject>,
    pipeline: InferencePipeline<SimulatedDetector, SimulatedTracker>,
    frame_count: u64,
    outputs: Vec<PipelineOutput>,
    all_track_updates: Vec<TrackUpdate>,
}

impl InferenceHarness {
    /// Create a new test harness for a scenario
    pub fn new(scenario: Scenario) -> Self {
        let config = scenario.config();
        let ground_truth = scenario.create_ground_truth();

        // Create detector with scenario ground truth
        let detector_config = SimulatedDetectorConfig {
            latency: Duration::from_millis(
                (1000.0 / config.target_fps * 0.5) as u64, // Half frame time for detection
            ),
            precision: 0.91,
            recall: 0.87,
            confidence_noise: 0.05,
            position_noise: 0.01,
            false_positive_rate: 0.02,
            ..Default::default()
        };

        let mut detector = SimulatedDetector::new(detector_config);
        detector.set_ground_truth(ground_truth.clone());

        // Create tracker
        let tracker_config = TrackerConfig {
            min_hits: 3,
            max_age: 30,
            max_lost_age: 60,
            iou_threshold: 0.3,
            reid_threshold: 0.5,
            appearance_weight: 0.5,
        };
        let tracker = SimulatedTracker::new(tracker_config);

        // Create pipeline
        let pipeline_config = PipelineConfig {
            platform_id: "Test-Platform".to_string(),
            model_id: "Test-Model".to_string(),
            reference_position: Some(config.reference_position),
            camera_bearing: config.camera_bearing,
            ..Default::default()
        };
        let pipeline = InferencePipeline::new(detector, tracker, pipeline_config);

        Self {
            scenario,
            config,
            ground_truth,
            pipeline,
            frame_count: 0,
            outputs: Vec::new(),
            all_track_updates: Vec::new(),
        }
    }

    /// Initialize the harness
    pub async fn initialize(&self) -> anyhow::Result<()> {
        self.pipeline.initialize().await
    }

    /// Run the scenario for one frame
    pub async fn step(&mut self) -> anyhow::Result<PipelineOutput> {
        // Update ground truth positions
        self.update_ground_truth();

        // Sync ground truth to detector
        {
            let mut detector = self.pipeline.detector.lock().await;
            detector.set_ground_truth(self.ground_truth.clone());
        }

        // Create frame
        let frame = VideoFrame::simulated(
            self.frame_count,
            self.config.frame_width,
            self.config.frame_height,
        );

        // Process frame
        let output = self.pipeline.process(&frame).await?;

        // Store output
        self.outputs.push(output.clone());
        self.all_track_updates.extend(output.track_updates.clone());

        self.frame_count += 1;

        Ok(output)
    }

    /// Update ground truth based on scenario-specific logic
    fn update_ground_truth(&mut self) {
        match self.scenario {
            Scenario::PersonWithPause => {
                // Pause between frames 60-90
                if let Some(gt) = self.ground_truth.get_mut(0) {
                    if self.frame_count >= 60 && self.frame_count < 90 {
                        gt.velocity = (0.0, 0.0);
                    } else {
                        gt.velocity = (0.004, 0.0);
                    }
                }
            }
            Scenario::OcclusionRecovery => {
                // Occlude between frames 50-80
                if let Some(gt) = self.ground_truth.get_mut(0) {
                    gt.visible = !(self.frame_count >= 50 && self.frame_count < 80);
                }
            }
            _ => {}
        }

        // Step all ground truth objects
        for gt in &mut self.ground_truth {
            gt.step();
        }
    }

    /// Run the full scenario
    pub async fn run(&mut self) -> anyhow::Result<HarnessResults> {
        self.initialize().await?;

        let frame_delay = Duration::from_secs_f64(1.0 / self.config.target_fps);

        for _ in 0..self.config.num_frames {
            self.step().await?;
            sleep(frame_delay).await;
        }

        Ok(self.results().await)
    }

    /// Run scenario at maximum speed (no frame delay)
    pub async fn run_fast(&mut self) -> anyhow::Result<HarnessResults> {
        self.initialize().await?;

        for _ in 0..self.config.num_frames {
            self.step().await?;
        }

        Ok(self.results().await)
    }

    /// Get results summary
    pub async fn results(&self) -> HarnessResults {
        let metrics = self.pipeline.metrics_summary().await;
        let tracker_stats = self.pipeline.tracker_stats().await;

        HarnessResults {
            scenario: self.scenario,
            config: self.config.clone(),
            frames_processed: self.frame_count,
            total_detections: metrics.total_detections,
            total_tracks_created: tracker_stats.total_tracks_created,
            total_track_updates: self.all_track_updates.len() as u64,
            avg_fps: metrics.avg_fps,
            avg_detection_latency_ms: metrics.detection_latency.mean_ms,
            avg_tracking_latency_ms: metrics.tracking_latency.mean_ms,
            avg_pipeline_latency_ms: metrics.pipeline_latency.mean_ms,
            p95_pipeline_latency_ms: metrics.pipeline_latency.p95_ms,
            track_updates: self.all_track_updates.clone(),
        }
    }

    /// Get all outputs
    pub fn outputs(&self) -> &[PipelineOutput] {
        &self.outputs
    }

    /// Get current frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

/// Results from running a scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessResults {
    pub scenario: Scenario,
    pub config: ScenarioConfig,
    pub frames_processed: u64,
    pub total_detections: u64,
    pub total_tracks_created: u64,
    pub total_track_updates: u64,
    pub avg_fps: f64,
    pub avg_detection_latency_ms: f64,
    pub avg_tracking_latency_ms: f64,
    pub avg_pipeline_latency_ms: f64,
    pub p95_pipeline_latency_ms: f64,
    pub track_updates: Vec<TrackUpdate>,
}

impl std::fmt::Display for HarnessResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Harness Results: {:?} ===", self.scenario)?;
        writeln!(f, "Frames: {}", self.frames_processed)?;
        writeln!(f, "Detections: {}", self.total_detections)?;
        writeln!(f, "Tracks created: {}", self.total_tracks_created)?;
        writeln!(f, "Track updates emitted: {}", self.total_track_updates)?;
        writeln!(f, "Avg FPS: {:.1}", self.avg_fps)?;
        writeln!(
            f,
            "Latency - Detection: {:.1}ms, Tracking: {:.1}ms, Pipeline: {:.1}ms (P95: {:.1}ms)",
            self.avg_detection_latency_ms,
            self.avg_tracking_latency_ms,
            self.avg_pipeline_latency_ms,
            self.p95_pipeline_latency_ms
        )?;
        Ok(())
    }
}

// Implement Serialize for Scenario
impl Serialize for Scenario {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let name = match self {
            Scenario::SinglePersonWalking => "single_person_walking",
            Scenario::TwoPeopleCrossing => "two_people_crossing",
            Scenario::PersonWithPause => "person_with_pause",
            Scenario::MixedTraffic => "mixed_traffic",
            Scenario::OcclusionRecovery => "occlusion_recovery",
            Scenario::HandoffSimulation => "handoff_simulation",
            Scenario::CrowdedScene => "crowded_scene",
            Scenario::VehicleTracking => "vehicle_tracking",
        };
        serializer.serialize_str(name)
    }
}

impl<'de> Deserialize<'de> for Scenario {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "single_person_walking" => Ok(Scenario::SinglePersonWalking),
            "two_people_crossing" => Ok(Scenario::TwoPeopleCrossing),
            "person_with_pause" => Ok(Scenario::PersonWithPause),
            "mixed_traffic" => Ok(Scenario::MixedTraffic),
            "occlusion_recovery" => Ok(Scenario::OcclusionRecovery),
            "handoff_simulation" => Ok(Scenario::HandoffSimulation),
            "crowded_scene" => Ok(Scenario::CrowdedScene),
            "vehicle_tracking" => Ok(Scenario::VehicleTracking),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &[
                    "single_person_walking",
                    "two_people_crossing",
                    "person_with_pause",
                    "mixed_traffic",
                    "occlusion_recovery",
                    "handoff_simulation",
                    "crowded_scene",
                    "vehicle_tracking",
                ],
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_person_scenario() {
        let mut harness = InferenceHarness::new(Scenario::SinglePersonWalking);
        let results = harness.run_fast().await.unwrap();

        println!("{}", results);

        assert!(results.frames_processed > 0);
        assert!(results.total_tracks_created > 0);
        assert!(results.total_track_updates > 0);
    }

    #[tokio::test]
    async fn test_two_people_crossing() {
        let mut harness = InferenceHarness::new(Scenario::TwoPeopleCrossing);
        let results = harness.run_fast().await.unwrap();

        println!("{}", results);

        // Should create at least 2 tracks
        assert!(results.total_tracks_created >= 2);
    }

    #[tokio::test]
    async fn test_mixed_traffic() {
        let mut harness = InferenceHarness::new(Scenario::MixedTraffic);
        let results = harness.run_fast().await.unwrap();

        println!("{}", results);

        // Should have people and vehicles
        let has_person = results
            .track_updates
            .iter()
            .any(|u| u.classification == "person");
        let has_car = results
            .track_updates
            .iter()
            .any(|u| u.classification == "car");

        assert!(has_person);
        assert!(has_car);
    }

    #[tokio::test]
    async fn test_crowded_scene() {
        let mut harness = InferenceHarness::new(Scenario::CrowdedScene);
        let results = harness.run_fast().await.unwrap();

        println!("{}", results);

        // Should handle many objects
        assert!(results.total_tracks_created >= 5);
    }

    #[tokio::test]
    async fn test_occlusion_recovery() {
        let mut harness = InferenceHarness::new(Scenario::OcclusionRecovery);
        let results = harness.run_fast().await.unwrap();

        println!("{}", results);

        // Track should be created, lost during occlusion, ideally recovered
        // Due to re-ID, we might see 1 track (recovered) or 2 tracks (new after occlusion)
        assert!(results.total_tracks_created >= 1);
    }

    #[tokio::test]
    async fn test_step_by_step() {
        let mut harness = InferenceHarness::new(Scenario::SinglePersonWalking);
        harness.initialize().await.unwrap();

        // Step through first 10 frames
        for _ in 0..10 {
            let output = harness.step().await.unwrap();
            println!(
                "Frame {}: {} detections, {} tracks",
                output.frame_id,
                output.detections.len(),
                output.tracks.len()
            );
        }

        assert_eq!(harness.frame_count(), 10);
    }

    #[tokio::test]
    async fn test_track_update_content() {
        let mut harness = InferenceHarness::new(Scenario::SinglePersonWalking);
        let results = harness.run_fast().await.unwrap();

        // Check that track updates have valid content
        for update in &results.track_updates {
            assert!(!update.track_id.is_empty());
            assert!(!update.classification.is_empty());
            assert!(update.confidence > 0.0 && update.confidence <= 1.0);
            assert!(update.position.lat.abs() < 90.0);
            assert!(update.position.lon.abs() < 180.0);
        }
    }
}
