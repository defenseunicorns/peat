//! Differential update system
//!
//! Implements delta generation, application, and priority assignment.

pub mod applicator;
pub mod change_tracker;
pub mod generator;
pub mod priority;

// Re-exports will be added as modules are implemented
