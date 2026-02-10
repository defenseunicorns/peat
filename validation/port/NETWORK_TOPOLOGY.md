# Phase 3: Port Terminal 200-Node Network Topology

## Overview

Full container terminal simulation with 200 HIVE nodes across 4 operational zones,
each with zone-specific network impairments modeling real-world industrial connectivity.

**Topology file:** `hive-sim/topologies/port-terminal-200node.clab.yaml`
**Impairment script:** `hive-sim/apply-port-terminal-impairments.sh`

## Topology Diagram

```
                        ┌─────────────────────┐
                        │         TOC          │  Fiber backbone
                        │  Terminal Operations │  1 Gbps / 1ms / 0% loss
                        │       Center         │
                        └──────┬───┬───┬───────┘
                               │   │   │
               ┌───────────────┘   │   └───────────────┐
               │                   │                   │
    ┌──────────▼──────────┐  ┌─────▼──────┐  ┌────────▼────────┐
    │  Berth Supervisors  │  │   Yard     │  │  Gate Manager   │
    │  (×2, one per berth)│  │  Manager   │  │                 │
    └──────────┬──────────┘  └─────┬──────┘  └────────┬────────┘
               │                   │                   │
    ╔══════════╧══════════╗  ╔═════╧══════╗  ╔════════╧════════╗
    ║  BERTH ZONE (×2)    ║  ║ YARD ZONE  ║  ║  GATE ZONE      ║
    ║  Industrial WiFi    ║  ║ Cell/WiFi  ║  ║  Wired backbone  ║
    ║  50 Mbps / 15ms     ║  ║ 100 Mbps   ║  ║  1 Gbps / 2ms   ║
    ║  5ms jitter / 2%    ║  ║ 10ms / 1%  ║  ║  1ms jitter      ║
    ╚═════════════════════╝  ╚════════════╝  ╚═════════════════╝
```

## Zone Breakdown (200 nodes total)

### TOC — Terminal Operations Center (1 node)

Top-level aggregation point. Receives aggregated data from all zone supervisors.

| Node | Role | Connects To |
|------|------|-------------|
| `toc` | Terminal Operations Center | (accepts from zone supervisors) |

### Berth Zone (2 berths x 57 = 114 nodes)

Each berth models quayside vessel operations.

| Component | Count per Berth | Role | Connects To |
|-----------|----------------|------|-------------|
| Berth Supervisor | 1 | Zone aggregation | TOC |
| Quay Cranes (STS) | 4 | Container load/unload | Berth Supervisor |
| Vessel Agents | 2 | Vessel coordination | Berth Supervisor |
| Stevedore Team Leads | 6 | Team coordination | Berth Supervisor |
| Stevedore Workers | 36 (6 per lead) | Lashing/unlashing | Team Lead |
| Yard Tractors | 4 | Berth-yard transport | Berth Supervisor |
| Reefer Monitors | 4 | Temperature monitoring | Berth Supervisor |

**Hierarchy depth:** TOC -> Berth Supervisor -> Team Lead -> Worker (4 levels)

### Yard Zone (53 nodes)

Container storage, stacking, and internal logistics.

| Component | Count | Role | Connects To |
|-----------|-------|------|-------------|
| Yard Manager | 1 | Zone aggregation | TOC |
| Block Supervisors | 8 | Yard block oversight | Yard Manager |
| Stacking Cranes (RTG/RMG) | 4 | Container stacking | Yard Manager |
| Reach Stackers | 8 | Container handling | Yard Manager |
| Yard Tractors (shared pool) | 12 | Internal transport | Yard Manager |
| TOS Terminals | 8 | Container tracking (TOS) | Yard Manager |
| Reefer Monitors | 8 | Cold chain monitoring | Yard Manager |
| Yard Inspectors | 4 | Physical inspection | Yard Manager |

### Gate Zone (32 nodes)

Ingress/egress control for trucks and rail.

| Component | Count | Role | Connects To |
|-----------|-------|------|-------------|
| Gate Manager | 1 | Zone aggregation | TOC |
| Truck Gate Controllers | 2 | Lane management | Gate Manager |
| Lane Scanners | 6 (3 per gate) | Container scanning | Gate Controller |
| Lane RFID Readers | 6 (3 per gate) | Container ID | Gate Controller |
| Lane Operators | 6 (3 per gate) | Manual verification | Gate Controller |
| Rail Gate Supervisor | 1 | Rail operations | Gate Manager |
| Rail Loaders | 2 | Rail car loading | Rail Supervisor |
| Rail Scanners | 2 | Rail container scan | Rail Supervisor |
| Security Workers | 4 | Perimeter security | Gate Manager |
| Customs Inspectors | 2 | Customs clearance | Gate Manager |

## Network Impairment Profiles

Applied post-deployment via `apply-port-terminal-impairments.sh`.

| Zone | Rate | Delay | Jitter | Loss | Rationale |
|------|------|-------|--------|------|-----------|
| TOC | 1 Gbps | 1ms | 0ms | 0% | Fiber backbone in control room |
| Berth | 50 Mbps | 15ms | 5ms | 2% | Industrial WiFi with metal interference from crane structures and vessel hulls |
| Yard | 100 Mbps | 10ms | 3ms | 1% | Mixed cellular/WiFi across open yard area |
| Gate | 1 Gbps | 2ms | 1ms | 0.1% | Hardwired infrastructure at fixed checkpoints |

## Deployment

```bash
# Build Docker image (once)
make build-docker

# Deploy topology + apply impairments
make clab-deploy-phase3

# Re-apply impairments (if needed after restart)
make clab-impairments-phase3

# Tear down
make clab-destroy-phase3
```

## Event Generation Rates

Leaf nodes generate events at domain-appropriate rates:

| Equipment Type | Detection Rate | Telemetry Rate | Example Events |
|---------------|---------------|----------------|----------------|
| Quay Cranes | 5/sec | 2/sec | Container lifts, spreader positions |
| Stacking Cranes | 5/sec | 2/sec | Stack moves, slot assignments |
| Tractors/Vehicles | 3/sec | 2/sec | Position updates, load status |
| Scanners | 10/sec | 1/sec | Container scans, OCR reads |
| RFID Readers | 15/sec | 1/sec | Tag reads, container IDs |
| TOS Terminals | 8/sec | 1/sec | Booking updates, bay plans |
| Reefer Monitors | 0.5/sec | 2/sec | Temperature, power status |
| Workers/Operators | 1-2/sec | 0.5/sec | Task completions, exceptions |

## Acceptance Criteria

- [ ] 200-node topology deploys successfully
- [ ] All nodes reach their aggregation peers (check via orchestrator or logs)
- [ ] Network impairments verified per zone (`containerlab tools netem show`)
- [ ] Event flow reaches TOC from all zones
- [ ] Aggregation reduces message volume at zone supervisor level
