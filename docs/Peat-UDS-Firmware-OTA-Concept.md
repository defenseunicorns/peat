# Peat + UDS: Firmware, Models, and Software Delivery Beyond the Enterprise Edge

**Concept Document — Defense Unicorns Integration Discussion**
**Date**: 2026-02-22
**Authors**: Kit Plummer

## Executive Summary

UDS delivers software to Kubernetes clusters. But customers operating at the tactical edge need to deliver **firmware to drones**, **AI models to GPU nodes**, **configs to radios**, and **software to vehicles** — none of which run Kubernetes.

**Peat extends UDS to every target type**, using a unified mesh protocol for coordination, distribution, and convergence tracking across the full spectrum of platforms — from cloud data centers to embedded microcontrollers.

This document outlines how Peat's protocol and mesh networking capabilities address the firmware/OTA delivery use case and broader software supply chain needs beyond the enterprise edge.

## The Customer Problem

### What Customers Are Asking For

Defense and intelligence customers have a consistent set of needs that no single tool addresses today:

1. **"Deliver firmware updates to platforms that don't run Kubernetes"**
   - Drone autopilots (PX4 on STM32)
   - Vehicle ECUs (embedded Linux, RTOS)
   - Radio systems (SDR firmware)
   - Sensor payloads (camera firmware, LIDAR processors)
   - Robotics controllers

2. **"Deliver AI models to inference hardware at the edge"**
   - NVIDIA Jetson / Xavier nodes
   - Intel edge accelerators
   - Qualcomm platforms
   - Models range from 10MB to 2GB+

3. **"Give me one view of what's running across my entire fleet"**
   - Which drones have autopilot v1.14.3?
   - Which GPU nodes have the latest perception model?
   - Which vehicles have the new radio firmware?
   - Is this formation mission-ready?

4. **"Updates must work over intermittent tactical links"**
   - Hours to days without connectivity
   - Bandwidth measured in Kbps, not Gbps
   - Contested/denied RF environments
   - Store-and-forward is essential, not optional

5. **"Coordinate multi-artifact updates as a single operation"**
   - Update autopilot firmware AND perception model AND radio firmware on a drone as one operation
   - Roll back the entire bundle if any piece fails
   - Track convergence of the complete platform loadout, not individual artifacts

### What Exists Today — And Where It Falls Short

| Tool | What It Does Well | What It Can't Do |
|------|------------------|-----------------|
| **Zarf** | Air-gap K8s package delivery | Deliver firmware to non-K8s targets |
| **UDS Core** | Secure K8s runtime platform | Manage embedded devices |
| **Mender/RAUC/SWUpdate** | Single-device firmware OTA | Mesh distribution, fleet coordination, DIL operation |
| **MLflow/Kubeflow** | Model registry and tracking | Disconnected distribution, hierarchical propagation |
| **Ansible/Puppet** | Configuration management | Intermittent connectivity, mesh networking |

**No tool provides unified delivery across firmware + models + containers + config in disconnected environments.**

## Peat's Value Proposition

### What Peat Brings to the Table

Peat is a mesh networking protocol built on CRDTs (Conflict-free Replicated Data Types) that provides:

| Capability | How It Helps |
|------------|-------------|
| **Multi-transport mesh** | QUIC, BLE, UDP, satellite — reaches every platform type |
| **Hierarchical distribution** | Cloud → FOB → vehicle → device, with caching at each tier |
| **CRDT-based sync** | Eventual consistency without central server — works in DIL |
| **Content-addressed blob transfer** | Large file distribution with deduplication and resumable transfers |
| **Convergence tracking** | Real-time visibility into deployment progress across the fleet |
| **Capability advertisement** | Every node advertises what it is and what it's running |
| **QoS and prioritization** | Critical safety updates take priority over routine maintenance |

### The Unified Delivery Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Cloud / Enterprise                         │
│                                                                   │
│   CI/CD Pipeline ──▶ Artifact Registry ──▶ Peat Gateway Node     │
│   (builds firmware,    (OCI, firmware     (publishes manifests,  │
│    models, Zarf pkgs)   images, ONNX)      distributes metadata) │
│                                                                   │
└─────────────────────────────────┬─────────────────────────────────┘
                                  │ Peat Sync (metadata + blobs)
                                  │
┌─────────────────────────────────┼─────────────────────────────────┐
│                     FOB / Base  ▼                                  │
│                                                                    │
│   ┌────────────┐  ┌────────────┐  ┌────────────┐                  │
│   │ Zarf Mirror│  │ Peat Node  │  │ Blob Cache │                  │
│   │ (K8s pkgs) │  │ (metadata) │  │ (firmware, │                  │
│   │            │  │            │  │  models)   │                  │
│   └────────────┘  └─────┬──────┘  └────────────┘                  │
│                          │                                         │
└──────────────────────────┼─────────────────────────────────────────┘
                           │ Peat Sync (hierarchical cascade)
              ┌────────────┼──────────────┐
              │            │              │
              ▼            ▼              ▼
     ┌──────────────┐ ┌──────────┐ ┌─────────────┐
     │  K8s Cluster │ │  Drone   │ │  Vehicle    │
     │  (Zarf)      │ │  Fleet   │ │  Fleet      │
     │              │ │          │ │             │
     │  containers  │ │ firmware │ │ firmware    │
     │  services    │ │ AI model │ │ radio FW    │
     │              │ │ config   │ │ config      │
     └──────────────┘ └──────────┘ └─────────────┘
           │               │              │
     K8s workloads    Autopilot FW    ECU firmware
     Helm charts      ONNX models     SDR firmware
     UDS Core         Camera FW       Nav config
```

**Peat is the coordination layer that ties all of this together.** Zarf remains the K8s delivery mechanism. Firmware OTA agents handle embedded targets. ONNX Runtime handles AI models. Peat provides the mesh, the metadata sync, the convergence tracking, and the fleet visibility across all of them.

## The Firmware OTA Use Case in Detail

### Why Firmware OTA Is Hard (And Different From Containers)

| Concern | Container Deployment | Firmware OTA |
|---------|---------------------|-------------|
| Failure mode | Container restarts | **Device is bricked** |
| Rollback | Delete pod, re-pull | **A/B partition swap, bootloader involvement** |
| Compatibility | Architecture match | **Exact board revision, bootloader version, peripheral config** |
| Activation | Start container | **Reboot into new partition, verify boot, commit** |
| Safety | OOMKill, health checks | **Battery level, not in flight, stable power, not actively engaged** |
| Runtime dependency | Container runtime, K8s | **None — just a bootloader and bare metal** |
| Size | 100MB-1GB (layered) | **500KB-100MB (monolithic binary)** |

### What Peat Provides for Firmware OTA

**1. Firmware Manifest — "What firmware goes on what hardware"**

Every firmware release is described by a manifest that includes:
- Hardware compatibility matrix (board type, revision, bootloader version)
- Update policy (immediate reboot, deferred, manual)
- Rollback configuration (auto-rollback on boot failure, golden image)
- Safety constraints (minimum battery, stable power, not in flight)
- Cryptographic signatures and provenance
- Delta patch availability (for bandwidth savings)

**2. Mesh Distribution — "Get firmware to devices over any link"**

```
Cloud ──QUIC──▶ FOB ──QUIC──▶ Vehicle Gateway ──BLE──▶ Drone
                 │                                      ▲
                 └──QUIC──▶ Other Vehicle ──Serial──▶ ECU
```

Peat's multi-transport mesh means firmware can flow over whatever link is available:
- QUIC for high-bandwidth backbone links
- BLE for close-range maintenance updates
- UDP for WiFi-connected embedded devices
- Serial bridge for MCUs behind a gateway processor
- Satellite (Iridium SBD) for remote platforms

**3. Hierarchical Caching — "Don't retransmit the same firmware 100 times"**

```
Cloud pushes 2MB firmware to FOB:           1x transfer
FOB pushes to 5 vehicle gateways:           5x transfer
Each gateway pushes to 20 drones:           5x transfer (cached at gateway)

Without hierarchy: 100 x 2MB = 200MB over backbone
With hierarchy:    1 x 2MB backbone + 5 x 2MB FOB-vehicle + 5 x 2MB vehicle-drone
                   = 22MB over backbone (91% reduction)
```

**4. Delta Updates — "Send only what changed"**

For incremental firmware versions, binary diff patches reduce transfer size by 80-95%:
- Full PX4 firmware: 2.1 MB
- Delta patch v1.14.2 → v1.14.3: 180 KB

Over a 9.6Kbps tactical link:
- Full image: 29 minutes
- Delta: 2.5 minutes

**5. Convergence Tracking — "Are all my drones updated?"**

Peat's CRDT-based status aggregation gives fleet-wide visibility:
- Each device reports its firmware version and OTA state
- Status aggregates through the hierarchy (squad → platoon → company → battalion)
- Operators see convergence percentage, blockers, and stragglers at each echelon
- No central server required — works even with intermittent connectivity

**6. Safety Enforcement — "Don't brick my drone mid-flight"**

The OTA agent enforces safety constraints before firmware activation:
- Battery above threshold (prevents bricking during flash)
- Platform in safe state (not in flight, not in motion)
- Stable power source confirmed
- Hardware compatibility verified
- All firmware dependencies satisfied

Automatic rollback if the new firmware fails boot verification.

### The OTA Lifecycle Through Peat

```
1. PUBLISH    — CI/CD builds firmware, publishes manifest to Peat
2. PROPAGATE  — Manifest syncs through hierarchy via CRDT
3. COMMAND    — Operator issues deployment directive targeting a formation
4. DISTRIBUTE — Firmware blob cascades through hierarchy (cached at each tier)
5. VERIFY     — OTA agent checks hardware compatibility and safety constraints
6. STAGE      — Firmware written to inactive partition
7. ACTIVATE   — Reboot (or hot-swap) into new firmware
8. VERIFY     — Boot health checks run; pass → commit, fail → rollback
9. REPORT     — Status flows back up through hierarchy
10. CONVERGE  — Operator sees fleet-wide convergence progress
```

## AI Model Delivery — The Other Half

Firmware and AI models are two sides of the same coin. Many platforms need both:

```
Drone Platform
├── Autopilot firmware          ← Firmware OTA
├── Perception model (YOLOv8)   ← AI Model Delivery
├── Camera firmware             ← Firmware OTA
├── Radio firmware              ← Firmware OTA
└── Mission config / ROE        ← Config sync (CRDT)
```

Peat handles AI model delivery with the same primitives:
- **ONNX as standard format** — vendor-neutral, auditable, portable across hardware
- **Variant selection** — INT8 for CPU nodes, FP16 for GPU nodes, auto-selected per device
- **Differential propagation** — only changed model weights transfer (29x bandwidth savings)
- **Performance monitoring** — inference latency, accuracy metrics aggregate through hierarchy
- **Convergence tracking** — "do all ISR platforms have the latest target recognition model?"

The key insight: **firmware + models + config should be coordinated as a single platform update**, not managed by three separate systems.

## Fleet Management and Orchestration

### The "Mission Ready?" Question

An operator's core question is not "what version is installed?" but **"can this formation execute its mission?"**

Peat's capability-focused model answers this by aggregating:
- Firmware versions and health status across all platforms
- AI model versions and inference performance metrics
- Configuration state (ROE, mission parameters)
- Hardware health (battery, sensors, comms)

Into a capability assessment:

```
Formation Alpha — Mission Readiness: 94%
├── Autopilot firmware v1.14.3:  38/40 drones (95%)  — 2 downloading
├── Perception model v4.2.1:     40/40 drones (100%) ✓
├── Radio firmware v3.7.0:       39/40 drones (98%)  — 1 failed (battery)
├── Mission config 2026-Q1:      40/40 drones (100%) ✓
│
├── Blockers:
│   ├── drone-047: radio FW failed — battery at 12%, needs charge
│   └── drone-089: autopilot FW downloading — ETA 3 min (low-bandwidth link)
│
└── Recommendation: Formation is mission-capable. 2 drones non-critical.
```

### Metadata as a CRDT — No Central Server Required

All fleet management metadata lives in Peat's CRDT data store:
- Firmware manifests replicate to all nodes that need them
- Device capability advertisements are eventually consistent
- Deployment status aggregates through the hierarchy
- No single point of failure — any node can answer fleet queries for its sub-tree

This is fundamentally different from centralized fleet management (Mender, hawkBit, Balena) which require a management server to be reachable.

## How This Extends UDS

### Today: UDS Delivers to K8s

```
Zarf → K8s cluster (containers, Helm charts, UDS Core)
```

### Tomorrow: UDS + Peat Delivers to Everything

```
                    ┌── Zarf ────────── K8s clusters (containers)
                    │
UDS + Peat ────────┼── Firmware OTA ── Drones, vehicles, radios (firmware)
                    │
                    ├── Model Delivery ─ GPU/NPU nodes (ONNX models)
                    │
                    ├── Config Sync ──── All platforms (CRDT-based config)
                    │
                    └── Peat-Lite ────── Sensors, MCUs (lightweight gossip)
```

Peat becomes the **universal coordination layer**:
- Zarf handles the K8s "last mile"
- Firmware OTA agents handle the embedded "last mile"
- ONNX Runtime handles the AI inference "last mile"
- Peat provides the mesh, metadata, convergence, and fleet visibility for all of them

### The Defense Unicorns Value Story

| Without Peat | With Peat |
|-------------|-----------|
| UDS delivers to K8s only | UDS delivers to every target type |
| Firmware updates require separate tools per platform | One coordination layer for all firmware targets |
| AI model delivery is ad-hoc | Hierarchical model distribution with convergence tracking |
| Fleet visibility requires multiple dashboards | Unified fleet view across all artifact types |
| Disconnected updates fail | Mesh distribution and store-and-forward work in DIL |
| Multi-artifact updates are uncoordinated | Bundled platform updates with ordered deployment and rollback |

## Technical Integration Points

### Where Peat Meets Zarf/UDS

Peat doesn't replace Zarf — it complements it:

| Layer | Tool | Role |
|-------|------|------|
| Package building | Zarf | Build air-gap packages for K8s workloads |
| Firmware building | CI/CD + Peat manifests | Build firmware images, publish to Peat |
| Model training | MLOps pipeline | Train models, export as ONNX, publish to Peat |
| Metadata coordination | Peat | Sync manifests, directives, status across mesh |
| K8s deployment | Zarf | Deploy containers to K8s clusters |
| Firmware deployment | Peat OTA Agent | Flash firmware to embedded targets |
| Model deployment | Peat + ONNX Runtime | Distribute and activate models on GPU nodes |
| Fleet visibility | Peat | Aggregate status through hierarchy |
| Security/provenance | Peat + Zarf (shared) | Signatures, SBOM, audit trail |

### Integration Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Build Pipeline                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │ Firmware  │  │ Zarf     │  │ Model    │  │ Config   │            │
│  │ Build     │  │ Package  │  │ Training │  │ Bundle   │            │
│  └─────┬────┘  └─────┬────┘  └─────┬────┘  └─────┬────┘            │
│        │             │             │             │                   │
│        ▼             ▼             ▼             ▼                   │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                  Artifact Registry                           │   │
│  │  Firmware images │ OCI packages │ ONNX models │ Config bundles│  │
│  └──────────────────────────────────────────────────────────────┘   │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 ▼
┌────────────────────────────────────────────────────────────────────┐
│                    Peat Mesh Protocol Layer                          │
│                                                                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐    │
│  │ Manifests  │  │ Deployment │  │ Device     │  │ Convergence│    │
│  │ (CRDT)     │  │ Directives │  │ Capability │  │ Status     │    │
│  │            │  │ (targeted) │  │ Advertise  │  │ (aggregated│    │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘    │
│                                                                      │
│  Transport: QUIC │ BLE │ UDP │ Satellite │ Serial Bridge             │
└────────────────────────────────────────────────────────────────────┘
```

## Competitive Differentiation

### Why Peat Is Different From Existing OTA Solutions

| Feature | Mender/RAUC/SWUpdate | Peat Firmware OTA |
|---------|---------------------|------------------|
| Architecture | Client-server | Peer-to-peer mesh |
| Connectivity | Requires server | Works disconnected (DIL) |
| Distribution | Direct from server | Hierarchical cascade with caching |
| Fleet coordination | Centralized dashboard | Distributed via CRDT |
| Multi-artifact | Firmware only | Firmware + models + containers + config |
| Convergence tracking | Server-side polling | CRDT-based hierarchical aggregation |
| Delta updates | Some support | Binary diff with mesh-optimized delivery |
| Transport | HTTP/HTTPS | Multi-transport (QUIC, BLE, UDP, satellite) |
| Security model | TLS + signatures | Zero-trust with signature chains and audit trail |

### Why Not Just Use Mender + MLflow + Zarf Separately?

You could — but you'd have:
- Three separate management planes with no coordination
- No unified fleet view across artifact types
- No coordinated multi-artifact updates
- No mesh distribution (each tool assumes connectivity to its own server)
- No store-and-forward for disconnected platforms
- No hierarchical caching for bandwidth-constrained tactical links
- Three separate security and audit systems

Peat provides the **connective tissue** that makes firmware, models, containers, and config work as a single coherent delivery system.

## Use Cases and Scenarios

### Scenario 1: Drone Fleet Firmware Update (100 drones, FOB environment)

**Without Peat:**
- Bring each drone to maintenance tent
- Connect via USB, flash firmware manually
- Track completion on a spreadsheet
- Time: 2 days for 100 drones

**With Peat:**
- Operator issues deployment directive from FOB Peat node
- Firmware cascades through mesh: FOB → vehicle gateways → drones
- Drones stage firmware, activate during next landing/idle period
- Convergence tracked automatically through hierarchy
- Time: 4-6 hours (mostly waiting for drones to land)

### Scenario 2: Emergency ROE Update + Model Refresh (contested environment)

**Situation:** New no-strike zone identified. Must update ROE config AND perception model that enforces it across all platforms.

**Without Peat:**
- Push ROE config via one system (maybe Ansible if connected)
- Push model update via another system (manual/ad-hoc)
- No coordination — some platforms have new ROE but old model
- No visibility into which platforms are updated

**With Peat:**
- Bundle ROE config + model as `PlatformUpdateBundle`
- Issue with `Critical` priority — takes precedence over all other traffic
- Peat distributes both artifacts, applies in correct order
- Convergence tracking shows real-time progress
- Platforms verify both artifacts before marking mission-ready

### Scenario 3: Disconnected Outpost Update (days without connectivity)

**Situation:** Remote outpost with 10 sensor nodes and 5 vehicles. Intermittent satellite link (9.6Kbps, 15 min/day window).

**Without Peat:**
- Can't push updates over 9.6Kbps in 15 minutes
- Physical media delivery (USB drives) — days/weeks delay
- No visibility into what's running at the outpost

**With Peat:**
- Delta firmware patch (180KB) fits in a single satellite window
- Peat mesh at outpost distributes to all devices locally via WiFi/BLE
- Status aggregation flows back up via next satellite window
- Full firmware image (if needed) trickles over multiple windows with resumable transfer

## Summary

Peat enables Defense Unicorns to extend UDS from "enterprise K8s delivery" to **"deliver anything to any platform, anywhere, over any link."**

The firmware OTA use case is the tip of the spear — it addresses the most immediate customer demand (delivering to non-K8s platforms). But the broader story is about **unified software supply chain management** across the full spectrum of defense platforms, from cloud data centers to embedded microcontrollers.

**What Peat adds to UDS:**
- Mesh networking that works in disconnected/intermittent/limited (DIL) environments
- Hierarchical distribution with caching at every tier
- CRDT-based fleet management without a central server
- Multi-artifact coordinated deployment (firmware + models + containers + config)
- Convergence tracking and capability-focused fleet visibility
- Multi-transport delivery (QUIC, BLE, UDP, satellite)

**What UDS adds to Peat:**
- Proven K8s packaging and deployment (Zarf)
- Secure runtime platform (UDS Core)
- Policy enforcement (Pepr)
- Air-gap tooling and SBOM generation
- Enterprise adoption and customer trust

Together, Peat + UDS = complete tactical software delivery from cloud to edge to embedded.
