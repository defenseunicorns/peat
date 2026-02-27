# ONNX Inference on Jetson

This document describes how to run YOLOv8 object detection using ONNX Runtime with GPU acceleration on NVIDIA Jetson devices.

## Overview

The PEAT M1 POC includes an ONNX-based inference pipeline for object detection. It supports multiple execution providers:

- **CPU**: Works everywhere, no special setup required
- **CUDA**: GPU acceleration via CUDA
- **TensorRT**: Optimized inference using NVIDIA TensorRT

## Prerequisites

### Hardware
- NVIDIA Jetson (tested on Jetson Orin with JetPack R36.4.7)
- Sufficient storage for models (~15 MB for YOLOv8n)

### Software (Jetson)
- JetPack SDK with CUDA and TensorRT
- Rust toolchain (stable)

Verify TensorRT is installed:
```bash
dpkg -l | grep tensorrt
# Should show: tensorrt 10.3.0.x or similar
```

## Quick Start

### 1. Download YOLOv8 Model

```bash
mkdir -p models
curl -L -o models/yolov8n.onnx \
  'https://huggingface.co/Kalray/yolov8/resolve/main/yolov8n.onnx'
```

The YOLOv8n model is ~12.8 MB and detects 80 COCO classes.

### 2. CPU-Only Build

For CPU-only inference (no GPU setup required):

```bash
cargo build --features onnx-inference --release
cargo run --example onnx_benchmark --features onnx-inference --release
```

### 3. GPU-Accelerated Build (Jetson)

For CUDA/TensorRT acceleration, you need the GPU-enabled ONNX Runtime library.

#### Download GPU ONNX Runtime

```bash
# Download the GPU wheel for aarch64
wget https://github.com/ultralytics/assets/releases/download/v8.3.0/onnxruntime_gpu-1.23.0-cp310-cp310-linux_aarch64.whl

# Extract to onnxruntime-gpu/
unzip onnxruntime_gpu-1.23.0-cp310-cp310-linux_aarch64.whl -d onnxruntime-gpu/

# Create required symlinks
cd onnxruntime-gpu/onnxruntime/capi
ln -sf libonnxruntime.so.1.23.0 libonnxruntime.so.1
ln -sf libonnxruntime.so.1.23.0 libonnxruntime.so
cd ../../..
```

#### Set Environment Variables

Before building or running with GPU support:

```bash
export ORT_LIB_LOCATION=/home/kit/Code/peat-m1-poc/onnxruntime-gpu/onnxruntime/capi
export LD_LIBRARY_PATH=$ORT_LIB_LOCATION:$LD_LIBRARY_PATH
```

Add these to your `.bashrc` for persistence.

#### Build and Run

```bash
# Build with GPU support
cargo build --features onnx-inference --release

# Run benchmark with different providers
cargo run --example onnx_benchmark --features onnx-inference --release           # CPU
cargo run --example onnx_benchmark --features onnx-inference --release -- --gpu  # CUDA
cargo run --example onnx_benchmark --features onnx-inference --release -- -t     # TensorRT
```

## Benchmark Results

Tested on Jetson Orin with JetPack R36.4.7, YOLOv8n model (640x640 input):

| Execution Provider | Avg Latency | FPS  |
|-------------------|-------------|------|
| CPU               | ~279 ms     | ~3.6 |
| CUDA              | ~272 ms     | ~3.7 |
| TensorRT          | ~266 ms     | ~3.8 |

**Note**: The similar performance across providers indicates that CPU-based preprocessing (image resize/normalize) dominates the total inference time. Future optimizations should focus on GPU-accelerated preprocessing.

## Code Usage

### Basic Detection

```rust
use peat_inference::inference::{
    Detector, ExecutionProvider, OnnxConfig, OnnxDetector, VideoFrame
};

// Configure detector
let config = OnnxConfig {
    model_path: "models/yolov8n.onnx".into(),
    confidence_threshold: 0.5,
    num_threads: num_cpus::get(),
    execution_provider: ExecutionProvider::TensorRT, // or Cuda, Cpu
    ..Default::default()
};

// Create and warm up detector
let mut detector = OnnxDetector::new(config)?;
detector.warm_up().await?;

// Run detection
let frame = VideoFrame { /* ... */ };
let detections = detector.detect(&frame).await?;

for det in detections {
    println!("Found {} at ({}, {}) conf={:.2}",
        det.class_name, det.bbox.x, det.bbox.y, det.confidence);
}
```

### Execution Provider Selection

```rust
use peat_inference::inference::ExecutionProvider;

// CPU - works everywhere
let provider = ExecutionProvider::Cpu;

// CUDA - requires GPU ONNX Runtime
let provider = ExecutionProvider::Cuda;

// TensorRT - best performance on Jetson (with first-run engine build)
let provider = ExecutionProvider::TensorRT;
```

## Architecture

```
VideoFrame (RGB bytes)
    │
    ▼
┌─────────────────────────────────────────┐
│         OnnxDetector                    │
│  ┌─────────────────────────────────┐   │
│  │  Preprocessing (CPU)            │   │
│  │  - Resize to 640x640            │   │
│  │  - RGB → float32                │   │
│  │  - Normalize (0-1)              │   │
│  └─────────────────────────────────┘   │
│                  │                      │
│                  ▼                      │
│  ┌─────────────────────────────────┐   │
│  │  ONNX Runtime Session           │   │
│  │  - CPU / CUDA / TensorRT        │   │
│  │  - YOLOv8 inference             │   │
│  └─────────────────────────────────┘   │
│                  │                      │
│                  ▼                      │
│  ┌─────────────────────────────────┐   │
│  │  Postprocessing (CPU)           │   │
│  │  - Parse YOLO output            │   │
│  │  - Apply confidence threshold   │   │
│  │  - NMS (non-max suppression)    │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
    │
    ▼
Vec<Detection>
```

## Troubleshooting

### "libonnxruntime.so.1: cannot open shared object file"

The runtime library is not in `LD_LIBRARY_PATH`:

```bash
export LD_LIBRARY_PATH=/path/to/onnxruntime-gpu/onnxruntime/capi:$LD_LIBRARY_PATH
```

### "CUDA provider not available"

Either:
1. Using CPU-only ONNX Runtime library
2. `ORT_LIB_LOCATION` not set correctly during build
3. CUDA libraries not available

Verify GPU library:
```bash
ldd onnxruntime-gpu/onnxruntime/capi/libonnxruntime.so | grep cuda
# Should show CUDA dependencies
```

### TensorRT first run is slow

TensorRT builds an optimized engine on first inference with a new model. This can take 30-60 seconds but only happens once (engine is cached).

### Model not found

Download the model to the `models/` directory:
```bash
curl -L -o models/yolov8n.onnx \
  'https://huggingface.co/Kalray/yolov8/resolve/main/yolov8n.onnx'
```

## Files

| File | Description |
|------|-------------|
| `src/inference/onnx.rs` | ONNX detector implementation |
| `examples/onnx_benchmark.rs` | Benchmark with execution provider flags |
| `models/yolov8n.onnx` | YOLOv8n ONNX model (not in git) |
| `onnxruntime-gpu/` | GPU ONNX Runtime (not in git) |

## Future Improvements

1. **GPU Preprocessing**: Move resize/normalize to GPU to reduce CPU bottleneck
2. **Batched Inference**: Process multiple frames in a single inference call
3. **INT8 Quantization**: Use TensorRT INT8 for faster inference
4. **Model Caching**: Cache TensorRT engines to avoid rebuild
5. **Async Pipeline**: Overlap preprocessing with inference
