# peat-sim-orchestrator

Process-per-node orchestrator for running large-scale peat-sim hierarchical simulations (Plan C) on a single VM without Docker or ContainerLab.

Each node runs as a separate OS process with its own TCP port on `127.0.0.1`. The orchestrator generates the full battalion hierarchy, wires up TCP connections between tiers, and manages the lifecycle of all processes.

## Prerequisites

- **peat-sim binary** built for your platform (`cargo build --release` in `peat-sim/`)
- **Python 3.8+** (stdlib only, no external dependencies)
- A machine with enough RAM and file descriptors (see resource table below)

Before running, ensure your OS limits are sufficient:

```bash
# Raise open file descriptor limit for the session
ulimit -n 65536

# Check current limit
ulimit -n
```

## Usage

### Quick start (small test - ~150 nodes)

```bash
python3 orchestrator.py \
  --binary ../target/release/peat-sim \
  --companies 1 \
  --platoons-per-company 2 \
  --squads-per-platoon 2 \
  --soldiers-per-squad 8 \
  --duration-secs 60 \
  --log-dir ./logs-test
```

### 1K nodes

```bash
python3 orchestrator.py \
  --binary ../target/release/peat-sim \
  --companies 6 \
  --platoons-per-company 4 \
  --squads-per-platoon 4 \
  --soldiers-per-squad 8 \
  --batch-size 200 \
  --batch-delay-secs 2 \
  --duration-secs 300 \
  --log-dir ./logs-1k
```

Node count: 1 + 6 + 24 + 96 + 768 = **895 nodes**

### 5K nodes

```bash
python3 orchestrator.py \
  --binary ../target/release/peat-sim \
  --companies 33 \
  --platoons-per-company 4 \
  --squads-per-platoon 4 \
  --soldiers-per-squad 8 \
  --batch-size 200 \
  --batch-delay-secs 3 \
  --duration-secs 300 \
  --log-dir ./logs-5k
```

Node count: 1 + 33 + 132 + 528 + 4224 = **4,918 nodes**

### 10K nodes

```bash
python3 orchestrator.py \
  --binary ../target/release/peat-sim \
  --companies 67 \
  --platoons-per-company 4 \
  --squads-per-platoon 4 \
  --soldiers-per-squad 8 \
  --batch-size 200 \
  --batch-delay-secs 3 \
  --duration-secs 300 \
  --log-dir ./logs-10k
```

Node count: 1 + 67 + 268 + 1072 + 8576 = **9,984 nodes**

### Hierarchy structure

```
battalion-hq (1)
  company-{c}-commander (N companies)
    company-{c}-platoon-{p}-leader (N * 4 platoons)
      company-{c}-platoon-{p}-squad-{s}-leader (N * 4 * 4 squads)
        company-{c}-platoon-{p}-squad-{s}-soldier-{i} (N * 4 * 4 * 8 soldiers)
```

Each tier connects upward via TCP:
- Soldiers connect to their squad leader
- Squad leaders connect to their platoon leader
- Platoon leaders connect to their company commander
- Company commanders connect to battalion HQ

## Collecting and analyzing metrics

After a run completes, analyze the log files:

```bash
python3 metrics.py --log-dir ./logs-10k --output results.csv
```

This will:
1. Scan all `.log` files for `METRICS:` prefixed JSONL lines
2. Compute per-tier latency distributions (p50, p95, p99)
3. Summarize aggregation events and bandwidth savings
4. Write a CSV and print a human-readable table

## CLI reference

### orchestrator.py

| Flag | Default | Description |
|------|---------|-------------|
| `--binary` | (required) | Path to peat-sim binary |
| `--companies` | 67 | Number of companies |
| `--platoons-per-company` | 4 | Platoons per company |
| `--squads-per-platoon` | 4 | Squads per platoon |
| `--soldiers-per-squad` | 8 | Soldiers per squad |
| `--base-port` | 10000 | Starting TCP port |
| `--batch-size` | 200 | Processes launched per batch |
| `--batch-delay-secs` | 3 | Delay between batches |
| `--backend` | automerge | Sync backend |
| `--log-dir` | ./logs | Directory for log files |
| `--duration-secs` | 300 | Simulation duration |

### metrics.py

| Flag | Default | Description |
|------|---------|-------------|
| `--log-dir` | (required) | Directory with .log files |
| `--output` | metrics.csv | Output CSV path |

## Resource requirements

| Scale | Nodes | Est. RAM | Est. Ports | Recommended VM |
|-------|-------|----------|------------|----------------|
| Test | ~150 | 2 GB | 150 | Any dev machine |
| 1K | ~900 | 12 GB | 900 | 16 GB, 8 vCPU |
| 5K | ~5,000 | 60 GB | 5,000 | 64 GB, 16 vCPU |
| 10K | ~10,000 | 120 GB | 10,000 | 128 GB, 32 vCPU |

RAM estimates assume ~12 MB RSS per peat-sim process (automerge backend). Actual usage depends on document size and sync volume. Monitor the orchestrator's periodic STATUS lines for real-time RSS tracking.

Disk usage for logs: roughly 1-5 MB per node for a 5-minute run, so budget 10-50 GB for a 10K run.
