# Automerge+Iroh Backend Implementation Progress

**Status**: Phase 6.1 Complete - P2P Connections Working
**Last Updated**: 2025-11-15

## Overview

The AutomergeIrohBackend provides an open-source alternative to DittoBackend using:
- **Automerge 0.7.1**: CRDT library for conflict-free state
- **RocksDB 0.22**: Persistent storage
- **Iroh 0.95**: QUIC-based P2P networking

## Implementation Phases

### ✅ Phase 1-2: Storage Layer (Previous Session)
- AutomergeStore with RocksDB persistence
- CRDT integration with Automerge
- 13 tests passing

### ✅ Phase 5: SyncCapable Trait (Previous Session)
- Lifecycle management (start_sync, stop_sync, sync_stats)
- Atomic state tracking
- 4 new tests passing

### ✅ Phase 6.1: Static Peer Configuration (Current Session)

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

## Current Capabilities

### ✅ Working
1. **Static Mesh Configuration**: TOML-based peer lists
2. **Direct Addressing**: Localhost P2P without relay
3. **Connection Establishment**: Sub-second connection times
4. **Accept Loop**: Background task receives incoming connections
5. **Connection Management**: Track peers, disconnect, peer count

### ❌ Not Yet Implemented
1. **Document Sync**: Changes don't propagate between nodes
2. **Background Sync Task**: No automatic sync on document changes
3. **CRDC Sync Protocol**: No Automerge sync messages over QUIC
4. **Incoming Sync Handler**: Can't receive/apply remote changes
5. **Metrics Tracking**: bytes_sent/bytes_received not updated

## Next Steps: Phase 6.2

### 1. Update AutomergeBackend.start_sync()
```rust
fn start_sync(&self) -> Result<()> {
    // Start accept loop
    if let Some(transport) = &self.transport {
        transport.start_accept_loop()?;
    }

    // Spawn background sync task (NEW)
    // TODO: Implement

    Ok(())
}
```

### 2. Implement Background Sync Task
- Detect document changes (polling or watch API)
- For each connected peer:
  - Generate Automerge sync message
  - Send over QUIC stream
  - Track bytes sent

### 3. Implement Incoming Sync Handler
- Accept incoming QUIC streams
- Receive Automerge sync messages
- Apply changes to local documents
- Track bytes received

### 4. Verify E2E Tests Pass
- test_document_sync_two_nodes
- test_bidirectional_sync
- test_concurrent_updates_merge
- test_sync_stats_tracking

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

### Connection Times
- First connection: ~0.6s
- Minimal test: 2.58s total (2 nodes, connection, verification)
- Previous (no accept): 30s timeout ❌

### Test Suite
- All automerge-backend tests: <10s
- Pre-commit checks: ~8s (fmt + clippy + tests)

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
