# ADR-043: Consumer Interface Adapters

**Status**: Proposed
**Date**: 2026-01-06
**Authors**: Kit Plummer, Codex
**Related ADRs**:
- [ADR-029](029-tak-transport-adapter.md) (TAK Transport Adapter)
- [ADR-032](032-pluggable-transport-abstraction.md) (Pluggable Transport Abstraction)
- [ADR-042](042-direct-udp-bypass-pathway.md) (Direct UDP Bypass Pathway)
- [ADR-005](005-datasync-abstraction-layer.md) (Data Sync Abstraction Layer)

---

## Context

### Problem Statement

HIVE Protocol's primary interface is via **direct Rust API integration** using the `hive-protocol` and `hive-ffi` crates. However, many potential consumers cannot integrate at this level:

1. **Legacy C2 Systems**: Existing command and control systems built on older stacks
2. **Web Dashboards**: Browser-based monitoring and control interfaces
3. **Scripting/Automation**: Python, Node.js, or other runtime integrations
4. **Hardware Devices**: Embedded systems with limited language support
5. **Third-Party Services**: Cloud services needing event streams

These systems require **network-based interfaces** that don't require Rust compilation or FFI bindings.

### Consumer Interface Requirements

From stakeholder feedback, the following interface types are needed:

| Interface | Use Case | Characteristics |
|-----------|----------|-----------------|
| **WebSocket** | Web dashboards, real-time UIs | Bidirectional, streaming, browser-compatible |
| **TCP** | Legacy C2, industrial systems | Reliable, simple framing, long-lived connections |
| **HTTP/REST** | Scripting, automation, monitoring | Request/response, stateless, cacheable |

### Existing Pattern: TAK Transport Adapter

ADR-029 established the **Transport Adapter** pattern for TAK integration:

```rust
#[async_trait]
pub trait TakTransport: Send + Sync {
    async fn connect(&mut self) -> Result<(), TakError>;
    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError>;
    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError>;
}
```

This pattern works for **outbound integration** (HIVE → TAK). We need the inverse for **consumer interfaces** (External Systems → HIVE).

### Existing Infrastructure

The `hive-transport` crate already provides basic HTTP/REST endpoints:

```
GET /api/v1/status - Node status
GET /api/v1/peers - Connected peers
GET /api/v1/cell - Cell information
POST /api/v1/command - Send command
```

This needs extension for:
- Streaming subscriptions (WebSocket/SSE)
- TCP adapter for non-HTTP clients
- Event-driven push (not just request/response)

---

## Decision

### Consumer Interface Adapter Architecture

We will implement **Consumer Interface Adapters** as a facade layer over the HIVE Protocol API, supporting multiple transport protocols with unified semantics.

```
┌─────────────────────────────────────────────────────────────────┐
│                   Consumer Interface Layer                       │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │  WebSocket   │  │    TCP       │  │  HTTP/REST   │           │
│  │   Adapter    │  │   Adapter    │  │   Adapter    │           │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘           │
│         │                 │                 │                    │
│         └─────────────────┼─────────────────┘                    │
│                           │                                      │
│                           ▼                                      │
│                  ┌────────────────┐                              │
│                  │  Interface     │                              │
│                  │  Coordinator   │                              │
│                  └────────┬───────┘                              │
│                           │                                      │
├───────────────────────────┼──────────────────────────────────────┤
│                           ▼                                      │
│                  ┌────────────────┐                              │
│                  │ hive-protocol  │                              │
│                  │     API        │                              │
│                  └────────────────┘                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Core Traits

```rust
/// Consumer interface adapter trait
///
/// Provides unified semantics across WebSocket, TCP, and HTTP
#[async_trait]
pub trait ConsumerAdapter: Send + Sync {
    /// Adapter type identifier
    fn adapter_type(&self) -> AdapterType;

    /// Start the adapter (bind to port, start accepting connections)
    async fn start(&self) -> Result<(), AdapterError>;

    /// Stop the adapter gracefully
    async fn stop(&self) -> Result<(), AdapterError>;

    /// Check if adapter is running
    fn is_running(&self) -> bool;

    /// Get adapter metrics
    fn metrics(&self) -> AdapterMetrics;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterType {
    WebSocket,
    Tcp,
    HttpRest,
}

/// Consumer session representing a connected client
#[async_trait]
pub trait ConsumerSession: Send + Sync {
    /// Unique session identifier
    fn session_id(&self) -> &str;

    /// Client address
    fn client_addr(&self) -> SocketAddr;

    /// Send message to client
    async fn send(&self, message: ConsumerMessage) -> Result<(), AdapterError>;

    /// Subscribe to collections with filter
    async fn subscribe(
        &self,
        collection: &str,
        filter: Option<ConsumerFilter>,
    ) -> Result<SubscriptionId, AdapterError>;

    /// Unsubscribe from a subscription
    async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<(), AdapterError>;

    /// Close the session
    async fn close(&self) -> Result<(), AdapterError>;
}
```

### Message Format

All adapters use a unified message format (JSON for WebSocket/HTTP, optionally binary for TCP):

```rust
/// Consumer message format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConsumerMessage {
    /// Document update notification
    DocumentUpdate {
        collection: String,
        document_id: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },

    /// Cell state change
    CellUpdate {
        cell_id: String,
        leader_id: Option<String>,
        members: Vec<String>,
        capabilities: Vec<CapabilityInfo>,
    },

    /// Peer connection change
    PeerUpdate {
        peer_id: String,
        connected: bool,
        address: Option<String>,
    },

    /// Command acknowledgment
    CommandAck {
        command_id: String,
        status: CommandStatus,
        message: Option<String>,
    },

    /// Error message
    Error {
        code: String,
        message: String,
    },
}

/// Command from consumer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConsumerCommand {
    /// Subscribe to collection updates
    Subscribe {
        collection: String,
        filter: Option<serde_json::Value>,
    },

    /// Unsubscribe from updates
    Unsubscribe {
        subscription_id: String,
    },

    /// Query documents
    Query {
        collection: String,
        query: serde_json::Value,
    },

    /// Write document
    Write {
        collection: String,
        document: serde_json::Value,
    },

    /// Send command to node/cell
    Command {
        command_id: String,
        target: CommandTarget,
        action: String,
        params: serde_json::Value,
    },
}
```

---

## WebSocket Adapter

### Overview

WebSocket provides bidirectional streaming for real-time applications:

- **Full-duplex**: Client and server can send at any time
- **Browser-compatible**: Works in web dashboards
- **Efficient**: Single connection, minimal overhead
- **Streaming**: Push updates as they happen

### Implementation

```rust
/// WebSocket consumer adapter
pub struct WebSocketAdapter {
    config: WebSocketConfig,
    sessions: Arc<RwLock<HashMap<String, Arc<WebSocketSession>>>>,
    hive: Arc<HiveClient>,
    metrics: Arc<AdapterMetrics>,
    running: AtomicBool,
    shutdown: broadcast::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Listen address (e.g., "0.0.0.0:8080")
    pub listen_addr: SocketAddr,

    /// WebSocket path (e.g., "/ws" or "/api/v1/stream")
    pub path: String,

    /// Enable TLS
    pub tls: Option<TlsConfig>,

    /// Maximum connections
    pub max_connections: usize,

    /// Ping interval for keepalive
    pub ping_interval: Duration,

    /// Authentication required
    pub require_auth: bool,

    /// CORS allowed origins
    pub cors_origins: Vec<String>,
}

impl WebSocketAdapter {
    pub fn new(config: WebSocketConfig, hive: Arc<HiveClient>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            hive,
            metrics: Arc::new(AdapterMetrics::default()),
            running: AtomicBool::new(false),
            shutdown: broadcast::channel(1).0,
            config,
        }
    }
}

#[async_trait]
impl ConsumerAdapter for WebSocketAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::WebSocket
    }

    async fn start(&self) -> Result<(), AdapterError> {
        let listener = TcpListener::bind(&self.config.listen_addr).await?;
        self.running.store(true, Ordering::SeqCst);

        let sessions = self.sessions.clone();
        let hive = self.hive.clone();
        let metrics = self.metrics.clone();
        let config = self.config.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let session = WebSocketSession::new(
                                    stream, addr, hive.clone(), config.clone()
                                ).await;

                                if let Ok(session) = session {
                                    let session = Arc::new(session);
                                    sessions.write().await.insert(
                                        session.session_id().to_string(),
                                        session.clone()
                                    );
                                    metrics.connections.fetch_add(1, Ordering::Relaxed);

                                    // Spawn session handler
                                    tokio::spawn(Self::handle_session(session, hive.clone()));
                                }
                            }
                            Err(e) => {
                                warn!("WebSocket accept error: {:?}", e);
                            }
                        }
                    }
                    _ = shutdown.recv() => {
                        break;
                    }
                }
            }
        });

        info!("WebSocket adapter started on {}", self.config.listen_addr);
        Ok(())
    }

    async fn stop(&self) -> Result<(), AdapterError> {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.shutdown.send(());

        // Close all sessions
        for session in self.sessions.read().await.values() {
            let _ = session.close().await;
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> AdapterMetrics {
        (*self.metrics).clone()
    }
}
```

### WebSocket Protocol

**Connection flow**:
```
Client                                    Server
   │                                         │
   ├─► GET /ws (HTTP Upgrade) ─────────────►│
   │                                         │
   │◄─ 101 Switching Protocols ─────────────│
   │                                         │
   ├─► {"type": "Subscribe",                │
   │     "collection": "cells"} ───────────►│
   │                                         │
   │◄─ {"type": "DocumentUpdate",           │
   │     "collection": "cells", ...} ───────│
   │                                         │
   │◄─ (push updates as they happen) ───────│
   │                                         │
```

---

## TCP Adapter

### Overview

TCP provides simple framed messaging for legacy systems:

- **Reliable**: Guaranteed delivery and ordering
- **Simple**: Basic length-prefixed framing
- **Persistent**: Long-lived connections
- **Efficient**: Binary or JSON payloads

### Implementation

```rust
/// TCP consumer adapter
pub struct TcpAdapter {
    config: TcpConfig,
    sessions: Arc<RwLock<HashMap<String, Arc<TcpSession>>>>,
    hive: Arc<HiveClient>,
    metrics: Arc<AdapterMetrics>,
    running: AtomicBool,
    shutdown: broadcast::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct TcpConfig {
    /// Listen address
    pub listen_addr: SocketAddr,

    /// Enable TLS
    pub tls: Option<TlsConfig>,

    /// Message framing mode
    pub framing: TcpFraming,

    /// Maximum message size
    pub max_message_size: usize,

    /// Read timeout
    pub read_timeout: Duration,

    /// Authentication required
    pub require_auth: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum TcpFraming {
    /// 4-byte big-endian length prefix
    LengthPrefixed,

    /// Newline-delimited JSON
    NewlineDelimited,

    /// Custom delimiter
    Delimiter(u8),
}

/// TCP message frame
struct TcpFrame {
    length: u32,
    payload: Vec<u8>,
}

impl TcpAdapter {
    /// Read a framed message from TCP stream
    async fn read_frame(
        stream: &mut TcpStream,
        framing: TcpFraming,
        max_size: usize,
    ) -> Result<Vec<u8>, AdapterError> {
        match framing {
            TcpFraming::LengthPrefixed => {
                let mut len_buf = [0u8; 4];
                stream.read_exact(&mut len_buf).await?;
                let len = u32::from_be_bytes(len_buf) as usize;

                if len > max_size {
                    return Err(AdapterError::MessageTooLarge(len));
                }

                let mut payload = vec![0u8; len];
                stream.read_exact(&mut payload).await?;
                Ok(payload)
            }
            TcpFraming::NewlineDelimited => {
                let mut line = String::new();
                let mut reader = BufReader::new(stream);
                reader.read_line(&mut line).await?;
                Ok(line.into_bytes())
            }
            TcpFraming::Delimiter(delim) => {
                let mut buf = Vec::new();
                loop {
                    let mut byte = [0u8; 1];
                    stream.read_exact(&mut byte).await?;
                    if byte[0] == delim {
                        break;
                    }
                    buf.push(byte[0]);
                    if buf.len() > max_size {
                        return Err(AdapterError::MessageTooLarge(buf.len()));
                    }
                }
                Ok(buf)
            }
        }
    }

    /// Write a framed message to TCP stream
    async fn write_frame(
        stream: &mut TcpStream,
        framing: TcpFraming,
        payload: &[u8],
    ) -> Result<(), AdapterError> {
        match framing {
            TcpFraming::LengthPrefixed => {
                let len = (payload.len() as u32).to_be_bytes();
                stream.write_all(&len).await?;
                stream.write_all(payload).await?;
            }
            TcpFraming::NewlineDelimited => {
                stream.write_all(payload).await?;
                stream.write_all(b"\n").await?;
            }
            TcpFraming::Delimiter(delim) => {
                stream.write_all(payload).await?;
                stream.write_all(&[delim]).await?;
            }
        }
        Ok(())
    }
}
```

### TCP Protocol

**Length-prefixed framing**:
```
┌─────────────────────────────────────────────┐
│  4 bytes     │         N bytes             │
│  length (BE) │         payload             │
└─────────────────────────────────────────────┘
```

**Example exchange**:
```
Client                                    Server
   │                                         │
   ├─► TCP Connect ────────────────────────►│
   │                                         │
   ├─► [len][{"type":"Subscribe",...}] ────►│
   │                                         │
   │◄─ [len][{"type":"DocumentUpdate",...}]─│
   │                                         │
```

---

## HTTP/REST Adapter

### Overview

HTTP/REST provides request/response semantics for scripting and automation:

- **Stateless**: Each request independent
- **Cacheable**: GET responses can be cached
- **Simple**: Standard HTTP verbs and status codes
- **Widely supported**: Works with any HTTP client

### Implementation

```rust
/// HTTP/REST consumer adapter
pub struct HttpRestAdapter {
    config: HttpRestConfig,
    hive: Arc<HiveClient>,
    metrics: Arc<AdapterMetrics>,
    running: AtomicBool,
    server_handle: Option<ServerHandle>,
}

#[derive(Debug, Clone)]
pub struct HttpRestConfig {
    /// Listen address
    pub listen_addr: SocketAddr,

    /// Enable TLS
    pub tls: Option<TlsConfig>,

    /// API base path (e.g., "/api/v1")
    pub base_path: String,

    /// Enable CORS
    pub cors: CorsConfig,

    /// Authentication configuration
    pub auth: AuthConfig,

    /// Rate limiting
    pub rate_limit: Option<RateLimitConfig>,
}

impl HttpRestAdapter {
    /// Build HTTP routes
    fn routes(&self) -> Router {
        let hive = self.hive.clone();

        Router::new()
            // Node status
            .route("/status", get(Self::get_status))
            .route("/node", get(Self::get_node))

            // Peers
            .route("/peers", get(Self::get_peers))

            // Cell operations
            .route("/cells", get(Self::list_cells))
            .route("/cells/:cell_id", get(Self::get_cell))

            // Document operations
            .route("/collections/:collection", get(Self::query_collection))
            .route("/collections/:collection", post(Self::write_document))
            .route("/collections/:collection/:doc_id", get(Self::get_document))
            .route("/collections/:collection/:doc_id", put(Self::update_document))
            .route("/collections/:collection/:doc_id", delete(Self::delete_document))

            // Commands
            .route("/commands", post(Self::send_command))
            .route("/commands/:command_id", get(Self::get_command_status))

            // Subscriptions via Server-Sent Events (SSE)
            .route("/stream", get(Self::event_stream))
            .route("/stream/:collection", get(Self::collection_stream))

            .with_state(hive)
    }
}
```

### REST API Endpoints

```yaml
# Base path: /api/v1

# Node status
GET /status
  Response: { "status": "healthy", "node_id": "...", "uptime_secs": 3600 }

# Node information
GET /node
  Response: { "id": "...", "platform": "UAV", "capabilities": [...] }

# Peers
GET /peers
  Response: [{ "id": "...", "connected": true, "address": "..." }, ...]

# Cells
GET /cells
  Response: [{ "id": "...", "leader": "...", "members": [...] }, ...]

GET /cells/{cell_id}
  Response: { "id": "...", "leader": "...", "members": [...], "capabilities": [...] }

# Collections (documents)
GET /collections/{collection}
  Query params: ?filter={json}&limit=100&offset=0
  Response: { "documents": [...], "total": 42, "has_more": true }

GET /collections/{collection}/{doc_id}
  Response: { "id": "...", "data": {...}, "updated_at": "..." }

POST /collections/{collection}
  Body: { "data": {...} }
  Response: { "id": "new_doc_id", "created": true }

PUT /collections/{collection}/{doc_id}
  Body: { "data": {...} }
  Response: { "id": "...", "updated": true }

DELETE /collections/{collection}/{doc_id}
  Response: { "deleted": true }

# Commands
POST /commands
  Body: { "target": "cell:abc", "action": "move_to", "params": {...} }
  Response: { "command_id": "...", "status": "pending" }

GET /commands/{command_id}
  Response: { "command_id": "...", "status": "completed", "result": {...} }

# Server-Sent Events (SSE) for streaming
GET /stream
  Headers: Accept: text/event-stream
  Response: SSE stream of all updates

GET /stream/{collection}
  Headers: Accept: text/event-stream
  Query params: ?filter={json}
  Response: SSE stream of collection updates
```

### Server-Sent Events (SSE)

For HTTP clients that need streaming without WebSocket:

```rust
/// Server-Sent Events endpoint
async fn event_stream(
    State(hive): State<Arc<HiveClient>>,
    Query(params): Query<StreamParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = hive.subscribe_all(params.filter).await.unwrap();

    let sse_stream = stream.map(|update| {
        let data = serde_json::to_string(&update).unwrap();
        Ok(Event::default()
            .event("update")
            .data(data))
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
}
```

**SSE format**:
```
event: update
data: {"type":"DocumentUpdate","collection":"cells",...}

event: update
data: {"type":"PeerUpdate","peer_id":"abc123",...}

: keepalive

event: update
data: {"type":"DocumentUpdate","collection":"nodes",...}
```

---

## Interface Coordinator

The Interface Coordinator manages all adapters and provides unified configuration:

```rust
/// Coordinates all consumer interface adapters
pub struct InterfaceCoordinator {
    adapters: Vec<Box<dyn ConsumerAdapter>>,
    hive: Arc<HiveClient>,
    config: InterfaceConfig,
    metrics: CoordinatorMetrics,
}

#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    /// WebSocket adapter configuration (optional)
    pub websocket: Option<WebSocketConfig>,

    /// TCP adapter configuration (optional)
    pub tcp: Option<TcpConfig>,

    /// HTTP/REST adapter configuration (optional)
    pub http: Option<HttpRestConfig>,

    /// Shared authentication configuration
    pub auth: SharedAuthConfig,
}

impl InterfaceCoordinator {
    pub fn new(config: InterfaceConfig, hive: Arc<HiveClient>) -> Self {
        let mut adapters: Vec<Box<dyn ConsumerAdapter>> = Vec::new();

        if let Some(ws_config) = &config.websocket {
            adapters.push(Box::new(WebSocketAdapter::new(
                ws_config.clone(),
                hive.clone(),
            )));
        }

        if let Some(tcp_config) = &config.tcp {
            adapters.push(Box::new(TcpAdapter::new(
                tcp_config.clone(),
                hive.clone(),
            )));
        }

        if let Some(http_config) = &config.http {
            adapters.push(Box::new(HttpRestAdapter::new(
                http_config.clone(),
                hive.clone(),
            )));
        }

        Self {
            adapters,
            hive,
            config,
            metrics: CoordinatorMetrics::default(),
        }
    }

    /// Start all configured adapters
    pub async fn start(&self) -> Result<(), AdapterError> {
        for adapter in &self.adapters {
            adapter.start().await?;
            info!("Started {:?} adapter", adapter.adapter_type());
        }
        Ok(())
    }

    /// Stop all adapters
    pub async fn stop(&self) -> Result<(), AdapterError> {
        for adapter in &self.adapters {
            adapter.stop().await?;
        }
        Ok(())
    }

    /// Get aggregated metrics
    pub fn metrics(&self) -> CoordinatorMetrics {
        let mut metrics = self.metrics.clone();
        for adapter in &self.adapters {
            metrics.merge(adapter.metrics());
        }
        metrics
    }
}
```

---

## Configuration

### YAML Configuration

```yaml
# hive-config.yaml

consumer_interfaces:
  # WebSocket for real-time dashboards
  websocket:
    enabled: true
    listen_addr: "0.0.0.0:8080"
    path: "/ws"
    tls:
      cert: "/etc/hive/tls/cert.pem"
      key: "/etc/hive/tls/key.pem"
    max_connections: 100
    ping_interval_secs: 30
    cors_origins:
      - "https://dashboard.example.com"
      - "http://localhost:3000"

  # TCP for legacy C2 systems
  tcp:
    enabled: true
    listen_addr: "0.0.0.0:5151"
    framing: "length_prefixed"
    max_message_size: 1048576  # 1 MB
    require_auth: true

  # HTTP/REST for scripting and automation
  http:
    enabled: true
    listen_addr: "0.0.0.0:8081"
    base_path: "/api/v1"
    tls:
      cert: "/etc/hive/tls/cert.pem"
      key: "/etc/hive/tls/key.pem"
    cors:
      allowed_origins: ["*"]
      allowed_methods: ["GET", "POST", "PUT", "DELETE"]
    rate_limit:
      requests_per_minute: 1000

  # Shared authentication
  auth:
    type: "bearer_token"  # or "basic", "mtls", "none"
    tokens:
      - name: "dashboard"
        token_hash: "sha256:..."
        permissions: ["read", "subscribe"]
      - name: "automation"
        token_hash: "sha256:..."
        permissions: ["read", "write", "command"]
```

---

## Authentication

### Token-Based Authentication

```rust
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Authentication type
    pub auth_type: AuthType,

    /// Registered tokens/credentials
    pub credentials: Vec<Credential>,
}

#[derive(Debug, Clone)]
pub enum AuthType {
    /// No authentication (development only)
    None,

    /// Bearer token in Authorization header
    BearerToken,

    /// HTTP Basic authentication
    Basic,

    /// Mutual TLS (client certificate)
    MutualTls,
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub name: String,
    pub token_hash: String,  // bcrypt or sha256 hash
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read documents and state
    Read,
    /// Subscribe to streams
    Subscribe,
    /// Write documents
    Write,
    /// Send commands
    Command,
    /// Administrative operations
    Admin,
}

/// Authenticate a request
pub async fn authenticate(
    auth_config: &AuthConfig,
    request: &Request,
) -> Result<AuthContext, AuthError> {
    match auth_config.auth_type {
        AuthType::None => Ok(AuthContext::anonymous()),

        AuthType::BearerToken => {
            let token = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .ok_or(AuthError::MissingToken)?;

            let credential = auth_config
                .credentials
                .iter()
                .find(|c| verify_token_hash(token, &c.token_hash))
                .ok_or(AuthError::InvalidToken)?;

            Ok(AuthContext {
                identity: credential.name.clone(),
                permissions: credential.permissions.clone(),
            })
        }

        AuthType::Basic => {
            // Similar flow with Basic auth header
            todo!()
        }

        AuthType::MutualTls => {
            // Extract client certificate from TLS session
            todo!()
        }
    }
}
```

---

## Metrics

```rust
/// Adapter metrics
#[derive(Debug, Clone, Default)]
pub struct AdapterMetrics {
    /// Total connections
    pub connections: AtomicU64,

    /// Active connections
    pub active_connections: AtomicU64,

    /// Messages received
    pub messages_received: AtomicU64,

    /// Messages sent
    pub messages_sent: AtomicU64,

    /// Bytes received
    pub bytes_received: AtomicU64,

    /// Bytes sent
    pub bytes_sent: AtomicU64,

    /// Errors
    pub errors: AtomicU64,

    /// Authentication failures
    pub auth_failures: AtomicU64,
}

/// Coordinator metrics
#[derive(Debug, Clone, Default)]
pub struct CoordinatorMetrics {
    pub websocket: AdapterMetrics,
    pub tcp: AdapterMetrics,
    pub http: AdapterMetrics,
    pub total_requests: AtomicU64,
    pub uptime_secs: AtomicU64,
}
```

---

## Implementation Plan

### Phase 1: HTTP/REST Adapter

- [ ] Extend `hive-transport` with full REST API
- [ ] Add SSE streaming endpoint
- [ ] Implement authentication middleware
- [ ] Add rate limiting

### Phase 2: WebSocket Adapter

- [ ] WebSocket server with Tokio
- [ ] Session management
- [ ] Subscription handling
- [ ] Ping/pong keepalive

### Phase 3: TCP Adapter

- [ ] TCP server with framing
- [ ] Length-prefixed and newline modes
- [ ] Binary message support
- [ ] Session management

### Phase 4: Interface Coordinator

- [ ] Unified configuration
- [ ] Multi-adapter startup
- [ ] Aggregated metrics
- [ ] Health endpoints

### Phase 5: Security

- [ ] Bearer token authentication
- [ ] Mutual TLS support
- [ ] Permission enforcement
- [ ] Audit logging

---

## Success Criteria

### Functional

- [ ] WebSocket clients can subscribe and receive real-time updates
- [ ] TCP clients can send/receive framed messages
- [ ] HTTP clients can query, write, and subscribe via SSE
- [ ] Authentication works for all adapters

### Performance

- [ ] WebSocket: 1000+ concurrent connections
- [ ] TCP: 500+ concurrent connections
- [ ] HTTP: 1000+ requests/second
- [ ] Latency: <10ms for local operations

### Testing

- [ ] Unit tests for each adapter
- [ ] Integration tests with mock clients
- [ ] Load tests with concurrent connections
- [ ] Security tests for auth bypass

---

## Security Considerations

### Transport Security

- TLS required for production (configurable for dev)
- Modern cipher suites only
- Certificate validation for mTLS

### Authentication

- Token-based auth for WebSocket/HTTP
- Optional mTLS for TCP
- Permission scopes limit access

### Rate Limiting

- Per-IP rate limits
- Per-token rate limits
- Global rate limits

### Input Validation

- Schema validation for JSON inputs
- Size limits for messages
- Sanitization of user data

---

## References

1. [ADR-029](029-tak-transport-adapter.md) - TAK Transport pattern
2. [ADR-032](032-pluggable-transport-abstraction.md) - Transport abstraction
3. [WebSocket RFC 6455](https://tools.ietf.org/html/rfc6455)
4. [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events)
5. [Axum Web Framework](https://docs.rs/axum)
6. [Tokio-tungstenite](https://docs.rs/tokio-tungstenite)

---

**Last Updated**: 2026-01-06
**Status**: PROPOSED - Awaiting review
