# ADR-001: CAP Protocol Proof-of-Concept Architecture

**Status:** Proposed  
**Date:** 2025-10-28  
**Decision Makers:** Research Team  
**Technical Story:** Implement CAP protocol to demonstrate hierarchical capability composition at scale

## Context and Problem Statement

Current autonomous systems architectures fail at scale due to O(n²) message complexity. The DIU COD experience confirmed that all-to-all communication topologies saturate around 10-20 nodes. We need to demonstrate that hierarchical capability composition using CRDTs can:

1. Reduce message complexity from O(n²) to O(n log n)
2. Support 100+ simulated nodes on constrained networks
3. Discover and advertise emergent team capabilities
4. Maintain eventual consistency despite network partitions

**Core Challenge:** How do nodes discovery from chaos into hierarchical organization without triggering O(n²) discovery overhead?

## Decision Drivers

### Primary Requirements
- **Scalability:** Support 100+ nodes in simulation, architected for 1000+
- **Network Efficiency:** 95%+ bandwidth reduction through differential updates
- **Latency Bounds:** Priority 1 updates propagate through hierarchy in <5 seconds
- **Eventual Consistency:** CRDT guarantees despite arbitrary network partitions
- **Real-Time Capable:** Fast enough for tactical decision-making

### Technical Constraints
- Must use Ditto Rust SDK for CRDT synchronization
- Target: Linux x86_64, ARM64 (future: embedded systems)
- Network: Support 9.6Kbps - 1Mbps with 100ms - 5s latency
- Language: Rust for safety, performance, and embedded future

### Validation Requirements
- Demonstrate all 3 operational phases
- Measure message complexity vs. node count
- Prove differential updates reduce bandwidth
- Show capability composition creates emergent behaviors

## Considered Options

### Option 1: Full-Scale Implementation
Build production-ready system with all features from day one.
- **Pros:** Complete validation, no throwaway work
- **Cons:** High risk, long timeline, unclear what actually matters

### Option 2: Minimal Viable Protocol (SELECTED)
Focus on core protocol phases with simplified capability model.
- **Pros:** Fast validation, iterative refinement, clear success criteria
- **Cons:** May need refactoring for production

### Option 3: Simulation-Only Approach
Build pure simulation with mocked networking.
- **Cons:** Doesn't validate real CRDT behavior or network constraints
- **Pros:** Faster development

## Decision Outcome

**Chosen Option:** Option 2 - Minimal Viable Protocol

Build a Rust library + reference application that demonstrates:
1. Three-phase protocol operation
2. CRDT-based capability representation using Ditto
3. Hierarchical message routing with O(n log n) complexity
4. Differential update generation and propagation
5. Basic capability composition patterns

**Out of Scope for POC:**
- Production deployment hardening
- Advanced mission planning
- Security/cryptography
- Multi-language bindings
- Embedded node ports

## Technical Approach

### Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                   Reference Application                 │
│  (Simulation Harness + Visualization + Metrics)        │
└─────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────┐
│                    CAP Protocol Library                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │   Phase 1:   │  │   Phase 2:   │  │   Phase 3:   │ │
│  │  Discovery   │→ │    Cell     │→ │ Hierarchical │ │
│  │             │  │  Formation   │  │   Operations │ │
│  └──────────────┘  └──────────────┘  └──────────────┘ │
│  ┌──────────────────────────────────────────────────┐  │
│  │        Capability Composition Engine             │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Differential Update Generator            │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────┐
│                   Ditto Rust SDK                        │
│         (CRDT Synchronization & Storage)                │
└─────────────────────────────────────────────────────────┘
```

### Core Components

#### 1. Node Agent
Represents individual autonomous node with:
- **Static Config:** Sensors, compute, protocols (G-Set CRDT)
- **Dynamic State:** Position, fuel, health (LWW-Register CRDT)
- **Capability Vector:** Available capabilities with confidence scores
- **Message Router:** Enforces hierarchical communication boundaries

#### 2. Cell Coordinator
Manages cell-level operations:
- **Member Registry:** OR-Set of active cell members
- **Leader Election:** Deterministic based on capability score
- **Capability Aggregator:** Composes node capabilities into cell capabilities
- **Upward Reporting:** Generates compressed cell summaries

#### 3. Capability Composition Engine
Implements composition patterns:
- **Additive:** Coverage area, lift capacity (sum)
- **Emergent:** ISR chain, 3D mapping (logical AND of requirements)
- **Redundant:** Detection reliability (probabilistic combination)
- **Constraint-Based:** Team speed, range (min/max)

#### 4. Differential Update System
Generates and applies deltas:
- **Change Detection:** Track modifications to CRDT state
- **Delta Generation:** Create minimal change descriptors
- **Priority Assignment:** P1 (immediate) → P4 (bulk)
- **Compression:** Achieve 95%+ reduction vs. full state

#### 5. Network Simulation Layer
Models realistic constraints:
- **Bandwidth Limiting:** 9.6Kbps - 1Mbps configurable
- **Latency Injection:** 100ms - 5s variable
- **Packet Loss:** 0-30% configurable
- **Partition Simulation:** Network split/merge scenarios

### Data Models

#### Node Capability Document (Ditto Collection: "nodes")
```rust
{
  "node_id": String,
  "static_config": {
    "sensors": Set<String>,           // G-Set CRDT
    "compute_power": f32,
    "protocols": Set<String>,
    "node_type": String
  },
  "dynamic_state": {
    "position": {                     // LWW-Register CRDT
      "lat": f64,
      "lon": f64, 
      "alt": f64,
      "timestamp": u64
    },
    "fuel_minutes": u32,              // PN-Counter CRDT
    "health": String,                 // LWW-Register
    "tasking": String
  },
  "capabilities": {
    "solo": Vec<Capability>,
    "confidence": Map<String, f32>
  },
  "cell_id": Option<String>,         // LWW-Register
  "phase": String                     // "discovery" | "cell" | "hierarchical"
}
```

#### Cell Capability Document (Ditto Collection: "cells")
```rust
{
  "cell_id": String,
  "leader_id": String,                // LWW-Register
  "members": Set<String>,             // OR-Set CRDT
  "cell_capabilities": {
    "coverage_area_km2": f32,
    "endurance_minutes": u32,
    "emergent": Vec<EmergentCapability>
  },
  "compressed_summary": {             // For upward reporting
    "node_count": usize,
    "mission_capabilities": Map<String, f32>,
    "readiness": String
  },
  "zone_id": Option<String>
}
```

#### Capability Change Delta
```rust
{
  "node_id": String,
  "timestamp": u64,
  "priority": u8,                     // 1-4
  "operations": Vec<{
    "op_type": String,                // "LWW" | "G_Set_Add" | "PN_Counter_Inc"
    "field": String,
    "value": serde_json::Value
  }>,
  "ttl_seconds": u32                  // Time-to-live for update
}
```

### Three-Phase Protocol Operation

#### Phase 1: Discovery (Constrained Discovery)
**Goal:** Form initial groups without O(n²) overhead

**Process:**
1. Node joins network, broadcasts existence ONCE
2. Listens for cell beacon messages (not all nodes)
3. Uses one of three strategies:
   - **Geographic:** Self-assign to grid cell, discover local peers
   - **Capability Query:** Respond if matching C2 query
   - **C2 Directed:** Accept explicit cell assignment

**Message Complexity:** O(√n) using geographic hashing or O(k) where k << n for queries

**Success Criteria:** 
- 100 nodes organize in <60 seconds
- Total messages < 1000 (vs. 10,000 for all-to-all)

#### Phase 2: Cell Formation
**Goal:** Establish cell cohesion and elect leader

**Process:**
1. Intra-cell capability exchange (O(k²) where k=cell size ~5)
2. Deterministic leader election: highest capability score
3. Cell leader computes aggregated capabilities
4. Roles assigned based on complementarity

**Message Complexity:** O(k²) within cell, isolated from network scale

**Success Criteria:**
- Leader election converges in <5 seconds
- Emergent capabilities discovered and advertised
- Cell ready for mission tasking

#### Phase 3: Hierarchical Operations
**Goal:** Maintain capabilities while enforcing hierarchy

**Process:**
1. Nodes send deltas to cell leader only
2. Cell leaders aggregate and send to platoon
3. Zone leaders aggregate and send to company
4. Priority routing ensures critical updates propagate fast

**Message Complexity:** O(n log n) - each node sends to ~5 peers

**Success Criteria:**
- Priority 1 updates reach top in <5 seconds
- Network utilization <10% of available bandwidth
- Capability staleness <30 seconds for 90% of updates

### Capability Composition Rules

#### Rule 1: Additive (Coverage Area)
```rust
fn compose_coverage(nodes: &[Node]) -> f32 {
    nodes.iter()
        .map(|p| p.coverage_area_km2)
        .sum()
}
```

#### Rule 2: Emergent (ISR Chain)
```rust
fn compose_isr_chain(nodes: &[Node]) -> Option<ISRCapability> {
    let has_sensor = nodes.iter().any(|p| p.has_sensor());
    let has_compute = nodes.iter().any(|p| p.compute_power > 10.0);
    let has_comms = nodes.iter().any(|p| p.has_satcom());
    
    if has_sensor && has_compute && has_comms {
        Some(ISRCapability {
            coverage: compose_coverage(nodes),
            resolution: nodes.iter()
                .filter_map(|p| p.resolution)
                .max()?,
            persistence: compute_overlap_schedule(nodes)
        })
    } else {
        None
    }
}
```

#### Rule 3: Redundant (Detection Reliability)
```rust
fn compose_detection_reliability(nodes: &[Node]) -> f32 {
    let failure_prob = nodes.iter()
        .map(|p| 1.0 - p.detection_probability)
        .product();
    1.0 - failure_prob
}
```

#### Rule 4: Constraint-Based (Team Speed)
```rust
fn compose_team_speed(nodes: &[Node]) -> f32 {
    nodes.iter()
        .map(|p| p.max_speed_mps)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
}
```

## Functional Requirements

### FR-1: Node Lifecycle Management
- FR-1.1: Node shall initialize with static configuration
- FR-1.2: Node shall update dynamic state at 1Hz
- FR-1.3: Node shall detect and advertise capability changes
- FR-1.4: Node shall gracefully handle join/leave events

### FR-2: Discovery Phase
- FR-2.1: System shall support geographic self-organization
- FR-2.2: System shall support C2-directed assignment
- FR-2.3: System shall support capability-based queries
- FR-2.4: Discovery shall complete in <60s for 100 platforms
- FR-2.5: Discovery message count shall be O(√n) or better

### FR-3: Cell Formation
- FR-3.1: Cell shall elect leader deterministically
- FR-3.2: Cell shall compute aggregated capabilities
- FR-3.3: Cell shall identify emergent capabilities
- FR-3.4: Cell formation shall converge in <5 seconds
- FR-3.5: Cell size shall be configurable (default: 5 nodes)

### FR-4: Hierarchical Operations
- FR-4.1: Nodes shall only communicate with cell peers
- FR-4.2: Cell leaders shall communicate with zone level
- FR-4.3: Message routing shall enforce hierarchical boundaries
- FR-4.4: Cross-cell communication shall be prohibited
- FR-4.5: Hierarchy depth shall be configurable (default: 4 levels)

### FR-5: Capability Composition
- FR-5.1: System shall implement additive composition
- FR-5.2: System shall discover emergent capabilities
- FR-5.3: System shall compute redundant capability improvements
- FR-5.4: System shall apply constraint-based limits
- FR-5.5: Composition rules shall be extensible

### FR-6: Differential Updates
- FR-6.1: System shall generate deltas for all state changes
- FR-6.2: Deltas shall be <5% size of full state on average
- FR-6.3: System shall assign priority levels (1-4) to updates
- FR-6.4: Priority 1 updates shall propagate in <5 seconds
- FR-6.5: Stale updates (past TTL) shall be dropped

### FR-7: Network Simulation
- FR-7.1: Simulator shall support configurable bandwidth limits
- FR-7.2: Simulator shall inject configurable latency
- FR-7.3: Simulator shall simulate packet loss
- FR-7.4: Simulator shall support network partition scenarios
- FR-7.5: Simulator shall log all network events for analysis

### FR-8: Metrics and Observability
- FR-8.1: System shall measure message count vs. node count
- FR-8.2: System shall measure update latency by priority
- FR-8.3: System shall measure bandwidth utilization
- FR-8.4: System shall measure capability staleness
- FR-8.5: System shall export metrics in JSON format

### FR-9: Reference Application
- FR-9.1: Application shall simulate 100+ platforms
- FR-9.2: Application shall visualize node organization
- FR-9.3: Application shall display capability composition
- FR-9.4: Application shall show real-time metrics
- FR-9.5: Application shall support scenario replay

### FR-10: Data Persistence
- FR-10.1: System shall persist node state via Ditto
- FR-10.2: System shall persist cell state via Ditto
- FR-10.3: System shall maintain change history
- FR-10.4: System shall support state snapshots
- FR-10.5: System shall recover from crashes

## Non-Functional Requirements

### NFR-1: Performance
- Node state update processing: <10ms p99
- Delta generation: <5ms p99
- Capability composition: <20ms p99
- Leader election convergence: <5 seconds

### NFR-2: Scalability
- Support 100+ nodes in POC
- Architecture for 1000+ platforms
- Memory per node: <10MB
- CPU per node: <5% of one core

### NFR-3: Reliability
- Handle 30% packet loss without data loss
- Survive network partitions with eventual consistency
- Gracefully handle node failures
- No message amplification cascades

### NFR-4: Maintainability
- Modular architecture with clear interfaces
- Comprehensive unit test coverage (>80%)
- Integration test scenarios
- API documentation with examples

### NFR-5: Extensibility
- Pluggable composition rules
- Configurable hierarchy structures
- Custom capability types
- Alternative discovery strategies

## Implementation Technology Stack

### Core Technologies
- **Language:** Rust 1.70+ (2021 edition)
- **CRDT Engine:** Ditto Rust SDK
- **Async Runtime:** Tokio 1.x
- **Serialization:** Serde + serde_json
- **Networking:** Tokio TcpStream (simulated constraints)

### Development Tools
- **Build:** Cargo + cargo-make
- **Testing:** cargo test + proptest for property testing
- **Benchmarking:** criterion.rs
- **Linting:** clippy
- **Formatting:** rustfmt

### Dependencies
```toml
[dependencies]
dittolive-ditto = "4.x"     # CRDT synchronization
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"             # Logging
tracing-subscriber = "0.3"
thiserror = "1"             # Error handling
anyhow = "1"                # Error propagation

[dev-dependencies]
criterion = "0.5"           # Benchmarking
proptest = "1"              # Property testing
```

## Success Metrics

### Primary Metrics
1. **Scalability:** Message complexity is O(n log n) - measured at 10, 50, 100 platforms
2. **Efficiency:** Differential updates achieve 95%+ bandwidth reduction
3. **Latency:** Priority 1 updates propagate in <5 seconds through 4-level hierarchy
4. **Discovery:** Discovery completes in <60 seconds for 100 platforms

### Secondary Metrics
1. Emergent capabilities discovered (>3 types demonstrated)
2. Network partition recovery time (<30 seconds to reconverge)
3. Memory efficiency (<10MB per node)
4. CPU efficiency (<5% per node on modern CPU)

## Risks and Mitigations

### Risk 1: Ditto SDK Limitations
**Risk:** Ditto Rust SDK may not expose needed CRDT operations  
**Mitigation:** Early spike to validate SDK capabilities, fallback to wrapper layer

### Risk 2: Network Simulation Fidelity
**Risk:** Simulated network may not reflect real constraints  
**Mitigation:** Validate against known DIU COD results, calibrate from literature

### Risk 3: Composition Rule Complexity
**Risk:** Real military capability patterns may be more complex than modeled  
**Mitigation:** Start with 4 simple patterns, design for extensibility

### Risk 4: Performance at Scale
**Risk:** 100+ nodes may exceed Rust/Ditto performance envelope  
**Mitigation:** Profile early, optimize hot paths, consider reduced simulation fidelity

## Future Enhancements (Post-POC)

1. **Security:** Capability authentication, encrypted comms
2. **Mission Planning:** Task decomposition and allocation
3. **Learning:** ML-based composition rule discovery
4. **Hardware:** Port to embedded nodes (ARM Cortex-M)
5. **Multi-Language:** FFI bindings for Python/C++
6. **Production:** Deployment hardening, monitoring, fault injection

## References

1. Capabilities Aggregation Framework (project document)
2. CAP Proposal: CRDT-Based Hierarchical Capability Composition
3. CAP Problem Statement: Breaking the N² Barrier
4. Ditto Rust SDK Documentation: https://docs.ditto.live/rust/

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-10-28 | Use Rust + Ditto | Safety, performance, proven CRDT implementation |
| 2025-10-28 | Three-phase protocol | Matches CAP specification exactly |
| 2025-10-28 | POC targets 100 nodes | Sufficient to demonstrate O(n log n) vs. O(n²) |
| 2025-10-28 | Simulated networking | Enables rapid iteration vs. hardware testbed |

---

**Last Updated:** 2025-10-28  
**Next Review:** After Week 4 implementation
