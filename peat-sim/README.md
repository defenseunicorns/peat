# Peat Protocol Network Simulation with ContainerLab

This directory contains ContainerLab-based network simulation infrastructure for testing Peat Protocol under realistic network conditions.

## Overview

ContainerLab provides declarative Lab-as-Code for creating network topologies with:
- Docker containers running CAP nodes with pluggable sync backends (Ditto or Automerge+Iroh)
- Network constraints (bandwidth, latency, jitter, packet loss)
- Scalable topologies (tested up to 112 nodes)
- Easy scenario reproduction and version control

### Supported Sync Backends

peat-sim supports two CRDT sync backends via the `--backend` flag:

| Backend | Description | Use Case | License |
|---------|-------------|----------|---------|
| **Ditto** (default) | Production-grade CRDT sync with multi-transport support | Production deployments, baseline benchmarks | Proprietary (requires license) |
| **Automerge+Iroh** | Open-source CRDT (Automerge 0.7) + QUIC transport (Iroh 0.95) | Open-source deployments, research, DoD/NATO environments | MIT/Apache 2.0 |

See [Backend Selection](#backend-selection) for usage details.

## Prerequisites

- **Linux**: Ubuntu 20.04+ or similar (ContainerLab requires Linux)
- **Docker**: Version 20.10+ with permissions to run containers
- **ContainerLab**: v0.71.1+ (install: `sudo bash -c "$(curl -sL https://get.containerlab.dev)"`)
- **tc/netem**: Traffic control tools (usually included in `iproute2` package)
- **Rust 1.86+**: For building peat-sim-node image

## Quick Start

### 1. Build the Docker Image

```bash
# From repository root
docker build -f peat-sim/Dockerfile -t peat-sim-node:latest .
```

This creates a Docker image with:
- Rust 1.86 environment
- Ditto SDK (v4.11.5)
- Peat Protocol simulation node binary
- Network debugging tools (ping, tcpdump, etc.)

### 2. Set Up Environment Variables

```bash
# Copy example file
cp peat-sim/.env.example peat-sim/.env

# Edit with your Ditto credentials from https://portal.ditto.live/
# The root .env file is used by ContainerLab via --env-file
```

### 3. Deploy a Topology

```bash
# Deploy 2-node POC (unconstrained)
cd peat-sim
sudo containerlab deploy -t topologies/poc-2node.yaml --env-file ../.env

# View running containers
sudo containerlab inspect -t topologies/poc-2node.yaml

# Check logs
docker logs clab-cap-poc-2node-node1
docker logs clab-cap-poc-2node-node2
```

### 4. Test with Network Constraints

```bash
# Deploy constrained topology (56 Kbps, 50ms latency, 1% loss)
sudo containerlab deploy -t topologies/poc-2node-constrained.yaml --env-file ../.env

# View applied impairments
sudo containerlab tools netem show -t topologies/poc-2node-constrained.yaml

# Watch sync behavior
docker logs -f clab-cap-poc-2node-constrained-node2
```

### 5. Clean Up

```bash
# Destroy topology (removes containers and networks)
sudo containerlab destroy -t topologies/poc-2node.yaml
```

## Directory Structure

```
peat-sim/
├── Dockerfile              # peat-sim-node image definition
├── entrypoint.sh           # Container startup script
├── README.md               # This file
├── .env.example            # Example Ditto credentials
│
├── src/                    # Rust source code
│   ├── main.rs             # Main simulation node binary
│   └── utils/              # Utility modules
│       ├── mod.rs          # Module exports
│       └── time.rs         # Time utilities (now_micros, extract_timestamp_us)
│
├── topologies/             # ContainerLab topology definitions
│   ├── poc-2node.yaml      # 2-node baseline (no constraints)
│   ├── poc-2node-constrained.yaml  # 2-node with tactical radio constraints
│   └── squad-formation.yaml        # 12-node squad (future)
│
└── scenarios/              # Shadow YAML scenarios (deprecated)
    └── *.yaml              # Old Shadow configs (kept for reference)
```

### Utils Module

The `src/utils/` module provides reusable utility functions:

#### `utils::time`

Time-related utilities for timestamp handling:

- **`now_micros() -> u128`**: Get current Unix timestamp in microseconds
- **`extract_timestamp_us(val: &serde_json::Value) -> u128`**: Extract timestamps from various formats:
  - Direct numeric values (u64, i64, f64)
  - Protobuf-style `{seconds, nanos}` objects

```rust
use utils::time::{now_micros, extract_timestamp_us};

// Get current time
let timestamp = now_micros();

// Extract from protobuf-style timestamp
let json = serde_json::json!({"seconds": 1234567890, "nanos": 123456789});
let timestamp_us = extract_timestamp_us(&json); // 1234567890123456
```

## Available Topologies

### poc-2node.yaml
- **Purpose**: Baseline connectivity test
- **Nodes**: 2 (writer + reader)
- **Constraints**: None
- **Use**: Validate basic Ditto sync in containers

### poc-2node-constrained.yaml
- **Purpose**: Network constraint validation
- **Nodes**: 2 (writer + reader)
- **Constraints**: 56 Kbps bandwidth, 50ms latency, 10ms jitter, 1% loss
- **Use**: Verify network impairments affect Ditto traffic

### squad-formation.yaml (TODO)
- **Purpose**: Small unit tactical scenario
- **Nodes**: 12 (9 soldiers + 1 UGV + 2 UAVs)
- **Constraints**: Variable by link type
- **Use**: Test squad-level capability composition

## Network Constraints

ContainerLab supports these network impairments via Linux tc/netem:

```yaml
links:
  - endpoints: ["node1:eth1", "node2:eth1"]
    impairments:
      rate: 56kbps        # Bandwidth limit
      delay: 50ms         # Latency (one-way)
      jitter: 10ms        # Latency variance
      loss: 1%            # Packet loss percentage
      corruption: 0.1%    # Bit error rate (optional)
```

All impairments are applied bidirectionally by default.

## Useful Commands

### ContainerLab Management

```bash
# Deploy topology
sudo containerlab deploy -t topologies/<name>.yaml --env-file ../.env

# Inspect running lab
sudo containerlab inspect -t topologies/<name>.yaml

# Destroy topology
sudo containerlab destroy -t topologies/<name>.yaml

# Destroy all labs
sudo containerlab destroy --all

# Graph topology (generates diagram)
sudo containerlab graph -t topologies/<name>.yaml
```

### Network Impairment Tools

```bash
# Show current impairments
sudo containerlab tools netem show -t topologies/<name>.yaml

# Set impairments manually (if not in YAML)
sudo containerlab tools netem set \
  -n clab-<lab-name>-<node-name> \
  -i eth1 \
  --delay 100ms \
  --loss 5

# Reset impairments
sudo containerlab tools netem reset -t topologies/<name>.yaml
```

### Container Debugging

```bash
# Enter container shell
docker exec -it clab-<lab-name>-<node-name> bash

# View logs
docker logs clab-<lab-name>-<node-name>
docker logs -f clab-<lab-name>-<node-name>  # Follow

# Check network interfaces
docker exec clab-<lab-name>-<node-name> ip addr

# Ping between nodes
docker exec clab-<lab-name>-node1 ping node2

# Check tc qdisc rules
docker exec clab-<lab-name>-node1 tc qdisc show

# Capture packets
docker exec clab-<lab-name>-node1 tcpdump -i eth1 -w /tmp/capture.pcap
```

## Backend Selection

peat-sim supports pluggable sync backends to enable both production deployments (Ditto) and open-source alternatives (Automerge+Iroh).

### Using Ditto Backend (Default)

No special configuration needed - Ditto is the default backend:

```bash
# Build with Ditto support
docker build -f peat-sim/Dockerfile -t peat-sim-node:latest .

# Run with Ditto (explicit)
peat-sim --backend ditto --node-id node1

# Run with Ditto (implicit default)
peat-sim --node-id node1
```

**Requirements:**
- Ditto credentials (`DITTO_APP_ID`, `DITTO_OFFLINE_TOKEN`, `DITTO_SHARED_KEY`)
- See `.env.example` for setup

### Using Automerge+Iroh Backend

Build and run with the `automerge-backend` feature:

```bash
# Build with Automerge+Iroh support
docker build -f peat-sim/Dockerfile \
  --build-arg FEATURES="automerge-backend" \
  -t peat-sim-node:automerge .

# Run with Automerge+Iroh
peat-sim --backend automerge --node-id node1
```

**Requirements:**
- No Ditto credentials needed
- Nodes discover each other via static configuration or mDNS
- QUIC transport via Iroh (no relay required for LAN)

### Backend Comparison

| Feature | Ditto | Automerge+Iroh |
|---------|-------|----------------|
| **CRDT Library** | Proprietary CBOR-based | Automerge 0.7 (columnar) |
| **Transport** | TCP, WebSocket, BLE, P2P | QUIC (Iroh 0.95) |
| **Storage** | SQLite | RocksDB |
| **Credentials** | Required (app_id, token, key) | Not required |
| **License** | Proprietary | MIT/Apache 2.0 |
| **Multi-node mesh** | ✅ Automatic | ✅ Manual (static config) |
| **Network partition handling** | ✅ Built-in | ✅ Heartbeat-based (Phase 7.1) |
| **E2E Test Coverage** | ✅ 3-node mesh | ✅ 3-node mesh |

### Choosing a Backend

**Use Ditto when:**
- Deploying to production environments with Ditto licenses
- Need multi-transport support (BLE, WebSocket, etc.)
- Want automatic peer discovery and mesh formation
- Establishing baseline performance benchmarks

**Use Automerge+Iroh when:**
- Deploying to DoD/NATO environments requiring open-source
- Conducting research on CRDT performance
- Testing without Ditto credentials
- Evaluating QUIC transport characteristics
- Contributing to open-source development

## Environment Variables

Nodes are configured via environment variables in topology YAML:

**Core Configuration:**
- `NODE_ID`: Unique identifier for this node (required)
- `MODE`: "writer" or "reader" (required for POC)
- `BACKEND`: Backend type - "ditto" (default) or "automerge"

**Networking:**
- `TCP_LISTEN`: Port to listen on for TCP connections
- `TCP_CONNECT`: Address:port to connect to (e.g., "node1:12345")

**Ditto Backend (required only when using `--backend ditto`):**
- `DITTO_APP_ID`: Ditto application ID (from portal)
- `DITTO_OFFLINE_TOKEN`: Ditto offline license token
- `DITTO_SHARED_KEY`: Ditto shared encryption key

Ditto credentials are loaded from `.env` file via `--env-file` flag.

**Automerge+Iroh Backend (when using `--backend automerge`):**
- No credentials required
- Peer discovery via static configuration or mDNS (Phase 7.3)

**Circuit Breaker Configuration (Automerge backend only):**

The circuit breaker prevents cascading failures when peers become unreachable. Configure via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `CIRCUIT_FAILURE_THRESHOLD` | 5 | Number of consecutive failures to trigger circuit open |
| `CIRCUIT_FAILURE_WINDOW_SECS` | 5 | Time window for counting failures (seconds) |
| `CIRCUIT_OPEN_TIMEOUT_SECS` | 5 | How long circuit stays open before trying half-open (seconds) |
| `CIRCUIT_SUCCESS_THRESHOLD` | 2 | Successes needed to close circuit from half-open state |

**Recommended settings by environment:**

| Environment | Window | Timeout | Use Case |
|-------------|--------|---------|----------|
| Lab (single machine) | 2s | 2s | Fast iteration, predictable network |
| Staging | 5s | 5s | Balanced (default) |
| Production | 10-30s | 10-30s | Variable network, external dependencies |

The topology generator (`generate-lab4-hierarchical-topology.py`) sets aggressive lab defaults (2s windows) automatically.

## Hierarchical Command Dissemination

The simulator supports bidirectional hierarchical command flow with optional acknowledgment collection:

### Command Reception

All nodes in hierarchical mode automatically:
- Monitor for incoming commands via Ditto observers (`CommandStorage.observe_commands()`)
- Process commands targeting them (by node ID, squad ID, or platoon ID)
- Send automatic acknowledgments (`CommandAcknowledgment` with `AckCompleted` status)

Metrics events emitted:
- `CommandReceived`: When a command is received and processed
- `CommandAcknowledged`: When acknowledgment is sent back to command originator

### Acknowledgment Collection (Optional)

Squad leaders can optionally track acknowledgments from subordinates:

```bash
# Enable acknowledgment collection for a squad leader
peat-sim-node --enable-ack-collection \
  --node-id squad-leader \
  --squad-id alpha \
  --mode hierarchical
```

When enabled:
- Monitors for acknowledgments via Ditto observers (`CommandStorage.observe_acknowledgments()`)
- Tracks acknowledgment count vs. expected count
- Emits `AcknowledgmentReceived` metrics events

**Use Cases**:
- **Fire-and-forget**: Disable ack collection for simple command broadcast
- **Mission-critical tracking**: Enable ack collection to verify all squad members acknowledged

### E2E Test Coverage

The bidirectional command flow is validated with E2E tests:
- `/Users/kit/Code/cap/peat-protocol/tests/bidirectional_flow_e2e.rs`

Tests verify:
1. Commands propagate from leader → members (downward flow)
2. Acknowledgments propagate from members → leader (upward flow)
3. Full-duplex bidirectional flow works simultaneously
4. Concurrent commands are handled correctly

Run tests with:
```bash
cargo test --test bidirectional_flow_e2e
```

## Troubleshooting

### "Permission denied" errors
ContainerLab requires root/sudo for network namespace operations:
```bash
sudo containerlab deploy -t ...
```

### "tc: command not found" inside container
The Dockerfile includes `iproute2`, but verify with:
```bash
docker exec clab-<name> which tc
```

### Containers can't resolve each other
ContainerLab creates a management network with DNS. Verify:
```bash
docker exec clab-<name>-node1 ping node2
```

### Impairments not applied
Check that tc module is loaded on host:
```bash
lsmod | grep sch_netem
tc qdisc help | grep netem
```

### Docker build fails with Ditto FFI errors
Ensure Rust 1.86+ is used:
```bash
rustup override set 1.86
docker build --no-cache -f peat-sim/Dockerfile -t peat-sim-node:latest .
```

## Next Steps

After validating the 2-node POC:

1. **Squad Formation (12 nodes)**: Create squad-formation.yaml with hierarchical structure
2. **Metrics Collection**: Add telemetry for sync times, bandwidth usage
3. **Partition Scenarios**: Test network splits and healing
4. **Company Scale (112 nodes)**: Full Army company topology

## References

- **ContainerLab Docs**: https://containerlab.dev/
- **ContainerLab Impairments**: https://containerlab.dev/manual/impairments/
- **Linux tc**: https://man7.org/linux/man-pages/man8/tc.8.html
- **Linux netem**: https://man7.org/linux/man-pages/man8/tc-netem.8.html
- **E8 Implementation**: `../docs/E8-IMPLEMENTATION-PLAN.md`
- **Simulator Comparison**: `../docs/E8_NETWORK_SIMULATOR_COMPARISON.md`
