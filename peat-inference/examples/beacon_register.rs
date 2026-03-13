//! Peat Beacon Registration Example
//!
//! Demonstrates registering an edge device with the Peat network,
//! advertising real camera and AI model capabilities.
//!
//! Usage:
//!   cargo run --example beacon_register --release
//!
//! Options:
//!   --platform-id <id>   Set platform identifier (default: auto-generated)
//!   --formation <id>     Join a specific formation
//!   --position <lat,lon> Set geographic position
//!   --json               Output registration as JSON

use peat_inference::beacon::{BeaconConfig, CameraSpec, ComputeSpec, ModelSpec, PeatBeacon};
use peat_inference::inference::JetsonInfo;
use peat_inference::messages::{ModelPerformance, OperationalStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let platform_id = args
        .iter()
        .position(|arg| arg == "--platform-id")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("edge-beacon-01");

    let formation_id = args
        .iter()
        .position(|arg| arg == "--formation")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let position: Option<(f64, f64)> = args
        .iter()
        .position(|arg| arg == "--position")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() == 2 {
                let lat: f64 = parts[0].parse().ok()?;
                let lon: f64 = parts[1].parse().ok()?;
                Some((lat, lon))
            } else {
                None
            }
        });

    let output_json = args.iter().any(|arg| arg == "--json");

    println!("Peat Beacon Registration");
    println!("========================");
    println!();

    // Detect Jetson platform
    println!("Detecting hardware...");
    let jetson_info = match JetsonInfo::detect() {
        Ok(info) => {
            println!("  Platform: {}", info.model);
            println!("  JetPack: {}", info.jetpack_version);
            println!("  CUDA: {}", info.cuda_version);
            println!("  TensorRT: {}", info.tensorrt_version);
            println!("  Memory: {} MB", info.gpu_memory_mb);
            println!("  CPU cores: {}", info.cpu_cores);
            Some(info)
        }
        Err(e) => {
            println!("  Not running on Jetson: {}", e);
            None
        }
    };
    println!();

    // Build configuration
    let mut config = BeaconConfig::new(platform_id)
        .with_name(&if let Some(ref info) = jetson_info {
            format!("{} @ {}", platform_id, info.model)
        } else {
            platform_id.to_string()
        })
        .with_camera(CameraSpec::imx219())
        .with_model(ModelSpec::yolov8n());

    // Add compute spec from detected Jetson
    if let Some(ref info) = jetson_info {
        config = config.with_compute(ComputeSpec::from_jetson(info));
    } else {
        // Fallback for non-Jetson systems
        config = config.with_compute(ComputeSpec::jetson_orin_nano());
    }

    // Add position if specified
    if let Some((lat, lon)) = position {
        config = config.with_position(lat, lon);
    }

    // Add formation if specified
    if let Some(formation) = formation_id {
        config = config.with_formation(formation);
    }

    // Create beacon
    println!("Creating beacon...");
    let beacon = PeatBeacon::new(config)?;
    println!();

    // Set to ready status
    beacon.set_status(OperationalStatus::Ready).await;

    // Simulate some measured performance (would come from actual inference)
    beacon
        .update_performance(ModelPerformance {
            precision: 0.72,
            recall: 0.68,
            fps: 3.2,
            latency_ms: Some(312.0),
        })
        .await;

    // Generate registration
    println!("Platform Registration");
    println!("---------------------");
    let registration = beacon.generate_registration().await;

    if output_json {
        println!("{}", serde_json::to_string_pretty(&registration)?);
    } else {
        println!("Platform ID: {}", registration["platform_id"]);
        println!("Name: {}", registration["name"]);
        println!("Type: {}", registration["type"]);
        println!("Status: {}", registration["status"]);

        if let Some(camera) = registration.get("camera") {
            println!();
            println!("Camera:");
            println!("  Model: {} {}", camera["manufacturer"], camera["model"]);
            println!("  Interface: {}", camera["interface"]);
            println!("  Max Resolution: {}", camera["max_resolution"]);
            println!(
                "  FOV: H={:.1}° V={:.1}° D={:.1}°",
                camera["fov"]["horizontal"].as_f64().unwrap_or(0.0),
                camera["fov"]["vertical"].as_f64().unwrap_or(0.0),
                camera["fov"]["diagonal"].as_f64().unwrap_or(0.0)
            );
            if let Some(modes) = camera["modes"].as_array() {
                println!("  Modes:");
                for mode in modes {
                    println!(
                        "    - {} @ {} fps ({})",
                        mode["resolution"], mode["fps"], mode["format"]
                    );
                }
            }
        }

        if let Some(model) = registration.get("model") {
            println!();
            println!("AI Model:");
            println!("  ID: {}", model["id"]);
            println!("  Name: {} v{}", model["name"], model["version"]);
            println!("  Type: {}", model["type"]);
            println!(
                "  Framework: {} ({})",
                model["framework"], model["quantization"]
            );
            println!("  Input: {:?}", model["input_size"]);
            println!("  Classes: {}", model["num_classes"]);
        }

        if let Some(compute) = registration.get("compute") {
            println!();
            println!("Compute Platform:");
            println!("  Model: {}", compute["model"]);
            println!(
                "  GPU: {} ({} CUDA cores, {} Tensor cores)",
                compute["gpu_arch"], compute["cuda_cores"], compute["tensor_cores"]
            );
            println!("  DLA cores: {}", compute["dla_cores"]);
            println!("  Memory: {} MB", compute["memory_mb"]);
            println!("  CPU cores: {}", compute["cpu_cores"]);
            println!("  SDK: {}", compute["sdk_version"]);
            println!("  CUDA: {}", compute["cuda_version"]);
            println!("  TensorRT: {}", compute["tensorrt_version"]);
        }

        if let Some(pos) = registration.get("position") {
            println!();
            println!("Position:");
            println!("  Lat: {}", pos["lat"]);
            println!("  Lon: {}", pos["lon"]);
            if let Some(alt) = pos.get("alt_m") {
                if !alt.is_null() {
                    println!("  Alt: {} m", alt);
                }
            }
        }

        if let Some(formation) = registration.get("formation_id") {
            println!();
            println!("Formation: {}", formation);
        }
    }

    // Generate capability advertisement
    println!();
    println!("Capability Advertisement");
    println!("------------------------");
    let advert = beacon.generate_advertisement().await;

    println!("Platform: {}", advert.platform_id);
    println!("Advertised at: {}", advert.advertised_at);

    if let Some(ref resources) = advert.resources {
        println!("Resources:");
        if let Some(gpu) = resources.gpu_utilization {
            println!("  GPU utilization: {:.1}%", gpu * 100.0);
        }
        if let Some(mem_used) = resources.memory_used_mb {
            if let Some(mem_total) = resources.memory_total_mb {
                println!("  Memory: {} / {} MB", mem_used, mem_total);
            } else {
                println!("  Memory used: {} MB", mem_used);
            }
        }
    }

    for model in &advert.models {
        println!();
        println!("Model: {} v{}", model.model_id, model.model_version);
        println!("  Type: {}", model.model_type);
        println!("  Status: {:?}", model.operational_status);
        println!("  Performance:");
        println!("    Precision: {:.2}", model.performance.precision);
        println!("    Recall: {:.2}", model.performance.recall);
        println!("    FPS: {:.1}", model.performance.fps);
        if let Some(latency) = model.performance.latency_ms {
            println!("    Latency: {:.1} ms", latency);
        }
    }

    println!();
    println!("Beacon ready for Peat network integration!");
    println!();
    println!("Next steps:");
    println!("  1. Connect to Peat mesh using AutomergeIrohBackend");
    println!("  2. Publish registration to 'platforms' collection");
    println!("  3. Periodically publish capability advertisements");
    println!("  4. Start inference pipeline and publish track updates");

    Ok(())
}
