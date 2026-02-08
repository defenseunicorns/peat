//! # CAP Transport
//!
//! External API transport layer for the Capability Aggregation Protocol (CAP).
//!
//! This crate provides HTTP/REST API access to CAP node state, enabling external
//! systems (C2 dashboards, legacy systems, monitoring tools) to query and interact
//! with the CAP mesh network.
//!
//! ## Architecture
//!
//! ```text
//! External System (C2 Dashboard, ROS2, etc.)
//!           ↓ HTTP/REST
//!   ┌──────────────────────┐
//!   │  cap-transport       │
//!   │  (HTTP Server)       │
//!   └──────────────────────┘
//!           ↓ queries
//!   ┌──────────────────────┐
//!   │  cap-protocol        │
//!   │  (DataSyncBackend)   │
//!   └──────────────────────┘
//!           ↓ stores in
//!   ┌──────────────────────┐
//!   │  Ditto / Automerge   │
//!   │  (Sync Backend)      │
//!   └──────────────────────┘
//! ```
//!
//! ## Features
//!
//! - **HTTP/REST API**: Query nodes, cells, and beacons via REST endpoints
//! - **Read-only**: External systems can query state but not mutate (safety)
//! - **Backend agnostic**: Works with Ditto or Automerge+Iroh sync backends
//! - **JSON responses**: Uses cap-schema protobuf → JSON encoding
//! - **Extensible**: Trait-based design for future WebSocket/gRPC support
//!
//! ## Usage
//!
//! ```rust,no_run
//! use hive_transport::http::Server;
//! use hive_protocol::sync::ditto::DittoBackend;
//! use hive_protocol::sync::DataSyncBackend;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize sync backend (Ditto)
//! let backend = Arc::new(DittoBackend::new());
//! // backend.initialize(config).await?;
//!
//! // Start HTTP server
//! let server = Server::new(backend)
//!     .bind("0.0.0.0:8080")
//!     .await?;
//!
//! println!("REST API listening on http://0.0.0.0:8080");
//! server.serve().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## REST API Endpoints
//!
//! - `GET /api/v1/health` - API health check
//! - `GET /api/v1/nodes` - List all nodes
//! - `GET /api/v1/nodes/{id}` - Get specific node
//! - `GET /api/v1/cells` - List all cells
//! - `GET /api/v1/cells/{id}` - Get specific cell
//! - `GET /api/v1/beacons` - Query beacons (with filters)
//!
//! ## Query Parameters
//!
//! Endpoints support filtering via query parameters:
//!
//! ```text
//! GET /api/v1/nodes?phase=cell&health=nominal
//! GET /api/v1/beacons?geohash_prefix=9q8yy
//! GET /api/v1/cells?leader_id=node-1
//! ```

pub mod error;
pub mod http;
pub mod tak;
pub mod ws;

// Re-export commonly used types
pub use error::{Error, Result};
pub use http::Server;

// Re-export TAK transport types
pub use tak::{
    MeshSaTransport, TakError, TakMessageQueue, TakMetrics, TakServerTransport, TakTransport,
    TakTransportConfig, TakTransportMode,
};

// Re-export WebSocket viewer protocol types
pub use ws::{try_into_viewer_event, IngestError, ViewerEvent};

#[cfg(test)]
mod tests {
    #[test]
    fn test_crate_compiles() {
        // Sanity check that crate structure is valid
        // If this compiles, the test passes
    }
}
