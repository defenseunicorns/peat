# CAP Protocol Major Refactoring Playbook

**Version**: 1.0
**Date**: 2025-11-06
**Status**: Planning
**Related ADRs**: ADR-011 (Automerge + Iroh), ADR-012 (Schema & Protocol Extensibility)

## Executive Summary

This playbook guides the 16-week refactoring of CAP Protocol to:
1. **Eliminate Ditto licensing dependency** (ADR-011)
2. **Separate schema from protocol** (ADR-012)
3. **Enable multi-transport support** (gRPC, ROS2, HTTP, MQTT)
4. **Implement Automerge + Iroh for CRDT sync**

**Timeline**: 16 weeks (4 months)
**Outcome**: Production-ready open-source CAP Protocol with multi-transport support

---

## Architecture Overview

### Before (Current State)

```
┌─────────────────────────────────────────┐
│        cap-protocol (monolithic)         │
├─────────────────────────────────────────┤
│  • Ditto SDK (proprietary)               │
│  • Inline message schemas                │
│  • TCP-only transport                    │
│  • Cell formation logic                  │
│  • Composition engine                    │
└─────────────────────────────────────────┘
```

**Issues**:
- ❌ Ditto licensing blocks open-source deployment
- ❌ Schemas embedded in code, no versioning
- ❌ Cannot integrate with ROS2, gRPC, or legacy C2
- ❌ TCP limitations: no multi-path, connection migration

### After (Target Architecture)

```
┌─────────────────────────────────────────────────────────────┐
│                    CAP ECOSYSTEM                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │      cap-schema (Foundational Library)                │  │
│  │  • Protobuf message definitions                       │  │
│  │  • Ontology (capabilities, cells, squads)             │  │
│  │  • Code generation (Rust, Python, C++, Java)          │  │
│  │  • Validation & versioning                            │  │
│  └──────────────────────────────────────────────────────┘  │
│                           ↓ uses                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │      cap-core (Protocol Implementation)               │  │
│  │  • Cell formation logic                               │  │
│  │  • Composition engine (E6: additive, emergent, etc)   │  │
│  │  • Hierarchical coordination                          │  │
│  │  • Business rules (no transport coupling)             │  │
│  └──────────────────────────────────────────────────────┘  │
│                    ↓ uses         ↓ uses                    │
│  ┌─────────────────────────┐  ┌──────────────────────────┐ │
│  │  cap-persistence        │  │  cap-transport           │ │
│  │  • DataStore trait      │  │  • MessageTransport trait│ │
│  │  • Automerge + Iroh impl│  │  • HTTP/WebSocket        │ │
│  │  • CRDT sync engine     │  │  • gRPC                  │ │
│  │  • QUIC multi-path      │  │  • ROS2 DDS              │ │
│  │  • Query interface      │  │  • MQTT (future)         │ │
│  └─────────────────────────┘  └──────────────────────────┘ │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Benefits**:
- ✅ Open-source licensing (Apache-2.0)
- ✅ Multi-path QUIC networking (Starlink + MANET + 5G)
- ✅ Multi-transport support (gRPC, ROS2, HTTP)
- ✅ Schema versioning and code generation
- ✅ External system integration (C2, robotics, IoT)

---

## Implementation Strategy

### Critical Path: ADR-012 → ADR-011

**ADR-012 MUST be implemented first** because:
1. Schema definitions drive CRDT document structure
2. Transport abstractions enable Automerge + Iroh integration
3. Persistence traits provide interface for sync engines

```
Weeks 1-2:   cap-schema (protobuf definitions)
Weeks 3-4:   cap-transport (HTTP/WebSocket adapter)
Weeks 5-6:   cap-persistence (traits + initial impl)
Weeks 7-10:  Protocol adapters (gRPC, ROS2)
Weeks 11-14: cap-core refactoring (use new abstractions)
Weeks 15-16: Integration validation
```

**Then ADR-011** (Automerge + Iroh):
- Implement `DataStore` trait with Automerge + Iroh
- Replace Ditto sync engine
- Multi-path QUIC networking
- Performance benchmarking

---

## Phase-by-Phase Breakdown

### Phase 0: Preparation (Week 1)

**Goal**: Set up infrastructure for new crates

**Tasks**:
1. Create workspace structure for new crates
2. Update ADR-010 status to "Superseded by ADR-011"
3. Create GitHub project board for tracking
4. Set up CI for new crates
5. Review and finalize ADR-011 and ADR-012

**Deliverables**:
- [ ] Workspace Cargo.toml configured
- [ ] New crate directories created
- [ ] GitHub issues created (see Issues section below)
- [ ] CI workflows updated

**Exit Criteria**:
- Clean build of existing code
- GitHub project board ready
- Team alignment on approach

---

### Phase 1: cap-schema - Schema Foundation (Weeks 1-2)

**Goal**: Create foundational schema library with Protobuf definitions

**Related Issue**: #44 (to be created)

**Tasks**:

**Week 1: Core Message Schemas**
1. Set up `cap-schema` crate structure
2. Define core protobuf messages:
   ```protobuf
   // node.proto
   message NodeConfig {
     string id = 1;
     string platform_type = 2;
     repeated Capability capabilities = 3;
     Metadata metadata = 4;
   }

   // capability.proto
   message Capability {
     string id = 1;
     string name = 2;
     CapabilityType type = 3;
     float confidence = 4;
     map<string, string> metadata = 5;
   }

   // cell.proto
   message CellState {
     CellConfig config = 1;
     repeated string members = 2;
     string leader_id = 3;
     repeated Capability capabilities = 4;
     CellFormationStatus status = 5;
   }
   ```

3. Generate Rust bindings with `prost`
4. Unit tests for serialization/deserialization

**Week 2: Ontology and Validation**
1. Define composition rules in protobuf
2. Add validation rules (e.g., max_members check)
3. Create code generation scripts for Python/C++
4. Documentation for schema usage

**Deliverables**:
- [ ] `cap-schema/protos/` with all message definitions
- [ ] Generated Rust code in `cap-schema/src/generated/`
- [ ] Python bindings generated
- [ ] Schema documentation

**Exit Criteria**:
- All core CAP messages defined in protobuf
- Rust bindings compile and tests pass
- Python code generation works
- Documentation covers all message types

**Success Metrics**:
- 100% coverage of existing CAP models
- < 100ms schema validation overhead
- < 5% size increase vs current JSON

---

### Phase 2: cap-transport - Transport Abstraction (Weeks 3-4)

**Goal**: Create transport abstraction with HTTP/WebSocket adapter

**Related Issue**: #45 (to be created)

**Tasks**:

**Week 3: Transport Trait Definition**
1. Define `MessageTransport` trait:
   ```rust
   #[async_trait]
   pub trait MessageTransport: Send + Sync {
       async fn send(&self, message: CapMessage) -> Result<()>;
       async fn receive(&self) -> Result<CapMessage>;
       async fn subscribe(&self, topic: &str) -> Result<Subscription>;
       async fn publish(&self, topic: &str, message: CapMessage) -> Result<()>;
   }
   ```

2. Create transport abstraction types
3. Define error handling and retry logic
4. Unit tests for trait implementations

**Week 4: HTTP/WebSocket Adapter**
1. Implement `HttpWebSocketTransport`:
   - Axum for HTTP server
   - tokio-tungstenite for WebSocket
   - Message routing logic
2. Integration tests with cap-schema messages
3. Performance benchmarks
4. Documentation and examples

**Deliverables**:
- [ ] `cap-transport/src/traits.rs` with core abstractions
- [ ] `cap-transport/src/http_ws.rs` with HTTP/WS adapter
- [ ] Integration tests passing
- [ ] Transport adapter documentation

**Exit Criteria**:
- HTTP/WebSocket transport works end-to-end
- Can send/receive cap-schema messages
- < 10ms transport overhead
- Documentation complete

---

### Phase 3: cap-persistence - Persistence Abstraction (Weeks 5-6)

**Goal**: Create persistence abstraction layer with external API

**Related Issue**: #46 (to be created)

**Tasks**:

**Week 5: Persistence Traits**
1. Define `DataStore` trait:
   ```rust
   #[async_trait]
   pub trait DataStore: Send + Sync {
       async fn put(&self, collection: &str, doc: Document) -> Result<String>;
       async fn get(&self, collection: &str, id: &str) -> Result<Option<Document>>;
       async fn query(&self, collection: &str, query: Query) -> Result<Vec<Document>>;
       async fn subscribe(&self, collection: &str, query: Query) -> Result<Subscription>;
       async fn delete(&self, collection: &str, id: &str) -> Result<()>;
   }
   ```

2. Define `SyncEngine` trait (similar to existing)
3. Create test implementation with SQLite
4. Unit tests

**Week 6: External REST API**
1. Build REST API on top of `DataStore`:
   - GET /collections/{name}/documents/{id}
   - POST /collections/{name}/documents
   - GET /collections/{name}/query
   - WebSocket /collections/{name}/subscribe
2. Add authentication/authorization middleware
3. API documentation (OpenAPI spec)
4. Integration tests

**Deliverables**:
- [ ] `cap-persistence/src/traits.rs` with core abstractions
- [ ] SQLite implementation for testing
- [ ] REST API service
- [ ] OpenAPI specification
- [ ] API documentation

**Exit Criteria**:
- DataStore trait allows full CRUD operations
- REST API functional
- Authentication works
- < 50ms query latency for simple queries

---

### Phase 4: Protocol Adapters - gRPC & ROS2 (Weeks 7-10)

**Goal**: Implement production-quality gRPC and ROS2 transports

**Related Issues**: #47 (gRPC), #48 (ROS2)

**Tasks**:

**Weeks 7-8: gRPC Transport**
1. Define gRPC service in `service.proto`:
   ```protobuf
   service CapProtocol {
     rpc GetNode(GetNodeRequest) returns (NodeConfig);
     rpc CreateCell(CreateCellRequest) returns (CellState);
     rpc StreamCellUpdates(StreamRequest) returns (stream CellState);
     rpc PublishCapability(Capability) returns (Ack);
   }
   ```

2. Generate gRPC bindings (tonic)
3. Implement client and server
4. Performance benchmarks vs HTTP
5. Integration tests

**Weeks 9-10: ROS2 Transport**
1. Generate ROS2 IDL from protobuf schemas
2. Create ROS2 package with message types
3. Implement DDS publisher/subscriber adapter
4. Integration tests with real ROS2 nodes
5. Example: Robot publishing position to CAP

**Deliverables**:
- [ ] gRPC transport adapter
- [ ] ROS2 transport adapter
- [ ] Performance benchmark results
- [ ] ROS2 example (robot integration)
- [ ] Transport adapter comparison matrix

**Exit Criteria**:
- gRPC transport < 5ms latency
- ROS2 transport works with ros2cli tools
- Can bridge ROS2 topics to CAP cells
- Documentation complete

---

### Phase 5: cap-core Refactoring (Weeks 11-14)

**Goal**: Refactor cap-protocol to use new abstractions

**Related Issue**: #49 (to be created)

**Tasks**:

**Week 11: Schema Migration**
1. Replace inline Rust structs with cap-schema types
2. Update CellState, NodeConfig, Capability to use protobuf
3. Migrate serialization logic
4. Update all unit tests

**Week 12: Transport Integration**
1. Remove direct Ditto SDK calls
2. Use cap-transport abstractions
3. Add transport selection via configuration
4. Update E2E tests

**Week 13: Persistence Integration**
1. Replace Ditto storage with cap-persistence traits
2. Keep DittoBackend as one DataStore implementation
3. Add configuration for storage backend selection
4. Migrate all storage layer tests

**Week 14: Testing and Documentation**
1. Run full E2E test suite
2. Performance regression testing
3. Update all documentation
4. Create migration guide for existing deployments

**Deliverables**:
- [ ] cap-protocol uses cap-schema exclusively
- [ ] All transports configurable
- [ ] All persistence configurable
- [ ] E2E tests passing
- [ ] Migration guide

**Exit Criteria**:
- All existing E2E tests pass
- No hard dependency on Ditto
- Can switch transports via config
- Performance at parity with current system

---

### Phase 6: Integration Validation (Weeks 15-16)

**Goal**: Validate external system integrations

**Related Issue**: #50 (to be created)

**Tasks**:

**Week 15: Integration Examples**
1. **ROS2 Example**: Robot publishing sensor data
   ```bash
   ros2 topic echo /cap/capabilities
   ros2 service call /cap/form_cell cap_msgs/srv/FormCell
   ```

2. **Python Example**: ML pipeline consuming CAP data
   ```python
   from cap_schema import CellState, Capability
   client = CapHttpClient("http://localhost:8080")
   cells = client.query_cells(has_capability="Sensor")
   ```

3. **Java Example**: Legacy C2 system querying CAP
   ```java
   CapGrpcClient client = new CapGrpcClient("localhost:50051");
   CellState cell = client.getCell("squad_alpha");
   ```

4. **JavaScript Example**: Web dashboard
   ```javascript
   const ws = new WebSocket('ws://localhost:8080/subscribe/cells');
   ws.onmessage = (event) => {
     const cell = CellState.decode(event.data);
     updateDashboard(cell);
   };
   ```

**Week 16: Performance & Polish**
1. Load testing (100 nodes, 1000 messages/sec)
2. Latency benchmarks across all transports
3. Memory profiling
4. Documentation polish
5. Example deployment guide

**Deliverables**:
- [ ] 4 working integration examples (ROS2, Python, Java, JavaScript)
- [ ] Performance test results
- [ ] Deployment guide
- [ ] Final documentation review

**Exit Criteria**:
- All integration examples work
- Latency < 100ms p99 for critical operations
- Memory usage stable under load
- Documentation complete and accurate

---

## ADR-011 Implementation (Post-ADR-012)

**Timeline**: 4-6 weeks after ADR-012 completion

Once cap-schema, cap-transport, and cap-persistence are in place:

### Automerge + Iroh Integration

**Week 1-2: Automerge DataStore Implementation**
1. Implement `DataStore` trait with Automerge
2. Map protobuf messages to Automerge document structure
3. Implement delta sync using Automerge sync protocol
4. Unit tests for all operations

**Week 3-4: Iroh Networking Layer**
1. Integrate Iroh for QUIC transport
2. Implement multi-path networking (Starlink + MANET + 5G)
3. Connection migration and failover
4. Performance benchmarks

**Week 5-6: DittoBackend Deprecation**
1. Feature flag for backend selection (Ditto vs Automerge+Iroh)
2. Migration tooling for data export/import
3. Performance comparison tests
4. Final decision on Ditto deprecation timeline

---

## Testing Strategy

### Unit Tests
- Every crate has > 80% code coverage
- Focus on trait implementations
- Mock external dependencies

### Integration Tests
- Cross-crate integration (e.g., transport + persistence)
- Message serialization round-trips
- Error handling and edge cases

### E2E Tests
- Existing E2E tests MUST continue to pass
- New E2E tests for multi-transport scenarios
- Performance regression tests

### Acceptance Tests
- External system integration (ROS2, gRPC clients)
- Load testing (100+ nodes, 1000 msg/sec)
- Network failure scenarios (partition, latency spikes)

---

## Risk Management

### Risk 1: Timeline Overruns

**Likelihood**: Medium
**Impact**: High

**Mitigation**:
- Implement incrementally with feature flags
- Keep Ditto backend working throughout refactoring
- Weekly progress reviews and re-estimation

### Risk 2: Performance Degradation

**Likelihood**: Medium
**Impact**: High

**Mitigation**:
- Benchmark every phase against current baseline
- Optimize critical paths early
- Accept temporary performance hit for correct architecture

### Risk 3: Breaking Changes

**Likelihood**: High
**Impact**: Medium

**Mitigation**:
- Feature flags for new vs old code paths
- Maintain backward compatibility in cap-schema
- Gradual migration with deprecation warnings

### Risk 4: Automerge + Iroh Integration Issues

**Likelihood**: Medium
**Impact**: High

**Mitigation**:
- POC validation before full implementation
- Keep Ditto as fallback option
- Abstract sync engine behind trait (already done)

### Risk 5: External Integration Complexity

**Likelihood**: Medium
**Impact**: Medium

**Mitigation**:
- Start with simplest integrations (HTTP/gRPC)
- ROS2 integration is optional enhancement
- Focus on schema quality for external consumption

---

## Success Metrics

### Technical Metrics

| Metric | Target | Rationale |
|--------|--------|-----------|
| **Schema Overhead** | < 5% vs current JSON | Protobuf efficiency |
| **Transport Latency** | < 10ms HTTP, < 5ms gRPC | Low-latency operations |
| **Query Performance** | < 50ms for simple queries | Usability |
| **Sync Throughput** | > 1000 ops/sec | Tactical scale |
| **Memory Usage** | < 500MB for 100 nodes | Resource efficiency |
| **Code Coverage** | > 80% | Quality assurance |

### Integration Metrics

| Metric | Target | Rationale |
|--------|--------|-----------|
| **ROS2 Integration** | < 100ms topic bridge latency | Real-time robotics |
| **gRPC Latency** | p99 < 20ms | C2 system responsiveness |
| **Multi-Language Support** | Rust, Python, C++, Java | Ecosystem breadth |
| **External API Uptime** | > 99.9% | Production readiness |

### Business Metrics

| Metric | Target | Rationale |
|--------|--------|-----------|
| **Licensing Cost** | $0 (open-source) | Eliminates Ditto fees |
| **Integration Effort** | < 1 week per new transport | Extensibility |
| **Time to Production** | 16-20 weeks | Acceptable timeline |
| **NATO STANAG Readiness** | Q2 2026 | Strategic goal |

---

## Rollout Strategy

### Phase 1 (Weeks 1-6): Foundation
- Deploy cap-schema internally
- cap-transport HTTP adapter for testing
- cap-persistence with SQLite backend

**Deployment**: Development environments only

### Phase 2 (Weeks 7-10): Protocol Adapters
- gRPC and ROS2 transports available
- External API accessible for testing
- Integration examples published

**Deployment**: Staging environments + partner testing

### Phase 3 (Weeks 11-14): Core Refactoring
- cap-protocol uses new abstractions
- Feature flags for new vs old paths
- Migration scripts available

**Deployment**: Beta release with opt-in feature flags

### Phase 4 (Weeks 15-16): Validation
- All integrations validated
- Performance benchmarks published
- Production-ready documentation

**Deployment**: General availability (v2.0 release)

### Post-Release: Automerge + Iroh
- ADR-011 implementation begins
- Parallel testing with Ditto backend
- Gradual rollout with monitoring

**Deployment**: Feature-flagged rollout over 4-6 weeks

---

## Communication Plan

### Internal Updates
- **Weekly**: Progress reports on GitHub project board
- **Bi-weekly**: Team sync on blockers and decisions
- **Monthly**: Executive summary for stakeholders

### External Updates
- **Blog posts**: Major milestones (cap-schema release, gRPC adapter, etc.)
- **Documentation**: Updated continuously in /docs
- **Demos**: Integration examples published as released

### Decision Points
- **Week 6**: Go/no-go for protocol adapters
- **Week 10**: Evaluate ROS2 integration feasibility
- **Week 14**: Final decision on Ditto deprecation timeline
- **Week 16**: Production readiness assessment

---

## Next Steps

### Immediate Actions (This Week)
1. ✅ Finalize ADR-011 and ADR-012
2. ⬜ Create GitHub issues (see next section)
3. ⬜ Set up GitHub project board
4. ⬜ Update ADR-010 to "Superseded"
5. ⬜ Team review and approval of playbook

### Week 1 Actions
1. Create workspace structure for new crates
2. Set up CI for cap-schema
3. Begin protobuf message definitions
4. Team kickoff meeting

---

## Appendix: Related Documents

- **ADR-011**: CRDT + Networking Stack Selection (Automerge + Iroh)
- **ADR-012**: Schema Definition and Protocol Extensibility
- **ADR-010**: Transport Layer (superseded by ADR-011)
- **ADR-007**: Automerge-Based Sync Engine (superseded by ADR-011)
- **ADR-005**: Data Sync Abstraction Layer (foundation for this work)

## Appendix: GitHub Issues Checklist

See `GITHUB-ISSUES.md` for detailed issue templates.

---

**End of Playbook**
