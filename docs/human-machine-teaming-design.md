# Human-Machine Teaming Architecture for CAP

## Problem Statement

The current CAP implementation treats all platforms as equivalent autonomous agents, using purely technical capabilities (compute, communication, sensors) for leader election and decision-making. This creates several architectural gaps:

1. **No human authority model**: Rank, role, and command authority are not represented
2. **Unclear human-machine binding**: Relationship between humans and platforms undefined
3. **Missing interface contracts**: No specification for human interaction when platform is leader vs follower
4. **Squad composition variants**: Multiple human-machine ratios not addressed (1:1, 1:N, N:1, N:M)

## Design Goals

1. **Support multiple human-machine teaming patterns** across different mission contexts
2. **Respect human authority** while leveraging machine autonomy appropriately
3. **Define clear interfaces** for human operators in different roles
4. **Enable gradual autonomy** from full human control to machine-only squads
5. **Maintain protocol consistency** whether platforms are human-operated or autonomous

## Proposed Architecture

### 1. Operator Model

#### Operator Struct

```rust
/// Human operator of a platform or squad
pub struct Operator {
    /// Unique operator identifier
    pub id: String,
    /// Name
    pub name: String,
    /// Military rank
    pub rank: OperatorRank,
    /// Authority level in current context
    pub authority: AuthorityLevel,
    /// Specialization (infantry, armor, aviation, etc.)
    pub mos: String,
    /// Cognitive load score (0.0-1.0, updated by platform)
    pub cognitive_load: f32,
    /// Fatigue level (0.0-1.0, updated by physiological sensors)
    pub fatigue: f32,
}

/// Military rank hierarchy
pub enum OperatorRank {
    // Enlisted
    E1, E2, E3, E4, E5, E6, E7, E8, E9,
    // Warrant Officers
    W1, W2, W3, W4, W5,
    // Officers
    O1, O2, O3, O4, O5, O6, O7, O8, O9, O10,
    // Civilian equivalents for allied/coalition
    Civilian(u8),  // Numeric level for equivalence
}

/// Authority level in human-machine teaming
pub enum AuthorityLevel {
    /// Human observes but cannot override
    Observer,
    /// Human can provide recommendations
    Advisor,
    /// Human provides high-level intent
    Supervisor,
    /// Human approves machine recommendations
    Commander,
    /// Human has full control (machine is tool)
    DirectControl,
}
```

#### Human-Machine Binding

```rust
/// Binding between operator(s) and platform(s)
pub struct HumanMachinePair {
    /// Operator(s) - can be empty for autonomous platforms
    pub operators: Vec<Operator>,
    /// Platform(s) bound to this pairing
    pub platform_ids: Vec<String>,
    /// Binding type
    pub binding_type: BindingType,
    /// Primary operator (for multi-operator scenarios)
    pub primary_operator_id: Option<String>,
}

pub enum BindingType {
    /// One human, one platform (traditional)
    OneToOne,
    /// One human controlling multiple platforms
    OneToMany,
    /// Multiple humans sharing one platform (e.g., command vehicle)
    ManyToOne,
    /// Complex teaming (platoon-level)
    ManyToMany,
    /// No human (autonomous platform)
    Autonomous,
}
```

### 2. Extended Leadership Scoring

Modify `LeadershipScore::from_capabilities()`:

```rust
impl LeadershipScore {
    pub fn from_capabilities_and_operator(
        capabilities: &[Capability],
        operator: Option<&Operator>,
        context: &ElectionContext,
    ) -> Self {
        // Base technical score (40% weight when human present)
        let technical_score = Self::compute_technical_score(capabilities);

        // Human authority score (60% weight when human present)
        let authority_score = operator.map(|op| {
            Self::compute_authority_score(op, context)
        }).unwrap_or(0.0);

        let total = if operator.is_some() {
            // Human-operated: authority dominates
            technical_score * 0.4 + authority_score * 0.6
        } else {
            // Autonomous: pure technical
            technical_score
        };

        // ... rest of implementation
    }

    fn compute_authority_score(operator: &Operator, context: &ElectionContext) -> f64 {
        let rank_score = Self::rank_to_score(operator.rank);
        let authority_score = Self::authority_to_score(operator.authority);
        let cognitive_score = 1.0 - operator.cognitive_load;
        let fatigue_score = 1.0 - operator.fatigue;

        // Weighted: rank(50%), authority(30%), cognitive(10%), fatigue(10%)
        rank_score * 0.5
            + authority_score * 0.3
            + cognitive_score * 0.1
            + fatigue_score * 0.1
    }

    fn rank_to_score(rank: OperatorRank) -> f64 {
        match rank {
            OperatorRank::E1 => 0.1,
            OperatorRank::E2 => 0.15,
            OperatorRank::E3 => 0.2,
            OperatorRank::E4 => 0.3,
            OperatorRank::E5 => 0.4,
            OperatorRank::E6 => 0.5,
            OperatorRank::E7 => 0.6,  // Squad leader typical rank
            OperatorRank::E8 => 0.7,
            OperatorRank::E9 => 0.8,
            OperatorRank::W1..=OperatorRank::W5 => 0.75,
            OperatorRank::O1 => 0.85,
            OperatorRank::O2 => 0.9,
            OperatorRank::O3 => 0.95,  // Platoon leader
            OperatorRank::O4..=OperatorRank::O10 => 1.0,
            OperatorRank::Civilian(level) => level as f64 / 10.0,
        }
    }
}
```

### 3. Squad Composition Scenarios

#### Scenario A: Traditional Infantry Squad (9 humans, 9 platforms, 1:1)

```
Squad Leader (E-7) + Platform A (leader)
├─ Team Leader (E-5) + Platform B (follower)
│  ├─ Rifleman (E-3) + Platform C (follower)
│  └─ Grenadier (E-4) + Platform D (follower)
└─ Team Leader (E-5) + Platform E (follower)
   ├─ SAW Gunner (E-4) + Platform F (follower)
   └─ Rifleman (E-3) + Platform G (follower)
```

**Leader Election**:
- E-7's platform has highest authority score → becomes squad leader
- Technical capabilities are secondary
- E-7 provides tactical decisions, platform coordinates execution

**Interfaces**:
- **Leader (E-7)**: Situational awareness dashboard, squad status, voice command input, approval UI for critical decisions
- **Followers (E-3 to E-5)**: Squad leader's intent display, individual status, local autonomy for movement

#### Scenario B: Robot-Augmented Squad (4 humans, 6 robots)

```
Squad Leader (E-7) + Platform A (leader)
├─ Team Leader (E-5) + Platform B (follower)
│  ├─ Robot Platform C (autonomous follower)
│  └─ Robot Platform D (autonomous follower)
└─ Team Leader (E-5) + Platform E (follower)
   ├─ Robot Platform F (autonomous follower)
   ├─ Robot Platform G (autonomous follower)
   └─ Specialist (E-4) + Platform H (follower)
```

**Leader Election**:
- E-7's platform becomes leader (human authority)
- Robots have high technical scores but no human authority
- Hybrid squad with mixed autonomy levels

**Decision-Making**:
- **Tactical decisions**: E-7 commands
- **Coordination/execution**: Platform A manages robot positioning
- **Local navigation**: Each robot autonomous within intent bounds

#### Scenario C: Single Operator, Multiple Robots (1:N)

```
Operator (E-6) + Platform A (command platform, leader)
├─ Robot 1 (follower)
├─ Robot 2 (follower)
├─ Robot 3 (follower)
├─ Robot 4 (follower)
└─ Robot 5 (follower)
```

**Leader Election**:
- Platform A is leader (only human-operated)
- Robots automatically follow

**Interface**:
- Operator needs **supervisory control**: set waypoints, approve engagements, monitor status
- Cannot micromanage 5 robots → platform autonomy is high
- Operator sets intent, robots execute with coordination from Platform A

#### Scenario D: Command Vehicle (N:1)

```
Platform A (command vehicle, high compute/comms)
├─ Commander (O-3) - primary authority
├─ NCO (E-7) - tactical advisor
└─ RTO (E-4) - communications

Commanding:
├─ Squad 1 (9 platforms)
├─ Squad 2 (9 platforms)
└─ Squad 3 (9 platforms)
```

**Leader Election**:
- Platform A has highest technical capabilities
- O-3 has ultimate authority
- Platform provides C2 infrastructure

**Interface**:
- **O-3**: Multi-squad dashboard, intent planning, approval workflows
- **E-7**: Tactical recommendations, squad status details
- **E-4**: Comms management, message routing

### 4. Interface Contracts

#### Leader Interface (Human is Squad Leader)

**Required UI Components**:
- Squad member status (health, fuel, position)
- Emergent capability summary
- Mission objective overlay
- Voice command input
- Critical decision approval (e.g., weapon engagement)
- Intent specification (natural language or gesture)

**Data Flow**:
```
Human Intent → Platform Interpretation → Squad Message Bus → Followers Execute
                ↓
         Continuous Feedback (visual/haptic)
```

#### Follower Interface (Human is Squad Member)

**Required UI Components**:
- Squad leader's intent (text or voice)
- Own platform status
- Immediate local environment (AR overlay)
- Quick action buttons (report contact, request support)
- Simplified situational awareness (leader's position, team positions)

**Data Flow**:
```
Squad Leader Intent → Squad Message Bus → Platform Receives → Human Display
                                                ↓
                                Human Override (if needed) → Platform Executes
```

#### Autonomous Platform (No Human)

**Behavior**:
- Full participation in squad protocols
- Receives orders via Squad Message Bus
- Reports status and observations
- No human interface, pure machine-to-machine
- May have higher technical capabilities than human-worn platforms

### 5. Authority Policies

Define when human authority is **required** vs **optional**:

**Required Human Approval** (by default):
- Use of lethal force
- Entry into restricted areas
- Communications with higher HQ
- Mission plan changes

**Machine Autonomous**:
- Movement within intent bounds
- Obstacle avoidance
- Formation maintenance
- Sensor fusion and reporting
- Inter-platform coordination

**Configurable per Mission**:
- Engagement rules (ROE)
- Autonomous vs supervised mode per platform
- Cognitive load-based handoff (if human is overloaded, increase autonomy)

### 6. Rank and Leader Election Rules

**Option 1: Rank Always Wins** (military hierarchy)
- Highest-ranking human's platform is always leader
- Technical capabilities break ties between same rank
- Autonomous platforms can never be leader if humans present

**Option 2: Contextual Leadership** (recommended)
- **Tactical leadership**: Highest rank (human authority)
- **Technical coordination**: Highest capability (may be robot)
- **Dual leadership**: Human provides intent, technical leader executes

**Option 3: Dynamic Leadership**
- Leader election considers both rank and capability
- Rank weight can be adjusted based on:
  - Human cognitive load (high load → reduce weight)
  - Mission phase (planning → high rank weight, execution → high tech weight)
  - Casualties (if senior leader KIA, election is purely technical among survivors)

### 7. Implementation Phases

**Phase 1: Model Extensions** (blocking for E4.3-E4.5)
- [ ] Add `Operator` model (`models/operator.rs`)
- [ ] Add `HumanMachinePair` binding model
- [ ] Extend `PlatformConfig` with optional operator binding

**Phase 2: Leader Election Updates** (modify E4.2)
- [ ] Add authority scoring to `LeadershipScore`
- [ ] Implement rank-to-score mapping
- [ ] Add `ElectionContext` for configurable policies
- [ ] Update tests for human-operated scenarios

**Phase 3: Interface Specifications** (new module)
- [ ] Define abstract `HumanInterface` trait
- [ ] Specify `LeaderInterface` and `FollowerInterface` contracts
- [ ] Create interface state machine (leader/follower transitions)

**Phase 4: Integration** (E4.3, E4.4, E4.5)
- [ ] Role assignment considers operator roles (E4.3)
- [ ] Capability aggregation includes human decision-making (E4.4)
- [ ] Phase transitions require human approval when present (E4.5)

## Open Questions

1. **What happens if squad leader (human) is incapacitated?**
   - Automatic re-election based on next-highest rank?
   - Platform continues last-known intent until new leader elected?
   - Transition to autonomous mode?

2. **How do we handle rank disagreements in distributed system?**
   - Rank should be cryptographically signed by C2
   - Platforms verify rank claims via certificate chain
   - Conflict resolution: defer to higher HQ

3. **What is the cognitive load measurement mechanism?**
   - Physiological sensors (heart rate, eye tracking)
   - Task performance (response time, accuracy)
   - Self-reported (scale 1-10 from human)

4. **Can a human override leader election and force leadership?**
   - Yes, via C2-directed assignment (E3.4)
   - Emergency override protocol (safety-critical)
   - Logged and reported to higher HQ for accountability

5. **How do we represent human capabilities?**
   - Training level (basic, advanced, expert)
   - Experience (time in service, combat deployments)
   - Physical fitness (impacts mobility, endurance)
   - Add as new `CapabilityType::HumanSkill`?

## Example: Leadership Scoring Comparison

### Scenario: Squad with 3 members

**Platform A** (autonomous robot):
- Compute: 1.0
- Communication: 1.0
- Sensors: 4 (maxed)
- No human operator
- **Technical Score**: 0.95
- **Authority Score**: 0.0
- **Total**: 0.95 * 1.0 = **0.95**

**Platform B** (E-5 Team Leader):
- Compute: 0.6
- Communication: 0.7
- Sensors: 1
- Operator: E-5, Commander authority, low cognitive load
- **Technical Score**: 0.55
- **Authority Score**: (0.4 rank + 0.9 authority + 0.9 cognitive + 0.95 fatigue) * weights ≈ 0.65
- **Total**: 0.55 * 0.4 + 0.65 * 0.6 = **0.61**

**Platform C** (E-7 Squad Leader):
- Compute: 0.5
- Communication: 0.6
- Sensors: 1
- Operator: E-7, Commander authority, moderate cognitive load
- **Technical Score**: 0.48
- **Authority Score**: (0.6 rank + 0.9 authority + 0.7 cognitive + 0.8 fatigue) * weights ≈ 0.75
- **Total**: 0.48 * 0.4 + 0.75 * 0.6 = **0.64**

**Result**: Platform C (E-7) becomes leader, despite lower technical capabilities than Platform A (robot).

## Recommendations

1. **Implement Phase 1 before continuing E4.3**: Human-machine model is foundational
2. **Use Option 2 (Contextual Leadership)**: Provides flexibility for different mission types
3. **Define interface contracts early**: Prevents integration issues in E5+ (hierarchical ops)
4. **Add rank/authority to bootstrap**: C2 should assign authority during E3.4 (directed assignment)
5. **Consider cognitive load as dynamic factor**: Human fatigue should trigger autonomy handoff

## References

- DARPA OFFensive Swarm-Enabled Tactics (OFFSET) program human-swarm interfaces
- Army Field Manual 3-0: Operations (leadership principles)
- NATO STANAG 4586 (UAV interoperability, human-machine interface standards)
- Levels of Autonomy for Human-Robot Interaction (Sheridan & Verplank scale)
