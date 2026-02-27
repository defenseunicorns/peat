//! ONNX Inference Benchmark
//!
//! Run with: cargo run --example onnx_benchmark --features onnx-inference --release
//!
//! Measures inference latency with YOLOv8n ONNX model.
//!
//! For GPU acceleration on Jetson, set:
//!   export LD_LIBRARY_PATH=/path/to/onnxruntime-gpu/onnxruntime/capi:$LD_LIBRARY_PATH
//!
//! Use --gpu flag to enable TensorRT/CUDA execution providers.

#[cfg(feature = "onnx-inference")]
use peat_inference::inference::{
    Detector, ExecutionProvider, OnnxConfig, OnnxDetector, VideoFrame,
};

#[cfg(feature = "onnx-inference")]
use std::time::Instant;

#[cfg(feature = "onnx-inference")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Check for --gpu flag
    let use_gpu = std::env::args().any(|arg| arg == "--gpu" || arg == "-g");
    let use_tensorrt = std::env::args().any(|arg| arg == "--tensorrt" || arg == "-t");

    let model_path = "models/yolov8n.onnx";
    if !std::path::Path::new(model_path).exists() {
        eprintln!(
            "Model not found at {}. Download from Hugging Face:",
            model_path
        );
        eprintln!("  curl -L -o models/yolov8n.onnx 'https://huggingface.co/Kalray/yolov8/resolve/main/yolov8n.onnx'");
        return Ok(());
    }

    let execution_provider = if use_tensorrt {
        ExecutionProvider::TensorRT
    } else if use_gpu {
        ExecutionProvider::Cuda
    } else {
        ExecutionProvider::Cpu
    };

    let num_threads = num_cpus::get();
    println!("ONNX YOLOv8n Benchmark");
    println!("======================");
    println!("Model: {}", model_path);
    println!("Execution Provider: {:?}", execution_provider);
    println!("Threads: {}", num_threads);
    println!();

    // Create detector
    let config = OnnxConfig {
        model_path: model_path.into(),
        confidence_threshold: 0.5,
        num_threads,
        execution_provider,
        ..Default::default()
    };

    let mut detector = OnnxDetector::new(config)?;

    // Warm up
    println!("Warming up model...");
    let warmup_start = Instant::now();
    detector.warm_up().await?;
    println!(
        "Warmup complete in {:.1}ms",
        warmup_start.elapsed().as_secs_f64() * 1000.0
    );
    println!();

    // Create test frames with different patterns
    let frames: Vec<VideoFrame> = (0..10)
        .map(|i| {
            let mut data = vec![0u8; 640 * 640 * 3];
            for (j, byte) in data.iter_mut().enumerate() {
                *byte = ((j * (i + 1) * 7) % 256) as u8;
            }
            VideoFrame {
                frame_id: i as u64,
                timestamp: chrono::Utc::now(),
                width: 640,
                height: 640,
                data,
                metadata: Default::default(),
            }
        })
        .collect();

    // Run benchmark
    println!("Running {} inferences...", frames.len());
    let mut latencies = Vec::new();
    let mut total_detections = 0;

    for frame in &frames {
        let start = Instant::now();
        let detections = detector.detect(frame).await?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        latencies.push(elapsed_ms);
        total_detections += detections.len();

        println!(
            "  Frame {}: {:.1}ms, {} detections",
            frame.frame_id,
            elapsed_ms,
            detections.len()
        );
    }

    // Calculate statistics
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = latencies.first().unwrap_or(&0.0);
    let max = latencies.last().unwrap_or(&0.0);
    let avg: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let p50 = latencies.get(latencies.len() / 2).unwrap_or(&0.0);
    let p95 = latencies.get(latencies.len() * 95 / 100).unwrap_or(max);
    let fps = 1000.0 / avg;

    println!();
    println!("Results");
    println!("-------");
    println!("  Inferences: {}", latencies.len());
    println!("  Total detections: {}", total_detections);
    println!("  Latency (ms):");
    println!("    Min: {:.1}", min);
    println!("    Avg: {:.1}", avg);
    println!("    P50: {:.1}", p50);
    println!("    P95: {:.1}", p95);
    println!("    Max: {:.1}", max);
    println!("  Throughput: {:.1} FPS", fps);

    Ok(())
}

#[cfg(not(feature = "onnx-inference"))]
fn main() {
    eprintln!("This example requires the 'onnx-inference' feature.");
    eprintln!("Run with: cargo run --example onnx_benchmark --features onnx-inference --release");
}
