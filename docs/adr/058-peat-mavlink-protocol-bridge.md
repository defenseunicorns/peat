# ADR-058: Peat-MAVLink Protocol Bridge Crate

**Status**: Proposed
**Date**: 2026-03-18
**Authors**: Kit Plummer
**Organization**: Defense Unicorns (https://defenseunicorns.com)
**Relates To**: ADR-029 (TAK Transport Adapter), ADR-032 (Pluggable Transport Abstraction), ADR-041 (Multi-Transport Embedded Integration), ADR-052 (Peat-LoRa Transport)

---

## Executive Summary

This ADR defines the architecture for `peat-mavlink`, a Rust crate providing bidirectional MAVLink protocol integration for the Peat ecosystem. Unlike transport crates (peat-btle, peat-lora, peat-sbd) that carry opaque Peat sync bytes over a physical link, `peat-mavlink` is a **protocol bridge** — it translates between MAVLink's semantic message vocabulary (telemetry, commands, missions) and Peat CRDT documents. The primary API surface is peat-rmw topic integration, mapping MAVLink messages to typed pub/sub topics following the pattern established by mavros in ROS 2. A standalone bridge mode using direct Automerge store access is also supported for deployments without peat-rmw.

---

## Context

### Problem Statement

Unmanned Aerial Systems (UAS) are increasingly central to tactical edge operations. MAVLink v2 is the dominant telemetry and command protocol for UAS, supported by ArduPilot, PX4, QGroundControl, and hundreds of companion payloads. Peat needs first-class UAS integration to:

1. **Sync vehicle telemetry across the mesh** — position, attitude, battery, GPS status, mission progress should be available to any Peat node, not just the GCS directly connected to the autopilot
2. **Enable distributed C2** — multiple operators on the mesh can issue commands to vehicles; CRDT-backed mission state prevents split-brain conflicts
3. **Bridge UAS data to TAK/CoT** — via existing peat-tak-bridge, MAVLink position data flows through to ATAK as friendly tracks
4. **Support companion computer architectures** — Jetson/Pi companion computers on UAS run Peat nodes; MAVLink is the local link to the autopilot

### Why Not a Transport Adapter?

The ADR-032 transport abstraction (`MeshTransport` trait) carries opaque Peat sync bytes — the transport doesn't understand the payload. MAVLink is fundamentally different:

| Aspect | Transport (BLE, LoRa) | Protocol Bridge (TAK, MAVLink) |
|--------|----------------------|-------------------------------|
| **Payload** | Opaque Peat sync bytes | Semantically typed messages |
| **Direction** | Peat ↔ Peat | External protocol ↔ Peat CRDTs |
| **Peer model** | Peat nodes discover Peat nodes | External devices (autopilots, GCS) |
| **Message format** | Peat frame marker (`0xEC`) | Protocol-native (MAVLink `0xFD`, CoT XML) |
| **Integration point** | TransportManager | peat-rmw topics / Automerge documents |

MAVLink messages have semantic meaning — `GLOBAL_POSITION_INT` is a position report, `COMMAND_LONG` is a vehicle command. Tunneling Peat sync bytes through MAVLink's `TUNNEL` or `ENCAPSULATED_DATA` messages would be fragile, nonstandard, and miss the point. The value is in **translating** MAVLink's domain model into Peat's CRDT-backed mesh.

This follows the same pattern as peat-tak-bridge (ADR-029), which translates CoT events ↔ Peat CRDTs rather than carrying Peat data over TAK's transport.

### Relationship to ADR-052 (Peat-LoRa)

ADR-052 already addresses the physical link layer where MAVLink and Peat coexist:

- **mLRS dual serial channels**: Companion computers can multiplex Peat sync frames + MAVLink telemetry over the same LoRa link
- **Frame marker disambiguation**: `0xEC` (Peat) vs `0xFD` (MAVLink v2)
- **peat-lora** handles the transport; **peat-mavlink** handles the protocol semantics

On a companion computer with mLRS, the data flow is:

```
Autopilot ──MAVLink──→ peat-mavlink (parse) ──peat-rmw topics──→ peat-mesh (CRDT sync)
                                                                       │
                                             peat-lora (transport) ←───┘
                                                  │
                                             mLRS serial ──LoRa──→ Ground station mesh
```

### MAVLink Protocol Overview

MAVLink v2 is a lightweight binary protocol designed for UAS communication:

| Property | Value |
|----------|-------|
| **Frame size** | 11-280 bytes (header + payload + checksum + signature) |
| **Max payload** | 255 bytes |
| **Addressing** | System ID (1-255) + Component ID (1-255) |
| **Dialects** | common.xml (base), ardupilot.xml, development.xml |
| **Transport** | Serial (UART/USB), UDP, TCP |
| **CRC** | Per-message CRC16 + message-type seed |
| **Signing** | Optional SHA-256 link signing |

Key message categories relevant to Peat:

| Category | Messages | Peat Use |
|----------|----------|----------|
| **Heartbeat** | HEARTBEAT | Vehicle discovery, type identification |
| **Position** | GLOBAL_POSITION_INT, LOCAL_POSITION_NED | PLI, track generation |
| **Attitude** | ATTITUDE, ATTITUDE_QUATERNION | Vehicle orientation |
| **Battery** | BATTERY_STATUS, SYS_STATUS | Resource monitoring |
| **Mission** | MISSION_ITEM_INT, MISSION_CURRENT, MISSION_COUNT | Mission state sync |
| **Commands** | COMMAND_LONG, COMMAND_INT, COMMAND_ACK | Remote vehicle control |
| **Status** | STATUSTEXT, NAMED_VALUE_FLOAT/INT | Diagnostics, custom telemetry |
| **GPS** | GPS_RAW_INT, GPS_STATUS | Navigation quality |

---

## Decision Drivers

### Requirements

1. **Bidirectional**: Ingest MAVLink telemetry into Peat CRDTs; translate Peat commands into MAVLink messages
2. **Multi-Vehicle**: Support multiple simultaneous vehicles via MAVLink system IDs
3. **peat-rmw Integration**: Map MAVLink messages to peat-rmw pub/sub topics as the primary API
4. **Standalone Mode**: Support direct Automerge store access without peat-rmw dependency
5. **Standard Dialects**: Support common.xml and ardupilot.xml dialects via feature flags
6. **Connection Types**: Serial (UART/USB), UDP, TCP connections to autopilots and GCS
7. **DIL Resilience**: Queue outbound commands when vehicle link is degraded; replay on reconnect
8. **Mesh Distribution**: Any Peat node on the mesh can observe vehicle state and (with authorization) issue commands

### Constraints

1. **MAVLink v2 Only**: v1 is deprecated; no need to support it
2. **No Custom MAVLink Messages**: Use standard dialect messages only; avoid vendor lock-in
3. **Not a Transport**: Does not implement `MeshTransport` trait — bridge pattern only
4. **Authorization**: Command issuance requires cell-level authorization (future ADR for C2 authority delegation)
5. **Bandwidth Awareness**: Telemetry rates must be configurable to avoid flooding constrained mesh links

---

## Architecture

### Crate Structure

```
peat-mavlink/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, re-exports, feature gates
│   ├── bridge.rs           # Core bridge: MAVLink ↔ CRDT translation
│   ├── config.rs           # Bridge configuration (connections, rates, filtering)
│   ├── connection.rs       # MAVLink connection management (serial/UDP/TCP)
│   ├── vehicle.rs          # Vehicle state model (CRDT-backed)
│   ├── topics.rs           # peat-rmw topic definitions and mappings
│   ├── commands.rs         # Peat → MAVLink command translation
│   ├── mission.rs          # Mission state synchronization
│   └── error.rs            # Error types
├── examples/
│   ├── bridge.rs           # Standalone bridge (serial → mesh)
│   ├── rmw_bridge.rs       # peat-rmw integrated bridge
│   ├── multi_vehicle.rs    # Multiple vehicle monitoring
│   └── companion.rs        # Companion computer deployment
└── tests/
    ├── bridge_tests.rs
    ├── vehicle_tests.rs
    └── integration_tests.rs
```

### Core Types

```rust
/// Bridge configuration
#[derive(Debug, Clone)]
pub struct MavlinkBridgeConfig {
    /// MAVLink connections to autopilots / GCS
    pub connections: Vec<MavlinkConnectionConfig>,
    /// This bridge's MAVLink system ID (for outbound messages)
    pub system_id: u8,
    /// This bridge's MAVLink component ID
    pub component_id: u8,
    /// Telemetry rate limiting (messages per second per vehicle)
    pub telemetry_rate_limit: Option<f64>,
    /// Which message types to bridge (None = all supported)
    pub message_filter: Option<Vec<MavlinkMessageType>>,
    /// Topic namespace prefix (default: "/mavlink")
    pub topic_prefix: String,
}

/// MAVLink connection endpoint
#[derive(Debug, Clone)]
pub enum MavlinkConnectionConfig {
    /// Serial port (UART/USB) — typical for companion computers
    Serial {
        port: String,
        baud_rate: u32,
    },
    /// UDP — typical for SITL, network autopilots
    Udp {
        bind_addr: SocketAddr,
        remote_addr: Option<SocketAddr>,
    },
    /// TCP client — typical for GCS connections
    Tcp {
        addr: SocketAddr,
    },
}

/// Tracked vehicle state, backed by CRDTs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleState {
    /// MAVLink system ID
    pub system_id: u8,
    /// Vehicle type (from HEARTBEAT)
    pub vehicle_type: VehicleType,
    /// Autopilot type (from HEARTBEAT)
    pub autopilot: AutopilotType,
    /// Current flight mode
    pub mode: FlightMode,
    /// Armed state
    pub armed: bool,
    /// Last known position (lat, lon, alt_msl_mm, relative_alt_mm)
    pub position: Option<GlobalPosition>,
    /// Attitude (roll, pitch, yaw in radians)
    pub attitude: Option<Attitude>,
    /// Battery state
    pub battery: Option<BatteryState>,
    /// GPS fix quality
    pub gps: Option<GpsState>,
    /// Current mission item index
    pub mission_current: Option<u16>,
    /// Total mission items
    pub mission_count: Option<u16>,
    /// Last heartbeat timestamp (monotonic ms)
    pub last_heartbeat_ms: u64,
    /// Last status text
    pub last_status: Option<String>,
}

/// Bridge error types
#[derive(Debug, thiserror::Error)]
pub enum MavlinkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Serial port error: {0}")]
    SerialError(String),
    #[error("Message parse error: {0}")]
    ParseError(String),
    #[error("Vehicle {0} not found")]
    VehicleNotFound(u8),
    #[error("Command rejected: {0}")]
    CommandRejected(String),
    #[error("Timeout waiting for ACK")]
    AckTimeout,
}
```

### peat-rmw Topic Mapping

MAVLink messages map to peat-rmw topics using a consistent namespace scheme. Each vehicle gets a topic subtree keyed by system ID:

```
/mavlink/{sys_id}/heartbeat        ← HEARTBEAT
/mavlink/{sys_id}/position         ← GLOBAL_POSITION_INT
/mavlink/{sys_id}/attitude         ← ATTITUDE
/mavlink/{sys_id}/battery          ← BATTERY_STATUS
/mavlink/{sys_id}/gps              ← GPS_RAW_INT
/mavlink/{sys_id}/status           ← STATUSTEXT
/mavlink/{sys_id}/mission/current  ← MISSION_CURRENT
/mavlink/{sys_id}/state            ← Aggregated VehicleState (all fields)

/mavlink/{sys_id}/cmd/arm          → COMMAND_LONG (MAV_CMD_COMPONENT_ARM_DISARM)
/mavlink/{sys_id}/cmd/mode         → SET_MODE / COMMAND_LONG
/mavlink/{sys_id}/cmd/takeoff      → COMMAND_LONG (MAV_CMD_NAV_TAKEOFF)
/mavlink/{sys_id}/cmd/land         → COMMAND_LONG (MAV_CMD_NAV_LAND)
/mavlink/{sys_id}/cmd/rtl          → COMMAND_LONG (MAV_CMD_NAV_RETURN_TO_LAUNCH)
/mavlink/{sys_id}/cmd/goto         → COMMAND_INT (MAV_CMD_DO_REPOSITION)
/mavlink/{sys_id}/cmd/mission      → MISSION_ITEM_INT sequence
```

The `/mavlink/{sys_id}/state` topic publishes the aggregated `VehicleState` struct, which is the most common consumption pattern — a single subscription gives you everything about a vehicle.

Individual field topics (`/position`, `/attitude`, etc.) are available for high-frequency consumers that only need specific data.

### Bridge Architecture

```rust
/// Core MAVLink bridge
///
/// Manages MAVLink connections and translates between MAVLink messages
/// and peat-rmw topics (or direct Automerge store access).
pub struct MavlinkBridge {
    config: MavlinkBridgeConfig,
    /// Active MAVLink connections
    connections: Vec<MavlinkConnection>,
    /// Per-vehicle state tracking
    vehicles: Arc<RwLock<HashMap<u8, VehicleState>>>,
    /// Outbound command queue (DIL resilient)
    command_queue: CommandQueue,
}

impl MavlinkBridge {
    /// Create bridge with peat-rmw node integration
    pub async fn with_rmw_node(
        config: MavlinkBridgeConfig,
        node: &mut peat_rmw::Node,
    ) -> Result<Self, MavlinkError>;

    /// Create standalone bridge with direct store access
    pub async fn with_store(
        config: MavlinkBridgeConfig,
        store: Arc<AutomergeStore>,
    ) -> Result<Self, MavlinkError>;

    /// Start the bridge (spawns connection + translation tasks)
    pub async fn start(&mut self) -> Result<(), MavlinkError>;

    /// Stop the bridge gracefully
    pub async fn stop(&mut self) -> Result<(), MavlinkError>;

    /// Get current state for a vehicle
    pub fn vehicle_state(&self, system_id: u8) -> Option<VehicleState>;

    /// List all discovered vehicles
    pub fn vehicles(&self) -> Vec<VehicleState>;

    /// Send a command to a vehicle (queued if disconnected)
    pub async fn send_command(
        &self,
        system_id: u8,
        command: VehicleCommand,
    ) -> Result<CommandAck, MavlinkError>;
}
```

### Message Flow

**Inbound (MAVLink → Peat)**:

```
Serial/UDP/TCP
    │
    ▼
MavlinkConnection::recv()          ← Raw MAVLink v2 frame
    │
    ▼
mavlink::Message::parse()          ← mavlink-rs dialect parsing
    │
    ▼
Bridge::handle_message()           ← Route by message ID
    │
    ├── HEARTBEAT → update VehicleState, publish /heartbeat + /state
    ├── GLOBAL_POSITION_INT → update position, publish /position + /state
    ├── ATTITUDE → update attitude, publish /attitude + /state
    ├── BATTERY_STATUS → update battery, publish /battery + /state
    ├── GPS_RAW_INT → update gps, publish /gps + /state
    ├── STATUSTEXT → update status, publish /status + /state
    ├── MISSION_CURRENT → update mission, publish /mission/current + /state
    └── COMMAND_ACK → resolve pending command future
```

**Outbound (Peat → MAVLink)**:

```
peat-rmw subscription on /mavlink/{sys_id}/cmd/*
    │
    ▼
Bridge::handle_command()           ← Parse VehicleCommand from topic
    │
    ▼
CommandQueue::enqueue()            ← DIL-resilient queuing
    │
    ▼
MavlinkConnection::send()         ← MAVLink v2 frame
    │
    ▼
Wait for COMMAND_ACK               ← Timeout + retry logic
```

### Multi-Vehicle Support

Each vehicle (identified by MAVLink system ID) gets:
- Independent `VehicleState` tracking
- Separate topic subtree (`/mavlink/1/...`, `/mavlink/2/...`)
- Independent CRDT documents in the Automerge store
- Heartbeat timeout monitoring (configurable, default 5s)

Vehicle discovery is implicit — the first HEARTBEAT from a new system ID creates the vehicle entry and topic publishers.

### DIL Resilience

Following the pattern from ADR-029 (TAK Transport Adapter):

- **Outbound command queue**: Commands are queued when the MAVLink link is down; replayed in order on reconnect
- **Stale state detection**: `VehicleState.last_heartbeat_ms` allows consumers to detect stale data
- **Configurable timeouts**: Heartbeat loss threshold is configurable per deployment (tight for SITL, loose for LoRa links)
- **Telemetry rate limiting**: Prevents flooding constrained mesh links with high-rate autopilot telemetry

### Standalone vs peat-rmw Mode

The crate supports two integration modes:

**peat-rmw mode** (recommended):
```rust
let mut node = NodeBuilder::new("mavlink_bridge", "formation-secret")
    .build().await?;

let bridge = MavlinkBridge::with_rmw_node(config, &mut node).await?;
bridge.start().await?;

// Other peat-rmw nodes on the mesh see /mavlink/* topics automatically
```

**Standalone mode** (for deployments without peat-rmw):
```rust
let store = Arc::new(AutomergeStore::new());
let bridge = MavlinkBridge::with_store(config, store.clone()).await?;
bridge.start().await?;

// Access vehicle state via store.collection("mavlink.vehicle.1")
```

The peat-rmw dependency is feature-gated (`feature = "rmw"`), so standalone deployments don't pull in peat-rmw.

---

## Dependencies

| Crate | Version | Role |
|-------|---------|------|
| `mavlink` | 0.14 | MAVLink v2 message parsing, dialect support |
| `peat-mesh` | 0.5 | Automerge store, CRDT sync (standalone mode) |
| `peat-rmw` | 0.1 (optional) | Topic pub/sub integration (feature = "rmw") |
| `tokio` | 1 | Async runtime, timers, channels |
| `tokio-serial` | 5 | Async serial port (feature = "serial") |
| `serde` / `serde_json` | 1 | Message serialization |
| `tracing` | 0.1 | Structured logging |
| `thiserror` | 2 | Error types |

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `serial` | yes | Serial port connections (tokio-serial) |
| `udp` | yes | UDP connections |
| `tcp` | yes | TCP connections |
| `rmw` | no | peat-rmw topic integration |
| `ardupilot` | no | ArduPilot dialect extensions |

---

## External Crate Pattern

Following the pattern established by peat-btle, peat-lora, and peat-sbd:

```
peat (main repo)
├── peat-protocol/    ← Core protocol, CRDT backends
├── peat-transport/   ← TAK bridge lives here
└── ...

peat-btle (external)  ← BLE transport
peat-lora (external)  ← LoRa transport
peat-sbd (external)   ← SBD satellite transport
peat-rmw (external)   ← ROS 2 middleware
peat-mavlink (external) ← MAVLink protocol bridge [NEW]
```

---

## Alternatives Considered

### A. MAVLink as MeshTransport Implementation

Implement `MeshTransport` trait, tunnel Peat sync bytes through MAVLink `TUNNEL` or `ENCAPSULATED_DATA` messages.

**Rejected because:**
- MAVLink's MTU (255 bytes) makes fragmentation painful
- `TUNNEL` / `ENCAPSULATED_DATA` are poorly supported by autopilots and GCS
- Misses the primary value: ingesting vehicle telemetry into Peat CRDTs
- The serial link itself is better served by peat-lora (mLRS) which is already designed for opaque byte transport

### B. Integrate Directly into peat-tak-bridge

Add MAVLink parsing to peat-tak-bridge, convert MAVLink → CoT → Peat.

**Rejected because:**
- Lossy double-translation (MAVLink → CoT loses MAVLink-specific fields)
- Not all MAVLink data maps to CoT (battery, GPS quality, mission state)
- peat-tak-bridge is a TAK-specific service; MAVLink integration is independently useful
- Violates single-responsibility — bridge should own one protocol boundary

### C. mavros-style ROS 2 Node Only

Build a pure peat-rmw node with no standalone capability.

**Rejected because:**
- Some deployments (embedded companion computers) may not want the full peat-rmw stack
- Feature-gated `rmw` support gives both options without code duplication

---

## Consequences

### Positive

- **Mesh-wide UAS visibility**: Any Peat node can observe vehicle state without direct MAVLink connection
- **Distributed C2**: CRDT-backed command state enables multi-operator scenarios
- **TAK interop**: MAVLink position data flows to ATAK via existing peat-tak-bridge
- **Familiar API**: peat-rmw topic mapping follows mavros conventions that ROS 2 developers know
- **Composable**: Bridge runs alongside peat-lora on companion computers; each handles its own concern

### Negative

- **New crate to maintain**: Another external crate in the ecosystem
- **MAVLink dialect churn**: ArduPilot adds custom messages frequently; dialect support needs periodic updates
- **Latency**: MAVLink → CRDT → mesh sync adds latency vs direct MAVLink forwarding (acceptable for tactical use)

### Risks

- **Command authority**: Without C2 delegation controls, any mesh node could command any vehicle. Mitigated by requiring authorization (future ADR) before exposing command topics.
- **Telemetry flooding**: High-rate autopilot streams (50Hz attitude) could overwhelm constrained links. Mitigated by configurable rate limiting in bridge config.

---

## Future Work

1. **C2 Authority Delegation ADR**: Define which cells/nodes are authorized to issue commands to which vehicles
2. **Mission Planning CRDT**: Collaborative mission editing across the mesh with conflict resolution
3. **MAVLink Signing**: Bridge-level MAVLink v2 link signing for authenticated connections
4. **Video Streaming**: MAVLink camera control + GStreamer pipeline integration (ties to peat-inference)
5. **Swarm Coordination**: Multi-vehicle coordination primitives built on peat-rmw service calls
6. **ArduPilot Dialect**: Extended support for ArduPilot-specific messages (RANGEFINDER, TERRAIN_REPORT, etc.)

---

## References

- [MAVLink v2 Protocol](https://mavlink.io/en/guide/serialization.html)
- [MAVLink Message Definitions](https://mavlink.io/en/messages/common.html)
- [mavlink-rs crate](https://crates.io/crates/mavlink)
- [mavros (ROS 2 MAVLink bridge)](https://github.com/mavlink/mavros)
- [mLRS (MAVLink LoRa System)](https://github.com/olliw42/mLRS)
- [ArduPilot Companion Computer Docs](https://ardupilot.org/dev/docs/companion-computers.html)
