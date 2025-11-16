//! Capability data structures
//!
//! This module re-exports the protobuf-generated Capability types from cap-schema
//! and provides extension traits for ergonomic usage.

// Re-export protobuf types from cap-schema
pub use hive_schema::cap::capability::v1::{Capability, CapabilityType};

/// Extension trait for Capability with ergonomic constructors and accessors
pub trait CapabilityExt {
    /// Create a new capability
    ///
    /// # Arguments
    /// * `id` - Unique capability identifier
    /// * `name` - Human-readable name
    /// * `capability_type` - Type of capability (sensor, compute, etc.)
    /// * `confidence` - Confidence score (0.0 - 1.0), will be clamped
    fn new(id: String, name: String, capability_type: CapabilityType, confidence: f32) -> Self;

    /// Get the capability type as the enum (not i32)
    ///
    /// Returns the CapabilityType enum value, converting from the protobuf i32 field.
    /// Returns Unspecified if the field contains an invalid value.
    fn get_capability_type(&self) -> CapabilityType;

    /// Set the capability type from the enum
    ///
    /// # Arguments
    /// * `capability_type` - The CapabilityType enum value to set
    fn set_capability_type(&mut self, capability_type: CapabilityType);

    /// Check if this capability is valid (confidence > threshold)
    ///
    /// # Arguments
    /// * `threshold` - Minimum confidence threshold (0.0 - 1.0)
    fn is_valid(&self, threshold: f32) -> bool;
}

impl CapabilityExt for Capability {
    fn new(id: String, name: String, capability_type: CapabilityType, confidence: f32) -> Self {
        Self {
            id,
            name,
            capability_type: capability_type as i32,
            confidence: confidence.clamp(0.0, 1.0),
            metadata_json: String::new(),
            registered_at: None,
        }
    }

    fn get_capability_type(&self) -> CapabilityType {
        CapabilityType::try_from(self.capability_type).unwrap_or(CapabilityType::Unspecified)
    }

    fn set_capability_type(&mut self, capability_type: CapabilityType) {
        self.capability_type = capability_type as i32;
    }

    fn is_valid(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_new() {
        let cap = Capability::new(
            "sensor-1".to_string(),
            "Camera".to_string(),
            CapabilityType::Sensor,
            0.85,
        );

        assert_eq!(cap.id, "sensor-1");
        assert_eq!(cap.name, "Camera");
        assert_eq!(cap.get_capability_type(), CapabilityType::Sensor);
        assert_eq!(cap.confidence, 0.85);
    }

    #[test]
    fn test_capability_confidence_clamping() {
        // Test upper bound
        let cap_high = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Compute,
            1.5,
        );
        assert_eq!(cap_high.confidence, 1.0);

        // Test lower bound
        let cap_low = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Compute,
            -0.5,
        );
        assert_eq!(cap_low.confidence, 0.0);

        // Test normal value
        let cap_normal = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Compute,
            0.75,
        );
        assert_eq!(cap_normal.confidence, 0.75);
    }

    #[test]
    fn test_get_capability_type() {
        let cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Communication,
            0.8,
        );

        assert_eq!(cap.get_capability_type(), CapabilityType::Communication);
        assert_eq!(cap.capability_type, CapabilityType::Communication as i32);
    }

    #[test]
    fn test_set_capability_type() {
        let mut cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        assert_eq!(cap.get_capability_type(), CapabilityType::Sensor);

        cap.set_capability_type(CapabilityType::Payload);
        assert_eq!(cap.get_capability_type(), CapabilityType::Payload);
        assert_eq!(cap.capability_type, CapabilityType::Payload as i32);
    }

    #[test]
    fn test_capability_type_roundtrip() {
        // Test all capability types
        let types = vec![
            CapabilityType::Unspecified,
            CapabilityType::Sensor,
            CapabilityType::Compute,
            CapabilityType::Communication,
            CapabilityType::Mobility,
            CapabilityType::Payload,
            CapabilityType::Emergent,
        ];

        for cap_type in types {
            let cap = Capability::new("test".to_string(), "Test".to_string(), cap_type, 0.8);
            assert_eq!(cap.get_capability_type(), cap_type);
        }
    }

    #[test]
    fn test_is_valid() {
        let cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        assert!(cap.is_valid(0.7));
        assert!(cap.is_valid(0.8));
        assert!(!cap.is_valid(0.9));
    }

    #[test]
    fn test_is_valid_edge_cases() {
        let cap_zero = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.0,
        );
        assert!(cap_zero.is_valid(0.0));
        assert!(!cap_zero.is_valid(0.1));

        let cap_one = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            1.0,
        );
        assert!(cap_one.is_valid(1.0));
        assert!(cap_one.is_valid(0.9));
    }

    #[test]
    fn test_invalid_capability_type_defaults_to_unspecified() {
        let mut cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        // Manually set to an invalid value
        cap.capability_type = 999;

        // Should return Unspecified for invalid values
        assert_eq!(cap.get_capability_type(), CapabilityType::Unspecified);
    }

    #[test]
    fn test_metadata_json_field() {
        let cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        // New capabilities start with empty metadata
        assert_eq!(cap.metadata_json, "");

        // Can be set to JSON string
        let mut cap_with_metadata = cap.clone();
        cap_with_metadata.metadata_json = r#"{"key": "value"}"#.to_string();
        assert_eq!(cap_with_metadata.metadata_json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_registered_at_field() {
        let cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        // New capabilities start with no timestamp
        assert!(cap.registered_at.is_none());
    }

    #[test]
    fn test_capability_with_empty_strings() {
        let cap = Capability::new(String::new(), String::new(), CapabilityType::Sensor, 0.5);

        assert_eq!(cap.id, "");
        assert_eq!(cap.name, "");
        assert_eq!(cap.confidence, 0.5);
    }

    #[test]
    fn test_capability_confidence_boundary_values() {
        // Test exact boundary values
        let cap_zero = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.0,
        );
        assert_eq!(cap_zero.confidence, 0.0);
        assert!(cap_zero.is_valid(0.0));
        assert!(!cap_zero.is_valid(0.000001));

        let cap_one = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            1.0,
        );
        assert_eq!(cap_one.confidence, 1.0);
        assert!(cap_one.is_valid(1.0));
        assert!(cap_one.is_valid(0.999999));
    }

    #[test]
    fn test_capability_type_set_then_get() {
        let mut cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Unspecified,
            0.5,
        );

        // Test setting each capability type
        for cap_type in [
            CapabilityType::Sensor,
            CapabilityType::Compute,
            CapabilityType::Communication,
            CapabilityType::Mobility,
            CapabilityType::Payload,
            CapabilityType::Emergent,
        ] {
            cap.set_capability_type(cap_type);
            assert_eq!(cap.get_capability_type(), cap_type);
            assert_eq!(cap.capability_type, cap_type as i32);
        }
    }

    #[test]
    fn test_capability_clone() {
        let cap1 = Capability::new(
            "sensor-1".to_string(),
            "Camera".to_string(),
            CapabilityType::Sensor,
            0.85,
        );

        let cap2 = cap1.clone();
        assert_eq!(cap1.id, cap2.id);
        assert_eq!(cap1.name, cap2.name);
        assert_eq!(cap1.capability_type, cap2.capability_type);
        assert_eq!(cap1.confidence, cap2.confidence);
    }

    #[test]
    fn test_capability_metadata_json_manipulation() {
        let mut cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.8,
        );

        // Set complex JSON
        cap.metadata_json =
            r#"{"manufacturer": "ACME", "model": "X1000", "version": "2.1"}"#.to_string();
        assert!(cap.metadata_json.contains("ACME"));

        // Parse to ensure it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&cap.metadata_json).unwrap();
        assert_eq!(parsed["manufacturer"], "ACME");
        assert_eq!(parsed["model"], "X1000");
    }

    #[test]
    fn test_is_valid_threshold_variations() {
        let cap = Capability::new(
            "test".to_string(),
            "Test".to_string(),
            CapabilityType::Sensor,
            0.75,
        );

        // Test various thresholds
        assert!(cap.is_valid(0.0));
        assert!(cap.is_valid(0.5));
        assert!(cap.is_valid(0.74));
        assert!(cap.is_valid(0.75)); // Equal to confidence
        assert!(!cap.is_valid(0.76));
        assert!(!cap.is_valid(0.9));
        assert!(!cap.is_valid(1.0));
    }

    #[test]
    fn test_capability_protobuf_field_access() {
        let cap = Capability::new(
            "test-id".to_string(),
            "Test Name".to_string(),
            CapabilityType::Compute,
            0.9,
        );

        // Direct protobuf field access
        assert_eq!(cap.id, "test-id");
        assert_eq!(cap.name, "Test Name");
        assert_eq!(cap.capability_type, CapabilityType::Compute as i32);
        assert_eq!(cap.confidence, 0.9);
        assert_eq!(cap.metadata_json, "");
        assert!(cap.registered_at.is_none());
    }
}
