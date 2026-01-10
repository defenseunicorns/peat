//! # CAP Persistence
//!
//! Storage abstraction layer for the Capability Aggregation Protocol (CAP).
//!
//! This crate provides a backend-agnostic interface for persisting and querying
//! HIVE protocol data, enabling external systems to access CAP state without
//! coupling to specific storage implementations.
//!
//! ## Architecture
//!
//! ```text
//! External System (C2 Dashboard, Analytics)
//!           ↓ HTTP/REST
//!   ┌──────────────────────┐
//!   │  cap-persistence     │
//!   │  (External API)      │
//!   └──────────────────────┘
//!           ↓ uses
//!   ┌──────────────────────┐
//!   │  DataStore Trait     │
//!   └──────────────────────┘
//!           ↓ implemented by
//!   ┌──────────────────────┐
//!   │  Storage Backends    │
//!   │  • Ditto             │
//!   │  • Automerge (planned)│
//!   │  • SQLite (testing)  │
//!   └──────────────────────┘
//! ```
//!
//! ## Features
//!
//! - **Backend Agnostic**: Works with Ditto, Automerge, or any CRDT backend
//! - **Query Interface**: Filter, sort, and paginate CAP data
//! - **Live Updates**: Subscribe to real-time changes via observers
//! - **External API**: HTTP/REST endpoints for non-CAP systems
//! - **Type Safe**: Strongly typed queries and results
//!
//! ## Usage
//!
//! ```rust,no_run
//! use hive_persistence::{DataStore, Query};
//! use hive_persistence::backends::DittoStore;
//! use hive_protocol::sync::ditto::DittoBackend;
//! use hive_protocol::sync::DataSyncBackend;
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize backend
//! let backend = Arc::new(DittoBackend::new());
//! // backend.initialize(config).await?;
//!
//! // Create store
//! let store = DittoStore::new(backend);
//!
//! // Save data
//! let node = json!({
//!     "node_id": "node-1",
//!     "phase": "discovery"
//! });
//! let id = store.save("node_states", &node).await?;
//!
//! // Query data
//! let nodes = store.query("node_states", Query::all()).await?;
//! println!("Found {} nodes", nodes.len());
//!
//! // Subscribe to changes
//! let mut stream = store.observe("node_states", Query::all()).await?;
//! while let Some(event) = stream.recv().await {
//!     println!("Change: {:?}", event);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## REST API
//!
//! The `external-api` feature provides HTTP endpoints for querying CAP data:
//!
//! ```rust,no_run
//! use hive_persistence::external::Server;
//! use hive_persistence::backends::DittoStore;
//! use hive_protocol::sync::ditto::DittoBackend;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = Arc::new(DittoBackend::new());
//! let store = Arc::new(DittoStore::new(backend));
//!
//! let server = Server::new(store)
//!     .bind("0.0.0.0:8080")
//!     .await?;
//!
//! server.serve().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## REST Endpoints
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
