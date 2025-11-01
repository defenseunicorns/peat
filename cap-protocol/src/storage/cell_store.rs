//! Cell state storage manager
//!
//! This module provides a high-level wrapper around DittoStore for managing
//! cell state using CRDT operations.

use crate::models::{cell::CellState, Capability};
use crate::storage::ditto_store::DittoStore;
use crate::{Error, Result};
use serde_json::json;
use tracing::{debug, info, instrument};

/// Collection name
const CELL_COLLECTION: &str = "cells";

/// Cell storage manager
pub struct CellStore {
    store: DittoStore,
}

impl CellStore {
    /// Create a new cell store
    pub fn new(store: DittoStore) -> Self {
        Self { store }
    }

    /// Store a cell state (OR-Set + LWW-Register operations)
    #[instrument(skip(self, cell))]
    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        info!("Storing cell: {}", cell.config.id);

        // Serialize cell state directly
        let mut doc = serde_json::to_value(cell)?;
        // Add cell_id field for querying
        if let Some(obj) = doc.as_object_mut() {
            obj.insert("cell_id".to_string(), json!(cell.config.id.clone()));
        }

        self.store.upsert(CELL_COLLECTION, doc).await.map_err(|e| {
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

        let where_clause = format!("cell_id == '{}'", cell_id);
        let docs = self.store.query(CELL_COLLECTION, &where_clause).await?;

        if docs.is_empty() {
            return Ok(None);
        }

        let cell: CellState = serde_json::from_value(docs[0].clone())?;
        Ok(Some(cell))
    }

    /// Get all valid cells (meeting minimum size requirements)
    #[instrument(skip(self))]
    pub async fn get_valid_cells(&self) -> Result<Vec<CellState>> {
        debug!("Querying valid cells");

        // Query all cells - we'll filter in code since DQL doesn't support array length
        let docs = self.store.query(CELL_COLLECTION, "true").await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
            .filter(|cell: &CellState| cell.is_valid())
            .collect();

        Ok(cells)
    }

    /// Get all cells in a platoon
    #[instrument(skip(self))]
    pub async fn get_cells_by_zone(&self, platoon_id: &str) -> Result<Vec<CellState>> {
        debug!("Querying cells by platoon: {}", platoon_id);

        let where_clause = format!("platoon_id == '{}'", platoon_id);
        let docs = self.store.query(CELL_COLLECTION, &where_clause).await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
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
        let docs = self.store.query(CELL_COLLECTION, "true").await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
            .filter(|cell: &CellState| cell.has_capability_type(capability_type))
            .collect();

        Ok(cells)
    }

    /// Get cells that are not full (can accept more members)
    #[instrument(skip(self))]
    pub async fn get_available_cells(&self) -> Result<Vec<CellState>> {
        debug!("Querying available cells");

        let docs = self.store.query(CELL_COLLECTION, "true").await?;

        let cells: Vec<CellState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
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

        self.store.remove(CELL_COLLECTION, cell_id).await
    }

    /// Get the underlying DittoStore reference
    pub fn store(&self) -> &DittoStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CellConfig;
    use crate::storage::ditto_store::DittoConfig;

    async fn create_test_store() -> Result<CellStore> {
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

        let config = DittoConfig {
            app_id,
            persistence_dir: persistence_path,
            shared_key,
            tcp_listen_port: None,
            tcp_connect_address: None,
        };

        let ditto_store = DittoStore::new(config)?;
        Ok(CellStore::new(ditto_store))
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
    // are tested indirectly via comprehensive E2E tests in tests/squad_formation_e2e.rs,
    // tests/storage_layer_e2e.rs, and tests/load_testing_e2e.rs
    //
    // Direct unit testing of these methods is challenging due to Ditto's eventual consistency -
    // the "get-modify-store" pattern these methods use doesn't guarantee immediate read-after-write
    // consistency even with sleep delays. E2E tests validate these methods work correctly in
    // realistic multi-peer scenarios.

    #[tokio::test]
    #[ignore] // Flaky due to Ditto eventual consistency - tested via E2E tests
    async fn test_add_member() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let cell = CellState::new(config);
        store.store_cell(&cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        store
            .add_member(&cell.config.id, "node_1".to_string())
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let retrieved = store.get_cell(&cell.config.id).await.unwrap().unwrap();
        assert_eq!(retrieved.member_count(), 1);
    }

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
    #[ignore] // Flaky due to Ditto eventual consistency - tested via E2E tests
    async fn test_remove_member() {
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
        cell.add_member("node_2".to_string());
        store.store_cell(&cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        store
            .remove_member(&cell.config.id, "node_1")
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let retrieved = store.get_cell(&cell.config.id).await.unwrap().unwrap();
        assert_eq!(retrieved.member_count(), 1);
        assert!(!retrieved.is_member("node_1"));
        assert!(retrieved.is_member("node_2"));
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
    #[ignore] // Flaky due to Ditto eventual consistency - tested via E2E tests
    async fn test_set_leader() {
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
        store.store_cell(&cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        store
            .set_leader(&cell.config.id, "node_1".to_string())
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let retrieved = store.get_cell(&cell.config.id).await.unwrap().unwrap();
        assert_eq!(retrieved.leader_id, Some("node_1".to_string()));
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
    #[ignore] // Flaky due to Ditto eventual consistency - tested via E2E tests
    async fn test_add_capability_to_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let cell = CellState::new(config);
        store.store_cell(&cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let capability = Capability::new(
            "sensor1".to_string(),
            "EO/IR".to_string(),
            crate::models::CapabilityType::Sensor,
            0.9,
        );
        store
            .add_capability(&cell.config.id, capability)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let retrieved = store.get_cell(&cell.config.id).await.unwrap().unwrap();
        assert_eq!(retrieved.capabilities.len(), 1);
        assert_eq!(retrieved.capabilities[0].id, "sensor1");
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
    #[ignore] // Flaky due to Ditto eventual consistency - tested via E2E tests
    async fn test_delete_cell() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let config = CellConfig::new(5);
        let cell = CellState::new(config);
        store.store_cell(&cell).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        store.delete_cell(&cell.config.id).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let retrieved = store.get_cell(&cell.config.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_store_accessor() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        // Just verify we can access the underlying store
        let _ditto_store = store.store();
        // If we get here without panic, the accessor works
    }
}
