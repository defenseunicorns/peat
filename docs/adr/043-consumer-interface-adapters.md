# ADR-043: Consumer Interface Adapters (Compatibility Layer)

**Status**: Proposed  
**Date**: 2026-01-06 (Updated: 2025-01-31)  
**Authors**: Kit Plummer, Claude  
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)  
**Related ADRs**:
- [ADR-029](029-tak-transport-adapter.md) (TAK Transport Adapter)
- [ADR-032](032-pluggable-transport-abstraction.md) (Pluggable Transport Abstraction)
- [ADR-042](042-direct-udp-bypass-pathway.md) (Direct UDP Bypass Pathway)
- [ADR-005](005-datasync-abstraction-layer.md) (Data Sync Abstraction Layer)
- [ADR-049](049-schema-extraction-and-codegen.md) (Schema Extraction)
- [ADR-050](050-sdk-integration.md) (HIVE SDK - Optimal Integration Path)

---

## Executive Summary

This ADR defines **Consumer Interface Adapters** - network-based interfaces (WebSocket, TCP, HTTP/REST) that enable external systems to interact with HIVE nodes.

> **⚠️ IMPORTANT**: Consumer interface adapters are the **compatibility layer**, not the optimal integration path. Systems that can integrate the HIVE SDK directly (ADR-050) should do so for better performance, offline capability, and full CRDT benefits. These adapters exist for systems that **cannot** be modified to embed the SDK.

---

## Integration Path Guidance

### Decision Tree

```
┌─────────────────────────────────────────────────────────────┐
│           Can you modify the external system?                │
└─────────────────────────┬───────────────────────────────────┘
                          │
              ┌───────────┴───────────┐
              │                       │
              ▼                       ▼
┌─────────────────────────┐   ┌─────────────────────────────────┐
│          YES            │   │              NO                  │
│                         │   │                                  │
│  Use hive-sdk           │   │  Use Consumer Interface Adapters│
│  (ADR-050)              │   │  (This ADR)                     │
│                         │   │                                  │
│  • Full CRDT sync       │   │  • Request/response only        │
│  • Offline capable      │   │  • Requires adapter reachable   │
│  • Minimal latency      │   │  • +50-200ms latency            │
│  • Native hierarchy     │   │  • Flat data model              │
└─────────────────────────┘   └─────────────────────────────────┘
```

### Comparison: SDK vs Consumer Interface Adapters

| Capability | SDK (ADR-050) | Adapters (This ADR) |
|------------|---------------|---------------------|
| **Latency** | Sync only (~10-50ms) | +50-200ms overhead |
| **Offline Operation** | ✅ Full - queues changes, syncs on reconnect | ❌ None - requires adapter |
| **CRDT Conflict Resolution** | ✅ Automatic, deterministic | ❌ Last-write-wins at adapter |
| **Hierarchical Membership** | ✅ Full cell participation | ❌ Not a cell member |
| **Capability Aggregation** | ✅ Contributes to cell capabilities | ❌ Not aggregated |
| **Bandwidth Efficiency** | ✅ Delta sync only | ❌ Full message per request |
| **Peer-to-Peer** | ✅ Direct sync with any peer | ❌ Must go through adapter |
| **Multi-Transport** | ✅ Iroh, BLE, LoRa, etc. | ❌ HTTP/WS/TCP only |

### When to Use Each Path

**Use Consumer Interface Adapters (This ADR) for:**
- Existing ATAK/WinTAK installations that cannot run custom plugins
- Legacy GCS software with no extension capability
- Legacy C2 systems built on older stacks
- IoT hubs that only speak MQTT
- Web browsers (until WASM SDK is available)
- Monitoring dashboards and debugging tools
- Compliance bridges (e.g., mandatory Link 16 feeds)
- Third-party services needing event streams

**Use the SDK (ADR-050) for:**
- New autonomous platforms (UAS, UGV, USV)
- Custom C2 applications
- ROS2 robots and robotic systems
- ATAK plugins (can embed SDK)
- Mobile apps with native development
- Any system where you control the software

---

## Latency Analysis

### SDK Integration (Optimal Path)

```
Platform A ──CRDT Sync──► Platform B
           ~10-50ms (network dependent)
           
• No intermediate hops
• Delta sync (only changes)
• Offline queue when disconnected
• Reconnect automatically syncs
```

### Adapter Integration (Compatibility Path)

```
Legacy System ──HTTP/WS──► Adapter ──Query──► HIVE ──Response──► Adapter ──HTTP/WS──► Legacy
              ~20ms        ~5ms      ~10ms     ~5ms      ~5ms       ~20ms
                                                                    
Total: ~65ms minimum, typically 100-200ms
```

**Latency Breakdown**:

| Component | Latency | Notes |
|-----------|---------|-------|
| Network to Adapter | 10-50ms | Depends on network topology |
| Protocol parsing | 1-5ms | JSON/Protobuf decode |
| HIVE query | 5-20ms | Local CRDT read |
| Response serialization | 1-5ms | JSON/Protobuf encode |
| Network from Adapter | 10-50ms | Return path |
| **Total** | **50-200ms** | Per request |

**Additional Adapter Limitations**:
- No offline operation (adapter must be reachable)
- No automatic conflict resolution (last-write-wins at adapter)
- No hierarchical aggregation for external system
- Polling required for updates (or WebSocket with its own overhead)
- Single point of failure if adapter node goes down

---

## Context

### Problem Statement

HIVE Protocol's primary interface is via **direct Rust API integration** using the `hive-protocol` and `hive-ffi` crates, or the **HIVE SDK** (ADR-050) for multi-language support. However, many potential consumers cannot integrate at this level:

1. **Legacy C2 Systems**: Existing command and control systems built on older stacks
2. **Web Dashboards**: Browser-based monitoring and control interfaces
3. **Scripting/Automation**: Python, Node.js, or other runtime integrations (when SDK is unavailable)
4. **Hardware Devices**: Embedded systems with limited language support
5. **Third-Party Services**: Cloud services needing event streams

These systems require **network-based interfaces** that don't require Rust compilation, FFI bindings, or SDK embedding.

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
┌─────────────────────────────────────────────────────────────────────────┐
│                         HIVE Node                                        │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │               Consumer Interface Layer (This ADR)                │   │
│  │                                                                  │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │   │
│  │  │  WebSocket   │  │    TCP       │  │  HTTP/REST   │          │   │
│  │  │   Adapter    │  │   Adapter    │  │   Adapter    │          │   │
│  │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │   │
│  │         │                 │                 │                   │   │
│  │         └─────────────────┼─────────────────┘                   │   │
│  │                           │                                     │   │
│  │                           ▼                                     │   │
│  │                  ┌────────────────┐                             │   │
│  │                  │   Interface    │                             │   │
│  │                  │  Coordinator   │                             │   │
│  │                  └────────┬───────┘                             │   │
│  │                           │                                     │   │
│  └───────────────────────────┼─────────────────────────────────────┘   │
│                              │                                          │
│                              ▼                                          │
│                     ┌────────────────┐                                  │
│                     │ hive-protocol  │                                  │
│                     │     API        │                                  │
│                     └────────────────┘                                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
              ▼                     ▼                     ▼
       ┌──────────┐          ┌──────────┐          ┌──────────┐
       │  Legacy  │          │   Web    │          │   IoT    │
       │   C2     │          │Dashboard │          │   Hub    │
       │ (TCP)    │          │  (WS)    │          │ (HTTP)   │
       └──────────┘          └──────────┘          └──────────┘
       
       These are NOT full mesh participants - they query/command only
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
        // Bind WebSocket server
        // Accept connections
        // Spawn session handlers
        todo!()
    }

    async fn stop(&self) -> Result<(), AdapterError> {
        // Signal shutdown
        // Close all sessions
        // Wait for cleanup
        todo!()
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> AdapterMetrics {
        self.metrics.as_ref().clone()
    }
}
```

---

## TCP Adapter

### Overview

TCP provides a simple, reliable interface for legacy systems:

- **Reliable**: Ordered, guaranteed delivery
- **Simple**: No HTTP overhead
- **Framed**: Length-prefixed or newline-delimited
- **Long-lived**: Persistent connections

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
    /// Listen address (e.g., "0.0.0.0:5151")
    pub listen_addr: SocketAddr,

    /// Message framing mode
    pub framing: FramingMode,

    /// Maximum message size
    pub max_message_size: usize,

    /// Require authentication
    pub require_auth: bool,

    /// Idle timeout
    pub idle_timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
pub enum FramingMode {
    /// 4-byte big-endian length prefix
    LengthPrefixed,

    /// Newline-delimited JSON
    NewlineDelimited,

    /// Custom delimiter
    Delimited(u8),
}

#[async_trait]
impl ConsumerAdapter for TcpAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Tcp
    }

    async fn start(&self) -> Result<(), AdapterError> {
        // Bind TCP listener
        // Accept connections
        // Spawn session handlers with framing codec
        todo!()
    }

    async fn stop(&self) -> Result<(), AdapterError> {
        todo!()
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> AdapterMetrics {
        self.metrics.as_ref().clone()
    }
}
```

---

## HTTP/REST Adapter

### Overview

HTTP/REST provides a familiar interface for scripting and automation:

- **Request/Response**: Simple query model
- **Stateless**: No connection management
- **Cacheable**: Standard HTTP caching
- **SSE**: Server-Sent Events for streaming

### Implementation

```rust
/// HTTP/REST consumer adapter
pub struct HttpAdapter {
    config: HttpConfig,
    hive: Arc<HiveClient>,
    metrics: Arc<AdapterMetrics>,
    running: AtomicBool,
    shutdown: broadcast::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Listen address (e.g., "0.0.0.0:8081")
    pub listen_addr: SocketAddr,

    /// Base path for API (e.g., "/api/v1")
    pub base_path: String,

    /// Enable TLS
    pub tls: Option<TlsConfig>,

    /// CORS configuration
    pub cors: CorsConfig,

    /// Rate limiting
    pub rate_limit: Option<RateLimitConfig>,

    /// Authentication required
    pub require_auth: bool,
}

#[async_trait]
impl ConsumerAdapter for HttpAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::HttpRest
    }

    async fn start(&self) -> Result<(), AdapterError> {
        // Build Axum router with endpoints:
        // GET  /api/v1/status
        // GET  /api/v1/peers
        // GET  /api/v1/cell
        // GET  /api/v1/documents/:collection
        // GET  /api/v1/documents/:collection/:id
        // POST /api/v1/documents/:collection
        // PUT  /api/v1/documents/:collection/:id
        // POST /api/v1/command
        // GET  /api/v1/stream (SSE)
        todo!()
    }

    async fn stop(&self) -> Result<(), AdapterError> {
        todo!()
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> AdapterMetrics {
        self.metrics.as_ref().clone()
    }
}
```

---

## Interface Coordinator

```rust
/// Coordinates multiple consumer interface adapters
pub struct InterfaceCoordinator {
    adapters: Vec<Box<dyn ConsumerAdapter>>,
    config: CoordinatorConfig,
    metrics: Arc<CoordinatorMetrics>,
}

impl InterfaceCoordinator {
    pub fn new(config: CoordinatorConfig, hive: Arc<HiveClient>) -> Self {
        let mut adapters: Vec<Box<dyn ConsumerAdapter>> = Vec::new();

        if config.websocket.enabled {
            adapters.push(Box::new(WebSocketAdapter::new(
                config.websocket.clone(),
                hive.clone(),
            )));
        }

        if config.tcp.enabled {
            adapters.push(Box::new(TcpAdapter::new(
                config.tcp.clone(),
                hive.clone(),
            )));
        }

        if config.http.enabled {
            adapters.push(Box::new(HttpAdapter::new(
                config.http.clone(),
                hive.clone(),
            )));
        }

        Self {
            adapters,
            config,
            metrics: Arc::new(CoordinatorMetrics::default()),
        }
    }

    pub async fn start_all(&self) -> Result<(), AdapterError> {
        for adapter in &self.adapters {
            adapter.start().await?;
        }
        Ok(())
    }

    pub async fn stop_all(&self) -> Result<(), AdapterError> {
        for adapter in &self.adapters {
            adapter.stop().await?;
        }
        Ok(())
    }

    pub fn metrics(&self) -> CoordinatorMetrics {
        let mut metrics = CoordinatorMetrics::default();
        for adapter in &self.adapters {
            match adapter.adapter_type() {
                AdapterType::WebSocket => metrics.websocket = adapter.metrics(),
                AdapterType::Tcp => metrics.tcp = adapter.metrics(),
                AdapterType::HttpRest => metrics.http = adapter.metrics(),
            }
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
- [ ] Latency: <200ms for operations (acceptable for compatibility layer)

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

## Consequences

### Positive

- **Legacy integration** without modifying external systems
- **Multiple protocols** (WS, TCP, HTTP) for different use cases
- **Unified semantics** across all adapters
- **Standard APIs** familiar to developers
- **Debugging/monitoring** - useful for dashboards even when SDK is primary

### Negative

- **Latency overhead** adds 50-200ms vs SDK integration
- **No offline support** - external system depends on adapter availability
- **No CRDT benefits** - external system doesn't get conflict resolution
- **Not a mesh participant** - cannot contribute to hierarchy/aggregation
- **Potential misuse** - developers may use adapters when SDK is better choice

### Neutral

- Coexists with SDK (ADR-050) - different tools for different situations
- Can be disabled if not needed (reduces attack surface)

---

## References

1. [ADR-029](029-tak-transport-adapter.md) - TAK Transport pattern
2. [ADR-032](032-pluggable-transport-abstraction.md) - Transport abstraction
3. [ADR-050](050-sdk-integration.md) - HIVE SDK (Optimal Integration Path)
4. [ADR-049](049-schema-extraction-and-codegen.md) - Schema Extraction
5. [WebSocket RFC 6455](https://tools.ietf.org/html/rfc6455)
6. [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events)
7. [Axum Web Framework](https://docs.rs/axum)
8. [Tokio-tungstenite](https://docs.rs/tokio-tungstenite)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-01-06 | Consumer Interface Adapters for legacy integration | Network-based access for systems that can't embed SDK |
| 2025-01-31 | Position as compatibility layer, not optimal path | SDK provides full CRDT benefits; adapters are fallback |
| 2025-01-31 | Document latency tradeoffs explicitly | Clear guidance on when to use adapters vs SDK |
| 2025-01-31 | Add references to ADR-049, ADR-050 | Complete integration picture |

---

**Last Updated**: 2025-01-31  
**Status**: PROPOSED - Awaiting review
