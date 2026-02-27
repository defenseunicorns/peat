//! Discovery phase implementation (Phase 1)
//!
//! Implements constrained discovery strategies to form initial cells.

pub mod capability_query;
pub mod coordinator;
pub mod directed;
pub mod geo;
pub mod geographic;

// Peer discovery for Automerge+Iroh backend (ADR-011 Phase 3)
#[cfg(feature = "automerge-backend")]
pub mod peer;

// Re-exports
pub use geo::{GeoCoordinate, OperationalBox};
