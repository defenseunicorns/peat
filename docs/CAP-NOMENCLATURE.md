# CAP Protocol Nomenclature
**Purpose:** Domain-agnostic terminology for hierarchical coordination
**Created:** 2025-10-31

---

## Problem Statement

Current terminology uses Army-specific terms (Platform, Squad, Platoon, Company) which:
- Limits applicability to non-military domains
- Creates cognitive barriers for civilian applications
- Assumes a specific organizational structure

**Goal:** Define abstract, domain-agnostic terminology that can be mapped to any hierarchical coordination domain.

---

## Proposed Abstract Nomenclature

### Core Hierarchy Levels

| Abstract Term | Description | Current (Military) | Alternative Mappings |
|--------------|-------------|-------------------|---------------------|
| **Node** | Individual autonomous unit | Platform | Robot, Vehicle, Drone, Agent, Device, Sensor |
| **Cell** | Small group (3-7 nodes) | Squad | Team, Group, Cluster, Pod, Ensemble |
| **Zone** | Mid-level group (3-7 cells) | Platoon | Region, Sector, District, Division, Area |
| **Network** | Top-level group (multiple zones) | Company | Federation, Mesh, Swarm, Fleet, System |

**Rationale:**
- **Node**: Universally understood in graph/network theory, computer science
- **Cell**: Biological analogy, widely used in robotics (cellular automata)
- **Zone**: Spatial/organizational, domain-neutral
- **Network**: Common in distributed systems, IoT, robotics

### Phase Terminology

| Abstract Term | Description | Current | Alternative Mappings |
|--------------|-------------|---------|---------------------|
| **Discovery** | Initial peer finding | Bootstrap | Join, Connect, Initialization |
| **Formation** | Group organization | Squad Formation | Clustering, Grouping, Coordination |
| **Operations** | Hierarchical coordination | Hierarchical Operations | Execution, Runtime, Active |

### Role Terminology

| Abstract Term | Description | Current | Alternative Mappings |
|--------------|-------------|---------|---------------------|
| **Coordinator** | Group leader | Leader, Commander | Head, Manager, Orchestrator |
| **Participant** | Group member | Member | Node, Agent, Peer |
| **Observer** | External monitor | C2, Human Operator | Supervisor, Monitor, Overseer |

### Capability Terminology

| Abstract Term | Description | Current | Alternative Mappings |
|--------------|-------------|---------|---------------------|
| **Capability** | Unit functionality | Capability | Service, Function, Skill, Feature |
| **Composition** | Combined capabilities | Aggregation | Fusion, Synthesis, Integration |
| **Emergence** | New capabilities from composition | Emergent | Synergistic, Derived, Compound |

---

## Domain Mapping Examples

### Military (Current)
```
Platform (UGV, UAV, UUV)
  └─> Squad (5 platforms)
      └─> Platoon (4 squads, ~20 platforms)
          └─> Company (4 platoons, ~80 platforms)
```

### Robotics / Manufacturing
```
Robot (assembly robot, AGV, inspection drone)
  └─> Cell (5 robots, work cell)
      └─> Zone (4 cells, production zone)
          └─> Factory Network (4 zones, entire facility)
```

### IoT / Smart City
```
Sensor Node (traffic camera, environmental sensor)
  └─> Cell (5 sensors, street segment)
      └─> Zone (4 cells, neighborhood)
          └─> City Network (multiple zones)
```

### Autonomous Vehicles
```
Vehicle (car, truck, bus)
  └─> Cell (5 vehicles, convoy)
      └─> Zone (4 cells, traffic corridor)
          └─> Fleet Network (entire region)
```

### Drone Swarms
```
Drone (quadcopter, fixed-wing)
  └─> Cell (5 drones, sub-swarm)
      └─> Zone (4 cells, swarm section)
          └─> Swarm Network (entire swarm)
```

### Distributed Computing
```
Compute Node (server, container, VM)
  └─> Cell (5 nodes, pod)
      └─> Zone (4 cells, cluster region)
          └─> Datacenter Network
```

---

## Proposed Refactoring

### Code Structure Changes

**Option A: Rename Everything (Breaking Change)**
```rust
// OLD
Platform → Node
Squad → Cell
Platoon → Zone
Company → Network

// Example
pub struct PlatformConfig → pub struct NodeConfig
pub struct SquadState → pub struct CellState
pub struct PlatoonCoordinator → pub struct ZoneCoordinator
```

**Pros:**
- Clean, domain-agnostic from start
- Easier to explain to non-military audiences
- More intuitive for broader applications

**Cons:**
- Breaks all existing E1-E4 code
- Disrupts current documentation
- Requires rewriting tests, ADRs, plans

---

**Option B: Alias + Gradual Migration (Non-Breaking)**
```rust
// Create type aliases
pub type Node = Platform;
pub type NodeConfig = PlatformConfig;
pub type NodeState = PlatformState;

pub type Cell = Squad;
pub type CellConfig = SquadConfig;
pub type CellState = SquadState;

pub type Zone = /* Platoon (not yet implemented) */;

// Allow both terms in code
let platform = PlatformConfig::new("UAV".to_string());
let node = NodeConfig::new("UAV".to_string());  // Same thing
```

**Pros:**
- Non-breaking for existing code
- Allows gradual migration
- Both terminologies coexist

**Cons:**
- Confusion with two names for same thing
- Doesn't fully solve the problem
- Technical debt (which name to use?)

---

**Option C: Configuration-Based Naming (Best of Both)**
```rust
// Abstract types with configurable labels
pub struct Hierarchy {
    level_0_name: String,  // "Platform" or "Node" or "Robot"
    level_1_name: String,  // "Squad" or "Cell" or "Team"
    level_2_name: String,  // "Platoon" or "Zone" or "Region"
    level_3_name: String,  // "Company" or "Network" or "Fleet"
}

// Internal code uses abstract names
pub struct L0Unit { /* ... */ }  // Level-0 unit (platform/node)
pub struct L1Group { /* ... */ } // Level-1 group (squad/cell)
pub struct L2Group { /* ... */ } // Level-2 group (platoon/zone)

// API exposed with configurable names
pub struct CAP {
    hierarchy: Hierarchy,
    // ...
}

impl CAP {
    pub fn with_military_terms() -> Self { /* ... */ }
    pub fn with_robotics_terms() -> Self { /* ... */ }
    pub fn with_iot_terms() -> Self { /* ... */ }
}
```

**Pros:**
- Domain-agnostic core implementation
- User-facing API uses their terminology
- Documentation can show multiple mappings
- Maximum flexibility

**Cons:**
- More complex implementation
- Harder to understand internal code
- Overkill for POC phase?

---

**Option D: Abstract Names + Domain Modules (Recommended)**
```rust
// Core protocol uses abstract terms
cap-protocol/src/
├── node/           // Individual units (was "platform")
├── cell/           // Small groups (was "squad")
├── zone/           // Mid-level groups (was "platoon")
├── network/        // Top-level (future "company")
└── domains/        // Domain-specific mappings
    ├── military.rs     // Platform, Squad, Platoon, Company
    ├── robotics.rs     // Robot, Cell, Zone, Factory
    ├── iot.rs          // Sensor, Cell, Zone, Network
    └── vehicles.rs     // Vehicle, Convoy, Corridor, Fleet

// Domain modules provide type aliases + terminology
pub mod military {
    pub use crate::node::{NodeConfig as PlatformConfig, NodeState as PlatformState};
    pub use crate::cell::{CellConfig as SquadConfig, CellState as SquadState};
    pub use crate::zone::{ZoneConfig as PlatoonConfig, ZoneState as PlatoonState};
}

// Users import their preferred domain
use cap_protocol::military::*;  // Military terminology
use cap_protocol::robotics::*;  // Robotics terminology
```

**Pros:**
- Core protocol is domain-agnostic
- Users choose their domain vocabulary
- Easy to add new domains
- Documentation shows domain mappings
- POC can use military terms via `domains::military`

**Cons:**
- Requires refactoring E1-E4 code
- More files to maintain
- Need to decide on abstract names

---

## Recommendation: Option D (Abstract Core + Domain Modules)

**Reasoning:**
1. **Future-proof**: Core protocol doesn't assume any domain
2. **User-friendly**: Users import their domain vocabulary
3. **Extensible**: Easy to add new domains (aviation, maritime, space, etc.)
4. **Documentation**: Can show examples in multiple domains
5. **POC-compatible**: Keep military terms via `domains::military` module

### Implementation Plan

**Phase 1: Define Abstract Nomenclature (0.5 days)**
- Finalize abstract terms: Node, Cell, Zone, Network
- Document rationale in this file
- Get stakeholder buy-in

**Phase 2: Refactor Core Types (2-3 days)**
- Rename `models/platform.rs` → `models/node.rs`
- Rename `models/squad/` → `models/cell/`
- Rename `bootstrap/` → `discovery/`
- Rename `squad/` → `cell/`
- Update all internal references

**Phase 3: Create Domain Modules (1 day)**
- Create `domains/military.rs` with type aliases
- Create `domains/robotics.rs` with type aliases
- Create `domains/iot.rs` with type aliases
- Document domain mappings

**Phase 4: Update Documentation (1 day)**
- Update all ADRs with abstract terminology
- Update README with domain examples
- Update project plan with abstract terms
- Update E5 plan with Zone (not Platoon)

**Phase 5: Update Tests (1 day)**
- Update test names to use abstract terms
- Tests can still use `domains::military` if desired
- Add domain-specific test examples

**Total Refactoring: ~5-6 days (1 week)**

---

## Proposed Timeline

**Option 1: Refactor Before E5** (Recommended)
```
Week 0: Prerequisites + Nomenclature Refactoring (10 days)
  ├── Days 1-4: Performance refactoring (caching, throttling)
  └── Days 5-10: Nomenclature refactoring (Node/Cell/Zone)

Week 1-4: E5 Implementation (using abstract terms)
  └── Zone instead of Platoon
```

**Pros:**
- E5 implemented with clean, domain-agnostic terms
- One breaking change instead of two
- Better foundation for E6-E10

**Cons:**
- Delays E5 start by 1 week
- More changes at once

---

**Option 2: Refactor After E5** (Faster)
```
Week 0: Prerequisites only (5 days)
Week 1-4: E5 Implementation (using Platoon)
Week 5: Nomenclature refactoring (all epics)
```

**Pros:**
- Faster to E5
- Smaller changes per week

**Cons:**
- E5 implemented with military terms, then refactored
- Two breaking changes
- More disruption

---

**Option 3: Gradual Migration (Lowest Risk)**
```
Week 0: Prerequisites only (5 days)
Week 1-4: E5 Implementation
  └── Use abstract terms for NEW code (Zone, not Platoon)
  └── Keep existing code as-is (Platform, Squad)
Week 5+: Gradually refactor E1-E4 as time permits
```

**Pros:**
- Minimal disruption
- E5 uses abstract terms from start
- E1-E4 refactored incrementally

**Cons:**
- Mixed terminology during transition
- Longer migration period

---

## Questions for Decision

1. **Which abstract terms?**
   - Node/Cell/Zone/Network (proposed)
   - Agent/Group/Region/System (alternative)
   - Unit/Team/Sector/Federation (alternative)
   - Other suggestions?

2. **Which refactoring approach?**
   - Option A: Rename everything (breaking, clean)
   - Option B: Aliases (non-breaking, confusing)
   - Option C: Config-based naming (complex, flexible)
   - Option D: Domain modules (recommended)

3. **When to refactor?**
   - Before E5 (1 week delay, clean start)
   - After E5 (faster, double refactor)
   - Gradual (lowest risk, mixed terminology)

4. **POC vs. Production considerations?**
   - POC: Keep military terms, refactor later for production
   - Production-ready: Refactor now, domain-agnostic from start

---

## Next Steps

1. **Decision Point**: Review proposed nomenclature with stakeholders
2. **Choose approach**: Select refactoring option (recommend Option D)
3. **Choose timing**: Decide when to refactor (recommend before E5)
4. **Update plans**: Revise E5 plan with abstract terminology
5. **Execute**: Begin refactoring or proceed to E5

---

**Status:** DRAFT - Awaiting Decision
**Decision Needed By:** Before E5 implementation starts
**Recommended:** Option D (Domain Modules) + Refactor Before E5
