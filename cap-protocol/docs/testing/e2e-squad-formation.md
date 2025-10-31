# End-to-End Testing: Squad Formation

## Overview

The Squad Formation E2E test suite validates the complete integration of Epic 4 components (E4.3, E4.4, E4.5) through scenario-driven, configuration-matrix testing. This document describes the test architecture, scenarios, and usage patterns.

## Test Architecture

### Scenario-Based Testing

The E2E test suite uses a **scenario configuration pattern** that allows systematic testing across multiple operational dimensions:

```rust
struct SquadFormationScenario {
    name: &'static str,
    squad_size: usize,
    include_operators: bool,
    authority_levels: Vec<Option<AuthorityLevel>>,
    health_statuses: Vec<HealthStatus>,
    expect_approval_required: bool,
    expect_success: bool,
    min_readiness: f32,
}
```

### Configuration Dimensions

Each scenario can vary across these dimensions:

1. **Squad Size** (3-5 members)
   - Minimum viable: 3 members
   - Medium: 4-5 members
   - Tests boundary conditions

2. **Authority Levels** (per platform)
   - `DirectControl`: Full autonomous authority
   - `Commander`: Tactical oversight required
   - `Observer`: Monitoring only
   - `Advisor`: Recommendations only
   - `None`: Fully autonomous (no operator)

3. **Health Status** (per platform)
   - `Nominal`: Fully operational
   - `Degraded`: Reduced capability
   - `Critical`: Severely limited
   - `Failed`: Non-operational

4. **Operator Presence**
   - Human-controlled platforms
   - Fully autonomous squads
   - Mixed configurations

5. **Approval Requirements**
   - Auto-approved (high authority)
   - Requires human oversight (low authority/autonomous)

6. **Readiness Thresholds**
   - Standard: 0.7 (70% readiness)
   - Degraded: 0.6 (60% readiness)
   - Critical: 0.5 (50% readiness)

## Test Scenarios

### 1. Optimal Squad Formation

**Configuration:**
- 5 members, all DirectControl authority
- All platforms Nominal health
- Auto-approved formation

**Purpose:** Validates ideal formation conditions with maximum authority and health.

**Expected Outcome:**
- Formation completes immediately
- No human approval required
- High readiness score (>0.7)
- Phase transition to Hierarchical succeeds

### 2. Mixed Authority Squad

**Configuration:**
- 4 members with mixed authorities:
  - Commander (tactical oversight)
  - DirectControl (full authority)
  - Observer (monitoring only)
  - Advisor (recommendations)
- All platforms Nominal health
- Requires human approval

**Purpose:** Tests human oversight workflow for low-authority platforms.

**Expected Outcome:**
- Formation status: `AwaitingApproval`
- Human approval required
- After approval: status becomes `Ready`
- Phase transition enabled post-approval

### 3. Degraded Health Squad

**Configuration:**
- 4 members, all DirectControl
- Mixed health: 2 Nominal, 2 Degraded
- Lower readiness threshold (0.6)

**Purpose:** Validates formation with platform health degradation.

**Expected Outcome:**
- Formation succeeds despite degraded platforms
- Readiness score meets lowered threshold
- Role scoring accounts for health impact
- Auto-approved (high authority compensates)

### 4. Autonomous-Only Squad

**Configuration:**
- 4 members, no human operators
- All platforms autonomous
- Requires oversight approval

**Purpose:** Tests fully autonomous squad formation requiring human supervision.

**Expected Outcome:**
- Formation status: `AwaitingApproval`
- Autonomous squads require human oversight
- After approval: Ready for operations
- Validates ADR-004 human-in-loop policy

### 5. Minimal Viable Squad

**Configuration:**
- Exactly 3 members (minimum size)
- All DirectControl, Nominal health
- Minimal capability coverage

**Purpose:** Boundary condition testing at minimum squad size.

**Expected Outcome:**
- Formation succeeds at exact minimum
- All 6 formation criteria met
- Leader elected despite minimal size
- Validates minimum viable squad concept

### 6. Critical Platform Squad

**Configuration:**
- 4 members, one with Critical health
- All DirectControl authority
- Very low readiness threshold (0.5)

**Purpose:** Tests severe health degradation impact on formation.

**Expected Outcome:**
- Formation succeeds despite critical member
- Readiness score heavily impacted
- Role scoring reflects health penalties
- Critical platform may get non-critical role

## E2E Flow Validation

Each scenario exercises this 8-step formation pipeline:

### Step 1: Platform Creation
```rust
let platforms = create_platforms_from_scenario(scenario);
```
- Generates platforms with specified health, authority, capabilities
- Assigns operators based on authority levels
- Distributes capabilities across squad members

### Step 2: Capability Aggregation (E4.4)
```rust
let aggregated = CapabilityAggregator::aggregate_capabilities(&platforms)?;
let readiness = CapabilityAggregator::calculate_readiness_score(&aggregated);
```
- Tests `CapabilityAggregator::aggregate_capabilities()`
- Validates readiness score calculation
- Checks capability coverage vs requirements
- Validates gap identification

### Step 3: Role Assignment (E4.3)
```rust
let roles = RoleAllocator::allocate_roles(&platforms)?;
```
- Tests `RoleAllocator::allocate_roles()`
- Validates leader election
- Ensures all platforms get assigned roles
- Verifies role scoring based on capabilities + health

### Step 4: Formation Coordination (E4.5)
```rust
let mut coord = SquadCoordinator::new("squad_id");
coord.min_readiness = scenario.min_readiness;
let complete = coord.check_formation_complete(&members, leader_id)?;
```
- Tests `SquadCoordinator::check_formation_complete()`
- Validates 6 formation criteria:
  1. Minimum squad size
  2. Leader elected
  3. All roles assigned
  4. Required capabilities present
  5. Readiness threshold met
  6. Human approval (if needed)

### Step 5: Human Approval Workflow
```rust
if coord.status == FormationStatus::AwaitingApproval {
    coord.approve_formation()?;
}
```
- Tests approval workflow (ADR-004)
- Validates state transitions
- Ensures idempotent approval
- Tests rejection path

### Step 6: Status Validation
```rust
assert_eq!(coord.status, FormationStatus::Ready);
assert!(coord.human_approved);
```
- Verifies final formation status
- Checks approval flags
- Validates state consistency

### Step 7: Phase Transition
```rust
assert!(coord.can_transition_to_hierarchical());
let phase = coord.get_hierarchical_phase()?;
assert_eq!(phase, Phase::Hierarchical);
```
- Tests phase transition capability
- Validates transition preconditions
- Ensures operational readiness

### Step 8: Metrics Validation
```rust
let duration = coord.formation_duration();
assert!(duration >= 0);
```
- Validates formation timing
- Checks metric collection
- Verifies duration tracking

## Running E2E Tests

### Run Individual Scenarios

```bash
# Run specific scenario
cargo test test_e2e_optimal_squad_formation

# Run with output
cargo test test_e2e_mixed_authority_squad -- --nocapture
```

### Run Full Scenario Matrix

```bash
# Runs all 6 scenarios sequentially
cargo test test_e2e_scenario_matrix -- --nocapture
```

### Run All E2E Tests

```bash
# Run all E2E integration tests
cargo test e2e_integration_tests
```

## Test Output

E2E tests produce detailed output showing the formation flow:

```
=== Running E2E Scenario: Mixed Authority: Requires human oversight ===
Created 4 platforms with health: [Nominal, Nominal, Nominal, Nominal]
Aggregated 4 capability types
Squad readiness score: 0.82
Assigned 4 roles
Leader elected: p1
Formation status: AwaitingApproval
Human approval granted, formation ready
Phase transition to Hierarchical verified
=== Scenario 'Mixed Authority: Requires human oversight' completed successfully ===
```

## Adding New Scenarios

To add a new test scenario:

```rust
impl SquadFormationScenario {
    fn new_your_scenario() -> Self {
        Self {
            name: "Your Scenario: Description",
            squad_size: 4,
            include_operators: true,
            authority_levels: vec![/* your config */],
            health_statuses: vec![/* your config */],
            expect_approval_required: false,
            expect_success: true,
            min_readiness: 0.7,
        }
    }
}

#[test]
fn test_e2e_your_scenario() {
    run_e2e_scenario(SquadFormationScenario::new_your_scenario());
}
```

## Coverage Matrix

| Scenario | Size | Authority | Health | Approval | Readiness |
|----------|------|-----------|--------|----------|-----------|
| Optimal | 5 | All DirectControl | All Nominal | Auto | 0.7 |
| Mixed Authority | 4 | Mixed (4 levels) | All Nominal | Required | 0.7 |
| Degraded Health | 4 | All DirectControl | 2N, 2D | Auto | 0.6 |
| Autonomous | 4 | None | All Nominal | Required | 0.7 |
| Minimal Viable | 3 | All DirectControl | All Nominal | Auto | 0.7 |
| Critical Platform | 4 | All DirectControl | 3N, 1C | Auto | 0.5 |

**Legend:**
- N = Nominal, D = Degraded, C = Critical
- Numbers indicate count of platforms in each state

## Key Validations

### Formation Criteria (6 checks)
- ✓ Minimum squad size met (default: 3)
- ✓ Leader elected and confirmed
- ✓ All members have assigned roles
- ✓ Required capabilities present (Communication + Sensor)
- ✓ Squad readiness above threshold
- ✓ Human approval obtained (if required)

### Integration Points
- ✓ E4.3 (Role Assignment) → E4.5 (Coordinator)
- ✓ E4.4 (Capability Aggregation) → E4.5 (Coordinator)
- ✓ E4.5 (Formation) → Phase Transition
- ✓ Human Approval Workflow (ADR-004)

### Edge Cases Covered
- ✓ Minimum size boundary (exactly 3 members)
- ✓ Health degradation (Degraded, Critical)
- ✓ Authority variations (4 levels + None)
- ✓ Autonomous squads (no operators)
- ✓ Mixed operator/autonomous configurations

## Best Practices

### When to Add E2E Scenarios

Add new E2E scenarios when:
1. New formation criteria are introduced
2. New authority levels are added
3. New health status values are created
4. New phase transitions are implemented
5. New approval workflows are required

### Scenario Naming Convention

Use descriptive names that indicate the primary test focus:
- `Optimal`: Best-case conditions
- `Mixed X`: Variation in dimension X
- `Degraded X`: Reduced capability in X
- `Minimal X`: Boundary condition for X
- `Critical X`: Severe limitation in X

### Readiness Thresholds

Choose thresholds based on scenario constraints:
- `0.7`: Standard operational readiness
- `0.6`: Degraded but acceptable
- `0.5`: Critical but viable
- `0.4`: Minimum for emergency operations

### Debugging Failed Scenarios

If a scenario fails:
1. Run with `--nocapture` to see detailed output
2. Check formation status in output
3. Verify readiness score meets threshold
4. Confirm leader election succeeded
5. Validate role assignments are complete
6. Check capability aggregation results

## Related Documentation

- **E4.3 Role Assignment**: `src/models/role.rs`
- **E4.4 Capability Aggregation**: `src/squad/capability_aggregation.rs`
- **E4.5 Squad Coordinator**: `src/squad/coordinator.rs`
- **ADR-004**: Human-in-the-Loop Authority Model
- **Phase System**: `src/traits.rs`

## Test Statistics

- **Total E2E Tests**: 7
- **Individual Scenarios**: 6
- **Matrix Test**: 1 (runs all scenarios)
- **Code Coverage**: ~400 lines of E2E test code
- **Total Test Suite**: 177 tests (170 unit + 7 E2E)

## Maintenance

### Updating Scenarios

When updating E4 components, verify:
1. All scenarios still pass
2. New functionality is covered
3. Thresholds remain appropriate
4. Documentation is updated

### Performance Considerations

E2E tests take longer than unit tests:
- Run unit tests frequently during development
- Run E2E tests before commits/PRs
- Use matrix test for comprehensive validation
- Individual scenarios for targeted debugging

---

**Last Updated**: 2025-10-30
**Epic**: E4 - Squad Formation Phase
**Test Framework**: Rust `#[test]` with scenario pattern
**Location**: `src/squad/coordinator.rs::e2e_integration_tests`
