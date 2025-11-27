# ADR-029: TAK Transport Adapter

**Status**: Proposed
**Date**: 2025-11-26
**Authors**: Kit Plummer
**Related ADRs**:
- [ADR-020](020-TAK-CoT-Integration.md) (TAK & CoT Integration)
- [ADR-010](010-transport-layer-udp-tcp.md) (Transport Layer)
- [ADR-019](019-qos-and-data-prioritization.md) (QoS and Data Prioritization)
- [ADR-028](028-cot-detail-extension-schema.md) (CoT Custom Detail Extension Schema)

**Source**: M1 POC integrator feedback (TAK_INTEGRATION_REQUIREMENTS.md)

## Context

### Problem Statement

ADR-020 defines the high-level TAK/CoT integration strategy, but the transport layer requires detailed architectural decisions:

1. **Protocol Complexity**: TAK supports multiple transport modes:
   - TAK Server TCP (ports 8087/8088)
   - TAK Server TCP+SSL (ports 8089)
   - Mesh SA UDP multicast
   - TAK Protocol v1 (Protobuf framing)

2. **DIL Environments**: M1 vignette operates in contested/disconnected-intermittent-limited (DIL) environments where TAK server connectivity is unreliable.

3. **QoS Integration**: Outgoing CoT messages need priority-aware handling that integrates with ADR-019 bandwidth allocation.

4. **Connection Lifecycle**: TAK connections require certificate-based authentication, reconnection logic, and health monitoring.

### Transport Adapter Pattern

HIVE already uses transport adapters for backend abstraction (Ditto, Iroh). TAK integration follows this pattern as an external bridge transport, not a CRDT sync backend.

## Decision

We will implement `TakTransport` as a first-class transport adapter with DIL-resilient message queuing, QoS integration, and support for multiple TAK protocol modes.

### Trait Definition

```rust
/// TAK Transport Adapter
///
/// Provides bidirectional CoT message transport between HIVE and TAK ecosystem.
/// Supports TAK Server (TCP/SSL) and Mesh SA (UDP multicast) modes.
#[async_trait]
pub trait TakTransport: Send + Sync {
    /// Connect to TAK server or mesh
    ///
    /// For TAK Server mode: Establishes TCP/SSL connection
    /// For Mesh SA mode: Joins multicast group
    async fn connect(&mut self) -> Result<(), TakError>;

    /// Disconnect gracefully
    ///
    /// Closes connection and flushes pending messages (if possible)
    async fn disconnect(&mut self) -> Result<(), TakError>;

    /// Send CoT event to TAK
    ///
    /// Message is queued if disconnected (DIL resilience).
    /// Priority determines queue position and drop precedence.
    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError>;

    /// Subscribe to incoming CoT events
    ///
    /// Returns a stream of CoT events matching the filter.
    /// Stream continues across reconnections.
    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError>;

    /// Check connection health
    fn is_connected(&self) -> bool;

    /// Get connection metrics
    fn metrics(&self) -> TakMetrics;

    /// Get current queue depth
    fn queue_depth(&self) -> QueueDepthMetrics;
}

/// Error types for TAK transport operations
#[derive(Debug, thiserror::Error)]
pub enum TakError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Message encoding failed: {0}")]
    EncodingError(String),

    #[error("Message decoding failed: {0}")]
    DecodingError(String),

    #[error("Queue full, message dropped")]
    QueueFull,

    #[error("Connection timeout")]
    Timeout,

    #[error("TLS/SSL error: {0}")]
    TlsError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

### Configuration Model

```rust
/// TAK Transport Configuration
#[derive(Debug, Clone)]
pub struct TakTransportConfig {
    /// Transport mode
    pub mode: TakTransportMode,

    /// Client identity for authentication
    pub identity: Option<TakIdentity>,

    /// Message queue configuration (DIL resilience)
    pub queue: QueueConfig,

    /// Reconnection policy
    pub reconnect: ReconnectPolicy,

    /// Protocol options
    pub protocol: ProtocolConfig,

    /// Metrics collection
    pub metrics_enabled: bool,
}

/// Transport mode selection
#[derive(Debug, Clone)]
pub enum TakTransportMode {
    /// TAK Server TCP connection
    TakServer {
        /// Server address (host:port)
        address: SocketAddr,
        /// Use SSL/TLS
        use_tls: bool,
    },

    /// Mesh SA UDP multicast
    MeshSa {
        /// Multicast group address
        multicast_group: IpAddr,
        /// Port (typically 6969)
        port: u16,
        /// Network interface to bind
        interface: Option<String>,
    },

    /// Dual mode: TAK Server primary, Mesh SA fallback
    Hybrid {
        server: Box<TakTransportMode>,
        mesh: Box<TakTransportMode>,
    },
}

/// Client identity for TAK authentication
#[derive(Debug, Clone)]
pub struct TakIdentity {
    /// Client certificate (PEM or DER)
    pub client_cert: PathBuf,

    /// Client private key (PEM or DER)
    pub client_key: PathBuf,

    /// CA certificate for server verification
    pub ca_cert: Option<PathBuf>,

    /// Callsign for TAK identification
    pub callsign: String,

    /// TAK user credentials (alternative to cert auth)
    pub credentials: Option<TakCredentials>,
}

#[derive(Debug, Clone)]
pub struct TakCredentials {
    pub username: String,
    pub password: String,
}

/// Protocol configuration
#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    /// Protocol version
    pub version: TakProtocolVersion,

    /// CoT XML encoding options
    pub xml_options: XmlEncodingOptions,

    /// Message size limit (bytes)
    pub max_message_size: usize,

    /// Heartbeat interval
    pub heartbeat_interval: Duration,
}

#[derive(Debug, Clone, Copy)]
pub enum TakProtocolVersion {
    /// CoT XML over TCP (legacy)
    XmlTcp,

    /// TAK Protocol v1 (Protobuf with framing)
    /// Format: [magic: u8][version: u8][magic: u8][payload_len: varint][payload]
    ProtobufV1,
}

#[derive(Debug, Clone)]
pub struct XmlEncodingOptions {
    /// Include XML declaration
    pub xml_declaration: bool,

    /// Pretty print (development only)
    pub pretty_print: bool,

    /// Include HIVE extension by default
    pub include_hive_extension: bool,
}
```

### DIL Message Queuing

The M1 vignette operates in contested environments where TAK connectivity is intermittent. The transport must buffer outgoing messages for retry.

```rust
/// Message queue configuration for DIL resilience
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum queue size (messages)
    pub max_messages: usize,

    /// Maximum queue size (bytes)
    pub max_bytes: usize,

    /// Per-priority queue limits
    pub priority_limits: PriorityQueueLimits,

    /// Stale message filtering
    pub filter_stale: bool,

    /// Queue persistence (survive restarts)
    pub persistent: bool,

    /// Persistence path (if persistent=true)
    pub persistence_path: Option<PathBuf>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_messages: 1000,
            max_bytes: 10 * 1024 * 1024, // 10 MB
            priority_limits: PriorityQueueLimits::default(),
            filter_stale: true,
            persistent: false,
            persistence_path: None,
        }
    }
}

/// Per-priority queue limits
///
/// Higher priorities get more queue space and are drained first.
#[derive(Debug, Clone)]
pub struct PriorityQueueLimits {
    /// P1 (Critical): Always accepted, drained first
    pub p1_limit: usize,
    /// P2 (High): High priority
    pub p2_limit: usize,
    /// P3 (Normal): Standard limit
    pub p3_limit: usize,
    /// P4 (Low): Reduced limit
    pub p4_limit: usize,
    /// P5 (Bulk): Minimal limit, first to drop
    pub p5_limit: usize,
}

impl Default for PriorityQueueLimits {
    fn default() -> Self {
        Self {
            p1_limit: 200,   // 20% - never dropped
            p2_limit: 300,   // 30%
            p3_limit: 250,   // 25%
            p4_limit: 150,   // 15%
            p5_limit: 100,   // 10% - first to drop
        }
    }
}

/// Priority-aware message queue
pub struct TakMessageQueue {
    config: QueueConfig,
    queues: [VecDeque<QueuedMessage>; 5],
    total_bytes: AtomicUsize,
    metrics: QueueMetrics,
}

#[derive(Debug)]
pub struct QueuedMessage {
    pub event: CotEvent,
    pub priority: Priority,
    pub enqueued_at: Instant,
    pub stale_time: DateTime<Utc>,
    pub size_bytes: usize,
}

impl TakMessageQueue {
    /// Enqueue a message for sending
    ///
    /// Returns error if queue is full and message cannot be accepted.
    pub fn enqueue(&mut self, event: CotEvent, priority: Priority) -> Result<(), TakError> {
        let msg = QueuedMessage {
            size_bytes: event.encoded_size(),
            stale_time: event.stale_time(),
            enqueued_at: Instant::now(),
            event,
            priority,
        };

        // Check per-priority limit
        let queue_idx = priority.as_index();
        let limit = self.config.priority_limits.limit_for(priority);

        if self.queues[queue_idx].len() >= limit {
            // Try to drop lower priority message
            if !self.drop_lowest_priority() {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                return Err(TakError::QueueFull);
            }
        }

        self.queues[queue_idx].push_back(msg);
        self.total_bytes.fetch_add(msg.size_bytes, Ordering::Relaxed);
        self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Dequeue next message (priority order)
    ///
    /// Filters out stale messages automatically.
    pub fn dequeue(&mut self) -> Option<QueuedMessage> {
        let now = Utc::now();

        // Drain in priority order (P1 first)
        for queue in &mut self.queues {
            while let Some(msg) = queue.pop_front() {
                self.total_bytes.fetch_sub(msg.size_bytes, Ordering::Relaxed);

                // Filter stale messages
                if self.config.filter_stale && msg.stale_time < now {
                    self.metrics.stale_dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                self.metrics.dequeued.fetch_add(1, Ordering::Relaxed);
                return Some(msg);
            }
        }

        None
    }

    /// Replay all queued messages on reconnection
    pub async fn replay(&mut self, transport: &impl TakTransport) -> ReplayResult {
        let mut sent = 0;
        let mut dropped = 0;

        while let Some(msg) = self.dequeue() {
            match transport.send_cot(&msg.event, msg.priority).await {
                Ok(_) => sent += 1,
                Err(_) => {
                    dropped += 1;
                    // Re-queue if still valid
                    if msg.stale_time > Utc::now() {
                        let _ = self.enqueue(msg.event, msg.priority);
                    }
                    break; // Connection likely lost
                }
            }
        }

        ReplayResult { sent, dropped }
    }
}
```

### Reconnection Policy

```rust
/// Reconnection behavior configuration
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    /// Enable automatic reconnection
    pub enabled: bool,

    /// Initial retry delay
    pub initial_delay: Duration,

    /// Maximum retry delay (exponential backoff cap)
    pub max_delay: Duration,

    /// Backoff multiplier
    pub backoff_multiplier: f64,

    /// Maximum reconnection attempts (None = unlimited)
    pub max_attempts: Option<usize>,

    /// Jitter factor (0.0 - 1.0)
    pub jitter: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
            jitter: 0.1,
        }
    }
}

/// Reconnection state machine
pub struct ReconnectionManager {
    policy: ReconnectPolicy,
    current_delay: Duration,
    attempts: usize,
    last_attempt: Option<Instant>,
}

impl ReconnectionManager {
    pub fn should_reconnect(&self) -> bool {
        if !self.policy.enabled {
            return false;
        }

        if let Some(max) = self.policy.max_attempts {
            if self.attempts >= max {
                return false;
            }
        }

        true
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current_delay;

        // Apply exponential backoff
        self.current_delay = Duration::from_secs_f64(
            (self.current_delay.as_secs_f64() * self.policy.backoff_multiplier)
                .min(self.policy.max_delay.as_secs_f64())
        );

        // Apply jitter
        let jitter_range = delay.as_secs_f64() * self.policy.jitter;
        let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
        let final_delay = Duration::from_secs_f64((delay.as_secs_f64() + jitter).max(0.0));

        self.attempts += 1;
        self.last_attempt = Some(Instant::now());

        final_delay
    }

    pub fn reset(&mut self) {
        self.current_delay = self.policy.initial_delay;
        self.attempts = 0;
    }
}
```

### TAK Server Implementation

```rust
/// TAK Server TCP/SSL transport implementation
pub struct TakServerTransport {
    config: TakTransportConfig,
    connection: Option<TakConnection>,
    queue: TakMessageQueue,
    reconnect: ReconnectionManager,
    metrics: TakMetrics,
    encoder: CotEncoder,
    decoder: CotDecoder,
}

enum TakConnection {
    Tcp(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl TakServerTransport {
    pub fn new(config: TakTransportConfig) -> Self {
        Self {
            queue: TakMessageQueue::new(config.queue.clone()),
            reconnect: ReconnectionManager::new(config.reconnect.clone()),
            encoder: CotEncoder::new(config.protocol.xml_options.clone()),
            decoder: CotDecoder::new(),
            connection: None,
            metrics: TakMetrics::default(),
            config,
        }
    }

    async fn establish_connection(&mut self) -> Result<(), TakError> {
        let TakTransportMode::TakServer { address, use_tls } = &self.config.mode else {
            return Err(TakError::ConnectionFailed("Invalid mode".into()));
        };

        let tcp_stream = TcpStream::connect(address).await
            .map_err(|e| TakError::ConnectionFailed(e.to_string()))?;

        if *use_tls {
            let tls_config = self.build_tls_config()?;
            let connector = TlsConnector::from(Arc::new(tls_config));
            let tls_stream = connector.connect(address.ip().to_string().as_str(), tcp_stream)
                .await
                .map_err(|e| TakError::TlsError(e.to_string()))?;

            self.connection = Some(TakConnection::Tls(tls_stream));
        } else {
            self.connection = Some(TakConnection::Tcp(tcp_stream));
        }

        // Send initial presence
        self.send_presence().await?;

        self.metrics.connections.fetch_add(1, Ordering::Relaxed);
        self.reconnect.reset();

        Ok(())
    }

    fn build_tls_config(&self) -> Result<rustls::ClientConfig, TakError> {
        let identity = self.config.identity.as_ref()
            .ok_or_else(|| TakError::AuthenticationFailed("No identity configured".into()))?;

        // Load client certificate
        let cert_pem = std::fs::read(&identity.client_cert)
            .map_err(|e| TakError::AuthenticationFailed(format!("Failed to read cert: {}", e)))?;
        let certs = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .map_err(|e| TakError::AuthenticationFailed(format!("Invalid cert: {}", e)))?;

        // Load client key
        let key_pem = std::fs::read(&identity.client_key)
            .map_err(|e| TakError::AuthenticationFailed(format!("Failed to read key: {}", e)))?;
        let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .map_err(|e| TakError::AuthenticationFailed(format!("Invalid key: {}", e)))?
            .ok_or_else(|| TakError::AuthenticationFailed("No private key found".into()))?;

        // Build config
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(self.load_ca_certs()?)
            .with_client_auth_cert(certs, key)
            .map_err(|e| TakError::TlsError(e.to_string()))?;

        Ok(config)
    }

    async fn send_presence(&mut self) -> Result<(), TakError> {
        let callsign = self.config.identity.as_ref()
            .map(|i| i.callsign.as_str())
            .unwrap_or("HIVE-BRIDGE");

        let presence = CotEvent::presence(callsign);
        self.send_raw(&presence).await
    }
}

#[async_trait]
impl TakTransport for TakServerTransport {
    async fn connect(&mut self) -> Result<(), TakError> {
        self.establish_connection().await
    }

    async fn disconnect(&mut self) -> Result<(), TakError> {
        if let Some(conn) = self.connection.take() {
            // Graceful shutdown
            match conn {
                TakConnection::Tcp(stream) => {
                    let _ = stream.shutdown().await;
                }
                TakConnection::Tls(stream) => {
                    let _ = stream.shutdown().await;
                }
            }
        }
        Ok(())
    }

    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError> {
        if self.is_connected() {
            self.send_raw(event).await
        } else {
            // Queue for later
            self.queue.enqueue(event.clone(), priority)
        }
    }

    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError> {
        // Implementation uses async channel to stream incoming events
        todo!()
    }

    fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    fn metrics(&self) -> TakMetrics {
        self.metrics.clone()
    }

    fn queue_depth(&self) -> QueueDepthMetrics {
        self.queue.metrics()
    }
}
```

### Mesh SA Implementation

```rust
/// Mesh SA UDP multicast transport
pub struct MeshSaTransport {
    config: TakTransportConfig,
    socket: Option<UdpSocket>,
    queue: TakMessageQueue,
    metrics: TakMetrics,
    encoder: CotEncoder,
    decoder: CotDecoder,
}

impl MeshSaTransport {
    pub fn new(config: TakTransportConfig) -> Self {
        Self {
            queue: TakMessageQueue::new(config.queue.clone()),
            encoder: CotEncoder::new(config.protocol.xml_options.clone()),
            decoder: CotDecoder::new(),
            socket: None,
            metrics: TakMetrics::default(),
            config,
        }
    }
}

#[async_trait]
impl TakTransport for MeshSaTransport {
    async fn connect(&mut self) -> Result<(), TakError> {
        let TakTransportMode::MeshSa { multicast_group, port, interface } = &self.config.mode else {
            return Err(TakError::ConnectionFailed("Invalid mode".into()));
        };

        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await
            .map_err(|e| TakError::ConnectionFailed(e.to_string()))?;

        // Join multicast group
        match multicast_group {
            IpAddr::V4(addr) => {
                socket.join_multicast_v4(*addr, Ipv4Addr::UNSPECIFIED)
                    .map_err(|e| TakError::ConnectionFailed(format!("Multicast join failed: {}", e)))?;
            }
            IpAddr::V6(addr) => {
                socket.join_multicast_v6(addr, 0)
                    .map_err(|e| TakError::ConnectionFailed(format!("Multicast join failed: {}", e)))?;
            }
        }

        self.socket = Some(socket);
        self.metrics.connections.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), TakError> {
        self.socket.take();
        Ok(())
    }

    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError> {
        if let Some(socket) = &self.socket {
            let TakTransportMode::MeshSa { multicast_group, port, .. } = &self.config.mode else {
                return Err(TakError::ConnectionFailed("Invalid mode".into()));
            };

            let encoded = self.encoder.encode(event)?;

            // TAK Protocol v1 framing for Mesh SA
            let framed = self.frame_mesh_sa(&encoded);

            socket.send_to(&framed, (*multicast_group, *port)).await
                .map_err(|e| TakError::IoError(e))?;

            self.metrics.messages_sent.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            self.queue.enqueue(event.clone(), priority)
        }
    }

    async fn subscribe(&self, _filter: CotFilter) -> Result<CotEventStream, TakError> {
        todo!()
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    fn metrics(&self) -> TakMetrics {
        self.metrics.clone()
    }

    fn queue_depth(&self) -> QueueDepthMetrics {
        self.queue.metrics()
    }
}

impl MeshSaTransport {
    /// Frame message for TAK Mesh SA protocol
    ///
    /// Format: [191][1][191][varint_len][payload]
    fn frame_mesh_sa(&self, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(4 + payload.len());

        // TAK Protocol v1 magic bytes for mesh
        frame.push(191);
        frame.push(1);
        frame.push(191);

        // Payload length as varint
        self.encode_varint(payload.len() as u64, &mut frame);

        // Payload
        frame.extend_from_slice(payload);

        frame
    }

    fn encode_varint(&self, mut value: u64, buf: &mut Vec<u8>) {
        while value >= 0x80 {
            buf.push((value as u8 & 0x7F) | 0x80);
            value >>= 7;
        }
        buf.push(value as u8);
    }
}
```

### Metrics

```rust
/// TAK transport metrics
#[derive(Debug, Clone, Default)]
pub struct TakMetrics {
    /// Total connections established
    pub connections: AtomicU64,

    /// Total messages sent
    pub messages_sent: AtomicU64,

    /// Total messages received
    pub messages_received: AtomicU64,

    /// Total bytes sent
    pub bytes_sent: AtomicU64,

    /// Total bytes received
    pub bytes_received: AtomicU64,

    /// Messages dropped (queue full)
    pub messages_dropped: AtomicU64,

    /// Reconnection attempts
    pub reconnect_attempts: AtomicU64,

    /// Last error message
    pub last_error: RwLock<Option<String>>,

    /// Connection uptime
    pub connected_since: RwLock<Option<Instant>>,
}

/// Queue depth metrics
#[derive(Debug, Clone)]
pub struct QueueDepthMetrics {
    pub p1_depth: usize,
    pub p2_depth: usize,
    pub p3_depth: usize,
    pub p4_depth: usize,
    pub p5_depth: usize,
    pub total_bytes: usize,
    pub stale_dropped: u64,
}
```

## Implementation Phases

### Phase 1: MVP (M1 Vignette Support)

1. `TakServerTransport` - TCP connection (no SSL)
2. Basic `CotEncoder` for TrackUpdate, CapabilityAdvertisement
3. Simple queue (in-memory, non-persistent)
4. Manual reconnection

**Success Criteria**:
- Connect to FreeTakServer
- Send track updates visible in ATAK
- Survive 30-second disconnection

### Phase 2: Production Ready

1. SSL/TLS with certificate authentication
2. Full DIL message queue with priority
3. Automatic reconnection with backoff
4. `CotDecoder` for incoming commands
5. Metrics collection

**Success Criteria**:
- Authenticate with TAK Server PKI
- Queue holds 1000 messages during 5-minute outage
- P1 messages delivered within 2s of reconnection

### Phase 3: Full Feature

1. Mesh SA UDP multicast
2. Hybrid mode (server + mesh fallback)
3. Persistent queue (survive restarts)
4. TAK Protocol v1 (Protobuf)
5. Subscription filtering

**Success Criteria**:
- Mesh SA works in disconnected LAN
- Hybrid mode auto-failover < 5s
- Persistent queue survives process restart

## Consequences

### Positive

1. **DIL Resilience**: Messages survive connectivity gaps
2. **QoS Integration**: Priority-aware queue draining
3. **Protocol Flexibility**: Support multiple TAK modes
4. **Observability**: Rich metrics for monitoring
5. **Testability**: Trait allows mock implementations

### Negative

1. **Complexity**: Multiple protocol implementations
2. **Memory Usage**: Message queue requires memory
3. **State Management**: Connection lifecycle complexity
4. **Testing**: Requires TAK server infrastructure

### Risks and Mitigations

**Risk 1**: Queue grows unbounded during long outages
- **Mitigation**: Per-priority limits, total byte limit
- **Mitigation**: Stale message filtering
- **Mitigation**: P5 messages dropped first

**Risk 2**: Certificate management complexity
- **Mitigation**: Support both cert and password auth
- **Mitigation**: Clear error messages for auth failures
- **Mitigation**: Documentation with examples

**Risk 3**: Protocol version incompatibility
- **Mitigation**: Version negotiation
- **Mitigation**: Fallback to XML if Protobuf fails
- **Mitigation**: Test against multiple TAK versions

## Testing Strategy

### Unit Tests

1. Queue priority ordering
2. Stale message filtering
3. Reconnection backoff calculation
4. CoT encoding/decoding
5. TAK Protocol framing

### Integration Tests

1. FreeTakServer connection lifecycle
2. Message round-trip (HIVE → TAK → ATAK)
3. Disconnection/reconnection handling
4. Certificate authentication

### E2E Tests

1. M1 vignette scenario
2. 5-minute network partition
3. Priority message delivery order
4. Mesh SA multicast

## Dependencies

### Rust Crates

- `tokio` - Async runtime
- `tokio-rustls` - TLS support
- `quick-xml` - CoT XML encoding
- `prost` - Protobuf encoding (TAK Protocol v1)
- `socket2` - Multicast socket options

### External

- FreeTakServer (development/testing)
- TAK Server (production validation)
- ATAK (UI verification)

## References

1. [TAK Protocol Documentation](https://tak.gov)
2. [FreeTakServer](https://github.com/FreeTAKTeam/FreeTakServer)
3. [cottak Rust crate](https://docs.rs/cottak/latest/cottak/)
4. [M1 POC TAK Integration Requirements](TAK_INTEGRATION_REQUIREMENTS.md)
5. [ADR-019 QoS](019-qos-and-data-prioritization.md)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-26 | Created ADR-029 | M1 POC feedback - need detailed transport architecture |
| 2025-11-26 | DIL message queuing required | M1 operates in contested environments |
| 2025-11-26 | Priority-aware queue | Integration with ADR-019 QoS |
| 2025-11-26 | Support both TAK Server and Mesh SA | Different deployment scenarios |
| 2025-11-26 | Trait-based design | Enables mock implementations for testing |
