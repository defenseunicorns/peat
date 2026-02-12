//! Broker error types.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Errors returned by the broker.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for BrokerError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            BrokerError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            BrokerError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn response_body_json(resp: Response) -> serde_json::Value {
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    #[test]
    fn test_not_found_status() {
        let err = BrokerError::NotFound("peer xyz".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_internal_status() {
        let err = BrokerError::Internal("something broke".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_error_display() {
        let err = BrokerError::NotFound("peer xyz".into());
        assert_eq!(err.to_string(), "not found: peer xyz");

        let err = BrokerError::Internal("db offline".into());
        assert_eq!(err.to_string(), "internal error: db offline");
    }

    #[tokio::test]
    async fn test_not_found_response_body() {
        let err = BrokerError::NotFound("peer xyz".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let json = response_body_json(resp).await;
        assert_eq!(json["error"], "peer xyz");
        assert_eq!(json["status"], 404);
    }

    #[tokio::test]
    async fn test_internal_response_body() {
        let err = BrokerError::Internal("db crashed".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let json = response_body_json(resp).await;
        assert_eq!(json["error"], "db crashed");
        assert_eq!(json["status"], 500);
    }
}
