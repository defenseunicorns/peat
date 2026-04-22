//! Transport abstraction for mesh topology connections
//!
//! Core transport types and traits are defined in [`peat_mesh::transport`] and
//! re-exported here for backwards compatibility. Backend-specific implementations
//! (Iroh) remain in this crate.

// Re-export everything from peat-mesh's transport module
pub use peat_mesh::transport::*;

// Backend implementations that remain in peat-protocol
#[cfg(feature = "automerge-backend")]
pub mod iroh;
