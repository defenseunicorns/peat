# GitHub Issues for Major Refactoring

**Related**: ADR-011 (Automerge + Iroh), ADR-012 (Schema & Protocol Extensibility)
**Playbook**: See `REFACTORING-PLAYBOOK.md`

This document contains issue templates for the 16-week refactoring project.

---

## Epic Issues

### Epic #44: ADR-012 - Schema Definition and Protocol Extensibility

**Title**: [EPIC] Schema Definition and Protocol Extensibility Architecture

**Labels**: `epic`, `adr-012`, `schema`, `protocol`, `enhancement`

**Description**:

Implement schema-first architecture to separate message definitions from protocol implementation, enabling multi-transport support and external system integration.

**Related ADRs**: ADR-012

**Goals**:
- Create `cap-schema` crate with Protobuf message definitions
- Create `cap-transport` crate with HTTP/gRPC/ROS2 adapters
- Create `cap-persistence` crate with storage abstraction
- Refactor `cap-protocol` to use new abstractions

**Timeline**: 16 weeks

**Dependencies**:
- Blocks ADR-011 (Automerge + Iroh)
- Must complete before Ditto replacement

**Sub-Issues**:
- #45: Phase 1 - cap-schema Foundation
- #46: Phase 2 - cap-transport Abstraction
- #47: Phase 3 - cap-persistence Layer
- #48: Phase 4 - gRPC Transport Adapter
- #49: Phase 5 - ROS2 Transport Adapter
- #50: Phase 6 - cap-core Refactoring
- #51: Phase 7 - Integration Validation

**Success Criteria**:
- [ ] All CAP messages defined in Protobuf
- [ ] 4 working transport adapters (HTTP, gRPC, ROS2, WebSocket)
- [ ] External REST API functional
- [ ] All E2E tests passing
- [ ] Integration examples for Python, Java, ROS2
- [ ] Performance at parity with current system

---

### Epic #52: ADR-011 - Automerge + Iroh Integration

**Title**: [EPIC] Replace Ditto with Automerge + Iroh for CRDT Sync

**Labels**: `epic`, `adr-011`, `automerge`, `iroh`, `networking`

**Description**:

Replace proprietary Ditto SDK with open-source Automerge (CRDT) + Iroh (QUIC networking) to eliminate licensing costs and enable multi-path tactical networking.

**Related ADRs**: ADR-011

**Blocked By**: Epic #44 (Schema & Protocol Extensibility must complete first)

**Goals**:
- Implement `DataStore` trait with Automerge
- Integrate Iroh for multi-path QUIC networking
- Enable connection migration and failover
- Deprecate Ditto dependency

**Timeline**: 6 weeks (starts after ADR-012 completion)

**Sub-Issues**:
- #53: Automerge DataStore Implementation
- #54: Iroh QUIC Networking Integration
- #55: Multi-Path Network Support
- #56: Ditto Deprecation and Migration Tooling

**Success Criteria**:
- [ ] Automerge + Iroh backend functional
- [ ] Multi-path networking working (Starlink + MANET + 5G)
- [ ] Performance equal or better than Ditto
- [ ] Migration path from Ditto documented
- [ ] Zero licensing costs

---

## Phase 1: cap-schema Foundation (Weeks 1-2)

### Issue #45: Implement cap-schema Crate with Protobuf Definitions

**Title**: Phase 1: Implement cap-schema with Protobuf Message Definitions

**Labels**: `phase-1`, `schema`, `protobuf`, `codegen`

**Related Epic**: #44

**Description**:

Create the foundational `cap-schema` crate containing all CAP Protocol message definitions in Protobuf format, with code generation for Rust, Python, and C++.

**Tasks**:

**Week 1: Core Message Schemas**
- [ ] Create `cap-schema` crate directory structure
- [ ] Set up Protobuf build configuration (`prost`, `tonic-build`)
- [ ] Define core message schemas:
  - [ ] `protos/node.proto` - NodeConfig, NodeStatus
  - [ ] `protos/capability.proto` - Capability, CapabilityType
  - [ ] `protos/cell.proto` - CellState, CellConfig, CellFormationStatus
  - [ ] `protos/composition.proto` - Composition rules, constraints
  - [ ] `protos/common.proto` - Common types (Metadata, Timestamp, etc.)
- [ ] Generate Rust bindings
- [ ] Unit tests for serialization/deserialization
- [ ] Verify size vs current JSON (target: < 5% increase)

**Week 2: Ontology and Code Generation**
- [ ] Define validation rules in Protobuf options
- [ ] Create code generation scripts for Python (using `protoc` + `grpcio-tools`)
- [ ] Create code generation scripts for C++ (using `protoc`)
- [ ] Add schema versioning (semantic versions in package)
- [ ] Generate API documentation from Protobuf comments
- [ ] Create examples showing schema usage
- [ ] Integration tests with all generated code

**Acceptance Criteria**:
- [ ] All CAP models (Node, Cell, Capability, etc.) have Protobuf definitions
- [ ] Rust code compiles without warnings
- [ ] Python bindings importable and functional
- [ ] C++ headers compile and link
- [ ] Schema documentation generated and readable
- [ ] < 5% size overhead vs current JSON serialization
- [ ] < 100ms validation overhead for typical messages

**Dependencies**: None (foundational work)

**Estimated Effort**: 2 weeks (1 developer)

---

## Phase 2: cap-transport Abstraction (Weeks 3-4)

### Issue #46: Implement cap-transport with HTTP/WebSocket Adapter

**Title**: Phase 2: Implement cap-transport with HTTP/WebSocket Adapter

**Labels**: `phase-2`, `transport`, `http`, `websocket`

**Related Epic**: #44

**Depends On**: #45 (cap-schema)

**Description**:

Create transport abstraction layer with initial HTTP/WebSocket implementation, enabling multiple transport protocols to coexist.

**Tasks**:

**Week 3: Transport Trait Definition**
- [ ] Create `cap-transport` crate directory
- [ ] Define `MessageTransport` trait with methods:
  - [ ] `async fn send(message: CapMessage) -> Result<()>`
  - [ ] `async fn receive() -> Result<CapMessage>`
  - [ ] `async fn subscribe(topic: &str) -> Result<Subscription>`
  - [ ] `async fn publish(topic: &str, message: CapMessage) -> Result<()>`
- [ ] Define `TransportConfig` struct
- [ ] Define error types and retry logic
- [ ] Create mock transport for testing
- [ ] Unit tests for trait contract

**Week 4: HTTP/WebSocket Implementation**
- [ ] Implement `HttpWebSocketTransport` using:
  - [ ] Axum for HTTP server (REST endpoints)
  - [ ] tokio-tungstenite for WebSocket (pub-sub)
- [ ] HTTP endpoints:
  - [ ] `POST /messages` - Send message
  - [ ] `GET /messages/{id}` - Receive message
  - [ ] `GET /subscribe/{topic}` - WebSocket subscription
  - [ ] `POST /publish/{topic}` - Publish to topic
- [ ] Message routing logic (topic-based pub-sub)
- [ ] Integration tests with cap-schema messages
- [ ] Performance benchmarks (latency, throughput)
- [ ] API documentation (OpenAPI spec)
- [ ] Usage examples

**Acceptance Criteria**:
- [ ] `MessageTransport` trait fully documented
- [ ] HTTP/WebSocket adapter functional
- [ ] Can send cap-schema messages end-to-end
- [ ] WebSocket pub-sub works for subscriptions
- [ ] < 10ms transport overhead (p99)
- [ ] 1000+ msg/sec throughput
- [ ] OpenAPI spec generated
- [ ] Integration tests passing

**Dependencies**: #45 (cap-schema)

**Estimated Effort**: 2 weeks (1 developer)

---

## Phase 3: cap-persistence Layer (Weeks 5-6)

### Issue #47: Implement cap-persistence with External REST API

**Title**: Phase 3: Implement cap-persistence with External REST API

**Labels**: `phase-3`, `persistence`, `rest-api`, `sqlite`

**Related Epic**: #44

**Depends On**: #45 (cap-schema), #46 (cap-transport)

**Description**:

Create persistence abstraction layer with external REST API, enabling different storage backends and external system access.

**Tasks**:

**Week 5: Persistence Traits and SQLite Backend**
- [ ] Create `cap-persistence` crate directory
- [ ] Define `DataStore` trait with CRUD operations:
  - [ ] `async fn put(collection, doc) -> Result<String>`
  - [ ] `async fn get(collection, id) -> Result<Option<Document>>`
  - [ ] `async fn query(collection, query) -> Result<Vec<Document>>`
  - [ ] `async fn subscribe(collection, query) -> Result<Subscription>`
  - [ ] `async fn delete(collection, id) -> Result<()>`
- [ ] Define `SyncEngine` trait (from existing ADR-005)
- [ ] Define `Query` type with filtering/sorting
- [ ] Implement `SqliteDataStore` for testing:
  - [ ] Schema creation
  - [ ] CRUD operations
  - [ ] Query execution (WHERE, ORDER BY, LIMIT)
  - [ ] Subscription via polling (temporary)
- [ ] Unit tests for all operations

**Week 6: External REST API**
- [ ] Build REST API service on top of `DataStore`:
  - [ ] `GET /collections/{name}/documents/{id}`
  - [ ] `POST /collections/{name}/documents`
  - [ ] `PUT /collections/{name}/documents/{id}`
  - [ ] `DELETE /collections/{name}/documents/{id}`
  - [ ] `GET /collections/{name}/query?filter=...&sort=...&limit=...`
  - [ ] `WS /collections/{name}/subscribe?query=...`
- [ ] Authentication middleware (JWT tokens)
- [ ] Authorization rules (read/write permissions)
- [ ] Rate limiting
- [ ] API documentation (OpenAPI spec)
- [ ] Integration tests with multiple clients
- [ ] Performance benchmarks

**Acceptance Criteria**:
- [ ] `DataStore` trait allows full CRUD + query
- [ ] SQLite backend functional for testing
- [ ] REST API accessible externally
- [ ] Authentication and authorization work
- [ ] OpenAPI spec published
- [ ] < 50ms query latency for simple queries (p99)
- [ ] < 200ms for complex aggregations
- [ ] Integration tests passing

**Dependencies**: #45 (cap-schema), #46 (cap-transport)

**Estimated Effort**: 2 weeks (1 developer)

---

## Phase 4: Protocol Adapters - gRPC (Weeks 7-8)

### Issue #48: Implement gRPC Transport Adapter

**Title**: Phase 4: Implement gRPC Transport Adapter

**Labels**: `phase-4`, `transport`, `grpc`, `tonic`

**Related Epic**: #44

**Depends On**: #46 (cap-transport)

**Description**:

Implement production-quality gRPC transport adapter for low-latency, type-safe communication with C2 systems and microservices.

**Tasks**:

**Week 7: gRPC Service Definition**
- [ ] Create `protos/service.proto` with gRPC service:
  ```protobuf
  service CapProtocol {
    rpc GetNode(GetNodeRequest) returns (NodeConfig);
    rpc ListNodes(ListNodesRequest) returns (stream NodeConfig);
    rpc CreateCell(CreateCellRequest) returns (CellState);
    rpc GetCell(GetCellRequest) returns (CellState);
    rpc StreamCellUpdates(StreamRequest) returns (stream CellState);
    rpc PublishCapability(Capability) returns (Ack);
    rpc QueryCells(QueryRequest) returns (QueryResponse);
  }
  ```
- [ ] Generate gRPC client/server with `tonic`
- [ ] Define request/response types
- [ ] Create gRPC error mappings

**Week 8: Implementation and Testing**
- [ ] Implement `GrpcTransport` struct (client-side)
- [ ] Implement `GrpcServer` service handlers
- [ ] Add TLS support (mTLS for authentication)
- [ ] Connection pooling and retry logic
- [ ] Streaming support (bidirectional)
- [ ] Integration tests (client ↔ server)
- [ ] Performance benchmarks vs HTTP
- [ ] Documentation and examples

**Acceptance Criteria**:
- [ ] gRPC service fully functional
- [ ] Client and server implementations complete
- [ ] Streaming works for real-time updates
- [ ] TLS/mTLS authentication working
- [ ] < 5ms latency (p99) for simple RPCs
- [ ] > 10,000 RPC/sec throughput
- [ ] Performance comparison with HTTP documented
- [ ] Integration tests passing

**Dependencies**: #46 (cap-transport)

**Estimated Effort**: 2 weeks (1 developer)

---

## Phase 5: Protocol Adapters - ROS2 (Weeks 9-10)

### Issue #49: Implement ROS2 DDS Transport Adapter

**Title**: Phase 5: Implement ROS2 DDS Transport Adapter

**Labels**: `phase-5`, `transport`, `ros2`, `dds`, `robotics`

**Related Epic**: #44

**Depends On**: #46 (cap-transport)

**Description**:

Implement ROS2 DDS transport adapter to enable seamless integration with robotic systems and autonomous platforms.

**Tasks**:

**Week 9: ROS2 Message Generation**
- [ ] Generate ROS2 IDL from Protobuf schemas using `protobuf_to_ros2_idl` tool
- [ ] Create ROS2 package (`cap_msgs`):
  - [ ] `package.xml` with dependencies
  - [ ] `CMakeLists.txt` for message generation
  - [ ] `msg/` directory with generated .msg files
- [ ] Build and install ROS2 package
- [ ] Verify messages with `ros2 interface show cap_msgs/msg/CellState`
- [ ] Create ROS2 service definitions for RPC-style operations

**Week 10: DDS Publisher/Subscriber Implementation**
- [ ] Implement `Ros2Transport` using `rclrs` (Rust ROS2 client library):
  - [ ] Publisher for outgoing messages
  - [ ] Subscriber for incoming messages
  - [ ] Service client/server for RPC
  - [ ] Action client/server for long-running ops (optional)
- [ ] Topic naming convention (e.g., `/cap/cells/{cell_id}`)
- [ ] QoS profile configuration (reliability, durability)
- [ ] Integration tests with real ROS2 nodes
- [ ] Example: Robot publishing position to CAP cell
- [ ] Documentation for ROS2 users

**Acceptance Criteria**:
- [ ] ROS2 messages generated from Protobuf
- [ ] `cap_msgs` package builds and installs
- [ ] Can publish CAP messages to ROS2 topics
- [ ] Can subscribe to ROS2 topics and receive CAP messages
- [ ] Works with `ros2 topic echo` and `ros2 service call`
- [ ] < 100ms topic bridge latency
- [ ] Integration tests with ros2cli passing
- [ ] Example robot integration working

**Dependencies**: #46 (cap-transport)

**Estimated Effort**: 2 weeks (1 developer with ROS2 experience)

**Note**: This is an optional enhancement. Can be deferred if ROS2 integration is not immediately needed.

---

## Phase 6: cap-core Refactoring (Weeks 11-14)

### Issue #50: Refactor cap-protocol to Use New Abstractions

**Title**: Phase 6: Refactor cap-protocol to Use cap-schema, cap-transport, cap-persistence

**Labels**: `phase-6`, `refactoring`, `cap-core`, `migration`

**Related Epic**: #44

**Depends On**: #45, #46, #47, #48

**Description**:

Major refactoring of `cap-protocol` crate to use new schema, transport, and persistence abstractions, eliminating hard dependencies on Ditto.

**Tasks**:

**Week 11: Schema Migration**
- [ ] Replace inline Rust structs with cap-schema Protobuf types:
  - [ ] `NodeConfig` → `cap_schema::NodeConfig`
  - [ ] `CellState` → `cap_schema::CellState`
  - [ ] `Capability` → `cap_schema::Capability`
- [ ] Update serialization logic (remove manual serde, use Protobuf)
- [ ] Migrate all model unit tests
- [ ] Verify E2E tests still compile

**Week 12: Transport Integration**
- [ ] Remove direct Ditto SDK transport calls
- [ ] Use `cap-transport` abstractions for all network operations
- [ ] Add transport configuration (select HTTP, gRPC, or ROS2)
- [ ] Update peer discovery to use transport-agnostic approach
- [ ] Migrate E2E tests to use transport selection

**Week 13: Persistence Integration**
- [ ] Replace direct Ditto storage calls with `cap-persistence` traits
- [ ] Keep `DittoBackend` as one `DataStore` implementation (for now)
- [ ] Add configuration for storage backend selection
- [ ] Implement `CellStore` and `NodeStore` using `DataStore` trait
- [ ] Migrate all storage layer tests

**Week 14: Testing and Documentation**
- [ ] Run full E2E test suite
- [ ] Performance regression testing (compare to baseline)
- [ ] Update all documentation:
  - [ ] Architecture diagrams
  - [ ] API documentation
  - [ ] Configuration guide
  - [ ] Migration guide for existing deployments
- [ ] Create example configurations for each transport
- [ ] Final code review and polish

**Acceptance Criteria**:
- [ ] `cap-protocol` has no inline message schemas
- [ ] All transports configurable via config file
- [ ] All persistence backends configurable
- [ ] All E2E tests passing
- [ ] No performance regression (< 5% slower acceptable)
- [ ] Migration guide complete
- [ ] Documentation updated

**Dependencies**: #45, #46, #47, #48

**Estimated Effort**: 4 weeks (2 developers)

---

## Phase 7: Integration Validation (Weeks 15-16)

### Issue #51: Integration Validation and External Examples

**Title**: Phase 7: Validate External System Integrations and Create Examples

**Labels**: `phase-7`, `integration`, `examples`, `validation`

**Related Epic**: #44

**Depends On**: #50 (cap-core refactoring)

**Description**:

Validate all external system integrations with working examples and comprehensive performance testing.

**Tasks**:

**Week 15: Integration Examples**
- [ ] **ROS2 Example** - Robot publishing sensor data:
  - [ ] Create ROS2 node that publishes position/velocity
  - [ ] Subscribe to CAP cell formation events
  - [ ] Example: `ros2 run cap_examples robot_publisher`
  - [ ] Documentation with screenshots
- [ ] **Python Example** - ML pipeline consuming CAP data:
  - [ ] Python client library using cap-schema bindings
  - [ ] Query cells by capability type
  - [ ] Subscribe to capability updates
  - [ ] Example Jupyter notebook
- [ ] **Java Example** - Legacy C2 system querying CAP:
  - [ ] Java client using gRPC
  - [ ] Query squad formation status
  - [ ] Display results in simple UI
  - [ ] Maven/Gradle build configuration
- [ ] **JavaScript Example** - Web dashboard:
  - [ ] React app with WebSocket subscription
  - [ ] Real-time cell state visualization
  - [ ] npm package for cap-schema types

**Week 16: Performance Testing and Polish**
- [ ] Load testing:
  - [ ] 100 nodes, 1000 messages/sec
  - [ ] Measure latency across all transports
  - [ ] Memory profiling under load
  - [ ] CPU utilization analysis
- [ ] Stress testing:
  - [ ] Network partition scenarios
  - [ ] High latency (1000ms+) links
  - [ ] Packet loss (10-30%)
- [ ] Document performance results
- [ ] Final documentation review
- [ ] Example deployment guide (Docker, Kubernetes)
- [ ] Security review (authentication, authorization, encryption)

**Acceptance Criteria**:
- [ ] All 4 integration examples functional
- [ ] Examples documented with README and screenshots
- [ ] Load testing results documented
- [ ] Latency < 100ms p99 for critical operations
- [ ] Memory usage stable under load (< 500MB for 100 nodes)
- [ ] Deployment guide complete
- [ ] Security review passed

**Dependencies**: #50 (cap-core refactoring)

**Estimated Effort**: 2 weeks (2 developers)

---

## ADR-011 Implementation Issues

### Issue #53: Implement Automerge DataStore

**Title**: Implement DataStore Trait with Automerge CRDT

**Labels**: `adr-011`, `automerge`, `crdt`, `persistence`

**Related Epic**: #52

**Blocked By**: Epic #44 (ADR-012 must complete first)

**Description**:

Implement the `DataStore` trait using Automerge for CRDT-based document storage and delta sync.

**Tasks**:
- [ ] Map Protobuf schemas to Automerge document structure
- [ ] Implement CRUD operations with Automerge
- [ ] Implement delta sync using Automerge sync protocol
- [ ] Implement queries (filtering, sorting)
- [ ] Add subscription support (observe document changes)
- [ ] Performance benchmarks vs Ditto
- [ ] Unit tests for all operations

**Acceptance Criteria**:
- [ ] All `DataStore` methods implemented
- [ ] Delta sync functional between two instances
- [ ] Query performance within 20% of Ditto baseline
- [ ] Unit tests > 80% coverage

**Dependencies**: Epic #44 complete, #47 (cap-persistence traits)

**Estimated Effort**: 2 weeks

---

### Issue #54: Integrate Iroh for QUIC Networking

**Title**: Integrate Iroh for Multi-Path QUIC Networking

**Labels**: `adr-011`, `iroh`, `quic`, `networking`

**Related Epic**: #52

**Depends On**: #53 (Automerge DataStore)

**Description**:

Integrate Iroh to provide QUIC-based peer-to-peer networking with multi-path support and connection migration.

**Tasks**:
- [ ] Integrate `iroh` crate
- [ ] Implement peer discovery using Iroh DHT
- [ ] Implement document sync over Iroh QUIC connections
- [ ] Add connection migration support
- [ ] Performance benchmarks (latency, throughput)
- [ ] Integration tests

**Acceptance Criteria**:
- [ ] Peers can discover each other via Iroh
- [ ] Documents sync over QUIC connections
- [ ] Connection migration works (network switch < 1 second)
- [ ] Performance meets ADR-011 targets

**Dependencies**: #53 (Automerge DataStore)

**Estimated Effort**: 2 weeks

---

### Issue #55: Implement Multi-Path Network Support

**Title**: Implement Multi-Path Networking (Starlink + MANET + 5G)

**Labels**: `adr-011`, `multi-path`, `networking`, `tactical`

**Related Epic**: #52

**Depends On**: #54 (Iroh integration)

**Description**:

Implement multi-path networking to use multiple network interfaces simultaneously for different data priorities.

**Tasks**:
- [ ] Detect available network interfaces
- [ ] Implement path selection logic (priority-based routing)
- [ ] Stream multiplexing (commands vs telemetry)
- [ ] Adaptive routing based on latency/bandwidth
- [ ] Failover and redundancy
- [ ] ContainerLab testing scenarios
- [ ] Performance validation

**Acceptance Criteria**:
- [ ] Can use 3+ interfaces simultaneously
- [ ] Critical messages route via low-latency path
- [ ] Bulk data routes via high-bandwidth path
- [ ] Failover < 1 second on network loss
- [ ] Performance improvement validated

**Dependencies**: #54 (Iroh integration)

**Estimated Effort**: 2 weeks

---

### Issue #56: Ditto Deprecation and Migration Tooling

**Title**: Create Ditto Deprecation Plan and Migration Tooling

**Labels**: `adr-011`, `migration`, `deprecation`, `ditto`

**Related Epic**: #52

**Depends On**: #53, #54, #55

**Description**:

Create migration path from Ditto to Automerge + Iroh with tooling and documentation.

**Tasks**:
- [ ] Feature flag for backend selection (Ditto vs Automerge+Iroh)
- [ ] Data export from Ditto to Automerge format
- [ ] Import tool for migrating existing data
- [ ] Performance comparison tests
- [ ] Migration guide documentation
- [ ] Deprecation timeline decision
- [ ] Remove Ditto dependency (final step)

**Acceptance Criteria**:
- [ ] Migration tooling functional
- [ ] Can run Ditto and Automerge+Iroh in parallel
- [ ] Performance parity or better
- [ ] Migration guide complete
- [ ] Timeline for Ditto removal documented

**Dependencies**: #53, #54, #55

**Estimated Effort**: 1 week

---

## Miscellaneous Issues

### Issue #57: Update ADR-010 Status to "Superseded"

**Title**: Mark ADR-010 as Superseded by ADR-011

**Labels**: `documentation`, `adr`

**Description**:

Update ADR-010 (Transport Layer UDP/TCP) to mark it as superseded by ADR-011, which adopts QUIC via Iroh instead of custom TCP/UDP implementation.

**Tasks**:
- [ ] Update ADR-010 header:
  ```markdown
  **Status**: Superseded by ADR-011 (Automerge + Iroh Integration)
  **Date**: 2025-11-06
  ```
- [ ] Add "Superseded" section explaining why:
  - Iroh provides superior multi-path QUIC
  - Connection migration and failover built-in
  - No need for custom UDP/TCP implementation
- [ ] Update cross-references in other ADRs

**Acceptance Criteria**:
- [ ] ADR-010 status updated
- [ ] Superseded section written
- [ ] Cross-references updated

**Estimated Effort**: 30 minutes

---

### Issue #58: Set Up GitHub Project Board

**Title**: Create GitHub Project Board for Refactoring

**Labels**: `project-management`

**Description**:

Create a GitHub Project board to track all refactoring work across 16 weeks.

**Tasks**:
- [ ] Create project board with columns:
  - Backlog
  - Phase 1 (Weeks 1-2)
  - Phase 2 (Weeks 3-4)
  - Phase 3 (Weeks 5-6)
  - Phase 4 (Weeks 7-10)
  - Phase 5 (Weeks 11-14)
  - Phase 6 (Weeks 15-16)
  - In Progress
  - Review
  - Done
- [ ] Link all issues to project board
- [ ] Set up automation (issue moves to In Progress when assigned)
- [ ] Add milestones for each phase

**Acceptance Criteria**:
- [ ] Project board created and organized
- [ ] All issues linked
- [ ] Automation configured

**Estimated Effort**: 1 hour

---

## Summary

**Total Issues**: 14 issues
- 2 Epic issues (#44, #52)
- 7 Phase implementation issues (#45-#51)
- 4 ADR-011 implementation issues (#53-#56)
- 1 Documentation issue (#57)
- 1 Project management issue (#58)

**Total Timeline**: 16 weeks for ADR-012, then 6 weeks for ADR-011

**Resource Requirements**:
- 1-2 developers full-time
- ROS2 expertise for Phase 5 (optional)
- DevOps for CI/CD setup

**Next Steps**:
1. Review and approve this issue list
2. Create issues in GitHub
3. Set up project board
4. Begin Phase 1 (cap-schema)
