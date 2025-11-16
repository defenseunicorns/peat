//! Data models for the HIVE protocol

pub mod capability;
pub mod cell;
pub mod node;
pub mod operator;
pub mod role;
pub mod zone;

// Re-export commonly used types at module level
pub use capability::{Capability, CapabilityExt, CapabilityType};
pub use cell::{CellConfig, CellConfigExt, CellState, CellStateExt};
pub use node::{HealthStatus, NodeConfig, NodeConfigExt, NodeState, NodeStateExt};
pub use operator::{
    AuthorityLevel, AuthorityLevelExt, BindingType, HumanMachinePair, HumanMachinePairExt,
    Operator, OperatorExt, OperatorRank, OperatorRankExt,
};
pub use role::{CellRole, RoleAssignment, RoleScorer};
pub use zone::{ZoneConfig, ZoneState, ZoneStats};

// Legacy compatibility aliases - allow both old and new names during transition
pub use cell::CellConfig as SquadConfig;
pub use cell::CellState as SquadState;
pub use node::NodeConfig as PlatformConfig;
pub use node::NodeState as PlatformState;
pub use role::CellRole as SquadRole;
