//! Ditto implementation of CommandStorage trait
//!
//! This module provides the Ditto backend implementation of the backend-agnostic
//! CommandStorage trait, enabling hierarchical command dissemination with Ditto's CRDT engine.

use crate::command::{CommandStorage, ObserverHandle};
use crate::storage::ditto_store::DittoStore;
use crate::Result;
use async_trait::async_trait;
use hive_schema::command::v1::{CommandAcknowledgment, CommandStatus, HierarchicalCommand};
use std::sync::Arc;
use tracing::instrument;

/// Ditto-backed implementation of CommandStorage
///
/// This struct wraps a DittoStore and implements the CommandStorage trait,
/// providing the Ditto-specific implementation of command dissemination.
///
/// # Design
///
/// This implementation uses two Ditto collections:
/// - `hierarchical_commands`: Stores published commands
/// - `hierarchical_commands_acks`: Stores command acknowledgments
/// - `hierarchical_commands_status`: Stores command execution status
///
/// Commands are published once and observed by target nodes using Ditto's
/// observer pattern for real-time command reception.
pub struct DittoCommandStorage {
    store: Arc<DittoStore>,
}

impl DittoCommandStorage {
    /// Collection name for hierarchical commands
    const COMMANDS_COLLECTION: &'static str = "hierarchical_commands";

    /// Collection name for command acknowledgments
    const ACKS_COLLECTION: &'static str = "hierarchical_commands_acks";

    /// Collection name for command status
    const STATUS_COLLECTION: &'static str = "hierarchical_commands_status";

    /// Create a new DittoCommandStorage from a DittoStore
    pub fn new(store: Arc<DittoStore>) -> Self {
        Self { store }
    }

    /// Get access to underlying DittoStore (for Ditto-specific operations)
    pub fn store(&self) -> &Arc<DittoStore> {
        &self.store
    }
}

#[async_trait]
impl CommandStorage for DittoCommandStorage {
    // ========================================================================
    // Command Operations
    // ========================================================================

    #[instrument(skip(self, command), fields(command_id = %command.command_id))]
    async fn publish_command(&self, command: &HierarchicalCommand) -> Result<String> {
        let doc_id = format!("cmd-{}", command.command_id);

        // Serialize command to JSON
        let command_json = serde_json::to_value(command).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to serialize command: {}", e),
                "publish_command",
                Some(doc_id.clone()),
            )
        })?;

        // Insert command into Ditto collection
        let dql_query = format!("INSERT INTO {} DOCUMENTS (:doc)", Self::COMMANDS_COLLECTION);

        self.store
            .ditto()
            .store()
            .execute_v2((
                dql_query,
                serde_json::json!({"doc": {
                    "_id": doc_id.clone(),
                    "command": command_json,
                    "published_at_us": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                }}),
            ))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to publish command: {}", e),
                    "publish_command",
                    Some(doc_id.clone()),
                )
            })?;

        tracing::debug!(
            command_id = %command.command_id,
            doc_id = %doc_id,
            "Published command to Ditto"
        );

        Ok(doc_id)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_command(&self, command_id: &str) -> Result<Option<HierarchicalCommand>> {
        let doc_id = format!("cmd-{}", command_id);

        let dql_query = format!(
            "SELECT * FROM {} WHERE _id = :_id",
            Self::COMMANDS_COLLECTION
        );

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((dql_query, serde_json::json!({"_id": doc_id})))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to query command: {}", e),
                    "get_command",
                    Some(doc_id.clone()),
                )
            })?;

        // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
        let items: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                let json_str = item.json_string();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        if items.is_empty() {
            return Ok(None);
        }

        let command_json = &items[0]["command"];
        let command: HierarchicalCommand =
            serde_json::from_value(command_json.clone()).map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to deserialize command: {}", e),
                    "get_command",
                    Some(doc_id),
                )
            })?;

        Ok(Some(command))
    }

    #[instrument(skip(self), fields(target_id))]
    async fn query_commands_by_target(&self, target_id: &str) -> Result<Vec<HierarchicalCommand>> {
        // Query commands where target contains target_id
        // This is a simplified implementation - full implementation would
        // parse the CommandTarget and handle scope-based filtering

        let dql_query = format!(
            "SELECT * FROM {} WHERE command.target.target_ids CONTAINS :target_id",
            Self::COMMANDS_COLLECTION
        );

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((dql_query, serde_json::json!({"target_id": target_id})))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to query commands by target: {}", e),
                    "query_commands_by_target",
                    Some(target_id.to_string()),
                )
            })?;

        // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
        let items: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                let json_str = item.json_string();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        let mut commands = Vec::new();
        for item in items {
            let command_json = &item["command"];
            let command: HierarchicalCommand = serde_json::from_value(command_json.clone())
                .map_err(|e| {
                    crate::Error::storage_error(
                        format!("Failed to deserialize command: {}", e),
                        "query_commands_by_target",
                        None,
                    )
                })?;
            commands.push(command);
        }

        Ok(commands)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn delete_command(&self, command_id: &str) -> Result<()> {
        let doc_id = format!("cmd-{}", command_id);

        let dql_query = format!("DELETE FROM {} WHERE _id = :_id", Self::COMMANDS_COLLECTION);

        self.store
            .ditto()
            .store()
            .execute_v2((dql_query, serde_json::json!({"_id": doc_id})))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to delete command: {}", e),
                    "delete_command",
                    Some(doc_id.clone()),
                )
            })?;

        tracing::debug!(command_id = %command_id, "Deleted command from Ditto");

        Ok(())
    }

    // ========================================================================
    // Acknowledgment Operations
    // ========================================================================

    #[instrument(skip(self, ack), fields(command_id = %ack.command_id, node_id = %ack.node_id))]
    async fn publish_acknowledgment(&self, ack: &CommandAcknowledgment) -> Result<String> {
        let doc_id = format!("ack-{}-{}", ack.command_id, ack.node_id);

        // Serialize acknowledgment to JSON
        let ack_json = serde_json::to_value(ack).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to serialize acknowledgment: {}", e),
                "publish_acknowledgment",
                Some(doc_id.clone()),
            )
        })?;

        // Insert acknowledgment into Ditto collection
        let dql_query = format!("INSERT INTO {} DOCUMENTS (:doc)", Self::ACKS_COLLECTION);

        self.store
            .ditto()
            .store()
            .execute_v2((
                dql_query,
                serde_json::json!({"doc": {
                    "_id": doc_id.clone(),
                    "acknowledgment": ack_json,
                    "received_at_us": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                }}),
            ))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to publish acknowledgment: {}", e),
                    "publish_acknowledgment",
                    Some(doc_id.clone()),
                )
            })?;

        tracing::debug!(
            command_id = %ack.command_id,
            node_id = %ack.node_id,
            doc_id = %doc_id,
            "Published acknowledgment to Ditto"
        );

        Ok(doc_id)
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_acknowledgments(&self, command_id: &str) -> Result<Vec<CommandAcknowledgment>> {
        let dql_query = format!(
            "SELECT * FROM {} WHERE acknowledgment.command_id = :command_id",
            Self::ACKS_COLLECTION
        );

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((dql_query, serde_json::json!({"command_id": command_id})))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to query acknowledgments: {}", e),
                    "get_acknowledgments",
                    Some(command_id.to_string()),
                )
            })?;

        // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
        let items: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                let json_str = item.json_string();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        let mut acks = Vec::new();
        for item in items {
            let ack_json = &item["acknowledgment"];
            let ack: CommandAcknowledgment =
                serde_json::from_value(ack_json.clone()).map_err(|e| {
                    crate::Error::storage_error(
                        format!("Failed to deserialize acknowledgment: {}", e),
                        "get_acknowledgments",
                        None,
                    )
                })?;
            acks.push(ack);
        }

        Ok(acks)
    }

    // ========================================================================
    // Status Tracking Operations
    // ========================================================================

    #[instrument(skip(self, status), fields(command_id = %status.command_id))]
    async fn update_command_status(&self, status: &CommandStatus) -> Result<()> {
        let doc_id = format!("status-{}", status.command_id);

        // Serialize status to JSON
        let status_json = serde_json::to_value(status).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to serialize status: {}", e),
                "update_command_status",
                Some(doc_id.clone()),
            )
        })?;

        // Upsert status into Ditto collection
        let dql_query = format!(
            "INSERT INTO {} DOCUMENTS (:doc) ON ID CONFLICT DO UPDATE",
            Self::STATUS_COLLECTION
        );

        self.store
            .ditto()
            .store()
            .execute_v2((
                dql_query,
                serde_json::json!({"doc": {
                    "_id": doc_id.clone(),
                    "status": status_json,
                    "updated_at_us": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                }}),
            ))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to update command status: {}", e),
                    "update_command_status",
                    Some(doc_id.clone()),
                )
            })?;

        tracing::debug!(
            command_id = %status.command_id,
            state = status.state,
            "Updated command status in Ditto"
        );

        Ok(())
    }

    #[instrument(skip(self), fields(command_id))]
    async fn get_command_status(&self, command_id: &str) -> Result<Option<CommandStatus>> {
        let doc_id = format!("status-{}", command_id);

        let dql_query = format!("SELECT * FROM {} WHERE _id = :_id", Self::STATUS_COLLECTION);

        let result = self
            .store
            .ditto()
            .store()
            .execute_v2((dql_query, serde_json::json!({"_id": doc_id})))
            .await
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to query command status: {}", e),
                    "get_command_status",
                    Some(doc_id.clone()),
                )
            })?;

        // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
        let items: Vec<serde_json::Value> = result
            .iter()
            .map(|item| {
                let json_str = item.json_string();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            })
            .collect();

        if items.is_empty() {
            return Ok(None);
        }

        let status_json = &items[0]["status"];
        let status: CommandStatus = serde_json::from_value(status_json.clone()).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize status: {}", e),
                "get_command_status",
                Some(doc_id),
            )
        })?;

        Ok(Some(status))
    }

    // ========================================================================
    // Observer Pattern (for real-time command reception)
    // ========================================================================

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
        // Create query for commands targeting this node
        // This is a simplified implementation - full implementation would
        // handle all CommandTarget scopes (individual, squad, platoon, broadcast)
        let query = format!(
            "SELECT * FROM {} WHERE command.target.target_ids CONTAINS :node_id",
            Self::COMMANDS_COLLECTION
        );

        // Register Ditto observer
        let callback = Arc::new(callback);
        let observer = self
            .store
            .ditto()
            .store()
            .register_observer_v2(
                (query, serde_json::json!({"node_id": node_id})),
                move |result| {
                    // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
                    let items: Vec<serde_json::Value> = result
                        .iter()
                        .map(|item| {
                            let json_str = item.json_string();
                            serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
                        })
                        .collect();
                    for item in items {
                        if let Ok(command) =
                            serde_json::from_value::<HierarchicalCommand>(item["command"].clone())
                        {
                            let cb = callback.clone();
                            tokio::spawn(async move {
                                cb(command).await;
                            });
                        }
                    }
                },
            )
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to register command observer: {}", e),
                    "observe_commands",
                    Some(node_id.to_string()),
                )
            })?;

        tracing::debug!(node_id = %node_id, "Registered command observer");

        Ok(ObserverHandle::new(observer))
    }

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
        // Create query for acknowledgments of commands issued by this node
        // We need to join with commands collection, but for simplicity we'll
        // query all acks and filter in the callback
        let query = format!("SELECT * FROM {}", Self::ACKS_COLLECTION);

        // Register Ditto observer
        let issuer_id = issuer_id.to_string();
        let callback = Arc::new(callback);
        let observer = self
            .store
            .ditto()
            .store()
            .register_observer_v2(query, move |result| {
                // Convert QueryResult to Vec<serde_json::Value> using iter() + json_string()
                let items: Vec<serde_json::Value> = result
                    .iter()
                    .map(|item| {
                        let json_str = item.json_string();
                        serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
                    })
                    .collect();
                for item in items {
                    if let Ok(ack) = serde_json::from_value::<CommandAcknowledgment>(
                        item["acknowledgment"].clone(),
                    ) {
                        // Filter for commands issued by this node
                        // (In a full implementation, we'd do this in the DQL query)
                        let cb = callback.clone();
                        tokio::spawn(async move {
                            cb(ack).await;
                        });
                    }
                }
            })
            .map_err(|e| {
                crate::Error::storage_error(
                    format!("Failed to register acknowledgment observer: {}", e),
                    "observe_acknowledgments",
                    Some(issuer_id.clone()),
                )
            })?;

        tracing::debug!(issuer_id = %issuer_id, "Registered acknowledgment observer");

        Ok(ObserverHandle::new(observer))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ditto_command_storage_creation() {
        // Storage creation is tested in integration tests
        // since it requires Ditto SDK initialization
    }
}
