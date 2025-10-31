//! Core trait definitions for the CAP protocol

use crate::Result;
use async_trait::async_trait;
use std::fmt::Debug;

/// Represents a protocol phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    /// Initial discovery and group formation
    Discovery,
    /// Cell cohesion and leader election
    Cell,
    /// Hierarchical operations with constrained messaging
    Hierarchical,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::Discovery => write!(f, "discovery"),
            Phase::Cell => write!(f, "cell"),
            Phase::Hierarchical => write!(f, "hierarchical"),
        }
    }
}

// Legacy compatibility - old names as aliases
impl Phase {
    pub const BOOTSTRAP: Phase = Phase::Discovery;
    pub const SQUAD: Phase = Phase::Cell;
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
