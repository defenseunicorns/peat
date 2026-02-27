# peat-btle Architectural Review

**Date**: December 2024
**Version**: 0.1.0
**Status**: Living Document

---

## Executive Summary

peat-btle is a BLE mesh transport library implementing distributed state synchronization using CRDTs. This review assesses the current architecture, identifies gaps, and provides prioritized recommendations for production readiness.

**Key Findings**:
- Strong core abstractions (CRDT sync, peer management, observer pattern)
- Platform consumers duplicate significant BLE logic
- ~35-40% test coverage with critical gaps in platform adapters
- Good high-level documentation, missing practical examples

**Priority Recommendations**:
1. Add MockBleAdapter for CI-testable code
2. Create persistence and gossip strategy abstractions
3. Build integration test suite
4. Provide runnable examples per platform

---

## 1. Module Architecture

### 1.1 Current Structure

```
peat-btle/src/
├── lib.rs              # Crate entry, public exports
├── config.rs           # BleConfig, profiles
├── error.rs            # BleError, Result alias
├── transport.rs        # MeshTransport trait
│
├── Core Data Layer
│   ├── document.rs         # PeatDocument wire format
│   ├── document_sync.rs    # DocumentSync state management
│   └── sync/
│       ├── crdt.rs         # GCounter, LWW, EmergencyEvent
│       ├── protocol.rs     # Sync message encoding
│       ├── batch.rs        # Operation batching
│       └── delta.rs        # Delta encoding
│
├── Peer Management Layer
│   ├── peer.rs             # PeatPeer, SignalStrength
│   ├── peer_manager.rs     # PeerManager (thread-safe)
│   ├── observer.rs         # PeatEvent, PeatObserver trait
│   └── peat_mesh.rs        # PeatMesh facade (std only)
│
├── BLE Service Layer
│   ├── discovery/
│   │   ├── advertiser.rs   # Beacon building
│   │   ├── scanner.rs      # Device scanning
│   │   └── beacon.rs       # PeatBeacon parsing
│   └── gatt/
│       ├── service.rs      # GATT service definition
│       ├── characteristics.rs  # Characteristic UUIDs
│       └── protocol.rs     # Fragmentation/reassembly
│
├── Radio Management Layer
│   ├── phy/
│   │   ├── controller.rs   # PHY selection
│   │   └── strategy.rs     # Adaptive PHY
│   └── power/
│       ├── profile.rs      # Power profiles
│       └── scheduler.rs    # Radio scheduling
│
├── Mesh Topology Layer
│   └── mesh/
│       ├── manager.rs      # Topology management
│       ├── routing.rs      # Message routing
│       └── topology.rs     # Network graph
│
└── Platform Abstraction Layer
    └── platform/
        ├── mod.rs          # BleAdapter trait
        ├── apple/          # iOS/macOS (CoreBluetooth)
        ├── android/        # Android (JNI)
        ├── linux/          # Linux (BlueZ/bluer)
        ├── windows.rs      # Windows (WinRT)
        ├── esp32.rs        # ESP32 (NimBLE)
        └── embedded.rs     # Bare-metal
```

### 1.2 Layer Dependencies

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  (iOS PeatTest, M5Stack firmware, Linux daemon)             │
├─────────────────────────────────────────────────────────────┤
│                    PeatMesh Facade                           │
│  Unified API: discovery, sync, events                       │
├─────────────────────────────────────────────────────────────┤
│  PeerManager  │  DocumentSync  │  ObserverManager           │
├───────────────┼────────────────┼─────────────────────────────┤
│  PeatPeer     │  PeatDocument  │  PeatEvent                 │
│               │  CRDT Layer    │  PeatObserver              │
├─────────────────────────────────────────────────────────────┤
│  Discovery    │  GATT Service  │  PHY/Power                 │
│  Advertiser   │  Sync Protocol │  Radio Scheduler           │
├─────────────────────────────────────────────────────────────┤
│                    Platform Abstraction                      │
│  BleAdapter trait (iOS, Android, Linux, ESP32, Windows)     │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Public API Surface Analysis

### 2.1 Primary Entry Points

**High-Level (Recommended)**:
```rust
use peat_btle::{PeatMesh, PeatMeshConfig, PeatEvent, PeatObserver};

let config = PeatMeshConfig::new(node_id, "ALPHA-1", "DEMO");
let mesh = PeatMesh::new(config);
mesh.add_observer(Arc::new(my_observer));

// Platform callbacks
mesh.on_ble_discovered(...);
mesh.on_ble_connected(...);
mesh.on_ble_data_received(...);

// Periodic maintenance
if let Some(data) = mesh.tick(now_ms) {
    broadcast_to_peers(&data);
}
```

**Low-Level (Platform Builders)**:
```rust
use peat_btle::{BleConfig, BleAdapter, BluetoothLETransport};

let config = BleConfig::peat_lite(node_id);
let adapter = platform::linux::BlueZAdapter::new()?;
let transport = BluetoothLETransport::new(config, adapter);
```

### 2.2 Export Inventory

| Category | Count | Notes |
|----------|-------|-------|
| Core types | 8 | NodeId, PeatDocument, PeatPeer, etc. |
| Configuration | 12 | BleConfig, PowerProfile, etc. |
| Traits | 6 | BleAdapter, PeatObserver, MeshTransport |
| Events | 11 | PeatEvent variants |
| Platform | 5 | Adapter impls (conditional) |
| **Total public** | ~60 | Many are implementation details |

### 2.3 API Issues Identified

1. **Over-exposure**: Semi-internal types like `SyncProtocol`, `MeshRouter` exported
2. **No prelude**: Common imports require 5+ use statements
3. **Inconsistent naming**: `PeatEvent` vs `BleError` (prefix inconsistency)
4. **Missing builder patterns**: Configuration structs lack fluent builders
5. **Weak type safety**: Identifiers passed as `String` across boundaries

---

## 3. Platform Integration Patterns

### 3.1 Current Implementations

| Platform | Adapter | Status | BLE Stack |
|----------|---------|--------|-----------|
| iOS/macOS | CoreBluetoothAdapter | Partial | CoreBluetooth (objc2) |
| Android | AndroidBleAdapter | Partial | JNI to Android BT |
| Linux | BlueZAdapter | Partial | bluer (D-Bus) |
| Windows | WinRTAdapter | Stub | WinRT Bluetooth |
| ESP32 | NimBLE (external) | Complete | esp-idf NimBLE |

### 3.2 Integration Gap Analysis

**iOS (PeatTest app)**:
- Maintains separate `PeatBLEManager` (CoreBluetooth wrapper)
- Does NOT use `PeatMesh` for peer tracking
- Manually parses device names
- Duplicates connection state management
- **Gap**: No clean path from CoreBluetooth callbacks → PeatMesh

**ESP32 (M5Stack)**:
- Uses `PeatMesh` correctly
- Implements gossip manually (`nimble::gossip_document()`)
- Custom NVS persistence (`DocumentStore`)
- **Gap**: No framework gossip strategy, no persistence abstraction

**Common Gaps**:
1. No unified BLE event → PeatMesh routing
2. No persistence abstraction (each platform reimplements)
3. No gossip/flooding strategy in framework
4. No connection state machine abstraction

---

## 4. CRDT & Sync Protocol

### 4.1 Current CRDT Types

| Type | Purpose | Merge Semantics |
|------|---------|-----------------|
| GCounter | Activity tracking | Sum per-node values |
| LWW-Register | Single values | Latest timestamp wins |
| Peripheral | Node metadata | LWW for each field |
| EmergencyEvent | Alert + ACKs | OR-set for ACK map |

### 4.2 Wire Format

```
PeatDocument (variable size):
┌──────────────────────────────────────────────┐
│ Header (8 bytes)                             │
│   version: u32 (LE)                          │
│   node_id: u32 (LE)                          │
├──────────────────────────────────────────────┤
│ GCounter (4 + N*12 bytes)                    │
│   num_entries: u32                           │
│   entries[]: { node_id: u32, count: u64 }    │
├──────────────────────────────────────────────┤
│ Extended Section (optional, marker 0xAB)     │
│   Peripheral data (callsign, location, etc.) │
├──────────────────────────────────────────────┤
│ Emergency Section (optional, marker 0xAC)    │
│   EmergencyEvent with ACK map                │
└──────────────────────────────────────────────┘
```

### 4.3 Sync Protocol Assessment

**Strengths**:
- Compact binary format
- CRDT semantics prevent conflicts
- Emergency/ACK flow works across multi-hop

**Weaknesses**:
- No versioning for format changes
- No compression for large ACK maps
- No partial sync (always full document)

### 4.4 Document Size Constraints

#### BLE MTU Limits

| MTU Type | Payload | Notes |
|----------|---------|-------|
| Default | 20 bytes | ATT header takes 3 of 23 |
| Extended | 244 bytes | Typical negotiated |
| Maximum | 512 bytes | BLE 5.0+ |

**Recommendation**: Design for 244-byte payloads with fallback fragmentation.

#### PeatDocument Size Calculation

```
Size = 8 (header)
     + 4 + (N × 12) (GCounter, N = active nodes)
     + 4 + 38 (Peripheral, if present)
     + 4 + 8 + ceil(M/8) (Emergency + ACK bitmap, M = known peers)

Examples:
  5 nodes, no emergency:   8 + 64 + 42 = 114 bytes ✓
  10 nodes, no emergency:  8 + 124 + 42 = 174 bytes ✓
  20 nodes + emergency:    8 + 244 + 42 + 15 = 309 bytes ⚠️
  50 nodes + emergency:    8 + 604 + 42 + 19 = 673 bytes ✗
```

#### Maximum Mesh Size Recommendations

| Scenario | Max Nodes | Rationale |
|----------|-----------|-----------|
| Single-hop mesh | 20 | Fits in extended MTU |
| Multi-hop mesh | 15 | Leave room for emergency |
| Resource-constrained | 10 | ESP32 memory limits |

#### CRDT Optimization Guidelines

1. **GCounter Pruning**: Nodes that haven't incremented recently can be pruned
   - Implement TTL for inactive node entries
   - Prune during sync when document exceeds threshold

2. **Emergency ACK Compression**: Use run-length encoding for sparse ACK maps
   - Most emergencies ACK quickly → bitmap is mostly 1s
   - Compress consecutive 0xFF bytes

3. **Delta Sync** (Future): Only send changed CRDT entries
   - Track last-sync-version per peer
   - Send delta from peer's known version

4. **Fragmentation Protocol** (Future): For documents exceeding MTU
   ```
   Fragment Header (3 bytes):
     fragment_id: u8
     fragment_num: u8
     total_fragments: u8
   ```

#### Constants to Add to `document.rs`

```rust
/// Maximum recommended mesh size for reliable sync
pub const MAX_MESH_SIZE: usize = 20;

/// Target document size for single-packet transmission
pub const TARGET_DOCUMENT_SIZE: usize = 244;

/// Hard limit before fragmentation is required
pub const MAX_DOCUMENT_SIZE: usize = 512;
```

---

## 5. Testing Assessment

### 5.1 Current Coverage

| Module | Tests | Coverage |
|--------|-------|----------|
| peer_manager.rs | 9 | Good |
| document_sync.rs | 9 | Good |
| peat_mesh.rs | 9 | Good |
| sync/crdt.rs | 19 | Excellent |
| sync/protocol.rs | 10 | Good |
| observer.rs | 2 | Minimal |
| **Platform adapters** | 0 | **Critical gap** |
| **Discovery** | 0 | **Gap** |
| **GATT** | 0 | **Gap** |
| **Power** | 0 | **Gap** |

**Total**: ~286 tests, ~35-40% estimated line coverage

### 5.2 Testing Infrastructure

**Available**:
- `CollectingObserver` for event capture
- Inline `#[cfg(test)]` modules
- Standard Rust test framework

**Missing**:
- MockBleAdapter for platform-agnostic testing
- Integration test directory (`tests/`)
- Functional test documentation
- CI for ARM64 Linux builds

### 5.3 Recommended Test Strategy

| Level | Scope | Approach |
|-------|-------|----------|
| Unit | Individual functions | Existing + expand to untested modules |
| Integration | Multi-component flows | New `tests/` directory with MockBleAdapter |
| Functional | Real hardware | Manual procedures documented |

---

## 6. Documentation Assessment

### 6.1 Current State

| Type | Status | Quality |
|------|--------|---------|
| Crate-level rustdoc | Exists | Excellent |
| Module docs | Most modules | Good |
| README | Comprehensive | Good |
| Examples directory | **Missing** | N/A |
| Platform guides | **Missing** | N/A |
| Troubleshooting | **Missing** | N/A |

### 6.2 Documentation Gaps

1. **No runnable examples**: README mentions examples that don't exist
2. **No platform integration guides**: Developers must reverse-engineer
3. **No CRDT merge semantics spec**: Informal descriptions only
4. **No troubleshooting**: Common BLE issues undocumented

---

## 7. Identified Anti-Patterns

### 7.1 Leaky Abstractions

**Issue**: iOS app bypasses PeatMesh entirely
```swift
// PeatViewModel.swift - duplicates peer tracking
var discoveredPeripherals: [String: CBPeripheral] = [:]
var connectedPeripherals: [String: CBPeripheral] = [:]
```

**Fix**: Provide clear callback → PeatMesh routing pattern

### 7.2 Manual Gossip

**Issue**: ESP32 implements gossip manually
```rust
// main.rs - ad-hoc gossip
if result.counter_changed || result.emergency_changed {
    let encoded = mesh.build_document();
    nimble::gossip_document(&encoded);
}
```

**Fix**: Add `GossipStrategy` trait and default implementations

### 7.3 Platform-Specific Persistence

**Issue**: Each platform implements storage differently
```rust
// ESP32: NVS
store.save(&mesh);

// iOS: Would need Keychain/UserDefaults
// Android: Would need SharedPreferences
// Linux: Would need file system
```

**Fix**: Add `DocumentStore` trait abstraction

### 7.4 Stringly-Typed Identifiers

**Issue**: BLE identifiers passed as raw strings
```rust
fn on_ble_discovered(identifier: &str, ...) // UUID? MAC? Handle?
```

**Fix**: Create newtype wrappers per platform

---

## 8. Prioritized Recommendations

### P0: Critical (Enable CI/Testing)

1. **Create MockBleAdapter** (`platform/mock.rs`)
   - Enables unit testing without hardware
   - Simulates discovery, connection, data exchange
   - Required for CI reliability

2. **Add Integration Tests** (`tests/`)
   - Multi-mesh sync scenarios
   - Emergency/ACK propagation
   - Peer lifecycle flows

### P1: High (Framework Completeness)

3. **Add Persistence Trait** (`persistence.rs`)
   ```rust
   pub trait DocumentStore: Send + Sync {
       fn save(&self, doc: &PeatDocument) -> Result<()>;
       fn load(&self) -> Result<Option<PeatDocument>>;
   }
   ```

4. **Add Gossip Strategy** (`gossip.rs`)
   ```rust
   pub trait GossipStrategy: Send + Sync {
       fn should_forward(&self, result: &MergeResult) -> bool;
       fn select_peers<'a>(&self, peers: &'a [PeatPeer]) -> Vec<&'a PeatPeer>;
   }
   ```

5. **Fix iOS PeatMesh Integration**
   - Route CoreBluetooth callbacks → PeatMesh
   - Remove duplicate peer tracking
   - Use document-based state

### P2: Medium (Developer Experience)

6. **Create Examples Directory**
   - `basic_mesh.rs` - Minimal node
   - `emergency_demo.rs` - Alert flow
   - `custom_observer.rs` - Event handling

7. **Write Platform Guides**
   - iOS/macOS integration
   - Android JNI setup
   - ESP32 NimBLE integration
   - Linux BlueZ setup
   - Raspberry Pi specifics

8. **Add Prelude Module**
   ```rust
   pub mod prelude {
       pub use crate::{PeatMesh, PeatMeshConfig, PeatEvent, PeatObserver};
       pub use crate::{PeatPeer, NodeId, Result};
   }
   ```

### P3: Low (Polish)

9. **Document Functional Tests**
   - Multi-device test procedures
   - Cross-platform interop matrix
   - Performance benchmarks

10. **Enhance API Documentation**
    - Add examples to all public types
    - Document CRDT merge semantics formally
    - Add troubleshooting section

---

## 9. Execution Roadmap

```
Week 1: Foundation
├── Create MockBleAdapter
├── Add persistence trait
└── Add gossip strategy trait

Week 2: Testing
├── Integration test directory
├── Unit tests for discovery/GATT
└── Document functional tests

Week 3: Examples & Guides
├── Create examples directory
├── Write platform guides
└── Add prelude module

Week 4: Polish
├── Fix iOS PeatMesh integration
├── API documentation enhancements
└── Final review and cleanup
```

---

## 10. Appendix: File Inventory

### Files to Create
- `peat-btle/src/prelude.rs`
- `peat-btle/src/persistence.rs`
- `peat-btle/src/gossip.rs`
- `peat-btle/src/platform/mock.rs`
- `peat-btle/tests/mesh_sync.rs`
- `peat-btle/tests/emergency_flow.rs`
- `peat-btle/tests/peer_lifecycle.rs`
- `peat-btle/examples/basic_mesh.rs`
- `peat-btle/examples/emergency_demo.rs`
- `peat-btle/examples/custom_observer.rs`
- `docs/testing/functional-test-plan.md`
- `docs/platform-guides/ios.md`
- `docs/platform-guides/android.md`
- `docs/platform-guides/esp32.md`
- `docs/platform-guides/linux.md`
- `docs/platform-guides/raspberry-pi.md`

### Files to Modify
- `peat-btle/src/lib.rs` - Add prelude, persistence, gossip exports
- `peat-btle/src/peat_mesh.rs` - Integrate new traits
- `peat-btle/src/platform/mod.rs` - Add mock feature
- `peat-btle/ios/peat-apple-ffi/src/lib.rs` - Expose full API
- `peat-btle/README.md` - Update with examples

---

## 11. CI/CD Infrastructure

### 11.1 Current CI Gaps

- No ARM64 Linux builds
- No hardware-in-loop BLE testing
- No multi-device test orchestration
- Limited to mock/unit tests only

### 11.2 Proposed: Self-Hosted RPi Runner

**Hardware Setup**:
```
┌─────────────────────────────────────────────────────────┐
│  Raspberry Pi 4 (8GB) - Self-Hosted GitHub Runner       │
│  ├── Built-in BLE adapter (for testing)                │
│  ├── USB BLE adapter #2 (second test node)             │
│  └── USB-connected M5Stack Core2 (ESP32 test target)   │
└─────────────────────────────────────────────────────────┘
```

**Capabilities**:
- ARM64 native builds (`aarch64-unknown-linux-gnu`)
- BlueZ BLE testing with real hardware
- Multi-node mesh formation tests
- ESP32 firmware flashing and testing
- Cross-platform interop validation

**Runner Configuration**:
```yaml
# .github/workflows/hardware-tests.yml
jobs:
  arm64-build:
    runs-on: [self-hosted, linux, ARM64, ble]
    steps:
      - uses: actions/checkout@v4
      - name: Build peat-btle (ARM64)
        run: cargo build --features linux
      - name: Run BLE integration tests
        run: cargo test --features linux,ble-hardware
```

**Required Setup**:
1. Raspberry Pi OS 64-bit (Bookworm+)
2. BlueZ 5.50+ for extended advertising
3. GitHub Actions runner installed
4. BLE adapters whitelisted for runner user
5. espflash for ESP32 firmware deployment

### 11.3 Test Orchestration

**Multi-Device Test Flow**:
```
1. Runner starts test
2. Flash ESP32 with test firmware
3. Start Linux BLE node on RPi
4. Wait for mesh formation
5. Trigger emergency on one node
6. Verify propagation to all nodes
7. Collect logs and assert
```

**Labels for Runner**:
- `self-hosted` - Required for self-hosted
- `linux` - OS type
- `ARM64` - Architecture
- `ble` - Has BLE hardware
- `esp32` - Can flash ESP32 devices

---

*This is a living document. Update as implementation progresses.*
