//! Storage backend implementations.
//!
//! No concrete backend lives here currently; `peat-persistence` ships only
//! the storage abstraction (`DataStore` trait and the beacon adapter). New
//! backends implementing `DataStore` should be added as sibling modules.
