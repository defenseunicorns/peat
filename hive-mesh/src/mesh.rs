//! HiveMesh facade — unified entry point for the mesh networking library.
//!
//! Provides [`HiveMesh`] as the single entry point that composes transport,
//! topology, routing, hierarchy, and (optionally) the HTTP/WS broker into a
//! cohesive mesh networking stack.

use crate::config::MeshConfig;
use crate::hierarchy::HierarchyStrategy;
use crate::routing::MeshRouter;
use crate::transport::{MeshTransport, NodeId, TransportError, TransportManager};
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::broadcast;

// ─── Lifecycle state ─────────────────────────────────────────────────────────

/// Lifecycle state of the mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshState {
    /// Mesh created but not yet started.
    Created,
    /// Mesh is in the process of starting.
    Starting,
    /// Mesh is running and accepting connections.
    Running,
    /// Mesh is in the process of stopping.
    Stopping,
    /// Mesh has been stopped.
    Stopped,
}

impl fmt::Display for MeshState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshState::Created => write!(f, "created"),
            MeshState::Starting => write!(f, "starting"),
            MeshState::Running => write!(f, "running"),
            MeshState::Stopping => write!(f, "stopping"),
            MeshState::Stopped => write!(f, "stopped"),
        }
    }
}

// ─── Error type ──────────────────────────────────────────────────────────────

/// Unified error type for mesh operations.
#[derive(Debug)]
pub enum MeshError {
    /// Operation requires the mesh to be running.
    NotRunning,
    /// Mesh is already running or starting.
    AlreadyRunning,
    /// Invalid configuration.
    InvalidConfig(String),
    /// Underlying transport error.
    Transport(TransportError),
    /// Catch-all for other errors.
    Other(String),
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::NotRunning => write!(f, "mesh is not running"),
            MeshError::AlreadyRunning => write!(f, "mesh is already running"),
            MeshError::InvalidConfig(msg) => write!(f, "invalid configuration: {}", msg),
            MeshError::Transport(err) => write!(f, "transport error: {}", err),
            MeshError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for MeshError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MeshError::Transport(err) => Some(err),
            _ => None,
        }
    }
}

impl From<TransportError> for MeshError {
    fn from(err: TransportError) -> Self {
        MeshError::Transport(err)
    }
}

// ─── Events ──────────────────────────────────────────────────────────────────

/// Mesh-wide events broadcast to subscribers.
#[derive(Debug, Clone)]
pub enum HiveMeshEvent {
    /// Mesh lifecycle state changed.
    StateChanged(MeshState),
    /// A new peer joined the mesh.
    PeerJoined(NodeId),
    /// A peer left the mesh.
    PeerLeft(NodeId),
    /// Topology changed.
    TopologyChanged(Box<crate::topology::TopologyEvent>),
}

// ─── Status snapshot ─────────────────────────────────────────────────────────

/// Point-in-time snapshot of mesh status.
#[derive(Debug, Clone)]
pub struct MeshStatus {
    /// Current lifecycle state.
    pub state: MeshState,
    /// Number of connected peers.
    pub peer_count: usize,
    /// This node's identifier.
    pub node_id: String,
    /// Time since the mesh was started.
    pub uptime: std::time::Duration,
}

// ─── HiveMesh facade ────────────────────────────────────────────────────────

const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Unified mesh facade composing all subsystems.
///
/// Create with [`HiveMesh::new`] for simple use or [`HiveMeshBuilder`] for
/// advanced construction with pre-configured subsystems.
pub struct HiveMesh {
    config: MeshConfig,
    node_id: String,
    state: RwLock<MeshState>,
    transport: Option<Arc<dyn MeshTransport>>,
    transport_manager: Option<TransportManager>,
    hierarchy: Option<Arc<dyn HierarchyStrategy>>,
    router: Option<MeshRouter>,
    event_tx: broadcast::Sender<HiveMeshEvent>,
    started_at: RwLock<Option<Instant>>,
}

impl HiveMesh {
    /// Create a new HiveMesh with the given configuration.
    ///
    /// If `config.node_id` is `None`, a UUID v4 is generated automatically.
    pub fn new(config: MeshConfig) -> Self {
        let node_id = config
            .node_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            config,
            node_id,
            state: RwLock::new(MeshState::Created),
            transport: None,
            transport_manager: None,
            hierarchy: None,
            router: None,
            event_tx,
            started_at: RwLock::new(None),
        }
    }

    /// Start the mesh (Created/Stopped → Starting → Running).
    pub fn start(&self) -> Result<(), MeshError> {
        let mut state = self.state.write().unwrap();
        match *state {
            MeshState::Created | MeshState::Stopped => {}
            MeshState::Running | MeshState::Starting | MeshState::Stopping => {
                return Err(MeshError::AlreadyRunning);
            }
        }

        *state = MeshState::Starting;
        let _ = self
            .event_tx
            .send(HiveMeshEvent::StateChanged(MeshState::Starting));

        *state = MeshState::Running;
        *self.started_at.write().unwrap() = Some(Instant::now());
        let _ = self
            .event_tx
            .send(HiveMeshEvent::StateChanged(MeshState::Running));

        Ok(())
    }

    /// Stop the mesh (Running → Stopping → Stopped).
    pub fn stop(&self) -> Result<(), MeshError> {
        let mut state = self.state.write().unwrap();
        match *state {
            MeshState::Running => {}
            _ => return Err(MeshError::NotRunning),
        }

        *state = MeshState::Stopping;
        let _ = self
            .event_tx
            .send(HiveMeshEvent::StateChanged(MeshState::Stopping));

        *state = MeshState::Stopped;
        let _ = self
            .event_tx
            .send(HiveMeshEvent::StateChanged(MeshState::Stopped));

        Ok(())
    }

    /// Get the current lifecycle state.
    pub fn state(&self) -> MeshState {
        *self.state.read().unwrap()
    }

    /// Get a point-in-time status snapshot.
    pub fn status(&self) -> MeshStatus {
        let state = *self.state.read().unwrap();
        let uptime = self
            .started_at
            .read()
            .unwrap()
            .map(|t| t.elapsed())
            .unwrap_or_default();
        let peer_count = self.transport.as_ref().map(|t| t.peer_count()).unwrap_or(0);

        MeshStatus {
            state,
            peer_count,
            node_id: self.node_id.clone(),
            uptime,
        }
    }

    /// Get the mesh configuration.
    pub fn config(&self) -> &MeshConfig {
        &self.config
    }

    /// Get the node ID.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Subscribe to mesh-wide events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<HiveMeshEvent> {
        self.event_tx.subscribe()
    }

    /// Set the transport layer.
    pub fn set_transport(&mut self, transport: Arc<dyn MeshTransport>) {
        self.transport = Some(transport);
    }

    /// Set the multi-transport manager for PACE-based transport selection.
    pub fn set_transport_manager(&mut self, tm: TransportManager) {
        self.transport_manager = Some(tm);
    }

    /// Get a reference to the transport manager, if set.
    pub fn transport_manager(&self) -> Option<&TransportManager> {
        self.transport_manager.as_ref()
    }

    /// Set the hierarchy strategy.
    pub fn set_hierarchy(&mut self, hierarchy: Arc<dyn HierarchyStrategy>) {
        self.hierarchy = Some(hierarchy);
    }

    /// Get a reference to the transport, if set.
    pub fn transport(&self) -> Option<&Arc<dyn MeshTransport>> {
        self.transport.as_ref()
    }

    /// Get a reference to the hierarchy strategy, if set.
    pub fn hierarchy(&self) -> Option<&Arc<dyn HierarchyStrategy>> {
        self.hierarchy.as_ref()
    }

    /// Get a reference to the router, if set.
    pub fn router(&self) -> Option<&MeshRouter> {
        self.router.as_ref()
    }
}

// ─── Feature-gated MeshBrokerState impl ──────────────────────────────────────

#[cfg(feature = "broker")]
#[async_trait::async_trait]
impl crate::broker::state::MeshBrokerState for HiveMesh {
    fn node_info(&self) -> crate::broker::state::MeshNodeInfo {
        let uptime = self
            .started_at
            .read()
            .unwrap()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        crate::broker::state::MeshNodeInfo {
            node_id: self.node_id.clone(),
            uptime_secs: uptime,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    async fn list_peers(&self) -> Vec<crate::broker::state::PeerSummary> {
        let Some(transport) = &self.transport else {
            return vec![];
        };
        transport
            .connected_peers()
            .into_iter()
            .map(|peer_id| {
                let health = transport.get_peer_health(&peer_id);
                crate::broker::state::PeerSummary {
                    id: peer_id.to_string(),
                    connected: true,
                    state: health
                        .as_ref()
                        .map(|h| h.state.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    rtt_ms: health.map(|h| h.rtt_ms as u64),
                }
            })
            .collect()
    }

    async fn get_peer(&self, id: &str) -> Option<crate::broker::state::PeerSummary> {
        let transport = self.transport.as_ref()?;
        let node_id = NodeId::new(id.to_string());
        if transport.is_connected(&node_id) {
            let health = transport.get_peer_health(&node_id);
            Some(crate::broker::state::PeerSummary {
                id: id.to_string(),
                connected: true,
                state: health
                    .as_ref()
                    .map(|h| h.state.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                rtt_ms: health.map(|h| h.rtt_ms as u64),
            })
        } else {
            None
        }
    }

    fn topology(&self) -> crate::broker::state::TopologySummary {
        let peer_count = self.transport.as_ref().map(|t| t.peer_count()).unwrap_or(0);
        crate::broker::state::TopologySummary {
            peer_count,
            role: "standalone".to_string(),
            hierarchy_level: 0,
        }
    }

    fn subscribe_events(&self) -> broadcast::Receiver<crate::broker::state::MeshEvent> {
        // Return a receiver from a dropped sender — no events will be delivered.
        // A future integration step can bridge HiveMeshEvent → MeshEvent.
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Builder for constructing a [`HiveMesh`] with pre-configured subsystems.
pub struct HiveMeshBuilder {
    config: MeshConfig,
    transport: Option<Arc<dyn MeshTransport>>,
    transport_manager: Option<TransportManager>,
    hierarchy: Option<Arc<dyn HierarchyStrategy>>,
    router: Option<MeshRouter>,
}

impl HiveMeshBuilder {
    /// Create a new builder with the given configuration.
    pub fn new(config: MeshConfig) -> Self {
        Self {
            config,
            transport: None,
            transport_manager: None,
            hierarchy: None,
            router: None,
        }
    }

    /// Set a single transport layer.
    pub fn with_transport(mut self, transport: Arc<dyn MeshTransport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Set the multi-transport manager for PACE-based transport selection.
    pub fn with_transport_manager(mut self, tm: TransportManager) -> Self {
        self.transport_manager = Some(tm);
        self
    }

    /// Set the hierarchy strategy.
    pub fn with_hierarchy(mut self, hierarchy: Arc<dyn HierarchyStrategy>) -> Self {
        self.hierarchy = Some(hierarchy);
        self
    }

    /// Set the router.
    pub fn with_router(mut self, router: MeshRouter) -> Self {
        self.router = Some(router);
        self
    }

    /// Build the [`HiveMesh`] instance.
    pub fn build(self) -> HiveMesh {
        let node_id = self
            .config
            .node_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        HiveMesh {
            config: self.config,
            node_id,
            state: RwLock::new(MeshState::Created),
            transport: self.transport,
            transport_manager: self.transport_manager,
            hierarchy: self.hierarchy,
            router: self.router,
            event_tx,
            started_at: RwLock::new(None),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MeshDiscoveryConfig;
    use crate::transport::PeerEventReceiver;
    use async_trait::async_trait;
    use std::time::Duration;

    // ── Mock transport for testing ───────────────────────────────

    struct MockTransport {
        peers: Vec<NodeId>,
    }

    impl MockTransport {
        fn new(peers: Vec<NodeId>) -> Self {
            Self { peers }
        }

        fn empty() -> Self {
            Self { peers: vec![] }
        }
    }

    #[async_trait]
    impl MeshTransport for MockTransport {
        async fn start(&self) -> crate::transport::Result<()> {
            Ok(())
        }
        async fn stop(&self) -> crate::transport::Result<()> {
            Ok(())
        }
        async fn connect(
            &self,
            _peer_id: &NodeId,
        ) -> crate::transport::Result<Box<dyn crate::transport::MeshConnection>> {
            Err(TransportError::NotStarted)
        }
        async fn disconnect(&self, _peer_id: &NodeId) -> crate::transport::Result<()> {
            Ok(())
        }
        fn get_connection(
            &self,
            _peer_id: &NodeId,
        ) -> Option<Box<dyn crate::transport::MeshConnection>> {
            None
        }
        fn peer_count(&self) -> usize {
            self.peers.len()
        }
        fn connected_peers(&self) -> Vec<NodeId> {
            self.peers.clone()
        }
        fn subscribe_peer_events(&self) -> PeerEventReceiver {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            rx
        }
    }

    // ── HiveMesh::new ────────────────────────────────────────────

    #[test]
    fn test_new_with_default_config() {
        let mesh = HiveMesh::new(MeshConfig::default());
        assert_eq!(mesh.state(), MeshState::Created);
        assert!(!mesh.node_id().is_empty());
    }

    #[test]
    fn test_new_with_explicit_node_id() {
        let cfg = MeshConfig {
            node_id: Some("my-node".to_string()),
            ..Default::default()
        };
        let mesh = HiveMesh::new(cfg);
        assert_eq!(mesh.node_id(), "my-node");
    }

    #[test]
    fn test_new_auto_generates_uuid_node_id() {
        let mesh = HiveMesh::new(MeshConfig::default());
        // UUID v4 format: 8-4-4-4-12 hex digits
        assert_eq!(mesh.node_id().len(), 36);
        assert_eq!(mesh.node_id().chars().filter(|&c| c == '-').count(), 4);
    }

    // ── Lifecycle: start / stop ──────────────────────────────────

    #[test]
    fn test_start_transitions_to_running() {
        let mesh = HiveMesh::new(MeshConfig::default());
        assert!(mesh.start().is_ok());
        assert_eq!(mesh.state(), MeshState::Running);
    }

    #[test]
    fn test_start_when_already_running_returns_error() {
        let mesh = HiveMesh::new(MeshConfig::default());
        mesh.start().unwrap();
        let err = mesh.start().unwrap_err();
        assert!(matches!(err, MeshError::AlreadyRunning));
    }

    #[test]
    fn test_stop_transitions_to_stopped() {
        let mesh = HiveMesh::new(MeshConfig::default());
        mesh.start().unwrap();
        assert!(mesh.stop().is_ok());
        assert_eq!(mesh.state(), MeshState::Stopped);
    }

    #[test]
    fn test_stop_when_not_running_returns_error() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let err = mesh.stop().unwrap_err();
        assert!(matches!(err, MeshError::NotRunning));
    }

    #[test]
    fn test_restart_after_stop() {
        let mesh = HiveMesh::new(MeshConfig::default());
        mesh.start().unwrap();
        mesh.stop().unwrap();
        assert!(mesh.start().is_ok());
        assert_eq!(mesh.state(), MeshState::Running);
    }

    #[test]
    fn test_stop_when_created_returns_error() {
        let mesh = HiveMesh::new(MeshConfig::default());
        assert!(matches!(mesh.stop().unwrap_err(), MeshError::NotRunning));
    }

    #[test]
    fn test_stop_when_already_stopped_returns_error() {
        let mesh = HiveMesh::new(MeshConfig::default());
        mesh.start().unwrap();
        mesh.stop().unwrap();
        assert!(matches!(mesh.stop().unwrap_err(), MeshError::NotRunning));
    }

    // ── Status ───────────────────────────────────────────────────

    #[test]
    fn test_status_before_start() {
        let cfg = MeshConfig {
            node_id: Some("status-node".to_string()),
            ..Default::default()
        };
        let mesh = HiveMesh::new(cfg);
        let status = mesh.status();
        assert_eq!(status.state, MeshState::Created);
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.node_id, "status-node");
        assert_eq!(status.uptime, Duration::ZERO);
    }

    #[test]
    fn test_status_while_running() {
        let mesh = HiveMesh::new(MeshConfig {
            node_id: Some("running-node".to_string()),
            ..Default::default()
        });
        mesh.start().unwrap();
        let status = mesh.status();
        assert_eq!(status.state, MeshState::Running);
        assert_eq!(status.node_id, "running-node");
        // Uptime should be non-zero (or at least zero on a very fast machine)
        assert!(status.uptime <= Duration::from_secs(1));
    }

    #[test]
    fn test_status_peer_count_with_transport() {
        let peers = vec![NodeId::new("p1".into()), NodeId::new("p2".into())];
        let mut mesh = HiveMesh::new(MeshConfig::default());
        mesh.set_transport(Arc::new(MockTransport::new(peers)));
        let status = mesh.status();
        assert_eq!(status.peer_count, 2);
    }

    // ── Config accessor ──────────────────────────────────────────

    #[test]
    fn test_config_accessor() {
        let cfg = MeshConfig {
            node_id: Some("cfg-test".to_string()),
            discovery: MeshDiscoveryConfig {
                mdns_enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mesh = HiveMesh::new(cfg);
        assert_eq!(mesh.config().node_id.as_deref(), Some("cfg-test"));
        assert!(!mesh.config().discovery.mdns_enabled);
    }

    // ── Event subscription ───────────────────────────────────────

    #[test]
    fn test_subscribe_events_receives_state_changes() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let mut rx = mesh.subscribe_events();

        mesh.start().unwrap();

        // Should receive Starting then Running
        let evt1 = rx.try_recv().unwrap();
        assert!(matches!(
            evt1,
            HiveMeshEvent::StateChanged(MeshState::Starting)
        ));
        let evt2 = rx.try_recv().unwrap();
        assert!(matches!(
            evt2,
            HiveMeshEvent::StateChanged(MeshState::Running)
        ));
    }

    #[test]
    fn test_subscribe_events_receives_stop_events() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let mut rx = mesh.subscribe_events();

        mesh.start().unwrap();
        // Drain start events
        let _ = rx.try_recv();
        let _ = rx.try_recv();

        mesh.stop().unwrap();

        let evt1 = rx.try_recv().unwrap();
        assert!(matches!(
            evt1,
            HiveMeshEvent::StateChanged(MeshState::Stopping)
        ));
        let evt2 = rx.try_recv().unwrap();
        assert!(matches!(
            evt2,
            HiveMeshEvent::StateChanged(MeshState::Stopped)
        ));
    }

    #[test]
    fn test_multiple_subscribers() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let mut rx1 = mesh.subscribe_events();
        let mut rx2 = mesh.subscribe_events();

        mesh.start().unwrap();

        // Both receivers should get events
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    // ── set_transport / set_hierarchy ────────────────────────────

    #[test]
    fn test_set_transport() {
        let mut mesh = HiveMesh::new(MeshConfig::default());
        assert!(mesh.transport().is_none());

        mesh.set_transport(Arc::new(MockTransport::empty()));
        assert!(mesh.transport().is_some());
    }

    #[test]
    fn test_set_hierarchy() {
        use crate::beacon::HierarchyLevel;
        use crate::hierarchy::{NodeRole, StaticHierarchyStrategy};

        let mut mesh = HiveMesh::new(MeshConfig::default());
        assert!(mesh.hierarchy().is_none());

        let strategy = StaticHierarchyStrategy {
            assigned_level: HierarchyLevel::Platoon,
            assigned_role: NodeRole::Leader,
        };
        mesh.set_hierarchy(Arc::new(strategy));
        assert!(mesh.hierarchy().is_some());
    }

    #[test]
    fn test_router_initially_none() {
        let mesh = HiveMesh::new(MeshConfig::default());
        assert!(mesh.router().is_none());
    }

    // ── MeshState ────────────────────────────────────────────────

    #[test]
    fn test_mesh_state_display() {
        assert_eq!(MeshState::Created.to_string(), "created");
        assert_eq!(MeshState::Starting.to_string(), "starting");
        assert_eq!(MeshState::Running.to_string(), "running");
        assert_eq!(MeshState::Stopping.to_string(), "stopping");
        assert_eq!(MeshState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_mesh_state_equality() {
        assert_eq!(MeshState::Created, MeshState::Created);
        assert_ne!(MeshState::Created, MeshState::Running);
    }

    #[test]
    fn test_mesh_state_clone_copy() {
        let s = MeshState::Running;
        let copied = s;
        // Verify Copy semantics: original is still usable after copy
        assert_eq!(s, copied);
    }

    #[test]
    fn test_mesh_state_debug() {
        let debug = format!("{:?}", MeshState::Running);
        assert!(debug.contains("Running"));
    }

    // ── MeshError ────────────────────────────────────────────────

    #[test]
    fn test_mesh_error_display_not_running() {
        let err = MeshError::NotRunning;
        assert_eq!(err.to_string(), "mesh is not running");
    }

    #[test]
    fn test_mesh_error_display_already_running() {
        let err = MeshError::AlreadyRunning;
        assert_eq!(err.to_string(), "mesh is already running");
    }

    #[test]
    fn test_mesh_error_display_invalid_config() {
        let err = MeshError::InvalidConfig("bad value".to_string());
        assert_eq!(err.to_string(), "invalid configuration: bad value");
    }

    #[test]
    fn test_mesh_error_display_transport() {
        let terr = TransportError::NotStarted;
        let err = MeshError::Transport(terr);
        assert!(err.to_string().contains("Transport not started"));
    }

    #[test]
    fn test_mesh_error_display_other() {
        let err = MeshError::Other("something went wrong".to_string());
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn test_mesh_error_source_transport() {
        use std::error::Error;
        let terr = TransportError::ConnectionFailed("timeout".into());
        let err = MeshError::Transport(terr);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_mesh_error_source_none_for_others() {
        use std::error::Error;
        assert!(MeshError::NotRunning.source().is_none());
        assert!(MeshError::AlreadyRunning.source().is_none());
        assert!(MeshError::InvalidConfig("x".into()).source().is_none());
        assert!(MeshError::Other("x".into()).source().is_none());
    }

    #[test]
    fn test_mesh_error_from_transport_error() {
        let terr = TransportError::NotStarted;
        let err: MeshError = terr.into();
        assert!(matches!(err, MeshError::Transport(_)));
    }

    #[test]
    fn test_mesh_error_debug() {
        let err = MeshError::NotRunning;
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotRunning"));
    }

    // ── HiveMeshEvent ────────────────────────────────────────────

    #[test]
    fn test_event_state_changed() {
        let evt = HiveMeshEvent::StateChanged(MeshState::Running);
        let debug = format!("{:?}", evt);
        assert!(debug.contains("Running"));
    }

    #[test]
    fn test_event_peer_joined() {
        let evt = HiveMeshEvent::PeerJoined(NodeId::new("peer-1".into()));
        let cloned = evt.clone();
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("peer-1"));
    }

    #[test]
    fn test_event_peer_left() {
        let evt = HiveMeshEvent::PeerLeft(NodeId::new("peer-2".into()));
        let cloned = evt.clone();
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("peer-2"));
    }

    #[test]
    fn test_event_topology_changed() {
        let topo_evt = crate::topology::TopologyEvent::PeerLost {
            lost_peer_id: "gone".to_string(),
        };
        let evt = HiveMeshEvent::TopologyChanged(Box::new(topo_evt));
        let cloned = evt.clone();
        let debug = format!("{:?}", cloned);
        assert!(debug.contains("gone"));
    }

    // ── MeshStatus ───────────────────────────────────────────────

    #[test]
    fn test_mesh_status_debug() {
        let status = MeshStatus {
            state: MeshState::Running,
            peer_count: 5,
            node_id: "n1".to_string(),
            uptime: Duration::from_secs(120),
        };
        let debug = format!("{:?}", status);
        assert!(debug.contains("Running"));
        assert!(debug.contains("n1"));
    }

    #[test]
    fn test_mesh_status_clone() {
        let status = MeshStatus {
            state: MeshState::Stopped,
            peer_count: 0,
            node_id: "n2".to_string(),
            uptime: Duration::ZERO,
        };
        let cloned = status.clone();
        assert_eq!(cloned.state, MeshState::Stopped);
        assert_eq!(cloned.node_id, "n2");
    }

    // ── HiveMeshBuilder ──────────────────────────────────────────

    #[test]
    fn test_builder_minimal() {
        let mesh = HiveMeshBuilder::new(MeshConfig::default()).build();
        assert_eq!(mesh.state(), MeshState::Created);
        assert!(mesh.transport().is_none());
        assert!(mesh.hierarchy().is_none());
        assert!(mesh.router().is_none());
    }

    #[test]
    fn test_builder_with_node_id() {
        let cfg = MeshConfig {
            node_id: Some("builder-node".to_string()),
            ..Default::default()
        };
        let mesh = HiveMeshBuilder::new(cfg).build();
        assert_eq!(mesh.node_id(), "builder-node");
    }

    #[test]
    fn test_builder_with_transport() {
        let mesh = HiveMeshBuilder::new(MeshConfig::default())
            .with_transport(Arc::new(MockTransport::empty()))
            .build();
        assert!(mesh.transport().is_some());
    }

    #[test]
    fn test_builder_with_hierarchy() {
        use crate::beacon::HierarchyLevel;
        use crate::hierarchy::{NodeRole, StaticHierarchyStrategy};

        let strategy = StaticHierarchyStrategy {
            assigned_level: HierarchyLevel::Squad,
            assigned_role: NodeRole::Member,
        };
        let mesh = HiveMeshBuilder::new(MeshConfig::default())
            .with_hierarchy(Arc::new(strategy))
            .build();
        assert!(mesh.hierarchy().is_some());
    }

    #[test]
    fn test_builder_with_router() {
        let router = MeshRouter::with_node_id("test");
        let mesh = HiveMeshBuilder::new(MeshConfig::default())
            .with_router(router)
            .build();
        assert!(mesh.router().is_some());
    }

    #[test]
    fn test_builder_all_subsystems() {
        use crate::beacon::HierarchyLevel;
        use crate::hierarchy::{NodeRole, StaticHierarchyStrategy};

        let strategy = StaticHierarchyStrategy {
            assigned_level: HierarchyLevel::Platoon,
            assigned_role: NodeRole::Leader,
        };
        let peers = vec![NodeId::new("p1".into())];
        let router = MeshRouter::with_node_id("full");

        let mesh = HiveMeshBuilder::new(MeshConfig {
            node_id: Some("full-node".to_string()),
            ..Default::default()
        })
        .with_transport(Arc::new(MockTransport::new(peers)))
        .with_hierarchy(Arc::new(strategy))
        .with_router(router)
        .build();

        assert_eq!(mesh.node_id(), "full-node");
        assert!(mesh.transport().is_some());
        assert!(mesh.hierarchy().is_some());
        assert!(mesh.router().is_some());
        assert_eq!(mesh.status().peer_count, 1);
    }

    #[test]
    fn test_builder_lifecycle() {
        let mesh = HiveMeshBuilder::new(MeshConfig::default()).build();
        assert!(mesh.start().is_ok());
        assert_eq!(mesh.state(), MeshState::Running);
        assert!(mesh.stop().is_ok());
        assert_eq!(mesh.state(), MeshState::Stopped);
    }

    // ── TransportManager integration ──────────────────────────────

    #[test]
    fn test_transport_manager_initially_none() {
        let mesh = HiveMesh::new(MeshConfig::default());
        assert!(mesh.transport_manager().is_none());
    }

    #[test]
    fn test_set_transport_manager() {
        use crate::transport::TransportManagerConfig;
        let mut mesh = HiveMesh::new(MeshConfig::default());
        let tm = TransportManager::new(TransportManagerConfig::default());
        mesh.set_transport_manager(tm);
        assert!(mesh.transport_manager().is_some());
    }

    #[test]
    fn test_builder_with_transport_manager() {
        use crate::transport::TransportManagerConfig;
        let tm = TransportManager::new(TransportManagerConfig::default());
        let mesh = HiveMeshBuilder::new(MeshConfig::default())
            .with_transport_manager(tm)
            .build();
        assert!(mesh.transport_manager().is_some());
    }

    #[test]
    fn test_builder_full_with_transport_manager() {
        use crate::beacon::HierarchyLevel;
        use crate::hierarchy::{NodeRole, StaticHierarchyStrategy};
        use crate::transport::TransportManagerConfig;

        let strategy = StaticHierarchyStrategy {
            assigned_level: HierarchyLevel::Platoon,
            assigned_role: NodeRole::Leader,
        };
        let peers = vec![NodeId::new("p1".into())];
        let router = MeshRouter::with_node_id("full");
        let tm = TransportManager::new(TransportManagerConfig::default());

        let mesh = HiveMeshBuilder::new(MeshConfig {
            node_id: Some("full-tm-node".to_string()),
            ..Default::default()
        })
        .with_transport(Arc::new(MockTransport::new(peers)))
        .with_transport_manager(tm)
        .with_hierarchy(Arc::new(strategy))
        .with_router(router)
        .build();

        assert_eq!(mesh.node_id(), "full-tm-node");
        assert!(mesh.transport().is_some());
        assert!(mesh.transport_manager().is_some());
        assert!(mesh.hierarchy().is_some());
        assert!(mesh.router().is_some());
    }
}

// ─── Broker feature tests ────────────────────────────────────────────────────

#[cfg(all(test, feature = "broker"))]
mod broker_tests {
    use super::*;
    use crate::broker::state::MeshBrokerState;
    use crate::config::MeshConfig;

    #[test]
    fn test_broker_node_info() {
        let mesh = HiveMesh::new(MeshConfig {
            node_id: Some("broker-node".to_string()),
            ..Default::default()
        });
        let info = mesh.node_info();
        assert_eq!(info.node_id, "broker-node");
        assert_eq!(info.uptime_secs, 0);
        assert!(!info.version.is_empty());
    }

    #[test]
    fn test_broker_node_info_with_uptime() {
        let mesh = HiveMesh::new(MeshConfig {
            node_id: Some("uptime-node".to_string()),
            ..Default::default()
        });
        mesh.start().unwrap();
        let info = mesh.node_info();
        assert_eq!(info.node_id, "uptime-node");
        // uptime_secs might be 0 on a fast machine, that's OK
    }

    #[tokio::test]
    async fn test_broker_list_peers_no_transport() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let peers = mesh.list_peers().await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_broker_get_peer_no_transport() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let peer = mesh.get_peer("unknown").await;
        assert!(peer.is_none());
    }

    #[test]
    fn test_broker_topology() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let topo = mesh.topology();
        assert_eq!(topo.peer_count, 0);
        assert_eq!(topo.role, "standalone");
        assert_eq!(topo.hierarchy_level, 0);
    }

    #[test]
    fn test_broker_subscribe_events() {
        let mesh = HiveMesh::new(MeshConfig::default());
        let _rx = MeshBrokerState::subscribe_events(&mesh);
        // Receiver is valid (won't panic)
    }
}
