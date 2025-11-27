# ADR-020: TAK (Team Awareness Kit) and Cursor-on-Target Integration

**Status**: Proposed
**Date**: 2025-11-17
**Updated**: 2025-11-26 (M1 POC integrator feedback)
**Authors**: Kit Plummer
**Related ADRs**:
- [ADR-012](012-schema-definition-protocol-extensibility.md) (Schema Definition & Protocol Extensibility)
- [ADR-006](006-security-authentication-authorization.md) (Security, Authentication, Authorization)
- [ADR-009](009-bidirectional-hierarchical-flows.md) (Bidirectional Hierarchical Flows)
- [ADR-010](010-transport-layer-udp-tcp.md) (Transport Layer)
- [ADR-019](019-qos-and-data-prioritization.md) (QoS and Data Prioritization)
- [ADR-028](028-cot-detail-extension-schema.md) (CoT Custom Detail Extension Schema) - NEW
- [ADR-029](029-tak-transport-adapter.md) (TAK Transport Adapter) - NEW

**Integrator Input**:
- [TAK Integration Requirements](TAK_INTEGRATION_REQUIREMENTS.md) (M1 POC feedback)
- [CoT Schema Mapping](COT_SCHEMA_MAPPING.md) (Field-level mappings)

## Context

### The TAK Ecosystem

**Team Awareness Kit (TAK)** is the U.S. Government's premier tool for shared situational awareness, with widespread adoption across:

- **Military**: ATAK (Android Tactical Assault Kit) - Used by U.S. Special Forces, conventional forces, and NATO allies
- **Government**: TAK-GOV - DHS, CBP, FBI, state/local first responders
- **Civilian**: ATAK-CIV/WinTAK/iTAK - Emergency response, search and rescue, humanitarian operations

**Key TAK Characteristics:**
- **Maturity**: Operational since 2010 (AFRL development), combat-proven
- **Scale**: Thousands of active users across DoD, DHS, and allied nations
- **Interoperability**: Common operating picture across heterogeneous systems
- **Extensibility**: Plugin architecture enabling domain-specific capabilities
- **Transport**: Supports UDP/TCP multicast, TAK servers, federation

### Cursor-on-Target (CoT) Protocol

**CoT** is an XML-based (or Google Protocol Buffers) message format for real-time situational awareness data exchange:

**Core Schema Elements:**
```xml
<event uid="..." type="..." time="..." start="..." stale="..." how="...">
  <point lat="..." lon="..." hae="..." ce="..." le="..."/>
  <detail>
    <!-- Extensible sub-schemas for mission-specific data -->
  </detail>
</event>
```

**CoT Type Hierarchy:**
- Uses **MIL-STD-2525** symbology (atoms, affiliation, battle dimension)
- Examples: `a-f-G-U-C` (friendly ground unit - combat), `a-h-G` (hostile ground equipment)
- Standardized types enable automatic symbol rendering

**CoT Message Characteristics:**
- **Size**: XML format up to 40KB per message (larger than HIVE's differential sync)
- **Transport**: UDP/TCP, optionally Protobuf-encoded for bandwidth efficiency
- **Update Rate**: Typically 1-5 second position updates
- **Network**: Designed for IP-based networks (not optimized for contested/DIL environments)

### Strategic Importance for HIVE

**AUKUS Integration Requirement:**
- TAK is extensively used within AUKUS partners (US, UK, Australia)
- AUKUS Pillar II emphasizes technology interoperability
- **Requirement**: HIVE must integrate with existing TAK infrastructure to enable adoption

**Ecosystem Benefits:**
1. **Common Operating Picture**: HIVE-coordinated assets visible in TAK
2. **Human-Machine Interface**: TAK serves as operator interface for HIVE teams
3. **Interoperability**: HIVE coordinates autonomy; TAK provides situational awareness to C2
4. **Gradual Adoption**: Organizations can integrate HIVE without replacing TAK infrastructure

**Challenge**: TAK's all-to-all event streaming model conflicts with HIVE's hierarchical aggregation approach.

### The Integration Problem

**TAK's Architecture (Event Streaming)**:
```
┌─────────────────────────────────────────────────────────┐
│            TAK Server / Federation                      │
│  (All events replicated to all clients - O(n²))        │
└─────────────┬───────────────────────────────────────────┘
              │
    ┌─────────┼─────────┬─────────┬─────────┐
    │         │         │         │         │
 [ATAK]   [WinTAK]  [ATAK]   [ATAK]   [ATAK]
```

**HIVE's Architecture (Hierarchical Aggregation)**:
```
┌─────────────────────────────────────────────────────────┐
│              Company HQ (Aggregated View)               │
└─────────────┬───────────────────────────────────────────┘
              │
       ┌──────┴──────┐
       │ Platoon HQ  │
       └──────┬──────┘
              │
       ┌──────┴──────┬──────────┬──────────┐
    [Squad A]    [Squad B]   [Squad C]   [Squad D]
    (10 UAVs)    (10 UGVs)   (8 UAVs)    (12 platforms)
```

**Architectural Tension:**
- TAK expects all position updates from all platforms
- HIVE provides aggregated capabilities and filtered/summarized position data
- **Risk**: Naively bridging HIVE → TAK could saturate network with O(n²) messages
- **Opportunity**: TAK can display HIVE's hierarchical abstractions as cells/formations

## Decision

We will implement **bidirectional integration between HIVE Protocol and TAK ecosystem** through a **three-tier integration architecture**:

### Tier 1: CoT Message Schema Integration (cap-schema layer)

**Create CoT message adapters in `cap-schema` crate:**

```
cap-schema/
├── proto/
│   ├── cot_bridge.proto       # Protobuf representation of CoT concepts
│   └── ...
├── src/
│   ├── cot/
│   │   ├── encoder.rs         # HIVE → CoT XML/Protobuf
│   │   ├── decoder.rs         # CoT → HIVE messages
│   │   ├── types.rs           # CoT type hierarchy mapping
│   │   └── validation.rs      # CoT schema validation
```

**Bidirectional Message Mapping:**

| HIVE Concept | CoT Representation | Direction | Notes |
|--------------|-------------------|-----------|-------|
| Platform Position | CoT Event (type: a-f-G-U-C) | HIVE → TAK | Individual platform tracks |
| Squad Formation | CoT Event + detail/link | HIVE → TAK | Cell as tactical graphic |
| Capability Aggregate | CoT Event (custom detail) | HIVE → TAK | Squad-level capability summary |
| Command Intent | CoT Event (type: t-x-m) | TAK → HIVE | Mission tasking from C2 |
| Geofence/ROZ | CoT Event (type: u-d-r) | TAK → HIVE | Operational boundaries |
| Chat/Text | CoT Event (type: b-t-f) | Bidirectional | Text messaging |

**MIL-STD-2525 Entity Type Mappings** (from M1 POC):

| HIVE Entity | CoT Type | MIL-STD-2525 Description |
|-------------|----------|--------------------------|
| Tracked Person (POI) | `a-f-G-E-S` | Friendly Ground Equipment - Sensor |
| Tracked Vehicle | `a-f-G-E-V` | Friendly Ground Equipment - Vehicle |
| Unknown Track | `a-u-G` | Unknown Ground |
| Hostile Track | `a-h-G` | Hostile Ground |
| HIVE Platform (UGV) | `a-f-G-U-C` | Friendly Ground Unit - Combat |
| HIVE Platform (UAV) | `a-f-A-M-F-Q` | Friendly Air - Military Fixed Wing - UAV |
| HIVE Operator | `a-f-G-U-C-I` | Friendly Ground Unit - Infantry |
| HIVE Cell/Team | `a-f-G-U-C` + links | Friendly Ground Unit with subordinates |
| Formation | `a-f-G-U-C` + links | Higher echelon unit |
| Geofence/ROZ | `u-d-r` | Drawing - Route/Area |
| Mission Tasking | `t-x-m-c` | Tasking - Mission - Track |
| Handoff Event | `a-x-h-h` | Custom - HIVE Handoff |

**Hierarchy Encoding via CoT Links:**

HIVE's hierarchical relationships map to CoT `<link>` elements with relation types:

```xml
<!-- Platform belongs to cell -->
<link uid="Alpha-Team" type="a-f-G-U-C" relation="p-p" remarks="parent-cell"/>

<!-- Cell belongs to formation -->
<link uid="Formation-1" type="a-f-G-U-C" relation="p-p" remarks="parent-formation"/>

<!-- Track handoff relationship -->
<link uid="Bravo-Team" type="a-f-G-U-C" relation="h-h" remarks="handoff-target"/>
```

| Relation | Meaning | Usage |
|----------|---------|-------|
| `p-p` | Parent | Hierarchical ownership (platform→cell→formation) |
| `h-h` | Handoff | Track transfer between cells |
| `s-s` | Sibling | Same echelon coordination |
| `o-o` | Observing | Sensor→track relationship |

**Key Design Principles:**
1. **Semantic Preservation**: HIVE capabilities map to appropriate CoT detail sub-schemas
2. **Minimal Overhead**: Only send necessary data; leverage CoT's extensible detail fields
3. **Standard Compliance**: Use existing CoT sub-schemas where possible (flow-tags, sensor, etc.)
4. **Hierarchy Encoding**: Use CoT link elements to represent squad→platform relationships

### Tier 2: TAK Transport Adapter (cap-transport layer)

**Implement TAK protocol adapter as new transport:**

```rust
// cap-transport/src/tak_transport.rs

pub struct TakTransport {
    config: TakConfig,
    client: TakClient,
    cot_encoder: CotEncoder,
    cot_decoder: CotDecoder,
}

pub struct TakConfig {
    /// TAK server connection (TCP/SSL)
    server_address: SocketAddr,
    
    /// Client certificate for authentication
    client_cert: Option<Certificate>,
    
    /// Multicast group for mesh SA
    multicast_group: Option<IpAddr>,
    
    /// CoT protocol version (XML vs Protobuf)
    protocol_version: CotProtocolVersion,
    
    /// Message filtering rules
    filters: Vec<MessageFilter>,
}

#[async_trait]
impl MessageTransport for TakTransport {
    async fn send(&self, message: &dyn CapMessage, ...) -> Result<...> {
        // 1. Convert HIVE message to CoT event
        let cot_event = self.cot_encoder.encode(message)?;
        
        // 2. Serialize to XML or Protobuf
        let bytes = match self.config.protocol_version {
            CotProtocolVersion::Xml => cot_event.to_xml(),
            CotProtocolVersion::Protobuf => cot_event.to_protobuf(),
        };
        
        // 3. Send via TAK protocol (TAK Server or Mesh)
        self.client.send(bytes).await?;
        
        Ok(...)
    }
    
    async fn subscribe<M: CapMessage>(&self, filter: MessageFilter) 
        -> Result<MessageStream<M>, TransportError> 
    {
        // 1. Subscribe to CoT messages from TAK
        let cot_stream = self.client.subscribe().await?;
        
        // 2. Filter and decode into HIVE messages
        let hive_stream = cot_stream
            .filter_map(|cot_event| self.cot_decoder.decode::<M>(cot_event))
            .filter(|msg| filter.matches(msg));
        
        Ok(hive_stream)
    }
}
```

**TAK Protocol Implementation Details:**

1. **TAK Server Mode**: 
   - TCP connection to TAK server (typically port 8087/8089 SSL)
   - Supports TAK federation for multi-server environments
   - Handles TAK authentication (client certificates, TAK Server user accounts)

2. **Mesh SA (Situational Awareness) Mode**:
   - UDP multicast for local tactical networks
   - Uses TAK Protocol Version 1 (Protobuf with static/dynamic headers)
   - Format: `191 1 191 <payload>` for mesh, `191 <varint> <payload>` for streaming

3. **Message Throttling**:
   - Implement intelligent filtering to prevent O(n²) message explosion
   - Only forward relevant updates based on recipient's echelon
   - Aggregate position updates into squad-level summaries when appropriate

### Tier 3: Hierarchical Filtering & Aggregation Bridge

**Implement HIVE-specific logic to bridge hierarchical model with TAK's flat model:**

```rust
// cap-protocol/src/tak_bridge.rs

pub struct HiveTakBridge {
    hive_node: HiveNode,
    tak_transport: TakTransport,
    aggregation_policy: AggregationPolicy,
}

pub enum AggregationPolicy {
    /// Send all individual platform positions (O(n²) - use cautiously)
    FullFidelity,

    /// Send only squad leader positions + squad aggregate capabilities
    SquadLeaderOnly,

    /// Send hierarchical summaries: company → platoon → squad → platforms
    /// Recipients see detail appropriate to their echelon
    HierarchicalFiltering,

    /// Custom filtering based on CoT type, recipient, and HIVE cell membership
    CustomFilters(Vec<FilterRule>),

    // === Additional policies from M1 POC feedback ===

    /// Track-focused: Only active tracks visible, not platform positions
    /// Useful when operators care about targets, not HIVE assets
    TracksOnly,

    /// Capability-focused: Formation capabilities only, not positions
    /// For high-level C2 that needs capability awareness without clutter
    CapabilitySummaryOnly,

    /// Time-windowed: Aggregate position updates over N seconds
    /// Reduces message rate while maintaining accuracy
    TimeWindowed { window_secs: u32 },

    /// Bandwidth-adaptive: Dynamically adjust based on link quality
    /// Integrates with ADR-019 QoS bandwidth monitoring
    BandwidthAdaptive { target_kbps: u32 },
}

/// QoS Priority to CoT Flow-Tags Mapping (ADR-019 integration)
///
/// Maps HIVE QoS priorities to CoT `_flow-tags_` for TAK-side prioritization.
/// TAK servers that honor flow-tags will process messages accordingly.
///
/// | HIVE Priority | CoT Flow-Tag | TAK Behavior |
/// |---------------|--------------|--------------|
/// | P1 (Critical) | `priority=flash` | Immediate delivery |
/// | P2 (High) | `priority=immediate` | High priority queue |
/// | P3 (Normal) | `priority=routine` | Normal delivery |
/// | P4 (Low) | `priority=deferred` | Best effort |
/// | P5 (Bulk) | `priority=bulk` | Background delivery |
pub fn priority_to_flow_tag(priority: Priority) -> &'static str {
    match priority {
        Priority::Critical => "flash",
        Priority::High => "immediate",
        Priority::Normal => "routine",
        Priority::Low => "deferred",
        Priority::Bulk => "bulk",
    }
}

impl HiveTakBridge {
    /// Publish HIVE state to TAK
    pub async fn publish_to_tak(&self) -> Result<()> {
        match self.aggregation_policy {
            AggregationPolicy::HierarchicalFiltering => {
                // Company HQ sees: platoon summaries
                self.send_aggregated_view(Echelon::Company).await?;
                
                // Platoon HQ sees: squad details
                self.send_aggregated_view(Echelon::Platoon).await?;
                
                // Squad leaders see: individual platform positions
                self.send_platform_positions().await?;
            },
            // ... other policies
        }
        Ok(())
    }
    
    /// Ingest TAK events into HIVE
    pub async fn ingest_from_tak(&self) -> Result<()> {
        let mut tak_stream = self.tak_transport.subscribe::<CotEvent>(
            MessageFilter::default()
        ).await?;
        
        while let Some(cot_event) = tak_stream.next().await {
            match cot_event.event_type {
                CotType::MissionTasking => {
                    // Convert TAK mission tasking to HIVE command
                    let command = self.convert_to_hive_command(cot_event)?;
                    self.hive_node.execute_command(command).await?;
                },
                CotType::Geofence => {
                    // Import geofence as HIVE operational constraint
                    let constraint = self.convert_to_constraint(cot_event)?;
                    self.hive_node.add_constraint(constraint).await?;
                },
                CotType::FriendlyPosition => {
                    // Track external friendly units in HIVE
                    self.hive_node.update_external_track(cot_event).await?;
                },
                _ => {
                    // Log other event types for situational awareness
                },
            }
        }
        Ok(())
    }
}
```

**Filtering Strategy:**
```
┌─────────────────────────────────────────────────────────┐
│             TAK Server/Federation                       │
└─────────────┬───────────────────────────────────────────┘
              │
              │ (Filtered CoT messages)
              │
    ┌─────────▼─────────┐
    │  HIVE TAK Bridge  │
    │  (Aggregation +   │
    │   Filtering)      │
    └─────────┬─────────┘
              │
              │ (HIVE internal protocol - differential sync)
              │
    ┌─────────▼─────────┐
    │    HIVE Network   │
    │  (Hierarchical)   │
    └───────────────────┘
```

**Filtering Rules:**
- **Geographic**: Only forward CoT events within HIVE cell's area of operations
- **Temporal**: Stale events (beyond stale time) filtered out
- **Type-Based**: Only relevant CoT types forwarded (filter out irrelevant chat, admin messages)
- **Authority-Based**: Mission commands only accepted from authorized TAK users
- **Bandwidth-Aware**: Dynamically adjust forwarding rate based on network conditions

### Integration Deployment Models

**Model 1: HIVE as TAK Plugin (Tight Integration)**
- Develop ATAK plugin that exposes HIVE capabilities
- Plugin displays HIVE cell formations, hierarchical summaries
- Operators send commands to HIVE via TAK UI
- **Pros**: Seamless UX, no separate infrastructure
- **Cons**: Limited to Android devices, plugin development complexity

**Model 2: HIVE TAK Bridge Node (Federated)**
- Standalone HIVE node acts as TAK federation member
- Appears as TAK server to TAK clients
- Translates between HIVE and TAK protocols
- **Pros**: Works with all TAK clients (ATAK, WinTAK, iTAK)
- **Cons**: Additional infrastructure, configuration complexity

**Model 3: Hybrid - HIVE Core + TAK Interface Layer**
- HIVE operates independently with native protocol
- TAK transport adapter provides bidirectional bridge
- Multiple TAK bridges for scalability
- **Pros**: Best performance, flexibility, supports multiple deployment scenarios
- **Cons**: Most complex architecture

**Recommended**: **Model 3 (Hybrid)** - Provides maximum flexibility and aligns with cap-transport abstraction architecture.

## Implementation Phases

### Phase 0: Requirements & Design (Weeks 1-2)

**Goal**: Validate integration requirements with AUKUS stakeholders

**Tasks**:
1. Survey TAK deployment patterns within AUKUS partners
2. Identify critical CoT message types for HIVE integration
3. Define success criteria for integration (latency, bandwidth, usability)
4. Document security requirements (PKI, certificate management)

**Deliverables**:
- [ ] TAK integration requirements document
- [ ] CoT message mapping specification
- [ ] Security architecture design
- [ ] Stakeholder sign-off on approach

### Phase 1: CoT Schema Adapter (Weeks 3-4)

**Goal**: Implement bidirectional CoT ↔ HIVE message conversion in `cap-schema`

**Tasks**:
1. Implement CoT XML parser/generator
2. Implement CoT Protobuf encoder/decoder
3. Create HIVE → CoT message mappings (platform position, squad formation, etc.)
4. Create CoT → HIVE message mappings (mission tasking, geofences, etc.)
5. Unit tests for all conversions

**Success Criteria**:
- [ ] HIVE platform position converts to valid CoT event (MIL-STD-2525 type)
- [ ] CoT mission tasking converts to HIVE command
- [ ] Round-trip conversion preserves semantic meaning
- [ ] Validation catches malformed CoT messages

### Phase 2: TAK Transport Adapter (Weeks 5-7)

**Goal**: Implement `TakTransport` adapter in `cap-transport`

**Tasks**:
1. Implement TAK Server TCP client (with SSL/TLS support)
2. Implement Mesh SA UDP multicast client
3. Add TAK Protocol Version 1 framing (static/dynamic headers)
4. Implement TAK authentication (client certificates)
5. Integration tests with real TAK Server / FreeTakServer

**Success Criteria**:
- [ ] Can connect to TAK Server and authenticate
- [ ] Can send CoT events and receive events
- [ ] Mesh SA mode works in local network
- [ ] Handles connection failures gracefully

### Phase 3: HIVE-TAK Bridge Logic (Weeks 8-10)

**Goal**: Implement hierarchical filtering and aggregation bridge

**Tasks**:
1. Implement `HiveTakBridge` with aggregation policies
2. Add hierarchical filtering logic (echelon-based visibility)
3. Implement bandwidth-aware throttling
4. Create configuration system for filtering rules
5. End-to-end testing with HIVE + TAK ecosystem

**Success Criteria**:
- [ ] Company HQ in TAK sees platoon summaries, not individual platforms
- [ ] Squad leaders in TAK see full platform details
- [ ] Mission commands from TAK correctly execute in HIVE
- [ ] Bandwidth usage < 10% of full event streaming

### Phase 4: ATAK Plugin Development (Weeks 11-14) [Optional]

**Goal**: Create native ATAK plugin for HIVE integration

**Tasks**:
1. ATAK plugin skeleton (Java/Kotlin)
2. Display HIVE cell formations on map
3. Display aggregated capabilities in UI
4. Send commands to HIVE via plugin
5. User acceptance testing with operators

**Success Criteria**:
- [ ] Plugin installs on ATAK devices
- [ ] HIVE squad formations visible on map
- [ ] Operators can task HIVE cells via natural interface
- [ ] Performance acceptable on tactical devices

### Phase 5: Field Validation (Weeks 15-16)

**Goal**: Validate integration in realistic operational scenarios

**Tasks**:
1. Talisman Sabre 2025 integration (if timeline aligns)
2. AUKUS partner demonstration
3. Performance testing under tactical network conditions
4. Security audit and penetration testing
5. Documentation and training materials

**Success Criteria**:
- [ ] HIVE-coordinated assets visible in TAK common operating picture
- [ ] Operators successfully task HIVE via TAK
- [ ] Performance meets operational requirements
- [ ] Security audit passes with no critical findings

## Consequences

### Positive

1. **AUKUS Interoperability**: Enables HIVE adoption within existing TAK infrastructure
2. **Operator Familiarity**: Leverages existing TAK training and muscle memory
3. **Ecosystem Access**: Connects HIVE to broader TAK plugin ecosystem
4. **Gradual Adoption**: Organizations can integrate HIVE incrementally
5. **Common Operating Picture**: HIVE assets visible alongside traditional C2
6. **Standards Alignment**: Uses widely-adopted CoT message format
7. **Multi-National Collaboration**: TAK used across NATO and AUKUS partners
8. **Bidirectional Data Flow**: Both HIVE → TAK (awareness) and TAK → HIVE (tasking)

### Negative

1. **Complexity**: Additional abstraction layer increases system complexity
2. **Bandwidth Overhead**: CoT messages larger than HIVE's differential sync
3. **Impedance Mismatch**: TAK's flat model vs HIVE's hierarchical model requires careful bridging
4. **Security Surface**: Additional attack vectors through TAK integration points
5. **Dependency**: Adds TAK ecosystem as external dependency
6. **Testing Burden**: Must test against multiple TAK server implementations
7. **Maintenance**: CoT schema evolution requires ongoing adapter updates
8. **Performance**: Message translation adds latency

### Risks and Mitigations

**Risk 1**: TAK event flooding overwhelms HIVE network
- **Mitigation**: Hierarchical filtering prevents O(n²) message forwarding
- **Mitigation**: Bandwidth monitoring with dynamic throttling
- **Mitigation**: Configurable aggregation policies

**Risk 2**: CoT message translation loses semantic meaning
- **Mitigation**: Comprehensive unit tests for all message types
- **Mitigation**: Round-trip conversion validation
- **Mitigation**: Semantic validation layer in cap-schema

**Risk 3**: Security vulnerabilities in TAK integration
- **Mitigation**: PKI-based authentication for TAK connections
- **Mitigation**: Message signing for commands (ADR-006 integration)
- **Mitigation**: Security audit before operational deployment
- **Mitigation**: Network segmentation between TAK and HIVE domains

**Risk 4**: TAK server federation complexity
- **Mitigation**: Start with single TAK server deployments
- **Mitigation**: Leverage existing TAK federation expertise
- **Mitigation**: Comprehensive testing with FreeTakServer (open-source)

**Risk 5**: ATAK plugin distribution and updates
- **Mitigation**: Use standard TAK data package distribution mechanism
- **Mitigation**: Implement auto-update capabilities
- **Mitigation**: Fallback to standalone bridge mode if plugin unavailable

## Alternatives Considered

### Alternative 1: Ignore TAK Ecosystem

**Approach**: HIVE operates entirely independently, no TAK integration

**Rejected Because**:
- Blocks AUKUS adoption (TAK is deeply entrenched)
- Requires operators to learn entirely new interface
- Loses access to TAK's extensive plugin ecosystem
- Misses opportunity for common operating picture

### Alternative 2: Replace TAK with HIVE-Native UI

**Approach**: Build HIVE's own mobile/desktop UI instead of TAK integration

**Rejected Because**:
- Massive development effort (ATAK took years to mature)
- Operators already trained on TAK
- Would fragment situational awareness landscape
- Not aligned with "complementary, not competitive" strategy

### Alternative 3: TAK-Only Mode (No Hierarchical Optimization)

**Approach**: Naive bridge that forwards all HIVE events to TAK without filtering

**Rejected Because**:
- Defeats HIVE's core value proposition (hierarchical scaling)
- Would recreate O(n²) problem HIVE solves
- Unacceptable bandwidth usage at scale
- Doesn't leverage HIVE's aggregation capabilities

### Alternative 4: Custom Protocol Instead of CoT

**Approach**: Create new message format instead of using CoT

**Rejected Because**:
- Breaks interoperability with existing TAK deployments
- Requires custom TAK plugin development
- Would not work with WinTAK, iTAK, other TAK variants
- CoT is widely understood and standardized

## Success Metrics

1. **Integration Completeness**:
   - [ ] Bidirectional message flow works (HIVE ↔ TAK)
   - [ ] 5+ CoT message types supported in each direction
   - [ ] Works with TAK Server, FreeTakServer, and Mesh SA

2. **Performance**:
   - [ ] Message translation latency < 10ms
   - [ ] Bandwidth usage < 10% of naive event streaming
   - [ ] Supports 100+ HIVE platforms visible in TAK
   - [ ] TAK UI remains responsive with HIVE integration

3. **Operational Validation**:
   - [ ] Successfully demonstrated in AUKUS context
   - [ ] Operators can task HIVE teams via TAK
   - [ ] HIVE status updates visible in TAK within 2 seconds
   - [ ] Works in contested network conditions (30% packet loss)

4. **Developer Experience**:
   - [ ] Adding new CoT message type takes < 1 day
   - [ ] TAK transport adapter is plugin-compatible (no core changes)
   - [ ] Documentation enables third-party TAK integrations

5. **Security**:
   - [ ] All TAK connections authenticated (PKI)
   - [ ] Commands from TAK cryptographically verified
   - [ ] Security audit passes with no critical vulnerabilities
   - [ ] Supports TAK's standard security model (X.509 certificates)

## Related Standards & Technologies

1. **Cursor-on-Target (CoT)**:
   - [MITRE CoT Documentation](http://cot.mitre.org)
   - CoT XML Schema (publicly available)
   - TAK Protocol Version 1 (Protobuf encoding)

2. **MIL-STD-2525** (Military Symbology):
   - Standardized symbology for CoT types
   - Ensures consistent symbol rendering across systems

3. **TAK Server**:
   - Official TAK Server (government distribution)
   - FreeTakServer (open-source alternative)
   - TAK federation protocol

4. **TAK Plugins**:
   - ATAK plugin SDK (Java/Kotlin)
   - Data package distribution mechanism
   - Plugin marketplace ecosystem

5. **Related ADRs**:
   - **ADR-012**: Defines cap-schema and cap-transport abstractions that enable TAK integration
   - **ADR-009**: Bidirectional flows architecture aligns with TAK ↔ HIVE communication
   - **ADR-006**: Security architecture extends to TAK authentication

## References

1. [TAK.gov](https://tak.gov) - Official TAK Product Center
2. [CoT Developer's Guide](https://tutorials.techrad.co.za/wp-content/uploads/2021/06/The-Developers-Guide-to-Cursor-on-Target-1.pdf)
3. [FreeTakServer](https://github.com/FreeTAKTeam/FreeTakServer) - Open-source TAK server
4. [cottak Rust Library](https://docs.rs/cottak/latest/cottak/) - Rust CoT implementation
5. ATAK Plugin Development Guide (government restricted)
6. TAK Product Center Documentation (SIPR/JWICS)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-17 | Proposed TAK/CoT integration | AUKUS interoperability requirement |
| 2025-11-17 | Selected Hybrid deployment model (Model 3) | Maximum flexibility, aligns with cap-transport |
| 2025-11-17 | Hierarchical filtering mandatory | Prevents O(n²) message explosion |
| 2025-11-26 | Added `_hive_` CoT extension schema (ADR-028) | M1 POC integrator feedback - preserve HIVE semantics |
| 2025-11-26 | Added MIL-STD-2525 entity type mappings | M1 POC integrator feedback - concrete type codes |
| 2025-11-26 | Added QoS→flow-tags mapping | ADR-019 integration for TAK-side prioritization |
| 2025-11-26 | Resolved Q5: No model distribution via TAK | Keep on HIVE blob transport (size limits, hash verification) |
| 2025-11-26 | Resolved Q7: Yes, cells as TAK groups | Natural fit for operator workflow |
| 2025-11-26 | Formation-level track correlation | Hierarchical aggregation before TAK bridge |
| 2025-11-26 | Created ADR-029 for TAK Transport Adapter | DIL message queuing, separate architectural component |
| TBD | Approved/Rejected | After AUKUS stakeholder review |

## Open Questions

### Resolved (M1 POC Feedback - 2025-11-26)

**Q5: Should HIVE AI models distribute via TAK data packages?**
- **Answer**: **No** - Keep model distribution on HIVE's content-addressed blob transport.
- **Rationale**:
  - TAK data packages have size limits (~50MB typical)
  - HIVE's Iroh-based blob transport provides hash verification, resumable transfers
  - Model updates are P5 (bulk) priority - shouldn't compete with tactical data on TAK
  - Separation of concerns: TAK for SA, HIVE for autonomy coordination

**Q7: Should HIVE cells appear as TAK "groups"?**
- **Answer**: **Yes** - Map HIVE cells to TAK contact groups.
- **Rationale**:
  - Natural fit for operators managing multiple teams
  - Enables group messaging to cells
  - Supports TAK's existing group management UI
- **Implementation**:
  ```xml
  <detail>
    <__group name="Alpha-Team" role="Team Member"/>
    <contact callsign="Alpha-Team"/>
  </detail>
  ```

**Q (New): How to handle track correlation across teams?**
- **Answer**: **Formation correlates** (Option 3)
- **Context**: In M1 vignette, Alpha and Bravo teams may independently detect the same POI.
- **Rationale**: Aligns with HIVE's hierarchical aggregation philosophy - coordinator correlates before bridge, single track to TAK.

### Still Open

1. **Should HIVE support TAK federation directly?** Or only single TAK server connections?
2. ~~How do we handle TAK server outages?~~ → Resolved: DIL Message Queuing (see ADR-029)
3. **What is the priority for ATAK plugin vs standalone bridge?** Resource allocation?
4. **Should we support TAK's video streaming features?** Integration with HIVE sensor data?
6. **Do we need CoT→HIVE conversion for all CoT types?** Or subset initially?

## Next Steps

### Immediate Actions (Next 30 Days)

1. **Stakeholder Engagement**:
   - [ ] Brief AUKUS partners on proposed TAK integration
   - [ ] Identify TAK system owners for coordination
   - [ ] Determine Talisman Sabre 2025 integration timeline
   - [ ] Secure access to TAK Server test environments

2. **Technical Prototyping**:
   - [ ] Prototype CoT XML encoder/decoder
   - [ ] Test with FreeTakServer (open-source)
   - [ ] Validate message conversion semantics
   - [ ] Measure latency and bandwidth impact

3. **Requirements Refinement**:
   - [ ] Document critical CoT message types (top 10)
   - [ ] Define filtering rules for hierarchical bridge
   - [ ] Specify security requirements (PKI, certificates)
   - [ ] Establish performance targets

4. **Resource Planning**:
   - [ ] Estimate development effort (Phases 1-5)
   - [ ] Identify TAK subject matter experts
   - [ ] Secure devices for ATAK plugin testing
   - [ ] Plan field validation exercises

### Funding Considerations

**Navy NIWC PAC Proposal**:
- TAK integration addresses Maritime Big Play requirements
- Enables HIVE adoption within existing USN TAK infrastructure
- Demonstrates interoperability with allied systems

**BlackFlag.vc Seed Round**:
- TAK integration is key differentiator for defense customers
- Reduces adoption friction ("works with existing systems")
- Demonstrates understanding of operational environment

---

**Critical Success Factor**: TAK integration must be **demonstrably operational** before major NATO STANAG proposals. Working TAK bridge provides credibility and shows HIVE complements (rather than replaces) existing C2 infrastructure.

**Author's Note**: This ADR represents a strategic integration that enables HIVE adoption within the existing TAK ecosystem prevalent in AUKUS and broader DoD/NATO operations. The hierarchical filtering bridge is essential—naive event forwarding would recreate the O(n²) problem HIVE solves. By treating TAK as a first-class integration target, we position HIVE as complementary infrastructure that enhances existing situational awareness tools rather than competing with them.
