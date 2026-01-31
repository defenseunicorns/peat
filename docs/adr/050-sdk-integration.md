# ADR-050: HIVE SDK Integration (Optimal Path)

**Status**: Proposed  
**Date**: 2025-01-31  
**Authors**: Kit Plummer, Codex  
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)  
**Depends On**: ADR-049 (Schema Extraction)  
**Relates To**: ADR-043 (Consumer Interface Adapters - Compatibility Path)

---

## Executive Summary

This ADR defines the **HIVE SDK** (`hive-sdk`) - the **optimal integration path** for systems that can incorporate HIVE directly. Unlike the consumer interface adapters (ADR-043), SDK integration provides:

- **Full CRDT synchronization** with eventual consistency guarantees
- **Offline operation** with automatic reconnection and sync
- **Hierarchical participation** as a first-class HIVE node
- **Minimal latency** (sync latency only, no adapter overhead)
- **Native capability aggregation** and cell membership

> **⚠️ GUIDANCE**: If you can modify your system's software, use the SDK. The consumer interface adapters (ADR-043) exist only for legacy systems that cannot be modified.

---

## Context

### Why Direct Integration Matters

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         HIVE Mesh                                        │
│                                                                          │
│    ┌──────────┐      ┌──────────┐      ┌──────────┐      ┌──────────┐  │
│    │  Squad   │◄────►│  UAS-1   │◄────►│  UGV-2   │◄────►│  Human   │  │
│    │  Leader  │      │(hive-sdk)│      │(hive-sdk)│      │(hive-sdk)│  │
│    └──────────┘      └──────────┘      └──────────┘      └──────────┘  │
│         ▲                                                               │
│         │ All nodes are equal CRDT participants                         │
│         │ • Sync directly with peers                                    │
│         │ • Operate offline                                             │
│         │ • Participate in hierarchy                                    │
│         ▼                                                               │
│    ┌──────────┐                                                         │
│    │ Platoon  │                                                         │
│    │ Leader   │                                                         │
│    └──────────┘                                                         │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘

vs.

┌─────────────────────────────────────────────────────────────────────────┐
│                   Consumer Interface Architecture                        │
│                                                                          │
│    ┌──────────┐      ┌──────────┐                                       │
│    │  HIVE    │◄────►│  HIVE    │                                       │
│    │  Node    │      │  Node    │◄───HTTP/WS───► Legacy System          │
│    └──────────┘      └──────────┘                (not a real participant)│
│                           │                                              │
│                           │ Legacy system:                               │
│                           │ • Cannot sync directly                       │
│                           │ • No offline operation                       │
│                           │ • Not in hierarchy                           │
│                           │ • +50-200ms latency                          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Comparison: SDK vs Consumer Interface Adapters

| Capability | SDK (This ADR) | Consumer Adapters (ADR-043) |
|------------|----------------|----------------------------|
| **Latency** | Sync only (~10-50ms) | +50-200ms overhead |
| **Offline Operation** | ✅ Full - queues changes, syncs on reconnect | ❌ None - requires adapter |
| **CRDT Conflict Resolution** | ✅ Automatic, deterministic | ❌ Last-write-wins at adapter |
| **Hierarchical Membership** | ✅ Full cell participation | ❌ Not a cell member |
| **Capability Aggregation** | ✅ Contributes to cell capabilities | ❌ Not aggregated |
| **Bandwidth Efficiency** | ✅ Delta sync only | ❌ Full message per request |
| **Peer-to-Peer** | ✅ Direct sync with any peer | ❌ Must go through adapter |
| **Multi-Transport** | ✅ Iroh, BLE, LoRa, etc. | ❌ HTTP/WS/TCP only |

### Target Platforms

The SDK targets systems where HIVE can be embedded:

| Platform | Language Binding | Use Case |
|----------|------------------|----------|
| Linux (x86_64, aarch64) | Rust native, Python, Go, C++ | Servers, edge compute, Jetson |
| Android | Kotlin/Java via JNI | Tablets, ATAK plugins, phones |
| iOS | Swift via UniFFI | iPhones, iPads |
| Embedded Linux | Rust native, Go | Drones, robots, sensors |
| ROS2 | Rust native + ROS2 bridge | Robotic platforms |
| Windows | Rust native, C#, Python, Go | Desktop C2, WinTAK plugins |
| WASM | Rust → WASM | Browser-based dashboards |
| Kubernetes/Cloud Native | Go | Zarf/UDS integration, operators, controllers |

---

## Decision

### SDK Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            hive-sdk                                      │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     Language Bindings                               │ │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐      │ │
│  │  │  Rust   │ │ Python  │ │   Go    │ │ Kotlin  │ │  Swift  │      │ │
│  │  │ (native)│ │ (PyO3)  │ │ (cgo)   │ │  (JNI)  │ │(UniFFI) │      │ │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘      │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                     │
│                                    ▼                                     │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     High-Level API                                  │ │
│  │                                                                     │ │
│  │  • HiveNode - main entry point                                     │ │
│  │  • Platform - represent this platform's state                      │ │
│  │  • Cell - cell membership and queries                              │ │
│  │  • Capabilities - advertise and discover                           │ │
│  │  • Commands - send and receive                                     │ │
│  │  • Subscriptions - reactive state updates                          │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                     │
│                                    ▼                                     │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     Core Components                                 │ │
│  │                                                                     │ │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐    │ │
│  │  │  hive-protocol  │  │   hive-schema   │  │  MeshProvider   │    │ │
│  │  │                 │  │   (ADR-049)     │  │                 │    │ │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                     │
│                                    ▼                                     │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                     Transport Layer (ADR-032)                       │ │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐                  │ │
│  │  │  Iroh   │ │hive-btle│ │  LoRa   │ │ Custom  │                  │ │
│  │  │ (QUIC)  │ │  (BLE)  │ │         │ │         │                  │ │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘                  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### High-Level API Design

#### Rust (Native)

```rust
// hive-sdk/src/lib.rs

use hive_schema::hive::v1::*;
use std::sync::Arc;

/// Main entry point for HIVE integration
pub struct HiveNode {
    platform: Platform,
    config: HiveConfig,
}

impl HiveNode {
    /// Create a new HIVE node with the given configuration
    pub async fn new(config: HiveConfig) -> Result<Self, HiveError> {
        let platform = Platform::new(&config.platform_id);
        Ok(Self { platform, config })
    }
    
    /// Start the node and begin mesh participation
    pub async fn start(&self) -> Result<(), HiveError> {
        self.platform.start_beacon_broadcast().await?;
        Ok(())
    }
    
    /// Get this node's platform handle
    pub fn platform(&self) -> &Platform {
        &self.platform
    }
    
    /// Query platforms in the mesh
    pub async fn platforms(&self) -> PlatformQuery {
        PlatformQuery::new()
    }
    
    /// Query cells in the mesh
    pub async fn cells(&self) -> CellQuery {
        CellQuery::new()
    }
    
    /// Subscribe to state changes
    pub fn subscribe(&self) -> SubscriptionBuilder {
        SubscriptionBuilder::new()
    }
    
    /// Send a command to another platform or cell
    pub async fn command(&self, cmd: Command) -> Result<CommandReceipt, HiveError> {
        todo!()
    }
}

/// Represents this platform in the mesh
pub struct Platform {
    id: NodeId,
}

impl Platform {
    /// Update this platform's position
    pub async fn set_position(&self, position: Position) -> Result<(), HiveError> {
        Ok(())
    }
    
    /// Update this platform's operational status
    pub async fn set_operational(&self, operational: bool) -> Result<(), HiveError> {
        Ok(())
    }
    
    /// Advertise a capability
    pub async fn advertise_capability(&self, cap: CapabilityAdvertisement) -> Result<(), HiveError> {
        Ok(())
    }
    
    /// Remove a capability advertisement
    pub async fn remove_capability(&self, capability_id: &str) -> Result<(), HiveError> {
        Ok(())
    }
    
    /// Get this platform's current cell membership
    pub async fn cell(&self) -> Option<CellId> {
        None
    }
}

/// Fluent query builder for platforms
pub struct PlatformQuery {
    spatial: Option<SpatialFilter>,
    capability: Option<String>,
    operational_only: bool,
}

impl PlatformQuery {
    pub fn new() -> Self {
        Self {
            spatial: None,
            capability: None,
            operational_only: false,
        }
    }
    
    /// Filter to platforms within radius of a point
    pub fn within_radius(mut self, center: Position, radius_meters: f64) -> Self {
        self.spatial = Some(SpatialFilter::WithinRadius { center, radius_meters });
        self
    }
    
    /// Filter to platforms with a specific capability
    pub fn with_capability(mut self, capability_type: &str) -> Self {
        self.capability = Some(capability_type.to_string());
        self
    }
    
    /// Filter to operational platforms only
    pub fn operational(mut self) -> Self {
        self.operational_only = true;
        self
    }
    
    /// Execute the query and return results
    pub async fn execute(&self) -> Result<Vec<PlatformBeacon>, HiveError> {
        Ok(vec![])
    }
}

pub enum SpatialFilter {
    WithinRadius { center: Position, radius_meters: f64 },
    WithinBounds { min: Position, max: Position },
}
```

#### Python (PyO3)

```python
# hive_sdk/__init__.py

import asyncio

async def main():
    # Create and start a HIVE node
    config = HiveConfig(
        platform_id="uav-001",
        mesh_backend="automerge",  # or "ditto"
        transports=["iroh", "ble"],
    )
    
    node = await HiveNode.create(config)
    await node.start()
    
    # Update our position
    await node.platform.set_position(Position(
        latitude=37.7749,
        longitude=-122.4194,
        altitude_meters=100.0,
    ))
    
    # Advertise a capability
    await node.platform.advertise_capability(Capability(
        capability_type="sensor/camera/rgb",
        parameters={
            "resolution": "4K",
            "frame_rate": 30,
        }
    ))
    
    # Query nearby platforms
    nearby = await node.platforms() \
        .within_radius(node.platform.position, 5000) \
        .with_capability("sensor/camera") \
        .execute()
    
    for platform in nearby:
        print(f"Found: {platform.id} at {platform.position}")
    
    # Subscribe to changes
    async for update in node.platforms().subscribe():
        print(f"Platform update: {update}")

if __name__ == "__main__":
    asyncio.run(main())
```

#### Kotlin (Android/JNI)

```kotlin
// HiveSDK.kt

package com.revolveteam.hive

import kotlinx.coroutines.flow.Flow

class HiveNode private constructor(
    private val native: Long  // JNI pointer
) {
    companion object {
        suspend fun create(config: HiveConfig): HiveNode {
            return HiveNode(nativeCreate(config))
        }
        
        private external fun nativeCreate(config: HiveConfig): Long
    }
    
    val platform: Platform = Platform(this)
    
    suspend fun start() = nativeStart(native)
    
    fun platforms(): PlatformQuery = PlatformQuery(this)
    
    fun cells(): CellQuery = CellQuery(this)
    
    suspend fun command(cmd: Command): CommandReceipt = 
        nativeSendCommand(native, cmd)
    
    private external fun nativeStart(ptr: Long)
    private external fun nativeSendCommand(ptr: Long, cmd: Command): CommandReceipt
}

// Usage in Android Activity/ViewModel
class DroneViewModel : ViewModel() {
    private lateinit var hive: HiveNode
    
    fun initialize() {
        viewModelScope.launch {
            hive = HiveNode.create(HiveConfig(
                platformId = "android-${Build.SERIAL}",
                meshBackend = MeshBackend.AUTOMERGE,
                transports = listOf(Transport.IROH, Transport.BLE),
            ))
            hive.start()
            
            // Observe nearby platforms
            hive.platforms()
                .withinRadius(currentPosition, 5000.0)
                .subscribe()
                .collect { platform ->
                    _nearbyPlatforms.value += platform
                }
        }
    }
}
```

#### Go (cgo)

```go
// hive-sdk-go/hive.go

package hive

/*
#cgo LDFLAGS: -lhive_sdk
#include "hive_sdk.h"
*/
import "C"
import (
    "context"
    "encoding/json"
    "unsafe"
)

// HiveNode represents a HIVE mesh participant
type HiveNode struct {
    ptr unsafe.Pointer
}

// Config for creating a HiveNode
type Config struct {
    PlatformID   string      `json:"platform_id"`
    MeshBackend  string      `json:"mesh_backend"` // "automerge" or "ditto"
    Transports   []string    `json:"transports"`   // ["iroh", "ble"]
    BeaconInterval int       `json:"beacon_interval_secs"`
}

// NewHiveNode creates a new HIVE node
func NewHiveNode(cfg Config) (*HiveNode, error) {
    cfgJSON, _ := json.Marshal(cfg)
    cConfig := C.CString(string(cfgJSON))
    defer C.free(unsafe.Pointer(cConfig))
    
    ptr := C.hive_node_create(cConfig)
    if ptr == nil {
        return nil, fmt.Errorf("failed to create HiveNode")
    }
    
    return &HiveNode{ptr: ptr}, nil
}

// Start begins mesh participation
func (h *HiveNode) Start(ctx context.Context) error {
    result := C.hive_node_start(h.ptr)
    if result != 0 {
        return fmt.Errorf("failed to start: %d", result)
    }
    return nil
}

// SetPosition updates this platform's position
func (h *HiveNode) SetPosition(lat, lon, alt float64) error {
    result := C.hive_platform_set_position(h.ptr, C.double(lat), C.double(lon), C.double(alt))
    if result != 0 {
        return fmt.Errorf("failed to set position: %d", result)
    }
    return nil
}

// Platform represents a discovered platform
type Platform struct {
    ID        string   `json:"id"`
    Latitude  float64  `json:"latitude"`
    Longitude float64  `json:"longitude"`
    Altitude  float64  `json:"altitude"`
    Operational bool   `json:"operational"`
}

// QueryPlatforms returns platforms matching the query
func (h *HiveNode) QueryPlatforms(opts QueryOpts) ([]Platform, error) {
    optsJSON, _ := json.Marshal(opts)
    cOpts := C.CString(string(optsJSON))
    defer C.free(unsafe.Pointer(cOpts))
    
    var resultLen C.int
    resultPtr := C.hive_query_platforms(h.ptr, cOpts, &resultLen)
    if resultPtr == nil {
        return nil, fmt.Errorf("query failed")
    }
    defer C.free(unsafe.Pointer(resultPtr))
    
    resultJSON := C.GoStringN(resultPtr, resultLen)
    var platforms []Platform
    json.Unmarshal([]byte(resultJSON), &platforms)
    
    return platforms, nil
}

// SubscribePlatforms returns a channel of platform updates
func (h *HiveNode) SubscribePlatforms(ctx context.Context) (<-chan Platform, error) {
    ch := make(chan Platform, 100)
    
    go func() {
        defer close(ch)
        // Implementation uses C callback mechanism
        // ...
    }()
    
    return ch, nil
}

// Close shuts down the node
func (h *HiveNode) Close() error {
    C.hive_node_destroy(h.ptr)
    h.ptr = nil
    return nil
}
```

```go
// Example usage: Zarf/UDS operator integration
package main

import (
    "context"
    "log"
    "os"
    "os/signal"
    
    "github.com/revolveteam/hive-sdk-go"
)

func main() {
    ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
    defer cancel()
    
    // Create HIVE node for this operator instance
    node, err := hive.NewHiveNode(hive.Config{
        PlatformID:  os.Getenv("HOSTNAME"),
        MeshBackend: "automerge",
        Transports:  []string{"iroh"},
    })
    if err != nil {
        log.Fatal(err)
    }
    defer node.Close()
    
    if err := node.Start(ctx); err != nil {
        log.Fatal(err)
    }
    
    // Subscribe to platform updates
    platforms, _ := node.SubscribePlatforms(ctx)
    
    for {
        select {
        case <-ctx.Done():
            return
        case p := <-platforms:
            log.Printf("Platform update: %s at (%f, %f)", p.ID, p.Latitude, p.Longitude)
            // Update Kubernetes resources, trigger reconciliation, etc.
        }
    }
}
```

#### ROS2 Integration

```rust
// hive-ros2-bridge/src/lib.rs

use hive_sdk::HiveNode;

/// ROS2 bridge that publishes HIVE state as ROS2 topics
/// and subscribes to ROS2 topics to update HIVE state
pub struct HiveRos2Bridge {
    hive: HiveNode,
    // ROS2 node, publishers, subscribers...
}

impl HiveRos2Bridge {
    pub async fn new(hive_config: HiveConfig) -> Result<Self> {
        let hive = HiveNode::new(hive_config).await?;
        Ok(Self { hive })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.hive.start().await?;
        
        loop {
            tokio::select! {
                // ROS2 odometry → HIVE position
                // HIVE platform updates → ROS2 topics
            }
        }
    }
}
```

---

## Configuration

```rust
/// SDK configuration
#[derive(Debug, Clone)]
pub struct HiveConfig {
    /// Unique identifier for this platform
    pub platform_id: String,
    
    /// Human-readable name
    pub platform_name: Option<String>,
    
    /// Mesh backend selection
    pub mesh_backend: MeshBackend,
    
    /// Enabled transports (in priority order)
    pub transports: Vec<TransportConfig>,
    
    /// Beacon broadcast interval
    pub beacon_interval: Duration,
    
    /// Initial capabilities to advertise
    pub initial_capabilities: Vec<CapabilityAdvertisement>,
    
    /// Hierarchy participation level
    pub hierarchy_level: HierarchyLevel,
    
    /// Storage path for offline data
    pub storage_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum MeshBackend {
    /// Open source Automerge + Iroh (recommended)
    Automerge,
    /// Commercial Ditto SDK
    Ditto { app_id: String, token: String },
}

#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// QUIC-based P2P via Iroh
    Iroh { 
        bind_port: Option<u16>,
        relay_url: Option<String>,
    },
    /// Bluetooth Low Energy mesh
    Ble {
        advertise: bool,
        scan: bool,
        coded_phy: bool,  // Long range mode
    },
    /// LoRa radio (requires hardware)
    LoRa {
        device: String,
        frequency_mhz: f64,
    },
}
```

---

## Implementation Plan

### Phase 1: Core Rust SDK (Week 1-3)

- [ ] Create `hive-sdk` crate
- [ ] Implement HiveNode, Platform, PlatformQuery
- [ ] Implement subscription system
- [ ] Unit tests with MockMeshProvider
- [ ] Integration tests with real mesh

**Deliverable**: Rust SDK functional with Automerge backend

### Phase 2: Python Bindings (Week 4-5)

- [ ] PyO3 bindings for core types
- [ ] Async support via pyo3-asyncio
- [ ] Python package structure (hive_sdk)
- [ ] PyPI publication pipeline
- [ ] Python examples and documentation

**Deliverable**: `pip install hive-sdk` works

### Phase 3: Go Bindings (Week 6-7)

- [ ] C header generation from Rust (cbindgen)
- [ ] Go package with cgo bindings
- [ ] Idiomatic Go API (channels for subscriptions, context for cancellation)
- [ ] Go module publication
- [ ] Zarf/UDS integration example
- [ ] Kubernetes operator example

**Deliverable**: `go get github.com/revolveteam/hive-sdk-go` works

### Phase 4: Mobile Bindings (Week 8-10)

- [ ] Kotlin/JNI bindings for Android
- [ ] Swift/UniFFI bindings for iOS
- [ ] Android AAR packaging
- [ ] iOS framework packaging
- [ ] Mobile-specific documentation

**Deliverable**: Android and iOS SDK packages available

### Phase 5: ROS2 Bridge (Week 11-12)

- [ ] hive-ros2-bridge crate
- [ ] Standard message conversions
- [ ] Launch file templates
- [ ] ROS2 Humble/Iron compatibility
- [ ] Integration with common robot platforms

**Deliverable**: ROS2 robots can join HIVE mesh

### Phase 6: Documentation & Examples (Week 13-14)

- [ ] Integration guide for each platform
- [ ] Example applications (drone, robot, mobile, operator)
- [ ] API reference documentation
- [ ] Performance tuning guide
- [ ] Troubleshooting guide

**Deliverable**: Complete developer documentation

---

## Success Criteria

1. **Rust Integration**: < 50 lines of code to join mesh and broadcast position
2. **Python Integration**: `pip install hive-sdk` + 10 lines to basic functionality
3. **Go Integration**: `go get` + idiomatic Go API with channels and context
4. **Android Integration**: AAR dependency + Kotlin coroutines API
5. **iOS Integration**: Swift Package Manager + async/await API
6. **ROS2 Integration**: Single launch file to bridge robot to HIVE
7. **Latency**: < 50ms position sync between SDK nodes (network permitting)
8. **Offline**: Survives 10-minute network partition, syncs on reconnect
9. **Documentation**: New developer productive in < 1 hour per language

---

## Consequences

### Positive

- **Optimal performance** - no adapter overhead
- **Full CRDT benefits** - offline, conflict resolution, eventual consistency
- **First-class citizen** - SDK nodes are full mesh participants
- **Multi-language** - Rust, Python, Go, Kotlin, Swift, C++
- **Cloud-native ready** - Go bindings enable Kubernetes operators, Zarf/UDS integration
- **Multi-transport** - same SDK works over Iroh, BLE, LoRa

### Negative

- **Integration effort** - requires modifying target system
- **Binary size** - SDK adds ~5-15MB depending on features
- **Platform support** - may not work on very constrained devices (use hive-btle Lite)
- **Learning curve** - developers must understand CRDT concepts
- **cgo overhead** - Go bindings have FFI overhead vs native Go

### Risks

- **Binding maintenance** - must keep language bindings in sync
- **Platform fragmentation** - different capabilities per platform
- **Version compatibility** - SDK version must match mesh protocol version

---

## Alternatives Considered

### 1. Consumer Adapters Only (No SDK)

**Pros**: Single integration point, simpler
**Cons**: Loses all CRDT benefits, adds latency, no offline
**Decision**: Rejected - defeats the purpose of HIVE's architecture

### 2. WASM-Only Cross-Platform

**Pros**: True write-once-run-anywhere
**Cons**: No native performance, limited system access, no BLE/LoRa
**Decision**: Rejected for native platforms, but WASM supported for browsers

### 3. REST Client Library (Not Full SDK)

**Pros**: Simpler implementation
**Cons**: Still goes through adapters, not a real mesh participant
**Decision**: Rejected - this is just the adapter path with nicer syntax

### 4. Pure Go Implementation (No cgo)

**Pros**: No FFI overhead, pure Go toolchain
**Cons**: Duplicate implementation, must reimplement Automerge/Iroh in Go
**Decision**: Rejected - maintain single Rust implementation, bind via cgo

---

## References

- [PyO3 - Rust bindings for Python](https://pyo3.rs/)
- [UniFFI - Rust bindings for mobile](https://mozilla.github.io/uniffi-rs/)
- [JNI - Java Native Interface](https://docs.oracle.com/javase/8/docs/technotes/guides/jni/)
- [cgo - Go C interop](https://pkg.go.dev/cmd/cgo)
- [cbindgen - C header generation from Rust](https://github.com/mozilla/cbindgen)
- [r2r - Rust ROS2 client](https://github.com/sequenceplanner/r2r)
- ADR-043: Consumer Interface Adapters (Compatibility Path)
- ADR-045: Zarf/UDS Integration
- ADR-049: hive-mesh Extraction

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-01-31 | SDK is optimal path, adapters are compatibility | Full CRDT benefits require direct participation |
| 2025-01-31 | Rust core with language bindings | Performance + safety + cross-platform |
| 2025-01-31 | PyO3 for Python | Best Rust-Python interop, async support |
| 2025-01-31 | UniFFI for mobile | Mozilla-backed, supports both iOS and Android |
| 2025-01-31 | ROS2 as dedicated bridge | Robotics is key use case, deserves first-class support |
