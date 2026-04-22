//! # CAP Persistence
//!
//! Storage abstraction layer for the Capability Aggregation Protocol (CAP).
//!
//! This crate provides a backend-agnostic interface for persisting and
//! querying Peat protocol data. It ships only the abstraction — the
//! `DataStore` trait, domain adapters, and an optional HTTP API — so that
//! concrete storage backends can be swapped without touching consumers.
//!
//! ## Architecture
//!
//! ```text
//! External System (C2 Dashboard, Analytics)
//!           ↓ HTTP/REST
//!   ┌──────────────────────┐
//!   │  peat-persistence    │
//!   │  (External API)      │
//!   └──────────────────────┘
//!           ↓ uses
//!   ┌──────────────────────┐
//!   │  DataStore Trait     │
//!   └──────────────────────┘
//!           ↓ implemented by
//!   ┌──────────────────────┐
//!   │  (consumer-supplied) │
//!   └──────────────────────┘
//! ```
//!
//! ## Features
//!
//! - **Backend Agnostic**: `DataStore` trait abstracts over any storage impl
//! - **Query Interface**: Filter, sort, and paginate CAP data
//! - **Live Updates**: Subscribe to real-time changes via observers
//! - **External API**: HTTP/REST endpoints for non-CAP systems
//! - **Type Safe**: Strongly typed queries and results
//!
//! ## REST API (optional `external-api` feature)
//!
//! Consumers pass an `Arc<dyn DataStore>` to `external::Server`:
//!
//! - `GET /api/v1/health` - Health check
//! - `GET /api/v1/collections/:name` - Query collection
//! - `GET /api/v1/collections/:name/:id` - Get specific document
//! - `WS /api/v1/collections/:name/subscribe` - Subscribe to changes
//!

pub mod adapters;
pub mod backends;
pub mod error;
pub mod store;
pub mod types;

// Re-export commonly used types
pub use adapters::PersistentBeaconStorage;
pub use error::{Error, Result};
pub use store::{ChangeEvent, DataStore, StoreInfo};
pub use types::{Document, DocumentId, MessagePriority, Query, SubscribeOptions, WriteOptions};

// External API (optional feature)
#[cfg(feature = "external-api")]
pub mod external;

#[cfg(feature = "external-api")]
pub use external::Server;

#[cfg(test)]
mod tests {
    #[test]
    fn test_crate_compiles() {
        // Sanity check that crate structure is valid
    }
}
