//! Automerge implementation of CommandStorage trait
//!
//! This module provides the Automerge backend implementation of the backend-agnostic
//! CommandStorage trait, enabling hierarchical command dissemination with Automerge's CRDT engine.

#[cfg(feature = "automerge-backend")]
use crate::command::{CommandStorage, ObserverHandle};
#[cfg(feature = "automerge-backend")]
use crate::storage::automerge_conversion::{automerge_to_message, message_to_automerge};
#[cfg(feature = "automerge-backend")]
use crate::storage::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use crate::Result;
#[cfg(feature = "automerge-backend")]
use async_trait::async_trait;
#[cfg(feature = "automerge-backend")]
use hive_schema::command::v1::{CommandAcknowledgment, CommandStatus, HierarchicalCommand};
#[cfg(feature = "automerge-backend")]
use std::sync::Arc;
#[cfg(feature = "automerge-backend")]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "automerge-backend")]
use tracing::instrument;

/// Automerge-backed implementation of CommandStorage
///
/// This struct wraps an AutomergeStore and implements the CommandStorage trait,
/// providing the Automerge-specific implementation of command dissemination.
///
/// # Design
///
/// This implementation uses three namespaces via key prefixes:
/// - `cmd:`: Stores published commands
/// - `ack:`: Stores command acknowledgments
/// - `status:`: Stores command execution status
///
/// Commands are published once and discovered by target nodes using polling
/// or the AutomergeStore's change notification channel.
///
/// # Observer Pattern
///
/// Unlike Ditto which has native observers, Automerge uses a change notification
/// channel. For the observer methods, we spawn background tasks that poll for
/// changes and filter based on target_id/issuer_id.
#[cfg(feature = "automerge-backend")]
pub struct AutomergeCommandStorage {
    store: Arc<AutomergeStore>,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeCommandStorage {
    /// Key prefix for commands
    const COMMANDS_PREFIX: &'static str = "cmd:";

    /// Key prefix for acknowledgments
    const ACKS_PREFIX: &'static str = "ack:";

    /// Key prefix for status
    const STATUS_PREFIX: &'static str = "status:";

    /// Create a new AutomergeCommandStorage from an AutomergeStore
    pub fn new(store: Arc<AutomergeStore>) -> Self {
        Self { store }
    }

    /// Get access to underlying AutomergeStore (for Automerge-specific operations)
    pub fn store(&self) -> &Arc<AutomergeStore> {
        &self.store
    }

    fn command_key(command_id: &str) -> String {
        format!("{}{}", Self::COMMANDS_PREFIX, command_id)
    }

    fn ack_key(command_id: &str, node_id: &str) -> String {
        format!("{}{}-{}", Self::ACKS_PREFIX, command_id, node_id)
    }

    fn ack_prefix(command_id: &str) -> String {
        format!("{}{}-", Self::ACKS_PREFIX, command_id)
    }

    fn status_key(command_id: &str) -> String {
        format!("{}{}", Self::STATUS_PREFIX, command_id)
    }

    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }
}

#[cfg(feature = "automerge-backend")]
#[async_trait]
impl CommandStorage for AutomergeCommandStorage {
    // ========================================================================
    // Command Operations
    // ========================================================================

    #[instrument(skip(self, command), fields(command_id = %command.command_id))]
    async fn publish_command(&self, command: &HierarchicalCommand) -> Result<String> {
        let key = Self::command_key(&command.command_id);

        // Wrap command with metadata for storage
        let wrapper = CommandWrapper {
            command: command.clone(),
            published_at_us: Self::now_us(),
        };

        // Convert to Automerge document and store
        let doc = message_to_automerge(&wrapper).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert command to Automerge: {}", e),
                "publish_command",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store command: {}", e),
                "publish_command",
                Some(key.clone()),
            )
        })?;

        tracing::debug!(
            command_id = %command.command_id,
            key = %key,
            "Published command to Automerge"
        );

        Ok(key)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_command(&self, command_id: &str) -> Result<Option<HierarchicalCommand>> {
        let key = Self::command_key(command_id);

        let doc = match self.store.get(&key) {
            Ok(Some(doc)) => doc,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(crate::Error::storage_error(
                    format!("Failed to get command: {}", e),
                    "get_command",
                    Some(key),
                ))
            }
        };

        let wrapper: CommandWrapper = automerge_to_message(&doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize command: {}", e),
                "get_command",
                Some(key),
            )
        })?;

        Ok(Some(wrapper.command))
    }

    #[instrument(skip(self), fields(target_id))]
    async fn query_commands_by_target(&self, target_id: &str) -> Result<Vec<HierarchicalCommand>> {
        // Scan all commands and filter by target
        let docs = self.store.scan_prefix(Self::COMMANDS_PREFIX).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to scan commands: {}", e),
                "query_commands_by_target",
                None,
            )
        })?;

        let mut commands = Vec::new();
        for (_key, doc) in docs {
            if let Ok(wrapper) = automerge_to_message::<CommandWrapper>(&doc) {
                // Check if target_ids contains the target_id
                if let Some(ref target) = wrapper.command.target {
                    if target.target_ids.contains(&target_id.to_string()) {
                        commands.push(wrapper.command);
                    }
                }
            }
        }

        Ok(commands)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn delete_command(&self, command_id: &str) -> Result<()> {
        let key = Self::command_key(command_id);

        self.store.delete(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to delete command: {}", e),
                "delete_command",
                Some(key.clone()),
            )
        })?;

        tracing::debug!(command_id = %command_id, "Deleted command from Automerge");

        Ok(())
    }

    // ========================================================================
    // Acknowledgment Operations
    // ========================================================================

    #[instrument(skip(self, ack), fields(command_id = %ack.command_id, node_id = %ack.node_id))]
    async fn publish_acknowledgment(&self, ack: &CommandAcknowledgment) -> Result<String> {
        let key = Self::ack_key(&ack.command_id, &ack.node_id);

        // Wrap acknowledgment with metadata
        let wrapper = AckWrapper {
            acknowledgment: ack.clone(),
            received_at_us: Self::now_us(),
        };

        // Convert to Automerge document and store
        let doc = message_to_automerge(&wrapper).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert acknowledgment to Automerge: {}", e),
                "publish_acknowledgment",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store acknowledgment: {}", e),
                "publish_acknowledgment",
                Some(key.clone()),
            )
        })?;

        tracing::debug!(
            command_id = %ack.command_id,
            node_id = %ack.node_id,
            key = %key,
            "Published acknowledgment to Automerge"
        );

        Ok(key)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_acknowledgments(&self, command_id: &str) -> Result<Vec<CommandAcknowledgment>> {
        let prefix = Self::ack_prefix(command_id);

        let docs = self.store.scan_prefix(&prefix).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to scan acknowledgments: {}", e),
                "get_acknowledgments",
                Some(command_id.to_string()),
            )
        })?;

        let mut acks = Vec::new();
        for (_key, doc) in docs {
            if let Ok(wrapper) = automerge_to_message::<AckWrapper>(&doc) {
                acks.push(wrapper.acknowledgment);
            }
        }

        Ok(acks)
    }

    // ========================================================================
    // Status Tracking Operations
    // ========================================================================

    #[instrument(skip(self, status), fields(command_id = %status.command_id))]
    async fn update_command_status(&self, status: &CommandStatus) -> Result<()> {
        let key = Self::status_key(&status.command_id);

        // Wrap status with metadata
        let wrapper = StatusWrapper {
            status: status.clone(),
            updated_at_us: Self::now_us(),
        };

        // Convert to Automerge document and store (upsert semantics)
        let doc = message_to_automerge(&wrapper).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert status to Automerge: {}", e),
                "update_command_status",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store status: {}", e),
                "update_command_status",
                Some(key.clone()),
            )
        })?;

        tracing::debug!(
            command_id = %status.command_id,
            state = status.state,
            "Updated command status in Automerge"
        );

        Ok(())
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_command_status(&self, command_id: &str) -> Result<Option<CommandStatus>> {
        let key = Self::status_key(command_id);

        let doc = match self.store.get(&key) {
            Ok(Some(doc)) => doc,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(crate::Error::storage_error(
                    format!("Failed to get status: {}", e),
                    "get_command_status",
                    Some(key),
                ))
            }
        };

        let wrapper: StatusWrapper = automerge_to_message(&doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize status: {}", e),
                "get_command_status",
                Some(key),
            )
        })?;

        Ok(Some(wrapper.status))
    }

    // ========================================================================
    // Observer Pattern
    // ========================================================================

    /// Register a callback for new commands targeting this node
    ///
    /// # Note on Automerge Implementation
    ///
    /// Unlike Ditto's native observer pattern, Automerge uses change notifications.
    /// This implementation spawns a background task that:
    /// 1. Subscribes to the store's change channel
    /// 2. Filters changes for command keys
    /// 3. Deserializes and checks if the command targets this node
    /// 4. Invokes the callback for matching commands
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
    ) -> Result<ObserverHandle> {
        let store = Arc::clone(&self.store);
        let node_id = node_id.to_string();
        let callback = Arc::new(callback);

        // Create a cancellation token using a channel
        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn background task to poll for commands
        let poll_store = Arc::clone(&store);
        let poll_node_id = node_id.clone();
        let poll_callback = Arc::clone(&callback);

        tokio::spawn(async move {
            let mut seen_commands: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            // Initial scan for existing commands
            if let Ok(docs) = poll_store.scan_prefix(Self::COMMANDS_PREFIX) {
                for (key, doc) in docs {
                    if let Ok(wrapper) = automerge_to_message::<CommandWrapper>(&doc) {
                        if let Some(ref target) = wrapper.command.target {
                            if target.target_ids.contains(&poll_node_id) {
                                seen_commands.insert(key);
                                let cmd = wrapper.command.clone();
                                let cb = Arc::clone(&poll_callback);
                                tokio::spawn(async move {
                                    cb(cmd).await;
                                });
                            }
                        }
                    }
                }
            }

            // Polling loop
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
            loop {
                tokio::select! {
                    _ = cancel_rx.recv() => {
                        tracing::debug!(node_id = %poll_node_id, "Command observer cancelled");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Ok(docs) = poll_store.scan_prefix(Self::COMMANDS_PREFIX) {
                            for (key, doc) in docs {
                                if seen_commands.contains(&key) {
                                    continue;
                                }
                                if let Ok(wrapper) = automerge_to_message::<CommandWrapper>(&doc) {
                                    if let Some(ref target) = wrapper.command.target {
                                        if target.target_ids.contains(&poll_node_id) {
                                            seen_commands.insert(key);
                                            let cmd = wrapper.command.clone();
                                            let cb = Arc::clone(&poll_callback);
                                            tokio::spawn(async move {
                                                cb(cmd).await;
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        tracing::debug!(node_id = %node_id, "Registered command observer");

        Ok(ObserverHandle::new(cancel_tx))
    }

    /// Register a callback for new acknowledgments for commands issued by this node
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
    ) -> Result<ObserverHandle> {
        let store = Arc::clone(&self.store);
        let issuer_id = issuer_id.to_string();
        let callback = Arc::new(callback);

        // Create a cancellation token
        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn background task to poll for acknowledgments
        let poll_store = Arc::clone(&store);
        let poll_issuer_id = issuer_id.clone();
        let poll_callback = Arc::clone(&callback);

        tokio::spawn(async move {
            let mut seen_acks: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Initial scan for existing acks
            if let Ok(docs) = poll_store.scan_prefix(Self::ACKS_PREFIX) {
                for (key, doc) in docs {
                    if let Ok(wrapper) = automerge_to_message::<AckWrapper>(&doc) {
                        // We need to check if the command was issued by this node
                        // For now, we pass all acks - the caller can filter
                        seen_acks.insert(key);
                        let ack = wrapper.acknowledgment.clone();
                        let cb = Arc::clone(&poll_callback);
                        tokio::spawn(async move {
                            cb(ack).await;
                        });
                    }
                }
            }

            // Polling loop
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
            loop {
                tokio::select! {
                    _ = cancel_rx.recv() => {
                        tracing::debug!(issuer_id = %poll_issuer_id, "Acknowledgment observer cancelled");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Ok(docs) = poll_store.scan_prefix(Self::ACKS_PREFIX) {
                            for (key, doc) in docs {
                                if seen_acks.contains(&key) {
                                    continue;
                                }
                                if let Ok(wrapper) = automerge_to_message::<AckWrapper>(&doc) {
                                    seen_acks.insert(key);
                                    let ack = wrapper.acknowledgment.clone();
                                    let cb = Arc::clone(&poll_callback);
                                    tokio::spawn(async move {
                                        cb(ack).await;
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });

        tracing::debug!(issuer_id = %issuer_id, "Registered acknowledgment observer");

        Ok(ObserverHandle::new(cancel_tx))
    }
}

// ============================================================================
// Internal wrapper types for storage with metadata
// ============================================================================

#[cfg(feature = "automerge-backend")]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct CommandWrapper {
    command: HierarchicalCommand,
    published_at_us: u64,
}

#[cfg(feature = "automerge-backend")]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct AckWrapper {
    acknowledgment: CommandAcknowledgment,
    received_at_us: u64,
}

#[cfg(feature = "automerge-backend")]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct StatusWrapper {
    status: CommandStatus,
    updated_at_us: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use hive_schema::command::v1::{CommandTarget, CommandTargetScope};
    use tempfile::TempDir;

    fn create_test_storage() -> (AutomergeCommandStorage, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store = AutomergeStore::open(temp_dir.path()).expect("Failed to create store");
        (AutomergeCommandStorage::new(Arc::new(store)), temp_dir)
    }

    fn create_test_command(command_id: &str, target_ids: Vec<String>) -> HierarchicalCommand {
        HierarchicalCommand {
            command_id: command_id.to_string(),
            originator_id: "test-originator".to_string(),
            target: Some(CommandTarget {
                scope: CommandTargetScope::Individual as i32,
                target_ids,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn create_test_ack(command_id: &str, node_id: &str) -> CommandAcknowledgment {
        CommandAcknowledgment {
            command_id: command_id.to_string(),
            node_id: node_id.to_string(),
            ..Default::default()
        }
    }

    fn create_test_status(command_id: &str, state: i32) -> CommandStatus {
        CommandStatus {
            command_id: command_id.to_string(),
            state,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_command_crud() {
        let (storage, _temp) = create_test_storage();

        // Create
        let command = create_test_command("cmd-1", vec!["node-1".to_string()]);
        let doc_id = storage.publish_command(&command).await.unwrap();
        assert!(doc_id.starts_with("cmd:"));

        // Read
        let retrieved = storage.get_command("cmd-1").await.unwrap().unwrap();
        assert_eq!(retrieved.command_id, "cmd-1");
        assert_eq!(retrieved.originator_id, "test-originator");

        // Query by target
        let commands = storage.query_commands_by_target("node-1").await.unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_id, "cmd-1");

        // Query miss
        let empty = storage.query_commands_by_target("node-2").await.unwrap();
        assert!(empty.is_empty());

        // Delete
        storage.delete_command("cmd-1").await.unwrap();
        let deleted = storage.get_command("cmd-1").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_acknowledgment_crud() {
        let (storage, _temp) = create_test_storage();

        // Publish command first
        let command =
            create_test_command("cmd-1", vec!["node-1".to_string(), "node-2".to_string()]);
        storage.publish_command(&command).await.unwrap();

        // Publish acknowledgments from multiple nodes
        let ack1 = create_test_ack("cmd-1", "node-1");
        let ack2 = create_test_ack("cmd-1", "node-2");

        storage.publish_acknowledgment(&ack1).await.unwrap();
        storage.publish_acknowledgment(&ack2).await.unwrap();

        // Get all acknowledgments
        let acks = storage.get_acknowledgments("cmd-1").await.unwrap();
        assert_eq!(acks.len(), 2);

        let node_ids: Vec<&str> = acks.iter().map(|a| a.node_id.as_str()).collect();
        assert!(node_ids.contains(&"node-1"));
        assert!(node_ids.contains(&"node-2"));
    }

    #[tokio::test]
    async fn test_status_crud() {
        let (storage, _temp) = create_test_storage();

        // Initial status
        let status1 = create_test_status("cmd-1", 1); // Pending
        storage.update_command_status(&status1).await.unwrap();

        let retrieved = storage.get_command_status("cmd-1").await.unwrap().unwrap();
        assert_eq!(retrieved.command_id, "cmd-1");
        assert_eq!(retrieved.state, 1);

        // Update status (upsert semantics)
        let status2 = create_test_status("cmd-1", 2); // Completed
        storage.update_command_status(&status2).await.unwrap();

        let updated = storage.get_command_status("cmd-1").await.unwrap().unwrap();
        assert_eq!(updated.state, 2);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (storage, _temp) = create_test_storage();

        assert!(storage.get_command("nonexistent").await.unwrap().is_none());
        assert!(storage
            .get_command_status("nonexistent")
            .await
            .unwrap()
            .is_none());
        assert!(storage
            .get_acknowledgments("nonexistent")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_multiple_commands() {
        let (storage, _temp) = create_test_storage();

        // Create multiple commands targeting different nodes
        let cmd1 = create_test_command("cmd-1", vec!["node-1".to_string()]);
        let cmd2 = create_test_command("cmd-2", vec!["node-1".to_string(), "node-2".to_string()]);
        let cmd3 = create_test_command("cmd-3", vec!["node-2".to_string()]);

        storage.publish_command(&cmd1).await.unwrap();
        storage.publish_command(&cmd2).await.unwrap();
        storage.publish_command(&cmd3).await.unwrap();

        // Query by node-1: should get cmd-1 and cmd-2
        let node1_cmds = storage.query_commands_by_target("node-1").await.unwrap();
        assert_eq!(node1_cmds.len(), 2);

        // Query by node-2: should get cmd-2 and cmd-3
        let node2_cmds = storage.query_commands_by_target("node-2").await.unwrap();
        assert_eq!(node2_cmds.len(), 2);
    }
}
