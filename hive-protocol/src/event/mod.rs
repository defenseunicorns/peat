//! Event Routing and Aggregation (ADR-027)
//!
//! This module implements the event routing protocol for hierarchical event flow
//! and aggregation policies as specified in ADR-027.
//!
//! ## Overview
//!
//! Events flow upward through the HIVE hierarchy (platform → squad → platoon → company).
//! Each echelon applies aggregation policies to reduce bandwidth while preserving
//! critical information.
//!
//! ## Components
//!
//! - [`EventEmitter`]: Emits events with routing policies to priority queues
//! - [`PriorityEventQueue`]: 4-level priority queue for event transmission
//!
//! ## Event Flow
//!
//! ```text
//! Platform → EventEmitter → PriorityQueue → Parent Echelon
//!                               ↓
//!                         (CRITICAL preempts)
//!                         (HIGH/NORMAL/LOW weighted)
//! ```
//!
//! ## Propagation Modes
//!
//! - `Full`: Forward complete event upward immediately
//! - `Summary`: Aggregate events at echelon, forward summaries
//! - `Query`: Store locally, respond to queries from higher echelons
//! - `Local`: No propagation, local storage only

mod emitter;
mod priority_queue;

pub use emitter::EventEmitter;
pub use priority_queue::PriorityEventQueue;

// Re-export schema types for convenience
pub use hive_schema::event::v1::{
    AggregationPolicy, EventClass, EventPriority, EventQuery, EventQueryResponse, EventSummary,
    HiveEvent, PropagationMode,
};
