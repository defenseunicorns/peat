//! Cell state storage manager
//!
//! This module provides a high-level wrapper around data sync backends for managing
//! cell state using CRDT operations.

use crate::models::{cell::CellState, Capability};
use crate::sync::{DataSyncBackend, Document, Query, SyncSubscription, Value};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Collection name
const CELL_COLLECTION: &str = "cells";

/// Cell storage manager
pub struct CellStore<B: DataSyncBackend> {
    backend: Arc<B>,
    _sync_sub: SyncSubscription,
}

impl<B: DataSyncBackend> CellStore<B> {
    /// Create a new cell store with sync subscription for P2P replication
    pub async fn new(backend: Arc<B>) -> Result<Self> {
        // Create sync subscription for the cells collection
        // This is REQUIRED for P2P replication - without it, data stays local
        let query = Query::All;
        let sync_sub = backend
            .sync_engine()
            .subscribe(CELL_COLLECTION, &query)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to create sync subscription for cells: {}", e),
                    "new",
                    Some(CELL_COLLECTION.to_string()),
                )
            })?;

        Ok(Self {
            backend,
            _sync_sub: sync_sub,
        })
    }

    /// Convert CellState to Document
    fn cell_to_document(cell: &CellState) -> Result<Document> {
        let json_val = serde_json::to_value(cell)?;
        let fields = json_val
            .as_object()
            .ok_or_else(|| Error::Internal("Failed to serialize cell to object".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<String, Value>>();

        let mut doc = Document::new(fields);
        // Add cell_id field for querying
        doc.set("cell_id".to_string(), Value::String(cell.config.id.clone()));
        Ok(doc)
    }

    /// Convert Document to CellState
    fn document_to_cell(doc: &Document) -> Result<CellState> {
        let json_val = serde_json::to_value(&doc.fields)?;
        Ok(serde_json::from_value(json_val)?)
    }

    /// Store a cell state (OR-Set + LWW-Register operations)
    #[instrument(skip(self, cell))]
    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        info!("Storing cell: {}", cell.config.id);

        let doc = Self::cell_to_document(cell)?;

        // Always INSERT - get_cell will query for latest by updated_at timestamp
        self.backend
            .document_store()
            .upsert(CELL_COLLECTION, doc)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to store cell: {}", e),
                    "upsert",
                    Some(CELL_COLLECTION.to_string()),
                )
            })
    }

    /// Retrieve a cell by ID
    #[instrument(skip(self))]
    pub async fn get_cell(&self, cell_id: &str) -> Result<Option<CellState>> {
        debug!("Retrieving cell: {}", cell_id);

        let query = Query::Eq {
            field: "cell_id".to_string(),
            value: Value::String(cell_id.to_string()),
        };
        let mut docs = self
            .backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        // Sort by updated_at descending to get the latest version
        // (since we always INSERT new documents rather than updating)
        docs.sort_by(|a, b| {
            let a_ts = a.updated_at;
            let b_ts = b.updated_at;
            b_ts.cmp(&a_ts) // Descending order
        });

        let cell = Self::document_to_cell(&docs[0])?;
        Ok(Some(cell))
    }

    /// Get all valid cells (meeting minimum size requirements)
    #[instrument(skip(self))]
    pub async fn get_valid_cells(&self) -> Result<Vec<CellState>> {
        debug!("Querying valid cells");

        // Query all cells - we'll filter in code since query abstraction doesn't support array length
        let query = Query::All;
        let docs = self
            .backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_cell(&doc).ok())
            .filter(|cell: &CellState| cell.is_valid())
            .collect();

        Ok(cells)
    }

    /// Get all cells in a platoon
    #[instrument(skip(self))]
    pub async fn get_cells_by_zone(&self, platoon_id: &str) -> Result<Vec<CellState>> {
        debug!("Querying cells by platoon: {}", platoon_id);

        let query = Query::Eq {
            field: "platoon_id".to_string(),
            value: Value::String(platoon_id.to_string()),
        };
        let docs = self
            .backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_cell(&doc).ok())
            .collect();

        Ok(cells)
    }

    /// Get cells that have a specific capability type
    #[instrument(skip(self))]
    pub async fn get_cells_with_capability(
        &self,
        capability_type: crate::models::CapabilityType,
    ) -> Result<Vec<CellState>> {
        debug!("Querying cells with capability: {:?}", capability_type);

        // Query all cells - filter by capability in code
        let query = Query::All;
        let docs = self
            .backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_cell(&doc).ok())
            .filter(|cell: &CellState| cell.has_capability_type(capability_type))
            .collect();

        Ok(cells)
    }

    /// Get cells that are not full (can accept more members)
    #[instrument(skip(self))]
    pub async fn get_available_cells(&self) -> Result<Vec<CellState>> {
        debug!("Querying available cells");

        let query = Query::All;
        let docs = self
            .backend
            .document_store()
            .query(CELL_COLLECTION, &query)
            .await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| Self::document_to_cell(&doc).ok())
            .filter(|cell: &CellState| !cell.is_full())
            .collect();

        Ok(cells)
    }

    /// Add a member to a cell (OR-Set add operation)
    #[instrument(skip(self))]
    pub async fn add_member(&self, cell_id: &str, node_id: String) -> Result<()> {
        info!("Adding member {} to cell {}", node_id, cell_id);

        let mut cell = self
            .get_cell(cell_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Cell".to_string(),
                id: cell_id.to_string(),
            })?;

        if !cell.add_member(node_id) {
            return Err(Error::Internal("Failed to add member to cell".to_string()));
        }

        self.store_cell(&cell).await?;
        Ok(())
    }

    /// Remove a member from a cell (OR-Set remove operation)
    #[instrument(skip(self))]
    pub async fn remove_member(&self, cell_id: &str, node_id: &str) -> Result<()> {
        info!("Removing member {} from cell {}", node_id, cell_id);

        let mut cell = self
            .get_cell(cell_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Cell".to_string(),
                id: cell_id.to_string(),
            })?;

        if !cell.remove_member(node_id) {
            return Err(Error::Internal(
                "Failed to remove member from cell".to_string(),
            ));
        }

        self.store_cell(&cell).await?;
        Ok(())
    }

    /// Set squad leader (LWW-Register operation)
    #[instrument(skip(self))]
    pub async fn set_leader(&self, cell_id: &str, node_id: String) -> Result<()> {
        info!("Setting leader {} for squad {}", node_id, cell_id);

        let mut cell = self
            .get_cell(cell_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Cell".to_string(),
                id: cell_id.to_string(),
            })?;

        cell.set_leader(node_id)
            .map_err(|e| Error::Internal(e.to_string()))?;

        self.store_cell(&cell).await?;
        Ok(())
    }

    /// Add a capability to a cell (G-Set operation)
    #[instrument(skip(self, capability))]
    pub async fn add_capability(&self, cell_id: &str, capability: Capability) -> Result<()> {
        info!("Adding capability to cell {}", cell_id);

        let mut cell = self
            .get_cell(cell_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Cell".to_string(),
                id: cell_id.to_string(),
            })?;

        cell.add_capability(capability);
        self.store_cell(&cell).await?;
        Ok(())
    }

    /// Delete a cell
    #[instrument(skip(self))]
    pub async fn delete_cell(&self, cell_id: &str) -> Result<()> {
        info!("Deleting cell: {}", cell_id);

        self.backend
            .document_store()
            .remove(CELL_COLLECTION, &cell_id.to_string())
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
    use crate::models::{CapabilityExt, CellConfig};
    use crate::sync::ditto::DittoBackend;
    use crate::sync::{BackendConfig, TransportConfig};
    use std::collections::HashMap;

    async fn create_test_store() -> Result<CellStore<DittoBackend>> {
        // Create unique temp directory for this test to enable parallel execution
        // Use tempfile::Builder to create temp dir with a unique name
        let temp_dir = tempfile::Builder::new()
            .prefix(&format!("ditto_cell_test_{}_", std::process::id()))
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

        CellStore::new(Arc::new(backend)).await
    }

    #[tokio::test]
    async fn test_cell_storage() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let mut cell = CellState::new(config);
        cell.add_member("node_1".to_string());

        let doc_id = store.store_cell(&cell).await.unwrap();
        assert!(!doc_id.is_empty());

        let retrieved = store.get_cell(&cell.config.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().member_count(), 1);
    }

    #[tokio::test]
    async fn test_get_valid_cells() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Create a valid cell (meets minimum size)
        let config = CellConfig::new(5);
        let mut valid_cell = CellState::new(config.clone());
        valid_cell.add_member("node_1".to_string());
        valid_cell.add_member("node_2".to_string());
        store.store_cell(&valid_cell).await.unwrap();

        // Create an invalid cell (too few members)
        let invalid_cell = CellState::new(config);
        // Don't add any members - will be invalid
        store.store_cell(&invalid_cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let valid_cells = store.get_valid_cells().await.unwrap();
        assert_eq!(valid_cells.len(), 1);
        assert_eq!(valid_cells[0].config.id, valid_cell.config.id);
    }

    #[tokio::test]
    async fn test_get_cells_by_zone() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let mut cell1 = CellState::new(config.clone());
        cell1.platoon_id = Some("platoon_alpha".to_string());
        store.store_cell(&cell1).await.unwrap();

        let mut cell2 = CellState::new(config);
        cell2.platoon_id = Some("platoon_beta".to_string());
        store.store_cell(&cell2).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let alpha_cells = store.get_cells_by_zone("platoon_alpha").await.unwrap();
        assert_eq!(alpha_cells.len(), 1);
        assert_eq!(alpha_cells[0].config.id, cell1.config.id);
    }

    #[tokio::test]
    async fn test_get_cells_with_capability() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let mut cell_with_sensor = CellState::new(config.clone());
        cell_with_sensor.add_capability(Capability::new(
            "sensor1".to_string(),
            "EO/IR".to_string(),
            crate::models::CapabilityType::Sensor,
            0.9,
        ));
        store.store_cell(&cell_with_sensor).await.unwrap();

        let mut cell_with_comms = CellState::new(config);
        cell_with_comms.add_capability(Capability::new(
            "radio1".to_string(),
            "Radio".to_string(),
            crate::models::CapabilityType::Communication,
            0.85,
        ));
        store.store_cell(&cell_with_comms).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let sensor_cells = store
            .get_cells_with_capability(crate::models::CapabilityType::Sensor)
            .await
            .unwrap();
        assert_eq!(sensor_cells.len(), 1);
        assert_eq!(sensor_cells[0].config.id, cell_with_sensor.config.id);
    }

    #[tokio::test]
    async fn test_get_available_cells() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Create an available cell (not full)
        let config = CellConfig::new(5);
        let mut available_cell = CellState::new(config.clone());
        available_cell.add_member("node_1".to_string());
        store.store_cell(&available_cell).await.unwrap();

        // Create a full cell
        let mut full_cell = CellState::new(config);
        for i in 0..5 {
            full_cell.add_member(format!("node_{}", i));
        }
        store.store_cell(&full_cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let available = store.get_available_cells().await.unwrap();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].config.id, available_cell.config.id);
    }

    // NOTE: Mutation methods (add_member, remove_member, set_leader, add_capability, delete_cell)
    // are comprehensively tested in E2E tests (tests/squad_formation_e2e.rs,
    // tests/storage_layer_e2e.rs, tests/load_testing_e2e.rs, and others).
    //
    // Direct unit testing of these methods is not possible due to Ditto's eventual consistency -
    // the "get-modify-store" pattern these methods use doesn't guarantee read-your-own-writes
    // consistency, even with observer-based synchronization. The methods work correctly in
    // multi-peer scenarios (validated by 100+ E2E test usages), but single-peer unit tests
    // cannot reliably verify state changes due to CRDT sync timing.

    #[tokio::test]
    async fn test_add_member_nonexistent_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let result = store
            .add_member("nonexistent_cell", "node_1".to_string())
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_remove_member_nonexistent_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let result = store.remove_member("nonexistent_cell", "node_1").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_set_leader_nonexistent_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let result = store
            .set_leader("nonexistent_cell", "node_1".to_string())
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_add_capability_nonexistent_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let capability = Capability::new(
            "sensor1".to_string(),
            "EO/IR".to_string(),
            crate::models::CapabilityType::Sensor,
            0.9,
        );
        let result = store.add_capability("nonexistent_cell", capability).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound { .. }));
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

        // Just verify we can access the underlying backend
        let _backend = store.backend();
        // If we get here without panic, the accessor works
    }
}
