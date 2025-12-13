# HIVE-BTLE Implementation Kickstart

**Organization**: (r)evolve - Revolve Team LLC  
**Project**: HIVE Protocol - Bluetooth LE Mesh Transport  
**Date**: 2025-12-13  

---

## Overview

This document provides the kickstart materials for implementing the `hive-btle` crate, including GitHub issues, development environment setup, and initial implementation guidance.

---

## GitHub Issues Template

### Epic: HIVE-BTLE Mesh Transport Crate

```markdown
## Epic: HIVE-BTLE Mesh Transport Crate

**Summary**: Implement Bluetooth Low Energy mesh transport for HIVE Protocol supporting P2P discovery, advertisement, connectivity, and HIVE-Lite synchronization across x86/ARM on Linux, Android, macOS, iOS, and Windows.

**Related ADRs**: 
- ADR-039: HIVE-BTLE Mesh Transport Crate
- ADR-032: Pluggable Transport Abstraction
- ADR-035: HIVE-Lite Embedded Nodes
- ADR-037: Resource-Constrained Device Optimization

**Success Criteria**:
- [ ] 18+ hour battery life on Samsung Watch (vs 3-4 hours with Ditto)
- [ ] >50% battery improvement over Ditto baseline
- [ ] Cross-platform builds (Linux, Android, macOS, iOS, Windows)
- [ ] Cross-architecture builds (x86_64, aarch64, armv7)
- [ ] Coded PHY support for 300m+ range
- [ ] HIVE-Lite sync protocol working over GATT

**Timeline**: 14 weeks (Q1 2026)
```

### Issue Templates

#### Issue #1: Core Crate Infrastructure

```markdown
## Issue: Core Crate Infrastructure

**Type**: Feature  
**Priority**: P0 - Critical  
**Sprint**: Phase 1 (Weeks 1-3)  
**Estimate**: 5 days  

### Description

Set up the foundational crate structure, Cargo.toml with feature flags, and core trait definitions.

### Acceptance Criteria

- [ ] Crate compiles with `cargo build`
- [ ] Feature flags work: `--features linux`, `--features android`, etc.
- [ ] `no_std` feature compiles (for embedded targets)
- [ ] Core types defined: `BleConfig`, `BleError`, `NodeId`
- [ ] Platform abstraction traits defined: `BleAdapter`, `BleConnection`
- [ ] ADR-032 `Transport` trait implemented (stub)

### Technical Details

```rust
// Key types to define
pub struct BleConfig { ... }
pub enum BleError { ... }
pub struct TransportCapabilities { ... }

// Key traits
pub trait BleAdapter: Send + Sync { ... }
pub trait BleConnection: Send + Sync { ... }
```

### Files to Create

- `Cargo.toml`
- `src/lib.rs`
- `src/config.rs`
- `src/error.rs`
- `src/transport.rs`
- `src/platform/mod.rs`
```

#### Issue #2: Linux/BlueZ Platform Implementation

```markdown
## Issue: Linux/BlueZ Platform Implementation

**Type**: Feature  
**Priority**: P0 - Critical  
**Sprint**: Phase 1 (Weeks 1-3)  
**Estimate**: 8 days  
**Depends On**: #1 (Core Crate Infrastructure)

### Description

Implement the BLE adapter for Linux using the `bluer` crate (BlueZ D-Bus bindings). This serves as the reference platform for initial development.

### Acceptance Criteria

- [ ] Can discover nearby BLE devices
- [ ] Can advertise HIVE beacon
- [ ] Can establish GATT connection
- [ ] Can read/write GATT characteristics
- [ ] Works on x86_64 and aarch64 (Raspberry Pi)

### Technical Details

```rust
// Linux-specific implementation
#[cfg(target_os = "linux")]
pub struct BluerAdapter {
    adapter: bluer::Adapter,
    session: bluer::Session,
}

#[async_trait]
impl BleAdapter for BluerAdapter {
    async fn start_scan(&self, config: ScanConfig) -> Result<(), BleError> {
        // Use bluer discovery API
    }
}
```

### Dependencies

- `bluer = "0.17"` - BlueZ D-Bus bindings
- `tokio = "1.0"` - Async runtime

### Testing

```bash
# Requires Linux with BlueZ
cargo test --features linux -- --ignored
```
```

#### Issue #3: HIVE Beacon Discovery

```markdown
## Issue: HIVE Beacon Discovery

**Type**: Feature  
**Priority**: P0 - Critical  
**Sprint**: Phase 1 (Weeks 1-3)  
**Estimate**: 5 days  
**Depends On**: #2 (Linux/BlueZ Platform Implementation)

### Description

Implement HIVE beacon format for BLE advertisements and discovery/parsing of peer beacons.

### Acceptance Criteria

- [ ] Beacon encodes node ID, capabilities, hierarchy level, geohash
- [ ] Beacon fits in legacy advertising (31 bytes)
- [ ] Extended advertising supported for larger payloads
- [ ] Scanner filters for HIVE service UUID
- [ ] Deduplicate beacons from same node
- [ ] Track RSSI for proximity estimation

### Technical Details

```rust
/// HIVE beacon wire format (16 bytes)
pub struct HiveBeacon {
    version: u8,              // 4 bits
    capabilities: u16,        // 12 bits
    node_id_short: u32,       // 32 bits
    hierarchy_level: u8,      // 8 bits
    geohash: u32,             // 24 bits (6-char precision)
    battery_percent: u8,      // 8 bits
    seq_num: u16,             // 16 bits
}

const HIVE_SERVICE_UUID: Uuid = uuid!("HIVE-SERVICE-UUID-128");
const HIVE_COMPANY_ID: u16 = 0xFFFF; // Placeholder
```

### Files to Create/Modify

- `src/discovery/mod.rs`
- `src/discovery/beacon.rs`
- `src/discovery/advertiser.rs`
- `src/discovery/scanner.rs`
```

#### Issue #4: GATT Service Definition

```markdown
## Issue: GATT Service Definition

**Type**: Feature  
**Priority**: P0 - Critical  
**Sprint**: Phase 1 (Weeks 1-3)  
**Estimate**: 5 days  
**Depends On**: #2 (Linux/BlueZ Platform Implementation)

### Description

Define and implement the HIVE GATT service with characteristics for sync operations.

### Acceptance Criteria

- [ ] HIVE service registered with platform BLE stack
- [ ] Node Info characteristic (read)
- [ ] Sync State characteristic (read/notify)
- [ ] Sync Data characteristic (write/indicate)
- [ ] Command characteristic (write)
- [ ] Status characteristic (read/notify)
- [ ] MTU negotiation works (target 251 bytes)

### Technical Details

```
HIVE GATT Service
├── Node Info (read)
│   └── UUID: 0x0001
├── Sync State (read/notify)
│   └── UUID: 0x0002
├── Sync Data (write/indicate)
│   └── UUID: 0x0003
├── Command (write)
│   └── UUID: 0x0004
└── Status (read/notify)
    └── UUID: 0x0005
```

### Files to Create

- `src/gatt/mod.rs`
- `src/gatt/service.rs`
- `src/gatt/characteristics.rs`
- `src/gatt/protocol.rs`
```

#### Issue #5: Mesh Topology Manager

```markdown
## Issue: Mesh Topology Manager

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 2 (Weeks 4-5)  
**Estimate**: 8 days  
**Depends On**: #3, #4

### Description

Implement mesh topology management for parent/child/peer relationships in the BLE mesh.

### Acceptance Criteria

- [ ] Node can connect as child to parent
- [ ] Node can accept children connections
- [ ] Parent failover when connection lost
- [ ] Connection limit enforcement (max 7-10 peers)
- [ ] RSSI-based peer selection
- [ ] Topology events published to subscribers

### Technical Details

```rust
pub struct MeshManager {
    topology: RwLock<MeshTopology>,
    connections: RwLock<HashMap<NodeId, BleConnection>>,
}

pub struct MeshTopology {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    peers: Vec<NodeId>,
    my_level: HierarchyLevel,
}
```

### Files to Create

- `src/mesh/mod.rs`
- `src/mesh/topology.rs`
- `src/mesh/connection.rs`
- `src/mesh/routing.rs`
```

#### Issue #6: HIVE-Lite Sync Protocol

```markdown
## Issue: HIVE-Lite Sync Protocol

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 3 (Weeks 6-7)  
**Estimate**: 10 days  
**Depends On**: #4, #5

### Description

Implement HIVE-Lite synchronization protocol over GATT characteristics.

### Acceptance Criteria

- [ ] Batch accumulator collects changes over sync window
- [ ] Delta encoder sends only changed state
- [ ] Chunked transfer for messages > MTU
- [ ] Vector clock tracking for sync resumption
- [ ] LWW-Register and G-Counter CRDT support
- [ ] Position and health state sync working

### Technical Details

```rust
pub struct BatchAccumulator {
    config: BatchConfig,
    pending_changes: Vec<CrdtOperation>,
    last_sync: Instant,
    bytes_accumulated: usize,
}

pub struct DeltaEncoder {
    last_sent_state: HashMap<String, Vec<u8>>,
}

pub struct GattSyncProtocol {
    mtu: usize,
    pending_tx: VecDeque<SyncChunk>,
    pending_rx: HashMap<u32, PartialMessage>,
}
```

### Files to Create

- `src/sync/mod.rs`
- `src/sync/batch.rs`
- `src/sync/delta.rs`
- `src/sync/crdt.rs`
- `src/sync/protocol.rs`
```

#### Issue #7: PHY Configuration (Coded PHY)

```markdown
## Issue: PHY Configuration (Coded PHY)

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 4 (Weeks 8-9)  
**Estimate**: 6 days  
**Depends On**: #2

### Description

Implement BLE PHY selection including LE Coded PHY for extended range.

### Acceptance Criteria

- [ ] LE 1M PHY works (default)
- [ ] LE 2M PHY works (high throughput)
- [ ] LE Coded S=2 PHY works (500kbps, 2x range)
- [ ] LE Coded S=8 PHY works (125kbps, 4x range)
- [ ] Adaptive PHY selection based on RSSI
- [ ] PHY switching without disconnection

### Technical Details

```rust
pub enum BlePhy {
    Le1M,       // 1 Mbps, 100m range
    Le2M,       // 2 Mbps, 50m range
    LeCodedS2,  // 500 kbps, 200m range
    LeCodedS8,  // 125 kbps, 400m range
}

pub enum PhyStrategy {
    Fixed(BlePhy),
    Adaptive { rssi_threshold_high: i8, rssi_threshold_low: i8 },
    MaxRange,       // Always Coded S=8
    MaxThroughput,  // Always 2M
}
```

### Hardware Requirements

- BLE 5.0+ capable hardware
- Linux: BlueZ 5.48+
- Tested adapters: Intel AX200, nRF52840

### Files to Create

- `src/phy/mod.rs`
- `src/phy/coded.rs`
- `src/phy/uncoded.rs`
- `src/phy/adaptive.rs`
```

#### Issue #8: Power Management

```markdown
## Issue: Power Management

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 3 (Weeks 6-7)  
**Estimate**: 5 days  
**Depends On**: #3, #4

### Description

Implement power management profiles and radio duty cycle scheduling for battery efficiency.

### Acceptance Criteria

- [ ] Three power profiles: Aggressive, Balanced, LowPower
- [ ] LowPower profile achieves <5% radio duty cycle
- [ ] Radio scheduler coordinates scan/adv/sync
- [ ] Critical data triggers immediate sync
- [ ] Battery level exposed via beacon
- [ ] Profile auto-adjustment based on battery state

### Technical Details

```rust
pub enum PowerProfile {
    Aggressive { ... },  // 20% duty cycle
    Balanced { ... },    // 10% duty cycle
    LowPower { ... },    // 2% duty cycle
    Custom { ... },
}

pub struct RadioScheduler {
    profile: PowerProfile,
    next_scan_window: Instant,
    next_adv_event: Instant,
    pending_syncs: VecDeque<PendingSync>,
}
```

### Target Metrics

| Profile | Duty Cycle | Watch Battery Life |
|---------|------------|-------------------|
| Aggressive | 20% | ~6 hours |
| Balanced | 10% | ~12 hours |
| LowPower | 2% | ~20+ hours |

### Files to Create

- `src/power/mod.rs`
- `src/power/profile.rs`
- `src/power/scheduler.rs`
```

#### Issue #9: Android Platform Implementation

```markdown
## Issue: Android Platform Implementation

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 5 (Weeks 10-12)  
**Estimate**: 10 days  
**Depends On**: #1-#8

### Description

Implement BLE adapter for Android using JNI to interface with Android Bluetooth APIs.

### Acceptance Criteria

- [ ] Compiles with Android NDK
- [ ] Works on armv7 and aarch64
- [ ] Discovery works
- [ ] GATT server works
- [ ] GATT client works
- [ ] Works on Samsung Galaxy Watch (WearOS)

### Technical Details

```rust
#[cfg(target_os = "android")]
pub struct AndroidBleAdapter {
    jvm: JavaVM,
    bluetooth_manager: GlobalRef,
    ble_adapter: GlobalRef,
}
```

### Dependencies

- `jni = "0.21"`
- `ndk = "0.8"`
- Android SDK 21+ (BLE)
- Android SDK 26+ (Coded PHY)

### Testing

Requires physical Android device or emulator with BLE support.
```

#### Issue #10: macOS/iOS Platform Implementation

```markdown
## Issue: macOS/iOS Platform Implementation

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 5 (Weeks 10-12)  
**Estimate**: 8 days  
**Depends On**: #1-#8

### Description

Implement BLE adapter for Apple platforms using CoreBluetooth.

### Acceptance Criteria

- [ ] Compiles for macOS (x86_64, aarch64)
- [ ] Compiles for iOS (aarch64)
- [ ] Discovery works
- [ ] GATT peripheral mode works
- [ ] GATT central mode works
- [ ] Background execution works on iOS

### Technical Details

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub struct CoreBluetoothAdapter {
    central_manager: id,  // CBCentralManager
    peripheral_manager: id,  // CBPeripheralManager
}
```

### Dependencies

- `core-bluetooth = "0.3"` or manual bindings
- Xcode for iOS builds

### iOS-Specific Requirements

- Background mode: `bluetooth-central`, `bluetooth-peripheral`
- Info.plist: `NSBluetoothAlwaysUsageDescription`
```

#### Issue #11: Windows Platform Implementation

```markdown
## Issue: Windows Platform Implementation

**Type**: Feature  
**Priority**: P2 - Medium  
**Sprint**: Phase 5 (Weeks 10-12)  
**Estimate**: 6 days  
**Depends On**: #1-#8

### Description

Implement BLE adapter for Windows using WinRT Bluetooth APIs.

### Acceptance Criteria

- [ ] Compiles for Windows x86_64
- [ ] Discovery works
- [ ] GATT client works
- [ ] GATT server works (Windows 10 1803+)
- [ ] Works with common BLE adapters

### Technical Details

```rust
#[cfg(target_os = "windows")]
pub struct WinRtBleAdapter {
    watcher: BluetoothLEAdvertisementWatcher,
    publisher: BluetoothLEAdvertisementPublisher,
}
```

### Dependencies

- `windows = "0.52"` with features:
  - `Devices_Bluetooth`
  - `Devices_Bluetooth_Advertisement`
  - `Devices_Bluetooth_GenericAttributeProfile`

### Requirements

- Windows 10 version 1703+ (Creators Update)
- BLE adapter (most USB adapters work)
```

#### Issue #12: Security Integration

```markdown
## Issue: Security Integration

**Type**: Feature  
**Priority**: P1 - High  
**Sprint**: Phase 6 (Weeks 13-14)  
**Estimate**: 6 days  
**Depends On**: #4, #5, #6

### Description

Integrate BLE security (pairing/bonding) with HIVE security layer (ADR-006).

### Acceptance Criteria

- [ ] BLE pairing modes implemented (Just Works, Numeric Comparison)
- [ ] Bond storage and retrieval
- [ ] Encrypted characteristics enforced
- [ ] HIVE PKI authentication after BLE connection
- [ ] Application-layer encryption optional
- [ ] Secure characteristic writes

### Technical Details

```rust
pub struct BleSecurityManager {
    config: BleSecurityConfig,
    bond_store: Box<dyn BondStore>,
    hive_security: Arc<dyn SecurityManager>,
}

pub enum PairingMode {
    JustWorks,
    NumericComparison,
    PasskeyEntry,
    OutOfBand { oob_data: Vec<u8> },
}
```

### Files to Create

- `src/security/mod.rs`
- `src/security/pairing.rs`
- `src/security/bonding.rs`
- `src/security/encryption.rs`
```

---

## Development Environment Setup

### Prerequisites

```bash
# Linux (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install -y \
    bluez \
    libbluetooth-dev \
    libdbus-1-dev \
    pkg-config \
    build-essential

# Enable BLE experimental features (for some PHY options)
sudo vim /etc/bluetooth/main.conf
# Add: Experimental = true
sudo systemctl restart bluetooth

# macOS
# Install Xcode Command Line Tools
xcode-select --install

# Windows
# Install Visual Studio Build Tools
# Install Windows SDK
```

### Rust Toolchain

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add targets
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add aarch64-apple-ios
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
rustup target add x86_64-pc-windows-msvc

# For embedded
rustup target add thumbv7em-none-eabihf  # ARM Cortex-M4
```

### Android Setup

```bash
# Install Android NDK
sdkmanager "ndk;25.2.9519653"

# Set environment variables
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/25.2.9519653
export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH
```

### iOS Setup

```bash
# Install cargo-lipo for universal binaries
cargo install cargo-lipo

# Build for iOS
cargo lipo --release --targets aarch64-apple-ios
```

---

## Initial Implementation Guide

### Step 1: Create Crate Structure

```bash
cargo new hive-btle --lib
cd hive-btle

# Create directory structure
mkdir -p src/{discovery,mesh,gatt,sync,security,power,platform,phy}
mkdir -p src/platform/{linux,android,macos,ios,windows,embedded}
mkdir -p examples benches tests
```

### Step 2: Initial Cargo.toml

```toml
[package]
name = "hive-btle"
version = "0.1.0"
edition = "2021"
authors = ["(r)evolve - Revolve Team LLC"]
license = "Apache-2.0"
description = "Bluetooth Low Energy mesh transport for HIVE Protocol"

[features]
default = ["std", "linux"]
std = []
no_std = ["embedded-hal"]
linux = ["bluer", "tokio"]
android = ["jni", "ndk"]
macos = ["objc", "block"]
ios = ["objc", "block"]
windows = ["windows"]
embedded = ["esp-idf-hal", "no_std"]
coded-phy = []
extended-adv = []

[dependencies]
async-trait = "0.1"
futures = "0.3"
thiserror = "1.0"
uuid = { version = "1.0", features = ["v4"] }
bytes = "1.0"
bitflags = "2.0"
log = "0.4"

# Platform-specific (all optional)
tokio = { version = "1.0", features = ["sync", "time", "macros", "rt-multi-thread"], optional = true }
bluer = { version = "0.17", optional = true }
jni = { version = "0.21", optional = true }
ndk = { version = "0.8", optional = true }
objc = { version = "0.2", optional = true }
block = { version = "0.1", optional = true }
windows = { version = "0.52", features = ["Devices_Bluetooth", "Devices_Bluetooth_Advertisement", "Devices_Bluetooth_GenericAttributeProfile"], optional = true }
esp-idf-hal = { version = "0.43", optional = true }
embedded-hal = { version = "1.0", optional = true }

[dev-dependencies]
tokio-test = "0.4"
env_logger = "0.11"
```

### Step 3: Core Types (src/lib.rs)

```rust
//! HIVE-BTLE: Bluetooth Low Energy mesh transport for HIVE Protocol
//!
//! This crate provides BLE-based peer-to-peer mesh networking for HIVE,
//! supporting discovery, advertisement, connectivity, and HIVE-Lite sync.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod config;
pub mod error;
pub mod transport;
pub mod phy;
pub mod discovery;
pub mod mesh;
pub mod gatt;
pub mod sync;
pub mod security;
pub mod power;

#[cfg(any(feature = "linux", feature = "android", feature = "macos", 
          feature = "ios", feature = "windows", feature = "embedded"))]
pub mod platform;

pub use config::BleConfig;
pub use error::BleError;
pub use transport::BluetoothLETransport;

/// HIVE BLE Service UUID (128-bit)
pub const HIVE_SERVICE_UUID: uuid::Uuid = 
    uuid::uuid!("f47ac10b-58cc-4372-a567-0e02b2c3d479");

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

### Step 4: Error Types (src/error.rs)

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BleError {
    #[error("Bluetooth adapter not available")]
    AdapterNotAvailable,
    
    #[error("Bluetooth is powered off")]
    NotPowered,
    
    #[error("Feature not supported: {0}")]
    NotSupported(String),
    
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),
    
    #[error("GATT error: {0}")]
    GattError(String),
    
    #[error("Security error: {0}")]
    SecurityError(String),
    
    #[error("Sync error: {0}")]
    SyncError(String),
    
    #[error("Platform error: {0}")]
    PlatformError(String),
    
    #[error("Timeout")]
    Timeout,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BleError>;
```

### Step 5: Configuration (src/config.rs)

```rust
use crate::phy::BlePhy;
use crate::power::PowerProfile;

/// BLE transport configuration
#[derive(Debug, Clone)]
pub struct BleConfig {
    /// Discovery configuration
    pub discovery: DiscoveryConfig,
    /// GATT configuration
    pub gatt: GattConfig,
    /// Mesh configuration
    pub mesh: MeshConfig,
    /// Power profile
    pub power_profile: PowerProfile,
    /// PHY configuration
    pub phy: PhyConfig,
    /// Security configuration
    pub security: SecurityConfig,
}

impl Default for BleConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::default(),
            gatt: GattConfig::default(),
            mesh: MeshConfig::default(),
            power_profile: PowerProfile::Balanced,
            phy: PhyConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub scan_interval_ms: u32,
    pub scan_window_ms: u32,
    pub adv_interval_ms: u32,
    pub tx_power: i8,
    pub adv_phy: BlePhy,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            scan_interval_ms: 500,
            scan_window_ms: 50,
            adv_interval_ms: 500,
            tx_power: 0,  // 0 dBm
            adv_phy: BlePhy::Le1M,
        }
    }
}

// ... additional config structs
```

---

## Testing Strategy

### Unit Tests

```rust
// tests/discovery_tests.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_encode_decode() {
        let beacon = HiveBeacon {
            version: 1,
            capabilities: NodeCapabilities::LITE,
            node_id_short: 0x12345678,
            hierarchy_level: HierarchyLevel::Squad,
            geohash: 0x9q8yy8,
            battery_percent: 75,
            seq_num: 42,
        };
        
        let encoded = beacon.encode();
        let decoded = HiveBeacon::decode(&encoded).unwrap();
        
        assert_eq!(beacon.node_id_short, decoded.node_id_short);
        assert_eq!(beacon.battery_percent, decoded.battery_percent);
    }
}
```

### Integration Tests

```bash
# Requires two BLE-capable machines
# Machine A
cargo run --example basic_mesh -- --role central

# Machine B  
cargo run --example basic_mesh -- --role peripheral
```

### Benchmark Tests

```rust
// benches/throughput.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn throughput_benchmark(c: &mut Criterion) {
    c.bench_function("sync_1kb_payload", |b| {
        b.iter(|| {
            // Sync 1KB payload over GATT
        })
    });
}

criterion_group!(benches, throughput_benchmark);
criterion_main!(benches);
```

---

## Milestone Checklist

### Phase 1 Complete (Week 3)
- [ ] Crate compiles on Linux x86_64
- [ ] Two nodes discover each other
- [ ] GATT connection established
- [ ] Basic read/write works

### Phase 2 Complete (Week 5)
- [ ] 3+ node mesh forms
- [ ] Parent/child topology works
- [ ] Connection failover works

### Phase 3 Complete (Week 7)
- [ ] HIVE-Lite sync working
- [ ] Power profiles implemented
- [ ] Battery metrics visible

### Phase 4 Complete (Week 9)
- [ ] Coded PHY working
- [ ] 200m+ range demonstrated
- [ ] Adaptive PHY switching

### Phase 5 Complete (Week 12)
- [ ] Android builds and runs
- [ ] iOS builds and runs
- [ ] Windows builds and runs
- [ ] ARM targets verified

### Phase 6 Complete (Week 14)
- [ ] Security integration complete
- [ ] Documentation complete
- [ ] All examples working
- [ ] Ready for WearTAK integration testing

---

## Contact

**Organization**: (r)evolve - Revolve Team LLC  
**Website**: https://revolveteam.com  
**Project Lead**: Kit Plummer  
