//! Event Routing and Aggregation (ADR-027)
//!
//! This module implements the event routing protocol for hierarchical event flow
//! and aggregation policies as specified in ADR-027.
//!
//! ## Overview
//!
//! Events flow upward through the Peat hierarchy (platform → squad → platoon → company).
//! Each echelon applies aggregation policies to reduce bandwidth while preserving
//! critical information.
//!
//! ## Components
//!
//! - [`EventEmitter`]: Emits events with routing policies to priority queues (Phase 1)
//! - [`PriorityEventQueue`]: 4-level priority queue for event transmission (Phase 1)
//! - [`EchelonAggregator`]: Aggregates events at echelon boundaries (Phase 2)
//! - [`SummaryStrategy`]: Trait for type-specific summarization (Phase 2)
//! - [`EventQueryHandler`]: Handles queries for locally stored events (Phase 3)
//! - [`EventStore`]: Trait for event storage backends (Phase 3)
//! - [`EventTransmitter`]: Bandwidth-controlled event transmission (Phase 4)
//! - [`BandwidthAllocation`]: Bandwidth allocation configuration (Phase 4)
//!
//! ## Event Flow
//!
//! ```text
//! Platform → EventEmitter → PriorityQueue → EchelonAggregator → Parent Echelon
//!                               ↓                  ↓
//!                         (CRITICAL preempts) (Summary/Full/Query)
//!                         (HIGH/NORMAL/LOW weighted)
//!
//! Parent Echelon ──EventQuery──> EventQueryHandler ──> EventStore
//!        ↑                                               ↓
//!        └────────────EventQueryResponse────────────────┘
//! ```
//!
//! ## Propagation Modes
//!
//! - `Full`: Forward complete event upward immediately
//! - `Summary`: Aggregate events at echelon, forward summaries
//! - `Query`: Store locally, respond to queries from higher echelons
//! - `Local`: No propagation, local storage only
//!
//! ## Example
//!
//! ```ignore
//! use peat_protocol::event::{EventEmitter, EchelonAggregator, EchelonType, EventQueryHandler};
//!
//! // Platform emits events
//! let emitter = EventEmitter::new("platform-1".to_string(), "squad-1".to_string());
//! emitter.emit_product("detection", payload, PropagationMode::PropagationSummary, EventPriority::PriorityNormal)?;
//!
//! // Squad leader aggregates events from platforms
//! let aggregator = EchelonAggregator::new("squad-1".to_string(), EchelonType::Squad);
//! for event in platform_events {
//!     aggregator.receive(event)?;
//! }
//!
//! // Periodically flush windows and forward summaries
//! aggregator.flush_expired_windows();
//! let events_to_parent = aggregator.pop_all();
//!
//! // Query handler for locally stored events
//! let query_handler = EventQueryHandler::with_memory_store("squad-1".to_string(), "squad-1".to_string());
//! query_handler.store_event(event);
//! let response = query_handler.query_local(&query);
//! ```

mod aggregator;
mod emitter;
mod priority_queue;
mod query;
mod summary;
mod transmitter;

pub use aggregator::{AggregationWindow, EchelonAggregator, EchelonType};
pub use emitter::EventEmitter;
pub use priority_queue::PriorityEventQueue;
pub use query::{create_filters, EventQueryHandler, EventStore, InMemoryEventStore, QueryResult};
pub use summary::{
    AnomalySummary, AnomalySummaryStrategy, DefaultSummaryStrategy, DetectionSummary,
    DetectionSummaryStrategy, MetricStats, MetricSummaryStats, SummaryStrategy, TelemetrySummary,
    TelemetrySummaryStrategy,
};
pub use transmitter::{BandwidthAllocation, EventTransmitter, OverflowPolicy, TransmitterStats};

// Re-export schema types for convenience
pub use peat_schema::event::v1::{
    AggregationPolicy, EventClass, EventFilters, EventPriority, EventQuery, EventQueryResponse,
    EventSummary, PeatEvent, PropagationMode, QueryScope,
};
