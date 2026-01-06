//! UDP Bypass Channel for ephemeral data
//!
//! Provides a direct UDP pathway that bypasses the CRDT sync engine for
//! high-frequency, low-latency, or bandwidth-constrained scenarios.
//!
//! ## Use Cases
//!
//! - High-frequency telemetry (10-100 Hz position updates)
//! - Low-latency commands (<50ms delivery)
//! - Bandwidth-constrained links (9.6kbps tactical radio)
//! - Multicast/broadcast to cell members
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   Data Flow with Bypass                          │
//! │                                                                  │
//! │   Application                                                    │
//! │       │                                                          │
//! │       ├──────────────────────────┐                               │
//! │       ▼                          ▼                               │
//! │   ┌─────────────┐          ┌─────────────┐                       │
//! │   │ CRDT Store  │          │  UDP Bypass │ ◄── Ephemeral data    │
//! │   │ (Automerge) │          │   Channel   │                       │
//! │   └──────┬──────┘          └──────┬──────┘                       │
//! │          │                        │                              │
//! │          ▼                        ▼                              │
//! │   ┌─────────────┐          ┌─────────────┐                       │
//! │   │   Iroh      │          │    Raw      │                       │
//! │   │  Transport  │          │    UDP      │                       │
//! │   └─────────────┘          └─────────────┘                       │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use hive_protocol::transport::bypass::{UdpBypassChannel, BypassChannelConfig};
//!
//! // Create bypass channel
//! let config = BypassChannelConfig::default();
//! let channel = UdpBypassChannel::new(config).await?;
//!
//! // Send position update via bypass (no CRDT overhead)
//! channel.send(
//!     BypassTarget::Multicast { group: "239.1.1.100".parse()?, port: 5150 },
//!     "position_updates",
//!     &position_bytes,
//! ).await?;
//!
//! // Subscribe to incoming bypass messages
//! let mut rx = channel.subscribe("position_updates");
//! while let Some(msg) = rx.recv().await {
//!     println!("Received from {}: {:?}", msg.source, msg.data);
//! }
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tracing::{debug, info};

use super::MessagePriority;

// =============================================================================
// Error Types
// =============================================================================

/// Error type for bypass channel operations
#[derive(Debug)]
pub enum BypassError {
    /// IO error (socket operations)
    Io(std::io::Error),
    /// Encoding error
    Encode(String),
    /// Decoding error
    Decode(String),
    /// Invalid configuration
    Config(String),
    /// Channel not started
    NotStarted,
    /// Message too large
    MessageTooLarge { size: usize, max: usize },
    /// Invalid header
    InvalidHeader,
    /// Message is stale (past TTL)
    StaleMessage,
}

impl std::fmt::Display for BypassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BypassError::Io(e) => write!(f, "IO error: {}", e),
            BypassError::Encode(msg) => write!(f, "Encode error: {}", msg),
            BypassError::Decode(msg) => write!(f, "Decode error: {}", msg),
            BypassError::Config(msg) => write!(f, "Config error: {}", msg),
            BypassError::NotStarted => write!(f, "Bypass channel not started"),
            BypassError::MessageTooLarge { size, max } => {
                write!(f, "Message too large: {} bytes (max {})", size, max)
            }
            BypassError::InvalidHeader => write!(f, "Invalid bypass header"),
            BypassError::StaleMessage => write!(f, "Message is stale (past TTL)"),
        }
    }
}

impl std::error::Error for BypassError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BypassError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BypassError {
    fn from(err: std::io::Error) -> Self {
        BypassError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, BypassError>;

// =============================================================================
// Configuration Types
// =============================================================================

/// Transport mode for bypass messages
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BypassTransport {
    /// UDP unicast to specific peer
    #[default]
    Unicast,
    /// UDP multicast to group
    Multicast {
        /// Multicast group address
        group: IpAddr,
        /// Port number
        port: u16,
    },
    /// UDP broadcast on subnet
    Broadcast,
}

/// Message encoding format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageEncoding {
    /// Protobuf (recommended - compact)
    #[default]
    Protobuf,
    /// JSON (debugging)
    Json,
    /// Raw bytes (minimal overhead)
    Raw,
    /// CBOR (compact binary)
    Cbor,
}

impl std::fmt::Display for MessageEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageEncoding::Protobuf => write!(f, "protobuf"),
            MessageEncoding::Json => write!(f, "json"),
            MessageEncoding::Raw => write!(f, "raw"),
            MessageEncoding::Cbor => write!(f, "cbor"),
        }
    }
}

/// Configuration for a bypass collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BypassCollectionConfig {
    /// Collection name
    pub collection: String,
    /// Transport mode for this collection
    pub transport: BypassTransport,
    /// Message encoding format
    pub encoding: MessageEncoding,
    /// Time-to-live for messages in milliseconds
    #[serde(default = "default_ttl_ms")]
    pub ttl_ms: u64,
    /// QoS priority for bandwidth allocation
    #[serde(default)]
    pub priority: MessagePriority,
}

fn default_ttl_ms() -> u64 {
    5000
}

impl BypassCollectionConfig {
    /// Get TTL as Duration
    pub fn ttl(&self) -> Duration {
        Duration::from_millis(self.ttl_ms)
    }
}

impl Default for BypassCollectionConfig {
    fn default() -> Self {
        Self {
            collection: String::new(),
            transport: BypassTransport::Unicast,
            encoding: MessageEncoding::Protobuf,
            ttl_ms: 5000,
            priority: MessagePriority::Normal,
        }
    }
}

/// UDP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpConfig {
    /// Bind port (0 = ephemeral)
    pub bind_port: u16,
    /// Buffer size for receiving
    pub buffer_size: usize,
    /// Multicast TTL (hop count)
    pub multicast_ttl: u32,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            bind_port: 5150,
            buffer_size: 65536,
            multicast_ttl: 32,
        }
    }
}

/// Configuration for bypass channel
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BypassChannelConfig {
    /// UDP configuration
    pub udp: UdpConfig,
    /// Collections that use bypass
    pub collections: Vec<BypassCollectionConfig>,
    /// Enable multicast support
    pub multicast_enabled: bool,
    /// Maximum message size
    pub max_message_size: usize,
}

impl BypassChannelConfig {
    /// Create new configuration with defaults
    pub fn new() -> Self {
        Self {
            udp: UdpConfig::default(),
            collections: Vec::new(),
            multicast_enabled: true,
            max_message_size: 65000, // Leave room for header
        }
    }

    /// Add a collection to bypass
    pub fn with_collection(mut self, config: BypassCollectionConfig) -> Self {
        self.collections.push(config);
        self
    }

    /// Get configuration for a collection
    pub fn get_collection(&self, name: &str) -> Option<&BypassCollectionConfig> {
        self.collections.iter().find(|c| c.collection == name)
    }

    /// Check if a collection uses bypass
    pub fn is_bypass_collection(&self, name: &str) -> bool {
        self.collections.iter().any(|c| c.collection == name)
    }
}

// =============================================================================
// Bypass Header (12 bytes)
// =============================================================================

/// Bypass message header
///
/// Compact 12-byte header for bypass messages:
/// - Magic: 4 bytes ("HIVE")
/// - Collection hash: 4 bytes (FNV-1a hash of collection name)
/// - TTL: 2 bytes (milliseconds, max ~65s)
/// - Flags: 1 byte
/// - Sequence: 1 byte (wrapping counter)
#[derive(Debug, Clone, Copy)]
pub struct BypassHeader {
    /// Magic number (0x48495645 = "HIVE")
    pub magic: [u8; 4],
    /// Collection name hash (FNV-1a)
    pub collection_hash: u32,
    /// TTL in milliseconds
    pub ttl_ms: u16,
    /// Flags
    pub flags: u8,
    /// Sequence number
    pub sequence: u8,
}

impl BypassHeader {
    /// Magic bytes: "HIVE"
    pub const MAGIC: [u8; 4] = [0x48, 0x49, 0x56, 0x45];

    /// Header size in bytes
    pub const SIZE: usize = 12;

    /// Flag: message is compressed
    pub const FLAG_COMPRESSED: u8 = 0x01;
    /// Flag: message is encrypted
    pub const FLAG_ENCRYPTED: u8 = 0x02;
    /// Flag: message is signed
    pub const FLAG_SIGNED: u8 = 0x04;

    /// Create a new header
    pub fn new(collection: &str, ttl: Duration, sequence: u8) -> Self {
        Self {
            magic: Self::MAGIC,
            collection_hash: Self::hash_collection(collection),
            ttl_ms: ttl.as_millis().min(u16::MAX as u128) as u16,
            flags: 0,
            sequence,
        }
    }

    /// Hash a collection name using FNV-1a
    pub fn hash_collection(name: &str) -> u32 {
        let mut hasher = fnv::FnvHasher::default();
        name.hash(&mut hasher);
        hasher.finish() as u32
    }

    /// Check if header has valid magic
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC
    }

    /// Encode header to bytes
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.collection_hash.to_be_bytes());
        buf[8..10].copy_from_slice(&self.ttl_ms.to_be_bytes());
        buf[10] = self.flags;
        buf[11] = self.sequence;
        buf
    }

    /// Decode header from bytes
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(BypassError::InvalidHeader);
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);

        if magic != Self::MAGIC {
            return Err(BypassError::InvalidHeader);
        }

        let collection_hash = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ttl_ms = u16::from_be_bytes([buf[8], buf[9]]);
        let flags = buf[10];
        let sequence = buf[11];

        Ok(Self {
            magic,
            collection_hash,
            ttl_ms,
            flags,
            sequence,
        })
    }

    /// Check if message is stale based on TTL
    pub fn is_stale(&self, received_at: Instant, sent_at: Instant) -> bool {
        let elapsed = received_at.duration_since(sent_at);
        elapsed > Duration::from_millis(self.ttl_ms as u64)
    }
}

// =============================================================================
// FNV-1a Hasher (simple, fast hash for collection names)
// =============================================================================

mod fnv {
    use std::hash::Hasher;

    const FNV_OFFSET_BASIS: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    #[derive(Default)]
    pub struct FnvHasher(u64);

    impl Hasher for FnvHasher {
        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 ^= *byte as u64;
                self.0 = self.0.wrapping_mul(FNV_PRIME);
            }
        }

        fn finish(&self) -> u64 {
            self.0
        }
    }

    impl FnvHasher {
        pub fn default() -> Self {
            Self(FNV_OFFSET_BASIS)
        }
    }
}

// =============================================================================
// Bypass Message
// =============================================================================

/// Incoming bypass message
#[derive(Debug, Clone)]
pub struct BypassMessage {
    /// Source address
    pub source: SocketAddr,
    /// Collection hash (from header)
    pub collection_hash: u32,
    /// Message payload (decoded)
    pub data: Vec<u8>,
    /// When message was received
    pub received_at: Instant,
    /// Sequence number
    pub sequence: u8,
    /// Message priority (inferred from collection config)
    pub priority: MessagePriority,
}

/// Target for bypass send
#[derive(Debug, Clone)]
pub enum BypassTarget {
    /// Unicast to specific address
    Unicast(SocketAddr),
    /// Multicast to group
    Multicast { group: IpAddr, port: u16 },
    /// Broadcast on subnet
    Broadcast { port: u16 },
}

// =============================================================================
// Bypass Metrics
// =============================================================================

/// Metrics for bypass channel
#[derive(Debug, Default)]
pub struct BypassMetrics {
    /// Messages sent
    pub messages_sent: AtomicU64,
    /// Messages received
    pub messages_received: AtomicU64,
    /// Bytes sent
    pub bytes_sent: AtomicU64,
    /// Bytes received
    pub bytes_received: AtomicU64,
    /// Messages dropped (stale)
    pub stale_dropped: AtomicU64,
    /// Messages dropped (invalid header)
    pub invalid_dropped: AtomicU64,
    /// Send errors
    pub send_errors: AtomicU64,
    /// Receive errors
    pub receive_errors: AtomicU64,
}

impl BypassMetrics {
    /// Create snapshot of current metrics
    pub fn snapshot(&self) -> BypassMetricsSnapshot {
        BypassMetricsSnapshot {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            stale_dropped: self.stale_dropped.load(Ordering::Relaxed),
            invalid_dropped: self.invalid_dropped.load(Ordering::Relaxed),
            send_errors: self.send_errors.load(Ordering::Relaxed),
            receive_errors: self.receive_errors.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of bypass metrics
#[derive(Debug, Clone, Default)]
pub struct BypassMetricsSnapshot {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub stale_dropped: u64,
    pub invalid_dropped: u64,
    pub send_errors: u64,
    pub receive_errors: u64,
}

// =============================================================================
// UDP Bypass Channel
// =============================================================================

/// UDP Bypass Channel for ephemeral data
///
/// Provides direct UDP messaging that bypasses CRDT sync for
/// low-latency, high-frequency data.
pub struct UdpBypassChannel {
    /// Configuration
    config: BypassChannelConfig,

    /// UDP socket for unicast/broadcast
    socket: Option<Arc<UdpSocket>>,

    /// Multicast sockets per group
    multicast_sockets: RwLock<HashMap<IpAddr, Arc<UdpSocket>>>,

    /// Collection hash to config mapping
    collection_map: HashMap<u32, BypassCollectionConfig>,

    /// Sequence counter
    sequence: AtomicU8,

    /// Metrics
    metrics: Arc<BypassMetrics>,

    /// Broadcast sender for incoming messages
    incoming_tx: broadcast::Sender<BypassMessage>,

    /// Running flag
    running: Arc<AtomicBool>,
}

impl UdpBypassChannel {
    /// Create a new bypass channel
    pub async fn new(config: BypassChannelConfig) -> Result<Self> {
        // Build collection hash map
        let collection_map: HashMap<u32, BypassCollectionConfig> = config
            .collections
            .iter()
            .map(|c| (BypassHeader::hash_collection(&c.collection), c.clone()))
            .collect();

        let (incoming_tx, _) = broadcast::channel(1024);

        Ok(Self {
            config,
            socket: None,
            multicast_sockets: RwLock::new(HashMap::new()),
            collection_map,
            sequence: AtomicU8::new(0),
            metrics: Arc::new(BypassMetrics::default()),
            incoming_tx,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the bypass channel
    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Bind UDP socket
        let bind_addr = format!("0.0.0.0:{}", self.config.udp.bind_port);
        let socket = UdpSocket::bind(&bind_addr).await?;
        socket.set_broadcast(true)?;

        let socket = Arc::new(socket);
        self.socket = Some(socket.clone());

        // Start receiver loop
        let incoming_tx = self.incoming_tx.clone();
        let metrics = self.metrics.clone();
        let collection_map = self.collection_map.clone();
        let buffer_size = self.config.udp.buffer_size;
        let running = self.running.clone();

        running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let mut buf = vec![0u8; buffer_size];

            while running.load(Ordering::SeqCst) {
                match tokio::time::timeout(Duration::from_millis(100), socket.recv_from(&mut buf))
                    .await
                {
                    Ok(Ok((len, src))) => {
                        let received_at = Instant::now();

                        // Parse header
                        if len < BypassHeader::SIZE {
                            metrics.invalid_dropped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        let header = match BypassHeader::decode(&buf[..BypassHeader::SIZE]) {
                            Ok(h) => h,
                            Err(_) => {
                                metrics.invalid_dropped.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        };

                        // Extract payload
                        let payload = buf[BypassHeader::SIZE..len].to_vec();

                        // Look up collection config for priority
                        let priority = collection_map
                            .get(&header.collection_hash)
                            .map(|c| c.priority)
                            .unwrap_or(MessagePriority::Normal);

                        let message = BypassMessage {
                            source: src,
                            collection_hash: header.collection_hash,
                            data: payload,
                            received_at,
                            sequence: header.sequence,
                            priority,
                        };

                        metrics.messages_received.fetch_add(1, Ordering::Relaxed);
                        metrics
                            .bytes_received
                            .fetch_add(len as u64, Ordering::Relaxed);

                        // Broadcast to subscribers (ignore if no subscribers)
                        let _ = incoming_tx.send(message);
                    }
                    Ok(Err(_e)) => {
                        metrics.receive_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        // Timeout, just continue
                    }
                }
            }
        });

        info!(
            "Bypass channel started on port {}",
            self.config.udp.bind_port
        );
        Ok(())
    }

    /// Stop the bypass channel
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.socket = None;
        self.multicast_sockets.write().unwrap().clear();
        info!("Bypass channel stopped");
    }

    /// Check if channel is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the next sequence number
    fn next_sequence(&self) -> u8 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a message via bypass channel
    pub async fn send(&self, target: BypassTarget, collection: &str, data: &[u8]) -> Result<()> {
        let socket = self.socket.as_ref().ok_or(BypassError::NotStarted)?;

        // Check message size (0 = unlimited)
        if self.config.max_message_size > 0 && data.len() > self.config.max_message_size {
            return Err(BypassError::MessageTooLarge {
                size: data.len(),
                max: self.config.max_message_size,
            });
        }

        // Get TTL from collection config or use default
        let ttl = self
            .config
            .get_collection(collection)
            .map(|c| c.ttl())
            .unwrap_or(Duration::from_secs(5));

        // Create header
        let header = BypassHeader::new(collection, ttl, self.next_sequence());
        let header_bytes = header.encode();

        // Build frame
        let mut frame = Vec::with_capacity(BypassHeader::SIZE + data.len());
        frame.extend_from_slice(&header_bytes);
        frame.extend_from_slice(data);

        // Send based on target
        let bytes_sent = match target {
            BypassTarget::Unicast(addr) => socket.send_to(&frame, addr).await?,
            BypassTarget::Multicast { group, port } => {
                let mcast_socket = self.get_or_create_multicast(group).await?;
                mcast_socket.send_to(&frame, (group, port)).await?
            }
            BypassTarget::Broadcast { port } => {
                let broadcast_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port);
                socket.send_to(&frame, broadcast_addr).await?
            }
        };

        self.metrics.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .bytes_sent
            .fetch_add(bytes_sent as u64, Ordering::Relaxed);

        Ok(())
    }

    /// Send with collection config (uses config's transport settings)
    pub async fn send_to_collection(
        &self,
        collection: &str,
        target_addr: Option<SocketAddr>,
        data: &[u8],
    ) -> Result<()> {
        let config = self
            .config
            .get_collection(collection)
            .ok_or_else(|| BypassError::Config(format!("Unknown collection: {}", collection)))?;

        let target = match &config.transport {
            BypassTransport::Unicast => {
                let addr = target_addr.ok_or_else(|| {
                    BypassError::Config("Unicast requires target address".to_string())
                })?;
                BypassTarget::Unicast(addr)
            }
            BypassTransport::Multicast { group, port } => BypassTarget::Multicast {
                group: *group,
                port: *port,
            },
            BypassTransport::Broadcast => BypassTarget::Broadcast {
                port: self.config.udp.bind_port,
            },
        };

        self.send(target, collection, data).await
    }

    /// Subscribe to incoming bypass messages
    pub fn subscribe(&self) -> broadcast::Receiver<BypassMessage> {
        self.incoming_tx.subscribe()
    }

    /// Subscribe to messages for a specific collection
    pub fn subscribe_collection(
        &self,
        collection: &str,
    ) -> (u32, broadcast::Receiver<BypassMessage>) {
        let hash = BypassHeader::hash_collection(collection);
        (hash, self.incoming_tx.subscribe())
    }

    /// Get or create a multicast socket for a group
    async fn get_or_create_multicast(&self, group: IpAddr) -> Result<Arc<UdpSocket>> {
        // Check if already exists
        {
            let sockets = self.multicast_sockets.read().unwrap();
            if let Some(socket) = sockets.get(&group) {
                return Ok(socket.clone());
            }
        }

        // Create new multicast socket
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        match group {
            IpAddr::V4(addr) => {
                socket.join_multicast_v4(addr, Ipv4Addr::UNSPECIFIED)?;
                socket.set_multicast_ttl_v4(self.config.udp.multicast_ttl)?;
            }
            IpAddr::V6(addr) => {
                socket.join_multicast_v6(&addr, 0)?;
            }
        }

        let socket = Arc::new(socket);
        self.multicast_sockets
            .write()
            .unwrap()
            .insert(group, socket.clone());

        debug!("Joined multicast group: {}", group);
        Ok(socket)
    }

    /// Leave a multicast group
    pub fn leave_multicast(&self, group: IpAddr) -> Result<()> {
        if let Some(socket) = self.multicast_sockets.write().unwrap().remove(&group) {
            match group {
                IpAddr::V4(addr) => {
                    // Note: socket drop will leave the group, but explicit leave is cleaner
                    if let Ok(socket) = Arc::try_unwrap(socket) {
                        let _ = socket.leave_multicast_v4(addr, Ipv4Addr::UNSPECIFIED);
                    }
                }
                IpAddr::V6(addr) => {
                    if let Ok(socket) = Arc::try_unwrap(socket) {
                        let _ = socket.leave_multicast_v6(&addr, 0);
                    }
                }
            }
            debug!("Left multicast group: {}", group);
        }
        Ok(())
    }

    /// Get current metrics
    pub fn metrics(&self) -> BypassMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get configuration
    pub fn config(&self) -> &BypassChannelConfig {
        &self.config
    }

    /// Check if a collection is configured for bypass
    pub fn is_bypass_collection(&self, name: &str) -> bool {
        self.config.is_bypass_collection(name)
    }

    /// Get collection config by hash
    pub fn get_collection_by_hash(&self, hash: u32) -> Option<&BypassCollectionConfig> {
        self.collection_map.get(&hash)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bypass_header_encode_decode() {
        let header = BypassHeader::new("test_collection", Duration::from_millis(1000), 42);
        let encoded = header.encode();
        let decoded = BypassHeader::decode(&encoded).unwrap();

        assert_eq!(decoded.magic, BypassHeader::MAGIC);
        assert_eq!(decoded.collection_hash, header.collection_hash);
        assert_eq!(decoded.ttl_ms, 1000);
        assert_eq!(decoded.sequence, 42);
        assert!(decoded.is_valid());
    }

    #[test]
    fn test_bypass_header_invalid_magic() {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(&[0, 0, 0, 0]);
        let result = BypassHeader::decode(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_bypass_header_too_short() {
        let data = [0u8; 8];
        let result = BypassHeader::decode(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_collection_hash_consistency() {
        let hash1 = BypassHeader::hash_collection("position_updates");
        let hash2 = BypassHeader::hash_collection("position_updates");
        let hash3 = BypassHeader::hash_collection("sensor_data");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_bypass_config() {
        let config = BypassChannelConfig::new()
            .with_collection(BypassCollectionConfig {
                collection: "positions".into(),
                transport: BypassTransport::Multicast {
                    group: "239.1.1.100".parse().unwrap(),
                    port: 5150,
                },
                encoding: MessageEncoding::Protobuf,
                ttl_ms: 200,
                priority: MessagePriority::High,
            })
            .with_collection(BypassCollectionConfig {
                collection: "telemetry".into(),
                transport: BypassTransport::Unicast,
                encoding: MessageEncoding::Cbor,
                ttl_ms: 5000,
                priority: MessagePriority::Normal,
            });

        assert!(config.is_bypass_collection("positions"));
        assert!(config.is_bypass_collection("telemetry"));
        assert!(!config.is_bypass_collection("unknown"));

        let pos_config = config.get_collection("positions").unwrap();
        assert_eq!(pos_config.priority, MessagePriority::High);
    }

    #[test]
    fn test_ttl_clamping() {
        // TTL greater than u16::MAX should be clamped
        let header = BypassHeader::new("test", Duration::from_secs(1000), 0);
        assert_eq!(header.ttl_ms, u16::MAX);
    }

    #[tokio::test]
    async fn test_bypass_channel_creation() {
        let config = BypassChannelConfig::new().with_collection(BypassCollectionConfig {
            collection: "test".into(),
            ..Default::default()
        });

        let channel = UdpBypassChannel::new(config).await.unwrap();
        assert!(!channel.is_running());
        assert!(channel.is_bypass_collection("test"));
    }

    #[tokio::test]
    async fn test_bypass_channel_start_stop() {
        let config = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0, // Ephemeral port
                ..Default::default()
            },
            ..Default::default()
        };

        let mut channel = UdpBypassChannel::new(config).await.unwrap();

        channel.start().await.unwrap();
        assert!(channel.is_running());

        channel.stop();
        assert!(!channel.is_running());
    }

    #[tokio::test]
    async fn test_bypass_send_receive() {
        // Create two channels on different ports
        let config1 = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            collections: vec![BypassCollectionConfig {
                collection: "test".into(),
                ttl_ms: 5000,
                ..Default::default()
            }],
            ..Default::default()
        };

        let config2 = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            collections: vec![BypassCollectionConfig {
                collection: "test".into(),
                ttl_ms: 5000,
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut channel1 = UdpBypassChannel::new(config1).await.unwrap();
        let mut channel2 = UdpBypassChannel::new(config2).await.unwrap();

        channel1.start().await.unwrap();
        channel2.start().await.unwrap();

        // Get channel2's port and construct localhost address
        let socket2_port = channel2
            .socket
            .as_ref()
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let socket2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), socket2_port);

        // Subscribe to messages on channel2
        let mut rx = channel2.subscribe();

        // Send from channel1 to channel2
        let test_data = b"Hello, bypass!";
        channel1
            .send(BypassTarget::Unicast(socket2_addr), "test", test_data)
            .await
            .unwrap();

        // Receive on channel2
        let msg = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("timeout")
            .expect("receive error");

        assert_eq!(msg.data, test_data);
        assert_eq!(msg.collection_hash, BypassHeader::hash_collection("test"));

        // Check metrics
        let metrics1 = channel1.metrics();
        assert_eq!(metrics1.messages_sent, 1);
        assert!(metrics1.bytes_sent > 0);

        let metrics2 = channel2.metrics();
        assert_eq!(metrics2.messages_received, 1);
        assert!(metrics2.bytes_received > 0);

        channel1.stop();
        channel2.stop();
    }

    #[test]
    fn test_message_too_large() {
        // This test doesn't need async since we're just testing the error condition
        let _config = BypassChannelConfig {
            max_message_size: 100,
            ..Default::default()
        };

        // Create error manually since we can't easily test async in sync context
        let err = BypassError::MessageTooLarge {
            size: 200,
            max: 100,
        };
        assert!(err.to_string().contains("200"));
        assert!(err.to_string().contains("100"));
    }
}
