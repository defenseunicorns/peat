# ADR-019: Quality of Service and Data Prioritization for HIVE Sync

**Status**: Proposed  
**Date**: 2025-11-16  
**Authors**: Kit Plummer, Codex  
**Relates To**: ADR-005 (Data Sync Abstraction), ADR-009 (Bidirectional Hierarchical Flows), ADR-010 (Transport Layer), ADR-011 (Ditto vs Automerge/Iroh), ADR-016 (TTL and Data Lifecycle)

## Context

### The Problem: Not All Data Is Created Equal

Customer feedback has revealed a critical requirement: **HIVE must support delivering model products (contact reports, images, audio/video clips, etc.) through its hierarchical network with appropriate prioritization.**

The operational reality:

```
Mission Scenario - Reconnection After Network Partition:
├─ Platform has been offline for 45 minutes
├─ Accumulated data to sync:
│  ├─ 1x Contact Report (enemy observation) - 2KB - CRITICAL
│  ├─ 3x Images (target verification) - 15MB - HIGH PRIORITY
│  ├─ 2x Audio clips (comms intercept) - 8MB - HIGH PRIORITY  
│  ├─ 150x Position updates (track history) - 300KB - LOW PRIORITY
│  ├─ 45x Health status updates - 90KB - MEDIUM PRIORITY
│  └─ 12x Capability state changes - 24KB - MEDIUM PRIORITY
└─ Bandwidth available: 500 Kbps (tactical radio link)

WITHOUT QoS: 
  Time to sync everything = ~7 minutes
  Contact report arrives at minute 4 (delayed by track history)
  Mission impact: Commander unaware of enemy contact for 4 minutes

WITH QoS:
  Contact report syncs in 4 seconds (Priority 1)
  Images/audio sync in next 90 seconds (Priority 2) 
  Health/capability updates sync in next 30 seconds (Priority 3)
  Track history syncs in background (Priority 4)
  Mission impact: Commander aware in 4 seconds, can act immediately
```

### The Fundamental Issue

Current HIVE architecture (as inherited from Ditto/Automerge) treats all CRDT updates with approximately equal priority. This creates several operational problems:

1. **Priority Inversion**: Critical contact reports wait behind routine telemetry
2. **Bandwidth Starvation**: Large media files (images/video) block small critical updates
3. **Context Blindness**: System doesn't understand mission phase importance (ingress vs egress)
4. **Recovery Inefficiency**: After network partition, syncs everything instead of critical-first
5. **Storage Overflow Risk**: No mechanism to preferentially retain high-value data

### What We Need

A comprehensive **Quality of Service (QoS) framework** that provides:

1. **Data Type Prioritization**: Different data types have different inherent priorities
2. **Context-Aware Profiles**: Priorities adjust based on mission context (ingress, execution, egress, emergency)
3. **Bandwidth Management**: Allocate limited bandwidth to highest-priority data first
4. **Storage Management**: Preferentially retain high-priority data when storage fills
5. **Sync Ordering**: When connectivity restores, sync in priority order
6. **Obsolescence Handling**: Automatically discard stale low-priority data
7. **Preemption Support**: Critical data can interrupt lower-priority transfers

## Decision

### Core QoS Framework Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    HIVE QoS Framework                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │     Priority Classification Layer                   │   │
│  │  • Assigns base priority to each data type          │   │
│  │  • Maps data to QoS class (P1-P5)                   │   │
│  │  • Considers message size, freshness, source        │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │     Context Profile Layer                           │   │
│  │  • Applies mission context to adjust priorities     │   │
│  │  • Profiles: Ingress, Execution, Egress, Emergency  │   │
│  │  • Dynamic reprioritization based on conditions     │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │     Bandwidth Allocation Layer                      │   │
│  │  • Allocates bandwidth quota per priority class     │   │
│  │  • Implements preemption for critical data          │   │
│  │  • Adaptive rate limiting per class                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │     Sync Orchestration Layer                        │   │
│  │  • Priority-ordered sync queues                     │   │
│  │  • Obsolescence filtering                           │   │
│  │  • Chunked transfer management                      │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ↓                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │     Storage Management Layer                        │   │
│  │  • Priority-based eviction policies                 │   │
│  │  • Critical data retention guarantees               │   │
│  │  • Compression for lower-priority bulk data         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Priority Classification System

Define **5 priority classes** (P1-P5) with decreasing urgency:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QoSClass {
    Critical = 1,      // P1: Mission-critical, immediate sync required
    High = 2,          // P2: Important, sync within seconds
    Normal = 3,        // P3: Standard operational data, sync within minutes
    Low = 4,           // P4: Routine telemetry, sync when bandwidth available
    Bulk = 5,          // P5: Archival/historical, opportunistic sync
}

#[derive(Debug, Clone)]
pub struct QoSPolicy {
    pub base_class: QoSClass,
    pub max_latency_ms: Option<u64>,
    pub max_size_bytes: Option<usize>,
    pub ttl_seconds: Option<u64>,
    pub retention_priority: u8,
    pub preemptable: bool,
}
```

### Data Type to QoS Class Mapping

**Priority 1 - Critical (Sub-second sync required)**
- Contact reports (enemy sightings, threats)
- Emergency alerts (platform failure, casualty, weapon discharge)
- Abort commands (ROE violations, fratricide risk)
- Critical capability loss (weapon system failure, sensor failure)

**Priority 2 - High (Seconds to sync)**
- Target verification images/video (< 5MB)
- Communications intercepts (audio clips)
- Significant capability changes (fuel bingo, EW degradation)
- Commander's intent updates
- Mission re-tasking orders
- Deconfliction boundaries (no-strike zone changes)

**Priority 3 - Normal (Minutes to sync)**
- Health/status updates
- Routine capability state changes
- Formation commands
- Non-critical images/video (situational awareness)
- Mission progress reports

**Priority 4 - Low (Sync when bandwidth available)**
- Position/velocity/orientation telemetry (track history)
- Routine heartbeats
- Diagnostic telemetry
- Environmental sensor data

**Priority 5 - Bulk (Opportunistic/background sync)**
- Large AI model updates (> 50MB)
- Software packages
- Historical data archives
- Debug logs
- Full mission recordings

### Context-Based Priority Profiles

Priority classes can be **dynamically adjusted** based on mission context:

```rust
pub enum MissionContext {
    Ingress,      // Moving to objective
    Execution,    // On objective, executing mission
    Egress,       // Returning from objective
    Emergency,    // Emergency situation (all critical data prioritized)
    Standby,      // Waiting/holding (lower urgency)
}

impl MissionContext {
    pub fn apply_profile(&self, base_policy: &QoSPolicy) -> QoSPolicy {
        match self {
            MissionContext::Ingress => {
                // During ingress: prioritize enemy detection, reduce telemetry
                // Contact reports: P1 → P1 (unchanged)
                // Track history: P4 → P5 (deprioritized)
            },
            MissionContext::Execution => {
                // During execution: prioritize all intel products
                // Images/video: P2 → P1 (elevated)
                // Audio intercepts: P2 → P1 (elevated)
            },
            MissionContext::Egress => {
                // During egress: prioritize health/capability status
                // Health updates: P3 → P2 (elevated)
                // Track history: P4 → P4 (unchanged)
            },
            MissionContext::Emergency => {
                // Emergency: everything critical except bulk data
                // Contact reports: P1 → P1
                // Health updates: P3 → P1 (elevated significantly)
                // Model updates: P5 → P5 (unchanged, still bulk)
            },
            MissionContext::Standby => {
                // Standby: normal priorities, use for background sync
                // All priorities unchanged, opportunistic bulk sync
            },
        }
    }
}
```

**Example Application:**

```rust
// Scenario: Platform detects enemy contact during ingress
let contact_report = ContactReport {
    enemy_type: "armored_vehicle",
    position: GeoPoint { lat: 34.5, lon: 69.2 },
    confidence: 0.92,
    timestamp: Utc::now(),
};

let context = MissionContext::Ingress;

let policy = QoSPolicy::for_data_type(DataType::ContactReport)
    .apply_context(context);

// Result: 
// - Base priority: P1 (Critical)
// - Context adjustment: P1 → P1 (no change, already highest)
// - Max latency: 500ms
// - Preemption enabled: true
// - Retention: Forever (never evict)
```

### Bandwidth Allocation and Preemption

Allocate bandwidth budget per QoS class:

```rust
pub struct BandwidthAllocation {
    pub total_bandwidth_bps: u64,
    pub allocations: HashMap<QoSClass, BandwidthQuota>,
}

pub struct BandwidthQuota {
    pub min_guaranteed_bps: u64,     // Minimum bandwidth reserved
    pub max_burst_bps: u64,           // Maximum burst rate
    pub preemption_enabled: bool,     // Can preempt lower priorities
    pub current_usage_bps: u64,       // Current utilization
}

impl BandwidthAllocation {
    pub fn default_tactical() -> Self {
        // For 1 Mbps tactical link
        BandwidthAllocation {
            total_bandwidth_bps: 1_000_000,
            allocations: hashmap! {
                QoSClass::Critical => BandwidthQuota {
                    min_guaranteed_bps: 200_000,  // 20% guaranteed
                    max_burst_bps: 800_000,       // Can use 80% if available
                    preemption_enabled: true,
                    current_usage_bps: 0,
                },
                QoSClass::High => BandwidthQuota {
                    min_guaranteed_bps: 300_000,  // 30% guaranteed
                    max_burst_bps: 600_000,
                    preemption_enabled: true,     // Can preempt Normal/Low/Bulk
                    current_usage_bps: 0,
                },
                QoSClass::Normal => BandwidthQuota {
                    min_guaranteed_bps: 200_000,  // 20% guaranteed
                    max_burst_bps: 400_000,
                    preemption_enabled: false,
                    current_usage_bps: 0,
                },
                QoSClass::Low => BandwidthQuota {
                    min_guaranteed_bps: 150_000,  // 15% guaranteed
                    max_burst_bps: 300_000,
                    preemption_enabled: false,
                    current_usage_bps: 0,
                },
                QoSClass::Bulk => BandwidthQuota {
                    min_guaranteed_bps: 50_000,   // 5% guaranteed
                    max_burst_bps: 200_000,
                    preemption_enabled: false,
                    current_usage_bps: 0,
                },
            },
        }
    }
}
```

**Preemption Behavior:**

```rust
impl SyncOrchestrator {
    pub fn handle_critical_data(&mut self, critical_msg: Message) -> Result<()> {
        // Critical (P1) data arrives while bulk (P5) transfer in progress
        
        if self.bandwidth.is_congested() {
            // Pause lower-priority transfers
            self.pause_transfers_below(QoSClass::High)?;
            
            // Transmit critical message immediately
            self.transmit_with_priority(critical_msg, QoSClass::Critical)?;
            
            // Resume lower-priority transfers after critical message sent
            self.resume_paused_transfers()?;
        } else {
            // Sufficient bandwidth, no preemption needed
            self.transmit_with_priority(critical_msg, QoSClass::Critical)?;
        }
        
        Ok(())
    }
}
```

### Sync Ordering and Recovery

When connectivity restores after partition, sync in **priority order**:

```rust
pub struct SyncRecovery {
    pub queued_updates: BTreeMap<QoSClass, Vec<UpdateBatch>>,
    pub obsolescence_window_seconds: u64,
}

impl SyncRecovery {
    pub async fn recover_from_partition(&mut self) -> Result<()> {
        // Step 1: Filter obsolete data
        self.apply_obsolescence_filter()?;
        
        // Step 2: Sync in priority order (P1 first, P5 last)
        for class in [
            QoSClass::Critical,
            QoSClass::High,
            QoSClass::Normal,
            QoSClass::Low,
            QoSClass::Bulk,
        ] {
            if let Some(batches) = self.queued_updates.get(&class) {
                for batch in batches {
                    self.sync_batch(batch, class).await?;
                    
                    // Check if we should pause (e.g., bandwidth exhausted)
                    if self.should_pause_recovery()? {
                        info!("Recovery paused, will resume later");
                        return Ok(());
                    }
                }
            }
        }
        
        info!("Sync recovery complete");
        Ok(())
    }
    
    fn apply_obsolescence_filter(&mut self) -> Result<()> {
        let now = Utc::now();
        
        for (_class, batches) in self.queued_updates.iter_mut() {
            batches.retain(|batch| {
                let age_seconds = (now - batch.timestamp).num_seconds() as u64;
                
                match batch.data_type {
                    DataType::PositionUpdate => {
                        // Position updates older than 5 minutes are stale
                        age_seconds < 300
                    },
                    DataType::ContactReport => {
                        // Contact reports never obsolete (always sync)
                        true
                    },
                    DataType::HealthStatus => {
                        // Health updates older than 10 minutes are stale
                        age_seconds < 600
                    },
                    DataType::Image | DataType::Video => {
                        // Media older than 1 hour may still be valuable
                        age_seconds < 3600
                    },
                    _ => true, // Default: don't discard
                }
            });
        }
        
        Ok(())
    }
}
```

### Storage Management with QoS

When storage fills, **evict lower-priority data first**:

```rust
pub struct QoSAwareStorage {
    pub max_storage_bytes: usize,
    pub current_storage_bytes: usize,
    pub retention_policies: HashMap<QoSClass, RetentionPolicy>,
}

pub struct RetentionPolicy {
    pub min_retain_seconds: u64,      // Minimum time to keep data
    pub max_retain_seconds: u64,      // Maximum time to keep data
    pub eviction_priority: u8,        // Lower = evict first (P5 before P1)
    pub compression_eligible: bool,   // Can compress to save space
}

impl QoSAwareStorage {
    pub fn evict_to_free_space(&mut self, required_bytes: usize) -> Result<()> {
        let mut freed_bytes = 0;
        
        // Evict in reverse priority order (P5 first, P1 last)
        for class in [
            QoSClass::Bulk,
            QoSClass::Low,
            QoSClass::Normal,
            QoSClass::High,
            // Never evict Critical (P1) data
        ] {
            if freed_bytes >= required_bytes {
                break;
            }
            
            // Within each class, evict oldest data first
            freed_bytes += self.evict_oldest_in_class(class)?;
        }
        
        if freed_bytes < required_bytes {
            // Still not enough space - compress lower-priority data
            self.compress_bulk_data()?;
            
            if self.current_storage_bytes + required_bytes > self.max_storage_bytes {
                return Err(Error::InsufficientStorage {
                    required: required_bytes,
                    available: self.max_storage_bytes - self.current_storage_bytes,
                });
            }
        }
        
        Ok(())
    }
}
```

**Retention Guarantees:**

```rust
impl RetentionPolicy {
    pub fn for_qos_class(class: QoSClass) -> Self {
        match class {
            QoSClass::Critical => RetentionPolicy {
                min_retain_seconds: 86400 * 7,  // 7 days minimum
                max_retain_seconds: u64::MAX,   // Never auto-delete
                eviction_priority: 5,           // Evict last (never in practice)
                compression_eligible: false,    // Never compress
            },
            QoSClass::High => RetentionPolicy {
                min_retain_seconds: 86400,      // 24 hours minimum
                max_retain_seconds: 86400 * 7,  // 7 days max
                eviction_priority: 4,
                compression_eligible: true,
            },
            QoSClass::Normal => RetentionPolicy {
                min_retain_seconds: 3600,       // 1 hour minimum
                max_retain_seconds: 86400,      // 24 hours max
                eviction_priority: 3,
                compression_eligible: true,
            },
            QoSClass::Low => RetentionPolicy {
                min_retain_seconds: 300,        // 5 minutes minimum
                max_retain_seconds: 3600,       // 1 hour max
                eviction_priority: 2,
                compression_eligible: true,
            },
            QoSClass::Bulk => RetentionPolicy {
                min_retain_seconds: 60,         // 1 minute minimum
                max_retain_seconds: 300,        // 5 minutes max
                eviction_priority: 1,           // Evict first
                compression_eligible: true,
            },
        }
    }
}
```

## Implementation Strategy

### Phase 1: Core QoS Infrastructure (Weeks 1-2)

**Deliverables:**
1. QoS policy definitions (`QoSClass`, `QoSPolicy`)
2. Data type to QoS class mapping registry
3. Priority-ordered sync queues
4. Basic bandwidth allocation (no preemption yet)

**Integration Points:**
- Modify `cap-transport` to accept QoS metadata
- Update Automerge/Iroh sync to support priority queues
- Add QoS metadata to CRDT documents

**Testing:**
- Unit tests for priority classification
- Integration tests for priority-ordered sync
- Performance benchmarks (overhead measurement)

### Phase 2: Context Profiles and Preemption (Weeks 3-4)

**Deliverables:**
1. Mission context profile system
2. Dynamic priority adjustment
3. Preemption logic for critical data
4. Bandwidth monitoring and throttling

**Integration Points:**
- Mission planner interface to set context
- Real-time bandwidth measurement
- Interrupt-based preemption for critical messages

**Testing:**
- Test priority adjustments per context
- Test preemption of bulk transfers by critical data
- Validate bandwidth allocation enforcement

### Phase 3: Recovery and Storage Management (Weeks 5-6)

**Deliverables:**
1. Obsolescence filtering logic
2. Priority-based sync recovery
3. QoS-aware storage eviction
4. Compression for bulk data

**Integration Points:**
- Modify beacon storage to track QoS class
- Implement priority-based eviction in `cap-persistence`
- Add obsolescence checks to sync engine

**Testing:**
- Test sync recovery after 1-hour partition
- Test storage eviction under pressure
- Validate critical data retention guarantees

### Phase 4: Validation and Optimization (Weeks 7-8)

**Deliverables:**
1. Large-scale validation with Shadow simulator
2. Performance optimization
3. Operational dashboards for QoS monitoring
4. Documentation and runbooks

**Validation Scenarios:**
- 100-node network, 1 Mbps links, 45-minute partition
- Measure: Time to sync critical data after reconnection
- Goal: < 10 seconds for P1, < 60 seconds for P2

**Metrics:**
- Priority inversion incidents (should be 0)
- Bandwidth utilization per QoS class
- Storage eviction events by class
- Sync latency distribution per class

## Consequences

### Positive

1. **Operational Effectiveness**: Critical data arrives in time to affect decisions
2. **Bandwidth Efficiency**: Limited bandwidth allocated to highest-value data
3. **Graceful Degradation**: System maintains critical functions under bandwidth constraints
4. **Storage Optimization**: High-value data retained, low-value data evicted
5. **Context Awareness**: Priorities adapt to mission phase automatically
6. **Predictable Behavior**: Operators understand what data syncs first
7. **Measurable SLAs**: Can define service level objectives per data type

### Negative

1. **Complexity**: QoS framework adds ~3000 lines of code, increased testing burden
2. **Configuration Overhead**: Operators must define/tune priority mappings
3. **Potential for Misconfiguration**: Wrong priorities could delay important data
4. **Resource Overhead**: Priority queue management, bandwidth monitoring costs ~5-10% CPU
5. **Testing Difficulty**: Must test all priority combinations and contexts
6. **Documentation Burden**: Must explain QoS policies to operators

### Risks and Mitigation

**Risk 1: Priority Misclassification**
- **Impact**: HIGH - Wrong priority delays critical data
- **Likelihood**: MEDIUM - Requires domain expertise to classify correctly
- **Mitigation**: 
  - Provide sensible defaults based on military doctrine
  - Allow operator overrides per mission
  - Monitor priority inversion incidents, alert if detected
  - Extensive testing with subject matter experts

**Risk 2: Starvation of Low-Priority Data**
- **Impact**: MEDIUM - Routine telemetry never syncs if bandwidth always limited
- **Likelihood**: MEDIUM - Possible in sustained low-bandwidth environments
- **Mitigation**:
  - Guarantee minimum bandwidth per class (even P5 gets 5%)
  - Implement aging: P4 data becomes P3 after 1 hour unsynced
  - Monitor queue depths, alert if P4/P5 queues growing unbounded

**Risk 3: Bandwidth Allocation Inaccuracy**
- **Impact**: MEDIUM - Preemption misbehavior, unfair allocation
- **Likelihood**: LOW - Standard traffic shaping techniques well-understood
- **Mitigation**:
  - Use proven token bucket algorithm for rate limiting
  - Real-time bandwidth measurement, not estimates
  - Adaptive allocation based on actual usage patterns

**Risk 4: Storage Eviction Deletes Needed Data**
- **Impact**: HIGH - Mission-critical data lost
- **Likelihood**: LOW - If retention policies configured correctly
- **Mitigation**:
  - Conservative retention for P1/P2 (never evict P1)
  - Storage monitoring alerts before 90% full
  - Operator override to mark specific data as "never evict"
  - Audit log of all evictions for post-mission review

**Risk 5: Obsolescence Filter Too Aggressive**
- **Impact**: MEDIUM - Discards data that was still valuable
- **Likelihood**: MEDIUM - Hard to predict what's "stale"
- **Mitigation**:
  - Conservative obsolescence windows (e.g., 5min for position, not 30sec)
  - Operator-configurable obsolescence policies per data type
  - Log all obsolescence decisions for tuning
  - Allow "archive everything" mode for post-mission analysis

## Integration with Existing ADRs

### ADR-005 (Data Sync Abstraction Layer)

The QoS framework is **backend-agnostic**:
- Ditto: Map QoS classes to Collection-based prioritization
- Automerge: Extend sync protocol with priority metadata
- Iroh: Use QUIC stream priorities (native support)

```rust
trait SyncBackend {
    fn sync_with_priority(&self, data: &[u8], policy: QoSPolicy) -> Result<()>;
}

// Automerge + Iroh implementation
impl SyncBackend for AutomergeIrohSync {
    fn sync_with_priority(&self, data: &[u8], policy: QoSPolicy) -> Result<()> {
        // Map QoSClass to QUIC stream priority
        let stream_priority = match policy.base_class {
            QoSClass::Critical => StreamPriority::Highest,
            QoSClass::High => StreamPriority::High,
            QoSClass::Normal => StreamPriority::Medium,
            QoSClass::Low => StreamPriority::Low,
            QoSClass::Bulk => StreamPriority::Background,
        };
        
        // Iroh's QUIC implementation handles rest
        self.iroh_endpoint.send_with_priority(data, stream_priority)
    }
}
```

### ADR-009 (Bidirectional Hierarchical Flows)

QoS applies to **both upward and downward flows**:

```rust
pub enum FlowDirection {
    Upward,   // Platform → Squad → Platoon → Company
    Downward, // Company → Platoon → Squad → Platform
}

impl QoSPolicy {
    pub fn for_direction(&self, direction: FlowDirection) -> QoSPolicy {
        match direction {
            FlowDirection::Upward => {
                // Upward: prioritize contact reports, intel products
                self.clone()
            },
            FlowDirection::Downward => {
                // Downward: prioritize commands, abort orders
                // Commands always P1, models/software P5
                if self.data_type.is_command() {
                    QoSPolicy { base_class: QoSClass::Critical, ..self.clone() }
                } else {
                    self.clone()
                }
            }
        }
    }
}
```

### ADR-010 (Transport Layer)

QoS maps naturally to **transport protocol selection**:

| QoS Class | Data Size | Transport | Rationale |
|-----------|-----------|-----------|-----------|
| P1 (Critical) | < 10KB | QUIC High-Priority Stream | Low latency, reliable |
| P1 (Critical) | > 10KB | QUIC High-Priority Stream (chunked) | Reliable, preemptible |
| P2 (High) | < 1MB | QUIC Medium-Priority Stream | Balanced latency/throughput |
| P2 (High) | > 1MB | QUIC Medium-Priority Stream (chunked) | Reliable, orderly |
| P3 (Normal) | Any | QUIC Low-Priority Stream | Standard reliability |
| P4 (Low) | < 100KB | UDP with optional retry | Best-effort, low overhead |
| P5 (Bulk) | > 10MB | QUIC Background Stream or Multicast | Bandwidth-efficient |

### ADR-011 (Ditto vs Automerge/Iroh)

**Automerge + Iroh provides superior QoS support**:

1. **Native QUIC Priorities**: Iroh's QUIC implementation supports per-stream priorities (Ditto does not)
2. **Stream Multiplexing**: Multiple streams prevent head-of-line blocking (critical)
3. **Bandwidth Control**: Fine-grained rate limiting per stream
4. **Preemption**: Can interrupt low-priority streams for critical data

**This strengthens the case for Automerge + Iroh over Ditto.**

### ADR-016 (TTL and Data Lifecycle)

QoS **extends TTL** with priority-based retention:

```rust
pub struct LifecyclePolicy {
    pub qos_policy: QoSPolicy,
    pub ttl_policy: TtlPolicy,
}

impl LifecyclePolicy {
    pub fn should_evict(&self, age_seconds: u64, storage_pressure: f32) -> bool {
        // Critical data: never evict regardless of age
        if self.qos_policy.base_class == QoSClass::Critical {
            return false;
        }
        
        // High storage pressure: evict based on priority
        if storage_pressure > 0.9 {
            return self.qos_policy.base_class >= QoSClass::Low;
        }
        
        // Normal pressure: use TTL
        age_seconds > self.ttl_policy.soft_delete_after_seconds
    }
}
```

## Operational Considerations

### Monitoring and Observability

Operators need **real-time visibility** into QoS behavior:

```rust
pub struct QoSMetrics {
    // Queue depths per priority
    pub queue_depth_p1: usize,
    pub queue_depth_p2: usize,
    pub queue_depth_p3: usize,
    pub queue_depth_p4: usize,
    pub queue_depth_p5: usize,
    
    // Bandwidth utilization per priority
    pub bandwidth_used_p1_bps: u64,
    pub bandwidth_used_p2_bps: u64,
    pub bandwidth_used_p3_bps: u64,
    pub bandwidth_used_p4_bps: u64,
    pub bandwidth_used_p5_bps: u64,
    
    // Latency distribution per priority
    pub latency_p1_ms: LatencyHistogram,
    pub latency_p2_ms: LatencyHistogram,
    pub latency_p3_ms: LatencyHistogram,
    pub latency_p4_ms: LatencyHistogram,
    pub latency_p5_ms: LatencyHistogram,
    
    // Priority violations
    pub priority_inversions: u64,        // P4 delivered before P1 (should be 0)
    pub preemption_events: u64,          // How often critical data preempted bulk
    pub bandwidth_starvation_events: u64, // P1 couldn't get minimum bandwidth
    
    // Storage pressure
    pub evictions_by_class: HashMap<QoSClass, u64>,
    pub storage_utilization: f32,
}
```

**Dashboard Example:**

```
=== HIVE QoS Dashboard ===
Bandwidth: 487 Kbps / 1000 Kbps (48.7% utilized)

Priority Queues:
  P1 (Critical):  3 msgs (12 KB) │████░░░░░░░░░░░░░░░░│ 20% BW  Latency: 450ms avg
  P2 (High):     15 msgs (3.2MB) │████████████░░░░░░░░│ 60% BW  Latency: 2.1s avg  
  P3 (Normal):   47 msgs (890KB) │███░░░░░░░░░░░░░░░░░│ 15% BW  Latency: 8.7s avg
  P4 (Low):     203 msgs (4.5MB) │█░░░░░░░░░░░░░░░░░░░│  5% BW  Latency: 45s avg
  P5 (Bulk):      2 msgs (67MB)  │░░░░░░░░░░░░░░░░░░░░│  0% BW  PAUSED

Storage: 2.3 GB / 8 GB (28.8% used)
  P1 retained: 1247 items (14 days avg age)
  P2 retained: 4521 items (2.3 days avg age)
  Evictions (last hour): 47 (all P5)

Alerts:
  ⚠ P2 queue depth increasing (15 → 23 over 5min)
  ✓ No priority inversions detected
  ✓ All classes meeting latency SLAs
```

### Configuration Management

Provide **sensible defaults** but allow customization:

```yaml
# config/qos_policy.yaml

# Global defaults for tactical operations
global:
  bandwidth_mbps: 1.0
  storage_gb: 8.0
  mission_context: ingress

# Per-data-type overrides
data_types:
  contact_report:
    qos_class: critical
    max_latency_ms: 500
    retention_days: 7
    
  target_image:
    qos_class: high
    max_latency_ms: 5000
    max_size_mb: 5
    retention_hours: 24
    
  position_update:
    qos_class: low
    obsolescence_seconds: 300  # 5 minutes
    retention_minutes: 60

# Context-specific profiles
contexts:
  execution:
    # During mission execution: elevate all intel products
    overrides:
      target_image: critical
      audio_intercept: critical
      
  emergency:
    # During emergency: elevate health/status
    overrides:
      health_status: critical
      capability_state: high
```

### Backwards Compatibility

Support **gradual rollout** of QoS:

```rust
pub struct QoSConfig {
    pub enabled: bool,
    pub enforcement_mode: EnforcementMode,
}

pub enum EnforcementMode {
    Off,           // QoS disabled, FIFO sync
    Monitor,       // QoS metadata collected but not enforced (for tuning)
    Partial,       // Enforce prioritization but not preemption
    Full,          // Full QoS enforcement including preemption
}
```

## Success Criteria

### Technical Metrics

1. **Priority Inversions**: 0 inversions in 1000 hours of operation
2. **Critical Data Latency**: P1 data syncs in < 1 second, 95th percentile
3. **Bandwidth Efficiency**: < 5% overhead for QoS metadata/processing
4. **Storage Utilization**: Never evict P1 data, P2 retained for 24+ hours
5. **Recovery Time**: After 1-hour partition, P1 data syncs within 10 seconds

### Operational Metrics

1. **Mission Impact**: Contact reports arrive in time to affect 95%+ of tactical decisions
2. **Operator Confidence**: Operators trust prioritization, don't request "send everything" mode
3. **Configuration Stability**: Default policies work for 80%+ of missions without tuning
4. **Observability**: Operators can diagnose QoS issues from dashboard within 2 minutes

### Validation Experiments

**Experiment 1: Partition Recovery**
- Scenario: 100 nodes, 1 Mbps links, 45-minute partition
- Queued data: 1 P1 (2KB), 10 P2 (20MB), 100 P3 (5MB), 1000 P4 (50MB)
- Success: P1 syncs in < 5s, P2 in < 60s, P3 in < 300s

**Experiment 2: Sustained Low Bandwidth**
- Scenario: 100 nodes, 300 Kbps links (tactical radio), 2-hour mission
- Traffic: 1 P1/min, 5 P2/min, 20 P3/min, 100 P4/min
- Success: All P1 delivered, 95%+ P2 delivered, P3/P4 best-effort

**Experiment 3: Storage Pressure**
- Scenario: 1 node, 1 GB storage, generate 10 GB of data over 4 hours
- Data mix: 10% P1, 20% P2, 30% P3, 40% P5
- Success: 100% P1 retained, 90%+ P2 retained, P3/P5 evicted as needed

## Related Work and Standards

### Military Standards

**STANAG 4586 (UAV C2)**: Defines message priorities for UAV control
- Our P1-P5 aligns with 4586's "Immediate", "Priority", "Routine", "Deferred"
- HIVE extends to multi-platform coordination beyond single UAV

**Link 16 Message Standard**: Tactical data link with priority levels
- J-Series messages have defined precedence
- HIVE provides similar prioritization for autonomous platforms

### Commercial Standards

**DiffServ (RFC 2474)**: IP packet QoS markings
- We can map HIVE QoS classes to DSCP values for network-layer QoS

**MQTT QoS Levels**: Quality of Service for pub-sub messaging
- MQTT has 3 levels (0, 1, 2) - we extend to 5 for finer control

### Academic Research

- "Priority-Based Data Synchronization in MANETs" (IEEE 2023)
- "Content-Aware Routing for Tactical Networks" (MILCOM 2024)
- "Edge Data Governance for Autonomous Systems" (ACM Edge Computing 2024)

## References

- ADR-005: Data Synchronization Abstraction Layer
- ADR-009: Bidirectional Hierarchical Flows
- ADR-010: Transport Layer (UDP vs TCP)
- ADR-011: Ditto vs Automerge/Iroh (QUIC advantages)
- ADR-016: TTL and Data Lifecycle Abstraction
- CAP_Architecture_EventStreaming_vs_DeltaSync.md (Collection-based prioritization)
- RFC 2474: Definition of the Differentiated Services Field (DiffServ)
- STANAG 4586: Standard Interfaces of UAV Control System (UCS)

## Amendment: Sync Modes and Subscription Granularity

**Date**: 2025-12-09
**Related**: Issue #346 (Automerge-iroh sync not flowing in hierarchical topologies)

### The Missing Dimension: Delta Retention

The original ADR addresses **what** data syncs first (priority ordering) but not **how much history** syncs. This distinction is critical:

```
Current Behavior (Issue #346):
├─ Squad Leader offline for 5 minutes
├─ 7 soldiers sending beacons every second = 2,100 beacon updates
├─ On reconnection: ALL 2,100 deltas must sync
├─ Broadcast channel lags, messages dropped
└─ Documents never reach Platoon Leader

Desired Behavior with Sync Modes:
├─ Squad Leader offline for 5 minutes
├─ Beacons configured as "LatestOnly" sync mode
├─ On reconnection: Only 7 current positions sync (one per soldier)
├─ Sync completes in milliseconds
└─ Platoon Leader receives current state immediately
```

### Sync Mode Classification

```rust
/// Determines how much document history syncs between peers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// Sync all deltas - observers see every historical change
    /// Use for: Audit logs, event streams, mission recordings
    FullHistory,

    /// Sync only current document state, discard intermediate deltas
    /// Use for: Positions, status updates, health telemetry
    LatestOnly,

    /// Sync deltas within a time window, discard older
    /// Use for: Recent track history, last N minutes of updates
    WindowedHistory { window_seconds: u64 },
}

/// Extended QoS policy with sync mode
#[derive(Debug, Clone)]
pub struct QoSPolicy {
    pub priority_class: QoSClass,      // P1-P5 (from original ADR)
    pub sync_mode: SyncMode,           // NEW: How much history to sync
    pub max_latency_ms: Option<u64>,
    pub ttl_seconds: Option<u64>,
    pub retention_priority: u8,
}
```

### Sync Mode Behavior Matrix

| Sync Mode | Automerge Sync | Reconnection Cost | Observer Behavior |
|-----------|----------------|-------------------|-------------------|
| **FullHistory** | Delta-based (`generate_sync_message`) | O(n) where n = missed updates | See every historical change |
| **LatestOnly** | State-based (`doc.save()`) | O(1) constant | See only current state |
| **WindowedHistory** | Delta-based with time filter | O(w) where w = window size | See changes within window |

### Implementation with Automerge

```rust
impl AutomergeSyncCoordinator {
    /// Sync document respecting QoS sync mode
    pub async fn sync_with_mode(
        &self,
        doc_key: &str,
        peer_id: EndpointId,
        policy: &QoSPolicy,
    ) -> Result<()> {
        match policy.sync_mode {
            SyncMode::FullHistory => {
                // Current behavior: delta-based sync
                self.sync_document_with_peer(doc_key, peer_id).await
            }
            SyncMode::LatestOnly => {
                // New: Send full document state, no sync protocol
                let doc = self.store.get(doc_key)?
                    .context("Document not found")?;
                let state_bytes = doc.save();
                self.send_state_snapshot(peer_id, doc_key, state_bytes).await
            }
            SyncMode::WindowedHistory { window_seconds } => {
                // New: Filter sync messages by timestamp
                let cutoff = Instant::now() - Duration::from_secs(window_seconds);
                self.sync_document_with_time_filter(doc_key, peer_id, cutoff).await
            }
        }
    }
}
```

### Subscription Granularity

**Current HIVE subscription model:**
```rust
// Subscribe to entire collection
backend.subscribe("beacons", Query::All).await?;
```

**Ditto's DQL provides spatial/attribute filtering:**
```javascript
// Ditto example - subscribe to nearby beacons only
ditto.store.collection("beacons")
    .find("distance(position, $myPosition) < 5000 AND squad_id == $mySquad")
    .subscribe();
```

**Proposed HIVE extension:**
```rust
/// Enhanced query with spatial and attribute predicates
pub enum Query {
    All,
    ById(DocumentId),
    Filter(FilterPredicate),

    // NEW: Spatial queries
    WithinRadius { center: GeoPoint, radius_meters: f64 },
    WithinBounds { min: GeoPoint, max: GeoPoint },

    // NEW: Compound queries
    And(Box<Query>, Box<Query>),
    Or(Box<Query>, Box<Query>),
}

/// Subscription with QoS policy
pub struct Subscription {
    pub collection: String,
    pub query: Query,
    pub qos_policy: QoSPolicy,  // Includes sync_mode
}

// Usage: Subscribe to nearby beacons with LatestOnly sync
let subscription = backend.subscribe_with_qos(
    "beacons",
    Query::WithinRadius {
        center: my_position,
        radius_meters: 5000.0
    },
    QoSPolicy {
        priority_class: QoSClass::Low,
        sync_mode: SyncMode::LatestOnly,
        ..Default::default()
    }
).await?;
```

### Default Sync Modes by Data Type

| Data Type | Default Sync Mode | Rationale |
|-----------|-------------------|-----------|
| Position/Beacon | LatestOnly | Only current location matters |
| Health/Status | LatestOnly | Only current state matters |
| Contact Reports | FullHistory | All sightings are important |
| Commands | FullHistory | Audit trail required |
| Images/Media | LatestOnly | Only latest version needed |
| Audit Logs | FullHistory | Complete history required |
| Track History | WindowedHistory(300) | Last 5 minutes useful |
| Capability State | WindowedHistory(60) | Recent changes useful |

### Impact on Issue #346

The current sync failure at scale is caused by:
1. Every document change triggers broadcast notification
2. Broadcast channel capacity (8192) overwhelmed under load
3. Lagged messages trigger expensive full resync

With sync modes:
1. **LatestOnly** documents don't need delta tracking at all
2. Reconnection sends current state, not history
3. Broadcast channel only needs to track "document changed", not each delta
4. Full resync is cheap (just current state per document)

**Estimated impact:**
- Current: 2,100 delta syncs after 5-minute disconnect (7 soldiers × 60 sec × 5 min)
- With LatestOnly: 7 state syncs after 5-minute disconnect
- **300× reduction in reconnection sync traffic**

### Observer Behavior with Sync Modes

```rust
/// Observer callback receives mode-appropriate events
pub enum ChangeEvent {
    /// Initial snapshot when subscription starts
    Initial { documents: Vec<Document> },

    /// Document updated (FullHistory: every delta, LatestOnly: current state)
    Updated { collection: String, document: Document },

    /// NEW: For FullHistory mode - individual delta applied
    DeltaApplied {
        collection: String,
        doc_id: DocumentId,
        delta: AutomergeDelta,
        resulting_state: Document,
    },

    /// Document removed
    Removed { collection: String, doc_id: DocumentId },
}

// Observer behavior depends on sync mode:
//
// FullHistory subscription:
//   - Receives DeltaApplied for each historical change
//   - Can reconstruct full history
//   - More events, higher bandwidth
//
// LatestOnly subscription:
//   - Receives Updated with current state only
//   - No history available
//   - Fewer events, lower bandwidth
```

### Configuration Example

```yaml
# config/qos_policies.yaml

collections:
  beacons:
    priority_class: low
    sync_mode: latest_only
    ttl_seconds: 300

  contact_reports:
    priority_class: critical
    sync_mode: full_history
    ttl_seconds: null  # Never expire

  track_history:
    priority_class: low
    sync_mode: windowed_history
    window_seconds: 300  # Last 5 minutes

  squad_summaries:
    priority_class: high
    sync_mode: latest_only  # Only current aggregation matters
```

### Implementation Phases

**Phase 1 (Immediate - Issue #346 fix):**
- Add `SyncMode` enum
- Implement `LatestOnly` mode using `doc.save()`
- Add per-collection sync mode configuration
- Default beacons/status to `LatestOnly`

**Phase 2 (Short-term):**
- Implement `WindowedHistory` with time-based filtering
- Add spatial query support (`WithinRadius`, `WithinBounds`)
- Extend subscription API with QoS policy

**Phase 3 (Medium-term):**
- Compound queries (`And`, `Or`)
- Dynamic subscription updates (change query without re-subscribing)
- Sync mode metrics and observability

## Future Work

### Phase 5: Advanced Features (Post-MVP)

1. **Machine Learning for Priority Tuning**
   - Learn optimal priorities from operator corrections
   - Predict bandwidth availability, pre-adjust priorities
   - Anomaly detection: flag unusual priority patterns

2. **Multi-Criteria Optimization**
   - Balance latency, bandwidth, storage simultaneously
   - Pareto-optimal sync strategies
   - User-defined optimization functions

3. **Content-Based Routing**
   - Route high-priority data over best available path
   - Multi-path redundancy for critical data
   - Erasure coding for reliable delivery

4. **Application-Level QoS**
   - Allow applications to register custom QoS policies
   - Per-mission profile library
   - Dynamic policy updates during mission

---

**Status**: Proposed (pending review and validation)  
**Next Steps**:
1. Review with stakeholders (military operators, systems engineers)
2. Validate QoS class definitions against real operational scenarios
3. Prototype Phase 1 (core infrastructure) in cap-transport
4. Define success metrics and validation experiments
5. Begin integration with Automerge + Iroh (leverages QUIC priorities)

**Authors**: Kit Plummer, Codex  
**Last Updated**: 2025-11-16
