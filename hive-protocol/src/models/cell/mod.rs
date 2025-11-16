//! Cell state data structures
//!
//! This module defines squad data models with CRDT operations:
//! - Member list: OR-Set (observed-remove set) - members can be added and removed
//! - Leader election: LWW-Register (last-write-wins) - leader updates with timestamps
//! - Aggregated capabilities: G-Set (grow-only set) - capabilities accumulate

use crate::models::{Capability, CapabilityExt};
use uuid::Uuid;

// Re-export protobuf types
pub use hive_schema::cell::v1::{CellConfig, CellState};

// Extension trait for CellConfig helper methods
pub trait CellConfigExt {
    /// Create a new cell configuration
    fn new(max_size: u32) -> Self;

    /// Create a new cell configuration with a specific ID
    fn with_id(id: String, max_size: u32) -> Self;
}

impl CellConfigExt for CellConfig {
    fn new(max_size: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            max_size,
            min_size: 2,
            created_at: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }

    fn with_id(id: String, max_size: u32) -> Self {
        Self {
            id,
            max_size,
            min_size: 2,
            created_at: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }
}

// Extension trait for CellState helper methods with CRDT operations
pub trait CellStateExt {
    /// Create a new cell state
    fn new(config: CellConfig) -> Self;

    /// Update the timestamp to current time
    fn update_timestamp(&mut self);

    /// Check if the cell is at capacity
    fn is_full(&self) -> bool;

    /// Check if the cell meets minimum size
    fn is_valid(&self) -> bool;

    /// Add a member to the cell (OR-Set add operation)
    ///
    /// This implements an OR-Set CRDT where members can be added and removed.
    /// Concurrent add/remove operations are resolved by: Add wins over Remove.
    fn add_member(&mut self, node_id: String) -> bool;

    /// Remove a member from the cell (OR-Set remove operation)
    fn remove_member(&mut self, node_id: &str) -> bool;

    /// Set the cell leader (LWW-Register operation)
    ///
    /// This implements Last-Write-Wins semantics for leader election.
    /// The leader must be a current member of the cell.
    fn set_leader(&mut self, node_id: String) -> Result<(), &'static str>;

    /// Clear the cell leader
    fn clear_leader(&mut self);

    /// Add a capability to the cell (G-Set operation)
    ///
    /// This implements a G-Set CRDT where capabilities can only be added.
    /// Capabilities are aggregated from all cell members.
    fn add_capability(&mut self, capability: Capability);

    /// Get all capabilities of a specific type
    fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability>;

    /// Check if cell has a specific capability type
    fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool;

    /// Assign cell to a platoon (LWW-Register operation)
    fn assign_platoon(&mut self, platoon_id: String);

    /// Remove cell from platoon
    fn leave_platoon(&mut self);

    /// Merge with another cell state (CRDT merge)
    ///
    /// Merges two cell states using CRDT semantics:
    /// - Members: Union (OR-Set merge)
    /// - Leader: Take newer timestamp (LWW-Register merge)
    /// - Capabilities: Union (G-Set merge)
    fn merge(&mut self, other: &CellState);

    /// Get the count of members
    fn member_count(&self) -> usize;

    /// Check if a node is a member
    fn is_member(&self, node_id: &str) -> bool;

    /// Check if this node is the leader
    fn is_leader(&self, node_id: &str) -> bool;

    /// Get the cell ID (convenience method)
    fn get_id(&self) -> Option<&str>;
}

impl CellStateExt for CellState {
    fn new(config: CellConfig) -> Self {
        Self {
            config: Some(config),
            leader_id: None,
            members: Vec::new(),
            capabilities: Vec::new(),
            platoon_id: None,
            timestamp: Some(hive_schema::common::v1::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                nanos: 0,
            }),
        }
    }

    fn update_timestamp(&mut self) {
        self.timestamp = Some(hive_schema::common::v1::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            nanos: 0,
        });
    }

    fn is_full(&self) -> bool {
        if let Some(ref config) = self.config {
            self.members.len() >= config.max_size as usize
        } else {
            false
        }
    }

    fn is_valid(&self) -> bool {
        if let Some(ref config) = self.config {
            self.members.len() >= config.min_size as usize
        } else {
            false
        }
    }

    fn add_member(&mut self, node_id: String) -> bool {
        if self.is_full() {
            false
        } else {
            // Check if already a member
            if self.members.contains(&node_id) {
                false
            } else {
                self.members.push(node_id);
                self.update_timestamp();
                true
            }
        }
    }

    fn remove_member(&mut self, node_id: &str) -> bool {
        if let Some(pos) = self.members.iter().position(|id| id == node_id) {
            self.members.remove(pos);
            self.update_timestamp();
            // If leader is removed, clear leader
            if self.leader_id.as_deref() == Some(node_id) {
                self.leader_id = None;
            }
            true
        } else {
            false
        }
    }

    fn set_leader(&mut self, node_id: String) -> Result<(), &'static str> {
        if !self.members.contains(&node_id) {
            return Err("Leader must be a squad member");
        }
        self.leader_id = Some(node_id);
        self.update_timestamp();
        Ok(())
    }

    fn clear_leader(&mut self) {
        self.leader_id = None;
        self.update_timestamp();
    }

    fn add_capability(&mut self, capability: Capability) {
        // Check if capability already exists (by ID)
        if !self.capabilities.iter().any(|c| c.id == capability.id) {
            self.capabilities.push(capability);
            self.update_timestamp();
        }
    }

    fn get_capabilities_by_type(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Vec<&Capability> {
        self.capabilities
            .iter()
            .filter(|c| c.get_capability_type() == capability_type)
            .collect()
    }

    fn has_capability_type(&self, capability_type: crate::models::CapabilityType) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.get_capability_type() == capability_type)
    }

    fn assign_platoon(&mut self, platoon_id: String) {
        self.platoon_id = Some(platoon_id);
        self.update_timestamp();
    }

    fn leave_platoon(&mut self) {
        self.platoon_id = None;
        self.update_timestamp();
    }

    fn merge(&mut self, other: &CellState) {
        // Merge members (OR-Set union)
        for member in &other.members {
            if !self.members.contains(member) {
                self.members.push(member.clone());
            }
        }

        // Merge capabilities (G-Set union)
        for cap in &other.capabilities {
            if !self.capabilities.iter().any(|c| c.id == cap.id) {
                self.capabilities.push(cap.clone());
            }
        }

        // Merge leader and platoon (LWW-Register - take newer)
        let self_ts = self.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);
        let other_ts = other.timestamp.as_ref().map(|t| t.seconds).unwrap_or(0);

        if other_ts > self_ts {
            self.leader_id = other.leader_id.clone();
            self.platoon_id = other.platoon_id.clone();
            self.timestamp = other.timestamp;
        }
    }

    fn member_count(&self) -> usize {
        self.members.len()
    }

    fn is_member(&self, node_id: &str) -> bool {
        self.members.iter().any(|id| id == node_id)
    }

    fn is_leader(&self, node_id: &str) -> bool {
        self.leader_id.as_deref() == Some(node_id)
    }

    fn get_id(&self) -> Option<&str> {
        self.config.as_ref().map(|c| c.id.as_str())
    }
}

#[cfg(test)]
mod tests;
