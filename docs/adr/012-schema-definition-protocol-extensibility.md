# ADR-012: Schema Definition and Protocol Extensibility Architecture

**Status**: Proposed  
**Date**: 2025-11-06  
**Authors**: Kit Plummer, Codex  
**Blocks**: ADR-011 (Automerge + Iroh Integration)  
**Influences**: ADR-005 (Data Sync Abstraction Layer), ADR-007 (Automerge-Based Sync Engine)

## Context

### The Problem: Schema and Protocol Coupling

Current CAP architecture tightly couples message schemas, ontology definitions, and transport mechanisms within the protocol implementation. This creates several critical issues:

1. **Schema Lock-in**: Message schemas are embedded in the protocol code rather than being first-class, versioned artifacts
2. **Limited Extensibility**: Adding new transport protocols (gRPC, MQTT, etc.) requires modifying core protocol logic
3. **Integration Barriers**: External systems (ROS2, legacy C2) cannot easily adopt CAP messages without reimplementing schema parsing
4. **Ontology Fragmentation**: Capability definitions, cell composition rules, and hierarchical structures lack a unified semantic model
5. **Code Generation Gap**: No standard schema format means no tooling for generating type-safe bindings across languages

### Architectural Insight

The feedback revealed a fundamental separation of concerns:

```
┌─────────────────────────────────────────────────────────────┐
│                    CAP ECOSYSTEM                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │      peat-schema (Foundational Library)                │  │
│  │  • Message schemas (Protobuf/Avro/JSON Schema)        │  │
│  │  • Ontology definitions (capabilities, cells, etc)    │  │
│  │  • Validation rules                                   │  │
│  │  • Code generation tooling                            │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓ uses                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │      cap-core (Protocol Implementation)               │  │
│  │  • CRDT sync engine                                   │  │
│  │  • Cell formation logic                               │  │
│  │  • Hierarchical coordination                          │  │
│  │  • Business rules                                     │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓ uses                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │      peat-transport (Protocol Adapters)                │  │
│  │  • HTTP/WebSocket adapter                             │  │
│  │  • gRPC adapter                                       │  │
│  │  • ROS2 DDS adapter                                   │  │
│  │  • MQTT adapter (IoT edge)                            │  │
│  │  • Link 16 adapter (military C2)                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Why This Matters Now

The current ADR-005 (Data Sync Abstraction Layer) proposes abstracting sync backends (Ditto vs Automerge), but it **assumes** a fixed message schema and protocol interface. However:

1. **Integration Reality**: Different systems speak different protocols
   - ROS2 uses DDS/RTPS for pub-sub messaging
   - Military C2 systems use Link 16, VMF, JREAP
   - Cloud/web clients expect REST/GraphQL/WebSocket
   - IoT devices often use MQTT or CoAP

2. **Schema Evolution**: As CAP capabilities grow, message schemas will evolve
   - Need versioning and backward compatibility
   - Need migration paths for deployed systems
   - Need validation and type safety

3. **Multi-Language Support**: CAP isn't just Rust
   - C2 applications are often Java/JavaScript
   - Legacy systems may be C/C++
   - AI/ML pipelines often use Python
   - Mobile apps use Swift/Kotlin

**The architectural principle**: Separate the **WHAT** (schemas/ontology) from the **HOW** (protocol/transport) and the **WHERE** (storage/sync).

## Decision Drivers

### Requirements

1. **Schema as First-Class Artifact**
   - Message schemas are versioned, documented artifacts
   - Can be consumed independently of protocol implementation
   - Support code generation for multiple languages

2. **Protocol Extensibility**
   - Adding new transport protocols doesn't require modifying core logic
   - Protocol adapters implement standard interfaces
   - Can support multiple protocols simultaneously

3. **Ontology Definition**
   - Formal definition of CAP concepts (cells, squads, capabilities)
   - Machine-readable for validation and reasoning
   - Human-readable for documentation and specification

4. **Integration Enablement**
   - External systems can adopt CAP messages without protocol coupling
   - Adapters for common military/IoT protocols
   - Standard interfaces for persistence layer access

5. **Type Safety and Validation**
   - Generate type-safe bindings for Rust, Python, JavaScript, C++
   - Validate messages against schema at runtime
   - Catch schema violations at compile-time where possible

6. **Backward Compatibility**
   - Schema versioning allows evolution without breaking existing systems
   - Migration utilities for upgrading message formats
   - Graceful handling of unknown schema versions

## Decision

We will **separate schema definition, ontology, and protocol extensibility into distinct architectural layers**, creating three new foundational crates:

### 1. `peat-schema` - Schema Definition Library

**Purpose**: Define CAP message schemas and ontology in a standard, code-generatable format

**Technology Choice**: Protocol Buffers (Protobuf) with semantic annotations

**Rationale**:
- Industry-standard IDL with excellent tooling
- Native code generation for 10+ languages
- Compact binary encoding (important for tactical bandwidth)
- Schema evolution features (field deprecation, optional fields)
- gRPC integration if we choose that transport
- Can express both data schemas and service interfaces

**Alternative Considered**: Apache Avro
- Pros: Schema evolution without version numbers, JSON compatibility
- Cons: Less ecosystem support, no service definition like gRPC

**Structure**:
```
peat-schema/
├── proto/
│   ├── core.proto           # Core message types (Position, Timestamp, UUID)
│   ├── platform.proto       # Platform state, capabilities, beacons
│   ├── cell.proto           # Cell formation, membership
│   ├── squad.proto          # Squad composition, hierarchy
│   ├── command.proto        # Command & control messages
│   ├── ontology.proto       # Capability ontology definitions
│   └── service.proto        # Service interfaces (if using gRPC)
├── src/
│   ├── lib.rs               # Generated Rust bindings
│   ├── validation.rs        # Custom validation logic
│   └── semantic.rs          # Semantic type wrappers
├── Cargo.toml
└── README.md
```

**Example Schema** (`platform.proto`):
```protobuf
syntax = "proto3";

package cap.platform.v1;

import "core.proto";
import "ontology.proto";

// Platform state beacon broadcast over mesh
message PlatformBeacon {
  // Unique platform identifier
  string platform_id = 1;
  
  // Current 3D position
  cap.core.v1.Position position = 2;
  
  // Geohash for spatial indexing
  string geohash_cell = 3;
  
  // Platform is operational
  bool operational = 4;
  
  // Last update timestamp
  cap.core.v1.Timestamp updated_at = 5;
  
  // Platform capabilities
  repeated cap.ontology.v1.Capability capabilities = 6;
  
  // Fuel/battery remaining (percentage)
  float fuel_remaining_pct = 7;
  
  // Communication link quality (0.0-1.0)
  float link_quality = 8;
  
  // Message schema version
  uint32 schema_version = 15;
}

// Platform capability advertisement
message CapabilityAdvertisement {
  string platform_id = 1;
  repeated cap.ontology.v1.Capability capabilities = 2;
  cap.core.v1.Timestamp advertised_at = 3;
  
  // Capability metadata
  message CapabilityMetadata {
    string capability_id = 1;
    float quality_score = 2;      // 0.0-1.0
    float availability = 3;        // 0.0-1.0
    uint32 capacity = 4;           // e.g., MB storage, compute cores
    string status = 5;             // "available", "degraded", "offline"
  }
  
  repeated CapabilityMetadata metadata = 4;
}

// Platform command message
message PlatformCommand {
  string command_id = 1;
  string target_platform_id = 2;
  string issued_by = 3;
  cap.core.v1.Timestamp issued_at = 4;
  
  oneof command {
    MoveToPosition move = 10;
    ActivateCapability activate = 11;
    DeactivateCapability deactivate = 12;
    UpdateConfiguration config = 13;
    EmergencyStop stop = 14;
  }
  
  bool requires_acknowledgment = 20;
  cap.core.v1.Timestamp expires_at = 21;
}

message MoveToPosition {
  cap.core.v1.Position target = 1;
  float speed_mps = 2;
  string formation = 3;  // e.g., "follow", "echelon_left"
}

// ... other command types
```

**Example Ontology** (`ontology.proto`):
```protobuf
syntax = "proto3";

package cap.ontology.v1;

// Core capability types
enum CapabilityType {
  CAPABILITY_TYPE_UNSPECIFIED = 0;
  CAPABILITY_TYPE_SENSOR = 1;
  CAPABILITY_TYPE_COMMUNICATION = 2;
  CAPABILITY_TYPE_COMPUTATION = 3;
  CAPABILITY_TYPE_KINETIC = 4;
  CAPABILITY_TYPE_LOGISTICS = 5;
  CAPABILITY_TYPE_MOBILITY = 6;
}

// Capability definition
message Capability {
  string id = 1;
  string name = 2;
  CapabilityType type = 3;
  string description = 4;
  
  // Semantic properties
  map<string, string> properties = 10;
  
  // Resource requirements
  ResourceRequirements requirements = 11;
  
  // Aggregation rules
  AggregationRules aggregation = 12;
}

message ResourceRequirements {
  float power_watts = 1;
  float bandwidth_mbps = 2;
  uint64 compute_mips = 3;
  uint64 storage_mb = 4;
}

// How capabilities combine
message AggregationRules {
  enum AggregationType {
    AGGREGATION_TYPE_UNSPECIFIED = 0;
    AGGREGATION_TYPE_ADDITIVE = 1;      // More platforms = more capability
    AGGREGATION_TYPE_MULTIPLICATIVE = 2; // Synergistic effect
    AGGREGATION_TYPE_COMPLEMENTARY = 3;  // Need specific combinations
    AGGREGATION_TYPE_REDUNDANT = 4;      // No benefit from duplication
  }
  
  AggregationType type = 1;
  repeated string requires_capabilities = 2;  // Prerequisites
  repeated string enhances_capabilities = 3;  // Synergies
}

// Cell composition rules
message CellCompositionRule {
  string rule_id = 1;
  string name = 2;
  
  // Minimum requirements
  uint32 min_platforms = 3;
  uint32 max_platforms = 4;
  
  // Required capabilities
  repeated string required_capabilities = 5;
  
  // Optional capabilities (improve cell effectiveness)
  repeated string optional_capabilities = 6;
  
  // Formation constraints
  FormationConstraints formation = 7;
}

message FormationConstraints {
  float max_distance_meters = 1;
  float optimal_spacing_meters = 2;
  string formation_type = 3;  // "line", "wedge", "column", etc.
}
```

**Code Generation**:
```rust
// Cargo.toml
[build-dependencies]
tonic-build = "0.10"
prost-build = "0.12"

// build.rs
fn main() {
    tonic_build::configure()
        .build_server(false)  // Only client code for now
        .compile(
            &[
                "proto/core.proto",
                "proto/platform.proto",
                "proto/ontology.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
```

**Usage in Rust**:
```rust
use cap_schema::platform::v1::{PlatformBeacon, CapabilityAdvertisement};
use cap_schema::ontology::v1::{Capability, CapabilityType};

// Type-safe message construction
let beacon = PlatformBeacon {
    platform_id: "alpha-1".to_string(),
    position: Some(Position {
        latitude: 37.7749,
        longitude: -122.4194,
        altitude: 100.0,
    }),
    geohash_cell: geohash::encode(37.7749, -122.4194, 7).unwrap(),
    operational: true,
    updated_at: Some(Timestamp::now()),
    capabilities: vec![
        Capability {
            id: "eo-ir-sensor".to_string(),
            capability_type: CapabilityType::Sensor as i32,
            ..Default::default()
        },
    ],
    fuel_remaining_pct: 0.75,
    link_quality: 0.9,
    schema_version: 1,
};

// Serialization to binary (for network)
let bytes = beacon.encode_to_vec();

// Deserialization
let decoded = PlatformBeacon::decode(&bytes[..])?;
```

**Code Generation for Other Languages**:
```bash
# Python
protoc --python_out=./python proto/*.proto

# JavaScript/TypeScript
protoc --js_out=import_style=commonjs,binary:./js proto/*.proto
protoc --ts_out=./ts proto/*.proto

# C++
protoc --cpp_out=./cpp proto/*.proto

# Java
protoc --java_out=./java proto/*.proto
```

### 2. `peat-transport` - Protocol Adapter Abstraction

**Purpose**: Define standard interfaces for protocol adapters and implement concrete transports

**Architecture**: Trait-based plugin system for transport protocols

**Core Abstraction**:
```rust
// peat-transport/src/lib.rs

use cap_schema::platform::v1::PlatformBeacon;
use async_trait::async_trait;
use bytes::Bytes;

/// Core trait for CAP message transport
#[async_trait]
pub trait MessageTransport: Send + Sync {
    /// Send a message to one or more recipients
    async fn send(
        &self,
        message: &dyn CapMessage,
        recipients: &[Recipient],
        metadata: &TransportMetadata,
    ) -> Result<SendReceipt, TransportError>;
    
    /// Subscribe to incoming messages of a specific type
    async fn subscribe<M: CapMessage>(
        &self,
        filter: MessageFilter,
    ) -> Result<MessageStream<M>, TransportError>;
    
    /// Get transport capabilities and status
    fn capabilities(&self) -> TransportCapabilities;
    
    /// Graceful shutdown
    async fn shutdown(&self) -> Result<(), TransportError>;
}

/// All CAP messages implement this trait
pub trait CapMessage: prost::Message + Clone + Send + Sync + 'static {
    fn message_type() -> MessageType;
    fn schema_version() -> u32;
    fn validate(&self) -> Result<(), ValidationError>;
}

/// Transport metadata for QoS, routing, etc.
#[derive(Debug, Clone)]
pub struct TransportMetadata {
    pub priority: Priority,
    pub ttl: Option<Duration>,
    pub reliability: Reliability,
    pub compression: bool,
    pub encryption: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Priority {
    Critical = 0,  // Commands, safety-critical
    High = 1,      // CRDT sync, important state
    Normal = 2,    // Regular telemetry
    Low = 3,       // Bulk data, software updates
}

#[derive(Debug, Clone, Copy)]
pub enum Reliability {
    BestEffort,    // UDP-like, no guarantees
    AtLeastOnce,   // TCP-like, may duplicate
    ExactlyOnce,   // Transactional, expensive
}

/// Recipient addressing (transport-agnostic)
#[derive(Debug, Clone)]
pub enum Recipient {
    PlatformId(String),
    Cell(String),
    Squad(String),
    Broadcast,
    Multicast(MulticastGroup),
}

#[derive(Debug, Clone)]
pub struct MulticastGroup {
    pub id: String,
    pub scope: MulticastScope,
}

#[derive(Debug, Clone, Copy)]
pub enum MulticastScope {
    Local,      // Same subnet
    Cell,       // Cell members
    Squad,      // Squad members
    Platoon,    // Platoon members
}
```

**Transport Implementations**:

```rust
// peat-transport/src/adapters/http_websocket.rs

use axum::{Router, routing::post};
use tokio_tungstenite::WebSocketStream;

/// HTTP/WebSocket transport for web clients and REST APIs
pub struct HttpWebSocketTransport {
    config: HttpConfig,
    router: Router,
    websocket_registry: Arc<RwLock<HashMap<String, WebSocketStream>>>,
    message_bus: Arc<MessageBus>,
}

#[async_trait]
impl MessageTransport for HttpWebSocketTransport {
    async fn send(
        &self,
        message: &dyn CapMessage,
        recipients: &[Recipient],
        metadata: &TransportMetadata,
    ) -> Result<SendReceipt, TransportError> {
        // For WebSocket connections
        for recipient in recipients {
            if let Recipient::PlatformId(id) = recipient {
                if let Some(ws) = self.websocket_registry.read().await.get(id) {
                    let bytes = message.encode_to_vec();
                    ws.send(Message::Binary(bytes)).await?;
                }
            }
        }
        
        // For HTTP POST (less common, mainly for commands from C2)
        // Would use REST endpoints like POST /api/v1/platforms/{id}/commands
        
        Ok(SendReceipt::default())
    }
    
    async fn subscribe<M: CapMessage>(
        &self,
        filter: MessageFilter,
    ) -> Result<MessageStream<M>, TransportError> {
        // WebSocket clients subscribe to message streams
        let (tx, rx) = mpsc::channel(100);
        self.message_bus.register_subscriber(filter, tx).await?;
        Ok(MessageStream::new(rx))
    }
    
    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            max_message_size: 1024 * 1024, // 1MB for WebSocket
            supports_multicast: false,
            supports_broadcast: false,
            latency_ms: 50,
            reliability: Reliability::AtLeastOnce,
        }
    }
}

impl HttpWebSocketTransport {
    pub async fn new(config: HttpConfig) -> Result<Self, TransportError> {
        let message_bus = Arc::new(MessageBus::new());
        
        let router = Router::new()
            .route("/api/v1/platforms/:id/beacon", post(handle_beacon))
            .route("/api/v1/platforms/:id/commands", post(handle_command))
            .route("/ws", axum::routing::get(handle_websocket))
            .with_state(message_bus.clone());
        
        // Start HTTP server
        let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
        tokio::spawn(async move {
            axum::serve(listener, router).await
        });
        
        Ok(Self {
            config,
            router,
            websocket_registry: Arc::new(RwLock::new(HashMap::new())),
            message_bus,
        })
    }
}
```

```rust
// peat-transport/src/adapters/grpc.rs

use tonic::{transport::Server, Request, Response, Status};

/// gRPC transport for high-performance, typed communication
pub struct GrpcTransport {
    config: GrpcConfig,
    server: Option<Server>,
    client_pool: Arc<RwLock<HashMap<String, PlatformServiceClient>>>,
}

// gRPC service definitions in peat-schema/proto/service.proto
// service PlatformService {
//   rpc SendBeacon(PlatformBeacon) returns (SendReceipt);
//   rpc StreamBeacons(StreamRequest) returns (stream PlatformBeacon);
//   rpc SendCommand(PlatformCommand) returns (CommandAcknowledgment);
// }

#[async_trait]
impl MessageTransport for GrpcTransport {
    async fn send(
        &self,
        message: &dyn CapMessage,
        recipients: &[Recipient],
        metadata: &TransportMetadata,
    ) -> Result<SendReceipt, TransportError> {
        // Use gRPC clients from pool
        for recipient in recipients {
            if let Recipient::PlatformId(id) = recipient {
                let mut client = self.get_or_create_client(id).await?;
                
                // Type-safe gRPC call based on message type
                match message.message_type() {
                    MessageType::PlatformBeacon => {
                        let beacon = message
                            .as_any()
                            .downcast_ref::<PlatformBeacon>()
                            .unwrap();
                        client.send_beacon(beacon.clone()).await?;
                    }
                    MessageType::PlatformCommand => {
                        let cmd = message
                            .as_any()
                            .downcast_ref::<PlatformCommand>()
                            .unwrap();
                        client.send_command(cmd.clone()).await?;
                    }
                    // ... other types
                }
            }
        }
        
        Ok(SendReceipt::default())
    }
    
    async fn subscribe<M: CapMessage>(
        &self,
        filter: MessageFilter,
    ) -> Result<MessageStream<M>, TransportError> {
        // gRPC streaming subscription
        // Depends on specific message type
        todo!("Implement gRPC streaming based on filter")
    }
    
    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            max_message_size: 4 * 1024 * 1024, // 4MB
            supports_multicast: false,
            supports_broadcast: false,
            latency_ms: 10, // Lower latency than WebSocket
            reliability: Reliability::AtLeastOnce,
        }
    }
}
```

```rust
// peat-transport/src/adapters/ros2.rs

use rclrs::{Node, Publisher, Subscription};
use rosidl_runtime_rs::Message as RosMessage;

/// ROS2 DDS transport for robotics integration
pub struct Ros2Transport {
    node: Arc<Node>,
    publishers: Arc<RwLock<HashMap<String, Box<dyn RosPublisher>>>>,
    subscriptions: Arc<RwLock<Vec<Box<dyn RosSubscription>>>>,
}

#[async_trait]
impl MessageTransport for Ros2Transport {
    async fn send(
        &self,
        message: &dyn CapMessage,
        recipients: &[Recipient],
        metadata: &TransportMetadata,
    ) -> Result<SendReceipt, TransportError> {
        // Convert CAP protobuf message to ROS2 IDL message
        let ros_msg = self.convert_to_ros_message(message)?;
        
        // Publish on appropriate ROS2 topic
        let topic = self.get_topic_for_message(message.message_type());
        
        let publisher = self.publishers
            .write()
            .await
            .entry(topic.clone())
            .or_insert_with(|| {
                self.node.create_publisher(&topic, rclrs::QOS_PROFILE_DEFAULT)
            });
        
        publisher.publish(&ros_msg)?;
        
        Ok(SendReceipt::default())
    }
    
    async fn subscribe<M: CapMessage>(
        &self,
        filter: MessageFilter,
    ) -> Result<MessageStream<M>, TransportError> {
        let topic = self.get_topic_for_filter(&filter);
        
        let (tx, rx) = mpsc::channel(100);
        
        let subscription = self.node.create_subscription(
            &topic,
            rclrs::QOS_PROFILE_DEFAULT,
            move |ros_msg: RosMessage| {
                // Convert ROS2 message back to CAP protobuf
                if let Ok(cap_msg) = convert_from_ros_message::<M>(ros_msg) {
                    let _ = tx.try_send(cap_msg);
                }
            },
        )?;
        
        self.subscriptions.write().await.push(subscription);
        
        Ok(MessageStream::new(rx))
    }
    
    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            max_message_size: 1024 * 1024, // 1MB
            supports_multicast: true,  // DDS supports multicast
            supports_broadcast: true,
            latency_ms: 5, // Very low latency
            reliability: Reliability::BestEffort, // Configurable in DDS
        }
    }
}

impl Ros2Transport {
    /// Convert CAP protobuf message to ROS2 IDL message
    fn convert_to_ros_message(
        &self,
        message: &dyn CapMessage,
    ) -> Result<RosMessage, TransportError> {
        // This requires generated ROS2 message types
        // Can be automated with a code generation step that creates
        // both protobuf and ROS2 IDL from the same schema
        match message.message_type() {
            MessageType::PlatformBeacon => {
                let beacon = message
                    .as_any()
                    .downcast_ref::<PlatformBeacon>()
                    .unwrap();
                
                // Convert to ROS2 PlatformBeacon message
                // (generated from IDL or manual mapping)
                Ok(ros2_msgs::PlatformBeacon {
                    platform_id: beacon.platform_id.clone(),
                    position: ros2_msgs::Position {
                        latitude: beacon.position.as_ref().unwrap().latitude,
                        longitude: beacon.position.as_ref().unwrap().longitude,
                        altitude: beacon.position.as_ref().unwrap().altitude,
                    },
                    // ... map other fields
                }.into())
            }
            // ... other message types
        }
    }
}
```

### 3. `peat-persistence` - Storage Abstraction

**Purpose**: Define standard interfaces for accessing the data persistence layer, allowing external systems to interact with CAP's data store

**Architecture**: Trait-based abstraction with multiple backend implementations

```rust
// peat-persistence/src/lib.rs

use cap_schema::platform::v1::PlatformBeacon;
use cap_schema::cell::v1::CellState;
use async_trait::async_trait;

/// Core trait for CAP data persistence
#[async_trait]
pub trait DataStore: Send + Sync {
    /// Store or update a document
    async fn upsert<D: Document>(
        &self,
        collection: &str,
        document: D,
    ) -> Result<DocumentId, PersistenceError>;
    
    /// Query documents with filtering
    async fn query<D: Document>(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<Vec<D>, PersistenceError>;
    
    /// Subscribe to live updates
    async fn subscribe<D: Document>(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<ChangeStream<D>, PersistenceError>;
    
    /// Remove a document
    async fn remove(
        &self,
        collection: &str,
        doc_id: &DocumentId,
    ) -> Result<(), PersistenceError>;
    
    /// Execute a transaction
    async fn transaction<F, T>(
        &self,
        f: F,
    ) -> Result<T, PersistenceError>
    where
        F: FnOnce(&mut Transaction) -> Result<T, PersistenceError> + Send;
}

/// Documents must be serializable and have identity
pub trait Document: Serialize + DeserializeOwned + Clone + Send + Sync {
    fn collection() -> &'static str;
    fn id(&self) -> Option<&DocumentId>;
    fn set_id(&mut self, id: DocumentId);
}

/// Query builder for filtering/sorting
#[derive(Debug, Clone)]
pub struct Query {
    filters: Vec<Filter>,
    sort: Option<Sort>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl Query {
    pub fn new() -> Self { /* ... */ }
    
    pub fn filter(mut self, filter: Filter) -> Self { /* ... */ }
    
    pub fn sort(mut self, field: &str, order: SortOrder) -> Self { /* ... */ }
    
    pub fn limit(mut self, limit: usize) -> Self { /* ... */ }
}
```

**Storage Backends**:
```rust
// peat-persistence/src/backends/

pub mod automerge;   // CRDT-based sync store
pub mod ditto;       // Ditto SDK wrapper
pub mod sqlite;      // Local SQLite database
pub mod postgres;    // PostgreSQL for centralized C2
pub mod rocksdb;     // Embedded key-value store
pub mod redis;       // In-memory cache/pub-sub
```

**External Access Interface**:
```rust
// peat-persistence/src/external_api.rs

/// External API for non-CAP systems to access CAP data
pub struct ExternalApi {
    store: Arc<dyn DataStore>,
    auth: Arc<dyn AuthProvider>,
}

impl ExternalApi {
    /// Query platform beacons (read-only for external systems)
    pub async fn query_beacons(
        &self,
        auth_token: &str,
        filter: BeaconFilter,
    ) -> Result<Vec<PlatformBeacon>, ApiError> {
        // Authenticate
        let claims = self.auth.verify_token(auth_token).await?;
        
        // Authorize (can this system read beacons?)
        if !claims.has_permission("cap:read:beacons") {
            return Err(ApiError::Unauthorized);
        }
        
        // Query with filtering
        let query = Query::new()
            .filter(self.build_filter_from_params(filter)?);
        
        self.store.query("platform_beacons", query).await
    }
    
    /// Subscribe to beacon updates (streaming)
    pub async fn subscribe_beacons(
        &self,
        auth_token: &str,
        filter: BeaconFilter,
    ) -> Result<ChangeStream<PlatformBeacon>, ApiError> {
        // Similar auth/authz flow
        let claims = self.auth.verify_token(auth_token).await?;
        if !claims.has_permission("cap:subscribe:beacons") {
            return Err(ApiError::Unauthorized);
        }
        
        let query = Query::new()
            .filter(self.build_filter_from_params(filter)?);
        
        self.store.subscribe("platform_beacons", query).await
    }
}
```

**REST API for External Systems**:
```rust
// Example: ROS2 node querying CAP data via HTTP
// GET /api/v1/beacons?geohash=9q8yy&operational=true
// Authorization: Bearer <token>

pub async fn handle_query_beacons(
    State(api): State<Arc<ExternalApi>>,
    auth: BearerToken,
    Query(params): Query<BeaconQueryParams>,
) -> Result<Json<Vec<PlatformBeacon>>, ApiError> {
    let beacons = api.query_beacons(
        &auth.token,
        BeaconFilter {
            geohash_prefix: params.geohash,
            operational: params.operational,
            capabilities: params.capabilities,
        },
    ).await?;
    
    Ok(Json(beacons))
}
```

## Integration Examples

### Example 1: ROS2 Robot Publishing to CAP

```python
# ROS2 node that publishes robot state to CAP mesh

import rclpy
from rclpy.node import Node
import cap_schema_pb2 as cap  # Generated from protobuf
import grpc

class CapBridge(Node):
    def __init__(self):
        super().__init__('cap_bridge')
        
        # Create gRPC client to CAP node
        channel = grpc.insecure_channel('localhost:50051')
        self.cap_client = cap.PlatformServiceStub(channel)
        
        # Subscribe to ROS2 robot state
        self.subscription = self.create_subscription(
            geometry_msgs.PoseStamped,
            '/robot/pose',
            self.pose_callback,
            10
        )
    
    def pose_callback(self, msg):
        # Convert ROS2 pose to CAP beacon
        beacon = cap.PlatformBeacon(
            platform_id='robot-1',
            position=cap.Position(
                latitude=msg.pose.position.x,  # Simplified
                longitude=msg.pose.position.y,
                altitude=msg.pose.position.z,
            ),
            operational=True,
            capabilities=['mobility', 'manipulation'],
        )
        
        # Send to CAP mesh via gRPC
        try:
            receipt = self.cap_client.SendBeacon(beacon)
            self.get_logger().info(f'Sent beacon: {receipt.id}')
        except grpc.RpcError as e:
            self.get_logger().error(f'Failed to send beacon: {e}')

def main():
    rclpy.init()
    bridge = CapBridge()
    rclpy.spin(bridge)

if __name__ == '__main__':
    main()
```

### Example 2: Legacy C2 System Querying CAP Data

```java
// Java application querying CAP via REST API

import com.fasterxml.jackson.databind.ObjectMapper;
import java.net.http.*;
import java.net.URI;

public class C2Integration {
    private final HttpClient httpClient;
    private final String capApiUrl;
    private final String authToken;
    
    public C2Integration(String apiUrl, String token) {
        this.httpClient = HttpClient.newHttpClient();
        this.capApiUrl = apiUrl;
        this.authToken = token;
    }
    
    public List<PlatformBeacon> queryNearbyPlatforms(
        String geohash,
        boolean operationalOnly
    ) throws Exception {
        String url = String.format(
            "%s/api/v1/beacons?geohash=%s&operational=%b",
            capApiUrl, geohash, operationalOnly
        );
        
        HttpRequest request = HttpRequest.newBuilder()
            .uri(URI.create(url))
            .header("Authorization", "Bearer " + authToken)
            .GET()
            .build();
        
        HttpResponse<String> response = 
            httpClient.send(request, HttpResponse.BodyHandlers.ofString());
        
        if (response.statusCode() != 200) {
            throw new RuntimeException("Query failed: " + response.statusCode());
        }
        
        // Parse JSON response
        ObjectMapper mapper = new ObjectMapper();
        return mapper.readValue(
            response.body(),
            new TypeReference<List<PlatformBeacon>>() {}
        );
    }
    
    public void displayPlatformsOnMap(String geohash) {
        try {
            List<PlatformBeacon> platforms = 
                queryNearbyPlatforms(geohash, true);
            
            for (PlatformBeacon beacon : platforms) {
                System.out.printf(
                    "Platform %s at (%.4f, %.4f) - Capabilities: %s\n",
                    beacon.getPlatformId(),
                    beacon.getPosition().getLatitude(),
                    beacon.getPosition().getLongitude(),
                    beacon.getCapabilitiesList()
                );
                
                // Update C2 display...
                c2Display.addPlatformMarker(beacon);
            }
        } catch (Exception e) {
            System.err.println("Failed to query CAP: " + e.getMessage());
        }
    }
}
```

### Example 3: Python ML Pipeline Using CAP Data

```python
# Python script for AI/ML pipeline consuming CAP sensor data

import grpc
import cap_schema_pb2 as cap
import cap_schema_pb2_grpc as cap_grpc
import pandas as pd
import torch

class CapDataLoader:
    """Load CAP sensor data for ML training"""
    
    def __init__(self, cap_endpoint: str):
        channel = grpc.insecure_channel(cap_endpoint)
        self.client = cap_grpc.PlatformServiceStub(channel)
    
    def stream_sensor_detections(self, time_window_sec: int):
        """Stream sensor detections for ML feature extraction"""
        
        request = cap.StreamRequest(
            message_type=cap.MESSAGE_TYPE_SENSOR_DETECTION,
            filter=cap.MessageFilter(
                time_range=cap.TimeRange(
                    start=int(time.time()) - time_window_sec,
                    end=int(time.time()),
                )
            )
        )
        
        for detection in self.client.StreamSensorDetections(request):
            yield {
                'timestamp': detection.detected_at.seconds,
                'platform_id': detection.platform_id,
                'object_type': detection.object_type,
                'confidence': detection.confidence,
                'position': (
                    detection.position.latitude,
                    detection.position.longitude,
                    detection.position.altitude,
                ),
                'features': detection.features,
            }
    
    def create_training_dataset(self, time_window_sec: int) -> pd.DataFrame:
        """Create pandas DataFrame for ML training"""
        
        data = list(self.stream_sensor_detections(time_window_sec))
        return pd.DataFrame(data)

# Usage in ML pipeline
if __name__ == '__main__':
    loader = CapDataLoader('localhost:50051')
    
    # Load data for training
    df = loader.create_training_dataset(time_window_sec=3600)
    print(f"Loaded {len(df)} detections for training")
    
    # Train model...
    X = df[['confidence', 'features']].values
    y = df['object_type'].values
    
    model = train_detection_model(X, y)
    
    # Stream live data for inference
    for detection in loader.stream_sensor_detections(time_window_sec=60):
        prediction = model.predict(detection['features'])
        print(f"Predicted: {prediction}")
```

## Implementation Roadmap

### Why This Blocks ADR-011 (Automerge + Iroh Integration)

This ADR **blocks ADR-011** because the schema and transport abstractions must be in place before implementing the Automerge + Iroh sync engine. Here's the dependency chain:

```
┌─────────────────────────────────────────────────────────────────┐
│                  Dependency Hierarchy                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ADR-012: Schema Definition & Protocol Extensibility            │
│  ↓ (defines WHAT messages look like)                            │
│  │                                                               │
│  ├─→ peat-schema (protobuf definitions)                          │
│  ├─→ peat-transport (HTTP/gRPC/ROS2 adapters)                    │
│  └─→ peat-persistence (storage interfaces)                       │
│                                                                  │
│                         ↓ used by                                │
│                                                                  │
│  ADR-011: Automerge + Iroh Integration                          │
│  ↓ (implements HOW to sync those messages)                      │
│  │                                                               │
│  ├─→ Automerge CRDT layer                                       │
│  ├─→ Iroh networking layer (QUIC transport)                     │
│  └─→ Implements peat-persistence traits                          │
│                                                                  │
│                         ↓ used by                                │
│                                                                  │
│  ADR-005: Data Sync Abstraction Layer                           │
│  (provides backend switching: Ditto vs Automerge+Iroh)          │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Why ADR-012 is Foundational:**

1. **Schema Contract**: Automerge + Iroh needs to know what to sync
   - Protobuf schemas → Automerge document structure
   - Message types drive CRDT operations (LWW, OR-Set, etc.)
   - Without schema, no way to validate sync correctness

2. **Transport Independence**: Iroh provides QUIC networking, but we need more
   - Iroh is ONE transport option (peer-to-peer mesh)
   - Must also support HTTP/gRPC for C2 integration
   - ROS2 DDS for robotics integration
   - Schema layer enables multiple transports over same sync data

3. **Integration Points**: External systems need schema before sync
   - ROS2 nodes need message definitions to publish/subscribe
   - C2 REST clients need typed API responses
   - Python ML pipelines need structured data formats
   - Can't integrate if only sync protocol exists

4. **Code Generation**: Automerge implementation benefits from generated code
   - Type-safe Rust bindings from protobuf
   - Automatic serialization/deserialization
   - Compile-time validation prevents runtime errors

**Impact on Development**:

- **Current State**: Developing ADR-011 without ADR-012 means:
  - Ad-hoc message formats
  - Manual serialization code
  - No external integration story
  - Difficult to test integration scenarios

- **With ADR-012 First**:
  - Clear schema definitions guide Automerge document design
  - Generated code reduces boilerplate
  - Can test ROS2 integration independently
  - External systems can develop against stable schema

**Recommendation**: Implement Phase 0 of ADR-012 (Schema Definition) before starting ADR-011 implementation. This 2-week investment will save weeks of refactoring later.

### Phase 0: Schema Definition (Weeks 1-2) - **HIGHEST PRIORITY**

**Goal**: Create `peat-schema` crate with core message definitions

**Tasks**:
1. Define protobuf schemas for core messages:
   - `core.proto` - Position, Timestamp, UUID, basic types
   - `platform.proto` - PlatformBeacon, CapabilityAdvertisement
   - `cell.proto` - CellState, CellMembership
   - `ontology.proto` - Capability definitions, aggregation rules

2. Set up code generation:
   - Configure `build.rs` for Rust bindings
   - Add scripts for Python, JavaScript, Java generation
   - Validate generated code compiles

3. Create validation layer:
   - Implement `CapMessage` trait
   - Add semantic validation (e.g., valid geohash, lat/lon ranges)
   - Unit tests for validation

4. Documentation:
   - README with usage examples
   - Schema design rationale
   - Migration guide from current JSON schemas

**Success Criteria**:
- [ ] `peat-schema` crate compiles and passes all tests
- [ ] Can generate bindings for Rust, Python, JavaScript
- [ ] Validation catches common errors
- [ ] Documentation complete

### Phase 1: Transport Abstraction (Weeks 3-4)

**Goal**: Create `peat-transport` crate with HTTP/WebSocket adapter

**Tasks**:
1. Define `MessageTransport` trait
2. Implement `HttpWebSocketTransport` adapter
3. Add message routing and pub-sub logic
4. Integration tests with `peat-schema` messages

**Success Criteria**:
- [ ] Can send/receive CAP messages over HTTP/WebSocket
- [ ] Message serialization/deserialization works
- [ ] Integration tests pass

### Phase 2: Persistence Abstraction (Weeks 5-6)

**Goal**: Create `peat-persistence` crate with external API

**Tasks**:
1. Define `DataStore` trait
2. Implement SQLite backend for testing
3. Create external REST API
4. Add authentication/authorization hooks

**Success Criteria**:
- [ ] Can store/query CAP messages in SQLite
- [ ] External API accessible via HTTP
- [ ] Auth middleware works

### Phase 3: Protocol Adapter Implementations (Weeks 7-10)

**Goal**: Implement gRPC and ROS2 adapters

**Tasks**:
1. gRPC transport:
   - Define service interfaces in `service.proto`
   - Implement gRPC client/server
   - Performance benchmarks vs HTTP

2. ROS2 transport:
   - Generate ROS2 IDL from protobuf
   - Implement DDS publisher/subscriber
   - Integration tests with ROS2 nodes

**Success Criteria**:
- [ ] gRPC transport works end-to-end
- [ ] ROS2 transport works with real ROS2 nodes
- [ ] Performance acceptable for tactical use

### Phase 4: CAP Core Refactoring (Weeks 11-14)

**Goal**: Refactor `peat-protocol` crate to use new abstractions

**Tasks**:
1. Replace inline schemas with `peat-schema`
2. Replace direct Ditto/Automerge calls with `peat-persistence` traits
3. Add transport selection logic
4. Update all tests

**Success Criteria**:
- [ ] All existing E2E tests pass
- [ ] No direct dependency on specific transport
- [ ] Can switch transports via configuration

### Phase 5: Integration Validation (Weeks 15-16)

**Goal**: Validate integrations with ROS2 and legacy systems

**Tasks**:
1. ROS2 example: Robot publishing position to CAP
2. Python example: ML pipeline consuming CAP data
3. Java example: Legacy C2 querying CAP
4. Performance testing under load

**Success Criteria**:
- [ ] All integration examples work
- [ ] Performance meets requirements (latency, throughput)
- [ ] Documentation complete

## Consequences

### Positive

1. **Schema Clarity**: Message schemas are first-class artifacts, not buried in code
2. **Type Safety**: Code generation prevents schema drift across languages
3. **Extensibility**: New transports can be added without modifying core protocol
4. **Integration**: External systems can adopt CAP messages without Peat protocol
5. **Tooling**: Standard schema enables validation, visualization, debugging tools
6. **Multi-Language**: Python, JavaScript, Java, C++ can all use CAP messages natively
7. **Versioning**: Protobuf supports schema evolution with backward compatibility
8. **Performance**: Binary protobuf encoding is compact and fast
9. **Documentation**: Schemas serve as API documentation
10. **Testing**: Can test schemas independently of protocol implementation

### Negative

1. **Complexity**: More abstraction layers to understand
2. **Learning Curve**: Team must learn protobuf, gRPC, ROS2 DDS
3. **Code Generation**: Build process becomes more complex
4. **Maintenance**: Must keep schemas in sync across transports
5. **Migration Effort**: Existing code must be refactored to use new abstractions
6. **Testing Burden**: More integration tests needed
7. **Performance Overhead**: Serialization/deserialization adds latency
8. **Debuggability**: More layers make tracing issues harder

### Risks and Mitigations

**Risk 1**: Protobuf schema evolution breaks deployed systems
- **Mitigation**: Strict versioning policy, never remove fields, only deprecate
- **Mitigation**: Version field in every message for compatibility checks

**Risk 2**: Transport adapter performance is inadequate
- **Mitigation**: Benchmark early and often
- **Mitigation**: Optimize hot paths, consider zero-copy serialization

**Risk 3**: External systems struggle with integration
- **Mitigation**: Comprehensive documentation and examples
- **Mitigation**: Reference implementations in multiple languages

**Risk 4**: Schema becomes too complex
- **Mitigation**: Keep schemas focused and modular
- **Mitigation**: Regular review and simplification

## Alternatives Considered

### Alternative 1: Stay with Current Approach (Embedded Schemas)

**Pros**:
- No migration effort
- Simpler architecture

**Cons**:
- Hard to integrate with external systems
- No type safety across languages
- Schema evolution is manual and error-prone

**Verdict**: Rejected - Integration and extensibility are critical requirements

### Alternative 2: Use JSON Schema Instead of Protobuf

**Pros**:
- More human-readable
- Better web/JavaScript support
- Simpler tooling

**Cons**:
- Larger wire format (important for tactical bandwidth)
- Less efficient serialization
- No native service definition (like gRPC)
- Weaker code generation support

**Verdict**: Rejected - Binary efficiency is critical for tactical environments

### Alternative 3: Use Apache Avro

**Pros**:
- Excellent schema evolution
- Compact binary format
- JSON compatibility

**Cons**:
- Less ecosystem support than Protobuf
- No service definition standard
- Smaller community

**Verdict**: Considered - Could revisit if Protobuf proves inadequate

### Alternative 4: Multiple Schema Formats

**Pros**:
- Use best format for each transport (JSON for HTTP, Protobuf for gRPC, IDL for ROS2)

**Cons**:
- Massive maintenance burden
- Schema drift inevitable
- No single source of truth

**Verdict**: Rejected - Complexity outweighs benefits

## Success Metrics

1. **Schema Adoption**:
   - [ ] All CAP messages defined in `peat-schema`
   - [ ] Code generation works for 3+ languages
   - [ ] Zero manual serialization code in `peat-protocol`

2. **Transport Extensibility**:
   - [ ] 3+ transport implementations (HTTP/WS, gRPC, ROS2)
   - [ ] Adding new transport takes <1 week
   - [ ] Can run multiple transports simultaneously

3. **Integration Success**:
   - [ ] ROS2 integration works end-to-end
   - [ ] External systems can query CAP data via REST
   - [ ] Python ML pipeline can consume CAP sensor data

4. **Performance**:
   - [ ] Message serialization <1ms
   - [ ] Transport overhead <10ms for local communication
   - [ ] Supports 1000+ messages/sec per node

5. **Developer Experience**:
   - [ ] Schema changes propagate automatically via code generation
   - [ ] Validation catches 90%+ of schema errors at compile-time
   - [ ] Documentation rated "good" or better by external developers

## Appendix: Peat Protocol Schemas (v1)

> **Added 2025-11-25**: These schemas define the core Peat Protocol primitives for software distribution, capability advertisement, and event routing. They supersede the earlier example schemas above and represent the canonical protocol definitions.
>
> **Design Principle**: Peat Protocol defines the envelope, applications define the contents. All payloads use `google.protobuf.Any` or `google.protobuf.Struct` to remain application-agnostic.

### A.1 Blob Reference (ADR-025)

```protobuf
syntax = "proto3";
package peat.blob.v1;

// Content-addressed blob reference
message BlobReference {
  string hash = 1;              // Content hash (hex)
  string hash_algorithm = 2;    // "sha256", "blake3"
  uint64 size_bytes = 3;

  // Application-defined metadata (opaque to Peat)
  map<string, string> metadata = 10;
}
```

### A.2 Capability Advertisement

```protobuf
syntax = "proto3";
package peat.capability.v1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";

message CapabilityAdvertisement {
  string node_id = 1;
  string formation_id = 2;
  google.protobuf.Timestamp advertised_at = 3;
  repeated Capability capabilities = 4;
  ResourceStatus resources = 5;
}

message Capability {
  string capability_type = 1;   // "inference", "sensor", "comms", "software"
  string capability_id = 2;     // "target_recognition", "gps", "mesh_radio"
  string version = 3;
  CapabilityState state = 4;
  google.protobuf.Struct attributes = 10;  // Application-defined
}

enum CapabilityState {
  CAPABILITY_STATE_UNSPECIFIED = 0;
  CAPABILITY_STATE_AVAILABLE = 1;
  CAPABILITY_STATE_DEGRADED = 2;
  CAPABILITY_STATE_OFFLINE = 3;
  CAPABILITY_STATE_STARTING = 4;
}

message ResourceStatus {
  double cpu_available = 1;
  uint64 memory_available_bytes = 2;
  uint64 gpu_memory_available_bytes = 3;
  uint64 storage_available_bytes = 4;
  map<string, double> custom = 10;
}

message FormationCapabilitySummary {
  string formation_id = 1;
  string formation_type = 2;    // "squad", "platoon", "company"
  google.protobuf.Timestamp summarized_at = 3;
  uint32 total_members = 4;
  uint32 members_available = 5;
  uint32 members_degraded = 6;
  uint32 members_offline = 7;
  repeated AggregatedCapability capabilities = 10;
}

message AggregatedCapability {
  string capability_type = 1;
  string capability_id = 2;
  uint32 count_available = 3;
  uint32 count_degraded = 4;
  uint32 count_total = 5;
  google.protobuf.Struct aggregated_attributes = 10;
}
```

### A.3 Peat Event (Products, Anomalies, Telemetry)

```protobuf
syntax = "proto3";
package peat.event.v1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/any.proto";

message PeatEvent {
  string event_id = 1;
  google.protobuf.Timestamp timestamp = 2;
  string source_node_id = 3;
  string source_formation_id = 4;
  optional string source_instance_id = 5;
  EventClass event_class = 6;
  string event_type = 7;        // Application-defined type identifier
  AggregationPolicy routing = 8;
  google.protobuf.Any payload = 10;  // Application-defined payload
}

enum EventClass {
  EVENT_CLASS_UNSPECIFIED = 0;
  EVENT_CLASS_PRODUCT = 1;      // Outputs from software (detections, decisions)
  EVENT_CLASS_ANOMALY = 2;      // Anomalies requiring attention
  EVENT_CLASS_TELEMETRY = 3;    // Metrics, health, diagnostics
  EVENT_CLASS_COMMAND = 4;      // Downward directives
}

message AggregationPolicy {
  PropagationMode propagation = 1;
  EventPriority priority = 2;
  uint32 ttl_seconds = 3;
  uint32 aggregation_window_ms = 4;
}

enum PropagationMode {
  PROPAGATION_FULL = 0;         // Forward complete event upward
  PROPAGATION_SUMMARY = 1;      // Aggregate events, forward summary
  PROPAGATION_QUERY = 2;        // Store locally, respond to queries
  PROPAGATION_LOCAL = 3;        // No propagation, local only
}

enum EventPriority {
  PRIORITY_CRITICAL = 0;        // Immediate, preempts other traffic
  PRIORITY_HIGH = 1;
  PRIORITY_NORMAL = 2;
  PRIORITY_LOW = 3;
}

message EventSummary {
  string formation_id = 1;
  google.protobuf.Timestamp window_start = 2;
  google.protobuf.Timestamp window_end = 3;
  EventClass event_class = 4;
  string event_type = 5;
  uint32 event_count = 6;
  repeated string source_node_ids = 7;
  google.protobuf.Any summary_payload = 10;
}
```

### A.4 Deployment Directive (Commands Through Hierarchy)

```protobuf
syntax = "proto3";
package peat.deployment.v1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "peat/blob/v1/blob.proto";

message DeploymentDirective {
  string directive_id = 1;
  google.protobuf.Timestamp issued_at = 2;
  string issuer_node_id = 3;
  string issuer_formation_id = 4;
  DeploymentScope scope = 5;
  peat.blob.v1.BlobReference artifact = 6;
  string artifact_type = 7;     // "onnx_model", "container", "config_package"
  google.protobuf.Struct config = 10;  // Application-defined
  DeploymentOptions options = 11;
}

message DeploymentScope {
  oneof target {
    string formation_id = 1;
    NodeList specific_nodes = 2;
    CapabilityFilter capability_filter = 3;
    bool broadcast = 4;
  }
}

message NodeList {
  repeated string node_ids = 1;
}

message CapabilityFilter {
  optional double min_cpu = 1;
  optional uint64 min_memory_bytes = 2;
  optional uint64 min_gpu_memory_bytes = 3;
  optional uint64 min_storage_bytes = 4;
  repeated string required_capability_ids = 10;
  map<string, string> custom_filters = 20;
}

message DeploymentOptions {
  peat.event.v1.EventPriority priority = 1;
  uint32 timeout_seconds = 2;
  bool replace_existing = 3;
  optional uint32 rollback_threshold_percent = 4;
}

message DeploymentStatus {
  string directive_id = 1;
  string node_id = 2;
  google.protobuf.Timestamp reported_at = 3;
  DeploymentState state = 4;
  uint32 progress_percent = 5;
  optional string error_message = 6;
  optional string instance_id = 7;
}

enum DeploymentState {
  DEPLOYMENT_STATE_UNSPECIFIED = 0;
  DEPLOYMENT_STATE_PENDING = 1;
  DEPLOYMENT_STATE_DOWNLOADING = 2;
  DEPLOYMENT_STATE_ACTIVATING = 3;
  DEPLOYMENT_STATE_ACTIVE = 4;
  DEPLOYMENT_STATE_FAILED = 5;
  DEPLOYMENT_STATE_ROLLED_BACK = 6;
}
```

### A.5 Protocol Behavior Summary

| Schema | Direction | Aggregation | Peat Responsibility |
|--------|-----------|-------------|---------------------|
| BlobReference | N/A | N/A | Transfer bytes, verify hash |
| CapabilityAdvertisement | ↑ Upward | Summarize at echelons | Route through hierarchy |
| PeatEvent | ↑ Upward | Per AggregationPolicy | Apply routing policy |
| DeploymentDirective | ↓ Downward | Scope filtering | Route to matching nodes |
| DeploymentStatus | ↑ Upward | Aggregate per directive | Collect status reports |

### A.6 Extension Points

Applications extend Peat by:
1. **Defining capability attributes** (`Capability.attributes`)
2. **Defining event payloads** (`PeatEvent.payload`)
3. **Defining artifact types** (`DeploymentDirective.artifact_type`)
4. **Defining deployment config** (`DeploymentDirective.config`)
5. **Defining custom filters** (`CapabilityFilter.custom_filters`)

Peat routes, aggregates, and enforces policies without understanding application semantics.

---

## References

1. [Protocol Buffers Documentation](https://protobuf.dev/)
2. [gRPC Documentation](https://grpc.io/docs/)
3. [ROS2 Documentation](https://docs.ros.org/en/humble/)
4. ADR-005: Data Synchronization Abstraction Layer
5. ADR-007: Automerge-Based Sync Engine
6. ADR-010: Transport Layer UDP vs TCP
7. [Link 16 Standard](https://en.wikipedia.org/wiki/Link_16) - Military data link inspiration
8. [DDS Standard](https://www.omg.org/spec/DDS/) - ROS2 middleware
9. [JSON Schema](https://json-schema.org/) - Alternative considered

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-06 | Proposed separation of schema, transport, persistence | Extensibility and integration requirements |
| 2025-11-06 | Selected Protobuf as schema format | Binary efficiency, code generation, ecosystem |
| 2025-11-06 | Proposed trait-based transport adapters | Pluggable architecture for multiple protocols |
| TBD | Approved/Rejected | After team review and prototyping |

## Open Questions

1. **Should we support runtime schema negotiation?** Or only compile-time validation?
2. **How do we handle schema versions in mixed deployments?** (e.g., old nodes + new nodes)
3. **Should transport adapters be dynamically loaded?** Or compile-time only?
4. **Do we need a schema registry service?** For distributed schema discovery?
5. **How do we handle large binary payloads?** (e.g., AI models, video) - Reference vs inline?
6. **Should we standardize on gRPC streaming?** Or support multiple streaming protocols?

## Next Steps

### Critical Path (Blocks ADR-011)

**These steps must be completed before starting ADR-011 (Automerge + Iroh) implementation:**

1. **Review this ADR with team** (1 week)
   - Align on schema-first approach
   - Validate Protobuf selection
   - Confirm ROS2 integration requirements

2. **Prototype `peat-schema` with core messages** (1 week)
   - Implement Phase 0 (Schema Definition)
   - Generate Rust, Python, JavaScript bindings
   - Validate with simple examples

3. **Benchmark protobuf vs current JSON serialization** (3 days)
   - Measure serialization performance
   - Compare wire format sizes
   - Validate bandwidth savings

4. **Create proof-of-concept gRPC transport** (3 days)
   - Implement basic `MessageTransport` trait
   - Test message round-trip
   - Measure latency vs direct TCP

5. **Validate ROS2 integration feasibility** (1 week)
   - Create ROS2 bridge prototype
   - Test protobuf → ROS2 IDL conversion
   - Demo robot publishing to CAP

### After ADR-012 Phase 0 Complete

6. **Begin ADR-011 (Automerge + Iroh) implementation**
   - Use peat-schema messages as Automerge document structure
   - Implement peat-persistence traits
   - Integrate with Iroh networking

7. **Update ADR-005 implementation plan** 
   - Reframe as backend abstraction within peat-persistence
   - Not top-level protocol interface

---

**Critical Insight**: Completing Phase 0 (Schema Definition) takes only 2 weeks but provides the foundation for both ADR-011 (sync engine) and all future integration work. This is a blocking dependency that must be resolved before proceeding with sync engine implementation.

---

**Author's Note**: This ADR represents a significant architectural shift but is essential for CAP's long-term viability. **Critically, it blocks ADR-011 (Automerge + Iroh Integration)** because the schema and transport abstractions must exist before implementing the sync engine. The separation of schema (WHAT), protocol (HOW), and transport (WHERE) concerns will enable integration with existing systems (ROS2, legacy C2), support multi-language clients, and provide the extensibility needed for diverse operational environments. 

**Recommendation**: Pause ADR-011 development and complete Phase 0 (Schema Definition) first. This 2-week investment establishes the foundation that makes everything else—sync engines, transport adapters, external integrations—significantly easier to implement correctly. The investment in proper abstraction now will pay dividends in reduced integration friction, enhanced interoperability, and a cleaner architecture for years to come.
