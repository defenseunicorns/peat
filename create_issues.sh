#!/bin/bash

# Epic 1: Project Foundation & Setup
gh issue create --title "[E1] Project Foundation & Setup" \
  --label "epic,priority:p0" \
  --body "**Goal:** Establish development environment, repository structure, and core abstractions

**Success Criteria:**
- ✓ Repository initialized with CI/CD
- ✓ Ditto SDK integrated and validated
- ✓ Core trait definitions established
- ✓ Development workflow documented

**Duration:** Week 1
**Dependencies:** None

See project plan for detailed stories."

gh issue create --title "[E1.1] Repository & Workspace Setup" \
  --label "story,priority:p0,epic:E1" \
  --body "- Initialize Rust workspace with library + binary crates
- Configure cargo-make for build automation
- Set up GitHub Actions for CI (test, lint, format)
- Add pre-commit hooks

**Epic:** E1 - Project Foundation & Setup"

gh issue create --title "[E1.2] Ditto SDK Integration Spike" \
  --label "story,priority:p0,epic:E1,spike" \
  --body "- Install and validate Ditto Rust SDK
- Create sample CRDT collections (G-Set, OR-Set, LWW-Register)
- Verify sync behavior between two Ditto instances
- Document SDK quirks and limitations

**Epic:** E1 - Project Foundation & Setup"

gh issue create --title "[E1.3] Core Trait Definitions" \
  --label "story,priority:p0,epic:E1" \
  --body "- Define \`Platform\` trait with lifecycle methods
- Define \`CapabilityProvider\` trait
- Define \`MessageRouter\` trait
- Define \`PhaseTransition\` trait

**Epic:** E1 - Project Foundation & Setup"

gh issue create --title "[E1.4] Error Handling & Logging" \
  --label "story,priority:p0,epic:E1" \
  --body "- Design error type hierarchy with thiserror
- Set up tracing with configurable log levels
- Add structured logging for key events
- Create error propagation patterns

**Epic:** E1 - Project Foundation & Setup"

# Epic 2: CRDT Integration & Data Models
gh issue create --title "[E2] CRDT Integration & Data Models" \
  --label "epic,priority:p0" \
  --body "**Goal:** Implement core data structures using Ditto CRDTs

**Success Criteria:**
- ✓ Platform state persists and syncs via Ditto
- ✓ Squad state updates propagate correctly
- ✓ CRDT operations handle concurrent updates
- ✓ State can be queried efficiently

**Duration:** Week 1-2
**Dependencies:** E1

See project plan for detailed stories."

gh issue create --title "[E2.1] Platform Capability Model" \
  --label "story,priority:p0,epic:E2" \
  --body "- Implement \`PlatformCapability\` struct
- Map static config to G-Set CRDT
- Map dynamic state to LWW-Register CRDT
- Add fuel counter using PN-Counter
- Write unit tests for each CRDT type

**Epic:** E2 - CRDT Integration & Data Models"

gh issue create --title "[E2.2] Squad State Model" \
  --label "story,priority:p0,epic:E2" \
  --body "- Implement \`SquadState\` struct
- Map member list to OR-Set CRDT
- Add leader election state (LWW-Register)
- Implement aggregated capability storage
- Add squad lifecycle methods

**Epic:** E2 - CRDT Integration & Data Models"

gh issue create --title "[E2.3] Ditto Collection Managers" \
  --label "story,priority:p0,epic:E2" \
  --body "- Create \`PlatformStore\` wrapper around Ditto
- Create \`SquadStore\` wrapper around Ditto
- Implement query helpers (by ID, by squad, by capability)
- Add batch update operations
- Handle Ditto subscription callbacks

**Epic:** E2 - CRDT Integration & Data Models"

gh issue create --title "[E2.4] State Serialization" \
  --label "story,priority:p0,epic:E2" \
  --body "- Define JSON schema for platform documents
- Define JSON schema for squad documents
- Implement serde serialization/deserialization
- Add schema validation
- Create test fixtures

**Epic:** E2 - CRDT Integration & Data Models"

# Epic 3: Bootstrap Phase Implementation
gh issue create --title "[E3] Bootstrap Phase Implementation" \
  --label "epic,priority:p0" \
  --body "**Goal:** Implement Phase 1 protocol for initial group formation

**Success Criteria:**
- ✓ 100 platforms organize into squads in <60s
- ✓ Message count is O(√n) or better
- ✓ All three bootstrap strategies work
- ✓ Graceful handling of concurrent joins

**Duration:** Week 2-3
**Dependencies:** E2

See project plan for detailed stories."

gh issue create --title "[E3.1] Geographic Self-Organization" \
  --label "story,priority:p0,epic:E3" \
  --body "- Implement geohash-based grid assignment
- Add local peer discovery within range
- Implement \"find nearest squad\" logic
- Handle squad capacity limits (max 5 platforms)
- Add metrics for discovery message count

**Epic:** E3 - Bootstrap Phase Implementation"

gh issue create --title "[E3.2] C2-Directed Assignment" \
  --label "story,priority:p0,epic:E3" \
  --body "- Define squad assignment message format
- Implement assignment broadcast receiver
- Add platform-to-squad matching logic
- Handle assignment conflicts (prefer first assignment)
- Add assignment acknowledgment

**Epic:** E3 - Bootstrap Phase Implementation"

gh issue create --title "[E3.3] Capability-Based Queries" \
  --label "story,priority:p0,epic:E3" \
  --body "- Define capability query message format
- Implement query matching algorithm
- Add response with platform capabilities
- Implement \"first N responders form squad\"
- Add query timeout handling

**Epic:** E3 - Bootstrap Phase Implementation"

gh issue create --title "[E3.4] Bootstrap Coordinator" \
  --label "story,priority:p0,epic:E3" \
  --body "- Implement phase state machine (bootstrap → squad)
- Add bootstrap timeout (60s default)
- Track unassigned platforms
- Generate bootstrap metrics
- Handle re-bootstrap on failure

**Epic:** E3 - Bootstrap Phase Implementation"

# Epic 4: Squad Formation Phase
gh issue create --title "[E4] Squad Formation Phase" \
  --label "epic,priority:p0" \
  --body "**Goal:** Implement Phase 2 protocol for squad cohesion and leader election

**Success Criteria:**
- ✓ Leader election converges in <5 seconds
- ✓ Emergent capabilities discovered
- ✓ Squad ready for tasking
- ✓ Handles leader failure gracefully

**Duration:** Week 3-5
**Dependencies:** E3

See project plan for detailed stories."

gh issue create --title "[E4.1] Intra-Squad Communication" \
  --label "story,priority:p0,epic:E4" \
  --body "- Implement squad membership messaging
- Add capability exchange protocol
- Create squad message bus (publish/subscribe)
- Handle message ordering within squad
- Add retransmission for lost messages

**Epic:** E4 - Squad Formation Phase"

gh issue create --title "[E4.2] Leader Election Algorithm" \
  --label "story,priority:p0,epic:E4" \
  --body "- Define capability scoring function
- Implement deterministic leader selection
- Add leader announcement message
- Handle split-brain scenarios
- Implement leader failure detection

**Epic:** E4 - Squad Formation Phase"

gh issue create --title "[E4.3] Role Assignment" \
  --label "story,priority:p0,epic:E4" \
  --body "- Define role types (sensor, compute, relay, etc.)
- Implement role allocation algorithm
- Add role announcement to squad
- Handle role conflicts
- Support dynamic role changes

**Epic:** E4 - Squad Formation Phase"

gh issue create --title "[E4.4] Squad Capability Aggregation" \
  --label "story,priority:p0,epic:E4" \
  --body "- Implement capability collector
- Add composition rule dispatcher
- Generate squad capability summary
- Detect emergent capabilities
- Publish squad capabilities to platoon

**Epic:** E4 - Squad Formation Phase"

gh issue create --title "[E4.5] Phase Transition Logic" \
  --label "story,priority:p0,epic:E4" \
  --body "- Implement squad formation completion detection
- Add transition from \"squad\" to \"hierarchical\" phase
- Generate formation metrics
- Handle incomplete formations (timeouts)
- Add squad stability verification

**Epic:** E4 - Squad Formation Phase"

# Epic 5: Hierarchical Operations Phase
gh issue create --title "[E5] Hierarchical Operations Phase" \
  --label "epic,priority:p0" \
  --body "**Goal:** Implement Phase 3 protocol with hierarchical message routing

**Success Criteria:**
- ✓ Platforms only message squad peers
- ✓ Squad leaders message platoon level
- ✓ Cross-squad messages rejected
- ✓ Message complexity is O(n log n)

**Duration:** Week 5-7
**Dependencies:** E4

See project plan for detailed stories."

gh issue create --title "[E5.1] Hierarchical Message Router" \
  --label "story,priority:p0,epic:E5" \
  --body "- Implement routing table (platform → squad → platoon)
- Add routing rules enforcement
- Reject cross-squad direct messages
- Support upward propagation (platform → squad → platoon)
- Add routing metrics (hops, latency)

**Epic:** E5 - Hierarchical Operations Phase"

gh issue create --title "[E5.2] Platoon Level Aggregation" \
  --label "story,priority:p0,epic:E5" \
  --body "- Implement platoon coordinator
- Add squad summary collection
- Create platoon capability composition
- Generate platoon-level abstractions
- Publish to company level

**Epic:** E5 - Hierarchical Operations Phase"

gh issue create --title "[E5.3] Priority-Based Routing" \
  --label "story,priority:p0,epic:E5" \
  --body "- Define priority levels (P1-P4)
- Implement priority queue per routing hop
- Add priority-based transmission scheduling
- Handle priority inversion scenarios
- Measure priority vs. latency

**Epic:** E5 - Hierarchical Operations Phase"

gh issue create --title "[E5.4] Message Flow Control" \
  --label "story,priority:p0,epic:E5" \
  --body "- Add per-link bandwidth limits
- Implement backpressure mechanisms
- Add message dropping for overload
- Track dropped message metrics
- Generate flow control alerts

**Epic:** E5 - Hierarchical Operations Phase"

gh issue create --title "[E5.5] Hierarchy Maintenance" \
  --label "story,priority:p0,epic:E5" \
  --body "- Handle squad merges (low membership)
- Handle squad splits (excess membership)
- Rebalance hierarchy on changes
- Update routing tables dynamically
- Minimize disruption during rebalancing

**Epic:** E5 - Hierarchical Operations Phase"

# Epic 6: Capability Composition Engine
gh issue create --title "[E6] Capability Composition Engine" \
  --label "epic,priority:p1" \
  --body "**Goal:** Implement composition rules for aggregating platform capabilities

**Success Criteria:**
- ✓ All 4 composition patterns work
- ✓ Emergent capabilities detected
- ✓ Composition is associative/commutative
- ✓ Rules are extensible

**Duration:** Week 4-6
**Dependencies:** E4

See project plan for detailed stories."

gh issue create --title "[E6.1] Composition Rule Framework" \
  --label "story,priority:p1,epic:E6" \
  --body "- Define \`CompositionRule\` trait
- Create rule registry pattern
- Add rule matching logic (preconditions)
- Implement rule application (composition function)
- Support rule priority/ordering

**Epic:** E6 - Capability Composition Engine"

gh issue create --title "[E6.2] Additive Composition Rules" \
  --label "story,priority:p1,epic:E6" \
  --body "- Implement coverage area summation
- Implement lift capacity summation
- Add ammunition pooling
- Create additive rule tests
- Verify associative/commutative properties

**Epic:** E6 - Capability Composition Engine"

gh issue create --title "[E6.3] Emergent Composition Rules" \
  --label "story,priority:p1,epic:E6" \
  --body "- Implement ISR chain detection (sensor + compute + comms)
- Implement 3D mapping (camera + lidar + compute)
- Add strike chain (ISR + strike + BDA)
- Create emergent capability structs
- Add emergence detection tests

**Epic:** E6 - Capability Composition Engine"

gh issue create --title "[E6.4] Redundant Composition Rules" \
  --label "story,priority:p1,epic:E6" \
  --body "- Implement detection reliability (probabilistic)
- Add continuous coverage (temporal overlap)
- Create redundancy benefit calculations
- Test redundancy edge cases
- Document redundancy math

**Epic:** E6 - Capability Composition Engine"

gh issue create --title "[E6.5] Constraint-Based Composition" \
  --label "story,priority:p1,epic:E6" \
  --body "- Implement team speed (min of platform speeds)
- Add communication range (max with mesh, min without)
- Create constraint propagation logic
- Handle constraint violations
- Test constraint edge cases

**Epic:** E6 - Capability Composition Engine"

# Epic 7: Differential Updates System
gh issue create --title "[E7] Differential Updates System" \
  --label "epic,priority:p1" \
  --body "**Goal:** Implement delta generation and propagation for bandwidth efficiency

**Success Criteria:**
- ✓ Deltas are <5% of full state size
- ✓ Delta application is idempotent
- ✓ Supports all CRDT types
- ✓ Priority assignment is correct

**Duration:** Week 6-8
**Dependencies:** E5

See project plan for detailed stories."

gh issue create --title "[E7.1] Change Detection System" \
  --label "story,priority:p1,epic:E7" \
  --body "- Implement state change tracker
- Add dirty field marking
- Create change log buffer
- Support manual change marking
- Add change coalescing logic

**Epic:** E7 - Differential Updates System"

gh issue create --title "[E7.2] Delta Generation" \
  --label "story,priority:p1,epic:E7" \
  --body "- Define delta operation types (LWW, G-Set, PN-Counter, etc.)
- Implement delta serialization (JSON)
- Add delta batching (multiple ops in one message)
- Create delta compression
- Measure delta size vs. full state

**Epic:** E7 - Differential Updates System"

gh issue create --title "[E7.3] Delta Application" \
  --label "story,priority:p1,epic:E7" \
  --body "- Implement delta deserialization
- Add delta validation (timestamp, schema)
- Apply delta to Ditto CRDT
- Handle apply failures gracefully
- Support delta replay for testing

**Epic:** E7 - Differential Updates System"

gh issue create --title "[E7.4] Priority Assignment" \
  --label "story,priority:p1,epic:E7" \
  --body "- Define priority rules (capability loss = P1, etc.)
- Implement priority classifier
- Add priority override mechanisms
- Track priority distribution metrics
- Validate priority assignment logic

**Epic:** E7 - Differential Updates System"

gh issue create --title "[E7.5] TTL and Obsolescence" \
  --label "story,priority:p1,epic:E7" \
  --body "- Add timestamp to all deltas
- Implement TTL checking
- Drop stale deltas before application
- Generate staleness metrics
- Handle clock skew scenarios

**Epic:** E7 - Differential Updates System"

# Epic 8: Network Simulation Layer
gh issue create --title "[E8] Network Simulation Layer" \
  --label "epic,priority:p1" \
  --body "**Goal:** Create realistic network simulation with configurable constraints

**Success Criteria:**
- ✓ Bandwidth limiting works accurately
- ✓ Latency injection is realistic
- ✓ Packet loss simulated correctly
- ✓ Network partitions testable

**Duration:** Week 7-9
**Dependencies:** E5

See project plan for detailed stories."

gh issue create --title "[E8.1] Simulated Network Transport" \
  --label "story,priority:p1,epic:E8" \
  --body "- Implement message passing abstraction
- Add in-memory transport for simulation
- Create message queues per link
- Support broadcast vs. unicast
- Add transport metrics collection

**Epic:** E8 - Network Simulation Layer"

gh issue create --title "[E8.2] Bandwidth Limiting" \
  --label "story,priority:p1,epic:E8" \
  --body "- Implement token bucket rate limiter
- Add per-link bandwidth configuration
- Track bytes transmitted per second
- Generate bandwidth utilization metrics
- Test bandwidth enforcement accuracy

**Epic:** E8 - Network Simulation Layer"

gh issue create --title "[E8.3] Latency Injection" \
  --label "story,priority:p1,epic:E8" \
  --body "- Add configurable latency per link
- Support variable latency (distribution)
- Implement latency jitter
- Create latency histograms
- Test latency timing accuracy

**Epic:** E8 - Network Simulation Layer"

gh issue create --title "[E8.4] Packet Loss Simulation" \
  --label "story,priority:p1,epic:E8" \
  --body "- Add configurable loss probability
- Implement random drop algorithm
- Support burst loss patterns
- Track loss statistics
- Test loss rate accuracy

**Epic:** E8 - Network Simulation Layer"

gh issue create --title "[E8.5] Network Partition Scenarios" \
  --label "story,priority:p1,epic:E8" \
  --body "- Implement link enable/disable
- Add partition scenario definitions
- Create partition/heal timelines
- Support split-brain testing
- Measure partition recovery time

**Epic:** E8 - Network Simulation Layer"

# Epic 9: Reference Application & Visualization
gh issue create --title "[E9] Reference Application & Visualization" \
  --label "epic,priority:p2" \
  --body "**Goal:** Build simulation harness and visualization for demonstrations

**Success Criteria:**
- ✓ Simulates 100+ platforms
- ✓ Visualizes hierarchy formation
- ✓ Shows real-time metrics
- ✓ Supports scenario replay

**Duration:** Week 8-10
**Dependencies:** E7, E8

See project plan for detailed stories."

gh issue create --title "[E9.1] Simulation Harness" \
  --label "story,priority:p2,epic:E9" \
  --body "- Implement platform spawner (configurable count)
- Add scenario definition format (JSON/YAML)
- Create simulation orchestrator
- Support simulation pause/resume
- Add simulation time control (fast-forward)

**Epic:** E9 - Reference Application & Visualization"

gh issue create --title "[E9.2] Terminal UI Dashboard" \
  --label "story,priority:p2,epic:E9" \
  --body "- Use \`tui-rs\` for terminal interface
- Display platform count by phase
- Show squad formation status
- Display real-time metrics
- Add capability composition view

**Epic:** E9 - Reference Application & Visualization"

gh issue create --title "[E9.3] Metrics Collection System" \
  --label "story,priority:p2,epic:E9" \
  --body "- Define metrics schema
- Implement metrics aggregator
- Add time-series data collection
- Create metrics export (JSON, CSV)
- Generate summary statistics

**Epic:** E9 - Reference Application & Visualization"

gh issue create --title "[E9.4] Visualization Export" \
  --label "story,priority:p2,epic:E9" \
  --body "- Export hierarchy graph (DOT format)
- Export capability composition tree
- Generate message flow diagrams
- Create timeline visualizations
- Support SVG/PNG output

**Epic:** E9 - Reference Application & Visualization"

gh issue create --title "[E9.5] Scenario Library" \
  --label "story,priority:p2,epic:E9" \
  --body "- Create 10-platform simple scenario
- Create 50-platform medium scenario
- Create 100-platform large scenario
- Add network stress scenarios
- Add failure injection scenarios

**Epic:** E9 - Reference Application & Visualization"

# Epic 10: Testing & Validation
gh issue create --title "[E10] Testing & Validation" \
  --label "epic,priority:p0" \
  --body "**Goal:** Comprehensive testing and validation of all requirements

**Success Criteria:**
- ✓ All functional requirements validated
- ✓ Performance benchmarks pass
- ✓ Scale tests demonstrate O(n log n)
- ✓ Documentation complete

**Duration:** Week 9-12
**Dependencies:** All

See project plan for detailed stories."

gh issue create --title "[E10.1] Unit Test Coverage" \
  --label "story,priority:p0,epic:E10" \
  --body "- Achieve 80%+ code coverage
- Add property tests for CRDT operations
- Test all composition rules
- Test all bootstrap strategies
- Add edge case tests

**Epic:** E10 - Testing & Validation"

gh issue create --title "[E10.2] Integration Test Scenarios" \
  --label "story,priority:p0,epic:E10" \
  --body "- Test bootstrap phase end-to-end
- Test squad formation end-to-end
- Test hierarchical operations end-to-end
- Test phase transitions
- Test failure recovery

**Epic:** E10 - Testing & Validation"

gh issue create --title "[E10.3] Performance Benchmarks" \
  --label "story,priority:p0,epic:E10" \
  --body "- Benchmark platform update processing
- Benchmark delta generation
- Benchmark capability composition
- Benchmark message routing
- Create performance regression tests

**Epic:** E10 - Testing & Validation"

gh issue create --title "[E10.4] Scale Validation" \
  --label "story,priority:p0,epic:E10" \
  --body "- Test with 10, 50, 100, 200 platforms
- Measure message count vs. platform count
- Prove O(n log n) complexity
- Measure memory per platform
- Measure CPU per platform

**Epic:** E10 - Testing & Validation"

gh issue create --title "[E10.5] Network Stress Testing" \
  --label "story,priority:p0,epic:E10" \
  --body "- Test at 9.6Kbps (worst case)
- Test with 30% packet loss
- Test with 5-second latency
- Test network partition recovery
- Measure capability staleness

**Epic:** E10 - Testing & Validation"

gh issue create --title "[E10.6] Documentation" \
  --label "story,priority:p0,epic:E10,documentation" \
  --body "- Complete API documentation (rustdoc)
- Write user guide
- Create architecture diagrams
- Document composition rules
- Add troubleshooting guide

**Epic:** E10 - Testing & Validation"

echo "All GitHub issues created successfully!"
