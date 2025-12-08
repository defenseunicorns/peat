# Interface Contract: Experiments ↔ All Teams

**Document Version**: 1.0  
**Status**: Draft - Awaiting Approval  
**Owner Team**: Experiments (defines test harness)  
**Consumer Teams**: Core, ATAK, AI  
**Required By**: All Phases (Validation Infrastructure)

---

## Overview

This contract defines how the Experiments team provides test infrastructure and validation services to all other teams. The Experiments team owns the Containerlab topology, network simulation, and metrics collection that validates demo success criteria.

## Test Infrastructure

### Containerlab Topology

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           CONTAINERLAB TOPOLOGY                                  │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │                        C2 Network (10.0.0.0/24)                           │   │
│  │                                                                           │   │
│  │   ┌─────────────┐      ┌─────────────┐      ┌─────────────┐              │   │
│  │   │ tak-server  │      │  webttak    │      │ mlops-server│              │   │
│  │   │ 10.0.0.10   │      │ 10.0.0.11   │      │ 10.0.0.12   │              │   │
│  │   └─────────────┘      └─────────────┘      └─────────────┘              │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                    │                                             │
│                          ┌─────────┴─────────┐                                  │
│                          │   coordinator     │                                  │
│                          │   (bridge)        │                                  │
│                          │   10.0.0.1        │                                  │
│                          └─────────┬─────────┘                                  │
│                        ┌───────────┴───────────┐                                │
│                        │                       │                                │
│  ┌─────────────────────┴──────────┐ ┌─────────┴──────────────────────┐         │
│  │    Network A (10.1.0.0/24)     │ │    Network B (10.2.0.0/24)     │         │
│  │                                │ │                                │         │
│  │  ┌─────────┐ ┌─────────┐      │ │  ┌─────────┐ ┌─────────┐      │         │
│  │  │ alpha-1 │ │ alpha-2 │      │ │  │ bravo-1 │ │ bravo-2 │      │         │
│  │  │ (ATAK)  │ │ (UGV)   │      │ │  │ (ATAK)  │ │ (UAV)   │      │         │
│  │  │10.1.0.1 │ │10.1.0.2 │      │ │  │10.2.0.1 │ │10.2.0.2 │      │         │
│  │  └─────────┘ └─────────┘      │ │  └─────────┘ └─────────┘      │         │
│  │       │                       │ │       │                       │         │
│  │  ┌─────────┐                  │ │  ┌─────────┐                  │         │
│  │  │ alpha-3 │                  │ │  │ bravo-3 │                  │         │
│  │  │ (Jetson)│                  │ │  │ (Jetson)│                  │         │
│  │  │10.1.0.3 │                  │ │  │10.2.0.3 │                  │         │
│  │  └─────────┘                  │ │  └─────────┘                  │         │
│  └────────────────────────────────┘ └────────────────────────────────┘         │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Network Parameters

| Link | Bandwidth | Latency | Packet Loss |
|------|-----------|---------|-------------|
| C2 ↔ Coordinator | 10 Mbps | 50 ms | 0% |
| Coordinator ↔ Network A/B | 500 Kbps | 100 ms | 1% |
| Intra-team (Network A/B) | 5 Mbps | 10 ms | 0% |

### Degraded Network Scenarios

| Scenario | Parameters | Purpose |
|----------|-----------|---------|
| `nominal` | As above | Baseline |
| `contested-light` | 30% loss, 200ms latency | Test resilience |
| `contested-heavy` | 50% loss, 500ms latency | Stress test |
| `bandwidth-limited` | 100 Kbps | MLOps distribution test |
| `partition-alpha` | Network A isolated | Handoff test |

---

## Interface: Test Harness API

### Start Topology

```bash
# Experiments team provides
clab deploy -t hive-demo.clab.yml

# Returns node IPs and status
```

### Configure Network Conditions

```bash
# Set network scenario
./scripts/set-network.sh contested-light

# Custom parameters
./scripts/set-link.sh coordinator-alpha --bandwidth 200kbps --latency 150ms --loss 5%
```

### Inject Events

```bash
# Trigger POI appearance at location
./scripts/inject-poi.sh --lat 33.7749 --lon -84.3958 --description "blue jacket"

# Trigger POI movement
./scripts/move-poi.sh --track-id TRACK-001 --bearing 45 --speed 1.2

# Trigger model update
./scripts/push-model.sh --version 1.3.0 --targets "Alpha-3,Bravo-3"
```

### Collect Metrics

```bash
# Start metrics collection
./scripts/start-metrics.sh --output /data/run-001/

# Metrics collected:
# - Message latency (edge → coordinator → C2)
# - Bandwidth usage per link
# - Sync convergence time
# - Track update rate
```

---

## Validation Criteria by Phase

### Phase 1: Initialization

| ID | Metric | Target | Collection Method |
|----|--------|--------|-------------------|
| P1-1 | Team formation time | < 30s | Timestamp diff |
| P1-2 | Capability advertisement latency | < 5s | Message timestamp |
| P1-3 | TAK registration | Success | TAK Server log |
| P1-4 | WebTAK displays formation | Visual | Screenshot |

### Phase 3: Active Tracking

| ID | Metric | Target | Collection Method |
|----|--------|--------|-------------------|
| P3-1 | Track update latency (edge → C2) | < 2s | Timestamp diff |
| P3-2 | Bandwidth usage (tracking only) | < 10 Kbps | tcpdump analysis |
| P3-3 | Track update rate | ≥ 2 Hz | Message count |
| P3-4 | Track appears on WebTAK | < 5s from detection | Manual timing |

### Phase 4: Track Handoff

| ID | Metric | Target | Collection Method |
|----|--------|--------|-------------------|
| P4-1 | Handoff gap (no track) | < 10s | Track timeline |
| P4-2 | Handoff detection accuracy | > 95% | Ground truth |
| P4-3 | Track ID continuity | Same ID | Log analysis |
| P4-4 | Bravo acquires before Alpha loses | Yes | Timestamp overlap |

### Phase 5: MLOps

| ID | Metric | Target | Collection Method |
|----|--------|--------|-------------------|
| P5-1 | Model distribution time (45 MB) | < 5 min | Download duration |
| P5-2 | Tracking interruption | < 5s | Gap analysis |
| P5-3 | Capability re-advertisement | < 10s | Timestamp diff |
| P5-4 | Rollback on failure | < 30s | Fault injection |
| P5-5 | System operates on 500 Kbps | Yes | Bandwidth cap |

---

## Experiments Team Responsibilities

- [ ] Provide Containerlab topology file (`hive-demo.clab.yml`)
- [ ] Implement network scenario scripts
- [ ] Implement POI injection scripts
- [ ] Implement metrics collection pipeline
- [ ] Provide validation report template
- [ ] Create demo script with timing cues
- [ ] Support rehearsal runs (min 3 before demo)

## All Teams Responsibilities

### Core Team
- [ ] Provide HIVE nodes as Docker containers
- [ ] Expose metrics endpoint (Prometheus format)
- [ ] Log messages with timestamps for analysis
- [ ] Support injected test events

### ATAK Team
- [ ] Provide HIVE-TAK Bridge as Docker container
- [ ] Provide ATAK emulator or record/playback
- [ ] Log CoT messages with timestamps
- [ ] Support WebTAK screenshot capture

### AI Team
- [ ] Provide Jetson inference container (or emulator for sim)
- [ ] Accept injected "detections" in simulation mode
- [ ] Log inference events with timestamps
- [ ] Support model hot-swap timing measurement

---

## Test Artifacts

### Input Files (Experiments Provides)
```
test-inputs/
├── poi-tracks/
│   ├── track-001-path.json      # POI movement path
│   └── track-001-attributes.json # POI description
├── models/
│   ├── object_tracker-1.2.0.onnx
│   └── object_tracker-1.3.0.onnx
└── scenarios/
    ├── nominal.yml
    ├── contested-light.yml
    └── contested-heavy.yml
```

### Output Files (Experiments Collects)
```
test-outputs/
├── run-001/
│   ├── metrics/
│   │   ├── latency.csv
│   │   ├── bandwidth.csv
│   │   └── convergence.csv
│   ├── logs/
│   │   ├── coordinator.log
│   │   ├── alpha-3.log
│   │   └── tak-server.log
│   ├── captures/
│   │   └── webttak-screenshots/
│   └── report.md
```

---

## Integration Points

### Experiments ↔ Core
- Core provides: Docker image `hive-node:latest`
- Core expects: Network connectivity per topology
- Interface: Environment variables for node config

### Experiments ↔ ATAK
- ATAK provides: Docker image `hive-tak-bridge:latest`
- ATAK expects: TAK Server available at `10.0.0.10:8089`
- Interface: CoT/TCP socket

### Experiments ↔ AI
- AI provides: Docker image `hive-ai-node:latest` (x86 simulation)
- AI expects: Injected detections in simulation mode
- Interface: Environment variable `SIMULATION_MODE=true`

---

## Mock Data for Parallel Development

Experiments team provides mock data so teams can develop independently:

### Mock Capability Advertisement
```json
{
  "platform_id": "Alpha-3-Mock",
  "advertised_at": "2025-12-08T10:00:00Z",
  "models": [{
    "model_id": "object_tracker",
    "model_version": "1.2.0",
    "model_hash": "sha256:mock1234...",
    "model_type": "detector_tracker",
    "performance": { "precision": 0.91, "recall": 0.87, "fps": 15 },
    "operational_status": "READY"
  }]
}
```

### Mock Track Update
```json
{
  "track_id": "TRACK-MOCK-001",
  "classification": "person",
  "confidence": 0.89,
  "position": { "lat": 33.7749, "lon": -84.3958, "cep_m": 2.5 },
  "velocity": { "bearing": 45, "speed_mps": 1.2 },
  "timestamp": "2025-12-08T14:10:00Z"
}
```

Teams can use these mocks to develop against while waiting for real implementations.

---

## Approval

| Team | Approver | Date | Signature |
|------|----------|------|-----------|
| Experiments | | | ☐ Approved |
| Core | | | ☐ Approved |
| ATAK | | | ☐ Approved |
| AI | | | ☐ Approved |

---

*Document maintained by (r)evolve - Revolve Team LLC*  
*https://revolveteam.com*
