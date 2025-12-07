//! ONNX Runtime detector for YOLOv8
//!
//! Provides object detection using ONNX Runtime with support for:
//! - CPU execution (fallback)
//! - CUDA execution provider
//! - TensorRT execution provider (best performance on Jetson)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::inference::{OnnxDetector, OnnxConfig};
//!
//! let config = OnnxConfig {
//!     model_path: "/models/yolov8n.onnx".into(),
//!     input_size: (640, 640),
//!     confidence_threshold: 0.5,
//!     ..Default::default()
//! };
//!
//! let mut detector = OnnxDetector::new(config)?;
//! detector.warm_up().await?;
//!
//! let detections = detector.detect(&frame).await?;
//! ```

use super::detector::{Detection, Detector, DetectorInfo};
use super::types::{BoundingBox, Classification, VideoFrame};
use async_trait::async_trait;
use ndarray::{Array, ArrayView, IxDyn};
use ort::execution_providers::{CUDAExecutionProvider, TensorRTExecutionProvider};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

/// ONNX Runtime execution provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionProvider {
    /// CPU (always available)
    Cpu,
    /// CUDA GPU acceleration
    Cuda,
    /// TensorRT (best for Jetson)
    TensorRT,
}

impl Default for ExecutionProvider {
    fn default() -> Self {
        Self::Cpu
    }
}

/// ONNX detector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnnxConfig {
    /// Path to ONNX model file
    pub model_path: PathBuf,

    /// Model input size (width, height)
    pub input_size: (u32, u32),

    /// Confidence threshold for detections
    pub confidence_threshold: f32,

    /// NMS IoU threshold
    pub nms_threshold: f32,

    /// Maximum detections per frame
    pub max_detections: usize,

    /// Preferred execution provider
    pub execution_provider: ExecutionProvider,

    /// Model ID for reporting
    pub model_id: String,

    /// Model version for reporting
    pub model_version: String,

    /// Class labels (COCO by default)
    pub class_labels: Vec<String>,

    /// Number of inference threads (CPU)
    pub num_threads: usize,
}

impl Default for OnnxConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/yolov8n.onnx"),
            input_size: (640, 640),
            confidence_threshold: 0.5,
            nms_threshold: 0.45,
            max_detections: 100,
            execution_provider: ExecutionProvider::Cpu,
            model_id: "yolov8n".to_string(),
            model_version: "8.0.0".to_string(),
            class_labels: coco_labels(),
            num_threads: 4,
        }
    }
}

impl OnnxConfig {
    /// Create config for YOLOv8 nano model
    pub fn yolov8n(model_path: &str) -> Self {
        Self {
            model_path: PathBuf::from(model_path),
            model_id: "yolov8n".to_string(),
            ..Default::default()
        }
    }

    /// Create config for YOLOv8 small model
    pub fn yolov8s(model_path: &str) -> Self {
        Self {
            model_path: PathBuf::from(model_path),
            model_id: "yolov8s".to_string(),
            ..Default::default()
        }
    }

    /// Set execution provider
    pub fn with_provider(mut self, provider: ExecutionProvider) -> Self {
        self.execution_provider = provider;
        self
    }

    /// Set confidence threshold
    pub fn with_confidence(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }
}

/// ONNX Runtime based object detector
pub struct OnnxDetector {
    config: OnnxConfig,
    session: Option<Session>,
    info: DetectorInfo,
    inference_count: u64,
    total_inference_time_ms: f64,
    ready: bool,
}

impl OnnxDetector {
    /// Create a new ONNX detector
    pub fn new(config: OnnxConfig) -> anyhow::Result<Self> {
        let info = DetectorInfo {
            model_id: config.model_id.clone(),
            model_version: config.model_version.clone(),
            model_type: "yolov8".to_string(),
            input_size: config.input_size,
            classes: config.class_labels.clone(),
        };

        Ok(Self {
            config,
            session: None,
            info,
            inference_count: 0,
            total_inference_time_ms: 0.0,
            ready: false,
        })
    }

    /// Load the ONNX model and create inference session
    fn load_model(&mut self) -> anyhow::Result<()> {
        info!("Loading ONNX model: {:?}", self.config.model_path);
        info!(
            "Requested execution provider: {:?}",
            self.config.execution_provider
        );

        if !self.config.model_path.exists() {
            return Err(anyhow::anyhow!(
                "Model file not found: {:?}",
                self.config.model_path
            ));
        }

        // Read model file into memory
        let model_bytes = std::fs::read(&self.config.model_path)
            .map_err(|e| anyhow::anyhow!("Failed to read model file: {}", e))?;

        // Build session with appropriate execution provider
        let mut builder = Session::builder()
            .map_err(|e| anyhow::anyhow!("Failed to create session builder: {}", e))?;

        // Register execution providers based on config
        // Order matters: TensorRT > CUDA > CPU (fallback)
        match self.config.execution_provider {
            ExecutionProvider::TensorRT => {
                info!("Registering TensorRT and CUDA execution providers");
                builder = builder
                    .with_execution_providers([
                        TensorRTExecutionProvider::default().build(),
                        CUDAExecutionProvider::default().build(),
                    ])
                    .map_err(|e| anyhow::anyhow!("Failed to set execution providers: {}", e))?;
            }
            ExecutionProvider::Cuda => {
                info!("Registering CUDA execution provider");
                builder = builder
                    .with_execution_providers([CUDAExecutionProvider::default().build()])
                    .map_err(|e| anyhow::anyhow!("Failed to set CUDA provider: {}", e))?;
            }
            ExecutionProvider::Cpu => {
                info!("Using CPU execution provider");
                // CPU is the default fallback, no explicit registration needed
            }
        }

        let session = builder
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Failed to set optimization level: {}", e))?
            .with_intra_threads(self.config.num_threads)
            .map_err(|e| anyhow::anyhow!("Failed to set thread count: {}", e))?
            .commit_from_memory(&model_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        info!(
            "ONNX session created with {} inputs, {} outputs",
            session.inputs.len(),
            session.outputs.len()
        );

        // Log input/output info
        for input in &session.inputs {
            debug!("Input: {} - {:?}", input.name, input.input_type);
        }
        for output in &session.outputs {
            debug!("Output: {} - {:?}", output.name, output.output_type);
        }

        self.session = Some(session);
        self.ready = true;
        Ok(())
    }

    /// Preprocess frame for YOLOv8 input
    fn preprocess(&self, frame: &VideoFrame) -> anyhow::Result<Array<f32, IxDyn>> {
        let (target_w, target_h) = self.config.input_size;

        // Frame data is RGB, need to resize and normalize
        let img = image::RgbImage::from_raw(frame.width, frame.height, frame.data.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to create image from frame data"))?;

        // Resize to model input size
        let resized = image::imageops::resize(
            &img,
            target_w,
            target_h,
            image::imageops::FilterType::Triangle,
        );

        // Convert to NCHW format and normalize to [0, 1]
        let mut input = Array::<f32, _>::zeros((1, 3, target_h as usize, target_w as usize));

        for y in 0..target_h as usize {
            for x in 0..target_w as usize {
                let pixel = resized.get_pixel(x as u32, y as u32);
                input[[0, 0, y, x]] = pixel[0] as f32 / 255.0; // R
                input[[0, 1, y, x]] = pixel[1] as f32 / 255.0; // G
                input[[0, 2, y, x]] = pixel[2] as f32 / 255.0; // B
            }
        }

        Ok(input.into_dyn())
    }

    /// Run YOLOv8 postprocessing on raw output
    fn postprocess(
        &self,
        output: ArrayView<f32, IxDyn>,
        orig_width: u32,
        orig_height: u32,
        frame_id: u64,
    ) -> Vec<Detection> {
        // YOLOv8 output shape: [1, 84, 8400] for COCO (80 classes + 4 box coords)
        let output = output.to_owned();

        // Get dimensions
        let shape = output.shape();
        if shape.len() != 3 {
            warn!("Unexpected output shape: {:?}", shape);
            return Vec::new();
        }

        let num_classes = shape[1] - 4; // 84 - 4 = 80 classes
        let num_boxes = shape[2];

        let (target_w, target_h) = self.config.input_size;
        let scale_x = orig_width as f32 / target_w as f32;
        let scale_y = orig_height as f32 / target_h as f32;

        let mut detections = Vec::new();

        // Process each detection
        for i in 0..num_boxes {
            // Get box coordinates (center_x, center_y, width, height)
            let cx = output[[0, 0, i]];
            let cy = output[[0, 1, i]];
            let w = output[[0, 2, i]];
            let h = output[[0, 3, i]];

            // Find best class score
            let mut best_class: u32 = 0;
            let mut best_score = 0.0f32;
            for c in 0..num_classes {
                let score = output[[0, 4 + c, i]];
                if score > best_score {
                    best_score = score;
                    best_class = c as u32;
                }
            }

            // Filter by confidence
            if best_score < self.config.confidence_threshold {
                continue;
            }

            // Convert from center format to corner format and scale
            let x1 = (cx - w / 2.0) * scale_x;
            let y1 = (cy - h / 2.0) * scale_y;
            let box_w = w * scale_x;
            let box_h = h * scale_y;

            // Normalize to 0-1 range
            let norm_x = (x1 / orig_width as f32).max(0.0).min(1.0);
            let norm_y = (y1 / orig_height as f32).max(0.0).min(1.0);
            let norm_w = (box_w / orig_width as f32).max(0.0).min(1.0 - norm_x);
            let norm_h = (box_h / orig_height as f32).max(0.0).min(1.0 - norm_y);

            let class_name = self
                .config
                .class_labels
                .get(best_class as usize)
                .cloned()
                .unwrap_or_else(|| format!("class_{}", best_class));

            let detection = Detection::new(
                BoundingBox::new(norm_x, norm_y, norm_w, norm_h),
                Classification::new(class_name, best_class, best_score),
                frame_id,
            );

            detections.push(detection);
        }

        // Apply NMS
        let detections = self.nms(detections);

        // Limit detections
        detections
            .into_iter()
            .take(self.config.max_detections)
            .collect()
    }

    /// Non-maximum suppression
    fn nms(&self, mut detections: Vec<Detection>) -> Vec<Detection> {
        // Sort by confidence (descending)
        detections.sort_by(|a, b| {
            b.classification
                .confidence
                .partial_cmp(&a.classification.confidence)
                .unwrap()
        });

        let mut keep = Vec::new();

        while !detections.is_empty() {
            let best = detections.remove(0);
            keep.push(best.clone());

            detections.retain(|d| {
                // Only compare same class
                if d.classification.class_id != best.classification.class_id {
                    return true;
                }
                // Calculate IoU
                let iou = best.bbox.iou(&d.bbox);
                iou < self.config.nms_threshold
            });
        }

        keep
    }
}

#[async_trait]
impl Detector for OnnxDetector {
    async fn detect(&mut self, frame: &VideoFrame) -> anyhow::Result<Vec<Detection>> {
        if self.session.is_none() {
            return Err(anyhow::anyhow!("Model not loaded - call warm_up() first"));
        }

        let start = Instant::now();

        // Preprocess
        let input = self.preprocess(frame)?;

        // Create input tensor using ort 2.x Value::from_array API
        let input_value = Value::from_array(input)
            .map_err(|e| anyhow::anyhow!("Failed to create input tensor: {}", e))?;

        // Run inference and extract output data - scoped to release borrows
        let (dims, output_data) = {
            let session = self.session.as_mut().unwrap();
            let outputs = session
                .run(ort::inputs![input_value])
                .map_err(|e| anyhow::anyhow!("Inference failed: {}", e))?;

            // Get output tensor - ort 2.x: try_extract_tensor returns (Shape, &[T])
            let (shape, data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow::anyhow!("Failed to extract tensor: {}", e))?;

            // Clone data to owned vec to release borrow
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            let output_data: Vec<f32> = data.to_vec();
            (dims, output_data)
        };

        // Convert to ndarray view for postprocessing (now using owned data)
        let output_view = ndarray::ArrayView::from_shape(ndarray::IxDyn(&dims), &output_data)
            .map_err(|e| anyhow::anyhow!("Failed to create array view: {}", e))?;

        // Postprocess
        let detections = self.postprocess(output_view, frame.width, frame.height, frame.frame_id);

        let inference_time = start.elapsed().as_secs_f64() * 1000.0;
        self.inference_count += 1;
        self.total_inference_time_ms += inference_time;

        debug!(
            "Inference {}: {} detections in {:.1}ms",
            self.inference_count,
            detections.len(),
            inference_time
        );

        Ok(detections)
    }

    async fn warm_up(&mut self) -> anyhow::Result<()> {
        info!("Warming up ONNX detector...");

        // Load model if not already loaded
        if self.session.is_none() {
            self.load_model()?;
        }

        // Run a dummy inference to warm up
        let dummy_frame = VideoFrame {
            frame_id: 0,
            timestamp: chrono::Utc::now(),
            width: self.config.input_size.0,
            height: self.config.input_size.1,
            data: vec![0u8; (self.config.input_size.0 * self.config.input_size.1 * 3) as usize],
            metadata: super::types::FrameMetadata::default(),
        };

        let start = Instant::now();
        let _ = self.detect(&dummy_frame).await?;
        let warmup_time = start.elapsed();

        info!("ONNX detector warmed up in {:?}", warmup_time);
        Ok(())
    }

    fn model_info(&self) -> DetectorInfo {
        self.info.clone()
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}

/// COCO class labels
fn coco_labels() -> Vec<String> {
    vec![
        "person",
        "bicycle",
        "car",
        "motorcycle",
        "airplane",
        "bus",
        "train",
        "truck",
        "boat",
        "traffic light",
        "fire hydrant",
        "stop sign",
        "parking meter",
        "bench",
        "bird",
        "cat",
        "dog",
        "horse",
        "sheep",
        "cow",
        "elephant",
        "bear",
        "zebra",
        "giraffe",
        "backpack",
        "umbrella",
        "handbag",
        "tie",
        "suitcase",
        "frisbee",
        "skis",
        "snowboard",
        "sports ball",
        "kite",
        "baseball bat",
        "baseball glove",
        "skateboard",
        "surfboard",
        "tennis racket",
        "bottle",
        "wine glass",
        "cup",
        "fork",
        "knife",
        "spoon",
        "bowl",
        "banana",
        "apple",
        "sandwich",
        "orange",
        "broccoli",
        "carrot",
        "hot dog",
        "pizza",
        "donut",
        "cake",
        "chair",
        "couch",
        "potted plant",
        "bed",
        "dining table",
        "toilet",
        "tv",
        "laptop",
        "mouse",
        "remote",
        "keyboard",
        "cell phone",
        "microwave",
        "oven",
        "toaster",
        "sink",
        "refrigerator",
        "book",
        "clock",
        "vase",
        "scissors",
        "teddy bear",
        "hair drier",
        "toothbrush",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = OnnxConfig::default();
        assert_eq!(config.input_size, (640, 640));
        assert_eq!(config.confidence_threshold, 0.5);
        assert_eq!(config.class_labels.len(), 80);
    }

    #[test]
    fn test_config_builder() {
        let config = OnnxConfig::yolov8n("/models/yolov8n.onnx")
            .with_provider(ExecutionProvider::Cuda)
            .with_confidence(0.7);

        assert_eq!(config.model_id, "yolov8n");
        assert_eq!(config.execution_provider, ExecutionProvider::Cuda);
        assert_eq!(config.confidence_threshold, 0.7);
    }

    #[test]
    fn test_iou_calculation() {
        let box_a = BoundingBox::new(0.0, 0.0, 0.5, 0.5);
        let box_b = BoundingBox::new(0.25, 0.25, 0.5, 0.5);

        let iou = box_a.iou(&box_b);
        // Intersection: 0.25 * 0.25 = 0.0625
        // Union: 0.25 + 0.25 - 0.0625 = 0.4375
        // IoU: 0.0625/0.4375 ≈ 0.143
        assert!((iou - 0.143).abs() < 0.01);
    }

    /// Test loading and running inference with real YOLOv8n model
    /// Requires the model file to be present at models/yolov8n.onnx
    #[tokio::test]
    async fn test_real_model_inference() {
        let model_path = std::path::Path::new("models/yolov8n.onnx");
        if !model_path.exists() {
            eprintln!("Skipping test: model not found at {:?}", model_path);
            return;
        }

        let config = OnnxConfig {
            model_path: model_path.to_path_buf(),
            confidence_threshold: 0.25, // Lower threshold to get more detections
            ..Default::default()
        };

        let mut detector = OnnxDetector::new(config).expect("Failed to create detector");

        // Warm up the model
        detector.warm_up().await.expect("Failed to warm up model");
        assert!(detector.is_ready());

        // Create a test frame (640x640 RGB with some noise for variety)
        let mut data = vec![0u8; 640 * 640 * 3];
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = ((i * 7) % 256) as u8; // Pseudo-random pattern
        }

        let frame = VideoFrame {
            frame_id: 1,
            timestamp: chrono::Utc::now(),
            width: 640,
            height: 640,
            data,
            metadata: super::super::types::FrameMetadata::default(),
        };

        // Run inference
        let start = std::time::Instant::now();
        let detections = detector.detect(&frame).await.expect("Inference failed");
        let elapsed = start.elapsed();

        println!(
            "Inference completed in {:.1}ms, found {} detections",
            elapsed.as_secs_f64() * 1000.0,
            detections.len()
        );

        // Just verify it runs without error - random data won't have real detections
        // The important thing is the model loads and inference completes
        assert!(detector.inference_count >= 2); // warm_up + this inference
    }
}
