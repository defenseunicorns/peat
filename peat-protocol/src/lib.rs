//! # Peat Protocol - Hierarchical Intelligence for Versatile Entities
//!
//! A hierarchical capability composition protocol using CRDTs for autonomous systems.
//!
//! ## Overview
//!
//! The Peat protocol enables scalable coordination of autonomous nodes through:
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
pub mod command; // Bidirectional command coordination
pub mod composition;
pub mod cot; // Cursor-on-Target translation layer (ADR-020, ADR-028)
pub mod credentials; // Backend-agnostic credential management
pub mod discovery;
pub mod distribution; // AI model distribution (manifests, updates, requirements)
pub mod error;
pub mod event; // Event routing and aggregation (ADR-027)
pub mod ffi; // FFI bindings for ATAK and other native consumers (Issue #258)
pub mod hierarchy;
pub mod mesh_integration;
pub mod models;
pub mod network;
pub mod policy; // Generic policy engine for conflict resolution
pub mod qos; // Quality of Service framework (ADR-019)
pub mod security; // Device authentication and PKI (ADR-006)
pub mod storage;
pub mod sync; // Data synchronization abstraction layer
pub mod testing;
pub mod traits;
pub mod transport; // Backend-agnostic transport abstraction for mesh topology // PEAT-specific adapters for peat-mesh

pub use error::{Error, Result};

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default cell size (nodes per cell)
pub const DEFAULT_CELL_SIZE: usize = 5;

/// Default discovery timeout in seconds
pub const DEFAULT_DISCOVERY_TIMEOUT_SECS: u64 = 60;

/// Default hierarchy depth (node -> cell -> zone -> network)
pub const DEFAULT_HIERARCHY_DEPTH: usize = 4;
