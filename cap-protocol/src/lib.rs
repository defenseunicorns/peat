//! # CAP Protocol - Capabilities Aggregation Protocol
//!
//! A hierarchical capability composition protocol using CRDTs for autonomous systems.
//!
//! ## Overview
//!
//! The CAP protocol enables scalable coordination of autonomous nodes through:
//! - **Three-phase protocol**: Discovery, Cell Formation, Hierarchical Operations
//! - **CRDT-based state**: Eventual consistency via Ditto SDK
//! - **Capability composition**: Additive, emergent, redundant, and constraint-based patterns
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
//! │   Phase 1:   │→ │   Phase 2:   │→ │   Phase 3:   │
//! │  Discovery   │  │    Cell      │  │ Hierarchical │
//! │              │  │  Formation   │  │  Operations  │
//! └──────────────┘  └──────────────┘  └──────────────┘
//! ```

pub mod cell;
pub mod composition;
pub mod discovery;
pub mod error;
pub mod hierarchy;
pub mod models;
pub mod network;
pub mod storage;
pub mod sync; // Data synchronization abstraction layer
pub mod testing;
pub mod traits;

pub use error::{Error, Result};

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default cell size (nodes per cell)
pub const DEFAULT_CELL_SIZE: usize = 5;

/// Default discovery timeout in seconds
pub const DEFAULT_DISCOVERY_TIMEOUT_SECS: u64 = 60;

/// Default hierarchy depth (node -> cell -> zone -> network)
pub const DEFAULT_HIERARCHY_DEPTH: usize = 4;
