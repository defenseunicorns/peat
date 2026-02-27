# PEAT Protocol Specification

```
Internet-Draft                                              K. Plummer
Intended Status: Standards Track                       (r)evolve, Inc.
                                                           December 2025

      Hierarchical Intelligence for Versatile Entities (PEAT) Protocol
                        draft-peat-protocol-00
```

## Abstract

This document specifies the Hierarchical Intelligence for Versatile Entities (PEAT) Protocol, a distributed coordination protocol for autonomous systems operating in constrained, partition-prone networks. PEAT enables scalable coordination through CRDT-based hierarchical capability composition, achieving O(n log n) message complexity while maintaining eventual consistency guarantees.

## Status of This Memo

This Internet-Draft is submitted in full conformance with the provisions of BCP 78 and BCP 79.

Internet-Drafts are working documents of the Internet Engineering Task Force (IETF). Note that other groups may also distribute working documents as Internet-Drafts.

## Copyright Notice

This specification is released under CC BY 4.0. The Protocol Buffer definitions are released under CC0 1.0 (public domain).

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Protocol Overview](#3-protocol-overview)
4. [Node Model](#4-node-model)
5. [Capability Model](#5-capability-model)
6. [Phase 1: Discovery](#6-phase-1-discovery)
7. [Phase 2: Cell Formation](#7-phase-2-cell-formation)
8. [Phase 3: Hierarchical Operations](#8-phase-3-hierarchical-operations)
9. [CRDT Semantics](#9-crdt-semantics)
10. [Message Complexity Analysis](#10-message-complexity-analysis)
11. [Security Considerations](#11-security-considerations)
12. [IANA Considerations](#12-iana-considerations)
13. [References](#13-references)

---

## 1. Introduction

### 1.1 Problem Statement

Autonomous systems operating in tactical environments face a fundamental coordination challenge: traditional all-to-all communication architectures exhibit O(n²) message complexity, causing network saturation at approximately 20 platforms on bandwidth-constrained links (9.6Kbps - 1Mbps).

Existing approaches suffer from:

- **Centralized architectures**: Single points of failure incompatible with partition-prone networks
- **Consensus protocols** (Paxos, Raft): Require majority availability, fail during partitions
- **Broadcast mesh**: O(n²) scaling limits practical deployment to small teams

### 1.2 Solution Overview

PEAT addresses these challenges through:

1. **Hierarchical organization**: Bounded cells with elected leaders reduce message paths
2. **CRDT-based state**: Eventual consistency without coordination overhead
3. **Capability composition**: Team capabilities emerge from individual platform capabilities
4. **Differential updates**: Only changes propagate, reducing bandwidth by 95%+

### 1.3 Design Goals

| Goal | Target |
|------|--------|
| Message complexity | O(n log n) vs O(n²) baseline |
| Bandwidth reduction | 95%+ via differential updates |
| Priority 1 latency | < 5 seconds through 4-level hierarchy |
| Scale | 100+ nodes validated, 1000+ architected |
| Partition tolerance | Full operation during network splits |

### 1.4 Scope

This specification defines:

- Node and capability data models
- Three-phase protocol operation (Discovery, Cell Formation, Hierarchical Operations)
- CRDT semantics for state synchronization
- Hierarchical aggregation and command dissemination
- Message formats (Protocol Buffers)

This specification does NOT define:

- Transport layer binding (implementation-specific)
- CRDT implementation details (use conforming CRDT library)
- Physical/link layer requirements
- Specific platform integrations

---

## 2. Terminology

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in BCP 14 [RFC2119] [RFC8174] when, and only when, they appear in all capitals, as shown here.

### 2.1 Protocol Terms

| Term | Definition |
|------|------------|
| **Node** | A single platform (UAV, UGV, sensor, soldier system) participating in the PEAT mesh |
| **Cell** | A bounded group of nodes (typically 5-8) with a single elected leader |
| **Beacon** | Discovery broadcast message advertising node presence and capabilities |
| **Capability** | A discrete function a node or cell can perform (sense, compute, relay, etc.) |
| **Composition** | The process of aggregating individual capabilities into team capabilities |
| **Phase** | One of three operational stages: Discovery, Cell, Hierarchy |

### 2.2 CRDT Terms

| Term | Definition |
|------|------------|
| **CRDT** | Conflict-free Replicated Data Type - data structure that can be replicated and updated independently with guaranteed convergence |
| **LWW-Register** | Last-Writer-Wins Register - CRDT where most recent write (by timestamp) wins |
| **G-Set** | Grow-only Set - CRDT supporting only additions, never removals |
| **OR-Set** | Observed-Remove Set - CRDT supporting additions and removals |
| **PN-Counter** | Positive-Negative Counter - CRDT supporting increment and decrement |

### 2.3 Military Terms

| Term | Definition |
|------|------------|
| **Squad** | Smallest tactical unit, typically 5-8 personnel/platforms (maps to Cell) |
| **Platoon** | 3-4 squads, typically 24-32 personnel/platforms |
| **Company** | 3-4 platoons, typically 96-128 personnel/platforms |
| **Echelon** | Level in the military hierarchy |

---

## 3. Protocol Overview

### 3.1 Three-Phase Operation

PEAT operates in three sequential phases:

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│   Phase 1:       │     │   Phase 2:       │     │   Phase 3:       │
│   DISCOVERY      │ ──▶ │   CELL           │ ──▶ │   HIERARCHY      │
│                  │     │                  │     │                  │
│ • Beacon broadcast│     │ • Cell formation │     │ • Normal ops     │
│ • Peer discovery │     │ • Leader election│     │ • Aggregation    │
│ • Geohash bucket │     │ • Capability     │     │ • Commands       │
│                  │     │   exchange       │     │ • Differential   │
│                  │     │                  │     │   updates        │
└──────────────────┘     └──────────────────┘     └──────────────────┘
     O(√n)                    O(k²)                   O(n log n)
```

### 3.2 Phase Transitions

Nodes MUST start in `PHASE_DISCOVERY`.

Transitions:

1. **DISCOVERY → CELL**: When node joins or forms a cell with sufficient members
2. **CELL → HIERARCHY**: When cell has elected leader and is assigned to a zone
3. **Regression**: Nodes MAY regress to earlier phases on partition recovery

### 3.3 Data Flow Architecture

```
                    ┌─────────────────────────────────┐
                    │         Company Summary         │
                    │    (aggregated from platoons)   │
                    └─────────────────────────────────┘
                                    ▲
                    ┌───────────────┴───────────────┐
                    │                               │
           ┌────────┴────────┐             ┌───────┴────────┐
           │ Platoon Summary │             │ Platoon Summary│
           │ (from squads)   │             │ (from squads)  │
           └────────┬────────┘             └───────┬────────┘
                    │                              │
        ┌───────────┼───────────┐                  │
        │           │           │                  │
   ┌────┴────┐ ┌────┴────┐ ┌────┴────┐       ┌────┴────┐
   │ Squad   │ │ Squad   │ │ Squad   │       │ Squad   │
   │ Summary │ │ Summary │ │ Summary │       │ Summary │
   └────┬────┘ └────┬────┘ └────┬────┘       └────┬────┘
        │           │           │                 │
     Nodes       Nodes       Nodes             Nodes
```

**Upward flow** (data/status): Individual state → Squad summary → Platoon summary → Company summary

**Downward flow** (commands): Company → Platoons → Squads → Individual nodes

---

## 4. Node Model

### 4.1 Node Structure

A Node consists of static configuration (`NodeConfig`) and dynamic state (`NodeState`).

```
Node
├── NodeConfig (immutable)
│   ├── id: UUID v4 (REQUIRED)
│   ├── platform_type: string (REQUIRED)
│   ├── capabilities: [Capability] (REQUIRED, G-Set)
│   ├── comm_range_m: float (OPTIONAL)
│   ├── max_speed_mps: float (OPTIONAL)
│   ├── operator_binding: HumanMachinePair (OPTIONAL)
│   └── created_at: Timestamp (OPTIONAL)
│
└── NodeState (CRDT-backed)
    ├── position: Position (REQUIRED, LWW-Register)
    ├── fuel_minutes: uint32 (OPTIONAL, PN-Counter)
    ├── health: HealthStatus (REQUIRED, LWW-Register)
    ├── phase: Phase (REQUIRED, LWW-Register)
    ├── cell_id: string (OPTIONAL, LWW-Register)
    ├── zone_id: string (OPTIONAL, LWW-Register)
    └── timestamp: Timestamp (REQUIRED)
```

### 4.2 Node Identity

- Node `id` MUST be a valid UUID version 4
- Node `id` MUST be unique across the entire mesh
- Node `id` MUST NOT change during node lifetime

### 4.3 Health Status

Implementations MUST support the following health states:

| Status | Value | Description |
|--------|-------|-------------|
| NOMINAL | 1 | Fully operational |
| DEGRADED | 2 | Reduced capability but operational |
| CRITICAL | 3 | Failure imminent, limited operations |
| FAILED | 4 | Non-operational |

Nodes with `FAILED` health SHOULD be excluded from:
- Leader election candidates
- Capability aggregation
- Active mission assignment

### 4.4 Human-Machine Teaming

When a node has an associated human operator (`operator_binding`), the operator's rank and authority level affect:

1. **Leader election scoring**: Higher rank/authority increases leadership score
2. **Command authorization**: ROE may require specific authority levels
3. **Cognitive load adjustment**: Degraded operator performance reduces effective authority

---

## 5. Capability Model

### 5.1 Capability Types

| Type | Value | Description |
|------|-------|-------------|
| SENSOR | 1 | Sensing: cameras, radar, sonar, SIGINT |
| COMPUTE | 2 | Processing: inference, analysis |
| COMMUNICATION | 3 | Relay, mesh networking, BLOS |
| MOBILITY | 4 | Flight, ground movement, maritime |
| PAYLOAD | 5 | Cargo, weapons, countermeasures |
| EMERGENT | 6 | Created through composition |

### 5.2 Capability Composition

Cell leaders aggregate member capabilities using four composition patterns:

#### 5.2.1 Additive Composition

Sum individual capabilities:

```
team_capability = Σ individual_capabilities
```

Example: Total sensor coverage = sum of individual coverage areas

#### 5.2.2 Emergent Composition

New capabilities from combinations:

```
IF (sensor ∈ team AND compute ∈ team AND comms ∈ team)
THEN team.add(ISR_Chain)
```

Example: ISR chain emerges when team has sensor, compute, and communications

#### 5.2.3 Redundant Composition

Improved reliability through overlap:

```
team_reliability = 1 - Π(1 - individual_reliability)
```

Example: Detection probability improves with multiple sensors

#### 5.2.4 Constraint Composition

Team limited by weakest/strongest member:

```
team_speed = min(individual_speeds)
team_range = max(individual_ranges)
```

---

## 6. Phase 1: Discovery

### 6.1 Beacon Broadcasting

During Phase 1, nodes MUST:

1. Broadcast `Beacon` messages at configurable intervals (default: 1 second)
2. Include current position, capabilities, and state in beacons
3. Increment `sequence_number` monotonically with each beacon
4. Process received beacons from peers

### 6.2 Geographic Scoping

To achieve O(√n) discovery complexity, implementations SHOULD use geographic hashing:

1. Compute geohash from current position (precision 5-6)
2. Broadcast beacons only within geohash bucket
3. Query neighboring buckets for boundary conditions

### 6.3 Beacon TTL

For multi-hop relay:

1. Initial beacon TTL SHOULD be 3 (configurable)
2. Each relay node decrements TTL
3. Beacons with TTL=0 MUST NOT be relayed

### 6.4 Phase Transition

Node transitions to `PHASE_CELL` when:

- Sufficient peers discovered (implementation-defined threshold)
- OR cell formation request received
- OR C2 directive received

---

## 7. Phase 2: Cell Formation

### 7.1 Cell Structure

A Cell consists of:

- 1 elected leader
- 4-11 members (configurable, default max_size=8)
- Aggregated capabilities

### 7.2 Leader Election

Leader election uses deterministic scoring:

```
score = technical_score × technical_weight + authority_score × authority_weight
```

Where:
- `technical_score` = f(compute, comms, sensors, power, reliability)
- `authority_score` = f(rank, authority_level, cognitive_load, fatigue) [if human present]
- Weights are policy-configurable

Tie-breaking: Lexicographically lowest node ID wins.

### 7.3 Membership Protocol

1. **Join Request**: Node sends `CellFormationRequest` to discovered peers
2. **Join Response**: Existing cell leader responds with `CellFormationResponse`
3. **State Update**: New member updates `cell_id` in `NodeState`
4. **Capability Recomputation**: Leader recomputes aggregated capabilities

### 7.4 Phase Transition

Cell transitions to `PHASE_HIERARCHY` when:

- Leader elected AND confirmed
- Minimum membership threshold met
- Zone assignment received (for multi-tier hierarchies)

---

## 8. Phase 3: Hierarchical Operations

### 8.1 Hierarchical Aggregation

Leaders at each level publish aggregated summaries:

| Level | Summary Type | Typical Size | Aggregates |
|-------|--------------|--------------|------------|
| Squad | `SquadSummary` | 5-8 nodes | Individual NodeState |
| Platoon | `PlatoonSummary` | 24-32 nodes | SquadSummary |
| Company | `CompanySummary` | 96-128 nodes | PlatoonSummary |

Each level achieves approximately 95% bandwidth reduction through:
- Averaging positions to centroids
- Summarizing health to worst-case
- Aggregating capabilities through composition

### 8.2 Summary Contents

All summaries MUST include:

- Unit identifier
- Leader identifier
- Member/subordinate count
- Position centroid
- Worst health status
- Operational count
- Aggregated capabilities
- Readiness score [0.0, 1.0]
- Aggregation timestamp

### 8.3 Command Dissemination

Commands flow downward through the hierarchy:

1. Originator creates `HierarchicalCommand` with target scope
2. Command propagates to target level (platoon/squad/individual)
3. Targets execute command and send `CommandAcknowledgment`
4. Acknowledgments flow upward to originator

### 8.4 Priority Handling

| Priority | Behavior |
|----------|----------|
| ROUTINE (1) | Normal queue processing |
| PRIORITY (2) | Expedited processing |
| IMMEDIATE (3) | Preempts lower priority |
| FLASH (4) | Immediate execution, conflict override |

---

## 9. CRDT Semantics

### 9.1 CRDT Types Used

| Data | CRDT Type | Merge Semantics |
|------|-----------|-----------------|
| Position | LWW-Register | Latest timestamp wins |
| Health | LWW-Register | Latest timestamp wins |
| Phase | LWW-Register | Latest timestamp wins |
| Capabilities | G-Set | Union of all observed |
| Cell members | OR-Set | Add/remove with tombstones |
| Fuel | PN-Counter | Sum of increments minus decrements |

### 9.2 Timestamp Requirements

- Timestamps MUST use Unix epoch with nanosecond precision
- Implementations SHOULD use synchronized time sources (NTP, GPS)
- For LWW semantics, implementations MUST ensure monotonically increasing timestamps per node

### 9.3 Conflict Resolution

When concurrent updates conflict:

1. **LWW fields**: Higher timestamp wins
2. **G-Set fields**: Union (all values preserved)
3. **OR-Set fields**: Observed-remove semantics (see CRDT literature)
4. **Tie-breaking**: Lexicographically lowest node ID wins

### 9.4 Consistency Guarantees

PEAT provides **eventual consistency**:

- All replicas converge to the same state
- No coordination required during partitions
- Updates are durable once observed by any node

PEAT does NOT provide:

- Linearizability
- Strong consistency
- Total ordering of operations

---

## 10. Message Complexity Analysis

### 10.1 Discovery Phase: O(√n)

Geographic hashing limits discovery to local peers:

- n nodes distributed over area A
- Geohash bucket covers area A/b
- Expected peers per bucket: n/b
- With appropriate b: O(√n) messages per node

### 10.2 Cell Formation: O(k²) per cell

Within cells of bounded size k:

- Full state exchange within cell: k² messages
- Total across n/k cells: O(kn)
- With constant k: O(n)

### 10.3 Hierarchical Operations: O(n log n)

With hierarchy depth d = log(n/k):

- Each level aggregates ~95% of updates
- Updates propagate through d levels
- Total: O(n log n)

### 10.4 Comparison to Baseline

| Architecture | Messages (100 nodes) | Messages (1000 nodes) |
|--------------|---------------------|----------------------|
| All-to-all | 9,900 | 999,000 |
| PEAT | ~664 | ~6,644 |
| Reduction | 93% | 99.3% |

---

## 11. Security Considerations

### 11.1 Authentication

Implementations SHOULD provide:

- Node identity authentication (PKI/certificates)
- Operator credential verification
- Message integrity (signatures)

### 11.2 Authorization

Implementations SHOULD enforce:

- Role-based access control for commands
- Authority level requirements for ROE
- Cell membership authorization

### 11.3 Confidentiality

For classified environments, implementations SHOULD provide:

- Message encryption (ChaCha20-Poly1305 RECOMMENDED)
- Key management (X25519 RECOMMENDED)
- Forward secrecy

### 11.4 Denial of Service

Implementations SHOULD mitigate:

- Beacon flooding (rate limiting)
- Invalid capability claims (validation)
- Leader election manipulation (deterministic scoring)

---

## 12. IANA Considerations

This document has no IANA actions at this time.

Future versions may request:

- Protocol port assignment
- Capability type registry
- Command type registry

---

## 13. References

### 13.1 Normative References

- [RFC2119] Bradner, S., "Key words for use in RFCs to Indicate Requirement Levels", BCP 14, RFC 2119, March 1997.
- [RFC8174] Leiba, B., "Ambiguity of Uppercase vs Lowercase in RFC 2119 Key Words", BCP 14, RFC 8174, May 2017.
- [RFC4122] Leach, P., Mealling, M., and R. Salz, "A Universally Unique IDentifier (UUID) URN Namespace", RFC 4122, July 2005.

### 13.2 Informative References

- Shapiro, M., Preguiça, N., Baquero, C., and M. Zawirski, "Conflict-free Replicated Data Types", SSS 2011.
- Kleppmann, M., "Making Sense of Stream Processing", O'Reilly Media, 2016.
- NATO STANAG 4586, "Standard Interfaces of UAV Control System (UCS) for NATO UAV Interoperability".

---

## Appendix A. Protocol Buffer Schema

The normative Protocol Buffer definitions are available in the `proto/` directory:

- `cap/v1/common.proto` - Common types
- `cap/v1/node.proto` - Node model
- `cap/v1/capability.proto` - Capability model
- `cap/v1/cell.proto` - Cell model
- `cap/v1/beacon.proto` - Discovery beacons
- `cap/v1/composition.proto` - Composition rules
- `cap/v1/hierarchy.proto` - Hierarchical summaries
- `cap/v1/command.proto` - Command dissemination

---

## Appendix B. Example Message Flows

### B.1 Discovery and Cell Formation

```
Node A                    Node B                    Node C
  │                         │                         │
  │──── Beacon ────────────▶│                         │
  │                         │──── Beacon ────────────▶│
  │◀──────────── Beacon ────│                         │
  │                         │◀──────────── Beacon ────│
  │◀─────────────────────────────────── Beacon ───────│
  │                         │                         │
  │── CellFormationReq ────▶│                         │
  │                         │── CellFormationReq ────▶│
  │◀── CellFormationResp ───│                         │
  │                         │◀── CellFormationResp ───│
  │                         │                         │
  │        [Leader Election: B wins]                  │
  │                         │                         │
  │◀─── SquadSummary ───────│─── SquadSummary ───────▶│
```

### B.2 Hierarchical Command Flow

```
Company Cmd ─────▶ Platoon Leader ─────▶ Squad Leader ─────▶ Node
                        │                     │               │
                        │◀──── Squad Ack ─────│◀──── Ack ─────│
    ◀── Platoon Ack ────│                     │               │
```

---

## Author's Address

Kit Plummer
(r)evolve, Inc.
Email: kit@revolveteam.com
GitHub: https://github.com/defenseunicorns/peat
