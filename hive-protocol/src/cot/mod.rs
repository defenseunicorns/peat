//! # Cursor-on-Target (CoT) Translation Layer
//!
//! This module provides bidirectional translation between HIVE messages and
//! CoT XML format for TAK (Team Awareness Kit) integration.
//!
//! ## Architecture (ADR-020, ADR-028)
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │  HIVE Messages  │ ──▶│   CoT Encoder   │ ──▶│    TAK/ATAK     │
//! │                 │    │                 │    │                 │
//! │  TrackUpdate    │    │  XML Generation │    │  Situational    │
//! │  Capability     │    │  _hive_ Ext     │    │  Awareness      │
//! │  Handoff        │    │  MIL-STD-2525   │    │                 │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Components
//!
//! - [`types`]: HIVE message types for TAK integration (TrackUpdate, etc.)
//! - [`event`]: CoT Event structure and XML encoding
//! - [`type_mapper`]: MIL-STD-2525 symbol type mappings
//! - [`hive_extension`]: `<_hive_>` custom detail extension schema
//! - [`encoder`]: HIVE → CoT message encoding

pub mod encoder;
pub mod event;
pub mod hive_extension;
pub mod type_mapper;
pub mod types;

// Re-export main types
pub use encoder::CotEncoder;
pub use event::{CotDetail, CotEvent, CotEventBuilder, CotLink, CotPoint, CotTrack};
pub use hive_extension::{HiveConfidence, HiveExtension, HiveHierarchy, HiveSource, HiveStatus};
pub use type_mapper::{Affiliation, CotType, CotTypeMapper, EntityClassification};
pub use types::{
    CapabilityAdvertisement, FormationCapabilitySummary, HandoffMessage, HandoffState,
    MissionBoundary, MissionPriority, MissionTarget, MissionTask, MissionTaskType,
    OperationalStatus, Position, TrackUpdate, Velocity,
};
