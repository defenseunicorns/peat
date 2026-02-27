//! # Cursor-on-Target (CoT) Translation Layer
//!
//! This module provides bidirectional translation between PEAT messages and
//! CoT XML format for TAK (Team Awareness Kit) integration.
//!
//! ## Architecture (ADR-020, ADR-028)
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │  PEAT Messages  │ ──▶│   CoT Encoder   │ ──▶│    TAK/ATAK     │
//! │                 │    │                 │    │                 │
//! │  TrackUpdate    │    │  XML Generation │    │  Situational    │
//! │  Capability     │    │  _peat_ Ext     │    │  Awareness      │
//! │  Handoff        │    │  MIL-STD-2525   │    │                 │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Components
//!
//! - [`types`]: PEAT message types for TAK integration (TrackUpdate, etc.)
//! - [`event`]: CoT Event structure and XML encoding
//! - [`type_mapper`]: MIL-STD-2525 symbol type mappings
//! - [`peat_extension`]: `<_peat_>` custom detail extension schema
//! - [`encoder`]: PEAT → CoT message encoding

pub mod encoder;
pub mod event;
pub mod peat_extension;
pub mod type_mapper;
pub mod types;

// Re-export main types
pub use encoder::CotEncoder;
pub use event::{CotDetail, CotEvent, CotEventBuilder, CotLink, CotPoint, CotTrack};
pub use peat_extension::{PeatConfidence, PeatExtension, PeatHierarchy, PeatSource, PeatStatus};
pub use type_mapper::{Affiliation, CotType, CotTypeMapper, EntityClassification};
pub use types::{
    CapabilityAdvertisement, FormationCapabilitySummary, HandoffMessage, HandoffState,
    MissionBoundary, MissionPriority, MissionTarget, MissionTask, MissionTaskType,
    OperationalStatus, Position, TrackUpdate, Velocity,
};
