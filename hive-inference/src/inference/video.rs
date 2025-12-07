//! Video input abstraction for inference pipeline
//!
//! Provides unified interface for different video sources:
//! - USB cameras (V4L2)
//! - CSI cameras (Jetson specific)
//! - RTSP streams
//! - Video files (for testing)
//! - Simulated frames (for testing)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::inference::video::{VideoSource, VideoConfig};
//!
//! // USB camera
//! let config = VideoConfig::usb_camera("/dev/video0", 1920, 1080, 30.0);
//! let mut source = VideoSource::new(config)?;
//! source.start().await?;
//!
//! while let Some(frame) = source.next_frame().await? {
//!     // Process frame
//! }
//! ```

use super::types::{FrameMetadata, VideoFrame};
use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Video source type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoSourceType {
    /// USB camera via V4L2
    UsbCamera { device: String },
    /// CSI camera (Jetson)
    CsiCamera { sensor_id: u32 },
    /// RTSP stream
    RtspStream {
        url: String,
        username: Option<String>,
        password: Option<String>,
    },
    /// Video file
    VideoFile { path: PathBuf, loop_playback: bool },
    /// Simulated frames for testing
    Simulated {
        /// Frames per second to generate
        fps: f64,
    },
}

/// Video source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    /// Source type
    pub source_type: VideoSourceType,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Target FPS
    pub fps: f64,
    /// Pixel format (e.g., "RGB24", "BGR24", "NV12")
    pub pixel_format: String,
    /// Buffer size (frames)
    pub buffer_size: usize,
    /// Platform ID for metadata
    pub platform_id: Option<String>,
    /// Geographic position for metadata
    pub position: Option<(f64, f64, f64)>,
    /// Camera bearing in degrees
    pub bearing: Option<f64>,
    /// Horizontal FOV in degrees
    pub hfov: Option<f64>,
}

impl VideoConfig {
    /// Create config for USB camera
    pub fn usb_camera(device: &str, width: u32, height: u32, fps: f64) -> Self {
        Self {
            source_type: VideoSourceType::UsbCamera {
                device: device.to_string(),
            },
            width,
            height,
            fps,
            pixel_format: "RGB24".to_string(),
            buffer_size: 4,
            platform_id: None,
            position: None,
            bearing: None,
            hfov: None,
        }
    }

    /// Create config for CSI camera (Jetson)
    pub fn csi_camera(sensor_id: u32, width: u32, height: u32, fps: f64) -> Self {
        Self {
            source_type: VideoSourceType::CsiCamera { sensor_id },
            width,
            height,
            fps,
            pixel_format: "NV12".to_string(), // Native format for Jetson
            buffer_size: 4,
            platform_id: None,
            position: None,
            bearing: None,
            hfov: None,
        }
    }

    /// Create config for RTSP stream
    pub fn rtsp(url: &str, width: u32, height: u32) -> Self {
        Self {
            source_type: VideoSourceType::RtspStream {
                url: url.to_string(),
                username: None,
                password: None,
            },
            width,
            height,
            fps: 30.0, // Will be determined by stream
            pixel_format: "RGB24".to_string(),
            buffer_size: 8, // Larger buffer for network jitter
            platform_id: None,
            position: None,
            bearing: None,
            hfov: None,
        }
    }

    /// Create config for video file
    pub fn video_file(path: &str, loop_playback: bool) -> Self {
        Self {
            source_type: VideoSourceType::VideoFile {
                path: PathBuf::from(path),
                loop_playback,
            },
            width: 0,  // Will be set from file
            height: 0, // Will be set from file
            fps: 0.0,  // Will be set from file
            pixel_format: "RGB24".to_string(),
            buffer_size: 4,
            platform_id: None,
            position: None,
            bearing: None,
            hfov: None,
        }
    }

    /// Create config for simulated source
    pub fn simulated(width: u32, height: u32, fps: f64) -> Self {
        Self {
            source_type: VideoSourceType::Simulated { fps },
            width,
            height,
            fps,
            pixel_format: "RGB24".to_string(),
            buffer_size: 2,
            platform_id: None,
            position: None,
            bearing: None,
            hfov: None,
        }
    }

    /// Set platform ID
    pub fn with_platform_id(mut self, id: &str) -> Self {
        self.platform_id = Some(id.to_string());
        self
    }

    /// Set geographic position
    pub fn with_position(mut self, lat: f64, lon: f64, alt: f64) -> Self {
        self.position = Some((lat, lon, alt));
        self
    }

    /// Set camera bearing
    pub fn with_bearing(mut self, bearing: f64) -> Self {
        self.bearing = Some(bearing);
        self
    }

    /// Set horizontal FOV
    pub fn with_hfov(mut self, hfov: f64) -> Self {
        self.hfov = Some(hfov);
        self
    }
}

/// Video source trait
#[async_trait]
pub trait VideoInput: Send {
    /// Start capturing frames
    async fn start(&mut self) -> anyhow::Result<()>;

    /// Stop capturing
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// Get next frame (blocks until available)
    async fn next_frame(&mut self) -> anyhow::Result<Option<VideoFrame>>;

    /// Check if source is running
    fn is_running(&self) -> bool;

    /// Get current FPS
    fn current_fps(&self) -> f64;

    /// Get configuration
    fn config(&self) -> &VideoConfig;
}

/// Video source implementation
pub struct VideoSource {
    config: VideoConfig,
    running: Arc<AtomicBool>,
    frame_count: Arc<AtomicU64>,
    frame_rx: Option<mpsc::Receiver<VideoFrame>>,
    current_fps: f64,
    pipeline: Option<gst::Pipeline>,
}

impl VideoSource {
    /// Create a new video source
    pub fn new(config: VideoConfig) -> anyhow::Result<Self> {
        // Initialize GStreamer once
        gst::init()?;

        Ok(Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            frame_count: Arc::new(AtomicU64::new(0)),
            frame_rx: None,
            current_fps: 0.0,
            pipeline: None,
        })
    }

    /// Create frame metadata from config
    fn create_metadata(&self) -> FrameMetadata {
        FrameMetadata {
            source: match &self.config.source_type {
                VideoSourceType::UsbCamera { device } => device.clone(),
                VideoSourceType::CsiCamera { sensor_id } => format!("csi{}", sensor_id),
                VideoSourceType::RtspStream { url, .. } => url.clone(),
                VideoSourceType::VideoFile { path, .. } => path.to_string_lossy().to_string(),
                VideoSourceType::Simulated { .. } => "simulated".to_string(),
            },
            platform_id: self.config.platform_id.clone(),
            position: self.config.position,
            bearing: self.config.bearing,
            hfov: self.config.hfov,
        }
    }

    /// Build GStreamer pipeline for video file
    fn build_file_pipeline(
        &self,
        path: &PathBuf,
        loop_playback: bool,
        tx: mpsc::Sender<VideoFrame>,
    ) -> anyhow::Result<gst::Pipeline> {
        let path_str = path.to_string_lossy();

        // Check if Jetson HW decoder is available
        let has_nvdec = gst::ElementFactory::find("nvv4l2decoder").is_some();

        // Build pipeline string based on available decoders
        let pipeline_str = if has_nvdec {
            // Jetson hardware accelerated pipeline
            // Note: nvvidconv outputs RGBA (not RGB), so we use videoconvert for final RGB conversion
            format!(
                "filesrc location=\"{}\" ! qtdemux ! h264parse ! nvv4l2decoder ! \
                 nvvidconv ! video/x-raw,format=RGBA ! videoconvert ! \
                 video/x-raw,format=RGB ! appsink name=sink emit-signals=true sync=false",
                path_str
            )
        } else {
            // Software decode pipeline (fallback)
            format!(
                "filesrc location=\"{}\" ! decodebin ! videoconvert ! \
                 video/x-raw,format=RGB ! appsink name=sink emit-signals=true sync=false",
                path_str
            )
        };

        tracing::info!("Creating GStreamer pipeline: {}", pipeline_str);

        let pipeline = gst::parse::launch(&pipeline_str)?
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Failed to cast to Pipeline"))?;

        // Get appsink
        let appsink = pipeline
            .by_name("sink")
            .ok_or_else(|| anyhow::anyhow!("Failed to get appsink"))?
            .dynamic_cast::<gst_app::AppSink>()
            .map_err(|_| anyhow::anyhow!("Failed to cast to AppSink"))?;

        // Configure appsink
        appsink.set_max_buffers(2);
        appsink.set_drop(true);

        let metadata = self.create_metadata();
        let frame_count = self.frame_count.clone();
        let running = self.running.clone();
        let loop_playback = loop_playback;
        let pipeline_weak = pipeline.downgrade();

        // Set up callback for new samples
        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    if !running.load(Ordering::Relaxed) {
                        return Err(gst::FlowError::Eos);
                    }

                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    // Get video info from caps
                    let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let width = video_info.width();
                    let height = video_info.height();

                    // Map buffer to read data
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let data = map.as_slice().to_vec();

                    let frame_id = frame_count.fetch_add(1, Ordering::Relaxed);

                    let frame = VideoFrame {
                        frame_id,
                        timestamp: chrono::Utc::now(),
                        width,
                        height,
                        data,
                        metadata: metadata.clone(),
                    };

                    // Send frame (non-blocking)
                    if tx.blocking_send(frame).is_err() {
                        tracing::warn!("Frame receiver dropped");
                        return Err(gst::FlowError::Eos);
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .eos(move |_| {
                    tracing::info!("End of stream");
                    if loop_playback {
                        if let Some(pipeline) = pipeline_weak.upgrade() {
                            // Seek back to beginning
                            let _ = pipeline.seek_simple(
                                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                gst::ClockTime::ZERO,
                            );
                        }
                    }
                })
                .build(),
        );

        Ok(pipeline)
    }

    /// Build GStreamer pipeline for USB camera
    fn build_usb_pipeline(
        &self,
        device: &str,
        tx: mpsc::Sender<VideoFrame>,
    ) -> anyhow::Result<gst::Pipeline> {
        let pipeline_str = format!(
            "v4l2src device={} ! video/x-raw,width={},height={},framerate={}/1 ! \
             videoconvert ! video/x-raw,format=RGB ! appsink name=sink emit-signals=true sync=false",
            device, self.config.width, self.config.height, self.config.fps as i32
        );

        tracing::info!("Creating USB camera pipeline: {}", pipeline_str);

        let pipeline = gst::parse::launch(&pipeline_str)?
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Failed to cast to Pipeline"))?;

        self.setup_appsink_callbacks(&pipeline, tx)?;

        Ok(pipeline)
    }

    /// Build GStreamer pipeline for CSI camera (Jetson)
    fn build_csi_pipeline(
        &self,
        sensor_id: u32,
        tx: mpsc::Sender<VideoFrame>,
    ) -> anyhow::Result<gst::Pipeline> {
        // Note: nvvidconv outputs RGBA (not RGB), so we use videoconvert for final RGB conversion
        let pipeline_str = format!(
            "nvarguscamerasrc sensor-id={} ! \
             video/x-raw(memory:NVMM),width={},height={},framerate={}/1,format=NV12 ! \
             nvvidconv ! video/x-raw,format=RGBA ! videoconvert ! \
             video/x-raw,format=RGB ! appsink name=sink emit-signals=true sync=false",
            sensor_id, self.config.width, self.config.height, self.config.fps as i32
        );

        tracing::info!("Creating CSI camera pipeline: {}", pipeline_str);

        let pipeline = gst::parse::launch(&pipeline_str)?
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| anyhow::anyhow!("Failed to cast to Pipeline"))?;

        self.setup_appsink_callbacks(&pipeline, tx)?;

        Ok(pipeline)
    }

    /// Setup appsink callbacks (shared by camera pipelines)
    fn setup_appsink_callbacks(
        &self,
        pipeline: &gst::Pipeline,
        tx: mpsc::Sender<VideoFrame>,
    ) -> anyhow::Result<()> {
        let appsink = pipeline
            .by_name("sink")
            .ok_or_else(|| anyhow::anyhow!("Failed to get appsink"))?
            .dynamic_cast::<gst_app::AppSink>()
            .map_err(|_| anyhow::anyhow!("Failed to cast to AppSink"))?;

        appsink.set_max_buffers(2);
        appsink.set_drop(true);

        let metadata = self.create_metadata();
        let frame_count = self.frame_count.clone();
        let running = self.running.clone();

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    if !running.load(Ordering::Relaxed) {
                        return Err(gst::FlowError::Eos);
                    }

                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let width = video_info.width();
                    let height = video_info.height();

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let data = map.as_slice().to_vec();

                    let frame_id = frame_count.fetch_add(1, Ordering::Relaxed);

                    let frame = VideoFrame {
                        frame_id,
                        timestamp: chrono::Utc::now(),
                        width,
                        height,
                        data,
                        metadata: metadata.clone(),
                    };

                    if tx.blocking_send(frame).is_err() {
                        return Err(gst::FlowError::Eos);
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        Ok(())
    }
}

#[async_trait]
impl VideoInput for VideoSource {
    async fn start(&mut self) -> anyhow::Result<()> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        self.frame_rx = Some(rx);

        match &self.config.source_type {
            VideoSourceType::Simulated { fps } => {
                let fps = *fps;
                let width = self.config.width;
                let height = self.config.height;
                let metadata = self.create_metadata();
                let running = self.running.clone();
                let frame_count = self.frame_count.clone();

                self.running.store(true, Ordering::Relaxed);

                tokio::spawn(async move {
                    let frame_duration = Duration::from_secs_f64(1.0 / fps);

                    while running.load(Ordering::Relaxed) {
                        let frame_id = frame_count.fetch_add(1, Ordering::Relaxed);
                        let frame = VideoFrame::simulated(frame_id, width, height)
                            .with_metadata(metadata.clone());

                        if tx.send(frame).await.is_err() {
                            break;
                        }

                        tokio::time::sleep(frame_duration).await;
                    }
                });

                return Ok(());
            }

            VideoSourceType::VideoFile {
                path,
                loop_playback,
            } => {
                if !path.exists() {
                    anyhow::bail!("Video file not found: {:?}", path);
                }

                let pipeline = self.build_file_pipeline(path, *loop_playback, tx)?;

                // Start pipeline
                pipeline.set_state(gst::State::Playing)?;

                self.pipeline = Some(pipeline);
                self.running.store(true, Ordering::Relaxed);

                tracing::info!("Video file pipeline started: {:?}", path);
            }

            VideoSourceType::UsbCamera { device } => {
                let pipeline = self.build_usb_pipeline(device, tx)?;
                pipeline.set_state(gst::State::Playing)?;
                self.pipeline = Some(pipeline);
                self.running.store(true, Ordering::Relaxed);

                tracing::info!("USB camera pipeline started: {}", device);
            }

            VideoSourceType::CsiCamera { sensor_id } => {
                let pipeline = self.build_csi_pipeline(*sensor_id, tx)?;
                pipeline.set_state(gst::State::Playing)?;
                self.pipeline = Some(pipeline);
                self.running.store(true, Ordering::Relaxed);

                tracing::info!("CSI camera pipeline started: sensor {}", sensor_id);
            }

            VideoSourceType::RtspStream { url, .. } => {
                // RTSP support can be added similarly
                tracing::warn!("RTSP stream not yet implemented - url: {}", url);
                anyhow::bail!("RTSP stream capture not yet implemented");
            }
        }

        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::Relaxed);

        if let Some(pipeline) = self.pipeline.take() {
            pipeline.set_state(gst::State::Null)?;
            tracing::info!("Pipeline stopped");
        }

        self.frame_rx = None;
        Ok(())
    }

    async fn next_frame(&mut self) -> anyhow::Result<Option<VideoFrame>> {
        if let Some(ref mut rx) = self.frame_rx {
            match rx.recv().await {
                Some(frame) => Ok(Some(frame)),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    fn current_fps(&self) -> f64 {
        self.current_fps
    }

    fn config(&self) -> &VideoConfig {
        &self.config
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(pipeline) = self.pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}

/// GStreamer pipeline builder for Jetson
///
/// Creates optimized pipelines for different sources on Jetson.
#[allow(dead_code)]
pub struct GstPipelineBuilder {
    elements: Vec<String>,
}

#[allow(dead_code)]
impl GstPipelineBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
        }
    }

    /// Add CSI camera source (nvarguscamerasrc)
    pub fn csi_camera(mut self, sensor_id: u32, width: u32, height: u32, fps: u32) -> Self {
        self.elements.push(format!(
            "nvarguscamerasrc sensor-id={} ! video/x-raw(memory:NVMM),width={},height={},framerate={}/1,format=NV12",
            sensor_id, width, height, fps
        ));
        self
    }

    /// Add USB camera source (v4l2src)
    pub fn usb_camera(mut self, device: &str, width: u32, height: u32, fps: u32) -> Self {
        self.elements.push(format!(
            "v4l2src device={} ! video/x-raw,width={},height={},framerate={}/1",
            device, width, height, fps
        ));
        self
    }

    /// Add RTSP source
    pub fn rtsp(mut self, url: &str) -> Self {
        self.elements.push(format!(
            "rtspsrc location={} latency=0 ! rtph264depay ! h264parse ! nvv4l2decoder",
            url
        ));
        self
    }

    /// Add NVMM to CPU memory conversion
    pub fn nvmm_to_cpu(mut self) -> Self {
        self.elements
            .push("nvvidconv ! video/x-raw,format=BGRx".to_string());
        self
    }

    /// Add format conversion
    pub fn convert(mut self, format: &str) -> Self {
        self.elements
            .push(format!("videoconvert ! video/x-raw,format={}", format));
        self
    }

    /// Add appsink for frame retrieval
    pub fn appsink(mut self, name: &str) -> Self {
        self.elements.push(format!(
            "appsink name={} emit-signals=true max-buffers=2 drop=true sync=false",
            name
        ));
        self
    }

    /// Build the pipeline string
    pub fn build(self) -> String {
        self.elements.join(" ! ")
    }
}

impl Default for GstPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_usb() {
        let config = VideoConfig::usb_camera("/dev/video0", 1920, 1080, 30.0);
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 30.0);
        matches!(config.source_type, VideoSourceType::UsbCamera { .. });
    }

    #[test]
    fn test_config_csi() {
        let config = VideoConfig::csi_camera(0, 1280, 720, 60.0);
        assert_eq!(config.pixel_format, "NV12");
        matches!(
            config.source_type,
            VideoSourceType::CsiCamera { sensor_id: 0 }
        );
    }

    #[test]
    fn test_config_rtsp() {
        let config = VideoConfig::rtsp("rtsp://192.168.1.100/stream", 1920, 1080)
            .with_platform_id("Camera-1")
            .with_position(33.7749, -84.3958, 0.0)
            .with_bearing(45.0)
            .with_hfov(60.0);

        assert_eq!(config.platform_id, Some("Camera-1".to_string()));
        assert_eq!(config.position, Some((33.7749, -84.3958, 0.0)));
        assert_eq!(config.bearing, Some(45.0));
        assert_eq!(config.hfov, Some(60.0));
    }

    #[test]
    fn test_gst_pipeline_csi() {
        let pipeline = GstPipelineBuilder::new()
            .csi_camera(0, 1920, 1080, 30)
            .nvmm_to_cpu()
            .convert("RGB")
            .appsink("sink")
            .build();

        assert!(pipeline.contains("nvarguscamerasrc"));
        assert!(pipeline.contains("sensor-id=0"));
        assert!(pipeline.contains("nvvidconv"));
        assert!(pipeline.contains("appsink"));
    }

    #[tokio::test]
    async fn test_simulated_source() {
        let config = VideoConfig::simulated(640, 480, 10.0);
        let mut source = VideoSource::new(config).unwrap();

        source.start().await.unwrap();
        assert!(source.is_running());

        // Get a few frames
        for _ in 0..3 {
            let frame = source.next_frame().await.unwrap();
            assert!(frame.is_some());
            let frame = frame.unwrap();
            assert_eq!(frame.width, 640);
            assert_eq!(frame.height, 480);
        }

        source.stop().await.unwrap();
        assert!(!source.is_running());
    }
}
