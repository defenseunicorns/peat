// Allow some clippy lints for ported code - will clean up incrementally
#![allow(clippy::obfuscated_if_else)]
#![allow(clippy::wrong_self_convention)]
#![allow(clippy::enum_variant_names)]
#![allow(dead_code)]

//! HIVE M1 POC - Object Tracking Across Distributed Human-Machine-AI Teams
//!
//! This crate implements the M1 vignette demonstrating:
//! - Capability-based operations with upward aggregation
//! - Bidirectional hierarchical flows (tracks up, models down)
//! - Cross-network coordination via bridge nodes
//! - TAK integration through HIVE-TAK Bridge
//!
//! ## Inference Pipeline
//!
//! The `inference` module provides the AI inference pipeline:
//! - Object detection (simulated or ONNX Runtime on Jetson)
//! - Multi-object tracking (DeepSORT-style)
//! - TrackUpdate message generation
//! - Performance metrics collection
//!
//! ## HIVE Sync Integration
//!
//! The `sync` module connects inference to HIVE protocol:
//! - Publish TrackUpdates via Automerge CRDT
//! - Capability advertisement
//! - P2P sync across formations

pub mod beacon;
pub mod bridge;
pub mod coordinator;
pub mod inference;
pub mod messages;
pub mod models;
pub mod platform;
pub mod sync;
pub mod team;
pub mod testing;

// Platform types and traits
pub use coordinator::{Coordinator, FormationCapabilitySummary};
pub use platform::{
    AiModelInfo, AiModelPlatform, AuthorityLevel, CapabilityProvider, OperatorPlatform, Platform,
    PlatformType, SensorCapability, VehiclePlatform,
};
pub use team::{
    AiModelSummary, M1CriteriaResult, Team, TeamCapabilitySummary, TeamFormation,
    TeamFormationStatus,
};

// Inference pipeline
pub use inference::{
    Detection, Detector, InferenceHarness, InferencePipeline, PipelineConfig, Scenario,
    SimulatedDetector, SimulatedTracker, Track, Tracker, VideoFrame,
};

// HIVE sync integration
pub use sync::{ConnectedPipeline, HiveSyncClient, SyncConfig, SyncStats};

// HIVE beacon (edge device registration)
pub use beacon::{BeaconConfig, CameraSpec, ComputeSpec, HiveBeacon, ModelSpec};
