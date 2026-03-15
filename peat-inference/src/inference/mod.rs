//! Inference module - Object detection, tracking, and LLM pipeline
//!
//! This module provides the AI inference pipeline for the M1 vignette:
//! - **Detector**: Object detection (YOLOv8/YOLOv11 on Jetson, simulated for testing)
//! - **Tracker**: Multi-object tracking (DeepSORT/ByteTrack style)
//! - **Pipeline**: Connects detector → tracker → TrackUpdate messages
//! - **LLM**: Local language model inference (Ministral, Llama, etc. via llama.cpp)
//! - **Metrics**: Real-time performance measurement
//! - **Video**: Camera and video file input abstraction
//! - **Jetson**: Platform-specific utilities and GPU metrics
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │ VideoSource │────▶│  Detector   │────▶│  Tracker    │────▶ TrackUpdate
//! └─────────────┘     └─────────────┘     └─────────────┘
//!       │                   │                   │
//!   USB/CSI/RTSP     SimulatedDetector   SimulatedTracker  (testing)
//!   Simulated        TensorRTDetector    ByteTracker       (Jetson)
//! ```
//!
//! ## Feature Flags
//!
//! - `tensorrt`: Enable TensorRT detector (requires Jetson + JetPack)
//! - `gstreamer`: Enable GStreamer video pipelines
//!
//! ## Quick Start (Simulated)
//!
//! ```rust,ignore
//! use peat_inference::inference::{InferenceHarness, Scenario};
//!
//! let mut harness = InferenceHarness::new(Scenario::MixedTraffic);
//! let results = harness.run_fast().await?;
//! println!("Tracks: {}, FPS: {:.1}", results.total_tracks_created, results.avg_fps);
//! ```
//!
//! ## Quick Start (Jetson)
//!
//! ```rust,ignore
//! use peat_inference::inference::{
//!     TensorRTDetector, TensorRTConfig,
//!     SimulatedTracker, TrackerConfig,
//!     InferencePipeline, PipelineConfig,
//!     VideoSource, VideoConfig,
//! };
//!
//! // Setup detector
//! let detector = TensorRTDetector::new(TensorRTConfig::default())?;
//!
//! // Setup tracker
//! let tracker = SimulatedTracker::new(TrackerConfig::default());
//!
//! // Create pipeline
//! let pipeline = InferencePipeline::new(detector, tracker, PipelineConfig::default());
//! pipeline.initialize().await?;
//!
//! // Setup video source
//! let mut video = VideoSource::new(VideoConfig::csi_camera(0, 1920, 1080, 30.0))?;
//! video.start().await?;
//!
//! // Process frames
//! while let Some(frame) = video.next_frame().await? {
//!     let output = pipeline.process(&frame).await?;
//!     for update in output.track_updates {
//!         // Send to Peat network
//!     }
//! }
//! ```

mod chipout;
mod detector;
mod harness;
mod jetson;
#[cfg(feature = "llm-inference")]
mod llm;
mod metrics;
#[cfg(feature = "onnx-inference")]
mod onnx;
mod pipeline;
mod tensorrt;
mod tracker;
mod types;
#[cfg(feature = "video-capture")]
mod video;

// Core types
pub use detector::GroundTruthObject;
pub use detector::{Detection, Detector, DetectorInfo, SimulatedDetector, SimulatedDetectorConfig};
pub use harness::{HarnessResults, InferenceHarness, Scenario, ScenarioConfig};
pub use metrics::{InferenceMetrics, LatencyStats, MetricsSummary};
pub use pipeline::{InferencePipeline, PipelineConfig, PipelineOutput};
pub use tracker::{SimulatedTracker, Track, TrackState, Tracker, TrackerConfig, TrackerStats};
pub use types::{BoundingBox, Classification, FrameMetadata, VideoFrame};

// Chipout extraction
pub use chipout::ChipoutExtractor;

// Jetson-specific
pub use jetson::{JetsonInfo, JetsonMetrics, JetsonStats};

// TensorRT detector (stub - kept for future native TensorRT support)
pub use tensorrt::{ReIDConfig, ReIDExtractor, TensorRTConfig, TensorRTDetector};

// ONNX Runtime detector (requires onnx-inference feature)
#[cfg(feature = "onnx-inference")]
pub use onnx::{ExecutionProvider, OnnxConfig, OnnxDetector};

// LLM inference (requires llm-inference feature)
#[cfg(feature = "llm-inference")]
pub use llm::{
    LlamaInference, LlmConfig, LlmError, LlmInference, LlmModelInfo, LlmResult, LlmStats,
};

// Video input (requires video-capture feature)
#[cfg(feature = "video-capture")]
pub use video::{GstPipelineBuilder, VideoConfig, VideoInput, VideoSource, VideoSourceType};
