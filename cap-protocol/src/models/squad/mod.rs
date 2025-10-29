//! Squad state data structures
//!
//! This module defines squad data models with CRDT operations:
//! - Member list: OR-Set (observed-remove set) - members can be added and removed
//! - Leader election: LWW-Register (last-write-wins) - leader updates with timestamps
//! - Aggregated capabilities: G-Set (grow-only set) - capabilities accumulate

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

    /// Update the timestamp to current time
    fn update_timestamp(&mut self) {
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Check if the squad is at capacity
    pub fn is_full(&self) -> bool {
        self.members.len() >= self.config.max_size
    }

    /// Check if the squad meets minimum size
    pub fn is_valid(&self) -> bool {
        self.members.len() >= self.config.min_size
    }

    /// Add a member to the squad (OR-Set add operation)
    ///
    /// This implements an OR-Set CRDT where members can be added and removed.
    /// Concurrent add/remove operations are resolved by: Add wins over Remove.
    pub fn add_member(&mut self, platform_id: String) -> bool {
        if self.is_full() {
            false
        } else {
            let added = self.members.insert(platform_id);
            if added {
                self.update_timestamp();
            }
            added
        }
    }

    /// Remove a member from the squad (OR-Set remove operation)
    pub fn remove_member(&mut self, platform_id: &str) -> bool {
        let removed = self.members.remove(platform_id);
        if removed {
            self.update_timestamp();
            // If leader is removed, clear leader
            if self.leader_id.as_deref() == Some(platform_id) {
                self.leader_id = None;
            }
        }
        removed
    }

    /// Set the squad leader (LWW-Register operation)
    ///
    /// This implements Last-Write-Wins semantics for leader election.
    /// The leader must be a current member of the squad.
    pub fn set_leader(&mut self, platform_id: String) -> Result<(), &'static str> {
        if !self.members.contains(&platform_id) {
            return Err("Leader must be a squad member");
        }
        self.leader_id = Some(platform_id);
        self.update_timestamp();
        Ok(())
    }

    /// Clear the squad leader
    pub fn clear_leader(&mut self) {
        self.leader_id = None;
        self.update_timestamp();
    }

    /// Add a capability to the squad (G-Set operation)
    ///
    /// This implements a G-Set CRDT where capabilities can only be added.
    /// Capabilities are aggregated from all squad members.
    pub fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists (by ID)
        if !self.capabilities.iter().any(|c| c.id == capability.id) {
            self.capabilities.push(capability);
            self.update_timestamp();
        }
    }

    /// Get all capabilities of a specific type
    pub fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability> {
        self.capabilities
            .iter()
            .filter(|c| c.capability_type == capability_type)
            .collect()
    }

    /// Check if squad has a specific capability type
    pub fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.capability_type == capability_type)
    }

    /// Assign squad to a platoon (LWW-Register operation)
    pub fn assign_platoon(&mut self, platoon_id: String) {
        self.platoon_id = Some(platoon_id);
        self.update_timestamp();
    }

    /// Remove squad from platoon
    pub fn leave_platoon(&mut self) {
        self.platoon_id = None;
        self.update_timestamp();
    }

    /// Merge with another squad state (CRDT merge)
    ///
    /// Merges two squad states using CRDT semantics:
    /// - Members: Union (OR-Set merge)
    /// - Leader: Take newer timestamp (LWW-Register merge)
    /// - Capabilities: Union (G-Set merge)
    pub fn merge(&mut self, other: &SquadState) {
        // Merge members (OR-Set union)
        for member in &other.members {
            self.members.insert(member.clone());
        }

        // Merge capabilities (G-Set union)
        for cap in &other.capabilities {
            if !self.capabilities.iter().any(|c| c.id == cap.id) {
                self.capabilities.push(cap.clone());
            }
        }

        // Merge leader and platoon (LWW-Register - take newer)
        if other.timestamp > self.timestamp {
            self.leader_id = other.leader_id.clone();
            self.platoon_id = other.platoon_id.clone();
            self.timestamp = other.timestamp;
        }
    }

    /// Get the count of members
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Check if a platform is a member
    pub fn is_member(&self, platform_id: &str) -> bool {
        self.members.contains(platform_id)
    }

    /// Check if this platform is the leader
    pub fn is_leader(&self, platform_id: &str) -> bool {
        self.leader_id.as_deref() == Some(platform_id)
    }
}

#[cfg(test)]
mod tests;
