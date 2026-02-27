# Lab 4: Hierarchical PEAT CRDT - Testing Guide

**Purpose**: Prove hierarchical architecture enables O(log n) scaling with bounded latency and 95% bandwidth savings

**Core Thesis**: Hierarchical aggregation reduces cross-tier traffic, enabling 1000+ node deployments where flat mesh fails at 30-50 nodes

---

## Quick Start

### Prerequisites

Ensure you have a `.env` file in the parent directory (`peat/.env`) with:
```bash
PEAT_APP_ID="your-app-id"
PEAT_SHARED_KEY="your-shared-key"
PEAT_SECRET_KEY="your-secret-key"
```

### Deploy and Monitor

```bash
# Build the Docker image first
make build

# Deploy 24-node topology (fastest for validation)
make lab4-24

# Wait 30 seconds for connections to establish, then check metrics
make lab4-metrics

# Watch real-time status
make lab4-watch

# View aggregation results
make lab4-results

# Clean up when done
make lab4-destroy
```

### Available Scale Options

| Command | Nodes | Structure | Use Case |
|---------|-------|-----------|----------|
| `make lab4-24` | 24 | 1 company, 1 platoon, 3 squads × 7 soldiers | Quick validation |
| `make lab4-48` | 48 | 1 company, 2 platoons, 6 squads × 7 soldiers | Medium test |
| `make lab4-96` | 96 | 1 company, 4 platoons, 12 squads × 7 soldiers | Scale test |
| `make lab4-384` | 384 | 1 company, 16 platoons, 48 squads × 7 soldiers | Full scale |

### Metrics Commands

| Command | Description |
|---------|-------------|
| `make lab4-metrics` | Show latency, throughput, and aggregation metrics |
| `make lab4-results` | Show hierarchical aggregation results |
| `make lab4-status` | Show deployment status and memory usage |
| `make lab4-watch` | Continuously monitor (Ctrl+C to stop) |
| `make lab4-save-logs` | Save container logs to `/work/peat-sim-results/` |
| `make lab4-experiment` | Run full matrix (all backends × nodes × bandwidths) with comparison |
| `make lab4-single` | Run single test (NODES, BACKEND, BANDWIDTH) |
| `make lab4-analyze` | Analyze saved results (uses `--latest` or `RESULTS_DIR=path`) |
| `make lab4-destroy` | Destroy all Lab 4 containers |

### Running Full Experiments

#### Full Matrix Comparison (Recommended)

Run all combinations of backends, node counts, and bandwidths:

```bash
# Run full matrix: 2 backends × 4 node counts × 4 bandwidths = 32 tests
make lab4-experiment

# With custom duration per test (default: 120s)
DURATION=60 make lab4-experiment
```

**Output:**
- Individual results: `/work/peat-sim-results/lab4-{backend}-{nodes}n-{bandwidth}-{timestamp}/`
- Summary CSV: `/work/peat-sim-results/lab4-comparison-{timestamp}.csv`
- Comparison report: `/work/peat-sim-results/lab4-comparison-report-{timestamp}.md`

#### Single Test

Run a specific configuration:

```bash
# Single test with specific parameters
NODES=96 BACKEND=automerge BANDWIDTH=256kbps make lab4-single

# Results saved to: /work/peat-sim-results/lab4-automerge-96n-256kbps-YYYYMMDD-HHMMSS/
```

#### Manual Workflow

```bash
# Deploy first
BANDWIDTH=256kbps make lab4-24

# Check live metrics
make lab4-metrics

# Save logs before destroying
BACKEND=automerge BANDWIDTH=256kbps make lab4-save-logs

# Analyze saved results
make lab4-analyze                           # Uses most recent results
RESULTS_DIR=/work/peat-sim-results/lab4-... make lab4-analyze  # Specific dir
```

### Backend Options

Lab 4 supports both Automerge and Ditto backends. Override with the `BACKEND` environment variable:

```bash
# Default: Automerge backend
make lab4-24

# Explicitly use Automerge
BACKEND=automerge make lab4-24

# Use Ditto backend
BACKEND=ditto make lab4-24

# In-memory storage (default: true for faster testing)
CAP_IN_MEMORY=false BACKEND=automerge make lab4-24
```

### Bandwidth Options

Lab 4 supports 4 bandwidth profiles to simulate different network conditions:

| Bandwidth | Use Case | Example |
|-----------|----------|---------|
| `1gbps` | LAN/datacenter (default) | `make lab4-24` |
| `100mbps` | Good tactical network | `BANDWIDTH=100mbps make lab4-24` |
| `1mbps` | Degraded tactical network | `BANDWIDTH=1mbps make lab4-24` |
| `256kbps` | VHF radio / SATCOM | `BANDWIDTH=256kbps make lab4-24` |

```bash
# Deploy 24 nodes with 256kbps bandwidth constraint
BANDWIDTH=256kbps make lab4-24

# Combine backend and bandwidth options
BACKEND=ditto BANDWIDTH=1mbps make lab4-96

# Full example: Ditto backend, 100mbps, persistent storage
BACKEND=ditto BANDWIDTH=100mbps CAP_IN_MEMORY=false make lab4-384
```

Topology files are auto-generated on first use if they don't exist.

### Example Output

```
=== P2P Sync Latency (per tier, excluding warmup >500ms) ===
  Soldier → Squad:   Min: 0.91 ms | Avg: 110.94 ms | Max: 466.47 ms (n=17)
  Squad → Platoon:   Min: 0.35 ms | Avg: 1.78 ms | Max: 3.49 ms (n=3)
  Platoon → Company: Min: 0.00 ms | Avg: 0.01 ms | Max: 0.01 ms (n=11)

=== Reduction Ratio (per tier) ===
  Squad (7 NodeStates → 1 Summary):   7.0
  Platoon (3 Squads → 1 Summary):     3.0
  Company (1 Platoon → 1 Summary):    1.0
```

---

## What We Are Proving

### The Problem with Flat Mesh (Lab 3b)
- Every node syncs with every other node: O(n²) connections
- At 50 nodes: 2,450 sync relationships, network saturated
- Latency explodes as n grows

### The Hierarchical Solution (Lab 4)
- Soldiers sync only with their squad leader (7:1)
- Squad leaders sync only with their platoon leader (3:1)
- Platoon leaders sync only with company commander (4:1)
- Total reduction: 7 × 3 × 4 = **84:1 bandwidth savings per update cycle**

---

## Critical Metrics (What We MUST Measure)

### 1. Cross-Tier Propagation Latency (PRIMARY METRIC)

**NOT local upsert latency** - we measure time for data to traverse tiers:

| Metric | Description | Target |
|--------|-------------|--------|
| Soldier → Squad Leader | Time from soldier write to squad leader DocumentReceived | <50ms P95 |
| Squad → Platoon Leader | Time from squad summary to platoon leader DocumentReceived | <50ms P95 |
| Platoon → Commander | Time from platoon summary to commander DocumentReceived | <50ms P95 |
| **End-to-End** | Total time: Soldier write → Commander receives | <100ms P95 |

**How to measure**:
```json
{"event_type":"DocumentReceived","latency_type":"soldier_to_squad_leader","latency_ms":45.2}
{"event_type":"DocumentReceived","latency_type":"squad_to_platoon_leader","latency_ms":38.1}
{"event_type":"DocumentReceived","latency_type":"platoon_to_commander","latency_ms":22.5}
```

### 2. Throughput (Bytes/Second Per Tier)

| Tier | Expected Traffic | Measurement |
|------|------------------|-------------|
| Soldier → Squad | High (raw data) | bytes_sent / duration |
| Squad → Platoon | ~7x less (aggregated) | bytes_sent / duration |
| Platoon → Commander | ~21x less (double aggregated) | bytes_sent / duration |

**Bandwidth Savings Calculation**:
```
Flat Mesh (Lab 3b):    n × (n-1) × msg_size × updates_per_sec
Hierarchical (Lab 4):  n × msg_size × updates_per_sec (soldiers only send to 1 leader)

Savings = 1 - (Hierarchical / Flat Mesh) = 95%+ at 100 nodes
```

### 3. Connection Establishment (MUST COMPLETE BEFORE MEASURING)

**Warmup Phase Requirements**:
1. All nodes start and initialize backend
2. All TCP/QUIC connections established
3. Initial sync handshake completed
4. First document exchange verified
5. **ONLY THEN** start measuring latency/throughput

**Warmup Verification**:
```
[squad-1-leader] ✓ Connected to platoon-1-leader
[squad-1-leader] ✓ 7/7 soldiers connected
[squad-1-leader] WARMUP COMPLETE - starting measurement phase
```

---

## Experiment Protocol

### Phase 1: Deploy and Warmup (DO NOT MEASURE)
```
0:00 - 0:30   Deploy containers
0:30 - 1:00   Connection establishment
1:00 - 1:30   Initial sync and document exchange
1:30 - 2:00   Verify all connections stable
```

### Phase 2: Measurement (MEASURE HERE)
```
2:00 - 4:00   Steady-state measurement period
              - Cross-tier latency samples
              - Throughput bytes/second
              - Aggregation events
```

### Phase 3: Cleanup
```
4:00 - 4:30   Final log collection
              Destroy topology
              Process metrics
```

---

## Metrics Events (What the Code MUST Emit)

### Cross-Tier Latency (REQUIRED)

When a document propagates from one tier to another:

```json
{
  "event_type": "DocumentReceived",
  "node_id": "squad-1-leader",
  "doc_id": "soldier-1-status",
  "source_tier": "soldier",
  "dest_tier": "squad_leader",
  "latency_type": "soldier_to_squad_leader",
  "created_at_us": 1766129764445279,
  "received_at_us": 1766129764495279,
  "latency_ms": 50.0,
  "is_warmup": false
}
```

### Throughput (REQUIRED)

Every 10 seconds, emit tier throughput:

```json
{
  "event_type": "TierThroughput",
  "node_id": "squad-1-leader",
  "tier": "squad_leader",
  "interval_secs": 10,
  "bytes_received": 25600,
  "bytes_sent": 3657,
  "docs_received": 70,
  "docs_sent": 10,
  "throughput_in_bps": 2560,
  "throughput_out_bps": 365
}
```

### Aggregation Efficiency (REQUIRED)

On each aggregation cycle:

```json
{
  "event_type": "AggregationCompleted",
  "node_id": "squad-1-leader",
  "tier": "squad",
  "input_docs": 7,
  "input_bytes": 2450,
  "output_docs": 1,
  "output_bytes": 350,
  "reduction_ratio": 7.0,
  "bytes_saved": 2100,
  "processing_time_us": 150
}
```

### Warmup Complete (REQUIRED)

When a node is ready for measurement:

```json
{
  "event_type": "WarmupComplete",
  "node_id": "squad-1-leader",
  "peers_connected": 8,
  "docs_synced": 14,
  "ready_for_measurement": true,
  "timestamp_us": 1766129760000000
}
```

---

## Expected Results

### Latency Comparison: Lab 3b vs Lab 4

| Nodes | Lab 3b P95 | Lab 4 P95 (End-to-End) | Improvement |
|-------|------------|------------------------|-------------|
| 24    | ~77ms      | <30ms                  | 2.5× faster |
| 48    | ~400ms     | <50ms                  | 8× faster   |
| 96    | FAIL       | <70ms                  | ∞ (3b breaks) |
| 384   | N/A        | <100ms                 | Proves scaling |
| 1000  | N/A        | <150ms                 | Battalion scale |

### Throughput Comparison

| Scale | Flat Mesh Bytes/sec | Hierarchical Bytes/sec | Savings |
|-------|---------------------|------------------------|---------|
| 24    | 86,400              | 8,640                  | 90%     |
| 96    | 1,382,400           | 34,560                 | 97.5%   |
| 384   | 22,118,400          | 138,240                | 99.4%   |

---

## Success Criteria

### ✅ Technical Success

1. **Warmup completes** before measurement starts
2. **Cross-tier latency** measured (not local upsert)
3. **Throughput** measured at each tier
4. **All tiers receive documents** (soldier → squad → platoon → commander)

### ✅ Scientific Success

1. **End-to-end P95 <100ms** at 384 nodes
2. **Throughput reduction** matches theoretical (7:1 at squad, 21:1 at platoon)
3. **Scales beyond Lab 3b** failure point (50 nodes)
4. **Bandwidth savings >90%** vs flat mesh

### ✅ Epic #132 Completion

1. **Hierarchical architecture proven** to enable scale
2. **Quantitative comparison** with Lab 3b
3. **Reproducible methodology** documented
4. **No measuring during warmup** - only steady-state

---

## Common Mistakes to Avoid

### ❌ WRONG: Measuring CRDTUpsert Latency
```json
{"event_type":"CRDTUpsert","latency_ms":2.3}  // This is LOCAL write time, not useful
```

### ✅ RIGHT: Measuring Cross-Tier Propagation
```json
{"event_type":"DocumentReceived","latency_type":"soldier_to_squad_leader","latency_ms":45.2}
```

### ❌ WRONG: Measuring During Connection Setup
```
[0:05] Started measurement  // TOO EARLY - connections not established
```

### ✅ RIGHT: Wait for Warmup
```
[0:05] Connecting peers...
[0:30] All peers connected
[0:45] Initial sync complete
[1:00] WARMUP COMPLETE - starting measurement
[1:00] Started measurement  // CORRECT - steady state
```

### ❌ WRONG: Only Measuring Latency
```
P95 latency: 45ms  // Incomplete - where's throughput?
```

### ✅ RIGHT: Latency AND Throughput
```
End-to-End P95: 45ms
Soldier → Squad: 2,560 bytes/sec (7 soldiers × 365 bytes × 1/sec)
Squad → Platoon: 365 bytes/sec (1 summary × 365 bytes × 1/sec)
Bandwidth Savings: 85.7%
```

---

## Implementation Checklist

Before running Lab 4, verify:

- [ ] `DocumentReceived` events emit `latency_type` for cross-tier tracking
- [ ] `TierThroughput` events emit every 10 seconds
- [ ] `WarmupComplete` event emitted when node is ready
- [ ] Measurement phase does NOT start until ALL nodes emit `WarmupComplete`
- [ ] End-to-end latency calculated (soldier write → commander receive)
- [ ] Throughput calculated for each tier direction

---

## Quick Validation

```bash
# Run 24-node test with 2-minute duration
./run-lab4-experiment.sh --nodes 24 --duration 120

# Check for required metrics
grep "DocumentReceived" logs/*.log | grep "latency_type" | head -5
grep "TierThroughput" logs/*.log | head -5
grep "WarmupComplete" logs/*.log | head -5

# If any of these are missing, the metrics code needs fixing
```

---

## Troubleshooting

### Latency Shows N/A

**Symptom**: `make lab4-metrics` shows "N/A" for sync latencies.

**Cause**: Latency measurements filter out warmup period (>500ms). If all measurements are during warmup, no steady-state data exists.

**Solution**: Wait 30+ seconds after deployment before running `make lab4-metrics`.

### High Latency (>1 second)

**Symptom**: Avg latency is several seconds instead of milliseconds.

**Cause**: Initial peer discovery takes ~5 seconds. Measurements during this phase inflate averages.

**Solution**: The metrics automatically filter latencies >500ms. If still high:
1. Check container logs for connection errors: `docker logs <container> 2>&1 | grep -i error`
2. Verify all containers are running: `make lab4-status`
3. Ensure adequate system resources (CPU, memory, network)

### Sync Loop / Excessive Messages

**Historical Fix**: Prior versions had a bug where receiving a document triggered syncing it back to the sender, creating an infinite loop.

**Resolution** (already applied in `peat-protocol/src/storage/automerge_sync.rs`):
- Changed `store.put()` to `store.put_without_notify()` when saving received documents
- This still triggers observers (for aggregation) but avoids the sync-back loop

### Reduction Ratio Shows Wrong Values

**Expected values**:
- Squad: 7.0 (7 soldiers → 1 summary)
- Platoon: 3.0 (3 squads → 1 summary)
- Company: varies by topology (e.g., 1.0 for single platoon, 4.0 for 4 platoons)

**If incorrect**: Check that aggregation is completing (`AggregationCompleted` events in logs).
