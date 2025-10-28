//! Differential update system
//!
//! Implements delta generation, application, and priority assignment.

pub mod generator;
pub mod applicator;
pub mod priority;
pub mod change_tracker;

// Re-exports will be added as modules are implemented
