//! Schema conversion module for HIVE Protocol integration
//!
//! This module provides conversion between hive-inference's AI-specific message types
//! and hive-schema's generic proto-generated types. This enables:
//!
//! - Interoperability with other HIVE components using proto schemas
//! - Schema validation against Core team's canonical definitions
//! - Multi-language compatibility through protobuf serialization
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::schema::{ToProtoCapability, ToProtoTrack};
//! use hive_inference::messages::CapabilityAdvertisement;
//!
//! // Convert AI-specific capability to generic proto type
//! let proto_cap = capability.to_proto();
//!
//! // Serialize to protobuf bytes for transport
//! let bytes = proto_cap.encode_to_vec();
//! ```

use crate::messages::{
    CapabilityAdvertisement, ModelCapability, ModelPerformance, OperationalStatus, Position,
    ResourceMetrics, TrackUpdate, Velocity,
};

// Re-export hive-schema types for convenience
pub use hive_schema::capability::v1::{
    Capability as ProtoCapability, CapabilityAdvertisement as ProtoCapabilityAdvertisement,
    CapabilityType as ProtoCapabilityType, OperationalStatus as ProtoOperationalStatus,
    ResourceStatus as ProtoResourceStatus,
};
pub use hive_schema::common::v1::{Position as ProtoPosition, Timestamp as ProtoTimestamp};
pub use hive_schema::track::v1::{
    SourceType as ProtoSourceType, Track as ProtoTrack, TrackPosition as ProtoTrackPosition,
    TrackSource as ProtoTrackSource, TrackState as ProtoTrackState,
    TrackUpdate as ProtoTrackUpdate, UpdateType as ProtoUpdateType, Velocity as ProtoVelocity,
};

// ============================================================================
// Timestamp Conversion
// ============================================================================

/// Convert chrono DateTime to proto Timestamp
pub fn datetime_to_proto(dt: &chrono::DateTime<chrono::Utc>) -> ProtoTimestamp {
    ProtoTimestamp {
        seconds: dt.timestamp() as u64,
        nanos: dt.timestamp_subsec_nanos(),
    }
}

/// Convert proto Timestamp to chrono DateTime
pub fn proto_to_datetime(ts: &ProtoTimestamp) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(ts.seconds as i64, ts.nanos)
        .single()
        .unwrap_or_else(chrono::Utc::now)
}

// ============================================================================
// Capability Advertisement Conversion
// ============================================================================

/// Trait for converting to proto CapabilityAdvertisement
pub trait ToProtoCapability {
    /// Convert to proto CapabilityAdvertisement
    fn to_proto(&self) -> ProtoCapabilityAdvertisement;
}

/// Trait for converting from proto CapabilityAdvertisement
pub trait FromProtoCapability {
    /// Create from proto CapabilityAdvertisement
    fn from_proto(proto: &ProtoCapabilityAdvertisement) -> Self;
}

impl ToProtoCapability for CapabilityAdvertisement {
    fn to_proto(&self) -> ProtoCapabilityAdvertisement {
        // Convert ModelCapabilities to generic Capabilities
        let capabilities: Vec<ProtoCapability> =
            self.models.iter().map(model_capability_to_proto).collect();

        // Convert resource metrics
        let resources = self.resources.as_ref().map(resource_metrics_to_proto);

        // Determine overall operational status from models
        let operational_status = self
            .models
            .first()
            .map(|m| operational_status_to_proto(m.operational_status))
            .unwrap_or(ProtoOperationalStatus::Ready as i32);

        ProtoCapabilityAdvertisement {
            platform_id: self.platform_id.clone(),
            advertised_at: Some(datetime_to_proto(&self.advertised_at)),
            capabilities,
            resources,
            operational_status,
        }
    }
}

impl FromProtoCapability for CapabilityAdvertisement {
    fn from_proto(proto: &ProtoCapabilityAdvertisement) -> Self {
        // Convert capabilities back to ModelCapabilities
        // Note: Only capabilities that look like AI models are converted
        let models: Vec<ModelCapability> = proto
            .capabilities
            .iter()
            .filter_map(proto_to_model_capability)
            .collect();

        let resources = proto.resources.as_ref().map(proto_to_resource_metrics);

        let advertised_at = proto
            .advertised_at
            .as_ref()
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now);

        CapabilityAdvertisement {
            platform_id: proto.platform_id.clone(),
            advertised_at,
            models,
            resources,
        }
    }
}

/// Convert ModelCapability to generic proto Capability
fn model_capability_to_proto(model: &ModelCapability) -> ProtoCapability {
    // Encode model-specific metadata as JSON
    let metadata = serde_json::json!({
        "model_id": model.model_id,
        "model_version": model.model_version,
        "model_hash": model.model_hash,
        "model_type": model.model_type,
        "precision": model.performance.precision,
        "recall": model.performance.recall,
        "fps": model.performance.fps,
        "latency_ms": model.performance.latency_ms,
        "framework": model.framework,
        "quantization": model.quantization,
        "model_size_bytes": model.model_size_bytes,
        "input_signature": model.input_signature,
        "output_signature": model.output_signature,
        "class_labels": model.class_labels,
        "num_classes": model.num_classes,
        "degraded": model.degraded,
        "degradation_reason": model.degradation_reason,
    });

    ProtoCapability {
        id: model.model_id.clone(),
        name: format!("{} v{}", model.model_id, model.model_version),
        capability_type: ProtoCapabilityType::Compute as i32, // AI models are compute capabilities
        confidence: model.performance.precision as f32,       // Use precision as confidence proxy
        metadata_json: metadata.to_string(),
        registered_at: model.loaded_at.as_ref().map(datetime_to_proto),
    }
}

/// Convert proto Capability back to ModelCapability (if it looks like an AI model)
fn proto_to_model_capability(proto: &ProtoCapability) -> Option<ModelCapability> {
    // Parse metadata JSON
    let metadata: serde_json::Value = serde_json::from_str(&proto.metadata_json).ok()?;

    // Check if this is an AI model capability
    let model_id = metadata.get("model_id")?.as_str()?;
    let model_version = metadata.get("model_version")?.as_str()?;
    let model_hash = metadata.get("model_hash")?.as_str().unwrap_or("unknown");
    let model_type = metadata.get("model_type")?.as_str()?;

    let precision = metadata
        .get("precision")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let recall = metadata
        .get("recall")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let fps = metadata.get("fps").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let latency_ms = metadata.get("latency_ms").and_then(|v| v.as_f64());

    let performance = ModelPerformance {
        precision,
        recall,
        fps,
        latency_ms,
    };

    Some(ModelCapability {
        model_id: model_id.to_string(),
        model_version: model_version.to_string(),
        model_hash: model_hash.to_string(),
        model_type: model_type.to_string(),
        performance,
        operational_status: OperationalStatus::Ready, // Default, actual status in parent
        resource_requirements: None,
        input_signature: metadata
            .get("input_signature")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        output_signature: metadata
            .get("output_signature")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        model_size_bytes: metadata.get("model_size_bytes").and_then(|v| v.as_u64()),
        framework: metadata
            .get("framework")
            .and_then(|v| v.as_str())
            .map(String::from),
        quantization: metadata
            .get("quantization")
            .and_then(|v| v.as_str())
            .map(String::from),
        class_labels: metadata
            .get("class_labels")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        num_classes: metadata
            .get("num_classes")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize),
        loaded_at: proto.registered_at.as_ref().map(proto_to_datetime),
        inference_count: None,
        last_inference_at: None,
        degraded: metadata
            .get("degraded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        degradation_reason: metadata
            .get("degradation_reason")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Convert ResourceMetrics to proto ResourceStatus
fn resource_metrics_to_proto(metrics: &ResourceMetrics) -> ProtoResourceStatus {
    ProtoResourceStatus {
        compute_utilization: metrics.gpu_utilization.unwrap_or(0.0) as f32,
        memory_utilization: metrics
            .memory_used_mb
            .and_then(|used| {
                metrics
                    .memory_total_mb
                    .map(|total| used as f32 / total as f32)
            })
            .unwrap_or(0.0),
        power_level: 1.0,           // Not tracked in current metrics
        storage_utilization: 0.0,   // Not tracked in current metrics
        bandwidth_utilization: 0.0, // Not tracked in current metrics
        extra_json: serde_json::json!({
            "gpu_utilization": metrics.gpu_utilization,
            "memory_used_mb": metrics.memory_used_mb,
            "memory_total_mb": metrics.memory_total_mb,
            "cpu_utilization": metrics.cpu_utilization,
        })
        .to_string(),
    }
}

/// Convert proto ResourceStatus back to ResourceMetrics
fn proto_to_resource_metrics(proto: &ProtoResourceStatus) -> ResourceMetrics {
    // Try to parse extended metrics from extra_json
    let extra: serde_json::Value =
        serde_json::from_str(&proto.extra_json).unwrap_or(serde_json::json!({}));

    ResourceMetrics {
        gpu_utilization: extra
            .get("gpu_utilization")
            .and_then(|v| v.as_f64())
            .or(Some(proto.compute_utilization as f64)),
        memory_used_mb: extra.get("memory_used_mb").and_then(|v| v.as_u64()),
        memory_total_mb: extra.get("memory_total_mb").and_then(|v| v.as_u64()),
        cpu_utilization: extra.get("cpu_utilization").and_then(|v| v.as_f64()),
    }
}

/// Convert OperationalStatus to proto enum value
fn operational_status_to_proto(status: OperationalStatus) -> i32 {
    match status {
        OperationalStatus::Ready => ProtoOperationalStatus::Ready as i32,
        OperationalStatus::Active => ProtoOperationalStatus::Active as i32,
        OperationalStatus::Degraded => ProtoOperationalStatus::Degraded as i32,
        OperationalStatus::Offline => ProtoOperationalStatus::Offline as i32,
        OperationalStatus::Loading => ProtoOperationalStatus::Unspecified as i32,
        OperationalStatus::Failed => ProtoOperationalStatus::Offline as i32,
        OperationalStatus::Updating => ProtoOperationalStatus::Maintenance as i32,
        OperationalStatus::Unloaded => ProtoOperationalStatus::Offline as i32,
    }
}

// ============================================================================
// Track Update Conversion
// ============================================================================

/// Trait for converting to proto TrackUpdate
pub trait ToProtoTrack {
    /// Convert to proto TrackUpdate
    fn to_proto(&self) -> ProtoTrackUpdate;
}

/// Trait for converting from proto TrackUpdate
pub trait FromProtoTrack {
    /// Create from proto TrackUpdate
    fn from_proto(proto: &ProtoTrackUpdate) -> Option<Self>
    where
        Self: Sized;
}

impl ToProtoTrack for TrackUpdate {
    fn to_proto(&self) -> ProtoTrackUpdate {
        let position = ProtoTrackPosition {
            latitude: self.position.lat,
            longitude: self.position.lon,
            altitude: self.position.hae.unwrap_or(0.0) as f32,
            cep_m: self.position.cep_m.unwrap_or(0.0) as f32,
            vertical_error_m: 0.0,
        };

        let velocity = self.velocity.as_ref().map(|v| ProtoVelocity {
            bearing: v.bearing as f32,
            speed_mps: v.speed_mps as f32,
            vertical_speed_mps: 0.0,
        });

        let source = ProtoTrackSource {
            platform_id: self.source_platform.clone(),
            sensor_id: self.source_model.clone(),
            model_version: self.model_version.clone(),
            source_type: ProtoSourceType::AiModel as i32,
        };

        let track = ProtoTrack {
            track_id: self.track_id.clone(),
            classification: self.classification.clone(),
            confidence: self.confidence as f32,
            position: Some(position),
            velocity,
            state: ProtoTrackState::Confirmed as i32,
            source: Some(source),
            attributes_json: serde_json::to_string(&self.attributes).unwrap_or_default(),
            first_seen: Some(datetime_to_proto(&self.timestamp)),
            last_seen: Some(datetime_to_proto(&self.timestamp)),
            observation_count: 1,
        };

        ProtoTrackUpdate {
            update_type: ProtoUpdateType::Update as i32,
            track: Some(track),
            previous_track_id: String::new(),
            timestamp: Some(datetime_to_proto(&self.timestamp)),
        }
    }
}

impl FromProtoTrack for TrackUpdate {
    fn from_proto(proto: &ProtoTrackUpdate) -> Option<Self> {
        let track = proto.track.as_ref()?;
        let pos = track.position.as_ref()?;
        let source = track.source.as_ref()?;

        let position = Position {
            lat: pos.latitude,
            lon: pos.longitude,
            cep_m: Some(pos.cep_m as f64),
            hae: if pos.altitude != 0.0 {
                Some(pos.altitude as f64)
            } else {
                None
            },
        };

        let velocity = track.velocity.as_ref().map(|v| Velocity {
            bearing: v.bearing as f64,
            speed_mps: v.speed_mps as f64,
        });

        let timestamp = proto
            .timestamp
            .as_ref()
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now);

        let attributes: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&track.attributes_json).unwrap_or_default();

        Some(TrackUpdate {
            track_id: track.track_id.clone(),
            classification: track.classification.clone(),
            confidence: track.confidence as f64,
            position,
            velocity,
            attributes,
            source_platform: source.platform_id.clone(),
            source_model: source.sensor_id.clone(),
            model_version: source.model_version.clone(),
            timestamp,
        })
    }
}

// ============================================================================
// Protobuf Encoding Helpers
// ============================================================================

/// Extension trait for encoding proto messages to bytes
pub trait EncodeProto {
    /// Encode to protobuf bytes
    fn encode_to_vec(&self) -> Vec<u8>;
}

impl EncodeProto for ProtoCapabilityAdvertisement {
    fn encode_to_vec(&self) -> Vec<u8> {
        use prost::Message;
        let mut buf = Vec::new();
        self.encode(&mut buf).expect("encoding should not fail");
        buf
    }
}

impl EncodeProto for ProtoTrackUpdate {
    fn encode_to_vec(&self) -> Vec<u8> {
        use prost::Message;
        let mut buf = Vec::new();
        self.encode(&mut buf).expect("encoding should not fail");
        buf
    }
}

/// Extension trait for decoding proto messages from bytes
pub trait DecodeProto: Sized {
    /// Decode from protobuf bytes
    fn decode_from_slice(buf: &[u8]) -> Result<Self, prost::DecodeError>;
}

impl DecodeProto for ProtoCapabilityAdvertisement {
    fn decode_from_slice(buf: &[u8]) -> Result<Self, prost::DecodeError> {
        use prost::Message;
        Self::decode(buf)
    }
}

impl DecodeProto for ProtoTrackUpdate {
    fn decode_from_slice(buf: &[u8]) -> Result<Self, prost::DecodeError> {
        use prost::Message;
        Self::decode(buf)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{ModelPerformance, Position};

    #[test]
    fn test_capability_roundtrip() {
        let original = CapabilityAdvertisement {
            platform_id: "test-platform".to_string(),
            advertised_at: chrono::Utc::now(),
            models: vec![ModelCapability::new(
                "yolov8n",
                "1.0.0",
                "sha256:abc123",
                "detector",
                ModelPerformance::new(0.9, 0.85, 30.0),
            )],
            resources: Some(ResourceMetrics {
                gpu_utilization: Some(0.5),
                memory_used_mb: Some(2048),
                memory_total_mb: Some(8192),
                cpu_utilization: Some(0.3),
            }),
        };

        // Convert to proto
        let proto = original.to_proto();

        // Verify proto fields
        assert_eq!(proto.platform_id, "test-platform");
        assert_eq!(proto.capabilities.len(), 1);
        assert!(proto.resources.is_some());

        // Convert back
        let restored = CapabilityAdvertisement::from_proto(&proto);

        assert_eq!(restored.platform_id, original.platform_id);
        assert_eq!(restored.models.len(), 1);
        assert_eq!(restored.models[0].model_id, "yolov8n");
        assert_eq!(restored.models[0].model_version, "1.0.0");
    }

    #[test]
    fn test_track_roundtrip() {
        let original = TrackUpdate::new(
            "TRACK-001",
            "person",
            0.89,
            Position::with_cep(33.7749, -84.3958, 2.5),
            "Alpha-2",
            "Alpha-3",
            "1.3.0",
        )
        .with_velocity(crate::messages::Velocity::new(45.0, 1.2));

        // Convert to proto
        let proto = original.to_proto();

        // Verify proto fields
        assert!(proto.track.is_some());
        let track = proto.track.as_ref().unwrap();
        assert_eq!(track.track_id, "TRACK-001");
        assert_eq!(track.classification, "person");

        // Convert back
        let restored = TrackUpdate::from_proto(&proto).unwrap();

        assert_eq!(restored.track_id, original.track_id);
        assert_eq!(restored.classification, original.classification);
        assert!((restored.confidence - original.confidence).abs() < 0.001);
        assert_eq!(restored.source_platform, original.source_platform);
    }

    #[test]
    fn test_proto_encoding() {
        let cap = CapabilityAdvertisement::new(
            "test",
            vec![ModelCapability::new(
                "model",
                "1.0.0",
                "hash",
                "detector",
                ModelPerformance::new(0.9, 0.8, 30.0),
            )],
        );

        let proto = cap.to_proto();
        let bytes = proto.encode_to_vec();

        // Should produce non-empty bytes
        assert!(!bytes.is_empty());

        // Should be decodable
        let decoded = ProtoCapabilityAdvertisement::decode_from_slice(&bytes).unwrap();
        assert_eq!(decoded.platform_id, "test");
    }

    #[test]
    fn test_timestamp_conversion() {
        let now = chrono::Utc::now();
        let proto = datetime_to_proto(&now);
        let restored = proto_to_datetime(&proto);

        // Should be within 1 second (accounting for nanos precision)
        let diff = (now - restored).num_milliseconds().abs();
        assert!(diff < 1000);
    }
}
