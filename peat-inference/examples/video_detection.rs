//! End-to-end video detection pipeline
//!
//! Runs YOLOv8 object detection on video frames from a file.
//!
//! Usage:
//!   cargo run --example video_detection --features onnx-inference --release -- <video_file>
//!
//! Options:
//!   --gpu        Use CUDA execution provider
//!   --tensorrt   Use TensorRT execution provider (best on Jetson)
//!   --frames N   Process N frames (default: 100)
//!
//! Example:
//!   cargo run --example video_detection --features onnx-inference --release -- 223461.mp4 --tensorrt --frames 50

#[cfg(feature = "onnx-inference")]
use peat_inference::inference::{
    Detector, ExecutionProvider, OnnxConfig, OnnxDetector, VideoConfig, VideoFrame, VideoInput,
    VideoSource,
};

#[cfg(feature = "onnx-inference")]
use std::time::Instant;

#[cfg(feature = "onnx-inference")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

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
                "Usage: {} <video_file> [--gpu|--tensorrt] [--frames N]",
                args[0]
            );
            eprintln!();
            eprintln!("Example: {} 223461.mp4 --tensorrt --frames 50", args[0]);
            std::process::exit(1);
        }
    };

    let use_tensorrt = args.iter().any(|arg| arg == "--tensorrt" || arg == "-t");
    let use_gpu = args.iter().any(|arg| arg == "--gpu" || arg == "-g");

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
        eprintln!("  curl -L -o models/yolov8n.onnx 'https://huggingface.co/Kalray/yolov8/resolve/main/yolov8n.onnx'");
        std::process::exit(1);
    }

    println!("Video Detection Pipeline");
    println!("========================");
    println!("Video: {}", video_path);
    println!("Model: {}", model_path);
    println!("Execution: {:?}", execution_provider);
    println!("Max frames: {}", max_frames);
    println!();

    // Initialize detector
    println!("Loading ONNX model...");
    let config = OnnxConfig {
        model_path: model_path.into(),
        confidence_threshold: 0.5,
        num_threads: num_cpus::get(),
        execution_provider,
        ..Default::default()
    };

    let mut detector = OnnxDetector::new(config)?;

    println!("Warming up model...");
    let warmup_start = Instant::now();
    detector.warm_up().await?;
    println!(
        "Warmup complete in {:.1}ms",
        warmup_start.elapsed().as_secs_f64() * 1000.0
    );
    println!();

    // Initialize video source
    println!("Opening video...");
    let video_config =
        VideoConfig::video_file(video_path, false).with_platform_id("detection-platform");

    let mut video = VideoSource::new(video_config)?;
    video.start().await?;

    // Process frames
    println!("Processing frames...");
    println!();

    let mut frame_count = 0usize;
    let mut total_detections = 0usize;
    let mut latencies = Vec::new();
    let pipeline_start = Instant::now();

    while let Some(frame) = video.next_frame().await? {
        if frame_count >= max_frames {
            break;
        }

        // Resize frame to model input size (640x640)
        let resized_frame = resize_frame(&frame, 640, 640);

        let inference_start = Instant::now();
        let detections = detector.detect(&resized_frame).await?;
        let inference_ms = inference_start.elapsed().as_secs_f64() * 1000.0;

        latencies.push(inference_ms);
        total_detections += detections.len();

        // Print detections
        if !detections.is_empty() {
            println!(
                "Frame {}: {} detections in {:.1}ms",
                frame_count,
                detections.len(),
                inference_ms
            );
            for det in &detections {
                // Convert normalized coords to pixel coords (relative to 640x640 model input)
                let px = (det.bbox.x * 640.0) as i32;
                let py = (det.bbox.y * 640.0) as i32;
                let pw = (det.bbox.width * 640.0) as i32;
                let ph = (det.bbox.height * 640.0) as i32;
                println!(
                    "  - {} ({:.1}%) at ({}, {}, {}x{})",
                    det.classification.label,
                    det.classification.confidence * 100.0,
                    px,
                    py,
                    pw,
                    ph
                );
            }
        } else if frame_count % 10 == 0 {
            println!(
                "Frame {}: no detections ({:.1}ms)",
                frame_count, inference_ms
            );
        }

        frame_count += 1;
    }

    video.stop().await?;

    // Calculate statistics
    let total_time = pipeline_start.elapsed();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let min_latency = latencies.first().copied().unwrap_or(0.0);
    let max_latency = latencies.last().copied().unwrap_or(0.0);
    let p50_latency = latencies.get(latencies.len() / 2).copied().unwrap_or(0.0);
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p95_latency = latencies.get(p95_idx).copied().unwrap_or(max_latency);
    let fps = frame_count as f64 / total_time.as_secs_f64();

    println!();
    println!("Results");
    println!("=======");
    println!("Frames processed: {}", frame_count);
    println!("Total detections: {}", total_detections);
    println!("Total time: {:.2}s", total_time.as_secs_f64());
    println!("Throughput: {:.2} FPS", fps);
    println!();
    println!("Inference latency (ms):");
    println!("  Min: {:.1}", min_latency);
    println!("  Avg: {:.1}", avg_latency);
    println!("  P50: {:.1}", p50_latency);
    println!("  P95: {:.1}", p95_latency);
    println!("  Max: {:.1}", max_latency);

    Ok(())
}

/// Resize a frame to target dimensions using simple nearest-neighbor
/// This is a basic implementation - production would use GPU resize
#[cfg(feature = "onnx-inference")]
fn resize_frame(frame: &VideoFrame, target_width: u32, target_height: u32) -> VideoFrame {
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

#[cfg(not(feature = "onnx-inference"))]
fn main() {
    eprintln!("This example requires the 'onnx-inference' feature.");
    eprintln!("Run with: cargo run --example video_detection --features onnx-inference --release -- <video>");
}
