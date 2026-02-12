//! WebSocket upgrade handler for streaming mesh events.

use super::state::MeshBrokerState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use std::sync::Arc;

/// GET /api/v1/ws — upgrades to WebSocket and streams `MeshEvent`s as JSON.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<dyn MeshBrokerState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<dyn MeshBrokerState>) {
    let mut rx = state.subscribe_events();

    loop {
        tokio::select! {
            // Forward mesh events to the client as JSON text frames.
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(e) => {
                                tracing::warn!("failed to serialize mesh event: {}", e);
                                continue;
                            }
                        };
                        if socket.send(Message::Text(json)).await.is_err() {
                            // Client disconnected.
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("ws client lagged, skipped {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Sender dropped — shut down gracefully.
                        break;
                    }
                }
            }
            // If the client sends a close or the connection drops, exit.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore other client messages
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ws_handler_exists() {
        // Compile-time check — functional WS tests live in server.rs
        // (test_ws_streams_events, test_ws_multiple_event_types,
        // test_ws_sender_drop_closes_stream).
        let _ = super::ws_handler as fn(_, _) -> _;
    }
}
