//! # peat-registry
//!
//! OCI registry synchronization control plane for DDIL environments.
//!
//! Implements digest-level delta sync with checkpoint/resume, topology-aware routing,
//! and CRDT-synced convergence tracking between OCI registries (ADR-054).

pub mod config;
pub mod convergence;
pub mod delta;
pub mod error;
pub mod oci;
pub mod scheduler;
pub mod topology;
pub mod transfer;
pub mod types;
