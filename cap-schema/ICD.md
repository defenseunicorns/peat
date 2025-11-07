# CAP Protocol Interface Control Document (ICD)

**Document Number**: CAP-ICD-001
**Version**: 0.0.1
**Status**: DRAFT
**Date**: 2025-11-06
**Classification**: UNCLASSIFIED

---

## Document Control

### Revision History

| Version | Date | Author | Changes | Approval |
|---------|------|--------|---------|----------|
| 0.0.1 | 2025-11-06 | CAP Development Team | Initial release | DRAFT |

### Approval Authority

| Role | Name | Organization | Signature | Date |
|------|------|--------------|-----------|------|
| Technical Lead | TBD | TBD | | |
| Architecture Review Board | TBD | TBD | | |
| Product Owner | TBD | TBD | | |

### Distribution List

- CAP Development Team
- Integration Partners
- Open Source Community (upon public release)

---

## 1. Introduction

### 1.1 Purpose

This Interface Control Document (ICD) defines the message schemas, data formats, and protocol interfaces for the **Capability Aggregation Protocol (CAP)**. It establishes the contractual interface between:

- CAP Protocol implementations (Rust, Python, Java, C++)
- External systems integrating with CAP (ROS2, gRPC, C2 systems)
- Storage backends (Ditto, Automerge, custom implementations)
- Transport layers (HTTP, gRPC, WebSocket, MQTT, ROS2 DDS)

### 1.2 Scope

This ICD covers:

- **Message Schemas**: Protocol Buffer definitions for all CAP messages
- **Data Formats**: Serialization formats, encoding rules, field constraints
- **Interface Specifications**: API contracts for transport adapters and storage backends
- **Version Control**: Schema versioning, backward compatibility rules
- **Semantic Constraints**: CRDT operations, validation rules, ontology

This ICD does **not** cover:

- Internal implementation details of specific CAP libraries
- Transport-specific protocols (covered by transport specifications)
- Storage backend internals (covered by storage specifications)

### 1.3 Applicable Documents

| Document ID | Title | Version | Date |
|-------------|-------|---------|------|
| ADR-012 | Schema Definition and Protocol Extensibility | 1.0 | 2025-11-06 |
| ADR-011 | CRDT + Networking Stack Selection | 1.0 | 2025-11-06 |
| SCHEMAS.md | CAP Protocol Schema Reference | 1.0 | 2025-11-06 |
| README.md | cap-schema Documentation | 1.0 | 2025-11-06 |

### 1.4 Points of Contact

| Role | Organization | Email | Phone |
|------|--------------|-------|-------|
| Technical Lead | TBD | TBD | TBD |
| Schema Working Group Chair | TBD | TBD | TBD |
| Integration Support | TBD | TBD | TBD |

---

## 2. System Overview

### 2.1 CAP Protocol Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    External Systems                          │
│  (ROS2 Robots, C2 Systems, Python Clients, Java Services)  │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      │ Uses: Protobuf Schemas (ICD-controlled)
                      ↓
┌─────────────────────────────────────────────────────────────┐
│                   cap-schema (This ICD)                      │
│                                                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ common   │  │capability│  │   node   │  │   cell   │   │
│  │   .v1    │  │   .v1    │  │   .v1    │  │   .v1    │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  zone    │  │   role   │  │  beacon  │  │composition│   │
│  │   .v1    │  │   .v1    │  │   .v1    │  │   .v1    │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      │ Implemented by:
                      ↓
┌─────────────────────────────────────────────────────────────┐
│              CAP Protocol Implementations                    │
│    (cap-protocol, cap-transport, cap-persistence)           │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Three-Tier Hierarchy

CAP implements a three-tier organizational hierarchy:

1. **Nodes** (Tier 1): Individual platforms (UAVs, UGVs, soldier systems)
2. **Cells** (Tier 2): Tactical squads (2-8 nodes with complementary capabilities)
3. **Zones** (Tier 3): Strategic coordination (multiple cells under zone commander)

### 2.3 Protocol Phases

1. **Discovery Phase**: Nodes broadcast beacons, discover neighbors
2. **Cell Formation Phase**: Nodes form cells based on capability composition
3. **Hierarchical Operations Phase**: Cells coordinate within zones

---

## 3. Interface Specifications

### 3.1 Schema Packages

All schemas are defined using **Protocol Buffers v3** syntax.

#### 3.1.1 Package Naming Convention

```
cap.<domain>.v<major_version>
```

Examples:
- `cap.common.v1` - Common types
- `cap.node.v1` - Node schemas
- `cap.cell.v2` - Cell schemas (future major version)

#### 3.1.2 Schema Inventory

| Package | File | Purpose | Message Count | Status |
|---------|------|---------|---------------|--------|
| `cap.common.v1` | `common.proto` | Foundation types | 5 | STABLE |
| `cap.capability.v1` | `capability.proto` | Capabilities | 4 | STABLE |
| `cap.node.v1` | `node.proto` | Nodes & operators | 8 | STABLE |
| `cap.cell.v1` | `cell.proto` | Cell formation | 6 | STABLE |
| `cap.zone.v1` | `zone.proto` | Zone hierarchy | 7 | STABLE |
| `cap.role.v1` | `role.proto` | Tactical roles | 7 | STABLE |
| `cap.beacon.v1` | `beacon.proto` | Discovery | 4 | STABLE |
| `cap.composition.v1` | `composition.proto` | Composition rules | 4 | STABLE |

**Total**: 8 packages, 45 message types

### 3.2 Data Type Specifications

#### 3.2.1 Common Types (`cap.common.v1`)

```protobuf
// Unique identifier (UUID v4)
message Uuid {
  string value = 1;  // Format: "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx"
}

// Timestamp (Unix epoch)
message Timestamp {
  uint64 seconds = 1;  // Seconds since 1970-01-01T00:00:00Z
  uint32 nanos = 2;    // Nanoseconds (0-999,999,999)
}

// Geographic position (WGS84)
message Position {
  double latitude = 1;   // Degrees, range: [-90.0, 90.0]
  double longitude = 2;  // Degrees, range: [-180.0, 180.0]
  double altitude = 3;   // Meters above sea level
}

// Confidence score
message Confidence {
  float value = 1;  // Range: [0.0, 1.0]
}

// Generic metadata
message Metadata {
  map<string, string> fields = 1;
}
```

**Constraints**:
- `Uuid.value`: MUST match UUID v4 format (RFC 4122)
- `Timestamp.seconds`: MUST be non-negative
- `Timestamp.nanos`: MUST be in range [0, 999999999]
- `Position.latitude`: MUST be in range [-90.0, 90.0]
- `Position.longitude`: MUST be in range [-180.0, 180.0]
- `Confidence.value`: MUST be in range [0.0, 1.0]

#### 3.2.2 Enumerations

All enumerations MUST include an `_UNSPECIFIED = 0` value per Protobuf best practices.

**Example**:
```protobuf
enum CapabilityType {
  CAPABILITY_TYPE_UNSPECIFIED = 0;  // Default/unknown
  CAPABILITY_TYPE_SENSOR = 1;
  CAPABILITY_TYPE_COMPUTE = 2;
  CAPABILITY_TYPE_COMMUNICATION = 3;
  CAPABILITY_TYPE_MOBILITY = 4;
  CAPABILITY_TYPE_PAYLOAD = 5;
  CAPABILITY_TYPE_EMERGENT = 6;
}
```

### 3.3 Message Size Limits

| Message Type | Max Size | Rationale |
|--------------|----------|-----------|
| `Beacon` | 10 KB | Must fit in single UDP packet for efficiency |
| `NodeConfig` | 50 KB | Support up to 100 capabilities per node |
| `CellState` | 100 KB | Support max 8 members with full capabilities |
| `ZoneState` | 500 KB | Support max 10 cells with aggregated data |
| `CompositionResult` | 100 KB | Support complex rule sets |

**Enforcement**: Implementations SHOULD validate message sizes and reject oversized messages.

---

## 4. Versioning and Compatibility

### 4.1 Semantic Versioning

Schema versions follow **semantic versioning** (SemVer 2.0.0):

```
<major>.<minor>.<patch>
```

- **Major**: Breaking changes (incompatible API changes)
- **Minor**: Backward-compatible functionality additions
- **Patch**: Backward-compatible bug fixes

### 4.2 Backward Compatibility Rules

#### 4.2.1 MUST NOT (Breaking Changes)

1. ❌ Remove a field
2. ❌ Change a field number
3. ❌ Change a field type
4. ❌ Rename a package
5. ❌ Remove or rename an enum value

#### 4.2.2 MUST (Compatibility Preservation)

1. ✅ Mark deprecated fields with `[deprecated = true]`
2. ✅ Add new optional fields only
3. ✅ Provide default values for all new fields
4. ✅ Document migration path for deprecated features

#### 4.2.3 Example: Field Deprecation

```protobuf
message NodeConfig {
  string id = 1;

  // DEPRECATED: Use capabilities_v2 instead (since v1.5.0)
  repeated Capability capabilities = 2 [deprecated = true];

  // New field with improved design (added v1.5.0)
  repeated CapabilityV2 capabilities_v2 = 3;
}
```

### 4.3 Version Migration

When creating a new major version (e.g., v1 → v2):

1. **Create new package**: `cap.node.v2`
2. **Maintain parallel support**: Keep v1 and v2 for minimum 6 months
3. **Provide migration utilities**: Code-generated converters
4. **Document breaking changes**: CHANGELOG.md with migration guide
5. **Deprecation notice**: Mark v1 as deprecated with sunset date

**Example Migration Utility**:
```rust
impl From<cap::node::v1::NodeConfig> for cap::node::v2::NodeConfig {
    fn from(v1: cap::node::v1::NodeConfig) -> Self {
        // Migration logic
    }
}
```

---

## 5. CRDT Semantics

CAP Protocol uses **Conflict-free Replicated Data Types (CRDTs)** for distributed consistency.

### 5.1 CRDT Operations by Schema

| Schema | Field | CRDT Type | Operations |
|--------|-------|-----------|------------|
| `node.v1` | `capabilities` | G-Set | Add only (monotonic) |
| `node.v1` | `position`, `health`, `phase` | LWW-Register | Last-write-wins (timestamp) |
| `node.v1` | `fuel_minutes` | PN-Counter | Increment/decrement |
| `cell.v1` | `leader_id` | LWW-Register | Last-write-wins |
| `cell.v1` | `members` | OR-Set | Add wins over remove |
| `cell.v1` | `capabilities` | G-Set | Add only (aggregated) |
| `zone.v1` | `coordinator_id` | LWW-Register | Last-write-wins |
| `zone.v1` | `cells` | OR-Set | Add wins over remove |
| `zone.v1` | `aggregated_capabilities` | G-Set | Add only |

### 5.2 Conflict Resolution Rules

#### 5.2.1 Last-Write-Wins (LWW-Register)

**Rule**: Compare `timestamp` field; highest timestamp wins.

**Example**:
```protobuf
message NodeState {
  Position position = 1;
  Timestamp timestamp = 7;  // Used for LWW resolution
}
```

**Implementation**:
```rust
pub fn merge(&mut self, other: &NodeState) {
    if other.timestamp > self.timestamp {
        self.position = other.position.clone();
        self.timestamp = other.timestamp.clone();
    }
}
```

#### 5.2.2 Observed-Remove Set (OR-Set)

**Rule**: Add operation wins over concurrent remove operation.

**Example**:
```protobuf
message CellState {
  repeated string members = 3;  // OR-Set
  Timestamp timestamp = 6;
}
```

#### 5.2.3 Grow-Only Set (G-Set)

**Rule**: Elements can only be added, never removed.

**Example**:
```protobuf
message NodeConfig {
  repeated Capability capabilities = 3;  // G-Set
}
```

---

## 6. Validation Rules

### 6.1 Required Field Validation

All implementations MUST validate:

1. **Non-empty strings**: `id`, `name`, `platform_type` fields MUST NOT be empty
2. **Range constraints**: See Section 3.2.1 for numeric ranges
3. **Enum values**: MUST be valid enum members (not UNSPECIFIED unless documented)
4. **Timestamps**: MUST be non-negative and reasonable (not far future)

### 6.2 Semantic Validation

| Validation | Rule | Error Code |
|------------|------|------------|
| Cell size | `members.len() <= config.max_size` | `CELL_FULL` |
| Cell validity | `members.len() >= config.min_size` | `CELL_INVALID` |
| Leader in members | `leader_id` MUST be in `members` | `INVALID_LEADER` |
| Confidence range | `0.0 <= confidence <= 1.0` | `INVALID_CONFIDENCE` |
| Position validity | Lat in [-90, 90], Lon in [-180, 180] | `INVALID_POSITION` |

### 6.3 Example Validation Code

```rust
pub fn validate_cell_state(state: &CellState) -> Result<(), ValidationError> {
    // Check leader is in members
    if let Some(leader_id) = &state.leader_id {
        if !state.members.contains(leader_id) {
            return Err(ValidationError::InvalidLeader);
        }
    }

    // Check size constraints
    if let Some(config) = &state.config {
        if state.members.len() > config.max_size as usize {
            return Err(ValidationError::CellFull);
        }
    }

    Ok(())
}
```

---

## 7. Change Control Process

### 7.1 Schema Change Request (SCR)

All schema changes MUST follow this process:

1. **Proposal**: Submit SCR via GitHub issue with template
2. **Review**: Schema Working Group reviews (1 week)
3. **Comment Period**: Public comment period (2 weeks for major changes)
4. **Approval**: Architecture Review Board approves
5. **Implementation**: Update `.proto` files, regenerate code
6. **Testing**: Backward compatibility tests pass
7. **Release**: Publish new version with CHANGELOG

### 7.2 SCR Template

```markdown
## Schema Change Request

**Type**: [ ] Minor (backward compatible) [ ] Major (breaking change)
**Affected Package**: cap.<package>.v<version>
**Proposed Version**: <new version>

### Motivation
Why is this change needed?

### Proposed Changes
- [ ] Add field X to message Y
- [ ] Deprecate field Z

### Backward Compatibility Impact
How does this affect existing implementations?

### Migration Path
How should users migrate from old to new version?

### Testing Plan
How will backward compatibility be verified?
```

### 7.3 Approval Authority

| Change Type | Approval Required |
|-------------|-------------------|
| Patch (bug fix) | Technical Lead |
| Minor (new optional field) | Schema Working Group |
| Major (breaking change) | Architecture Review Board + Public Comment |

---

## 8. Implementation Requirements

### 8.1 Conformance Levels

Implementations MUST support one of these conformance levels:

#### 8.1.1 Level 1: Core Schemas (REQUIRED)

- `cap.common.v1` ✅
- `cap.capability.v1` ✅
- `cap.node.v1` ✅
- `cap.beacon.v1` ✅

#### 8.1.2 Level 2: Cell Formation (RECOMMENDED)

- Level 1 +
- `cap.cell.v1` ✅
- `cap.composition.v1` ✅

#### 8.1.3 Level 3: Full Hierarchy (OPTIONAL)

- Level 2 +
- `cap.zone.v1` ✅
- `cap.role.v1` ✅

### 8.2 Code Generation Requirements

All implementations MUST:

1. ✅ Use official Protocol Buffers compiler (`protoc`)
2. ✅ Generate code from `.proto` files (no manual schemas)
3. ✅ Include validation utilities
4. ✅ Support JSON encoding (for HTTP/REST APIs)
5. ✅ Support binary encoding (for gRPC/efficient transport)

### 8.3 Testing Requirements

All implementations MUST pass:

1. ✅ **Schema validation tests**: All required fields present
2. ✅ **Round-trip serialization**: `deserialize(serialize(msg)) == msg`
3. ✅ **Backward compatibility tests**: v1 messages readable by v1.x parsers
4. ✅ **Forward compatibility tests**: v1.x messages readable by v1 parsers (with defaults)

---

## 9. Security Considerations

### 9.1 Input Validation

All implementations MUST validate:

1. ✅ Message size limits (prevent DoS)
2. ✅ String length limits (prevent buffer overflows)
3. ✅ Numeric ranges (prevent invalid states)
4. ✅ Enum values (reject unknown values)

### 9.2 Sanitization

Implementations SHOULD sanitize:

1. ✅ User-provided strings (strip control characters)
2. ✅ Metadata fields (validate key-value pairs)
3. ✅ File paths (prevent directory traversal)

### 9.3 Encryption

- **At Rest**: Protobuf messages MAY be encrypted by storage backend
- **In Transit**: Protobuf messages SHOULD be encrypted via TLS/mTLS

---

## 10. Appendices

### Appendix A: Glossary

| Term | Definition |
|------|------------|
| CRDT | Conflict-free Replicated Data Type |
| ICD | Interface Control Document |
| G-Set | Grow-only Set (CRDT) |
| LWW | Last-Write-Wins (CRDT) |
| OR-Set | Observed-Remove Set (CRDT) |
| PN-Counter | Positive-Negative Counter (CRDT) |
| SCR | Schema Change Request |
| WGS84 | World Geodetic System 1984 (GPS coordinate system) |

### Appendix B: References

- **Protocol Buffers**: https://protobuf.dev/
- **SemVer 2.0**: https://semver.org/
- **RFC 4122**: UUID Specification
- **CRDT Theory**: https://crdt.tech/
- **ADR-012**: Schema Definition and Protocol Extensibility

### Appendix C: Schema Source Files

All official schema files are maintained at:

```
https://github.com/<org>/cap/tree/main/cap-schema/proto/
```

- `common.proto`
- `capability.proto`
- `node.proto`
- `cell.proto`
- `zone.proto`
- `role.proto`
- `beacon.proto`
- `composition.proto`

### Appendix D: License

This ICD and all associated schema files are released under:

**License**: Apache 2.0 (or as determined by governing foundation)

**Copyright**: © 2025 CAP Protocol Contributors

---

## Document End

**Document Number**: CAP-ICD-001
**Version**: 1.0.0
**Status**: DRAFT
**Next Review Date**: 2026-02-06 (3 months)
