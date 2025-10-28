//! Network simulation layer
//!
//! Implements simulated transport with bandwidth, latency, and loss constraints.

pub mod constraints;
pub mod metrics;
pub mod partition;
pub mod transport;

// Re-exports will be added as modules are implemented
