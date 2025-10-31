//! Storage abstractions and implementations

pub mod cell_store;
pub mod ditto_store;
pub mod node_store;
pub mod throttled_node_store;

pub use cell_store::CellStore;
pub use ditto_store::DittoStore;
pub use node_store::NodeStore;
pub use throttled_node_store::{ThrottleStats, ThrottledNodeStore};

// Legacy compatibility aliases
pub use cell_store::CellStore as SquadStore;
pub use node_store::NodeStore as PlatformStore;
