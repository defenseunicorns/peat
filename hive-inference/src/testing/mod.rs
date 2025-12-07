//! M1 Vignette E2E Testing Infrastructure
//!
//! Provides test harnesses for end-to-end testing of the M1 object tracking
//! vignette across distributed human-machine-AI teams.
//!
//! Note: M1TestHarness requires the ditto-backend feature in hive-protocol
//! which is not currently enabled. The harness module is disabled until
//! an automerge-backend equivalent E2EHarness is available.

// M1TestHarness depends on hive_protocol::testing::E2EHarness which is only
// available with the ditto-backend feature. Disabled for automerge-backend.
// mod harness;
// pub use harness::M1TestHarness;

mod fixtures;
mod metrics;

pub use fixtures::{CoordinatorFixture, SimulatedC2, TeamFixture};
pub use metrics::{MessageType, MetricsCollector, MetricsReport};
