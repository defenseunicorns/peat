//! HIVE-Lite transport for embedded/constrained devices
//!
//! This transport enables Full HIVE nodes to communicate with HIVE-Lite nodes
//! (ESP32, M5Stack, etc.) over simple UDP. It implements the ADR-035 wire protocol.
//!
//! # Architecture
//!
//! - Listens on UDP port for HIVE-Lite messages (default 5555)
//! - Maintains virtual connections to Lite nodes based on heartbeats
//! - Translates primitive CRDTs to/from Automerge documents
//! - Emits PeerEvents for connection lifecycle
//!
//! # Wire Protocol (ADR-035)
//!
//! ```text
//! ┌──────────┬─────────┬──────────┬──────────┬──────────┬──────────────┐
//! │  MAGIC   │ Version │   Type   │  Flags   │  NodeID  │   SeqNum     │
//! │  4 bytes │ 1 byte  │  1 byte  │  2 bytes │  4 bytes │   4 bytes    │
//! ├──────────┴─────────┴──────────┴──────────┴──────────┴──────────────┤
//! │                          Payload                                    │
//! │                       (variable, max 496 bytes)                     │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::transport::lite::{LiteMeshTransport, LiteTransportConfig};
//!
//! let config = LiteTransportConfig {
//!     listen_port: 5555,
//!     broadcast_port: 5555,
//!     peer_timeout_secs: 30,
//! };
//!
//! let transport = LiteMeshTransport::new(config);
//! transport.start().await?;
//!
//! // Subscribe to peer events
//! let mut events = transport.subscribe_peer_events();
//! while let Some(event) = events.recv().await {
//!     match event {
//!         PeerEvent::Connected { peer_id, .. } => {
//!             println!("Lite node connected: {}", peer_id);
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};

// For sync-context locks (MeshTransport trait methods are sync)
use std::sync::Mutex as StdMutex;

use super::{
    ConnectionHealth, ConnectionState, DisconnectReason, MeshConnection, MeshTransport, NodeId,
    PeerEvent, PeerEventReceiver, PeerEventSender, Result, TransportError,
    PEER_EVENT_CHANNEL_CAPACITY,
};

/// Type alias for CRDT callback to avoid clippy::type_complexity
type CrdtCallback = Arc<StdMutex<Option<Box<dyn Fn(&str, &str, CrdtType, &[u8]) + Send + Sync>>>>;

// =============================================================================
// Wire Protocol Constants (ADR-035)
// =============================================================================

/// Magic bytes to identify HIVE-Lite packets
pub const MAGIC: [u8; 4] = [0x48, 0x49, 0x56, 0x45]; // "HIVE"

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Default UDP port for HIVE-Lite communication
pub const DEFAULT_PORT: u16 = 5555;

/// Message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Announce presence and capabilities
    Announce = 0x01,
    /// Heartbeat / keep-alive
    Heartbeat = 0x02,
    /// Data update (CRDT state)
    Data = 0x03,
    /// Query for specific state
    Query = 0x04,
    /// Acknowledge receipt
    Ack = 0x05,
    /// Leave notification
    Leave = 0x06,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Announce),
            0x02 => Some(Self::Heartbeat),
            0x03 => Some(Self::Data),
            0x04 => Some(Self::Query),
            0x05 => Some(Self::Ack),
            0x06 => Some(Self::Leave),
            _ => None,
        }
    }
}

/// CRDT type identifiers for Data messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CrdtType {
    LwwRegister = 0x01,
    GCounter = 0x02,
    PnCounter = 0x03,
    OrSet = 0x04,
}

impl CrdtType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::LwwRegister),
            0x02 => Some(Self::GCounter),
            0x03 => Some(Self::PnCounter),
            0x04 => Some(Self::OrSet),
            _ => None,
        }
    }
}

/// Capability flags (ADR-035)
#[derive(Debug, Clone, Copy, Default)]
pub struct LiteCapabilities(pub u16);

impl LiteCapabilities {
    pub const PERSISTENT_STORAGE: u16 = 0x0001;
    pub const RELAY_CAPABLE: u16 = 0x0002;
    pub const FULL_CRDT: u16 = 0x0004;
    pub const PRIMITIVE_CRDT: u16 = 0x0008;
    pub const DISPLAY_OUTPUT: u16 = 0x0100;
    pub const SENSOR_INPUT: u16 = 0x0200;

    pub fn has(&self, cap: u16) -> bool {
        self.0 & cap != 0
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() >= 2 {
            Self(u16::from_le_bytes([bytes[0], bytes[1]]))
        } else {
            Self(0)
        }
    }
}

// =============================================================================
// Parsed Message
// =============================================================================

/// Parsed HIVE-Lite message
#[derive(Debug, Clone)]
pub struct LiteMessage {
    pub msg_type: MessageType,
    pub flags: u16,
    pub node_id: u32,
    pub seq_num: u32,
    pub payload: Vec<u8>,
}

impl LiteMessage {
    /// Decode a message from bytes
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < 16 {
            return None;
        }

        // Check magic
        if buf[0..4] != MAGIC {
            return None;
        }

        // Check version
        if buf[4] != PROTOCOL_VERSION {
            return None;
        }

        let msg_type = MessageType::from_u8(buf[5])?;
        let flags = u16::from_le_bytes(buf[6..8].try_into().ok()?);
        let node_id = u32::from_le_bytes(buf[8..12].try_into().ok()?);
        let seq_num = u32::from_le_bytes(buf[12..16].try_into().ok()?);

        let payload = if buf.len() > 16 {
            buf[16..].to_vec()
        } else {
            Vec::new()
        };

        Some(Self {
            msg_type,
            flags,
            node_id,
            seq_num,
            payload,
        })
    }

    /// Encode a message to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.payload.len());
        buf.extend_from_slice(&MAGIC);
        buf.push(PROTOCOL_VERSION);
        buf.push(self.msg_type as u8);
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.node_id.to_le_bytes());
        buf.extend_from_slice(&self.seq_num.to_le_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Create an ACK message
    pub fn ack(node_id: u32, ack_seq: u32) -> Self {
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&ack_seq.to_le_bytes());
        Self {
            msg_type: MessageType::Ack,
            flags: 0,
            node_id,
            seq_num: 0,
            payload,
        }
    }

    /// Create a DATA message with CRDT payload
    pub fn data(node_id: u32, seq_num: u32, crdt_type: CrdtType, crdt_data: &[u8]) -> Self {
        let mut payload = Vec::with_capacity(1 + crdt_data.len());
        payload.push(crdt_type as u8);
        payload.extend_from_slice(crdt_data);
        Self {
            msg_type: MessageType::Data,
            flags: 0,
            node_id,
            seq_num,
            payload,
        }
    }
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for HIVE-Lite transport
#[derive(Debug, Clone)]
pub struct LiteTransportConfig {
    /// Port to listen on for incoming messages
    pub listen_port: u16,

    /// Port to broadcast to (usually same as listen_port)
    pub broadcast_port: u16,

    /// Seconds before considering a peer offline (no heartbeat)
    pub peer_timeout_secs: u64,

    /// Enable broadcast sending (for bidirectional sync)
    pub enable_broadcast: bool,

    /// Broadcast interval in seconds (for Full → Lite sync)
    pub broadcast_interval_secs: u64,

    /// Collections to sync TO Lite nodes (Full → Lite)
    /// If empty, no outbound sync. Common values:
    /// - "beacons" - Friendly force positions
    /// - "alerts" - Time-critical notifications
    /// - "commands" - Issued commands for this node
    /// - "waypoints" - Navigation points
    pub outbound_collections: Vec<String>,

    /// Collections to accept FROM Lite nodes (Lite → Full)
    /// If empty, accepts all. Common values:
    /// - "lite_sensors" - Sensor readings (temp, accel, etc.)
    /// - "lite_events" - Button presses, detections
    /// - "lite_status" - Battery, health, etc.
    pub inbound_collections: Vec<String>,

    /// Maximum document age (seconds) to sync to Lite nodes
    /// Older documents are skipped to save bandwidth
    /// 0 = no age limit
    pub max_document_age_secs: u64,

    /// Sync mode for outbound data
    pub outbound_sync_mode: LiteSyncMode,
}

/// Sync mode for Full → Lite communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiteSyncMode {
    /// Only send latest state (no history)
    #[default]
    LatestOnly,

    /// Send deltas since last sync
    DeltaSync,

    /// No outbound sync (receive only)
    ReceiveOnly,
}

impl Default for LiteTransportConfig {
    fn default() -> Self {
        Self {
            listen_port: DEFAULT_PORT,
            broadcast_port: DEFAULT_PORT,
            peer_timeout_secs: 30,
            enable_broadcast: true,
            broadcast_interval_secs: 2,
            // Default: accept sensor data from Lite, send alerts/beacons to Lite
            outbound_collections: vec!["beacons".to_string(), "alerts".to_string()],
            inbound_collections: vec![
                "lite_sensors".to_string(),
                "lite_events".to_string(),
                "lite_status".to_string(),
            ],
            max_document_age_secs: 300, // 5 minutes
            outbound_sync_mode: LiteSyncMode::LatestOnly,
        }
    }
}

// =============================================================================
// Lite Peer State
// =============================================================================

/// State tracked for each connected Lite peer
#[derive(Debug, Clone)]
pub struct LitePeerState {
    /// Node ID (u32 from wire protocol)
    pub node_id_raw: u32,

    /// Last known address
    pub address: SocketAddr,

    /// Capabilities announced by peer
    pub capabilities: LiteCapabilities,

    /// Last heartbeat received
    pub last_seen: Instant,

    /// Last sequence number received
    pub last_seq: u32,

    /// When connection was established
    pub connected_at: Instant,

    /// Message count received
    pub message_count: u64,
}

// =============================================================================
// Lite Connection
// =============================================================================

/// Virtual connection to a HIVE-Lite peer
pub struct LiteConnection {
    node_id: NodeId,
    state: Arc<std::sync::RwLock<LitePeerState>>,
    connected_at: Instant,
}

impl MeshConnection for LiteConnection {
    fn peer_id(&self) -> &NodeId {
        &self.node_id
    }

    fn is_alive(&self) -> bool {
        // Check if we've received a heartbeat recently
        if let Ok(state) = self.state.read() {
            state.last_seen.elapsed() < Duration::from_secs(30)
        } else {
            true // Assume alive if we can't check
        }
    }

    fn connected_at(&self) -> Instant {
        self.connected_at
    }
}

// =============================================================================
// Lite Mesh Transport
// =============================================================================

/// Transport for communicating with HIVE-Lite nodes
pub struct LiteMeshTransport {
    config: LiteTransportConfig,

    /// Connected Lite peers indexed by NodeId string
    /// Using std::sync::RwLock for sync trait method access
    peers: Arc<std::sync::RwLock<HashMap<String, Arc<std::sync::RwLock<LitePeerState>>>>>,

    /// UDP socket for sending/receiving
    socket: Arc<Mutex<Option<Arc<UdpSocket>>>>,

    /// Running flag
    running: Arc<std::sync::RwLock<bool>>,

    /// Event senders for peer notifications
    /// Using std::sync::Mutex for sync trait method access
    event_senders: Arc<StdMutex<Vec<PeerEventSender>>>,

    /// Our node ID (for outgoing messages)
    pub local_node_id: u32,

    /// Sequence number for outgoing messages
    pub seq_num: Arc<Mutex<u32>>,

    /// Callback for received CRDT data
    /// (collection, doc_id, crdt_type, crdt_data)
    crdt_callback: CrdtCallback,
}

impl LiteMeshTransport {
    /// Create a new HIVE-Lite transport
    pub fn new(config: LiteTransportConfig, local_node_id: u32) -> Self {
        Self {
            config,
            peers: Arc::new(std::sync::RwLock::new(HashMap::new())),
            socket: Arc::new(Mutex::new(None)),
            running: Arc::new(std::sync::RwLock::new(false)),
            event_senders: Arc::new(StdMutex::new(Vec::new())),
            local_node_id,
            seq_num: Arc::new(Mutex::new(0)),
            crdt_callback: Arc::new(StdMutex::new(None)),
        }
    }

    /// Set callback for received CRDT data
    ///
    /// The callback receives (collection, doc_id, crdt_type, crdt_data)
    pub fn set_crdt_callback<F>(&self, callback: F)
    where
        F: Fn(&str, &str, CrdtType, &[u8]) + Send + Sync + 'static,
    {
        let mut cb = self.crdt_callback.lock().unwrap();
        *cb = Some(Box::new(callback));
    }

    /// Broadcast a message to all Lite peers
    pub async fn broadcast(&self, msg: &LiteMessage) -> Result<()> {
        let socket_guard = self.socket.lock().await;
        let socket = socket_guard.as_ref().ok_or(TransportError::NotStarted)?;

        let data = msg.encode();
        let broadcast_addr = format!("255.255.255.255:{}", self.config.broadcast_port);

        socket
            .send_to(&data, &broadcast_addr)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        Ok(())
    }

    /// Send a message to a specific Lite peer
    pub async fn send_to(&self, peer_id: &NodeId, msg: &LiteMessage) -> Result<()> {
        let socket_guard = self.socket.lock().await;
        let socket = socket_guard.as_ref().ok_or(TransportError::NotStarted)?;

        let addr = {
            let peers = self.peers.read().unwrap();
            let peer_state = peers
                .get(peer_id.as_str())
                .ok_or_else(|| TransportError::PeerNotFound(peer_id.to_string()))?;
            let addr = peer_state.read().unwrap().address;
            addr
        };

        let data = msg.encode();

        socket
            .send_to(&data, addr)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        Ok(())
    }

    /// Send peer event to all subscribers
    fn send_event(&self, event: PeerEvent) {
        let senders = self.event_senders.lock().unwrap();
        for sender in senders.iter() {
            let _ = sender.try_send(event.clone());
        }
    }

    /// Handle incoming message
    fn handle_message(&self, msg: LiteMessage, src: SocketAddr) {
        // Ignore messages from ourselves (received via broadcast loopback)
        if msg.node_id == self.local_node_id {
            return;
        }

        let node_id_str = format!("lite-{:08X}", msg.node_id);
        let node_id = NodeId::new(node_id_str.clone());

        // Update or create peer state
        let is_new_peer = {
            let mut peers = self.peers.write().unwrap();

            if let Some(peer_state) = peers.get(&node_id_str) {
                // Update existing peer
                let mut state = peer_state.write().unwrap();
                state.last_seen = Instant::now();
                state.last_seq = msg.seq_num;
                state.address = src;
                state.message_count += 1;

                // Update capabilities from ANNOUNCE
                if msg.msg_type == MessageType::Announce && !msg.payload.is_empty() {
                    state.capabilities = LiteCapabilities::from_bytes(&msg.payload);
                }

                false
            } else {
                // New peer
                let capabilities =
                    if msg.msg_type == MessageType::Announce && !msg.payload.is_empty() {
                        LiteCapabilities::from_bytes(&msg.payload)
                    } else {
                        LiteCapabilities::default()
                    };

                let state = LitePeerState {
                    node_id_raw: msg.node_id,
                    address: src,
                    capabilities,
                    last_seen: Instant::now(),
                    last_seq: msg.seq_num,
                    connected_at: Instant::now(),
                    message_count: 1,
                };

                peers.insert(node_id_str.clone(), Arc::new(std::sync::RwLock::new(state)));
                true
            }
        };

        // Emit connected event for new peers
        if is_new_peer {
            log::info!("Lite peer connected: {} from {}", node_id_str, src);
            self.send_event(PeerEvent::Connected {
                peer_id: node_id.clone(),
                connected_at: Instant::now(),
            });
        }

        // Handle message by type
        match msg.msg_type {
            MessageType::Announce => {
                log::debug!("ANNOUNCE from {} caps=0x{:04X}", node_id_str, msg.flags);
            }
            MessageType::Heartbeat => {
                log::trace!("HEARTBEAT from {} seq={}", node_id_str, msg.seq_num);
            }
            MessageType::Data => {
                if !msg.payload.is_empty() {
                    if let Some(crdt_type) = CrdtType::from_u8(msg.payload[0]) {
                        let crdt_data = &msg.payload[1..];

                        log::debug!(
                            "DATA from {} type={:?} len={}",
                            node_id_str,
                            crdt_type,
                            crdt_data.len()
                        );

                        // Call CRDT callback if set
                        if let Some(callback) = self.crdt_callback.lock().unwrap().as_ref() {
                            // Use node_id as doc_id, "lite_sensors" as collection
                            callback("lite_sensors", &node_id_str, crdt_type, crdt_data);
                        }
                    }
                }
            }
            MessageType::Leave => {
                log::info!("LEAVE from {}", node_id_str);
                // Remove peer and emit disconnected event
                let mut peers = self.peers.write().unwrap();
                if let Some(peer_state) = peers.remove(&node_id_str) {
                    let state = peer_state.read().unwrap();
                    self.send_event(PeerEvent::Disconnected {
                        peer_id: node_id,
                        reason: DisconnectReason::RemoteClosed,
                        connection_duration: state.connected_at.elapsed(),
                    });
                }
            }
            _ => {
                log::trace!(
                    "Unhandled message type {:?} from {}",
                    msg.msg_type,
                    node_id_str
                );
            }
        }
    }

    /// Check for stale peers and emit disconnect events
    fn check_stale_peers(&self) {
        let timeout = Duration::from_secs(self.config.peer_timeout_secs);
        let mut peers = self.peers.write().unwrap();

        let mut stale_peers = Vec::new();
        for (id, state) in peers.iter() {
            let state = state.read().unwrap();
            if state.last_seen.elapsed() > timeout {
                stale_peers.push((id.clone(), state.connected_at.elapsed()));
            }
        }

        for (id, duration) in stale_peers {
            peers.remove(&id);
            log::info!("Lite peer timed out: {}", id);
            self.send_event(PeerEvent::Disconnected {
                peer_id: NodeId::new(id),
                reason: DisconnectReason::Timeout,
                connection_duration: duration,
            });
        }
    }
}

#[async_trait]
impl MeshTransport for LiteMeshTransport {
    async fn start(&self) -> Result<()> {
        // Bind UDP socket
        let addr = format!("0.0.0.0:{}", self.config.listen_port);
        let socket = UdpSocket::bind(&addr)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        // Enable broadcast
        socket
            .set_broadcast(true)
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        let socket = Arc::new(socket);

        {
            let mut socket_guard = self.socket.lock().await;
            *socket_guard = Some(socket.clone());
        }

        {
            let mut running = self.running.write().unwrap();
            *running = true;
        }

        log::info!("LiteMeshTransport started on {}", addr);

        // Spawn receive loop
        let peers = self.peers.clone();
        let running = self.running.clone();
        let event_senders = self.event_senders.clone();
        let crdt_callback = self.crdt_callback.clone();
        let _config = self.config.clone();
        let transport = Self {
            config: self.config.clone(),
            peers: peers.clone(),
            socket: Arc::new(Mutex::new(Some(socket.clone()))),
            running: running.clone(),
            event_senders: event_senders.clone(),
            local_node_id: self.local_node_id,
            seq_num: self.seq_num.clone(),
            crdt_callback: crdt_callback.clone(),
        };

        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let mut last_stale_check = Instant::now();

            loop {
                // Check if still running
                if !*running.read().unwrap() {
                    break;
                }

                // Receive with timeout
                let recv_result =
                    tokio::time::timeout(Duration::from_millis(500), socket.recv_from(&mut buf))
                        .await;

                match recv_result {
                    Ok(Ok((len, src))) => {
                        if let Some(msg) = LiteMessage::decode(&buf[..len]) {
                            transport.handle_message(msg, src);
                        }
                    }
                    Ok(Err(e)) => {
                        log::warn!("UDP receive error: {}", e);
                    }
                    Err(_) => {
                        // Timeout - check for stale peers
                    }
                }

                // Periodically check for stale peers
                if last_stale_check.elapsed() > Duration::from_secs(5) {
                    transport.check_stale_peers();
                    last_stale_check = Instant::now();
                }
            }

            log::info!("LiteMeshTransport receive loop stopped");
        });

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        {
            let mut running = self.running.write().unwrap();
            *running = false;
        }

        // Clear socket
        {
            let mut socket_guard = self.socket.lock().await;
            *socket_guard = None;
        }

        log::info!("LiteMeshTransport stopped");
        Ok(())
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>> {
        // For Lite transport, connections are created implicitly when we receive messages
        // This method can be used to "expect" a connection from a known peer

        let peers = self.peers.read().unwrap();
        if let Some(state) = peers.get(peer_id.as_str()) {
            let state_clone = state.clone();
            let connected_at = state.read().unwrap().connected_at;
            Ok(Box::new(LiteConnection {
                node_id: peer_id.clone(),
                state: state_clone,
                connected_at,
            }))
        } else {
            Err(TransportError::PeerNotFound(peer_id.to_string()))
        }
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let mut peers = self.peers.write().unwrap();
        if let Some(state) = peers.remove(peer_id.as_str()) {
            let state = state.read().unwrap();
            self.send_event(PeerEvent::Disconnected {
                peer_id: peer_id.clone(),
                reason: DisconnectReason::LocalClosed,
                connection_duration: state.connected_at.elapsed(),
            });
            Ok(())
        } else {
            Err(TransportError::PeerNotFound(peer_id.to_string()))
        }
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnection>> {
        let peers = self.peers.read().unwrap();
        peers.get(peer_id.as_str()).map(|state| {
            let connected_at = state.read().unwrap().connected_at;
            Box::new(LiteConnection {
                node_id: peer_id.clone(),
                state: state.clone(),
                connected_at,
            }) as Box<dyn MeshConnection>
        })
    }

    fn peer_count(&self) -> usize {
        self.peers.read().unwrap().len()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.peers
            .read()
            .unwrap()
            .keys()
            .map(|k| NodeId::new(k.clone()))
            .collect()
    }

    async fn send_to(&self, peer_id: &NodeId, data: &[u8]) -> Result<usize> {
        let socket_guard = self.socket.lock().await;
        let socket = socket_guard.as_ref().ok_or(TransportError::NotStarted)?;

        let addr = {
            let peers = self.peers.read().unwrap();
            match peers.get(peer_id.as_str()) {
                Some(peer_state) => {
                    let addr = peer_state.read().unwrap().address;
                    addr
                }
                None => return Err(TransportError::PeerNotFound(peer_id.to_string())),
            }
        };

        let sent = socket
            .send_to(data, addr)
            .await
            .map_err(|e| TransportError::Other(Box::new(e)))?;

        Ok(sent)
    }

    fn subscribe_peer_events(&self) -> PeerEventReceiver {
        let (tx, rx) = mpsc::channel(PEER_EVENT_CHANNEL_CAPACITY);
        self.event_senders.lock().unwrap().push(tx);
        rx
    }

    fn get_peer_health(&self, peer_id: &NodeId) -> Option<ConnectionHealth> {
        let peers = self.peers.read().unwrap();
        peers.get(peer_id.as_str()).map(|state| {
            let state = state.read().unwrap();
            ConnectionHealth {
                rtt_ms: 0, // UDP doesn't track RTT
                rtt_variance_ms: 0,
                packet_loss_percent: 0,
                state: if state.last_seen.elapsed() < Duration::from_secs(10) {
                    ConnectionState::Healthy
                } else if state.last_seen.elapsed() < Duration::from_secs(30) {
                    ConnectionState::Degraded
                } else {
                    ConnectionState::Dead
                },
                last_activity: state.last_seen,
            }
        })
    }
}

// =============================================================================
// DocumentStore Integration
// =============================================================================

/// Integrates LiteMeshTransport with a DocumentStore
///
/// This struct handles:
/// - Translating incoming primitive CRDTs to Document upserts
/// - Observing DocumentStore changes and syncing to Lite nodes
/// - Collection filtering per configuration
pub struct LiteDocumentBridge {
    transport: Arc<LiteMeshTransport>,
    config: LiteTransportConfig,
}

impl LiteDocumentBridge {
    /// Create a new bridge between transport and document store
    pub fn new(transport: Arc<LiteMeshTransport>, config: LiteTransportConfig) -> Self {
        Self { transport, config }
    }

    /// Check if a collection should be accepted from Lite nodes
    pub fn accepts_inbound(&self, collection: &str) -> bool {
        self.config.inbound_collections.is_empty()
            || self
                .config
                .inbound_collections
                .iter()
                .any(|c| c == collection)
    }

    /// Check if a collection should be sent to Lite nodes
    pub fn sends_outbound(&self, collection: &str) -> bool {
        self.config
            .outbound_collections
            .iter()
            .any(|c| c == collection)
    }

    /// Decode a GCounter from wire format and return (node_counts, total)
    pub fn decode_gcounter(data: &[u8]) -> Option<(Vec<(u32, u64)>, u64)> {
        if data.len() < 6 {
            return None;
        }

        let _local_node_id = u32::from_le_bytes(data[0..4].try_into().ok()?);
        let num_entries = u16::from_le_bytes(data[4..6].try_into().ok()?) as usize;

        if data.len() < 6 + (num_entries * 12) {
            return None;
        }

        let mut counts = Vec::with_capacity(num_entries);
        let mut total = 0u64;
        let mut offset = 6;

        for _ in 0..num_entries {
            let node_id = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
            let count = u64::from_le_bytes(data[offset + 4..offset + 12].try_into().ok()?);
            counts.push((node_id, count));
            total += count;
            offset += 12;
        }

        Some((counts, total))
    }

    /// Decode an LWW-Register from wire format
    /// Returns (timestamp, node_id, value_bytes)
    pub fn decode_lww_register(data: &[u8]) -> Option<(u64, u32, Vec<u8>)> {
        if data.len() < 12 {
            return None;
        }

        let timestamp = u64::from_le_bytes(data[0..8].try_into().ok()?);
        let node_id = u32::from_le_bytes(data[8..12].try_into().ok()?);
        let value = data[12..].to_vec();

        Some((timestamp, node_id, value))
    }

    /// Convert a GCounter to Document fields
    pub fn gcounter_to_fields(
        node_id: &str,
        counts: &[(u32, u64)],
        total: u64,
    ) -> std::collections::HashMap<String, serde_json::Value> {
        let mut fields = std::collections::HashMap::new();

        fields.insert("type".to_string(), serde_json::json!("gcounter"));
        fields.insert("source_node".to_string(), serde_json::json!(node_id));
        fields.insert("total".to_string(), serde_json::json!(total));

        // Store per-node counts as nested object
        let node_counts: std::collections::HashMap<String, u64> = counts
            .iter()
            .map(|(nid, count)| (format!("{:08X}", nid), *count))
            .collect();
        fields.insert("node_counts".to_string(), serde_json::json!(node_counts));

        fields.insert(
            "updated_at".to_string(),
            serde_json::json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64),
        );

        fields
    }

    /// Convert an LWW-Register to Document fields
    pub fn lww_register_to_fields(
        node_id: &str,
        timestamp: u64,
        value: &[u8],
    ) -> std::collections::HashMap<String, serde_json::Value> {
        let mut fields = std::collections::HashMap::new();

        fields.insert("type".to_string(), serde_json::json!("lww_register"));
        fields.insert("source_node".to_string(), serde_json::json!(node_id));
        fields.insert("timestamp".to_string(), serde_json::json!(timestamp));

        // Try to interpret value as different types
        if value.len() == 4 {
            // Could be i32 or f32
            let int_val = i32::from_le_bytes(value.try_into().unwrap());
            fields.insert("value_i32".to_string(), serde_json::json!(int_val));
        } else if value.len() == 8 {
            // Could be i64 or f64
            let int_val = i64::from_le_bytes(value.try_into().unwrap());
            fields.insert("value_i64".to_string(), serde_json::json!(int_val));
        }

        // Always include raw bytes as hex
        fields.insert(
            "value_hex".to_string(),
            serde_json::json!(hex::encode(value)),
        );

        fields.insert(
            "updated_at".to_string(),
            serde_json::json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64),
        );

        fields
    }

    /// Encode a Document as a GCounter for transmission to Lite nodes
    pub fn encode_gcounter_from_doc(
        doc: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<Vec<u8>> {
        // Extract node_counts from document
        let node_counts = doc.get("node_counts")?.as_object()?;

        let entries: Vec<(u32, u64)> = node_counts
            .iter()
            .filter_map(|(k, v)| {
                let node_id = u32::from_str_radix(k, 16).ok()?;
                let count = v.as_u64()?;
                Some((node_id, count))
            })
            .collect();

        // Encode: [local_node_id:4][num_entries:2][entries:N*12]
        let mut buf = Vec::with_capacity(6 + entries.len() * 12);

        // Use 0 as local_node_id for Full node
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());

        for (node_id, count) in entries {
            buf.extend_from_slice(&node_id.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
        }

        Some(buf)
    }

    /// Encode a generic document as LWW-Register payload
    ///
    /// Used for simple key-value data like beacons, alerts
    pub fn encode_lww_from_doc(
        doc: &std::collections::HashMap<String, serde_json::Value>,
        local_node_id: u32,
    ) -> Option<Vec<u8>> {
        // Serialize document to compact JSON
        let json = serde_json::to_vec(doc).ok()?;

        // Use current time as timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;

        let mut buf = Vec::with_capacity(12 + json.len());
        buf.extend_from_slice(&timestamp.to_le_bytes());
        buf.extend_from_slice(&local_node_id.to_le_bytes());
        buf.extend_from_slice(&json);

        Some(buf)
    }

    /// Broadcast a document to all connected Lite nodes
    ///
    /// Only sends if collection is in outbound_collections config
    pub async fn broadcast_document(
        &self,
        collection: &str,
        doc_id: &str,
        fields: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        if !self.sends_outbound(collection) {
            return Ok(()); // Filtered out
        }

        // Determine CRDT type based on document fields
        let (crdt_type, payload) = if fields.contains_key("node_counts") {
            // GCounter document
            let payload = Self::encode_gcounter_from_doc(fields)
                .ok_or_else(|| TransportError::Other("Failed to encode GCounter".into()))?;
            (CrdtType::GCounter, payload)
        } else {
            // Default to LWW-Register with JSON payload
            let payload = Self::encode_lww_from_doc(fields, self.transport.local_node_id)
                .ok_or_else(|| TransportError::Other("Failed to encode LWW".into()))?;
            (CrdtType::LwwRegister, payload)
        };

        let seq = {
            let mut seq = self.transport.seq_num.lock().await;
            *seq += 1;
            *seq
        };

        let msg = LiteMessage::data(self.transport.local_node_id, seq, crdt_type, &payload);

        // Send via broadcast
        self.transport.broadcast(&msg).await?;

        // Also send unicast to all known peers (broadcast sometimes doesn't reach all devices)
        let peers = self.transport.connected_peers();
        for peer_id in &peers {
            if let Err(e) = self.transport.send_to(peer_id, &msg).await {
                log::warn!("Failed to unicast to {}: {}", peer_id, e);
            }
        }

        log::debug!(
            "Broadcast {} doc {} to {} Lite nodes ({} bytes)",
            collection,
            doc_id,
            peers.len(),
            payload.len()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_encode_decode() {
        let msg = LiteMessage {
            msg_type: MessageType::Heartbeat,
            flags: 0,
            node_id: 0x12345678,
            seq_num: 42,
            payload: vec![],
        };

        let encoded = msg.encode();
        let decoded = LiteMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::Heartbeat);
        assert_eq!(decoded.node_id, 0x12345678);
        assert_eq!(decoded.seq_num, 42);
    }

    #[test]
    fn test_message_with_payload() {
        let msg = LiteMessage::data(0xAABBCCDD, 100, CrdtType::GCounter, &[1, 2, 3, 4]);

        let encoded = msg.encode();
        let decoded = LiteMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, MessageType::Data);
        assert_eq!(decoded.payload[0], CrdtType::GCounter as u8);
        assert_eq!(&decoded.payload[1..], &[1, 2, 3, 4]);
    }

    #[test]
    fn test_invalid_magic() {
        let buf = [
            0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert!(LiteMessage::decode(&buf).is_none());
    }

    #[test]
    fn test_capabilities() {
        let caps =
            LiteCapabilities(LiteCapabilities::PRIMITIVE_CRDT | LiteCapabilities::SENSOR_INPUT);
        assert!(caps.has(LiteCapabilities::PRIMITIVE_CRDT));
        assert!(caps.has(LiteCapabilities::SENSOR_INPUT));
        assert!(!caps.has(LiteCapabilities::FULL_CRDT));
    }

    #[test]
    fn test_decode_gcounter() {
        // GCounter with 2 entries: node 0x11111111=5, node 0x22222222=10
        let mut data = Vec::new();
        data.extend_from_slice(&0x11111111u32.to_le_bytes()); // local node id
        data.extend_from_slice(&2u16.to_le_bytes()); // num entries
        data.extend_from_slice(&0x11111111u32.to_le_bytes()); // entry 1 node
        data.extend_from_slice(&5u64.to_le_bytes()); // entry 1 count
        data.extend_from_slice(&0x22222222u32.to_le_bytes()); // entry 2 node
        data.extend_from_slice(&10u64.to_le_bytes()); // entry 2 count

        let (counts, total) = LiteDocumentBridge::decode_gcounter(&data).unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0], (0x11111111, 5));
        assert_eq!(counts[1], (0x22222222, 10));
        assert_eq!(total, 15);
    }

    #[test]
    fn test_decode_lww_register() {
        // LWW-Register: timestamp=1000, node=0xAABBCCDD, value="Hi"
        let mut data = Vec::new();
        data.extend_from_slice(&1000u64.to_le_bytes());
        data.extend_from_slice(&0xAABBCCDDu32.to_le_bytes());
        data.extend_from_slice(b"Hi");

        let (ts, node, value) = LiteDocumentBridge::decode_lww_register(&data).unwrap();
        assert_eq!(ts, 1000);
        assert_eq!(node, 0xAABBCCDD);
        assert_eq!(value, b"Hi");
    }

    #[test]
    fn test_gcounter_roundtrip() {
        let counts = vec![(0x11111111u32, 5u64), (0x22222222u32, 10u64)];
        let fields = LiteDocumentBridge::gcounter_to_fields("test-node", &counts, 15);

        assert_eq!(fields.get("type").unwrap(), "gcounter");
        assert_eq!(fields.get("total").unwrap(), 15);

        // Re-encode
        let encoded = LiteDocumentBridge::encode_gcounter_from_doc(&fields).unwrap();

        // Decode again (skip local_node_id which is 0 from Full node)
        let (decoded_counts, decoded_total) =
            LiteDocumentBridge::decode_gcounter(&encoded).unwrap();

        assert_eq!(decoded_total, 15);
        assert_eq!(decoded_counts.len(), 2);
    }

    #[test]
    fn test_collection_filtering() {
        let config = LiteTransportConfig {
            outbound_collections: vec!["beacons".to_string(), "alerts".to_string()],
            inbound_collections: vec!["lite_sensors".to_string()],
            ..Default::default()
        };

        // Create a mock transport (we just need the bridge for testing filtering)
        let transport = Arc::new(LiteMeshTransport::new(config.clone(), 0x12345678));
        let bridge = LiteDocumentBridge::new(transport, config);

        // Outbound checks
        assert!(bridge.sends_outbound("beacons"));
        assert!(bridge.sends_outbound("alerts"));
        assert!(!bridge.sends_outbound("squad_summaries"));

        // Inbound checks
        assert!(bridge.accepts_inbound("lite_sensors"));
        assert!(!bridge.accepts_inbound("lite_events")); // Not in list
    }
}
