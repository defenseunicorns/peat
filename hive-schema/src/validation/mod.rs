//! Schema validation utilities
//!
//! This module provides validation functions for HIVE Protocol messages to ensure:
//! - Confidence scores are within valid range (0.0 - 1.0)
//! - Required fields are present
//! - Semantic constraints are satisfied
//! - CRDT invariants are maintained

mod actuator;
mod capability;
mod command;
mod core;
mod effector;
mod model;
mod product;
mod sensor;
mod tasking;
mod track;

/// Validation error types
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Invalid confidence score: {0} (must be between 0.0 and 1.0)")]
    InvalidConfidence(f32),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field value: {0}")]
    InvalidValue(String),

    #[error("Semantic constraint violated: {0}")]
    ConstraintViolation(String),
}

pub type ValidationResult<T> = Result<T, ValidationError>;

// Re-export all validators
pub use actuator::*;
pub use capability::*;
pub use command::*;
pub use core::*;
pub use effector::*;
pub use model::*;
pub use product::*;
pub use sensor::*;
pub use tasking::*;
pub use track::*;
