//! Event buffer with state reconstruction.
//!
//! Maintains a rolling buffer of recent events and a document snapshot.
//! New WebSocket clients receive the snapshot + buffered events to
//! reconstruct current state without replaying the entire simulation.

use crate::ws::protocol::{HiveEvent, OodaCycleEvent, SimClock, ViewerEvent};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum number of events to buffer for late-joining clients.
const MAX_BUFFER_SIZE: usize = 1000;

/// Shared state buffer accessible by ingest (write) and WebSocket handler (read).
#[derive(Debug, Clone)]
pub struct StateBuffer {
    inner: Arc<RwLock<BufferInner>>,
}

#[derive(Debug)]
struct BufferInner {
    /// Document snapshots keyed by "{collection}/{doc_id}".
    documents: HashMap<String, serde_json::Value>,

    /// Rolling event buffer (most recent events).
    events: Vec<HiveEvent>,

    /// Most recent OODA cycles per node_id.
    latest_cycles: HashMap<String, OodaCycleEvent>,

    /// Current simulation clock.
    sim_clock: Option<SimClock>,

    /// Total events ingested (for stats).
    total_ingested: u64,
}

impl StateBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(BufferInner {
                documents: HashMap::new(),
                events: Vec::new(),
                latest_cycles: HashMap::new(),
                sim_clock: None,
                total_ingested: 0,
            })),
        }
    }

    /// Apply a ViewerEvent to the buffer state.
    pub async fn apply(&self, event: &ViewerEvent) {
        let mut inner = self.inner.write().await;
        inner.total_ingested += 1;

        match event {
            ViewerEvent::OodaCycle(cycle) => {
                inner
                    .latest_cycles
                    .insert(cycle.node_id.clone(), cycle.clone());
            }
            ViewerEvent::DocumentUpdate(update) => {
                let key = format!("{}/{}", update.collection, update.doc_id);
                inner.documents.insert(key, update.fields.clone());
            }
            ViewerEvent::HiveEvent(hive_event) => {
                inner.events.push(hive_event.clone());
                if inner.events.len() > MAX_BUFFER_SIZE {
                    let drain_count = inner.events.len() - MAX_BUFFER_SIZE;
                    inner.events.drain(..drain_count);
                }
            }
            ViewerEvent::SimClock(clock) => {
                inner.sim_clock = Some(clock.clone());
            }
            ViewerEvent::StateSnapshot { .. } => {
                // Snapshots are outbound-only, not applied to buffer.
            }
        }
    }

    /// Build a state snapshot for a newly connected client.
    pub async fn snapshot(&self) -> ViewerEvent {
        let inner = self.inner.read().await;

        // Merge document snapshots with latest cycle data
        let mut documents = inner.documents.clone();
        for (node_id, cycle) in &inner.latest_cycles {
            let key = format!("ooda_cycles/{}", node_id);
            documents.insert(key, serde_json::to_value(cycle).unwrap_or_default());
        }

        ViewerEvent::StateSnapshot {
            documents,
            events: inner.events.clone(),
            sim_clock: inner.sim_clock.clone(),
        }
    }

    /// Get stats for the /health endpoint.
    pub async fn stats(&self) -> BufferStats {
        let inner = self.inner.read().await;
        BufferStats {
            total_ingested: inner.total_ingested,
            buffered_events: inner.events.len(),
            documents: inner.documents.len(),
            active_nodes: inner.latest_cycles.len(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BufferStats {
    pub total_ingested: u64,
    pub buffered_events: usize,
    pub documents: usize,
    pub active_nodes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_includes_latest_cycles() {
        let buffer = StateBuffer::new();

        let event = ViewerEvent::OodaCycle(OodaCycleEvent {
            node_id: "crane-1".into(),
            cycle: 3,
            sim_time: "08:04".into(),
            action: "complete_container_move".into(),
            success: true,
            contention_retry: false,
            observe_ms: 1.0,
            decide_ms: 0.5,
            act_ms: 0.3,
            extra: HashMap::new(),
        });

        buffer.apply(&event).await;

        let snapshot = buffer.snapshot().await;
        match snapshot {
            ViewerEvent::StateSnapshot { documents, .. } => {
                assert!(documents.contains_key("ooda_cycles/crane-1"));
            }
            _ => panic!("expected StateSnapshot"),
        }
    }

    #[tokio::test]
    async fn buffer_caps_at_max_size() {
        let buffer = StateBuffer::new();

        for i in 0..1500 {
            let event = ViewerEvent::HiveEvent(HiveEvent {
                event_type: "TEST".into(),
                source: format!("node-{}", i),
                priority: "ROUTINE".into(),
                details: serde_json::Value::Null,
                timestamp: None,
            });
            buffer.apply(&event).await;
        }

        let stats = buffer.stats().await;
        assert_eq!(stats.buffered_events, MAX_BUFFER_SIZE);
        assert_eq!(stats.total_ingested, 1500);
    }
}
