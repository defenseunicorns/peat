# HIVE Protocol Buffer Definitions

This directory contains the **normative** Protocol Buffer schema definitions for the HIVE Protocol.

## License

These Protocol Buffer definitions are released under [CC0 1.0 Universal (Public Domain Dedication)](https://creativecommons.org/publicdomain/zero/1.0/).

You are free to:
- Implement these schemas in any language
- Use them in commercial or non-commercial projects
- Modify and redistribute without restriction
- No attribution required (though appreciated)

## Schema Overview

### Core Types (`cap.common.v1`)

| Message | Purpose |
|---------|---------|
| `Uuid` | Unique identifier wrapper |
| `Timestamp` | Unix epoch timestamp with nanosecond precision |
| `Position` | WGS84 geographic coordinates |
| `Confidence` | Normalized confidence score [0.0, 1.0] |
| `Metadata` | Generic key-value pairs |

### Node Model (`cap.node.v1`)

| Message/Enum | Purpose |
|--------------|---------|
| `Node` | Complete node representation |
| `NodeConfig` | Static platform configuration |
| `NodeState` | Dynamic platform state (CRDT-backed) |
| `Operator` | Human operator information |
| `HumanMachinePair` | Human-machine teaming binding |
| `Phase` | Protocol phase (Discovery, Cell, Hierarchy) |
| `HealthStatus` | Platform health enumeration |
| `OperatorRank` | Military rank mapping |
| `AuthorityLevel` | Human authority levels |

### Capability Model (`cap.capability.v1`)

| Message/Enum | Purpose |
|--------------|---------|
| `Capability` | Individual platform capability |
| `CapabilityType` | Capability classification |
| `CapabilityQuery` | Discovery query filter |
| `CapabilityResponse` | Query response |

### Cell Model (`cap.cell.v1`)

| Message | Purpose |
|---------|---------|
| `Cell` | Squad-level formation |
| `CellConfig` | Cell configuration |
| `CellState` | Cell dynamic state |
| `CellFormationRequest/Response` | Formation protocol |
| `CellMembershipChange` | Membership events |

### Discovery (`cap.beacon.v1`)

| Message | Purpose |
|---------|---------|
| `Beacon` | Discovery broadcast message |
| `BeaconQuery` | Beacon filter criteria |
| `BeaconQueryResponse` | Query results |
| `BeaconRecord` | Persistence wrapper |

### Composition (`cap.composition.v1`)

| Message/Enum | Purpose |
|--------------|---------|
| `CompositionRule` | Capability composition definition |
| `CompositionRuleType` | Rule types (Additive, Emergent, Redundant, Constraint) |
| `CompositionResult` | Composition output |

### Hierarchy (`cap.hierarchy.v1`)

| Message | Purpose |
|---------|---------|
| `SquadSummary` | Squad-level aggregation |
| `PlatoonSummary` | Platoon-level aggregation |
| `CompanySummary` | Company-level aggregation |
| `BoundingBox` | Spatial extent |
| `AggregationMetadata` | Aggregation provenance |

### Command (`cap.command.v1`)

| Message/Enum | Purpose |
|--------------|---------|
| `HierarchicalCommand` | Command dissemination |
| `CommandTarget` | Target specification |
| `CommandPriority` | Priority levels |
| `MissionOrder` | Mission commands |
| `EngagementOrder` | Engagement commands |
| `FormationChange` | Formation commands |
| `CommandAcknowledgment` | Acknowledgment flow |
| Policy enums | Configurable behavior |

## CRDT Semantics

The schema documents which CRDT types back each field:

| Annotation | CRDT Type | Semantics |
|------------|-----------|-----------|
| `LWW-Register` | Last-Writer-Wins Register | Most recent timestamp wins |
| `G-Set` | Grow-only Set | Add-only, no removal |
| `OR-Set` | Observed-Remove Set | Add and remove with tombstones |
| `PN-Counter` | Positive-Negative Counter | Increment and decrement |

## Code Generation

### Rust (with prost)

```toml
[build-dependencies]
prost-build = "0.13"
```

```rust
// build.rs
fn main() {
    prost_build::compile_protos(
        &["spec/proto/cap/v1/node.proto"],
        &["spec/proto/"]
    ).unwrap();
}
```

### Python (with grpcio-tools)

```bash
python -m grpc_tools.protoc \
    -I spec/proto \
    --python_out=. \
    --grpc_python_out=. \
    spec/proto/cap/v1/*.proto
```

### Go (with protoc-gen-go)

```bash
protoc \
    -I spec/proto \
    --go_out=. \
    --go_opt=paths=source_relative \
    spec/proto/cap/v1/*.proto
```

### Java (with protoc)

```bash
protoc \
    -I spec/proto \
    --java_out=src/main/java \
    spec/proto/cap/v1/*.proto
```

## Versioning

Schemas are versioned via package namespace:
- `cap.*.v1` - Version 1 (current)
- `cap.*.v2` - Version 2 (future, breaking changes)

Within a major version, only backward-compatible changes are permitted:
- Adding new fields (with new field numbers)
- Adding new enum values
- Adding new messages

Breaking changes require a new major version.

## Validation Requirements

Implementations MUST validate:

1. **UUID format**: `id` fields MUST be valid UUID v4
2. **Confidence range**: Values MUST be in [0.0, 1.0]
3. **Position validity**: Latitude [-90, 90], Longitude [-180, 180]
4. **Timestamp monotonicity**: For LWW fields, timestamps MUST increase
5. **Required fields**: Fields marked in spec as REQUIRED MUST be present

## Wire Format

All messages use Protocol Buffers binary encoding (proto3 syntax). Implementations:
- MUST support proto3 binary format
- MAY support JSON encoding for debugging/interoperability
- MUST preserve unknown fields for forward compatibility
