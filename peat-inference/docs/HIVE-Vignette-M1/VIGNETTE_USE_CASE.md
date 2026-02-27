# PEAT Protocol Proof-of-Concept Vignette
## Object Tracking Across Distributed Human-Machine-AI Teams

**Document Version**: 3.0
**Date**: 2025-12-10
**Author**: (r)evolve Inc. — https://revolveteam.com
**Status**: Draft

**Revision History**:
| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2025-11-26 | Initial vignette with flat team structure |
| 3.0 | 2025-12-10 | Added sub-tier architecture (PEAT-Lite), M5Stack Core2 wearables, Waveshare UGV Beast platforms, ADR-035 reference |

---

## Executive Summary

This vignette demonstrates PEAT Protocol's core value proposition: enabling coordinated object tracking across geographically distributed, network-separated teams of humans, machines, and AI models. The scenario showcases hierarchical capability aggregation, cross-network synchronization, TAK-based command and control, edge MLOps with model redistribution, and the "stop moving data, start moving decisions" philosophy that differentiates PEAT from traditional data-centric approaches.

---

## 1. Scenario Overview

### 1.1 Mission Context

A **Person of Interest (POI)** is being tracked as they move through an operational area monitored by two independent tactical teams. The POI's movement path crosses team boundaries, requiring seamless handoff of tracking responsibility. A central coordinator manages both teams, while a higher C2 element issues tasking and monitors mission progress via TAK.

During the mission, the C2 element pushes an updated AI model to improve tracking performance — demonstrating PEAT's **bidirectional flows**: decisions flow up, capabilities (including models) flow down.

### 1.2 Operational Challenge

Traditional approaches require each sensor to stream raw video/imagery to a central fusion center, consuming massive bandwidth. PEAT's approach:
- Each team runs object-tracking AI locally
- Teams **advertise tracking capability** rather than stream raw data
- Only **track updates (decisions)** flow upward — not raw sensor data
- Model updates flow **downward** through the hierarchy
- C2 sees a unified track across team boundaries with minimal bandwidth

### 1.3 Key Demonstration Objectives

| Objective | PEAT Feature Demonstrated |
|-----------|--------------------------|
| Cross-network coordination | PEAT Bridge + Relay Discovery |
| Human-AI-Machine teaming | Operator model with authority composition |
| Capability-based tasking | AI Model Capability Advertisement |
| TAK integration | CoT ↔ PEAT message translation |
| Hierarchical aggregation | Track summaries flow upward, not raw data |
| Edge MLOps | Model retraining + redistribution via PEAT |
| Bandwidth efficiency | 95%+ reduction vs. raw streaming |

---

## 2. Participants and Topology

### 2.1 Team Alpha (Network A)

Team Alpha demonstrates the **hierarchical sub-tier architecture** where both human operators and autonomous platforms aggregate multiple PEAT nodes:

| Entity | PEAT Node(s) | Description |
|--------|--------------|-------------|
| **Alpha-1 (Operator)** | Squad Member (aggregation tier) | Human operator identity |
| ↳ Alpha-1a | ATAK on Android (PEAT-Full) | SA, comms, mapping, tasking |
| ↳ Alpha-1b | M5Stack Core2 (PEAT-Lite) | Biometrics, activity, panic button |
| **Alpha-2 (UGV)** | UGV (aggregation tier) | Waveshare UGV Beast platform |
| ↳ Alpha-2a | Jetson Orin Nano (PEAT-Full) | AI vision, navigation, ROS2 |
| ↳ Alpha-2b | ESP32 (PEAT-Lite) | Motor control, proximity, IMU |

**Hardware Details:**

| Component | Model | Capabilities |
|-----------|-------|--------------|
| Operator Phone | Android + ATAK | GPS, camera, mesh radio, full PEAT |
| Wearable Sensor | M5Stack Core2 | 6-axis IMU (MPU6886), microphone (SPM1423), touch, RTC, battery monitoring |
| UGV Platform | Waveshare UGV Beast | Tracked chassis, 360° pan-tilt, 5MP 160° camera, LiDAR |
| UGV Compute | Jetson Orin Nano | YOLOv8 + DeepSORT, ROS2, full PEAT |
| UGV Controller | ESP32 | Motor PID, sensor fusion, PEAT-Lite |

**Sub-Tier Data Flow:**
```
Alpha Squad (PEAT aggregation)
├── Alpha-1 (Operator) ──────────────────────────────┐
│   ├── ATAK Phone (Full) ◄──────────────────────────┤ Operator
│   │   └── Position, SA, tasking, comms             │ Document
│   └── Core2 (Lite) ◄───────────────────────────────┤
│       └── Activity, posture, battery, panic button │
│                                                    ▼
├── Alpha-2 (UGV) ───────────────────────────────────┐
│   ├── Jetson Orin (Full) ◄─────────────────────────┤ UGV
│   │   └── Tracks, vision, navigation               │ Document
│   └── ESP32 (Lite) ◄───────────────────────────────┤
│       └── Motor state, IMU, proximity, battery     │
│                                                    ▼
└── Squad-level aggregation ─────────────────────────► Coordinator
```

**Team Composition**: 1 Human (2 nodes) + 1 UGV (2 nodes) = 4 PEAT nodes forming 2 logical entities

### 2.2 Team Bravo (Network B)

| Entity | PEAT Node(s) | Description |
|--------|--------------|-------------|
| **Bravo-1 (Operator)** | Squad Member (aggregation tier) | Human operator identity |
| ↳ Bravo-1a | ATAK on Android (PEAT-Full) | SA, comms, mapping, tasking |
| ↳ Bravo-1b | M5Stack Core2 (PEAT-Lite) | Biometrics, activity, panic button |
| **Bravo-2 (UGV)** | UGV (aggregation tier) | Waveshare UGV Beast platform |
| ↳ Bravo-2a | Jetson Orin Nano (PEAT-Full) | AI vision, navigation, ROS2 |
| ↳ Bravo-2b | ESP32 (PEAT-Lite) | Motor control, proximity, IMU |

**Team Composition**: 1 Human (2 nodes) + 1 UGV (2 nodes) = 4 PEAT nodes forming 2 logical entities

> **Note**: Original vignette specified Bravo-2 as UAV. Updated to UGV for hardware consistency with available Waveshare platforms. UAV variant remains a valid extension scenario.

### 2.3 Coordinator Node (Network Bridge)

| Role | Platform | Description |
|------|----------|-------------|
| **Coord-1 (Bridge)** | Laptop/Tablet | Runs PEAT Bridge connecting Networks A & B |
| **Coord-2 (Aggregator)** | Same device | Aggregates tracks from both teams |
| **Coord-3 (Operator)** | ATAK on Android | Coordinator's tactical display |

**Connectivity**: Dual-homed to both Network A and Network B (e.g., dual WiFi or WiFi + cellular)

### 2.4 Command Element (C2)

| Role | Platform | Description |
|-----------|----------|-------------|
| **C2-1 (TAK Server)** | TAK Server | Receives aggregated tracks, hosts WebTAK |
| **C2-2 (Commander)** | WebTAK (Browser) | Issues track-target commands, monitors mission |
| **C2-3 (MLOps)** | Training Server | Retrains models, pushes updates via PEAT |

---

## 3. Network Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        C2 Element                                    │
│                                                                      │
│  ┌─────────────┐    ┌───────────────┐    ┌───────────────────────┐  │
│  │ TAK Server  │◄──►│ WebTAK (C2)   │    │ MLOps Training Server │  │
│  │             │    │ (Browser UI)  │    │ • Model retraining    │  │
│  └──────┬──────┘    └───────────────┘    │ • Version management  │  │
│         │                                 └───────────┬───────────┘  │
│         │ CoT/TCP                                     │              │
└─────────┼─────────────────────────────────────────────┼──────────────┘
          │                                             │
          │                                             │ Model Push
          ▼                                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Coordinator (Bridge Node)                       │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    PEAT-TAK Bridge                            │   │
│  │  • Aggregates tracks from Alpha + Bravo                       │   │
│  │  • Converts PEAT → CoT for TAK Server                        │   │
│  │  • Converts CoT → PEAT for team tasking                      │   │
│  │  • Routes model updates to teams (downward flow)             │   │
│  └─────────────────────────┬────────────────────────────────────┘   │
│                            │                                         │
│  ┌────────────────────────┬┴───────────────────────────────────┐    │
│  │        Network A       │           Network B                 │    │
│  │       Interface        │           Interface                │    │
│  └──────────┬─────────────┴──────────────┬─────────────────────┘    │
└─────────────┼────────────────────────────┼──────────────────────────┘
              │                            │
              │ PEAT Protocol              │ PEAT Protocol
              │ (Mesh Sync)                │ (Mesh Sync)
              │                            │
              ▼                            ▼
┌──────────────────────────────────┐  ┌──────────────────────────────────┐
│      TEAM ALPHA (Network A)      │  │      TEAM BRAVO (Network B)      │
│                                  │  │                                  │
│  ┌────────────────────────────┐  │  │  ┌────────────────────────────┐  │
│  │ Alpha-1 (Operator)         │  │  │  │ Bravo-1 (Operator)         │  │
│  │ Squad Member aggregation   │  │  │  │ Squad Member aggregation   │  │
│  │  ┌──────────┐ ┌──────────┐ │  │  │  │  ┌──────────┐ ┌──────────┐ │  │
│  │  │ ATAK     │ │ Core2    │ │  │  │  │  │ ATAK     │ │ Core2    │ │  │
│  │  │ Phone    │ │ Wearable │ │  │  │  │  │ Phone    │ │ Wearable │ │  │
│  │  │ (Full)   │ │ (Lite)   │ │  │  │  │  │ (Full)   │ │ (Lite)   │ │  │
│  │  │ SA,comms │ │ IMU,mic  │ │  │  │  │  │ SA,comms │ │ IMU,mic  │ │  │
│  │  └──────────┘ └──────────┘ │  │  │  │  └──────────┘ └──────────┘ │  │
│  └────────────────────────────┘  │  │  └────────────────────────────┘  │
│                                  │  │                                  │
│  ┌────────────────────────────┐  │  │  ┌────────────────────────────┐  │
│  │ Alpha-2 (UGV Beast)        │  │  │  │ Bravo-2 (UGV Beast)        │  │
│  │ Platform aggregation       │  │  │  │ Platform aggregation       │  │
│  │  ┌──────────┐ ┌──────────┐ │  │  │  │  ┌──────────┐ ┌──────────┐ │  │
│  │  │ Jetson   │ │ ESP32    │ │  │  │  │  │ Jetson   │ │ ESP32    │ │  │
│  │  │ Orin     │ │ Lower    │ │  │  │  │  │ Orin     │ │ Lower    │ │  │
│  │  │ (Full)   │ │ (Lite)   │ │  │  │  │  │ (Full)   │ │ (Lite)   │ │  │
│  │  │ AI,nav   │ │ motors   │ │  │  │  │  │ AI,nav   │ │ motors   │ │  │
│  │  └──────────┘ └──────────┘ │  │  │  │  └──────────┘ └──────────┘ │  │
│  └────────────────────────────┘  │  │  └────────────────────────────┘  │
│                                  │  │                                  │
│  4 PEAT nodes → 2 entities       │  │  4 PEAT nodes → 2 entities       │
└──────────────────────────────────┘  └──────────────────────────────────┘

Legend:
  (Full) = PEAT-Full node: persistent storage, full CRDTs, relay capable
  (Lite) = PEAT-Lite node: ephemeral, primitive CRDTs, first-class mesh participant
```

### 3.1 Sub-Tier Architecture Detail

The vignette demonstrates PEAT's **hierarchical sub-tier architecture** where complex entities (humans with wearables, robots with multiple computers) aggregate multiple PEAT nodes into unified logical identities:

```
┌─────────────────────────────────────────────────────────────────────┐
│              Sub-Tier Aggregation Pattern                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Human Operator                      UGV Beast                       │
│  ┌─────────────────┐                 ┌─────────────────┐            │
│  │ Squad Member    │                 │ Platform        │            │
│  │ (logical ID)    │                 │ (logical ID)    │            │
│  └────────┬────────┘                 └────────┬────────┘            │
│           │                                   │                      │
│     ┌─────┴─────┐                       ┌─────┴─────┐               │
│     ▼           ▼                       ▼           ▼               │
│  ┌──────┐   ┌──────┐                ┌───────┐   ┌───────┐          │
│  │ ATAK │   │Core2 │                │Jetson │   │ ESP32 │          │
│  │Phone │   │      │                │Orin   │   │       │          │
│  │(Full)│   │(Lite)│                │(Full) │   │(Lite) │          │
│  └──────┘   └──────┘                └───────┘   └───────┘          │
│                                                                      │
│  Produces:                           Produces:                       │
│  • Position, heading                 • POI tracks                    │
│  • Activity level (IMU)              • Camera imagery                │
│  • Ambient noise (mic)               • LiDAR scans                   │
│  • Battery status                    • Motor telemetry               │
│  • Panic button events               • Obstacle detection            │
│  • Status acknowledgments            • Battery/power status          │
│                                                                      │
│  Aggregated Operator Doc:            Aggregated Platform Doc:        │
│  {                                   {                               │
│    "callsign": "Alpha-1",              "platform_id": "Alpha-2",    │
│    "position": {...},                  "tracks": [...],             │
│    "activity": "moving",               "position": {...},           │
│    "battery": 84,                      "heading": 45,               │
│    "alerts": [],                       "battery": 72,               │
│    "last_ack": "14:32:00Z"             "obstacles": [...]           │
│  }                                   }                               │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Key Insight**: PEAT-Lite nodes (Core2, ESP32) are **first-class mesh participants**, not bridged peripherals. They speak the same protocol as Full nodes but with capability negotiation:

- Full nodes: persistent storage, document CRDTs, relay capable
- Lite nodes: ephemeral, primitive CRDTs (LWW-Register, G-Counter), direct mesh participation

See [ADR-035: PEAT-Lite Embedded Sensor Nodes](../../docs/adr/035-peat-lite-embedded-nodes.md) for detailed architecture.

---

## 4. Use Case Flow

### Phase 1: Initialization & Capability Advertisement

**Time: T+0:00**

1. **Team Formation**
   - Alpha-1 powers on ATAK, joins Network A
   - Alpha-2 (UGV) boots, advertises sensor capabilities via PEAT
   - Alpha-3 (AI) advertises object-tracking model capability:
     ```json
     {
       "model_id": "object_tracker",
       "model_type": "detector_tracker",
       "model_version": "1.2.0",
       "model_hash": "sha256:a7f8b3c1...",
       "performance": {
         "precision": 0.91,
         "recall": 0.87,
         "fps": 15
       },
       "operational_status": "READY",
       "input_signature": ["video_stream", "640x480"],
       "output_signature": ["bounding_boxes", "track_ids", "classifications"]
     }
     ```

2. **Hierarchical Discovery**
   - Alpha team members discover each other via PEAT mesh discovery
   - Alpha-1 (human operator) elected team leader (authority weight)
   - Team capability aggregated: "1 camera, 1 object tracker v1.2.0, precision 0.91"

3. **Same process for Team Bravo on Network B**

4. **Coordinator Discovery**
   - Coordinator bridges both networks
   - Discovers Alpha team via Network A interface
   - Discovers Bravo team via Network B interface
   - Aggregates capabilities:
     ```
     Formation: Platoon-Level (2 teams)
     Total cameras: 2
     Total trackers: 2 (both v1.2.0)
     Coverage: Sectors A + B
     ```

5. **TAK Server Registration**
   - Coordinator's PEAT-TAK Bridge connects to TAK Server
   - Registers formation as TAK contact (MIL-STD-2525 symbol)
   - WebTAK (C2) sees unified platoon on map

### Phase 2: Mission Tasking

**Time: T+5:00**

1. **C2 Issues Track Command via TAK**
   - Commander in WebTAK creates "Track POI" mission
   - Specifies operational boundary (geofence)
   - Specifies POI description: "Adult male, blue jacket, carrying backpack"
   - TAK Server sends CoT mission command

2. **PEAT-TAK Bridge Translation**
   - Bridge receives CoT `<t-x-m>` mission tasking
   - Converts to PEAT command:
     ```json
     {
       "command_type": "TRACK_TARGET",
       "target_description": "Adult male, blue jacket, backpack",
       "operational_boundary": {
         "type": "polygon",
         "coordinates": [[...]]
       },
       "priority": "HIGH",
       "source_authority": "C2-Commander"
     }
     ```

3. **Command Propagation**
   - Command flows down hierarchy: C2 → Coordinator → Teams
   - Teams acknowledge receipt via PEAT sync
   - ATAK operators on each team see tasking on their displays

### Phase 3: Active Tracking — Team Alpha

**Time: T+10:00**

1. **POI Enters Alpha Sector**
   - Alpha-2 (UGV) camera captures POI
   - Video frames fed to Alpha-3 (AI model)

2. **AI Model Processing**
   - YOLOv8 detects person matching description
   - DeepSORT assigns track ID: `TRACK-001`
   - Bounding box + confidence: 0.89

3. **Track Advertisement (NOT raw video)**
   - Alpha-3 publishes track update via PEAT:
     ```json
     {
       "track_id": "TRACK-001",
       "classification": "person",
       "confidence": 0.89,
       "position": {
         "lat": 33.7749,
         "lon": -84.3958,
         "cep_m": 2.5
       },
       "velocity": {"bearing": 45, "speed_mps": 1.2},
       "attributes": {"jacket_color": "blue", "has_backpack": true},
       "source_platform": "Alpha-2",
       "source_model": "Alpha-3",
       "model_version": "1.2.0",
       "timestamp": "2025-11-26T14:10:00Z"
     }
     ```

4. **Hierarchical Aggregation**
   - Alpha team aggregates: "Tracking 1 POI, confidence HIGH"
   - Coordinator receives aggregated track (not raw video)
   - PEAT-TAK Bridge converts to CoT position event
   - TAK Server/WebTAK displays POI icon on map

**Bandwidth Comparison:**
- Traditional: 5 Mbps video stream
- PEAT: ~500 bytes per track update @ 2 Hz = ~1 Kbps
- **Reduction: 99.98%**

### Phase 4: Track Handoff — Alpha to Bravo

**Time: T+20:00**

1. **POI Approaches Sector Boundary**
   - Alpha-3 detects POI moving toward Bravo sector
   - Track includes predicted trajectory

2. **Handoff Initiation**
   - Coordinator detects overlap: POI entering Bravo coverage
   - Sends PREPARE_HANDOFF to Bravo team
   - Includes track history and POI description

3. **Bravo Acquires Track**
   - Bravo-2 (UAV) repositions based on predicted location
   - Bravo-3 (AI) searches for matching POI
   - Detection confirmed: matches description + trajectory

4. **Track Correlation**
   - Bravo-3 creates `TRACK-002` (local ID)
   - Coordinator correlates: `TRACK-001 == TRACK-002`
   - Unified track ID maintained: `TRACK-001`
   - Track continuity preserved across team boundary

5. **Alpha Releases Track**
   - Alpha-3 marks track as "HANDED_OFF"
   - Alpha-2 (UGV) reallocated to other coverage

### Phase 5: MLOps — Model Update Distribution

**Time: T+25:00**

C2 observes that tracking confidence drops in low-light conditions. MLOps team has a retrained model ready.

1. **Model Retraining (Pre-staged)**
   - MLOps server has retrained YOLOv8 with low-light augmentation
   - New model version: `1.3.0`
   - Performance improvement: precision 0.91 → 0.94 in low-light

2. **Model Package Creation**
   - MLOps creates PEAT model package:
     ```json
     {
       "package_type": "AI_MODEL_UPDATE",
       "model_id": "object_tracker",
       "model_version": "1.3.0",
       "model_hash": "sha256:b8d9c4e2...",
       "model_size_bytes": 45000000,
       "target_platforms": ["Alpha-3", "Bravo-3"],
       "deployment_policy": "ROLLING",
       "rollback_version": "1.2.0",
       "metadata": {
         "changelog": "Improved low-light detection",
         "training_date": "2025-11-26",
         "validation_accuracy": 0.94
       }
     }
     ```

3. **Downward Distribution via PEAT**
   - Model package pushed to Coordinator
   - Coordinator distributes to both teams via PEAT's **downward flow**
   - Content-addressed blob transfer (only sends delta if partial model cached)
   - QoS: Model update = Priority 5 (Bulk) — doesn't interrupt active tracking

4. **Rolling Deployment**
   - Alpha-3 receives model, validates hash
   - Alpha-3 hot-swaps model (brief tracking pause ~2 seconds)
   - Alpha-3 re-advertises capability:
     ```json
     {
       "model_id": "object_tracker",
       "model_version": "1.3.0",
       "performance": {
         "precision": 0.94,
         "recall": 0.89,
         "fps": 15
       },
       "operational_status": "READY"
     }
     ```
   - Bravo-3 follows same process

5. **Capability Re-Aggregation**
   - Coordinator sees both teams now on v1.3.0
   - Aggregated capability updated:
     ```
     Formation: Platoon-Level
     Trackers: 2 (both v1.3.0)
     Performance: precision 0.94 (improved)
     ```
   - WebTAK commander notified: "Model update complete"

6. **Improved Tracking Continues**
   - Bravo-3 resumes tracking with improved low-light performance
   - Track confidence increases from 0.82 → 0.91 in shadowed area

### Phase 6: Mission Completion

**Time: T+35:00**

1. **POI Exits Operational Boundary**
   - Bravo-3 detects POI leaving geofence
   - Track status: "EXITED_AOI"

2. **Mission Summary**
   - Coordinator aggregates mission statistics:
     ```
     Track Duration: 25 minutes
     Handoffs: 1 (Alpha → Bravo)
     Track Continuity: 100%
     Average Confidence: 0.88
     Model Updates: 1 (v1.2.0 → v1.3.0)
     
     Data Transmitted:
       Track Updates:    47 KB
       Model Update:     45 MB (compressed, P5 bulk)
       Total:            ~45 MB
     
     Traditional Approach Would Have Used:
       Video Streams:    938 MB (25 min × 2 cameras × 5 Mbps)
     
     PEAT Savings:       95% bandwidth reduction
     (Even with full model push)
     ```

3. **TAK Update**
   - WebTAK shows track complete
   - Mission logged with full audit trail
   - Model deployment logged for compliance

---

## 5. PEAT Protocol Features Demonstrated

### 5.1 Hierarchical Capability Aggregation

```
C2 Level View:
├─ Platoon has "object tracking" capability
├─ 2 active tracks, 0 lost tracks
├─ Model version: v1.3.0 (all platforms)
└─ Coverage: 100% of operational boundary

Coordinator View:
├─ Alpha Team: 1 tracker v1.3.0, sector A, confidence HIGH
├─ Bravo Team: 1 tracker v1.3.0, sector B, confidence HIGH
└─ Handoff capability: READY

Team View:
├─ UGV: camera operational, position known
├─ AI Model: tracker v1.3.0, 15 FPS, precision 0.94
└─ Operator: supervising, authority COMMANDER
```

### 5.2 Bidirectional Hierarchical Flows

| Direction | Data Type | Example |
|-----------|-----------|---------|
| **Upward** | Track updates | POI position, confidence |
| **Upward** | Capability advertisements | Model version, performance |
| **Upward** | Health/status | Platform operational state |
| **Downward** | Mission commands | Track target tasking |
| **Downward** | Model updates | YOLOv8 v1.3.0 package |
| **Downward** | Configuration | Detection thresholds |

### 5.3 Human-Machine Authority Composition

| Decision | Authority |
|----------|-----------|
| Approve track target assignment | Human operator |
| Adjust camera aim | AI + UGV autonomous |
| Initiate handoff | Coordinator (delegated) |
| Approve model deployment | C2 (MLOps) |
| Abort mission | Human operator or C2 |
| Lethal engagement (if applicable) | Human only (ROE) |

### 5.4 Edge MLOps via PEAT

| Capability | Description |
|------------|-------------|
| Model versioning | Content-addressed (SHA256 hash) |
| Deployment policy | Rolling, canary, or immediate |
| Rollback support | Previous version cached locally |
| Performance tracking | Runtime metrics vs. design specs |
| Capability re-advertisement | Automatic after model swap |

### 5.5 QoS Data Prioritization

| Data Type | Priority | Latency Target |
|-----------|----------|----------------|
| Track updates | P1 - Critical | < 1 second |
| Handoff commands | P1 - Critical | < 500 ms |
| Capability status | P2 - High | < 5 seconds |
| AI model metrics | P3 - Normal | < 30 seconds |
| Model updates | P5 - Bulk | Best effort (background) |

---

## 6. Success Criteria

### 6.1 Functional Requirements

| ID | Requirement | Validation Method |
|----|-------------|-------------------|
| F1 | Teams form and advertise capabilities within 30 seconds | Automated test |
| F2 | C2 can issue track command via WebTAK | End-to-end test |
| F3 | Tracks appear on WebTAK within 2 seconds of detection | Latency measurement |
| F4 | Track handoff completes with < 5 second gap | Continuity analysis |
| F5 | Model update distributes to all platforms within 5 minutes | Timing measurement |
| F6 | Platforms re-advertise capability after model update | Log verification |
| F7 | Mission summary available in WebTAK at completion | Verification |

### 6.2 Performance Requirements

| ID | Metric | Target | Validation |
|----|--------|--------|------------|
| P1 | Track update latency (edge → C2) | < 2 seconds | Timestamped messages |
| P2 | Bandwidth usage (tracking only) | < 10 Kbps | Traffic capture |
| P3 | Handoff detection accuracy | > 95% | Ground truth comparison |
| P4 | Model distribution time (45 MB) | < 5 minutes | Timing measurement |
| P5 | System operates on 500 Kbps link | Yes | Network emulation |

### 6.3 MLOps Requirements

| ID | Requirement | Validation |
|----|-------------|------------|
| M1 | Model hash verified before deployment | Hash check log |
| M2 | Rolling deployment doesn't interrupt tracking > 5s | Gap analysis |
| M3 | Capability re-advertised within 10s of model swap | Log timing |
| M4 | Rollback possible if deployment fails | Inject failure test |

---

## 7. Hardware & Software Requirements

### 7.1 Team Equipment (per team)

| Component | Model | Specification | Est. Cost |
|-----------|-------|---------------|-----------|
| Operator Phone | Android + ATAK | PEAT-Full node, SA/comms | $300-500 |
| Operator Wearable | M5Stack Core2 | PEAT-Lite node, ESP32, 6-axis IMU, mic, touch, RTC | $50 |
| UGV Platform | Waveshare UGV Beast | Tracked chassis, 360° pan-tilt, 5MP camera | $400 |
| UGV Compute | Jetson Orin Nano | PEAT-Full node, AI/vision, ROS2, 4GB+ RAM | $200-500 |
| UGV Controller | ESP32 (included) | PEAT-Lite node, motor PID, sensor fusion | (included) |
| Local network | WiFi router or mesh radio | Team connectivity | $50-200 |

**Per-Team Total**: ~$1,000-1,700

**M5Stack Core2 Sensor Capabilities (PEAT-Lite):**
| Sensor | Chip | CRDT Mapping | Data |
|--------|------|--------------|------|
| 6-axis IMU | MPU6886 | LWW-Register | Orientation, activity, fall detection |
| Microphone | SPM1423 | LWW-Register | Ambient noise, acoustic events |
| Touch | FT6336U | G-Counter | Panic button, acknowledgments |
| RTC | BM8563 | (clock sync) | Accurate timestamps |
| Battery | AXP192 | LWW-Register | Device health |

**Waveshare UGV Beast Capabilities:**
| Component | Specification | Data |
|-----------|---------------|------|
| Camera | 5MP, 160° wide-angle | Video frames for AI |
| Pan-Tilt | 360° horizontal, servo-driven | Camera positioning |
| LiDAR | (optional addon) | Obstacle detection, mapping |
| Chassis | Tracked, all-terrain | Mobility |
| ESP32 | Dual-core 240MHz | Motor PID, IMU, proximity |

### 7.2 Coordinator Equipment

| Component | Specification | Est. Cost |
|-----------|---------------|-----------|
| Bridge device | Laptop with dual network interfaces | $1000-2000 |
| Network adapters | 2x USB WiFi or 1 WiFi + 1 LTE modem | $50-150 |
| ATAK device | Android phone/tablet | $300-500 |

### 7.3 C2 Equipment

| Component | Specification | Est. Cost |
|-----------|---------------|-----------|
| TAK Server | TAK Server on Linux VM/container | Per TAK licensing |
| WebTAK | Browser-based (no additional HW) | $0 |
| MLOps Server | GPU-equipped training server | $2000-5000 (or cloud) |

### 7.4 Software Components

| Component | Description | Source |
|-----------|-------------|--------|
| PEAT Protocol | Core sync + capability advertisement | (r)evolve |
| PEAT-TAK Bridge | CoT ↔ PEAT translation | (r)evolve |
| PEAT MLOps Agent | Model distribution + hot-swap | (r)evolve |
| Object Tracker | YOLOv8 + DeepSORT | Open source + custom |
| ATAK | Tactical display | Government distribution |
| TAK Server | Official TAK server | Government distribution |
| WebTAK | Browser-based TAK client | Government distribution |

---

## 8. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Network latency causes handoff gaps | Medium | Medium | Pre-positioned handoff, trajectory prediction |
| AI model misses POI | Low | High | Redundant cameras, manual operator override |
| TAK integration issues | Medium | Medium | Extensive CoT format testing |
| Model update interrupts tracking | Medium | Medium | Rolling deployment, brief pause acceptable |
| Model update fails mid-transfer | Low | Medium | Resumable transfer, hash verification |
| Cross-network sync fails | Low | High | Store-and-forward in bridge |

---

## 9. Extension Scenarios

### 9.1 Multi-POI Tracking
- Add 2-3 additional POIs
- Demonstrate parallel tracking
- Show capability-based task allocation

### 9.2 Federated Learning
- Collect inference results from both teams
- Retrain model with edge-collected data
- Redistribute improved model

### 9.3 Contested Network
- Inject packet loss (30%)
- Demonstrate graceful degradation
- Show QoS prioritization (tracks before model updates)

### 9.4 Model Rollback
- Push faulty model (simulated performance drop)
- Automatic detection of degradation
- Rollback to previous version

### 9.5 Heterogeneous Models
- Alpha runs YOLOv8, Bravo runs different model
- Demonstrate capability advertisement differences
- Task allocation based on model strengths

---

## Appendix A: Message Schemas

### A.1 Track Update Message (PEAT)

```json
{
  "track_id": "TRACK-001",
  "classification": "person",
  "confidence": 0.89,
  "position": { "lat": 33.7749, "lon": -84.3958, "cep_m": 2.5 },
  "velocity": { "bearing": 45, "speed_mps": 1.2 },
  "attributes": { "jacket_color": "blue", "has_backpack": true },
  "source_platform": "Alpha-2",
  "source_model": "Alpha-3",
  "model_version": "1.3.0",
  "timestamp": "2025-11-26T14:10:00Z"
}
```

### A.2 Model Update Package (PEAT)

```json
{
  "package_type": "AI_MODEL_UPDATE",
  "model_id": "object_tracker",
  "model_version": "1.3.0",
  "model_hash": "sha256:b8d9c4e2f1a3...",
  "model_size_bytes": 45000000,
  "blob_reference": "peat://blobs/sha256:b8d9c4e2f1a3...",
  "target_platforms": ["Alpha-3", "Bravo-3"],
  "deployment_policy": "ROLLING",
  "rollback_version": "1.2.0",
  "metadata": {
    "changelog": "Improved low-light detection",
    "training_date": "2025-11-26",
    "validation_accuracy": 0.94
  }
}
```

### A.3 Capability Advertisement (PEAT)

```json
{
  "platform_id": "Alpha-3",
  "advertised_at": "2025-11-26T14:25:00Z",
  "models": [{
    "model_id": "object_tracker",
    "model_version": "1.3.0",
    "model_hash": "sha256:b8d9c4e2f1a3...",
    "model_type": "detector_tracker",
    "performance": {
      "precision": 0.94,
      "recall": 0.89,
      "fps": 15,
      "latency_ms": 67
    },
    "operational_status": "READY"
  }],
  "resources": {
    "gpu_utilization": 0.65,
    "memory_used_mb": 2048,
    "memory_total_mb": 4096
  }
}
```

### A.4 Track CoT Event (TAK)

```xml
<event uid="TRACK-001" type="a-f-G-E-S" time="2025-11-26T14:10:00Z" 
       start="2025-11-26T14:10:00Z" stale="2025-11-26T14:15:00Z" how="m-g">
  <point lat="33.7749" lon="-84.3958" hae="0" ce="2.5" le="1"/>
  <detail>
    <track course="45" speed="1.2"/>
    <remarks>Adult male, blue jacket, backpack (89% confidence)</remarks>
    <link uid="Alpha-2" type="a-f-G-U-C" relation="p-p"/>
    <_peat_ model_version="1.3.0"/>
  </detail>
</event>
```

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| ATAK | Android Tactical Assault Kit - Mobile situational awareness app |
| CoT | Cursor on Target - XML message format for SA data |
| CRDT | Conflict-free Replicated Data Type - Data structures that merge without conflicts |
| G-Counter | Grow-only Counter - CRDT that only increments (used for event counts) |
| PEAT | Hierarchical Information and Value Exchange Protocol |
| PEAT-Full | Full-capability PEAT node with persistent storage, document CRDTs, relay capability |
| PEAT-Lite | Resource-constrained PEAT node with ephemeral storage, primitive CRDTs, first-class mesh participation |
| LWW-Register | Last-Writer-Wins Register - CRDT where latest timestamp wins (used for sensor readings) |
| M5Stack Core2 | ESP32-based development board with IMU, microphone, touch, RTC, display |
| MLOps | Machine Learning Operations - Model lifecycle management |
| POI | Person of Interest - Target being tracked |
| ROS2 | Robot Operating System 2 - Middleware for robot software development |
| Squad Member | Logical aggregation tier for human operator (aggregates ATAK + wearables) |
| Sub-tier | Hierarchical level below Squad where multiple devices aggregate into single entity |
| TAK | Team Awareness Kit - SA ecosystem |
| UGV | Unmanned Ground Vehicle |
| UGV Beast | Waveshare tracked robot platform with Jetson compute and ESP32 controller |
| UAV | Unmanned Aerial Vehicle |
| WebTAK | Browser-based TAK client |

---

## Appendix C: References

- [ADR-035: PEAT-Lite Embedded Sensor Nodes](../../docs/adr/035-peat-lite-embedded-nodes.md)
- [M5Stack Core2 Specifications](https://docs.m5stack.com/en/core/core2)
- [Waveshare UGV Beast](https://www.waveshare.com/ugv-beast.htm)
- [NVIDIA Jetson Orin Nano](https://developer.nvidia.com/embedded/jetson-orin-nano-developer-kit)

---

*Document End*
