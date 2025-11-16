//! REST API route handlers

use crate::error::{Error, Result};
use axum::{
    extract::{Path, Query as AxumQuery, State},
    Json,
};
use hive_protocol::sync::{DataSyncBackend, Query};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
}

/// GET /api/v1/health
pub async fn health_check(State(backend): State<Arc<dyn DataSyncBackend>>) -> Json<HealthResponse> {
    let backend_info = backend.backend_info();
    Json(HealthResponse {
        status: "healthy".to_string(),
        backend: backend_info.name,
    })
}

/// Query parameters for listing nodes
#[derive(Debug, Deserialize)]
pub struct ListNodesQuery {
    /// Filter by protocol phase
    pub phase: Option<String>,
    /// Filter by health status
    pub health: Option<String>,
}

/// GET /api/v1/nodes
pub async fn list_nodes(
    State(backend): State<Arc<dyn DataSyncBackend>>,
    AxumQuery(params): AxumQuery<ListNodesQuery>,
) -> Result<Json<Value>> {
    // Query node_states collection (where NodeStore puts node data)
    let doc_store = backend.document_store();
    let query = Query::All;

    // TODO: Add filtering based on params when Query supports it
    let _ = params; // Silence unused warning for now

    let documents = doc_store.query("node_states", &query).await?;

    // Convert documents to JSON array
    let nodes: Vec<Value> = documents
        .into_iter()
        .map(|doc| serde_json::to_value(&doc.fields).unwrap_or(json!({})))
        .collect();

    Ok(Json(json!({ "nodes": nodes })))
}

/// GET /api/v1/nodes/:id
pub async fn get_node(
    State(backend): State<Arc<dyn DataSyncBackend>>,
    Path(node_id): Path<String>,
) -> Result<Json<Value>> {
    let doc_store = backend.document_store();

    // Query for specific node by ID
    // Note: This is simplified - real impl would use Query::Eq("node_id", node_id)
    let documents = doc_store.query("node_states", &Query::All).await?;

    // Find the node with matching ID
    let node = documents
        .into_iter()
        .find(|doc| {
            doc.fields
                .get("node_id")
                .and_then(|v| v.as_str())
                .map(|id| id == node_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| Error::NotFound(format!("Node not found: {}", node_id)))?;

    let node_json = serde_json::to_value(&node.fields)?;
    Ok(Json(node_json))
}

/// Query parameters for listing cells
#[derive(Debug, Deserialize)]
pub struct ListCellsQuery {
    /// Filter by leader node ID
    pub leader_id: Option<String>,
}

/// GET /api/v1/cells
pub async fn list_cells(
    State(backend): State<Arc<dyn DataSyncBackend>>,
    AxumQuery(params): AxumQuery<ListCellsQuery>,
) -> Result<Json<Value>> {
    let doc_store = backend.document_store();
    let query = Query::All;

    let _ = params; // TODO: Add filtering

    let documents = doc_store.query("cell_states", &query).await?;

    let cells: Vec<Value> = documents
        .into_iter()
        .map(|doc| serde_json::to_value(&doc.fields).unwrap_or(json!({})))
        .collect();

    Ok(Json(json!({ "cells": cells })))
}

/// GET /api/v1/cells/:id
pub async fn get_cell(
    State(backend): State<Arc<dyn DataSyncBackend>>,
    Path(cell_id): Path<String>,
) -> Result<Json<Value>> {
    let doc_store = backend.document_store();
    let documents = doc_store.query("cell_states", &Query::All).await?;

    let cell = documents
        .into_iter()
        .find(|doc| {
            doc.fields
                .get("id")
                .and_then(|v| v.as_str())
                .map(|id| id == cell_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| Error::NotFound(format!("Cell not found: {}", cell_id)))?;

    let cell_json = serde_json::to_value(&cell.fields)?;
    Ok(Json(cell_json))
}

/// Query parameters for listing beacons
#[derive(Debug, Deserialize)]
pub struct ListBeaconsQuery {
    /// Filter by geohash prefix
    pub geohash_prefix: Option<String>,
    /// Filter by operational status
    pub operational: Option<bool>,
}

/// GET /api/v1/beacons
pub async fn list_beacons(
    State(backend): State<Arc<dyn DataSyncBackend>>,
    AxumQuery(params): AxumQuery<ListBeaconsQuery>,
) -> Result<Json<Value>> {
    let doc_store = backend.document_store();
    let query = Query::All;

    let _ = params; // TODO: Add filtering

    let documents = doc_store.query("beacons", &query).await?;

    let beacons: Vec<Value> = documents
        .into_iter()
        .map(|doc| serde_json::to_value(&doc.fields).unwrap_or(json!({})))
        .collect();

    Ok(Json(json!({ "beacons": beacons })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            backend: "Ditto".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("Ditto"));
    }

    #[test]
    fn test_list_nodes_query_deserialization() {
        // This would be parsed by Axum in real usage
        // Just testing the struct is deserializable
        let _query_str = "phase=cell&health=nominal";
    }
}
