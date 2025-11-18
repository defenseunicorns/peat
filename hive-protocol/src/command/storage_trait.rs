//! Storage abstraction for hierarchical commands
//!
//! This module defines the backend-agnostic storage interface for command
//! dissemination, allowing different CRDT backends (Ditto, Automerge/Iroh) to be
//! used interchangeably.

use crate::Result;
use async_trait::async_trait;
use hive_schema::command::v1::{CommandAcknowledgment, CommandStatus, HierarchicalCommand};

/// Backend-agnostic storage interface for hierarchical commands
///
/// This trait abstracts over different CRDT storage backends (Ditto, Automerge/Iroh)
/// and provides the core operations needed for command dissemination.
///
/// # Design Principles
///
/// 1. **Publish-Once Pattern**: Each command is published once to the collection
/// 2. **Acknowledge-Many Pattern**: Multiple nodes acknowledge a single command
/// 3. **Backend Flexibility**: Implementations handle CRDT semantics differently
///
/// # Implementation Notes
///
/// - **Ditto**: Uses DQL INSERT for commands, observer-based subscription for reception
/// - **Automerge/Iroh**: Uses CRDT operations on Automerge documents
/// - Both must support observer patterns for real-time command reception
#[async_trait]
pub trait CommandStorage: Send + Sync {
    // ========================================================================
    // Command Operations
    // ========================================================================

    /// Publish a command to the storage backend
    ///
    /// # Arguments
    ///
    /// * `command` - The hierarchical command to publish
    ///
    /// # Returns
    ///
    /// Document ID on success
    ///
    /// # Errors
    ///
    /// Returns error if publish fails (network error, validation failure)
    async fn publish_command(&self, command: &HierarchicalCommand) -> Result<String>;

    /// Retrieve a command by ID
    ///
    /// # Returns
    ///
    /// Some(HierarchicalCommand) if found, None if not found
    async fn get_command(&self, command_id: &str) -> Result<Option<HierarchicalCommand>>;

    /// Query commands by target
    ///
    /// Returns all commands targeting the specified node/squad/platoon
    async fn query_commands_by_target(&self, target_id: &str) -> Result<Vec<HierarchicalCommand>>;

    /// Delete a command (when expired or completed)
    async fn delete_command(&self, command_id: &str) -> Result<()>;

    // ========================================================================
    // Acknowledgment Operations
    // ========================================================================

    /// Publish an acknowledgment for a command
    ///
    /// # Arguments
    ///
    /// * `ack` - The command acknowledgment to publish
    ///
    /// # Returns
    ///
    /// Document ID on success
    async fn publish_acknowledgment(&self, ack: &CommandAcknowledgment) -> Result<String>;

    /// Get all acknowledgments for a command
    ///
    /// # Returns
    ///
    /// Vector of acknowledgments for the specified command
    async fn get_acknowledgments(&self, command_id: &str) -> Result<Vec<CommandAcknowledgment>>;

    // ========================================================================
    // Status Tracking Operations
    // ========================================================================

    /// Update command status
    ///
    /// # Arguments
    ///
    /// * `status` - The updated command status
    async fn update_command_status(&self, status: &CommandStatus) -> Result<()>;

    /// Get command status
    ///
    /// # Returns
    ///
    /// Some(CommandStatus) if found, None if not found
    async fn get_command_status(&self, command_id: &str) -> Result<Option<CommandStatus>>;

    // ========================================================================
    // Observer Pattern (for real-time command reception)
    // ========================================================================

    /// Register a callback for new commands targeting this node
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node ID to filter commands for
    /// * `callback` - Async callback invoked when new commands arrive
    ///
    /// # Returns
    ///
    /// Observer handle (implementation-specific)
    ///
    /// # Note
    ///
    /// This is the critical method for real-time command reception.
    /// Implementations should use native observer patterns (Ditto observers, Automerge subscriptions).
    async fn observe_commands(
        &self,
        node_id: &str,
        callback: Box<
            dyn Fn(
                    HierarchicalCommand,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    ) -> Result<ObserverHandle>;

    /// Register a callback for new acknowledgments for commands issued by this node
    ///
    /// # Arguments
    ///
    /// * `issuer_id` - The node ID that issued commands
    /// * `callback` - Async callback invoked when new acknowledgments arrive
    async fn observe_acknowledgments(
        &self,
        issuer_id: &str,
        callback: Box<
            dyn Fn(
                    CommandAcknowledgment,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    ) -> Result<ObserverHandle>;
}

/// Handle for an active observer subscription
///
/// Dropping this handle should cancel the observation.
pub struct ObserverHandle {
    /// Implementation-specific handle (Arc<dyn Any> for type erasure)
    inner: std::sync::Arc<dyn std::any::Any + Send + Sync>,
}

impl ObserverHandle {
    /// Create a new observer handle
    pub fn new<T: std::any::Any + Send + Sync>(handle: T) -> Self {
        Self {
            inner: std::sync::Arc::new(handle),
        }
    }

    /// Get the inner handle (for backend-specific operations)
    pub fn inner(&self) -> &std::sync::Arc<dyn std::any::Any + Send + Sync> {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_observer_handle_creation() {
        // Observer handle creation is tested in integration tests
        // since it requires backend initialization
    }
}
