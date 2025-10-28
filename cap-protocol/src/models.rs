//! Data models for platform and squad state

pub mod capability;
pub mod platform;
pub mod squad;

pub use capability::Capability;
pub use platform::{PlatformConfig, PlatformState};
pub use squad::{SquadConfig, SquadState};
