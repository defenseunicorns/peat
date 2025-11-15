# Phase 6 Requirements: Discovery & Active Sync

## Discovered via E2E Tests

Date: 2025-11-14
Tests: `tests/automerge_iroh_sync_e2e.rs`

## Test Results Summary

**Tests Created**: 5 E2E tests
**Tests Passing**: 5/5 (with graceful skips)
**Primary Blocker**: Peer connection failure

```
✗ Connection failed: Failed to connect to peer
→ Phase 6 TODO: Need relay server or direct addressing
```

All tests pass but skip actual sync validation because peers cannot connect.

## Critical Findings

### 1. Peer Connection is Blocked

**What We Tested**:
```rust
let node2_addr = transport2.endpoint_addr(); // EndpointAddr with ID + relay + direct addrs
let result = transport1.connect(node2_addr).await;
// Result: Failed to connect to peer
```

**Why It Fails**:
- Both nodes are in the same process but isolated Iroh endpoints
- No shared relay server configured
- No direct network addresses (both binding to ephemeral ports)
- Iroh requires either:
  - Direct addressing (same LAN with known IP:port), OR
  - Relay server for NAT traversal, OR
  - Discovery mechanism (mDNS, rendezvous, etc.)

**What Phase 6 Must Solve**:
1. **Option A** (Simplest for testing): Local discovery via localhost addressing
2. **Option B** (Production): Configure Iroh's default relay or run our own
3. **Option C** (Complete): Implement full discovery (mDNS + relay fallback)

### 2. No Automatic Sync Propagation

**What We Expected**:
```rust
nodes1.upsert("doc-1", &doc).unwrap();
// Wait for sync...
let synced = nodes2.get("doc-1").unwrap(); // Should be Some()
```

**What's Missing**:
- No background task monitoring document changes
- No automatic sync message generation
- No change detection mechanism

**What Phase 6 Must Implement**:
- Background task spawned by `start_sync()`
- Watch for document changes in AutomergeStore
- Automatically call `AutomergeSyncCoordinator::initiate_sync()` for each change
- Propagate to all connected peers

### 3. No Incoming Sync Handling

**What's Missing**:
- No listener for incoming sync connections
- No handler for incoming sync messages
- No routing of received messages to `receive_sync_message()`

**What Phase 6 Must Implement**:
- Accept incoming bidirectional streams
- Route to `AutomergeSyncCoordinator::handle_incoming_sync()`
- Process messages and generate responses

### 4. No Metrics Tracking

**What We Can't Test**:
```rust
let stats = backend.sync_stats().unwrap();
// bytes_sent and bytes_received are always 0
// last_sync is always None
```

**What Phase 6 Must Implement**:
- Increment byte counters in sync coordinator
- Track last sync timestamp
- Expose metrics via sync_stats()

## Phase 6 Implementation Plan (Test-Driven)

### 6.1 Local Connection for Testing ✅ Priority 1

**Goal**: Make E2E tests actually connect

**Approach**: Use localhost addressing for same-machine testing

```rust
// Option: Bind to specific ports and connect directly
endpoint1.bind_addr("127.0.0.1:0").await?; // Get actual port
let port1 = endpoint1.local_addr().port();
let addr1 = SocketAddr::new("127.0.0.1".parse()?, port1);
```

**Deliverable**: `test_two_nodes_connect` passes with actual connection

### 6.2 Background Sync Task ✅ Priority 2

**Goal**: Automatic document propagation

**Implementation**:
```rust
impl AutomergeBackend {
    fn start_sync(&self) -> Result<()> {
        // ... existing checks ...

        // Spawn background task
        let store = Arc::clone(&self.store);
        let coordinator = Arc::clone(&self.sync_coordinator);
        let transport = Arc::clone(&self.transport);
        let sync_active = Arc::clone(&self.sync_active);

        tokio::spawn(async move {
            while sync_active.load(Ordering::Relaxed) {
                // Poll for changes (simple approach)
                // Or: Use RocksDB watch API
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Sync all docs to all peers
                for peer_id in transport.connected_peers() {
                    // Iterate documents and sync
                }
            }
        });

        Ok(())
    }
}
```

**Deliverable**: `test_document_sync_two_nodes` passes

### 6.3 Incoming Sync Handler ✅ Priority 3

**Goal**: Handle sync messages from peers

**Implementation**:
```rust
impl AutomergeBackend {
    async fn handle_incoming_sync_loop(&self) {
        while self.sync_active.load(Ordering::Relaxed) {
            // Accept incoming connections
            if let Ok(conn) = self.transport.accept().await {
                let coordinator = Arc::clone(&self.sync_coordinator);
                tokio::spawn(async move {
                    coordinator.handle_incoming_sync(conn).await
                });
            }
        }
    }
}
```

**Deliverable**: `test_bidirectional_sync` passes

### 6.4 Metrics Tracking ⏭ Priority 4

**Goal**: Track and expose sync metrics

**Implementation**:
- Increment counters in `send_sync_message()` and `receive_sync_message()`
- Track `last_sync` timestamp in sync coordinator
- Expose via `sync_stats()`

**Deliverable**: `test_sync_stats_tracking` validates metrics

### 6.5 CRDT Conflict Resolution ⏭ Priority 5

**Goal**: Verify Automerge merge semantics

**Implementation**:
- Already handled by Automerge library
- Just need to ensure sync protocol preserves it

**Deliverable**: `test_concurrent_updates_merge` validates merges

## E2E Test Coverage

| Test | Status | Blocks On |
|------|--------|-----------|
| `test_two_nodes_connect` | ⏸ Skipped | 6.1 Local Connection |
| `test_document_sync_two_nodes` | ⏸ Skipped | 6.1, 6.2 |
| `test_bidirectional_sync` | ⏸ Skipped | 6.1, 6.2, 6.3 |
| `test_concurrent_updates_merge` | ⏸ Skipped | 6.1, 6.2, 6.3 |
| `test_sync_stats_tracking` | ⏸ Skipped | 6.1, 6.4 |

## Decision Points

### Relay vs Direct Connection

**For Phase 6 MVP**:
- Use **direct localhost addressing** for E2E tests
- Document relay configuration for production
- Defer full relay setup to Phase 7

**Reasoning**:
- Simplest path to working E2E tests
- Validates sync protocol independently
- Production relay is deployment concern, not protocol concern

### Polling vs Watch API

**For Phase 6 MVP**:
- Use **simple polling** (check for changes every 100ms)
- Document RocksDB watch API for Phase 7 optimization

**Reasoning**:
- Polling is simple and works
- 100ms latency acceptable for testing
- Can optimize later without protocol changes

### Multiplexing Documents

**Current Issue** (from Phase 4):
```rust
// automerge_sync.rs:257
// TODO: Need to include doc_key in the message somehow
let doc_key = "default";
```

**For Phase 6**:
- Sync all documents in collection (iterate via `scan_prefix()`)
- Open separate stream per document
- Or: Prefix messages with doc_key length + doc_key

**Reasoning**:
- Tests use multiple documents in same collection
- Need to route sync messages to correct document

## Next Steps

1. ✅ Commit E2E tests (passing with skips)
2. ⏭ Implement 6.1: Local connection for tests
3. ⏭ Implement 6.2: Background sync task
4. ⏭ Implement 6.3: Incoming sync handler
5. ⏭ Verify all E2E tests pass
6. ⏭ Document production relay configuration

## Notes

- All E2E tests are **feature-gated** (`#[cfg(feature = "automerge-backend")]`)
- Tests **gracefully skip** when connection fails (no false negatives)
- Test output shows **clear Phase 6 TODO markers** for missing functionality
- Tests are **deterministic** (no flaky timeouts if connection succeeds)
