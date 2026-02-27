# Traditional IoT Baseline - Design Document

**Purpose:** Non-CRDT baseline implementation for PEAT Protocol architectural comparison
**Date:** 2025-11-07
**Status:** Design phase

## Architecture Overview

### Core Concept

**Traditional event-driven IoT messaging:**
- NO CRDT (no delta-state sync, no automatic convergence)
- Periodic full-state transmission at configurable frequency
- Simple TCP client-server or pub/sub messaging
- Receiver overwrites state with latest message (last-write-wins)

### Comparison Target

This implementation provides the "control group" to measure:
1. **CRDT overhead** (CAP Full vs Traditional Baseline)
2. **CAP filtering benefit** (CAP Differential vs CAP Full)
3. **Net architectural advantage** (CAP Differential vs Traditional Baseline)

## System Design

### Message Flow

```
Traditional IoT (Client-Server):

┌─────────┐                        ┌─────────┐
│ Node 2  │───── Full State ─────→│ Node 1  │
│(Reader) │      Every 5s          │(Server) │
└─────────┘                        └─────────┘
                                       ▲
                                       │
                                   Full State
                                    Every 5s
                                       │
┌─────────┐                            │
│ Node 3  │────────────────────────────┘
│(Reader) │
└─────────┘

Every node sends ENTIRE state every period.
Server receives, aggregates, redistributes.
```

**PEAT Protocol (for comparison):**
```
┌─────────┐                        ┌─────────┐
│ Node 2  │←──── Delta Sync ───────│ Node 1  │
│(Reader) │      Event-driven      │(CRDT)   │
└─────────┘                        └─────────┘
                                       ▲
                                       │
                                   Delta Sync
                                  Event-driven
                                       │
┌─────────┐                            │
│ Node 3  │────────────────────────────┘
│(Reader) │
└─────────┘

Only transmits changes (deltas).
CRDT guarantees eventual convergence.
```

### Data Model

**Full State Message:**
```rust
#[derive(Serialize, Deserialize, Clone)]
struct FullStateMessage {
    // Metadata
    node_id: String,
    timestamp_us: u64,
    sequence_number: u64,

    // Complete node state (NOT deltas)
    documents: Vec<Document>,

    // Metrics
    message_size_bytes: usize,
}

#[derive(Serialize, Deserialize, Clone)]
struct Document {
    doc_id: String,
    content: String,
    version: u64,
    // NO CRDT metadata
}
```

**Key Differences from CRDT:**
- No delta tracking
- No merge semantics
- No vector clocks or causal ordering
- Simple last-write-wins (LWW) based on timestamp

### Node Types

#### 1. Server Node (Writer)
**Role:** Centralized coordinator (like soldier-1 squad leader)

**Behavior:**
```rust
loop {
    // 1. Generate/update own state
    update_local_state();

    // 2. Serialize FULL state
    let message = FullStateMessage {
        node_id: "soldier-1",
        timestamp_us: now(),
        documents: self.all_documents.clone(), // ENTIRE collection
        ...
    };

    // 3. Broadcast to all connected clients
    for client in &self.connected_clients {
        send_tcp(client, &message);
    }

    // 4. Emit metrics
    emit_metric(MessageSent { size: message.len() });

    // 5. Wait for next period
    sleep(self.update_frequency); // e.g., 5 seconds
}
```

**Also receives:** Full state messages from clients (aggregates data)

#### 2. Client Node (Reader)
**Role:** Edge device (like soldier-2 rifleman)

**Behavior:**
```rust
// Send thread
loop {
    // 1. Serialize FULL local state
    let message = FullStateMessage {
        node_id: "soldier-2",
        documents: self.local_documents.clone(),
        ...
    };

    // 2. Send to server
    send_tcp(&self.server_address, &message);

    // 3. Wait
    sleep(self.update_frequency);
}

// Receive thread
loop {
    // 1. Receive full state from server
    let message = receive_tcp();

    // 2. Overwrite local state (NO MERGE!)
    self.remote_state = message.documents;

    // 3. Emit metrics
    emit_metric(MessageReceived {
        latency: now() - message.timestamp_us,
        size: message.len(),
    });
}
```

### Topology Support

#### Client-Server (Primary)
```
All nodes → soldier-1 (server)
soldier-1 → All nodes

Benefits:
- Avoids n-squared problem
- Centralized coordination
- Simple routing

Drawbacks:
- Single point of failure
- All traffic through server
```

#### Hub-Spoke (Secondary)
```
Team members → Team leader → Squad leader

Benefits:
- Hierarchical scaling
- Load distribution

Drawbacks:
- More complex routing
- Multi-hop latency
```

#### Mesh (EXPLICITLY AVOIDED)
```
Every node → Every other node

N-squared problem:
- 12 nodes × 11 destinations = 132 connections
- Bandwidth explosion
- NOT SUPPORTED in traditional baseline
```

## Implementation Plan

### File Structure
```
peat-protocol/examples/traditional_baseline.rs

Dependencies:
- tokio (async runtime)
- serde_json (message serialization)
- peat-schema (reuse protobuf Document types for consistency)
```

### Command-Line Arguments
```bash
traditional_baseline \
  --node-id soldier-1 \
  --mode server \
  --listen 0.0.0.0:12345 \
  --update-frequency 5s \
  --num-documents 1

traditional_baseline \
  --node-id soldier-2 \
  --mode client \
  --connect soldier-1:12345 \
  --update-frequency 5s
```

### Metrics Output

**Same format as CAP tests (JSON):**
```json
{"event_type":"MessageSent","node_id":"soldier-1","message_number":1,"message_size_bytes":1024,"timestamp_us":1762561629283549}
{"event_type":"MessageReceived","node_id":"soldier-2","latency_us":15000,"message_size_bytes":1024,"timestamp_us":1762561629298549}
```

### Configuration Matrix

| Parameter | Traditional IoT | CAP Full | CAP Differential |
|-----------|-----------------|----------|------------------|
| Update mechanism | Periodic (5s) | Event-driven | Event-driven |
| Message type | Full state | CRDT deltas | CRDT deltas (filtered) |
| Sync guarantee | None (LWW) | Eventual consistency | Eventual consistency |
| Bandwidth | High (full messages) | Medium (deltas) | Low (filtered deltas) |
| Latency | 0-5s (periodic) | <100ms (event-driven) | <100ms (event-driven) |

## Comparison Scenarios

### Test 1: Single Document, 2 Nodes

**Setup:**
- Node 1 (server): Creates 1 document
- Node 2 (client): Receives updates

**Traditional Baseline:**
- Node 1 sends full state every 5s (1KB payload)
- Bandwidth: 1KB / 5s = 0.2 KB/s = 1.6 Kbps
- Latency: 0-5 seconds (depends on timing)

**CAP Full:**
- Node 1 sends delta on document creation (~200 bytes)
- Bandwidth: 200 bytes once = 0.04 KB/s average
- Latency: <100ms (event-driven)

**Result:** CRDT is **95% more efficient** for infrequent updates

### Test 2: 10 Documents, 12 Nodes, Client-Server

**Traditional Baseline:**
- Each node sends 10KB (10 × 1KB docs) every 5s
- 12 nodes → server: 12 × 10KB / 5s = 24 KB/s = 192 Kbps
- Server → 12 nodes: 12 × 120KB (all docs) / 5s = 288 KB/s = 2.3 Mbps
- **Total: ~2.5 Mbps continuous**

**CAP Full:**
- Initial sync: 120KB across 12 nodes
- Subsequent updates: Only deltas (event-driven)
- Steady-state: ~10-50 Kbps (95% reduction)

**CAP Differential (with role filtering):**
- Only authorized documents transmitted
- Example: 60 docs instead of 120 (50% reduction)
- Steady-state: ~5-25 Kbps (98% reduction vs Traditional)

### Test 3: High-Frequency Updates

**Traditional Baseline (1s frequency):**
- 12 nodes × 10KB / 1s = 120 KB/s = 960 Kbps

**Traditional Baseline (10s frequency):**
- 12 nodes × 10KB / 10s = 12 KB/s = 96 Kbps

**Trade-off:** Latency vs Bandwidth
- Low frequency (10s) → Better bandwidth, worse latency
- High frequency (1s) → Better latency, worse bandwidth

**PEAT Protocol:**
- Event-driven updates → Best of both worlds
- Low latency (<100ms) AND low bandwidth (deltas only)

## Expected Test Results

### Hypothesis

**Bandwidth Usage (12-node squad, 10 documents):**
```
Traditional IoT (5s):     ~2.5 Mbps  (baseline)
CAP Full Replication:     ~0.1 Mbps  (96% reduction) ← CRDT delta sync benefit
CAP Differential:         ~0.05 Mbps (98% reduction) ← CRDT + filtering benefit
```

**Latency (document creation → reception):**
```
Traditional IoT:   0-5000ms (depends on transmission cycle)
CAP Full:          <100ms   (event-driven)
CAP Differential:  <100ms   (event-driven + filtered)
```

### Validation Criteria

✅ **Traditional baseline is working correctly if:**
1. Bandwidth scales with message frequency (inversely proportional)
2. Latency is bounded by update period (0 to period_ms)
3. Bandwidth usage matches calculation: `nodes × msg_size / period`
4. Full state transmitted every period (no delta compression)

✅ **PEAT Protocol demonstrates value if:**
1. CRDT reduces bandwidth by 60-95% vs Traditional (delta sync benefit)
2. CRDT reduces latency by 50-90% vs Traditional (event-driven benefit)
3. CAP filtering reduces bandwidth by additional 30-50% (capability benefit)
4. Net result: CAP Differential is 75-98% more efficient than Traditional

## Implementation Checklist

### Phase 2A: Core Implementation
- [ ] Create `traditional_baseline.rs`
- [ ] Implement `FullStateMessage` struct
- [ ] Implement TCP server mode (listen, accept clients, broadcast)
- [ ] Implement TCP client mode (connect, send, receive)
- [ ] Periodic transmission loop (configurable frequency)
- [ ] Metrics collection (JSON output identical to CAP tests)
- [ ] Command-line argument parsing
- [ ] 2-node validation test

### Phase 2B: Integration
- [ ] Update `peat-sim/Dockerfile` to build `traditional_baseline`
- [ ] Create ContainerLab topology files for Traditional baseline
- [ ] Integrate with existing test scripts
- [ ] 12-node validation test

### Phase 2C: Comparison Testing
- [ ] Run Traditional baseline across all bandwidths
- [ ] Generate comparison metrics vs CAP Full
- [ ] Generate comparison metrics vs CAP Differential
- [ ] Create three-way comparison report

## Risk Mitigation

### Risk 1: TCP Connection Management
**Problem:** Handling client disconnects/reconnects

**Mitigation:**
- Server tracks active connections
- Clients auto-reconnect on failure
- Heartbeat mechanism (optional)

### Risk 2: Message Ordering
**Problem:** Out-of-order delivery

**Mitigation:**
- Sequence numbers in messages
- Timestamp-based LWW (last-write-wins)
- Accept out-of-order (matches real IoT behavior)

### Risk 3: State Size Growth
**Problem:** Full state messages grow unbounded

**Mitigation:**
- Document count limit (configurable)
- Message size metrics to track growth
- Matches real IoT constraint (limited state)

## Success Criteria

✅ **Phase 2A Complete when:**
1. `traditional_baseline.rs` compiles and runs
2. 2-node test demonstrates periodic full-state transmission
3. Metrics output in JSON format
4. Bandwidth usage matches theoretical calculation

✅ **Phase 2B Complete when:**
1. Docker image includes `traditional_baseline` binary
2. 12-node test runs successfully
3. Topology files support client-server and hub-spoke

✅ **Phase 2C Complete when:**
1. Three-way comparison completed (Traditional, CAP Full, CAP Differential)
2. Bandwidth reduction quantified
3. Latency comparison quantified
4. ROI analysis report generated

---

**Next Step:** Begin implementation of `traditional_baseline.rs`

**Estimated Effort:** 3-4 hours

**Target Completion:** 2025-11-08
