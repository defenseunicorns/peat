//! Quality of Service primitives for mesh synchronization
//!
//! This module provides deletion policies (ADR-034) and sync mode configuration
//! (ADR-019 Amendment) that are generic across all HIVE mesh implementations.

pub mod deletion;
pub mod sync_mode;

// Re-export key types
pub use deletion::{
    DeleteResult, DeletionPolicy, DeletionPolicyRegistry, PropagationDirection, Tombstone,
    TombstoneBatch, TombstoneDecodeError, TombstoneSyncMessage,
};
pub use sync_mode::{SyncMode, SyncModeRegistry};
