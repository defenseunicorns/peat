# E12: Comprehensive Empirical Validation of CAP Protocol

**Status:** Infrastructure Complete - Ready for Execution
**Date:** November 9, 2025

---

## Objective

Provide **rigorous empirical proof** of CAP Protocol's architectural advantages through comprehensive testing across:
- 3 Architectures (Traditional IoT, CAP Full Mesh, CAP Hierarchical)
- Multiple Scales (2, 12, 24 nodes)
- 4 Bandwidth Constraints (1Gbps, 100Mbps, 1Mbps, 256Kbps)

---

## What This Lab Proves

### Core Claims

1. **CRDT Differential Sync** reduces bandwidth 60-95% vs traditional full-message IoT
2. **P2P Mesh Routing** reduces latency 50-90% vs centralized client-server
3. **Hierarchical Aggregation** achieves 95%+ bandwidth reduction at scale (24+ nodes)

### Why This Matters

Traditional event-driven IoT architectures transmit complete messages on every update cycle:
- High bandwidth consumption (n² message replication)
- Periodic polling introduces latency (0-5 seconds)
- Centralized collection creates bottlenecks

CAP Protocol uses:
- **Document model** with differential sync (only changes propagate)
- **P2P mesh** for event-driven updates (sub-100ms latency)
- **Hierarchical aggregation** for scalability (O(n log n) vs O(n²))

---

## Experimental Framework

### Test Infrastructure

**Comprehensive Test Harness:**
- `scripts/run-comprehensive-suite.sh` - Executes all test configurations
- Automated deployment via ContainerLab
- Standardized metrics collection
- Docker network statistics capture

**Analysis Pipeline:**
- `scripts/analyze-comprehensive-results.py` - Comparative analysis
- Statistical validation of claims
- Executive summary generation
- Markdown reports with visualizations

### Metrics Collected

#### 1. Application-Level Metrics (from logs)
```json
{
  "event_type": "MessageSent|MessageReceived|DocumentInserted|DocumentReceived",
  "node_id": "...",
  "message_size_bytes": 1024,
  "latency_us": 15000,
  "timestamp_us": 1762561629283549
}
```

#### 2. Docker Network Statistics
```json
{
  "node_name": {
    "net_input_bytes": 1234567,
    "net_output_bytes": 7654321,
    "net_total_bytes": 8888888,
    "avg_cpu_percent": 12.5,
    "avg_mem_bytes": 134217728
  }
}
```

#### 3. Summary Statistics
- Total bandwidth (bytes transmitted)
- Document replication factor
- Latency distribution (p50, p90, p99)
- Per-node breakdown

---

## Test Configurations

### Architectures

**1. Traditional IoT (Baseline)**
- Event-driven periodic messages
- Full state transmitted every 5 seconds
- No CRDT, no differential sync
- Binary: `traditional_baseline`

**2. CAP Full Mesh**
- CRDT document model (Ditto/Automerge)
- Differential sync (only changes propagate)
- P2P mesh topology (n² replication)
- Binary: `cap_sim_node` with `CAP_FILTER_ENABLED=false`

**3. CAP Hierarchical**
- CRDT document model
- Differential sync
- Hierarchical aggregation topology
- Squad leaders aggregate → Platoon leader aggregates
- Binary: `cap_sim_node` with `MODE=hierarchical`

### Test Matrix

```
Traditional IoT:
  ├── 2-node  × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests
  ├── 12-node × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests
  └── 24-node × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests

CAP Full Mesh:
  ├── 12-node × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests
  └── 24-node × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests

CAP Hierarchical:
  └── 24-node × [1Gbps, 100Mbps, 1Mbps, 256Kbps]   = 4 tests

Total: 24 test configurations
```

---

## Quick Start

### Prerequisites

- Docker with ContainerLab installed
- Ditto credentials in `.env` file (repository root)
- Python 3.8+ (for analysis scripts)
- Linux with tc/netem support (for bandwidth constraints)

### Run Complete Test Suite

```bash
cd labs/e12-comprehensive-empirical-validation/scripts
./run-comprehensive-suite.sh
```

**Duration:** ~3-4 hours (24 tests, automated)

### Analyze Results

```bash
# After test suite completes
python3 scripts/analyze-comprehensive-results.py e12-comprehensive-results-YYYYMMDD-HHMMSS/

# View reports
cd e12-comprehensive-results-YYYYMMDD-HHMMSS/
cat EXECUTIVE-SUMMARY.md
cat COMPARATIVE-ANALYSIS.md
cat RESULTS-SUMMARY.md
```

---

## How It Works

### Test Execution Flow

1. **Deploy Topology**
   - ContainerLab deploys nodes as Docker containers
   - Network topology configured per test

2. **Warm-up Period (30s)**
   - Allow nodes to initialize
   - Establish connections

3. **Apply Bandwidth Constraint**
   - Use `containerlab tools netem` to set rate limits
   - Constraint stabilization (10s)

4. **Start Metrics Collection**
   - Launch Docker stats collector (background process)
   - Collect stats every 5 seconds

5. **Measurement Period (60-90s)**
   - Nodes execute normal operations
   - Metrics logged to stdout (captured by Docker)

6. **Collect & Aggregate**
   - Stop stats collection
   - Extract logs from all containers
   - Parse JSONL metrics
   - Calculate summary statistics
   - Aggregate Docker network stats

7. **Cleanup**
   - Destroy topology
   - Prepare for next test

### Metrics Aggregation

**Application Metrics:**
```python
# Extract from logs
grep "METRICS:" *.log | sed 's/.*METRICS: //' > all-metrics.jsonl

# Calculate statistics
- Count: MessageSent, DocumentReceived
- Bandwidth: sum(message_size_bytes)
- Latency: percentiles from latency_us
- Replication: DocumentReceived / DocumentInserted
```

**Docker Stats:**
```python
# Collected every 5s during test
docker stats --no-stream --format json > stats-${timestamp}.json

# Aggregated after test
- Net I/O: max(net_input_bytes), max(net_output_bytes)
- CPU: avg(cpu_percent)
- Memory: avg(mem_usage_bytes)
```

### Comparative Analysis

```python
# For each bandwidth constraint:
traditional = load_results("traditional-24node-1gbps")
cap_hierarchical = load_results("cap-hierarchical-24node-1gbps")

# Calculate reduction
bandwidth_reduction = (1 - cap_hierarchical.bytes / traditional.bytes) * 100
latency_improvement = (1 - cap_hierarchical.p50 / traditional.p50) * 100

# Report
print(f"Bandwidth reduction: {bandwidth_reduction:.1f}%")
print(f"Latency improvement: {latency_improvement:.1f}%")
```

---

## Expected Results

### Bandwidth Usage (24 nodes)

| Architecture | @ 1Gbps | @ 100Mbps | @ 1Mbps | @ 256Kbps |
|-------------|---------|-----------|---------|-----------|
| Traditional IoT | ~2-5 MB/s | ~2-5 MB/s | Limited | Saturated |
| CAP Full Mesh | ~100-500 KB/s | ~100-500 KB/s | ~50-200 KB/s | ~10-50 KB/s |
| CAP Hierarchical | ~10-50 KB/s | ~10-50 KB/s | ~5-20 KB/s | ~1-5 KB/s |

**Reduction:** 95-98% (Traditional → CAP Hierarchical)

### Latency Distribution

| Architecture | p50 | p90 | p99 |
|-------------|-----|-----|-----|
| Traditional IoT (5s cycle) | ~2500ms | ~4500ms | ~5000ms |
| CAP Full Mesh | <100ms | <200ms | <500ms |
| CAP Hierarchical | <150ms | <300ms | <600ms |

**Improvement:** 20-50× faster (Traditional → CAP)

### Document Replication (24 nodes, 1 update cycle)

| Architecture | Ops | Explanation |
|-------------|-----|-------------|
| Traditional IoT | 576 | 24 nodes × 24 messages = 576 receptions |
| CAP Full Mesh | 576 | 24 documents × 24 peers = 576 receptions |
| CAP Hierarchical | ~120 | 20 members + 3 squads → aggregated |

**Reduction:** 79% fewer replication operations

---

## File Structure

```
e12-comprehensive-empirical-validation/
├── README.md                           # This file
├── EXPERIMENTAL-DESIGN.md              # Detailed design document
├── scripts/
│   ├── run-comprehensive-suite.sh      # Main test harness
│   └── analyze-comprehensive-results.py # Analysis script
└── e12-comprehensive-results-YYYYMMDD-HHMMSS/
    ├── EXECUTIVE-SUMMARY.md            # High-level results
    ├── COMPARATIVE-ANALYSIS.md         # Detailed comparisons
    ├── RESULTS-SUMMARY.md              # All test results
    ├── aggregate-statistics.json       # JSON metrics
    ├── traditional-2node-1gbps/
    │   ├── test-config.txt
    │   ├── test-summary.json
    │   ├── test-summary.txt
    │   ├── docker-stats-summary.json
    │   ├── docker-stats-summary.txt
    │   ├── all-metrics.jsonl
    │   ├── *.log                       # Per-node logs
    │   └── docker-stats/               # Raw stats files
    ├── traditional-2node-100mbps/
    ├── ... (22 more test directories)
    └── cap-hierarchical-24node-256kbps/
```

---

## Success Criteria

### Hypothesis Validation

✅ **H1: CRDT Differential Sync reduces bandwidth 60-95% vs Traditional IoT**
- Measure: Total bytes transmitted (Docker stats + app metrics)
- Target: >60% reduction observed
- Method: Compare Traditional vs CAP Full at same scale

✅ **H2: P2P Mesh reduces latency 50-90% vs centralized polling**
- Measure: p50 latency
- Target: <250ms CAP vs >2500ms Traditional
- Method: Compare event-driven (CAP) vs periodic (Traditional)

✅ **H3: Hierarchical Aggregation achieves 95%+ bandwidth reduction at scale**
- Measure: Document replication operations
- Target: >75% reduction in operations (576 → <150)
- Method: Compare CAP Full (n²) vs CAP Hierarchical (O(n log n))

✅ **H4: Performance maintained under bandwidth constraints**
- Measure: Latency at 256Kbps
- Target: CAP functional, Traditional degraded
- Method: Compare all architectures at tactical edge bandwidth

---

## Troubleshooting

### Issue: Docker Stats Collection Fails

**Symptom:** Empty docker-stats directory

**Solution:**
```bash
# Check Docker daemon is running
docker ps

# Ensure stats command works
docker stats --no-stream
```

### Issue: Test Hangs

**Symptom:** Test doesn't progress beyond warm-up

**Solution:**
```bash
# Check container logs
docker logs clab-<topology>-<node>

# Verify Ditto credentials
cat ../../.env | grep DITTO_
```

### Issue: High Disk Usage

**Symptom:** Disk space warning during tests

**Solution:**
```bash
# Clean old results
rm -rf e12-comprehensive-results-*/

# Clean Docker
docker system prune -af
```

---

## Next Steps

### After Running Tests

1. **Analyze Results**
   ```bash
   python3 scripts/analyze-comprehensive-results.py <results-dir>
   ```

2. **Review Reports**
   - Check EXECUTIVE-SUMMARY.md for validation status
   - Review COMPARATIVE-ANALYSIS.md for detailed metrics
   - Examine per-test directories for anomalies

3. **Validate Claims**
   - Verify bandwidth reductions meet targets (>60%, >95%)
   - Check latency improvements (>50%)
   - Confirm replication efficiency

4. **Document Findings**
   - Update ADRs with empirical evidence
   - Share results with team
   - Prepare publication materials

### Future Enhancements

- [ ] Add visualization generation (charts, graphs)
- [ ] Long-duration stability tests (30+ minutes)
- [ ] Failure injection tests (network partitions, node failures)
- [ ] Multi-platoon testing (48+ nodes, company-level)
- [ ] Real-world tactical radio testing

---

## References

- **E11:** Mode 4 P2P Refactoring & Comprehensive Bandwidth Testing
- **E8:** Three-Way Baseline Comparison Framework
- **ADR-012:** Bidirectional Command Flow Implementation Status
- **EXPERIMENTAL-DESIGN.md:** Detailed methodology

---

**Infrastructure Status:** ✅ Complete and Ready
**Next Action:** Execute `./scripts/run-comprehensive-suite.sh`
