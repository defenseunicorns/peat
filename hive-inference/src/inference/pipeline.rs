//! Inference pipeline connecting detector → tracker → TrackUpdate
//!
//! The pipeline orchestrates the full inference flow:
//! 1. Receive video frame
//! 2. Run object detection
//! 3. Update multi-object tracker
//! 4. Convert tracks to HIVE TrackUpdate messages
//! 5. Collect performance metrics

use super::detector::{Detection, Detector};
use super::metrics::InferenceMetrics;
use super::tracker::{Track, Tracker};
use super::types::VideoFrame;
use crate::messages::{Position, TrackUpdate, Velocity};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Platform ID for source attribution
    pub platform_id: String,
    /// Model ID for source attribution
    pub model_id: String,
    /// Minimum confidence to emit track update
    pub min_confidence: f32,
    /// Only emit confirmed tracks
    pub confirmed_only: bool,
    /// Geographic reference point (lat, lon) for image-to-world conversion
    pub reference_position: Option<(f64, f64)>,
    /// Meters per pixel (approximate, for converting bbox to world coords)
    pub meters_per_pixel: f64,
    /// Camera bearing in degrees (0 = North)
    pub camera_bearing: f64,
    /// Camera horizontal field of view in degrees
    pub camera_hfov: f64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            platform_id: "Alpha-2".to_string(),
            model_id: "Alpha-3".to_string(),
            min_confidence: 0.5,
            confirmed_only: true,
            reference_position: None,
            meters_per_pixel: 0.05, // 5cm per pixel at typical range
            camera_bearing: 0.0,
            camera_hfov: 60.0,
        }
    }
}

/// Output from a pipeline processing step
#[derive(Debug, Clone)]
pub struct PipelineOutput {
    /// Frame that was processed
    pub frame_id: u64,
    /// Raw detections from detector
    pub detections: Vec<Detection>,
    /// Active tracks from tracker
    pub tracks: Vec<Track>,
    /// HIVE TrackUpdate messages
    pub track_updates: Vec<TrackUpdate>,
    /// Processing latency in milliseconds
    pub latency_ms: f64,
}

/// The inference pipeline
pub struct InferencePipeline<D: Detector, T: Tracker> {
    /// Object detector (public for test harness access)
    pub detector: Arc<Mutex<D>>,
    /// Multi-object tracker
    pub tracker: Arc<Mutex<T>>,
    config: PipelineConfig,
    metrics: Arc<Mutex<InferenceMetrics>>,
    model_version: String,
}

impl<D: Detector, T: Tracker> InferencePipeline<D, T> {
    /// Create a new inference pipeline
    pub fn new(detector: D, tracker: T, config: PipelineConfig) -> Self {
        let model_version = detector.model_info().model_version.clone();

        Self {
            detector: Arc::new(Mutex::new(detector)),
            tracker: Arc::new(Mutex::new(tracker)),
            config,
            metrics: Arc::new(Mutex::new(InferenceMetrics::default())),
            model_version,
        }
    }

    /// Initialize the pipeline (warm up detector)
    pub async fn initialize(&self) -> anyhow::Result<()> {
        let mut detector = self.detector.lock().await;
        detector.warm_up().await?;
        Ok(())
    }

    /// Check if pipeline is ready
    pub async fn is_ready(&self) -> bool {
        let detector = self.detector.lock().await;
        detector.is_ready()
    }

    /// Process a single frame through the pipeline
    pub async fn process(&self, frame: &VideoFrame) -> anyhow::Result<PipelineOutput> {
        let start = Instant::now();

        // Record frame for FPS calculation
        {
            let mut metrics = self.metrics.lock().await;
            metrics.record_frame();
        }

        // Run detection
        let detection_start = Instant::now();
        let detections = {
            let mut detector = self.detector.lock().await;
            detector.detect(frame).await?
        };
        let detection_latency = detection_start.elapsed().as_secs_f64() * 1000.0;

        // Record detection metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.record_detection_latency(detection_latency, detections.len());
        }

        // Run tracking
        let tracking_start = Instant::now();
        let tracks = {
            let mut tracker = self.tracker.lock().await;
            tracker.update(detections.clone()).await?
        };
        let tracking_latency = tracking_start.elapsed().as_secs_f64() * 1000.0;

        // Record tracking metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.record_tracking_latency(tracking_latency, tracks.len());
        }

        // Convert tracks to TrackUpdate messages
        let track_updates = self.tracks_to_updates(&tracks, frame);

        let total_latency = start.elapsed().as_secs_f64() * 1000.0;

        // Record pipeline metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.record_pipeline_latency(total_latency);
        }

        Ok(PipelineOutput {
            frame_id: frame.frame_id,
            detections,
            tracks,
            track_updates,
            latency_ms: total_latency,
        })
    }

    /// Convert tracks to HIVE TrackUpdate messages
    fn tracks_to_updates(&self, tracks: &[Track], frame: &VideoFrame) -> Vec<TrackUpdate> {
        tracks
            .iter()
            .filter(|t| {
                // Filter by confidence
                if t.confidence < self.config.min_confidence {
                    return false;
                }
                // Filter by confirmation status
                if self.config.confirmed_only && !t.is_confirmed() {
                    return false;
                }
                true
            })
            .map(|track| self.track_to_update(track, frame))
            .collect()
    }

    /// Convert a single track to a TrackUpdate message
    fn track_to_update(&self, track: &Track, frame: &VideoFrame) -> TrackUpdate {
        // Convert bbox center to geographic position
        let position = self.bbox_to_position(track, frame);

        // Convert velocity to geographic velocity
        let velocity = self.track_velocity_to_world(track, frame);

        let mut update = TrackUpdate::new(
            &track.id,
            &track.class_label,
            track.confidence as f64,
            position,
            &self.config.platform_id,
            &self.config.model_id,
            &self.model_version,
        );

        if let Some(vel) = velocity {
            update = update.with_velocity(vel);
        }

        // Add attributes
        update = update
            .with_attribute("track_age", track.age)
            .with_attribute("track_hits", track.hits)
            .with_attribute("class_id", track.class_id);

        update
    }

    /// Convert bounding box to geographic position
    fn bbox_to_position(&self, track: &Track, frame: &VideoFrame) -> Position {
        // If we have a reference position, calculate real-world coordinates
        if let Some((ref_lat, ref_lon)) = self.config.reference_position {
            let (cx, cy) = track.bbox.center();

            // Convert normalized coords to offset from center
            let offset_x = (cx - 0.5) * frame.width as f32;
            let offset_y = (cy - 0.5) * frame.height as f32;

            // Convert pixels to meters
            let meters_x = offset_x as f64 * self.config.meters_per_pixel;
            let meters_y = offset_y as f64 * self.config.meters_per_pixel;

            // Apply camera bearing rotation
            let bearing_rad = self.config.camera_bearing.to_radians();
            let world_x = meters_x * bearing_rad.cos() - meters_y * bearing_rad.sin();
            let world_y = meters_x * bearing_rad.sin() + meters_y * bearing_rad.cos();

            // Convert to lat/lon offset (approximate)
            // 1 degree latitude ≈ 111,000 meters
            // 1 degree longitude ≈ 111,000 * cos(lat) meters
            let lat_offset = world_y / 111_000.0;
            let lon_offset = world_x / (111_000.0 * ref_lat.to_radians().cos());

            // CEP based on bbox size and detection confidence
            let cep = (track.bbox.width.max(track.bbox.height) as f64
                * frame.width as f64
                * self.config.meters_per_pixel)
                / track.confidence as f64;

            Position {
                lat: ref_lat + lat_offset,
                lon: ref_lon + lon_offset,
                cep_m: Some(cep.clamp(1.0, 100.0)),
                hae: None,
            }
        } else if let Some((lat, lon, _)) = frame.metadata.position {
            // Use frame metadata position as reference
            let (cx, cy) = track.bbox.center();
            let offset_x = (cx - 0.5) * frame.width as f32;
            let offset_y = (cy - 0.5) * frame.height as f32;

            let meters_x = offset_x as f64 * self.config.meters_per_pixel;
            let meters_y = offset_y as f64 * self.config.meters_per_pixel;

            let lat_offset = meters_y / 111_000.0;
            let lon_offset = meters_x / (111_000.0 * lat.to_radians().cos());

            Position {
                lat: lat + lat_offset,
                lon: lon + lon_offset,
                cep_m: Some(5.0),
                hae: None,
            }
        } else {
            // No geographic reference - use normalized coordinates as placeholder
            let (cx, cy) = track.bbox.center();
            Position {
                lat: cx as f64 * 0.001, // Placeholder
                lon: cy as f64 * 0.001,
                cep_m: Some(999.0), // Unknown accuracy
                hae: None,
            }
        }
    }

    /// Convert track velocity to world velocity
    fn track_velocity_to_world(&self, track: &Track, frame: &VideoFrame) -> Option<Velocity> {
        let speed = track.speed();
        if speed < 0.001 {
            return None;
        }

        // Convert pixels/frame to meters/second
        // Assuming ~15 FPS
        let fps = 15.0;
        let pixels_per_second = speed * frame.width as f32 * fps;
        let meters_per_second = pixels_per_second as f64 * self.config.meters_per_pixel;

        // Get bearing (track.bearing() returns compass bearing)
        // Adjust for camera bearing
        let bearing = (track.bearing() as f64 + self.config.camera_bearing) % 360.0;

        Some(Velocity::new(bearing, meters_per_second))
    }

    /// Get current metrics
    pub async fn metrics(&self) -> InferenceMetrics {
        let _metrics = self.metrics.lock().await;
        // Clone the metrics for return
        InferenceMetrics::new(1000) // Return a new instance - in real impl we'd clone
    }

    /// Get metrics summary
    pub async fn metrics_summary(&self) -> super::metrics::MetricsSummary {
        let metrics = self.metrics.lock().await;
        metrics.summary()
    }

    /// Get tracker statistics
    pub async fn tracker_stats(&self) -> super::tracker::TrackerStats {
        let tracker = self.tracker.lock().await;
        tracker.stats()
    }

    /// Reset pipeline state
    pub async fn reset(&self) {
        let mut tracker = self.tracker.lock().await;
        tracker.reset();

        let mut metrics = self.metrics.lock().await;
        metrics.reset();
    }

    /// Get model info
    pub async fn model_info(&self) -> super::detector::DetectorInfo {
        let detector = self.detector.lock().await;
        detector.model_info()
    }

    /// Update configuration
    pub fn update_config(&mut self, config: PipelineConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::detector::{
        GroundTruthObject, SimulatedDetector, SimulatedDetectorConfig,
    };
    use crate::inference::tracker::{SimulatedTracker, TrackerConfig};
    use crate::inference::types::BoundingBox;

    async fn create_test_pipeline() -> InferencePipeline<SimulatedDetector, SimulatedTracker> {
        let detector_config = SimulatedDetectorConfig {
            latency: std::time::Duration::from_millis(1),
            recall: 1.0,
            false_positive_rate: 0.0,
            ..Default::default()
        };

        let tracker_config = TrackerConfig {
            min_hits: 1, // Immediate confirmation for testing
            ..Default::default()
        };

        let config = PipelineConfig {
            platform_id: "Test-Vehicle".to_string(),
            model_id: "Test-AI".to_string(),
            reference_position: Some((33.7749, -84.3958)),
            ..Default::default()
        };

        let detector = SimulatedDetector::new(detector_config);
        let tracker = SimulatedTracker::new(tracker_config);

        InferencePipeline::new(detector, tracker, config)
    }

    #[tokio::test]
    async fn test_pipeline_initialization() {
        let pipeline = create_test_pipeline().await;
        assert!(!pipeline.is_ready().await);

        pipeline.initialize().await.unwrap();
        assert!(pipeline.is_ready().await);
    }

    #[tokio::test]
    async fn test_pipeline_process_empty() {
        let pipeline = create_test_pipeline().await;
        pipeline.initialize().await.unwrap();

        let frame = VideoFrame::simulated(1, 1920, 1080);
        let output = pipeline.process(&frame).await.unwrap();

        assert_eq!(output.frame_id, 1);
        assert!(output.detections.is_empty());
        assert!(output.tracks.is_empty());
        assert!(output.track_updates.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_process_with_detection() {
        let pipeline = create_test_pipeline().await;
        pipeline.initialize().await.unwrap();

        // Add ground truth
        {
            let mut detector = pipeline.detector.lock().await;
            let gt = GroundTruthObject::new(1, BoundingBox::new(0.4, 0.4, 0.1, 0.2), "person", 0);
            detector.add_ground_truth(gt);
        }

        // Process enough frames to confirm the track (min_hits=1 means 1 update needed)
        // But tracks start tentative, so we need at least 1 matching frame
        let mut output = pipeline
            .process(&VideoFrame::simulated(1, 1920, 1080))
            .await
            .unwrap();

        // First frame creates tentative track
        assert_eq!(output.detections.len(), 1);

        // Process a second frame to confirm (track gets matched and updated)
        output = pipeline
            .process(&VideoFrame::simulated(2, 1920, 1080))
            .await
            .unwrap();

        // Now track should be confirmed
        assert!(!output.tracks.is_empty(), "Should have confirmed tracks");
        assert!(
            !output.track_updates.is_empty(),
            "Should have track updates"
        );

        let update = &output.track_updates[0];
        assert_eq!(update.classification, "person");
        assert_eq!(update.source_platform, "Test-Vehicle");
        assert_eq!(update.source_model, "Test-AI");
    }

    #[tokio::test]
    async fn test_pipeline_track_update_position() {
        let pipeline = create_test_pipeline().await;
        pipeline.initialize().await.unwrap();

        // Add ground truth at center
        {
            let mut detector = pipeline.detector.lock().await;
            let gt = GroundTruthObject::new(
                1,
                BoundingBox::new(0.45, 0.45, 0.1, 0.1), // Near center
                "person",
                0,
            );
            detector.add_ground_truth(gt);
        }

        // Process two frames to confirm track
        pipeline
            .process(&VideoFrame::simulated(1, 1920, 1080))
            .await
            .unwrap();
        let output = pipeline
            .process(&VideoFrame::simulated(2, 1920, 1080))
            .await
            .unwrap();

        assert!(
            !output.track_updates.is_empty(),
            "Should have track updates"
        );
        let update = &output.track_updates[0];

        // Position should be near reference point (33.7749, -84.3958)
        assert!((update.position.lat - 33.7749).abs() < 0.01);
        assert!((update.position.lon - (-84.3958)).abs() < 0.01);
        assert!(update.position.cep_m.is_some());
    }

    #[tokio::test]
    async fn test_pipeline_metrics() {
        let pipeline = create_test_pipeline().await;
        pipeline.initialize().await.unwrap();

        // Process several frames
        for i in 0..5 {
            let frame = VideoFrame::simulated(i, 1920, 1080);
            pipeline.process(&frame).await.unwrap();
        }

        let summary = pipeline.metrics_summary().await;
        assert_eq!(summary.total_frames, 5);
    }

    #[tokio::test]
    async fn test_pipeline_reset() {
        let pipeline = create_test_pipeline().await;
        pipeline.initialize().await.unwrap();

        // Add ground truth and process
        {
            let mut detector = pipeline.detector.lock().await;
            let gt = GroundTruthObject::new(1, BoundingBox::new(0.5, 0.5, 0.1, 0.2), "person", 0);
            detector.add_ground_truth(gt);
        }

        let frame = VideoFrame::simulated(1, 1920, 1080);
        pipeline.process(&frame).await.unwrap();

        let stats_before = pipeline.tracker_stats().await;
        assert!(stats_before.total_tracks_created > 0);

        pipeline.reset().await;

        let stats_after = pipeline.tracker_stats().await;
        assert_eq!(stats_after.total_tracks_created, 0);
    }
}
