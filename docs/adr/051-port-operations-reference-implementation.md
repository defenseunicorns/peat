# ADR-030: Port Operations Reference Implementation — Container Terminal Coordination

**Status**: Proposed  
**Date**: 2026-02-06  
**Authors**: Kit Plummer, Art Recesso  
**Type**: Reference Implementation (parallel to Army echelon validation)  
**Relates to**: ADR-008 (Network Simulation Layer), ADR-015 (Experimental Validation), ADR-021 (Document-Oriented Architecture), ADR-012 (Schema Definition), ADR-027 (Event Routing)

## Context

### Why a Second Reference Implementation?

HIVE Protocol's Army echelon reference implementation (ADR-008, ADR-015) validates hierarchical CRDT coordination using military organizational structures: squad (12 nodes) → platoon (24 nodes) → company (112 nodes). This proves the protocol works for command-and-control hierarchy with constrained tactical networks.

However, HIVE's core claim is broader: **hierarchical capability aggregation works wherever heterogeneous entities — human, machine, AI — must coordinate toward shared goals under constraints.** A single reference domain doesn't prove that. A second domain does.

**The Port of Savannah container terminal operation provides this second domain.** It independently validates every core HIVE concept in a completely different operational context while adding validation dimensions the military domain cannot easily provide:

| Validation Dimension | Army Reference | Port Reference |
|---|---|---|
| Hierarchy structure | Fixed doctrine (squad→platoon→company) | Dynamic, goal-organized (berth→hold→team) |
| Entity types | Soldiers, UAVs, UGVs | Workers, cranes, tractors, AI agents, vessels |
| Capability model | Platform capabilities (sensors, weapons) | Workforce skills, certifications, equipment status |
| Goal activation | Mission orders (top-down) | Ship arrival event (bottom-up discovery) |
| Network constraints | DDIL (tactical radio, SATCOM) | Industrial wireless (WiFi, BLE, degraded RF in steel canyon) |
| Trust requirements | Classification, coalition sharing | OSHA, USCG, TWIC, union certification |
| Success metric | Decision latency, SA convergence | Container moves/hour, turnaround time |
| Verification | Classified environment limitations | Unclassified, measurable, commercially relevant |

### The Gastown Approach

This reference implementation will be built using Gastown as the engineering execution environment, paired with the HIVE/KnowledgeOptimized knowledge base. The Gastown approach means:

1. **Domain scenario presented** → Port of Savannah, MV Ever Forward arrival, 16,247 TEU, 72-hour turnaround
2. **Engineering team builds** → ContainerLab topology, entity models, capability schemas, experiment scripts
3. **Protocol validates** → Same HIVE CRDT synchronization, hierarchical aggregation, capability composition

This is not a separate codebase. It reuses the existing `hive-sim` infrastructure with new topology definitions, entity schemas, and validation scripts specific to port operations.

## Decision

We will create a parallel reference implementation modeling container terminal operations at the Port of Savannah, using the same ContainerLab infrastructure as the Army echelon experiments, to validate HIVE Protocol's domain-independence.

## Port Operations Hierarchy

### Operational Structure

The port terminal hierarchy is goal-organized, not doctrinally fixed. When a ship arrives, the hierarchy forms around the operational objective:

```
                    ┌─────────────────────────┐
                    │   TERMINAL OPERATIONS    │  H4 — AI scheduling,
                    │   CENTER (TOC)           │       yard optimization
                    └────────────┬────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                   │
    ┌─────────▼────────┐ ┌──────▼───────┐ ┌────────▼────────┐
    │   BERTH MANAGER  │ │ YARD MANAGER │ │  GATE MANAGER   │  H3 — Zone
    │   (Ship-side)    │ │  (Storage)   │ │  (Truck/Rail)   │  coordination
    └────────┬─────────┘ └──────┬───────┘ └────────┬────────┘
             │                  │                   │
    ┌────────┼────────┐         │            ┌──────┼──────┐
    │        │        │         │            │      │      │
  ┌─▼──┐  ┌─▼──┐  ┌──▼─┐   ┌──▼──┐     ┌───▼┐  ┌─▼──┐ ┌─▼──┐
  │Hold│  │Hold│  │Hold│   │Yard │     │Gate│  │Gate│ │Rail│  H2 — Work
  │ 1  │  │ 2  │  │ 3  │   │Block│     │ A  │  │ B  │ │ 1  │  groups
  └─┬──┘  └─┬──┘  └─┬──┘   └──┬──┘     └─┬──┘  └─┬──┘ └─┬──┘
    │        │       │         │           │       │      │
  ┌─▼──────────────────┐  ┌───▼────┐   ┌──▼──────────────────┐
  │ Crane, workers,    │  │Tractors│   │ Scanners, workers,  │  H1 — Assets
  │ tractors, sensors  │  │AGVs    │   │ RFID readers        │
  └────────────────────┘  └────────┘   └─────────────────────┘
                                                                 H0 — Sensors
                                                                 GPS, load cells,
                                                                 cameras, RFID
```

### Key Difference from Army Hierarchy

In the Army reference, the hierarchy is **pre-defined** by doctrine. A squad always has ~12 members in known roles. In port operations, the hierarchy is **goal-organized**: when MV Ever Forward arrives, the protocol dynamically forms teams around each hold based on available assets, required capabilities, and operational constraints. Teams reform as holds are completed.

This validates HIVE's **dynamic clustering** — the same protocol primitive that enables military units to reform around objectives rather than staying in static formations.

## Entity Model

### Node Types and Capability Schemas

Each entity in the port simulation is a HIVE node with capability assertions. Using ADR-012 schema extension patterns:

#### H0 — Sensors and Instruments

```
Entity: load_cell_01
  capabilities:
    - type: WEIGHT_MEASUREMENT
      range_tons: 65
      accuracy_pct: 0.5
      status: OPERATIONAL
      last_calibration: "2026-01-15T00:00:00Z"

Entity: rfid_reader_gate_a
  capabilities:
    - type: CONTAINER_IDENTIFICATION
      read_range_m: 12
      protocols: [ISO_18000-6C, EPC_GEN2]
      status: OPERATIONAL
```

#### H1 — Equipment and Workers

```
Entity: gantry_crane_07
  capabilities:
    - type: CONTAINER_LIFT
      lift_capacity_tons: 65
      reach_rows: 22
      moves_per_hour: 30
      status: OPERATIONAL
      maintenance_due: "2026-02-20"
    - type: HAZMAT_RATED
      classes: [1, 3, 8, 9]
      certification_expiry: "2026-06-15"

Entity: worker_martinez_j
  capabilities:
    - type: CRANE_OPERATION
      proficiency: EXPERT
      certification: "OSHA_1926.1400"
      cert_expiry: "2026-09-01"
    - type: HAZMAT_HANDLING
      proficiency: COMPETENT
      classes: [3, 8, 9]
      cert_expiry: "2025-12-01"        # EXPIRED — 67 days ago
      recent_handling_count: 47         # Evidence chain
      recent_handling_incidents: 0
    - type: LASHING
      proficiency: ADVANCED_BEGINNER

Entity: yard_tractor_142
  capabilities:
    - type: CONTAINER_TRANSPORT
      mode: SEMI_AUTONOMOUS
      load_capacity_tons: 40
      gps_tracked: true
      status: OPERATIONAL
      battery_pct: 78

Entity: ai_scheduler_alpha
  capabilities:
    - type: CONTAINER_SEQUENCING
      algorithm: "stow_plan_optimizer_v3"
      containers_per_second: 500
      optimization_targets: [TURNAROUND_TIME, CRANE_UTILIZATION, WEIGHT_BALANCE]
    - type: YARD_OPTIMIZATION
      algorithm: "block_allocation_ml_v2"
      status: ACTIVE
```

#### H2–H4 — Aggregation Entities

These are HIVE aggregation nodes that **do not exist as physical entities** — they're coordination abstractions that the protocol creates:

```
Entity: hold_3_team (H2 — dynamic cluster)
  aggregated_capabilities:
    - CONTAINER_LIFT: 2 cranes, combined_moves_per_hour: 58
    - CRANE_OPERATION: 3 workers (2 expert, 1 competent)
    - HAZMAT_HANDLING: 2 workers certified, 1 expired-with-evidence
    - CONTAINER_TRANSPORT: 8 tractors available
  status:
    moves_completed: 847
    moves_remaining: 1203
    current_rate: 38 moves/hour
    target_rate: 35 moves/hour

Entity: berth_manager_b5 (H3 — zone coordination)
  aggregated_capabilities:
    - holds: [hold_1_team, hold_2_team, hold_3_team, ...]
    - total_moves_per_hour: 185
    - hazmat_holds_status: {hold_3: ACTIVE, hold_7: PENDING_CERTIFICATION}
    - estimated_completion: "2026-02-08T14:00:00Z"
  gap_analysis:
    - type: HAZMAT_CRANE_OPERATOR
      required: 4
      available: 2
      adjacent: 3  # expired cert, strong evidence chain
```

## ContainerLab Topology

### Mapping to Simulation

The port topology parallels the Army topology structure from ADR-008:

| Army Concept | Port Concept | ContainerLab Mapping |
|---|---|---|
| Squad (12 nodes) | Hold Team (~15 nodes) | 1 hold team per topology instance |
| Platoon (24 nodes → 3 squads) | Berth Operation (~50 nodes) | 3-4 hold teams + yard allocation |
| Company (112 nodes) | Full Terminal Operation (~200 nodes) | Multiple berths + yard + gate |

### Phase 1: Hold Team (Single Hold Operation) — ~15 nodes

**Parallel to**: Army 12-node squad validation

This is the atomic unit: one ship hold being worked by a coordinated team.

```yaml
name: hive-port-hold-team

topology:
  nodes:
    # Gantry Cranes (2)
    crane-1:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: crane-1
        ROLE: gantry_crane
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        LIFT_CAPACITY_TONS: 65
        MOVES_PER_HOUR: 30
        HAZMAT_RATED: "true"

    crane-2:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: crane-2
        ROLE: gantry_crane
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        LIFT_CAPACITY_TONS: 50
        MOVES_PER_HOUR: 28
        HAZMAT_RATED: "false"

    # Workers (5)
    worker-lead:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: worker-lead
        ROLE: crane_operator
        ENTITY_TYPE: worker
        HIVE_LEVEL: H1
        PROFICIENCY: expert
        HAZMAT_CERT: "valid"
        CERT_CLASSES: "1,3,8,9"

    worker-2:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: worker-2
        ROLE: crane_operator
        ENTITY_TYPE: worker
        HIVE_LEVEL: H1
        PROFICIENCY: competent
        HAZMAT_CERT: "expired"
        HAZMAT_EVIDENCE_COUNT: 47
        HAZMAT_EVIDENCE_INCIDENTS: 0

    worker-3:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: worker-3
        ROLE: lashing_crew
        ENTITY_TYPE: worker
        HIVE_LEVEL: H1

    worker-4:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: worker-4
        ROLE: lashing_crew
        ENTITY_TYPE: worker
        HIVE_LEVEL: H1

    worker-5:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: worker-5
        ROLE: signaler
        ENTITY_TYPE: worker
        HIVE_LEVEL: H1

    # Yard Tractors (4)
    tractor-1:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: tractor-1
        ROLE: yard_tractor
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        MODE: semi_autonomous
        GPS_TRACKED: "true"

    tractor-2:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: tractor-2
        ROLE: yard_tractor
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        MODE: human_driven

    tractor-3:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: tractor-3
        ROLE: yard_tractor
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        MODE: semi_autonomous

    tractor-4:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: tractor-4
        ROLE: yard_tractor
        ENTITY_TYPE: equipment
        HIVE_LEVEL: H1
        MODE: human_driven

    # AI Scheduling Agent (1)
    ai-scheduler:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: ai-scheduler
        ROLE: scheduling_agent
        ENTITY_TYPE: ai_agent
        HIVE_LEVEL: H4
        ALGORITHM: stow_plan_optimizer_v3

    # Hold Aggregator (1) — HIVE protocol coordination node
    hold-aggregator:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: hold-3-aggregator
        ROLE: hold_team_leader
        ENTITY_TYPE: aggregator
        HIVE_LEVEL: H2
        HOLD_NUMBER: 3
        TARGET_MOVES_PER_HOUR: 35

    # Sensors (2)
    load-cell-1:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: load-cell-1
        ROLE: load_sensor
        ENTITY_TYPE: sensor
        HIVE_LEVEL: H0

    rfid-reader:
      kind: linux
      image: hive-sim-node:latest
      env:
        NODE_ID: rfid-reader
        ROLE: container_id_reader
        ENTITY_TYPE: sensor
        HIVE_LEVEL: H0

  links:
    # Industrial WiFi mesh — crane to crane (fiber backbone simulated)
    - endpoints: ["crane-1:eth1", "crane-2:eth1"]
      impairments:
        delay: 5ms
        jitter: 2ms
        loss: 0.1%
        rate: 10mbps

    # WiFi mesh — workers to cranes (standard industrial WiFi)
    - endpoints: ["worker-lead:eth1", "crane-1:eth2"]
      impairments:
        delay: 15ms
        jitter: 5ms
        loss: 1%
        rate: 1mbps

    - endpoints: ["worker-2:eth1", "crane-2:eth2"]
      impairments:
        delay: 15ms
        jitter: 5ms
        loss: 1%
        rate: 1mbps

    # Worker mesh — BLE/WiFi between workers on the dock
    - endpoints: ["worker-lead:eth2", "worker-2:eth2"]
      impairments:
        delay: 20ms
        jitter: 8ms
        loss: 2%
        rate: 500kbps

    - endpoints: ["worker-3:eth1", "worker-4:eth1"]
      impairments:
        delay: 20ms
        jitter: 8ms
        loss: 2%
        rate: 500kbps

    - endpoints: ["worker-lead:eth3", "worker-5:eth1"]
      impairments:
        delay: 20ms
        jitter: 8ms
        loss: 2%
        rate: 500kbps

    # Tractor links — GPS/cellular (higher latency, more reliable)
    - endpoints: ["tractor-1:eth1", "hold-aggregator:eth1"]
      impairments:
        delay: 50ms
        jitter: 15ms
        loss: 0.5%
        rate: 2mbps

    - endpoints: ["tractor-2:eth1", "hold-aggregator:eth2"]
      impairments:
        delay: 50ms
        jitter: 15ms
        loss: 0.5%
        rate: 2mbps

    - endpoints: ["tractor-3:eth1", "hold-aggregator:eth3"]
      impairments:
        delay: 50ms
        jitter: 15ms
        loss: 0.5%
        rate: 2mbps

    - endpoints: ["tractor-4:eth1", "hold-aggregator:eth4"]
      impairments:
        delay: 50ms
        jitter: 15ms
        loss: 0.5%
        rate: 2mbps

    # Sensors to aggregator — wired ethernet
    - endpoints: ["load-cell-1:eth1", "crane-1:eth3"]
      impairments:
        delay: 1ms
        rate: 100mbps

    - endpoints: ["rfid-reader:eth1", "hold-aggregator:eth5"]
      impairments:
        delay: 2ms
        rate: 10mbps

    # AI scheduler — backbone connection
    - endpoints: ["ai-scheduler:eth1", "hold-aggregator:eth6"]
      impairments:
        delay: 10ms
        jitter: 3ms
        loss: 0.1%
        rate: 10mbps

    # Crane to aggregator — backbone
    - endpoints: ["crane-1:eth4", "hold-aggregator:eth7"]
      impairments:
        delay: 5ms
        rate: 10mbps

    - endpoints: ["crane-2:eth3", "hold-aggregator:eth8"]
      impairments:
        delay: 5ms
        rate: 10mbps
```

**Total: 15 nodes** (2 cranes, 5 workers, 4 tractors, 1 AI agent, 1 aggregator, 2 sensors)

### Phase 2: Berth Operation (~50 nodes)

**Parallel to**: Army 24-node platoon validation

Scale to 3 holds being worked simultaneously on one vessel, plus yard tractors shared across holds, plus berth-level coordination:

- Hold 1 team: 15 nodes
- Hold 2 team: 15 nodes  
- Hold 3 team (hazmat): 15 nodes + additional hazmat-certified workers
- Berth Manager: 1 aggregation node
- Shared yard tractor pool: 8 additional tractors
- Yard block allocation nodes: 4

**Total: ~58 nodes**

Key validation: the berth manager sees **3 hold summaries**, not 45+ raw entity states. This is the identical validation to "platoon leader sees 3 squad summaries, not 24 raw states."

### Phase 3: Full Terminal Operation (~200 nodes)

**Parallel to**: Army 112-node company validation

- 2 berths operating simultaneously
- Full yard operations (blocks, tractors, stacking cranes)
- Gate operations (truck queuing, rail loading)
- Terminal Operations Center (TOC) as top-level aggregation
- Multiple AI agents (scheduling, yard optimization, gate flow)

This validates the full HIVE hierarchy at comparable scale to the company-level Army experiment, in a completely different operational domain.

## Experiment Design

### Scenario: MV Ever Forward Arrival

The driving scenario follows Art Recesso's design directly:

**T=0: Ship Arrival (Goal Fires)**

The vessel manifest arrives as an event injected into the simulation. This triggers:

1. **Discovery phase**: Protocol identifies all available entities within operational scope
2. **Capability matching**: Stow plan requirements matched against available capabilities
3. **Gap analysis**: Identifies hazmat certification shortage for Holds 3 and 7
4. **Team formation**: Dynamic clusters form around each hold based on capability fit
5. **Continuous operation**: Teams work, metrics flow up, constraints propagate down

**T=5min: Gap Resolution**

The gap analysis identifies 3 workers with expired hazmat certifications but strong evidence chains. The protocol:

1. Routes targeted recertification content to those 3 workers' devices
2. Updates their capability status as recertification completes
3. Reassigns them to hazmat holds
4. Hold team aggregation updates to reflect new capability

**T=30min: Equipment Degradation**

Crane-2 develops hydraulic issues. The protocol:

1. Crane-2's capability assertion updates (reduced moves/hour, then DEGRADED status)
2. Hold aggregator detects degraded capability, recalculates team capacity
3. Berth manager receives updated hold summary showing reduced throughput
4. AI scheduler redistributes containers across holds to maintain overall target

**T=60min: Shift Change**

Half the workforce transitions. The protocol:

1. Departing workers' capability assertions go to OFFLINE
2. Arriving workers' capability assertions register on the network
3. Hold teams dynamically reform around available capabilities
4. No disruption to aggregate berth-level metrics

### Metrics and Success Criteria

#### Phase 1 (Hold Team — ~15 nodes)

| ID | Metric | Target | Rationale |
|---|---|---|---|
| P1-1 | Capability discovery (all 15 nodes) | < 10s | Entity registration after "ship arrival" event |
| P1-2 | Hold team aggregation convergence | < 5s | Aggregator has summary of all team capabilities |
| P1-3 | Capability update propagation (crane degradation) | < 2s | Real-time status must reach aggregator quickly |
| P1-4 | Gap analysis identification | < 3s | Time from manifest injection to gap report |
| P1-5 | Dynamic reassignment propagation | < 5s | Worker recertification → new team assignment visible |
| P1-6 | Sync success rate | 100% | Same as Army reference — no data loss |
| P1-7 | Document cardinality invariant | 15 entity docs + 1 summary | Per ADR-021, documents created once, updated via delta |

#### Phase 2 (Berth Operation — ~58 nodes)

| ID | Metric | Target | Rationale |
|---|---|---|---|
| P2-1 | Berth aggregation convergence | < 15s | Berth manager sees 3 hold summaries |
| P2-2 | Berth manager data volume | 3 summaries (not 45+ raw) | Validates hierarchical aggregation |
| P2-3 | Cross-hold tractor reassignment | < 10s | Tractor moved from Hold 1 to Hold 3 pool |
| P2-4 | Bandwidth reduction vs flat topology | > 60% | Same target as Army platoon validation |
| P2-5 | Shift change convergence | < 30s | All teams reformed after workforce transition |

#### Phase 3 (Terminal — ~200 nodes)

| ID | Metric | Target | Rationale |
|---|---|---|---|
| P3-1 | TOC convergence | < 30s | Terminal-wide operational picture from 200 nodes |
| P3-2 | O(n log n) scaling validation | Message complexity sub-quadratic | Core mathematical claim |
| P3-3 | End-to-end latency (sensor → TOC) | < 10s | Container event to terminal dashboard |
| P3-4 | Concurrent ship operations | No interference | Two berths operating independently with shared yard |

## What This Proves

### For HIVE Protocol

1. **Domain independence**: The same protocol primitives (capability assertion, hierarchical aggregation, dynamic clustering, continuous verification) work in a completely different domain without protocol modification.

2. **Dynamic hierarchy**: Unlike the Army reference where hierarchy is fixed by doctrine, port operations demonstrate HIVE's ability to form and reform hierarchies around operational goals.

3. **Heterogeneous entity coordination**: Workers, cranes, tractors, AI agents, sensors — all as first-class participants with verified capabilities. More entity type diversity than the Army reference.

4. **Workforce engineering**: Capabilities include human skills, certifications, proficiency levels, and evidence chains. This validates HIVE's model for human capability assertion, not just machine capabilities.

5. **Gap analysis as a protocol primitive**: The hazmat certification gap scenario demonstrates that HIVE's capability aggregation naturally surfaces gaps between required and available capabilities — the protocol doesn't just report state, it enables decision-making about that state.

### For the GPA Opportunity

1. **Technical credibility**: A working simulation with measurable results demonstrates the approach before asking for a pilot contract.

2. **Operational understanding**: Building the simulation forces precise modeling of port operations, which builds domain expertise the team will need.

3. **Demo capability**: The simulation can be shown to GPA stakeholders as a concrete illustration of what the protocol does, not just slides.

4. **Integration pathway visibility**: Modeling TOS, WMS, labor scheduling, equipment maintenance, and vessel planning as separate data sources that HIVE unifies makes the integration story tangible.

### For Dual-Use Positioning

The existence of two independent reference implementations — military and commercial — in the same codebase with the same protocol primitives makes the strongest possible case that HIVE is infrastructure, not a point solution. This is the positioning that matters for investors, for TRANSCOM, and for standards bodies.

## Implementation Plan

### Week 1: Schema and Entity Definitions

- Define port-domain capability types extending ADR-012 schema
- Create entity configuration files for all Phase 1 node types
- Define the "ship manifest" event schema that triggers goal activation
- Define aggregation policies for port metrics (moves/hour, capacity, gaps)

### Week 2: Phase 1 Topology and Baseline

- Create `hive-sim/topologies/port-hold-team-15node.yaml`
- Implement port-specific entity initialization in hive-sim-node (or port-sim-node)
- Run baseline discovery and convergence tests
- Validate document-oriented architecture (ADR-021) compliance

### Week 3: Scenario Scripts and Event Injection

- Implement ship arrival event injection script
- Implement crane degradation fault injection
- Implement shift change event sequence
- Implement gap analysis detection and reporting
- Run full Phase 1 scenario with metrics collection

### Week 4: Phase 2 Scale-Up

- Create `hive-sim/topologies/port-berth-operation-58node.yaml`
- Validate hierarchical aggregation (berth manager sees hold summaries)
- Run bandwidth reduction measurements
- Cross-hold tractor reassignment scenario
- Document results and comparison to Army reference at equivalent scale

### Ongoing: Phase 3 and Demonstration

- Scale to full terminal topology
- Build demo visualization via HIVE Operational Viewer ([ADR-053](053-hive-operational-viewer.md)) with port operations skin
- Prepare GPA-facing demonstration materials
- Publish results comparison (Army vs Port) as protocol validation evidence

## Team Responsibilities

| Person | Phase 1 Role | Phase 2+ Role |
|---|---|---|
| **Kit** | Protocol architecture, ContainerLab topology, CRDT validation | Integration architecture, demo |
| **Art** | Scenario design, capability schema review, workforce engineering validation | Operational scenario expansion |
| **Jack** | Physical systems modeling (crane ops, tractor behavior, sensor specs) | Equipment integration design |
| **Fred** | Workforce capability model validation, longshoreman domain expertise | GPA stakeholder engagement |
| **Rob** | Trust schema (certifications, evidence chains, audit trail) | Zephyr integration for credential verification |
| **Austin** (if available) | CV model specs for container ID, damage detection | Active learning pipeline design |

## Relationship to Existing ADRs

| ADR | Relationship |
|---|---|
| ADR-008 | Uses same ContainerLab infrastructure with new topology |
| ADR-012 | Extends capability schema with port-domain entity types |
| ADR-015 | Parallel validation track — same success criteria patterns |
| ADR-021 | Must satisfy same document-oriented architecture invariants |
| ADR-027 | Event routing policies adapted for port operations (moves/hour → aggregate, equipment failure → immediate) |

## File Structure

```
hive-sim/
├── topologies/
│   ├── squad-12node.yaml              # Existing Army reference
│   ├── platoon-24node.yaml            # Existing Army reference
│   ├── port-hold-team-15node.yaml     # NEW — Phase 1
│   ├── port-berth-operation-58node.yaml # NEW — Phase 2
│   └── port-terminal-200node.yaml     # NEW — Phase 3
├── scenarios/
│   ├── army/                          # Existing Army scenarios
│   └── port/                          # NEW
│       ├── ship-arrival.json          # Manifest event injection
│       ├── crane-degradation.json     # Fault injection
│       ├── shift-change.json          # Workforce transition
│       └── hazmat-gap.json            # Gap analysis trigger
├── schemas/
│   ├── army-entities.toml             # Existing
│   └── port-entities.toml             # NEW — Port capability types
├── test-hold-team.sh                  # NEW — Phase 1 test script
├── test-berth-operation.sh            # NEW — Phase 2 test script
└── validation/
    ├── army/                          # Existing results
    └── port/                          # NEW — Port validation results
```

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Port domain requires schema changes to ADR-012 | HIGH | LOW | Schema is designed for extension; port types are additions, not modifications |
| Dynamic hierarchy harder to validate than fixed doctrine | MEDIUM | MEDIUM | Define clear success criteria for "team formed" and "team reformed" states |
| hive-sim-node needs significant changes for port entities | MEDIUM | MEDIUM | Evaluate whether port-sim-node wrapper is cleaner than extending existing binary |
| Phase 3 (200 nodes) exceeds current workstation capacity | LOW | MEDIUM | Cloud-based ContainerLab or Containerlab-in-Kubernetes |
| Team availability (5-6 people, part-time) | HIGH | HIGH | Phase 1 is Kit-executable solo; others contribute domain expertise asynchronously |

## References

- ADR-008: Network Simulation Layer (ContainerLab infrastructure)
- ADR-012: Schema Definition and Protocol Extensibility
- ADR-015: Experimental Validation — Hierarchical Aggregation
- ADR-021: Document-Oriented Architecture and Update Semantics
- ADR-026: Reference Implementation — Software Orchestration
- ADR-027: Event Routing and Aggregation Protocol
- Port of Savannah scenario by Art Recesso (February 2026)
- Georgia Ports Authority operational data (public)

---

**Decision Record:**
- **Proposed:** 2026-02-06
- **Accepted:** TBD
- **Phase 1 Complete:** TBD
- **Phase 2 Complete:** TBD

**Authors:** Kit Plummer, Art Recesso  
**Contributors:** Rob Murtha, Jack Zentner, Fred Gregory, Austin Ruth (TBD)
