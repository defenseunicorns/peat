//! REST API route handlers

use crate::store::DataStore;
use crate::types::{DocumentId, Query};
use crate::Result;
use axum::{
    extract::{Path, Query as AxumQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub store: String,
    pub version: String,
}

/// GET /api/v1/health
pub async fn health_check(State(store): State<Arc<dyn DataStore>>) -> Json<HealthResponse> {
    let store_info = store.store_info();
    Json(HealthResponse {
        status: "healthy".to_string(),
        store: store_info.name,
        version: store_info.version,
    })
}

/// Query parameters for collection queries
#[derive(Debug, Deserialize)]
pub struct CollectionQuery {
    /// Limit number of results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Sort field
    pub sort_by: Option<String>,
    /// Sort order (asc/desc)
    pub order: Option<String>,
}

/// GET /api/v1/collections/:name
pub async fn query_collection(
    State(store): State<Arc<dyn DataStore>>,
    Path(collection): Path<String>,
    AxumQuery(params): AxumQuery<CollectionQuery>,
) -> Result<Json<Value>> {
    // Build query
    let mut query = Query::new();

    if let Some(limit) = params.limit {
        query = query.limit(limit);
    }

    if let Some(offset) = params.offset {
        query = query.offset(offset);
    }

    // Execute query (returns generic JSON values)
    let documents: Vec<Value> = store.query(&collection, query).await?;

    Ok(Json(json!({
        "collection": collection,
        "count": documents.len(),
        "documents": documents,
    })))
}

/// GET /api/v1/collections/:name/:id
pub async fn get_document(
    State(store): State<Arc<dyn DataStore>>,
    Path((collection, id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let doc_id = DocumentId::new(id);
    let document: Value = store.find_by_id(&collection, &doc_id).await?;

    Ok(Json(json!({
        "collection": collection,
        "id": doc_id.as_str(),
        "document": document,
    })))
}

/// Convert our errors to HTTP responses
impl IntoResponse for crate::Error {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            crate::Error::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            crate::Error::InvalidQuery(msg) => (StatusCode::BAD_REQUEST, msg),
            crate::Error::Serialization(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            crate::Error::Backend(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            crate::Error::Subscription(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            crate::Error::Transaction(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            crate::Error::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": error_message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            store: "TestStore".to_string(),
            version: "1.0.0".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("TestStore"));
    }

    #[test]
    fn test_collection_query_deserialization() {
        // Test query parameter parsing
        let _query = CollectionQuery {
            limit: Some(10),
            offset: Some(0),
            sort_by: Some("created_at".to_string()),
            order: Some("desc".to_string()),
        };
    }
}
