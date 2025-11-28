//! # TAK Transport Adapter
//!
//! Provides bidirectional CoT message transport between HIVE and TAK ecosystem.
//! Supports TAK Server (TCP/SSL) and Mesh SA (UDP multicast) modes.
//!
//! ## Architecture (ADR-029)
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │  HIVE Protocol  │───▶│  TakTransport   │───▶│  TAK Server/    │
//! │                 │    │                 │    │  Mesh SA        │
//! │  CotEncoder     │    │  DIL Queue      │    │                 │
//! │  CotEvent       │    │  Reconnection   │    │  ATAK/WinTAK    │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Features
//!
//! - **TAK Server Mode**: TCP/SSL connection to TAK Server (ports 8087/8089)
//! - **Mesh SA Mode**: UDP multicast for local SA sharing
//! - **DIL Resilience**: Priority-aware message queuing during disconnections
//! - **Protobuf Support**: TAK Protocol v1 for 3-5x bandwidth reduction
//! - **Certificate Auth**: Client certificate authentication for TAK Server

mod config;
mod error;
mod metrics;
mod queue;
mod reconnect;
mod traits;

pub mod mesh;
pub mod server;

// Re-export main types
pub use config::{
    PriorityQueueLimits, ProtocolConfig, QueueConfig, ReconnectPolicy, TakCredentials, TakIdentity,
    TakProtocolVersion, TakTransportConfig, TakTransportMode, XmlEncodingOptions,
};
pub use error::TakError;
pub use metrics::{QueueDepthMetrics, TakMetrics};
pub use queue::TakMessageQueue;
pub use reconnect::ReconnectionManager;
pub use traits::{CotEventStream, CotFilter, TakTransport};

pub use mesh::MeshSaTransport;
pub use server::TakServerTransport;
