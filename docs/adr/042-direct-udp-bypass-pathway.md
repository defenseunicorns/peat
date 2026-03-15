# ADR-042: Direct-to-UDP Bypass Pathway

**Status**: Proposed
**Date**: 2026-01-06
**Authors**: Kit Plummer, Codex
**Related ADRs**:
- [ADR-010](010-transport-layer-udp-tcp.md) (Transport Layer - UDP vs TCP, superseded)
- [ADR-011](011-ditto-vs-automerge-iroh.md) (Automerge + Iroh Integration)
- [ADR-019](019-qos-and-data-prioritization.md) (QoS and Data Prioritization)
- [ADR-032](032-pluggable-transport-abstraction.md) (Pluggable Transport Abstraction)

---

## Context

### Problem Statement

Peat Protocol uses CRDT-based synchronization (Automerge over Iroh/QUIC) for data consistency across mesh networks. This approach provides:
- Conflict-free eventual consistency
- Automatic merge of concurrent updates
- Reliable, ordered delivery via QUIC

However, some operational scenarios require **bypassing the sync engine entirely** for specific data:

1. **High-frequency telemetry** (10-100 Hz position updates)
   - GPS position every 100ms is stale by the time CRDT sync completes
   - Next update supersedes previous; no need for conflict resolution
   - Loss tolerance is high (next update coming)

2. **Low-latency commands**
   - Emergency stop commands need <50ms delivery
   - CRDT overhead adds unnecessary latency
   - One-way fire-and-forget semantics acceptable

3. **Bandwidth-constrained networks**
   - Tactical radio at 9.6kbps cannot afford CRDT metadata overhead
   - Raw UDP is 10x more efficient for simple telemetry
   - QUIC handshake overhead prohibitive for intermittent links

4. **Multicast/broadcast scenarios**
   - Position broadcasts to all nearby nodes
   - Single packet to multicast group vs N QUIC connections
   - Formation commands to all cell members

### Current Architecture Gap

The existing architecture assumes all data flows through the CRDT sync engine:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Current Data Flow                             │
│                                                                  │
│   Application                                                    │
│       │                                                          │
│       ▼                                                          │
│   ┌─────────────┐                                                │
│   │ CRDT Store  │ ◄── ALL data goes through sync                 │
│   │ (Automerge) │                                                │
│   └──────┬──────┘                                                │
│          │                                                       │
│          ▼                                                       │
│   ┌─────────────┐                                                │
│   │   Iroh      │ ◄── QUIC transport only                        │
│   │  Transport  │                                                │
│   └─────────────┘                                                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

This is suboptimal for ephemeral, high-frequency, or latency-sensitive data.

### Reference: ADR-010 Transport Matrix

ADR-010 (superseded) identified the transport mismatch problem:

| Data Type | Frequency | Loss Tolerance | Natural Transport |
|-----------|-----------|----------------|-------------------|
| Position | 10 Hz | High | **UDP** |
| Sensor detections | 1-10 Hz | Medium | **UDP** |
| Capability changes | Event | Zero | TCP/QUIC |
| CRDT sync | Event | Zero | TCP/QUIC |
| Commands | Event | Zero | UDP + TCP confirm |

The insight remains valid: not all data benefits from CRDT sync.

---

## Decision

### Direct UDP Bypass Architecture

We will implement a **Direct UDP Bypass Pathway** that allows specific collections or document types to be transmitted via raw UDP, bypassing the CRDT sync engine entirely.

```
┌─────────────────────────────────────────────────────────────────┐
│                   Proposed Data Flow                             │
│                                                                  │
│   Application                                                    │
│       │                                                          │
│       ├──────────────────────────┐                               │
│       ▼                          ▼                               │
│   ┌─────────────┐          ┌─────────────┐                       │
│   │ CRDT Store  │          │  UDP Bypass │ ◄── Ephemeral data    │
│   │ (Automerge) │          │   Channel   │                       │
│   └──────┬──────┘          └──────┬──────┘                       │
│          │                        │                              │
│          ▼                        ▼                              │
│   ┌─────────────┐          ┌─────────────┐                       │
│   │   Iroh      │          │    Raw      │                       │
│   │  Transport  │          │    UDP      │                       │
│   └─────────────┘          └─────────────┘                       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Bypass Channel Configuration

#### Collection-Level Bypass

Designate entire collections to use UDP bypass:

```rust
/// Configuration for bypass channel
#[derive(Debug, Clone)]
pub struct BypassChannelConfig {
    /// Collections that use UDP bypass instead of CRDT sync
    pub bypass_collections: Vec<BypassCollectionConfig>,

    /// Default UDP configuration
    pub udp_config: UdpChannelConfig,

    /// Enable multicast for broadcast scenarios
    pub multicast_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct BypassCollectionConfig {
    /// Collection name (e.g., "telemetry", "position_updates")
    pub collection: String,

    /// Transport mode for this collection
    pub transport: BypassTransport,

    /// Message encoding format
    pub encoding: MessageEncoding,

    /// Time-to-live for messages (enables stale filtering)
    pub ttl: Duration,

    /// QoS priority for bandwidth allocation
    pub priority: Priority,
}

#[derive(Debug, Clone)]
pub enum BypassTransport {
    /// UDP unicast to specific peer
    UdpUnicast,

    /// UDP multicast to group
    UdpMulticast {
        group: IpAddr,
        port: u16,
    },

    /// UDP broadcast on subnet
    UdpBroadcast,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageEncoding {
    /// Protobuf (recommended - compact)
    Protobuf,

    /// JSON (debugging)
    Json,

    /// Raw bytes (minimal overhead)
    Raw,

    /// CBOR (compact binary)
    Cbor,
}
```

#### Document-Level Bypass

Individual writes can specify bypass at the document level:

```rust
/// Write options for data storage
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    /// Use UDP bypass instead of CRDT sync
    pub bypass_sync: bool,

    /// Override default transport for this write
    pub transport: Option<BypassTransport>,

    /// TTL for ephemeral data
    pub ttl: Option<Duration>,

    /// Priority for QoS
    pub priority: Priority,
}

impl DocumentStore {
    /// Write with options including bypass
    pub async fn write_with_options(
        &self,
        collection: &str,
        doc: Document,
        options: WriteOptions,
    ) -> Result<(), WriteError> {
        if options.bypass_sync || self.is_bypass_collection(collection) {
            self.send_via_bypass(collection, doc, options).await
        } else {
            self.write_to_crdt(collection, doc).await
        }
    }
}
```

#### Subscription-Level Bypass

Subscribe to bypass channel for incoming data:

```rust
/// Subscription options
#[derive(Debug, Clone)]
pub struct SubscribeOptions {
    /// Receive data from UDP bypass channel
    pub include_bypass: bool,

    /// Receive data from CRDT sync
    pub include_sync: bool,

    /// Filter by source transport
    pub transport_filter: Option<Vec<BypassTransport>>,
}

impl DocumentStore {
    /// Subscribe with options
    pub async fn subscribe_with_options(
        &self,
        collection: &str,
        options: SubscribeOptions,
    ) -> Result<DocumentStream, SubscribeError> {
        // Merge streams from CRDT and bypass channels
        let streams = vec![];

        if options.include_sync {
            streams.push(self.crdt_subscription(collection).await?);
        }

        if options.include_bypass {
            streams.push(self.bypass_subscription(collection).await?);
        }

        Ok(DocumentStream::merge(streams))
    }
}
```

### Bypass Channel Implementation

```rust
/// UDP Bypass Channel for ephemeral data
pub struct UdpBypassChannel {
    /// UDP socket for unicast
    unicast_socket: Arc<UdpSocket>,

    /// Multicast sockets per group
    multicast_sockets: RwLock<HashMap<IpAddr, Arc<UdpSocket>>>,

    /// Message encoder
    encoder: MessageEncoder,

    /// Configuration
    config: BypassChannelConfig,

    /// Metrics
    metrics: BypassMetrics,

    /// Receiver for incoming bypass messages
    incoming_tx: broadcast::Sender<BypassMessage>,
}

impl UdpBypassChannel {
    /// Create new bypass channel
    pub async fn new(config: BypassChannelConfig) -> Result<Self, BypassError> {
        let unicast_socket = UdpSocket::bind("0.0.0.0:0").await?;

        Ok(Self {
            unicast_socket: Arc::new(unicast_socket),
            multicast_sockets: RwLock::new(HashMap::new()),
            encoder: MessageEncoder::new(config.default_encoding),
            config,
            metrics: BypassMetrics::default(),
            incoming_tx: broadcast::channel(1024).0,
        })
    }

    /// Send document via bypass channel
    pub async fn send(
        &self,
        target: BypassTarget,
        collection: &str,
        doc: &Document,
        options: &WriteOptions,
    ) -> Result<(), BypassError> {
        // Encode message
        let config = self.get_collection_config(collection);
        let encoded = self.encoder.encode(doc, config.encoding)?;

        // Frame with header
        let framed = self.frame_message(collection, &encoded, options)?;

        // Send based on transport mode
        match &config.transport {
            BypassTransport::UdpUnicast => {
                self.unicast_socket.send_to(&framed, target.address()).await?;
            }
            BypassTransport::UdpMulticast { group, port } => {
                let socket = self.get_or_create_multicast(*group).await?;
                socket.send_to(&framed, (*group, *port)).await?;
            }
            BypassTransport::UdpBroadcast => {
                self.unicast_socket.set_broadcast(true)?;
                self.unicast_socket.send_to(&framed, "255.255.255.255:5150").await?;
            }
        }

        self.metrics.messages_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Subscribe to incoming bypass messages
    pub fn subscribe(&self, collection: &str) -> broadcast::Receiver<BypassMessage> {
        self.incoming_tx.subscribe()
    }

    /// Start receiving loop
    pub async fn start_receiver(&self) -> Result<(), BypassError> {
        let socket = self.unicast_socket.clone();
        let incoming_tx = self.incoming_tx.clone();
        let encoder = self.encoder.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        if let Ok(msg) = Self::parse_message(&buf[..len], &encoder) {
                            let _ = incoming_tx.send(BypassMessage {
                                source: src,
                                document: msg,
                                received_at: Instant::now(),
                            });
                        }
                    }
                    Err(e) => {
                        warn!("Bypass receive error: {:?}", e);
                    }
                }
            }
        });

        Ok(())
    }
}
```

### Message Framing

Bypass messages use a compact header for identification and TTL:

```rust
/// Bypass message header (12 bytes)
#[repr(C, packed)]
struct BypassHeader {
    /// Magic number (0xPeat)
    magic: [u8; 4],

    /// Message type/collection hash (4 bytes)
    collection_hash: u32,

    /// TTL in milliseconds (2 bytes, max ~65s)
    ttl_ms: u16,

    /// Flags (1 byte)
    flags: u8,

    /// Sequence number (1 byte, wrapping)
    sequence: u8,
}

impl BypassHeader {
    const MAGIC: [u8; 4] = [0x48, 0x49, 0x56, 0x45]; // "Peat"

    fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC
    }

    fn is_stale(&self, received_at: Instant) -> bool {
        received_at.elapsed() > Duration::from_millis(self.ttl_ms as u64)
    }
}
```

### Integration with TransportManager

The bypass channel integrates with ADR-032's TransportManager:

```rust
impl TransportManager {
    /// Send message with automatic transport selection
    pub async fn send(
        &self,
        peer_id: &NodeId,
        data: &[u8],
        requirements: MessageRequirements,
    ) -> Result<(), TransportError> {
        // Check if bypass is preferred
        if requirements.bypass_sync {
            return self.bypass_channel.send(
                BypassTarget::Peer(peer_id.clone()),
                &requirements.collection,
                data,
                &requirements.into(),
            ).await.map_err(Into::into);
        }

        // Normal transport selection
        let transport = self.select_transport(peer_id, &requirements)?;
        transport.send(data).await
    }
}
```

---

## Use Cases

### Use Case 1: High-Frequency Position Updates

**Scenario**: UAV sends position at 10 Hz to squad members

```rust
// Configure position_updates collection for bypass
let config = BypassChannelConfig {
    bypass_collections: vec![
        BypassCollectionConfig {
            collection: "position_updates".into(),
            transport: BypassTransport::UdpMulticast {
                group: "239.1.1.100".parse().unwrap(),
                port: 5150,
            },
            encoding: MessageEncoding::Protobuf,
            ttl: Duration::from_millis(200), // Stale after 200ms
            priority: Priority::P3,
        },
    ],
    ..Default::default()
};

// Send position update (bypasses CRDT)
store.write_with_options(
    "position_updates",
    PositionUpdate { lat: 38.8977, lon: -77.0365, alt: 100.0 },
    WriteOptions { bypass_sync: true, ..Default::default() },
).await?;
```

### Use Case 2: Emergency Stop Command

**Scenario**: Operator sends emergency stop to all platforms

```rust
// Emergency command via UDP broadcast
store.write_with_options(
    "emergency_commands",
    EmergencyStop { command_id: uuid, reason: "Geofence breach" },
    WriteOptions {
        bypass_sync: true,
        transport: Some(BypassTransport::UdpBroadcast),
        priority: Priority::P0,
        ..Default::default()
    },
).await?;

// Also write to CRDT for persistence/audit
store.write("emergency_commands", EmergencyStop { .. }).await?;
```

### Use Case 3: Sensor Telemetry on Constrained Link

**Scenario**: Ground sensor on 9.6kbps radio sends detections

```rust
// Configure for minimal overhead
let config = BypassCollectionConfig {
    collection: "sensor_detections".into(),
    transport: BypassTransport::UdpUnicast,
    encoding: MessageEncoding::Cbor, // Most compact
    ttl: Duration::from_secs(5),
    priority: Priority::P2,
};

// Each detection is ~50 bytes instead of ~500 with CRDT
store.write_with_options(
    "sensor_detections",
    Detection { class: "vehicle", confidence: 0.85, bearing: 45.0 },
    WriteOptions { bypass_sync: true, ..Default::default() },
).await?;
```

---

## Message Flow Comparison

### With CRDT Sync (Default)

```
Sender                                           Receiver
  │                                                  │
  ├─► Automerge encode (~100ms)                      │
  │                                                  │
  ├─► QUIC stream setup (if new) (~50ms)            │
  │                                                  │
  ├─► Send over QUIC ──────────────────────────────►│
  │                                                  │
  │                                    Automerge decode (~50ms)
  │                                                  │
  │                                    Merge with local state (~10ms)
  │                                                  │
  Total: ~210ms + network latency                    │
```

### With UDP Bypass

```
Sender                                           Receiver
  │                                                  │
  ├─► Protobuf encode (~1ms)                        │
  │                                                  │
  ├─► UDP send ────────────────────────────────────►│
  │                                                  │
  │                                    Protobuf decode (~1ms)
  │                                                  │
  │                                    Direct to application
  │                                                  │
  Total: ~2ms + network latency                      │
```

---

## Configuration Examples

### YAML Configuration

```yaml
# peat-config.yaml

bypass_channel:
  enabled: true

  # Default UDP settings
  udp:
    bind_port: 5150
    buffer_size: 65536

  # Multicast settings
  multicast:
    enabled: true
    ttl: 32

  # Collections using bypass
  collections:
    - name: "position_updates"
      transport: "multicast"
      multicast_group: "239.1.1.100"
      encoding: "protobuf"
      ttl_ms: 200
      priority: 3

    - name: "sensor_telemetry"
      transport: "unicast"
      encoding: "cbor"
      ttl_ms: 5000
      priority: 2

    - name: "emergency_commands"
      transport: "broadcast"
      encoding: "protobuf"
      ttl_ms: 1000
      priority: 0
```

---

## Security Considerations

### Authentication

Bypass messages do not benefit from Iroh's QUIC authentication. Mitigations:

1. **Message signing**: Optional Ed25519 signature in header
2. **Pre-shared key**: Symmetric encryption with formation key
3. **Source filtering**: Accept only from known peer IPs

```rust
#[derive(Debug, Clone)]
pub struct BypassSecurityConfig {
    /// Require message signatures
    pub require_signature: bool,

    /// Encrypt payload with formation key
    pub encrypt_payload: bool,

    /// Filter sources by known peer addresses
    pub source_allowlist: Option<Vec<IpAddr>>,
}
```

### Replay Protection

Stale message filtering provides basic replay protection:
- TTL prevents old messages from being accepted
- Sequence numbers detect duplicates within window
- Timestamps enable ordering if needed

---

## Implementation Plan

### Phase 1: Core Bypass Channel

- [ ] `UdpBypassChannel` struct with send/receive
- [ ] `BypassHeader` framing
- [ ] Protobuf and CBOR encoding
- [ ] Basic metrics

### Phase 2: Collection Configuration

- [ ] `BypassCollectionConfig` parsing
- [ ] Collection-level bypass routing
- [ ] YAML configuration support

### Phase 3: Integration

- [ ] `WriteOptions::bypass_sync` support
- [ ] `SubscribeOptions` with bypass streams
- [ ] TransportManager integration

### Phase 4: Multicast/Broadcast

- [ ] Multicast group management
- [ ] Broadcast support
- [ ] TTL-based stale filtering

### Phase 5: Security

- [ ] Message signing
- [ ] Payload encryption
- [ ] Source allowlisting

---

## Success Criteria

### Performance

- [ ] Bypass latency < 5ms (vs ~200ms for CRDT)
- [ ] Bypass overhead < 20 bytes (header only)
- [ ] Support 100 Hz message rate per collection

### Functional

- [ ] Collection-level bypass configuration works
- [ ] Document-level `bypass_sync` option works
- [ ] Multicast delivery to all cell members
- [ ] Stale message filtering operational

### Testing

- [ ] Unit tests for bypass channel
- [ ] Integration tests with CRDT + bypass mixed
- [ ] Performance benchmarks (latency, throughput)

---

## Risks and Mitigations

### Risk 1: Data Inconsistency

Bypass data doesn't go through CRDT merge, may cause inconsistency.

**Mitigation**:
- Only use for ephemeral data (positions, telemetry)
- Never use for persistent state (capabilities, membership)
- Document clearly which collections support bypass

### Risk 2: Security Downgrade

UDP bypass lacks Iroh's authentication.

**Mitigation**:
- Optional message signing
- Payload encryption with formation key
- Source IP filtering
- Clear security documentation

### Risk 3: Debugging Complexity

Two data paths increase debugging complexity.

**Mitigation**:
- Rich metrics per channel
- Clear logging with transport indicator
- Unified subscription API merges both sources

---

## References

1. [ADR-010](010-transport-layer-udp-tcp.md) - Original UDP vs TCP analysis
2. [ADR-011](011-ditto-vs-automerge-iroh.md) - Iroh adoption decision
3. [ADR-019](019-qos-and-data-prioritization.md) - Priority framework
4. [ADR-032](032-pluggable-transport-abstraction.md) - Transport abstraction

---

**Last Updated**: 2026-01-06
**Status**: PROPOSED - Awaiting review
