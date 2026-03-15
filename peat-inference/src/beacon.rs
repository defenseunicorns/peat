//! Peat Beacon Client
//!
//! Registers the edge device as a sensor platform with the Peat network,
//! advertising camera and AI model capabilities with real hardware specs.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use peat_inference::beacon::{PeatBeacon, BeaconConfig, CameraSpec, ModelSpec};
//!
//! // Create beacon with auto-detected hardware
//! let beacon = PeatBeacon::new(BeaconConfig::auto_detect()?)?;
//!
//! // Or with explicit configuration
//! let config = BeaconConfig::new("edge-platform-01")
//!     .with_camera(CameraSpec::imx219())
//!     .with_model(ModelSpec::yolov8n())
//!     .with_position(33.7749, -84.3958);
//!
//! let beacon = PeatBeacon::new(config)?;
//!
//! // Start publishing capabilities
//! beacon.start().await?;
//! ```

use crate::inference::JetsonInfo;
use crate::messages::{
    CapabilityAdvertisement, ModelCapability, ModelPerformance, OperationalStatus, ResourceMetrics,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Camera sensor specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraSpec {
    /// Sensor model (e.g., "IMX219")
    pub model: String,
    /// Manufacturer
    pub manufacturer: String,
    /// Sensor type (e.g., "CMOS")
    pub sensor_type: String,
    /// Interface (e.g., "CSI-2", "USB")
    pub interface: String,
    /// Maximum resolution width
    pub max_width: u32,
    /// Maximum resolution height
    pub max_height: u32,
    /// Supported resolutions with frame rates
    pub modes: Vec<CameraMode>,
    /// Horizontal field of view in degrees
    pub hfov_degrees: f64,
    /// Vertical field of view in degrees
    pub vfov_degrees: f64,
    /// Diagonal field of view in degrees
    pub dfov_degrees: f64,
    /// Pixel size in micrometers
    pub pixel_size_um: f64,
    /// Sensor size (width x height in mm)
    pub sensor_size_mm: (f64, f64),
}

/// Camera resolution mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraMode {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Frame rate in FPS
    pub fps: f64,
    /// Pixel format (e.g., "RG10", "RGBA")
    pub format: String,
}

impl CameraSpec {
    /// Sony IMX219 - Raspberry Pi Camera Module v2 (commonly used on Jetson)
    pub fn imx219() -> Self {
        Self {
            model: "IMX219".to_string(),
            manufacturer: "Sony".to_string(),
            sensor_type: "CMOS".to_string(),
            interface: "CSI-2".to_string(),
            max_width: 3280,
            max_height: 2464,
            modes: vec![
                CameraMode {
                    width: 3280,
                    height: 2464,
                    fps: 21.0,
                    format: "RG10".to_string(),
                },
                CameraMode {
                    width: 3280,
                    height: 1848,
                    fps: 28.0,
                    format: "RG10".to_string(),
                },
                CameraMode {
                    width: 1920,
                    height: 1080,
                    fps: 30.0,
                    format: "RG10".to_string(),
                },
                CameraMode {
                    width: 1640,
                    height: 1232,
                    fps: 30.0,
                    format: "RG10".to_string(),
                },
                CameraMode {
                    width: 1280,
                    height: 720,
                    fps: 60.0,
                    format: "RG10".to_string(),
                },
            ],
            // IMX219 specs: 3.68mm x 2.76mm sensor, 1.12μm pixels
            hfov_degrees: 62.2, // With standard lens
            vfov_degrees: 48.8,
            dfov_degrees: 77.0,
            pixel_size_um: 1.12,
            sensor_size_mm: (3.68, 2.76),
        }
    }

    /// Sony IMX477 - Raspberry Pi HQ Camera
    pub fn imx477() -> Self {
        Self {
            model: "IMX477".to_string(),
            manufacturer: "Sony".to_string(),
            sensor_type: "CMOS".to_string(),
            interface: "CSI-2".to_string(),
            max_width: 4056,
            max_height: 3040,
            modes: vec![
                CameraMode {
                    width: 4056,
                    height: 3040,
                    fps: 10.0,
                    format: "RG12".to_string(),
                },
                CameraMode {
                    width: 1920,
                    height: 1080,
                    fps: 50.0,
                    format: "RG12".to_string(),
                },
            ],
            hfov_degrees: 41.0, // Depends on lens
            vfov_degrees: 31.0,
            dfov_degrees: 51.0,
            pixel_size_um: 1.55,
            sensor_size_mm: (6.287, 4.712),
        }
    }

    /// Generic USB camera
    pub fn usb_camera(width: u32, height: u32, fps: f64) -> Self {
        Self {
            model: "USB Camera".to_string(),
            manufacturer: "Generic".to_string(),
            sensor_type: "CMOS".to_string(),
            interface: "USB".to_string(),
            max_width: width,
            max_height: height,
            modes: vec![CameraMode {
                width,
                height,
                fps,
                format: "YUYV".to_string(),
            }],
            hfov_degrees: 60.0,
            vfov_degrees: 45.0,
            dfov_degrees: 75.0,
            pixel_size_um: 2.0,
            sensor_size_mm: (4.0, 3.0),
        }
    }

    /// Get the best mode for a target resolution
    pub fn best_mode_for(&self, target_width: u32, target_height: u32) -> Option<&CameraMode> {
        self.modes
            .iter()
            .filter(|m| m.width >= target_width && m.height >= target_height)
            .min_by_key(|m| m.width * m.height)
    }

    /// Get the highest FPS mode
    pub fn highest_fps_mode(&self) -> Option<&CameraMode> {
        self.modes
            .iter()
            .max_by(|a, b| a.fps.partial_cmp(&b.fps).unwrap())
    }
}

/// AI model specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Model identifier
    pub model_id: String,
    /// Model name
    pub name: String,
    /// Model version
    pub version: String,
    /// Model type (e.g., "detector", "tracker", "detector_tracker")
    pub model_type: String,
    /// Model file hash (SHA256)
    pub hash: String,
    /// Model file size in bytes
    pub size_bytes: u64,
    /// Input dimensions (width, height, channels)
    pub input_size: (u32, u32, u32),
    /// Number of classes
    pub num_classes: usize,
    /// Class labels
    pub class_labels: Vec<String>,
    /// Expected performance metrics
    pub expected_performance: ModelPerformance,
    /// Quantization type (e.g., "FP32", "FP16", "INT8")
    pub quantization: String,
    /// Framework (e.g., "ONNX", "TensorRT", "PyTorch")
    pub framework: String,
}

impl ModelSpec {
    /// YOLOv8 Nano - smallest and fastest YOLOv8 variant
    pub fn yolov8n() -> Self {
        Self {
            model_id: "yolov8n".to_string(),
            name: "YOLOv8 Nano".to_string(),
            version: "8.0.0".to_string(),
            model_type: "detector".to_string(),
            hash: "sha256:pending".to_string(),
            size_bytes: 6_200_000, // ~6.2MB ONNX
            input_size: (640, 640, 3),
            num_classes: 80,
            class_labels: coco_labels(),
            expected_performance: ModelPerformance {
                precision: 0.37, // mAP50-95 on COCO
                recall: 0.52,
                fps: 3.5, // On Jetson Orin Nano with TensorRT
                latency_ms: Some(285.0),
            },
            quantization: "FP16".to_string(),
            framework: "ONNX".to_string(),
        }
    }

    /// YOLOv8 Small
    pub fn yolov8s() -> Self {
        Self {
            model_id: "yolov8s".to_string(),
            name: "YOLOv8 Small".to_string(),
            version: "8.0.0".to_string(),
            model_type: "detector".to_string(),
            hash: "sha256:pending".to_string(),
            size_bytes: 22_500_000, // ~22.5MB ONNX
            input_size: (640, 640, 3),
            num_classes: 80,
            class_labels: coco_labels(),
            expected_performance: ModelPerformance {
                precision: 0.45,
                recall: 0.61,
                fps: 2.0,
                latency_ms: Some(500.0),
            },
            quantization: "FP16".to_string(),
            framework: "ONNX".to_string(),
        }
    }

    /// Custom detector model
    pub fn custom(
        model_id: &str,
        name: &str,
        version: &str,
        input_size: (u32, u32, u32),
        num_classes: usize,
    ) -> Self {
        Self {
            model_id: model_id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            model_type: "detector".to_string(),
            hash: "sha256:pending".to_string(),
            size_bytes: 0,
            input_size,
            num_classes,
            class_labels: Vec::new(),
            expected_performance: ModelPerformance {
                precision: 0.0,
                recall: 0.0,
                fps: 0.0,
                latency_ms: None,
            },
            quantization: "FP32".to_string(),
            framework: "ONNX".to_string(),
        }
    }

    /// Update performance from actual measurements
    pub fn with_measured_performance(mut self, perf: ModelPerformance) -> Self {
        self.expected_performance = perf;
        self
    }

    /// Set model hash
    pub fn with_hash(mut self, hash: &str) -> Self {
        self.hash = hash.to_string();
        self
    }
}

/// Compute platform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeSpec {
    /// Platform model (e.g., "Jetson Orin Nano")
    pub model: String,
    /// Manufacturer
    pub manufacturer: String,
    /// GPU architecture (e.g., "Ampere", "Volta")
    pub gpu_arch: String,
    /// CUDA cores
    pub cuda_cores: u32,
    /// Tensor cores
    pub tensor_cores: u32,
    /// DLA cores
    pub dla_cores: u32,
    /// Total memory in MB
    pub memory_mb: u64,
    /// CPU cores
    pub cpu_cores: usize,
    /// JetPack/SDK version
    pub sdk_version: String,
    /// CUDA version
    pub cuda_version: String,
    /// TensorRT version
    pub tensorrt_version: String,
}

impl ComputeSpec {
    /// Detect from current Jetson hardware
    pub fn from_jetson(info: &JetsonInfo) -> Self {
        // Determine GPU specs based on model
        let (gpu_arch, cuda_cores, tensor_cores) = if info.model.contains("Orin Nano") {
            ("Ampere", 512, 16)
        } else if info.model.contains("Orin NX") {
            ("Ampere", 1024, 32)
        } else if info.model.contains("AGX Orin") {
            ("Ampere", 2048, 64)
        } else if info.model.contains("Xavier NX") {
            ("Volta", 384, 48)
        } else if info.model.contains("AGX Xavier") {
            ("Volta", 512, 64)
        } else {
            ("Unknown", 0, 0)
        };

        Self {
            model: info.model.clone(),
            manufacturer: "NVIDIA".to_string(),
            gpu_arch: gpu_arch.to_string(),
            cuda_cores,
            tensor_cores,
            dla_cores: info.dla_cores as u32,
            memory_mb: info.gpu_memory_mb,
            cpu_cores: info.cpu_cores,
            sdk_version: info.jetpack_version.clone(),
            cuda_version: info.cuda_version.clone(),
            tensorrt_version: info.tensorrt_version.clone(),
        }
    }

    /// Manual specification for Jetson Orin Nano
    pub fn jetson_orin_nano() -> Self {
        Self {
            model: "Jetson Orin Nano".to_string(),
            manufacturer: "NVIDIA".to_string(),
            gpu_arch: "Ampere".to_string(),
            cuda_cores: 512,
            tensor_cores: 16,
            dla_cores: 0, // Orin Nano doesn't have DLA
            memory_mb: 8192,
            cpu_cores: 6,
            sdk_version: "JetPack 6.0".to_string(),
            cuda_version: "12.2".to_string(),
            tensorrt_version: "8.6".to_string(),
        }
    }
}

/// Beacon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconConfig {
    /// Unique platform identifier
    pub platform_id: String,
    /// Human-readable name
    pub name: String,
    /// Camera specification
    pub camera: Option<CameraSpec>,
    /// AI model specification
    pub model: Option<ModelSpec>,
    /// Compute platform specification
    pub compute: Option<ComputeSpec>,
    /// Geographic position (lat, lon)
    pub position: Option<(f64, f64)>,
    /// Altitude in meters (optional)
    pub altitude_m: Option<f64>,
    /// Formation/cell ID to join
    pub formation_id: Option<String>,
    /// Capability advertisement interval in seconds
    pub advertise_interval_secs: u64,
}

impl BeaconConfig {
    /// Create a new beacon configuration
    pub fn new(platform_id: &str) -> Self {
        Self {
            platform_id: platform_id.to_string(),
            name: platform_id.to_string(),
            camera: None,
            model: None,
            compute: None,
            position: None,
            altitude_m: None,
            formation_id: None,
            advertise_interval_secs: 30,
        }
    }

    /// Auto-detect hardware configuration
    pub fn auto_detect(platform_id: &str) -> anyhow::Result<Self> {
        let mut config = Self::new(platform_id);

        // Detect Jetson platform
        if let Ok(jetson_info) = JetsonInfo::detect() {
            config.compute = Some(ComputeSpec::from_jetson(&jetson_info));
            config.name = format!("{} ({})", platform_id, jetson_info.model);
        }

        // Default to IMX219 camera (most common on Jetson dev kits)
        // In production, would probe v4l2 devices
        config.camera = Some(CameraSpec::imx219());

        // Default model
        config.model = Some(ModelSpec::yolov8n());

        Ok(config)
    }

    /// Set camera specification
    pub fn with_camera(mut self, camera: CameraSpec) -> Self {
        self.camera = Some(camera);
        self
    }

    /// Set AI model specification
    pub fn with_model(mut self, model: ModelSpec) -> Self {
        self.model = Some(model);
        self
    }

    /// Set compute platform specification
    pub fn with_compute(mut self, compute: ComputeSpec) -> Self {
        self.compute = Some(compute);
        self
    }

    /// Set geographic position
    pub fn with_position(mut self, lat: f64, lon: f64) -> Self {
        self.position = Some((lat, lon));
        self
    }

    /// Set altitude
    pub fn with_altitude(mut self, altitude_m: f64) -> Self {
        self.altitude_m = Some(altitude_m);
        self
    }

    /// Set formation to join
    pub fn with_formation(mut self, formation_id: &str) -> Self {
        self.formation_id = Some(formation_id.to_string());
        self
    }

    /// Set capability advertisement interval
    pub fn with_advertise_interval(mut self, secs: u64) -> Self {
        self.advertise_interval_secs = secs;
        self
    }

    /// Set human-readable name
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }
}

/// Runtime state of the beacon
#[derive(Debug, Clone)]
pub struct BeaconState {
    /// Current operational status
    pub status: OperationalStatus,
    /// Last capability advertisement time
    pub last_advertised: Option<DateTime<Utc>>,
    /// Current resource metrics
    pub resources: ResourceMetrics,
    /// Measured model performance (updated at runtime)
    pub measured_performance: Option<ModelPerformance>,
    /// Number of tracks published
    pub tracks_published: u64,
    /// Uptime in seconds
    pub uptime_secs: f64,
}

impl Default for BeaconState {
    fn default() -> Self {
        Self {
            status: OperationalStatus::Loading,
            last_advertised: None,
            resources: ResourceMetrics {
                gpu_utilization: None,
                memory_used_mb: None,
                memory_total_mb: None,
                cpu_utilization: None,
            },
            measured_performance: None,
            tracks_published: 0,
            uptime_secs: 0.0,
        }
    }
}

/// Peat Beacon - registers edge device with the Peat network
pub struct PeatBeacon {
    config: BeaconConfig,
    state: Arc<RwLock<BeaconState>>,
    start_time: std::time::Instant,
}

impl PeatBeacon {
    /// Create a new Peat beacon
    pub fn new(config: BeaconConfig) -> anyhow::Result<Self> {
        info!(
            "Creating Peat beacon: {} ({})",
            config.platform_id, config.name
        );

        if let Some(ref camera) = config.camera {
            info!(
                "  Camera: {} {} ({}x{} max)",
                camera.manufacturer, camera.model, camera.max_width, camera.max_height
            );
        }

        if let Some(ref model) = config.model {
            info!(
                "  Model: {} v{} ({})",
                model.name, model.version, model.framework
            );
        }

        if let Some(ref compute) = config.compute {
            info!(
                "  Compute: {} ({} CUDA cores, {} MB)",
                compute.model, compute.cuda_cores, compute.memory_mb
            );
        }

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(BeaconState::default())),
            start_time: std::time::Instant::now(),
        })
    }

    /// Get the platform ID
    pub fn platform_id(&self) -> &str {
        &self.config.platform_id
    }

    /// Get the beacon configuration
    pub fn config(&self) -> &BeaconConfig {
        &self.config
    }

    /// Get current state
    pub async fn state(&self) -> BeaconState {
        let mut state = self.state.read().await.clone();
        state.uptime_secs = self.start_time.elapsed().as_secs_f64();
        state
    }

    /// Set operational status
    pub async fn set_status(&self, status: OperationalStatus) {
        let mut state = self.state.write().await;
        state.status = status;
    }

    /// Update resource metrics
    pub async fn update_resources(&self, resources: ResourceMetrics) {
        let mut state = self.state.write().await;
        state.resources = resources;
    }

    /// Update measured model performance
    pub async fn update_performance(&self, performance: ModelPerformance) {
        let mut state = self.state.write().await;
        state.measured_performance = Some(performance);
    }

    /// Increment tracks published counter
    pub async fn record_track_published(&self) {
        let mut state = self.state.write().await;
        state.tracks_published += 1;
    }

    /// Generate a capability advertisement
    pub async fn generate_advertisement(&self) -> CapabilityAdvertisement {
        let state = self.state.read().await;

        let mut models = Vec::new();

        if let Some(ref model_spec) = self.config.model {
            // Use measured performance if available, otherwise expected
            let performance = state
                .measured_performance
                .clone()
                .unwrap_or_else(|| model_spec.expected_performance.clone());

            let mut model_cap = ModelCapability::new(
                &model_spec.model_id,
                &model_spec.version,
                &model_spec.hash,
                &model_spec.model_type,
                performance,
            )
            .with_status(state.status)
            .with_framework(&model_spec.framework, &model_spec.quantization)
            .with_size(model_spec.size_bytes);

            model_cap.input_signature = vec![format!(
                "image:{}x{}x{}",
                model_spec.input_size.0, model_spec.input_size.1, model_spec.input_size.2
            )];
            model_cap.output_signature = vec![
                "detections:bbox,class,confidence".to_string(),
                "tracks:id,bbox,velocity".to_string(),
            ];
            if !model_spec.class_labels.is_empty() {
                model_cap.class_labels = model_spec.class_labels.clone();
                model_cap.num_classes = Some(model_spec.num_classes);
            }

            models.push(model_cap);
        }

        CapabilityAdvertisement {
            platform_id: self.config.platform_id.clone(),
            advertised_at: Utc::now(),
            models,
            resources: Some(state.resources.clone()),
        }
    }

    /// Generate platform registration document
    pub async fn generate_registration(&self) -> serde_json::Value {
        let state = self.state.read().await;

        let mut registration = serde_json::json!({
            "platform_id": self.config.platform_id,
            "name": self.config.name,
            "type": "edge_sensor",
            "status": format!("{:?}", state.status),
            "registered_at": Utc::now().to_rfc3339(),
        });

        if let Some(ref camera) = self.config.camera {
            registration["camera"] = serde_json::json!({
                "model": camera.model,
                "manufacturer": camera.manufacturer,
                "interface": camera.interface,
                "max_resolution": format!("{}x{}", camera.max_width, camera.max_height),
                "modes": camera.modes.iter().map(|m| {
                    serde_json::json!({
                        "resolution": format!("{}x{}", m.width, m.height),
                        "fps": m.fps,
                        "format": m.format
                    })
                }).collect::<Vec<_>>(),
                "fov": {
                    "horizontal": camera.hfov_degrees,
                    "vertical": camera.vfov_degrees,
                    "diagonal": camera.dfov_degrees
                },
                "sensor_size_mm": camera.sensor_size_mm,
                "pixel_size_um": camera.pixel_size_um
            });
        }

        if let Some(ref model) = self.config.model {
            registration["model"] = serde_json::json!({
                "id": model.model_id,
                "name": model.name,
                "version": model.version,
                "type": model.model_type,
                "framework": model.framework,
                "quantization": model.quantization,
                "input_size": model.input_size,
                "num_classes": model.num_classes,
                "hash": model.hash,
                "size_bytes": model.size_bytes
            });
        }

        if let Some(ref compute) = self.config.compute {
            registration["compute"] = serde_json::json!({
                "model": compute.model,
                "manufacturer": compute.manufacturer,
                "gpu_arch": compute.gpu_arch,
                "cuda_cores": compute.cuda_cores,
                "tensor_cores": compute.tensor_cores,
                "dla_cores": compute.dla_cores,
                "memory_mb": compute.memory_mb,
                "cpu_cores": compute.cpu_cores,
                "sdk_version": compute.sdk_version,
                "cuda_version": compute.cuda_version,
                "tensorrt_version": compute.tensorrt_version
            });
        }

        if let Some((lat, lon)) = self.config.position {
            registration["position"] = serde_json::json!({
                "lat": lat,
                "lon": lon,
                "alt_m": self.config.altitude_m
            });
        }

        if let Some(ref formation_id) = self.config.formation_id {
            registration["formation_id"] = serde_json::Value::String(formation_id.clone());
        }

        registration
    }

    /// Generate a proto ModelDeployment from the configured model
    ///
    /// Returns None if no model is configured.
    pub fn to_proto_model_deployment(&self) -> Option<peat_schema::model::v1::ModelDeployment> {
        self.config.model.as_ref().map(|m| m.into())
    }

    /// Generate a proto SensorSpec from the configured camera
    ///
    /// Returns None if no camera is configured.
    pub fn to_proto_sensor_spec(&self) -> Option<peat_schema::sensor::v1::SensorSpec> {
        self.config.camera.as_ref().map(|c| c.into())
    }
}

/// COCO class labels (80 classes)
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
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imx219_spec() {
        let camera = CameraSpec::imx219();
        assert_eq!(camera.model, "IMX219");
        assert_eq!(camera.max_width, 3280);
        assert_eq!(camera.max_height, 2464);
        assert_eq!(camera.modes.len(), 5);

        // Check 1080p mode
        let mode = camera.best_mode_for(1920, 1080);
        assert!(mode.is_some());
        let mode = mode.unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.fps, 30.0);
    }

    #[test]
    fn test_yolov8n_spec() {
        let model = ModelSpec::yolov8n();
        assert_eq!(model.model_id, "yolov8n");
        assert_eq!(model.input_size, (640, 640, 3));
        assert_eq!(model.num_classes, 80);
        assert_eq!(model.class_labels.len(), 80);
    }

    #[test]
    fn test_beacon_config() {
        let config = BeaconConfig::new("test-beacon")
            .with_camera(CameraSpec::imx219())
            .with_model(ModelSpec::yolov8n())
            .with_position(33.7749, -84.3958)
            .with_formation("alpha-formation");

        assert_eq!(config.platform_id, "test-beacon");
        assert!(config.camera.is_some());
        assert!(config.model.is_some());
        assert_eq!(config.position, Some((33.7749, -84.3958)));
        assert_eq!(config.formation_id, Some("alpha-formation".to_string()));
    }

    #[tokio::test]
    async fn test_beacon_advertisement() {
        let config = BeaconConfig::new("test-beacon")
            .with_camera(CameraSpec::imx219())
            .with_model(ModelSpec::yolov8n());

        let beacon = PeatBeacon::new(config).unwrap();
        beacon.set_status(OperationalStatus::Ready).await;

        let advert = beacon.generate_advertisement().await;
        assert_eq!(advert.platform_id, "test-beacon");
        assert_eq!(advert.models.len(), 1);
        assert_eq!(advert.models[0].model_id, "yolov8n");
    }

    #[tokio::test]
    async fn test_beacon_registration() {
        let config = BeaconConfig::new("test-beacon")
            .with_name("Test Edge Device")
            .with_camera(CameraSpec::imx219())
            .with_model(ModelSpec::yolov8n())
            .with_compute(ComputeSpec::jetson_orin_nano())
            .with_position(33.7749, -84.3958);

        let beacon = PeatBeacon::new(config).unwrap();
        let registration = beacon.generate_registration().await;

        assert_eq!(registration["platform_id"], "test-beacon");
        assert_eq!(registration["name"], "Test Edge Device");
        assert!(registration["camera"].is_object());
        assert!(registration["model"].is_object());
        assert!(registration["compute"].is_object());
        assert_eq!(registration["position"]["lat"], 33.7749);
    }
}
