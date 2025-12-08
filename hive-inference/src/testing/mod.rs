//! M1 Vignette E2E Testing Infrastructure
//!
//! Provides test harnesses for end-to-end testing of the M1 object tracking
//! vignette across distributed human-machine-AI teams.

mod fixtures;
mod harness;
mod metrics;

pub use fixtures::{CoordinatorFixture, SimulatedC2, TeamFixture};
pub use harness::collections;
pub use metrics::{MessageType, MetricsCollector, MetricsReport};
