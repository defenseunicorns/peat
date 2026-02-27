//! Testing utilities and infrastructure for PEAT Protocol
//!
//! This module provides test harnesses and utilities for E2E and integration testing.

#[cfg(any(feature = "ditto-backend", feature = "automerge-backend"))]
pub mod e2e_harness;

#[cfg(any(feature = "ditto-backend", feature = "automerge-backend"))]
pub use e2e_harness::*;
