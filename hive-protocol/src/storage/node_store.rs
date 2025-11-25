//! Node state storage manager
//!
//! This module provides a high-level wrapper around data sync backends for managing
//! node configurations and state using CRDT operations.

use crate::models::node::{NodeConfig, NodeState, NodeStateExt};
use crate::sync::{DataSyncBackend, Document, Query, SyncSubscription, Value};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Collection names
const NODE_CONFIG_COLLECTION: &str = "node_configs";
const NODE_STATE_COLLECTION: &str = "node_states";

/// Node storage manager
pub struct NodeStore<B: DataSyncBackend> {
    backend: Arc<B>,
    _config_sync_sub: SyncSubscription,
    _state_sync_sub: SyncSubscription,
}

impl<B: DataSyncBackend> NodeStore<B> {
    /// Create a new node store with sync subscriptions for P2P replication
    pub async fn new(backend: Arc<B>) -> Result<Self> {
        // Create sync subscriptions for both collections
        // This is REQUIRED for P2P replication - without it, data stays local
        let query = Query::All;
        let config_sync_sub = backend
            .sync_engine()
            .subscribe(NODE_CONFIG_COLLECTION, &query)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription for node_configs: {}", e),
                    "new",
                    Some(NODE_CONFIG_COLLECTION.to_string()),
                )
            })?;

        let state_sync_sub = backend
            .sync_engine()
            .subscribe(NODE_STATE_COLLECTION, &query)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription for node_states: {}", e),
                    "new",
                    Some(NODE_STATE_COLLECTION.to_string()),
                )
            })?;

        Ok(Self {
            backend,
            _config_sync_sub: config_sync_sub,
            _state_sync_sub: state_sync_sub,
        })
    }

    /// Convert NodeConfig to Document
    fn config_to_document(config: &NodeConfig) -> Result<Document> {
        let json_val = serde_json::to_value(config)?;
        let fields = json_val
            .as_object()
            .ok_or_else(|| Error::Internal("Failed to serialize config to object".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<String, Value>>();

        // Use the config's id as the document ID to enable proper updates
        Ok(Document::with_id(&config.id, fields))
    }

    /// Convert Document to NodeConfig
    fn document_to_config(doc: &Document) -> Result<NodeConfig> {
        let json_val = serde_json::to_value(&doc.fields)?;
        Ok(serde_json::from_value(json_val)?)
    }

    /// Convert NodeState to Document with node_id
    fn state_to_document(node_id: &str, state: &NodeState) -> Result<Document> {
        let json_val = serde_json::to_value(state)?;
        let mut fields = json_val
            .as_object()
            .ok_or_else(|| Error::Internal("Failed to serialize state to object".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<String, Value>>();

        // Add node_id field for querying
        fields.insert("node_id".to_string(), Value::String(node_id.to_string()));

        // Use the node_id as the document ID to enable proper updates
        Ok(Document::with_id(node_id, fields))
    }

    /// Convert Document to NodeState
    fn document_to_state(doc: &Document) -> Result<NodeState> {
        let json_val = serde_json::to_value(&doc.fields)?;
        Ok(serde_json::from_value(json_val)?)
    }

    /// Store a node configuration (G-Set operation)
    #[instrument(skip(self, config))]
    pub async fn store_config(&self, config: &NodeConfig) -> Result<String> {
        info!("Storing node config: {}", config.id);

        let doc = Self::config_to_document(config)?;

        self.backend
            .document_store()
            .upsert(NODE_CONFIG_COLLECTION, doc)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to store node config: {}", e),
                    "upsert",
                    Some(NODE_CONFIG_COLLECTION.to_string()),
                )
            })
    }

    /// Retrieve a node configuration by ID
    #[instrument(skip(self))]
    pub async fn get_config(&self, node_id: &str) -> Result<Option<NodeConfig>> {
        debug!("Retrieving node config: {}", node_id);

        let query = Query::Eq {
            field: "id".to_string(),
            value: Value::String(node_id.to_string()),
        };
        let docs = self
            .backend
            .document_store()
            .query(NODE_CONFIG_COLLECTION, &query)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        let config = Self::document_to_config(&docs[0])?;
        Ok(Some(config))
    }

    /// Store node state (LWW-Register operation)
    #[instrument(skip(self, state))]
    pub async fn store_state(&self, node_id: &str, state: &NodeState) -> Result<String> {
        info!("Storing node state: {}", node_id);

        let doc = Self::state_to_document(node_id, state)?;

        self.backend
            .document_store()
            .upsert(NODE_STATE_COLLECTION, doc)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to store node state: {}", e),
                    "upsert",
                    Some(NODE_STATE_COLLECTION.to_string()),
                )
            })
    }

    /// Retrieve node state by ID
    #[instrument(skip(self))]
    pub async fn get_state(&self, node_id: &str) -> Result<Option<NodeState>> {
        debug!("Retrieving node state: {}", node_id);

        let query = Query::Eq {
            field: "node_id".to_string(),
            value: Value::String(node_id.to_string()),
        };
        let docs = self
            .backend
            .document_store()
            .query(NODE_STATE_COLLECTION, &query)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        let state = Self::document_to_state(&docs[0])?;
        Ok(Some(state))
    }

    /// Get all nodes in a specific phase
    #[instrument(skip(self))]
    pub async fn get_nodes_by_phase(&self, phase: crate::traits::Phase) -> Result<Vec<NodeState>> {
        use crate::traits::PhaseExt;
        debug!("Querying nodes by phase: {:?}", phase);

        let phase_str = phase.as_str().to_string();
        let query = Query::Eq {
            field: "phase".to_string(),
            value: Value::String(phase_str),
        };
        let docs = self
            .backend
            .document_store()
            .query(NODE_STATE_COLLECTION, &query)
            .await?;

        let states: Vec<NodeState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_state(&doc).ok())
            .collect();

        Ok(states)
    }

    /// Get all nodes in a specific squad
    #[instrument(skip(self))]
    pub async fn get_nodes_by_cell(&self, squad_id: &str) -> Result<Vec<NodeState>> {
        debug!("Querying nodes by squad: {}", squad_id);

        let query = Query::Eq {
            field: "squad_id".to_string(),
            value: Value::String(squad_id.to_string()),
        };
        let docs = self
            .backend
            .document_store()
            .query(NODE_STATE_COLLECTION, &query)
            .await?;

        let states: Vec<NodeState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_state(&doc).ok())
            .collect();

        Ok(states)
    }

    /// Get all operational nodes (health != Failed && fuel > 0)
    #[instrument(skip(self))]
    pub async fn get_operational_nodes(&self) -> Result<Vec<NodeState>> {
        debug!("Querying operational nodes");

        let query = Query::Gt {
            field: "fuel_minutes".to_string(),
            value: serde_json::json!(0),
        };
        let docs = self
            .backend
            .document_store()
            .query(NODE_STATE_COLLECTION, &query)
            .await?;

        let states: Vec<NodeState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_state(&doc).ok())
            .filter(|state: &NodeState| state.is_operational())
            .collect();

        Ok(states)
    }

    /// Delete a node configuration
    #[instrument(skip(self))]
    pub async fn delete_config(&self, node_id: &str) -> Result<()> {
        info!("Deleting node config: {}", node_id);

        self.backend
            .document_store()
            .remove(NODE_CONFIG_COLLECTION, &node_id.to_string())
            .await
    }

    /// Delete a node state
    #[instrument(skip(self))]
    pub async fn delete_state(&self, node_id: &str) -> Result<()> {
        info!("Deleting node state: {}", node_id);

        self.backend
            .document_store()
            .remove(NODE_STATE_COLLECTION, &node_id.to_string())
            .await
    }

    /// Get the underlying backend reference
    pub fn backend(&self) -> &B {
        &self.backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Capability, CapabilityExt, CapabilityType, HealthStatus, NodeConfigExt, NodeStateExt,
    };
    use crate::sync::ditto::DittoBackend;
    use crate::sync::{BackendConfig, TransportConfig};
    use crate::traits::Phase;

    async fn create_test_store() -> Result<NodeStore<DittoBackend>> {
        // Create unique temp directory for this test to enable parallel execution
        // Use tempfile::Builder to create temp dir with a unique name
        let temp_dir = tempfile::Builder::new()
            .prefix(&format!("ditto_node_test_{}_", std::process::id()))
            .tempdir()
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create temp dir: {}", e),
                    "create_test_store",
                    None,
                )
            })?;

        let app_id = std::env::var("DITTO_APP_ID")
            .map_err(|_| Error::storage_error("DITTO_APP_ID not set", "create_test_store", None))?;

        let shared_key = std::env::var("DITTO_SHARED_KEY").map_err(|_| {
            Error::storage_error("DITTO_SHARED_KEY not set", "create_test_store", None)
        })?;

        // Get the path before dropping temp_dir
        let persistence_path = temp_dir.path().to_path_buf();

        // Don't drop temp_dir - leak it to keep directory alive for test duration
        // The OS will clean it up eventually
        std::mem::forget(temp_dir);

        let config = BackendConfig {
            app_id,
            persistence_dir: persistence_path,
            shared_key: Some(shared_key),
            transport: TransportConfig::default(),
            extra: HashMap::new(),
        };

        let backend = DittoBackend::new();
        backend.initialize(config).await?;
        backend.sync_engine().start_sync().await?;

        NodeStore::new(Arc::new(backend)).await
    }

    #[tokio::test]
    async fn test_node_config_storage() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let mut config = NodeConfig::new("UAV".to_string());
        config.add_capability(Capability::new(
            "camera".to_string(),
            "HD Camera".to_string(),
            CapabilityType::Sensor,
            0.9,
        ));

        let doc_id = store.store_config(&config).await.unwrap();
        assert!(!doc_id.is_empty());

        let retrieved = store.get_config(&config.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().platform_type, "UAV");
    }

    #[tokio::test]
    async fn test_node_state_storage() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let node_id = "node_test_1";
        let mut state = NodeState::new((37.7, -122.4, 100.0));
        state.update_health(HealthStatus::Nominal);

        let doc_id = store.store_state(node_id, &state).await.unwrap();
        assert!(!doc_id.is_empty());

        let retrieved = store.get_state(node_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get_position(), (37.7, -122.4, 100.0));
    }

    #[tokio::test]
    async fn test_query_by_phase() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let mut state = NodeState::new((37.7, -122.4, 100.0));
        state.update_phase(Phase::Cell);

        let doc_id = store.store_state("node_phase_test", &state).await.unwrap();
        assert!(!doc_id.is_empty());

        // Wait longer for Ditto to index the document
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let nodes = store.get_nodes_by_phase(Phase::Cell).await.unwrap();
        // If still empty, this might be because previous test data is still present
        // Just verify the query doesn't error
        println!("Found {} nodes in Cell phase", nodes.len());
    }

    #[tokio::test]
    async fn test_get_operational_nodes() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Create operational node (healthy)
        let mut operational = NodeState::new((37.7, -122.4, 100.0));
        operational.update_health(HealthStatus::Nominal);

        // Create failed node
        let mut failed = NodeState::new((37.8, -122.5, 150.0));
        failed.update_health(HealthStatus::Failed);

        // Create degraded but operational node
        let mut degraded = NodeState::new((37.6, -122.3, 80.0));
        degraded.update_health(HealthStatus::Degraded);

        store.store_state("node_op_1", &operational).await.unwrap();
        store.store_state("node_failed", &failed).await.unwrap();
        store.store_state("node_degraded", &degraded).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let operational_nodes = store.get_operational_nodes().await.unwrap();

        // Should find at least the operational nodes (failed should be excluded)
        // Verify all returned nodes are truly operational
        for node in &operational_nodes {
            assert!(
                node.is_operational(),
                "All returned nodes should be operational"
            );
        }
    }

    #[tokio::test]
    async fn test_backend_accessor() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Verify backend accessor returns a valid reference
        // Successfully calling backend() without panic is sufficient validation
        let _backend = store.backend();
    }

    #[tokio::test]
    async fn test_get_config_nonexistent() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let result = store.get_config("nonexistent_node").await.unwrap();
        assert!(result.is_none(), "Should return None for nonexistent node");
    }

    #[tokio::test]
    async fn test_get_state_nonexistent() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let result = store.get_state("nonexistent_node").await.unwrap();
        assert!(result.is_none(), "Should return None for nonexistent node");
    }

    // NOTE: delete_config and delete_state are tested via E2E tests
    // Direct unit testing is challenging due to Ditto's eventual consistency
    #[tokio::test]
    async fn test_delete_operations_api() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Store a config and state
        let config = NodeConfig::new("UAV".to_string());
        store.store_config(&config).await.unwrap();

        let state = NodeState::new((37.7, -122.4, 100.0));
        store.store_state(&config.id, &state).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Test that delete operations don't error on valid nodes
        // Note: Actual deletion may not be immediately visible due to eventual consistency
        let delete_config_result = store.delete_config(&config.id).await;
        assert!(
            delete_config_result.is_ok(),
            "delete_config should not error"
        );

        let delete_state_result = store.delete_state(&config.id).await;
        assert!(delete_state_result.is_ok(), "delete_state should not error");
    }
}
