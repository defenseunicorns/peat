# PEAT Protocol Specification: Coordination Protocol

**Spec ID**: PEAT-SPEC-004
**Status**: Draft
**Version**: 0.1.0
**Date**: 2025-01-07
**Authors**: (r)evolve - Revolve Team LLC

## Abstract

This document specifies the coordination protocol for PEAT. It defines cell formation, leader election, hierarchical organization, and inter-cell coordination mechanisms.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Cell Fundamentals](#3-cell-fundamentals)
4. [Cell Formation](#4-cell-formation)
5. [Leader Election](#5-leader-election)
6. [Hierarchical Organization](#6-hierarchical-organization)
7. [Role Assignment](#7-role-assignment)
8. [State Synchronization](#8-state-synchronization)
9. [Failure Handling](#9-failure-handling)
10. [Inter-Cell Coordination](#10-inter-cell-coordination)
11. [Security Considerations](#11-security-considerations)

---

## 1. Introduction

### 1.1 Purpose

The PEAT coordination protocol enables autonomous and semi-autonomous systems to form dynamic teams ("cells") that operate effectively without centralized control. It provides mechanisms for:
- Discovering and joining cells
- Electing leaders based on capabilities and authority
- Organizing hierarchically (team → group → formation)
- Handling node failures and network partitions

### 1.2 Design Goals

- **Decentralized**: No single point of failure
- **Adaptive**: Responds to changing conditions
- **Hybrid Human-Machine**: Integrates human authority
- **Resilient**: Continues operating during partitions

### 1.3 Requirements Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Cell** | A group of nodes coordinating together |
| **Formation** | The process of establishing a cell |
| **Leader** | Node responsible for cell coordination |
| **Member** | Any node in a cell (including leader) |
| **Parent Cell** | Higher echelon cell (e.g., group to team) |
| **Child Cell** | Lower echelon cell |
| **Hierarchy Level** | Position in command structure (0=root) |
| **Capability Score** | Numeric rating of node capabilities |
| **Authority Score** | Numeric rating of human authority |

---

## 3. Cell Fundamentals

### 3.1 Cell Identity

Each cell has:
- **Cell ID**: UUID v4 (16 bytes)
- **Formation ID**: Shared secret for admission (32 bytes)
- **Callsign**: Human-readable name (e.g., "ALPHA")
- **Hierarchy Level**: Position in structure (0-7)

### 3.2 Cell Configuration

```rust
pub struct CellConfig {
    /// Minimum nodes required for quorum
    pub min_members: usize,
    /// Maximum nodes allowed
    pub max_members: usize,
    /// Leader election interval
    pub election_interval: Duration,
    /// Heartbeat timeout
    pub heartbeat_timeout: Duration,
    /// Leadership policy
    pub leadership_policy: LeadershipPolicy,
    /// Whether humans are required
    pub requires_human: bool,
}

pub enum LeadershipPolicy {
    /// Highest rank always wins
    RankDominant,
    /// Best technical capabilities wins
    TechnicalDominant,
    /// Weighted combination
    Hybrid { authority_weight: f32, technical_weight: f32 },
    /// Adapts to mission phase
    Contextual,
}
```

### 3.3 Cell States

```
                    ┌──────────────┐
                    │   Forming    │
                    └──────┬───────┘
                           │ quorum reached
                           ▼
                    ┌──────────────┐
              ┌─────│   Active     │─────┐
              │     └──────────────┘     │
              │             │             │
        partition│         │ quorum lost  │ merge
              │             ▼             │
              │     ┌──────────────┐     │
              └────>│  Degraded    │<────┘
                    └──────┬───────┘
                           │ dissolved
                           ▼
                    ┌──────────────┐
                    │  Dissolved   │
                    └──────────────┘
```

---

## 4. Cell Formation

### 4.1 Discovery

Nodes discover potential cells through:
1. **mDNS broadcast**: Local network discovery
2. **Static configuration**: Pre-configured peer list
3. **BLE advertising**: Bluetooth discovery
4. **Parent assignment**: Directed by higher echelon

### 4.2 Formation Protocol

```
    Initiator                          Responder
        │                                   │
        │-------- FormationRequest -------->│
        │  (formation_id, capabilities)     │
        │                                   │
        │<------- FormationChallenge -------|
        │  (nonce)                          │
        │                                   │
        │-------- FormationResponse ------->│
        │  (signature over nonce)           │
        │                                   │
        │<------- FormationAccept ----------|
        │  (cell_id, members, leader)       │
        │                                   │
```

### 4.3 Formation Messages

```protobuf
message FormationRequest {
    // Pre-shared formation key hash
    bytes formation_id = 1;
    // Requester's device ID
    bytes device_id = 2;
    // Requester's public key
    bytes public_key = 3;
    // Capability advertisement
    CapabilityAdvertisement capabilities = 4;
    // Requested role (optional)
    optional Role requested_role = 5;
}

message FormationChallenge {
    // Random nonce (32 bytes)
    bytes nonce = 1;
    // Challenge timestamp
    Timestamp timestamp = 2;
    // Challenger's device ID
    bytes challenger_id = 3;
}

message FormationResponse {
    // Original nonce
    bytes nonce = 1;
    // Ed25519 signature over (nonce || formation_id)
    bytes signature = 2;
    // Responder's device ID
    bytes device_id = 3;
}

message FormationAccept {
    // Assigned cell ID
    bytes cell_id = 1;
    // Current cell members
    repeated CellMember members = 2;
    // Current leader
    bytes leader_id = 3;
    // Cell configuration
    CellConfig config = 4;
}

message CellMember {
    bytes device_id = 1;
    bytes public_key = 2;
    Role role = 3;
    Timestamp joined_at = 4;
}
```

### 4.4 Admission Control

Nodes MUST be rejected if:
- Formation key challenge fails
- Cell is at max capacity
- Device is on blocklist
- Required capabilities not present

---

## 5. Leader Election

### 5.1 Election Trigger

Leader election occurs when:
1. Cell is newly formed
2. Current leader fails (heartbeat timeout)
3. Current leader resigns
4. Periodic re-election interval expires
5. Higher authority overrides

### 5.2 Scoring Algorithm

```rust
pub fn compute_leadership_score(
    node: &Node,
    policy: &LeadershipPolicy,
) -> f64 {
    let technical = compute_technical_score(node);
    let authority = compute_authority_score(node);

    match policy {
        LeadershipPolicy::TechnicalDominant => technical,
        LeadershipPolicy::RankDominant => authority,
        LeadershipPolicy::Hybrid { authority_weight, technical_weight } => {
            technical * technical_weight + authority * authority_weight
        }
        LeadershipPolicy::Contextual => {
            // Adapts based on mission phase
            context_adaptive_score(node)
        }
    }
}

fn compute_technical_score(node: &Node) -> f64 {
    // Weighted components (sum to 1.0)
    const COMPUTE_WEIGHT: f64 = 0.30;
    const COMMS_WEIGHT: f64 = 0.25;
    const SENSORS_WEIGHT: f64 = 0.20;
    const POWER_WEIGHT: f64 = 0.15;
    const RELIABILITY_WEIGHT: f64 = 0.10;

    normalize(node.compute) * COMPUTE_WEIGHT
        + normalize(node.comms) * COMMS_WEIGHT
        + normalize(node.sensors) * SENSORS_WEIGHT
        + normalize(node.power) * POWER_WEIGHT
        + normalize(node.reliability) * RELIABILITY_WEIGHT
}

fn compute_authority_score(node: &Node) -> f64 {
    if let Some(operator) = &node.operator_binding {
        rank_to_score(operator.rank) * 0.6
            + authority_level_to_score(operator.authority) * 0.3
            + (1.0 - operator.cognitive_load) * 0.1
    } else {
        0.0 // No human operator
    }
}
```

### 5.3 Election Protocol

```
    Node A (candidate)          Node B (candidate)          Node C (voter)
         │                           │                           │
         │<────────── RequestVote ───┼───────────────────────────│
         │   (score: 0.85)           │                           │
         │                           │                           │
         │───────── RequestVote ─────┼──────────────────────────>│
         │   (score: 0.72)           │                           │
         │                           │                           │
         │                           │<───── VoteGrant ──────────│
         │                           │   (for: A)                │
         │<───────── VoteGrant ──────┼───────────────────────────│
         │   (for: A)                │                           │
         │                           │                           │
         │────────── Elected ────────┼──────────────────────────>│
         │                           │                           │
```

### 5.4 Tie-Breaking

If scores are equal (within 0.01), ties are broken by:
1. Higher human authority rank
2. Longer cell membership duration
3. Lexicographically higher device ID

### 5.5 Election Timeout

Elections MUST complete within:
- Normal: 5 seconds
- Emergency (leader failed): 2 seconds

If no consensus in timeout, the node with highest score self-declares.

---

## 6. Hierarchical Organization

### 6.1 Hierarchy Levels

| Level | Name | Typical Size | Parent |
|-------|------|--------------|--------|
| 0 | Root | 1 | None |
| 1 | Cluster | 100-200 | Root |
| 2 | Formation | 30-50 | Cluster |
| 3 | Group | 8-12 | Formation |
| 4 | Team | 2-4 | Group |
| 5 | Node | 1 | Team |

### 6.2 Parent-Child Relationship

```protobuf
message HierarchyBinding {
    // Child cell ID
    bytes child_cell_id = 1;
    // Parent cell ID
    bytes parent_cell_id = 2;
    // Parent leader's device ID
    bytes parent_leader_id = 3;
    // Binding timestamp
    Timestamp bound_at = 4;
    // Binding status
    BindingStatus status = 5;
}

enum BindingStatus {
    BINDING_STATUS_UNSPECIFIED = 0;
    BINDING_STATUS_PENDING = 1;
    BINDING_STATUS_ACTIVE = 2;
    BINDING_STATUS_SUSPENDED = 3;
    BINDING_STATUS_DISSOLVED = 4;
}
```

### 6.3 Capability Aggregation and Emergent Behavior

A core principle of PEAT is that **cells exhibit emergent capabilities** greater than the sum of their individual members. Capability aggregation flows upward through the hierarchy, enabling higher echelons to understand and task based on collective capabilities.

#### 6.3.1 Capability Flow Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CLUSTER COORDINATOR                                │
│  Sees: "3 formations with combined Sensing, Action, and Relay capabilities" │
│  Emergent: Full-spectrum observation and coordinated action package          │
└─────────────────────────────────────────────────────────────────────────────┘
                                    ▲
                    Aggregated capability summaries
                                    │
        ┌───────────────────────────┼───────────────────────────┐
        │                           │                           │
        ▼                           ▼                           ▼
┌───────────────────┐     ┌───────────────────┐     ┌───────────────────┐
│   FORMATION 1     │     │   FORMATION 2     │     │   FORMATION 3     │
│  Sensing + Relay  │     │  Action + Sensing │     │  Action + Signal  │
│  Emergent: Wide   │     │  Emergent: Sense- │     │  Emergent: Coord- │
│  area coverage    │     │  and-act          │     │  inated response  │
└─────────┬─────────┘     └─────────┬─────────┘     └─────────┬─────────┘
          │                         │                         │
    ┌─────┴─────┐             ┌─────┴─────┐             ┌─────┴─────┐
    ▼           ▼             ▼           ▼             ▼           ▼
┌───────┐   ┌───────┐     ┌───────┐   ┌───────┐     ┌───────┐   ┌───────┐
│Group 1│   │Group 2│     │Group 3│   │Group 4│     │Group 5│   │Group 6│
│EO+IR  │   │COMMS  │     │Action │   │EO     │     │Action │   │Signal │
└───┬───┘   └───┬───┘     └───┬───┘   └───┬───┘     └───┬───┘   └───┬───┘
    │           │             │           │             │           │
┌───┴───┐   ┌───┴───┐     ┌───┴───┐   ┌───┴───┐     ┌───┴───┐   ┌───┴───┐
│4 UAV  │   │2 Relay│     │2 Auton│   │4 UAV  │     │2 Auton│   │2 Signal│
│sensors│   │nodes  │     │actors │   │sensors│     │actors │   │ nodes  │
└───────┘   └───────┘     └───────┘   └───────┘     └───────┘   └───────┘

Individual platforms → Group capabilities → Formation emergent → Cluster emergent
```

#### 6.3.2 Emergent Capability Discovery

Emergent capabilities arise from the **composition** of individual platform capabilities:

```rust
/// Emergent capability patterns recognized by PEAT
pub enum EmergentCapability {
    /// Multiple sensors with overlapping coverage → Wide-area observation
    WideAreaObservation {
        sensor_count: usize,
        coverage_area_km2: f64,
    },

    /// Sensing + Actuation in same cell → Sense-and-act loop
    SenseAndAct {
        sensing_platforms: Vec<DeviceId>,
        actuation_platforms: Vec<DeviceId>,
    },

    /// Signal + Actuation → Coordinated response
    CoordinatedResponse {
        signal_nodes: Vec<DeviceId>,
        actuators: Vec<DeviceId>,
    },

    /// Multiple relay nodes → Extended mesh range
    ExtendedRange {
        relay_chain: Vec<DeviceId>,
        range_extension_km: f64,
    },

    /// Heterogeneous sensors → Multi-spectral fusion
    MultiSpectralFusion {
        eo_sensors: Vec<DeviceId>,
        ir_sensors: Vec<DeviceId>,
        radar_sensors: Vec<DeviceId>,
    },

    /// Compute + sensors → Edge AI processing
    EdgeIntelligence {
        compute_nodes: Vec<DeviceId>,
        sensor_feeds: Vec<DeviceId>,
        models_available: Vec<String>,
    },
}

/// Detect emergent capabilities from member capabilities
pub fn discover_emergent_capabilities(
    members: &[CapabilityAdvertisement],
) -> Vec<EmergentCapability> {
    let mut emergent = Vec::new();

    // Collect capability types across all members
    let all_sensors: Vec<_> = members.iter()
        .flat_map(|m| &m.sensors)
        .collect();
    let all_actuators: Vec<_> = members.iter()
        .flat_map(|m| &m.actuators)
        .collect();
    let total_compute: f64 = members.iter()
        .filter_map(|m| m.compute.as_ref())
        .map(|c| c.compute_tflops.unwrap_or(0.0))
        .sum();

    // Pattern: Sensing + Actuation = Sense-and-Act Loop
    let sensing_capable: Vec<_> = members.iter()
        .filter(|m| has_sensing_capability(m))
        .map(|m| m.device_id)
        .collect();
    let actuation_capable: Vec<_> = members.iter()
        .filter(|m| has_actuation_capability(m))
        .map(|m| m.device_id)
        .collect();

    if !sensing_capable.is_empty() && !actuation_capable.is_empty() {
        emergent.push(EmergentCapability::SenseAndAct {
            sensing_platforms: sensing_capable,
            actuation_platforms: actuation_capable,
        });
    }

    // Pattern: Multiple overlapping sensors = Wide Area Observation
    if all_sensors.len() >= 3 {
        let coverage = calculate_combined_coverage(&all_sensors);
        emergent.push(EmergentCapability::WideAreaObservation {
            sensor_count: all_sensors.len(),
            coverage_area_km2: coverage,
        });
    }

    // Pattern: Compute + Sensors = Edge Intelligence
    if total_compute > 1.0 && !all_sensors.is_empty() {
        emergent.push(EmergentCapability::EdgeIntelligence {
            compute_nodes: members.iter()
                .filter(|m| m.compute.as_ref()
                    .map(|c| c.compute_tflops.unwrap_or(0.0) > 0.5)
                    .unwrap_or(false))
                .map(|m| m.device_id)
                .collect(),
            sensor_feeds: members.iter()
                .filter(|m| !m.sensors.is_empty())
                .map(|m| m.device_id)
                .collect(),
            models_available: collect_available_models(members),
        });
    }

    emergent
}
```

#### 6.3.3 Capability Aggregation Protocol

Each cell leader periodically computes and advertises aggregated capabilities:

```protobuf
message CellCapabilitySummary {
    // Cell identifier
    bytes cell_id = 1;
    // Hierarchy level
    uint32 level = 2;
    // Timestamp of this summary
    Timestamp timestamp = 3;

    // Member count by type
    map<int32, uint32> member_counts = 4;  // PlatformType -> count

    // Aggregated sensor capabilities
    AggregatedSensors sensors = 5;

    // Aggregated actuator capabilities
    AggregatedActuators actuators = 6;

    // Total compute available
    double total_compute_tflops = 7;

    // Communication reach
    CommunicationSummary comms = 8;

    // Average power/endurance
    PowerSummary power = 9;

    // Discovered emergent capabilities
    repeated EmergentCapability emergent = 10;

    // Geographic coverage
    BoundingBox coverage_area = 11;

    // Operational readiness (0.0 - 1.0)
    float readiness = 12;
}

message AggregatedSensors {
    // Count by sensor type
    map<int32, uint32> type_counts = 1;
    // Combined detection range (best case)
    double max_detection_range_m = 2;
    // Combined coverage area
    double coverage_area_km2 = 3;
    // Available sensor modalities
    repeated SensorType modalities = 4;
}

message AggregatedActuators {
    // Count by actuator type
    map<int32, uint32> type_counts = 1;
    // Total available resources/uses
    uint32 total_resources = 2;
    // Combined action range
    double max_action_range_m = 3;
    // Total payload capacity
    double total_payload_kg = 4;
}
```

#### 6.3.4 Aggregation Policies

Different data types require different aggregation strategies:

```rust
pub struct AggregationPolicy {
    /// How to aggregate track data
    pub track_aggregation: TrackAggregation,
    /// How to summarize capabilities
    pub capability_mode: CapabilitySummaryMode,
    /// Status report interval to parent
    pub status_interval: Duration,
    /// Priority threshold for immediate escalation
    pub escalation_priority: Priority,
    /// Whether to include individual member details
    pub include_member_details: bool,
    /// Emergent capability detection enabled
    pub detect_emergent: bool,
}

pub enum TrackAggregation {
    /// Send all tracks (high bandwidth)
    Full,
    /// Send summary counts by type (minimal bandwidth)
    CountOnly,
    /// Send priority tracks + counts (balanced)
    PriorityPlusCounts { max_tracks: usize },
    /// Spatial clustering (geographic compression)
    Clustered { cluster_radius_m: f64 },
}

pub enum CapabilitySummaryMode {
    /// Full capability details per member
    Detailed,
    /// Aggregated totals only
    Totals,
    /// Totals + emergent capabilities
    TotalsWithEmergent,
    /// Only report capability changes
    DeltaOnly,
}
```

### 6.4 Bidirectional Flow Model

PEAT operates as a **full-duplex hierarchical synchronization system**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            UPWARD FLOW                                       │
│  (Edge → Coordinator)                                                        │
│                                                                              │
│  • Capability advertisements (what can I do?)                                │
│  • Track/detection reports (what do I see?)                                  │
│  • Status updates (how am I doing?)                                          │
│  • Emergent capability discovery (what can WE do together?)                  │
│                                                                              │
│  Characteristics: High volume, compressible, tolerates staleness             │
├─────────────────────────────────────────────────────────────────────────────┤
│                            DOWNWARD FLOW                                     │
│  (Coordinator → Edge)                                                        │
│                                                                              │
│  • Mission tasking (what should I do?)                                       │
│  • Operational constraints (what am I allowed to do?)                        │
│  • AI model distribution (how should I process data?)                        │
│  • Configuration changes (how should I operate?)                             │
│  • Coordinator intent (what is the goal?)                                    │
│                                                                              │
│  Characteristics: Low volume, high priority, cannot tolerate loss            │
├─────────────────────────────────────────────────────────────────────────────┤
│                           HORIZONTAL FLOW                                    │
│  (Peer → Peer)                                                               │
│                                                                              │
│  • Track handoffs (transferring responsibility)                              │
│  • Deconfliction (avoiding collisions/interference)                          │
│  • Mutual support requests (need sensor/actuator coverage)                   │
│  • Boundary coordination (adjacent cell awareness)                           │
│                                                                              │
│  Characteristics: Time-critical, requires peer authentication                │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 6.4.1 Policy-Based Routing

Events carry routing policies that PEAT enforces:

```rust
pub struct EventRoutingPolicy {
    /// How far up the hierarchy to propagate
    pub propagation: PropagationMode,
    /// Priority for bandwidth allocation
    pub priority: Priority,
    /// What to do on network partition
    pub partition_handling: PartitionPolicy,
    /// Time-to-live before expiration
    pub ttl: Duration,
}

pub enum PropagationMode {
    /// Store locally, respond to queries only
    Local,
    /// Propagate to immediate parent only
    Parent,
    /// Propagate to all ancestors
    AllAncestors,
    /// Immediate propagation, preempt other traffic
    Critical,
}

pub enum PartitionPolicy {
    /// Buffer and retry when connection restored
    BufferAndRetry,
    /// Drop if cannot deliver immediately
    DropOnPartition,
    /// Require immediate delivery or fail
    RequireImmediate,
}
```

### 6.5 Downward Command Flow

Commands flow from parent to child:

```protobuf
message CommandMessage {
    // Source cell ID
    bytes source_cell = 1;
    // Target cell ID (or broadcast)
    optional bytes target_cell = 2;
    // Command type
    CommandType type = 3;
    // Command payload
    bytes payload = 4;
    // Priority
    Priority priority = 5;
    // Acknowledgment required
    bool ack_required = 6;
}

enum CommandType {
    COMMAND_TYPE_UNSPECIFIED = 0;
    COMMAND_TYPE_MISSION_ASSIGN = 1;
    COMMAND_TYPE_POSITION_UPDATE = 2;
    COMMAND_TYPE_FORMATION_CHANGE = 3;
    COMMAND_TYPE_ABORT = 4;
    COMMAND_TYPE_RALLY = 5;
}
```

---

## 7. Role Assignment

### 7.1 Standard Roles

```protobuf
enum Role {
    ROLE_UNSPECIFIED = 0;
    ROLE_LEADER = 1;      // Cell leader
    ROLE_DEPUTY = 2;      // Backup leader
    ROLE_SCOUT = 3;       // Forward observer
    ROLE_RELAY = 4;       // Communications relay
    ROLE_SENSOR = 5;      // Primary sensor platform
    ROLE_ACTUATOR = 6;    // Primary actuator
    ROLE_SUPPORT = 7;     // Supply/support
    ROLE_OBSERVER = 8;    // Passive observer
}
```

### 7.2 Role Assignment Algorithm

```rust
pub fn assign_roles(cell: &Cell) -> HashMap<DeviceId, Role> {
    let mut assignments = HashMap::new();

    // Leader is already elected
    assignments.insert(cell.leader_id, Role::Leader);

    // Deputy = second-highest leadership score
    let deputy = cell.members
        .iter()
        .filter(|m| m.device_id != cell.leader_id)
        .max_by_key(|m| m.leadership_score);
    if let Some(d) = deputy {
        assignments.insert(d.device_id, Role::Deputy);
    }

    // Assign remaining roles by capability match
    for member in &cell.members {
        if assignments.contains_key(&member.device_id) {
            continue;
        }

        let role = match_best_role(member, &cell.mission);
        assignments.insert(member.device_id, role);
    }

    assignments
}
```

### 7.3 Role Handoff

When roles change (e.g., leader failure):

```
    Old Leader                New Leader               Members
         │                        │                        │
         │ (fails)                │                        │
         │                        │                        │
         │          ┌─────────────┼────────────────────────│
         │          │ election    │                        │
         │          ▼             │                        │
         │     ┌─────────┐        │                        │
         │     │ ELECTED │        │                        │
         │     └────┬────┘        │                        │
         │          │             │                        │
         │          │────────── RoleChange ───────────────>│
         │          │  (new_leader, new_deputy)            │
         │          │                                      │
         │          │<──────── RoleAck ────────────────────│
         │          │                                      │
```

---

## 8. State Synchronization

### 8.1 Cell State Document

Cell state is maintained as a CRDT document:

```rust
pub struct CellState {
    /// Cell identifier
    pub cell_id: CellId,
    /// Current members
    pub members: HashMap<DeviceId, MemberState>,
    /// Current leader
    pub leader_id: DeviceId,
    /// Role assignments
    pub roles: HashMap<DeviceId, Role>,
    /// Active missions
    pub missions: Vec<MissionId>,
    /// Parent binding
    pub parent: Option<HierarchyBinding>,
    /// Children
    pub children: Vec<CellId>,
    /// Last election epoch
    pub election_epoch: u64,
    /// Configuration
    pub config: CellConfig,
}

pub struct MemberState {
    pub device_id: DeviceId,
    pub last_heartbeat: Timestamp,
    pub position: Option<Position>,
    pub status: OperationalStatus,
    pub capabilities: CapabilityAdvertisement,
}
```

### 8.2 Heartbeat Protocol

Members MUST send heartbeats to maintain membership:

```protobuf
message Heartbeat {
    bytes device_id = 1;
    bytes cell_id = 2;
    Timestamp timestamp = 3;
    Position position = 4;
    OperationalStatus status = 5;
    uint32 power_level = 6;
}
```

**Timing**:
- Heartbeat interval: 5 seconds (configurable)
- Failure threshold: 3 missed heartbeats
- Grace period after rejoin: 10 seconds

---

## 9. Failure Handling

### 9.1 Member Failure Detection

```
    Member A                  Leader                   Member B
        │                        │                        │
        │──── Heartbeat ────────>│                        │
        │                        │                        │
        │     (fails)            │                        │
        │                        │                        │
        │                        │<─── Heartbeat ─────────│
        │                        │                        │
        │                   ┌────┴────┐                   │
        │                   │ Timeout │                   │
        │                   └────┬────┘                   │
        │                        │                        │
        │                        │──── MemberFailed ─────>│
        │                        │     (device_id: A)     │
        │                        │                        │
```

### 9.2 Leader Failure

1. Deputy detects leader heartbeat timeout
2. Deputy initiates emergency election
3. Election completes within 2 seconds
4. New leader announces to all members
5. New leader notifies parent cell

### 9.3 Network Partition

```
          Pre-Partition                     Post-Partition
    ┌───────────────────────┐         ┌──────────┐  ┌──────────┐
    │  Cell A               │         │ Cell A-1 │  │ Cell A-2 │
    │  Leader: L            │   ──>   │ Leader:L │  │Leader:D  │
    │  Members: L,D,M1,M2   │         │ M: L,M1  │  │ M: D,M2  │
    └───────────────────────┘         └──────────┘  └──────────┘
```

**Partition rules**:
1. Each partition independently elects leader
2. Partition with original leader retains Cell ID
3. Other partition generates new Cell ID (same Formation ID)
4. On heal, merge negotiation occurs

### 9.4 Partition Healing

```
    Cell A-1 (original)           Cell A-2 (split)
         │                             │
         │<────── PartitionHealing ────│
         │   (members, state_hash)     │
         │                             │
         │───── MergeProposal ────────>│
         │   (merged_state)            │
         │                             │
         │<────── MergeAccept ─────────│
         │                             │
         │ (re-election with all)      │
         │                             │
```

---

## 10. Inter-Cell Coordination

### 10.1 Peer Cell Discovery

Cells at the same hierarchy level discover each other for:
- Handoff coordination
- Mutual support
- De-confliction

### 10.2 Handoff Protocol

When a tracked entity moves between cell coverage areas:

```
    Cell A (tracking)            Cell B (receiving)
         │                             │
         │                        (detects target entering AOI)
         │                             │
         │<───── HandoffRequest ───────│
         │   (track_id, my_coverage)   │
         │                             │
         │────── HandoffOffer ────────>│
         │   (track_history, sensor)   │
         │                             │
         │<───── HandoffAccept ────────│
         │                             │
         │   (A stops tracking)        │
         │                             │
```

### 10.3 Mutual Support

Cells can request support from peers:

```protobuf
message SupportRequest {
    bytes requesting_cell = 1;
    SupportType type = 2;
    Position location = 3;
    Priority priority = 4;
    Duration duration = 5;
}

enum SupportType {
    SUPPORT_TYPE_UNSPECIFIED = 0;
    SUPPORT_TYPE_SENSOR = 1;      // Need sensor coverage
    SUPPORT_TYPE_RELAY = 2;       // Need comm relay
    SUPPORT_TYPE_ACTUATOR = 3;    // Need actuation capability
    SUPPORT_TYPE_LOGISTICS = 4;   // Need resupply
}
```

---

## 11. Security Considerations

### 11.1 Formation Key

The Formation ID MUST be:
- Pre-shared out-of-band
- At least 256 bits of entropy
- Rotated periodically or after compromise

### 11.2 Leader Authority

Leaders can:
- Assign roles
- Accept/reject members
- Dissolve cell

Leaders MUST NOT:
- Forge member messages
- Bypass formation authentication
- Override human authority (unless autonomous mode)

### 11.3 Hierarchy Trust

- Child cells trust parent commands (verified by signature)
- Parent cells trust child reports (verified by signature)
- Sibling cells verify each other before coordination

### 11.4 Replay Protection

All coordination messages include:
- Timestamp (reject if > 30 seconds old)
- Nonce (track in replay cache)
- Sequence number (per sender)

---

## Appendix A: References

- Raft Consensus Algorithm (leader election inspiration)
- STANAG 4586 (UAV interoperability)
- ADR-004: Human-Machine Cell Composition
- ADR-014: Distributed Coordination Primitives
- ADR-024: Flexible Hierarchy Strategies
- ADR-027: Event Routing Aggregation Protocol

## Appendix B: Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-07 | Initial draft |
