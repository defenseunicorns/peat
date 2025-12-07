# ADR-032: Pluggable Transport Abstraction for Multi-Network Operations

**Status**: Proposed
**Date**: 2025-12-07
**Authors**: Kit Plummer, Claude
**Relates to**: ADR-011 (Automerge + Iroh), ADR-017 (P2P Mesh Management), ADR-030 (Multi-Interface Transport)
**Implements**: Issue #255

---

## Context

### The Multi-Transport Reality

Tactical edge environments operate across diverse communication channels simultaneously:

| Transport | Bandwidth | Latency | Range | Power | Use Case |
|-----------|-----------|---------|-------|-------|----------|
| QUIC/Iroh | 1-100 Mbps | 1-50ms | Unlimited (IP) | Low | Primary mesh |
| WiFi Direct | 250 Mbps | 5-20ms | 200m | Medium | Peer-to-peer clusters |
| Bluetooth LE | 2 Mbps | 10-100ms | 100m | Very Low | Device pairing, beacons |
| LoRa | 0.3-50 kbps | 100-2000ms | 15km+ | Minimal | Long-range telemetry |
| Tactical Radio | 9.6-2000 kbps | 50-500ms | Variable | High | Secure MANET |
| Starlink | 50-200 Mbps | 20-40ms | Global | High | Backhaul to C2 |

**Critical Requirement**: A single HIVE node must be able to use multiple transports simultaneously, selecting the optimal one based on message requirements and current network conditions.

### Current Implementation Gap

**What exists today:**
- `MeshTransport` trait in `hive-protocol/src/transport/mod.rs`
- `IrohMeshTransport` implementing QUIC-based transport
- `DittoMeshTransport` delegating to Ditto's transport
- `HealthMonitor` for connection quality tracking

**What's missing:**
- Transport capability declaration (bandwidth, latency, range, power)
- Transport selection based on message requirements
- Multi-transport coordination (TransportManager)
- Pluggable transport implementations for non-IP networks
- Fallback and handoff between transports

### ADR-030 vs ADR-032

**ADR-030** solved: "How do we use multiple network interfaces (NICs) with the same transport protocol?"
- Answer: Iroh automatically binds to all interfaces

**ADR-032** solves: "How do we use fundamentally different transport protocols simultaneously?"
- Bluetooth vs QUIC vs LoRa are not just different interfaces - they have different APIs, semantics, and capabilities

---

## Decision Drivers

### Requirements

1. **Transport Independence**: Mesh logic must not depend on specific transport implementation
2. **Capability Awareness**: System must know what each transport can do (bandwidth, latency, range)
3. **Dynamic Selection**: Choose transport based on message size, urgency, and current conditions
4. **Graceful Degradation**: Fallback to alternative transport when primary fails
5. **Battery Efficiency**: Consider power consumption for mobile/tactical platforms
6. **Range Optimization**: Use appropriate transport for peer distance
7. **Security Parity**: All transports must support authentication/encryption

### Constraints

1. **Existing Trait**: Must extend or wrap existing `MeshTransport` trait
2. **Feature Flags**: Transport implementations behind cargo features
3. **Platform Support**: Some transports only available on specific platforms (BLE on mobile)
4. **No Breaking Changes**: Current `IrohMeshTransport` users unaffected

---

## Decision

### 1. Enhanced Transport Trait Hierarchy

```rust
/// Transport capability declaration
#[derive(Debug, Clone)]
pub struct TransportCapabilities {
    /// Transport type identifier
    pub transport_type: TransportType,

    /// Maximum bandwidth in bytes/second (0 = unknown)
    pub max_bandwidth_bps: u64,

    /// Typical latency in milliseconds
    pub typical_latency_ms: u32,

    /// Maximum practical range in meters (0 = unlimited/IP)
    pub max_range_meters: u32,

    /// Supports bidirectional streams
    pub bidirectional: bool,

    /// Supports reliable delivery (vs best-effort)
    pub reliable: bool,

    /// Battery impact score (0-100, 100 = high drain)
    pub battery_impact: u8,

    /// Supports broadcast/multicast
    pub supports_broadcast: bool,

    /// Requires pairing/bonding before use
    pub requires_pairing: bool,

    /// Maximum message size in bytes (0 = unlimited)
    pub max_message_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    /// QUIC over IP (Iroh)
    Quic,
    /// Classic Bluetooth (RFCOMM)
    BluetoothClassic,
    /// Bluetooth Low Energy (GATT)
    BluetoothLE,
    /// WiFi Direct (P2P)
    WifiDirect,
    /// LoRa (long range, low power)
    LoRa,
    /// Tactical radio (MANET)
    TacticalRadio,
    /// Satellite (Starlink, Iridium)
    Satellite,
    /// Custom/Vendor-specific
    Custom(u32),
}

/// Extended transport trait with capability advertisement
#[async_trait]
pub trait Transport: MeshTransport {
    /// Get transport capabilities
    fn capabilities(&self) -> &TransportCapabilities;

    /// Check if transport is currently available/enabled
    fn is_available(&self) -> bool;

    /// Get current signal quality (0-100, for wireless transports)
    fn signal_quality(&self) -> Option<u8> {
        None  // Default for wired/IP transports
    }

    /// Estimate if peer is reachable via this transport
    fn can_reach(&self, peer_id: &NodeId) -> bool;

    /// Get estimated delivery time for message of given size
    fn estimate_delivery_ms(&self, message_size: usize) -> u32 {
        let caps = self.capabilities();
        let transfer_time = if caps.max_bandwidth_bps > 0 {
            (message_size as u64 * 1000 / caps.max_bandwidth_bps) as u32
        } else {
            0
        };
        caps.typical_latency_ms + transfer_time
    }
}
```

### 2. Message Requirements for Transport Selection

```rust
/// Requirements for message delivery
#[derive(Debug, Clone, Default)]
pub struct MessageRequirements {
    /// Minimum required bandwidth (bytes/second)
    pub min_bandwidth_bps: u64,

    /// Maximum acceptable latency (ms)
    pub max_latency_ms: Option<u32>,

    /// Message size in bytes (for capacity checking)
    pub message_size: usize,

    /// Requires reliable delivery
    pub reliable: bool,

    /// Priority level (higher = more important)
    pub priority: MessagePriority,

    /// Prefer low power consumption
    pub power_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MessagePriority {
    /// Background sync, can use any available transport
    Background = 0,
    /// Normal operational messages
    #[default]
    Normal = 1,
    /// Time-sensitive, prefer low-latency transports
    High = 2,
    /// Emergency/critical, use fastest available path
    Critical = 3,
}
```

### 3. Transport Manager

```rust
/// Manages multiple transports and handles transport selection
pub struct TransportManager {
    /// Registered transports by type
    transports: HashMap<TransportType, Arc<dyn Transport>>,

    /// Transport preference order (user-configured)
    preference_order: Vec<TransportType>,

    /// Active transport per peer (learned from successful deliveries)
    peer_transports: RwLock<HashMap<NodeId, TransportType>>,

    /// Health monitor for transport quality tracking
    health_monitor: Arc<HealthMonitor>,

    /// Configuration
    config: TransportManagerConfig,
}

impl TransportManager {
    /// Register a transport
    pub fn register(&mut self, transport: Arc<dyn Transport>) {
        let transport_type = transport.capabilities().transport_type;
        self.transports.insert(transport_type, transport);
    }

    /// Remove a transport
    pub fn unregister(&mut self, transport_type: TransportType) -> Option<Arc<dyn Transport>> {
        self.transports.remove(&transport_type)
    }

    /// Get available transports for a peer
    pub fn available_transports(&self, peer_id: &NodeId) -> Vec<TransportType> {
        self.transports
            .iter()
            .filter(|(_, t)| t.is_available() && t.can_reach(peer_id))
            .map(|(tt, _)| *tt)
            .collect()
    }

    /// Select best transport for message
    pub fn select_transport(
        &self,
        peer_id: &NodeId,
        requirements: &MessageRequirements,
    ) -> Option<TransportType> {
        let available = self.available_transports(peer_id);

        // Filter by requirements
        let candidates: Vec<_> = available
            .into_iter()
            .filter_map(|tt| {
                let transport = self.transports.get(&tt)?;
                let caps = transport.capabilities();

                // Check hard requirements
                if requirements.reliable && !caps.reliable {
                    return None;
                }
                if caps.max_bandwidth_bps > 0 && caps.max_bandwidth_bps < requirements.min_bandwidth_bps {
                    return None;
                }
                if caps.max_message_size > 0 && caps.max_message_size < requirements.message_size {
                    return None;
                }

                // Calculate score (higher = better)
                let mut score = 100i32;

                // Prefer faster transports for high-priority messages
                if requirements.priority >= MessagePriority::High {
                    score += 50 - (caps.typical_latency_ms.min(50) as i32);
                }

                // Penalize high power consumption if power-sensitive
                if requirements.power_sensitive {
                    score -= caps.battery_impact as i32;
                }

                // Check latency requirement
                let est_delivery = transport.estimate_delivery_ms(requirements.message_size);
                if let Some(max_latency) = requirements.max_latency_ms {
                    if est_delivery > max_latency {
                        return None; // Can't meet latency requirement
                    }
                }

                // Bonus for user preference order
                if let Some(pref_idx) = self.preference_order.iter().position(|&t| t == tt) {
                    score += 20 - (pref_idx as i32 * 5);
                }

                Some((tt, score))
            })
            .collect();

        // Return highest-scoring transport
        candidates
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .map(|(tt, _)| tt)
    }

    /// Send message via appropriate transport
    pub async fn send(
        &self,
        peer_id: &NodeId,
        data: &[u8],
        requirements: MessageRequirements,
    ) -> Result<(), TransportError> {
        let transport_type = self
            .select_transport(peer_id, &requirements)
            .ok_or_else(|| TransportError::PeerNotFound(peer_id.to_string()))?;

        let transport = self.transports.get(&transport_type)
            .ok_or_else(|| TransportError::NotStarted)?;

        // Connect and send
        let conn = transport.connect(peer_id).await?;
        // Note: Actual send implementation depends on MeshConnection extension

        // Remember successful transport for this peer
        self.peer_transports.write().unwrap().insert(peer_id.clone(), transport_type);

        Ok(())
    }
}
```

### 4. Transport Implementation Examples

#### Bluetooth LE Transport (Conceptual)

```rust
#[cfg(feature = "bluetooth")]
pub struct BluetoothLETransport {
    capabilities: TransportCapabilities,
    adapter: BluetoothAdapter,
    connections: RwLock<HashMap<NodeId, GattConnection>>,
    service_uuid: Uuid,
}

impl BluetoothLETransport {
    pub fn new(adapter: BluetoothAdapter) -> Self {
        Self {
            capabilities: TransportCapabilities {
                transport_type: TransportType::BluetoothLE,
                max_bandwidth_bps: 250_000,  // ~2 Mbps theoretical
                typical_latency_ms: 30,
                max_range_meters: 100,
                bidirectional: true,
                reliable: true,
                battery_impact: 15,  // BLE is very efficient
                supports_broadcast: true,  // Advertising
                requires_pairing: false,   // Can use just-works or no pairing
                max_message_size: 512,     // MTU limit per characteristic
            },
            adapter,
            connections: RwLock::new(HashMap::new()),
            service_uuid: Uuid::parse_str("HIVE-SERVICE-UUID").unwrap(),
        }
    }
}

#[async_trait]
impl Transport for BluetoothLETransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    fn is_available(&self) -> bool {
        self.adapter.is_powered_on()
    }

    fn signal_quality(&self) -> Option<u8> {
        // Return average RSSI across connections
        Some(75) // Placeholder
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        // Check if peer is in BLE range (via scan or cached info)
        self.connections.read().unwrap().contains_key(peer_id)
    }
}
```

#### LoRa Transport (Conceptual)

```rust
#[cfg(feature = "lora")]
pub struct LoRaTransport {
    capabilities: TransportCapabilities,
    radio: LoRaRadio,
    spreading_factor: u8,
}

impl LoRaTransport {
    pub fn new(radio: LoRaRadio, spreading_factor: u8) -> Self {
        // Higher SF = longer range, lower bandwidth
        let bandwidth = match spreading_factor {
            7 => 21_900,   // ~21.9 kbps
            8 => 12_500,   // ~12.5 kbps
            9 => 7_000,    // ~7 kbps
            10 => 3_900,   // ~3.9 kbps
            11 => 2_100,   // ~2.1 kbps
            12 => 1_100,   // ~1.1 kbps
            _ => 5_000,    // Default
        };

        Self {
            capabilities: TransportCapabilities {
                transport_type: TransportType::LoRa,
                max_bandwidth_bps: bandwidth,
                typical_latency_ms: 500,
                max_range_meters: 15_000,  // 15km typical
                bidirectional: true,
                reliable: false,  // Best-effort (can add ACKs)
                battery_impact: 10,
                supports_broadcast: true,
                requires_pairing: false,
                max_message_size: 255,  // LoRa packet size limit
            },
            radio,
            spreading_factor,
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Core Abstractions (Issue #255)

**Goal**: Define traits and refactor IrohMeshTransport

- [ ] Define `TransportCapabilities` struct
- [ ] Define `TransportType` enum
- [ ] Define `Transport` trait extending `MeshTransport`
- [ ] Define `MessageRequirements` and `MessagePriority`
- [ ] Implement `Transport` for `IrohMeshTransport`
- [ ] Add capability constants for QUIC transport
- [ ] Unit tests for transport selection logic

**Estimated scope**: ~500 lines new code, ~100 lines modifications

### Phase 2: Transport Manager

**Goal**: Multi-transport coordination

- [ ] Implement `TransportManager` struct
- [ ] Implement transport registration/unregistration
- [ ] Implement transport selection algorithm
- [ ] Add peer-transport affinity tracking
- [ ] Integrate with existing `MeshRouter`
- [ ] Add fallback logic when primary transport fails

**Estimated scope**: ~800 lines

### Phase 3: Bluetooth Transport (Separate Issue)

**Goal**: Android/iOS peer discovery and messaging

- [ ] Create `hive-bluetooth` crate (behind feature flag)
- [ ] Implement `BluetoothLETransport`
- [ ] Implement BLE advertising for peer discovery
- [ ] Implement GATT service for data exchange
- [ ] Handle pairing and bonding
- [ ] Platform-specific implementations (Android NDK, iOS)

**Estimated scope**: ~2000 lines + platform-specific code

### Phase 4: LoRa Transport (Separate Issue)

**Goal**: Long-range, low-power telemetry

- [ ] Create `hive-lora` crate (behind feature flag)
- [ ] Implement `LoRaTransport`
- [ ] Handle spreading factor selection
- [ ] Implement duty cycle management
- [ ] Add CAD (channel activity detection)
- [ ] Define compact message format for low bandwidth

**Estimated scope**: ~1500 lines

### Phase 5: WiFi Direct Transport (Separate Issue)

**Goal**: High-bandwidth peer-to-peer clusters

- [ ] Create `hive-wifi-direct` crate
- [ ] Implement `WifiDirectTransport`
- [ ] Handle group formation (GO negotiation)
- [ ] Integrate with IP-based QUIC after connection

**Estimated scope**: ~1200 lines

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Application Layer                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                      │
│  │ CRDT Sync   │  │ Telemetry   │  │ Commands    │                      │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                      │
│         │                │                │                              │
│         └────────────────┼────────────────┘                              │
│                          ▼                                               │
│         ┌────────────────────────────────┐                               │
│         │        Transport Manager        │ ◄── Transport Selection      │
│         │   (Multi-Transport Coordinator) │     Message Requirements     │
│         └──────────────┬─────────────────┘                               │
│                        │                                                 │
├────────────────────────┼────────────────────────────────────────────────┤
│                Transport Abstraction Layer                               │
│                        │                                                 │
│         ┌──────────────┴──────────────┐                                  │
│         ▼              ▼              ▼              ▼                   │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐            │
│  │   QUIC     │ │ Bluetooth  │ │   LoRa     │ │ WiFi Direct│            │
│  │  (Iroh)    │ │    LE      │ │            │ │            │            │
│  └─────┬──────┘ └─────┬──────┘ └─────┬──────┘ └─────┬──────┘            │
│        │              │              │              │                    │
├────────┼──────────────┼──────────────┼──────────────┼───────────────────┤
│        ▼              ▼              ▼              ▼                    │
│  ┌──────────────────────────────────────────────────────────┐           │
│  │                    Physical Layer                          │           │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐       │           │
│  │  │ Ethernet│  │  BLE    │  │ LoRa    │  │  WiFi   │       │           │
│  │  │  NIC    │  │ Adapter │  │ Radio   │  │ Adapter │       │           │
│  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘       │           │
│  └──────────────────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Transport Selection Algorithm

```
FUNCTION select_transport(peer_id, requirements):
    available = []

    FOR each transport in registered_transports:
        IF transport.is_available() AND transport.can_reach(peer_id):
            IF meets_requirements(transport, requirements):
                score = calculate_score(transport, requirements)
                available.append((transport, score))

    IF available.is_empty():
        RETURN None

    RETURN transport with highest score

FUNCTION meets_requirements(transport, requirements):
    caps = transport.capabilities()

    # Hard requirements - must be met
    IF requirements.reliable AND NOT caps.reliable:
        RETURN False
    IF caps.max_bandwidth < requirements.min_bandwidth:
        RETURN False
    IF caps.max_message_size > 0 AND caps.max_message_size < requirements.message_size:
        RETURN False
    IF requirements.max_latency IS SET:
        est_delivery = transport.estimate_delivery_ms(requirements.message_size)
        IF est_delivery > requirements.max_latency:
            RETURN False

    RETURN True

FUNCTION calculate_score(transport, requirements):
    caps = transport.capabilities()
    score = 100

    # Latency bonus for high-priority messages
    IF requirements.priority >= High:
        score += 50 - min(caps.typical_latency_ms, 50)

    # Power penalty if power-sensitive
    IF requirements.power_sensitive:
        score -= caps.battery_impact

    # Preference bonus
    IF transport.type in preference_order:
        idx = preference_order.index(transport.type)
        score += 20 - (idx * 5)

    # Signal quality bonus for wireless
    IF transport.signal_quality() IS SET:
        score += transport.signal_quality() / 10

    RETURN score
```

---

## Success Criteria

### Functional Requirements

- [ ] Single node can register multiple transports
- [ ] Transport selection respects message requirements
- [ ] Fallback works when primary transport fails
- [ ] Peer-transport affinity reduces selection overhead
- [ ] All transports share same `NodeId` namespace

### Performance Requirements

- [ ] Transport selection < 1ms
- [ ] No memory overhead when transport not in use
- [ ] Graceful degradation under load

### Testing

- [ ] Unit tests for TransportManager selection logic
- [ ] Integration tests with mock transports
- [ ] E2E test with QUIC + simulated LoRa

---

## Security Considerations

### Per-Transport Security

| Transport | Authentication | Encryption | Notes |
|-----------|---------------|------------|-------|
| QUIC/Iroh | TLS 1.3 | Built-in | Strongest |
| Bluetooth LE | BLE pairing | AES-CCM | Good |
| WiFi Direct | WPS/WPA2 | AES | Good |
| LoRa | Custom | Optional | Add app-layer encryption |
| Tactical Radio | Built-in | MIL-STD | Platform-dependent |

### Recommendations

1. **Enforce encryption at transport manager level** - Reject transports without encryption for sensitive messages
2. **Node identity verification** - Use same keypair across all transports
3. **Message authentication** - Sign messages at application layer (independent of transport)

---

## Open Questions

1. **Message Fragmentation**: How to handle large messages on limited transports (LoRa 255-byte limit)?
   - Option A: Transport-layer fragmentation
   - Option B: Application-layer chunking (recommended)

2. **Discovery Protocol**: How do peers advertise which transports they support?
   - Option A: Extended beacon with transport capabilities
   - Option B: Separate discovery protocol per transport

3. **Connection Handoff**: How to migrate active streams between transports?
   - Option A: Don't migrate - just use best for new messages
   - Option B: QUIC connection migration (only works for IP-based)

4. **Ditto Integration**: How does this work with DittoMeshTransport?
   - Ditto manages its own transport internally
   - May need to expose Ditto as a "black box" transport

---

## References

1. [Iroh Multipath](https://iroh.computer/docs) - Native multi-interface support
2. [QUIC Multipath RFC](https://datatracker.ietf.org/doc/draft-ietf-quic-multipath/)
3. [Bluetooth Core Spec 5.4](https://www.bluetooth.com/specifications/specs/core-specification-5-4/)
4. [LoRa Alliance Specifications](https://lora-alliance.org/resource_hub/lorawan-specification-v1-0-4/)
5. [WiFi Direct P2P Specification](https://www.wi-fi.org/discover-wi-fi/wi-fi-direct)
6. ADR-011: CRDT + Networking Stack Selection
7. ADR-017: P2P Mesh Management and Discovery
8. ADR-030: Multi-Interface Transport

---

**Last Updated**: 2025-12-07
**Status**: PROPOSED - Awaiting discussion
