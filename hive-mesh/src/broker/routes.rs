//! HTTP route handlers for the mesh broker.

use super::error::BrokerError;
use super::state::MeshBrokerState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub node_id: String,
}

/// GET /api/v1/health
pub async fn health(State(state): State<Arc<dyn MeshBrokerState>>) -> Json<HealthResponse> {
    let info = state.node_info();
    Json(HealthResponse {
        status: "healthy".into(),
        node_id: info.node_id,
    })
}

/// GET /api/v1/node
pub async fn node_info(State(state): State<Arc<dyn MeshBrokerState>>) -> Json<Value> {
    let info = state.node_info();
    Json(serde_json::to_value(info).unwrap_or_default())
}

/// GET /api/v1/peers
pub async fn list_peers(State(state): State<Arc<dyn MeshBrokerState>>) -> Json<Value> {
    let peers = state.list_peers().await;
    Json(json!({
        "peers": peers,
        "count": peers.len(),
    }))
}

/// GET /api/v1/peers/:id
pub async fn get_peer(
    State(state): State<Arc<dyn MeshBrokerState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, BrokerError> {
    let peer = state
        .get_peer(&id)
        .await
        .ok_or_else(|| BrokerError::NotFound(format!("peer not found: {}", id)))?;
    Ok(Json(serde_json::to_value(peer).unwrap_or_default()))
}

/// GET /api/v1/topology
pub async fn topology(State(state): State<Arc<dyn MeshBrokerState>>) -> Json<Value> {
    let topo = state.topology();
    Json(serde_json::to_value(topo).unwrap_or_default())
}

/// GET /api/v1/documents/:collection
pub async fn list_documents(
    State(state): State<Arc<dyn MeshBrokerState>>,
    Path(collection): Path<String>,
) -> Result<Json<Value>, BrokerError> {
    let docs = state.list_documents(&collection).await.ok_or_else(|| {
        BrokerError::NotFound(format!("collection not available: {}", collection))
    })?;
    Ok(Json(json!({
        "collection": collection,
        "count": docs.len(),
        "documents": docs,
    })))
}

/// GET /api/v1/documents/:collection/:id
pub async fn get_document(
    State(state): State<Arc<dyn MeshBrokerState>>,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>, BrokerError> {
    let doc = state.get_document(&collection, &id).await.ok_or_else(|| {
        BrokerError::NotFound(format!("document not found: {}/{}", collection, id))
    })?;
    Ok(Json(json!({
        "collection": collection,
        "id": id,
        "document": doc,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "healthy".into(),
            node_id: "node-1".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("node-1"));
    }

    #[test]
    fn test_health_response_debug() {
        let resp = HealthResponse {
            status: "healthy".into(),
            node_id: "n1".into(),
        };
        let debug = format!("{:?}", resp);
        assert!(debug.contains("HealthResponse"));
        assert!(debug.contains("healthy"));
        assert!(debug.contains("n1"));
    }

    #[test]
    fn test_health_response_fields() {
        let resp = HealthResponse {
            status: "degraded".into(),
            node_id: "mesh-42".into(),
        };
        assert_eq!(resp.status, "degraded");
        assert_eq!(resp.node_id, "mesh-42");
    }

    #[test]
    fn test_health_response_serde_roundtrip() {
        let resp = HealthResponse {
            status: "healthy".into(),
            node_id: "roundtrip-node".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["status"], "healthy");
        assert_eq!(parsed["node_id"], "roundtrip-node");
    }
}
