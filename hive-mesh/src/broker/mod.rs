//! HTTP/WS service broker for the mesh (ADR-049 Phase 6).
//!
//! Feature-gated under `"broker"`. Provides an Axum-based HTTP REST API
//! and WebSocket endpoint so external applications can observe and interact
//! with the mesh at runtime.

pub mod error;
pub mod routes;
pub mod server;
pub mod state;
pub mod ws;

pub use error::BrokerError;
pub use server::{Broker, BrokerConfig};
pub use state::{MeshBrokerState, MeshEvent, MeshNodeInfo, PeerSummary, TopologySummary};
