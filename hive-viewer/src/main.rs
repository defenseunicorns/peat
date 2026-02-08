//! HIVE Operational Viewer — Real-time WebSocket relay server.
//!
//! Reads simulation events from stdin/TCP/file, maintains state buffer,
//! and broadcasts to browser clients via WebSocket.
//!
//! Usage:
//!   # Pipe from Python sim
//!   ./run-phase1a.sh --max-cycles 30 2>/dev/null | hive-viewer --ingest stdin
//!
//!   # TCP ingest
//!   hive-viewer --ingest tcp://0.0.0.0:9100
//!
//!   # File replay
//!   hive-viewer --ingest file://sim-run.jsonl

mod relay;
mod replay;
mod ws;

use crate::relay::broadcast::Broadcaster;
use crate::relay::buffer::StateBuffer;
use crate::relay::ingest::{IngestSource, run_ingest};
use crate::ws::handler::{WsState, ws_upgrade};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(Parser, Debug)]
#[command(name = "hive-viewer", about = "HIVE Operational Viewer relay server")]
struct Cli {
    /// Ingest source: "stdin", "tcp://host:port", or "file://path"
    #[arg(long, default_value = "stdin")]
    ingest: String,

    /// HTTP/WebSocket listen address
    #[arg(long, default_value = "0.0.0.0:9090")]
    listen: String,

    /// Static files directory to serve (for frontend)
    #[arg(long)]
    static_dir: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hive_viewer=debug,tower_http=debug".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let ingest_source = IngestSource::parse(&cli.ingest)?;

    tracing::info!("HIVE Viewer starting");
    tracing::info!("  Ingest: {:?}", ingest_source);
    tracing::info!("  Listen: {}", cli.listen);

    // Shared state
    let buffer = StateBuffer::new();
    let broadcaster = Broadcaster::new();

    let ws_state = Arc::new(WsState {
        buffer: buffer.clone(),
        broadcaster: broadcaster.clone(),
    });

    // Build Axum router — API routes first, static files as fallback
    let mut app = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/health", get(health_handler))
        .route("/snapshot", get(snapshot_handler))
        .route("/wstest", get(wstest_handler))
        .layer(CorsLayer::permissive())
        .with_state(ws_state.clone());

    // Optionally serve static files (frontend build output).
    // Uses fallback_service so /ws, /health, /snapshot routes take priority.
    if let Some(ref dir) = cli.static_dir {
        tracing::info!("  Static: {}", dir);
        app = app.fallback_service(
            tower_http::services::ServeDir::new(dir)
                .append_index_html_on_directories(true),
        );
    }

    // Start HTTP/WebSocket server
    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    tracing::info!("Listening on {}", cli.listen);

    // Run ingest and server concurrently.
    // When ingest ends (stdin EOF, file exhausted), the server keeps running
    // so clients can still view the final state.
    let ingest_buffer = buffer.clone();
    let ingest_broadcaster = broadcaster.clone();
    let ingest_handle = tokio::spawn(async move {
        if let Err(e) = run_ingest(ingest_source, ingest_buffer, ingest_broadcaster).await {
            tracing::error!("Ingest error: {}", e);
        }
        tracing::info!("Ingest ended — server continues for connected viewers");
    });

    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("Server error: {}", e);
        }
    });

    // Wait for both tasks. In practice, the server runs indefinitely
    // and ingest ends when the source is exhausted.
    tokio::select! {
        _ = ingest_handle => {
            tracing::info!("Ingest task completed. Press Ctrl+C to stop server.");
            // Keep server running — wait for Ctrl+C
            tokio::signal::ctrl_c().await.ok();
        }
        _ = server_handle => {
            tracing::info!("Server task ended");
        }
    }

    Ok(())
}

/// Health check endpoint — returns buffer stats as JSON.
async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<WsState>>,
) -> axum::Json<serde_json::Value> {
    let stats = state.buffer.stats().await;
    axum::Json(serde_json::json!({
        "status": "ok",
        "viewers": state.broadcaster.receiver_count(),
        "buffer": stats,
    }))
}

/// Debug endpoint — returns the same snapshot a WebSocket client would receive.
async fn snapshot_handler(
    axum::extract::State(state): axum::extract::State<Arc<WsState>>,
) -> axum::Json<serde_json::Value> {
    let snapshot = state.buffer.snapshot().await;
    let json = serde_json::to_value(&snapshot).unwrap_or_default();
    axum::Json(json)
}

/// Diagnostic page — standalone WebSocket test independent of React frontend.
async fn wstest_handler() -> Html<&'static str> {
    Html(r#"<!DOCTYPE html>
<html><head><title>HIVE WS Test</title></head>
<body style="background:#111;color:#eee;font-family:monospace;padding:20px">
<h2>HIVE Viewer — WebSocket Diagnostic</h2>
<div id="log"></div>
<script>
function log(msg, color) {
    const d = document.getElementById('log');
    const p = document.createElement('div');
    p.style.color = color || '#aaa';
    p.textContent = new Date().toISOString().substr(11,12) + ' ' + msg;
    d.appendChild(p);
    console.log(msg);
}
const wsUrl = (location.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + location.host + '/ws';
log('WebSocket URL: ' + wsUrl, '#6ee');
log('Connecting...', '#ee6');
const ws = new WebSocket(wsUrl);
ws.onopen = () => log('CONNECTED', '#6e6');
ws.onclose = (e) => log('CLOSED code=' + e.code + ' reason=' + e.reason + ' clean=' + e.wasClean, '#e66');
ws.onerror = (e) => log('ERROR: ' + JSON.stringify(e), '#e66');
ws.onmessage = (e) => {
    try {
        const data = JSON.parse(e.data);
        log('MESSAGE type=' + data.type + ' docs=' + Object.keys(data.documents||{}).length + ' events=' + (data.events||[]).length, '#6e6');
        if (data.documents) {
            Object.keys(data.documents).forEach(k => log('  doc: ' + k, '#8c8'));
        }
    } catch(err) {
        log('MESSAGE (parse error): ' + e.data.substring(0, 200), '#ea6');
    }
};
</script>
</body></html>"#)
}
