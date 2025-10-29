//! Bootstrap phase implementation (Phase 1)
//!
//! Implements constrained discovery strategies to form initial squads.

pub mod capability_query;
pub mod coordinator;
pub mod directed;
pub mod geo;
pub mod geographic;

// Re-exports
pub use geo::{GeoCoordinate, OperationalBox};
