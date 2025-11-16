//! Simple HTTP/REST API server example
//!
//! This example demonstrates how to start a CAP transport HTTP server
//! with a Ditto backend.
//!
//! Usage:
//!     cargo run --example simple_server
//!
//! Then query the API:
//!     curl http://localhost:8080/api/v1/health
//!     curl http://localhost:8080/api/v1/nodes
//!     curl http://localhost:8080/api/v1/cells

use hive_protocol::sync::ditto::DittoBackend;
use hive_protocol::sync::{BackendConfig, DataSyncBackend, TransportConfig};
use hive_transport::http::Server;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info,cap_transport=debug")
        .init();

    println!("Starting CAP Transport HTTP Server Example");
    println!("==========================================\n");

    // Create Ditto backend
    let backend = Arc::new(DittoBackend::new());

    // Configure backend
    let config = BackendConfig {
        app_id: "cap-transport-example".to_string(),
        persistence_dir: PathBuf::from("/tmp/cap-transport-example"),
        shared_key: Some("example-shared-key-replace-in-production".to_string()),
        transport: TransportConfig {
            tcp_listen_port: Some(12345),
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

    println!("\nHTTP API Server Configuration:");
    println!("  Bind Address: 0.0.0.0:8080");
    println!("  Backend: Ditto");
    println!("\nAvailable Endpoints:");
    println!("  GET http://localhost:8080/api/v1/health");
    println!("  GET http://localhost:8080/api/v1/nodes");
    println!("  GET http://localhost:8080/api/v1/nodes/:id");
    println!("  GET http://localhost:8080/api/v1/cells");
    println!("  GET http://localhost:8080/api/v1/cells/:id");
    println!("  GET http://localhost:8080/api/v1/beacons");
    println!("\nExample queries:");
    println!("  curl http://localhost:8080/api/v1/health");
    println!("  curl http://localhost:8080/api/v1/nodes?phase=cell");
    println!("  curl http://localhost:8080/api/v1/cells?leader_id=node-1");
    println!("\nServer starting...\n");

    // Create and start HTTP server
    let server = Server::new(backend).bind("0.0.0.0:8080").await?;

    server.serve().await?;

    Ok(())
}
