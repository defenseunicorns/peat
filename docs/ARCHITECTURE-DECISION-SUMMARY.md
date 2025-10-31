# Architecture Decision Summary: Human-Machine Cell Composition

**Date**: 2025-10-30
**Status**: PROPOSED - Implementation starting
**Impact**: CRITICAL - Foundational architecture change

## The Decision

**We will implement a hybrid human-machine composition model before continuing with E4.3-E4.5**, adding human authority factors to the existing capability-based leader election protocol. This is a **2-3 day implementation** that prevents weeks of refactoring later.

## Why Now?

Cell composition is **foundational architecture** that affects:
- E4.3 (Role Assignment) - needs to consider human roles
- E4.4 (Capability Aggregation) - must include human authority
- E4.5 (Phase Transition) - requires human approval policies
- E5+ (Hierarchical Operations) - depends on proper composition model

**Cost Analysis**:
- Implementing now: 2-3 days
- Refactoring later: 6-15 days (3-5x multiplier)
- **Decision: Pay upfront cost**

## What Changes

### 1. New Models

**Operator Model** (`models/operator.rs`):
```rust
- Operator struct (rank, authority, cognitive load, fatigue)
- OperatorRank enum (E-1 through O-10)
- AuthorityLevel enum (Observer → DirectControl)
- HumanMachinePair binding (supports 1:1, 1:N, N:1, N:M)
```

**Node Extension** (`models/node.rs`):
```rust
+ operator_binding: Option<HumanMachinePair>
```

### 2. Modified Leader Election

**Current** (E4.2):
```rust
score = compute(30%) + comms(25%) + sensors(20%) + power(15%) + reliability(10%)
```

**New** (Hybrid):
```rust
score = technical_score * technical_weight + authority_score * authority_weight

Where:
- technical_weight + authority_weight = 1.0
- Configurable per mission (default: 0.4 / 0.6)
- authority_score considers: rank, authority level, cognitive load, fatigue
```

### 3. Tunable Configuration

**Configuration Sources** (priority order):
1. C2 Directive (real-time override)
2. Mission Config File (YAML)
3. Environment Variables
4. Compiled Defaults

**Example Policies**:
- `RankDominant`: Highest rank always wins (traditional military)
- `TechnicalDominant`: Best sensors/comms wins (pure autonomous)
- `Hybrid`: Weighted combination (most missions)
- `Contextual`: Adapts to mission phase

## Supported Scenarios

### Scenario A: Traditional Infantry Squad
- 9 humans, 9 nodes (1:1 binding)
- E-7 Cell Leader's node → cell leader
- Policy: `Hybrid(authority=0.7, technical=0.3)`

### Scenario B: Robot-Augmented Squad
- 4 humans, 6 autonomous robots (mixed)
- E-7's node → tactical leader
- Policy: `Hybrid(authority=0.6, technical=0.4)`

### Scenario C: Swarm Operator
- 1 human, 8 robots (1:N binding)
- Human node → supervisor
- Policy: `Hybrid(authority=0.5, technical=0.5)` or `TechnicalDominant` for robot sub-coordination

### Scenario D: Command Vehicle
- 3 humans (O-3, E-7, E-4), 1 node (N:1 binding)
- O-3 has ultimate authority
- Policy: `RankDominant`

### Scenario E: Pure Autonomous
- 0 humans, 9 robots
- Policy: `TechnicalDominant` (fallback when no humans present)

## Implementation Checklist

### Phase 1: Foundational Models (Critical Path)

**Week 1: Models & Configuration**
- [ ] Create `models/operator.rs`
  - [ ] Operator struct
  - [ ] OperatorRank enum (all ranks)
  - [ ] AuthorityLevel enum
  - [ ] HumanMachinePair struct
  - [ ] Unit tests
- [ ] Extend `models/node.rs`
  - [ ] Add operator_binding field
  - [ ] Helper methods
  - [ ] Update existing tests
- [ ] Create `config/election_policy.rs`
  - [ ] ElectionPolicyConfig struct
  - [ ] LeadershipPolicy enum
  - [ ] YAML loading
  - [ ] Validation logic
  - [ ] Unit tests

**Week 1: Leader Election Update**
- [ ] Modify `cell/leader_election.rs`
  - [ ] Add ElectionContext parameter
  - [ ] Implement compute_authority_score()
  - [ ] Implement rank_to_score() mapping
  - [ ] Add hybrid scoring with weights
  - [ ] Update 10 existing tests
  - [ ] Add 5 new human-machine tests
- [ ] Integration testing
  - [ ] Test all 5 scenarios (A-E)
  - [ ] Performance validation (<5s convergence)
  - [ ] Configuration loading tests

### Phase 2: Integration (Ongoing with E4.3-E4.5)
- [ ] E4.3: Role assignment considers human MOS
- [ ] E4.4: Aggregate human authority as capability
- [ ] E4.5: Human approval for critical transitions

### Phase 3: Security (Post-E4)
- [ ] Cryptographic operator credentials
- [ ] Rank verification via C2
- [ ] Audit logging

## Key Metrics

**Success Criteria**:
- [x] Leader election with 0 humans (pure autonomous) ✓
- [ ] Leader election with 1+ humans (hybrid)
- [ ] E-7 beats E-5 beats robot (authority > 0.5)
- [ ] Robot beats human (technical = 1.0)
- [ ] Configuration loads from YAML/env
- [ ] Performance: <5s convergence, <100ms scoring overhead

**Test Coverage**:
- Existing: 10 tests (pure technical)
- New: 5+ tests (human-machine scenarios)
- Integration: 5 scenario tests

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Configuration errors | Validation logic, safe defaults, examples |
| Authority spoofing | Cryptographic signatures, C2 verification |
| Bad sensor data | Fallback to self-report, conservative defaults |
| Policy conflicts | Negotiation protocol, defer to higher authority |
| Development delay | Timeboxed to 3 days, can defer security to Phase 3 |

## Documentation

- **ADR**: [`docs/adr/004-human-machine-cell-composition.md`](./adr/004-human-machine-cell-composition.md)
- **Design**: [`docs/human-machine-teaming-design.md`](./human-machine-teaming-design.md)
- **This Summary**: `docs/ARCHITECTURE-DECISION-SUMMARY.md`

## Next Steps

1. **Review & Approve ADR** (you)
2. **Create implementation branch** (`feat/human-machine-composition`)
3. **Phase 1 Implementation** (2-3 days)
4. **Integration with E4.3-E4.5** (ongoing)
5. **Demonstration scenarios** (after E4.5)

## Questions for Approval

1. **Approve the decision to implement now vs later?** (Recommendation: Now)
2. **Default policy preference?**
   - Option A: `Hybrid(0.6 authority, 0.4 technical)` - Balanced
   - Option B: `Contextual` - Adapts to situation
   - Recommendation: Start with A, add B later
3. **Security implementation timing?**
   - Option A: Phase 1 (delays by 1-2 days, cryptographic verification)
   - Option B: Phase 3 (faster to demo, adds later)
   - Recommendation: B - defer crypto to Phase 3
4. **Cognitive load measurement?**
   - Option A: Mock/simulated initially
   - Option B: Real sensors (requires hardware integration)
   - Recommendation: A - simulate for POC

## References

- [E4.2 Leader Election PR](https://github.com/kitplummer/cap/pull/24)
- [CAP Project Plan](./CAP-POC-Project-Plan.md)
- NATO STANAG 4586 (UAV interoperability)
- DARPA OFFSET program (human-swarm interfaces)
