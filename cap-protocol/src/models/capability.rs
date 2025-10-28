//! Capability data structures

use serde::{Deserialize, Serialize};

/// Represents a platform or squad capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    /// Unique capability identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Capability type (sensor, compute, comms, etc.)
    pub capability_type: CapabilityType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Types of capabilities
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityType {
    /// Sensing capabilities (cameras, radar, etc.)
    Sensor,
    /// Computing capabilities
    Compute,
    /// Communication capabilities
    Communication,
    /// Mobility capabilities
    Mobility,
    /// Payload/weapon capabilities
    Payload,
    /// Emergent capability from composition
    Emergent,
}

impl Capability {
    /// Create a new capability
    pub fn new(id: String, name: String, capability_type: CapabilityType, confidence: f32) -> Self {
        Self {
            id,
            name,
            capability_type,
            confidence: confidence.clamp(0.0, 1.0),
            metadata: serde_json::Value::Null,
        }
    }

    /// Check if this capability is valid (confidence > threshold)
    pub fn is_valid(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}
