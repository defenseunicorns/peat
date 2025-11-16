//! Core trait definitions for the HIVE protocol

use crate::Result;
use async_trait::async_trait;
use std::fmt::Debug;

// Re-export Phase from cap_schema
pub use hive_schema::node::v1::Phase;

/// Extension trait for Phase enum
pub trait PhaseExt {
    /// Get lowercase string representation
    fn as_str(&self) -> &'static str;

    /// Legacy compatibility constants
    const BOOTSTRAP: Phase;
    const SQUAD: Phase;
    const HIERARCHICAL: Phase;
}

impl PhaseExt for Phase {
    fn as_str(&self) -> &'static str {
        match self {
            Phase::Unspecified => "unspecified",
            Phase::Discovery => "discovery",
            Phase::Cell => "cell",
            Phase::Hierarchy => "hierarchical",
        }
    }

    // Legacy compatibility - old names as aliases
    const BOOTSTRAP: Phase = Phase::Discovery;
    const SQUAD: Phase = Phase::Cell;
    const HIERARCHICAL: Phase = Phase::Hierarchy;
}

/// Node lifecycle management
#[async_trait]
pub trait Platform: Send + Sync + Debug {
    /// Initialize the platform with configuration
    async fn initialize(&mut self) -> Result<()>;

    /// Update node state (called at regular intervals)
    async fn update(&mut self) -> Result<()>;

    /// Get the current phase
    fn phase(&self) -> Phase;

    /// Transition to a new phase
    async fn transition_to(&mut self, phase: Phase) -> Result<()>;

    /// Shutdown the platform gracefully
    async fn shutdown(&mut self) -> Result<()>;
}

/// Capability provider trait
#[async_trait]
pub trait CapabilityProvider: Send + Sync + Debug {
    /// Get the platform's static capabilities
    fn static_capabilities(&self) -> Vec<String>;

    /// Get the platform's dynamic capabilities (may change over time)
    fn dynamic_capabilities(&self) -> Vec<String>;

    /// Check if the platform has a specific capability
    fn has_capability(&self, capability: &str) -> bool;

    /// Get confidence score for a capability (0.0 - 1.0)
    fn capability_confidence(&self, capability: &str) -> f32;
}

/// Message routing trait
#[async_trait]
pub trait MessageRouter: Send + Sync + Debug {
    /// Route a message according to hierarchical rules
    async fn route(&mut self, message: Vec<u8>) -> Result<()>;

    /// Check if a route is valid for the current phase
    fn is_route_valid(&self, from: &str, to: &str) -> bool;

    /// Get valid routing targets for this platform
    fn valid_targets(&self) -> Vec<String>;
}

/// Phase transition logic
#[async_trait]
pub trait PhaseTransition: Send + Sync + Debug {
    /// Check if the platform can transition to a new phase
    fn can_transition_to(&self, phase: Phase) -> bool;

    /// Perform the phase transition
    async fn perform_transition(&mut self, from: Phase, to: Phase) -> Result<()>;

    /// Get transition completion percentage (0.0 - 1.0)
    fn transition_progress(&self) -> f32;
}

/// Storage abstraction for CRDT operations
#[async_trait]
pub trait Storage: Send + Sync + Debug {
    /// Store a value with a key
    async fn store(&mut self, key: &str, value: serde_json::Value) -> Result<()>;

    /// Retrieve a value by key
    async fn retrieve(&self, key: &str) -> Result<Option<serde_json::Value>>;

    /// Delete a value by key
    async fn delete(&mut self, key: &str) -> Result<()>;

    /// Query values matching a predicate
    async fn query(&self, query: &str) -> Result<Vec<serde_json::Value>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_as_str() {
        assert_eq!(Phase::Unspecified.as_str(), "unspecified");
        assert_eq!(Phase::Discovery.as_str(), "discovery");
        assert_eq!(Phase::Cell.as_str(), "cell");
        assert_eq!(Phase::Hierarchy.as_str(), "hierarchical");
    }

    #[test]
    fn test_phase_legacy_constants() {
        assert_eq!(Phase::BOOTSTRAP, Phase::Discovery);
        assert_eq!(Phase::SQUAD, Phase::Cell);
        assert_eq!(Phase::HIERARCHICAL, Phase::Hierarchy);
    }

    #[test]
    fn test_phase_as_str_all_variants() {
        // Ensure all Phase variants have string representations
        let phases = vec![
            Phase::Unspecified,
            Phase::Discovery,
            Phase::Cell,
            Phase::Hierarchy,
        ];

        for phase in phases {
            let s = phase.as_str();
            assert!(!s.is_empty());
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn test_phase_enum_values() {
        // Test that Phase enum can be converted to/from i32
        assert_eq!(Phase::Unspecified as i32, 0);
        assert_eq!(Phase::Discovery as i32, 1);
        assert_eq!(Phase::Cell as i32, 2);
        assert_eq!(Phase::Hierarchy as i32, 3);
    }

    #[test]
    fn test_phase_pattern_matching() {
        let phase = Phase::Discovery;
        match phase {
            Phase::Unspecified => panic!("Wrong phase"),
            Phase::Discovery => {} // Expected
            Phase::Cell => panic!("Wrong phase"),
            Phase::Hierarchy => panic!("Wrong phase"),
        }
    }

    #[test]
    fn test_phase_equality() {
        assert_eq!(Phase::Discovery, Phase::Discovery);
        assert_ne!(Phase::Discovery, Phase::Cell);
        assert_eq!(Phase::BOOTSTRAP, Phase::Discovery); // Legacy constant
    }
}
