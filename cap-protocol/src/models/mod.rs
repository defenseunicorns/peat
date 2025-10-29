//! Data models for the CAP protocol

pub mod capability;
pub mod platform;
pub mod squad;

pub use capability::{Capability, CapabilityType};
pub use platform::{HealthStatus, PlatformConfig, PlatformState};
pub use squad::{SquadConfig, SquadState};
