//! # CAP Protocol - Capabilities Aggregation Protocol
//!
//! A hierarchical capability composition protocol using CRDTs for autonomous systems.
//!
//! ## Overview
//!
//! The CAP protocol enables scalable coordination of autonomous platforms through:
//! - **Three-phase protocol**: Bootstrap, Squad Formation, Hierarchical Operations
//! - **CRDT-based state**: Eventual consistency via Ditto SDK
//! - **Capability composition**: Additive, emergent, redundant, and constraint-based patterns
//! - **Differential updates**: Bandwidth-efficient delta propagation
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
//! │   Phase 1:   │→ │   Phase 2:   │→ │   Phase 3:   │
//! │  Bootstrap   │  │    Squad     │  │ Hierarchical │
//! │              │  │  Formation   │  │  Operations  │
//! └──────────────┘  └──────────────┘  └──────────────┘
//! ```

pub mod bootstrap;
pub mod composition;
pub mod delta;
pub mod error;
pub mod hierarchy;
pub mod models;
pub mod network;
pub mod squad;
pub mod storage;
pub mod testing;
pub mod traits;

pub use error::{Error, Result};

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default squad size (platforms per squad)
pub const DEFAULT_SQUAD_SIZE: usize = 5;

/// Default bootstrap timeout in seconds
pub const DEFAULT_BOOTSTRAP_TIMEOUT_SECS: u64 = 60;

/// Default hierarchy depth (platform -> squad -> platoon -> company)
pub const DEFAULT_HIERARCHY_DEPTH: usize = 4;
