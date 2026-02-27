//! Object detector trait and implementations
//!
//! Provides abstraction over detection backends:
//! - `SimulatedDetector`: Generates realistic detections for testing
//! - `TensorRTDetector`: Real YOLOv8 inference on Jetson (future)

use super::types::{BoundingBox, Classification, VideoFrame};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rand::prelude::*;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A single object detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Bounding box in normalized coordinates
    pub bbox: BoundingBox,
    /// Classification with confidence
    pub classification: Classification,
    /// Frame ID this detection came from
    pub frame_id: u64,
    /// Detection timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional embedding vector for re-identification (128-D typical)
    pub embedding: Option<Vec<f32>>,
}

impl Detection {
    /// Create a new detection
    pub fn new(bbox: BoundingBox, classification: Classification, frame_id: u64) -> Self {
        Self {
            bbox,
            classification,
            frame_id,
            timestamp: Utc::now(),
            embedding: None,
        }
    }

    /// Add a re-identification embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Object detector trait
#[async_trait]
pub trait Detector: Send + Sync {
    /// Run detection on a video frame
    async fn detect(&mut self, frame: &VideoFrame) -> anyhow::Result<Vec<Detection>>;

    /// Get detector model info
    fn model_info(&self) -> DetectorInfo;

    /// Check if detector is ready
    fn is_ready(&self) -> bool;

    /// Warm up the detector (load model, allocate buffers)
    async fn warm_up(&mut self) -> anyhow::Result<()>;
}

/// Detector model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorInfo {
    pub model_id: String,
    pub model_version: String,
    pub model_type: String,
    pub input_size: (u32, u32),
    pub classes: Vec<String>,
}

// ============================================================================
// Simulated Detector
// ============================================================================

/// Configuration for the simulated detector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedDetectorConfig {
    /// Model ID to report
    pub model_id: String,
    /// Model version to report
    pub model_version: String,
    /// Simulated inference latency
    pub latency: Duration,
    /// Base precision (detection accuracy)
    pub precision: f64,
    /// Base recall (detection coverage)
    pub recall: f64,
    /// Confidence noise (standard deviation)
    pub confidence_noise: f64,
    /// Position noise in normalized coordinates
    pub position_noise: f64,
    /// Probability of false positive per frame
    pub false_positive_rate: f64,
    /// Classes to detect
    pub classes: Vec<String>,
}

impl Default for SimulatedDetectorConfig {
    fn default() -> Self {
        Self {
            model_id: "object_tracker".to_string(),
            model_version: "1.3.0".to_string(),
            latency: Duration::from_millis(67), // ~15 FPS
            precision: 0.91,
            recall: 0.87,
            confidence_noise: 0.05,
            position_noise: 0.02,
            false_positive_rate: 0.02,
            classes: vec![
                "person".to_string(),
                "bicycle".to_string(),
                "car".to_string(),
                "motorcycle".to_string(),
                "bus".to_string(),
                "truck".to_string(),
            ],
        }
    }
}

/// Simulated ground truth object for generating detections
#[derive(Debug, Clone)]
pub struct GroundTruthObject {
    /// Object ID
    pub id: u32,
    /// Current bounding box
    pub bbox: BoundingBox,
    /// Object class
    pub class_label: String,
    /// Class ID
    pub class_id: u32,
    /// Velocity (normalized coords per frame)
    pub velocity: (f32, f32),
    /// Is object currently visible
    pub visible: bool,
    /// Re-ID embedding (fixed per object for consistency)
    pub embedding: Vec<f32>,
}

impl GroundTruthObject {
    /// Create a new ground truth object
    pub fn new(id: u32, bbox: BoundingBox, class_label: impl Into<String>, class_id: u32) -> Self {
        // Generate a consistent embedding for this object
        let mut rng = StdRng::seed_from_u64(id as u64);
        let embedding: Vec<f32> = (0..128).map(|_| rng.random::<f32>() * 2.0 - 1.0).collect();

        Self {
            id,
            bbox,
            class_label: class_label.into(),
            class_id,
            velocity: (0.0, 0.0),
            visible: true,
            embedding,
        }
    }

    /// Set velocity
    pub fn with_velocity(mut self, vx: f32, vy: f32) -> Self {
        self.velocity = (vx, vy);
        self
    }

    /// Update position based on velocity
    pub fn step(&mut self) {
        self.bbox.x += self.velocity.0;
        self.bbox.y += self.velocity.1;

        // Wrap or hide at boundaries
        if self.bbox.x < -0.1 || self.bbox.x > 1.1 || self.bbox.y < -0.1 || self.bbox.y > 1.1 {
            self.visible = false;
        }
    }
}

/// Simulated object detector for testing
///
/// Generates realistic detections based on ground truth objects,
/// with configurable noise, miss rates, and false positives.
pub struct SimulatedDetector {
    config: SimulatedDetectorConfig,
    ground_truth: Vec<GroundTruthObject>,
    rng: StdRng,
    ready: bool,
    frames_processed: u64,
}

impl SimulatedDetector {
    /// Create a new simulated detector
    pub fn new(config: SimulatedDetectorConfig) -> Self {
        Self {
            config,
            ground_truth: Vec::new(),
            rng: StdRng::from_rng(&mut rand::rng()),
            ready: false,
            frames_processed: 0,
        }
    }

    /// Create with default config
    pub fn default_config() -> Self {
        Self::new(SimulatedDetectorConfig::default())
    }

    /// Set ground truth objects to detect
    pub fn set_ground_truth(&mut self, objects: Vec<GroundTruthObject>) {
        self.ground_truth = objects;
    }

    /// Add a ground truth object
    pub fn add_ground_truth(&mut self, object: GroundTruthObject) {
        self.ground_truth.push(object);
    }

    /// Update ground truth positions (call each frame)
    pub fn step_ground_truth(&mut self) {
        for obj in &mut self.ground_truth {
            obj.step();
        }
    }

    /// Generate a detection from ground truth with noise
    fn generate_detection(&mut self, gt: &GroundTruthObject, frame_id: u64) -> Option<Detection> {
        // Miss based on recall
        if self.rng.random::<f64>() > self.config.recall {
            return None;
        }

        // Add position noise
        let noise_x = self.rng.random::<f32>() * self.config.position_noise as f32 * 2.0
            - self.config.position_noise as f32;
        let noise_y = self.rng.random::<f32>() * self.config.position_noise as f32 * 2.0
            - self.config.position_noise as f32;
        let noise_w = self.rng.random::<f32>() * self.config.position_noise as f32 * 2.0
            - self.config.position_noise as f32;
        let noise_h = self.rng.random::<f32>() * self.config.position_noise as f32 * 2.0
            - self.config.position_noise as f32;

        let bbox = BoundingBox::new(
            (gt.bbox.x + noise_x).clamp(0.0, 1.0),
            (gt.bbox.y + noise_y).clamp(0.0, 1.0),
            (gt.bbox.width + noise_w).clamp(0.01, 1.0),
            (gt.bbox.height + noise_h).clamp(0.01, 1.0),
        );

        // Add confidence noise
        let base_confidence = self.config.precision as f32;
        let conf_noise = self.rng.random::<f32>() * self.config.confidence_noise as f32 * 2.0
            - self.config.confidence_noise as f32;
        let confidence = (base_confidence + conf_noise).clamp(0.5, 0.99);

        let classification = Classification::new(&gt.class_label, gt.class_id, confidence);

        // Add noisy embedding (same base + noise)
        let noisy_embedding: Vec<f32> = gt
            .embedding
            .iter()
            .map(|&v| v + self.rng.random::<f32>() * 0.1 - 0.05)
            .collect();

        Some(Detection::new(bbox, classification, frame_id).with_embedding(noisy_embedding))
    }

    /// Generate a false positive detection
    fn generate_false_positive(&mut self, frame_id: u64) -> Detection {
        let bbox = BoundingBox::new(
            self.rng.random::<f32>() * 0.8 + 0.1,
            self.rng.random::<f32>() * 0.8 + 0.1,
            self.rng.random::<f32>() * 0.1 + 0.05,
            self.rng.random::<f32>() * 0.2 + 0.1,
        );

        let class_idx = self.rng.random_range(0..self.config.classes.len());
        let class_label = &self.config.classes[class_idx];
        let confidence = self.rng.random::<f32>() * 0.3 + 0.5; // Lower confidence for FPs

        let classification = Classification::new(class_label, class_idx as u32, confidence);

        // Random embedding for false positive
        let embedding: Vec<f32> = (0..128)
            .map(|_| self.rng.random::<f32>() * 2.0 - 1.0)
            .collect();

        Detection::new(bbox, classification, frame_id).with_embedding(embedding)
    }
}

#[async_trait]
impl Detector for SimulatedDetector {
    async fn detect(&mut self, frame: &VideoFrame) -> anyhow::Result<Vec<Detection>> {
        // Simulate inference latency
        tokio::time::sleep(self.config.latency).await;

        let mut detections = Vec::new();

        // Generate detections from ground truth
        for gt in &self.ground_truth.clone() {
            if !gt.visible {
                continue;
            }

            if let Some(det) = self.generate_detection(gt, frame.frame_id) {
                detections.push(det);
            }
        }

        // Add false positives
        if self.rng.random::<f64>() < self.config.false_positive_rate {
            detections.push(self.generate_false_positive(frame.frame_id));
        }

        self.frames_processed += 1;
        Ok(detections)
    }

    fn model_info(&self) -> DetectorInfo {
        DetectorInfo {
            model_id: self.config.model_id.clone(),
            model_version: self.config.model_version.clone(),
            model_type: "detector_tracker".to_string(),
            input_size: (640, 640),
            classes: self.config.classes.clone(),
        }
    }

    fn is_ready(&self) -> bool {
        self.ready
    }

    async fn warm_up(&mut self) -> anyhow::Result<()> {
        // Simulate model loading time
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.ready = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulated_detector_warmup() {
        let mut detector = SimulatedDetector::default_config();
        assert!(!detector.is_ready());

        detector.warm_up().await.unwrap();
        assert!(detector.is_ready());
    }

    #[tokio::test]
    async fn test_simulated_detector_no_ground_truth() {
        let mut detector = SimulatedDetector::new(SimulatedDetectorConfig {
            false_positive_rate: 0.0, // Disable FPs for this test
            ..Default::default()
        });
        detector.warm_up().await.unwrap();

        let frame = VideoFrame::simulated(1, 1920, 1080);
        let detections = detector.detect(&frame).await.unwrap();

        assert!(detections.is_empty());
    }

    #[tokio::test]
    async fn test_simulated_detector_with_ground_truth() {
        let mut detector = SimulatedDetector::new(SimulatedDetectorConfig {
            recall: 1.0, // Always detect
            false_positive_rate: 0.0,
            latency: Duration::from_millis(1), // Fast for testing
            ..Default::default()
        });
        detector.warm_up().await.unwrap();

        let gt = GroundTruthObject::new(1, BoundingBox::new(0.3, 0.3, 0.2, 0.4), "person", 0);
        detector.add_ground_truth(gt);

        let frame = VideoFrame::simulated(1, 1920, 1080);
        let detections = detector.detect(&frame).await.unwrap();

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].classification.label, "person");
        assert!(detections[0].embedding.is_some());
    }

    #[tokio::test]
    async fn test_simulated_detector_model_info() {
        let detector = SimulatedDetector::default_config();
        let info = detector.model_info();

        assert_eq!(info.model_id, "object_tracker");
        assert_eq!(info.model_version, "1.3.0");
        assert!(!info.classes.is_empty());
    }

    #[test]
    fn test_ground_truth_step() {
        let mut gt = GroundTruthObject::new(1, BoundingBox::new(0.5, 0.5, 0.1, 0.2), "person", 0)
            .with_velocity(0.01, 0.02);

        gt.step();

        assert!((gt.bbox.x - 0.51).abs() < 0.001);
        assert!((gt.bbox.y - 0.52).abs() < 0.001);
    }

    #[test]
    fn test_ground_truth_exits_frame() {
        let mut gt = GroundTruthObject::new(1, BoundingBox::new(0.95, 0.5, 0.1, 0.2), "person", 0)
            .with_velocity(0.1, 0.0);

        assert!(gt.visible);
        gt.step();
        gt.step();
        assert!(!gt.visible);
    }
}
