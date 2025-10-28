//! Squad state data structures

use crate::models::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

/// Squad configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadConfig {
    /// Unique squad identifier
    pub id: String,
    /// Maximum squad size
    pub max_size: usize,
    /// Minimum squad size
    pub min_size: usize,
}

impl SquadConfig {
    /// Create a new squad configuration
    pub fn new(max_size: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            max_size,
            min_size: 2,
        }
    }
}

/// Squad state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadState {
    /// Squad configuration
    pub config: SquadConfig,
    /// Current squad leader platform ID
    pub leader_id: Option<String>,
    /// Set of member platform IDs
    pub members: HashSet<String>,
    /// Aggregated squad capabilities
    pub capabilities: Vec<Capability>,
    /// Parent platoon ID (if any)
    pub platoon_id: Option<String>,
    /// Last update timestamp
    pub timestamp: u64,
}

impl SquadState {
    /// Create a new squad state
    pub fn new(config: SquadConfig) -> Self {
        Self {
            config,
            leader_id: None,
            members: HashSet::new(),
            capabilities: Vec::new(),
            platoon_id: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Check if the squad is at capacity
    pub fn is_full(&self) -> bool {
        self.members.len() >= self.config.max_size
    }

    /// Check if the squad meets minimum size
    pub fn is_valid(&self) -> bool {
        self.members.len() >= self.config.min_size
    }

    /// Add a member to the squad
    pub fn add_member(&mut self, platform_id: String) -> bool {
        if self.is_full() {
            false
        } else {
            self.members.insert(platform_id)
        }
    }

    /// Remove a member from the squad
    pub fn remove_member(&mut self, platform_id: &str) -> bool {
        self.members.remove(platform_id)
    }
}
