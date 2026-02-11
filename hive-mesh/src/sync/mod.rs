//! Data synchronization abstraction layer
//!
//! This module provides a unified interface for CRDT-based data synchronization,
//! enabling the mesh to work with multiple sync engines without changing
//! business logic.
//!
//! ## Core Traits
//!
//! 1. **`DocumentStore`** - CRUD operations, queries, and live observers
//! 2. **`PeerDiscovery`** - Finding and connecting to other nodes
//! 3. **`SyncEngine`** - Controlling synchronization behavior
//! 4. **`DataSyncBackend`** - Lifecycle management and trait composition

pub mod traits;
pub mod types;

// Re-export core types and traits for convenience
pub use traits::*;
pub use types::*;
