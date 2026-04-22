//! Testing utilities and infrastructure for Peat Protocol
//!
//! This module provides test harnesses and utilities for E2E and integration testing.

#[cfg(feature = "automerge-backend")]
pub mod e2e_harness;

#[cfg(feature = "automerge-backend")]
pub use e2e_harness::*;
