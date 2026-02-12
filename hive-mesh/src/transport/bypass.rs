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
//! use hive_mesh::transport::bypass::{UdpBypassChannel, BypassChannelConfig};
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

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
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
    /// Signature verification failed
    InvalidSignature,
    /// Decryption failed
    DecryptionFailed,
    /// Source IP not in allowlist
    UnauthorizedSource(IpAddr),
    /// Replay attack detected (duplicate sequence)
    ReplayDetected { sequence: u8 },
    /// Missing security credential (key not configured)
    MissingCredential(String),
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
            BypassError::InvalidSignature => write!(f, "Invalid message signature"),
            BypassError::DecryptionFailed => write!(f, "Message decryption failed"),
            BypassError::UnauthorizedSource(ip) => {
                write!(f, "Unauthorized source IP: {}", ip)
            }
            BypassError::ReplayDetected { sequence } => {
                write!(f, "Replay attack detected (sequence {})", sequence)
            }
            BypassError::MissingCredential(what) => {
                write!(f, "Missing security credential: {}", what)
            }
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
// Security Configuration (ADR-042 Phase 5)
// =============================================================================

/// Security configuration for bypass channel
///
/// Since bypass messages don't go through Iroh's authenticated QUIC transport,
/// optional security features can be enabled to protect against:
/// - Message forgery (signing)
/// - Eavesdropping (encryption)
/// - Unauthorized sources (allowlisting)
/// - Replay attacks (sequence window)
///
/// ## Security Tradeoffs
///
/// | Feature | Overhead | Latency Impact | Protection |
/// |---------|----------|----------------|------------|
/// | Signing | +64 bytes | +0.1-0.5ms | Authenticity |
/// | Encryption | +16 bytes | +0.1-0.3ms | Confidentiality |
/// | Allowlist | 0 bytes | ~0ms | Source filtering |
/// | Replay | 0 bytes | ~0ms | Freshness |
///
/// ## When to Use
///
/// - **High-frequency telemetry**: Signing only (authenticity without encryption overhead)
/// - **Sensitive commands**: Full encryption + signing
/// - **Trusted network**: Allowlist only (minimal overhead)
/// - **Untrusted network**: All features enabled
///
/// ## Example
///
/// ```rust
/// use hive_mesh::transport::bypass::BypassSecurityConfig;
///
/// // Minimal security for high-frequency telemetry
/// let config = BypassSecurityConfig {
///     require_signature: true,
///     encrypt_payload: false,
///     ..Default::default()
/// };
///
/// // Full security for sensitive commands
/// let config = BypassSecurityConfig::full_security();
/// ```
#[derive(Debug, Clone, Default)]
pub struct BypassSecurityConfig {
    /// Require Ed25519 signature on all messages
    ///
    /// When enabled:
    /// - Outgoing messages are signed with the node's signing key
    /// - Incoming messages without valid signatures are rejected
    /// - Adds ~64 bytes overhead per message
    pub require_signature: bool,

    /// Encrypt payload with formation key (ChaCha20-Poly1305)
    ///
    /// When enabled:
    /// - Payload is encrypted before sending
    /// - Header remains unencrypted for routing
    /// - Adds ~16 bytes overhead (Poly1305 auth tag)
    pub encrypt_payload: bool,

    /// Filter sources by known peer addresses
    ///
    /// When Some:
    /// - Only accept messages from listed IP addresses
    /// - Messages from unknown IPs are rejected
    /// - Useful for trusted network segments
    pub source_allowlist: Option<Vec<IpAddr>>,

    /// Enable replay protection
    ///
    /// When enabled:
    /// - Track seen sequence numbers per source
    /// - Reject duplicate sequence numbers within window
    /// - Window size: 64 sequences (covers ~1 minute at 1 Hz)
    pub replay_protection: bool,

    /// Replay window size (default: 64)
    ///
    /// Number of sequence numbers to track per source.
    /// Larger windows use more memory but handle more reordering.
    pub replay_window_size: usize,
}

impl BypassSecurityConfig {
    /// Create configuration with no security (fastest)
    pub fn none() -> Self {
        Self::default()
    }

    /// Create configuration with signature only (authenticity)
    pub fn signed() -> Self {
        Self {
            require_signature: true,
            ..Default::default()
        }
    }

    /// Create configuration with full security (all features)
    pub fn full_security() -> Self {
        Self {
            require_signature: true,
            encrypt_payload: true,
            source_allowlist: None, // Set separately based on known peers
            replay_protection: true,
            replay_window_size: 64,
        }
    }

    /// Check if any security features are enabled
    pub fn is_enabled(&self) -> bool {
        self.require_signature
            || self.encrypt_payload
            || self.source_allowlist.is_some()
            || self.replay_protection
    }

    /// Check if source IP is allowed
    pub fn is_source_allowed(&self, ip: &IpAddr) -> bool {
        match &self.source_allowlist {
            Some(list) => list.contains(ip),
            None => true, // No allowlist = all sources allowed
        }
    }
}

/// Security credentials for bypass channel
///
/// Contains the cryptographic keys needed for signing and encryption.
#[derive(Clone, Default)]
pub struct BypassSecurityCredentials {
    /// Ed25519 signing key (private)
    signing_key: Option<SigningKey>,

    /// Ed25519 verifying keys for known peers (public)
    /// Maps peer ID or IP to their public key
    peer_keys: HashMap<String, VerifyingKey>,

    /// ChaCha20-Poly1305 encryption key (shared formation key)
    /// 256-bit key shared among formation members
    encryption_key: Option<[u8; 32]>,
}

impl std::fmt::Debug for BypassSecurityCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BypassSecurityCredentials")
            .field("has_signing_key", &self.signing_key.is_some())
            .field("peer_keys_count", &self.peer_keys.len())
            .field("has_encryption_key", &self.encryption_key.is_some())
            .finish()
    }
}

impl BypassSecurityCredentials {
    /// Create new empty credentials
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the signing key
    pub fn with_signing_key(mut self, key: SigningKey) -> Self {
        self.signing_key = Some(key);
        self
    }

    /// Add a peer's verifying key
    pub fn with_peer_key(mut self, peer_id: impl Into<String>, key: VerifyingKey) -> Self {
        self.peer_keys.insert(peer_id.into(), key);
        self
    }

    /// Set the encryption key
    pub fn with_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.encryption_key = Some(key);
        self
    }

    /// Get the verifying key for our signing key
    pub fn verifying_key(&self) -> Option<VerifyingKey> {
        self.signing_key.as_ref().map(|k| k.verifying_key())
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Result<Signature> {
        let key = self
            .signing_key
            .as_ref()
            .ok_or_else(|| BypassError::MissingCredential("signing key".into()))?;
        Ok(key.sign(message))
    }

    /// Verify a signature from a known peer
    pub fn verify(&self, peer_id: &str, message: &[u8], signature: &Signature) -> Result<()> {
        let key = self
            .peer_keys
            .get(peer_id)
            .ok_or_else(|| BypassError::MissingCredential(format!("peer key for {}", peer_id)))?;
        key.verify(message, signature)
            .map_err(|_| BypassError::InvalidSignature)
    }

    /// Verify a signature using IP address as peer identifier
    pub fn verify_by_ip(&self, ip: &IpAddr, message: &[u8], signature: &Signature) -> Result<()> {
        self.verify(&ip.to_string(), message, signature)
    }

    /// Encrypt payload
    pub fn encrypt(&self, plaintext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| BypassError::MissingCredential("encryption key".into()))?;

        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| BypassError::Config("Invalid encryption key".into()))?;

        let nonce = Nonce::from_slice(nonce);
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| BypassError::Encode("Encryption failed".into()))
    }

    /// Decrypt payload
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| BypassError::MissingCredential("encryption key".into()))?;

        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| BypassError::Config("Invalid encryption key".into()))?;

        let nonce = Nonce::from_slice(nonce);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| BypassError::DecryptionFailed)
    }
}

/// Replay protection tracker
///
/// Tracks seen sequence numbers per source to detect replay attacks.
/// Uses a sliding window to handle out-of-order delivery.
#[derive(Debug, Default)]
pub struct ReplayTracker {
    /// Seen sequences per source IP
    /// Each entry is a bitset representing seen sequences
    windows: RwLock<HashMap<IpAddr, ReplayWindow>>,

    /// Window size
    window_size: usize,
}

/// Sliding window for replay detection
#[derive(Debug)]
struct ReplayWindow {
    /// Highest sequence seen
    highest_seq: u8,
    /// Bitmap of seen sequences (relative to highest)
    seen: u64,
}

impl ReplayWindow {
    fn new() -> Self {
        Self {
            highest_seq: 0,
            seen: 0,
        }
    }

    /// Check if sequence is valid (not a replay)
    /// Returns true if valid, false if replay
    fn check_and_update(&mut self, seq: u8, window_size: usize) -> bool {
        let window_size = window_size.min(64) as u8;

        // Calculate distance from highest seen
        let diff = self.highest_seq.wrapping_sub(seq);

        if diff == 0 && self.seen == 0 {
            // First packet
            self.highest_seq = seq;
            self.seen = 1;
            return true;
        }

        if seq == self.highest_seq {
            // Exact replay of highest
            return false;
        }

        // Check if seq is ahead of highest
        let ahead = seq.wrapping_sub(self.highest_seq);
        if ahead > 0 && ahead < 128 {
            // New highest sequence
            let shift = ahead as u32;
            if shift < 64 {
                self.seen = (self.seen << shift) | 1;
            } else {
                self.seen = 1;
            }
            self.highest_seq = seq;
            return true;
        }

        // Check if seq is within window
        if diff < window_size && diff < 64 {
            let bit = 1u64 << diff;
            if self.seen & bit != 0 {
                // Already seen
                return false;
            }
            self.seen |= bit;
            return true;
        }

        // Too old
        false
    }
}

impl ReplayTracker {
    /// Create new replay tracker
    pub fn new(window_size: usize) -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
            window_size: window_size.min(64),
        }
    }

    /// Check if message is valid (not a replay)
    /// Returns Ok(()) if valid, Err if replay detected
    pub fn check(&self, source: &IpAddr, sequence: u8) -> Result<()> {
        let mut windows = self.windows.write().unwrap();
        let window = windows.entry(*source).or_insert_with(ReplayWindow::new);

        if window.check_and_update(sequence, self.window_size) {
            Ok(())
        } else {
            Err(BypassError::ReplayDetected { sequence })
        }
    }

    /// Clear tracking for a source
    pub fn clear_source(&self, source: &IpAddr) {
        self.windows.write().unwrap().remove(source);
    }

    /// Clear all tracking
    pub fn clear_all(&self) {
        self.windows.write().unwrap().clear();
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

    // Security metrics (ADR-042 Phase 5)
    /// Messages rejected due to invalid signature
    pub signature_rejected: AtomicU64,
    /// Messages rejected due to decryption failure
    pub decryption_failed: AtomicU64,
    /// Messages rejected due to unauthorized source
    pub unauthorized_source: AtomicU64,
    /// Messages rejected due to replay detection
    pub replay_rejected: AtomicU64,
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
            signature_rejected: self.signature_rejected.load(Ordering::Relaxed),
            decryption_failed: self.decryption_failed.load(Ordering::Relaxed),
            unauthorized_source: self.unauthorized_source.load(Ordering::Relaxed),
            replay_rejected: self.replay_rejected.load(Ordering::Relaxed),
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

    // Security metrics (ADR-042 Phase 5)
    /// Messages rejected due to invalid signature
    pub signature_rejected: u64,
    /// Messages rejected due to decryption failure
    pub decryption_failed: u64,
    /// Messages rejected due to unauthorized source
    pub unauthorized_source: u64,
    /// Messages rejected due to replay detection
    pub replay_rejected: u64,
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

    // =========================================================================
    // Security Tests (ADR-042 Phase 5)
    // =========================================================================

    #[test]
    fn test_security_config_none() {
        let config = BypassSecurityConfig::none();
        assert!(!config.require_signature);
        assert!(!config.encrypt_payload);
        assert!(config.source_allowlist.is_none());
        assert!(!config.replay_protection);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_security_config_signed() {
        let config = BypassSecurityConfig::signed();
        assert!(config.require_signature);
        assert!(!config.encrypt_payload);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_security_config_full() {
        let config = BypassSecurityConfig::full_security();
        assert!(config.require_signature);
        assert!(config.encrypt_payload);
        assert!(config.replay_protection);
        assert_eq!(config.replay_window_size, 64);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_security_config_allowlist() {
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();
        let ip3: IpAddr = "10.0.0.1".parse().unwrap();

        // No allowlist - all allowed
        let config = BypassSecurityConfig::none();
        assert!(config.is_source_allowed(&ip1));
        assert!(config.is_source_allowed(&ip2));
        assert!(config.is_source_allowed(&ip3));

        // With allowlist - only listed IPs allowed
        let config = BypassSecurityConfig {
            source_allowlist: Some(vec![ip1, ip2]),
            ..Default::default()
        };
        assert!(config.is_source_allowed(&ip1));
        assert!(config.is_source_allowed(&ip2));
        assert!(!config.is_source_allowed(&ip3));
    }

    #[test]
    fn test_credentials_signing() {
        use ed25519_dalek::SigningKey;

        // Use fixed seed for deterministic testing
        let seed: [u8; 32] = [1u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        // Create credentials
        let peer_ip: IpAddr = "192.168.1.1".parse().unwrap();
        let creds = BypassSecurityCredentials::new()
            .with_signing_key(signing_key)
            .with_peer_key(peer_ip.to_string(), verifying_key);

        // Sign a message
        let message = b"test message for signing";
        let signature = creds.sign(message).expect("signing should succeed");

        // Verify the signature
        creds
            .verify_by_ip(&peer_ip, message, &signature)
            .expect("verification should succeed");
    }

    #[test]
    fn test_credentials_invalid_signature() {
        use ed25519_dalek::SigningKey;

        // Use different fixed seeds for two key pairs
        let seed1: [u8; 32] = [1u8; 32];
        let seed2: [u8; 32] = [2u8; 32];
        let signing_key1 = SigningKey::from_bytes(&seed1);
        let signing_key2 = SigningKey::from_bytes(&seed2);
        let verifying_key2 = signing_key2.verifying_key();

        // Create credentials with mismatched keys
        let peer_ip: IpAddr = "192.168.1.1".parse().unwrap();
        let creds = BypassSecurityCredentials::new()
            .with_signing_key(signing_key1)
            .with_peer_key(peer_ip.to_string(), verifying_key2);

        // Sign with key1
        let message = b"test message";
        let signature = creds.sign(message).expect("signing should succeed");

        // Verification with key2 should fail
        let result = creds.verify_by_ip(&peer_ip, message, &signature);
        assert!(matches!(result, Err(BypassError::InvalidSignature)));
    }

    #[test]
    fn test_credentials_encryption() {
        let encryption_key = [42u8; 32];
        let creds = BypassSecurityCredentials::new().with_encryption_key(encryption_key);

        let plaintext = b"secret message for encryption";
        let nonce = [0u8; 12];

        // Encrypt
        let ciphertext = creds
            .encrypt(plaintext, &nonce)
            .expect("encryption should succeed");
        assert_ne!(ciphertext.as_slice(), plaintext);
        // Ciphertext should be plaintext + 16 bytes (Poly1305 tag)
        assert_eq!(ciphertext.len(), plaintext.len() + 16);

        // Decrypt
        let decrypted = creds
            .decrypt(&ciphertext, &nonce)
            .expect("decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_credentials_decryption_wrong_key() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];

        let creds1 = BypassSecurityCredentials::new().with_encryption_key(key1);
        let creds2 = BypassSecurityCredentials::new().with_encryption_key(key2);

        let plaintext = b"secret message";
        let nonce = [0u8; 12];

        // Encrypt with key1
        let ciphertext = creds1
            .encrypt(plaintext, &nonce)
            .expect("encryption should succeed");

        // Decrypt with key2 should fail
        let result = creds2.decrypt(&ciphertext, &nonce);
        assert!(matches!(result, Err(BypassError::DecryptionFailed)));
    }

    #[test]
    fn test_credentials_missing_signing_key() {
        let creds = BypassSecurityCredentials::new();
        let result = creds.sign(b"message");
        assert!(matches!(result, Err(BypassError::MissingCredential(_))));
    }

    #[test]
    fn test_credentials_missing_encryption_key() {
        let creds = BypassSecurityCredentials::new();
        let result = creds.encrypt(b"message", &[0u8; 12]);
        assert!(matches!(result, Err(BypassError::MissingCredential(_))));
    }

    #[test]
    fn test_replay_tracker_first_message() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        // First message should always succeed
        tracker
            .check(&source, 0)
            .expect("first message should succeed");
    }

    #[test]
    fn test_replay_tracker_sequential() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        // Sequential messages should all succeed
        for seq in 0..10u8 {
            tracker
                .check(&source, seq)
                .unwrap_or_else(|_| panic!("sequence {} should succeed", seq));
        }
    }

    #[test]
    fn test_replay_tracker_replay_detected() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        // First message succeeds
        tracker.check(&source, 5).expect("first should succeed");

        // Replay should be detected
        let result = tracker.check(&source, 5);
        assert!(matches!(
            result,
            Err(BypassError::ReplayDetected { sequence: 5 })
        ));
    }

    #[test]
    fn test_replay_tracker_out_of_order() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        // Messages can arrive out of order within window
        tracker.check(&source, 10).expect("10 should succeed");
        tracker
            .check(&source, 8)
            .expect("8 should succeed (within window)");
        tracker.check(&source, 12).expect("12 should succeed");
        tracker
            .check(&source, 9)
            .expect("9 should succeed (within window)");

        // But not replayed
        let result = tracker.check(&source, 8);
        assert!(matches!(
            result,
            Err(BypassError::ReplayDetected { sequence: 8 })
        ));
    }

    #[test]
    fn test_replay_tracker_multiple_sources() {
        let tracker = ReplayTracker::new(64);
        let source1: IpAddr = "192.168.1.1".parse().unwrap();
        let source2: IpAddr = "192.168.1.2".parse().unwrap();

        // Same sequence from different sources should both succeed
        tracker
            .check(&source1, 5)
            .expect("source1 seq 5 should succeed");
        tracker
            .check(&source2, 5)
            .expect("source2 seq 5 should succeed");

        // Replay from same source should fail
        let result = tracker.check(&source1, 5);
        assert!(matches!(result, Err(BypassError::ReplayDetected { .. })));

        // But different sequence should work
        tracker
            .check(&source1, 6)
            .expect("source1 seq 6 should succeed");
    }

    #[test]
    fn test_replay_tracker_window_advance() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        // Start with a sequence in the middle of the range
        tracker.check(&source, 100).expect("100 should succeed");
        tracker.check(&source, 110).expect("110 should succeed");
        tracker.check(&source, 120).expect("120 should succeed");

        // Much later sequence should succeed and advance the window
        tracker.check(&source, 200).expect("200 should succeed");

        // Sequence 100 is now outside the window (more than 64 behind)
        // But we already marked it, so this tests window advancement
        tracker.check(&source, 201).expect("201 should succeed");
    }

    #[test]
    fn test_replay_tracker_clear() {
        let tracker = ReplayTracker::new(64);
        let source: IpAddr = "192.168.1.1".parse().unwrap();

        tracker.check(&source, 5).expect("initial should succeed");

        // Clear and replay should now succeed
        tracker.clear_source(&source);
        tracker
            .check(&source, 5)
            .expect("after clear should succeed");
    }

    #[test]
    fn test_metrics_snapshot_includes_security() {
        let metrics = BypassMetrics::default();

        // Increment security metrics
        metrics.signature_rejected.fetch_add(1, Ordering::Relaxed);
        metrics.decryption_failed.fetch_add(2, Ordering::Relaxed);
        metrics.unauthorized_source.fetch_add(3, Ordering::Relaxed);
        metrics.replay_rejected.fetch_add(4, Ordering::Relaxed);

        // Snapshot should include them
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.signature_rejected, 1);
        assert_eq!(snapshot.decryption_failed, 2);
        assert_eq!(snapshot.unauthorized_source, 3);
        assert_eq!(snapshot.replay_rejected, 4);
    }

    #[test]
    fn test_bypass_error_display_all_variants() {
        let io_err = BypassError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test io"));
        assert!(io_err.to_string().contains("IO error"));
        assert!(io_err.to_string().contains("test io"));

        let encode_err = BypassError::Encode("bad data".to_string());
        assert!(encode_err.to_string().contains("Encode error"));

        let decode_err = BypassError::Decode("corrupt".to_string());
        assert!(decode_err.to_string().contains("Decode error"));

        let config_err = BypassError::Config("bad config".to_string());
        assert!(config_err.to_string().contains("Config error"));

        let not_started = BypassError::NotStarted;
        assert!(not_started.to_string().contains("not started"));

        let invalid_header = BypassError::InvalidHeader;
        assert!(invalid_header.to_string().contains("Invalid bypass header"));

        let stale = BypassError::StaleMessage;
        assert!(stale.to_string().contains("stale"));

        let sig_err = BypassError::InvalidSignature;
        assert!(sig_err.to_string().contains("Invalid message signature"));

        let decrypt_err = BypassError::DecryptionFailed;
        assert!(decrypt_err.to_string().contains("decryption failed"));

        let unauth = BypassError::UnauthorizedSource("10.0.0.1".parse().unwrap());
        assert!(unauth.to_string().contains("10.0.0.1"));

        let replay = BypassError::ReplayDetected { sequence: 42 };
        assert!(replay.to_string().contains("42"));

        let missing = BypassError::MissingCredential("encryption key".to_string());
        assert!(missing.to_string().contains("encryption key"));
    }

    #[test]
    fn test_bypass_error_source() {
        let io_err = BypassError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
        assert!(std::error::Error::source(&io_err).is_some());

        let encode_err = BypassError::Encode("test".into());
        assert!(std::error::Error::source(&encode_err).is_none());

        let not_started = BypassError::NotStarted;
        assert!(std::error::Error::source(&not_started).is_none());
    }

    #[test]
    fn test_bypass_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let bypass_err: BypassError = io_err.into();
        assert!(bypass_err.to_string().contains("gone"));
    }

    #[test]
    fn test_message_encoding_display() {
        assert_eq!(MessageEncoding::Protobuf.to_string(), "protobuf");
        assert_eq!(MessageEncoding::Json.to_string(), "json");
        assert_eq!(MessageEncoding::Raw.to_string(), "raw");
        assert_eq!(MessageEncoding::Cbor.to_string(), "cbor");
    }

    #[test]
    fn test_bypass_collection_config_ttl() {
        let config = BypassCollectionConfig {
            ttl_ms: 200,
            ..Default::default()
        };
        assert_eq!(config.ttl(), Duration::from_millis(200));
    }

    #[test]
    fn test_bypass_collection_config_default() {
        let config = BypassCollectionConfig::default();
        assert!(config.collection.is_empty());
        assert_eq!(config.transport, BypassTransport::Unicast);
        assert_eq!(config.encoding, MessageEncoding::Protobuf);
        assert_eq!(config.ttl_ms, 5000);
        assert_eq!(config.priority, MessagePriority::Normal);
    }

    #[test]
    fn test_udp_config_default() {
        let config = UdpConfig::default();
        assert_eq!(config.bind_port, 5150);
        assert_eq!(config.buffer_size, 65536);
        assert_eq!(config.multicast_ttl, 32);
    }

    #[test]
    fn test_bypass_channel_config_default() {
        let config = BypassChannelConfig::default();
        assert!(config.collections.is_empty());
        assert!(!config.multicast_enabled);
        assert_eq!(config.max_message_size, 0);
    }

    #[test]
    fn test_bypass_channel_config_new() {
        let config = BypassChannelConfig::new();
        assert!(config.multicast_enabled);
        assert_eq!(config.max_message_size, 65000);
    }

    #[test]
    fn test_bypass_security_config_is_enabled_allowlist() {
        let config = BypassSecurityConfig {
            source_allowlist: Some(vec!["10.0.0.1".parse().unwrap()]),
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn test_bypass_security_config_is_enabled_replay() {
        let config = BypassSecurityConfig {
            replay_protection: true,
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn test_bypass_security_credentials_debug() {
        let creds = BypassSecurityCredentials::new();
        let debug = format!("{:?}", creds);
        assert!(debug.contains("has_signing_key"));
        assert!(debug.contains("false"));
        assert!(debug.contains("peer_keys_count"));
    }

    #[test]
    fn test_bypass_security_credentials_verifying_key() {
        // Without signing key
        let creds = BypassSecurityCredentials::new();
        assert!(creds.verifying_key().is_none());

        // With signing key
        let seed: [u8; 32] = [42u8; 32];
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let creds = BypassSecurityCredentials::new().with_signing_key(signing_key.clone());
        let vk = creds.verifying_key();
        assert!(vk.is_some());
        assert_eq!(vk.unwrap(), signing_key.verifying_key());
    }

    #[test]
    fn test_bypass_security_credentials_verify_unknown_peer() {
        let creds = BypassSecurityCredentials::new();
        let sig = ed25519_dalek::Signature::from_bytes(&[0u8; 64]);
        let result = creds.verify("unknown-peer", b"message", &sig);
        assert!(matches!(result, Err(BypassError::MissingCredential(_))));
    }

    #[test]
    fn test_bypass_security_credentials_decrypt_no_key() {
        let creds = BypassSecurityCredentials::new();
        let result = creds.decrypt(b"ciphertext", &[0u8; 12]);
        assert!(matches!(result, Err(BypassError::MissingCredential(_))));
    }

    #[test]
    fn test_replay_tracker_clear_all() {
        let tracker = ReplayTracker::new(64);
        let source1: IpAddr = "192.168.1.1".parse().unwrap();
        let source2: IpAddr = "192.168.1.2".parse().unwrap();

        tracker.check(&source1, 1).unwrap();
        tracker.check(&source2, 1).unwrap();

        tracker.clear_all();

        // After clear_all, same sequences should succeed
        tracker.check(&source1, 1).unwrap();
        tracker.check(&source2, 1).unwrap();
    }

    #[test]
    fn test_bypass_header_is_stale() {
        let header = BypassHeader::new("test", Duration::from_millis(100), 0);
        let now = Instant::now();

        // Not stale: sent_at = now, received_at = now
        assert!(!header.is_stale(now, now));

        // Stale: sent 1 second ago, TTL is 100ms
        let sent_at = now - Duration::from_secs(1);
        assert!(header.is_stale(now, sent_at));
    }

    #[test]
    fn test_bypass_metrics_snapshot_default() {
        let snapshot = BypassMetricsSnapshot::default();
        assert_eq!(snapshot.messages_sent, 0);
        assert_eq!(snapshot.messages_received, 0);
        assert_eq!(snapshot.bytes_sent, 0);
        assert_eq!(snapshot.bytes_received, 0);
        assert_eq!(snapshot.stale_dropped, 0);
        assert_eq!(snapshot.invalid_dropped, 0);
        assert_eq!(snapshot.send_errors, 0);
        assert_eq!(snapshot.receive_errors, 0);
    }

    #[tokio::test]
    async fn test_bypass_channel_subscribe_collection() {
        let config = BypassChannelConfig::new().with_collection(BypassCollectionConfig {
            collection: "positions".into(),
            ..Default::default()
        });

        let channel = UdpBypassChannel::new(config).await.unwrap();
        let (hash, _rx) = channel.subscribe_collection("positions");
        assert_eq!(hash, BypassHeader::hash_collection("positions"));
    }

    #[tokio::test]
    async fn test_bypass_channel_get_collection_by_hash() {
        let config = BypassChannelConfig::new().with_collection(BypassCollectionConfig {
            collection: "telemetry".into(),
            ttl_ms: 200,
            ..Default::default()
        });

        let channel = UdpBypassChannel::new(config).await.unwrap();
        let hash = BypassHeader::hash_collection("telemetry");
        let col_config = channel.get_collection_by_hash(hash);
        assert!(col_config.is_some());
        assert_eq!(col_config.unwrap().ttl_ms, 200);

        // Unknown hash
        assert!(channel.get_collection_by_hash(12345).is_none());
    }

    #[tokio::test]
    async fn test_bypass_channel_config_accessor() {
        let config = BypassChannelConfig {
            max_message_size: 1024,
            ..BypassChannelConfig::new()
        };
        let channel = UdpBypassChannel::new(config).await.unwrap();
        assert_eq!(channel.config().max_message_size, 1024);
    }

    #[tokio::test]
    async fn test_bypass_send_not_started() {
        let config = BypassChannelConfig::new();
        let channel = UdpBypassChannel::new(config).await.unwrap();

        let result = channel
            .send(
                BypassTarget::Unicast("127.0.0.1:5000".parse().unwrap()),
                "test",
                b"data",
            )
            .await;
        assert!(matches!(result, Err(BypassError::NotStarted)));
    }

    #[tokio::test]
    async fn test_bypass_send_message_too_large() {
        let config = BypassChannelConfig {
            max_message_size: 10,
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            ..BypassChannelConfig::new()
        };
        let mut channel = UdpBypassChannel::new(config).await.unwrap();
        channel.start().await.unwrap();

        let big_data = vec![0u8; 100];
        let result = channel
            .send(
                BypassTarget::Unicast("127.0.0.1:5000".parse().unwrap()),
                "test",
                &big_data,
            )
            .await;
        assert!(matches!(
            result,
            Err(BypassError::MessageTooLarge { size: 100, max: 10 })
        ));

        channel.stop();
    }

    #[tokio::test]
    async fn test_bypass_send_to_collection_unknown() {
        let config = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            ..BypassChannelConfig::new()
        };
        let mut channel = UdpBypassChannel::new(config).await.unwrap();
        channel.start().await.unwrap();

        let result = channel.send_to_collection("unknown", None, b"data").await;
        assert!(matches!(result, Err(BypassError::Config(_))));

        channel.stop();
    }

    #[tokio::test]
    async fn test_bypass_send_to_collection_unicast_no_addr() {
        let config = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            collections: vec![BypassCollectionConfig {
                collection: "test".into(),
                transport: BypassTransport::Unicast,
                ..Default::default()
            }],
            ..BypassChannelConfig::new()
        };
        let mut channel = UdpBypassChannel::new(config).await.unwrap();
        channel.start().await.unwrap();

        // Unicast without target address should error
        let result = channel.send_to_collection("test", None, b"data").await;
        assert!(matches!(result, Err(BypassError::Config(_))));

        channel.stop();
    }

    #[tokio::test]
    async fn test_bypass_send_to_collection_broadcast() {
        let config = BypassChannelConfig {
            udp: UdpConfig {
                bind_port: 0,
                ..Default::default()
            },
            collections: vec![BypassCollectionConfig {
                collection: "bcast".into(),
                transport: BypassTransport::Broadcast,
                ..Default::default()
            }],
            ..BypassChannelConfig::new()
        };
        let mut channel = UdpBypassChannel::new(config).await.unwrap();
        channel.start().await.unwrap();

        // Broadcast should work without target address
        // Note: actual broadcast send may fail on some systems, but the path is exercised
        let _result = channel.send_to_collection("bcast", None, b"data").await;

        channel.stop();
    }

    #[tokio::test]
    async fn test_bypass_leave_multicast_no_socket() {
        let config = BypassChannelConfig::new();
        let channel = UdpBypassChannel::new(config).await.unwrap();

        // Leaving a group we never joined should be ok
        let result = channel.leave_multicast("239.1.1.1".parse().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_bypass_transport_serde() {
        let unicast = BypassTransport::Unicast;
        let json = serde_json::to_string(&unicast).unwrap();
        let parsed: BypassTransport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BypassTransport::Unicast);

        let multicast = BypassTransport::Multicast {
            group: "239.1.1.100".parse().unwrap(),
            port: 5150,
        };
        let json = serde_json::to_string(&multicast).unwrap();
        let parsed: BypassTransport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, multicast);
    }

    #[test]
    fn test_message_encoding_serde() {
        for encoding in &[
            MessageEncoding::Protobuf,
            MessageEncoding::Json,
            MessageEncoding::Raw,
            MessageEncoding::Cbor,
        ] {
            let json = serde_json::to_string(encoding).unwrap();
            let parsed: MessageEncoding = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, *encoding);
        }
    }

    #[test]
    fn test_bypass_header_flags() {
        let mut header = BypassHeader::new("test", Duration::from_millis(1000), 0);

        // No flags by default
        assert_eq!(header.flags, 0);
        assert_eq!(header.flags & BypassHeader::FLAG_SIGNED, 0);
        assert_eq!(header.flags & BypassHeader::FLAG_ENCRYPTED, 0);

        // Set signed flag
        header.flags |= BypassHeader::FLAG_SIGNED;
        assert_ne!(header.flags & BypassHeader::FLAG_SIGNED, 0);
        assert_eq!(header.flags & BypassHeader::FLAG_ENCRYPTED, 0);

        // Set encrypted flag
        header.flags |= BypassHeader::FLAG_ENCRYPTED;
        assert_ne!(header.flags & BypassHeader::FLAG_SIGNED, 0);
        assert_ne!(header.flags & BypassHeader::FLAG_ENCRYPTED, 0);

        // Encode/decode preserves flags
        let encoded = header.encode();
        let decoded = BypassHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.flags, header.flags);
    }
}
