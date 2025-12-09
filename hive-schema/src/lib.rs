//! # CAP Schema
//!
//! Protocol Buffer message definitions for the Capability Aggregation Protocol (CAP).
//!
//! This crate provides schema-first message definitions that enable:
//! - Multi-transport support (HTTP, gRPC, ROS2, WebSocket, MQTT)
//! - Multi-language integration (Rust, Python, Java, C++, JavaScript)
//! - Schema versioning and backward compatibility
//! - Code generation for all supported languages
//!
//! ## Message Packages
//!
//! ### Core Types
//! - **`common.v1`**: Common types (Position, Timestamp, Uuid, Metadata)
//!
//! ### Entity Schemas
//! - **`capability.v1`**: Capability definitions and queries
//! - **`node.v1`**: Node configuration, state, operators, human-machine binding
//!
//! ### Organization Schemas
//! - **`cell.v1`**: Cell (squad) formation and management
//! - **`zone.v1`**: Zone (hierarchy) coordination and management
//! - **`role.v1`**: Tactical role assignments within cells
//!
//! ### Protocol Schemas
//! - **`beacon.v1`**: Discovery phase beacons and queries
//! - **`composition.v1`**: Capability composition rules (additive, emergent, redundant, constraint)
//! - **`model.v1`**: AI model deployment and distribution
//! - **`sensor.v1`**: Sensor specifications (mount types, orientation, FOV, gimbal state)
//! - **`actuator.v1`**: Actuator specifications (linear, rotary, gripper, barrier, winch)
//!
//! ## Three-Tier Hierarchy
//!
//! The HIVE Protocol implements a three-tier hierarchical structure:
//!
//! 1. **Nodes** (Individual platforms): UAVs, UGVs, soldier systems
//! 2. **Cells** (Tactical squads): Groups of 2-8 nodes with complementary capabilities
//! 3. **Zones** (Strategic coordination): Multiple cells coordinated by a zone commander
//!
//! ## Usage
//!
//! ```rust
//! use hive_schema::node::v1::{NodeConfig, NodeState, Phase, HealthStatus};
//! use hive_schema::capability::v1::{Capability, CapabilityType};
//!
//! // Create a node configuration
//! let config = NodeConfig {
//!     id: "node-1".to_string(),
//!     platform_type: "UAV".to_string(),
//!     capabilities: vec![],
//!     comm_range_m: 1000.0,
//!     max_speed_mps: 10.0,
//!     operator_binding: None,
//!     created_at: None,
//! };
//! ```

// Include generated protobuf code
pub mod cap {
    pub mod common {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.common.v1.rs"));
        }
    }

    pub mod capability {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.capability.v1.rs"));
        }
    }

    pub mod node {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.node.v1.rs"));
        }
    }

    pub mod cell {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.cell.v1.rs"));
        }
    }

    pub mod beacon {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.beacon.v1.rs"));
        }
    }

    pub mod composition {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.composition.v1.rs"));
        }
    }

    pub mod zone {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.zone.v1.rs"));
        }
    }

    pub mod role {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.role.v1.rs"));
        }
    }

    pub mod hierarchy {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.hierarchy.v1.rs"));
        }
    }

    pub mod command {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.command.v1.rs"));
        }
    }

    pub mod security {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.security.v1.rs"));
        }
    }

    pub mod track {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.track.v1.rs"));
        }
    }

    pub mod model {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.model.v1.rs"));
        }
    }

    pub mod sensor {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.sensor.v1.rs"));
        }
    }

    #[allow(clippy::enum_variant_names)]
    pub mod actuator {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/cap.actuator.v1.rs"));
        }
    }
}

// Re-export for convenience
pub use cap::*;

/// Validation utilities for schema types
pub mod validation;

/// Ontology vocabulary and semantic definitions
pub mod ontology;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_packages_accessible() {
        // Verify all proto packages compile and are accessible
        // This test ensures code generation worked correctly
        use capability::v1::CapabilityType;
        use common::v1::Position;
        use node::v1::Phase;

        // Create instances to verify types are accessible
        let _pos = Position {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
        };

        let _cap_type = CapabilityType::Sensor;
        let _phase = Phase::Discovery;

        // If we got here, all packages are accessible
        assert_eq!(CapabilityType::Sensor as i32, 1);
        assert_eq!(Phase::Discovery as i32, 1);
    }

    #[test]
    fn test_capability_advertisement_accessible() {
        // Verify CapabilityAdvertisement is accessible from capability.v1
        use capability::v1::{CapabilityAdvertisement, OperationalStatus, ResourceStatus};

        let _cap_ad = CapabilityAdvertisement {
            platform_id: "Alpha-3".to_string(),
            advertised_at: None,
            capabilities: vec![],
            resources: None,
            operational_status: OperationalStatus::Ready as i32,
        };

        let _resources = ResourceStatus {
            compute_utilization: 0.5,
            memory_utilization: 0.3,
            power_level: 0.9,
            storage_utilization: 0.2,
            bandwidth_utilization: 0.1,
            extra_json: String::new(),
        };

        assert_eq!(OperationalStatus::Ready as i32, 1);
    }

    #[test]
    fn test_track_types_accessible() {
        // Verify Track types are accessible from track.v1
        use track::v1::{
            SourceType, Track, TrackPosition, TrackSource, TrackState, TrackUpdate, UpdateType,
        };

        let _track = Track {
            track_id: "TRK-001".to_string(),
            classification: "person".to_string(),
            confidence: 0.95,
            position: Some(TrackPosition {
                latitude: 38.0,
                longitude: -122.0,
                altitude: 0.0,
                cep_m: 5.0,
                vertical_error_m: 0.0,
            }),
            velocity: None,
            state: TrackState::Confirmed as i32,
            source: Some(TrackSource {
                platform_id: "Alpha-3".to_string(),
                sensor_id: "camera-1".to_string(),
                model_version: "1.0.0".to_string(),
                source_type: SourceType::AiModel as i32,
            }),
            attributes_json: r#"{"color":"red"}"#.to_string(),
            first_seen: None,
            last_seen: None,
            observation_count: 5,
        };

        let _update = TrackUpdate {
            update_type: UpdateType::New as i32,
            track: Some(_track),
            previous_track_id: String::new(),
            timestamp: None,
        };

        assert_eq!(TrackState::Confirmed as i32, 2);
        assert_eq!(SourceType::AiModel as i32, 2);
        assert_eq!(UpdateType::New as i32, 1);
    }
}
