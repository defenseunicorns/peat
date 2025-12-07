//! Platform module - Individual entities (operators, UGVs/UAVs, AI models)
//!
//! Platforms advertise capabilities upward and receive commands/model updates downward.
//! This module implements the M1 vignette platform types:
//! - **Operator**: Human with TAK device (Alpha-1, Bravo-1)
//! - **Vehicle**: UGV/UAV with sensors (Alpha-2, Bravo-2)
//! - **AiModel**: Object tracker running inference (Alpha-3, Bravo-3)

use crate::messages::{
    CapabilityAdvertisement, ModelCapability, ModelPerformance, OperationalStatus, ResourceMetrics,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hive_protocol::models::{Capability, CapabilityExt, CapabilityType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Core Traits
// ============================================================================

/// Trait for platforms that can advertise capabilities to the HIVE network
#[async_trait]
pub trait CapabilityProvider: Send + Sync {
    /// Get the platform's unique identifier
    fn id(&self) -> &str;

    /// Get the platform's human-readable name
    fn name(&self) -> &str;

    /// Get the platform type
    fn platform_type(&self) -> PlatformType;

    /// Get current operational status
    fn operational_status(&self) -> OperationalStatus;

    /// Set operational status
    fn set_operational_status(&mut self, status: OperationalStatus);

    /// Get capabilities as hive-protocol Capability objects
    fn get_capabilities(&self) -> Vec<Capability>;

    /// Generate a capability advertisement message for the HIVE network
    fn advertise_capabilities(&self) -> CapabilityAdvertisement;

    /// Update the platform's state (called periodically)
    async fn update(&mut self) -> anyhow::Result<()>;
}

// ============================================================================
// Platform Types
// ============================================================================

/// Platform types in the M1 vignette
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlatformType {
    /// Human operator with TAK device
    Operator,
    /// Unmanned Ground Vehicle
    Ugv,
    /// Unmanned Aerial Vehicle
    Uav,
    /// AI inference model
    AiModel,
}

impl std::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformType::Operator => write!(f, "Operator"),
            PlatformType::Ugv => write!(f, "UGV"),
            PlatformType::Uav => write!(f, "UAV"),
            PlatformType::AiModel => write!(f, "AI Model"),
        }
    }
}

// ============================================================================
// Authority Levels for Operators
// ============================================================================

/// Authority level for human operators
///
/// Determines decision-making weight in the human-machine team.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityLevel {
    /// Observer - can view but not command
    Observer = 0,
    /// Advisor - can suggest actions
    Advisor = 1,
    /// Supervisor - can approve/deny AI recommendations
    Supervisor = 2,
    /// Commander - full authority over the team
    Commander = 3,
}

impl AuthorityLevel {
    /// Get the authority weight as a float (0.0 - 1.0)
    pub fn weight(&self) -> f32 {
        match self {
            AuthorityLevel::Observer => 0.0,
            AuthorityLevel::Advisor => 0.3,
            AuthorityLevel::Supervisor => 0.7,
            AuthorityLevel::Commander => 1.0,
        }
    }
}

// ============================================================================
// Operator Platform
// ============================================================================

/// Human operator platform with TAK device
///
/// Represents a human operator (e.g., Alpha-1, Bravo-1) with authority
/// to supervise and command the human-machine team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorPlatform {
    /// Unique identifier
    pub id: String,
    /// Human-readable name (e.g., "Alpha-1")
    pub name: String,
    /// Callsign for comms
    pub callsign: String,
    /// Authority level in the team
    pub authority: AuthorityLevel,
    /// TAK device type (e.g., "ATAK", "WinTAK")
    pub tak_device: String,
    /// Current operational status
    pub status: OperationalStatus,
    /// Last heartbeat timestamp
    pub last_heartbeat: DateTime<Utc>,
    /// Current position (lat, lon)
    pub position: Option<(f64, f64)>,
}

impl OperatorPlatform {
    /// Create a new operator platform
    pub fn new(
        name: impl Into<String>,
        callsign: impl Into<String>,
        authority: AuthorityLevel,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            callsign: callsign.into(),
            authority,
            tak_device: "ATAK".to_string(),
            status: OperationalStatus::Ready,
            last_heartbeat: Utc::now(),
            position: None,
        }
    }

    /// Set the TAK device type
    pub fn with_tak_device(mut self, device: impl Into<String>) -> Self {
        self.tak_device = device.into();
        self
    }

    /// Set initial position
    pub fn with_position(mut self, lat: f64, lon: f64) -> Self {
        self.position = Some((lat, lon));
        self
    }

    /// Update heartbeat timestamp
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
    }
}

#[async_trait]
impl CapabilityProvider for OperatorPlatform {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Operator
    }

    fn operational_status(&self) -> OperationalStatus {
        self.status
    }

    fn set_operational_status(&mut self, status: OperationalStatus) {
        self.status = status;
    }

    fn get_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        // Human-in-the-loop capability
        let mut hitl_cap = Capability::new(
            format!("{}-hitl", self.id),
            "Human-in-the-Loop".to_string(),
            CapabilityType::Compute, // Using Compute for decision-making
            self.authority.weight(),
        );
        hitl_cap.metadata_json = serde_json::json!({
            "authority_level": format!("{:?}", self.authority),
            "tak_device": self.tak_device,
            "type": "human_operator"
        })
        .to_string();
        caps.push(hitl_cap);

        // Communication capability (TAK)
        let comm_cap = Capability::new(
            format!("{}-comm", self.id),
            "TAK Communication".to_string(),
            CapabilityType::Communication,
            0.95, // High confidence for established comms
        );
        caps.push(comm_cap);

        caps
    }

    fn advertise_capabilities(&self) -> CapabilityAdvertisement {
        CapabilityAdvertisement::new(
            &self.name,
            vec![], // Operators don't have AI models
        )
    }

    async fn update(&mut self) -> anyhow::Result<()> {
        self.heartbeat();
        Ok(())
    }
}

// ============================================================================
// Vehicle Platform
// ============================================================================

/// Sensor capability for vehicles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorCapability {
    /// Sensor type (e.g., "EO/IR", "Camera", "LIDAR")
    pub sensor_type: String,
    /// Resolution (e.g., "1920x1080")
    pub resolution: Option<String>,
    /// Field of view in degrees
    pub fov_degrees: Option<f64>,
    /// Range in meters
    pub range_m: Option<f64>,
    /// Frame rate (FPS)
    pub frame_rate: Option<f64>,
}

impl SensorCapability {
    /// Create a new camera sensor
    pub fn camera(resolution: impl Into<String>, fov: f64, fps: f64) -> Self {
        Self {
            sensor_type: "Camera".to_string(),
            resolution: Some(resolution.into()),
            fov_degrees: Some(fov),
            range_m: None,
            frame_rate: Some(fps),
        }
    }

    /// Create a new EO/IR sensor
    pub fn eo_ir(resolution: impl Into<String>, fov: f64, range_m: f64) -> Self {
        Self {
            sensor_type: "EO/IR".to_string(),
            resolution: Some(resolution.into()),
            fov_degrees: Some(fov),
            range_m: Some(range_m),
            frame_rate: Some(30.0),
        }
    }
}

/// Vehicle platform (UGV/UAV) with sensors
///
/// Represents a robotic platform (e.g., Alpha-2 UGV, Bravo-2 UAV) that
/// carries sensors and provides data to AI models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehiclePlatform {
    /// Unique identifier
    pub id: String,
    /// Human-readable name (e.g., "Alpha-2")
    pub name: String,
    /// Vehicle type (UGV or UAV)
    pub vehicle_type: PlatformType,
    /// Equipped sensors
    pub sensors: Vec<SensorCapability>,
    /// Current operational status
    pub status: OperationalStatus,
    /// Current position (lat, lon, alt)
    pub position: Option<(f64, f64, f64)>,
    /// Battery level (0.0 - 1.0)
    pub battery_level: f64,
    /// Maximum speed in m/s
    pub max_speed_mps: f64,
    /// Communication range in meters
    pub comm_range_m: f64,
}

impl VehiclePlatform {
    /// Create a new UGV platform
    pub fn new_ugv(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            vehicle_type: PlatformType::Ugv,
            sensors: Vec::new(),
            status: OperationalStatus::Ready,
            position: None,
            battery_level: 1.0,
            max_speed_mps: 5.0, // 5 m/s typical for UGV
            comm_range_m: 2000.0,
        }
    }

    /// Create a new UAV platform
    pub fn new_uav(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            vehicle_type: PlatformType::Uav,
            sensors: Vec::new(),
            status: OperationalStatus::Ready,
            position: None,
            battery_level: 1.0,
            max_speed_mps: 15.0, // 15 m/s typical for small UAV
            comm_range_m: 5000.0,
        }
    }

    /// Add a sensor to the vehicle
    pub fn with_sensor(mut self, sensor: SensorCapability) -> Self {
        self.sensors.push(sensor);
        self
    }

    /// Set initial position
    pub fn with_position(mut self, lat: f64, lon: f64, alt: f64) -> Self {
        self.position = Some((lat, lon, alt));
        self
    }

    /// Update battery level
    pub fn set_battery(&mut self, level: f64) {
        self.battery_level = level.clamp(0.0, 1.0);
        if self.battery_level < 0.1 {
            self.status = OperationalStatus::Degraded;
        }
    }
}

#[async_trait]
impl CapabilityProvider for VehiclePlatform {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn platform_type(&self) -> PlatformType {
        self.vehicle_type
    }

    fn operational_status(&self) -> OperationalStatus {
        self.status
    }

    fn set_operational_status(&mut self, status: OperationalStatus) {
        self.status = status;
    }

    fn get_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        // Mobility capability
        let mut mobility_cap = Capability::new(
            format!("{}-mobility", self.id),
            format!("{} Mobility", self.vehicle_type),
            CapabilityType::Mobility,
            self.battery_level as f32 * 0.95, // Confidence based on battery
        );
        mobility_cap.metadata_json = serde_json::json!({
            "max_speed_mps": self.max_speed_mps,
            "vehicle_type": format!("{:?}", self.vehicle_type),
            "battery_level": self.battery_level
        })
        .to_string();
        caps.push(mobility_cap);

        // Sensor capabilities
        for (i, sensor) in self.sensors.iter().enumerate() {
            let mut sensor_cap = Capability::new(
                format!("{}-sensor-{}", self.id, i),
                format!("{} {}", sensor.sensor_type, self.name),
                CapabilityType::Sensor,
                0.9, // High confidence for operational sensors
            );
            sensor_cap.metadata_json = serde_json::to_string(sensor).unwrap_or_default();
            caps.push(sensor_cap);
        }

        // Communication capability
        let mut comm_cap = Capability::new(
            format!("{}-comm", self.id),
            "Mesh Communication".to_string(),
            CapabilityType::Communication,
            0.85,
        );
        comm_cap.metadata_json = serde_json::json!({
            "range_m": self.comm_range_m,
            "type": "mesh"
        })
        .to_string();
        caps.push(comm_cap);

        caps
    }

    fn advertise_capabilities(&self) -> CapabilityAdvertisement {
        CapabilityAdvertisement::new(
            &self.name,
            vec![], // Vehicle doesn't have AI models (those are on AiModelPlatform)
        )
        .with_resources(ResourceMetrics {
            gpu_utilization: None,
            memory_used_mb: None,
            memory_total_mb: None,
            cpu_utilization: Some(0.3),
        })
    }

    async fn update(&mut self) -> anyhow::Result<()> {
        // Simulate battery drain
        self.battery_level = (self.battery_level - 0.001).max(0.0);
        if self.battery_level < 0.1 {
            self.status = OperationalStatus::Degraded;
        }
        Ok(())
    }
}

// ============================================================================
// AI Model Platform
// ============================================================================

/// AI model metadata for inference platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelInfo {
    /// Model identifier (e.g., "object_tracker")
    pub model_id: String,
    /// Model version (semver)
    pub version: String,
    /// Model type (e.g., "detector_tracker", "classifier")
    pub model_type: String,
    /// SHA256 hash of model file
    pub model_hash: String,
    /// Performance metrics
    pub precision: f64,
    pub recall: f64,
    pub fps: f64,
    /// Inference latency in ms
    pub latency_ms: Option<f64>,
}

impl AiModelInfo {
    /// Create a YOLOv8 + DeepSORT object tracker model info
    pub fn object_tracker(version: impl Into<String>) -> Self {
        Self {
            model_id: "object_tracker".to_string(),
            version: version.into(),
            model_type: "detector_tracker".to_string(),
            model_hash: "sha256:pending".to_string(),
            precision: 0.91,
            recall: 0.87,
            fps: 15.0,
            latency_ms: Some(67.0),
        }
    }

    /// Update the model hash after loading
    pub fn with_hash(mut self, hash: impl Into<String>) -> Self {
        self.model_hash = hash.into();
        self
    }

    /// Update performance metrics
    pub fn with_performance(mut self, precision: f64, recall: f64, fps: f64) -> Self {
        self.precision = precision;
        self.recall = recall;
        self.fps = fps;
        self
    }
}

/// AI model platform for inference
///
/// Represents an AI model running on edge compute (e.g., Alpha-3, Bravo-3)
/// that processes sensor data and produces track updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelPlatform {
    /// Unique identifier
    pub id: String,
    /// Human-readable name (e.g., "Alpha-3")
    pub name: String,
    /// AI model information
    pub model: AiModelInfo,
    /// Current operational status
    pub status: OperationalStatus,
    /// GPU utilization (0.0 - 1.0)
    pub gpu_utilization: f64,
    /// Memory used in MB
    pub memory_used_mb: u64,
    /// Total memory in MB
    pub memory_total_mb: u64,
    /// Associated sensor platform ID (data source)
    pub sensor_source: Option<String>,
    /// Inference count since startup
    pub inference_count: u64,
}

impl AiModelPlatform {
    /// Create a new AI model platform
    pub fn new(name: impl Into<String>, model: AiModelInfo) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            model,
            status: OperationalStatus::Loading,
            gpu_utilization: 0.0,
            memory_used_mb: 0,
            memory_total_mb: 4096, // 4GB default
            sensor_source: None,
            inference_count: 0,
        }
    }

    /// Set the sensor source platform
    pub fn with_sensor_source(mut self, sensor_id: impl Into<String>) -> Self {
        self.sensor_source = Some(sensor_id.into());
        self
    }

    /// Set total GPU memory
    pub fn with_memory(mut self, total_mb: u64) -> Self {
        self.memory_total_mb = total_mb;
        self
    }

    /// Mark model as ready after loading
    pub fn mark_ready(&mut self, memory_used_mb: u64) {
        self.status = OperationalStatus::Ready;
        self.memory_used_mb = memory_used_mb;
    }

    /// Update resource utilization
    pub fn update_utilization(&mut self, gpu: f64, memory_mb: u64) {
        self.gpu_utilization = gpu.clamp(0.0, 1.0);
        self.memory_used_mb = memory_mb.min(self.memory_total_mb);
    }

    /// Record an inference
    pub fn record_inference(&mut self) {
        self.inference_count += 1;
        self.status = OperationalStatus::Active;
    }

    /// Update model version (for model updates)
    pub fn update_model(&mut self, new_model: AiModelInfo) {
        self.model = new_model;
        self.status = OperationalStatus::Loading;
        self.inference_count = 0;
    }

    /// Convert to ModelCapability for registry/query operations
    ///
    /// Creates a ModelCapability from this platform's current state.
    pub fn to_model_capability(&self) -> crate::messages::ModelCapability {
        use crate::messages::{ModelCapability, ModelPerformance};

        let performance =
            ModelPerformance::new(self.model.precision, self.model.recall, self.model.fps);
        let performance = if let Some(latency) = self.model.latency_ms {
            performance.with_latency(latency)
        } else {
            performance
        };

        let mut cap = ModelCapability::new(
            &self.model.model_id,
            &self.model.version,
            &self.model.model_hash,
            &self.model.model_type,
            performance,
        )
        .with_status(self.status);

        // Set runtime metrics
        cap.inference_count = Some(self.inference_count);

        cap
    }
}

#[async_trait]
impl CapabilityProvider for AiModelPlatform {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::AiModel
    }

    fn operational_status(&self) -> OperationalStatus {
        self.status
    }

    fn set_operational_status(&mut self, status: OperationalStatus) {
        self.status = status;
    }

    fn get_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        // Compute capability (AI inference)
        let mut compute_cap = Capability::new(
            format!("{}-compute", self.id),
            format!("{} Inference", self.model.model_id),
            CapabilityType::Compute,
            self.model.precision as f32, // Use precision as confidence
        );
        compute_cap.metadata_json = serde_json::json!({
            "model_id": self.model.model_id,
            "model_version": self.model.version,
            "model_type": self.model.model_type,
            "fps": self.model.fps,
            "latency_ms": self.model.latency_ms
        })
        .to_string();
        caps.push(compute_cap);

        caps
    }

    fn advertise_capabilities(&self) -> CapabilityAdvertisement {
        let model_cap = ModelCapability::new(
            &self.model.model_id,
            &self.model.version,
            &self.model.model_hash,
            &self.model.model_type,
            ModelPerformance::new(self.model.precision, self.model.recall, self.model.fps)
                .with_latency(self.model.latency_ms.unwrap_or(0.0)),
        )
        .with_status(self.status);

        CapabilityAdvertisement::new(&self.name, vec![model_cap]).with_resources(ResourceMetrics {
            gpu_utilization: Some(self.gpu_utilization),
            memory_used_mb: Some(self.memory_used_mb),
            memory_total_mb: Some(self.memory_total_mb),
            cpu_utilization: None,
        })
    }

    async fn update(&mut self) -> anyhow::Result<()> {
        // Simulate utilization changes
        if self.status == OperationalStatus::Active {
            self.gpu_utilization = (self.gpu_utilization + 0.1).min(0.95);
        } else {
            self.gpu_utilization = (self.gpu_utilization - 0.05).max(0.0);
        }
        Ok(())
    }
}

// ============================================================================
// Platform Container (for dynamic dispatch)
// ============================================================================

/// Container for any platform type (for collections)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Platform {
    Operator(OperatorPlatform),
    Vehicle(VehiclePlatform),
    AiModel(AiModelPlatform),
}

impl Platform {
    /// Get the platform ID
    pub fn id(&self) -> &str {
        match self {
            Platform::Operator(p) => &p.id,
            Platform::Vehicle(p) => &p.id,
            Platform::AiModel(p) => &p.id,
        }
    }

    /// Get the platform name
    pub fn name(&self) -> &str {
        match self {
            Platform::Operator(p) => &p.name,
            Platform::Vehicle(p) => &p.name,
            Platform::AiModel(p) => &p.name,
        }
    }

    /// Get the platform type
    pub fn platform_type(&self) -> PlatformType {
        match self {
            Platform::Operator(_) => PlatformType::Operator,
            Platform::Vehicle(p) => p.vehicle_type,
            Platform::AiModel(_) => PlatformType::AiModel,
        }
    }

    /// Get capabilities from the platform
    pub fn get_capabilities(&self) -> Vec<Capability> {
        match self {
            Platform::Operator(p) => p.get_capabilities(),
            Platform::Vehicle(p) => p.get_capabilities(),
            Platform::AiModel(p) => p.get_capabilities(),
        }
    }

    /// Advertise capabilities
    pub fn advertise_capabilities(&self) -> CapabilityAdvertisement {
        match self {
            Platform::Operator(p) => p.advertise_capabilities(),
            Platform::Vehicle(p) => p.advertise_capabilities(),
            Platform::AiModel(p) => p.advertise_capabilities(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_platform_creation() {
        let operator = OperatorPlatform::new("Alpha-1", "ALPHA-1", AuthorityLevel::Commander)
            .with_tak_device("ATAK")
            .with_position(33.7749, -84.3958);

        assert_eq!(operator.name, "Alpha-1");
        assert_eq!(operator.callsign, "ALPHA-1");
        assert_eq!(operator.authority, AuthorityLevel::Commander);
        assert_eq!(operator.authority.weight(), 1.0);
        assert!(operator.position.is_some());
    }

    #[test]
    fn test_operator_capabilities() {
        let operator = OperatorPlatform::new("Alpha-1", "ALPHA-1", AuthorityLevel::Commander);
        let caps = operator.get_capabilities();

        assert_eq!(caps.len(), 2); // HITL + Communication
        assert!(caps.iter().any(|c| c.name.contains("Human-in-the-Loop")));
        assert!(caps.iter().any(|c| c.name.contains("TAK Communication")));
    }

    #[test]
    fn test_vehicle_platform_ugv() {
        let ugv = VehiclePlatform::new_ugv("Alpha-2")
            .with_sensor(SensorCapability::camera("1920x1080", 60.0, 30.0))
            .with_position(33.7749, -84.3958, 0.0);

        assert_eq!(ugv.name, "Alpha-2");
        assert_eq!(ugv.vehicle_type, PlatformType::Ugv);
        assert_eq!(ugv.sensors.len(), 1);
        assert!(ugv.position.is_some());
    }

    #[test]
    fn test_vehicle_platform_uav() {
        let uav = VehiclePlatform::new_uav("Bravo-2")
            .with_sensor(SensorCapability::eo_ir("4K", 45.0, 5000.0))
            .with_position(33.7749, -84.3958, 100.0);

        assert_eq!(uav.name, "Bravo-2");
        assert_eq!(uav.vehicle_type, PlatformType::Uav);
        assert_eq!(uav.max_speed_mps, 15.0);
    }

    #[test]
    fn test_vehicle_capabilities() {
        let ugv = VehiclePlatform::new_ugv("Alpha-2")
            .with_sensor(SensorCapability::camera("1080p", 60.0, 30.0));

        let caps = ugv.get_capabilities();

        // Mobility + Sensor + Communication
        assert_eq!(caps.len(), 3);
        assert!(caps.iter().any(|c| c.name.contains("Mobility")));
        assert!(caps.iter().any(|c| c.name.contains("Camera")));
        assert!(caps.iter().any(|c| c.name.contains("Communication")));
    }

    #[test]
    fn test_vehicle_battery_degradation() {
        let mut ugv = VehiclePlatform::new_ugv("Alpha-2");
        assert_eq!(ugv.status, OperationalStatus::Ready);

        ugv.set_battery(0.05);
        assert_eq!(ugv.status, OperationalStatus::Degraded);
    }

    #[test]
    fn test_ai_model_platform_creation() {
        let model_info = AiModelInfo::object_tracker("1.3.0")
            .with_hash("sha256:abc123")
            .with_performance(0.94, 0.89, 20.0);

        let ai = AiModelPlatform::new("Alpha-3", model_info)
            .with_sensor_source("Alpha-2")
            .with_memory(8192);

        assert_eq!(ai.name, "Alpha-3");
        assert_eq!(ai.model.model_id, "object_tracker");
        assert_eq!(ai.model.version, "1.3.0");
        assert_eq!(ai.model.precision, 0.94);
        assert_eq!(ai.sensor_source, Some("Alpha-2".to_string()));
        assert_eq!(ai.memory_total_mb, 8192);
    }

    #[test]
    fn test_ai_model_capabilities() {
        let model_info = AiModelInfo::object_tracker("1.3.0");
        let ai = AiModelPlatform::new("Alpha-3", model_info);

        let caps = ai.get_capabilities();
        assert_eq!(caps.len(), 1); // Compute capability
        assert!(caps[0].name.contains("Inference"));
    }

    #[test]
    fn test_ai_model_capability_advertisement() {
        let model_info = AiModelInfo::object_tracker("1.3.0");
        let mut ai = AiModelPlatform::new("Alpha-3", model_info);
        ai.mark_ready(2048);
        ai.update_utilization(0.65, 2048);

        let advert = ai.advertise_capabilities();

        assert_eq!(advert.platform_id, "Alpha-3");
        assert_eq!(advert.models.len(), 1);
        assert_eq!(advert.models[0].model_id, "object_tracker");
        assert_eq!(advert.models[0].model_version, "1.3.0");
        assert!(advert.resources.is_some());

        let resources = advert.resources.unwrap();
        assert_eq!(resources.gpu_utilization, Some(0.65));
        assert_eq!(resources.memory_used_mb, Some(2048));
    }

    #[test]
    fn test_ai_model_update() {
        let old_model = AiModelInfo::object_tracker("1.2.0");
        let mut ai = AiModelPlatform::new("Alpha-3", old_model);
        ai.mark_ready(2048);
        ai.record_inference();
        assert!(ai.inference_count > 0);

        let new_model = AiModelInfo::object_tracker("1.3.0").with_performance(0.95, 0.91, 25.0);

        ai.update_model(new_model);

        assert_eq!(ai.model.version, "1.3.0");
        assert_eq!(ai.status, OperationalStatus::Loading);
        assert_eq!(ai.inference_count, 0);
    }

    #[test]
    fn test_platform_enum_dispatch() {
        let op = Platform::Operator(OperatorPlatform::new(
            "Alpha-1",
            "A1",
            AuthorityLevel::Commander,
        ));
        let ugv = Platform::Vehicle(VehiclePlatform::new_ugv("Alpha-2"));
        let ai = Platform::AiModel(AiModelPlatform::new(
            "Alpha-3",
            AiModelInfo::object_tracker("1.0.0"),
        ));

        assert_eq!(op.platform_type(), PlatformType::Operator);
        assert_eq!(ugv.platform_type(), PlatformType::Ugv);
        assert_eq!(ai.platform_type(), PlatformType::AiModel);

        // All can get capabilities
        assert!(!op.get_capabilities().is_empty());
        assert!(!ugv.get_capabilities().is_empty());
        assert!(!ai.get_capabilities().is_empty());
    }

    #[test]
    fn test_authority_level_ordering() {
        assert!(AuthorityLevel::Commander > AuthorityLevel::Supervisor);
        assert!(AuthorityLevel::Supervisor > AuthorityLevel::Advisor);
        assert!(AuthorityLevel::Advisor > AuthorityLevel::Observer);
    }

    #[test]
    fn test_platform_json_roundtrip() {
        let platform = Platform::AiModel(AiModelPlatform::new(
            "Alpha-3",
            AiModelInfo::object_tracker("1.3.0"),
        ));

        let json = serde_json::to_string(&platform).unwrap();
        let parsed: Platform = serde_json::from_str(&json).unwrap();

        assert_eq!(platform.name(), parsed.name());
        assert_eq!(platform.platform_type(), parsed.platform_type());
    }

    #[tokio::test]
    async fn test_platform_update() {
        let mut ai = AiModelPlatform::new("Alpha-3", AiModelInfo::object_tracker("1.0.0"));
        ai.status = OperationalStatus::Active;
        ai.gpu_utilization = 0.5;

        ai.update().await.unwrap();

        // GPU utilization should increase when active
        assert!(ai.gpu_utilization > 0.5);
    }
}
