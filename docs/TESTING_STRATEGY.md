# CAP Protocol Testing Strategy

## Critical Value of Testing

The CAP Protocol is designed for **autonomous multi-agent systems** operating in dynamic, real-world environments. Testing is not just about code correctness—it's about **mission assurance** in scenarios where:

- **Human lives depend on system reliability**
- **Autonomous agents must coordinate without human intervention**
- **P2P mesh networks must maintain consistency under network partitions**
- **Cell formation must happen deterministically across distributed nodes**
- **Authority levels and human oversight must be enforced correctly**

### Why Testing Matters for CAP

1. **Safety-Critical Systems**: Autonomous military/tactical systems cannot fail
2. **Distributed Consensus**: CRDT-based state must converge correctly
3. **Human-Machine Teaming**: Authority boundaries must be enforced
4. **Network Resilience**: Must work under adverse network conditions
5. **Emergent Behavior**: Multi-agent interactions must be validated

## Test Pyramid

```
           E2E (Ditto P2P Sync)
          /                    \
         /  Integration Tests   \
        /________________________\
       /                          \
      /      Unit Tests            \
     /______________________________\
```

### Unit Tests (Foundation)
- **70% of test effort**
- Fast execution (<1ms per test)
- Test individual functions/modules
- No external dependencies (mocked)
- **Example**: Capability aggregation logic, role scoring

### Integration Tests (Middle Layer)
- **20% of test effort**
- Test component interactions
- May use test doubles for external systems
- **Example**: Cell coordinator with role allocator

### E2E Tests (Critical Validation)
- **10% of test effort, 100% of mission assurance value**
- Test real Ditto P2P synchronization
- Validate distributed state convergence
- Observer-based event-driven assertions
- **Example**: Multi-peer cell formation with CRDT sync

## Test Categories

### 1. Unit Tests (`#[test]` in source files)

**Purpose**: Validate individual component logic

**Coverage**:
- Models (PlatformConfig, SquadState, Capability, etc.)
- Algorithms (capability aggregation, role scoring, leader election)
- State machines (phase transitions, formation status)
- Business logic (readiness calculation, gap identification)

**Characteristics**:
- Runs in milliseconds
- No I/O, no network, no filesystem
- Deterministic and repeatable
- Run on every file save in TDD workflow

**Location**: Inline in `src/**/*.rs` files as `#[cfg(test)] mod tests`

### 2. Integration Tests (`tests/` directory)

**Purpose**: Validate cross-component interactions without external dependencies

**Coverage**:
- Storage abstractions (mocked Ditto)
- Cell formation workflow (without real P2P)
- Phase transitions
- Error handling paths

**Characteristics**:
- Runs in ~100ms
- May use filesystem (temp directories)
- No real network/Ditto required
- Can run in CI without credentials

**Location**: `cap-protocol/tests/*_integration.rs`

### 3. E2E Tests (Real Ditto P2P)

**Purpose**: Validate distributed system behavior with real CRDT sync

**Coverage**:
- Multi-peer Ditto synchronization
- Node advertisement propagation
- Cell formation state convergence
- Role assignment sync across mesh
- Human approval workflow distribution
- Observer-based event notification

**Characteristics**:
- Runs in ~500ms per scenario
- **Requires real Ditto instances**
- Observer-based (no polling)
- Event-driven assertions
- Isolated test sessions

**Location**: `cap-protocol/tests/*_e2e.rs`

**Critical**: E2E tests are THE validation that the p2p mesh works!

## E2E Test Strategy (The Critical Layer)

### Why E2E Tests Are Essential

**Unit/Integration tests validate logic, E2E tests validate reality.**

In a distributed CRDT system:
- ✅ Unit test: "Capability aggregation calculates readiness correctly"
- ❌ Unit test: Cannot validate that capabilities sync across peers
- ✅ E2E test: "Node capabilities stored on peer1 appear on peer2 via observers"

### E2E Test Requirements

Every E2E test MUST:
1. **Use real Ditto instances** (no mocks)
2. **Create isolated sessions** (unique persistence directories)
3. **Store data in Ditto** (PlatformConfig, SquadState, etc.)
4. **Validate sync via observers** (not polling!)
5. **Be fast** (<1s per test)
6. **Be deterministic** (no flaky timeouts)
7. **Clean up resources** (prevent test interference)

### E2E Test Scenarios

See [`cap-protocol/docs/testing/e2e-cell-formation.md`](../cap-protocol/docs/testing/e2e-cell-formation.md) for detailed scenario matrix.

**Core Scenarios** (must be validated with real Ditto sync):

1. **Node Advertisement Sync**
   - Store PlatformConfig on peer1
   - Observe appearance on peer2
   - Validate capability data integrity

2. **Cell Formation Propagation**
   - Create cell on peer1
   - Validate members list syncs to peer2/peer3
   - Observer triggers on formation complete

3. **Role Assignment Distribution**
   - Leader election on peer1
   - Role assignments stored in Ditto
   - All peers converge to same role map

4. **Human Approval Workflow**
   - Formation awaits approval on peer1
   - Approval state syncs to all peers
   - Phase transition happens mesh-wide

5. **Network Partition Recovery**
   - Disconnect peer2 from mesh
   - Make changes on peer1/peer3
   - Reconnect peer2, validate convergence

### E2E Test Infrastructure

**Test Harness** ([`cap-protocol/src/testing/e2e_harness.rs`](../cap-protocol/src/testing/e2e_harness.rs)):

```rust
pub struct E2EHarness {
    // Creates isolated Ditto instances
    pub async fn create_ditto_store(&mut self) -> Result<DittoStore>

    // Observer-based sync validation
    pub async fn observe_cell(&self, store: &DittoStore, cell_id: &str) -> Result<SquadObserver>

    // Event-driven peer connection
    pub async fn wait_for_peer_connection(&self, ...) -> Result<()>

    // Clean resource management
    pub async fn shutdown_store(&self, store: DittoStore)
}
```

**Key Features**:
- ✅ Unique temp directories per test
- ✅ mDNS-based peer discovery (no TCP config needed)
- ✅ Observer channels for event-driven assertions
- ✅ Graceful timeout handling
- ✅ Automatic cleanup

## Test Execution

### During Development

```bash
# Run unit tests (fast feedback loop)
cargo test --lib

# Run specific test
cargo test test_capability_aggregation -- --nocapture

# Run with coverage
cargo llvm-cov --lib --html
```

### Before Commit

```bash
# Run all tests
make test

# Run E2E tests specifically
make test-e2e

# Pre-commit checks (fmt + clippy + test)
make pre-commit
```

### CI/CD Pipeline

```bash
# Full CI pipeline
make ci

# Format check (no modifications)
cargo fmt --all -- --check

# Clippy with warnings as errors
cargo clippy --all-targets --all-features -- -D warnings

# All tests (unit + integration + E2E)
cargo test -- --test-threads=1
```

## Test Data Management

### Fixtures
- Node configurations in `tests/fixtures/`
- Cell scenarios as code (not JSON)
- Reusable test helpers

### Ditto Test Data
- Isolated persistence directories (auto-cleanup)
- Unique app_id per test run
- Observer-based validation (no manual queries)

### Environment Configuration
- `.env` file for Ditto credentials
- `DITTO_APP_ID`, `DITTO_OFFLINE_TOKEN`, `DITTO_SHARED_KEY`
- Makefile loads .env automatically

## Test Metrics

### Current Coverage (as of 2025-10-31)

| Category | Count | LOC | Coverage |
|----------|-------|-----|----------|
| Unit Tests | ~170 | ~3000 | 85% |
| Integration Tests | 2 | ~100 | N/A |
| E2E Tests | 2 | ~100 | Critical paths |

**E2E Test Status**:
- ✅ Infrastructure validated (harness + peer sync)
- ⏳ Cell formation scenarios (pending implementation)
- ⏳ Multi-peer convergence tests (pending)
- ⏳ Network partition recovery (pending)

### Target Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Unit test coverage | 85% | 90% |
| E2E scenarios | 2 | 10+ |
| Test execution time | <1s | <2s |
| E2E test time | 0.6s | <1s per scenario |

## Test Development Guidelines

### Writing Good Unit Tests

```rust
#[test]
fn test_capability_aggregation_nominal_health() {
    // Arrange: Create nodes with known state
    let nodes = create_test_nodes();

    // Act: Execute function under test
    let result = CapabilityAggregator::aggregate_capabilities(&nodes);

    // Assert: Validate expected outcomes
    assert!(result.is_ok());
    let aggregated = result.unwrap();
    assert_eq!(aggregated.len(), 5);
}
```

**Principles**:
- ✅ Test one thing
- ✅ Descriptive names
- ✅ Arrange-Act-Assert pattern
- ✅ No external dependencies
- ✅ Fast execution

### Writing Good E2E Tests

```rust
#[tokio::test]
async fn test_node_sync_across_peers() {
    let mut harness = E2EHarness::new("node_sync");

    // Create isolated Ditto instances
    let peer1 = harness.create_ditto_store().await.unwrap();
    let peer2 = harness.create_ditto_store().await.unwrap();

    // Set up observer BEFORE storing data
    let mut observer = harness.observe_node(&peer2, "node1").await.unwrap();

    // Store node on peer1
    peer1.store_node(&node_config).await.unwrap();

    // Wait for observer event (not polling!)
    let event = observer.wait_for_event(Duration::from_secs(5)).await.unwrap();

    // Validate sync
    let synced = peer2.get_node("node1").await.unwrap();
    assert_eq!(synced.id, node_config.id);

    // Cleanup
    harness.shutdown_store(peer1).await;
    harness.shutdown_store(peer2).await;
}
```

**Principles**:
- ✅ Real Ditto instances (no mocks)
- ✅ Observer-based validation
- ✅ Event-driven (no sleep/polling)
- ✅ Isolated sessions
- ✅ Proper cleanup

## Continuous Improvement

### When to Add Tests

**Always add tests when**:
1. Fixing a bug (regression test)
2. Adding new feature (coverage)
3. Refactoring code (safety net)
4. Changing CRDT schema (E2E validation)

### Test Maintenance

- Review test failures immediately
- Keep tests fast (<1s unit, <1s E2E)
- Remove duplicate coverage
- Update docs when scenarios change

## Related Documentation

- **E2E Cell Formation**: [`cap-protocol/docs/testing/e2e-cell-formation.md`](../cap-protocol/docs/testing/e2e-cell-formation.md)
- **E2E Test Harness**: [`cap-protocol/src/testing/e2e_harness.rs`](../cap-protocol/src/testing/e2e_harness.rs)
- **ADR-004**: Human-in-the-Loop Authority ([`docs/adr/004-human-machine-cell-composition.md`](adr/004-human-machine-cell-composition.md))

---

**Last Updated**: 2025-10-31
**Status**: Living Document
**Owner**: CAP Protocol Team
