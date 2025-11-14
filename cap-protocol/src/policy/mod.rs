//! Generic policy engine for conflict resolution and resource management
//!
//! This module provides a generic, trait-based policy engine that can be used
//! with any domain model (Commands, Nodes, Cells, Capabilities, etc.).
//!
//! ## Architecture
//!
//! - **Conflictable**: Trait that types implement to participate in conflict resolution
//! - **ResolutionPolicy**: Trait for defining custom conflict resolution strategies
//! - **GenericConflictResolver**: Generic resolver that works with any Conflictable type
//!
//! ## Example
//!
//! ```rust,ignore
//! use cap_protocol::policy::{Conflictable, GenericConflictResolver, LastWriteWinsPolicy};
//! use std::collections::HashMap;
//!
//! // Any type can implement Conflictable
//! struct MyType {
//!     my_id: String,
//!     resource: String,
//!     created_at: u64,
//! }
//!
//! impl Conflictable for MyType {
//!     fn id(&self) -> String { self.my_id.clone() }
//!     fn conflict_keys(&self) -> Vec<String> { vec![self.resource.clone()] }
//!     fn timestamp(&self) -> Option<u64> { Some(self.created_at) }
//!     fn attributes(&self) -> HashMap<String, AttributeValue> { HashMap::new() }
//! }
//!
//! // Use the generic resolver
//! let resolver = GenericConflictResolver::<MyType>::new();
//! let policy = LastWriteWinsPolicy;
//! ```

mod conflictable;
mod policies;
mod resolver;

pub use conflictable::{AttributeValue, ConflictResult, Conflictable};
pub use policies::{
    HighestAttributeWinsPolicy, LastWriteWinsPolicy, RejectConflictPolicy, ResolutionPolicy,
};
pub use resolver::GenericConflictResolver;
