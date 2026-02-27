# ADR-051: PEAT-SBD Satellite Transport Crate

**Status**: Proposed
**Date**: 2025-02-10
**Authors**: Kit Plummer, Codex
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-032 (Pluggable Transport Abstraction), ADR-035 (PEAT-Lite Embedded Nodes), ADR-037 (Resource-Constrained Device Optimization), ADR-039 (PEAT-BTLE Mesh Transport), ADR-041 (Multi-Transport Embedded Integration)

---

## Executive Summary

This ADR defines the architecture for `peat-sbd`, a Rust crate providing Iridium Short Burst Data (SBD) satellite transport for PEAT Protocol. The crate enables global, infrastructure-independent message exchange via the Iridium satellite constellation, targeting beyond-line-of-sight (BLOS) scenarios where no terrestrial network exists. It implements the ADR-032 `Transport` trait as an external transport extension, following the same pattern established by `peat-btle`.

---

## Context

### The Satellite Communication Gap

PEAT's current transport options—QUIC/Iroh (IP networks) and peat-btle (BLE mesh)—share a common limitation: they require terrestrial infrastructure or proximity between peers. Tactical edge operations frequently occur in environments where neither is available:

| Scenario | Gap | PEAT Use Case |
|----------|-----|---------------|
| Maritime patrol | No cellular, no WiFi | Ship-to-shore PLI and status |
| Remote overwatch | Beyond radio range of C2 | Forward observer position reports |
| Disaster response | Infrastructure destroyed | Field team coordination with HQ |
| Long-range ISR | UAS beyond BLOS | Telemetry relay from autonomous platforms |
| Expeditionary ops | Austere, denied environments | Emergency PACE fallback |

### Why Iridium SBD?

Iridium SBD is uniquely suited as a PEAT transport for several reasons:

| Property | Iridium SBD | Starlink | Inmarsat BGAN |
|----------|-------------|----------|---------------|
| **Coverage** | True global (pole-to-pole) | ~±57° latitude | ±76° latitude |
| **Terminal size** | Pager-sized (9603: 32×30×12mm) | Pizza box | Laptop-sized |
| **Power draw** | ~1.5W transmit, ~0.5W standby | ~100W | ~15W |
| **Latency** | 5-20s (store-and-forward) | 20-40ms (real-time) | 600ms |
| **Message size** | MO: 1,960 bytes / MT: 1,890 bytes | Streaming | Streaming |
| **Cost per message** | ~$0.04-0.13/message (SBD plans) | $120+/mo flat | ~$5/MB |
| **Antenna** | Omnidirectional stub | Motorized phased array | Directional |
| **Integration** | AT commands over serial | Ethernet/WiFi | Ethernet |

SBD's small message size, low power, tiny form factor, and true global coverage make it ideal as a **contingency/emergency transport** in the PACE model—not a primary data pipe, but a lifeline when everything else fails.

### Why a Dedicated Crate?

Following the same rationale as `peat-btle` (ADR-039) and the pluggable transport architecture (ADR-032), SBD has fundamentally different semantics than stream-based transports:

1. **Store-and-Forward**: Messages are queued at the Iridium gateway, not delivered in real-time
2. **Asymmetric Addressing**: Mobile-Originated (MO) goes to a gateway; Mobile-Terminated (MT) requires IMEI-based routing
3. **AT Command Interface**: Serial/UART communication with the modem, not sockets
4. **No Peer Discovery**: Peers don't discover each other over SBD—routing requires pre-configured IMEI mappings or gateway relay
5. **Extreme Size Constraints**: 1,960 bytes MO / 1,890 bytes MT maximum
6. **High Latency**: 5-20 seconds per message, not milliseconds
7. **Cost Sensitivity**: Each message incurs airtime charges; chatty protocols are expensive

### External Crate Pattern

Like `peat-btle`, `peat-sbd` will be developed as an external crate hosted on Radicle, following the established pattern:

```
peat (main repo)
├── peat-protocol/    ← Transport trait definitions (ADR-032)
├── peat-ffi/         ← FFI bindings with SBD config
└── ...

peat-btle (external) ← BLE mesh transport
    └── rad:z458mp9Um3AYNQQFMdHaNEUtmiohq

peat-sbd (external)  ← SBD satellite transport [NEW]
    └── rad:zXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

---

## Decision Drivers

### Requirements

1. **Global Coverage**: Operate anywhere on Earth with sky visibility
2. **Low Power**: Suitable for battery-powered field devices (< 2W transmit)
3. **Small Form Factor**: Integrate with man-portable and UAS platforms
4. **PEAT Transport Trait**: Implement ADR-032 `Transport` trait for TransportManager integration
5. **Message Efficiency**: Maximize PEAT data per SBD message given 1,960-byte limit
6. **Reliability**: Handle message queuing, retry, and confirmation
7. **Security**: Application-layer encryption over SBD (per ADR-006)
8. **Dual-Mode Operation**: Standalone (embedded gateway) or transport plugin (full PEAT node)

### Constraints

1. **Message Size**: Hard limit of 1,960 bytes MO / 1,890 bytes MT
2. **Latency**: 5-20 second delivery, not suitable for real-time
3. **Cost**: Per-message billing; protocol must minimize message count
4. **Serial Interface**: AT command protocol over UART/RS-232
5. **Modem Hardware**: Requires Iridium 9602, 9603, RockBLOCK, or compatible transceiver
6. **Network Registration**: IMEI must be provisioned on Iridium network with SBD service
7. **No Broadcast**: SBD is point-to-point (MO→Gateway, Gateway→MT); no mesh/broadcast capability
8. **Regulatory**: Iridium L-band transmitters require appropriate licensing in some jurisdictions

---

## Decision

### Architecture

`peat-sbd` implements the ADR-032 `Transport` trait with SBD-specific adaptations for store-and-forward satellite communication.

### Crate Structure

```
peat-sbd/
├── src/
│   ├── lib.rs              # Public API, re-exports
│   ├── transport.rs         # Transport trait implementation (ADR-032)
│   ├── config.rs            # SBD configuration (serial port, IMEI, gateway)
│   ├── modem/
│   │   ├── mod.rs           # Modem abstraction trait
│   │   ├── at_commands.rs   # AT command protocol (+SBDWB, +SBDI, etc.)
│   │   ├── iridium_9603.rs  # 9603/9602 modem driver
│   │   └── mock.rs          # Mock modem for testing
│   ├── message/
│   │   ├── mod.rs           # Message framing and types
│   │   ├── encoding.rs      # Compact binary encoding (PEAT → SBD payload)
│   │   ├── fragmentation.rs # Multi-message fragmentation for >1960 byte payloads
│   │   └── compression.rs   # Optional payload compression (LZ4/zstd)
│   ├── gateway/
│   │   ├── mod.rs           # Gateway relay abstraction
│   │   ├── directip.rs      # DirectIP socket gateway client
│   │   ├── email.rs         # Email-based gateway (SMTP/IMAP)
│   │   └── relay.rs         # PEAT relay server (SBD ↔ PEAT mesh bridge)
│   ├── routing/
│   │   ├── mod.rs           # Peer-to-IMEI routing
│   │   └── imei_map.rs      # NodeId ↔ IMEI mapping table
│   ├── queue/
│   │   ├── mod.rs           # Outbound message queue
│   │   └── priority.rs      # Priority-based queue with cost awareness
│   ├── power/
│   │   ├── mod.rs           # Power management
│   │   └── schedule.rs      # Scheduled transmission windows
│   └── platform/
│       ├── mod.rs           # Platform abstraction
│       ├── linux.rs         # Linux serial (termios)
│       ├── android.rs       # Android serial via USB OTG / JNI
│       └── embedded.rs      # no_std embedded serial (ESP32, STM32)
├── tests/
│   ├── modem_tests.rs
│   ├── encoding_tests.rs
│   ├── fragmentation_tests.rs
│   └── integration_tests.rs
├── examples/
│   ├── send_position.rs     # Send a PLI report via SBD
│   ├── gateway_relay.rs     # Run a gateway relay server
│   └── scheduled_sync.rs    # Scheduled batch sync
├── Cargo.toml
└── README.md
```

### Core Types

```rust
/// SBD transport configuration
#[derive(Debug, Clone)]
pub struct SbdConfig {
    /// Serial port path (e.g., "/dev/ttyUSB0", "COM3")
    pub serial_port: String,

    /// Serial baud rate (default: 19200 for 9603)
    pub baud_rate: u32,

    /// Modem IMEI (auto-detected if None)
    pub imei: Option<String>,

    /// Gateway configuration
    pub gateway: GatewayConfig,

    /// Power management profile
    pub power_profile: PowerProfile,

    /// Maximum messages per hour (cost control)
    pub max_messages_per_hour: Option<u16>,

    /// Enable compression (reduces message count at CPU cost)
    pub compression: bool,

    /// Transmission schedule (None = send immediately)
    pub tx_schedule: Option<TransmitSchedule>,

    /// Peer IMEI routing table
    pub peer_imei_map: HashMap<String, String>, // NodeId hex → IMEI
}

/// Gateway configuration for MT message delivery
#[derive(Debug, Clone)]
pub enum GatewayConfig {
    /// DirectIP socket connection to Iridium gateway
    DirectIp {
        host: String,
        port: u16,
    },
    /// Email-based gateway (legacy)
    Email {
        smtp_server: String,
        imap_server: String,
        credentials: String, // Reference to credential store, not inline
    },
    /// PEAT relay server (bridges SBD ↔ PEAT mesh)
    PeatRelay {
        relay_url: String,
    },
    /// No gateway (MO-only, fire-and-forget)
    None,
}

/// Power management profile for satellite modem
#[derive(Debug, Clone, Copy)]
pub enum PowerProfile {
    /// Modem always on, fastest response (highest power)
    AlwaysOn,
    /// Modem powered on for scheduled windows
    Scheduled {
        /// Transmit window interval in seconds
        interval_secs: u32,
        /// Window duration in seconds
        window_secs: u32,
    },
    /// Modem powered on only when messages queued
    OnDemand {
        /// Minimum interval between power-on cycles (seconds)
        min_interval_secs: u32,
    },
    /// Modem off, manual trigger only
    Manual,
}

/// Transmit schedule for batched operations
#[derive(Debug, Clone)]
pub struct TransmitSchedule {
    /// Interval between transmission windows (seconds)
    pub interval_secs: u32,
    /// Maximum messages per window
    pub max_per_window: u8,
    /// Priority threshold—only messages at or above this priority
    /// are sent outside scheduled windows
    pub immediate_priority: MessagePriority,
}
```

### Transport Trait Implementation

```rust
pub struct PeatSbdTransport {
    config: SbdConfig,
    modem: Arc<Mutex<dyn SbdModem>>,
    outbound_queue: Arc<Mutex<PriorityQueue<SbdMessage>>>,
    capabilities: TransportCapabilities,
    signal_strength: AtomicU8,
    available: AtomicBool,
    stats: Arc<SbdStats>,
}

#[async_trait]
impl Transport for PeatSbdTransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    fn signal_quality(&self) -> Option<u8> {
        // Map Iridium 0-5 RSSI to 0-100 scale
        let rssi = self.signal_strength.load(Ordering::Relaxed);
        Some(rssi * 20) // 0→0, 1→20, 2→40, 3→60, 4→80, 5→100
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        // Can reach if we have an IMEI mapping for this peer
        // OR if gateway relay is configured (any peer reachable via mesh bridge)
        let peer_hex = hex::encode(peer_id);
        self.config.peer_imei_map.contains_key(&peer_hex)
            || matches!(self.config.gateway, GatewayConfig::PeatRelay { .. })
    }
}

impl PeatSbdTransport {
    pub fn new(config: SbdConfig) -> Result<Self, SbdError> {
        let capabilities = TransportCapabilities {
            transport_type: TransportType::Satellite,
            max_bandwidth_bps: 33,          // ~1960 bytes / 60 sec realistic throughput
            typical_latency_ms: 10_000,     // 10 seconds typical
            max_range_meters: 0,            // 0 = unlimited/global
            bidirectional: true,
            reliable: true,                 // Store-and-forward with ACK
            battery_impact: 40,             // Moderate—transmit burst is high but brief
            supports_broadcast: false,      // Point-to-point only
            requires_pairing: false,        // No pairing, but needs IMEI routing
            max_message_size: 1_960,        // MO SBD limit
        };

        Ok(Self {
            config,
            modem: Arc::new(Mutex::new(Iridium9603Modem::new(/* ... */)?)),
            outbound_queue: Arc::new(Mutex::new(PriorityQueue::new())),
            capabilities,
            signal_strength: AtomicU8::new(0),
            available: AtomicBool::new(false),
            stats: Arc::new(SbdStats::default()),
        })
    }

    /// Queue a message for transmission
    pub async fn queue_message(
        &self,
        payload: &[u8],
        destination: SbdDestination,
        priority: MessagePriority,
    ) -> Result<SbdMessageId, SbdError> {
        // Check cost budget
        if let Some(max) = self.config.max_messages_per_hour {
            if self.stats.messages_this_hour() >= max as u64 {
                return Err(SbdError::BudgetExceeded);
            }
        }

        // Compress if enabled and beneficial
        let payload = if self.config.compression {
            compress_if_smaller(payload)?
        } else {
            payload.to_vec()
        };

        // Fragment if necessary
        let fragments = if payload.len() > MAX_SBD_PAYLOAD {
            fragment_message(&payload, MAX_SBD_PAYLOAD)?
        } else {
            vec![SbdFragment::single(payload)]
        };

        let msg_id = SbdMessageId::new();
        let mut queue = self.outbound_queue.lock().await;
        for fragment in fragments {
            queue.push(SbdMessage {
                id: msg_id,
                fragment,
                destination: destination.clone(),
                priority,
                queued_at: Instant::now(),
            });
        }

        // If priority >= immediate threshold, trigger transmission
        if let Some(ref schedule) = self.config.tx_schedule {
            if priority >= schedule.immediate_priority {
                self.trigger_transmit().await?;
            }
        } else {
            self.trigger_transmit().await?;
        }

        Ok(msg_id)
    }

    /// Initiate SBD session (send MO, check for MT)
    async fn trigger_transmit(&self) -> Result<(), SbdError> {
        let mut modem = self.modem.lock().await;

        // Check signal strength
        let rssi = modem.signal_strength().await?;
        self.signal_strength.store(rssi, Ordering::Relaxed);

        if rssi == 0 {
            return Err(SbdError::NoSignal);
        }

        // Dequeue highest priority message
        let mut queue = self.outbound_queue.lock().await;
        if let Some(msg) = queue.pop() {
            // Write to modem buffer
            modem.write_binary(&msg.fragment.data).await?;

            // Initiate SBD session (+SBDI / +SBDIX)
            let result = modem.initiate_session().await?;

            if result.mo_status.is_success() {
                self.stats.record_sent();
                // Check for MT message
                if result.mt_queued > 0 {
                    let mt_data = modem.read_binary().await?;
                    self.handle_incoming(mt_data).await?;
                }
            } else {
                // Re-queue on failure
                queue.push(msg);
                return Err(SbdError::TransmitFailed(result.mo_status));
            }
        }

        Ok(())
    }
}
```

### Compact Message Encoding

Given the 1,960-byte constraint, efficient encoding is critical. PEAT messages must be packed tightly:

```rust
/// Compact PEAT-over-SBD message format
///
/// Header (8 bytes):
///   [0]     Version + flags (1 byte)
///   [1]     Message type (1 byte)
///   [2-3]   Sequence number (2 bytes, big-endian)
///   [4-5]   Payload length (2 bytes, big-endian)
///   [6-7]   CRC-16 of payload (2 bytes)
///
/// Payload (up to 1,952 bytes):
///   Encoded PEAT data (protobuf, CBOR, or raw)
///
/// Total: max 1,960 bytes
///
pub struct SbdFrame {
    pub version: u8,         // Protocol version (upper 4 bits) + flags (lower 4)
    pub msg_type: SbdMessageType,
    pub sequence: u16,
    pub payload: Vec<u8>,    // Max 1,952 bytes
}

#[repr(u8)]
pub enum SbdMessageType {
    /// Position Location Information (PLI)
    /// Compact: lat(4) + lon(4) + alt(2) + heading(2) + speed(2) + time(4) = 18 bytes
    Pli = 0x01,

    /// Status report (battery, health, mission state)
    Status = 0x02,

    /// Text message (compressed UTF-8)
    TextMessage = 0x03,

    /// CRDT delta sync (for peat-lite state)
    CrdtDelta = 0x04,

    /// Fragmented message (part of larger payload)
    Fragment = 0x05,

    /// Acknowledgment
    Ack = 0x06,

    /// Command (from C2 via gateway)
    Command = 0x07,

    /// Heartbeat / keepalive
    Heartbeat = 0x08,
}

impl SbdFrame {
    /// Encode a PLI report in minimal bytes
    pub fn encode_pli(lat: f64, lon: f64, alt: f32, heading: u16, speed: u16) -> Self {
        let mut payload = Vec::with_capacity(18);
        payload.extend_from_slice(&(lat as f32).to_be_bytes());  // 4 bytes (sufficient for ~1m precision)
        payload.extend_from_slice(&(lon as f32).to_be_bytes());  // 4 bytes
        payload.extend_from_slice(&(alt as i16).to_be_bytes());  // 2 bytes (meters, ±32km)
        payload.extend_from_slice(&heading.to_be_bytes());       // 2 bytes (degrees × 10)
        payload.extend_from_slice(&speed.to_be_bytes());         // 2 bytes (cm/s)
        payload.extend_from_slice(
            &(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32)
                .to_be_bytes(),
        ); // 4 bytes (epoch seconds, wraps 2106)

        Self {
            version: 0x10, // v1, no flags
            msg_type: SbdMessageType::Pli,
            sequence: 0,
            payload,
        }
    }

    /// Total frame size including header
    pub fn wire_size(&self) -> usize {
        8 + self.payload.len()
    }
}
```

### Message Fragmentation

For payloads exceeding the SBD limit (e.g., CRDT deltas, images):

```rust
/// Fragment header (4 bytes, fits within SbdFrame payload)
///
///   [0-1]   Fragment group ID (2 bytes)
///   [2]     Fragment index (1 byte, 0-indexed)
///   [3]     Total fragments (1 byte)
///
/// Effective payload per fragment: 1,952 - 4 = 1,948 bytes
/// Maximum reassembled payload: 255 × 1,948 = 496,740 bytes
///
pub struct FragmentHeader {
    pub group_id: u16,
    pub index: u8,
    pub total: u8,
}

pub fn fragment_message(data: &[u8], max_fragment_payload: usize) -> Result<Vec<SbdFragment>, SbdError> {
    let effective_payload = max_fragment_payload - FRAGMENT_HEADER_SIZE;
    let total_fragments = (data.len() + effective_payload - 1) / effective_payload;

    if total_fragments > 255 {
        return Err(SbdError::PayloadTooLarge);
    }

    let group_id = rand::random::<u16>();
    let mut fragments = Vec::with_capacity(total_fragments);

    for (i, chunk) in data.chunks(effective_payload).enumerate() {
        let header = FragmentHeader {
            group_id,
            index: i as u8,
            total: total_fragments as u8,
        };

        let mut fragment_data = Vec::with_capacity(FRAGMENT_HEADER_SIZE + chunk.len());
        fragment_data.extend_from_slice(&header.to_bytes());
        fragment_data.extend_from_slice(chunk);

        fragments.push(SbdFragment { data: fragment_data });
    }

    Ok(fragments)
}
```

### Gateway Relay Architecture

The key architectural decision is how SBD messages flow between isolated SBD-equipped nodes and the broader PEAT mesh:

```
                        Iridium Constellation
                              ▲    ▲
                             /      \
                            /        \
┌──────────────────┐      /          \      ┌──────────────────┐
│  Field Device A  │     /            \     │  Field Device B  │
│  ┌─────────────┐ │    /              \    │ ┌─────────────┐  │
│  │ PEAT Node   │ │   ▼                ▼   │ │ PEAT Node   │  │
│  │ peat-sbd    │◄──► Iridium    Iridium ◄──►│ peat-sbd    │  │
│  │ (MO/MT)     │ │   Gateway    Gateway   │ │ (MO/MT)     │  │
│  └─────────────┘ │      │            │    │ └─────────────┘  │
└──────────────────┘      │            │    └──────────────────┘
                          ▼            ▼
                    ┌────────────────────────┐
                    │   PEAT SBD Relay       │
                    │   (DirectIP server)    │
                    │                        │
                    │   MO → PEAT mesh pub   │
                    │   PEAT mesh sub → MT   │
                    │                        │
                    │   Peer IMEI registry   │
                    │   Message routing      │
                    │   Fragment reassembly  │
                    └───────────┬────────────┘
                                │
                    ┌───────────▼────────────┐
                    │   PEAT Mesh            │
                    │   (QUIC/Iroh)          │
                    │   Full CRDT sync       │
                    └────────────────────────┘
```

**PEAT SBD Relay** is a server-side component that:
1. Receives MO messages from the Iridium gateway via DirectIP
2. Decodes PEAT-over-SBD frames
3. Publishes decoded data into the PEAT mesh (as a full PEAT node)
4. Subscribes to PEAT mesh data destined for SBD-connected peers
5. Encodes PEAT data into SBD frames
6. Sends MT messages to field devices via the Iridium gateway

### Dual-Mode Operation

Following `peat-btle`'s pattern:

```
┌─────────────────────────────────┐
│   Full PEAT (ATAK, CLI, etc.)   │
│   TransportManager (PACE policy)│ ← peat-sbd is one transport option
└──────────┬──────────────────────┘
           │
┌──────────▼──────────────────────┐
│      peat-sbd crate             │
│ (Standalone OR transport plugin)│ ← Same protocol, dual modes
└──────────┬──────────────────────┘
           │
┌──────────▼──────────────────────┐
│  Embedded tracker               │
│  (ESP32 + Iridium 9603)         │ ← Standalone peat-sbd
│  Can't run full PEAT            │
└─────────────────────────────────┘
```

**Mode 1 - Standalone**: Embedded devices (ESP32 + Iridium 9603, asset trackers) use peat-sbd directly to send compact PLI/status reports via satellite.

**Mode 2 - Transport Plugin**: Full PEAT nodes wrap peat-sbd via `PeatSbdTransport` in `TransportManager`, using it as a contingency/emergency PACE transport.

### PACE Integration

SBD naturally fits as a contingency or emergency transport:

```yaml
transports:
  - id: "sbd-primary"
    type: satellite
    interface: /dev/ttyUSB0
    description: "Iridium SBD (9603)"
    config:
      baud_rate: 19200
      max_messages_per_hour: 60
      power_profile: on_demand
      compression: true

transport_policy:
  name: "expeditionary"
  primary: ["iroh-wlan0"]
  alternate: ["iroh-starlink"]
  contingency: ["ble-mesh", "lora-primary"]
  emergency: ["sbd-primary"]
```

### Cost-Aware Transport Selection

SBD's per-message billing requires cost-awareness in the TransportManager:

```rust
/// Extended capabilities for cost-aware transports
pub struct CostCapabilities {
    /// Cost model for this transport
    pub cost_model: CostModel,
    /// Current budget remaining (messages or bytes)
    pub budget_remaining: Option<u64>,
}

pub enum CostModel {
    /// No per-use cost (IP, BLE, WiFi)
    Flat,
    /// Per-message cost (SBD)
    PerMessage {
        cost_cents: u32,
        budget_messages: Option<u64>,
    },
    /// Per-byte cost (satellite streaming)
    PerByte {
        cost_cents_per_kb: u32,
        budget_bytes: Option<u64>,
    },
}
```

This allows the TransportManager to prefer free transports and only fall through to SBD when no alternatives exist, or when message priority justifies the cost.

---

## peat-ffi Integration

Extend `TransportConfigFFI` (per ADR-050) to support SBD:

```rust
pub struct TransportConfigFFI {
    pub enable_ble: bool,
    pub ble_mesh_id: Option<String>,
    pub ble_power_profile: Option<String>,
    pub transport_preference: Option<Vec<String>>,

    // SBD satellite transport
    pub enable_sbd: bool,
    pub sbd_serial_port: Option<String>,
    pub sbd_baud_rate: Option<u32>,
    pub sbd_max_messages_per_hour: Option<u16>,
    pub sbd_power_profile: Option<String>,
    pub sbd_gateway_url: Option<String>,
}
```

---

## Modem Abstraction

```rust
/// Abstraction over SBD-capable modems
#[async_trait]
pub trait SbdModem: Send + Sync {
    /// Check if modem is responding
    async fn ping(&mut self) -> Result<(), SbdError>;

    /// Get modem IMEI
    async fn imei(&mut self) -> Result<String, SbdError>;

    /// Get current signal strength (0-5)
    async fn signal_strength(&mut self) -> Result<u8, SbdError>;

    /// Write binary data to MO buffer
    async fn write_binary(&mut self, data: &[u8]) -> Result<(), SbdError>;

    /// Read binary data from MT buffer
    async fn read_binary(&mut self) -> Result<Vec<u8>, SbdError>;

    /// Initiate SBD session (transmit MO, receive MT)
    async fn initiate_session(&mut self) -> Result<SbdSessionResult, SbdError>;

    /// Clear message buffers
    async fn clear_buffers(&mut self, which: BufferTarget) -> Result<(), SbdError>;

    /// Power on/off the modem
    async fn set_power(&mut self, on: bool) -> Result<(), SbdError>;

    /// Register for ring alert notifications (MT message waiting)
    async fn enable_ring_alert(&mut self, enable: bool) -> Result<(), SbdError>;
}

/// Result of an SBD session (+SBDIX response)
pub struct SbdSessionResult {
    /// MO status (0 = success, 1 = success but too large, 2+ = failure)
    pub mo_status: MoStatus,
    /// MO sequence number assigned by gateway
    pub mo_msn: u16,
    /// MT status (0 = no message, 1 = message received, 2 = error)
    pub mt_status: MtStatus,
    /// MT sequence number
    pub mt_msn: u16,
    /// MT message length in bytes
    pub mt_length: u16,
    /// Number of MT messages queued at gateway
    pub mt_queued: u8,
}
```

---

## Implementation Plan

### Phase 1: Core Modem Driver

- [ ] Define `SbdModem` trait
- [ ] Implement AT command parser (+SBDWB, +SBDRB, +SBDI/+SBDIX, +CSQ, +CGSN)
- [ ] Implement `Iridium9603Modem` for 9602/9603 transceivers
- [ ] Implement `MockModem` for testing
- [ ] Serial port abstraction (Linux termios, cross-platform via `serialport` crate)
- [ ] Unit tests with mock modem

### Phase 2: Message Encoding & Framing

- [ ] Define `SbdFrame` compact message format
- [ ] Implement `SbdMessageType` encoders (PLI, Status, TextMessage, CrdtDelta)
- [ ] Implement fragmentation / reassembly
- [ ] Implement optional LZ4 compression
- [ ] CRC-16 validation
- [ ] Round-trip encoding tests

### Phase 3: Transport Trait & Queue

- [ ] Implement `PeatSbdTransport` (ADR-032 `Transport` trait)
- [ ] Priority-based outbound message queue
- [ ] Cost budgeting and rate limiting
- [ ] Power management (scheduled windows, on-demand)
- [ ] Signal monitoring and availability reporting
- [ ] Integration tests with mock modem

### Phase 4: Gateway Relay

- [ ] DirectIP server for MO message reception
- [ ] DirectIP client for MT message sending
- [ ] PEAT mesh bridge (full PEAT node that relays SBD ↔ mesh)
- [ ] Peer IMEI registry and routing table
- [ ] Fragment reassembly at gateway
- [ ] End-to-end integration tests

### Phase 5: Platform Support

- [ ] Linux serial driver (primary target)
- [ ] Android USB OTG serial via JNI
- [ ] `no_std` embedded driver for ESP32 + Iridium 9603
- [ ] Cross-compile verification (aarch64, armv7)

---

## Success Criteria

### Functional Requirements

- [ ] Send MO SBD message via Iridium 9603 modem
- [ ] Receive MT SBD message from gateway
- [ ] Round-trip message through gateway relay into PEAT mesh
- [ ] Fragment and reassemble messages exceeding 1,960 bytes
- [ ] Compress payloads to maximize data per message
- [ ] Implement ADR-032 `Transport` trait for TransportManager
- [ ] Cost budgeting prevents exceeding configured message limits

### Performance Requirements

- [ ] PLI report in ≤ 26 bytes (18 payload + 8 header)
- [ ] Message encoding/decoding < 1ms
- [ ] Queue management < 100μs per operation
- [ ] Modem session initiation < 30s (including satellite acquisition)

### Testing

- [ ] Unit tests with mock modem (no hardware required)
- [ ] Integration tests with mock gateway (DirectIP server)
- [ ] Hardware-in-the-loop tests with Iridium 9603 + RockBLOCK developer kit
- [ ] End-to-end test: field device → SBD → gateway → PEAT mesh → response → SBD → field device

---

## Consequences

### Positive

- **True global reach**: PEAT nodes can communicate from anywhere with sky visibility
- **PACE completeness**: Provides a genuine emergency transport when all terrestrial options fail
- **Low power**: Iridium 9603 draws ~1.5W transmit, suitable for battery-powered platforms
- **Small form factor**: 9603 module is 32×30×12mm, embeddable in almost anything
- **Proven infrastructure**: Iridium constellation has been operational since 1998 with 99.9% uptime
- **Dual-mode flexibility**: Same crate works standalone on embedded or as transport plugin on full PEAT

### Negative

- **High latency**: 5-20s per message rules out real-time applications
- **Tiny payloads**: 1,960-byte limit requires careful encoding and fragmentation
- **Per-message cost**: ~$0.04-0.13 per message requires cost-aware protocol design
- **No broadcast**: Point-to-point only; mesh-wide sync requires gateway relay infrastructure
- **Hardware dependency**: Requires Iridium modem and active airtime subscription
- **Gateway complexity**: Full bidirectional operation requires a relay server with Iridium DirectIP

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Iridium network outage | Low | High | Retry queue with exponential backoff; SBD is store-and-forward by nature |
| Cost overrun from chatty sync | Medium | Medium | Hard budget limits, priority filtering, scheduled windows |
| Signal acquisition failure (indoor, dense foliage) | Medium | Medium | Queue messages, retry on schedule; require sky visibility |
| Modem hardware failure | Low | High | Graceful degradation—node continues on other transports |
| Fragment loss (partial reassembly) | Medium | Low | Fragment timeout + retransmit; group-level ACK |

---

## Alternatives Considered

### Option 1: Starlink as Primary Satellite Transport
**Pros**: High bandwidth (50-200 Mbps), low latency (20-40ms), IP-native (just another Iroh interface)
**Cons**: Large terminal, high power (~100W), limited coverage (no polar), expensive
**Decision**: Starlink is better served as an IP transport via Iroh (ADR-032 QUIC), not a dedicated crate. SBD serves a fundamentally different niche (low-power, tiny terminal, global).

### Option 2: Integrate SBD into peat-protocol Directly
**Pros**: Simpler dependency graph, no external crate
**Cons**: Adds serial/modem code to core protocol crate, platform-specific dependencies pollute core, harder to test independently
**Decision**: External crate follows established pattern (peat-btle) and keeps core clean.

### Option 3: Use Existing Rust SBD Libraries
**Pros**: Faster initial development
**Cons**: No mature Rust SBD library exists with the features needed (power management, fragmentation, gateway relay); would still need significant custom code
**Decision**: Build from scratch with clear modem abstraction to support future hardware.

### Option 4: Iridium Certus Instead of SBD
**Pros**: Higher bandwidth (up to 704 kbps), IP-based
**Cons**: Certus terminals are larger, more expensive, higher power; overkill for the emergency/contingency use case
**Decision**: SBD's simplicity and tiny form factor better serve the PACE emergency role. Certus could be a future extension using the same crate structure.

---

## References

1. [Iridium Short Burst Data Service](https://www.iridium.com/services/iridium-sbd/) - Official SBD service page
2. [Iridium SBD Developers Guide v3.0](https://www.ydoc.biz/download/IRDM_IridiumSBDService.pdf) - Protocol specification
3. [Iridium 9603 Transceiver](https://www.iridium.com/products/iridium-9603/) - Primary target hardware
4. [RockBLOCK Developer Kit](https://www.groundcontrol.com/products/iridium/rockblock/) - Development hardware
5. ADR-032: Pluggable Transport Abstraction
6. ADR-039: PEAT-BTLE Mesh Transport Crate
7. ADR-041: Multi-Transport Embedded Integration
8. [peat-btle on Radicle](https://app.radicle.xyz/nodes/rosa.radicle.xyz/rad%3Az458mp9Um3AYNQQFMdHaNEUtmiohq) - External transport crate pattern

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-02-10 | Proposed ADR-051 | Need BLOS transport for expeditionary and maritime PACE scenarios |
| 2025-02-10 | Chose Iridium SBD over Certus | SBD's low power, tiny terminal, and store-and-forward model fit emergency/contingency PACE role |
| 2025-02-10 | External crate pattern | Follows peat-btle precedent; keeps modem/serial code out of core |

---

**Next Steps:**
1. Review and approve ADR
2. Create `peat-sbd` Radicle repository
3. Phase 1: Modem driver with mock testing
4. Acquire RockBLOCK developer kit for hardware-in-the-loop testing
5. Phase 4: Gateway relay for bidirectional SBD ↔ PEAT mesh bridging

**Radicle:**
- Create `rad:z...` for peat-sbd (pending approval)
