# ADR 004: Human-Machine Cell Composition and Authority Model

## Status

**PROPOSED** - Pending approval before implementation

## Context

### The Problem

The current CAP implementation (through E4.2) treats all nodes as equivalent autonomous agents, using purely technical capabilities for leader election:
- Compute resources (30%)
- Communication (25%)
- Sensors (20%)
- Power (15%)
- Reliability (10%)

This creates critical architectural gaps:

1. **No human authority representation**: Military rank, command authority, and human decision-making are not modeled
2. **Undefined human-machine relationships**: The binding between operators and nodes is unclear
3. **Missing interface contracts**: No specification for how humans interact with nodes in leader vs follower roles
4. **Unaddressed composition variants**: Multiple teaming patterns (1:1, 1:N, N:1, N:M) are not supported
5. **Scalability concern**: Without proper composition model, hierarchical scaling (cells → zones → companies) becomes intractable

### Business Drivers

- **Mission reality**: Real-world military operations involve human-machine teams, not pure autonomous systems
- **Authority compliance**: Rules of engagement, ethical AI guidelines, and military law require human oversight for critical decisions
- **Operator trust**: Soldiers must understand and trust the system - clarity in human-machine roles is essential
- **Scalability requirement**: System must scale from 9-person cells to company-level (100+) with consistent protocols

### Technical Constraints

1. **Distributed system**: No centralized authority, must work with peer-to-peer Ditto sync
2. **Heterogeneous nodes**: Mix of human-worn, human-controlled, and autonomous platforms
3. **Dynamic membership**: Cell composition changes due to casualties, equipment failure, mission adaptation
4. **Network partitions**: Humans and machines must operate during communication disruptions
5. **Real-time requirements**: Leader election must converge in <5 seconds

### Assumptions

1. **Human authority matters**: When humans are present, their rank/role should influence leadership more than raw technical capability
2. **Tunability required**: Different missions need different authority policies (human-led vs machine-led)
3. **Graceful degradation**: System should work with pure machines, pure humans, or hybrid teams
4. **Trust verification**: Rank/authority claims must be cryptographically verifiable to prevent spoofing
5. **Cognitive load is dynamic**: Human operators experience fatigue, stress, and cognitive overload - system should adapt

## Decision

We will **implement a hybrid human-machine composition model** that extends the current capability-based protocol with human authority factors. This will be done in **Phase 1 (Foundational) now**, before continuing with E4.3-E4.5.

### Architecture Components

#### 1. Operator Model (New)

```rust
// Location: hive-protocol/src/models/operator.rs

/// Human operator of a platform
pub struct Operator {
    pub id: String,
    pub name: String,
    pub rank: OperatorRank,
    pub authority: AuthorityLevel,
    pub mos: String,  // Military Occupational Specialty
    pub cognitive_load: f32,  // 0.0-1.0, updated by sensors
    pub fatigue: f32,  // 0.0-1.0, updated by sensors
}

pub enum OperatorRank {
    E1, E2, E3, E4, E5, E6, E7, E8, E9,  // Enlisted
    W1, W2, W3, W4, W5,  // Warrant Officers
    O1, O2, O3, O4, O5, O6, O7, O8, O9, O10,  // Officers
    Civilian(u8),  // For coalition/allied forces
}

pub enum AuthorityLevel {
    Observer,       // Can view but not influence
    Advisor,        // Can provide recommendations
    Supervisor,     // Provides high-level intent
    Commander,      // Approves machine recommendations
    DirectControl,  // Full manual control
}
```

#### 2. Human-Machine Binding (New)

```rust
// Location: hive-protocol/src/models/operator.rs

pub struct HumanMachinePair {
    pub operators: Vec<Operator>,
    pub node_ids: Vec<String>,
    pub binding_type: BindingType,
    pub primary_operator_id: Option<String>,
}

pub enum BindingType {
    OneToOne,      // 1 human : 1 node (traditional)
    OneToMany,     // 1 human : N nodes (swarm operator)
    ManyToOne,     // N humans : 1 node (command vehicle)
    ManyToMany,    // Complex (zone/company level)
    Autonomous,    // 0 humans : 1 node (robot)
}
```

#### 3. Extended Node Model (Modified)

```rust
// Location: hive-protocol/src/models/node.rs

pub struct PlatformConfig {
    pub id: String,
    pub node_type: String,
    pub capabilities: Vec<Capability>,
    pub comm_range_m: f32,
    pub max_speed_mps: f32,
    // NEW FIELDS:
    pub operator_binding: Option<HumanMachinePair>,
}
```

#### 4. Extended Leadership Scoring (Modified E4.2)

```rust
// Location: hive-protocol/src/cell/leader_election.rs

pub struct ElectionContext {
    pub policy: LeadershipPolicy,
    pub mission_phase: MissionPhase,
    pub authority_required: bool,
}

pub enum LeadershipPolicy {
    /// Rank always wins (military hierarchy)
    RankDominant,
    /// Technical capability always wins (machine-optimized)
    TechnicalDominant,
    /// Weighted hybrid (configurable)
    Hybrid { authority_weight: f64, technical_weight: f64 },
    /// Dynamic based on context
    Contextual,
}

impl LeadershipScore {
    pub fn from_capabilities_and_operator(
        capabilities: &[Capability],
        operator: Option<&Operator>,
        context: &ElectionContext,
    ) -> Self {
        let technical_score = Self::compute_technical_score(capabilities);

        let (authority_score, weights) = match operator {
            Some(op) => {
                let auth_score = Self::compute_authority_score(op, context);
                let weights = context.policy.get_weights();
                (auth_score, weights)
            }
            None => (0.0, (0.0, 1.0)),  // Pure technical
        };

        let total = technical_score * weights.1 + authority_score * weights.0;

        Self {
            compute: technical_score,  // Preserve for debugging
            authority: authority_score,
            total,
            // ... other fields
        }
    }

    fn compute_authority_score(operator: &Operator, context: &ElectionContext) -> f64 {
        let rank_score = Self::rank_to_score(operator.rank);
        let authority_score = Self::authority_to_score(operator.authority);
        let cognitive_penalty = operator.cognitive_load;
        let fatigue_penalty = operator.fatigue;

        // Base authority score
        let base = rank_score * 0.5 + authority_score * 0.5;

        // Reduce for cognitive load and fatigue
        base * (1.0 - cognitive_penalty * 0.3) * (1.0 - fatigue_penalty * 0.2)
    }
}
```

#### 5. Tunable Configuration (New)

```rust
// Location: hive-protocol/src/config/election_policy.rs

/// Configuration for leader election policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectionPolicyConfig {
    /// Default policy
    pub default_policy: LeadershipPolicy,
    /// Authority weight when using Hybrid policy
    pub authority_weight: f64,
    /// Technical weight when using Hybrid policy
    pub technical_weight: f64,
    /// Minimum rank required for cell leader
    pub min_leader_rank: Option<OperatorRank>,
    /// Whether autonomous nodes can be leaders
    pub allow_autonomous_leaders: bool,
    /// Cognitive load threshold for leadership disqualification
    pub max_cognitive_load: f32,
    /// Fatigue threshold for leadership disqualification
    pub max_fatigue: f32,
}

impl ElectionPolicyConfig {
    /// Load from environment, configuration file, or C2 directive
    pub fn load() -> Result<Self> {
        // Priority: C2 directive > config file > environment > defaults
        // ...
    }
}
```

### Configuration Sources (Priority Order)

1. **C2 Directive** (highest priority): Real-time policy changes from command
2. **Mission Configuration File**: Pre-mission planning parameters
3. **Environment Variables**: Deployment-time configuration
4. **Compiled Defaults**: Fallback values

Example configuration:

```yaml
# mission_config.yaml
election_policy:
  default_policy:
    type: Hybrid
    authority_weight: 0.6
    technical_weight: 0.4
  min_leader_rank: E5  # Team leader minimum
  allow_autonomous_leaders: false  # Humans required for tactical leadership
  max_cognitive_load: 0.85
  max_fatigue: 0.75

# Override per cell type
cell_variants:
  infantry_cell:
    policy: RankDominant  # Traditional hierarchy
  robot_zone:
    policy: TechnicalDominant  # Pure autonomous
    allow_autonomous_leaders: true
  mixed_cell:
    policy:
      type: Contextual  # Adapts to situation
```

## Consequences

### Positive

1. **Realistic human-machine teaming**: Properly models actual military command structures
2. **Mission flexibility**: Tunable policies support different operational contexts
3. **Scalability enabler**: Clear composition model supports hierarchical scaling
4. **Authority compliance**: Human oversight enforced where required
5. **Graceful degradation**: Works with pure humans, pure machines, or hybrid
6. **Trust building**: Explicit human authority improves operator confidence
7. **Future-proof**: Supports autonomous evolution as trust/capability improves

### Negative

1. **Increased complexity**: More models, more logic, more testing
2. **Configuration burden**: Operators must understand policy tuning
3. **Security surface**: Rank/authority claims must be cryptographically verified
4. **Performance overhead**: Authority scoring adds computation to leader election
5. **Development time**: Delays E4.3-E4.5 implementation by ~2-3 days

### Risks

1. **Configuration errors**: Incorrect policy could cause inappropriate leader selection
   - **Mitigation**: Validation logic, safe defaults, extensive testing
2. **Authority spoofing**: Malicious actor claims false rank
   - **Mitigation**: Cryptographic signatures on operator credentials, C2 verification
3. **Cognitive load measurement failure**: Bad sensors give wrong fatigue data
   - **Mitigation**: Fallback to human self-report, conservative defaults
4. **Policy conflicts**: Different cells with incompatible policies merge
   - **Mitigation**: Policy negotiation protocol, defer to higher authority

### Trade-offs

**Why now vs later?**
- **Now**: Prevents architectural rework, enables proper E4.3-E4.5 design
- **Later**: Maintains momentum but risks expensive refactoring

**Decision: Do it now** because:
1. Cell composition is foundational - impacts all subsequent work
2. Role assignment (E4.3) must consider human roles
3. Capability aggregation (E4.4) should include human decision authority
4. Hierarchical ops (E5) assumes proper cell composition model
5. Technical debt compounds - fixing later is 3-5x more expensive

## Implementation Plan

### Phase 1: Model Implementation (2-3 days)

**Priority: CRITICAL - Blocks E4.3-E4.5**

1. **Create operator model** (`models/operator.rs`)
   - [ ] `Operator` struct with rank, authority, cognitive state
   - [ ] `OperatorRank` enum with all military ranks
   - [ ] `AuthorityLevel` enum
   - [ ] `HumanMachinePair` binding struct
   - [ ] Unit tests for operator model

2. **Extend node model** (`models/node.rs`)
   - [ ] Add `operator_binding: Option<HumanMachinePair>` to `PlatformConfig`
   - [ ] Add helper methods: `has_operator()`, `get_primary_operator()`
   - [ ] Update existing tests

3. **Create configuration system** (`config/election_policy.rs`)
   - [ ] `ElectionPolicyConfig` struct
   - [ ] `LeadershipPolicy` enum
   - [ ] YAML loading (serde)
   - [ ] Environment variable overrides
   - [ ] Configuration validation logic

4. **Extend leader election** (`cell/leader_election.rs`)
   - [ ] Add `ElectionContext` parameter to election methods
   - [ ] Implement `compute_authority_score()`
   - [ ] Implement `rank_to_score()` mapping
   - [ ] Add hybrid scoring logic with configurable weights
   - [ ] Update all 10 existing tests
   - [ ] Add 5 new tests for human-machine scenarios

5. **Update capability model** (optional enhancement)
   - [ ] Add `CapabilityType::Authority` variant
   - [ ] Operators can advertise authority as capability

### Phase 2: Integration with E4.3-E4.5 (ongoing)

1. **E4.3 Role Assignment**
   - Consider human MOS when assigning roles
   - Node roles complement human roles

2. **E4.4 Capability Aggregation**
   - Aggregate human authority as emergent capability
   - "Cell has E-7 leader" is a capability

3. **E4.5 Phase Transition**
   - Require human approval for critical transitions
   - Log authority decisions for accountability

### Phase 3: Security & Verification (post-E4)

1. **Cryptographic identity**
   - Operator credentials signed by C2
   - Certificate chain verification

2. **Audit logging**
   - All authority decisions logged
   - Forensic capability for AAR

## Alternatives Considered

### Alternative 1: Pure Technical Scoring (Status Quo)

**Decision**: Rejected - Doesn't match operational reality

**Pros**:
- Simple, already implemented
- No human model complexity
- Fast development

**Cons**:
- Unrealistic for military operations
- No human authority compliance
- Doesn't scale to hierarchical operations
- Requires expensive refactoring later

### Alternative 2: Rank Always Wins

**Decision**: Rejected - Too rigid, ignores technical capability

**Pros**:
- Simple military hierarchy
- Clear chain of command
- Easy to explain

**Cons**:
- Ignores node capabilities completely
- Robot with better sensors/comms can't lead even in pure technical tasks
- Doesn't support autonomous-only squads
- No flexibility for different mission types

### Alternative 3: Implement Later (Post-E5)

**Decision**: Rejected - Architectural debt too expensive

**Pros**:
- Maintains current development velocity
- Delivers machine-only functionality first

**Cons**:
- E4.3-E4.5 designed for wrong model → rework required
- E5 (hierarchical ops) fundamentally depends on composition model
- Refactoring cost estimated at 3-5x the upfront cost
- Creates technical debt that blocks realistic demonstrations

### Alternative 4: Hybrid with Fixed Weights (60/40)

**Decision**: Rejected in favor of tunable hybrid - Too inflexible

**Pros**:
- Simpler than fully tunable
- Reasonable default for most missions

**Cons**:
- No adaptation to mission type
- Can't support pure autonomous squads
- Can't support strict hierarchy when needed
- Reduces operator control

## Success Criteria

1. **Functional**:
   - [ ] Leader election works with 0, 1, or N humans in squad
   - [ ] E-7 beats E-5 beats robot (when authority weight > 0)
   - [ ] Robot beats human (when technical weight = 1.0)
   - [ ] Configuration loading works from YAML, env vars, and C2

2. **Performance**:
   - [ ] Leader election still converges in <5 seconds
   - [ ] Authority scoring adds <100ms overhead

3. **Testing**:
   - [ ] 15+ new tests covering human-machine scenarios
   - [ ] All existing tests still pass
   - [ ] Configuration validation tested

4. **Documentation**:
   - [ ] ADR approved and archived
   - [ ] Design document updated
   - [ ] Configuration examples provided
   - [ ] Integration guide for E4.3-E4.5

## References

- [Design Document: Human-Machine Teaming Architecture](../human-machine-teaming-design.md)
- [E4.2: Leader Election Algorithm](https://github.com/kitplummer/hive/pull/24)
- DARPA OFFSET program - Human-swarm interfaces
- NATO STANAG 4586 - UAV interoperability
- Army FM 3-0: Operations - Leadership principles
- Sheridan & Verplank - Levels of autonomy scale

## Decision Date

**2025-10-30** (Proposed)

## Decision Makers

- @kitplummer (Project Lead)
- Claude Code (AI Assistant providing technical analysis)

## Notes

This is a foundational architectural decision that affects all subsequent work. Taking 2-3 days now to implement properly will save weeks of refactoring later and enable realistic demonstrations of the HIVE protocol in human-machine teaming scenarios.

The tunable configuration system is critical for research - allows experimentation with different authority policies to find optimal human-machine teaming strategies.
