//! Cell state storage manager
//!
//! This module provides a high-level wrapper around data sync backends for managing
//! cell state using CRDT operations.

use crate::models::{
    cell::{CellState, CellStateExt},
    Capability,
};
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
        let mut fields = json_val
            .as_object()
            .ok_or_else(|| Error::Internal("Failed to serialize cell to object".into()))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<String, Value>>();

        // Add cell_id field for querying
        if let Some(id) = cell.get_id() {
            fields.insert("cell_id".to_string(), Value::String(id.to_string()));
            // Use cell_id as document ID to enable proper updates
            Ok(Document::with_id(id, fields))
        } else {
            Ok(Document::new(fields))
        }
    }

    /// Convert Document to CellState
    fn document_to_cell(doc: &Document) -> Result<CellState> {
        let json_val = serde_json::to_value(&doc.fields)?;
        Ok(serde_json::from_value(json_val)?)
    }

    /// Store a cell state (OR-Set + LWW-Register operations)
    #[instrument(skip(self, cell))]
    pub async fn store_cell(&self, cell: &CellState) -> Result<String> {
        info!("Storing cell: {}", cell.get_id().unwrap_or("<unknown>"));

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
            .await?;
        Ok(())
    }

    /// Get the underlying backend reference
    pub fn backend(&self) -> &B {
        &self.backend
    }
}
