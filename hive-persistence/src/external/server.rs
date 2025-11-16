//! HTTP server for external API access

use crate::store::DataStore;
use crate::Result;
use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Configuration for HTTP server
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
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

/// HTTP server for external API access
pub struct Server {
    store: Arc<dyn DataStore>,
    config: ServerConfig,
}

impl Server {
    /// Create a new server with the given data store
    pub fn new(store: Arc<dyn DataStore>) -> Self {
        Self {
            store,
            config: ServerConfig::default(),
        }
    }

    /// Set the bind address
    pub async fn bind(mut self, addr: &str) -> Result<Self> {
        self.config.bind_addr = addr
            .parse()
            .map_err(|e| crate::Error::Internal(format!("Invalid bind address: {}", e)))?;
        Ok(self)
    }

    /// Set the request timeout
    pub fn timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// Build the Axum router
    fn build_router(&self) -> Router {
        use super::routes;

        let api_router = Router::new()
            .route("/health", get(routes::health_check))
            .route("/collections/:name", get(routes::query_collection))
            .route("/collections/:name/:id", get(routes::get_document))
            .with_state(self.store.clone());

        Router::new()
            .nest("/api/v1", api_router)
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .layer(TimeoutLayer::new(Duration::from_secs(
                self.config.timeout_secs,
            )))
            .layer(TraceLayer::new_for_http())
    }

    /// Start the HTTP server
    pub async fn serve(self) -> Result<()> {
        let router = self.build_router();
        let addr = self.config.bind_addr;

        tracing::info!("Starting HTTP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| crate::Error::Internal(format!("Failed to bind to {}: {}", addr, e)))?;

        axum::serve(listener, router)
            .await
            .map_err(|e| crate::Error::Internal(format!("Server error: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_server_creation() {
        // We can't test full server without a real DataStore implementation
        // but we can test configuration
        let config = ServerConfig::default();
        assert!(config.bind_addr.port() == 8080);
    }
}
