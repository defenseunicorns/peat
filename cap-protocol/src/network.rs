//! Network layer for CAP Protocol
//!
//! This module provides both:
//! - Network simulation (bandwidth, latency, loss constraints)
//! - Real P2P transport via Iroh (for AutomergeIrohBackend)

// Network simulation modules
pub mod constraints;
pub mod metrics;
pub mod partition;
pub mod transport;

// Real P2P transport (Phase 3: Iroh integration)
#[cfg(feature = "automerge-backend")]
pub mod iroh_transport;

// Re-exports
#[cfg(feature = "automerge-backend")]
pub use iroh_transport::IrohTransport;
