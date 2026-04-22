# Peat Operator Guide

> **Version**: 1.0
> **Last Updated**: 2025-12-08
> **Audience**: System Administrators, DevOps Engineers, Mission Operators

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Quick Start](#2-quick-start)
3. [Installation](#3-installation)
4. [Configuration](#4-configuration)
5. [Deployment Patterns](#5-deployment-patterns)
6. [Network Configuration](#6-network-configuration)
7. [Security](#7-security)
8. [Monitoring & Observability](#8-monitoring--observability)
9. [TAK/ATAK Integration](#9-takatak-integration)
10. [Backup & Recovery](#10-backup--recovery)
11. [Troubleshooting](#11-troubleshooting)
12. [Reference](#12-reference)

---

## 1. Introduction

### 1.1 What is Peat?

Peat (Hierarchical Intelligence for Versatile Entities) is a protocol for scalable coordination of autonomous nodes (100+ nodes) using CRDTs with O(n log n) message complexity. It enables autonomous systems to coordinate without centralized control through:

- **Three-phase protocol**: Discovery → Cell Formation → Hierarchical Operations
- **CRDT-based state**: Eventual consistency via distributed data structures
- **Capability composition**: Nodes advertise and combine capabilities dynamically
- **Differential updates**: 95%+ bandwidth reduction through delta propagation

### 1.2 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Peat Network                            │
│                                                                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐                  │
│  │  Zone 1  │    │  Zone 2  │    │  Zone 3  │   Hierarchical   │
│  │ (Leader) │────│ (Leader) │────│ (Leader) │   Operations     │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘                  │
│       │               │               │                         │
│  ┌────┴────┐     ┌────┴────┐     ┌────┴────┐                   │
│  │  Cell   │     │  Cell   │     │  Cell   │   Cell Formation  │
│  │ ┌─┬─┬─┐ │     │ ┌─┬─┬─┐ │     │ ┌─┬─┬─┐ │                   │
│  │ │N│N│N│ │     │ │N│N│N│ │     │ │N│N│N│ │   Discovery       │
│  │ └─┴─┴─┘ │     │ └─┴─┴─┘ │     │ └─┴─┴─┘ │                   │
│  └─────────┘     └─────────┘     └─────────┘                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 Key Components

| Component | Description | Crate |
|-----------|-------------|-------|
| **Protocol Core** | Three-phase protocol, capabilities, composition | `peat-protocol` |
| **Mesh Layer** | P2P mesh topology and beacon management | `peat-mesh` |
| **Transport** | HTTP/REST API for external integration | `peat-transport` |
| **Discovery** | Peer discovery (mDNS, static, hybrid) | `peat-discovery` |
| **Persistence** | Storage abstraction for state | `peat-persistence` |
| **Simulator** | Network simulation and testing | `peat-sim` |
| **FFI** | Mobile bindings (Kotlin/Swift) | `peat-ffi` |
| **Inference** | Edge AI/ML inference pipeline | `peat-inference` |

### 1.4 Supported Platforms

Peat ships with a single Automerge + Iroh CRDT/transport stack on all supported
platforms.

| Platform | Status |
|----------|--------|
| Linux (x86_64) | Production |
| Linux (aarch64) | Production |
| macOS (x86_64, arm64) | Production |
| Android | Beta |
| Windows | Experimental |
| Jetson (CUDA) | Beta |

---

## 2. Quick Start

Get Peat running in 10 minutes.

### 2.1 Prerequisites

- **Rust**: 1.70 or later
- **Git**: For cloning the repository
- **8GB RAM** minimum (16GB recommended for simulation)
- **Network**: Internet access for dependencies

### 2.2 Installation

```bash
# Clone the repository
git clone https://github.com/defenseunicorns/peat.git
cd peat

# Build the project (first build takes 5-10 minutes)
cargo build --release
```

### 2.3 Run Your First Simulation

```bash
# Run the network simulator
cargo run --release --bin peat-sim
```

Expected output:
```
[INFO] Peat Simulator starting...
[INFO] Creating 10 nodes with random capabilities
[INFO] Phase 1: Discovery starting...
[INFO] Node UAV-001 discovered 9 peers
[INFO] Phase 2: Cell formation starting...
[INFO] Cell ALPHA formed with leader UAV-003
[INFO] Cell BETA formed with leader UGV-002
[INFO] Phase 3: Hierarchical operations active
[INFO] Zone coordinator elected: UAV-003
[INFO] Simulation complete. Press Ctrl+C to exit.
```

### 2.4 Verify Installation

```bash
# Run the test suite
make test-fast

# Check the build
cargo check --all-features
```

### 2.5 Next Steps

- [Configuration](#4-configuration) - Customize your deployment
- [Deployment Patterns](#5-deployment-patterns) - Production deployment options
- [TAK Integration](#9-takatak-integration) - Connect to ATAK

---

## 3. Installation

### 3.1 System Requirements

#### Minimum Requirements

| Resource | Requirement |
|----------|-------------|
| CPU | 2 cores |
| RAM | 4 GB |
| Disk | 2 GB free |
| Network | 100 Mbps |
| OS | Linux 4.4+, macOS 10.15+, Windows 10+ |

#### Recommended for Production

| Resource | Requirement |
|----------|-------------|
| CPU | 4+ cores |
| RAM | 16 GB |
| Disk | 20 GB SSD |
| Network | 1 Gbps |
| OS | Ubuntu 22.04 LTS, RHEL 8+ |

### 3.2 Install Rust

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Reload shell configuration
source $HOME/.cargo/env

# Verify installation
rustc --version   # Should show 1.70.0 or later
cargo --version
```

### 3.3 Build from Source

#### Standard Build

```bash
git clone https://github.com/defenseunicorns/peat.git
cd peat

# Build all crates (uses the default Automerge + Iroh backend)
cargo build --release

# Binaries located at:
# - target/release/peat-sim
```

#### Build for Android

```bash
# Install Android NDK and configure toolchains
# See .cargo/config.toml for toolchain configuration

# Build FFI library for Android
cd peat-ffi
cargo build --release --target aarch64-linux-android
```

### 3.4 Pre-built Binaries

Pre-built binaries are available for releases:

```bash
# Download latest release (example)
curl -LO https://github.com/defenseunicorns/peat/releases/latest/download/peat-sim-linux-x86_64.tar.gz
tar -xzf peat-sim-linux-x86_64.tar.gz
./peat-sim
```

### 3.5 Container Deployment

```dockerfile
# Example Dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/peat-sim /usr/local/bin/
ENTRYPOINT ["peat-sim"]
```

Build and run:
```bash
docker build -t peat-sim .
docker run --rm -it peat-sim
```

### 3.6 Development Tools (Optional)

```bash
# Fast test runner
cargo install cargo-nextest

# File watcher for auto-rebuild
cargo install cargo-watch

# Performance profiling
cargo install flamegraph

# Mold linker (faster linking on Linux)
# Ubuntu/Debian:
sudo apt install mold
```

---

## 4. Configuration

### 4.1 Environment Variables

#### Core Configuration

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `PEAT_APP_ID` | Peat application identifier (distinguishes logical deployments) | - | Yes |
| `PEAT_SECRET_KEY` | Shared secret for formation authentication | - | Yes |
| `PEAT_PERSISTENCE_DIR` | Directory for persisted CRDT state | `./peat_data` | No |

#### General Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level configuration | `info` |
| `PEAT_NODE_ID` | Unique node identifier | Auto-generated UUID |
| `PEAT_CELL_SIZE` | Target cell size | `5` |
| `PEAT_DISCOVERY_TIMEOUT` | Discovery phase timeout (seconds) | `30` |
| `PEAT_LEADER_ELECTION_TIMEOUT` | Leader election timeout (seconds) | `10` |

#### Network Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `PEAT_BIND_ADDRESS` | Address to bind for incoming connections | `0.0.0.0` |
| `PEAT_BIND_PORT` | Port for P2P communication | `4040` |
| `PEAT_HTTP_PORT` | Port for HTTP API | `8080` |
| `PEAT_DISCOVERY_MODE` | Discovery mode: `mdns`, `static`, `hybrid` | `mdns` |

### 4.2 Configuration File

Create `peat.toml` for file-based configuration:

```toml
# peat.toml - Peat Configuration

[node]
id = "node-001"                    # Optional: auto-generated if not set
name = "Primary Sensor Node"
platform_type = "UAV"

[network]
bind_address = "0.0.0.0"
p2p_port = 4040
http_port = 8080
discovery_mode = "hybrid"          # mdns, static, or hybrid

[discovery]
timeout_seconds = 30
mdns_enabled = true
static_peers = [
    "192.168.1.10:4040",
    "192.168.1.11:4040",
]

[cell]
target_size = 5
leader_election_timeout = 10
heartbeat_interval = 5

[hierarchy]
zone_size = 25
aggregation_interval = 10

[security]
formation_key = "your-formation-key-here"  # Required for secure formation
encryption_enabled = true
tls_enabled = true

[storage]
persistence_dir = "/var/lib/peat/data"
max_state_size_mb = 100

[logging]
level = "info"                     # trace, debug, info, warn, error
format = "json"                    # json or pretty
file = "/var/log/peat/peat.log"   # Optional: log to file

[capabilities]
# Define node capabilities
[capabilities.sensor]
type = "EO_IR"
range_km = 10.0
resolution = "4K"

[capabilities.compute]
type = "EDGE_ML"
tflops = 5.2
models = ["yolov8", "detection"]
```

### 4.3 Static Peer Configuration

For environments without mDNS, configure static peers in `peers.toml`:

```toml
# peers.toml - Static peer configuration

[[peers]]
id = "node-alpha"
address = "192.168.1.10"
port = 4040
role = "LEADER"

[[peers]]
id = "node-beta"
address = "192.168.1.11"
port = 4040
role = "FOLLOWER"

[[peers]]
id = "node-gamma"
address = "192.168.1.12"
port = 4040
role = "FOLLOWER"
```

### 4.4 Feature Flags

Enable/disable features at compile time:

| Feature | Description | Default |
|---------|-------------|---------|
| `automerge-backend` | Automerge + Iroh CRDT/transport stack | Enabled |
| `onnx-inference` | Enable ONNX ML inference | Disabled |
| `video-capture` | Enable GStreamer video | Disabled |
| `llm-inference` | Enable LLM via llama.cpp | Disabled |

Build with specific features:
```bash
# Default build (Automerge + Iroh)
cargo build --release

# With ML inference
cargo build --release --features onnx-inference
```

### 4.5 Logging Configuration

#### Log Levels

```bash
# All debug logs
RUST_LOG=debug cargo run --bin peat-sim

# Specific module tracing
RUST_LOG=peat_protocol::discovery=trace,peat_protocol::cell=debug cargo run

# Production logging (info + warnings/errors)
RUST_LOG=info cargo run --release
```

#### JSON Logging for Production

Set `format = "json"` in configuration or:
```bash
PEAT_LOG_FORMAT=json cargo run --release
```

Output example:
```json
{"timestamp":"2025-12-08T10:30:00Z","level":"INFO","target":"peat_protocol::cell","message":"Cell formed","cell_id":"alpha","member_count":5}
```

---

## 5. Deployment Patterns

### 5.1 Development/Single-Node

For development and testing on a single machine:

```bash
# Start the simulator with default configuration
cargo run --release --bin peat-sim

# Or with custom node count
PEAT_NODE_COUNT=20 cargo run --release --bin peat-sim
```

### 5.2 Multi-Node Production

Deploy across multiple machines for production testing:

#### Node 1 (Seed Node)
```bash
# Set as discovery seed
export PEAT_NODE_ID="seed-001"
export PEAT_DISCOVERY_MODE="hybrid"
export PEAT_BIND_ADDRESS="0.0.0.0"
export PEAT_BIND_PORT="4040"

./peat-sim --seed
```

#### Nodes 2-N (Joining Nodes)
```bash
export PEAT_NODE_ID="node-002"
export PEAT_STATIC_PEERS="192.168.1.10:4040"

./peat-sim
```

### 5.3 Edge Device Deployment

For resource-constrained edge devices (Jetson, Raspberry Pi):

```bash
# Build with default features
cargo build --release

# Deploy binary
scp target/release/peat-sim edge-device:/opt/peat/

# Run with reduced resource usage
ssh edge-device "cd /opt/peat && PEAT_CELL_SIZE=3 ./peat-sim"
```

### 5.4 Containerized Deployment

#### Docker Compose Example

```yaml
# docker-compose.yml
version: '3.8'

services:
  peat-seed:
    image: peat-sim:latest
    environment:
      - PEAT_NODE_ID=seed-001
      - PEAT_DISCOVERY_MODE=static
      - RUST_LOG=info
    ports:
      - "4040:4040"
      - "8080:8080"
    volumes:
      - peat-data-seed:/var/lib/peat
    networks:
      - peat-net

  peat-node-1:
    image: peat-sim:latest
    environment:
      - PEAT_NODE_ID=node-001
      - PEAT_STATIC_PEERS=peat-seed:4040
      - RUST_LOG=info
    depends_on:
      - peat-seed
    volumes:
      - peat-data-1:/var/lib/peat
    networks:
      - peat-net

  peat-node-2:
    image: peat-sim:latest
    environment:
      - PEAT_NODE_ID=node-002
      - PEAT_STATIC_PEERS=peat-seed:4040
      - RUST_LOG=info
    depends_on:
      - peat-seed
    volumes:
      - peat-data-2:/var/lib/peat
    networks:
      - peat-net

volumes:
  peat-data-seed:
  peat-data-1:
  peat-data-2:

networks:
  peat-net:
    driver: bridge
```

Start the cluster:
```bash
docker-compose up -d
docker-compose logs -f
```

### 5.5 Kubernetes Deployment

```yaml
# peat-deployment.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: peat-node
spec:
  serviceName: peat
  replicas: 5
  selector:
    matchLabels:
      app: peat
  template:
    metadata:
      labels:
        app: peat
    spec:
      containers:
      - name: peat
        image: peat-sim:latest
        ports:
        - containerPort: 4040
          name: p2p
        - containerPort: 8080
          name: http
        env:
        - name: PEAT_NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: PEAT_DISCOVERY_MODE
          value: "static"
        - name: PEAT_STATIC_PEERS
          value: "peat-node-0.peat:4040"
        volumeMounts:
        - name: data
          mountPath: /var/lib/peat
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 10Gi
---
apiVersion: v1
kind: Service
metadata:
  name: peat
spec:
  clusterIP: None
  selector:
    app: peat
  ports:
  - port: 4040
    name: p2p
  - port: 8080
    name: http
```

---

## 6. Network Configuration

### 6.1 Port Requirements

| Port | Protocol | Purpose | Direction |
|------|----------|---------|-----------|
| 4040 | UDP/TCP | P2P mesh communication | Bidirectional |
| 8080 | TCP | HTTP API | Inbound |
| 5353 | UDP | mDNS discovery | Multicast |

### 6.2 Firewall Rules

#### Linux (iptables)
```bash
# Allow P2P traffic
sudo iptables -A INPUT -p tcp --dport 4040 -j ACCEPT
sudo iptables -A INPUT -p udp --dport 4040 -j ACCEPT

# Allow HTTP API
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT

# Allow mDNS (for discovery)
sudo iptables -A INPUT -p udp --dport 5353 -j ACCEPT
```

#### Linux (firewalld)
```bash
sudo firewall-cmd --permanent --add-port=4040/tcp
sudo firewall-cmd --permanent --add-port=4040/udp
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --permanent --add-service=mdns
sudo firewall-cmd --reload
```

### 6.3 NAT Traversal

Peat uses Iroh for NAT traversal:

```toml
# peat.toml
[network]
nat_traversal = true
relay_servers = [
    "relay.example.com:3478",
]
stun_servers = [
    "stun.l.google.com:19302",
]
```

### 6.4 Bandwidth Considerations

Peat is designed for constrained networks:

| Profile | Bandwidth | Use Case |
|---------|-----------|----------|
| `minimal` | 9.6 Kbps | Tactical radio |
| `low` | 64 Kbps | Satellite link |
| `medium` | 256 Kbps | Cellular backup |
| `standard` | 1 Mbps | WiFi mesh |
| `high` | 10+ Mbps | Ethernet/5G |

Configure bandwidth limits:
```toml
[network]
bandwidth_limit_kbps = 256
qos_enabled = true
```

### 6.5 Network Partition Handling

Peat automatically handles network partitions:

1. **Detection**: Heartbeat timeout (configurable, default 30s)
2. **Recovery**: Exponential backoff reconnection (2s, 4s, 8s, 16s...)
3. **Reconciliation**: CRDT automatic merge on reconnection

Monitor partition status via HTTP API:
```bash
curl http://localhost:8080/api/v1/network/partitions
```

---

## 7. Security

### 7.1 Formation Key

The formation key provides shared-secret authentication for cell formation:

```bash
# Generate a formation key
openssl rand -base64 32
# Example: K7j+3Zp8mN2xYtR5qW1vL9cF4hD6gB0nM8aE2sU7iO4=

# Set in environment
export PEAT_FORMATION_KEY="K7j+3Zp8mN2xYtR5qW1vL9cF4hD6gB0nM8aE2sU7iO4="
```

Or in configuration:
```toml
[security]
formation_key = "K7j+3Zp8mN2xYtR5qW1vL9cF4hD6gB0nM8aE2sU7iO4="
```

### 7.2 PKI Configuration

For production deployments with device certificates:

```toml
[security.pki]
enabled = true
ca_cert = "/etc/peat/certs/ca.crt"
node_cert = "/etc/peat/certs/node.crt"
node_key = "/etc/peat/certs/node.key"
verify_peer = true
```

Generate certificates:
```bash
# Generate CA
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days 365 -key ca.key -out ca.crt -subj "/CN=Peat CA"

# Generate node certificate
openssl genrsa -out node.key 2048
openssl req -new -key node.key -out node.csr -subj "/CN=peat-node-001"
openssl x509 -req -days 365 -in node.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out node.crt
```

### 7.3 Encryption

Peat uses ChaCha20-Poly1305 for symmetric encryption:

```toml
[security]
encryption_enabled = true
encryption_algorithm = "chacha20-poly1305"
```

### 7.4 Authentication

#### Device Authentication
Devices authenticate using ED25519 signatures:
```toml
[security.device_auth]
enabled = true
key_file = "/etc/peat/device.key"
```

#### User Authentication (for HTTP API)
```toml
[security.user_auth]
enabled = true
method = "totp"  # or "password"
password_hash_algorithm = "argon2"
```

### 7.5 Security Best Practices

1. **Always use formation keys** in production
2. **Enable TLS** for HTTP API endpoints
3. **Rotate credentials** regularly
4. **Use PKI** for device authentication in secure environments
5. **Audit logs** for security events
6. **Network segmentation** for Peat traffic

---

## 8. Monitoring & Observability

### 8.1 Health Checks

#### HTTP Health Endpoint
```bash
# Liveness check
curl http://localhost:8080/health
# Response: {"status": "healthy", "uptime_seconds": 3600}

# Readiness check
curl http://localhost:8080/ready
# Response: {"status": "ready", "cell_joined": true, "peers_connected": 4}
```

### 8.2 Metrics

Peat exposes Prometheus-compatible metrics:

```bash
curl http://localhost:8080/metrics
```

Key metrics:
| Metric | Description |
|--------|-------------|
| `peat_peers_connected` | Number of connected peers |
| `peat_cell_size` | Current cell membership count |
| `peat_messages_sent_total` | Total messages sent |
| `peat_messages_received_total` | Total messages received |
| `peat_sync_latency_seconds` | CRDT sync latency histogram |
| `peat_bandwidth_bytes_total` | Bandwidth usage |
| `peat_leader_elections_total` | Leader election count |

### 8.3 Logging

#### Log Levels
- `ERROR`: Critical failures requiring immediate attention
- `WARN`: Degraded operation, potential issues
- `INFO`: Normal operational events
- `DEBUG`: Detailed diagnostic information
- `TRACE`: Very detailed tracing (high volume)

#### Structured Logging
```bash
# Enable JSON logging
PEAT_LOG_FORMAT=json ./peat-sim

# Example output
{"timestamp":"2025-12-08T10:30:00.123Z","level":"INFO","target":"peat_protocol::cell","fields":{"cell_id":"alpha","event":"member_joined","node_id":"node-005"}}
```

### 8.4 Distributed Tracing

Enable OpenTelemetry tracing:

```toml
[telemetry]
tracing_enabled = true
otlp_endpoint = "http://jaeger:4317"
service_name = "peat-node"
```

### 8.5 Alerting Recommendations

| Condition | Severity | Action |
|-----------|----------|--------|
| No peers connected for 5 min | Critical | Check network connectivity |
| Leader election failed 3x | High | Investigate node health |
| Sync latency > 10s | Medium | Check bandwidth constraints |
| Partition detected | High | Monitor for recovery |
| Memory usage > 80% | Medium | Consider scaling |

---

## 9. TAK/ATAK Integration

### 9.1 Overview

Peat integrates with Team Awareness Kit (TAK) via Cursor-on-Target (CoT) protocol translation:

```
┌─────────┐      ┌───────────────┐      ┌──────────┐
│  Peat   │ ←──→ │ CoT Translator│ ←──→ │  ATAK    │
│ Network │      │   (peat-cot)  │      │  Devices │
└─────────┘      └───────────────┘      └──────────┘
```

### 9.2 ATAK Plugin Installation

1. Download the Peat ATAK plugin APK
2. Install on Android device with ATAK
3. Configure connection settings

```
Settings → Tool Preferences → Peat
- Server: 192.168.1.100
- Port: 8087
- Formation Key: [your-key]
```

### 9.3 CoT Configuration

```toml
[cot]
enabled = true
bind_address = "0.0.0.0"
bind_port = 8087
protocol = "tcp"  # tcp, udp, or multicast

[cot.translation]
# Map Peat capabilities to CoT types
platform_uav = "a-f-A-M-F-Q"      # UAV
platform_ugv = "a-f-G-U-C"        # UGV
platform_usv = "a-f-S-X-L"        # USV
sensor_eo_ir = "b-m-p-s-m"        # Sensor point
```

### 9.4 CoT Message Examples

Peat automatically translates to CoT XML:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0" uid="Peat-UAV-001" type="a-f-A-M-F-Q"
       time="2025-12-08T10:30:00Z" start="2025-12-08T10:30:00Z"
       stale="2025-12-08T10:35:00Z" how="m-g">
  <point lat="38.8977" lon="-77.0365" hae="100" ce="10" le="10"/>
  <detail>
    <contact callsign="UAV-001"/>
    <__group name="Alpha" role="Team Lead"/>
    <peat:capability type="EO_IR" range="10000"/>
  </detail>
</event>
```

### 9.5 Bidirectional Integration

Peat receives commands from TAK/ATAK:
- Position updates from ATAK users
- Mission waypoints
- Target designations
- Chat messages

Configure command reception:
```toml
[cot.commands]
accept_waypoints = true
accept_targets = true
accept_chat = true
command_authority = "C2_ONLY"  # or "ANY_TAK_USER"
```

---

## 10. Backup & Recovery

### 10.1 State Persistence

Peat persists state to disk for recovery:

```toml
[storage]
persistence_dir = "/var/lib/peat/data"
snapshot_interval_seconds = 60
max_snapshots = 10
```

### 10.2 Backup Procedures

#### Manual Backup
```bash
# Stop the node gracefully
kill -SIGTERM $(pgrep peat-sim)

# Backup state directory
tar -czf peat-backup-$(date +%Y%m%d).tar.gz /var/lib/peat/data

# Restart the node
./peat-sim
```

#### Automated Backup
```bash
#!/bin/bash
# /etc/cron.daily/peat-backup
BACKUP_DIR=/var/backups/peat
mkdir -p $BACKUP_DIR
tar -czf $BACKUP_DIR/peat-$(date +%Y%m%d%H%M).tar.gz /var/lib/peat/data
# Keep last 7 days
find $BACKUP_DIR -name "peat-*.tar.gz" -mtime +7 -delete
```

### 10.3 Recovery Procedures

#### From Backup
```bash
# Stop the node
kill -SIGTERM $(pgrep peat-sim)

# Restore state
rm -rf /var/lib/peat/data/*
tar -xzf peat-backup-20251208.tar.gz -C /

# Restart
./peat-sim
```

#### From Clean State
```bash
# Remove corrupted state
rm -rf /var/lib/peat/data/*

# Node will rejoin network and sync state from peers
./peat-sim
```

### 10.4 Disaster Recovery

In case of complete node loss:
1. Deploy new node with same configuration
2. Use same formation key
3. Node will automatically rejoin and sync
4. CRDT guarantees eventual consistency

---

## 11. Troubleshooting

### 11.1 Common Issues

#### Issue: Node Not Discovering Peers

**Symptoms**: Node starts but shows 0 peers connected

**Diagnosis**:
```bash
# Check mDNS
avahi-browse -a

# Check network connectivity
ping other-node-ip

# Check ports
netstat -tuln | grep 4040
```

**Solutions**:
1. Verify firewall allows port 4040
2. Check mDNS is enabled on network
3. Use static peer configuration if mDNS unavailable
4. Verify formation keys match across nodes

#### Issue: Leader Election Failing

**Symptoms**: Repeated "leader election timeout" messages

**Diagnosis**:
```bash
# Check node clocks
date
ntpq -p

# Check network latency
ping -c 10 peer-node
```

**Solutions**:
1. Synchronize clocks across nodes (NTP)
2. Increase `leader_election_timeout`
3. Reduce cell size in high-latency environments
4. Check for network partitions

#### Issue: High Memory Usage

**Symptoms**: Node consuming excessive RAM

**Diagnosis**:
```bash
# Check process memory
ps aux | grep peat
top -p $(pgrep peat-sim)

# Check state size
du -sh /var/lib/peat/data/
```

**Solutions**:
1. Enable state pruning with TTL
2. Reduce `max_state_size_mb`
3. Check for runaway capability updates
4. Restart node to clear accumulated state

#### Issue: Sync Latency High

**Symptoms**: Updates taking > 5 seconds to propagate

**Diagnosis**:
```bash
# Check bandwidth
iperf3 -c peer-node

# Check metrics
curl localhost:8080/metrics | grep sync_latency
```

**Solutions**:
1. Enable QoS prioritization
2. Reduce update frequency
3. Increase bandwidth allocation
4. Check for network congestion

#### Issue: Persistence / Backend Errors

**Symptoms**: CRDT open/load failures at startup, corrupted state errors

**Diagnosis**:
```bash
# Verify credentials
echo $PEAT_APP_ID
echo $PEAT_SECRET_KEY

# Check persistence directory
ls -la ./peat_data/
```

**Solutions**:
1. Verify `PEAT_APP_ID` and `PEAT_SECRET_KEY` are set
2. Ensure the persistence directory is writable and not full
3. If state is corrupted, archive and clear the persistence directory; the node will re-sync from peers
4. Check disk space and filesystem health

### 11.2 Diagnostic Commands

```bash
# System information
uname -a
cat /etc/os-release

# Network diagnostics
ip addr
netstat -tuln
ss -tuln

# Process information
ps aux | grep peat
lsof -p $(pgrep peat-sim)

# Log analysis
journalctl -u peat -f
tail -f /var/log/peat/peat.log | jq .

# API health
curl -s localhost:8080/health | jq .
curl -s localhost:8080/api/v1/status | jq .
```

### 11.3 Debug Mode

Enable verbose logging for debugging:

```bash
RUST_LOG=trace RUST_BACKTRACE=1 ./peat-sim 2>&1 | tee debug.log
```

### 11.4 Support Escalation

If issues persist:

1. Collect diagnostic information:
   ```bash
   # Create support bundle
   ./scripts/create-support-bundle.sh
   ```

2. Include:
   - Peat version (`cargo --version`, git commit)
   - Configuration (sanitized, no keys)
   - Logs from issue timeframe
   - Network topology diagram
   - Steps to reproduce

3. Open GitHub issue with collected information

---

## 12. Reference

### 12.1 CLI Reference

```bash
# peat-sim options
peat-sim [OPTIONS]

OPTIONS:
    --config <FILE>          Configuration file path
    --seed                   Run as discovery seed node
    --node-id <ID>           Node identifier
    --bind <ADDR>            Bind address [default: 0.0.0.0]
    --port <PORT>            P2P port [default: 4040]
    --http-port <PORT>       HTTP API port [default: 8080]
    --peers <PEERS>          Static peer list (comma-separated)
    --log-level <LEVEL>      Log level [default: info]
    --help                   Print help information
    --version                Print version information
```

### 12.2 HTTP API Reference

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/ready` | GET | Readiness check |
| `/metrics` | GET | Prometheus metrics |
| `/api/v1/status` | GET | Node status |
| `/api/v1/peers` | GET | Connected peers |
| `/api/v1/cell` | GET | Cell information |
| `/api/v1/capabilities` | GET | Node capabilities |
| `/api/v1/network/partitions` | GET | Partition status |

### 12.3 Makefile Targets

```bash
make help              # Show all targets
make build             # Build all crates
make test              # Run all tests
make test-fast         # Run unit tests only
make test-e2e          # Run E2E tests
make check             # Format + lint + test
make clean             # Clean build artifacts
make pre-commit        # Pre-commit checks
```

### 12.4 Related Documentation

- [Developer Guide](../developer/DEVELOPER_GUIDE.md) - For code contributors
- [Architecture Decisions](../../adr/) - Technical decision rationale
- [Testing Strategy](../../TESTING_STRATEGY.md) - Testing approach
- [Protobuf Schema](../../spec/proto/) - Protocol definitions

---

## Appendix A: Quick Reference Card

### Essential Commands

```bash
# Build
cargo build --release

# Run
cargo run --release --bin peat-sim

# Test
make test-fast

# Logs
RUST_LOG=debug ./peat-sim

# Health check
curl localhost:8080/health
```

### Essential Configuration

```bash
# Minimum environment
export PEAT_APP_ID="your-app-id"
export PEAT_SECRET_KEY="your-secret-key"
export PEAT_FORMATION_KEY="your-formation-key"
```

### Essential Ports

| Port | Purpose |
|------|---------|
| 4040 | P2P |
| 8080 | HTTP API |
| 5353 | mDNS |

---

**Document Version**: 1.0
**Last Updated**: 2025-12-08
**Maintainer**: Peat Operations Team
