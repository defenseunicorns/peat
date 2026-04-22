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
            .await?;
        Ok(())
    }

    /// Delete a node state
    #[instrument(skip(self))]
    pub async fn delete_state(&self, node_id: &str) -> Result<()> {
        info!("Deleting node state: {}", node_id);

        self.backend
            .document_store()
            .remove(NODE_STATE_COLLECTION, &node_id.to_string())
            .await?;
        Ok(())
    }

    /// Get the underlying backend reference
    pub fn backend(&self) -> &B {
        &self.backend
    }
}
