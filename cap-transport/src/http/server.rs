//! HTTP/REST API server implementation

use crate::error::{Error, Result};
use crate::http::routes;
use axum::{routing::get, Router};
use cap_protocol::sync::DataSyncBackend;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// HTTP server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind address
    pub bind_addr: SocketAddr,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            timeout_secs: 30,
        }
    }
}

/// HTTP/REST API server
pub struct Server {
    backend: Arc<dyn DataSyncBackend>,
    config: ServerConfig,
}

impl Server {
    /// Create a new HTTP server with the given backend
    pub fn new(backend: Arc<dyn DataSyncBackend>) -> Self {
        Self {
            backend,
            config: ServerConfig::default(),
        }
    }

    /// Set bind address (builder pattern)
    pub fn with_config(mut self, config: ServerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set bind address from string (builder pattern)
    pub async fn bind(mut self, addr: &str) -> Result<Self> {
        self.config.bind_addr = addr
            .parse()
            .map_err(|e| Error::Http(format!("Invalid bind address: {}", e)))?;
        Ok(self)
    }

    /// Build the Axum router with all routes
    fn build_router(&self) -> Router {
        // Create router with all API endpoints
        let api_router = Router::new()
            .route("/health", get(routes::health_check))
            .route("/nodes", get(routes::list_nodes))
            .route("/nodes/:id", get(routes::get_node))
            .route("/cells", get(routes::list_cells))
            .route("/cells/:id", get(routes::get_cell))
            .route("/beacons", get(routes::list_beacons))
            .with_state(self.backend.clone());

        // Main router with /api/v1 prefix
        Router::new()
            .nest("/api/v1", api_router)
            // Middleware layers
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .layer(TimeoutLayer::new(std::time::Duration::from_secs(
                self.config.timeout_secs,
            )))
            .layer(TraceLayer::new_for_http())
    }

    /// Start the HTTP server and serve requests
    pub async fn serve(self) -> Result<()> {
        let router = self.build_router();
        let addr = self.config.bind_addr;

        info!("Starting HTTP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Http(format!("Failed to bind to {}: {}", addr, e)))?;

        info!("HTTP server listening on http://{}", addr);

        axum::serve(listener, router)
            .await
            .map_err(|e| Error::Http(format!("Server error: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr.port(), 8080);
        assert_eq!(config.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_server_creation() {
        // Test requires a mock backend
        // TODO: Implement once mock backend is available
    }
}
