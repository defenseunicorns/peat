//! Topology formation module
//!
//! This module provides beacon-driven topology formation capabilities,
//! including parent selection algorithms and topology state management.

mod builder;
mod selection;

pub use builder::{ParentInfo, TopologyBuilder, TopologyConfig, TopologyEvent, TopologyState};
pub use selection::{ParentCandidate, ParentSelector, SelectionConfig};
