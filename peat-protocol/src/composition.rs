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
//! use peat_protocol::composition::{CompositionEngine, CompositionContext};
//! use peat_protocol::composition::additive::SensorCoverageRule;
//! use peat_protocol::models::capability::Capability;
//! use std::sync::Arc;
//!
//! # async fn example() -> peat_protocol::Result<()> {
//! let mut engine = CompositionEngine::new();
//! engine.register_rule(Arc::new(SensorCoverageRule::default()));
//!
//! let capabilities = vec![/* capabilities */];
//! let context = CompositionContext::new(vec!["node1".to_string()]);
//!
//! let results = engine.compose(&capabilities, &context).await?;
//! # Ok(())
//! # }
//! ```

pub mod additive;
pub mod constraint;
pub mod emergent;
pub mod engine;
pub mod redundant;
pub mod rules;

pub use engine::CompositionEngine;
pub use rules::{CompositionContext, CompositionResult, CompositionRule};
