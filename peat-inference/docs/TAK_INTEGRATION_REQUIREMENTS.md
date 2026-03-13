# TAK Integration Requirements for Peat Protocol

**Document Type**: Requirements & Recommendations for ADR-020
**Date**: 2025-11-26
**Source**: peat-m1-poc implementation experience
**Target**: Peat Core Team (cap repository)

## Executive Summary

This document captures requirements and recommendations derived from implementing the M1 vignette (object tracking across distributed human-machine-AI teams). These findings should inform ADR-020 implementation and potentially spawn new ADRs.

---

## 1. Schema Layer Requirements (cap-schema)

### 1.1 Peat Custom Detail Extension Schema

**Requirement**: Define a standardized `<_peat_>` CoT detail extension for preserving Peat-specific semantics in CoT messages.

**Rationale**: Peat messages contain rich context (model versions, confidence scores, cell membership) that TAK operators need but CoT doesn't natively support.

**Proposed Schema**:

```xml
<_peat_ version="1.0" xmlns:peat="urn:peat:cot:1.0">
  <!-- Source attribution -->
  <source platform="Alpha-2" model="Alpha-3" model_version="1.3.0"/>

  <!-- Track confidence -->
  <confidence value="0.89" threshold="0.70"/>

  <!-- Hierarchy membership -->
  <hierarchy>
    <cell id="Alpha-Team" role="tracker"/>
    <formation id="Formation-1"/>
    <zone id="Zone-A"/>
  </hierarchy>

  <!-- Custom attributes (pass-through from TrackUpdate.attributes) -->
  <attributes>
    <attr key="jacket_color" type="string">blue</attr>
    <attr key="has_backpack" type="boolean">true</attr>
    <attr key="estimated_age" type="string">adult</attr>
  </attributes>

  <!-- Operational status (for capability advertisements) -->
  <status operational="ACTIVE" readiness="0.91"/>
</_peat_>
```

**Action Items**:
- [ ] Add `_peat_` XSD schema to cap-schema
- [ ] Document extension in CoT compatibility guide
- [ ] Register namespace with TAK ecosystem maintainers (if applicable)

### 1.2 MIL-STD-2525 Symbol Type Mappings

**Requirement**: Define canonical CoT type codes for Peat entity classes.

**Proposed Mappings**:

| Peat Entity | CoT Type | MIL-STD-2525 Description |
|-------------|----------|--------------------------|
| Tracked Person (POI) | `a-f-G-E-S` | Friendly Ground Equipment - Sensor |
| Tracked Vehicle | `a-f-G-E-V` | Friendly Ground Equipment - Vehicle |
| Unknown Track | `a-u-G` | Unknown Ground |
| Hostile Track | `a-h-G` | Hostile Ground |
| Peat Platform (UGV) | `a-f-G-U-C` | Friendly Ground Unit - Combat |
| Peat Platform (UAV) | `a-f-A-M-F-Q` | Friendly Air - Military Fixed Wing - UAV |
| Peat Operator | `a-f-G-U-C-I` | Friendly Ground Unit - Infantry |
| Peat Cell/Team | `a-f-G-U-C` + links | Friendly Ground Unit with subordinates |
| Formation | `a-f-G-U-C` + links | Higher echelon unit |
| Geofence/ROZ | `u-d-r` | Drawing - Route/Area |
| Mission Tasking | `t-x-m` | Tasking - Mission |

**Action Items**:
- [ ] Add `CotTypeMapper` trait to cap-schema
- [ ] Implement default mappings with override capability
- [ ] Support affiliation inference from track context

### 1.3 Hierarchy Encoding in CoT Links

**Requirement**: Standardize how Peat's hierarchical relationships map to CoT `<link>` elements.

**Proposed Convention**:

```xml
<!-- Platform belongs to cell -->
<link uid="Alpha-Team" type="a-f-G-U-C" relation="p-p" remarks="parent-cell"/>

<!-- Cell belongs to formation -->
<link uid="Formation-1" type="a-f-G-U-C" relation="p-p" remarks="parent-formation"/>

<!-- Track handoff relationship -->
<link uid="Bravo-Team" type="a-f-G-U-C" relation="h-h" remarks="handoff-target"/>
```

**Relation Types**:
| Relation | Meaning |
|----------|---------|
| `p-p` | Parent (hierarchical ownership) |
| `h-h` | Handoff (track transfer) |
| `s-s` | Sibling (same echelon) |
| `o-o` | Observing (sensor relationship) |

**Action Items**:
- [ ] Document relation type conventions
- [ ] Add `HierarchyEncoder` to cap-schema CoT module

---

## 2. Transport Layer Requirements (cap-transport)

### 2.1 TAK Transport Adapter Interface

**Requirement**: Add `TakTransport` as a first-class transport adapter alongside HTTP.

**Proposed Trait**:

```rust
#[async_trait]
pub trait TakTransport: Send + Sync {
    /// Connect to TAK server or mesh
    async fn connect(&mut self) -> Result<(), TakError>;

    /// Disconnect gracefully
    async fn disconnect(&mut self) -> Result<(), TakError>;

    /// Send CoT event to TAK
    async fn send_cot(&self, event: &CotEvent) -> Result<(), TakError>;

    /// Subscribe to incoming CoT events
    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError>;

    /// Check connection health
    fn is_connected(&self) -> bool;

    /// Get connection metrics
    fn metrics(&self) -> TakMetrics;
}
```

**Configuration**:

```rust
pub struct TakTransportConfig {
    /// TAK server address (for server mode)
    pub server_address: Option<SocketAddr>,

    /// Multicast group (for mesh SA mode)
    pub multicast_group: Option<IpAddr>,

    /// Client certificate for authentication
    pub client_cert: Option<PathBuf>,
    pub client_key: Option<PathBuf>,
    pub ca_cert: Option<PathBuf>,

    /// Protocol version
    pub protocol: TakProtocol,

    /// Reconnection policy
    pub reconnect_policy: ReconnectPolicy,

    /// Message queue size for buffering
    pub queue_size: usize,
}

pub enum TakProtocol {
    /// CoT XML over TCP
    XmlTcp,
    /// CoT XML over TCP with TLS
    XmlTcpSsl,
    /// TAK Protocol v1 (Protobuf)
    ProtobufV1,
    /// Mesh SA UDP multicast
    MeshSa,
}
```

**Action Items**:
- [ ] Add `TakTransport` trait to cap-transport
- [ ] Evaluate `cottak` crate as dependency vs. custom implementation
- [ ] Implement TAK Server TCP/SSL client
- [ ] Implement Mesh SA UDP multicast support

### 2.2 Message Queuing for DIL Environments

**Requirement**: Buffer outgoing CoT messages when TAK connection is unavailable.

**Rationale**: M1 vignette operates in contested/DIL environments where TAK server connectivity may be intermittent.

**Proposed Behavior**:
1. Queue messages when disconnected (up to configurable limit)
2. Replay queued messages on reconnection (with staleness filtering)
3. Prioritize by Peat priority level (P1 messages first)
4. Drop stale messages (past CoT `stale` time)

**Action Items**:
- [ ] Add `MessageQueue` to TakTransport
- [ ] Implement priority-aware queue draining
- [ ] Add metrics for queue depth and dropped messages

---

## 3. Protocol Layer Requirements (cap-protocol)

### 3.1 Aggregation Policy Configuration

**Requirement**: Make hierarchical filtering policies configurable per-deployment.

**Current ADR-020 Policies**:
- `FullFidelity` - All platforms visible (O(n) bandwidth)
- `SquadLeaderOnly` - Only cell leaders visible
- `HierarchicalFiltering` - Echelon-appropriate detail

**Additional Policies Needed**:

```rust
pub enum AggregationPolicy {
    // ... existing policies ...

    /// Track-focused: Only active tracks visible, not platforms
    TracksOnly,

    /// Capability-focused: Formation capabilities, not positions
    CapabilitySummaryOnly,

    /// Time-windowed: Aggregate updates over N seconds
    TimeWindowed { window_secs: u32 },

    /// Bandwidth-adaptive: Adjust based on link quality
    BandwidthAdaptive { target_kbps: u32 },
}
```

**Action Items**:
- [ ] Expand `AggregationPolicy` enum
- [ ] Add runtime policy switching
- [ ] Implement bandwidth monitoring for adaptive mode

### 3.2 Priority to CoT Flow-Tags Mapping

**Requirement**: Map Peat QoS priorities (ADR-019) to CoT `_flow-tags_` for TAK-side prioritization.

**Proposed Mapping**:

| Peat Priority | CoT Flow-Tag | TAK Behavior |
|---------------|--------------|--------------|
| P1 (Critical) | `priority=flash` | Immediate delivery |
| P2 (High) | `priority=immediate` | High priority |
| P3 (Normal) | `priority=routine` | Normal delivery |
| P4 (Low) | `priority=deferred` | Best effort |
| P5 (Bulk) | `priority=bulk` | Background |

**Action Items**:
- [ ] Add flow-tag encoding to CoT encoder
- [ ] Verify TAK server honors flow-tags
- [ ] Document priority semantics for operators

### 3.3 Operational Status Representation

**Requirement**: Define how `OperationalStatus` maps to CoT for capability visibility.

**Proposed Approach**:

```xml
<!-- Platform capability advertisement -->
<event uid="Alpha-3" type="a-f-G-U-C" ...>
  <point lat="33.7749" lon="-84.3958" .../>
  <detail>
    <_peat_>
      <status operational="ACTIVE" readiness="0.91"/>
      <capability type="OBJECT_TRACKING"
                  model_version="1.3.0"
                  precision="0.94"
                  status="ACTIVE"/>
    </_peat_>
    <remarks>AI Platform: object_tracker v1.3.0 (Active, 91% ready)</remarks>
  </detail>
</event>
```

**Status Values**:
| Peat Status | CoT Representation | TAK Display |
|-------------|-------------------|-------------|
| `Ready` | `operational="READY"` | Green indicator |
| `Active` | `operational="ACTIVE"` | Blue/active indicator |
| `Degraded` | `operational="DEGRADED"` | Yellow/warning |
| `Offline` | `operational="OFFLINE"` | Red/offline |
| `Loading` | `operational="LOADING"` | Gray/transitioning |

**Action Items**:
- [ ] Add status encoding to capability CoT events
- [ ] Consider TAK plugin for custom status rendering

---

## 4. Security Requirements

### 4.1 Command Authentication

**Requirement**: Cryptographically verify commands received from TAK before execution.

**Rationale**: CoT mission tasking (`t-x-m`) received via TAK could be spoofed. Peat must verify command authority.

**Proposed Approach**:
1. TAK client certificate identity maps to Peat authority level
2. Commands require valid certificate from authorized source
3. Maintain allowlist of authorized TAK users/certificates
4. Log all command sources for audit

**Action Items**:
- [ ] Integrate with ADR-006 authority model
- [ ] Add certificate-to-authority mapping configuration
- [ ] Implement command audit logging

### 4.2 Track Data Classification

**Requirement**: Support marking tracks with classification levels that persist through CoT translation.

**Proposed Extension**:

```xml
<_peat_>
  <classification level="UNCLASSIFIED" caveat="FOUO"/>
</_peat_>
```

**Action Items**:
- [ ] Add classification field to TrackUpdate (if not present)
- [ ] Encode in CoT `_peat_` extension
- [ ] Validate TAK transport supports classification handling

---

## 5. Open Questions for ADR-020

These questions from ADR-020 warrant resolution based on M1 POC experience:

### Q5: Should Peat AI models distribute via TAK data packages?

**Recommendation**: No, keep model distribution on Peat's content-addressed blob transport.

**Rationale**:
- TAK data packages have size limits (~50MB typical)
- Peat's Iroh-based blob transport provides hash verification, resumable transfers
- Model updates are P5 (bulk) priority - shouldn't compete with tactical data on TAK
- Keep separation of concerns: TAK for SA, Peat for autonomy coordination

**Action**: Add explicit statement to ADR-020 that model distribution remains Peat-internal.

### Q7: Should Peat cells appear as TAK "groups"?

**Recommendation**: Yes, map Peat cells to TAK contact groups.

**Rationale**:
- Natural fit for operators managing multiple teams
- Enables group messaging to cells
- Supports TAK's existing group management UI

**Implementation**:
```xml
<detail>
  <__group name="Alpha-Team" role="Team Member"/>
  <contact callsign="Alpha-Team"/>
</detail>
```

**Action**: Add TAK group mapping to bridge implementation.

### Additional Question: How to handle track correlation across teams?

**Context**: In M1 vignette, Alpha and Bravo teams may independently detect the same POI. Should bridge correlate before sending to TAK?

**Options**:
1. **Bridge correlates**: Single track UID in TAK, sources noted in `_peat_`
2. **TAK correlates**: Multiple tracks with same description, operator correlates
3. **Formation correlates**: Coordinator correlates before bridge, single track to TAK

**Recommendation**: Option 3 (Formation correlates). This aligns with Peat's hierarchical aggregation philosophy.

---

## 6. Proposed New ADRs

Based on M1 POC findings, recommend creating:

### ADR-0XX: CoT Custom Detail Extension Schema

**Scope**: Define the `_peat_` XML namespace and schema for embedding Peat metadata in CoT messages.

**Why Separate ADR**: This is a contract with external systems (TAK ecosystem) and warrants dedicated documentation and versioning.

### ADR-0XX: TAK Transport Adapter

**Scope**: Define the `TakTransport` trait, configuration model, and implementation requirements.

**Why Separate ADR**: Transport adapters are significant architectural components. HTTP and TAK have different enough semantics to warrant separate treatment.

### ADR-0XX: Track Correlation and Deduplication

**Scope**: Define how Peat correlates tracks from multiple sources before external publication.

**Why Separate ADR**: Affects both internal Peat behavior and external representations in TAK.

---

## 7. Implementation Priority

Based on M1 vignette requirements:

### Phase 1 (MVP for M1)
1. CoT encoder for `TrackUpdate` → CoT Event
2. CoT encoder for `CapabilityAdvertisement` → CoT Event
3. Basic TAK Server TCP connection
4. `_peat_` detail extension (minimal)

### Phase 2 (Full M1)
1. CoT decoder for `t-x-m` → `TrackCommand`
2. CoT decoder for `u-d-r` → `OperationalBoundary`
3. Handoff message encoding with links
4. TAK SSL/certificate authentication

### Phase 3 (Production)
1. Mesh SA UDP multicast
2. Bandwidth-adaptive aggregation
3. Message queuing for DIL
4. Full status/capability representation

---

## 8. Testing Requirements

### Integration Test Environment

1. **FreeTakServer** - Open-source TAK server for development
2. **ATAK emulator** - Android emulator with ATAK for UI testing
3. **Network simulation** - Inject latency/loss for DIL testing

### Test Cases

| Test | Description | Success Criteria |
|------|-------------|------------------|
| TC-01 | TrackUpdate → CoT → ATAK | Track visible on ATAK map within 2s |
| TC-02 | ATAK mission task → TrackCommand | Command received by Peat team |
| TC-03 | Capability advertisement | Platform capabilities visible in ATAK |
| TC-04 | Track handoff | Handoff link visible, track transfers |
| TC-05 | DIL resilience | Messages queue, replay on reconnect |
| TC-06 | Certificate auth | Unauthorized commands rejected |

---

## References

- [ADR-020: TAK-CoT Integration](../../../cap/docs/adr/020-TAK-CoT-Integration.md)
- [M1 Vignette Use Case](./Peat-Vignette-M1/VIGNETTE_USE_CASE.md)
- [CoT Schema Mapping](./COT_SCHEMA_MAPPING.md) (companion document)
- [cottak crate](https://docs.rs/cottak/latest/cottak/)
- [CoT Developer's Guide](https://tutorials.techrad.co.za/wp-content/uploads/2021/06/The-Developers-Guide-to-Cursor-on-Target-1.pdf)
