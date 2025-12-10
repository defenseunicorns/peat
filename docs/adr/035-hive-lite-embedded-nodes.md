# ADR-035: HIVE-Lite Embedded Sensor Nodes

## Status

Proposed

## Context

Modern tactical and industrial environments increasingly rely on networks of inexpensive, discrete sensors - environmental monitors, motion detectors, asset trackers, biometric sensors, etc. These devices are typically built on constrained microcontroller platforms like ESP32 (M5Stack, various dev boards) with limited memory, processing power, and often no persistent storage.

The current approach for integrating such sensors is MQTT or similar broker-based protocols, which have significant drawbacks:

1. **Centralized dependency** - All data flows through a broker; broker failure = network failure
2. **No local intelligence** - Sensors are "dumb" producers; they can't benefit from peer data
3. **Bandwidth inefficiency** - All data sent upstream regardless of local relevance
4. **No hierarchical filtering** - Can't aggregate/filter at intermediate tiers
5. **Single point of compromise** - Broker is an attractive attack target

HIVE's mesh architecture offers a fundamentally different model where sensors can be first-class participants in a distributed data fabric, but our current implementation requires:
- Full Rust `std` library support
- Automerge CRDT engine (memory-intensive)
- Persistent storage backends
- Significant RAM (tens of MB minimum)

This ADR proposes HIVE-Lite: a minimal, resource-constrained implementation enabling embedded devices to participate as full mesh members while respecting their hardware limitations.

## Target Hardware Profile

**Reference Platform: M5Stack Core2**
- ESP32-D0WDQ6-V3 (dual-core Xtensa LX6 @ 240MHz)
- 520KB SRAM + 8MB PSRAM
- 16MB Flash
- WiFi 802.11 b/g/n, Bluetooth 4.2 BR/EDR + BLE
- Power: Battery + USB-C

**Minimum Target Specs:**
- 256KB RAM available for HIVE-Lite
- WiFi or BLE connectivity
- No persistent storage required (ephemeral operation)

**Stretch Targets:**
- Devices with 64KB RAM (aggressive optimization)
- LoRa connectivity for long-range mesh
- Optional flash storage for limited persistence

## Decision

We will create HIVE-Lite as a distinct but protocol-compatible implementation targeting embedded devices. Key design decisions:

### 1. Tiered Node Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        HIVE Node Tiers                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │
│  │  HIVE-Full  │    │ HIVE-Edge   │    │ HIVE-Lite   │         │
│  │             │    │             │    │             │         │
│  │ • Full CRDT │    │ • Selective │    │ • Minimal   │         │
│  │ • Persistent│    │   CRDTs     │    │   CRDTs     │         │
│  │ • Unlimited │    │ • Bounded   │    │ • Ephemeral │         │
│  │   history   │    │   storage   │    │ • No history│         │
│  │ • All proto │    │ • Core proto│    │ • Gossip    │         │
│  │             │    │             │    │   only      │         │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘         │
│         │                  │                  │                 │
│         └────────────┬─────┴──────────────────┘                 │
│                      │                                          │
│              Protocol Compatible                                │
│              (wire format, discovery, sync)                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**HIVE-Full**: Current implementation - servers, powerful edge devices
**HIVE-Edge**: Intermediate tier - Raspberry Pi, phones, tablets (future)
**HIVE-Lite**: This ADR - microcontrollers, embedded sensors

### 2. Ephemeral-First Design

HIVE-Lite nodes operate without persistent storage:

```
┌─────────────────────────────────────────────────────────────────┐
│                   Ephemeral Node Lifecycle                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Boot ──► Discover Peers ──► Join Mesh ──► Produce/Consume     │
│                                    │              │             │
│                                    │              ▼             │
│                                    │      Local State Only      │
│                                    │      (RAM, bounded)        │
│                                    │              │             │
│                                    │              ▼             │
│                                    │      Gossip to Peers       │
│                                    │      (they persist)        │
│                                    │              │             │
│                                    ▼              ▼             │
│                              Power Loss = State Loss            │
│                              (acceptable for sensors)           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Key insight**: Sensor data is inherently temporal. A temperature reading from 5 minutes ago that was never synced is likely stale anyway. Ephemeral operation is a feature, not a limitation.

### 3. Minimal CRDT Subset

Instead of full Automerge, HIVE-Lite implements only essential CRDTs:

| CRDT Type | Use Case | Memory | Complexity |
|-----------|----------|--------|------------|
| G-Counter | Event counts, heartbeats | O(n) nodes | Simple |
| PN-Counter | Bidirectional counts | O(n) nodes | Simple |
| LWW-Register | Latest sensor reading | O(1) | Simple |
| LWW-Map | Key-value sensor data | O(keys) | Moderate |
| OR-Set | Active alerts, tags | O(elements) | Moderate |

**Not included**: Full document CRDTs, text CRDTs, complex nested structures

```rust
// HIVE-Lite CRDT trait (no_std compatible)
#![no_std]

pub trait LiteCrdt: Sized {
    type Op;
    type Value;

    fn apply(&mut self, op: &Self::Op);
    fn merge(&mut self, other: &Self);
    fn value(&self) -> Self::Value;
    fn encode(&self, buf: &mut [u8]) -> usize;
    fn decode(buf: &[u8]) -> Option<Self>;
}

// Example: LWW-Register for sensor readings
pub struct LwwRegister<T, const MAX_SIZE: usize> {
    value: T,
    timestamp: u64,
    node_id: u32,
}
```

### 4. Lightweight Gossip Protocol

HIVE-Lite uses a simplified gossip protocol optimized for constrained networks:

```
┌─────────────────────────────────────────────────────────────────┐
│                  HIVE-Lite Gossip Protocol                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Message Types (4-bit type field):                              │
│                                                                 │
│  0x1 ANNOUNCE  - "I exist, here's my capabilities"              │
│  0x2 HEARTBEAT - "Still alive, clock sync"                      │
│  0x3 DATA      - "Here's my current state/delta"                │
│  0x4 QUERY     - "Send me state for key X"                      │
│  0x5 ACK       - "Received your data"                           │
│                                                                 │
│  Wire Format (compact binary, not JSON):                        │
│  ┌──────┬──────┬────────┬─────────┬──────────────┐             │
│  │ Type │ Flags│ NodeID │ SeqNum  │ Payload      │             │
│  │ 4bit │ 4bit │ 32bit  │ 32bit   │ Variable     │             │
│  └──────┴──────┴────────┴─────────┴──────────────┘             │
│                                                                 │
│  Minimum packet: 9 bytes + payload                              │
│  Maximum packet: 512 bytes (fits in single UDP datagram)        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Transport options:**
- UDP multicast/broadcast (primary for WiFi)
- BLE advertising + GATT (for BLE-only scenarios)
- ESP-NOW (ESP32-to-ESP32, very low latency)

### 5. Hierarchical Data Flow

This is where HIVE-Lite differs fundamentally from MQTT:

```
┌─────────────────────────────────────────────────────────────────┐
│              Hierarchical vs Broker Architecture                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MQTT (Centralized):              HIVE (Mesh + Hierarchy):      │
│                                                                 │
│       ┌─────────┐                      ┌─────────┐              │
│       │ Broker  │                      │ Squad   │              │
│       └────┬────┘                      │ Leader  │              │
│            │                           └────┬────┘              │
│     ┌──────┼──────┐                   Aggregated│Data           │
│     │      │      │                         │                   │
│     ▼      ▼      ▼               ┌─────────┼─────────┐         │
│   ┌───┐  ┌───┐  ┌───┐           ┌───┐    ┌───┐    ┌───┐        │
│   │ S │  │ S │  │ S │           │ S │◄──►│ S │◄──►│ S │        │
│   └───┘  └───┘  └───┘           └───┘    └───┘    └───┘        │
│   All data goes up              Sensors share locally,         │
│   No peer awareness             Only relevant data goes up     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Benefits of hierarchical integration:**

1. **Local aggregation** - Sensors can merge readings locally before upstream sync
2. **Peer awareness** - Sensor A can react to Sensor B's data without server round-trip
3. **Bandwidth efficiency** - Only meaningful changes propagate up the hierarchy
4. **Resilience** - Local mesh continues operating if upstream link fails
5. **Emergent capabilities** - More sensors = more local intelligence

### 6. First-Class Mesh Participation

HIVE-Lite nodes are **not** second-class citizens requiring a bridge. They participate directly in the mesh using the same protocol as Full nodes, with capability negotiation to handle feature differences.

```
┌─────────────────────────────────────────────────────────────────┐
│              First-Class vs Bridge Architecture                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Bridge Model (REJECTED):         First-Class Model (CHOSEN):   │
│                                                                 │
│  ┌──────────┐                     ┌──────────┐                  │
│  │HIVE-Full │                     │HIVE-Full │                  │
│  │          │                     │          │                  │
│  │ ┌──────┐ │                     └────┬─────┘                  │
│  │ │Bridge│ │                          │                        │
│  │ └──┬───┘ │                     Same Protocol                 │
│  └────┼─────┘                          │                        │
│       │                          ┌─────┴─────┐                  │
│  Translation                     │           │                  │
│       │                     ┌────┴──┐   ┌────┴──┐               │
│  ┌────┴────┐                │ Lite  │   │ Lite  │               │
│  │Lite Node│                │ Node  │◄─►│ Node  │               │
│  │(client) │                └───────┘   └───────┘               │
│  └─────────┘                Direct mesh participation           │
│  Dependent on Full          Peers with any node type            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**How this works:**

1. **Single wire protocol** - All node types speak the same binary gossip protocol
2. **Capability advertisement** - Nodes announce what they support (storage, relay, CRDTs)
3. **Graceful degradation** - Full nodes understand Lite limitations, don't request unsupported features
4. **Direct peering** - Lite nodes connect directly to any reachable node (Full, Edge, or Lite)
5. **No translation layer** - Data from Lite nodes is native HIVE data, not converted

**Capability Flags (announced during handshake):**

```rust
bitflags! {
    pub struct NodeCapabilities: u16 {
        const PERSISTENT_STORAGE = 0b0000_0001;  // Can store data across restarts
        const RELAY_CAPABLE      = 0b0000_0010;  // Can forward for multi-hop
        const DOCUMENT_CRDT      = 0b0000_0100;  // Supports full Automerge docs
        const PRIMITIVE_CRDT     = 0b0000_1000;  // Supports LWW, counters, sets
        const BLOB_STORAGE       = 0b0001_0000;  // Can store/serve blobs
        const HISTORY_QUERY      = 0b0010_0000;  // Can answer historical queries
        const AGGREGATION        = 0b0100_0000;  // Can aggregate upstream
    }
}

// HIVE-Lite typical capabilities:
const LITE_CAPS: NodeCapabilities = NodeCapabilities::PRIMITIVE_CRDT;

// HIVE-Full typical capabilities:
const FULL_CAPS: NodeCapabilities = NodeCapabilities::all();
```

**What Lite nodes CAN do:**
- Publish sensor data directly to the mesh (as CRDT updates)
- Subscribe to data from any peer (Full, Edge, or Lite)
- Participate in gossip protocol (receive and forward within session)
- Be discovered by and discover other nodes
- Contribute to local consensus/aggregation

**What Lite nodes CANNOT do:**
- Persist data across power cycles
- Act as reliable multi-hop relay (no guarantee of availability)
- Store or serve Automerge documents
- Answer historical queries

### 7. Implementation Strategy

**Phase 1: Unified Protocol Specification**
- Extend current HIVE wire protocol with capability negotiation
- Define compact binary encoding for primitive CRDTs
- Ensure protocol works identically for all node types
- Add feature flags for graceful capability discovery

**Phase 2: Reference Implementation (Rust, no_std)**
- Core CRDTs (G-Counter, LWW-Register, LWW-Map)
- Gossip protocol state machine (same as Full, subset of features)
- ESP32 HAL integration (using `esp-hal` or `esp-idf-hal`)
- UDP transport with multicast discovery

**Phase 3: HIVE-Full Compatibility**
- Update HIVE-Full to handle capability negotiation
- Ensure Full nodes work seamlessly with Lite peers
- Add primitive CRDT support to Full nodes (for interop)

**Phase 4: Extended Platforms**
- BLE transport
- LoRa transport (via LoRa-E5, RFM95W)
- Other MCU targets (STM32, nRF52)

### 8. Memory Budget

Target: 256KB RAM allocation for HIVE-Lite runtime

| Component | Budget | Notes |
|-----------|--------|-------|
| Network stack | 64KB | lwIP or smoltcp |
| CRDT state | 64KB | ~100 LWW registers or equivalent |
| Gossip buffers | 32KB | 64 x 512-byte packets |
| Protocol state | 16KB | Peer table, routing |
| Application | 80KB | Sensor logic, display |
| **Total** | **256KB** | Fits in PSRAM with margin |

### 9. Example: Environmental Sensor Mesh

```
┌─────────────────────────────────────────────────────────────────┐
│           Example: Building Environmental Monitoring            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Floor 3:  [Temp-301]◄──►[Temp-302]◄──►[Temp-303]              │
│                 │              │              │                 │
│                 └──────────────┼──────────────┘                 │
│                                │ (aggregated)                   │
│                                ▼                                │
│  Floor 2:  [Temp-201]◄──►[Hub-200]◄──►[Temp-202]               │
│                 │         (Pi/Edge)        │                    │
│                 │              │           │                    │
│                 └──────────────┼───────────┘                    │
│                                │                                │
│  Floor 1:  [Temp-101]◄──►[Gateway]◄──►[Temp-102]               │
│                         (HIVE-Full)                             │
│                              │                                  │
│                              ▼                                  │
│                    [Cloud/HQ Systems]                           │
│                                                                 │
│  Each sensor:                                                   │
│  - Publishes own readings (LWW-Register)                        │
│  - Receives peer readings (local awareness)                     │
│  - Can trigger local alerts (no server needed)                  │
│  - Hub aggregates floor data before upstream sync               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Emergent capability example**: Temp-301 detects rapid temperature rise. It sees Temp-302 and Temp-303 also rising. Local consensus: potential fire. Alert triggered immediately without waiting for server confirmation.

### 10. Primitive-to-Document Integration

A key capability is how primitive CRDTs from Lite nodes feed into Automerge documents on Full/Edge nodes. This enables rich, queryable data structures while keeping Lite nodes simple.

**Integration Pattern:**

```
┌─────────────────────────────────────────────────────────────────┐
│           Primitive CRDT → Automerge Document Flow              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  HIVE-Lite Node                    HIVE-Full/Edge Node          │
│  ┌─────────────────┐               ┌─────────────────────────┐  │
│  │ LWW-Register:   │   gossip      │ Automerge Document:     │  │
│  │ temp = 23.5°C   │ ──────────►   │ {                       │  │
│  │ ts = 1702234567 │               │   "sensors": {          │  │
│  │ node = 0xA3B2   │               │     "A3B2": {           │  │
│  └─────────────────┘               │       "temp": 23.5,     │  │
│                                    │       "updated": "...", │  │
│  ┌─────────────────┐               │       "history": [...]  │  │
│  │ G-Counter:      │   gossip      │     },                  │  │
│  │ motion = 47     │ ──────────►   │     "C4D5": {...}       │  │
│  │                 │               │   },                    │  │
│  └─────────────────┘               │   "alerts": [...],      │  │
│                                    │   "summary": {...}      │  │
│  ┌─────────────────┐               │ }                       │  │
│  │ OR-Set:         │   gossip      │                         │  │
│  │ alerts = {...}  │ ──────────►   │ (Full history,          │  │
│  └─────────────────┘               │  queryable, persistent) │  │
│                                    └─────────────────────────┘  │
│                                                                 │
│  Lite: Produces primitives         Full: Aggregates into docs   │
│  (stateless, ephemeral)            (stateful, persistent)       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**How it works:**

1. Lite node publishes primitive CRDT updates (e.g., LWW-Register with temperature)
2. Full/Edge node receives update via standard gossip protocol
3. Full node's **aggregation layer** maps primitive to document location
4. Document is updated using Automerge, preserving history
5. Document syncs to other Full nodes via normal Automerge sync

**The Lite node doesn't know or care about documents** - it just publishes primitives. The Full node decides how to incorporate that data.

### 11. M5Stack Core2 as Reference Platform

The M5Stack Core2 provides an excellent reference platform with meaningful onboard sensors:

| Sensor | Chip | Data Type | CRDT Mapping | Use Case |
|--------|------|-----------|--------------|----------|
| 6-axis IMU | MPU6886 | Accel XYZ, Gyro XYZ, Temp | LWW-Register | Motion, orientation, fall detection |
| Microphone | SPM1423 | Audio level (dB) | LWW-Register | Noise monitoring, activity detection |
| Touch | FT6336U | Touch events | G-Counter | User interaction counts |
| RTC | BM8563 | Timestamp | (clock sync) | Accurate event timing |
| Battery | AXP192 | Voltage, %, charging | LWW-Register | Device health monitoring |
| Vibration | Motor | (output) | - | Alert/feedback |
| Display | IPS LCD | (output) | - | Local status display |

**Example: Tactical Personnel Monitor**

An M5Stack Core2 worn by a team member could provide:

```rust
// Data published by a single HIVE-Lite node (M5Stack Core2)
struct PersonnelSensorData {
    // Motion/Posture (from IMU)
    orientation: LwwRegister<Orientation>,  // Standing, prone, moving
    activity_level: LwwRegister<u8>,        // 0-100 activity intensity
    fall_detected: LwwRegister<bool>,       // Sudden acceleration event

    // Environment (from mic)
    ambient_noise_db: LwwRegister<u8>,      // Sound level
    gunshot_detected: GCounter,             // Acoustic event count

    // Device Health (from power management)
    battery_percent: LwwRegister<u8>,
    charging: LwwRegister<bool>,

    // User Input (from touch)
    panic_button_presses: GCounter,         // Emergency count
    status_acks: GCounter,                  // "I'm OK" confirmations

    // Timestamp (from RTC)
    last_update: LwwRegister<u64>,
}
```

**Aggregated Document on Full Node:**

```json
{
  "squad": "alpha",
  "personnel": {
    "operator_1": {
      "node_id": "A3B2C4D5",
      "callsign": "Alpha-1",
      "current": {
        "orientation": "moving",
        "activity_level": 72,
        "battery": 84,
        "last_seen": "2024-12-10T14:32:15Z"
      },
      "alerts": [],
      "history": {
        "positions": [...],
        "activity_timeline": [...]
      }
    },
    "operator_2": {...},
    "operator_3": {...}
  },
  "squad_summary": {
    "active_count": 3,
    "avg_battery": 78,
    "alerts_active": 0,
    "last_comms_check": "2024-12-10T14:32:00Z"
  }
}
```

**What the Full node can derive that Lite nodes cannot:**

1. **Cross-operator correlation** - "3 operators reporting gunshots within 10 seconds"
2. **Historical trends** - "Battery drain rate suggests 4 hours remaining"
3. **Anomaly detection** - "Operator-2 hasn't moved in 15 minutes, others active"
4. **Squad summaries** - Aggregated health/status for upstream reporting
5. **Time-series analysis** - Activity patterns, location history

**Emergent Capability Example:**

Three operators' Lite nodes independently detect:
- Operator-1: High activity, gunshot counter +3
- Operator-2: Prone orientation, low activity
- Operator-3: High activity, moving toward Operator-2

A Full node correlates: "Possible casualty event - Operator-2 down, Operator-3 rendering aid, Operator-1 providing cover." This assessment is impossible from any single Lite node's perspective but emerges from mesh-wide awareness.

## Consequences

### Positive

1. **MQTT Alternative** - Decentralized sensor networks without broker dependency
2. **Local Intelligence** - Sensors benefit from peer data, enabling edge decisions
3. **Bandwidth Efficiency** - Hierarchical aggregation reduces upstream traffic
4. **Resilience** - Local mesh operates independently of upstream connectivity
5. **Low Cost** - $15-30 sensor nodes can participate in HIVE mesh
6. **Incremental Adoption** - Can add Lite nodes to existing HIVE deployments

### Negative

1. **Protocol Must Support All Tiers** - Single protocol must work for 256KB MCU and 100MB server
2. **Feature Gap** - Lite nodes can't participate in full document collaboration
3. **Testing Surface** - New platform targets increase testing requirements
4. **no_std Constraints** - Rust ecosystem support is more limited

### Neutral

1. **Separate Codebase** - HIVE-Lite likely needs its own repo/crate (shared protocol definitions)
2. **Different Skillset** - Embedded development differs from server development
3. **Hardware Dependency** - Testing requires physical devices or emulators
4. **Capability Negotiation** - All nodes must implement capability discovery

## Alternatives Considered

### Alternative 1: Full Automerge on ESP32

Attempt to run full Automerge on ESP32 with PSRAM.

**Rejected because:**
- Automerge's memory model assumes garbage collection
- Document history accumulation would exhaust memory
- `std` library requirement for Automerge

### Alternative 2: MQTT Bridge Only

Keep sensors on MQTT, bridge at HIVE-Full nodes.

**Rejected because:**
- Loses peer-to-peer benefits
- Sensors remain "dumb" producers
- Still requires broker infrastructure

### Alternative 3: Custom Protocol (Non-CRDT)

Simple pub/sub without CRDT guarantees.

**Rejected because:**
- Loses consistency guarantees
- Can't meaningfully merge conflicting data
- Defeats purpose of HIVE integration

## References

- [M5Stack Core2 Specifications](https://docs.m5stack.com/en/core/core2)
- [M5Stack Core2 IMU Documentation](https://docs.m5stack.com/en/arduino/m5core2/imu)
- [M5Stack Core2 v1.1](https://docs.m5stack.com/en/core/Core2%20v1.1)
- [ESP32 Technical Reference](https://www.espressif.com/en/products/socs/esp32)
- [CRDTs for Constrained Devices](https://arxiv.org/abs/1603.01529)
- [Rust Embedded Book](https://docs.rust-embedded.org/book/)
- [esp-hal - Rust HAL for ESP32](https://github.com/esp-rs/esp-hal)
- ADR-017: Hierarchical Mesh Architecture
- ADR-032: Pluggable Transport Abstraction

## Appendix A: Sensor Data Schema

```rust
/// Standard sensor reading format for HIVE-Lite
#[derive(Clone)]
pub struct SensorReading {
    /// Sensor type identifier
    pub sensor_type: u8,  // 0=temp, 1=humidity, 2=pressure, 3=motion, etc.
    /// Reading value (scaled integer to avoid float)
    pub value: i32,       // e.g., temperature in centidegrees (2350 = 23.50°C)
    /// Reading timestamp (seconds since node boot or GPS time)
    pub timestamp: u32,
    /// Quality/confidence indicator
    pub quality: u8,      // 0-100
}

/// Standard alert format
pub struct SensorAlert {
    pub alert_type: u8,
    pub severity: u8,     // 0=info, 1=warning, 2=critical
    pub source_node: u32,
    pub timestamp: u32,
    pub data: [u8; 16],   // Alert-specific payload
}
```

## Appendix B: Protocol Compatibility Matrix

All node types use the **same wire protocol**. Differences are in capabilities, not protocol dialect.

| Feature | HIVE-Full | HIVE-Edge | HIVE-Lite |
|---------|-----------|-----------|-----------|
| **Mesh Participation** | ✓ | ✓ | ✓ |
| **Direct Peering** | ✓ | ✓ | ✓ |
| **Gossip Protocol** | ✓ | ✓ | ✓ |
| **Discovery** | ✓ | ✓ | ✓ |
| Document CRDTs | ✓ | ✓ | ✗ |
| Primitive CRDTs | ✓ | ✓ | ✓ |
| Persistent Storage | ✓ | ✓ | ✗ |
| History/Time Travel | ✓ | Limited | ✗ |
| Blob Storage | ✓ | ✓ | ✗ |
| Multi-hop Relay | ✓ | ✓ | Session only |
| Aggregation | ✓ | ✓ | Local only |
| RAM Required | >100MB | >10MB | <1MB |

**Key point**: A Lite node can peer directly with a Full node, another Lite node, or an Edge node. No bridges, no translation, no second-class citizenship.
