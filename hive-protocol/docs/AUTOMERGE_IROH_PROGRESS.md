# Automerge+Iroh Backend Implementation Progress

**Status**: Phase 6.4 Complete - Heartbeat Exchange Implemented ✅
**Last Updated**: 2025-11-18

## Overview

The AutomergeIrohBackend provides an open-source alternative to DittoBackend using:
- **Automerge 0.7.1**: CRDT library for conflict-free state
- **RocksDB 0.22**: Persistent storage
- **Iroh 0.95**: QUIC-based P2P networking

## Implementation Phases

### ✅ Phase 1-2: Storage Layer
- AutomergeStore with RocksDB persistence
- CRDT integration with Automerge
- 13 tests passing

### ✅ Phase 5: SyncCapable Trait
- Lifecycle management (start_sync, stop_sync, sync_stats)
- Atomic state tracking
- 4 new tests passing

### ✅ Phase 6.1: Static Peer Configuration

#### Phase 6.1a: TOML Configuration
**Files Created:**
- `examples/peers.toml` - Example static mesh configuration
- `src/network/peer_config.rs` - Parser and loader (256 lines, 5 tests)

**Features:**
- TOML-based peer lists with node IDs and addresses
- Hex encoding/decoding for EndpointId (32-byte PublicKey)
- Socket address parsing and validation
- Local bind address configuration
- IPv4-only support (SocketAddrV4)

**Dependencies Added:**
```toml
toml = { version = "0.8", optional = true }
hex = { version = "0.4", optional = true }
```

**API:**
```rust
// Load from file
let config = PeerConfig::from_file("peers.toml")?;

// Get peer info
let peer = config.get_peer("node-1").unwrap();
let endpoint_id = peer.endpoint_id()?;
let addrs = peer.socket_addrs()?;

// Connect using PeerInfo
let conn = transport.connect_peer(peer).await?;
```

#### Phase 6.1b: Accept Loop Infrastructure
**Research:**
- Studied Iroh 0.95 documentation and examples
- Discovered endpoints require active `accept()` loop
- Pattern: Sequential acceptance, concurrent handling

**IrohTransport Enhancements:**
```rust
// New fields
accept_running: Arc<AtomicBool>
accept_task: Arc<RwLock<Option<JoinHandle<()>>>>

// New methods
pub fn start_accept_loop(&Arc<Self>) -> Result<()>
pub fn stop_accept_loop(&self) -> Result<()>
pub fn is_accept_loop_running(&self) -> bool
```

**Accept Loop Design:**
- Spawns background tokio task
- Continuously calls `transport.accept().await`
- Automatically stores accepted connections
- Stops gracefully on `stop_accept_loop()` or `close()`

**Tests Created:**
- `tests/iroh_minimal_connection.rs` - Minimal test proving concept
  - **Result**: ✅ PASS in 2.58s
  - Validates direct addressing works
  - Confirms accept loop pattern

**E2E Test Updates:**
- `test_two_nodes_connect` now uses `start_accept_loop()`
  - **Before**: Timeout after 30s ❌
  - **After**: PASS in 0.63s ✅

## Architecture

### Layered Design (Per ADR)

**Layer 1: IrohTransport** (Connection Management)
- Responsibility: P2P connection lifecycle
- Methods: `start_accept_loop()`, `stop_accept_loop()`, `bind()`, `connect_peer()`
- Independent of sync logic

**Layer 2: AutomergeBackend** (Sync Orchestration)
- Responsibility: CRDT document synchronization
- Will call: `transport.start_accept_loop()` in `start_sync()`
- Matches: Ditto's `start_sync()` pattern

**Layer 3: SyncCapable Trait** (Abstraction)
- Responsibility: Unified backend interface
- Implemented by: Both DittoBackend and AutomergeBackend
- API: `start_sync()`, `stop_sync()`, `sync_stats()`

## Test Coverage

### Unit Tests
- ✅ peer_config parsing (5 tests)
- ✅ IrohTransport creation and methods (4 tests)
- ✅ AutomergeBackend Phase 5 tests (4 tests)

### Integration Tests
- ✅ Minimal Iroh connection (1 test, 2.58s)

### E2E Tests
- ✅ test_two_nodes_connect (PASS 0.63s)
- ⏳ test_document_sync_two_nodes (connection works, sync TODO)
- ⏳ test_bidirectional_sync (connection works, sync TODO)
- ⏳ test_concurrent_updates_merge (connection works, sync TODO)
- ⏳ test_sync_stats_tracking (connection works, metrics TODO)

###  ✅ Phase 6.2: Document Sync Over Iroh (COMPLETE)

**Status**: ✅ All E2E tests passing (5/5)

#### Implementation Details

**Background Sync Task** (automerge_backend.rs:434-459):
- Subscribes to document change notifications via `AutomergeStore.subscribe_to_changes()`
- Automatically syncs changed documents with all connected peers
- Runs in background tokio task while `sync_active` is true

**Incoming Sync Handler** (automerge_backend.rs:399-428):
- Spawned by `start_sync()` to handle incoming sync requests
- Polls connected peers for incoming streams
- Calls `AutomergeSyncCoordinator.handle_incoming_sync()`  for each stream
- Applies remote changes to local documents

**Sync Coordinator** (automerge_sync.rs):
- `initiate_sync()` - Generate and send initial sync message
- `receive_sync_message()` - Apply incoming changes, generate response
- `sync_document_with_all_peers()` - Broadcast sync to all connected peers
- Wire format: `[2 bytes: doc_key_len][doc_key][4 bytes: msg_len][sync_message]`

**Metrics Tracking**:
- `total_bytes_sent` / `total_bytes_received` (AtomicU64)
- Per-peer statistics (bytes, sync count, last sync timestamp)
- Tracked in AutomergeSyncCoordinator

#### E2E Test Results

All tests passing in 2.69s:

1. ✅ **test_two_nodes_connect** (0.43s)
   - P2P connection establishment
   - Peer discovery via static config

2. ✅ **test_document_sync_two_nodes** (0.43s)
   - Automatic document synchronization
   - Document created on Node 1 syncs to Node 2

3. ✅ **test_bidirectional_sync** (passing)
   - Documents created on both nodes
   - Bidirectional sync verified

4. ✅ **test_concurrent_updates_merge** (passing)
   - Concurrent updates to same document
   - CRDT merge semantics verified (fuel=100, health=2)

5. ✅ **test_sync_stats_tracking** (passing)
   - Peer count tracking
   - Metrics collection verified

#### Key Features Working

1. **Automatic Sync**: Document changes trigger sync automatically
2. **Bidirectional**: Changes propagate in both directions
3. **CRDT Merging**: Concurrent updates merge correctly
4. **Error Handling**: Circuit breaker and retry logic active
5. **Metrics**: Bytes sent/received tracked per peer

### ✅ Phase 6.3: Partition Detection Infrastructure (COMPLETE)

**Status**: ✅ Foundation complete, heartbeat exchange pending

#### Implementation Details

**Partition Detection Module** (partition_detection.rs, 350 lines):
- `PeerPartitionState` - State machine: Connected/Partitioned/Recovering
- `PeerHeartbeat` - Per-peer heartbeat tracking with timestamps
- `PartitionDetector` - Coordinator for managing all peers
- `PartitionConfig` - Configurable timeouts (default: 5s interval, 15s timeout, 3 failures)

**Integration with AutomergeSyncCoordinator** (automerge_sync.rs:111):
- Added `partition_detector: Arc<PartitionDetector>` field
- Public accessor: `partition_detector()` method
- Ready for heartbeat exchange implementation

#### Conceptual Clarity

**Key Insight**: Automerge CRDTs handle "recovery" automatically via eventual consistency.
**What we implemented**: Operational mechanisms for partition handling:
- **Detection**: Heartbeat mechanism to identify unreachable peers
- **State Tracking**: Monitor partition lifecycle (Connected → Partitioned → Recovering → Connected)
- **Observability**: Foundation for metrics/events

The CRDT provides **correctness guarantees** (no data loss, automatic merge).
This module provides **operational mechanisms** (detection, efficiency, observability).

#### Unit Tests

All 5 tests passing ✅:
- `test_peer_heartbeat_success_resets_failures`
- `test_peer_heartbeat_partition_detection`
- `test_peer_heartbeat_recovery`
- `test_partition_config_defaults`
- `test_partition_detector_creation`

### ✅ Phase 6.4: Heartbeat Exchange (COMPLETE)

**Status**: ✅ Active heartbeat messaging implemented

#### Implementation Details

**Heartbeat Wire Protocol** (automerge_sync.rs:514-629):
- Wire format: `[1 byte: 0x01 marker][8 bytes: timestamp (u64, big-endian)]`
- Uses unidirectional Iroh streams (heartbeats don't need responses)
- Minimal 9-byte message per heartbeat

**Heartbeat Sender** (automerge_backend.rs:483-511):
- Background task sends periodic heartbeats to all connected peers
- Interval configured by `PartitionConfig` (default: 5s)
- Automatically registers peers with partition detector
- Records failures and triggers partition detection

**Heartbeat Receiver** (automerge_backend.rs:441-474):
- Background task accepts incoming heartbeat streams
- Validates heartbeat marker (0x01)
- Records heartbeat success in partition detector
- Updates peer partition state

**Partition Detection Integration**:
- Successful heartbeats → `partition_detector.record_heartbeat_success()`
- Failed heartbeats → `partition_detector.record_heartbeat_failure()`
- Timeout checks → `partition_detector.check_timeouts()`
- State transitions: Connected → Partitioned → Recovering → Connected

**Background Tasks** (3 tasks per node):
1. Heartbeat sender: Sends periodic heartbeats (5s interval)
2. Heartbeat receiver: Accepts incoming heartbeat streams
3. Timeout checker: Detects partitions based on elapsed time

#### Verified Behavior

- ✅ E2E tests still passing (5/5 in 2.83s)
- ✅ Heartbeat tasks spawn/stop with sync lifecycle
- ✅ No performance regression from Phase 6.3

#### Next Steps (Phase 7)

1. **Partition Lifecycle Metrics**: Emit events for partition detection/heal
2. **ContainerLab E2E Testing**: Simulate network partitions with tc/netem
3. **mDNS Discovery**: Zero-config peer discovery (Phase 7)

## Current Capabilities

### ✅ Working (Complete as of Phase 6.4)
1. **Static Mesh Configuration**: TOML-based peer lists
2. **Direct Addressing**: Localhost P2P without relay
3. **Connection Establishment**: Sub-second connection times (~0.4s)
4. **Accept Loop**: Background task receives incoming connections
5. **Connection Management**: Track peers, disconnect, peer count
6. **Document Sync**: ✅ Changes propagate automatically between nodes
7. **Background Sync Task**: ✅ Automatic sync on document changes
8. **Automerge Sync Protocol**: ✅ Sync messages over QUIC streams
9. **Incoming Sync Handler**: ✅ Receives and applies remote changes
10. **Metrics Tracking**: ✅ bytes_sent/bytes_received updated
11. **Partition Detection Infrastructure**: ✅ Heartbeat mechanism with state tracking (Phase 6.3)
12. **Active Heartbeat Exchange**: ✅ Periodic heartbeat messages over Iroh streams (Phase 6.4)

### 🔄 Optimizations Pending (Future)
1. **Partition Lifecycle Metrics**: Emit events for partition detection/heal (Phase 7)
2. **ContainerLab Testing**: Network partition simulation with tc/netem (Phase 7)
3. **Flow Control**: Backpressure for large documents
4. **mDNS Discovery**: Zero-config peer discovery (Phase 7)
5. **Relay Support**: Cross-network sync via Iroh relay (Phase 7)

## Next Steps: Phase 7 (Discovery & Relay)

### Planned Enhancements

1. **mDNS Discovery**
   - Zero-config peer discovery on local networks
   - Automatic peer announcement and discovery
   - Integration with existing static config

2. **Iroh Relay Support**
   - Cross-network sync via Iroh relay servers
   - NAT traversal for WAN deployments
   - Fallback to relay when direct connection fails

3. **Optimizations**
   - Flow control and backpressure for large documents
   - Partition detection and recovery
   - Delta sync optimization
   - Watch API for change detection (reduce polling overhead)

## Key Decisions

### Static Config (Not Relay)
**Decision**: Use TOML files with direct IP:port addressing
**Rationale**: Simplest for testing and small deployments
**Future**: Add relay and mDNS in Phase 7

### IPv4 Only
**Decision**: SocketAddrV4 only (no IPv6)
**Rationale**: Simplifies implementation, sufficient for localhost testing
**Future**: Add IPv6 support when needed

### Polling (Not Watch)
**Decision**: Simple polling for change detection
**Rationale**: Defer watch API optimization until profiling shows need
**Future**: Optimize with Automerge's watch capabilities

### Accept Loop in Transport
**Decision**: Accept loop lives in IrohTransport, not AutomergeBackend
**Rationale**: Separation of concerns - connection vs sync logic
**Benefit**: Can accept connections before sync starts

## Performance

### E2E Test Suite (Phase 6.2)
- All 5 E2E tests: 2.69s total ✅
- Single test: ~0.4-0.5s average
- P2P connection: ~0.4s
- Document sync: <100ms after connection

### Comparison
- **Phase 6.1** (connections only): 2.58s
- **Phase 6.2** (with sync): 2.69s (+110ms overhead for sync)
- **Previous** (no accept loop): 30s timeout ❌

### Test Suite Coverage
- Unit tests: 13 (automerge_store.rs)
- Integration tests: 5 (automerge_iroh_sync_e2e.rs)
- Pre-commit checks: ~10s (fmt + clippy + all tests)

## Dependencies

### Production
```toml
automerge = "0.7.1"
iroh = "0.95"
rocksdb = "0.22"
lru = "0.12"
toml = "0.8"
hex = "0.4"
```

### Development
```toml
tokio = { features = ["test-util"] }
tempfile = "3.13"
```

## Commits

1. `2a21f8b` - Phase 6.1: Static peer configuration for Automerge+Iroh mesh
2. `fcc00c7` - test: Update E2E tests to use static peer configuration
3. `837cea5` - Phase 6.1b: Add accept loop infrastructure to IrohTransport

## References

- [Iroh 0.95 Documentation](https://docs.rs/iroh/0.95.0/iroh/)
- [Iroh Examples](https://github.com/n0-computer/iroh/tree/main/iroh/examples)
- [Automerge 0.7 Documentation](https://docs.rs/automerge/0.7.1/automerge/)
- [ADR-011: AutomergeIrohBackend](../adr/011-automerge-iroh-backend.md)
- [Phase 6 Requirements](AUTOMERGE_IROH_PHASE6_REQUIREMENTS.md)
