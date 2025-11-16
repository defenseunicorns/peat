#!/bin/bash

# Epic 1
gh issue create --title "[E1] Project Foundation & Setup" \
  --body "**Goal:** Establish development environment, repository structure, and core abstractions

**Success Criteria:**
- Repository initialized with CI/CD
- Ditto SDK integrated and validated
- Core trait definitions established
- Development workflow documented

**Duration:** Week 1
**Dependencies:** None
**Priority:** P0

**Stories:**
- E1.1: Repository & Workspace Setup
- E1.2: Ditto SDK Integration Spike
- E1.3: Core Trait Definitions
- E1.4: Error Handling & Logging

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 2
gh issue create --title "[E2] CRDT Integration & Data Models" \
  --body "**Goal:** Implement core data structures using Ditto CRDTs

**Success Criteria:**
- Platform state persists and syncs via Ditto
- Squad state updates propagate correctly
- CRDT operations handle concurrent updates
- State can be queried efficiently

**Duration:** Week 1-2
**Dependencies:** E1
**Priority:** P0

**Stories:**
- E2.1: Platform Capability Model
- E2.2: Squad State Model
- E2.3: Ditto Collection Managers
- E2.4: State Serialization

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 3
gh issue create --title "[E3] Bootstrap Phase Implementation" \
  --body "**Goal:** Implement Phase 1 protocol for initial group formation

**Success Criteria:**
- 100 platforms organize into squads in <60s
- Message count is O(√n) or better
- All three bootstrap strategies work
- Graceful handling of concurrent joins

**Duration:** Week 2-3
**Dependencies:** E2
**Priority:** P0

**Stories:**
- E3.1: Geographic Self-Organization
- E3.2: C2-Directed Assignment
- E3.3: Capability-Based Queries
- E3.4: Bootstrap Coordinator

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 4
gh issue create --title "[E4] Squad Formation Phase" \
  --body "**Goal:** Implement Phase 2 protocol for squad cohesion and leader election

**Success Criteria:**
- Leader election converges in <5 seconds
- Emergent capabilities discovered
- Squad ready for tasking
- Handles leader failure gracefully

**Duration:** Week 3-5
**Dependencies:** E3
**Priority:** P0

**Stories:**
- E4.1: Intra-Squad Communication
- E4.2: Leader Election Algorithm
- E4.3: Role Assignment
- E4.4: Squad Capability Aggregation
- E4.5: Phase Transition Logic

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 5
gh issue create --title "[E5] Hierarchical Operations Phase" \
  --body "**Goal:** Implement Phase 3 protocol with hierarchical message routing

**Success Criteria:**
- Platforms only message squad peers
- Squad leaders message platoon level
- Cross-squad messages rejected
- Message complexity is O(n log n)

**Duration:** Week 5-7
**Dependencies:** E4
**Priority:** P0

**Stories:**
- E5.1: Hierarchical Message Router
- E5.2: Platoon Level Aggregation
- E5.3: Priority-Based Routing
- E5.4: Message Flow Control
- E5.5: Hierarchy Maintenance

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 6
gh issue create --title "[E6] Capability Composition Engine" \
  --body "**Goal:** Implement composition rules for aggregating platform capabilities

**Success Criteria:**
- All 4 composition patterns work
- Emergent capabilities detected
- Composition is associative/commutative
- Rules are extensible

**Duration:** Week 4-6
**Dependencies:** E4
**Priority:** P1

**Stories:**
- E6.1: Composition Rule Framework
- E6.2: Additive Composition Rules
- E6.3: Emergent Composition Rules
- E6.4: Redundant Composition Rules
- E6.5: Constraint-Based Composition

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 7
gh issue create --title "[E7] Differential Updates System" \
  --body "**Goal:** Implement delta generation and propagation for bandwidth efficiency

**Success Criteria:**
- Deltas are <5% of full state size
- Delta application is idempotent
- Supports all CRDT types
- Priority assignment is correct

**Duration:** Week 6-8
**Dependencies:** E5
**Priority:** P1

**Stories:**
- E7.1: Change Detection System
- E7.2: Delta Generation
- E7.3: Delta Application
- E7.4: Priority Assignment
- E7.5: TTL and Obsolescence

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 8
gh issue create --title "[E8] Network Simulation Layer" \
  --body "**Goal:** Create realistic network simulation with configurable constraints

**Success Criteria:**
- Bandwidth limiting works accurately
- Latency injection is realistic
- Packet loss simulated correctly
- Network partitions testable

**Duration:** Week 7-9
**Dependencies:** E5
**Priority:** P1

**Stories:**
- E8.1: Simulated Network Transport
- E8.2: Bandwidth Limiting
- E8.3: Latency Injection
- E8.4: Packet Loss Simulation
- E8.5: Network Partition Scenarios

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 9
gh issue create --title "[E9] Reference Application & Visualization" \
  --body "**Goal:** Build simulation harness and visualization for demonstrations

**Success Criteria:**
- Simulates 100+ platforms
- Visualizes hierarchy formation
- Shows real-time metrics
- Supports scenario replay

**Duration:** Week 8-10
**Dependencies:** E7, E8
**Priority:** P2

**Stories:**
- E9.1: Simulation Harness
- E9.2: Terminal UI Dashboard
- E9.3: Metrics Collection System
- E9.4: Visualization Export
- E9.5: Scenario Library

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

# Epic 10
gh issue create --title "[E10] Testing & Validation" \
  --body "**Goal:** Comprehensive testing and validation of all requirements

**Success Criteria:**
- All functional requirements validated
- Performance benchmarks pass
- Scale tests demonstrate O(n log n)
- Documentation complete

**Duration:** Week 9-12
**Dependencies:** All
**Priority:** P0

**Stories:**
- E10.1: Unit Test Coverage
- E10.2: Integration Test Scenarios
- E10.3: Performance Benchmarks
- E10.4: Scale Validation
- E10.5: Network Stress Testing
- E10.6: Documentation

See docs/CAP-POC-Project-Plan.md for detailed story descriptions."

echo ""
echo "Created 10 Epic issues. View them at: https://github.com/kitplummer/hive/issues"
echo ""
echo "Next steps:"
echo "1. Create labels (epic, story, priority:p0, priority:p1, priority:p2)"
echo "2. Run the full create_issues.sh script to create all story issues"
echo "3. Use GitHub Projects to organize the epics and stories"
