# ADR-025: Resource-Constrained Device Optimization (HIVE Lite)

**Status**: Proposed  
**Date**: 2025-12-11  
**Authors**: Kit Plummer, Codex  
**Relates To**: ADR-010 (Transport Layer), ADR-016 (TTL and Data Lifecycle), ADR-019 (QoS and Data Prioritization), ADR-024 (Flexible Hierarchy Strategies), ADR-026 (Protocol-Level Format Transformation Primitives)

## Context

### The Problem: Battery-Constrained Tactical Wearables

Customer feedback from Ascent (Alex Gorsuch) identified a critical pain point: **current sync solutions (specifically Ditto) drain battery rapidly on Samsung watches running WearTAK**. This represents a broader class of resource-constrained devices that need HIVE connectivity without the overhead of full mesh participation.

The operational reality for wearables:

```
Samsung Galaxy Watch Running WearTAK:
├─ Battery capacity: ~300-400 mAh
├─ Bluetooth LE radio: 5-10 mW active
├─ WiFi radio: 50-100 mW active
├─ CPU (Exynos): 100-500 mW under load
├─ Target mission duration: 12-24 hours
└─ Battery budget for sync: ~10% of total

Full HIVE Node Power Profile:
├─ Continuous sync heartbeats: Radio active 20% of time
├─ CRDT merge operations: CPU spikes every sync
├─ Full mesh state: Memory pressure → swap → battery
├─ Result: 3-4 hour battery life (unacceptable)

HIVE Lite Target Profile:
├─ Burst sync on connection: Radio active 2% of time
├─ Minimal state: Only upstream parent + own data
├─ Aggregated heartbeats: 60-second batched updates
├─ Result: 18-24 hour battery life (mission-capable)
```

### Resource Constraints Across Device Classes

| Device Class | CPU | RAM | Storage | Battery | Radio | Example |
|-------------|-----|-----|---------|---------|-------|---------|
| **Wearable** | 1-2 cores, 1GHz | 512MB-1GB | 4-16GB | 300-500 mAh | BLE/WiFi | Samsung Watch, Garmin |
| **Sensor Node** | MCU, 100MHz | 256KB-4MB | 1-16MB | Solar/Coin cell | LoRa/BLE | Environmental sensor |
| **Asset Tracker** | MCU, 50MHz | 64-256KB | 512KB | 500-2000 mAh | LTE-M/LoRa | GPS tracker |
| **Tactical EUD** | 4-8 cores, 2GHz | 4-8GB | 64-128GB | 4000-6000 mAh | Multi-band | ATAK phone |
| **Edge Compute** | 8+ cores, 2GHz+ | 8-32GB | 256GB+ | AC/Vehicle | Full stack | Jetson, vehicle server |

HIVE must support the full spectrum, with optimization profiles appropriate to each class.

### Why Ditto Drains Batteries

Based on analysis and customer feedback, Ditto's battery consumption stems from:

1. **Gossip-Based Sync**: Continuous peer discovery and state exchange
2. **Full Mesh Participation**: Every node maintains connections to multiple peers
3. **Opaque Sync Logic**: No visibility into what triggers radio wake
4. **Monolithic Design**: Can't disable features for constrained devices
5. **Background Activity**: Sync continues even when app is idle

### HIVE's Architectural Advantages

HIVE's hierarchical model enables natural optimization:

1. **Leaf Node Simplification**: Wearables are leaves, not mesh participants
2. **Differential Sync**: Only changed data transmits (95-99% reduction)
3. **Hierarchical Aggregation**: Parent handles mesh complexity
4. **Configurable Sync Intervals**: Batch updates for efficiency
5. **Transport Flexibility**: Choose optimal radio per device class

## Decision

### HIVE Lite: Resource-Constrained Device Profile

We introduce **HIVE Lite** as a configuration profile (not a separate codebase) that optimizes HIVE for resource-constrained devices through:

1. **Leaf-Only Operation**: No mesh routing, single parent connection
2. **Minimal CRDT State**: Only track own documents + immediate parent sync state
3. **Batched Sync Windows**: Aggregate changes, sync on schedule or threshold
4. **Reduced Protocol Overhead**: Simplified handshake, compressed payloads
5. **Aggressive Power Management**: Radio sleep, CPU idle between syncs

### Device Profile Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                    HIVE Device Profiles                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │  HIVE Full  │  │ HIVE Edge   │  │ HIVE Lite   │             │
│  │             │  │             │  │             │             │
│  │ • Full mesh │  │ • Limited   │  │ • Leaf only │             │
│  │ • All roles │  │   routing   │  │ • Single    │             │
│  │ • Unlimited │  │ • Squad     │  │   parent    │             │
│  │   state     │  │   leader    │  │ • Minimal   │             │
│  │ • Multi-    │  │ • Capped    │  │   state     │             │
│  │   parent    │  │   state     │  │ • Batched   │             │
│  │             │  │             │  │   sync      │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
│        │                │                │                      │
│    Servers         Phones/Tablets    Wearables/Sensors         │
│    Vehicles        Edge Compute      Asset Trackers            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### HIVE Lite Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    HIVE Lite Node                            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Minimal State Manager                     │  │
│  │  ┌──────────────┐  ┌──────────────┐                   │  │
│  │  │  Own Docs    │  │  Parent      │                   │  │
│  │  │  (position,  │  │  Sync State  │                   │  │
│  │  │   health,    │  │  (vector     │                   │  │
│  │  │   status)    │  │   clock)     │                   │  │
│  │  └──────────────┘  └──────────────┘                   │  │
│  └────────────────────────────────────────────────────────┘  │
│                           │                                   │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Batch Accumulator                         │  │
│  │  • Collects changes over sync window                  │  │
│  │  • Merges redundant updates (latest position only)    │  │
│  │  • Compresses for transmission                        │  │
│  └────────────────────────────────────────────────────────┘  │
│                           │                                   │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Power-Aware Sync Engine                   │  │
│  │  • Scheduled sync windows (configurable interval)     │  │
│  │  • Event-triggered sync (critical updates)            │  │
│  │  • Radio duty cycling (sleep between syncs)           │  │
│  │  • Battery-aware throttling                           │  │
│  └────────────────────────────────────────────────────────┘  │
│                           │                                   │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Single Parent Transport                   │  │
│  │  • BLE (wearable → phone)                             │  │
│  │  • LoRa (sensor → gateway)                            │  │
│  │  • LTE-M (tracker → cloud)                            │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Sync Window Optimization

```rust
/// HIVE Lite sync configuration
pub struct LiteSyncConfig {
    /// Minimum interval between sync attempts (power saving)
    pub min_sync_interval: Duration,
    
    /// Maximum interval before forced sync (data freshness)
    pub max_sync_interval: Duration,
    
    /// Number of accumulated changes to trigger early sync
    pub change_threshold: usize,
    
    /// Priority level that triggers immediate sync
    pub immediate_sync_priority: QosPriority,
    
    /// Enable battery-aware throttling
    pub battery_aware: bool,
    
    /// Battery percentage below which to reduce sync frequency
    pub low_battery_threshold: u8,
    
    /// Sync interval multiplier when battery is low
    pub low_battery_multiplier: f32,
}

impl Default for LiteSyncConfig {
    fn default() -> Self {
        Self {
            min_sync_interval: Duration::from_secs(30),
            max_sync_interval: Duration::from_secs(300),
            change_threshold: 10,
            immediate_sync_priority: QosPriority::Critical,
            battery_aware: true,
            low_battery_threshold: 20,
            low_battery_multiplier: 3.0,
        }
    }
}

/// Wearable-optimized preset
pub fn wearable_config() -> LiteSyncConfig {
    LiteSyncConfig {
        min_sync_interval: Duration::from_secs(60),
        max_sync_interval: Duration::from_secs(300),
        change_threshold: 5,
        immediate_sync_priority: QosPriority::Critical,
        battery_aware: true,
        low_battery_threshold: 30,
        low_battery_multiplier: 4.0,
    }
}

/// Sensor node preset (extreme power saving)
pub fn sensor_config() -> LiteSyncConfig {
    LiteSyncConfig {
        min_sync_interval: Duration::from_secs(300),
        max_sync_interval: Duration::from_secs(3600),
        change_threshold: 20,
        immediate_sync_priority: QosPriority::Emergency,
        battery_aware: true,
        low_battery_threshold: 40,
        low_battery_multiplier: 6.0,
    }
}
```

### Batched Update Accumulation

```rust
/// Accumulates updates for batched transmission
pub struct BatchAccumulator {
    /// Pending document updates (latest value per doc)
    pending_updates: HashMap<DocumentId, (Timestamp, Vec<u8>)>,
    
    /// High-priority updates (immediate sync candidates)
    priority_queue: BinaryHeap<PrioritizedUpdate>,
    
    /// Accumulated byte size
    pending_bytes: usize,
    
    /// Last sync timestamp
    last_sync: Instant,
    
    /// Configuration
    config: LiteSyncConfig,
}

impl BatchAccumulator {
    /// Add an update, potentially triggering sync
    pub fn accumulate(&mut self, update: DocumentUpdate) -> SyncDecision {
        // Deduplicate: Keep only latest update per document
        let doc_id = update.document_id.clone();
        let is_new = !self.pending_updates.contains_key(&doc_id);
        
        // Track for priority-based decisions
        if update.priority >= self.config.immediate_sync_priority {
            return SyncDecision::Immediate;
        }
        
        // Update or insert
        self.pending_updates.insert(
            doc_id,
            (update.timestamp, update.payload)
        );
        
        if is_new {
            self.pending_bytes += update.payload.len();
        }
        
        // Check thresholds
        if self.pending_updates.len() >= self.config.change_threshold {
            return SyncDecision::ThresholdReached;
        }
        
        if self.last_sync.elapsed() >= self.config.max_sync_interval {
            return SyncDecision::IntervalExpired;
        }
        
        SyncDecision::Accumulate
    }
    
    /// Drain accumulated updates for sync
    pub fn drain_for_sync(&mut self) -> BatchedPayload {
        let updates: Vec<_> = self.pending_updates.drain().collect();
        self.pending_bytes = 0;
        self.last_sync = Instant::now();
        
        BatchedPayload {
            updates,
            compressed: self.compress(&updates),
        }
    }
}

pub enum SyncDecision {
    /// Continue accumulating
    Accumulate,
    /// Sync immediately (critical priority)
    Immediate,
    /// Sync because change threshold reached
    ThresholdReached,
    /// Sync because max interval expired
    IntervalExpired,
}
```

### Power Management Integration

```rust
/// Power-aware sync scheduler
pub struct PowerAwareSyncScheduler {
    accumulator: BatchAccumulator,
    transport: Box<dyn LiteTransport>,
    battery_monitor: BatteryMonitor,
    radio_controller: RadioController,
}

impl PowerAwareSyncScheduler {
    pub async fn run(&mut self) {
        loop {
            // Sleep until next potential sync window
            let sleep_duration = self.calculate_sleep_duration();
            self.radio_controller.enter_sleep_mode();
            
            tokio::time::sleep(sleep_duration).await;
            
            // Check if sync is needed
            if self.should_sync() {
                self.perform_sync().await;
            }
        }
    }
    
    fn calculate_sleep_duration(&self) -> Duration {
        let base_interval = self.accumulator.config.min_sync_interval;
        
        // Extend sleep when battery is low
        if self.battery_monitor.percentage() < self.accumulator.config.low_battery_threshold {
            Duration::from_secs_f32(
                base_interval.as_secs_f32() * 
                self.accumulator.config.low_battery_multiplier
            )
        } else {
            base_interval
        }
    }
    
    async fn perform_sync(&mut self) {
        // Wake radio
        self.radio_controller.wake();
        
        // Wait for connection (with timeout)
        let connected = tokio::time::timeout(
            Duration::from_secs(10),
            self.transport.ensure_connected()
        ).await;
        
        if let Ok(Ok(())) = connected {
            // Drain and send batch
            let batch = self.accumulator.drain_for_sync();
            let _ = self.transport.send_batch(batch).await;
            
            // Receive any pending parent updates
            let _ = self.transport.receive_updates().await;
        }
        
        // Return to sleep
        self.radio_controller.enter_sleep_mode();
    }
}
```

### WearTAK Integration Architecture

**Note**: WearTAK integration depends on ADR-026 (Protocol-Level Format Transformation Primitives) for CoT ↔ HIVE transformation. See ADR-026 for the FormatAdapter framework, CoT-native mode, and parent-side bridging patterns.

```
┌─────────────────────────────────────────────────────────────────┐
│                 WearTAK + HIVE Lite Stack                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Samsung Galaxy Watch                    ATAK Phone (Parent)    │
│  ┌─────────────────────┐                ┌─────────────────────┐ │
│  │     WearTAK         │                │       ATAK          │ │
│  │  ┌──────────────┐   │                │  ┌──────────────┐   │ │
│  │  │   UI/UX      │   │                │  │   UI/UX      │   │ │
│  │  └──────────────┘   │                │  └──────────────┘   │ │
│  │         │           │                │         │           │ │
│  │  ┌──────────────┐   │    BLE Link    │  ┌──────────────┐   │ │
│  │  │ HIVE Lite    │───┼────────────────┼──│ HIVE Edge    │   │ │
│  │  │ (Leaf Node)  │   │   Batched      │  │ (Aggregator) │   │ │
│  │  │              │   │   Updates      │  │              │   │ │
│  │  └──────────────┘   │                │  └──────────────┘   │ │
│  │         │           │                │         │           │ │
│  │  ┌──────────────┐   │                │  ┌──────────────┐   │ │
│  │  │ CoT Minimal  │   │                │  │ CoT Full     │   │ │
│  │  │ (position,   │   │                │  │ (all data)   │   │ │
│  │  │  alerts)     │   │                │  └──────────────┘   │ │
│  │  └──────────────┘   │                │         │           │ │
│  └─────────────────────┘                │  ┌──────────────┐   │ │
│                                         │  │ TAK Server   │───┼─│
│                                         │  │ Connection   │   │ │
│                                         │  └──────────────┘   │ │
│                                         └─────────────────────┘ │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

Data Flow:
1. Watch generates position/health updates
2. HIVE Lite batches updates (60-second window)
3. BLE sync to phone on schedule
4. Phone aggregates into HIVE mesh
5. CoT events flow to TAK Server as normal
```

### Minimal State Schema for Wearables

```protobuf
// Minimal schema for HIVE Lite wearable nodes
message LiteNodeState {
  // Identity
  string node_id = 1;
  string parent_id = 2;
  
  // Position (most recent only)
  Position position = 3;
  
  // Health/Status
  NodeHealth health = 4;
  
  // Alerts (priority queue, limited depth)
  repeated Alert pending_alerts = 5;
  
  // Sync metadata
  SyncState sync = 6;
}

message Position {
  double latitude = 1;
  double longitude = 2;
  float altitude = 3;
  float accuracy = 4;
  fixed64 timestamp = 5;
}

message NodeHealth {
  uint32 battery_percent = 1;
  float heart_rate = 2;        // Wearable-specific
  uint32 step_count = 3;       // Activity tracking
  bool sos_active = 4;         // Emergency state
  fixed64 timestamp = 5;
}

message Alert {
  string alert_id = 1;
  AlertType type = 2;
  string message = 3;
  Priority priority = 4;
  fixed64 timestamp = 5;
}

message SyncState {
  // Vector clock for parent sync
  map<string, uint64> vector_clock = 1;
  
  // Last successful sync
  fixed64 last_sync_timestamp = 2;
  
  // Pending changes count
  uint32 pending_changes = 3;
}
```

### IoT Sensor Node Profile

```rust
/// Ultra-low-power sensor configuration
pub struct SensorNodeProfile {
    /// Supported data types (position, temperature, humidity, etc.)
    pub data_types: Vec<SensorDataType>,
    
    /// Measurement interval
    pub sample_interval: Duration,
    
    /// Samples to accumulate before sync
    pub samples_per_batch: usize,
    
    /// Transport options
    pub transport: SensorTransport,
    
    /// Power source characteristics
    pub power_source: PowerSource,
}

pub enum SensorTransport {
    /// LoRaWAN (915MHz, 50Kbps, 2-15km range)
    LoRaWAN { 
        spreading_factor: u8,  // 7-12
        bandwidth: u32,        // 125/250/500 kHz
    },
    /// BLE 5.0 (periodic advertising)
    BLE {
        advertising_interval: Duration,
        connection_interval: Duration,
    },
    /// LTE-M (low-power cellular)
    LteM {
        psm_tau: Duration,     // Power save mode timer
        edrx_cycle: Duration,  // eDRX cycle
    },
}

pub enum PowerSource {
    Battery { capacity_mah: u32, voltage: f32 },
    Solar { panel_watts: f32, battery_mah: u32 },
    Wired,
}

impl SensorNodeProfile {
    /// Calculate expected battery life
    pub fn estimate_battery_days(&self) -> f32 {
        match &self.power_source {
            PowerSource::Battery { capacity_mah, .. } => {
                let syncs_per_day = 86400.0 / 
                    (self.sample_interval.as_secs() * self.samples_per_batch as u64) as f32;
                let ma_per_sync = self.transport.sync_current_ma();
                let sync_duration_hours = 0.01; // ~36 seconds per sync
                let ma_hours_per_day = syncs_per_day * ma_per_sync * sync_duration_hours;
                
                *capacity_mah as f32 / ma_hours_per_day
            }
            PowerSource::Solar { .. } => f32::INFINITY, // Effectively unlimited
            PowerSource::Wired => f32::INFINITY,
        }
    }
}
```

### Benchmarking Framework

```rust
/// Power consumption tracking for optimization
pub struct PowerBenchmark {
    /// Energy consumed per sync (mWh)
    pub energy_per_sync: f32,
    
    /// Radio on-time per sync (ms)
    pub radio_time_per_sync: u32,
    
    /// CPU time per sync (ms)
    pub cpu_time_per_sync: u32,
    
    /// Bytes transferred per sync
    pub bytes_per_sync: usize,
    
    /// Sync success rate
    pub success_rate: f32,
}

impl PowerBenchmark {
    /// Compare against Ditto baseline
    pub fn improvement_over_ditto(&self, ditto_baseline: &PowerBenchmark) -> PowerImprovement {
        PowerImprovement {
            energy_reduction: 1.0 - (self.energy_per_sync / ditto_baseline.energy_per_sync),
            radio_time_reduction: 1.0 - (self.radio_time_per_sync as f32 / 
                                         ditto_baseline.radio_time_per_sync as f32),
            bandwidth_reduction: 1.0 - (self.bytes_per_sync as f32 / 
                                        ditto_baseline.bytes_per_sync as f32),
        }
    }
}

pub struct PowerImprovement {
    pub energy_reduction: f32,      // Target: >50%
    pub radio_time_reduction: f32,  // Target: >80%
    pub bandwidth_reduction: f32,   // Target: >90% (differential sync)
}
```

## Alternatives Considered

### Alternative 1: Separate HIVE Lite Codebase

Create a distinct minimal implementation for constrained devices.

**Pros**: Maximum optimization, no overhead from unused features
**Cons**: Code duplication, divergent evolution, maintenance burden, version drift

**Decision**: Rejected. Use feature flags and compile-time configuration instead.

### Alternative 2: Proxy-Only Mode

Wearables only relay data through BLE, no local CRDT state.

**Pros**: Minimal device complexity, near-zero state overhead
**Cons**: No offline operation, loses CRDT benefits, single point of failure

**Decision**: Partially adopted. Available as `StatelessRelay` mode for extremely constrained devices, but HIVE Lite retains minimal CRDT state for offline resilience.

### Alternative 3: Custom Lightweight CRDT

Design a custom minimal CRDT specifically for constrained devices.

**Pros**: Optimal for use case, no Automerge overhead
**Cons**: Compatibility risk, interoperability burden, maintenance of two CRDT implementations

**Decision**: Rejected. Use Automerge with restricted document schema instead.

## Implementation Plan

### Phase 1: Core HIVE Lite Profile (Q1 2026)
1. Define device profile configuration system
2. Implement batch accumulator
3. Add sync interval controls
4. Basic power consumption metrics

### Phase 2: WearTAK Integration (Q1 2026)
1. BLE transport for Samsung watches
2. Minimal CoT schema for wearables
3. ATAK bridge/aggregation
4. Field testing with Ascent

### Phase 3: IoT Sensor Support (Q2 2026)
1. LoRaWAN transport adapter
2. Sensor data types and schemas
3. Solar/battery power profiles
4. Gateway aggregation patterns

### Phase 4: Optimization & Benchmarking (Q2 2026)
1. Power consumption benchmarking framework
2. A/B testing against Ditto baseline
3. Profile auto-tuning based on battery state
4. Documentation and deployment guides

## Success Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Battery life (wearable) | 18-24 hours | Samsung Watch field test |
| Battery improvement vs Ditto | >50% reduction | Side-by-side benchmark |
| Radio duty cycle | <5% | Power analyzer |
| Sync latency (normal) | <5 seconds | Timestamp delta |
| Sync latency (critical) | <1 second | Timestamp delta |
| Bandwidth per sync | <1KB typical | Network capture |
| State memory footprint | <1MB | Process memory |

## References

- ADR-010: Transport Layer UDP/TCP
- ADR-016: TTL and Data Lifecycle Management
- ADR-019: QoS and Data Prioritization
- ADR-024: Flexible Hierarchy Strategies
- Ascent WearTAK feedback (Alex Gorsuch, December 2025)
- Samsung Galaxy Watch Power Profiling: https://developer.samsung.com/galaxy-watch
- BLE 5.0 Low Energy Features: Bluetooth SIG specifications
- LoRaWAN Regional Parameters: LoRa Alliance

## Appendix A: Power Budget Analysis

### Samsung Galaxy Watch 4 Power Profile

| Component | Active Power | Sleep Power | Duty Cycle (Full) | Duty Cycle (Lite) |
|-----------|-------------|-------------|-------------------|-------------------|
| CPU | 300 mW | 5 mW | 15% | 2% |
| WiFi | 80 mW | 0.1 mW | 20% | 0% |
| BLE | 10 mW | 0.01 mW | 10% | 2% |
| Display | 100 mW | 0.5 mW | 20% | 20% |
| Sensors | 15 mW | 1 mW | 50% | 50% |

**Full HIVE**: ~85 mW average → ~4 hours on 350 mAh
**HIVE Lite**: ~35 mW average → ~10 hours on 350 mAh (with display active)
**HIVE Lite (display off)**: ~15 mW average → ~23 hours on 350 mAh

### Sync Energy Comparison

| Sync Type | Energy per Sync | Syncs per Hour | Energy per Hour |
|-----------|----------------|----------------|-----------------|
| Ditto Continuous | 0.5 mWh | 60+ | 30+ mWh |
| HIVE Full | 0.3 mWh | 30 | 9 mWh |
| HIVE Lite (scheduled) | 0.2 mWh | 6 | 1.2 mWh |
| HIVE Lite (low battery) | 0.2 mWh | 2 | 0.4 mWh |
