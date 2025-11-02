//! Capability composition engine
//!
//! This module provides the composition framework for detecting emergent
//! capabilities and constraints from sets of individual capabilities.
//!
//! ## Composition Patterns
//!
//! - **Additive**: Capabilities that sum (coverage area, lift capacity)
//! - **Emergent**: New capabilities from combinations (ISR chains, mapping)
//! - **Redundant**: Reliability from redundancy (detection, coverage)
//! - **Constraint**: Team limits from individual constraints (speed, range)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use cap_protocol::composition::{CompositionEngine, CompositionContext};
//! use cap_protocol::models::capability::Capability;
//!
//! # async fn example() -> cap_protocol::Result<()> {
//! let mut engine = CompositionEngine::new();
//! // Register rules here
//!
//! let capabilities = vec![/* capabilities */];
//! let context = CompositionContext::new(vec!["node1".to_string()]);
//!
//! let results = engine.compose(&capabilities, &context).await?;
//! # Ok(())
//! # }
//! ```

pub mod engine;
pub mod rules;

pub use engine::CompositionEngine;
pub use rules::{CompositionContext, CompositionResult, CompositionRule};
