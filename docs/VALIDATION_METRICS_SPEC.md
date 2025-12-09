# HIVE Protocol Validation Metrics Specification

**Document Version**: 1.0
**Date**: 2025-12-08
**Status**: Draft
**Issue**: #296

---

## Executive Summary

This document defines the metrics that will validate HIVE Protocol demo success and the methods for collecting them. These metrics are derived from the [M1 Vignette Use Case](../hive-inference/docs/HIVE-Vignette-M1/VIGNETTE_USE_CASE.md) success criteria (Section 6) and align with the existing metrics infrastructure in the codebase.

---

## 1. Performance Metrics (P1-P5)

### P1: Track Update Latency

| Attribute | Value |
|-----------|-------|
| **Metric ID** | P1 |
| **Description** | End-to-end latency from edge AI track detection to C2 display |
| **Target** | < 2 seconds |
| **Priority** | Critical |

**What We Measure**:
- Time from `DocumentInserted` event at edge node to `DocumentReceived` event at C2/coordinator
- Includes: CRDT serialization, network transmission, TAK bridge translation, TAK Server delivery

**Collection Method**:
```
Origin: origin_updated_at_us (microseconds) - when track created at edge
Destination: received_at_us (microseconds) - when C2 receives track
Latency = received_at_us - origin_updated_at_us
```

**Existing Infrastructure**:
- `hive-sim/analyze_metrics.py`: `analyze_convergence()` function
- ADR-023: End-to-end propagation latency measurement
- Event types: `DocumentInserted`, `DocumentReceived`, `latency_ms` field

**Output Format** (JSONL):
```json
{
  "event_type": "DocumentReceived",
  "doc_id": "track-001",
  "origin_node_id": "alpha-3",
  "origin_updated_at_us": 1733680800000000,
  "received_at_us": 1733680801500000,
  "latency_ms": 1500,
  "receiving_node_id": "coordinator"
}
```

**Validation**:
- P50 latency < 1 second
- P95 latency < 2 seconds
- P99 latency < 3 seconds (degraded network acceptable)

---

### P2: Bandwidth Usage

| Attribute | Value |
|-----------|-------|
| **Metric ID** | P2 |
| **Description** | Bandwidth consumed for tracking operations (excluding model updates) |
| **Target** | < 10 Kbps per active track |
| **Priority** | Critical |

**What We Measure**:
- Total bytes transmitted for track updates
- Message frequency (updates per second)
- Per-node-type bandwidth breakdown

**Collection Method**:
```
Track update size: message_size_bytes from MessageSent events
Update frequency: 2 Hz typical (configurable)
Bandwidth = (message_size_bytes * update_frequency) / 1000 * 8 Kbps
```

**Existing Infrastructure**:
- `hive-transport/src/tak/metrics.rs`: `TakMetrics.messages_sent_bytes`
- `hive-protocol/src/qos/bandwidth.rs`: `BandwidthAllocation` with per-class tracking
- `hive-sim/analyze_metrics.py`: `analyze_traffic()` function

**Output Format** (JSONL):
```json
{
  "event_type": "MessageSent",
  "node_id": "alpha-3",
  "message_type": "track_update",
  "message_size_bytes": 512,
  "timestamp_us": 1733680800000000
}
```

**Validation**:
- Track update size: ~500 bytes (see vignette: 500 bytes @ 2 Hz = ~1 Kbps)
- Total bandwidth for tracking: < 10 Kbps
- Compare to baseline: Traditional video streaming = 5 Mbps
- Target reduction: > 95%

**Bandwidth Comparison Table**:
| Scenario | Bandwidth | Notes |
|----------|-----------|-------|
| HIVE track updates | ~1 Kbps | 500 bytes @ 2 Hz |
| Traditional video | 5 Mbps | Per camera stream |
| Savings | 99.98% | Track data only |

---

### P3: Handoff Gap

| Attribute | Value |
|-----------|-------|
| **Metric ID** | P3 |
| **Description** | Time gap in track continuity during team-to-team handoff |
| **Target** | < 10 seconds (functional: < 5 seconds) |
| **Priority** | High |

**What We Measure**:
- Time between last track update from releasing team and first track update from acquiring team
- Track correlation success rate
- Handoff prediction accuracy

**Collection Method**:
```
Last Alpha track: timestamp when Alpha-3 marks track as HANDED_OFF
First Bravo track: timestamp when Bravo-3 first detects correlated track
Gap = first_bravo_timestamp - last_alpha_timestamp
```

**Event Sequence**:
1. `TrackHandoffInitiated` - Coordinator sends PREPARE_HANDOFF
2. `TrackStatusChanged` - Alpha marks track as HANDED_OFF
3. `TrackAcquired` - Bravo confirms acquisition
4. `TrackCorrelated` - Coordinator correlates TRACK-001 == TRACK-002

**Output Format** (JSONL):
```json
{
  "event_type": "TrackHandoff",
  "track_id": "TRACK-001",
  "releasing_team": "alpha",
  "acquiring_team": "bravo",
  "handoff_initiated_at_us": 1733681200000000,
  "handoff_completed_at_us": 1733681203000000,
  "gap_ms": 3000,
  "correlation_success": true
}
```

**Validation**:
- Handoff gap < 5 seconds (target)
- Handoff gap < 10 seconds (acceptable)
- Track correlation accuracy > 95%

---

### P4: Model Distribution Time

| Attribute | Value |
|-----------|-------|
| **Metric ID** | P4 |
| **Description** | Time to distribute AI model update to all target platforms |
| **Target** | < 5 minutes for 45 MB model |
| **Priority** | High |

**What We Measure**:
- Total time from model push initiation to all platforms running new version
- Per-platform download time
- Verification and deployment time

**Collection Method**:
```
Start: C2/MLOps initiates model push
End: All target platforms re-advertise with new model_version
Duration = last_platform_ready - model_push_initiated
```

**Event Sequence**:
1. `ModelPushInitiated` - MLOps starts distribution
2. `ModelDownloadStarted` - Platform begins receiving blob
3. `ModelDownloadCompleted` - Platform has full model (hash verified)
4. `ModelDeployed` - Platform hot-swaps to new model
5. `CapabilityReadvertised` - Platform advertises new version

**Output Format** (JSONL):
```json
{
  "event_type": "ModelDeploymentComplete",
  "model_id": "object_tracker",
  "model_version": "1.3.0",
  "model_size_bytes": 45000000,
  "target_platforms": ["alpha-3", "bravo-3"],
  "push_initiated_at_us": 1733681500000000,
  "all_platforms_ready_at_us": 1733681740000000,
  "total_duration_ms": 240000,
  "per_platform_durations_ms": {
    "alpha-3": 180000,
    "bravo-3": 220000
  }
}
```

**Validation**:
- Total distribution time < 5 minutes (300 seconds)
- Individual platform deployment < 4 minutes
- Hash verification success rate: 100%
- Network constraint: Must work on 500 Kbps link (P5 QoS bulk)

**Calculation for 500 Kbps link**:
```
45 MB = 360 Mbit
At 500 Kbps = 720 seconds (12 minutes) theoretical minimum
With compression + delta transfer: Target < 5 minutes
```

---

### P5: Hot-Swap Interruption

| Attribute | Value |
|-----------|-------|
| **Metric ID** | P5 |
| **Description** | Tracking interruption during model hot-swap |
| **Target** | < 5 seconds |
| **Priority** | High |

**What We Measure**:
- Gap in track updates during model swap
- Time from model unload to new model producing detections

**Collection Method**:
```
Last track before swap: timestamp of last track update with old model
First track after swap: timestamp of first track update with new model
Interruption = first_new_model_track - last_old_model_track
```

**Event Sequence**:
1. `ModelSwapStarted` - Platform begins hot-swap
2. `TrackingPaused` - Tracking temporarily suspended
3. `ModelLoaded` - New model loaded into memory
4. `TrackingResumed` - Tracking resumes with new model
5. `CapabilityReadvertised` - New capability advertised

**Output Format** (JSONL):
```json
{
  "event_type": "ModelHotSwap",
  "platform_id": "alpha-3",
  "old_version": "1.2.0",
  "new_version": "1.3.0",
  "swap_started_at_us": 1733681700000000,
  "swap_completed_at_us": 1733681702000000,
  "interruption_ms": 2000,
  "tracks_affected": ["TRACK-001"]
}
```

**Validation**:
- Hot-swap interruption < 2 seconds (target)
- Hot-swap interruption < 5 seconds (acceptable)
- No track loss during swap (track resumes after pause)

---

## 2. Functional Metrics (F1-F7)

### F1: Team Formation Time

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F1 |
| **Description** | Time for teams to form and advertise capabilities |
| **Target** | < 30 seconds |
| **Validation** | Automated test |

**Collection Method**:
- Time from first node boot to all team members discovered and capabilities aggregated
- Log `TeamFormed` events with timestamps

---

### F2: C2 Track Command

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F2 |
| **Description** | C2 can issue track command via WebTAK |
| **Target** | Command received by teams |
| **Validation** | End-to-end test |

**Collection Method**:
- Log CoT mission command at TAK Server
- Log HIVE command at HIVE-TAK Bridge
- Log command receipt at team level

---

### F3: Track Display Latency

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F3 |
| **Description** | Tracks appear on WebTAK within 2 seconds of detection |
| **Target** | < 2 seconds |
| **Validation** | Latency measurement |

**Note**: Same as P1, but specifically measured at WebTAK display.

---

### F4: Track Handoff Continuity

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F4 |
| **Description** | Track handoff completes with minimal gap |
| **Target** | < 5 second gap |
| **Validation** | Continuity analysis |

**Note**: Same as P3.

---

### F5: Model Distribution Completion

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F5 |
| **Description** | Model update distributes to all platforms |
| **Target** | < 5 minutes |
| **Validation** | Timing measurement |

**Note**: Same as P4.

---

### F6: Capability Re-Advertisement

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F6 |
| **Description** | Platforms re-advertise capability after model update |
| **Target** | Within 10 seconds of model swap |
| **Validation** | Log verification |

**Collection Method**:
- Log `CapabilityAdvertisement` events with model_version
- Verify new version appears after model deployment

---

### F7: Mission Summary

| Attribute | Value |
|-----------|-------|
| **Metric ID** | F7 |
| **Description** | Mission summary available in WebTAK at completion |
| **Target** | Summary displayed |
| **Validation** | Manual verification |

---

## 3. MLOps Metrics (M1-M4)

### M1: Model Hash Verification

| Attribute | Value |
|-----------|-------|
| **Metric ID** | M1 |
| **Description** | Model hash verified before deployment |
| **Target** | 100% verification |
| **Validation** | Hash check log |

**Collection Method**:
```json
{
  "event_type": "ModelHashVerified",
  "model_version": "1.3.0",
  "expected_hash": "sha256:b8d9c4e2...",
  "actual_hash": "sha256:b8d9c4e2...",
  "verified": true
}
```

---

### M2: Rolling Deployment Interruption

| Attribute | Value |
|-----------|-------|
| **Metric ID** | M2 |
| **Description** | Rolling deployment doesn't interrupt tracking > 5s |
| **Target** | < 5 seconds |
| **Validation** | Gap analysis |

**Note**: Same as P5.

---

### M3: Capability Re-Advertisement Timing

| Attribute | Value |
|-----------|-------|
| **Metric ID** | M3 |
| **Description** | Capability re-advertised within 10s of model swap |
| **Target** | < 10 seconds |
| **Validation** | Log timing |

**Collection Method**:
```
Time from ModelSwapCompleted to CapabilityAdvertisement with new version
```

---

### M4: Rollback Capability

| Attribute | Value |
|-----------|-------|
| **Metric ID** | M4 |
| **Description** | Rollback possible if deployment fails |
| **Target** | Successful rollback |
| **Validation** | Inject failure test |

**Test Procedure**:
1. Push model with corrupted hash
2. Verify deployment fails hash check
3. Verify platform rolls back to previous version
4. Verify tracking continues on previous version

---

## 4. Collection Infrastructure

### 4.1 Existing Tools

| Tool | Location | Purpose |
|------|----------|---------|
| `analyze_metrics.py` | hive-sim/ | Parse JSONL, calculate P50/P90/P95/P99 |
| `InMemoryMetricsCollector` | hive-mesh/src/topology/metrics.rs | Pluggable metrics collection |
| `TakMetrics` | hive-transport/src/tak/metrics.rs | TAK transport metrics |
| `InferenceMetrics` | hive-inference/src/inference/metrics.rs | AI inference timing |
| `BandwidthAllocation` | hive-protocol/src/qos/bandwidth.rs | Per-QoS bandwidth tracking |

### 4.2 Metrics Output Format

All metrics are emitted as **JSON Lines (JSONL)** format to container stdout:

```
METRICS: {"event_type": "...", "timestamp_us": ..., ...}
```

Extracted via:
```bash
grep 'METRICS:' container.log | sed 's/.*METRICS: //' > metrics.jsonl
```

### 4.3 Analysis Pipeline

```
1. Deploy topology (containerlab)
2. Run test scenario
3. Collect logs from all containers
4. Extract METRICS lines
5. Run analyze_metrics.py
6. Generate report with P50/P90/P95/P99
```

### 4.4 Network Emulation

Tests should include network constraint scenarios:

| Scenario | Bandwidth | Latency | Packet Loss |
|----------|-----------|---------|-------------|
| Ideal | 1 Gbps | 1 ms | 0% |
| Good | 100 Mbps | 10 ms | 0% |
| Constrained | 1 Mbps | 50 ms | 1% |
| Tactical Radio | 500 Kbps | 100 ms | 5% |
| Degraded | 256 Kbps | 200 ms | 10% |

---

## 5. Validation Test Matrix

| Metric | Ideal | Constrained | Tactical | Pass Criteria |
|--------|-------|-------------|----------|---------------|
| P1 (Latency) | < 500 ms | < 1.5 s | < 2 s | P95 < target |
| P2 (Bandwidth) | < 5 Kbps | < 10 Kbps | < 10 Kbps | Avg < target |
| P3 (Handoff) | < 3 s | < 5 s | < 10 s | Max < target |
| P4 (Model Dist) | < 1 min | < 3 min | < 5 min | Max < target |
| P5 (Hot-Swap) | < 1 s | < 2 s | < 5 s | Max < target |

---

## 6. Implementation Notes

### 6.1 Timestamp Precision

All timestamps use **microsecond precision** (`_us` suffix):
- `timestamp_us`: Event occurrence time
- `origin_updated_at_us`: Original document creation time
- `received_at_us`: Document receipt time

Use `std::time::SystemTime` for wall-clock time, converted to microseconds since UNIX epoch.

### 6.2 Event Types for Vignette

New event types needed for full vignette metrics:

```rust
pub enum MetricsEvent {
    // Existing
    DocumentInserted,
    DocumentReceived,
    MessageSent,

    // Track Lifecycle
    TrackCreated,
    TrackUpdated,
    TrackHandoffInitiated,
    TrackHandoffCompleted,
    TrackLost,

    // Model Lifecycle
    ModelPushInitiated,
    ModelDownloadStarted,
    ModelDownloadCompleted,
    ModelHashVerified,
    ModelSwapStarted,
    ModelSwapCompleted,

    // Capability
    CapabilityAdvertised,
    TeamFormed,
}
```

### 6.3 Reference: Vignette Success Criteria

From [VIGNETTE_USE_CASE.md](../hive-inference/docs/HIVE-Vignette-M1/VIGNETTE_USE_CASE.md) Section 6:

**Performance Requirements**:
- P1: Track update latency < 2 seconds
- P2: Bandwidth usage < 10 Kbps
- P3: Handoff detection accuracy > 95%
- P4: Model distribution < 5 minutes
- P5: System operates on 500 Kbps link

**Functional Requirements**:
- F1: Team formation < 30 seconds
- F2: C2 track command via WebTAK
- F3: Tracks display < 2 seconds
- F4: Handoff gap < 5 seconds
- F5: Model distribution to all platforms < 5 minutes
- F6: Capability re-advertisement after model update
- F7: Mission summary in WebTAK

---

## Appendix A: Sample Analysis Output

```
================================================================================
                    HIVE M1 VIGNETTE VALIDATION REPORT
================================================================================

Test: Object Tracking Handoff Demo
Date: 2025-12-08
Duration: 35 minutes
Network: Constrained (1 Mbps, 50ms latency)

PERFORMANCE METRICS
-------------------

P1: Track Update Latency (Edge → C2)
  Target: < 2000 ms
  P50: 823 ms    ✓
  P90: 1245 ms   ✓
  P95: 1456 ms   ✓
  P99: 1892 ms   ✓
  Status: PASS

P2: Bandwidth Usage
  Target: < 10 Kbps
  Track updates: 1.2 Kbps   ✓
  Overhead: 0.3 Kbps
  Total: 1.5 Kbps           ✓
  vs Traditional (5 Mbps): 99.97% reduction
  Status: PASS

P3: Handoff Gap
  Target: < 10 seconds
  Handoff 1 (Alpha → Bravo): 3.2 seconds  ✓
  Status: PASS

P4: Model Distribution
  Target: < 5 minutes
  Model size: 45 MB
  Distribution time: 3 min 42 sec  ✓
  All platforms updated: Yes
  Status: PASS

P5: Hot-Swap Interruption
  Target: < 5 seconds
  Alpha-3 swap: 1.8 seconds  ✓
  Bravo-3 swap: 2.1 seconds  ✓
  Status: PASS

OVERALL: 5/5 PASS
================================================================================
```

---

## Appendix B: Related Documents

- [ADR-023: End-to-End Propagation Latency Measurement](adr/023-end-to-end-propagation-latency-measurement.md)
- [ADR-019: QoS and Data Prioritization](adr/019-qos-and-data-prioritization.md)
- [Vignette Use Case](../hive-inference/docs/HIVE-Vignette-M1/VIGNETTE_USE_CASE.md)
- [Existing Validation Results](VALIDATION_RESULTS.md)
- [Lab 4 Hierarchical Metrics](../hive-sim/LAB-4-PHASE1-METRICS-COMPLETE.md)

---

*Document End*
