//! Storage backend trait abstractions
//!
//! Defines the core traits for the mesh storage layer, enabling runtime
//! backend selection between various storage implementations.

pub mod traits;

// Re-export key types
pub use traits::{Collection, DocumentPredicate, StorageBackend};
