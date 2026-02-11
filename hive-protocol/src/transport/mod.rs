//! Transport abstraction for mesh topology connections
//!
//! Core transport types and traits are defined in [`hive_mesh::transport`] and
//! re-exported here for backwards compatibility. Backend-specific implementations
//! (Iroh, Ditto) remain in this crate.

// Re-export everything from hive-mesh's transport module
pub use hive_mesh::transport::*;

// Backend implementations that remain in hive-protocol
#[cfg(feature = "automerge-backend")]
pub mod iroh;

#[cfg(feature = "ditto-backend")]
pub mod ditto;
