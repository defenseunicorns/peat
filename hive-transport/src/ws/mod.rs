//! WebSocket viewer relay protocol
//!
//! This module defines the viewer event types and ingest logic for converting
//! [`HiveEvent`](hive_schema::event::v1::HiveEvent) messages into typed
//! [`ViewerEvent`] variants for frontend consumption over WebSocket connections.

pub mod ingest;
pub mod protocol;

pub use ingest::{try_into_viewer_event, IngestError};
pub use protocol::ViewerEvent;
