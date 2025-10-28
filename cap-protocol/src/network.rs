//! Network simulation layer
//!
//! Implements simulated transport with bandwidth, latency, and loss constraints.

pub mod transport;
pub mod constraints;
pub mod partition;
pub mod metrics;

// Re-exports will be added as modules are implemented
