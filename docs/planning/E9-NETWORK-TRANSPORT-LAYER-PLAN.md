# E9: Network Transport Layer Implementation Plan

**Epic**: Implement network transport layer for Automerge backend to achieve feature parity with Ditto mesh capabilities

**Status**: Planning
**Created**: 2025-11-05
**Related**: ADR-007 (CRDT Backend Evaluation), ADR-010 (Transport Layer UDP/TCP)

## Problem Statement

The AutomergeBackend (E8) currently lacks network transport functionality. While the CRDT sync protocol works (via `generate_sync_message`/`receive_sync_message`), there's no way for instances to actually connect and exchange messages over the network.

**Ditto provides:**
- Automatic peer discovery (mDNS, Bluetooth)
- Multi-transport mesh (TCP + Bluetooth + mDNS simultaneously)
- Background sync tasks
- Connection lifecycle management
- Network change adaptation

**We currently have:**
- ✅ CRDT sync protocol (Automerge)
- ✅ Sync state management (per-peer)
- ✅ Message generation/reception
- ❌ Network transport layer
- ❌ Peer discovery (stub only)
- ❌ Background sync tasks

## Architecture Overview

### Components to Build

```
┌─────────────────────────────────────────────────────────────┐
│                    AutomergeBackend                          │
├─────────────────────────────────────────────────────────────┤
│  Existing:                                                   │
│  - Document storage (Automerge CRDTs)                       │
│  - Sync protocol (generate/receive_sync_message)            │
│  - Per-peer sync state                                       │
│                                                              │
│  NEW - Network Transport Layer:                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ PeerDiscovery                                          │ │
│  │ - mDNS discovery (mdns-sd crate)                       │ │
│  │ - Manual TCP peers                                     │ │
│  │ - Bluetooth discovery (optional Phase 2)               │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ ConnectionManager                                       │ │
│  │ - TCP listener (tokio::net::TcpListener)               │ │
│  │ - TCP client connections                                │ │
│  │ - Connection pool (HashMap<PeerId, Connection>)        │ │
│  │ - Reconnection logic                                    │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ SyncCoordinator                                         │ │
│  │ - Background sync task (tokio::spawn)                  │ │
│  │ - Continuous sync message exchange                      │ │
│  │ - Per-connection message queues                         │ │
│  │ - Backpressure handling                                 │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ WireProtocol                                            │ │
│  │ - Length-prefixed message framing                       │ │
│  │ - Handshake (peer ID exchange)                          │ │
│  │ - Heartbeat/keepalive                                   │ │
│  │ - Error handling                                        │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Wire Protocol Design

**Message Format** (length-prefixed):
```
┌────────────┬─────────────┬────────────────────┐
│ Length     │ MessageType │ Payload            │
│ (4 bytes)  │ (1 byte)    │ (variable)         │
└────────────┴─────────────┴────────────────────┘
```

**Message Types:**
- `0x01` - Handshake (send peer ID)
- `0x02` - SyncMessage (Automerge sync data)
- `0x03` - Heartbeat
- `0x04` - DocumentRequest (request full doc)
- `0xFF` - Error

**Handshake Flow:**
```
Client                           Server
  |                                |
  |-------- Handshake(peer_id) -->|
  |<------- Handshake(peer_id) ---|
  |                                |
  |<------ SyncMessage(doc1) ----->|
  |<------ SyncMessage(doc2) ----->|
  |         (continuous)            |
```

## Implementation Phases

### Phase 1: TCP Transport Foundation (Week 1)

**Goal**: Basic TCP connectivity and sync

**Tasks:**
1. **WireProtocol module** (`src/sync/automerge/wire_protocol.rs`)
   - Define message types
   - Implement length-prefixed framing
   - Serialization/deserialization

2. **Connection module** (`src/sync/automerge/connection.rs`)
   - TcpStream wrapper
   - Read/write message functions
   - Connection state management

3. **ConnectionManager** (`src/sync/automerge/connection_manager.rs`)
   - TCP listener on configurable port
   - Accept incoming connections
   - Maintain connection pool
   - Handle disconnections

4. **Implement PeerDiscovery::add_peer()** (manual TCP)
   - Parse address
   - Establish TCP connection
   - Perform handshake
   - Add to connection pool

5. **SyncCoordinator basics**
   - Background task to process connections
   - For each connection: generate & send sync messages
   - Receive and apply sync messages

**Success Criteria:**
- Two AutomergeBackend instances can connect via TCP
- Documents sync between instances
- Disconnection handled gracefully

**Test:**
```rust
#[tokio::test]
async fn test_tcp_sync() {
    let backend1 = AutomergeBackend::new();
    backend1.initialize(config_with_port(4000)).await?;

    let backend2 = AutomergeBackend::new();
    backend2.initialize(config_with_port(4001)).await?;

    // Backend2 connects to backend1
    backend2.peer_discovery()
        .add_peer("127.0.0.1:4000", TransportType::Tcp)
        .await?;

    // Create document on backend1
    backend1.document_store().upsert("test", doc).await?;

    // Wait for sync
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify it synced to backend2
    let doc2 = backend2.document_store().get("test", &doc_id).await?;
    assert!(doc2.is_some());
}
```

### Phase 2: mDNS Discovery (Week 2)

**Goal**: Automatic local network peer discovery

**Dependencies:**
- `mdns-sd = "0.11"` (pure Rust mDNS)

**Tasks:**
1. **mDNS module** (`src/sync/automerge/mdns_discovery.rs`)
   - Register service (`_automerge-cap._tcp.local`)
   - Browse for peers
   - Extract IP/port from service info

2. **Integrate with PeerDiscovery::start()**
   - Start mDNS service registration
   - Start browsing
   - Automatically call `add_peer()` for discovered peers

3. **Handle service changes**
   - Detect new peers
   - Detect peer removal
   - Update connection pool

**Success Criteria:**
- AutomergeBackend instances discover each other automatically on LAN
- No manual `add_peer()` needed
- E2E tests work with automatic discovery

**Test:**
```rust
#[tokio::test]
async fn test_mdns_discovery() {
    let backend1 = AutomergeBackend::new();
    backend1.initialize(config).await?;
    backend1.peer_discovery().start().await?; // Starts mDNS

    let backend2 = AutomergeBackend::new();
    backend2.initialize(config).await?;
    backend2.peer_discovery().start().await?;

    // Wait for discovery
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify they found each other
    let peers = backend1.peer_discovery().discovered_peers().await?;
    assert!(peers.len() >= 1);
}
```

### Phase 3: Sync Optimization (Week 3)

**Goal**: Efficient, continuous sync

**Tasks:**
1. **Selective sync**
   - Only sync documents that changed
   - Track document versions/timestamps
   - Implement document request protocol

2. **Backpressure handling**
   - Bounded message queues
   - Flow control
   - Prevent memory exhaustion

3. **Connection recovery**
   - Automatic reconnection on disconnect
   - Exponential backoff
   - Resume sync from last state

4. **Performance tuning**
   - Batch sync messages
   - Compression (optional)
   - Reduce allocations

**Success Criteria:**
- Sync scales to 100+ documents
- Network failures don't cause data loss
- Memory usage remains bounded

### Phase 4: Bluetooth Support (Optional - Week 4)

**Goal**: Ad-hoc mesh without infrastructure

**Dependencies:**
- `btleplug = "0.11"` (Bluetooth LE)

**Tasks:**
1. **Bluetooth discovery module**
   - Advertise as GATT server
   - Scan for peers
   - Establish L2CAP connections

2. **Bluetooth transport adapter**
   - Implement same Connection interface
   - Handle Bluetooth-specific constraints (packet size, latency)

3. **Multi-transport coordination**
   - Use TCP when available
   - Fall back to Bluetooth when isolated
   - Prefer faster transport

**Success Criteria:**
- Peers can sync over Bluetooth when no WiFi
- Seamless transport switching

## Dependencies & Crates

**Network:**
- `tokio = { version = "1.48", features = ["net", "sync", "time"] }` (already have)
- `mdns-sd = "0.11"` - Pure Rust mDNS (NEW)
- `btleplug = "0.11"` - Bluetooth LE (OPTIONAL, Phase 4)

**Serialization:**
- `serde = "1.0"` (already have)
- `bincode = "1.3"` - Efficient binary serialization (NEW, optional)

## Integration with Existing Code

### Changes to `AutomergeBackend`

**New fields:**
```rust
pub struct AutomergeBackend {
    // Existing fields...
    documents: Arc<Mutex<HashMap<String, Automerge>>>,
    sync_states: Arc<Mutex<HashMap<String, sync::State>>>,

    // NEW - Network components
    connection_manager: Arc<ConnectionManager>,
    sync_coordinator: Arc<SyncCoordinator>,
    mdns_discovery: Arc<Mutex<Option<MdnsDiscovery>>>,
}
```

**Modified methods:**
```rust
impl PeerDiscovery for AutomergeBackend {
    async fn start(&self) -> Result<()> {
        // Start TCP listener
        self.connection_manager.start_listener().await?;

        // Start mDNS discovery
        let mdns = MdnsDiscovery::new(self.config.tcp_port)?;
        mdns.start()?;
        *self.mdns_discovery.lock().unwrap() = Some(mdns);

        // Start sync coordinator
        self.sync_coordinator.start().await?;

        Ok(())
    }

    async fn add_peer(&self, address: &str, transport: TransportType) -> Result<()> {
        match transport {
            TransportType::Tcp => {
                let conn = TcpConnection::connect(address).await?;
                conn.handshake(&self.peer_id).await?;
                self.connection_manager.add_connection(conn).await?;
                Ok(())
            }
            _ => Err(Error::Internal("Transport not supported".into()))
        }
    }
}
```

## Testing Strategy

### Unit Tests
- Wire protocol serialization/deserialization
- Connection state machine
- Message framing/parsing

### Integration Tests
- Two-instance TCP sync
- Connection recovery
- mDNS discovery
- Document sync verification

### E2E Tests
- Reuse existing E2E harness
- Replace DittoBackend with AutomergeBackend
- Verify all E2E tests pass

### Performance Tests
- Sync latency measurement
- Throughput benchmarks
- Memory usage under load
- Connection scaling (10, 50, 100 peers)

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| mDNS unreliable on some networks | High | Fallback to manual TCP peers |
| Connection storms (many simultaneous connects) | Medium | Rate limiting, connection pooling |
| Sync loops (redundant messages) | Medium | Track message IDs, deduplication |
| Memory leaks in long-running sync | High | Bounded queues, connection timeouts |
| Transport-specific bugs | Medium | Thorough testing, graceful degradation |

## Success Metrics

**Phase 1 (TCP):**
- ✅ Two instances sync over TCP
- ✅ E2E tests pass with TCP transport
- ✅ Clean shutdown without panics

**Phase 2 (mDNS):**
- ✅ Automatic discovery works on LAN
- ✅ <5 second discovery time
- ✅ Handles 10+ peers

**Phase 3 (Optimization):**
- ✅ 100+ documents sync without issues
- ✅ <100ms sync latency for small changes
- ✅ Connection recovery after network interruption

**Phase 4 (Bluetooth):**
- ✅ Sync works over Bluetooth
- ✅ Automatic transport fallback

## Timeline

- **Week 1**: TCP transport foundation (Phase 1)
- **Week 2**: mDNS discovery (Phase 2)
- **Week 3**: Sync optimization (Phase 3)
- **Week 4**: Bluetooth support (Phase 4, optional)

**Total**: 3-4 weeks to full feature parity with Ditto mesh capabilities

## Open Questions

1. **Do we need UDP transport per ADR-010?**
   - ADR-010 says UDP for telemetry, TCP for CRDT sync
   - For Automerge evaluation, TCP-only is sufficient
   - Can add UDP later if we choose Automerge

2. **How to handle peer ID generation?**
   - Use UUID v4 per instance?
   - Derive from node config ID?
   - **Decision**: Use node config ID if available, else UUID

3. **Should we support NAT traversal?**
   - Ditto handles this internally
   - For Phase 1, assume LAN/direct connectivity
   - Consider STUN/TURN for Phase 4

4. **Message encryption?**
   - Ditto provides built-in encryption
   - For evaluation, plaintext is OK
   - Add TLS wrapper for production

## Next Steps

1. ✅ Run local benchmarks (E8)
2. ✅ Document network functionality gap (this document)
3. → Review and approve E9 plan
4. → Implement Phase 1 (TCP transport)
5. → Implement Phase 2 (mDNS discovery)
6. → Run E2E tests with AutomergeBackend
7. → Collect performance data for ADR-007 decision
