//! Hierarchical operations module (Phase 3)
//!
//! This module implements the hierarchical coordination layer for E5,
//! including zone management and hierarchical message routing.

pub mod routing_cache;

pub use routing_cache::{CacheStats, RoutingCache};
