# ADR-052: HIVE-LoRa Long-Range Radio Transport

**Status**: Proposed
**Date**: 2026-02-25
**Authors**: Kit Plummer, Codex
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-032 (Pluggable Transport Abstraction), ADR-035 (HIVE-Lite Embedded Nodes), ADR-039 (HIVE-BTLE Mesh Transport), ADR-041 (Multi-Transport Embedded Integration), ADR-051 (HIVE-SBD Satellite Transport)

---

## Executive Summary

This ADR defines the architecture for `hive-lora`, a Rust crate providing long-range LoRa radio transport for HIVE Protocol. The crate fills the 7-87 km range gap between BLE mesh (100-400m) and SBD satellite (global), targeting remote sensor relay, forward observer links, and cross-ridge communication where IP infrastructure is unavailable. A single crate provides two feature-gated link backends: `mlrs-serial` for Linux/desktop nodes communicating via mLRS hardware over UART/USB, and `lora-phy` for embedded nodes (ESP32 + SX1262) driving LoRa chipsets directly. Both backends share the same over-the-air frame format and implement the ADR-032 `Transport` trait as an external transport extension, following the pattern established by `hive-btle` and `hive-sbd`.

---

## Context

### The Long-Range Gap

HIVE's current transport options cover short-range and global communication, but leave a critical gap in between:

| Transport | Range | Bandwidth | Latency | Power | Use Case |
|-----------|-------|-----------|---------|-------|----------|
| QUIC/Iroh | Unlimited (IP) | 1-100 Mbps | 1-50ms | Low | Primary mesh |
| hive-btle | 100-400m (Coded PHY) | 2 Mbps | 10-100ms | Very Low | Device mesh |
| **??? (Gap)** | **7-87 km** | **1.5-9.1 kB/s** | **50-500ms** | **Low** | **Long-range relay** |
| hive-sbd | Global | ~33 B/s | 5-20s | ~1.5W TX | Emergency PACE |

Tactical edge operations regularly need communication across distances that BLE cannot reach but where satellite is overkill or too slow:

| Scenario | Gap | HIVE Use Case |
|----------|-----|---------------|
| Cross-ridge relay | 5-15 km, no line-of-sight to IP | Forward observer ↔ command post PLI |
| Remote sensor network | 10-50 km, no infrastructure | Environmental / seismic sensor data relay |
| Maritime ship-to-shore | 20-80 km coastal | Small craft ↔ shore station coordination |
| Wildfire perimeter | 10-30 km across terrain | Firefighter position tracking |
| Rural disaster response | 15-50 km, infrastructure destroyed | Field team status relay to HQ |

### Why mLRS + Direct LoRa?

mLRS (Mavlink LoRa System) is an open-source LoRa firmware that provides transparent bidirectional serial passthrough over long-range LoRa radio links. It turns commodity LoRa hardware (SX1276, SX1262, STM32-based modules) into a serial cable replacement with ranges of 7-87 km depending on frequency band and conditions.

| Property | mLRS Serial Bridge | Direct LoRa (lora-phy) | LoRaWAN |
|----------|--------------------|------------------------|---------|
| **Topology** | Point-to-point (transparent) | Point-to-point (custom) | Star (gateway-centric) |
| **Payload** | Up to 252 bytes/frame | Up to 255 bytes/frame | 51-222 bytes |
| **Direction** | Full duplex (serial) | Half duplex (radio) | Uplink-dominant |
| **Latency** | 10-50ms (serial) | 50-500ms (radio timing) | 1-5s (class A) |
| **Configuration** | mLRS firmware handles radio | App controls radio directly | Requires LoRaWAN gateway |
| **Infrastructure** | None (peer-to-peer) | None (peer-to-peer) | Requires gateway + network server |
| **Range** | 7-87 km (mLRS optimized) | 5-50 km (depends on params) | 2-15 km typical |

mLRS is the fastest path to long-range capability—connect mLRS modules to UART/USB, push HIVE frames through the serial link. Direct LoRa via `lora-phy` enables embedded nodes (ESP32 + SX1262) to participate without separate mLRS hardware.

### Why a Single Crate with Feature Flags?

Both link backends share the same over-the-air frame format, the same `Transport` trait implementation, and the same gateway bridge logic. Splitting into two crates would duplicate the frame codec, fragmentation logic, and transport layer:

```
hive-lora/
├── src/
│   ├── frame.rs         # Shared frame format (both backends)
│   ├── transport.rs     # Shared Transport trait impl
│   ├── link/
│   │   ├── mod.rs       # LoRaLink trait
│   │   ├── mlrs.rs      # feature = "mlrs-serial"
│   │   └── direct.rs    # feature = "lora-phy"
```

Feature flags keep the dependency footprint minimal: `mlrs-serial` pulls in `tokio-serial` (Linux only), while `lora-phy` pulls in `embedded-hal` + `lora-phy` (no_std compatible).

### External Crate Pattern

Like `hive-btle` and `hive-sbd`, `hive-lora` will be developed as an external crate, following the established pattern:

```
hive (main repo)
├── hive-protocol/    ← Transport trait definitions (ADR-032)
├── hive-ffi/         ← FFI bindings with LoRa config
└── ...

hive-btle (external) ← BLE mesh transport
    └── rad:z458mp9Um3AYNQQFMdHaNEUtmiohq

hive-sbd (external)  ← SBD satellite transport
    └── rad:zXXXXXXXXXXXXXXXXXXXXXXXXXXXX

hive-lora (external) ← LoRa long-range transport [NEW]
    └── rad:zXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

---

## Decision Drivers

### Requirements

1. **Long-Range**: 7-87 km depending on frequency band and terrain
2. **Low Power**: Suitable for battery-powered field devices (< 1W transmit)
3. **No Infrastructure**: Peer-to-peer links, no gateways or network servers required
4. **HIVE Transport Trait**: Implement ADR-032 `Transport` trait for TransportManager integration
5. **Dual Link Backends**: mLRS serial bridge (Linux) and direct LoRa radio (embedded)
6. **Shared Frame Format**: Identical over-the-air encoding regardless of link backend
7. **Gateway Bridge**: Linux nodes bridge LoRa ↔ IP mesh per ADR-041
8. **Regulatory Compliance**: Duty cycle limits for EU 868 MHz, FCC rules for US 915 MHz

### Constraints

1. **Bandwidth**: 1.5-9.1 kB/s depending on band and spreading factor; not suitable for bulk data
2. **Half Duplex**: LoRa radio is half-duplex; only one direction at a time
3. **Payload Size**: 252 bytes max per mLRS frame; fragmentation needed for larger messages
4. **Point-to-Point**: Not a mesh protocol; each link is one-to-one
5. **Frequency Bands**: Must respect regional regulations (868 MHz EU, 915 MHz US, 433 MHz global, 2.4 GHz ISM)
6. **Duty Cycle**: EU 868 MHz band has 1% duty cycle limit (36 seconds per hour at some sub-bands)
7. **Hardware Diversity**: mLRS supports SX1276, SX1262, STM32-based modules; direct LoRa targets SX1262

---

## Decision

### Architecture

`hive-lora` implements the ADR-032 `Transport` trait with a link abstraction layer that supports both mLRS serial bridge and direct LoRa radio backends.

### Crate Structure

```
hive-lora/
├── src/
│   ├── lib.rs              # Public API, re-exports, feature gates
│   ├── transport.rs         # Transport trait implementation (ADR-032)
│   ├── config.rs            # LoRa configuration (serial port, radio params, peer map)
│   ├── frame.rs             # Over-the-air frame format (shared by both backends)
│   ├── fragment.rs          # Fragmentation / reassembly for >252 byte messages
│   ├── link/
│   │   ├── mod.rs           # LoRaLink trait abstraction
│   │   ├── mlrs.rs          # mLRS serial bridge (feature = "mlrs-serial")
│   │   └── direct.rs        # Direct SX1262 radio (feature = "lora-phy")
│   ├── discovery.rs         # Peer discovery (pre-configured or beacon)
│   ├── duty_cycle.rs        # Regional duty cycle tracking and enforcement
│   ├── encryption.rs        # App-layer ChaCha20-Poly1305 encryption
│   └── platform/
│       ├── mod.rs           # Platform abstraction
│       ├── linux.rs         # Linux serial (tokio-serial)
│       └── embedded.rs      # no_std embedded SPI (ESP32 + SX1262)
├── tests/
│   ├── frame_tests.rs
│   ├── fragment_tests.rs
│   ├── transport_tests.rs
│   └── integration_tests.rs
├── examples/
│   ├── mlrs_bridge.rs       # mLRS serial bridge relay
│   ├── send_position.rs     # Send PLI over LoRa
│   └── direct_radio.rs      # Direct SX1262 radio link
├── Cargo.toml
└── README.md
```

### Link Abstraction

The `LoRaLink` trait abstracts over the physical radio layer, allowing the transport and frame logic to be backend-agnostic:

```rust
/// Abstraction over LoRa link backends
#[async_trait]
pub trait LoRaLink: Send + Sync {
    /// Send a frame over the LoRa link
    async fn send(&self, frame: &LoRaFrame) -> Result<(), LoRaError>;

    /// Receive the next frame from the LoRa link
    async fn recv(&self) -> Result<LoRaFrame, LoRaError>;

    /// Check if the link is currently connected/available
    fn is_available(&self) -> bool;

    /// Get current RSSI (received signal strength) if available
    fn rssi(&self) -> Option<i16>;

    /// Get current SNR (signal-to-noise ratio) if available
    fn snr(&self) -> Option<f32>;
}
```

**mLRS serial backend** (`feature = "mlrs-serial"`):
- Opens a UART/USB serial port to the mLRS module via `tokio-serial`
- Wraps `LoRaFrame` bytes in the serial stream
- mLRS firmware handles radio parameters, frequency hopping, and link management
- Linux/desktop only (requires `tokio` runtime)

**Direct LoRa backend** (`feature = "lora-phy"`):
- Drives an SX1262 LoRa transceiver via SPI using the `lora-phy` crate
- Application controls spreading factor, bandwidth, coding rate, and TX power
- `no_std` compatible for embedded targets (ESP32, STM32)
- Requires `embedded-hal` SPI + GPIO traits

### Over-the-Air Frame Format

Both backends use the same frame format on the wire. The frame wraps an eche-lite protocol payload (ADR-035) with a minimal header:

```rust
/// LoRa over-the-air frame format
///
/// Header (3 bytes):
///   [0]     Marker byte (0xEC — "eche")
///   [1]     Flags (1 byte)
///             bit 0:   fragmented (1 = fragment, 0 = complete)
///             bit 1:   encrypted (1 = app-layer encryption)
///             bit 2:   compressed (1 = payload compressed)
///             bit 3:   ack-requested (1 = sender wants ACK)
///             bits 4-7: reserved
///   [2]     Payload length (1 byte, max 252)
///
/// Payload (up to 249 bytes unfragmented, or per-fragment):
///   eche-lite protocol header (16 bytes, ADR-035) + CRDT data
///
/// Total: max 252 bytes (matching mLRS serial frame limit)
///
pub struct LoRaFrame {
    pub flags: FrameFlags,
    pub payload: Vec<u8>,    // Max 249 bytes (252 - 3 header)
}

bitflags! {
    pub struct FrameFlags: u8 {
        const FRAGMENTED    = 0b0000_0001;
        const ENCRYPTED     = 0b0000_0010;
        const COMPRESSED    = 0b0000_0100;
        const ACK_REQUESTED = 0b0000_1000;
    }
}

pub const LORA_FRAME_MARKER: u8 = 0xEC;
pub const LORA_HEADER_SIZE: usize = 3;
pub const LORA_MAX_FRAME_SIZE: usize = 252;
pub const LORA_MAX_PAYLOAD_SIZE: usize = LORA_MAX_FRAME_SIZE - LORA_HEADER_SIZE; // 249

impl LoRaFrame {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(LORA_HEADER_SIZE + self.payload.len());
        buf.push(LORA_FRAME_MARKER);
        buf.push(self.flags.bits());
        buf.push(self.payload.len() as u8);
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, LoRaError> {
        if data.len() < LORA_HEADER_SIZE {
            return Err(LoRaError::FrameTooShort);
        }
        if data[0] != LORA_FRAME_MARKER {
            return Err(LoRaError::InvalidMarker);
        }
        let flags = FrameFlags::from_bits_truncate(data[1]);
        let length = data[2] as usize;
        if data.len() < LORA_HEADER_SIZE + length {
            return Err(LoRaError::IncompletePayload);
        }
        Ok(Self {
            flags,
            payload: data[LORA_HEADER_SIZE..LORA_HEADER_SIZE + length].to_vec(),
        })
    }
}
```

### Fragmentation

Messages exceeding 249 bytes (the payload limit after the 3-byte header) are fragmented:

```rust
/// Fragment header (3 bytes, prepended to fragment payload)
///
///   [0]     Fragment group ID (1 byte, wrapping counter)
///   [1]     Fragment index (1 byte, 0-indexed)
///   [2]     Total fragments (1 byte)
///
/// Effective payload per fragment: 249 - 3 = 246 bytes
/// Maximum reassembled payload: 255 x 246 = 62,730 bytes
///
pub struct FragmentHeader {
    pub group_id: u8,
    pub index: u8,
    pub total: u8,
}

pub const FRAGMENT_HEADER_SIZE: usize = 3;
pub const FRAGMENT_MAX_PAYLOAD: usize = LORA_MAX_PAYLOAD_SIZE - FRAGMENT_HEADER_SIZE; // 246
```

At LoRa data rates (1.5-9.1 kB/s), fragmentation should be kept to a minimum. Most HIVE messages (PLI, status, CRDT deltas) fit in a single frame. Fragmentation is available for larger payloads but not the common case.

### Core Types

```rust
/// LoRa transport configuration
#[derive(Debug, Clone)]
pub struct LoRaConfig {
    /// Link backend selection
    pub link: LinkConfig,

    /// Radio band configuration
    pub band: BandConfig,

    /// Pre-configured peer node IDs (for mLRS point-to-point)
    pub peers: Vec<PeerEntry>,

    /// Enable beacon-based discovery (direct LoRa only)
    pub beacon_discovery: bool,

    /// Beacon interval in seconds (default: 30)
    pub beacon_interval_secs: u32,

    /// Enable app-layer encryption (ChaCha20-Poly1305)
    pub encryption: bool,

    /// Pre-shared key for app-layer encryption (32 bytes)
    pub psk: Option<[u8; 32]>,

    /// Duty cycle enforcement (required for EU 868 MHz)
    pub duty_cycle: Option<DutyCycleConfig>,
}

/// Link backend configuration
#[derive(Debug, Clone)]
pub enum LinkConfig {
    /// mLRS serial bridge (feature = "mlrs-serial")
    MlrsSerial {
        /// Serial port path (e.g., "/dev/ttyUSB0")
        port: String,
        /// Baud rate (mLRS default: 115200)
        baud_rate: u32,
    },
    /// Direct LoRa radio (feature = "lora-phy")
    DirectRadio {
        /// SPI bus configuration
        spi_bus: String,
        /// Chip select GPIO pin
        cs_pin: u8,
        /// Reset GPIO pin
        reset_pin: u8,
        /// DIO1 interrupt GPIO pin
        dio1_pin: u8,
        /// Transmit power in dBm (default: 22 for SX1262)
        tx_power_dbm: i8,
        /// Spreading factor (7-12, default: 10)
        spreading_factor: u8,
        /// Bandwidth in Hz (125000, 250000, 500000)
        bandwidth_hz: u32,
        /// Coding rate (5-8, represents 4/5 to 4/8)
        coding_rate: u8,
    },
}

/// Frequency band configuration
#[derive(Debug, Clone)]
pub struct BandConfig {
    /// Center frequency in Hz
    pub frequency_hz: u32,
    /// Regulatory region
    pub region: Region,
}

#[derive(Debug, Clone, Copy)]
pub enum Region {
    /// EU 868 MHz (863-870 MHz, 1% duty cycle on some sub-bands)
    Eu868,
    /// US 915 MHz (902-928 MHz, no duty cycle, FCC Part 15)
    Us915,
    /// Global 433 MHz (433.05-434.79 MHz, 10mW ERP EU, varies by country)
    Global433,
    /// Global 2.4 GHz ISM (2400-2483.5 MHz, no duty cycle)
    Ism2400,
}

/// Pre-configured peer entry
#[derive(Debug, Clone)]
pub struct PeerEntry {
    /// HIVE node ID (hex string)
    pub node_id: String,
    /// Human-readable label
    pub label: Option<String>,
}
```

### Transport Trait Implementation

```rust
pub struct HiveLoRaTransport {
    config: LoRaConfig,
    link: Arc<dyn LoRaLink>,
    capabilities: TransportCapabilities,
    available: AtomicBool,
    duty_cycle: Option<DutyCycleTracker>,
    fragment_assembler: Mutex<FragmentAssembler>,
}

#[async_trait]
impl Transport for HiveLoRaTransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed) && self.link.is_available()
    }

    fn signal_quality(&self) -> Option<u8> {
        // Map RSSI to 0-100 scale
        // SX1262 typical: -120 dBm (worst) to -30 dBm (best)
        self.link.rssi().map(|rssi| {
            let clamped = rssi.clamp(-120, -30);
            ((clamped + 120) as u8 * 100 / 90).min(100)
        })
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        let peer_hex = hex::encode(peer_id);
        self.config.peers.iter().any(|p| p.node_id == peer_hex)
    }
}

impl HiveLoRaTransport {
    pub fn new(config: LoRaConfig) -> Result<Self, LoRaError> {
        let (data_rate, range, latency) = match config.band.region {
            Region::Ism2400 => (9_100, 7_000, 50),      // 9.1 kB/s, 7 km, 50ms
            Region::Us915   => (3_000, 30_000, 200),     // 3.0 kB/s, 30 km, 200ms
            Region::Eu868   => (1_500, 50_000, 500),     // 1.5 kB/s, 50 km, 500ms
            Region::Global433 => (1_500, 87_000, 500),   // 1.5 kB/s, 87 km, 500ms
        };

        let capabilities = TransportCapabilities {
            transport_type: TransportType::LoRa,
            max_bandwidth_bps: data_rate,
            typical_latency_ms: latency,
            max_range_meters: range,
            bidirectional: true,
            reliable: false,     // No built-in ACK at radio layer
            battery_impact: 15,  // Low — sub-1W transmit power
            supports_broadcast: false,  // Point-to-point
            requires_pairing: false,
            max_message_size: LORA_MAX_PAYLOAD_SIZE as u32,
        };

        // Build the link backend based on config
        #[cfg(feature = "mlrs-serial")]
        let link: Arc<dyn LoRaLink> = match &config.link {
            LinkConfig::MlrsSerial { port, baud_rate } => {
                Arc::new(MlrsSerialLink::open(port, *baud_rate)?)
            }
            _ => return Err(LoRaError::UnsupportedBackend),
        };

        #[cfg(feature = "lora-phy")]
        let link: Arc<dyn LoRaLink> = match &config.link {
            LinkConfig::DirectRadio { .. } => {
                Arc::new(DirectLoRaLink::new(&config)?)
            }
            _ => return Err(LoRaError::UnsupportedBackend),
        };

        let duty_cycle = config.duty_cycle.as_ref().map(|dc| {
            DutyCycleTracker::new(dc.clone())
        });

        Ok(Self {
            config,
            link,
            capabilities,
            available: AtomicBool::new(true),
            duty_cycle,
            fragment_assembler: Mutex::new(FragmentAssembler::new()),
        })
    }

    /// Send a HIVE message over LoRa
    pub async fn send_message(&self, payload: &[u8]) -> Result<(), LoRaError> {
        // Check duty cycle budget
        if let Some(ref tracker) = self.duty_cycle {
            if !tracker.can_transmit(payload.len()) {
                return Err(LoRaError::DutyCycleExceeded);
            }
        }

        // Encrypt if configured
        let payload = if self.config.encryption {
            encrypt_payload(payload, self.config.psk.as_ref().unwrap())?
        } else {
            payload.to_vec()
        };

        // Fragment if necessary
        if payload.len() > LORA_MAX_PAYLOAD_SIZE {
            let fragments = fragment_message(&payload)?;
            for frag in fragments {
                let frame = LoRaFrame {
                    flags: FrameFlags::FRAGMENTED
                        | if self.config.encryption { FrameFlags::ENCRYPTED } else { FrameFlags::empty() },
                    payload: frag,
                };
                self.link.send(&frame).await?;
                // Record airtime for duty cycle
                if let Some(ref tracker) = self.duty_cycle {
                    tracker.record_transmission(frame.encode().len());
                }
            }
        } else {
            let frame = LoRaFrame {
                flags: if self.config.encryption { FrameFlags::ENCRYPTED } else { FrameFlags::empty() },
                payload,
            };
            self.link.send(&frame).await?;
            if let Some(ref tracker) = self.duty_cycle {
                tracker.record_transmission(frame.encode().len());
            }
        }

        Ok(())
    }
}
```

### Data Rate and Range by Band

LoRa performance varies significantly by frequency band. These figures assume mLRS-optimized settings:

| Band | Frequency | Data Rate | Typical Range | Max Range (LOS) | Duty Cycle | Notes |
|------|-----------|-----------|---------------|------------------|------------|-------|
| 2.4 GHz ISM | 2400-2483 MHz | 9.1 kB/s | 3-7 km | 7 km | None | Globally license-free, smallest antennas |
| 915 MHz US | 902-928 MHz | 3.0 kB/s | 10-30 km | 40 km | None (FCC Part 15) | US/Canada/Australia |
| 868 MHz EU | 863-870 MHz | 1.5 kB/s | 15-50 km | 60 km | 1% (some sub-bands) | Europe, must enforce duty cycle |
| 433 MHz | 433-434 MHz | 1.5 kB/s | 20-50 km | 87 km | Varies | Longest range, largest antennas, lowest data rate |

### Topology: Point-to-Point with Gateway Bridge

LoRa links in HIVE are point-to-point, not mesh. A Linux gateway node bridges LoRa traffic into the IP mesh, following the ADR-041 multi-transport gateway pattern:

```
                LoRa Radio Link (7-87 km)
                    ◄──────────────►

┌──────────────────┐                    ┌──────────────────┐
│  Remote Sensor   │                    │  Gateway Node    │
│  (ESP32+SX1262)  │   LoRa frames     │  (Linux+mLRS)    │
│  ┌─────────────┐ │◄─────────────────►│ ┌─────────────┐  │
│  │ eche-lite   │ │                    │ │ hive-lora    │  │
│  │ hive-lora   │ │                    │ │ (mlrs-serial)│  │
│  │ (lora-phy)  │ │                    │ └──────┬───────┘  │
│  └─────────────┘ │                    │        │          │
└──────────────────┘                    │ ┌──────▼───────┐  │
                                        │ │ HIVE Node    │  │
                                        │ │ (QUIC/Iroh)  │  │
                                        │ └──────┬───────┘  │
                                        └────────┼──────────┘
                                                 │
                                        ┌────────▼──────────┐
                                        │   HIVE IP Mesh    │
                                        │   (Full CRDT sync)│
                                        └───────────────────┘
```

Multiple remote nodes can each have a dedicated LoRa link to the gateway (separate mLRS channel or time-slotted), but each individual link is point-to-point.

### Discovery

**mLRS serial mode**: Peers are pre-configured. The mLRS link is established at the radio firmware level (binding); HIVE sees a connected serial port. The `peers` list in `LoRaConfig` maps HIVE node IDs to known LoRa endpoints.

**Direct LoRa mode**: Optional beacon-based discovery. Nodes periodically transmit a beacon frame containing their HIVE node ID and capabilities. Receiving nodes add discovered peers to their local peer table.

```rust
/// Beacon frame payload (fits in a single LoRa frame)
///
///   [0-15]   Node ID (16 bytes)
///   [16]     Capabilities bitfield (NodeCapabilities from eche-lite)
///   [17-18]  Beacon sequence number (2 bytes, big-endian)
///   [19]     TX power (dBm, signed)
///
/// Total: 20 bytes
///
pub struct BeaconPayload {
    pub node_id: [u8; 16],
    pub capabilities: u8,
    pub sequence: u16,
    pub tx_power_dbm: i8,
}
```

### Encryption

Two layers of encryption protect LoRa traffic:

1. **Link-layer (mLRS only)**: mLRS firmware provides AES encryption of the radio link. This protects against casual eavesdropping but uses a shared key configured in mLRS firmware.

2. **Application-layer (both backends)**: HIVE app-layer ChaCha20-Poly1305 encryption using a pre-shared key (PSK). This protects the HIVE payload end-to-end, even if the link layer is compromised or absent (direct LoRa mode).

```rust
/// Encrypt payload with ChaCha20-Poly1305
/// Prepends 12-byte nonce to ciphertext
/// Output: [nonce (12 bytes)] [ciphertext] [tag (16 bytes)]
pub fn encrypt_payload(plaintext: &[u8], psk: &[u8; 32]) -> Result<Vec<u8>, LoRaError> {
    use chacha20poly1305::{ChaCha20Poly1305, KeyInit, AeadInPlace};
    use chacha20poly1305::aead::OsRng;

    let cipher = ChaCha20Poly1305::new(psk.into());
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    let mut buffer = plaintext.to_vec();
    cipher.encrypt_in_place(&nonce, b"", &mut buffer)
        .map_err(|_| LoRaError::EncryptionFailed)?;

    let mut output = Vec::with_capacity(12 + buffer.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&buffer);
    Ok(output)
}
```

Note: The 12-byte nonce + 16-byte AEAD tag adds 28 bytes of overhead per frame. With a 249-byte payload limit, this leaves 221 bytes for the eche-lite protocol header + CRDT data when encryption is enabled.

### Duty Cycle Compliance

EU 868 MHz band requires strict duty cycle enforcement. The `DutyCycleTracker` monitors cumulative airtime and blocks transmission when the limit is reached:

```rust
/// Duty cycle tracker for regulatory compliance
pub struct DutyCycleTracker {
    /// Maximum duty cycle as fraction (e.g., 0.01 for 1%)
    max_duty_cycle: f32,
    /// Sliding window duration (typically 1 hour)
    window_secs: u64,
    /// Accumulated airtime in the current window (microseconds)
    airtime_us: AtomicU64,
    /// Window start timestamp
    window_start: Instant,
}

impl DutyCycleTracker {
    pub fn can_transmit(&self, payload_bytes: usize) -> bool {
        let estimated_airtime_us = self.estimate_airtime_us(payload_bytes);
        let current = self.airtime_us.load(Ordering::Relaxed);
        let window_us = self.window_secs * 1_000_000;
        let max_airtime_us = (window_us as f32 * self.max_duty_cycle) as u64;
        current + estimated_airtime_us < max_airtime_us
    }
}
```

### MAVLink and UAS Integration

mLRS — "MAVLink LoRa System" — was originally designed for MAVLink telemetry, the standard protocol for drone autopilot communication (ArduPilot, PX4). This heritage creates a natural synergy with HIVE's UAS integration architecture (see CAP Protocol Technology Deep Dive, ROS-CAP Bridge):

**Shared radio link**: mLRS supports multiple serial channels on a single LoRa link. A companion computer on a UAS can multiplex HIVE sync frames and MAVLink telemetry over the same mLRS radio, eliminating the need for separate data links:

```
┌──────────────────────────────────┐
│  UAS (Drone)                     │
│  ┌────────────┐ ┌──────────────┐ │
│  │ Autopilot  │ │ Companion    │ │
│  │ (PX4/Ardu) │ │ (Jetson/Pi)  │ │
│  │ MAVLink    │ │ HIVE + ROS   │ │
│  └─────┬──────┘ └──────┬───────┘ │
│        │  UART1         │  UART2  │
│  ┌─────▼────────────────▼──────┐ │
│  │    mLRS TX Module           │ │
│  │    (dual serial → LoRa)     │ │
│  └─────────────┬───────────────┘ │
└────────────────┼─────────────────┘
                 │  LoRa (7-87 km)
┌────────────────┼─────────────────┐
│  ┌─────────────▼───────────────┐ │
│  │    mLRS RX Module           │ │
│  │    (LoRa → dual serial)     │ │
│  └─────┬────────────────┬──────┘ │
│        │  UART1         │  UART2  │
│  ┌─────▼──────┐ ┌──────▼───────┐ │
│  │ GCS        │ │ HIVE Gateway │ │
│  │ (QGroundC) │ │ (hive-lora)  │ │
│  └────────────┘ └──────────────┘ │
│  Ground Station                   │
└───────────────────────────────────┘
```

**Design implications**:
- `hive-lora` treats the mLRS serial port as a transparent byte pipe — it is unaware of MAVLink traffic on other serial channels
- Frame marker byte `0xEC` ensures HIVE frames are distinguishable from MAVLink frames (`0xFD` for MAVLink v2) if sharing a single serial channel
- Future work: optional MAVLink passthrough mode where `hive-lora` can forward MAVLink position data into HIVE CRDTs, bridging UAS telemetry directly into the HIVE mesh without a separate ROS-CAP bridge

### PACE Integration

LoRa fills the gap between BLE (short-range) and SBD (global) in the PACE transport hierarchy:

```yaml
transports:
  - id: "lora-primary"
    type: lora
    description: "LoRa 915 MHz via mLRS"
    config:
      link: mlrs-serial
      port: /dev/ttyUSB0
      baud_rate: 115200
      band: us915
      frequency_hz: 915000000
      encryption: true
      psk_ref: "lora-psk-alpha"

transport_policy:
  name: "expeditionary"
  primary: ["iroh-wlan0"]
  alternate: ["ble-mesh"]
  contingency: ["lora-primary"]
  emergency: ["sbd-primary"]
```

LoRa is a natural **alternate or contingency** transport:
- **Alternate**: When IP infrastructure is unavailable but peers are within 7-87 km
- **Contingency**: When BLE range (400m) is insufficient but SBD latency (5-20s) is unacceptable

For remote sensor deployments, LoRa may serve as the **primary** transport when sensors are deployed beyond BLE range with no IP connectivity.

### hive-ffi Integration

Extend `TransportConfigFFI` (per ADR-050) to support LoRa:

```rust
pub struct TransportConfigFFI {
    pub enable_ble: bool,
    pub ble_mesh_id: Option<String>,
    // ... existing fields ...

    // LoRa transport
    pub enable_lora: bool,
    pub lora_serial_port: Option<String>,
    pub lora_baud_rate: Option<u32>,
    pub lora_band: Option<String>,          // "us915", "eu868", "433", "2400"
    pub lora_frequency_hz: Option<u32>,
    pub lora_encryption: Option<bool>,
    pub lora_psk_hex: Option<String>,       // 64-char hex string (32 bytes)
    pub lora_peers: Option<Vec<String>>,    // Node ID hex strings
}
```

---

## Implementation Plan

### Phase 1: Frame Format + Core Types (No Hardware)

- [ ] Define `LoRaFrame` over-the-air frame format
- [ ] Define `FrameFlags` bitflags
- [ ] Implement frame encode/decode with validation
- [ ] Define `FragmentHeader` and fragmentation/reassembly logic
- [ ] Define `LoRaConfig`, `LinkConfig`, `BandConfig`, `Region` types
- [ ] Define `LoRaLink` trait abstraction
- [ ] Define `LoRaError` error types
- [ ] Unit tests: frame round-trip, fragmentation edge cases, config validation

### Phase 2: mLRS Serial Link (Linux)

- [ ] Implement `MlrsSerialLink` (feature = "mlrs-serial")
- [ ] Serial port open/configure via `tokio-serial`
- [ ] Frame delimiting over serial stream (marker byte sync)
- [ ] Read/write loop with timeout handling
- [ ] RSSI extraction from mLRS telemetry (if available)
- [ ] Integration tests with mock serial port
- [ ] Hardware-in-the-loop test with mLRS modules

### Phase 3: Transport Trait + Gateway Bridge

- [ ] Implement `HiveLoRaTransport` (ADR-032 `Transport` trait)
- [ ] Pre-configured peer routing
- [ ] Signal quality mapping (RSSI → 0-100)
- [ ] Gateway bridge: LoRa frames ↔ HIVE mesh messages (ADR-041 pattern)
- [ ] Integration tests with mock link

### Phase 4: Direct LoRa Radio (Embedded)

- [ ] Implement `DirectLoRaLink` (feature = "lora-phy")
- [ ] SX1262 driver via `lora-phy` crate + `embedded-hal` SPI
- [ ] Radio configuration (spreading factor, bandwidth, coding rate, TX power)
- [ ] Beacon-based peer discovery
- [ ] ESP32 + SX1262 hardware-in-the-loop test
- [ ] Cross-compile verification (xtensa-esp32, aarch64)

### Phase 5: Encryption + Duty Cycle Compliance

- [ ] ChaCha20-Poly1305 app-layer encryption
- [ ] PSK configuration and key management
- [ ] `DutyCycleTracker` for EU 868 MHz compliance
- [ ] Regional duty cycle configuration presets
- [ ] Encryption round-trip tests
- [ ] Duty cycle enforcement tests

### Phase 6: Adaptive Range Modes

- [ ] Configurable radio presets (high-rate/short-range vs low-rate/long-range)
- [ ] Runtime spreading factor adjustment based on link quality
- [ ] RSSI/SNR-based adaptive TX power
- [ ] Band-hopping support for mLRS

---

## Success Criteria

### Functional Requirements

- [ ] Send/receive HIVE messages via mLRS serial bridge (Linux)
- [ ] Send/receive HIVE messages via direct SX1262 radio (ESP32)
- [ ] Fragment and reassemble messages exceeding 249 bytes
- [ ] Gateway bridge relays LoRa traffic into HIVE IP mesh
- [ ] Pre-configured peer discovery works for mLRS mode
- [ ] Beacon-based discovery works for direct LoRa mode
- [ ] App-layer encryption protects payload end-to-end
- [ ] Duty cycle enforcement prevents regulatory violations on EU 868 MHz
- [ ] Implement ADR-032 `Transport` trait for TransportManager integration

### Performance Requirements

- [ ] Frame encode/decode < 100 us
- [ ] Fragmentation/reassembly < 500 us for max-size messages
- [ ] mLRS serial link latency < 50ms (serial overhead only)
- [ ] Direct LoRa link latency within expected range for configured spreading factor
- [ ] Encryption overhead < 1ms per frame

### Testing

- [ ] Unit tests with mock link (no hardware required)
- [ ] Integration tests with mock serial port
- [ ] Hardware-in-the-loop tests with mLRS modules (915 MHz)
- [ ] Hardware-in-the-loop tests with ESP32 + SX1262
- [ ] End-to-end test: remote sensor → LoRa → gateway → HIVE mesh → response → LoRa → sensor
- [ ] Range test at 915 MHz: verify > 10 km line-of-sight

---

## Consequences

### Positive

- **Range gap filled**: 7-87 km coverage bridges BLE and satellite
- **Low power**: Sub-1W transmit, suitable for battery-powered remote sensors
- **No infrastructure**: Pure peer-to-peer, no gateways or network servers required
- **Dual backend flexibility**: mLRS for quick deployment (plug in modules), direct LoRa for embedded integration
- **Shared frame format**: Both backends interoperate on the air
- **PACE completeness**: LoRa as alternate/contingency between BLE (alternate) and SBD (emergency)
- **Open-source stack**: mLRS is open-source; no vendor lock-in

### Negative

- **Low bandwidth**: 1.5-9.1 kB/s limits to small messages (PLI, status, compact CRDTs)
- **Point-to-point only**: Not a mesh; each link requires a dedicated radio pair (mLRS) or time-slotting
- **Half duplex**: Only one direction at a time; protocol must manage TX/RX switching
- **Hardware dependency**: Requires LoRa radio modules (SX1262, mLRS-compatible boards)
- **Regulatory complexity**: Different bands have different rules per region; must track duty cycle
- **Two feature flags**: Slightly more complex crate than single-backend transports

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| mLRS serial protocol changes | Low | Medium | Pin to specific mLRS version; serial passthrough is stable |
| Radio interference in contested spectrum | Medium | Medium | Frequency hopping (mLRS built-in); adaptive spreading factor |
| Duty cycle limits reduce effective throughput | Medium (EU only) | Low | Prioritize critical messages; queue non-urgent; use 2.4 GHz for higher throughput |
| SX1262 driver instability on ESP32 | Medium | Medium | `lora-phy` crate is actively maintained; fallback to mLRS serial |
| Range below theoretical maximum | High | Low | Use conservative range estimates; test in target terrain |
| Fragment loss on unreliable link | Medium | Medium | Fragment-level retry with configurable timeout; group-level ACK |

---

## Alternatives Considered

### Option 1: Separate Crates (hive-lora-mlrs + hive-lora-direct)
**Pros**: Cleaner dependency separation, each crate is simpler
**Cons**: Duplicates frame format, fragmentation, transport trait implementation; two crates to version and publish
**Decision**: Single crate with feature flags avoids duplication and ensures frame format consistency.

### Option 2: LoRaWAN (The Things Network / Helium)
**Pros**: Established ecosystem, public network coverage in urban areas
**Cons**: Uplink-dominant design (poor for bidirectional HIVE sync), 51-byte payload limit (SF12), requires LoRaWAN gateways (infrastructure dependency), class A latency of 1-5 seconds, not suitable for peer-to-peer tactical use
**Decision**: LoRaWAN's gateway-centric architecture and tiny payloads are fundamentally misaligned with HIVE's peer-to-peer model. Raw LoRa (via mLRS or direct) provides the full 252-byte frame and true peer-to-peer operation.

### Option 3: Meshtastic
**Pros**: Popular open-source LoRa mesh, large community, built-in mesh routing
**Cons**: Opinionated protocol (protobuf-based, own message types), not a transparent bridge, mesh routing adds latency and complexity, would require adapting HIVE protocol to Meshtastic's message format rather than using native eche-lite framing
**Decision**: mLRS's transparent serial passthrough lets HIVE use its own frame format directly. Meshtastic's mesh layer would conflict with HIVE's own routing and sync logic.

### Option 4: Integrate into hive-btle
**Pros**: Reuse existing external transport infrastructure
**Cons**: BLE and LoRa are fundamentally different radios with different APIs, ranges, data rates, and regulatory requirements; would bloat hive-btle with unrelated radio code; different dependency trees (BlueZ/CoreBluetooth vs tokio-serial/lora-phy)
**Decision**: Separate crate follows the established pattern of one transport per crate.

---

## References

1. [mLRS — Mavlink LoRa System](https://github.com/olliw42/mLRS) - Open-source LoRa firmware for transparent serial bridge
2. [lora-phy Rust crate](https://crates.io/crates/lora-phy) - Rust driver for Semtech SX1261/SX1262 LoRa transceivers
3. [Semtech SX1262 Datasheet](https://www.semtech.com/products/wireless-rf/lora-connect/sx1262) - Target LoRa transceiver
4. [LoRa Alliance — LoRa Technology Overview](https://lora-alliance.org/about-lorawan/) - LoRa modulation background
5. ADR-032: Pluggable Transport Abstraction
6. ADR-035: HIVE-Lite Embedded Nodes (eche-lite protocol)
7. ADR-039: HIVE-BTLE Mesh Transport Crate
8. ADR-041: Multi-Transport Embedded Integration
9. ADR-051: HIVE-SBD Satellite Transport
10. [MAVLink Protocol](https://mavlink.io/en/) - Standard UAS telemetry protocol (mLRS's native payload)
11. [hive-btle on Radicle](https://app.radicle.xyz/nodes/rosa.radicle.xyz/rad%3Az458mp9Um3AYNQQFMdHaNEUtmiohq) - External transport crate pattern
12. CAP Protocol Technology Deep Dive (ROS-CAP Bridge, MAVLink/mavros integration)

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-25 | Proposed ADR-052 | Need long-range transport (7-87 km) to fill gap between BLE (400m) and SBD (global) |
| 2026-02-25 | Single crate with feature flags | Avoids duplicating frame format and transport logic across two crates |
| 2026-02-25 | mLRS as primary serial bridge | Open-source, transparent serial passthrough, proven 87 km range, no protocol adaptation needed |
| 2026-02-25 | Direct LoRa via lora-phy for embedded | Enables ESP32+SX1262 nodes without separate mLRS hardware |
| 2026-02-25 | Point-to-point topology (not mesh) | LoRa is half-duplex; mesh adds latency and complexity; gateway bridge to IP mesh is cleaner (ADR-041) |
| 2026-02-25 | ChaCha20-Poly1305 app-layer encryption | Consistent with HIVE security model; supplements mLRS link-layer AES |

---

**Next Steps:**
1. Review and approve ADR
2. Create `hive-lora` Radicle repository
3. Phase 1: Frame format + core types (pure Rust, no hardware)
4. Acquire mLRS-compatible modules (SX1262-based) for hardware-in-the-loop testing
5. Phase 2: mLRS serial link integration on Linux
6. Phase 4: ESP32 + SX1262 direct radio integration

**Radicle:**
- Create `rad:z...` for hive-lora (pending approval)
