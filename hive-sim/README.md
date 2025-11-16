# HIVE Protocol Network Simulation with ContainerLab

This directory contains ContainerLab-based network simulation infrastructure for testing HIVE Protocol under realistic network conditions.

## Overview

ContainerLab provides declarative Lab-as-Code for creating network topologies with:
- Docker containers running real Ditto-based CAP nodes
- Network constraints (bandwidth, latency, jitter, packet loss)
- Scalable topologies (tested up to 112 nodes)
- Easy scenario reproduction and version control

## Prerequisites

- **Linux**: Ubuntu 20.04+ or similar (ContainerLab requires Linux)
- **Docker**: Version 20.10+ with permissions to run containers
- **ContainerLab**: v0.71.1+ (install: `sudo bash -c "$(curl -sL https://get.containerlab.dev)"`)
- **tc/netem**: Traffic control tools (usually included in `iproute2` package)
- **Rust 1.86+**: For building hive-sim-node image

## Quick Start

### 1. Build the Docker Image

```bash
# From repository root
docker build -f hive-sim/Dockerfile -t hive-sim-node:latest .
```

This creates a Docker image with:
- Rust 1.86 environment
- Ditto SDK (v4.11.5)
- HIVE Protocol simulation node binary
- Network debugging tools (ping, tcpdump, etc.)

### 2. Set Up Environment Variables

```bash
# Copy example file
cp hive-sim/.env.example hive-sim/.env

# Edit with your Ditto credentials from https://portal.ditto.live/
# The root .env file is used by ContainerLab via --env-file
```

### 3. Deploy a Topology

```bash
# Deploy 2-node POC (unconstrained)
cd hive-sim
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
hive-sim/
├── Dockerfile              # hive-sim-node image definition
├── entrypoint.sh           # Container startup script
├── README.md               # This file
├── .env.example            # Example Ditto credentials
│
├── topologies/             # ContainerLab topology definitions
│   ├── poc-2node.yaml      # 2-node baseline (no constraints)
│   ├── poc-2node-constrained.yaml  # 2-node with tactical radio constraints
│   └── squad-formation.yaml        # 12-node squad (future)
│
└── scenarios/              # Shadow YAML scenarios (deprecated)
    └── *.yaml              # Old Shadow configs (kept for reference)
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

## Environment Variables

Nodes are configured via environment variables in topology YAML:

- `NODE_ID`: Unique identifier for this node (required)
- `MODE`: "writer" or "reader" (required for POC)
- `TCP_LISTEN`: Port to listen on for TCP connections
- `TCP_CONNECT`: Address:port to connect to (e.g., "node1:12345")
- `DITTO_APP_ID`: Ditto application ID (from portal)
- `DITTO_OFFLINE_TOKEN`: Ditto offline license token
- `DITTO_SHARED_KEY`: Ditto shared encryption key

Ditto credentials are loaded from `.env` file via `--env-file` flag.

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
docker build --no-cache -f hive-sim/Dockerfile -t hive-sim-node:latest .
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
