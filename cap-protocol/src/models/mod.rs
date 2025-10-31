//! Data models for the CAP protocol

pub mod capability;
pub mod cell;
pub mod node;
pub mod operator;
pub mod role;

// Re-export commonly used types at module level
pub use capability::{Capability, CapabilityType};
pub use cell::{CellConfig, CellState};
pub use node::{HealthStatus, NodeConfig, NodeState};
pub use operator::{AuthorityLevel, BindingType, HumanMachinePair, Operator, OperatorRank};
pub use role::{CellRole, RoleAssignment, RoleScorer};

// Legacy compatibility aliases - allow both old and new names during transition
pub use cell::CellConfig as SquadConfig;
pub use cell::CellState as SquadState;
pub use node::NodeConfig as PlatformConfig;
pub use node::NodeState as PlatformState;
pub use role::CellRole as SquadRole;
