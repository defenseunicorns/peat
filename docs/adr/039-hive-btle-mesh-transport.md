# ADR-039: HIVE-BTLE Mesh Transport Crate

**Status**: Proposed  
**Date**: 2025-12-13  
**Authors**: Kit Plummer, Codex  
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)  
**Relates To**: ADR-032 (Pluggable Transport Abstraction), ADR-035 (HIVE-Lite Embedded Nodes), ADR-037 (Resource-Constrained Device Optimization), ADR-017 (P2P Mesh Management), ADR-006 (Security)

---

## Executive Summary

This ADR defines the architecture for `hive-btle`, a Rust crate providing Bluetooth Low Energy mesh networking for HIVE Protocol. The crate enables peer-to-peer discovery, advertisement, and connectivity across resource-constrained devices while supporting HIVE-Lite synchronization. It implements configurable Coded PHYs for throughput/range tradeoffs and targets cross-platform deployment on Linux, Android, macOS, iOS, and Windows across x86 and ARM architectures.

---

## Context

### The BLE Mesh Opportunity

Bluetooth Low Energy represents a critical transport for HIVE's tactical edge scenarios:

| Scenario | BLE Advantage | HIVE Use Case |
|----------|---------------|---------------|
| Wearable sync | Ultra-low power (10mW) | WearTAK on Samsung watches |
| Sensor mesh | No infrastructure required | Environmental sensors, asset trackers |
| Device pairing | Ubiquitous hardware support | ATAK ↔ Jetson sync |
| Indoor ops | Works through walls | Building-internal coordination |
| Denied spectrum | Non-standard frequencies | Alternative to WiFi/cellular |

### Customer Pain Point: Ditto Battery Drain

Per ADR-037, Ascent (Alex Gorsuch) identified critical issues with Ditto's BLE implementation on Samsung watches running WearTAK:

```
Ditto BLE Behavior:
├─ Continuous scanning: Radio active 20%+ of time
├─ Gossip-based discovery: Constant advertisements
├─ Full mesh participation: All devices relay everything
└─ Result: 3-4 hour battery life (mission failure)

HIVE-BTLE Target:
├─ Batched sync windows: Radio active <5% of time
├─ Hierarchical discovery: Leaf nodes don't scan
├─ HIVE Lite profile: Minimal state, single parent
└─ Result: 18-24 hour battery life (mission capable)
```

### Why a Dedicated Crate?

Per ADR-032 (Pluggable Transport Abstraction), each transport type requires fundamentally different APIs and semantics. BLE is not just "another socket"—it has:

1. **GATT Service Model**: Characteristic-based data exchange vs stream-based
2. **Advertising/Scanning**: Asymmetric discovery vs symmetric connection
3. **MTU Constraints**: 23-517 byte payloads vs unlimited streams
4. **PHY Selection**: Coded PHY (500kbps/125kbps) vs uncoded (1M/2M)
5. **Connection Events**: Discrete exchange windows vs continuous streams
6. **Power Management**: Radio duty cycle control vs always-on

---

## Decision Drivers

### Requirements

1. **Cross-Platform**: Single codebase targeting Linux, Android, macOS, iOS, Windows
2. **Cross-Architecture**: x86_64 and ARM (aarch64, armv7, armv7-a)
3. **HIVE-Lite Support**: Synchronize minimal CRDT state per ADR-035/037
4. **Coded PHY Support**: Configure LE Coded (S=2, S=8) for range/throughput tradeoffs
5. **Mesh Topology**: P2P discovery, advertisement, and multi-peer connectivity
6. **Power Efficiency**: >50% battery improvement over Ditto baseline
7. **Security**: BLE pairing + application-layer encryption per ADR-006
8. **Integration**: Implement ADR-032 `Transport` trait for TransportManager

### Constraints

1. **BLE 5.0 Minimum**: Coded PHY requires BLE 5.0+ hardware
2. **no_std Optional**: Support embedded targets (ESP32) via feature flag
3. **Platform SDKs**: Must interface with native BLE stacks (BlueZ, CoreBluetooth, WinRT)
4. **MTU Limits**: Design for 23-byte minimum MTU, optimize for 251-byte negotiated MTU
5. **Connection Limits**: BLE Central typically supports 7-10 simultaneous connections

---

## Architecture

### Crate Structure

```
hive-btle/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, feature gates
│   ├── transport.rs        # Transport trait implementation
│   ├── config.rs           # BLE configuration (PHY, intervals, power)
│   │
│   ├── phy/                # Physical layer configuration
│   │   ├── mod.rs
│   │   ├── coded.rs        # LE Coded PHY (S=2, S=8)
│   │   └── uncoded.rs      # LE 1M/2M PHY
│   │
│   ├── discovery/          # Peer discovery
│   │   ├── mod.rs
│   │   ├── advertiser.rs   # Advertisement broadcasting
│   │   ├── scanner.rs      # Passive/active scanning
│   │   └── beacon.rs       # HIVE beacon encoding/decoding
│   │
│   ├── mesh/               # Mesh topology management
│   │   ├── mod.rs
│   │   ├── topology.rs     # Parent/child/peer relationships
│   │   ├── connection.rs   # Connection lifecycle
│   │   └── routing.rs      # Multi-hop message routing
│   │
│   ├── gatt/               # GATT service definition
│   │   ├── mod.rs
│   │   ├── service.rs      # HIVE BLE service
│   │   ├── characteristics.rs
│   │   └── protocol.rs     # Characteristic read/write protocol
│   │
│   ├── sync/               # HIVE-Lite synchronization
│   │   ├── mod.rs
│   │   ├── batch.rs        # Batched sync accumulator
│   │   ├── delta.rs        # Differential sync encoding
│   │   └── crdt.rs         # Minimal CRDT support (LWW, counters)
│   │
│   ├── security/           # Security layer
│   │   ├── mod.rs
│   │   ├── pairing.rs      # BLE pairing modes
│   │   ├── bonding.rs      # Bond storage
│   │   └── encryption.rs   # Application-layer encryption
│   │
│   ├── power/              # Power management
│   │   ├── mod.rs
│   │   ├── profile.rs      # Power profiles (aggressive, balanced, low)
│   │   └── scheduler.rs    # Radio duty cycle scheduling
│   │
│   └── platform/           # Platform-specific implementations
│       ├── mod.rs
│       ├── linux/          # BlueZ via btleplug/bluer
│       ├── android/        # Android BLE via JNI
│       ├── macos/          # CoreBluetooth
│       ├── ios/            # CoreBluetooth
│       ├── windows/        # WinRT BLE
│       └── embedded/       # ESP32 via esp-idf-hal (no_std)
│
├── examples/
│   ├── basic_mesh.rs       # Simple 2-node mesh
│   ├── wearable_sync.rs    # WearTAK-style wearable sync
│   ├── sensor_network.rs   # Multi-sensor mesh
│   └── coded_phy_range.rs  # Range testing with Coded PHY
│
├── benches/
│   ├── throughput.rs       # Bytes/second measurement
│   ├── latency.rs          # RTT measurement
│   └── power.rs            # Power consumption measurement
│
└── tests/
    ├── discovery_tests.rs
    ├── mesh_tests.rs
    └── sync_tests.rs
```

### Core Abstractions

#### 1. BLE Transport (ADR-032 Integration)

```rust
// File: src/transport.rs

use hive_protocol::transport::{
    Transport, TransportCapabilities, TransportType, 
    MessageRequirements, MeshConnection
};

/// Bluetooth LE transport implementing HIVE Transport trait
pub struct BluetoothLETransport {
    config: BleConfig,
    adapter: BleAdapter,
    connections: RwLock<HashMap<NodeId, BleConnection>>,
    discovery: Arc<DiscoveryManager>,
    mesh: Arc<MeshManager>,
    gatt_server: Arc<GattServer>,
}

impl BluetoothLETransport {
    pub async fn new(config: BleConfig) -> Result<Self, BleError> {
        let adapter = BleAdapter::new(&config).await?;
        let discovery = DiscoveryManager::new(config.discovery.clone());
        let mesh = MeshManager::new(config.mesh.clone());
        let gatt_server = GattServer::new(config.gatt.clone()).await?;
        
        Ok(Self {
            config,
            adapter,
            connections: RwLock::new(HashMap::new()),
            discovery: Arc::new(discovery),
            mesh: Arc::new(mesh),
            gatt_server: Arc::new(gatt_server),
        })
    }
    
    pub fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            transport_type: TransportType::BluetoothLE,
            max_bandwidth_bps: self.config.phy.max_bandwidth(),
            typical_latency_ms: self.config.phy.typical_latency(),
            max_range_meters: self.config.phy.max_range(),
            bidirectional: true,
            reliable: true,
            battery_impact: self.config.power_profile.battery_impact(),
            supports_broadcast: true,  // Advertising
            requires_pairing: self.config.security.requires_pairing,
            max_message_size: self.config.gatt.max_mtu as usize,
        }
    }
}

#[async_trait]
impl Transport for BluetoothLETransport {
    fn capabilities(&self) -> &TransportCapabilities {
        &self.cached_capabilities
    }

    fn is_available(&self) -> bool {
        self.adapter.is_powered()
    }

    fn signal_quality(&self) -> Option<u8> {
        // Average RSSI across connections, normalized to 0-100
        self.connections.read().ok()
            .map(|conns| average_rssi_normalized(&conns))
    }

    fn can_reach(&self, peer_id: &NodeId) -> bool {
        self.discovery.has_seen_peer(peer_id) ||
        self.connections.read().ok()
            .map(|c| c.contains_key(peer_id))
            .unwrap_or(false)
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnection>, TransportError> {
        let conn = self.mesh.connect(peer_id).await?;
        self.connections.write().await.insert(peer_id.clone(), conn.clone());
        Ok(Box::new(conn))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<(), TransportError> {
        if let Some(conn) = self.connections.write().await.remove(peer_id) {
            conn.close().await?;
        }
        Ok(())
    }
}
```

#### 2. PHY Configuration (Coded PHY Support)

```rust
// File: src/phy/mod.rs

/// BLE Physical Layer configuration
#[derive(Debug, Clone)]
pub enum BlePhy {
    /// LE 1M PHY (default, 1 Mbps)
    Le1M,
    /// LE 2M PHY (high throughput, 2 Mbps, shorter range)
    Le2M,
    /// LE Coded PHY S=2 (500 kbps, ~2x range)
    LeCodedS2,
    /// LE Coded PHY S=8 (125 kbps, ~4x range)
    LeCodedS8,
}

impl BlePhy {
    /// Maximum theoretical bandwidth in bits per second
    pub fn max_bandwidth(&self) -> u32 {
        match self {
            BlePhy::Le1M => 1_000_000,
            BlePhy::Le2M => 2_000_000,
            BlePhy::LeCodedS2 => 500_000,
            BlePhy::LeCodedS8 => 125_000,
        }
    }
    
    /// Typical latency in milliseconds
    pub fn typical_latency(&self) -> u32 {
        match self {
            BlePhy::Le1M => 30,
            BlePhy::Le2M => 20,
            BlePhy::LeCodedS2 => 50,
            BlePhy::LeCodedS8 => 100,
        }
    }
    
    /// Maximum effective range in meters (line-of-sight)
    pub fn max_range(&self) -> u32 {
        match self {
            BlePhy::Le1M => 100,
            BlePhy::Le2M => 50,
            BlePhy::LeCodedS2 => 200,
            BlePhy::LeCodedS8 => 400,
        }
    }
    
    /// Requires BLE 5.0+
    pub fn requires_ble5(&self) -> bool {
        matches!(self, BlePhy::Le2M | BlePhy::LeCodedS2 | BlePhy::LeCodedS8)
    }
}

/// PHY selection strategy
#[derive(Debug, Clone)]
pub enum PhyStrategy {
    /// Fixed PHY for all connections
    Fixed(BlePhy),
    /// Adaptive based on RSSI
    Adaptive {
        rssi_threshold_high: i8,   // Switch to 2M above this
        rssi_threshold_low: i8,    // Switch to Coded below this
        hysteresis_db: u8,         // Prevent oscillation
    },
    /// Range-optimized (always use Coded S=8)
    MaxRange,
    /// Throughput-optimized (always use 2M)
    MaxThroughput,
}
```

#### 3. Mesh Discovery and Topology

```rust
// File: src/discovery/mod.rs

use crate::phy::BlePhy;

/// HIVE BLE beacon format (fits in BLE advertisement payload)
#[derive(Debug, Clone)]
pub struct HiveBeacon {
    /// HIVE protocol version (4 bits)
    pub version: u8,
    /// Node capabilities flags (12 bits)
    pub capabilities: NodeCapabilities,
    /// Truncated node ID (32 bits)
    pub node_id_short: u32,
    /// Hierarchy level (8 bits)
    pub hierarchy_level: HierarchyLevel,
    /// Geohash (24 bits, 6-character precision)
    pub geohash: u32,
    /// Battery level (8 bits, 0-100%)
    pub battery_percent: u8,
    /// Sequence number for deduplication (16 bits)
    pub seq_num: u16,
}

impl HiveBeacon {
    /// Encode to BLE advertisement data (max 31 bytes legacy, 254 bytes extended)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16);
        // HIVE service UUID (16 bits)
        buf.extend_from_slice(&HIVE_SERVICE_UUID_SHORT);
        // Packed beacon data (13 bytes)
        buf.push((self.version << 4) | ((self.capabilities.bits() >> 8) as u8 & 0x0F));
        buf.push(self.capabilities.bits() as u8);
        buf.extend_from_slice(&self.node_id_short.to_le_bytes());
        buf.push(self.hierarchy_level as u8);
        buf.extend_from_slice(&self.geohash.to_le_bytes()[..3]);
        buf.push(self.battery_percent);
        buf.extend_from_slice(&self.seq_num.to_le_bytes());
        buf
    }
    
    pub fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        // ... decode implementation
    }
}

/// Discovery manager for BLE peer discovery
pub struct DiscoveryManager {
    config: DiscoveryConfig,
    seen_peers: RwLock<HashMap<NodeId, PeerInfo>>,
    event_tx: mpsc::Sender<DiscoveryEvent>,
}

impl DiscoveryManager {
    /// Start passive scanning (low power)
    pub async fn start_passive_scan(&self) -> Result<(), BleError> {
        // Passive scan: no scan requests, just listen for advertisements
        self.adapter.start_scan(ScanConfig {
            active: false,
            interval_ms: self.config.scan_interval_ms,
            window_ms: self.config.scan_window_ms,
            filter_duplicates: true,
            filter_uuids: vec![HIVE_SERVICE_UUID],
        }).await
    }
    
    /// Start active scanning (discovers more peers, higher power)
    pub async fn start_active_scan(&self) -> Result<(), BleError> {
        // Active scan: send scan requests, get scan responses
        self.adapter.start_scan(ScanConfig {
            active: true,
            interval_ms: self.config.scan_interval_ms,
            window_ms: self.config.scan_window_ms,
            filter_duplicates: false,  // Track RSSI changes
            filter_uuids: vec![HIVE_SERVICE_UUID],
        }).await
    }
    
    /// Start advertising our beacon
    pub async fn start_advertising(&self, beacon: &HiveBeacon) -> Result<(), BleError> {
        let adv_data = AdvertisementData {
            service_uuids: vec![HIVE_SERVICE_UUID],
            manufacturer_data: Some((HIVE_COMPANY_ID, beacon.encode())),
            connectable: true,
            tx_power_level: Some(self.config.tx_power),
        };
        
        self.adapter.start_advertising(adv_data, AdvertisingConfig {
            interval_ms: self.config.adv_interval_ms,
            phy: self.config.adv_phy.clone(),
        }).await
    }
}
```

#### 4. GATT Service Definition

```rust
// File: src/gatt/service.rs

/// HIVE BLE GATT Service definition
/// 
/// Service UUID: 0xHIVE (custom 128-bit UUID)
/// 
/// Characteristics:
/// - Node Info (read): Node identity and capabilities
/// - Sync State (read/notify): Current CRDT state vector
/// - Sync Data (write/indicate): Incoming sync data
/// - Command (write): Control commands
/// - Status (read/notify): Connection status
pub struct HiveGattService {
    service_uuid: Uuid,
    characteristics: Vec<Characteristic>,
}

impl HiveGattService {
    pub fn new() -> Self {
        Self {
            service_uuid: HIVE_SERVICE_UUID,
            characteristics: vec![
                Characteristic {
                    uuid: CHAR_NODE_INFO_UUID,
                    properties: CharProperties::READ,
                    permissions: CharPermissions::READ_ENCRYPTED,
                    value: None,  // Dynamic
                },
                Characteristic {
                    uuid: CHAR_SYNC_STATE_UUID,
                    properties: CharProperties::READ | CharProperties::NOTIFY,
                    permissions: CharPermissions::READ_ENCRYPTED,
                    value: None,
                },
                Characteristic {
                    uuid: CHAR_SYNC_DATA_UUID,
                    properties: CharProperties::WRITE | CharProperties::INDICATE,
                    permissions: CharPermissions::WRITE_ENCRYPTED,
                    value: None,
                },
                Characteristic {
                    uuid: CHAR_COMMAND_UUID,
                    properties: CharProperties::WRITE,
                    permissions: CharPermissions::WRITE_ENCRYPTED,
                    value: None,
                },
                Characteristic {
                    uuid: CHAR_STATUS_UUID,
                    properties: CharProperties::READ | CharProperties::NOTIFY,
                    permissions: CharPermissions::READ_ENCRYPTED,
                    value: None,
                },
            ],
        }
    }
}

/// HIVE Sync Protocol over GATT
/// 
/// Uses chunked transfer for payloads > MTU
pub struct GattSyncProtocol {
    mtu: usize,
    pending_tx: VecDeque<SyncChunk>,
    pending_rx: HashMap<u32, PartialMessage>,
}

/// Chunk header for multi-MTU messages
#[repr(C, packed)]
struct ChunkHeader {
    /// Message ID (for reassembly)
    message_id: u32,
    /// Chunk index (0-based)
    chunk_index: u16,
    /// Total chunks
    total_chunks: u16,
    /// Payload follows
}

impl GattSyncProtocol {
    /// Chunk a message for transmission
    pub fn chunk_message(&self, message: &[u8]) -> Vec<SyncChunk> {
        let payload_size = self.mtu - std::mem::size_of::<ChunkHeader>();
        let total_chunks = (message.len() + payload_size - 1) / payload_size;
        let message_id = rand::random();
        
        message.chunks(payload_size)
            .enumerate()
            .map(|(i, chunk)| SyncChunk {
                header: ChunkHeader {
                    message_id,
                    chunk_index: i as u16,
                    total_chunks: total_chunks as u16,
                },
                payload: chunk.to_vec(),
            })
            .collect()
    }
    
    /// Reassemble chunks into complete message
    pub fn receive_chunk(&mut self, chunk: SyncChunk) -> Option<Vec<u8>> {
        let entry = self.pending_rx
            .entry(chunk.header.message_id)
            .or_insert_with(|| PartialMessage::new(chunk.header.total_chunks));
        
        entry.add_chunk(chunk);
        
        if entry.is_complete() {
            let msg = self.pending_rx.remove(&chunk.header.message_id)?;
            Some(msg.assemble())
        } else {
            None
        }
    }
}
```

#### 5. HIVE-Lite Sync Support

```rust
// File: src/sync/mod.rs

use hive_lite::{LiteCrdt, LwwRegister, GCounter};

/// Batched sync accumulator for power efficiency
/// 
/// Collects changes over sync window, merges redundant updates
pub struct BatchAccumulator {
    config: BatchConfig,
    pending_changes: Vec<CrdtOperation>,
    last_sync: Instant,
    bytes_accumulated: usize,
}

impl BatchAccumulator {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            pending_changes: Vec::new(),
            last_sync: Instant::now(),
            bytes_accumulated: 0,
        }
    }
    
    /// Add a change to the batch
    pub fn add_change(&mut self, op: CrdtOperation) -> SyncTrigger {
        self.pending_changes.push(op.clone());
        self.bytes_accumulated += op.encoded_size();
        
        // Check for immediate sync triggers
        if op.is_critical() {
            return SyncTrigger::Immediate;
        }
        
        if self.bytes_accumulated >= self.config.max_batch_bytes {
            return SyncTrigger::BatchFull;
        }
        
        if self.last_sync.elapsed() >= self.config.max_batch_duration {
            return SyncTrigger::TimeoutReached;
        }
        
        SyncTrigger::None
    }
    
    /// Get pending changes and reset batch
    pub fn flush(&mut self) -> Vec<CrdtOperation> {
        self.last_sync = Instant::now();
        self.bytes_accumulated = 0;
        std::mem::take(&mut self.pending_changes)
    }
}

/// Differential sync encoder
/// 
/// Encodes only changed state, not full document
pub struct DeltaEncoder {
    last_sent_state: HashMap<String, Vec<u8>>,
}

impl DeltaEncoder {
    /// Encode changes since last sync
    pub fn encode_delta<T: LiteCrdt>(
        &mut self, 
        key: &str, 
        current: &T
    ) -> Option<DeltaPayload> {
        let current_bytes = current.encode();
        
        match self.last_sent_state.get(key) {
            Some(last) if last == &current_bytes => None,  // No change
            _ => {
                self.last_sent_state.insert(key.to_string(), current_bytes.clone());
                Some(DeltaPayload {
                    key: key.to_string(),
                    value: current_bytes,
                    timestamp: SystemTime::now(),
                })
            }
        }
    }
}

/// HIVE-Lite sync state for BLE transport
pub struct BleHiveLiteSync {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    batch_accumulator: BatchAccumulator,
    delta_encoder: DeltaEncoder,
    /// Own state (position, health, alerts)
    own_state: LiteNodeState,
    /// Parent's sync vector (for resumption)
    parent_vector_clock: VectorClock,
}

impl BleHiveLiteSync {
    /// Process incoming sync message from parent
    pub async fn receive_sync(&mut self, msg: SyncMessage) -> Result<(), SyncError> {
        // Merge CRDT state
        for (key, value) in msg.state {
            self.own_state.merge_remote(key, value)?;
        }
        
        // Update vector clock
        self.parent_vector_clock.merge(&msg.vector_clock);
        
        Ok(())
    }
    
    /// Generate outgoing sync message to parent
    pub fn generate_sync(&mut self) -> Option<SyncMessage> {
        let changes = self.batch_accumulator.flush();
        if changes.is_empty() {
            return None;
        }
        
        Some(SyncMessage {
            from: self.node_id.clone(),
            vector_clock: self.own_state.vector_clock.clone(),
            state: changes.into_iter()
                .filter_map(|op| self.delta_encoder.encode_delta(&op.key, &op.value))
                .collect(),
        })
    }
}
```

#### 6. Power Management

```rust
// File: src/power/mod.rs

/// Power profile for BLE operations
#[derive(Debug, Clone)]
pub enum PowerProfile {
    /// Maximum performance, highest power consumption
    /// Radio active ~20% of time
    Aggressive {
        scan_interval_ms: u32,   // 100ms
        scan_window_ms: u32,     // 50ms (50% duty)
        adv_interval_ms: u32,    // 100ms
        conn_interval_ms: u32,   // 15ms
    },
    /// Balanced performance and power
    /// Radio active ~10% of time
    Balanced {
        scan_interval_ms: u32,   // 500ms
        scan_window_ms: u32,     // 50ms (10% duty)
        adv_interval_ms: u32,    // 500ms
        conn_interval_ms: u32,   // 30ms
    },
    /// Maximum battery life (HIVE Lite default)
    /// Radio active ~2% of time
    LowPower {
        scan_interval_ms: u32,   // 5000ms
        scan_window_ms: u32,     // 100ms (2% duty)
        adv_interval_ms: u32,    // 2000ms
        conn_interval_ms: u32,   // 100ms
    },
    /// Custom profile
    Custom {
        scan_interval_ms: u32,
        scan_window_ms: u32,
        adv_interval_ms: u32,
        conn_interval_ms: u32,
    },
}

impl PowerProfile {
    /// Estimated battery impact (0-100)
    pub fn battery_impact(&self) -> u8 {
        match self {
            PowerProfile::Aggressive { .. } => 80,
            PowerProfile::Balanced { .. } => 40,
            PowerProfile::LowPower { .. } => 15,
            PowerProfile::Custom { 
                scan_interval_ms, scan_window_ms, .. 
            } => {
                let duty_cycle = (*scan_window_ms as f32) / (*scan_interval_ms as f32);
                (duty_cycle * 100.0) as u8
            }
        }
    }
}

/// Radio scheduler for coordinated power management
pub struct RadioScheduler {
    profile: PowerProfile,
    state: RadioState,
    next_scan_window: Instant,
    next_adv_event: Instant,
    pending_syncs: VecDeque<PendingSync>,
}

impl RadioScheduler {
    /// Schedule next radio activity
    pub fn schedule(&mut self) -> RadioAction {
        let now = Instant::now();
        
        // Priority 1: Critical data (immediate)
        if let Some(sync) = self.pending_syncs.front() {
            if sync.priority == Priority::Critical {
                return RadioAction::ImmediateSync(sync.clone());
            }
        }
        
        // Priority 2: Scheduled scan window
        if now >= self.next_scan_window {
            self.next_scan_window = now + Duration::from_millis(
                self.profile.scan_interval_ms() as u64
            );
            return RadioAction::StartScan(Duration::from_millis(
                self.profile.scan_window_ms() as u64
            ));
        }
        
        // Priority 3: Scheduled advertisement
        if now >= self.next_adv_event {
            self.next_adv_event = now + Duration::from_millis(
                self.profile.adv_interval_ms() as u64
            );
            return RadioAction::Advertise;
        }
        
        // Priority 4: Pending non-critical syncs (batched)
        if !self.pending_syncs.is_empty() && self.can_batch_sync() {
            return RadioAction::BatchSync(self.pending_syncs.drain(..).collect());
        }
        
        RadioAction::Sleep
    }
}
```

#### 7. Security Integration

```rust
// File: src/security/mod.rs

use hive_security::{SecurityManager, DeviceIdentity};

/// BLE security configuration
#[derive(Debug, Clone)]
pub struct BleSecurityConfig {
    /// Require pairing before data exchange
    pub requires_pairing: bool,
    /// Pairing mode
    pub pairing_mode: PairingMode,
    /// Require encrypted characteristics
    pub require_encryption: bool,
    /// Enable application-layer encryption (on top of BLE encryption)
    pub app_layer_encryption: bool,
    /// Acceptable bond types
    pub acceptable_bonds: BondType,
}

/// BLE pairing modes
#[derive(Debug, Clone)]
pub enum PairingMode {
    /// Just Works (no MITM protection)
    JustWorks,
    /// Numeric Comparison (requires display)
    NumericComparison,
    /// Passkey Entry (requires input)
    PasskeyEntry,
    /// Out of Band (NFC, QR code)
    OutOfBand { oob_data: Vec<u8> },
    /// Legacy pairing (BLE 4.x)
    Legacy { pin: Option<String> },
}

/// Security manager for BLE transport
pub struct BleSecurityManager {
    config: BleSecurityConfig,
    bond_store: Box<dyn BondStore>,
    hive_security: Arc<dyn SecurityManager>,
}

impl BleSecurityManager {
    /// Verify peer before allowing sync
    pub async fn verify_peer(&self, peer_id: &NodeId, conn: &BleConnection) -> Result<DeviceIdentity, SecurityError> {
        // Step 1: Check bond status
        if self.config.requires_pairing {
            let bond = self.bond_store.get_bond(peer_id).await?;
            if bond.is_none() && !conn.is_paired().await? {
                return Err(SecurityError::NotPaired);
            }
        }
        
        // Step 2: Verify encryption level
        if self.config.require_encryption {
            let enc_level = conn.encryption_level().await?;
            if enc_level < EncryptionLevel::Encrypted {
                return Err(SecurityError::EncryptionRequired);
            }
        }
        
        // Step 3: HIVE-level authentication
        let identity = self.hive_security.authenticate_peer(peer_id).await?;
        
        Ok(identity)
    }
    
    /// Encrypt sync payload (application layer)
    pub fn encrypt_sync_payload(&self, payload: &[u8], recipient: &NodeId) -> Result<Vec<u8>, SecurityError> {
        if self.config.app_layer_encryption {
            self.hive_security.encrypt(payload, recipient)
        } else {
            Ok(payload.to_vec())
        }
    }
}
```

### Platform Abstraction

```rust
// File: src/platform/mod.rs

/// Platform-agnostic BLE adapter trait
#[async_trait]
pub trait BleAdapter: Send + Sync {
    /// Check if Bluetooth is powered on
    fn is_powered(&self) -> bool;
    
    /// Get adapter capabilities
    fn capabilities(&self) -> AdapterCapabilities;
    
    /// Start scanning for peripherals
    async fn start_scan(&self, config: ScanConfig) -> Result<(), BleError>;
    
    /// Stop scanning
    async fn stop_scan(&self) -> Result<(), BleError>;
    
    /// Start advertising
    async fn start_advertising(&self, data: AdvertisementData, config: AdvertisingConfig) -> Result<(), BleError>;
    
    /// Stop advertising
    async fn stop_advertising(&self) -> Result<(), BleError>;
    
    /// Connect to a peripheral
    async fn connect(&self, address: BleAddress) -> Result<Box<dyn BleConnection>, BleError>;
    
    /// Get event stream
    fn events(&self) -> Pin<Box<dyn Stream<Item = AdapterEvent> + Send>>;
}

/// Adapter capabilities
pub struct AdapterCapabilities {
    pub supports_le_1m: bool,
    pub supports_le_2m: bool,
    pub supports_le_coded: bool,
    pub supports_extended_advertising: bool,
    pub max_connections: u8,
    pub max_mtu: u16,
}

// Platform-specific implementations
#[cfg(target_os = "linux")]
mod linux {
    use bluer::{Adapter, AdapterEvent};
    // BlueZ implementation via bluer crate
}

#[cfg(target_os = "android")]
mod android {
    use jni::JNIEnv;
    // Android BLE via JNI
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use core_bluetooth::CentralManager;
    // CoreBluetooth implementation
}

#[cfg(target_os = "windows")]
mod windows {
    use windows::Devices::Bluetooth::Advertisement::*;
    // WinRT BLE implementation
}

#[cfg(feature = "embedded")]
mod embedded {
    use esp_idf_hal::ble::*;
    // ESP32 BLE implementation (no_std compatible)
}
```

---

## Cargo.toml Configuration

```toml
[package]
name = "hive-btle"
version = "0.1.0"
edition = "2021"
authors = ["(r)evolve - Revolve Team LLC"]
license = "Apache-2.0"
description = "Bluetooth Low Energy mesh transport for HIVE Protocol"
repository = "https://github.com/revolveteam/hive-protocol"
keywords = ["ble", "bluetooth", "mesh", "sync", "crdt"]
categories = ["network-programming", "embedded", "hardware-support"]

[features]
default = ["std", "linux"]
std = []
no_std = ["embedded-hal"]

# Platform features
linux = ["bluer", "tokio"]
android = ["jni", "ndk"]
macos = ["core-bluetooth"]
ios = ["core-bluetooth"]
windows = ["windows"]
embedded = ["esp-idf-hal", "no_std"]

# PHY features
coded-phy = []  # Enable LE Coded PHY support (requires BLE 5.0)
extended-adv = []  # Enable extended advertising (requires BLE 5.0)

# Optional features
metrics = ["prometheus"]
tracing = ["tracing"]

[dependencies]
# Core
async-trait = "0.1"
futures = "0.3"
thiserror = "1.0"
uuid = { version = "1.0", features = ["v4"] }
bytes = "1.0"
bitflags = "2.0"

# Async runtime (std only)
tokio = { version = "1.0", features = ["sync", "time", "macros"], optional = true }

# Platform-specific
bluer = { version = "0.17", optional = true }  # Linux/BlueZ
jni = { version = "0.21", optional = true }  # Android
ndk = { version = "0.8", optional = true }  # Android NDK
core-bluetooth = { version = "0.3", optional = true }  # macOS/iOS
windows = { version = "0.52", features = ["Devices_Bluetooth"], optional = true }
esp-idf-hal = { version = "0.43", optional = true }  # ESP32

# Embedded
embedded-hal = { version = "1.0", optional = true }

# HIVE Protocol integration
hive-protocol = { path = "../hive-protocol" }
hive-lite = { path = "../hive-lite", optional = true }

# Metrics/tracing
prometheus = { version = "0.13", optional = true }
tracing = { version = "0.1", optional = true }

[dev-dependencies]
tokio-test = "0.4"
criterion = "0.5"
fake = "2.0"

[[bench]]
name = "throughput"
harness = false
```

---

## Security Considerations

### BLE-Level Security

| Security Level | MITM Protection | Encryption | Use Case |
|----------------|-----------------|------------|----------|
| Level 1 | No | No | Service discovery only |
| Level 2 | No | Yes | Just Works pairing |
| Level 3 | Yes | Yes | MITM-protected pairing |
| Level 4 | Yes | Yes (FIPS) | LE Secure Connections |

**Recommendation**: HIVE-BTLE should require Level 3+ for sync operations.

### Application-Layer Security

Per ADR-006, BLE transport security is layered:

1. **BLE Pairing**: Establishes link encryption (AES-CCM)
2. **HIVE PKI**: Verifies device identity via certificates
3. **Application Encryption**: ChaCha20-Poly1305 for sync payloads (optional)

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Eavesdropping | BLE link encryption + optional app-layer encryption |
| MITM | Numeric comparison or passkey pairing |
| Replay | Sequence numbers in HIVE beacon/sync messages |
| Rogue device | PKI-based device authentication |
| Battery drain attack | Rate limiting, connection limits |

---

## Success Metrics

### Technical Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Battery life (wearable) | >18 hours | Samsung Watch field test |
| Battery improvement vs Ditto | >50% | Side-by-side benchmark |
| Throughput (LE 2M) | >200 KB/s | iperf-style test |
| Throughput (Coded S=8) | >10 KB/s | iperf-style test |
| Range (Coded S=8) | >300m LOS | Field measurement |
| Connection time | <3 seconds | Stopwatch |
| Sync latency | <500ms | Timestamp delta |

### Operational Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Discovery success rate | >95% | Test across scenarios |
| Mesh healing time | <10 seconds | Disconnect/reconnect test |
| Multi-peer scalability | 7+ peers | Connection limit test |

---

## Implementation Plan

### Phase 1: Core Infrastructure (Weeks 1-3)

**Deliverables:**
1. Crate structure and Cargo.toml
2. Platform abstraction traits
3. Linux/BlueZ implementation (reference platform)
4. Basic discovery (advertising + scanning)
5. GATT service definition

**Milestones:**
- [ ] Two nodes discover each other via BLE
- [ ] Basic GATT characteristic read/write working
- [ ] Linux + x86_64 compiles and runs

### Phase 2: Mesh Topology (Weeks 4-5)

**Deliverables:**
1. Mesh topology manager
2. Parent/child relationship establishment
3. Multi-peer connection management
4. Connection event handling

**Milestones:**
- [ ] 3+ node mesh forms correctly
- [ ] Parent failover works
- [ ] Connection limits enforced

### Phase 3: HIVE-Lite Sync (Weeks 6-7)

**Deliverables:**
1. Batch accumulator
2. Delta encoding
3. CRDT sync over GATT
4. Vector clock management

**Milestones:**
- [ ] Position updates sync reliably
- [ ] Batched sync reduces radio time
- [ ] Delta sync reduces bandwidth

### Phase 4: Coded PHY & Range (Weeks 8-9)

**Deliverables:**
1. PHY selection logic
2. Coded PHY S=2/S=8 support
3. Adaptive PHY switching
4. Range testing tools

**Milestones:**
- [ ] Coded PHY working on supported hardware
- [ ] 200m+ range demonstrated with S=8
- [ ] Adaptive switching based on RSSI

### Phase 5: Cross-Platform (Weeks 10-12)

**Deliverables:**
1. Android implementation (JNI)
2. macOS/iOS implementation (CoreBluetooth)
3. Windows implementation (WinRT)
4. ARM build verification

**Milestones:**
- [ ] Android builds and runs
- [ ] iOS builds and runs
- [ ] Windows builds and runs
- [ ] ARM (aarch64, armv7) verified

### Phase 6: Security & Polish (Weeks 13-14)

**Deliverables:**
1. Security manager integration
2. Pairing mode support
3. Application-layer encryption
4. Documentation and examples

**Milestones:**
- [ ] Secure pairing working
- [ ] PKI authentication integrated
- [ ] All examples working
- [ ] API documentation complete

---

## hive-ffi Integration

### Dual-Mode Operation

hive-btle is designed to operate in two modes:

```
┌─────────────────────────────────────────────────────────┐
│              Full HIVE (ATAK, CLI, etc.)                │
│                                                         │
│  TransportManager (PACE policy)                         │
│  ├── IrohTransport (QUIC/WiFi) - Primary                │
│  ├── HiveBleTransport ────────┐  - Alternate            │
│  └── ...                      │                         │
└───────────────────────────────│─────────────────────────┘
                                │ wraps
                                ▼
┌─────────────────────────────────────────────────────────┐
│                      hive-btle                          │
│         (Standalone OR as transport plugin)             │
│         Same protocol - devices interoperate            │
└─────────────────────────────────────────────────────────┘
                                ▲
                                │ uses directly
┌───────────────────────────────┴─────────────────────────┐
│              WearTAK (Samsung Watch)                    │
│         Can't run full HIVE - standalone hive-btle      │
└─────────────────────────────────────────────────────────┘
```

**Mode 1: Standalone** - For resource-constrained devices (WearTAK on Samsung watches, ESP32 sensors) that cannot run full HIVE. The device uses hive-btle directly with its lightweight CRDT sync.

**Mode 2: Transport Plugin** - For full HIVE nodes (ATAK, CLI, servers), hive-btle is wrapped by `HiveBleTransport` and registered with `TransportManager` alongside other transports (Iroh/QUIC, future LoRa, etc.).

**Interoperability**: Both modes use the same BLE protocol (GATT service, beacon format, sync protocol), so a Samsung Watch running standalone hive-btle can sync with ATAK running full HIVE with BLE as a transport.

### hive-ffi Transport Abstraction

#### Feature Flags

```toml
# hive-ffi/Cargo.toml
[features]
default = ["sync"]
sync = ["hive-protocol/automerge-backend"]
bluetooth = ["hive-protocol/bluetooth"]
```

#### Extended NodeConfig

```rust
// hive-ffi/src/lib.rs

#[derive(Debug, Clone, uniffi::Record)]
pub struct NodeConfig {
    pub app_id: String,
    pub shared_key: String,
    pub bind_address: Option<String>,
    pub storage_path: String,
    pub transport_config: Option<TransportConfigFFI>,  // NEW
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TransportConfigFFI {
    /// Enable BLE transport (requires bluetooth feature)
    pub enable_ble: bool,
    /// BLE mesh ID for WearTAK interoperability (e.g., "WEARTAK")
    pub ble_mesh_id: Option<String>,
    /// Power profile: "aggressive", "balanced", "low_power"
    pub ble_power_profile: Option<String>,
    /// PACE transport preference order (e.g., ["quic", "bluetooth_le"])
    pub transport_preference: Option<Vec<String>>,
}
```

#### HiveNode with TransportManager

```rust
// hive-ffi/src/lib.rs

pub struct HiveNode {
    sync_backend: Arc<AutomergeIrohBackend>,
    storage_backend: Arc<AutomergeBackend>,
    transport_manager: Arc<TransportManager>,  // Replaces: transport: Arc<IrohTransport>
    iroh_transport: Arc<IrohTransport>,
    #[cfg(feature = "bluetooth")]
    ble_transport: Option<Arc<HiveBleTransport>>,
    store: Arc<AutomergeStore>,
    storage_path: PathBuf,
    runtime: Arc<tokio::runtime::Runtime>,
    cleanup_running: Arc<AtomicBool>,
}
```

#### Platform Adapters

hive-btle provides platform-specific adapters that hive-ffi uses via feature flags:

| Platform | hive-btle Feature | Adapter |
|----------|-------------------|---------|
| Linux | `linux` | BlueZ via `bluer` crate |
| Android | `android` | JNI to Android Bluetooth API |
| macOS | `macos` | CoreBluetooth |
| iOS | `ios` | CoreBluetooth |
| Windows | `windows` | WinRT Bluetooth API |

hive-ffi doesn't implement its own adapters - it uses hive-btle's existing adapters:

```rust
// hive-ffi/src/lib.rs

#[cfg(all(feature = "bluetooth", target_os = "linux"))]
use hive_btle::platform::linux::LinuxAdapter;

#[cfg(all(feature = "bluetooth", target_os = "android"))]
use hive_btle::platform::android::AndroidAdapter;

#[cfg(all(feature = "bluetooth", any(target_os = "macos", target_os = "ios")))]
use hive_btle::platform::apple::AppleAdapter;

// Create transport with platform-appropriate adapter
#[cfg(feature = "bluetooth")]
fn create_ble_transport(config: &BleConfig) -> Result<HiveBleTransport<impl BleAdapter>> {
    #[cfg(target_os = "linux")]
    let adapter = LinuxAdapter::new()?;

    #[cfg(target_os = "android")]
    let adapter = AndroidAdapter::new()?;

    // ... etc

    Ok(HiveBleTransport::new(config, adapter))
}
```

For Android, hive-ffi exposes JNI functions to control the transport:

```kotlin
// HiveJni.kt
external fun enableBleTransport(nodeHandle: Long, meshId: String): Boolean
external fun disableBleTransport(nodeHandle: Long): Boolean
external fun getBleStatus(nodeHandle: Long): BleStatus
```

### Translation Layer (ADR-041 Integration)

Full HIVE nodes act as gateways between Automerge documents and hive-btle's lightweight CRDTs:

```rust
// hive-protocol/src/sync/ble_translation.rs

pub struct BleTranslationLayer {
    node_mapping: HashMap<u32, String>,
}

impl BleTranslationLayer {
    /// Receive HiveDocument from BLE, update Automerge
    pub fn on_ble_document(
        &mut self,
        doc: &HiveDocument,
        am_doc: &mut Automerge
    ) -> Result<()> {
        let node_id = format!("{:08X}", doc.node_id());

        // Map hive-btle fields to Automerge document structure
        am_doc.put(&format!("/devices/{}/counter", node_id), doc.counter())?;

        if let Some(peripheral) = doc.peripheral() {
            if let Some(loc) = peripheral.location() {
                am_doc.put(&format!("/devices/{}/lat", node_id), loc.lat)?;
                am_doc.put(&format!("/devices/{}/lon", node_id), loc.lon)?;
            }
            if let Some(event) = peripheral.last_event() {
                match event.event_type {
                    EventType::Emergency => {
                        am_doc.put(&format!("/alerts/{}/emergency", node_id), true)?;
                    }
                    EventType::Ack => {
                        am_doc.put(&format!("/alerts/{}/ack", node_id), true)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Build HiveDocument from Automerge for BLE broadcast
    pub fn build_ble_document(&self, node_id: u32, am_doc: &Automerge) -> Vec<u8> {
        let mut doc = HiveDocument::new(node_id);
        // Extract relevant fields from Automerge, populate lightweight doc
        doc.encode()
    }
}
```

### ATAK Plugin Migration

The ATAK plugin currently runs two parallel systems:

- `HiveNodeJni` → hive-ffi → IrohTransport (PLI broadcast)
- `HiveBleManager` → hive-btle AAR → BLE mesh (WearTAK sync)

**Problem**: Data doesn't flow between them. PLI broadcasts only go over Iroh; WearTAK devices on BLE never receive them.

**Solution**: Unified transport via hive-ffi:

```kotlin
// HivePluginLifecycle.kt - BEFORE (dual system)
val hiveNode = HiveJni.createNode(config)
val bleManager = HiveBleManager(context, meshId)  // Separate!

// HivePluginLifecycle.kt - AFTER (unified)
val config = NodeConfig(
    appId = "...",
    sharedKey = "...",
    transportConfig = TransportConfigFFI(
        enableBle = true,
        bleMeshId = "WEARTAK",
        blePowerProfile = "balanced"
    )
)
val hiveNode = HiveJni.createNode(config)
// BLE is now managed by TransportManager inside hive-ffi
```

**Migration Steps**:

1. Add `bluetooth` feature to hive-ffi, expose `TransportConfigFFI` via UniFFI
2. Add JNI bindings for BLE control (`enableBleTransport`, `onBleDiscovered`, etc.)
3. Update `HivePluginLifecycle` to use `TransportConfigFFI` with `enable_ble=true`
4. Deprecate `HiveBleManager` (keep working during transition)
5. Validate WearTAK sync works via unified path
6. Remove `HiveBleManager` once stable

---

## Alternatives Considered

### 1. Use btleplug Directly

**btleplug** is a cross-platform BLE library for Rust.

**Pros**: Already cross-platform, active maintenance
**Cons**: No GATT server support, limited PHY control, no mesh abstractions

**Decision**: Use btleplug/bluer as platform layer, build mesh abstractions on top.

### 2. Bluetooth Mesh (SIG Mesh)

Bluetooth SIG defines a mesh networking standard.

**Pros**: Standardized, multi-vendor support
**Cons**: Complex provisioning, no CRDT support, designed for lighting/IoT not tactical

**Decision**: Implement HIVE-native mesh over GATT, not SIG Mesh.

### 3. ESP-NOW on ESP32

ESP-NOW is Espressif's proprietary peer-to-peer protocol.

**Pros**: Very low latency, direct WiFi-based
**Cons**: ESP32-only, not BLE, short range

**Decision**: Support ESP-NOW as separate transport, BLE for cross-platform.

---

## References

1. [Bluetooth Core Spec 5.4](https://www.bluetooth.com/specifications/specs/core-specification-5-4/)
2. [LE Coded PHY Explained](https://www.bluetooth.com/blog/exploring-bluetooth-5-whats-new-in-coded-phy/)
3. [bluer crate](https://docs.rs/bluer/) - Linux BlueZ bindings
4. [btleplug crate](https://docs.rs/btleplug/) - Cross-platform BLE
5. ADR-032: Pluggable Transport Abstraction
6. ADR-035: HIVE-Lite Embedded Nodes
7. ADR-037: Resource-Constrained Device Optimization
8. ADR-006: Security, Authentication, and Authorization

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-13 | Create dedicated hive-btle crate | BLE requires fundamentally different abstractions than IP-based transports |
| 2025-12-13 | GATT-based sync over SIG Mesh | HIVE needs CRDT sync semantics, not broadcast mesh |
| 2025-12-13 | Platform abstraction with native implementations | Each platform has different BLE APIs and capabilities |
| 2025-12-13 | Coded PHY support as feature | Enables range/throughput tradeoffs, requires BLE 5.0 |
| 2025-01-12 | Dual-mode operation (standalone + transport plugin) | hive-btle must work standalone for constrained devices (WearTAK) AND as transport within full HIVE |
| 2025-01-12 | Integrate via TransportManager in hive-ffi | Unifies transport selection with PACE policy; avoids dual-system architecture in ATAK plugin |
| 2025-01-12 | Translation layer for Automerge ↔ hive-btle | Per ADR-041, full HIVE nodes act as gateways translating between document formats |

---

**Next Steps:**
1. ~~Review with stakeholders~~ (ongoing)
2. GitHub issues created for hive-ffi integration (see below)
3. Radicle issues for hive-btle API exposure
4. Begin Phase 1: Add bluetooth feature flag to hive-ffi
5. Coordinate with Ascent for WearTAK integration testing

**GitHub Issues (hive repo):**
- [#554](https://github.com/kitplummer/hive/issues/554) - Add `bluetooth` feature flag to hive-ffi
- [#555](https://github.com/kitplummer/hive/issues/555) - Refactor HiveNode to use TransportManager
- [#556](https://github.com/kitplummer/hive/issues/556) - Add BLE transport option to NodeConfig/create_node
- [#557](https://github.com/kitplummer/hive/issues/557) - Implement Automerge ↔ hive-btle translation layer
- [#558](https://github.com/kitplummer/hive/issues/558) - ATAK plugin: migrate to unified transport

**Radicle Issues (hive-btle repo):** *(create manually in Radicle)*

### Issue: Ensure platform adapters are usable from external crates (hive-ffi)

**Summary**: hive-btle has platform adapters for Linux (BlueZ), macOS/iOS (CoreBluetooth), Android (JNI), and Windows (WinRT). Ensure these can be used by hive-ffi via feature flags without hive-ffi needing to implement its own adapters.

**Implementation**:
- Verify platform feature flags work correctly when hive-btle is a dependency
- Ensure `BluetoothLETransport::new()` can be called with platform-appropriate adapter
- Export configuration types (`BleConfig`, `HiveMeshConfig`, `PowerProfile`) for external use
- Ensure observer/callback pattern works across crate boundary
- Document feature flag combinations for each target platform

**Acceptance Criteria**:
- [ ] hive-ffi can use `linux` feature → gets BlueZ adapter
- [ ] hive-ffi can use `android` feature → gets JNI adapter
- [ ] hive-ffi can use `macos`/`ios` feature → gets CoreBluetooth adapter
- [ ] Configuration types are public and documented
- [ ] `HiveObserver` works across crate boundary
- [ ] Example of external crate usage

---

### Issue: Add transport-agnostic document encoding helpers

**Summary**: Ensure `HiveDocument` encoding/decoding is public and transport-agnostic for use in the translation layer.

**Implementation**:
- Make `HiveDocument::from_bytes()` public
- Make `HiveDocument::to_bytes()` / `encode()` public
- Allow external construction with arbitrary node IDs
- Document the binary format for interoperability

**Acceptance Criteria**:
- [ ] `HiveDocument::from_bytes()` is public
- [ ] `HiveDocument::to_bytes()` is public
- [ ] External node ID assignment works
- [ ] Binary format documented
