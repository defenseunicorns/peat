//! Connected Detection Pipeline - Full End-to-End Example
//!
//! Demonstrates the complete inference pipeline with HIVE sync:
//!   Video → Detection → Tracking → TrackUpdates → HIVE Sync
//!
//! Usage:
//!   cargo run --example connected_detection --features onnx-inference --release -- <video_file>
//!
//! Options:
//!   --gpu        Use CUDA execution provider
//!   --tensorrt   Use TensorRT execution provider (best on Jetson)
//!   --frames N   Process N frames (default: 100)
//!   --sync       Enable HIVE sync (requires network setup)
//!
//! Example:
//!   cargo run --example connected_detection --features onnx-inference --release -- \
//!       223461.mp4 --tensorrt --frames 50

#[cfg(feature = "onnx-inference")]
use hive_inference::inference::{
    Detector, ExecutionProvider, InferencePipeline, OnnxConfig, OnnxDetector, PipelineConfig,
    SimulatedTracker, TrackerConfig, VideoConfig, VideoInput, VideoSource,
};
#[cfg(feature = "onnx-inference")]
use hive_inference::messages::TrackUpdate;

#[cfg(feature = "onnx-inference")]
use std::time::Instant;

#[cfg(feature = "onnx-inference")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,gstreamer=warn".to_string()),
        )
        .init();

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let video_path = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(|s| s.as_str());

    let video_path = match video_path {
        Some(p) => p,
        None => {
            eprintln!(
                "Usage: {} <video_file> [--gpu|--tensorrt] [--frames N] [--sync]",
                args[0]
            );
            eprintln!();
            eprintln!("Example: {} 223461.mp4 --tensorrt --frames 50", args[0]);
            std::process::exit(1);
        }
    };

    let use_tensorrt = args.iter().any(|arg| arg == "--tensorrt" || arg == "-t");
    let use_gpu = args.iter().any(|arg| arg == "--gpu" || arg == "-g");
    let enable_sync = args.iter().any(|arg| arg == "--sync" || arg == "-s");

    let max_frames: usize = args
        .iter()
        .position(|arg| arg == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let execution_provider = if use_tensorrt {
        ExecutionProvider::TensorRT
    } else if use_gpu {
        ExecutionProvider::Cuda
    } else {
        ExecutionProvider::Cpu
    };

    // Check model exists
    let model_path = "models/yolov8n.onnx";
    if !std::path::Path::new(model_path).exists() {
        eprintln!("Model not found at {}. Download with:", model_path);
        eprintln!(
            "  curl -L -o models/yolov8n.onnx 'https://huggingface.co/Kalray/yolov8/resolve/main/yolov8n.onnx'"
        );
        std::process::exit(1);
    }

    println!("Connected Detection Pipeline");
    println!("============================");
    println!("Video: {}", video_path);
    println!("Model: {}", model_path);
    println!("Execution: {:?}", execution_provider);
    println!("Max frames: {}", max_frames);
    println!(
        "HIVE sync: {}",
        if enable_sync {
            "enabled"
        } else {
            "disabled (dry run)"
        }
    );
    println!();

    // Initialize detector
    println!("Loading ONNX model...");
    let onnx_config = OnnxConfig {
        model_path: model_path.into(),
        confidence_threshold: 0.5,
        num_threads: num_cpus::get(),
        execution_provider,
        ..Default::default()
    };

    let mut detector = OnnxDetector::new(onnx_config)?;

    println!("Warming up model...");
    let warmup_start = Instant::now();
    detector.warm_up().await?;
    println!(
        "Warmup complete in {:.1}ms",
        warmup_start.elapsed().as_secs_f64() * 1000.0
    );

    // Initialize tracker
    let tracker_config = TrackerConfig {
        max_age: 30,        // Frames before track is deleted
        min_hits: 3,        // Hits before track is confirmed
        iou_threshold: 0.3, // IoU threshold for matching
        ..Default::default()
    };
    let tracker = SimulatedTracker::new(tracker_config);

    // Configure pipeline
    // Reference position: Atlanta (example - would come from GPS/INS in production)
    let pipeline_config = PipelineConfig {
        platform_id: "Jetson-Platform-1".to_string(),
        model_id: "YOLOv8n-Tracker".to_string(),
        min_confidence: 0.5,
        confirmed_only: true,
        reference_position: Some((33.7749, -84.3958)), // Atlanta
        meters_per_pixel: 0.05, // 5cm per pixel (depends on camera height/zoom)
        camera_bearing: 0.0,    // North-facing camera
        camera_hfov: 60.0,      // 60 degree horizontal FOV
    };

    // Create the inference pipeline
    let pipeline = InferencePipeline::new(detector, tracker, pipeline_config);
    pipeline.initialize().await?;

    println!("Pipeline initialized");
    println!();

    // Initialize video source
    println!("Opening video...");
    let video_config =
        VideoConfig::video_file(video_path, false).with_platform_id("Jetson-Platform-1");

    let mut video = VideoSource::new(video_config)?;
    video.start().await?;

    // Optional: Initialize HIVE sync
    // In production, this would connect to the HIVE mesh network
    let sync_client: Option<SyncTracker> = if enable_sync {
        println!("Initializing HIVE sync client...");
        // Note: Full HIVE sync requires AutomergeIrohBackend setup
        // For now, we use a tracking-only client that logs what would be synced
        Some(SyncTracker::new("Jetson-Platform-1", "alpha-formation"))
    } else {
        None
    };

    // Process frames
    println!("Processing frames...");
    println!();

    let mut frame_count = 0usize;
    let mut total_detections = 0usize;
    let mut total_updates = 0usize;
    let mut latencies = Vec::new();
    let pipeline_start = Instant::now();

    while let Some(frame) = video.next_frame().await? {
        if frame_count >= max_frames {
            break;
        }

        // Resize frame to model input size (640x640)
        let resized_frame = resize_frame(&frame, 640, 640);

        // Process through full pipeline
        let process_start = Instant::now();
        let output = pipeline.process(&resized_frame).await?;
        let process_ms = process_start.elapsed().as_secs_f64() * 1000.0;

        latencies.push(process_ms);
        total_detections += output.detections.len();
        total_updates += output.track_updates.len();

        // Publish track updates to HIVE (if enabled)
        if let Some(ref mut sync) = sync_client.as_ref() {
            for update in &output.track_updates {
                sync.track_update(update);
            }
        }

        // Print progress
        if !output.track_updates.is_empty() {
            println!(
                "Frame {}: {} detections, {} tracks, {} updates ({:.1}ms)",
                frame_count,
                output.detections.len(),
                output.tracks.len(),
                output.track_updates.len(),
                process_ms
            );
            for update in &output.track_updates {
                println!(
                    "  TRACK {} | {} ({:.0}%) | ({:.6}, {:.6}) CEP:{:.1}m",
                    update.track_id,
                    update.classification,
                    update.confidence * 100.0,
                    update.position.lat,
                    update.position.lon,
                    update.position.cep_m.unwrap_or(0.0)
                );
                if let Some(vel) = &update.velocity {
                    println!(
                        "         | bearing: {:.0}° speed: {:.2} m/s",
                        vel.bearing, vel.speed_mps
                    );
                }
            }
        } else if frame_count % 20 == 0 {
            println!(
                "Frame {}: {} detections, {} tracks (awaiting confirmation) ({:.1}ms)",
                frame_count,
                output.detections.len(),
                output.tracks.len(),
                process_ms
            );
        }

        frame_count += 1;
    }

    video.stop().await?;

    // Calculate statistics
    let total_time = pipeline_start.elapsed();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg_latency = latencies.iter().sum::<f64>() / latencies.len().max(1) as f64;
    let min_latency = latencies.first().copied().unwrap_or(0.0);
    let max_latency = latencies.last().copied().unwrap_or(0.0);
    let p50_latency = latencies.get(latencies.len() / 2).copied().unwrap_or(0.0);
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p95_latency = latencies.get(p95_idx).copied().unwrap_or(max_latency);
    let fps = frame_count as f64 / total_time.as_secs_f64();

    // Get pipeline metrics
    let metrics = pipeline.metrics_summary().await;
    let tracker_stats = pipeline.tracker_stats().await;

    println!();
    println!("Results");
    println!("=======");
    println!("Frames processed: {}", frame_count);
    println!("Total detections: {}", total_detections);
    println!(
        "Total tracks created: {}",
        tracker_stats.total_tracks_created
    );
    println!("Active tracks (final): {}", tracker_stats.active_tracks);
    println!("Track updates published: {}", total_updates);
    println!();
    println!("Timing:");
    println!("  Total time: {:.2}s", total_time.as_secs_f64());
    println!("  Throughput: {:.2} FPS", fps);
    println!();
    println!("Pipeline latency (ms):");
    println!("  Min: {:.1}", min_latency);
    println!("  Avg: {:.1}", avg_latency);
    println!("  P50: {:.1}", p50_latency);
    println!("  P95: {:.1}", p95_latency);
    println!("  Max: {:.1}", max_latency);
    println!();
    println!("Metrics summary:");
    println!(
        "  Detection avg: {:.1}ms",
        metrics.detection_latency.mean_ms
    );
    println!("  Tracking avg: {:.1}ms", metrics.tracking_latency.mean_ms);
    println!("  Pipeline avg: {:.1}ms", metrics.pipeline_latency.mean_ms);

    // Print sync stats if enabled
    if let Some(sync) = sync_client {
        println!();
        println!("HIVE Sync Stats:");
        println!("  Track updates queued: {}", sync.updates_count());
        println!("  Unique tracks: {}", sync.unique_tracks());
    }

    println!();
    println!("Pipeline ready for HIVE integration!");
    println!("Next: Connect HiveSyncClient with AutomergeIrohBackend");

    Ok(())
}

/// Resize a frame to target dimensions using simple nearest-neighbor
/// This is a basic implementation - production would use GPU resize
#[cfg(feature = "onnx-inference")]
fn resize_frame(
    frame: &hive_inference::inference::VideoFrame,
    target_width: u32,
    target_height: u32,
) -> hive_inference::inference::VideoFrame {
    use hive_inference::inference::VideoFrame;

    let src_width = frame.width;
    let src_height = frame.height;

    let mut resized_data = vec![0u8; (target_width * target_height * 3) as usize];

    let x_ratio = src_width as f32 / target_width as f32;
    let y_ratio = src_height as f32 / target_height as f32;

    for y in 0..target_height {
        for x in 0..target_width {
            let src_x = (x as f32 * x_ratio) as u32;
            let src_y = (y as f32 * y_ratio) as u32;

            let src_idx = ((src_y * src_width + src_x) * 3) as usize;
            let dst_idx = ((y * target_width + x) * 3) as usize;

            if src_idx + 2 < frame.data.len() && dst_idx + 2 < resized_data.len() {
                resized_data[dst_idx] = frame.data[src_idx];
                resized_data[dst_idx + 1] = frame.data[src_idx + 1];
                resized_data[dst_idx + 2] = frame.data[src_idx + 2];
            }
        }
    }

    VideoFrame {
        frame_id: frame.frame_id,
        timestamp: frame.timestamp,
        width: target_width,
        height: target_height,
        data: resized_data,
        metadata: frame.metadata.clone(),
    }
}

/// Simple sync tracker for demonstration
/// In production, this would be replaced with ConnectedPipeline + HiveSyncClient
#[cfg(feature = "onnx-inference")]
struct SyncTracker {
    #[allow(dead_code)]
    platform_id: String,
    formation_id: String,
    updates: std::sync::Mutex<Vec<TrackUpdate>>,
    track_ids: std::sync::Mutex<std::collections::HashSet<String>>,
}

#[cfg(feature = "onnx-inference")]
impl SyncTracker {
    fn new(platform_id: &str, formation_id: &str) -> Self {
        Self {
            platform_id: platform_id.to_string(),
            formation_id: formation_id.to_string(),
            updates: std::sync::Mutex::new(Vec::new()),
            track_ids: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    fn track_update(&self, update: &TrackUpdate) {
        tracing::debug!(
            "HIVE: Would publish track {} to formation {}",
            update.track_id,
            self.formation_id
        );
        self.updates.lock().unwrap().push(update.clone());
        self.track_ids
            .lock()
            .unwrap()
            .insert(update.track_id.clone());
    }

    fn updates_count(&self) -> usize {
        self.updates.lock().unwrap().len()
    }

    fn unique_tracks(&self) -> usize {
        self.track_ids.lock().unwrap().len()
    }
}

#[cfg(not(feature = "onnx-inference"))]
fn main() {
    eprintln!("This example requires the 'onnx-inference' feature.");
    eprintln!(
        "Run with: cargo run --example connected_detection --features onnx-inference --release -- <video>"
    );
}
