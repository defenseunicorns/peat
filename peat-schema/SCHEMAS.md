# Peat Protocol Schema Reference

Complete reference for all Peat Protocol message schemas.

## Overview

The CAP Schema defines **8 Protocol Buffer packages** organized into 4 categories:

1. **Core Types** (1 schema): Common types used across all messages
2. **Entity Schemas** (2 schemas): Individual nodes and capabilities
3. **Organization Schemas** (3 schemas): Cells, zones, and roles
4. **Protocol Schemas** (2 schemas): Discovery and composition

---

## 1. Core Types

### `cap.common.v1` - Common Types

**File**: `proto/common.proto`

**Purpose**: Shared types used across all Peat Protocol messages

**Key Messages**:
- `Uuid`: Unique identifier (UU ID format)
- `Timestamp`: Unix epoch timestamps (seconds + nanos)
- `Position`: Geographic coordinates (lat/lon/alt in degrees/meters)
- `Confidence`: Confidence score (0.0 - 1.0)
- `Metadata`: Generic key-value metadata

**Usage**: Imported by all other schema files for consistent typing

---

## 2. Entity Schemas

### `cap.capability.v1` - Capabilities

**File**: `proto/capability.proto`

**Purpose**: Define node and cell capabilities

**Key Messages**:
- `Capability`: Represents a capability with type, confidence, and metadata
- `CapabilityQuery`: Query for discovering nodes/cells by capabilities
- `CapabilityResponse`: Query results with matching capabilities

**Key Enums**:
- `CapabilityType`: SENSOR, COMPUTE, COMMUNICATION, MOBILITY, PAYLOAD, EMERGENT

**CRDT Semantics**: Capabilities use **G-Set** (grow-only set) - can only be added, never removed

---

### `cap.node.v1` - Nodes

**File**: `proto/node.proto`

**Purpose**: Define individual node (platform) configuration and state

**Key Messages**:
- `NodeConfig`: Static configuration (platform type, capabilities, communication range)
- `NodeState`: Dynamic state (position, fuel, health, phase, cell/zone assignment)
- `Node`: Complete node (config + state)
- `Operator`: Human operator information (rank, authority, MOS)
- `HumanMachinePair`: Human-machine binding (one-to-one, one-to-many, many-to-one)

**Key Enums**:
- `Phase`: DISCOVERY, CELL, HIERARCHY (protocol phase)
- `HealthStatus`: NOMINAL, DEGRADED, CRITICAL, FAILED
- `OperatorRank`: E1-E9 (enlisted), O1-O10 (officer), W1-W5 (warrant)
- `AuthorityLevel`: OBSERVER, ADVISOR, SUPERVISOR, COMMANDER
- `BindingType`: ONE_TO_ONE, ONE_TO_MANY, MANY_TO_ONE, MANY_TO_MANY

**CRDT Semantics**:
- Capabilities: **G-Set** (grow-only)
- State fields: **LWW-Register** (last-write-wins using timestamps)
- Fuel: **PN-Counter** (positive-negative counter for consume/replenish)

---

## 3. Organization Schemas

### `cap.cell.v1` - Cells (Squads)

**File**: `proto/cell.proto`

**Purpose**: Define tactical cell formation and management

**Key Messages**:
- `CellConfig`: Cell configuration (ID, min/max size limits)
- `CellState`: Dynamic state (leader, members, aggregated capabilities, platoon assignment)
- `Cell`: Complete cell (config + state)
- `CellFormationRequest/Response`: Cell formation messages
- `CellMembershipChange`: Membership change events (JOIN, LEAVE, LEADER)

**CRDT Semantics**:
- Leader: **LWW-Register** (last-write-wins)
- Members: **OR-Set** (observed-remove set - add wins over remove)
- Capabilities: **G-Set** (grow-only aggregation)

**Constraints**:
- Min size: 2 nodes
- Max size: 8 nodes (configurable)

---

### `cap.zone.v1` - Zones (Hierarchy)

**File**: `proto/zone.proto`

**Purpose**: Define strategic zone coordination across multiple cells

**Key Messages**:
- `ZoneConfig`: Zone configuration (ID, min/max cells)
- `ZoneState`: Dynamic state (coordinator, cells, aggregated capabilities)
- `Zone`: Complete zone (config + state)
- `ZoneStats`: Derived statistics (cell count, node count, validity)
- `ZoneFormationRequest/Response`: Zone formation messages
- `ZoneMembershipChange`: Change events (CELL_JOIN, CELL_LEAVE, COORDINATOR_CHANGE)

**CRDT Semantics**:
- Coordinator: **LWW-Register** (last-write-wins)
- Cells: **OR-Set** (observed-remove set)
- Capabilities: **G-Set** (grow-only aggregation from cells)

**Constraints**:
- Min cells: 2 (default)
- Max cells: 10 (configurable)

---

### `cap.role.v1` - Tactical Roles

**File**: `proto/role.proto`

**Purpose**: Define tactical role assignments within cells

**Key Messages**:
- `RoleCapabilities`: Role definition (required/preferred capabilities, MOS codes)
- `RoleAssignment`: Node-to-role assignment with fitness score
- `RoleScoringFactors`: Scoring breakdown (capability, MOS, health, endurance, position)
- `RoleAssignmentRequest/Response`: Role assignment messages
- `RoleChangeEvent`: Role change events

**Key Enums**:
- `CellRole`: LEADER, SENSOR, COMPUTE, RELAY, STRIKE, SUPPORT, FOLLOWER

**Scoring Factors**:
- Capability match (required vs available)
- Operator MOS match (military specialty)
- Platform health (NOMINAL > DEGRADED > CRITICAL)
- Endurance (fuel remaining)
- Position (proximity to mission area - optional)

---

## 4. Protocol Schemas

### `cap.beacon.v1` - Discovery

**File**: `proto/beacon.proto`

**Purpose**: Discovery phase beacon broadcasting and querying

**Key Messages**:
- `Beacon`: Discovery beacon (node config, state, capabilities, TTL, sequence number)
- `BeaconQuery`: Query for discovering nodes (by platform type, capabilities, phase, location, health)
- `BeaconQueryResponse`: Query results
- `BeaconRecord`: CRDT storage entry (beacon + first/last seen timestamps, seen count, active flag)

**Discovery Protocol**:
1. Nodes broadcast beacons periodically (every 1-5 seconds)
2. Beacons include sequence number (detect gaps/loss)
3. TTL field enables relay (multi-hop discovery)
4. Queries filter by multiple criteria (capabilities, location, health, fuel)

**Use Cases**:
- Finding nodes with specific capabilities (e.g., "find all UAVs with sensors")
- Geographic filtering (e.g., "find nodes within 1km of position X")
- Health-based filtering (e.g., "find only NOMINAL nodes")

---

### `cap.composition.v1` - Capability Composition

**File**: `proto/composition.proto`

**Purpose**: Define capability composition rules for cells

**Key Messages**:
- `CompositionRule`: Rule definition (type, inputs, outputs, confidence method, priority)
- `CompositionResult`: Result of applying rules (inputs, outputs, validation, errors)
- `ApplyCompositionRequest/Response`: Rule application messages

**Key Enums**:
- `CompositionRuleType`: ADDITIVE, EMERGENT, REDUNDANT, CONSTRAINT

**Rule Types**:

1. **Additive**: Simple aggregation (e.g., multiple sensors → improved coverage)
   - Confidence: Usually MIN or AVERAGE of inputs

2. **Emergent**: New capability emerges (e.g., UAV + ground relay → BLOS communication)
   - Confidence: PRODUCT or CUSTOM calculation
   - Creates new `Capability` not present in any single node

3. **Redundant**: Backup/failover (e.g., redundant GPS for robustness)
   - Confidence: MAX (best available)
   - Improves reliability, not capability scope

4. **Constraint**: Validates constraints (e.g., "cell must have at least one sensor")
   - Confidence: Binary (1.0 if met, 0.0 if not)
   - No output capability, only validation

**Confidence Methods**:
- MIN: Weakest link (conservative)
- MAX: Strongest link (optimistic)
- AVERAGE: Balanced estimate
- PRODUCT: Probabilistic independence (emergent capabilities)
- CUSTOM: User-defined calculation (in metadata)

---

## Schema Dependencies

```
common.proto (foundation)
    ↓
    ├─→ capability.proto
    │       ↓
    │       ├─→ node.proto
    │       │       ↓
    │       │       ├─→ cell.proto
    │       │       │       ↓
    │       │       │       └─→ zone.proto
    │       │       │
    │       │       ├─→ beacon.proto
    │       │       └─→ role.proto
    │       │
    │       └─→ composition.proto
```

---

## CRDT Operations Summary

| Schema | CRDT Type | Field | Operation |
|--------|-----------|-------|-----------|
| `node` | G-Set | capabilities | Add only |
| `node` | LWW-Register | position, health, phase, cell_id, zone_id | Last-write-wins |
| `node` | PN-Counter | fuel_minutes | Increment/decrement |
| `cell` | LWW-Register | leader_id, platoon_id | Last-write-wins |
| `cell` | OR-Set | members | Add wins over remove |
| `cell` | G-Set | capabilities | Add only (aggregated) |
| `zone` | LWW-Register | coordinator_id | Last-write-wins |
| `zone` | OR-Set | cells | Add wins over remove |
| `zone` | G-Set | aggregated_capabilities | Add only |

---

## Message Size Guidelines

**Small Messages** (< 1KB):
- `common.*` types
- `Capability`
- `RoleAssignment`
- `CellMembershipChange`

**Medium Messages** (1-10KB):
- `NodeConfig` (depends on capability count)
- `NodeState`
- `CellState` (depends on member count)
- `Beacon`

**Large Messages** (10-100KB):
- `ZoneState` (depends on cell count and aggregated capabilities)
- `BeaconQueryResponse` (depends on result count)
- `CompositionResult` (depends on rule count and capability count)

**Optimization Tip**: Use queries to filter before retrieving full objects. For example, use `BeaconQuery` to get node IDs first, then fetch full node details only for relevant nodes.

---

## Versioning

All schemas use the `v1` namespace (e.g., `cap.node.v1`). When breaking changes are needed:

1. Create new `v2` namespace (e.g., `cap.node.v2`)
2. Keep `v1` for backward compatibility (minimum 6 months)
3. Provide migration utilities in Rust code

See `README.md` for full versioning strategy.

---

## Code Generation

All languages use the same `.proto` files:

- **Rust**: `prost` (automatic via `build.rs`)
- **Python**: `grpc_tools.protoc`
- **Java**: `protoc` with `--java_out`
- **C++**: `protoc` with `--cpp_out`
- **JavaScript/TypeScript**: `protoc-gen-ts`

See `README.md` for language-specific code generation commands.

---

## References

- **ADR-012**: Schema Definition and Protocol Extensibility
- **REFACTORING-PLAYBOOK.md**: Phase 1 implementation plan
- **Protobuf Language Guide**: https://protobuf.dev/programming-guides/proto3/
- **CRDT Theory**: https://crdt.tech/
