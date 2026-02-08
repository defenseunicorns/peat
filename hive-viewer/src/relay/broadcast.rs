//! WebSocket broadcast channel.
//!
//! Uses tokio::sync::broadcast to fan out ViewerEvents to all connected
//! browser clients. Each client gets its own receiver from the channel.

use crate::ws::protocol::ViewerEvent;
use tokio::sync::broadcast;

/// Default broadcast channel capacity.
const BROADCAST_CAPACITY: usize = 256;

/// Broadcast hub — wraps a tokio broadcast channel.
#[derive(Debug, Clone)]
pub struct Broadcaster {
    tx: broadcast::Sender<String>,
}

impl Broadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx }
    }

    /// Broadcast a ViewerEvent to all connected clients.
    /// Serializes to JSON once, sends the string to all receivers.
    pub fn send(&self, event: &ViewerEvent) -> Result<usize, broadcast::error::SendError<String>> {
        let json = serde_json::to_string(event).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize event: {}", e);
            "{}".to_string()
        });
        self.tx.send(json)
    }

    /// Subscribe to the broadcast channel.
    /// Returns a receiver that will get all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Number of active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}
