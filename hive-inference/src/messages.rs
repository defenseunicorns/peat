//! Message types for M1 vignette
//!
//! Defines the core data structures for the M1 object tracking vignette.
//! These messages flow through the HIVE network hierarchy.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Track Update Messages (Upward Flow)
// ============================================================================

/// Track update message - flows upward from AI platforms to C2
///
/// Contains POI position, confidence, velocity, and attributes.
/// This is the primary output of edge AI processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackUpdate {
    /// Unique track identifier (e.g., "TRACK-001")
    pub track_id: String,
    /// Classification of the tracked object
    pub classification: String,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f64,
    /// Geographic position of the track
    pub position: Position,
    /// Velocity vector (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<Velocity>,
    /// Additional attributes (e.g., jacket_color, has_backpack)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, serde_json::Value>,
    /// Source platform ID (sensor platform)
    pub source_platform: String,
    /// Source AI model ID
    pub source_model: String,
    /// Version of the model that produced this track
    pub model_version: String,
    /// Timestamp of the observation
    pub timestamp: DateTime<Utc>,
}

/// Geographic position with circular error probable
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Position {
    /// Latitude in degrees
    pub lat: f64,
    /// Longitude in degrees
    pub lon: f64,
    /// Circular Error Probable in meters (position accuracy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cep_m: Option<f64>,
    /// Height above ellipsoid in meters (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hae: Option<f64>,
}

/// Velocity vector using bearing and speed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Velocity {
    /// Bearing/heading in degrees (0-360, 0 = North)
    pub bearing: f64,
    /// Speed in meters per second
    pub speed_mps: f64,
}

// ============================================================================
// Capability Advertisement Messages (Upward Flow)
// ============================================================================

/// Capability advertisement message - flows upward from platforms
///
/// Platforms advertise their AI model capabilities, performance metrics,
/// and resource utilization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityAdvertisement {
    /// Platform identifier
    pub platform_id: String,
    /// Timestamp of advertisement
    pub advertised_at: DateTime<Utc>,
    /// List of AI model capabilities
    pub models: Vec<ModelCapability>,
    /// Resource utilization metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceMetrics>,
}

/// AI model capability information
///
/// Comprehensive model metadata for capability advertisement and matching.
/// Includes identification, performance metrics, resource requirements,
/// and operational status per Issue #107 EPIC 4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCapability {
    /// Model identifier (e.g., "object_tracker")
    pub model_id: String,
    /// Model version (semver, e.g., "1.3.0")
    pub model_version: String,
    /// SHA256 hash of the model file for verification
    pub model_hash: String,
    /// Type of model (e.g., "detector", "tracker", "detector_tracker", "classifier")
    pub model_type: String,
    /// Performance metrics (precision, recall, fps, latency)
    pub performance: ModelPerformance,
    /// Current operational status
    pub operational_status: OperationalStatus,
    /// Resource requirements for running this model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_requirements: Option<ResourceRequirements>,
    /// Input data signature (e.g., "image:640x640x3")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_signature: Vec<String>,
    /// Output data signature (e.g., "detections:bbox,class,confidence")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_signature: Vec<String>,
    /// Model file size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_size_bytes: Option<u64>,
    /// Framework used (e.g., "ONNX", "TensorRT", "PyTorch")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Quantization type (e.g., "FP32", "FP16", "INT8")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Class labels for classification/detection models
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_labels: Vec<String>,
    /// Number of classes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_classes: Option<usize>,
    /// When the model was loaded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_at: Option<DateTime<Utc>>,
    /// Total inference count since load
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_count: Option<u64>,
    /// Last inference timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_inference_at: Option<DateTime<Utc>>,
    /// Performance degradation detected (vs baseline)
    #[serde(default)]
    pub degraded: bool,
    /// Degradation reason if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<String>,
}

/// Model performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPerformance {
    /// Precision (0.0 - 1.0)
    pub precision: f64,
    /// Recall (0.0 - 1.0)
    pub recall: f64,
    /// Frames per second throughput
    pub fps: f64,
    /// Inference latency in milliseconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
}

/// Operational status of a model or platform
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperationalStatus {
    /// Ready to process
    Ready,
    /// Currently processing
    Active,
    /// Temporarily unavailable (performance degraded)
    Degraded,
    /// Not available
    Offline,
    /// Loading or initializing
    Loading,
    /// Failed - requires intervention
    Failed,
    /// Updating - model update in progress
    Updating,
    /// Unloaded - model removed from memory
    Unloaded,
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceMetrics {
    /// GPU utilization (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization: Option<f64>,
    /// Memory used in megabytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_used_mb: Option<u64>,
    /// Total memory in megabytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_total_mb: Option<u64>,
    /// CPU utilization (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_utilization: Option<f64>,
}

/// Resource requirements for running a model
///
/// Specifies the minimum and recommended resources needed to run an AI model.
/// Used for capability matching when tasking platforms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceRequirements {
    /// Minimum GPU memory required in MB
    pub gpu_memory_mb: u64,
    /// Minimum system memory required in MB
    pub system_memory_mb: u64,
    /// Minimum CUDA compute capability (e.g., 7.5, 8.6)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cuda_compute_capability: Option<f32>,
    /// Whether TensorRT is required
    #[serde(default)]
    pub requires_tensorrt: bool,
    /// Whether DLA (Deep Learning Accelerator) is supported
    #[serde(default)]
    pub supports_dla: bool,
    /// Minimum CPU cores recommended
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<usize>,
    /// Supported execution providers (e.g., "CUDA", "TensorRT", "CPU")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_providers: Vec<String>,
}

impl ResourceRequirements {
    /// Create requirements for a typical edge AI model
    pub fn edge_default() -> Self {
        Self {
            gpu_memory_mb: 512,
            system_memory_mb: 1024,
            cuda_compute_capability: Some(5.3), // Jetson TX1 minimum
            requires_tensorrt: false,
            supports_dla: false,
            cpu_cores: Some(2),
            execution_providers: vec!["CUDA".to_string(), "CPU".to_string()],
        }
    }

    /// Create requirements for Jetson Orin Nano
    pub fn jetson_orin_nano() -> Self {
        Self {
            gpu_memory_mb: 1024,
            system_memory_mb: 2048,
            cuda_compute_capability: Some(8.7),
            requires_tensorrt: true,
            supports_dla: false, // Orin Nano doesn't have DLA
            cpu_cores: Some(4),
            execution_providers: vec![
                "TensorRT".to_string(),
                "CUDA".to_string(),
                "CPU".to_string(),
            ],
        }
    }

    /// Check if this platform meets the resource requirements
    pub fn is_satisfied_by(&self, metrics: &ResourceMetrics) -> bool {
        if let Some(total_mb) = metrics.memory_total_mb {
            if total_mb < self.gpu_memory_mb {
                return false;
            }
        }
        true
    }
}

// ============================================================================
// Model Update Messages (Downward Flow)
// ============================================================================

/// Model update package message - flows downward from C2/MLOps
///
/// Contains metadata for AI model distribution including
/// blob reference for content-addressed storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelUpdatePackage {
    /// Package type identifier
    pub package_type: PackageType,
    /// Model identifier
    pub model_id: String,
    /// New model version
    pub model_version: String,
    /// SHA256 hash of the model file
    pub model_hash: String,
    /// Size of the model in bytes
    pub model_size_bytes: u64,
    /// Content-addressed blob reference (e.g., "hive://blobs/sha256:...")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_reference: Option<String>,
    /// Target platform IDs for deployment
    pub target_platforms: Vec<String>,
    /// Deployment policy
    pub deployment_policy: DeploymentPolicy,
    /// Previous version for rollback support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_version: Option<String>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ModelMetadata>,
}

/// Type of update package
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageType {
    /// Full AI model update
    AiModelUpdate,
    /// Configuration update only
    ConfigUpdate,
    /// Delta/incremental model update
    DeltaUpdate,
}

/// Deployment policy for model updates
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentPolicy {
    /// Deploy to one platform at a time
    Rolling,
    /// Deploy to subset first, then all
    Canary,
    /// Deploy to all platforms immediately
    Immediate,
}

/// Additional metadata for model updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMetadata {
    /// Changelog description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// Training date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_date: Option<String>,
    /// Validation accuracy (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_accuracy: Option<f64>,
}

// ============================================================================
// Command Messages (Downward Flow)
// ============================================================================

/// Track command message - flows downward from C2 to teams
///
/// C2 issues tasking commands to track specific targets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackCommand {
    /// Command identifier
    pub command_id: Uuid,
    /// Type of command
    pub command_type: CommandType,
    /// Description of target to track
    pub target_description: String,
    /// Operational boundary (geofence)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_boundary: Option<OperationalBoundary>,
    /// Priority level (1 = highest, 5 = lowest)
    pub priority: Priority,
    /// Authority that issued the command
    pub source_authority: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Type of C2 command
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandType {
    /// Begin tracking a target
    TrackTarget,
    /// Stop tracking a target
    CancelTrack,
    /// Adjust tracking parameters
    UpdateParameters,
    /// Acknowledge track handoff
    AcknowledgeHandoff,
}

/// Priority levels for commands and data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    /// P1 - Critical (< 1 second latency)
    Critical,
    /// P2 - High (< 5 seconds latency)
    High,
    /// P3 - Normal (< 30 seconds latency)
    Normal,
    /// P4 - Low (best effort)
    Low,
    /// P5 - Bulk (background transfer)
    Bulk,
}

/// Operational boundary (geofence) for tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationalBoundary {
    /// Type of boundary geometry
    pub boundary_type: BoundaryType,
    /// Coordinates defining the boundary
    pub coordinates: Vec<Vec<f64>>,
}

/// Type of boundary geometry
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryType {
    /// Polygon boundary
    Polygon,
    /// Circular boundary (center + radius)
    Circle,
    /// Rectangular boundary
    Rectangle,
}

// ============================================================================
// Handoff Messages (Lateral/Upward Flow)
// ============================================================================

/// Handoff message for track coordination between teams
///
/// Used when a track crosses team boundaries and responsibility
/// must transfer from one team to another.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandoffMessage {
    /// Handoff message identifier
    pub handoff_id: Uuid,
    /// Type of handoff message
    pub handoff_type: HandoffType,
    /// Track being handed off
    pub track_id: String,
    /// Source team/platform releasing the track
    pub source_team: String,
    /// Target team/platform receiving the track
    pub target_team: String,
    /// Current track state
    pub track_state: TrackUpdate,
    /// Recent track history (last N updates)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track_history: Vec<TrackUpdate>,
    /// POI description for reacquisition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poi_description: Option<String>,
    /// Predicted position at handoff time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_position: Option<Position>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Type of handoff message
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HandoffType {
    /// Prepare receiving team for incoming track
    PrepareHandoff,
    /// Confirm receiving team has acquired track
    ConfirmAcquisition,
    /// Release track from source team
    ReleaseTrack,
    /// Handoff failed, source retains track
    HandoffFailed,
}

/// Track status for handoff coordination
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackStatus {
    /// Actively being tracked
    Active,
    /// Handed off to another team
    HandedOff,
    /// Track lost
    Lost,
    /// Exited area of interest
    ExitedAoi,
}

// ============================================================================
// Constructors and Helpers
// ============================================================================

impl TrackUpdate {
    /// Create a new track update with required fields
    pub fn new(
        track_id: impl Into<String>,
        classification: impl Into<String>,
        confidence: f64,
        position: Position,
        source_platform: impl Into<String>,
        source_model: impl Into<String>,
        model_version: impl Into<String>,
    ) -> Self {
        Self {
            track_id: track_id.into(),
            classification: classification.into(),
            confidence,
            position,
            velocity: None,
            attributes: HashMap::new(),
            source_platform: source_platform.into(),
            source_model: source_model.into(),
            model_version: model_version.into(),
            timestamp: Utc::now(),
        }
    }

    /// Set velocity
    pub fn with_velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = Some(velocity);
        self
    }

    /// Add an attribute
    pub fn with_attribute(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

impl Position {
    /// Create a new position with lat/lon
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            cep_m: None,
            hae: None,
        }
    }

    /// Create a position with circular error probable
    pub fn with_cep(lat: f64, lon: f64, cep_m: f64) -> Self {
        Self {
            lat,
            lon,
            cep_m: Some(cep_m),
            hae: None,
        }
    }
}

impl Velocity {
    /// Create a new velocity
    pub fn new(bearing: f64, speed_mps: f64) -> Self {
        Self { bearing, speed_mps }
    }
}

impl CapabilityAdvertisement {
    /// Create a new capability advertisement
    pub fn new(platform_id: impl Into<String>, models: Vec<ModelCapability>) -> Self {
        Self {
            platform_id: platform_id.into(),
            advertised_at: Utc::now(),
            models,
            resources: None,
        }
    }

    /// Add resource metrics
    pub fn with_resources(mut self, resources: ResourceMetrics) -> Self {
        self.resources = Some(resources);
        self
    }
}

impl ModelCapability {
    /// Create a new model capability with required fields
    pub fn new(
        model_id: impl Into<String>,
        model_version: impl Into<String>,
        model_hash: impl Into<String>,
        model_type: impl Into<String>,
        performance: ModelPerformance,
    ) -> Self {
        Self {
            model_id: model_id.into(),
            model_version: model_version.into(),
            model_hash: model_hash.into(),
            model_type: model_type.into(),
            performance,
            operational_status: OperationalStatus::Ready,
            resource_requirements: None,
            input_signature: Vec::new(),
            output_signature: Vec::new(),
            model_size_bytes: None,
            framework: None,
            quantization: None,
            class_labels: Vec::new(),
            num_classes: None,
            loaded_at: None,
            inference_count: None,
            last_inference_at: None,
            degraded: false,
            degradation_reason: None,
        }
    }

    /// Set operational status
    pub fn with_status(mut self, status: OperationalStatus) -> Self {
        self.operational_status = status;
        self
    }

    /// Set resource requirements
    pub fn with_resource_requirements(mut self, requirements: ResourceRequirements) -> Self {
        self.resource_requirements = Some(requirements);
        self
    }

    /// Set framework and quantization
    pub fn with_framework(mut self, framework: impl Into<String>, quantization: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self.quantization = Some(quantization.into());
        self
    }

    /// Set model size
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.model_size_bytes = Some(size_bytes);
        self
    }

    /// Set class labels
    pub fn with_classes(mut self, labels: Vec<String>) -> Self {
        self.num_classes = Some(labels.len());
        self.class_labels = labels;
        self
    }

    /// Mark as loaded (sets loaded_at timestamp)
    pub fn mark_loaded(mut self) -> Self {
        self.loaded_at = Some(Utc::now());
        self.operational_status = OperationalStatus::Ready;
        self
    }

    /// Record an inference (updates counters and timestamp)
    pub fn record_inference(&mut self) {
        self.inference_count = Some(self.inference_count.unwrap_or(0) + 1);
        self.last_inference_at = Some(Utc::now());
        if self.operational_status == OperationalStatus::Ready {
            self.operational_status = OperationalStatus::Active;
        }
    }

    /// Mark as degraded with reason
    pub fn mark_degraded(&mut self, reason: impl Into<String>) {
        self.degraded = true;
        self.degradation_reason = Some(reason.into());
        self.operational_status = OperationalStatus::Degraded;
    }

    /// Clear degradation status
    pub fn clear_degradation(&mut self) {
        self.degraded = false;
        self.degradation_reason = None;
        if self.operational_status == OperationalStatus::Degraded {
            self.operational_status = OperationalStatus::Ready;
        }
    }

    /// Check if model meets minimum version requirement
    pub fn meets_version(&self, min_version: &str) -> bool {
        version_compare(&self.model_version, min_version) >= std::cmp::Ordering::Equal
    }

    /// Check if model meets minimum precision requirement
    pub fn meets_precision(&self, min_precision: f64) -> bool {
        self.performance.precision >= min_precision
    }

    /// Check if model is operational (ready, active, or degraded but functional)
    pub fn is_operational(&self) -> bool {
        matches!(
            self.operational_status,
            OperationalStatus::Ready | OperationalStatus::Active | OperationalStatus::Degraded
        )
    }
}

/// Simple semver comparison (major.minor.patch)
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(a).cmp(&parse(b))
}

impl ModelPerformance {
    /// Create new performance metrics
    pub fn new(precision: f64, recall: f64, fps: f64) -> Self {
        Self {
            precision,
            recall,
            fps,
            latency_ms: None,
        }
    }

    /// Add latency measurement
    pub fn with_latency(mut self, latency_ms: f64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }
}

impl TrackCommand {
    /// Create a new track command
    pub fn new(
        target_description: impl Into<String>,
        priority: Priority,
        source_authority: impl Into<String>,
    ) -> Self {
        Self {
            command_id: Uuid::new_v4(),
            command_type: CommandType::TrackTarget,
            target_description: target_description.into(),
            operational_boundary: None,
            priority,
            source_authority: source_authority.into(),
            timestamp: Utc::now(),
        }
    }

    /// Set operational boundary
    pub fn with_boundary(mut self, boundary: OperationalBoundary) -> Self {
        self.operational_boundary = Some(boundary);
        self
    }
}

impl HandoffMessage {
    /// Create a new handoff message
    pub fn new(
        handoff_type: HandoffType,
        track_id: impl Into<String>,
        source_team: impl Into<String>,
        target_team: impl Into<String>,
        track_state: TrackUpdate,
    ) -> Self {
        Self {
            handoff_id: Uuid::new_v4(),
            handoff_type,
            track_id: track_id.into(),
            source_team: source_team.into(),
            target_team: target_team.into(),
            track_state,
            track_history: Vec::new(),
            poi_description: None,
            predicted_position: None,
            timestamp: Utc::now(),
        }
    }

    /// Add track history
    pub fn with_history(mut self, history: Vec<TrackUpdate>) -> Self {
        self.track_history = history;
        self
    }

    /// Set POI description
    pub fn with_poi_description(mut self, description: impl Into<String>) -> Self {
        self.poi_description = Some(description.into());
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_update_json_roundtrip() {
        let update = TrackUpdate::new(
            "TRACK-001",
            "person",
            0.89,
            Position::with_cep(33.7749, -84.3958, 2.5),
            "Alpha-2",
            "Alpha-3",
            "1.3.0",
        )
        .with_velocity(Velocity::new(45.0, 1.2))
        .with_attribute("jacket_color", "blue")
        .with_attribute("has_backpack", true);

        let json = serde_json::to_string_pretty(&update).unwrap();
        let parsed: TrackUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(update.track_id, parsed.track_id);
        assert_eq!(update.classification, parsed.classification);
        assert_eq!(update.confidence, parsed.confidence);
        assert_eq!(update.position, parsed.position);
        assert_eq!(update.velocity, parsed.velocity);
        assert_eq!(
            update.attributes.get("jacket_color"),
            parsed.attributes.get("jacket_color")
        );
    }

    #[test]
    fn test_capability_advertisement_json_roundtrip() {
        let cap = CapabilityAdvertisement::new(
            "Alpha-3",
            vec![ModelCapability::new(
                "object_tracker",
                "1.3.0",
                "sha256:b8d9c4e2f1a3",
                "detector_tracker",
                ModelPerformance::new(0.94, 0.89, 15.0).with_latency(67.0),
            )
            .with_status(OperationalStatus::Ready)],
        )
        .with_resources(ResourceMetrics {
            gpu_utilization: Some(0.65),
            memory_used_mb: Some(2048),
            memory_total_mb: Some(4096),
            cpu_utilization: None,
        });

        let json = serde_json::to_string_pretty(&cap).unwrap();
        let parsed: CapabilityAdvertisement = serde_json::from_str(&json).unwrap();

        assert_eq!(cap.platform_id, parsed.platform_id);
        assert_eq!(cap.models.len(), parsed.models.len());
        assert_eq!(cap.models[0].model_id, parsed.models[0].model_id);
        assert_eq!(cap.resources, parsed.resources);
    }

    #[test]
    fn test_model_update_package_json_roundtrip() {
        let pkg = ModelUpdatePackage {
            package_type: PackageType::AiModelUpdate,
            model_id: "object_tracker".to_string(),
            model_version: "1.3.0".to_string(),
            model_hash: "sha256:b8d9c4e2f1a3".to_string(),
            model_size_bytes: 45_000_000,
            blob_reference: Some("hive://blobs/sha256:b8d9c4e2f1a3".to_string()),
            target_platforms: vec!["Alpha-3".to_string(), "Bravo-3".to_string()],
            deployment_policy: DeploymentPolicy::Rolling,
            rollback_version: Some("1.2.0".to_string()),
            metadata: Some(ModelMetadata {
                changelog: Some("Improved low-light detection".to_string()),
                training_date: Some("2025-11-26".to_string()),
                validation_accuracy: Some(0.94),
            }),
        };

        let json = serde_json::to_string_pretty(&pkg).unwrap();
        let parsed: ModelUpdatePackage = serde_json::from_str(&json).unwrap();

        assert_eq!(pkg.model_id, parsed.model_id);
        assert_eq!(pkg.model_version, parsed.model_version);
        assert_eq!(pkg.deployment_policy, parsed.deployment_policy);
        assert_eq!(pkg.metadata, parsed.metadata);
    }

    #[test]
    fn test_track_command_json_roundtrip() {
        let cmd = TrackCommand::new(
            "Adult male, blue jacket, backpack",
            Priority::High,
            "C2-Commander",
        )
        .with_boundary(OperationalBoundary {
            boundary_type: BoundaryType::Polygon,
            coordinates: vec![
                vec![-84.40, 33.77],
                vec![-84.39, 33.77],
                vec![-84.39, 33.78],
                vec![-84.40, 33.78],
            ],
        });

        let json = serde_json::to_string_pretty(&cmd).unwrap();
        let parsed: TrackCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(cmd.command_id, parsed.command_id);
        assert_eq!(cmd.target_description, parsed.target_description);
        assert_eq!(cmd.priority, parsed.priority);
        assert!(parsed.operational_boundary.is_some());
    }

    #[test]
    fn test_handoff_message_json_roundtrip() {
        let track = TrackUpdate::new(
            "TRACK-001",
            "person",
            0.89,
            Position::with_cep(33.7749, -84.3958, 2.5),
            "Alpha-2",
            "Alpha-3",
            "1.3.0",
        );

        let handoff = HandoffMessage::new(
            HandoffType::PrepareHandoff,
            "TRACK-001",
            "Alpha",
            "Bravo",
            track,
        )
        .with_poi_description("Adult male, blue jacket, backpack")
        .with_history(vec![]);

        let json = serde_json::to_string_pretty(&handoff).unwrap();
        let parsed: HandoffMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(handoff.handoff_id, parsed.handoff_id);
        assert_eq!(handoff.track_id, parsed.track_id);
        assert_eq!(handoff.handoff_type, parsed.handoff_type);
        assert_eq!(handoff.poi_description, parsed.poi_description);
    }

    #[test]
    fn test_operational_status_serialization() {
        assert_eq!(
            serde_json::to_string(&OperationalStatus::Ready).unwrap(),
            "\"READY\""
        );
        assert_eq!(
            serde_json::to_string(&OperationalStatus::Active).unwrap(),
            "\"ACTIVE\""
        );
    }

    #[test]
    fn test_priority_serialization() {
        assert_eq!(
            serde_json::to_string(&Priority::Critical).unwrap(),
            "\"CRITICAL\""
        );
        assert_eq!(serde_json::to_string(&Priority::Bulk).unwrap(), "\"BULK\"");
    }

    #[test]
    fn test_position_equality() {
        let p1 = Position::with_cep(33.7749, -84.3958, 2.5);
        let p2 = Position::with_cep(33.7749, -84.3958, 2.5);
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_velocity_creation() {
        let vel = Velocity::new(45.0, 1.2);
        assert_eq!(vel.bearing, 45.0);
        assert_eq!(vel.speed_mps, 1.2);
    }
}
