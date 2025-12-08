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
pub mod orchestration;
pub mod platform;
pub mod registry;
pub mod schema;
pub mod sync;
pub mod team;
pub mod testing;
pub mod ugv_client;

// Platform types and traits
pub use coordinator::{
    Coordinator, DegradedModelInfo, FormationCapabilitySummary, ModelInventorySummary,
    ModelPerformanceStats, ModelQueryResult, PlatformModelMatch,
};
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
    ChipoutExtractor, Detection, Detector, InferenceHarness, InferencePipeline, PipelineConfig,
    Scenario, SimulatedDetector, SimulatedTracker, Track, Tracker, VideoFrame,
};

// HIVE sync integration
pub use sync::{
    ConnectedPipeline, ConnectedPipelineWithChipouts, HiveSyncClient, PipelineOutputWithChipouts,
    SyncConfig, SyncStats,
};

// HIVE beacon (edge device registration)
pub use beacon::{BeaconConfig, CameraSpec, ComputeSpec, HiveBeacon, ModelSpec};

// Model registry (Issue #107 EPIC 4)
pub use registry::{
    ModelEvent, ModelEventType, ModelQuery, ModelRegistry, PerformanceBaseline, RegisteredModel,
    RegistryError,
};

// Message types (expanded for Issue #107)
pub use messages::{OperationalStatus, ResourceRequirements};

// Chipout types for detection-triggered image extraction (Issue #321)
pub use messages::{
    ChipoutConfig, ChipoutDetection, ChipoutDocument, ChipoutImage, ChipoutTrigger, ImageFormat,
};

// Model update orchestration (Issue #177 / ADR-026)
pub use orchestration::{
    RolloutConfig, RolloutPlan, RolloutResult, UpdateCoordinator, UpdateError, UpdateRequest,
};

// Model fetcher for downloading models from URLs/blobs (Issue #319)
pub use orchestration::{FetchConfig, FetchError, FetchProgress, FetchResult, ModelFetcher};

// Re-export hive-protocol capability query types for convenience
pub use hive_protocol::discovery::capability_query::{
    CapabilityQuery, CapabilityQueryBuilder, CapabilityQueryEngine, CapabilityStats, QueryMatch,
};
pub use hive_protocol::models::CapabilityType;

// Schema conversion traits for proto interoperability (Issue #299)
pub use schema::{
    DecodeProto, EncodeProto, FromProtoCapability, FromProtoTrack, ToProtoCapability, ToProtoTrack,
};
// Re-export hive-schema proto types
pub use hive_schema::capability::v1 as proto_capability;
pub use hive_schema::track::v1 as proto_track;

// Simulated UGV client for demo (Issue #331)
pub use ugv_client::{
    MissionCommand, MovementMode, PatrolPattern, UgvClient, UgvConfig, UgvState,
};
