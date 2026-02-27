# PEAT Protocol - System Architecture & Capability Flow

## 1. Physical Network Topology

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                    C2 ELEMENT                                            │
│                                                                                          │
│   ┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐           │
│   │   TAK Server    │◄───────►│     WebTAK      │         │  MLOps Server   │           │
│   │   (CoT Hub)     │  HTTP   │  (Commander UI) │         │ (Model Training)│           │
│   └────────┬────────┘         └─────────────────┘         └────────┬────────┘           │
│            │                                                        │                    │
│            │ CoT/TCP                                    Model Packages                   │
└────────────┼────────────────────────────────────────────────────────┼────────────────────┘
             │                                                        │
             ▼                                                        ▼
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                              COORDINATOR NODE (Bridge)                                   │
│                                                                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────────┐   │
│   │                            PEAT-TAK Bridge                                       │   │
│   │  • Aggregates capabilities from both teams                                       │   │
│   │  • Translates PEAT ↔ CoT                                                        │   │
│   │  • Routes model updates downward                                                 │   │
│   └─────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                          │
│            ┌──────────────────┐              ┌──────────────────┐                        │
│            │   Network A IF   │              │   Network B IF   │                        │
│            └────────┬─────────┘              └────────┬─────────┘                        │
└─────────────────────┼────────────────────────────────┼──────────────────────────────────┘
                      │                                │
                      │ PEAT Protocol                  │ PEAT Protocol
                      ▼                                ▼
┌────────────────────────────────────┐  ┌────────────────────────────────────┐
│         TEAM ALPHA (Net A)         │  │         TEAM BRAVO (Net B)         │
│                                    │  │                                    │
│  ┌──────────┐  ┌──────────┐       │  │  ┌──────────┐  ┌──────────┐       │
│  │ Alpha-1  │  │ Alpha-2  │       │  │  │ Bravo-1  │  │ Bravo-2  │       │
│  │ Operator │  │   UGV    │       │  │  │ Operator │  │   UAV    │       │
│  │  (ATAK)  │  │ (Camera) │       │  │  │  (ATAK)  │  │ (Camera) │       │
│  └──────────┘  └────┬─────┘       │  │  └──────────┘  └────┬─────┘       │
│                     │ video       │  │                     │ video       │
│                     ▼             │  │                     ▼             │
│               ┌──────────┐        │  │               ┌──────────┐        │
│               │ Alpha-3  │        │  │               │ Bravo-3  │        │
│               │   AI     │        │  │               │   AI     │        │
│               │ (Jetson) │        │  │               │ (Jetson) │        │
│               └──────────┘        │  │               └──────────┘        │
└────────────────────────────────────┘  └────────────────────────────────────┘
```

---

## 2. Capability Advertisement Flow (Upward)

This diagram shows how capabilities bubble up through the hierarchy, aggregating at each level until C2 has a complete picture of available capabilities to inform tasking decisions.

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                          │
│                              C2 ELEMENT (WebTAK)                                         │
│                                                                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────────┐   │
│   │                      AGGREGATED FORMATION CAPABILITY                             │   │
│   │                                                                                  │   │
│   │   Formation: "Platoon" (2 teams)                                                │   │
│   │   ┌────────────────────────────────────────────────────────────────────────┐    │   │
│   │   │  OBJECT_TRACKING: AVAILABLE                                            │    │   │
│   │   │    • Coverage: Sectors A + B                                           │    │   │
│   │   │    • Platforms: 2 (Alpha-3, Bravo-3)                                   │    │   │
│   │   │    • Model: YOLOv8 v1.2.0                                              │    │   │
│   │   │    • Aggregate Precision: 0.91                                         │    │   │
│   │   │    • Status: READY                                                     │    │   │
│   │   └────────────────────────────────────────────────────────────────────────┘    │   │
│   │   ┌────────────────────────────────────────────────────────────────────────┐    │   │
│   │   │  SENSOR_COVERAGE: AVAILABLE                                            │    │   │
│   │   │    • Cameras: 2 (UGV + UAV)                                            │    │   │
│   │   │    • Combined FOV: 180°                                                │    │   │
│   │   └────────────────────────────────────────────────────────────────────────┘    │   │
│   │   ┌────────────────────────────────────────────────────────────────────────┐    │   │
│   │   │  HUMAN_SUPERVISION: AVAILABLE                                          │    │   │
│   │   │    • Operators: 2 (Alpha-1, Bravo-1)                                   │    │   │
│   │   │    • Authority: APPROVE_TRACK, ABORT_MISSION                           │    │   │
│   │   └────────────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                                  │   │
│   │   Commander sees: "I have object tracking capability across both sectors        │   │
│   │                    with human supervision. I can task a POI track mission."     │   │
│   │                                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                          │
│                                          ▲                                               │
│                                          │ Aggregated capabilities                       │
│                                          │ via PEAT-TAK Bridge                          │
└──────────────────────────────────────────┼──────────────────────────────────────────────┘
                                           │
┌──────────────────────────────────────────┼──────────────────────────────────────────────┐
│                                          │                                               │
│                              COORDINATOR NODE                                            │
│                                                                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────────┐   │
│   │                      AGGREGATED TEAM CAPABILITIES                                │   │
│   │                                                                                  │   │
│   │   Teams Online: 2                                                               │   │
│   │                                                                                  │   │
│   │   Alpha Team:                        Bravo Team:                                │   │
│   │   ┌─────────────────────────┐       ┌─────────────────────────┐                │   │
│   │   │ OBJECT_TRACKING         │       │ OBJECT_TRACKING         │                │   │
│   │   │   Model: v1.2.0         │       │   Model: v1.2.0         │                │   │
│   │   │   Precision: 0.91       │       │   Precision: 0.91       │                │   │
│   │   │   FPS: 15               │       │   FPS: 15               │                │   │
│   │   │   Status: READY         │       │   Status: READY         │                │   │
│   │   │   Sector: A             │       │   Sector: B             │                │   │
│   │   ├─────────────────────────┤       ├─────────────────────────┤                │   │
│   │   │ CAMERA                  │       │ CAMERA                  │                │   │
│   │   │   Platform: UGV         │       │   Platform: UAV         │                │   │
│   │   │   Resolution: 1080p     │       │   Resolution: 1080p     │                │   │
│   │   │   Status: READY         │       │   Status: READY         │                │   │
│   │   ├─────────────────────────┤       ├─────────────────────────┤                │   │
│   │   │ OPERATOR                │       │ OPERATOR                │                │   │
│   │   │   Authority: SUPERVISOR │       │   Authority: SUPERVISOR │                │   │
│   │   │   Status: ONLINE        │       │   Status: ONLINE        │                │   │
│   │   └─────────────────────────┘       └─────────────────────────┘                │   │
│   │                                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                          │
│                          ▲                               ▲                               │
│                          │                               │                               │
│                          │ Team Alpha                    │ Team Bravo                    │
│                          │ capabilities                  │ capabilities                  │
│                          │ (via Network A)               │ (via Network B)               │
└──────────────────────────┼───────────────────────────────┼──────────────────────────────┘
                           │                               │
┌──────────────────────────┼───────────────┐ ┌─────────────┼──────────────────────────────┐
│                          │               │ │             │                              │
│   TEAM ALPHA (Net A)     │               │ │             │        TEAM BRAVO (Net B)   │
│                          │               │ │             │                              │
│   ┌──────────────────────┴─────┐         │ │    ┌────────┴───────────────────────┐     │
│   │   TEAM CAPABILITY SUMMARY  │         │ │    │   TEAM CAPABILITY SUMMARY      │     │
│   │                            │         │ │    │                                │     │
│   │   Leader: Alpha-1 (Human)  │         │ │    │   Leader: Bravo-1 (Human)      │     │
│   │   Platforms: 3             │         │ │    │   Platforms: 3                 │     │
│   │   Capabilities:            │         │ │    │   Capabilities:                │     │
│   │     • Object Tracking ✓    │         │ │    │     • Object Tracking ✓        │     │
│   │     • Camera Sensor ✓      │         │ │    │     • Camera Sensor ✓          │     │
│   │     • Human Authority ✓    │         │ │    │     • Human Authority ✓        │     │
│   └────────────────────────────┘         │ │    └────────────────────────────────┘     │
│                  ▲                       │ │                    ▲                      │
│                  │ aggregate             │ │                    │ aggregate            │
│     ┌────────────┼────────────┐          │ │       ┌────────────┼────────────┐         │
│     │            │            │          │ │       │            │            │         │
│     ▲            ▲            ▲          │ │       ▲            ▲            ▲         │
│ ┌───┴───┐   ┌────┴────┐  ┌────┴────┐    │ │   ┌───┴───┐   ┌────┴────┐  ┌────┴────┐   │
│ │Alpha-1│   │ Alpha-2 │  │ Alpha-3 │    │ │   │Bravo-1│   │ Bravo-2 │  │ Bravo-3 │   │
│ │       │   │         │  │         │    │ │   │       │   │         │  │         │   │
│ │OPERATOR│  │ UGV     │  │ AI MODEL│    │ │   │OPERATOR│  │ UAV     │  │ AI MODEL│   │
│ │       │   │ CAMERA  │  │ TRACKER │    │ │   │       │   │ CAMERA  │  │ TRACKER │   │
│ │Author-│   │         │  │         │    │ │   │Author-│   │         │  │         │   │
│ │ity:   │   │Resol:   │  │Model:   │    │ │   │ity:   │   │Resol:   │  │Model:   │   │
│ │SUPER- │   │1080p    │  │v1.2.0   │    │ │   │SUPER- │   │1080p    │  │v1.2.0   │   │
│ │VISOR  │   │FOV: 90° │  │Prec:0.91│    │ │   │VISOR  │   │FOV: 90° │  │Prec:0.91│   │
│ │       │   │Status:  │  │FPS: 15  │    │ │   │       │   │Status:  │  │FPS: 15  │   │
│ │Status:│   │READY    │  │Status:  │    │ │   │Status:│   │READY    │  │Status:  │   │
│ │ONLINE │   │         │  │READY    │    │ │   │ONLINE │   │         │  │READY    │   │
│ └───────┘   └─────────┘  └─────────┘    │ │   └───────┘   └─────────┘  └─────────┘   │
│                                          │ │                                          │
│  Each platform advertises its own        │ │  Each platform advertises its own        │
│  capabilities via PEAT Protocol          │ │  capabilities via PEAT Protocol          │
│                                          │ │                                          │
└──────────────────────────────────────────┘ └──────────────────────────────────────────┘
```

---

## 3. Capability-Informed Tasking Flow

This shows how C2's view of aggregated capabilities **directly informs** the decision to task.

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                          │
│   STEP 1: C2 RECEIVES CAPABILITY PICTURE                                                │
│                                                                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────────┐   │
│   │  WebTAK Dashboard                                                                │   │
│   │                                                                                  │   │
│   │  ┌─────────────────────────────────────────────────────────────────────────┐    │   │
│   │  │  FORMATION CAPABILITIES                               [Auto-Updated]    │    │   │
│   │  │                                                                         │    │   │
│   │  │  ╔═══════════════════════════════════════════════════════════════════╗  │    │   │
│   │  │  ║  ✓ OBJECT_TRACKING    Ready   2 platforms   Sectors A+B          ║  │    │   │
│   │  │  ║  ✓ VIDEO_SENSOR       Ready   2 cameras     UGV + UAV            ║  │    │   │
│   │  │  ║  ✓ HUMAN_OVERSIGHT    Ready   2 operators   Both supervised      ║  │    │   │
│   │  │  ║  ✗ SIGNALS_INTEL      N/A     0 platforms   Not available        ║  │    │   │
│   │  │  ║  ✗ STRIKE             N/A     0 platforms   Not available        ║  │    │   │
│   │  │  ╚═══════════════════════════════════════════════════════════════════╝  │    │   │
│   │  │                                                                         │    │   │
│   │  │  Commander thinks: "I have tracking capability. I can task this."       │    │   │
│   │  └─────────────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                          │
└─────────────────────────────────────────────────────────────────────────────────────────┘
                                           │
                                           │ Commander decides to task
                                           ▼
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                          │
│   STEP 2: C2 ISSUES TASK (Informed by Capabilities)                                     │
│                                                                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────────┐   │
│   │  WebTAK - Create Mission                                                         │   │
│   │                                                                                  │   │
│   │  Mission Type: [ TRACK_TARGET ▼ ]  ◄── Only shows options matching capabilities │   │
│   │                                                                                  │   │
│   │  Target Description: [ Adult male, blue jacket, backpack ]                      │   │
│   │                                                                                  │   │
│   │  Operational Boundary: [ Draw on map... ]                                       │   │
│   │                                                                                  │   │
│   │  Assign To: [ ✓ Alpha Team (Sector A)  ]  ◄── Shows capable teams only          │   │
│   │             [ ✓ Bravo Team (Sector B)  ]                                        │   │
│   │                                                                                  │   │
│   │  Required Capabilities:                                                          │   │
│   │    [✓] Object Tracking  (2/2 teams have this)                                   │   │
│   │    [✓] Human Oversight  (2/2 teams have this)                                   │   │
│   │                                                                                  │   │
│   │                                              [ SEND TASKING ]                    │   │
│   │                                                                                  │   │
│   └─────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                          │
└─────────────────────────────────────────────────────────────────────────────────────────┘
                                           │
                                           │ Tasking sent via CoT → PEAT
                                           ▼
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                          │
│   STEP 3: TASKING FLOWS DOWN TO CAPABLE TEAMS                                           │
│                                                                                          │
│                              ┌─────────────────┐                                        │
│                              │    WebTAK       │                                        │
│                              │   (C2)          │                                        │
│                              └────────┬────────┘                                        │
│                                       │                                                 │
│                                       │ CoT: TRACK_TARGET                               │
│                                       ▼                                                 │
│                              ┌─────────────────┐                                        │
│                              │   TAK Server    │                                        │
│                              └────────┬────────┘                                        │
│                                       │                                                 │
│                                       │ CoT → PEAT translation                          │
│                                       ▼                                                 │
│                              ┌─────────────────┐                                        │
│                              │  Coordinator    │                                        │
│                              │  PEAT-TAK Bridge│                                        │
│                              └────────┬────────┘                                        │
│                                       │                                                 │
│                    ┌──────────────────┴──────────────────┐                              │
│                    │                                     │                              │
│                    │ PEAT: TRACK_TARGET                  │ PEAT: TRACK_TARGET           │
│                    ▼                                     ▼                              │
│           ┌─────────────────┐                   ┌─────────────────┐                     │
│           │   Team Alpha    │                   │   Team Bravo    │                     │
│           │                 │                   │                 │                     │
│           │  Alpha-1: ✓     │                   │  Bravo-1: ✓     │                     │
│           │  (acknowledges) │                   │  (acknowledges) │                     │
│           │                 │                   │                 │                     │
│           │  Alpha-3: ✓     │                   │  Bravo-3: ✓     │                     │
│           │  (begins scan)  │                   │  (begins scan)  │                     │
│           └─────────────────┘                   └─────────────────┘                     │
│                                                                                          │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Complete Bidirectional Flow Summary

```
                    ╔═══════════════════════════════════════════════════════════════╗
                    ║                        C2 / WebTAK                             ║
                    ║                                                                ║
                    ║   Sees: Aggregated capabilities from entire formation          ║
                    ║   Does: Tasks missions based on available capabilities         ║
                    ║                                                                ║
                    ╚═══════════════════════════╤═══════════════════════════════════╝
                                                │
                         ┌──────────────────────┴──────────────────────┐
                         │                                             │
                         ▼                                             ▼
          ╔══════════════════════════════╗          ╔══════════════════════════════╗
          ║       UPWARD FLOW            ║          ║       DOWNWARD FLOW          ║
          ║     (Capabilities)           ║          ║      (Decisions)             ║
          ╠══════════════════════════════╣          ╠══════════════════════════════╣
          ║                              ║          ║                              ║
          ║  • Platform capabilities     ║          ║  • Mission tasking           ║
          ║    - AI model version        ║          ║    - Track target commands   ║
          ║    - Model performance       ║          ║    - Operational boundaries  ║
          ║    - Operational status      ║          ║                              ║
          ║                              ║          ║  • Model updates             ║
          ║  • Aggregated team caps      ║          ║    - New model packages      ║
          ║    - Combined coverage       ║          ║    - Deployment policies     ║
          ║    - Available functions     ║          ║                              ║
          ║                              ║          ║  • Configuration             ║
          ║  • Track updates             ║          ║    - Detection thresholds    ║
          ║    - POI position            ║          ║    - Priority settings       ║
          ║    - Confidence              ║          ║                              ║
          ║    - Source model version    ║          ║                              ║
          ║                              ║          ║                              ║
          ╚══════════════════════════════╝          ╚══════════════════════════════╝
                         │                                             │
                         │                                             │
                         ▼                                             ▼
                    ╔═══════════════════════════════════════════════════════════════╗
                    ║                      COORDINATOR                               ║
                    ║                                                                ║
                    ║   Aggregates: Team capabilities → Formation capability         ║
                    ║   Routes: Commands down, capabilities up                       ║
                    ║   Bridges: Network A ↔ Network B                              ║
                    ║                                                                ║
                    ╚═══════════════════════════╤═══════════════════════════════════╝
                                                │
                         ┌──────────────────────┴──────────────────────┐
                         │                                             │
                         ▼                                             ▼
                    ╔═══════════════════╗                    ╔═══════════════════╗
                    ║    TEAM ALPHA     ║                    ║    TEAM BRAVO     ║
                    ║    (Network A)    ║                    ║    (Network B)    ║
                    ║                   ║                    ║                   ║
                    ║  Advertises:      ║                    ║  Advertises:      ║
                    ║  • Operator ✓     ║                    ║  • Operator ✓     ║
                    ║  • UGV Camera ✓   ║                    ║  • UAV Camera ✓   ║
                    ║  • AI Tracker ✓   ║                    ║  • AI Tracker ✓   ║
                    ║    v1.2.0         ║                    ║    v1.2.0         ║
                    ║    prec: 0.91     ║                    ║    prec: 0.91     ║
                    ║                   ║                    ║                   ║
                    ║  Receives:        ║                    ║  Receives:        ║
                    ║  • Track tasks    ║                    ║  • Track tasks    ║
                    ║  • Model updates  ║                    ║  • Model updates  ║
                    ╚═══════════════════╝                    ╚═══════════════════════╝
```

---

## 5. Key Insight: Capability-Driven Operations

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                          │
│   TRADITIONAL APPROACH                      PEAT APPROACH                               │
│   ────────────────────                      ─────────────                               │
│                                                                                          │
│   C2: "Team Alpha, do you have             C2 already knows:                            │
│        tracking capability?"                                                             │
│                                             ┌────────────────────────────┐              │
│   Alpha: "Yes, we have YOLOv8"             │ Alpha: OBJECT_TRACKING ✓   │              │
│                                             │   Model: v1.2.0            │              │
│   C2: "What's your precision?"             │   Precision: 0.91          │              │
│                                             │   FPS: 15                  │              │
│   Alpha: "91%"                              │   Status: READY            │              │
│                                             │   Sector: A                │              │
│   C2: "OK, track this target"              └────────────────────────────┘              │
│                                                                                          │
│   ───────────────────────────              ──────────────────────────────               │
│                                                                                          │
│   • Multiple back-and-forth                • Zero queries needed                        │
│   • Stale information                      • Real-time capability updates               │
│   • Manual capability tracking             • Automatic aggregation                      │
│   • C2 doesn't know what's                 • C2 sees full picture before               │
│     available until asked                    deciding to task                           │
│                                                                                          │
│   "Can you do this?"                       "I know you can do this."                    │
│                                                                                          │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Capability Aggregation Example

```
PLATFORM LEVEL                    TEAM LEVEL                      FORMATION LEVEL
─────────────────                 ──────────                      ───────────────

Alpha-3 advertises:               Alpha Team aggregates:          Coordinator aggregates:
┌─────────────────────┐           ┌─────────────────────┐         ┌─────────────────────┐
│ model_id:           │           │ team_id: Alpha      │         │ formation: Platoon  │
│   "object_tracker"  │           │                     │         │                     │
│ model_version:      │    ──►    │ capabilities:       │   ──►   │ capabilities:       │
│   "1.2.0"           │           │   OBJECT_TRACKING:  │         │   OBJECT_TRACKING:  │
│ precision: 0.91     │           │     platforms: 1    │         │     teams: 2        │
│ fps: 15             │           │     precision: 0.91 │         │     platforms: 2    │
│ status: READY       │           │     status: READY   │         │     coverage: A+B   │
└─────────────────────┘           │                     │         │     precision: 0.91 │
                                  │   CAMERA:           │         │     status: READY   │
Alpha-2 advertises:               │     platforms: 1    │         │                     │
┌─────────────────────┐           │     type: UGV       │         │   CAMERA:           │
│ sensor_type: camera │    ──►    │     status: READY   │         │     platforms: 2    │
│ resolution: 1080p   │           │                     │         │     types: UGV+UAV  │
│ fov: 90°            │           │   HUMAN_AUTHORITY:  │         │     status: READY   │
│ status: READY       │           │     operator: 1     │         │                     │
└─────────────────────┘           │     level: SUPER    │         │   HUMAN_AUTHORITY:  │
                                  └─────────────────────┘         │     operators: 2    │
Alpha-1 advertises:                                               │     level: SUPER    │
┌─────────────────────┐           (Bravo Team similar)            └─────────────────────┘
│ role: OPERATOR      │                    │                               │
│ authority: SUPER    │                    │                               │
│ status: ONLINE      │                    └───────────────────────────────┘
└─────────────────────┘                                                    │
                                                                           ▼
                                                                   ┌─────────────────────┐
                                                                   │ C2 (WebTAK) sees:   │
                                                                   │                     │
                                                                   │ "Formation has      │
                                                                   │  object tracking    │
                                                                   │  across sectors A+B │
                                                                   │  with 91% precision │
                                                                   │  and human super-   │
                                                                   │  vision on both     │
                                                                   │  teams. Ready to    │
                                                                   │  task."             │
                                                                   └─────────────────────┘
```

---

*This is the PEAT value proposition: C2 knows what capabilities exist before deciding to task, eliminating the query overhead and enabling truly capability-driven operations.*
