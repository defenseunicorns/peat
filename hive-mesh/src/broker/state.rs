//! Broker state trait and data types for the mesh service broker.

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

/// Information about the local mesh node.
#[derive(Debug, Clone, Serialize)]
pub struct MeshNodeInfo {
    pub node_id: String,
    pub uptime_secs: u64,
    pub version: String,
}

/// Summary of a connected peer.
#[derive(Debug, Clone, Serialize)]
pub struct PeerSummary {
    pub id: String,
    pub connected: bool,
    pub state: String,
    pub rtt_ms: Option<u64>,
}

/// Summary of the mesh topology.
#[derive(Debug, Clone, Serialize)]
pub struct TopologySummary {
    pub peer_count: usize,
    pub role: String,
    pub hierarchy_level: u32,
}

/// Events streamed over WebSocket.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum MeshEvent {
    PeerConnected {
        peer_id: String,
    },
    PeerDisconnected {
        peer_id: String,
        reason: String,
    },
    TopologyChanged {
        new_role: String,
        peer_count: usize,
    },
    SyncEvent {
        collection: String,
        doc_id: String,
        action: String,
    },
}

/// Trait that backs the broker with mesh data.
///
/// Consumers implement this trait to provide live mesh state to the HTTP/WS broker.
/// The broker holds `Arc<dyn MeshBrokerState>` (same pattern as `Arc<dyn DataStore>`
/// in hive-persistence).
#[async_trait::async_trait]
pub trait MeshBrokerState: Send + Sync + 'static {
    /// Returns information about the local node.
    fn node_info(&self) -> MeshNodeInfo;

    /// Lists all known peers.
    async fn list_peers(&self) -> Vec<PeerSummary>;

    /// Gets a specific peer by ID, if known.
    async fn get_peer(&self, id: &str) -> Option<PeerSummary>;

    /// Returns a summary of the current topology.
    fn topology(&self) -> TopologySummary;

    /// Subscribe to mesh events (for WebSocket streaming).
    fn subscribe_events(&self) -> broadcast::Receiver<MeshEvent>;

    /// Lists documents in a collection (optional — returns `None` by default).
    async fn list_documents(&self, _collection: &str) -> Option<Vec<Value>> {
        None
    }

    /// Gets a single document by collection and id (optional — returns `None` by default).
    async fn get_document(&self, _collection: &str, _id: &str) -> Option<Value> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_node_info_serialization() {
        let info = MeshNodeInfo {
            node_id: "node-1".into(),
            uptime_secs: 3600,
            version: "0.1.0".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&info).unwrap();
        assert_eq!(json["node_id"], "node-1");
        assert_eq!(json["uptime_secs"], 3600);
        assert_eq!(json["version"], "0.1.0");
    }

    #[test]
    fn test_mesh_node_info_zero_uptime() {
        let info = MeshNodeInfo {
            node_id: "fresh".into(),
            uptime_secs: 0,
            version: "0.0.0".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&info).unwrap();
        assert_eq!(json["uptime_secs"], 0);
    }

    #[test]
    fn test_peer_summary_with_rtt() {
        let peer = PeerSummary {
            id: "peer-1".into(),
            connected: true,
            state: "active".into(),
            rtt_ms: Some(42),
        };
        let json: serde_json::Value = serde_json::to_value(&peer).unwrap();
        assert_eq!(json["id"], "peer-1");
        assert_eq!(json["connected"], true);
        assert_eq!(json["state"], "active");
        assert_eq!(json["rtt_ms"], 42);
    }

    #[test]
    fn test_peer_summary_without_rtt() {
        let peer = PeerSummary {
            id: "peer-2".into(),
            connected: false,
            state: "disconnected".into(),
            rtt_ms: None,
        };
        let json: serde_json::Value = serde_json::to_value(&peer).unwrap();
        assert_eq!(json["id"], "peer-2");
        assert_eq!(json["connected"], false);
        assert!(json["rtt_ms"].is_null());
    }

    #[test]
    fn test_topology_summary_serialization() {
        let topo = TopologySummary {
            peer_count: 5,
            role: "leader".into(),
            hierarchy_level: 1,
        };
        let json: serde_json::Value = serde_json::to_value(&topo).unwrap();
        assert_eq!(json["peer_count"], 5);
        assert_eq!(json["role"], "leader");
        assert_eq!(json["hierarchy_level"], 1);
    }

    #[test]
    fn test_topology_summary_standalone_node() {
        let topo = TopologySummary {
            peer_count: 0,
            role: "standalone".into(),
            hierarchy_level: 0,
        };
        let json: serde_json::Value = serde_json::to_value(&topo).unwrap();
        assert_eq!(json["peer_count"], 0);
        assert_eq!(json["role"], "standalone");
    }

    #[test]
    fn test_mesh_event_peer_connected() {
        let event = MeshEvent::PeerConnected {
            peer_id: "peer-2".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "PeerConnected");
        assert_eq!(json["peer_id"], "peer-2");
        // Should only have type + peer_id
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    #[test]
    fn test_mesh_event_peer_disconnected() {
        let event = MeshEvent::PeerDisconnected {
            peer_id: "peer-3".into(),
            reason: "timeout".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "PeerDisconnected");
        assert_eq!(json["peer_id"], "peer-3");
        assert_eq!(json["reason"], "timeout");
    }

    #[test]
    fn test_mesh_event_topology_changed() {
        let event = MeshEvent::TopologyChanged {
            new_role: "follower".into(),
            peer_count: 3,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "TopologyChanged");
        assert_eq!(json["new_role"], "follower");
        assert_eq!(json["peer_count"], 3);
    }

    #[test]
    fn test_mesh_event_sync_event() {
        let event = MeshEvent::SyncEvent {
            collection: "docs".into(),
            doc_id: "d1".into(),
            action: "insert".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "SyncEvent");
        assert_eq!(json["collection"], "docs");
        assert_eq!(json["doc_id"], "d1");
        assert_eq!(json["action"], "insert");
    }

    #[test]
    fn test_mesh_node_info_debug_clone() {
        let info = MeshNodeInfo {
            node_id: "n1".into(),
            uptime_secs: 100,
            version: "1.0".into(),
        };
        let cloned = info.clone();
        assert_eq!(cloned.node_id, "n1");
        assert_eq!(cloned.uptime_secs, 100);
        let _ = format!("{:?}", info);
    }

    #[test]
    fn test_peer_summary_debug_clone() {
        let peer = PeerSummary {
            id: "p1".into(),
            connected: true,
            state: "active".into(),
            rtt_ms: Some(10),
        };
        let cloned = peer.clone();
        assert_eq!(cloned.id, "p1");
        assert!(cloned.connected);
        let _ = format!("{:?}", peer);
    }

    #[test]
    fn test_topology_summary_debug_clone() {
        let topo = TopologySummary {
            peer_count: 3,
            role: "leader".into(),
            hierarchy_level: 2,
        };
        let cloned = topo.clone();
        assert_eq!(cloned.peer_count, 3);
        assert_eq!(cloned.hierarchy_level, 2);
        let _ = format!("{:?}", topo);
    }

    #[test]
    fn test_mesh_event_debug_clone() {
        let event = MeshEvent::PeerConnected {
            peer_id: "p1".into(),
        };
        let cloned = event.clone();
        let _ = format!("{:?}", cloned);

        let event2 = MeshEvent::SyncEvent {
            collection: "docs".into(),
            doc_id: "d1".into(),
            action: "update".into(),
        };
        let cloned2 = event2.clone();
        let _ = format!("{:?}", cloned2);
    }

    #[test]
    fn test_mesh_event_topology_changed_clone() {
        let event = MeshEvent::TopologyChanged {
            new_role: "follower".into(),
            peer_count: 5,
        };
        let cloned = event.clone();
        let _ = format!("{:?}", cloned);
    }

    #[test]
    fn test_mesh_event_peer_disconnected_clone() {
        let event = MeshEvent::PeerDisconnected {
            peer_id: "p2".into(),
            reason: "timeout".into(),
        };
        let cloned = event.clone();
        let _ = format!("{:?}", cloned);
    }

    /// Verify MeshBrokerState default methods return None.
    #[tokio::test]
    async fn test_default_trait_methods() {
        struct MinimalState {
            tx: broadcast::Sender<MeshEvent>,
        }

        #[async_trait::async_trait]
        impl MeshBrokerState for MinimalState {
            fn node_info(&self) -> MeshNodeInfo {
                MeshNodeInfo {
                    node_id: "n".into(),
                    uptime_secs: 0,
                    version: "0".into(),
                }
            }
            async fn list_peers(&self) -> Vec<PeerSummary> {
                vec![]
            }
            async fn get_peer(&self, _id: &str) -> Option<PeerSummary> {
                None
            }
            fn topology(&self) -> TopologySummary {
                TopologySummary {
                    peer_count: 0,
                    role: "none".into(),
                    hierarchy_level: 0,
                }
            }
            fn subscribe_events(&self) -> broadcast::Receiver<MeshEvent> {
                self.tx.subscribe()
            }
            // list_documents and get_document use defaults
        }

        let (tx, _) = broadcast::channel(1);
        let state = MinimalState { tx };

        assert!(state.list_documents("any").await.is_none());
        assert!(state.get_document("any", "id").await.is_none());
    }

    #[tokio::test]
    async fn test_trait_required_methods() {
        struct FullState {
            tx: broadcast::Sender<MeshEvent>,
        }

        #[async_trait::async_trait]
        impl MeshBrokerState for FullState {
            fn node_info(&self) -> MeshNodeInfo {
                MeshNodeInfo {
                    node_id: "full".into(),
                    uptime_secs: 42,
                    version: "1.0".into(),
                }
            }
            async fn list_peers(&self) -> Vec<PeerSummary> {
                vec![PeerSummary {
                    id: "p1".into(),
                    connected: true,
                    state: "active".into(),
                    rtt_ms: Some(10),
                }]
            }
            async fn get_peer(&self, id: &str) -> Option<PeerSummary> {
                if id == "p1" {
                    Some(PeerSummary {
                        id: "p1".into(),
                        connected: true,
                        state: "active".into(),
                        rtt_ms: Some(10),
                    })
                } else {
                    None
                }
            }
            fn topology(&self) -> TopologySummary {
                TopologySummary {
                    peer_count: 1,
                    role: "leader".into(),
                    hierarchy_level: 2,
                }
            }
            fn subscribe_events(&self) -> broadcast::Receiver<MeshEvent> {
                self.tx.subscribe()
            }
        }

        let (tx, _) = broadcast::channel(16);
        let state = FullState { tx };

        let info = state.node_info();
        assert_eq!(info.node_id, "full");
        assert_eq!(info.uptime_secs, 42);

        let peers = state.list_peers().await;
        assert_eq!(peers.len(), 1);

        let peer = state.get_peer("p1").await;
        assert!(peer.is_some());

        let no_peer = state.get_peer("missing").await;
        assert!(no_peer.is_none());

        let topo = state.topology();
        assert_eq!(topo.peer_count, 1);

        let _rx = state.subscribe_events();
    }
}
