//! WebSocket upgrade handler and session management.
//!
//! Handles the HTTP → WebSocket upgrade, sends state snapshot on connect,
//! then streams broadcast events to the client.

use crate::relay::broadcast::Broadcaster;
use crate::relay::buffer::StateBuffer;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Global connection counter for logging.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Shared state passed to the WebSocket handler via Axum state.
#[derive(Debug, Clone)]
pub struct WsState {
    pub buffer: StateBuffer,
    pub broadcaster: Broadcaster,
}

/// HTTP handler for WebSocket upgrade at /ws.
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<WsState>>,
) -> impl IntoResponse {
    let conn_id = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    tracing::info!("WebSocket upgrade request (conn #{})", conn_id);
    ws.on_upgrade(move |socket| handle_socket(socket, state, conn_id))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<WsState>, conn_id: u64) {
    tracing::info!("WebSocket connected (conn #{})", conn_id);

    // Send state snapshot to the newly connected client.
    let snapshot = state.buffer.snapshot().await;
    let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();
    if let Err(e) = socket.send(Message::Text(snapshot_json.into())).await {
        tracing::warn!("Failed to send snapshot to conn #{}: {}", conn_id, e);
        return;
    }

    // Subscribe to the broadcast channel for future events.
    let mut rx = state.broadcaster.subscribe();

    // Stream events to the client until disconnect or error.
    loop {
        tokio::select! {
            // Forward broadcast events to this client.
            result = rx.recv() => {
                match result {
                    Ok(json) => {
                        if let Err(e) = socket.send(Message::Text(json.into())).await {
                            tracing::debug!("Client #{} send error (disconnected?): {}", conn_id, e);
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Client #{} lagged, dropped {} events", conn_id, n);
                        // Continue — client will miss some events but stay connected.
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Broadcast channel closed, disconnecting client #{}", conn_id);
                        break;
                    }
                }
            }
            // Handle incoming messages from client (ping/pong, close).
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Client #{} disconnected", conn_id);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {
                        // Ignore text/binary messages from client (viewer is read-only).
                    }
                    Some(Err(e)) => {
                        tracing::debug!("Client #{} recv error: {}", conn_id, e);
                        break;
                    }
                }
            }
        }
    }
}
