# CRDT E2E Test Gaps and Fixes

## Executive Summary

**Current Status (2025-11-24)**: 5 of 6 tests failing with data sync issues despite successful peer connections.

### Progress Made ✅
1. **Fixed Compilation Errors**: Changed `ditto_store()` → `get_ditto_store().unwrap()` and fixed Arc dereferencing
2. **Fixed Automerge Accept Loop Error**: Removed duplicate `start_accept_loop()` calls (harness already starts it)
3. **Verified Connections Work**: TCP/QUIC connections establish successfully, logs confirm peer connectivity

### Current Issue ❌
**Data is NOT syncing between peers** despite successful connections:
- Tests wait up to 10 seconds (20 × 500ms) for data to sync from peer1 to peer2
- All 5 failing tests show the same pattern: connection succeeds, data doesn't sync
- Only 1 test passing: `test_ditto_concurrent_writes_lww_resolution`

**Root Cause IDENTIFIED (2025-11-24)**:
Subscriptions are created at time T, but data is written immediately at time T+0ms. Ditto's sync protocol needs time to:
1. Process the `register_subscription_v2()` call
2. Notify the remote peer about the new subscription
3. Set up bidirectional sync channels for that subscription
4. Begin replicating matching documents

Evidence from logs:
- `clearing scan marker for outbound updates reason=local_subscription_change` (peer processing its own subscription)
- `clearing scan marker for outbound updates reason=remote_subscription_change` (peer processing remote subscription)

These log messages prove Ditto IS responding to subscriptions, but the test writes data before this setup completes.

**Solution Attempted - FAILED**: Adding delays (100-200ms → 2s → 10s) after `NodeStore::new()` made things progressively worse, going from 2 failing tests to 5 failing tests.

**Correct Solution**: These tests need proper structural fixes to set up peer connections and sync initialization correctly, similar to the working pattern in `squad_formation_e2e.rs:84-103`. Arbitrary time delays are NOT the solution.

## Fixes Applied (Session 2025-11-24)

### Ditto Test Fixes (tests/storage_layer_e2e.rs)
**Lines 166-167, 359-360, 540-541**: Changed method name
```rust
// BEFORE:
let store1 = backend1.ditto_store();

// AFTER:
let store1 = backend1.get_ditto_store().unwrap();
```

**Lines 173, 366, 547**: Fixed Arc dereferencing
```rust
// BEFORE:
harness.wait_for_peer_connection(store1, store2, Duration::from_secs(60))

// AFTER:
harness.wait_for_peer_connection(&*store1, &*store2, Duration::from_secs(60))
```

### Automerge Test Fixes (tests/storage_layer_e2e.rs)
**Lines 208-210, 402-403, 583-584**: Removed duplicate accept loop startup
```rust
// REMOVED (was causing "Accept loop already running" error):
// transport2.start_accept_loop().unwrap();
// tokio::time::sleep(Duration::from_millis(100)).await;

// ADDED COMMENT:
// Note: Accept loop already started by backend.initialize() in harness (e2e_harness.rs:248)
```

## Current Failures (5 of 6 tests)

**Test Results**:
```
test test_ditto_concurrent_writes_lww_resolution ... ok
test test_ditto_nodestore_gset_sync ... FAILED
test test_ditto_cellstore_orset_operations ... FAILED
test test_automerge_nodestore_gset_sync ... FAILED
test test_automerge_cellstore_orset_operations ... FAILED
test test_automerge_concurrent_writes_lww_resolution ... FAILED
```

**Common Failure Pattern**:
1. Peer connections establish successfully (TCP/QUIC logs confirm)
2. Subscriptions are created properly
3. peer1 stores data successfully
4. peer2 polls for data for up to 10 seconds
5. **Data never appears on peer2** - sync is not occurring

**Evidence from Logs**:
- Ditto: "Starting TCP server bind", "physical connection started", "switching active transport"
- Automerge: Accept loop starts, connections attempt but fail with "NotFound" when querying

## Root Cause Analysis

**This is NOT a connection issue - connections work!**

The problem is deeper: the CRDT sync mechanism is not activated or configured properly. Possible causes:
1. Subscriptions may not trigger automatic replication
2. Missing sync activation beyond just connection establishment
3. Ditto may need explicit sync start or collection-level configuration
4. Automerge+Iroh may need document-level sync setup beyond transport connection

## Investigation Needed

### For Ditto (3 failing tests)
1. Check if `DittoStore::start_sync()` needs to be called explicitly
2. Verify if subscriptions actually trigger replication or just enable queries
3. Investigate if collection-level sync configuration is needed
4. Review Ditto SDK documentation for replication setup

### For Automerge+Iroh (2 failing tests)
1. Verify document-level sync is initiated after transport connection
2. Check if `AutomergeBackend` needs explicit sync activation
3. Review if peer discovery needs additional configuration beyond connection
4. Investigate Automerge sync protocol requirements

## Previous Documentation (Pre-Fix)

### Ditto Pattern (from `squad_formation_e2e.rs:84-103`)

```rust
// 1. Create backends with explicit TCP configuration
let tcp_port = E2EHarness::allocate_tcp_port()?;
let backend1 = harness.create_ditto_backend_with_tcp(Some(tcp_port), None).await?;
let backend2 = harness.create_ditto_backend_with_tcp(None, Some(format!("127.0.0.1:{}", tcp_port))).await?;

// 2. Access underlying DittoStore (available via ditto_backend.rs:110-112)
let store1 = backend1.ditto_store();  // Returns &DittoStore
let store2 = backend2.ditto_store();

// 3. Start sync on both stores (may already be started by backend, verify)
store1.start_sync().unwrap();
store2.start_sync().unwrap();

// 4. Wait for peer connection using event-driven approach
harness.wait_for_peer_connection(&store1, &store2, Duration::from_secs(60)).await?;

// 5. Now run test logic - backends can sync
```

### Tests Requiring Fix

1. `test_ditto_nodestore_gset_sync` (line 145)
2. `test_ditto_cellstore_orset_operations` (line 197)
3. `test_ditto_concurrent_writes_lww_resolution` (line 339)

## Solution for Automerge Tests

### Working Pattern (from automerge_iroh_sync_e2e.rs:31-99)

```rust
// 1. Create two backends with explicit bind addresses
let addr1: SocketAddr = "127.0.0.1:0".parse().unwrap(); // Use port 0 for random
let addr2: SocketAddr = "127.0.0.1:0".parse().unwrap();

let backend1 = harness.create_automerge_backend_with_bind(Some(addr1)).await?;
let backend2 = harness.create_automerge_backend_with_bind(Some(addr2)).await?;

// 2. Get underlying transports (AutomergeIrohBackend provides transport())
let transport1 = backend1.transport(); // Returns Option<&Arc<IrohTransport>>
let transport2 = backend2.transport();

// 3. Start accept loop on Node 2 (makes it listen for connections)
transport2.unwrap().start_accept_loop()?;
tokio::time::sleep(Duration::from_millis(100)).await; // Give accept loop time to start

// 4. Create PeerInfo for Node 2
let node2_peer = PeerInfo {
    name: "node-2".to_string(),
    node_id: hex::encode(transport2.unwrap().endpoint_id().as_bytes()),
    addresses: vec![addr2.to_string()],
    relay_url: None,
};

// 5. Node 1 connects to Node 2
transport1.unwrap().connect_peer(&node2_peer).await?;

// 6. Verify connection established
assert_eq!(transport1.unwrap().peer_count(), 1);

// 7. Now run test logic - backends can sync
```

### Key Differences from Ditto

| Aspect | Ditto | Automerge+Iroh |
|--------|-------|----------------|
| Connection Type | TCP ports | QUIC over UDP |
| Setup Method | `create_ditto_backend_with_tcp()` | `create_automerge_backend_with_bind()` |
| Accept Pattern | Automatic via Ditto SDK | Manual `start_accept_loop()` |
| Connect Method | Implicit via TCP | Explicit `connect_peer(&PeerInfo)` |
| Verification | `wait_for_peer_connection()` | `transport.peer_count()` |
| Harness Support | Fully abstracted | Partially abstracted (no peer connection helper) |

### Implementation Notes

1. **AutomergeBackend.transport()**: Need to check if this method exists or if we need to store transports separately
2. **Port allocation**: Use `"127.0.0.1:0"` to let OS assign random ports (avoids conflicts)
3. **PeerInfo creation**: Requires converting Iroh `EndpointId` to hex string
4. **No wait helper**: Unlike Ditto, no `wait_for_peer_connection()` - just verify peer_count()

### Tests Blocked

1. `test_automerge_nodestore_gset_sync` (line 167)
2. `test_automerge_cellstore_orset_operations` (line 267)
3. `test_automerge_concurrent_writes_lww_resolution` (line 397)

## Test File Structure

```
tests/storage_layer_e2e.rs
├── Lines 51-141: run_nodestore_gset_sync_test() - Shared test logic ✅
├── Lines 143-162: test_ditto_nodestore_gset_sync() - BROKEN ❌
├── Lines 164-179: test_automerge_nodestore_gset_sync() - BROKEN ❌
├── Lines 182-320: run_cellstore_orset_operations_test() - Shared test logic ✅
├── Lines 322-... : test_ditto_cellstore_orset_operations() - BROKEN ❌
├── Lines ...: test_automerge_cellstore_orset_operations() - BROKEN ❌
├── Lines 324-450: run_cellstore_lww_resolution_test() - Shared test logic ✅
├── Lines ...: test_ditto_concurrent_writes_lww_resolution() - BROKEN ❌
└── Lines ...: test_automerge_concurrent_writes_lww_resolution() - BROKEN ❌
```

## Implementation Priority

1. **P0: Fix Ditto tests** - Clear pattern exists, infrastructure proven
2. **P1: Research Automerge peer connection** - No clear pattern yet
3. **P2: Document parity gaps** - Once tests pass, validate if backends have true CRDT parity

## Infrastructure Status

### ✅ EXISTS AND WORKS
- `E2EHarness::create_ditto_backend_with_tcp()` (e2e_harness.rs:146)
- `E2EHarness::wait_for_peer_connection(&DittoStore, &DittoStore)` (e2e_harness.rs:356)
- `DittoBackend::ditto_store() -> &DittoStore` (ditto_backend.rs:110)
- `E2EHarness::allocate_tcp_port()` (e2e_harness.rs:64)

### ❓ UNCLEAR/MISSING
- Automerge peer connection setup pattern
- `wait_for_peer_connection()` equivalent for Automerge
- Iroh transport peer discovery mechanics

## Per CLAUDE.md Policy

> "Acceptance of flaky tests is never allowed. We must resolve these issues immediately. Deferring is irresponsible."

**Action Required**: These tests must be fixed before considering the CRDT parity validation complete.

## Next Steps

### Immediate Priority
1. **Investigate Ditto sync activation** - Connections work, but data doesn't sync
   - Review DittoStore/DittoBackend sync initialization
   - Check if subscriptions require additional setup for replication
   - Compare with working `squad_formation_e2e.rs` test to find differences

2. **Investigate Automerge sync activation** - Connections work, but data doesn't sync
   - Review AutomergeBackend/AutomergeStore sync initialization
   - Check if document sync needs explicit activation
   - Verify Iroh transport is properly integrated with Automerge sync protocol

3. **Document findings** - Once root cause identified, update this file with solution

### Success Criteria
- All 6 tests passing
- Data syncing reliably between peers within 10 seconds
- CRDT semantics validated (G-Set, OR-Set, LWW-Register)

## References

- Working pattern: `tests/squad_formation_e2e.rs:84-103`
- E2E Harness: `src/testing/e2e_harness.rs`
- DittoBackend: `src/sync/ditto_backend.rs:110-112`
- Failing tests: `tests/storage_layer_e2e.rs:145, 167, 197, 267, 339, 397`
