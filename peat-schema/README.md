# peat-schema

Protocol Buffer message definitions for the Capability Aggregation Protocol (CAP).

## Overview

This crate provides schema-first message definitions that enable:

- **Multi-transport support**: HTTP, gRPC, ROS2, WebSocket, MQTT
- **Multi-language integration**: Rust, Python, Java, C++, JavaScript
- **Schema versioning**: Backward compatibility guarantees
- **Code generation**: Automatic bindings for all supported languages

## Quick Reference

📖 **[Complete Schema Reference](SCHEMAS.md)** - Detailed documentation for all 8 protobuf schemas

## Message Packages

### Core Types (1 schema)

**`cap.common.v1`** - Common types used across all messages
- `Position`, `Timestamp`, `Uuid`, `Metadata`

### Entity Schemas (2 schemas)

**`cap.capability.v1`** - Capability definitions and queries
- `Capability`, `CapabilityType`, `CapabilityQuery`, `CapabilityResponse`

**`cap.node.v1`** - Node configuration and state
- `NodeConfig`, `NodeState`, `Node`
- `Operator`, `HumanMachinePair`
- Enums: `Phase`, `HealthStatus`, `OperatorRank`, `AuthorityLevel`, `BindingType`

### Organization Schemas (3 schemas)

**`cap.cell.v1`** - Cell (squad) formation and management
- `CellConfig`, `CellState`, `Cell`
- `CellFormationRequest/Response`, `CellMembershipChange`

**`cap.zone.v1`** - Zone (hierarchy) coordination
- `ZoneConfig`, `ZoneState`, `Zone`, `ZoneStats`
- `ZoneFormationRequest/Response`, `ZoneMembershipChange`

**`cap.role.v1`** - Tactical role assignments
- `RoleCapabilities`, `RoleAssignment`, `RoleScoringFactors`
- `RoleAssignmentRequest/Response`, `RoleChangeEvent`
- Enum: `CellRole` (LEADER, SENSOR, COMPUTE, RELAY, STRIKE, SUPPORT, FOLLOWER)

### Protocol Schemas (2 schemas)

**`cap.beacon.v1`** - Discovery phase beacons
- `Beacon`, `BeaconQuery`, `BeaconQueryResponse`, `BeaconRecord`

**`cap.composition.v1`** - Capability composition rules
- `CompositionRule`, `CompositionResult`
- `ApplyCompositionRequest/Response`
- Enum: `CompositionRuleType` (ADDITIVE, EMERGENT, REDUNDANT, CONSTRAINT)

## Usage

### Rust

```rust
use cap_schema::node::v1::{NodeConfig, NodeState, Phase, HealthStatus};
use cap_schema::capability::v1::{Capability, CapabilityType};

// Create a node configuration
let config = NodeConfig {
    id: "node-1".to_string(),
    platform_type: "UAV".to_string(),
    capabilities: vec![],
    comm_range_m: 1000.0,
    max_speed_mps: 10.0,
    operator_binding: None,
    created_at: None,
};

// Create a node state
let state = NodeState {
    position: Some(cap_schema::common::v1::Position {
        latitude: 37.7749,
        longitude: -122.4194,
        altitude: 100.0,
    }),
    fuel_minutes: 120,
    health: HealthStatus::Nominal as i32,
    phase: Phase::Discovery as i32,
    cell_id: None,
    zone_id: None,
    timestamp: None,
};
```

### Python

```python
from cap_schema.node.v1 import NodeConfig, NodeState, Phase, HealthStatus
from cap_schema.common.v1 import Position

# Create a node configuration
config = NodeConfig(
    id="node-1",
    platform_type="UAV",
    capabilities=[],
    comm_range_m=1000.0,
    max_speed_mps=10.0,
)

# Create a node state
state = NodeState(
    position=Position(latitude=37.7749, longitude=-122.4194, altitude=100.0),
    fuel_minutes=120,
    health=HealthStatus.HEALTH_STATUS_NOMINAL,
    phase=Phase.PHASE_DISCOVERY,
)
```

### Java

```java
import cap.node.v1.NodeOuterClass.NodeConfig;
import cap.node.v1.NodeOuterClass.NodeState;
import cap.common.v1.CommonOuterClass.Position;

// Create a node configuration
NodeConfig config = NodeConfig.newBuilder()
    .setId("node-1")
    .setPlatformType("UAV")
    .setCommRangeM(1000.0f)
    .setMaxSpeedMps(10.0f)
    .build();

// Create a node state
NodeState state = NodeState.newBuilder()
    .setPosition(Position.newBuilder()
        .setLatitude(37.7749)
        .setLongitude(-122.4194)
        .setAltitude(100.0)
        .build())
    .setFuelMinutes(120)
    .setHealth(HealthStatus.HEALTH_STATUS_NOMINAL)
    .setPhase(Phase.PHASE_DISCOVERY)
    .build();
```

## Schema Versioning Strategy

### Version Numbering

- Schemas use **semantic versioning** via package namespaces (e.g., `cap.node.v1`, `cap.node.v2`)
- Major version changes (v1 → v2) indicate breaking changes
- Minor changes within a version are backward compatible

### Backward Compatibility Rules

1. **Never remove fields**: Mark as deprecated instead
2. **Never change field numbers**: Field numbers are immutable
3. **Never change field types**: Create a new field if type must change
4. **Always provide default values**: For optional fields
5. **Additive changes only**: New fields must be optional

### Deprecation Process

When a field needs to be deprecated:

```protobuf
message NodeConfig {
  string id = 1;
  string platform_type = 2;

  // DEPRECATED: Use `capabilities_v2` instead
  repeated Capability capabilities = 3 [deprecated = true];

  // New field with improved design
  repeated CapabilityV2 capabilities_v2 = 4;
}
```

### Version Migration

When creating a new major version:

1. Create new package namespace (e.g., `cap.node.v2`)
2. Copy existing messages to new namespace
3. Make breaking changes in new namespace
4. Provide migration utilities in Rust code
5. Support both versions during transition period (minimum 6 months)
6. Deprecate old version after transition period

Example migration utility:

```rust
impl From<cap::node::v1::NodeConfig> for cap::node::v2::NodeConfig {
    fn from(v1: cap::node::v1::NodeConfig) -> Self {
        Self {
            id: v1.id,
            platform_type: v1.platform_type,
            // ... migrate other fields
        }
    }
}
```

### Compatibility Testing

All schema changes must pass:

1. **Forward compatibility**: New clients can read old messages
2. **Backward compatibility**: Old clients can read new messages
3. **Round-trip serialization**: `serialize(deserialize(msg)) == msg`

## Validation

The crate provides validation utilities in `validation.rs`:

```rust
use cap_schema::validation::{validate_capability, validate_node_config};

let cap = Capability { /* ... */ };
validate_capability(&cap)?; // Returns ValidationError if invalid
```

Validation checks:
- Confidence scores are in range [0.0, 1.0]
- Required fields are present
- Semantic constraints are satisfied (e.g., min_size ≤ max_size)
- CRDT invariants are maintained

## Ontology

The crate includes a domain ontology in `ontology.rs`:

```rust
use cap_schema::ontology::build_cap_ontology;

let ontology = build_cap_ontology();

// Check if UAV is a subtype of platform
assert!(ontology.is_subtype_of("uav", "platform")); // true

// Get all capability concepts
let capabilities = ontology.concepts_by_category(ConceptCategory::Capability);
```

The ontology defines:
- Domain concepts (entities, organizations, capabilities, processes, roles)
- Semantic relationships (is-a, related-to)
- Concept properties and metadata

## Building

### Requirements

- Rust 1.70+
- Protocol Buffer compiler (`protoc`)

Install `protoc`:

```bash
# macOS
brew install protobuf

# Ubuntu/Debian
apt-get install protobuf-compiler

# From source
# See https://github.com/protocolbuffers/protobuf/releases
```

### Build

```bash
cargo build
```

The build script (`build.rs`) automatically generates Rust code from `.proto` files using `prost`.

### Test

```bash
cargo test
```

## Code Generation for Other Languages

### Python

```bash
# Install protoc Python plugin
pip install grpcio-tools

# Generate Python code
python -m grpc_tools.protoc \
  -I=proto \
  --python_out=. \
  --grpc_python_out=. \
  proto/*.proto
```

### Java

```bash
# Install protoc Java plugin
# https://github.com/grpc/grpc-java

# Generate Java code
protoc -I=proto \
  --java_out=java/src/main/java \
  --grpc-java_out=java/src/main/java \
  proto/*.proto
```

### C++

```bash
# Generate C++ code
protoc -I=proto \
  --cpp_out=cpp/src \
  --grpc_out=cpp/src \
  --plugin=protoc-gen-grpc=`which grpc_cpp_plugin` \
  proto/*.proto
```

### JavaScript/TypeScript

```bash
# Install protoc JavaScript plugin
npm install -g protoc-gen-ts

# Generate TypeScript code
protoc -I=proto \
  --plugin=protoc-gen-ts=./node_modules/.bin/protoc-gen-ts \
  --ts_out=js/src \
  proto/*.proto
```

## Documentation

- **Protobuf Language Guide**: https://protobuf.dev/programming-guides/proto3/
- **prost Documentation**: https://docs.rs/prost/
- **tonic Documentation**: https://docs.rs/tonic/

## License

MIT

## References

- ADR-012: Schema Definition and Protocol Extensibility
- PEAT Protocol Documentation: `/docs`
- Protobuf Style Guide: https://protobuf.dev/programming-guides/style/
