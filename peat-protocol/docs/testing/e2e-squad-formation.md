# End-to-End Testing: Cell Formation

## Overview

The Cell Formation E2E test suite validates **distributed CRDT mesh behavior** using **real Ditto P2P synchronization** across multiple peers. These tests exercise cell formation with actual Ditto instances to validate that node configurations, capability advertisements, and formation state sync correctly through the mesh.

**Critical Distinction**: These are **real E2E tests**, not in-memory scenario tests. They validate that:
- Node data stored on peer1 appears on peer2 via Ditto sync
- Cell formation state propagates across the mesh
- Observer-based notifications trigger on state changes
- CRDT convergence happens correctly under network conditions

For testing philosophy and requirements, see [TESTING_STRATEGY.md](../../../docs/TESTING_STRATEGY.md).

## Test Architecture

### Real Ditto P2P Testing

The E2E test infrastructure uses **real Ditto instances** with **observer-based validation** to test distributed behavior:

**Test Harness** ([`src/testing/e2e_harness.rs`](../../src/testing/e2e_harness.rs)):
```rust
pub struct E2EHarness {
    // Creates isolated Ditto stores with unique persistence directories
    pub async fn create_ditto_store(&mut self) -> Result<DittoStore>

    // Sets up observer subscriptions for event-driven assertions
    pub async fn observe_cell(&self, store: &DittoStore, cell_id: &str) -> Result<SquadObserver>
    pub async fn observe_node(&self, store: &DittoStore, node_id: &str) -> Result<PlatformObserver>

    // Event-driven peer connection detection
    pub async fn wait_for_peer_connection(&self, ...) -> Result<()>
}
```

**Key Features**:
- ✅ Isolated Ditto sessions (unique temp directories per test)
- ✅ mDNS-based peer discovery (no TCP configuration)
- ✅ Observer channels for event-driven assertions
- ✅ Fast execution (<1s per test)
- ✅ Deterministic results (no polling/arbitrary timeouts)

### Cell Formation Test Scenarios

The E2E tests validate cell formation across multiple operational dimensions:

```rust
struct SquadFormationScenario {
    name: &'static str,
    cell_size: usize,
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

1. **Cell Size** (3-5 members)
   - Minimum viable: 3 members
   - Medium: 4-5 members
   - Tests boundary conditions

2. **Authority Levels** (per node)
   - `DirectControl`: Full autonomous authority
   - `Commander`: Tactical oversight required
   - `Observer`: Monitoring only
   - `Advisor`: Recommendations only
   - `None`: Fully autonomous (no operator)

3. **Health Status** (per node)
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

### 1. Optimal Cell Formation

**Configuration:**
- 5 members, all DirectControl authority
- All nodes Nominal health
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
- All nodes Nominal health
- Requires human approval

**Purpose:** Tests human oversight workflow for low-authority nodes.

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

**Purpose:** Validates formation with node health degradation.

**Expected Outcome:**
- Formation succeeds despite degraded platforms
- Readiness score meets lowered threshold
- Role scoring accounts for health impact
- Auto-approved (high authority compensates)

### 4. Autonomous-Only Squad

**Configuration:**
- 4 members, no human operators
- All nodes autonomous
- Requires oversight approval

**Purpose:** Tests fully autonomous cell formation requiring human supervision.

**Expected Outcome:**
- Formation status: `AwaitingApproval`
- Autonomous cells require human oversight
- After approval: Ready for operations
- Validates ADR-004 human-in-loop policy

### 5. Minimal Viable Squad

**Configuration:**
- Exactly 3 members (minimum size)
- All DirectControl, Nominal health
- Minimal capability coverage

**Purpose:** Boundary condition testing at minimum cell size.

**Expected Outcome:**
- Formation succeeds at exact minimum
- All 6 formation criteria met
- Leader elected despite minimal size
- Validates minimum viable cell concept

### 6. Critical Node Squad

**Configuration:**
- 4 members, one with Critical health
- All DirectControl authority
- Very low readiness threshold (0.5)

**Purpose:** Tests severe health degradation impact on formation.

**Expected Outcome:**
- Formation succeeds despite critical member
- Readiness score heavily impacted
- Role scoring reflects health penalties
- Critical node may get non-critical role

## E2E Flow Validation with Ditto Sync

Each E2E test validates distributed behavior through real Ditto synchronization:

### Current E2E Infrastructure Tests

**Test 1: Isolated Store Creation** ([`tests/cell_formation_e2e.rs:27`](../../tests/cell_formation_e2e.rs))
```rust
#[tokio::test]
async fn test_harness_creates_isolated_stores() {
    let mut harness = E2EHarness::new("test_harness");
    let store1 = harness.create_ditto_store().await;
    let store2 = harness.create_ditto_store().await;

    assert!(store1.is_ok());
    assert!(store2.is_ok());
}
```
**Validates**: Test harness can create isolated Ditto instances

**Test 2: Multi-Peer Ditto Sync** ([`tests/cell_formation_e2e.rs:52`](../../tests/cell_formation_e2e.rs))
```rust
#[tokio::test]
async fn test_ditto_peer_sync_with_observers() {
    let mut harness = E2EHarness::new("e2e_peer_sync");

    let store1 = harness.create_ditto_store().await.unwrap();
    let store2 = harness.create_ditto_store().await.unwrap();

    store1.start_sync().unwrap();
    store2.start_sync().unwrap();

    // Wait for peers to connect (event-driven, not polling)
    harness.wait_for_peer_connection(&store1, &store2, Duration::from_secs(10)).await?;
}
```
**Validates**: Two Ditto peers can discover each other via mDNS and establish P2P connection

### Planned Cell Formation E2E Tests

The following tests need implementation to validate real Ditto sync behavior:

#### 1. Node Advertisement Sync
**Purpose**: Validate PlatformConfig propagates across mesh

```rust
#[tokio::test]
async fn test_node_sync_across_peers() {
    let mut harness = E2EHarness::new("node_sync");

    let peer1 = harness.create_ditto_store().await.unwrap();
    let peer2 = harness.create_ditto_store().await.unwrap();

    // Set up observer BEFORE storing data
    let mut observer = harness.observe_node(&peer2, "node1").await.unwrap();

    // Store node on peer1
    let node = create_test_node("node1", vec![Capability::Sensor]);
    peer1.store_node(&node).await.unwrap();

    // Wait for observer event (event-driven, not polling!)
    let event = observer.wait_for_event(Duration::from_secs(5)).await.unwrap();

    // Validate sync
    let synced = peer2.get_node("node1").await.unwrap();
    assert_eq!(synced.id, node.id);
    assert_eq!(synced.capabilities, node.capabilities);
}
```

#### 2. Cell Formation State Propagation
**Purpose**: Validate SquadState syncs across mesh

```rust
#[tokio::test]
async fn test_cell_formation_sync() {
    let mut harness = E2EHarness::new("cell_sync");

    let peer1 = harness.create_ditto_store().await.unwrap();
    let peer2 = harness.create_ditto_store().await.unwrap();

    // Observer on peer2
    let mut observer = harness.observe_cell(&peer2, "cell1").await.unwrap();

    // Create cell on peer1
    let mut coordinator = SquadCoordinator::new("cell1");
    coordinator.initiate_formation(vec!["p1", "p2", "p3"]).await.unwrap();
    peer1.store_cell_state(coordinator.to_cell_state()).await.unwrap();

    // Validate sync via observer
    let event = observer.wait_for_event(Duration::from_secs(5)).await.unwrap();

    let synced_cell = peer2.get_cell("cell1").await.unwrap();
    assert_eq!(synced_cell.id, "cell1");
    assert_eq!(synced_cell.members.len(), 3);
}
```

#### 3. Role Assignment Distribution
**Purpose**: Validate role assignments sync across mesh

#### 4. Human Approval Workflow Distribution
**Purpose**: Validate approval state syncs to all peers

#### 5. Network Partition Recovery
**Purpose**: Validate CRDT convergence after network partition

#### 6. Capability Aggregation Sync
**Purpose**: Validate aggregated capabilities propagate through mesh

## Running E2E Tests

### Prerequisites

E2E tests require Ditto credentials. Create a `.env` file in the workspace root:

```bash
# .env (workspace root)
DITTO_APP_ID=your-app-id
DITTO_SHARED_KEY=your-shared-key
```

Get credentials from [Ditto Portal](https://portal.ditto.live).

### Run E2E Tests

```bash
# From workspace root
make test-e2e

# From peat-protocol directory
cd peat-protocol && make test-e2e

# Run specific test
cargo test test_ditto_peer_sync_with_observers -- --nocapture

# Run with environment variables
DITTO_APP_ID=xxx cargo test --test cell_formation_e2e -- --nocapture
```

### Expected Output

Successful E2E test output:
```
Running E2E integration tests...
Waiting for peer connection...
✓ Peers connected
✓ Ditto sync infrastructure validated

test test_harness_creates_isolated_stores ... ok
test test_ditto_peer_sync_with_observers ... ok

test result: ok. 2 passed; 0 failed; 0 ignored
```

If tests skip due to missing credentials:
```
Skipping test - Ditto not configured
```
→ Solution: Add Ditto credentials to `.env` file

## Adding New E2E Tests

To add a new E2E test that validates Ditto sync:

```rust
#[tokio::test]
async fn test_your_sync_scenario() {
    if std::env::var("DITTO_APP_ID").is_err() {
        println!("Skipping test - Ditto not configured");
        return;
    }

    let mut harness = E2EHarness::new("your_scenario");

    // 1. Create isolated Ditto stores
    let peer1 = harness.create_ditto_store().await.unwrap();
    let peer2 = harness.create_ditto_store().await.unwrap();

    // 2. Start sync
    peer1.start_sync().unwrap();
    peer2.start_sync().unwrap();

    // 3. Wait for connection
    harness.wait_for_peer_connection(&peer1, &peer2, Duration::from_secs(10)).await.unwrap();

    // 4. Set up observer BEFORE storing data
    let mut observer = harness.observe_cell(&peer2, "cell1").await.unwrap();

    // 5. Store data on peer1
    let cell_state = create_test_cell_state("cell1");
    peer1.store_cell_state(&cell_state).await.unwrap();

    // 6. Wait for observer event (event-driven!)
    let event = observer.wait_for_event(Duration::from_secs(5)).await.unwrap();

    // 7. Validate sync
    let synced = peer2.get_cell("cell1").await.unwrap();
    assert_eq!(synced.id, cell_state.id);

    // 8. Clean shutdown
    harness.shutdown_store(peer1).await;
    harness.shutdown_store(peer2).await;
}
```

**Critical Requirements**:
- ✅ Skip test if Ditto not configured
- ✅ Set up observers BEFORE storing data
- ✅ Use event-driven assertions (no polling!)
- ✅ Clean shutdown to prevent interference

## E2E Test Coverage Status

### Current Status (2025-10-31)

| Test Category | Status | Count | Description |
|---------------|--------|-------|-------------|
| Infrastructure | ✅ Implemented | 2 | Harness creation, peer connection |
| Node Sync | ⏳ Planned | 0 | Node advertisement propagation |
| Cell Sync | ⏳ Planned | 0 | Cell formation state distribution |
| Role Sync | ⏳ Planned | 0 | Role assignment propagation |
| Approval Sync | ⏳ Planned | 0 | Human approval workflow distribution |
| Network Partition | ⏳ Planned | 0 | CRDT convergence after partition |

**Total E2E Tests**: 2 implemented, 6+ planned

### Test Execution Metrics

| Metric | Current | Target |
|--------|---------|--------|
| E2E test execution time | 0.46s | <1s |
| Peer connection time | <1s | <2s |
| Test isolation | ✅ 100% | 100% |
| Event-driven assertions | ✅ 100% | 100% |

## Key E2E Validations

### What E2E Tests Validate

**E2E tests validate distributed behavior, not business logic:**

✅ **DO validate with E2E tests**:
- Node data stored on peer1 appears on peer2 via Ditto sync
- Cell formation state propagates across mesh
- Role assignments sync to all peers
- Observer notifications trigger on state changes
- CRDT convergence after network partitions
- Multi-peer coordination

❌ **DO NOT validate with E2E tests** (use unit tests):
- Business logic (capability aggregation, role scoring, etc.)
- Formation criteria calculations
- State machine transitions
- Validation rules
- Algorithm correctness

### E2E Test Requirements (Critical)

Every E2E test MUST:
1. ✅ Use real Ditto instances (no mocks)
2. ✅ Create isolated sessions (unique temp directories)
3. ✅ Store data in Ditto on peer1
4. ✅ Validate sync via observers on peer2
5. ✅ Use event-driven assertions (no polling/sleep)
6. ✅ Be fast (<1s per test)
7. ✅ Be deterministic (no flaky timeouts)
8. ✅ Clean up resources (prevent interference)

See [TESTING_STRATEGY.md](../../../docs/TESTING_STRATEGY.md) for full requirements.

## Best Practices

### When to Add E2E Tests

Add new E2E tests when:
1. ✅ Adding new Ditto collections/schemas
2. ✅ Implementing new sync workflows
3. ✅ Changing CRDT data models
4. ✅ Adding observer-based features
5. ✅ Implementing multi-peer coordination

❌ Do NOT add E2E tests for:
- Business logic changes (use unit tests)
- Algorithm improvements (use unit tests)
- Validation rules (use unit tests)

### Test Naming Convention

Use descriptive names that indicate what sync behavior is tested:
- `test_node_sync_across_peers`: Node data syncs
- `test_cell_formation_sync`: Cell state propagates
- `test_role_assignment_distribution`: Roles sync to all peers
- `test_network_partition_recovery`: CRDT convergence after split

### Debugging Failed E2E Tests

If an E2E test fails:

1. **Check Ditto configuration**:
   ```bash
   echo $DITTO_APP_ID
   cat .env
   ```

2. **Run with verbose output**:
   ```bash
   cargo test test_name -- --nocapture
   ```

3. **Check peer connection**:
   - Look for "Peers connected" message
   - Verify mDNS discovery is working
   - Check firewall/network settings

4. **Validate observer setup**:
   - Observers must be created BEFORE storing data
   - Check observer query syntax
   - Verify sync subscription is active

5. **Check timing**:
   - E2E tests should complete in <1s
   - If timing out, check network/Ditto config
   - Avoid arbitrary sleep/polling

## Related Documentation

- **Testing Strategy**: [TESTING_STRATEGY.md](../../../docs/TESTING_STRATEGY.md) - Overall testing philosophy
- **E2E Test Harness**: [src/testing/e2e_harness.rs](../../src/testing/e2e_harness.rs) - Infrastructure implementation
- **Cell Coordinator**: [src/cell/coordinator.rs](../../src/cell/coordinator.rs) - Formation business logic (unit tested)
- **Ditto Store**: [src/storage/ditto_store.rs](../../src/storage/ditto_store.rs) - CRDT storage layer
- **ADR-002**: [Beacon Storage Architecture](../../../docs/adr/002-beacon-storage-architecture.md)

## Test Statistics

- **Total E2E Tests**: 2 (infrastructure validated)
- **Planned Tests**: 6+ (cell formation scenarios)
- **E2E Test Code**: ~99 lines (tests) + ~347 lines (harness)
- **Test Execution Time**: 0.46s
- **Total Test Suite**: ~172 tests (170 unit + 2 E2E)

## Maintenance

### Updating E2E Tests

When changing Ditto integration:
1. ✅ Update E2E tests if schema changes
2. ✅ Verify observers still work with new queries
3. ✅ Test sync behavior with new collections
4. ✅ Update this documentation

### Performance Considerations

E2E tests are fast but require Ditto:
- Run unit tests during active development (instant feedback)
- Run E2E tests before commits (`make test-e2e`)
- E2E tests validate the **critical path**: p2p mesh sync
- Unit tests validate **business logic**: algorithms, state machines

### Next Steps

**Immediate priorities** for E2E test implementation:
1. Node advertisement sync (peer1 → peer2)
2. Cell formation state propagation
3. Role assignment distribution
4. Human approval workflow sync
5. Network partition recovery

See [TESTING_STRATEGY.md](../../../docs/TESTING_STRATEGY.md) for implementation guidance.

---

**Last Updated**: 2025-10-31
**Epic**: E4 - Cell Formation Phase
**Test Framework**: Tokio async tests with Ditto SDK 4.12+
**Location**: `tests/cell_formation_e2e.rs`
**Status**: Infrastructure validated, scenarios pending implementation
