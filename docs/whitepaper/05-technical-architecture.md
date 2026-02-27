## IV. TECHNICAL ARCHITECTURE

**Thesis:** PEAT achieves scalable coordination through CRDT-based eventual consistency, hierarchical aggregation, and edge-native design.

---

### 4.1 Core Mechanisms

Coordination at scale requires different foundations than traditional distributed systems.

#### CRDTs for Eventual Consistency

PEAT uses Conflict-free Replicated Data Types (CRDTs), specifically Automerge documents, for all shared state. CRDTs guarantee:

- **Merge without coordination**: Any two replicas can merge deterministically
- **No consensus required**: Critical for intermittent connectivity
- **Partition tolerance**: Network splits don't cause conflicts—they're expected
- **Deterministic convergence**: All nodes reach the same state regardless of message order

```
Node A: Writes update → local merge → eventual propagation
Node B: Writes update → local merge → eventual propagation
        ↓                              ↓
        └──────── deterministic ───────┘
                    merge result
```

#### Why Consensus Fails

Traditional distributed consensus (Paxos, Raft, 2PC) requires majority availability. In constrained networks—wireless mesh, satellite links, intermittent connectivity—partitions are the norm, not the exception.

| Approach | Partition Behavior | Suitability |
|----------|-------------------|-------------|
| Consensus | Blocks until quorum | Always-connected only |
| CRDTs | Proceeds locally, merges later | Partition-tolerant |

PEAT assumes partitions. Nodes operate autonomously during disconnection and deterministically merge on reconnection.

#### Synchronization Protocol

PEAT uses Negentropy for efficient set reconciliation:

1. **Range-based comparison**: Nodes exchange fingerprints of document ranges
2. **Bisection**: Disagreements narrow recursively to specific changes
3. **Minimal transfer**: Only missing operations are exchanged

This achieves near-optimal sync efficiency—O(log n) rounds to reconcile n differences—even with significant divergence.

---

### 4.2 Hierarchical Aggregation

The bandwidth reduction comes from aggregation, not compression.

#### Differential Synchronization

Only changes propagate:
- CRDT operations, not full state snapshots
- Delta sync between hierarchy levels
- Summaries propagate upward; details stay local

#### Aggregation Algebra

Individual states combine into summaries appropriate for each level:

```
Level 5 (Node):      Full platform state (position, battery, sensors, tasks)
                            ↓ summarize
Level 4 (Team):      Team capabilities, coverage area, operational status
                            ↓ summarize
Level 3 (Group):     Aggregate capabilities, health metrics, resource availability
                            ↓ summarize
Level 2 (Formation): Strategic capability summary, readiness assessment
```

A coordinator at level 3 sees 3 team summaries instead of 12 individual node states—an 75% reduction in data volume at that level alone. Across the full hierarchy, reductions compound.

#### Validated Results

Laboratory validation confirms the architecture:

| Metric | Mesh Approach | PEAT Hierarchical |
|--------|--------------|-------------------|
| Bandwidth scaling | O(n²) | O(n log n) |
| Messages (100 nodes) | 10,000/cycle | ~700/cycle |
| Bandwidth reduction | Baseline | 93-99% |
| Partition recovery | Conflict resolution | Automatic merge |

Networks that saturate at 50 nodes with mesh coordination support 1,000+ with PEAT.

---

### 4.3 Five-Layer Architecture

PEAT separates concerns for flexibility and integration.

```
┌─────────────────────────────────────────────────────────────┐
│  LAYER 5: APPLICATION                                       │
│  Domain-specific integration: TAK/CoT, ROS2, custom apps    │
├─────────────────────────────────────────────────────────────┤
│  LAYER 4: BINDING (peat-ffi, peat-lite)                     │
│  Language bindings: C, Swift, Kotlin/Java, Python           │
├─────────────────────────────────────────────────────────────┤
│  LAYER 3: TRANSPORT (peat-transport, peat-mesh)             │
│  Network abstraction: QUIC/Iroh, UDP bypass, BLE mesh       │
├─────────────────────────────────────────────────────────────┤
│  LAYER 2: PROTOCOL (peat-protocol)                          │
│  Coordination logic: cell formation, aggregation, authority │
├─────────────────────────────────────────────────────────────┤
│  LAYER 1: SCHEMA (peat-schema)                              │
│  Message definitions: Protobuf, capability ontology         │
└─────────────────────────────────────────────────────────────┘
```

#### Layer 1: Schema (peat-schema)

Protocol buffer definitions for all PEAT messages:
- Beacons and capability advertisements
- Hierarchical commands and acknowledgments
- Track and mission data
- AI model descriptors

Language-agnostic, extensible for domain-specific needs.

#### Layer 2: Protocol (peat-protocol)

Core coordination logic:
- Cell formation and leader election
- Hierarchical routing and aggregation
- Authority enforcement and delegation
- Mission tracking and status aggregation

Pure logic, transport-independent.

#### Layer 3: Transport (peat-transport, peat-mesh)

Network abstraction layer:
- **Primary**: QUIC via Iroh for reliable, encrypted streams
- **Bypass**: Raw UDP for time-critical, loss-tolerant data
- **Mesh**: BLE mesh for short-range, infrastructure-free coordination

Multi-path capability: simultaneous use of multiple transports.

#### Layer 4: Binding (peat-ffi, peat-lite)

Platform integration:
- **peat-ffi**: C-compatible FFI for native library integration
- **peat-lite**: Embedded-friendly subset for resource-constrained devices

Bindings for Swift (iOS), Kotlin (Android), Python, and more.

#### Layer 5: Application

Domain-specific integration:
- **peat-tak-bridge**: TAK/ATAK interoperability via CoT translation
- **peat-commander**: Reference command interface
- Custom integrations per deployment

---

### 4.4 AI as Coordinator

AI participates in the hierarchy, not just at the edge.

**Traditional Model**: AI for perception on individual platforms. Each node runs inference locally; coordination is separate.

**PEAT Model**: AI as team member at every level:

| Level | AI Role |
|-------|---------|
| Node | Perception, local decisions |
| Team | Task allocation, peer optimization |
| Group | Resource balancing, pattern detection |
| Formation | Strategic optimization within constraints |

**Capability Advertisement**: AI capabilities are first-class citizens in the capability ontology. "Which nodes can identify [object type] with >90% confidence?" is a queryable property.

**Model Distribution**: AI models propagate through the hierarchy like any other data—downward dissemination, versioned, with rollback capability.

**Edge-Native**: Inference runs where decisions are made. Models work disconnected, sync when connected.

---

### 4.5 Validation Status

| Phase | Configuration | Results |
|-------|--------------|---------|
| Phase 1 | 2-node bidirectional | <1s latency, 100% consistency |
| Phase 2 | 12-node team | 26s full convergence |
| Phase 3 | 24-node group | 54s convergence, 6.1s mean |
| Phase 4 | Simulated 1,000+ | O(n log n) confirmed |

**Integration Pathways Validated**:
- TAK/CoT bridge: Real-time translation to ATAK
- UDP bypass: Sub-50ms latency for time-critical data
- BLE mesh: Infrastructure-free short-range coordination

**Technology Readiness**: TRL 4-5 (laboratory validated, integration demonstrated)

---

### Key Finding: Section IV

> "PEAT achieves O(n log n) scaling through hierarchical CRDT-based coordination—enabling 1,000+ node operations on networks that saturate at 50 with mesh approaches. The five-layer architecture enables flexible integration from embedded devices to enterprise systems."

---
