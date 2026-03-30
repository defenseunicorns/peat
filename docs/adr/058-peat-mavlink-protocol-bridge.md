# ADR-058: Peat-MAVLink Protocol Bridge Crate

**Status**: Proposed (Amended 2026-03-30)
**Date**: 2026-03-18
**Authors**: Kit Plummer
**Organization**: Defense Unicorns (https://defenseunicorns.com)
**Relates To**: ADR-029 (TAK Transport Adapter), ADR-032 (Pluggable Transport Abstraction), ADR-041 (Multi-Transport Embedded Integration), ADR-052 (Peat-LoRa Transport)

---

## Executive Summary

This ADR defines the architecture for `peat-mavlink`, a Rust **library crate** providing bidirectional MAVLink protocol integration for the Peat ecosystem. Unlike transport crates (peat-btle, peat-lora, peat-sbd) that carry opaque Peat sync bytes over a physical link, `peat-mavlink` is a **protocol bridge** — it translates between MAVLink's semantic message vocabulary (telemetry, commands, missions) and Peat document types defined in `peat-schema`. The crate has **no dependency on `peat-mesh`** — it is composed by the integrator's mission application alongside `peat-mesh` (and optionally `peat-rmw`), following the same external crate pattern as peat-btle et al. but with the integration point at the document layer rather than the transport layer.

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
3. **peat-rmw Integration**: Optionally map MAVLink messages to peat-rmw pub/sub topics (feature-gated)
4. **No peat-mesh dependency**: Produce `peat-schema` document types; the integrator's mission application wires them into `peat-mesh`
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
│   ├── mission_app.rs      # Mission app wiring peat-mavlink + peat-mesh
│   ├── serial_dump.rs      # Parse and print MAVLink from serial (no mesh)
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
/// Manages MAVLink connections, parses MAVLink messages into Peat document
/// types, and emits them via a channel. The integrator's mission application
/// receives VehicleState updates and writes them into peat-mesh.
/// peat-mavlink has NO dependency on peat-mesh — only on peat-schema.
pub struct MavlinkBridge {
    config: MavlinkBridgeConfig,
    /// Active MAVLink connections
    connections: Vec<MavlinkConnection>,
    /// Per-vehicle state tracking
    vehicles: Arc<RwLock<HashMap<u8, VehicleState>>>,
    /// Channel for emitting vehicle state updates to the integrator
    state_tx: tokio::sync::broadcast::Sender<VehicleStateUpdate>,
    /// Outbound command queue (DIL resilient)
    command_queue: CommandQueue,
}

/// Update emitted to the integrator's mission application
pub struct VehicleStateUpdate {
    pub system_id: u8,
    pub state: VehicleState,
    /// Which field(s) changed in this update
    pub changed: VehicleStateField,
}

impl MavlinkBridge {
    /// Create a new bridge. Returns the bridge and a receiver for state updates.
    /// The integrator consumes the receiver and writes updates into peat-mesh.
    pub async fn new(
        config: MavlinkBridgeConfig,
    ) -> Result<(Self, tokio::sync::broadcast::Receiver<VehicleStateUpdate>), MavlinkError>;

    /// Start the bridge (spawns connection + parsing tasks)
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
    ├── HEARTBEAT → update VehicleState, emit VehicleStateUpdate
    ├── GLOBAL_POSITION_INT → update position, emit VehicleStateUpdate
    ├── ATTITUDE → update attitude, emit VehicleStateUpdate
    ├── BATTERY_STATUS → update battery, emit VehicleStateUpdate
    ├── GPS_RAW_INT → update gps, emit VehicleStateUpdate
    ├── STATUSTEXT → update status, emit VehicleStateUpdate
    ├── MISSION_CURRENT → update mission, emit VehicleStateUpdate
    └── COMMAND_ACK → resolve pending command future
    │
    ▼
state_tx.send(VehicleStateUpdate)  ← Mission app receives via broadcast channel
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
- Independent `VehicleState` tracking within the bridge
- Distinct `VehicleStateUpdate` emissions keyed by `system_id` — the mission app decides how to map these to CRDT documents or peat-rmw topic subtrees
- Heartbeat timeout monitoring (configurable, default 5s)

Vehicle discovery is implicit — the first HEARTBEAT from a new system ID creates the vehicle entry and begins emitting updates.

### DIL Resilience

Following the pattern from ADR-029 (TAK Transport Adapter):

- **Outbound command queue**: Commands are queued when the MAVLink link is down; replayed in order on reconnect
- **Stale state detection**: `VehicleState.last_heartbeat_ms` allows consumers to detect stale data
- **Configurable timeouts**: Heartbeat loss threshold is configurable per deployment (tight for SITL, loose for LoRa links)
- **Telemetry rate limiting**: Prevents flooding constrained mesh links with high-rate autopilot telemetry

### Mission Application Integration Pattern

`peat-mavlink` is a library crate with **no dependency on `peat-mesh`**. The integrator's mission application composes both crates:

```
mission-app (integrator's binary)
├── peat-mesh       — mesh participation, CRDT storage
├── peat-mavlink    — MAVLink parsing, Peat document mapping
└── mission logic   — routing, filtering, app-specific behavior
```

The bridge emits `VehicleStateUpdate` values via a broadcast channel. The mission app receives them and decides how to write them into the mesh:

```rust
// Mission application — the integrator writes this, not peat-mavlink
use peat_mavlink::{MavlinkBridge, MavlinkBridgeConfig};
use peat_mesh::DocumentStore;

// Create the MAVLink bridge (no peat-mesh involved)
let config = MavlinkBridgeConfig {
    connections: vec![MavlinkConnectionConfig::Serial {
        port: "/dev/ttyACM0".into(),
        baud_rate: 57600,
    }],
    system_id: 254,
    component_id: 191,
    ..Default::default()
};
let (mut bridge, mut rx) = MavlinkBridge::new(config).await?;
bridge.start().await?;

// Create the peat-mesh node (no peat-mavlink involved)
let store = DocumentStore::open("./mesh-data").await?;

// The mission app wires them together
tokio::spawn(async move {
    while let Ok(update) = rx.recv().await {
        // Write vehicle state into mesh as a Peat document
        store.put(
            format!("mavlink.vehicle.{}", update.system_id),
            &update.state,
        ).await?;
    }
});
```

This separation means:
- `peat-mavlink` depends only on `peat-schema` for document types — clean dependency graph
- The integrator controls what gets written to the mesh and at what rate
- Mission-specific logic (filtering, aggregation, rate limiting) lives in the mission app, not the library
- The same `peat-mavlink` crate works whether the integrator uses peat-mesh directly, peat-rmw, or a custom storage backend

**Optional peat-rmw integration** remains available via `feature = "rmw"` for integrators who want automatic topic publication, but it is no longer the primary API.

---

## Dependencies

| Crate | Version | Role |
|-------|---------|------|
| `mavlink` | 0.14 | MAVLink v2 message parsing, dialect support |
| `peat-schema` | workspace | Peat document types (VehicleState maps to schema types) |
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

## Amendment 1: Library Crate / Mission App Composition Pattern (2026-03-30)

**Effective**: 2026-03-30

The original ADR described `peat-mavlink` with a direct dependency on `peat-mesh` (for standalone `AutomergeStore` access) and `peat-rmw` as the primary API. This amendment revises the integration model based on the following reasoning:

**Problem with the original design**: Having `peat-mavlink` depend on `peat-mesh` couples two independently useful crates. A mission computer integrating a MAVLink autopilot is building a **mission application** — it needs both MAVLink parsing and mesh participation, but the library providing MAVLink translation should not dictate how or where documents are stored.

**Revised model**: `peat-mavlink` is a pure library crate that depends on `peat-schema` for Peat document types but has **no dependency on `peat-mesh`**. The integrator's mission application composes both crates:

```
mission-app (Cargo.toml)
├── peat-mavlink = "0.1"    # MAVLink parsing + Peat document mapping
├── peat-mesh = "0.7"       # Mesh participation, CRDT storage
└── (mission-specific deps)
```

The bridge emits `VehicleStateUpdate` values via a `tokio::sync::broadcast` channel. The mission app receives updates and writes them into `peat-mesh` (or any other storage backend) as it sees fit.

**Key changes from the original design**:
1. `MavlinkBridge::with_store()` removed — the bridge does not own or reference a store
2. `MavlinkBridge::with_rmw_node()` moved behind `feature = "rmw"` — no longer the primary API
3. `MavlinkBridge::new()` returns `(Self, Receiver<VehicleStateUpdate>)` — the integrator consumes the channel
4. Dependency on `peat-mesh` replaced with `peat-schema`
5. Examples updated to show the mission app composition pattern

This follows the same principle as the `mavlink` crate itself — it parses MAVLink, it doesn't decide what you do with the parsed messages.

---

## References

- [MAVLink v2 Protocol](https://mavlink.io/en/guide/serialization.html)
- [MAVLink Message Definitions](https://mavlink.io/en/messages/common.html)
- [mavlink-rs crate](https://crates.io/crates/mavlink)
- [mavros (ROS 2 MAVLink bridge)](https://github.com/mavlink/mavros)
- [mLRS (MAVLink LoRa System)](https://github.com/olliw42/mLRS)
- [ArduPilot Companion Computer Docs](https://ardupilot.org/dev/docs/companion-computers.html)
