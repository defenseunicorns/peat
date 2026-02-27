//! Schema conversion utilities for peat-inference
//!
//! Provides `From` implementations to convert between native types
//! and peat-schema protobuf types.
//!
//! ## Model Types
//!
//! - `ModelSpec` ↔ `model::v1::ModelDeployment`
//! - `ModelPerformance` ↔ `model::v1::ModelPerformanceMetrics`
//!
//! ## Sensor Types
//!
//! - `CameraSpec` ↔ `sensor::v1::SensorSpec`
//! - `SensorCapability` ↔ `sensor::v1::SensorSpec`

use crate::beacon::{CameraSpec, ModelSpec};
use crate::messages::ModelPerformance;
use crate::platform::SensorCapability;
use peat_schema::model::v1::{
    AcceleratorType, HardwareRequirements, ModelDeployment, ModelMetadata, ModelPerformanceMetrics,
    ModelType,
};
use peat_schema::sensor::v1::{
    FieldOfView, SensorModality, SensorMountType, SensorOrientation, SensorSpec,
};

// ============================================================================
// Model Conversions
// ============================================================================

/// Convert ModelSpec to proto ModelDeployment
impl From<&ModelSpec> for ModelDeployment {
    fn from(spec: &ModelSpec) -> Self {
        let model_type = match spec.model_type.as_str() {
            "detector" => ModelType::Detector,
            "tracker" => ModelType::Tracker,
            "detector_tracker" => ModelType::DetectorTracker,
            "classifier" => ModelType::Classifier,
            "segmentation" => ModelType::Segmentation,
            "pose" => ModelType::Pose,
            _ => ModelType::Custom,
        };

        let accelerator = if spec.framework.contains("TensorRT") {
            AcceleratorType::Tensorrt
        } else if spec.framework.contains("CUDA") || spec.quantization == "FP16" {
            AcceleratorType::Cuda
        } else {
            AcceleratorType::Cpu
        };

        ModelDeployment {
            deployment_id: String::new(), // Set by caller
            model_id: spec.model_id.clone(),
            model_version: spec.version.clone(),
            model_type: model_type as i32,
            model_url: String::new(), // Set by caller for actual deployment
            checksum_sha256: spec.hash.clone(),
            file_size_bytes: spec.size_bytes,
            target_platforms: vec![],
            deployment_policy: 0, // Unspecified
            priority: 0,          // Unspecified
            deployed_at: None,
            deployed_by: String::new(),
            rollback_model_id: String::new(),
            metadata: Some(ModelMetadata {
                format: spec.framework.to_lowercase(),
                framework: spec.framework.clone(),
                input_dimensions: format!(
                    "{}x{}x{}",
                    spec.input_size.0, spec.input_size.1, spec.input_size.2
                ),
                classes: spec.class_labels.clone(),
                min_runtime_version: String::new(),
                hardware_requirements: Some(HardwareRequirements {
                    min_gpu_memory_mb: 2048, // Default 2GB
                    min_ram_mb: 4096,        // Default 4GB
                    min_storage_mb: (spec.size_bytes / 1_000_000) as u32 + 100,
                    accelerator_type: accelerator as i32,
                }),
                performance_metrics: Some(ModelPerformanceMetrics {
                    map: spec.expected_performance.precision as f32,
                    inference_time_ms: spec.expected_performance.latency_ms.unwrap_or(0.0) as f32,
                    fps: spec.expected_performance.fps as f32,
                    accuracy: spec.expected_performance.precision as f32,
                    benchmark_hardware: "Jetson Orin Nano".to_string(),
                }),
                extra_json: String::new(),
            }),
        }
    }
}

/// Convert proto ModelDeployment to native ModelSpec
impl From<&ModelDeployment> for ModelSpec {
    fn from(proto: &ModelDeployment) -> Self {
        let model_type = match ModelType::try_from(proto.model_type).unwrap_or(ModelType::Custom) {
            ModelType::Detector => "detector",
            ModelType::Tracker => "tracker",
            ModelType::DetectorTracker => "detector_tracker",
            ModelType::Classifier => "classifier",
            ModelType::Segmentation => "segmentation",
            ModelType::Pose => "pose",
            _ => "custom",
        };

        let (framework, quantization) = if let Some(ref meta) = proto.metadata {
            (
                meta.framework.clone(),
                if meta.format.contains("fp16") || meta.format.contains("FP16") {
                    "FP16".to_string()
                } else {
                    "FP32".to_string()
                },
            )
        } else {
            ("ONNX".to_string(), "FP32".to_string())
        };

        let input_size = if let Some(ref meta) = proto.metadata {
            parse_input_dimensions(&meta.input_dimensions)
        } else {
            (640, 640, 3)
        };

        let (classes, num_classes) = if let Some(ref meta) = proto.metadata {
            (meta.classes.clone(), meta.classes.len())
        } else {
            (vec![], 0)
        };

        let performance = if let Some(ref meta) = proto.metadata {
            if let Some(ref perf) = meta.performance_metrics {
                ModelPerformance {
                    precision: perf.map as f64,
                    recall: perf.accuracy as f64,
                    fps: perf.fps as f64,
                    latency_ms: Some(perf.inference_time_ms as f64),
                }
            } else {
                ModelPerformance::new(0.0, 0.0, 0.0)
            }
        } else {
            ModelPerformance::new(0.0, 0.0, 0.0)
        };

        ModelSpec {
            model_id: proto.model_id.clone(),
            name: proto.model_id.clone(), // Use model_id as name if not provided
            version: proto.model_version.clone(),
            model_type: model_type.to_string(),
            hash: proto.checksum_sha256.clone(),
            size_bytes: proto.file_size_bytes,
            input_size,
            num_classes,
            class_labels: classes,
            expected_performance: performance,
            quantization,
            framework,
        }
    }
}

/// Convert ModelPerformance to proto ModelPerformanceMetrics
impl From<&ModelPerformance> for ModelPerformanceMetrics {
    fn from(perf: &ModelPerformance) -> Self {
        ModelPerformanceMetrics {
            map: perf.precision as f32,
            inference_time_ms: perf.latency_ms.unwrap_or(0.0) as f32,
            fps: perf.fps as f32,
            accuracy: perf.precision as f32,
            benchmark_hardware: String::new(),
        }
    }
}

/// Convert proto ModelPerformanceMetrics to native ModelPerformance
impl From<&ModelPerformanceMetrics> for ModelPerformance {
    fn from(proto: &ModelPerformanceMetrics) -> Self {
        ModelPerformance {
            precision: proto.map as f64,
            recall: proto.accuracy as f64,
            fps: proto.fps as f64,
            latency_ms: if proto.inference_time_ms > 0.0 {
                Some(proto.inference_time_ms as f64)
            } else {
                None
            },
        }
    }
}

// ============================================================================
// Sensor Conversions
// ============================================================================

/// Convert CameraSpec to proto SensorSpec
impl From<&CameraSpec> for SensorSpec {
    fn from(camera: &CameraSpec) -> Self {
        SensorSpec {
            sensor_id: camera.model.to_lowercase().replace(' ', "-"),
            name: format!("{} {}", camera.manufacturer, camera.model),
            mount_type: SensorMountType::Fixed as i32, // Cameras are typically fixed
            base_orientation: Some(SensorOrientation {
                bearing_offset_deg: 0.0,   // Forward-facing
                elevation_offset_deg: 0.0, // Level
                roll_offset_deg: 0.0,
            }),
            field_of_view: Some(FieldOfView {
                horizontal_deg: camera.hfov_degrees as f32,
                vertical_deg: camera.vfov_degrees as f32,
                diagonal_deg: camera.dfov_degrees as f32,
                max_range_m: 0.0, // Unknown for cameras
            }),
            modality: SensorModality::Eo as i32, // Electro-Optical
            resolution_width: camera.max_width,
            resolution_height: camera.max_height,
            frame_rate_fps: camera
                .highest_fps_mode()
                .map(|m| m.fps as f32)
                .unwrap_or(30.0),
            gimbal_limits: None, // Fixed mount has no gimbal
            current_state: None,
            updated_at: None,
        }
    }
}

/// Convert proto SensorSpec to native CameraSpec (for EO sensors)
impl TryFrom<&SensorSpec> for CameraSpec {
    type Error = &'static str;

    fn try_from(proto: &SensorSpec) -> Result<Self, Self::Error> {
        // Only convert EO sensors
        let modality = SensorModality::try_from(proto.modality).unwrap_or(SensorModality::Eo);
        if modality != SensorModality::Eo {
            return Err("SensorSpec is not an EO sensor");
        }

        let (hfov, vfov, dfov) = if let Some(ref fov) = proto.field_of_view {
            (
                fov.horizontal_deg as f64,
                fov.vertical_deg as f64,
                fov.diagonal_deg as f64,
            )
        } else {
            (60.0, 45.0, 75.0) // Defaults
        };

        // Extract manufacturer and model from name
        let parts: Vec<&str> = proto.name.splitn(2, ' ').collect();
        let (manufacturer, model) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("Unknown".to_string(), proto.name.clone())
        };

        Ok(CameraSpec {
            model,
            manufacturer,
            sensor_type: "CMOS".to_string(),
            interface: "CSI-2".to_string(),
            max_width: proto.resolution_width,
            max_height: proto.resolution_height,
            modes: vec![crate::beacon::CameraMode {
                width: proto.resolution_width,
                height: proto.resolution_height,
                fps: proto.frame_rate_fps as f64,
                format: "RG10".to_string(),
            }],
            hfov_degrees: hfov,
            vfov_degrees: vfov,
            dfov_degrees: dfov,
            pixel_size_um: 1.5,         // Default
            sensor_size_mm: (6.0, 4.0), // Default
        })
    }
}

/// Convert SensorCapability to proto SensorSpec
impl From<&SensorCapability> for SensorSpec {
    fn from(sensor: &SensorCapability) -> Self {
        let modality = match sensor.sensor_type.to_lowercase().as_str() {
            "camera" | "eo" => SensorModality::Eo,
            "ir" | "thermal" | "flir" => SensorModality::Ir,
            "eo/ir" => SensorModality::Ir, // Dual-mode, default to IR
            "lidar" => SensorModality::Lidar,
            "radar" => SensorModality::Radar,
            _ => SensorModality::Eo,
        };

        let (width, height) = if let Some(ref res) = sensor.resolution {
            parse_resolution(res)
        } else {
            (1920, 1080)
        };

        SensorSpec {
            sensor_id: sensor.sensor_type.to_lowercase().replace(['/', ' '], "-"),
            name: sensor.sensor_type.clone(),
            mount_type: SensorMountType::Fixed as i32,
            base_orientation: Some(SensorOrientation {
                bearing_offset_deg: 0.0,
                elevation_offset_deg: 0.0,
                roll_offset_deg: 0.0,
            }),
            field_of_view: Some(FieldOfView {
                horizontal_deg: sensor.fov_degrees.unwrap_or(60.0) as f32,
                vertical_deg: (sensor.fov_degrees.unwrap_or(60.0) * 0.75) as f32, // Assume 4:3 aspect
                diagonal_deg: 0.0,
                max_range_m: sensor.range_m.unwrap_or(0.0) as f32,
            }),
            modality: modality as i32,
            resolution_width: width,
            resolution_height: height,
            frame_rate_fps: sensor.frame_rate.unwrap_or(30.0) as f32,
            gimbal_limits: None,
            current_state: None,
            updated_at: None,
        }
    }
}

/// Convert proto SensorSpec to native SensorCapability
impl From<&SensorSpec> for SensorCapability {
    fn from(proto: &SensorSpec) -> Self {
        let modality = SensorModality::try_from(proto.modality).unwrap_or(SensorModality::Eo);
        let sensor_type = match modality {
            SensorModality::Eo => "Camera",
            SensorModality::Ir | SensorModality::Mwir | SensorModality::Lwir => "EO/IR",
            SensorModality::Lidar => "LIDAR",
            SensorModality::Radar | SensorModality::Sar => "Radar",
            _ => "Camera",
        };

        let (fov, range) = if let Some(ref fov) = proto.field_of_view {
            (
                Some(fov.horizontal_deg as f64),
                if fov.max_range_m > 0.0 {
                    Some(fov.max_range_m as f64)
                } else {
                    None
                },
            )
        } else {
            (None, None)
        };

        SensorCapability {
            sensor_type: sensor_type.to_string(),
            resolution: Some(format!(
                "{}x{}",
                proto.resolution_width, proto.resolution_height
            )),
            fov_degrees: fov,
            range_m: range,
            frame_rate: Some(proto.frame_rate_fps as f64),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse input dimensions string (e.g., "640x640x3") to tuple
fn parse_input_dimensions(dims: &str) -> (u32, u32, u32) {
    let parts: Vec<&str> = dims.split('x').collect();
    if parts.len() >= 3 {
        let w = parts[0].parse().unwrap_or(640);
        let h = parts[1].parse().unwrap_or(640);
        let c = parts[2].parse().unwrap_or(3);
        (w, h, c)
    } else {
        (640, 640, 3)
    }
}

/// Parse resolution string (e.g., "1920x1080" or "1080p") to (width, height)
fn parse_resolution(res: &str) -> (u32, u32) {
    // Handle common formats
    match res.to_lowercase().as_str() {
        "4k" | "2160p" => (3840, 2160),
        "1080p" | "fhd" => (1920, 1080),
        "720p" | "hd" => (1280, 720),
        "480p" => (854, 480),
        _ => {
            // Try parsing "WxH" format
            let parts: Vec<&str> = res.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().unwrap_or(1920);
                let h = parts[1].parse().unwrap_or(1080);
                (w, h)
            } else {
                (1920, 1080)
            }
        }
    }
}

// ============================================================================
// Extension Traits
// ============================================================================

/// Extension trait for ModelSpec to add proto conversion methods
pub trait ModelSpecProtoExt {
    /// Convert to proto ModelDeployment for a specific deployment
    fn to_deployment(&self, deployment_id: &str, targets: Vec<String>) -> ModelDeployment;
}

impl ModelSpecProtoExt for ModelSpec {
    fn to_deployment(&self, deployment_id: &str, targets: Vec<String>) -> ModelDeployment {
        let mut deployment: ModelDeployment = self.into();
        deployment.deployment_id = deployment_id.to_string();
        deployment.target_platforms = targets;
        deployment
    }
}

/// Extension trait for SensorCapability to add proto conversion methods
pub trait SensorCapabilityProtoExt {
    /// Convert to proto SensorSpec with a specific sensor ID
    fn to_sensor_spec(&self, sensor_id: &str) -> SensorSpec;
}

impl SensorCapabilityProtoExt for SensorCapability {
    fn to_sensor_spec(&self, sensor_id: &str) -> SensorSpec {
        let mut spec: SensorSpec = self.into();
        spec.sensor_id = sensor_id.to_string();
        spec
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::CameraSpec;

    #[test]
    fn test_model_spec_to_proto() {
        let spec = ModelSpec::yolov8n();
        let proto: ModelDeployment = (&spec).into();

        assert_eq!(proto.model_id, "yolov8n");
        assert_eq!(proto.model_version, "8.0.0");
        assert_eq!(proto.model_type, ModelType::Detector as i32);
        assert!(proto.metadata.is_some());

        let meta = proto.metadata.unwrap();
        assert_eq!(meta.input_dimensions, "640x640x3");
        assert_eq!(meta.classes.len(), 80);
    }

    #[test]
    fn test_proto_to_model_spec() {
        let spec = ModelSpec::yolov8n();
        let proto: ModelDeployment = (&spec).into();
        let roundtrip: ModelSpec = (&proto).into();

        assert_eq!(roundtrip.model_id, spec.model_id);
        assert_eq!(roundtrip.version, spec.version);
        assert_eq!(roundtrip.model_type, spec.model_type);
        assert_eq!(roundtrip.num_classes, spec.num_classes);
    }

    #[test]
    fn test_camera_spec_to_proto() {
        let camera = CameraSpec::imx219();
        let proto: SensorSpec = (&camera).into();

        assert_eq!(proto.name, "Sony IMX219");
        assert_eq!(proto.resolution_width, 3280);
        assert_eq!(proto.resolution_height, 2464);
        assert_eq!(proto.modality, SensorModality::Eo as i32);

        let fov = proto.field_of_view.unwrap();
        assert!((fov.horizontal_deg - 62.2).abs() < 0.1);
    }

    #[test]
    fn test_proto_to_camera_spec() {
        let camera = CameraSpec::imx219();
        let proto: SensorSpec = (&camera).into();
        let roundtrip: CameraSpec = (&proto).try_into().unwrap();

        assert_eq!(roundtrip.model, "IMX219");
        assert_eq!(roundtrip.manufacturer, "Sony");
        assert_eq!(roundtrip.max_width, camera.max_width);
    }

    #[test]
    fn test_sensor_capability_to_proto() {
        let sensor = SensorCapability::eo_ir("1920x1080", 60.0, 5000.0);
        let proto: SensorSpec = (&sensor).into();

        assert_eq!(proto.resolution_width, 1920);
        assert_eq!(proto.resolution_height, 1080);
        assert_eq!(proto.modality, SensorModality::Ir as i32);

        let fov = proto.field_of_view.unwrap();
        assert_eq!(fov.horizontal_deg, 60.0);
        assert_eq!(fov.max_range_m, 5000.0);
    }

    #[test]
    fn test_proto_to_sensor_capability() {
        let sensor = SensorCapability::camera("1080p", 60.0, 30.0);
        let proto: SensorSpec = (&sensor).into();
        let roundtrip: SensorCapability = (&proto).into();

        assert_eq!(roundtrip.sensor_type, "Camera");
        assert_eq!(roundtrip.fov_degrees, Some(60.0));
        assert_eq!(roundtrip.frame_rate, Some(30.0));
    }

    #[test]
    fn test_parse_resolution() {
        assert_eq!(parse_resolution("4K"), (3840, 2160));
        assert_eq!(parse_resolution("1080p"), (1920, 1080));
        assert_eq!(parse_resolution("1920x1080"), (1920, 1080));
        assert_eq!(parse_resolution("640x480"), (640, 480));
    }

    #[test]
    fn test_model_spec_to_deployment() {
        let spec = ModelSpec::yolov8n();
        let deployment = spec.to_deployment("deploy-001", vec!["node-1".into(), "node-2".into()]);

        assert_eq!(deployment.deployment_id, "deploy-001");
        assert_eq!(deployment.target_platforms.len(), 2);
        assert_eq!(deployment.model_id, "yolov8n");
    }

    #[test]
    fn test_model_performance_roundtrip() {
        let perf = ModelPerformance {
            precision: 0.91,
            recall: 0.87,
            fps: 15.0,
            latency_ms: Some(67.0),
        };

        let proto: ModelPerformanceMetrics = (&perf).into();
        let roundtrip: ModelPerformance = (&proto).into();

        assert!((roundtrip.precision - perf.precision).abs() < 0.01);
        assert!((roundtrip.fps - perf.fps).abs() < 0.01);
        assert!(roundtrip.latency_ms.is_some());
    }
}
