//! HTTP server example for cap-persistence
//!
//! This example demonstrates how to start an HTTP server that provides
//! REST API access to CAP persistence data.
//!
//! Usage:
//!     cargo run --example http_server --features external-api
//!
//! Then query the API:
//!     curl http://localhost:8080/api/v1/health
//!     curl http://localhost:8080/api/v1/collections/node_states
//!     curl http://localhost:8080/api/v1/collections/cell_states

use cap_persistence::backends::DittoStore;
use cap_persistence::external::Server;
use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,cap_persistence=debug")
        .init();

    println!("Starting CAP Persistence HTTP Server Example");
    println!("============================================\n");

    // Create Ditto backend
    let backend = Arc::new(DittoBackend::new());

    // Configure backend
    let config = BackendConfig {
        app_id: "cap-persistence-example".to_string(),
        persistence_dir: PathBuf::from("/tmp/cap-persistence-example"),
        shared_key: Some("example-shared-key-replace-in-production".to_string()),
        transport: TransportConfig {
            tcp_listen_port: Some(12346),
            tcp_connect_address: None,
            enable_mdns: true,
            enable_bluetooth: false,
            enable_websocket: true,
            custom: HashMap::new(),
        },
        extra: HashMap::new(),
    };

    println!("Initializing Ditto backend...");
    backend.initialize(config).await?;

    println!("Starting peer discovery and sync...");
    backend.peer_discovery().start().await?;
    backend.sync_engine().start_sync().await?;

    // Create persistence store
    let store = Arc::new(DittoStore::new(backend));

    println!("\nHTTP API Server Configuration:");
    println!("  Bind Address: 0.0.0.0:8080");
    println!("  Storage Backend: Ditto");
    println!("\nAvailable Endpoints:");
    println!("  GET http://localhost:8080/api/v1/health");
    println!("  GET http://localhost:8080/api/v1/collections/:name");
    println!("  GET http://localhost:8080/api/v1/collections/:name/:id");
    println!("\nExample queries:");
    println!("  curl http://localhost:8080/api/v1/health");
    println!("  curl http://localhost:8080/api/v1/collections/node_states");
    println!("  curl 'http://localhost:8080/api/v1/collections/node_states?limit=5'");
    println!("\nServer starting...\n");

    // Create and start HTTP server
    let server = Server::new(store).bind("0.0.0.0:8080").await?;

    server.serve().await?;

    Ok(())
}
