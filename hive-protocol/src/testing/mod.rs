//! Testing utilities and infrastructure for HIVE Protocol
//!
//! This module provides test harnesses and utilities for E2E and integration testing.

#[cfg(feature = "ditto-backend")]
pub mod e2e_harness;

#[cfg(feature = "ditto-backend")]
pub use e2e_harness::*;
