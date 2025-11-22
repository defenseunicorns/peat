//! Storage adapters for domain-specific traits
//!
//! This module provides adapter implementations that bridge domain-specific
//! storage traits (from other crates) to the generic DataStore trait.

pub mod beacon;

pub use beacon::PersistentBeaconStorage;
