//! Storage abstractions and implementations

pub mod ditto_store;
pub mod platform_store;
pub mod squad_store;

pub use ditto_store::DittoStore;
pub use platform_store::PlatformStore;
pub use squad_store::SquadStore;
