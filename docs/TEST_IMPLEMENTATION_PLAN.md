# CAP Protocol Test Implementation Plan

**Status**: In Progress
**Last Updated**: 2025-11-01
**Total Tests Planned**: 16 E2E + Benchmarks

## Overview

This document outlines the comprehensive test implementation plan to achieve maximum test coverage for the CAP Protocol, including E2E integration tests, network partition tests, storage layer validation, and performance benchmarks.

## Current Status

### Completed (3 tests + infrastructure)
- ✅ test_harness_creates_isolated_stores - E2E infrastructure validation
- ✅ test_ditto_peer_sync_with_observers - Peer sync infrastructure
- ✅ test_e2e_node_advertisement_sync - NodeConfig CRDT sync validation
- ✅ DittoStore Clone implementation - Enables multi-store testing

### Current Test Count
- Unit/Integration tests: **283 passing**
- E2E tests: **10 passing** (7 E5 hierarchy + 3 E4 infrastructure)
- **Total: 293 tests, 100% pass rate**

## Phase 1: Squad Formation E2E Tests (Epic 4)

### Infrastructure Ready
- ✅ E2EHarness with isolated Ditto stores
- ✅ Observer-based event-driven assertions
- ✅ Graceful peer connection timeout handling
- ✅ NodeStore and CellStore CRDT operations

### Tests to Implement (6 remaining)

#### Test 2: Capability Multi-Peer Propagation
**Purpose**: Validate that nodes with different capabilities sync across mesh
**API Surface**:
- `NodeStore::store_config()`
- `NodeStore::get_config()`
**Validation**: 3 peers, 3 different capability types, cross-peer sync
**Estimated Time**: 30 min

#### Test 3: Cell Formation Multi-Peer
**Purpose**: Validate CellState member list sync
**API Surface**:
- `CellStore::store_cell()`
- `CellStore::get_cell()`
- `CellState.members` (HashSet<String>)
**Validation**: Cell with 3 members syncs to peer2
**Estimated Time**: 30 min

#### Test 4: Role Assignment Sync
**Purpose**: Validate role assignments propagate
**API Surface**:
- `CellStore::set_leader()`
- `CellState.leader_id` (Option<String>)
**Validation**: Leader role syncs across peers
**Estimated Time**: 20 min

#### Test 5: Leader Election Propagation
**Purpose**: Validate election results distribute
**API Surface**:
- `CellStore::set_leader()`
**Validation**: Election outcome syncs mesh-wide
**Estimated Time**: 20 min

#### Test 6: Timestamped State Updates
**Purpose**: Validate LWW-Register semantics
**API Surface**:
- `CellState.timestamp` (u64)
**Validation**: Latest update wins across peers
**Estimated Time**: 30 min

#### Test 7: Complete Formation Convergence
**Purpose**: Full lifecycle test
**Workflow**:
1. Nodes advertise capabilities
2. Cell formation
3. Leader election
4. Validation of final state
**Validation**: All state converges correctly
**Estimated Time**: 45 min

**Total Estimated Time**: 2-3 hours

## Phase 2: Network Partition E2E Tests (4 tests)

### Test Requirements
- Ditto transport control (start/stop sync)
- Multi-peer scenarios (3+ peers)
- State change during partition
- Convergence validation after reconnect

### Tests to Implement

#### Test 1: Partition During Formation
**Scenario**: Disconnect peer2 mid-formation
**Validation**: peer1/peer3 continue, peer2 catches up after reconnect
**Estimated Time**: 45 min

#### Test 2: Partition Recovery Convergence
**Scenario**: Split mesh into two partitions, make different changes, merge
**Validation**: CRDT convergence rules applied correctly
**Estimated Time**: 1 hour

#### Test 3: Leader Reelection After Partition
**Scenario**: Leader node partitioned, new election on remaining peers
**Validation**: Role reassignment, leader conflict resolution
**Estimated Time**: 45 min

#### Test 4: Multi-Zone Partition Isolation
**Scenario**: Zone-level partitions with cross-zone state
**Validation**: Zone isolation, hierarchical convergence
**Estimated Time**: 1 hour

**Total Estimated Time**: 3-4 hours

## Phase 3: Storage Layer E2E Tests (4 tests)

### Test Coverage

#### Test 1: NodeStore CRDT Sync
**Purpose**: Validate NodeConfig G-Set operations
**Validation**: Capability additions sync, no deletions
**Estimated Time**: 30 min

#### Test 2: CellStore OR-Set Operations
**Purpose**: Validate member add/remove semantics
**Validation**: Concurrent add/remove, add-wins resolution
**Estimated Time**: 45 min

#### Test 3: Concurrent Writes Conflict Resolution
**Purpose**: Validate LWW-Register for leader election
**Validation**: Timestamp-based conflict resolution
**Estimated Time**: 45 min

#### Test 4: Observer Notification Latency
**Purpose**: Measure observer trigger performance
**Validation**: Sub-second sync notifications
**Estimated Time**: 30 min

**Total Estimated Time**: 2-3 hours

## Phase 4: Discovery Module E2E Tests (3 tests)

### Test Coverage

#### Test 1: Geographic Discovery Sync
**Purpose**: Validate GPS coordinate-based discovery
**API**: Discovery coordinator with geo queries
**Estimated Time**: 45 min

#### Test 2: Capability-Based Peer Discovery
**Purpose**: Validate discovery by capability requirements
**API**: CapabilityQuery across mesh
**Estimated Time**: 45 min

#### Test 3: Directed Discovery Topology
**Purpose**: Validate explicit peer connections
**API**: Directed discovery with mesh changes
**Estimated Time**: 45 min

**Total Estimated Time**: 2-3 hours

## Phase 5: Performance Benchmarks (Criterion)

### Benchmark Suite Setup

```rust
// benchmarks/cell_formation.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_cell_formation(c: &mut Criterion) {
    c.bench_function("cell_formation_10_nodes", |b| {
        b.iter(|| {
            // Formation logic
        })
    });
}

criterion_group!(benches, bench_cell_formation);
criterion_main!(benches);
```

### Benchmarks to Implement

1. **Cell Formation Throughput** (10, 50, 100 nodes)
2. **Leader Election Performance** (various cell sizes)
3. **Capability Aggregation Speed** (10, 50, 100 capabilities)
4. **Routing Table Lookup Latency** (1-hop, 2-hop, 3-hop)
5. **Rebalancing Operation Cost** (merge, split)
6. **CRDT Sync Latency** (2, 5, 10 peers)

**Total Estimated Time**: 3-4 hours

## Phase 6: Load Testing (2 scenarios)

### Test Scenarios

#### Scenario 1: Large Formation (100+ nodes)
- 100 nodes forming 10 cells
- Validate formation time
- Monitor memory usage
- Track sync latency

#### Scenario 2: Multi-Zone Hierarchy (10+ cells)
- 3 zones, 10 cells, 100 nodes
- Full hierarchical routing
- Zone-level rebalancing
- Performance under load

**Total Estimated Time**: 4-5 hours

## Implementation Timeline

| Phase | Tests | Est. Time | Priority |
|-------|-------|-----------|----------|
| Phase 1: Squad Formation | 6 tests | 2-3 hours | P1 (In Progress) |
| Phase 2: Network Partitions | 4 tests | 3-4 hours | P2 |
| Phase 3: Storage Layer | 4 tests | 2-3 hours | P3 |
| Phase 4: Discovery Module | 3 tests | 2-3 hours | P4 |
| Phase 5: Benchmarks | 6 benches | 3-4 hours | P5 |
| Phase 6: Load Testing | 2 scenarios | 4-5 hours | P6 |
| **Total** | **25+ tests** | **16-22 hours** | |

## Success Criteria

### Coverage Goals
- [ ] Unit test coverage: 90%+ (currently 85%)
- [ ] E2E scenarios: 20+ tests (currently 10)
- [ ] All critical CRDT paths validated
- [ ] Performance baselines established
- [ ] Load testing scenarios documented

### Quality Metrics
- [ ] 100% test pass rate maintained
- [ ] All E2E tests <1s execution time
- [ ] No flaky tests (deterministic assertions)
- [ ] Comprehensive documentation
- [ ] CI/CD integration

## Known Challenges

### Ditto Peer Connection Timeouts
**Issue**: mDNS peer discovery can timeout in some environments
**Mitigation**: Graceful timeout handling, skip test with warning
**Impact**: Tests validate infrastructure even if peers don't connect

### File Locking in Parallel Tests
**Issue**: Ditto locks persistence directory, preventing parallel execution
**Mitigation**: Run with `--test-threads=1` or use isolated temp dirs
**Impact**: Slower test execution but guaranteed correctness

### CRDT Eventual Consistency
**Issue**: Sync is not instantaneous, requires polling
**Mitigation**: Retry loops with reasonable timeouts (10s max)
**Impact**: Tests are slower but deterministic

## Next Steps (Immediate)

1. ✅ Document test plan (this file)
2. ⏳ Complete Squad Formation tests 2-7
3. ⏳ Run full test suite validation
4. ⏳ Commit Squad Formation E2E tests
5. ⏳ Begin Network Partition tests

## References

- [TESTING_STRATEGY.md](./TESTING_STRATEGY.md) - Overall testing approach
- [E2E Cell Formation Tests](./testing/e2e-cell-formation.md) - Detailed scenario matrix
- [E2E Harness](../cap-protocol/src/testing/e2e_harness.rs) - Test infrastructure

---

**Document Version**: 1.0
**Author**: CAP Protocol Team
**Review Status**: Living Document
