# E8 Performance Testing - Quick Start Guide

**Last Updated:** 2025-11-07

---

## Prerequisites

Before running E8 performance tests, ensure you have:

### Required Software
- ✅ **Docker** - Container runtime
- ✅ **ContainerLab** - Network topology orchestration
- ✅ **Python 3** with PyYAML - `pip3 install pyyaml`
- ✅ **jq** - JSON processor (for metrics analysis)

### Required Setup
- ✅ **Docker Image Built** - `cap-sim-node:latest` must exist
- ✅ **Environment Variables** - `.env` file in `cap/` directory with:
  ```bash
  DITTO_APP_ID=your_app_id
  DITTO_OFFLINE_TOKEN=your_token
  DITTO_SHARED_KEY=your_key
  ```

### Check Prerequisites
```bash
# From cap/ directory
make sim-build              # Build Docker image (if needed)
docker image ls cap-sim-node  # Verify image exists
containerlab version        # Verify ContainerLab installed
python3 -c "import yaml"    # Verify PyYAML installed
jq --version                # Verify jq installed
cat .env                    # Verify Ditto credentials set
```

---

## Running Tests

### Option 1: Full Performance Test Suite (Recommended)
**32 test scenarios across 3 architectures × 4 bandwidth levels**
**Estimated time: ~22 minutes**

```bash
# From cap/ directory
make e8-performance-tests
```

**What it tests:**
- Traditional IoT Baseline (NO CRDT) - 8 tests
- CAP Full Replication (CRDT, no filtering) - 12 tests
- CAP Differential (CRDT + capability filtering) - 12 tests

**Bandwidth levels tested:** 100Mbps, 10Mbps, 1Mbps, 256Kbps

**Results location:**
```bash
cap-sim/e8-performance-results-latest/
```

---

### Option 2: Quick Baseline Comparison
**9 test scenarios - faster validation**
**Estimated time: ~5 minutes**

```bash
# From cap/ directory
make e8-baseline-comparison
```

**What it tests:**
- Traditional IoT: 2-node, 12-node client-server, 12-node hub-spoke
- CAP Full: Same topologies
- CAP Differential: Same topologies

**Use case:** Quick validation after code changes

---

### Option 3: Individual Test Scripts
For targeted testing:

```bash
cd cap-sim

# Traditional IoT bandwidth tests only
./test-bandwidth-traditional.sh

# CAP bandwidth tests (full replication)
./test-bandwidth-constraints.sh

# CAP differential tests (with filtering)
CAP_FILTER_ENABLED=true ./test-bandwidth-constraints.sh
```

---

## Viewing Results

### Main Summary Report
```bash
cat cap-sim/e8-performance-results-latest/E8-THREE-WAY-COMPARISON.md
```

### Architecture-Specific Summaries
```bash
# Traditional IoT results
cat cap-sim/e8-performance-results-latest/1-traditional-iot-baseline/COMPREHENSIVE_SUMMARY.md

# CAP Full Replication results
cat cap-sim/e8-performance-results-latest/2-cap-full-replication/COMPREHENSIVE_SUMMARY.md

# CAP Differential results
cat cap-sim/e8-performance-results-latest/3-cap-differential/COMPREHENSIVE_SUMMARY.md
```

### Detailed Metrics
```bash
# View metrics for specific test
cat cap-sim/e8-performance-results-latest/2-cap-full-replication/100mbps/mode1-client-server_summary.txt

# View JSON metrics
jq . cap-sim/e8-performance-results-latest/2-cap-full-replication/100mbps/mode1-client-server_metrics.json
```

### Per-Node Logs
```bash
# View logs for specific node
cat cap-sim/e8-performance-results-latest/2-cap-full-replication/100mbps/mode1-client-server_soldier-1.log
```

---

## Understanding Results

### Key Metrics Explained

#### Convergence Time
**Definition:** Time from first node insert to all nodes receiving the data
**Lower is better**

```
Convergence Time: 26135.26ms
```

#### Latency Statistics
**Mean:** Average message propagation time
**P90:** 90th percentile (90% of messages faster than this)
**P99:** 99th percentile (worst-case for most messages)

```
Latency Mean: 4529.28ms
Latency P90: 16014.45ms
Latency P99: 16128.17ms
```

#### Measured Bandwidth
Actual bandwidth constraint achieved (may differ slightly from configured)

```
Configured Bandwidth: 100mbps
Measured Bandwidth: 96.54 Mbps
```

### Expected Results

Based on architectural design:

| Architecture | Bandwidth Usage | vs Traditional | Key Benefit |
|--------------|----------------|----------------|-------------|
| Traditional IoT | ~170 msgs/60s | Baseline (100%) | Simple, predictable |
| CAP Full | ~60-70% of baseline | **-30-40%** | CRDT delta-state sync |
| CAP Differential | ~30-40% of baseline | **-60-70%** | CRDT + filtering |

---

## Troubleshooting

### Test Hangs During Cleanup
**Symptom:** Test stuck on "Cleaning up..." or "Clearing bandwidth constraints..."
**Solution:** Fixed in latest version (removes netem before containerlab destroy)

If still hanging:
```bash
# Force cleanup all containers
docker ps -a --filter "name=clab-" -q | xargs -r docker rm -f

# Cleanup networks
docker network prune -f
```

### "Docker image not found" Error
```bash
cd cap
make sim-build
```

### "No such file or directory: .env"
```bash
cd cap
cat > .env <<EOF
DITTO_APP_ID=your_app_id_here
DITTO_OFFLINE_TOKEN=your_token_here
DITTO_SHARED_KEY=your_key_here
EOF
```

Get credentials from: https://portal.ditto.live

### ContainerLab Permission Denied
Add your user to docker group:
```bash
sudo usermod -aG docker $USER
newgrp docker
```

### Python Import Error: "No module named 'yaml'"
```bash
pip3 install pyyaml
```

### Disk Space Issues
Clean up old results:
```bash
cd cap-sim
# Archive results older than 3 most recent
find . -maxdepth 1 -type d -name "e8-performance-results-*" -o \
                          -name "test-results-*" -o \
                          -name "baseline-comparison-*" | \
  sort -r | tail -n +4 | xargs -r rm -rf
```

---

## Test Matrix Overview

### Full Test Suite (32 tests)

```
Traditional IoT (8 tests):
  ├── 100Mbps: mode1-client-server, mode2-hub-spoke
  ├── 10Mbps:  mode1-client-server, mode2-hub-spoke
  ├── 1Mbps:   mode1-client-server, mode2-hub-spoke
  └── 256Kbps: mode1-client-server, mode2-hub-spoke

CAP Full Replication (12 tests):
  ├── 100Mbps: mode1, mode2, mode3-dynamic-mesh
  ├── 10Mbps:  mode1, mode2, mode3-dynamic-mesh
  ├── 1Mbps:   mode1, mode2, mode3-dynamic-mesh
  └── 256Kbps: mode1, mode2, mode3-dynamic-mesh

CAP Differential (12 tests):
  └── [Same structure as CAP Full]
```

**Total:** 32 test scenarios

---

## Next Steps After Running Tests

1. **Review Results**
   ```bash
   cat cap-sim/e8-performance-results-latest/E8-THREE-WAY-COMPARISON.md
   ```

2. **Document Findings**
   - Update ADR-008 with actual measurements
   - Note any unexpected results
   - Identify architectural insights

3. **Generate Visualizations** (if needed)
   - Export metrics to CSV
   - Create comparison charts
   - Generate executive summary

4. **Archive Results**
   - Commit significant results to Git
   - Clean up old test runs
   - Update documentation

---

## Advanced Usage

### Custom Test Duration
Modify test scripts to adjust duration:
```bash
# Edit test-bandwidth-constraints.sh
# Change: sleep 60
# To:     sleep 120  # for longer test
```

### Filter Specific Bandwidth Levels
```bash
# Run only 256Kbps tests
cd cap-sim
# Edit run-e8-performance-suite.sh
# Change: for bw_name in "100mbps" "10mbps" "1mbps" "256kbps"; do
# To:     for bw_name in "256kbps"; do
```

### Enable Verbose Logging
```bash
# Set in test scripts
set -x  # Enable bash debugging
```

---

## Additional Resources

- **Detailed Optimization Plan:** `cap-sim/E8-OPTIMIZATION-PLAN.md`
- **Baseline Requirements:** `cap-sim/BASELINE-TESTING-REQUIREMENTS.md`
- **Traditional IoT Design:** `cap-sim/TRADITIONAL-BASELINE-DESIGN.md`
- **Main README:** `cap-sim/README.md`
- **ADR-008:** Architecture decision record for E8 network simulation

---

## Getting Help

If you encounter issues:

1. Check this Quick Start guide
2. Review `E8-OPTIMIZATION-PLAN.md` for known issues
3. Check GitHub issues
4. Review container logs: `docker logs <container-name>`

---

**Happy Testing! 🚀**
