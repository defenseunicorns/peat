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
//!
//! ## Three-Tier Hierarchy
//!
//! The CAP Protocol implements a three-tier hierarchical structure:
//!
//! 1. **Nodes** (Individual platforms): UAVs, UGVs, soldier systems
//! 2. **Cells** (Tactical squads): Groups of 2-8 nodes with complementary capabilities
//! 3. **Zones** (Strategic coordination): Multiple cells coordinated by a zone commander
//!
//! ## Usage
//!
//! ```rust
//! use cap_schema::node::v1::{NodeConfig, NodeState, Phase, HealthStatus};
//! use cap_schema::capability::v1::{Capability, CapabilityType};
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
}
