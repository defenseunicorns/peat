# ADR-053: HIVE Operational Viewer — Real-Time Simulation Visualization

**Status**: Proposed
**Date**: 2026-02-07
**Authors**: Kit Plummer, Claude
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Related ADRs**:
- [ADR-030](051-port-operations-reference-implementation.md) (Port Operations Reference Implementation)
- [ADR-031](031-hive-commander-game.md) (HIVE Commander — Tactical Capability RPG)
- [ADR-043](043-consumer-interface-adapters.md) (Consumer Interface Adapters)
- [ADR-052](052-llm-runtime-strategy-agentic-simulation.md) (LLM Runtime Strategy)

---

## Executive Summary

A production-quality, web-based real-time viewer for HIVE simulations. Shows agents coordinating through the HIVE hierarchy as operations happen — container moves, capability changes, contention events, aggregation flows. Two layers: a domain-agnostic **HIVE Protocol View** (hierarchy, events, capabilities) and domain-specific **skins** (port operations first, military later).

Serves three audiences:
1. **External demo** (GPA, Kia/Hyundai, DoW shipbuilding) — "this is what HIVE does"
2. **Internal documentation** — visual explanation of hierarchical coordination
3. **Development tool** — real-time debugging of agent behavior and HIVE state flow

---

## Context

### The Problem

The port operations simulation (ADR-030, Phases 0 and 1a) produces structured METRICS JSON to stdout. A terminal dashboard renders ANSI text. Neither conveys what HIVE actually does — the coordination, contention, hierarchy, and emergent behavior are invisible in scrolling text.

Industry feedback (same feedback that drove ADR-031): *"You need to be able to visualize the hierarchy in some simple C2 map, video game kind of way."*

ADR-031 (HIVE Commander) addresses this with an interactive tactical RPG. But Commander is a *game* — players compose capabilities. What's missing is an *operational display* — a viewer that shows real agents coordinating through HIVE in real-time, without player input.

### Relationship to HIVE Commander (ADR-031)

| Aspect | Commander (ADR-031) | Viewer (This ADR) |
|--------|--------------------|--------------------|
| Mode | Interactive game | Passive observation |
| User role | Player composes capabilities | Observer watches coordination |
| Data source | Game engine state | Live/recorded simulation |
| Purpose | Teach composition through play | Demonstrate coordination at work |
| Shared tech | React/Three.js, capability cards | React/Three.js, capability cards |

Commander's eventual **spectator mode** would be this viewer. The rendering components, data models, and frontend architecture are shared. Building the viewer first creates reusable infrastructure for Commander.

### Relationship to Consumer Interface Adapters (ADR-043)

The viewer's WebSocket connection to the simulation backend follows the Consumer Interface Adapter pattern from ADR-043. The viewer is a read-only consumer of HIVE state — it subscribes to document updates, event streams, and capability changes via WebSocket.

---

## Decision

Build a **real-time operational viewer** as a web application with:

- **Rust backend** (Axum) — WebSocket relay server that bridges simulation events to browser clients
- **React/TypeScript frontend** with Three.js — production-quality 3D/2D rendering
- **Domain-agnostic protocol layer** — works for any HIVE simulation
- **Domain-specific skins** — port operations first, military/logistics later

### Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                          Browser Client                               │
│                                                                       │
│  ┌─────────────────────────┐  ┌────────────────────────────────────┐ │
│  │   HIVE Protocol View    │  │      Domain Skin (Port Ops)        │ │
│  │                         │  │                                     │ │
│  │  ┌─────────────────┐   │  │  ┌───────────────────────────────┐ │ │
│  │  │ Hierarchy Tree  │   │  │  │ Spatial View (Three.js)       │ │ │
│  │  │ H4→H3→H2→H1    │   │  │  │ Berth, hold, crane positions  │ │ │
│  │  └─────────────────┘   │  │  │ Container flow animation      │ │ │
│  │  ┌─────────────────┐   │  │  │ Queue progress overlay        │ │ │
│  │  │ Event Stream    │   │  │  └───────────────────────────────┘ │ │
│  │  │ Decisions, acts │   │  │  ┌───────────────────────────────┐ │ │
│  │  │ flowing up tree │   │  │  │ Capability Cards              │ │ │
│  │  └─────────────────┘   │  │  │ Crane stats, health, hazmat   │ │ │
│  │  ┌─────────────────┐   │  │  └───────────────────────────────┘ │ │
│  │  │ Capability State│   │  │  ┌───────────────────────────────┐ │ │
│  │  │ Per-entity cards│   │  │  │ Contention Indicators         │ │ │
│  │  └─────────────────┘   │  │  │ Claim conflicts, retries      │ │ │
│  │                         │  │  └───────────────────────────────┘ │ │
│  └─────────────────────────┘  └────────────────────────────────────┘ │
│                                                                       │
│                     WebSocket Connection                              │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│                     Viewer Relay Server (Rust/Axum)                    │
│                                                                       │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────────┐ │
│  │  WebSocket   │  │   Event      │  │   Replay Engine            │ │
│  │  Server      │  │   Buffer     │  │   (load JSON, scrub time)  │ │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬───────────────┘ │
│         │                 │                        │                  │
│         └─────────────────┼────────────────────────┘                  │
│                           │                                           │
│                           ▼                                           │
│                  ┌────────────────┐                                   │
│                  │  Event Ingest  │                                   │
│                  │  (stdin/TCP)   │                                   │
│                  └────────┬───────┘                                   │
│                           │                                           │
└───────────────────────────┼──────────────────────────────────────────┘
                            │
              ┌─────────────┼─────────────────┐
              │             │                 │
              ▼             ▼                 ▼
       ┌────────────┐ ┌──────────┐    ┌────────────┐
       │ Python Sim │ │ Rust     │    │ Recorded   │
       │ (Phase 0/  │ │ HIVE     │    │ JSON File  │
       │  1a stdout)│ │ Nodes    │    │ (replay)   │
       └────────────┘ └──────────┘    └────────────┘
```

### Event Protocol

All data flows as JSON over WebSocket. The protocol is domain-agnostic:

```typescript
// Server → Client messages
type ViewerEvent =
  | { type: "state_snapshot"; documents: Record<string, Record<string, any>>; events: any[] }
  | { type: "ooda_cycle"; node_id: string; cycle: number; sim_time: string; action: string; success: boolean; contention_retry: boolean; observe_ms: number; decide_ms: number; act_ms: number }
  | { type: "document_update"; collection: string; doc_id: string; fields: any }
  | { type: "hive_event"; event_type: string; source: string; priority: string; details: any }
  | { type: "sim_clock"; sim_time: string; real_elapsed_ms: number }
```

The Python sim already emits `ooda_cycle` METRICS JSON. The relay server parses these, maintains a state buffer, and broadcasts to connected browser clients. When Rust HIVE nodes replace the Python sim, they emit the same event types — the viewer doesn't change.

---

## Domain Skins

### Port Operations (First Skin)

**Spatial layout:**
- Top-down view of berth with hold grid (H1-H9 typical for large container vessel)
- Crane positions straddling holds, animated boom movement
- Container queue as a vertical strip beside each hold — green (done), yellow (in-progress), red (hazmat), gray (pending)
- Yard blocks as a grid below the berth — tractors moving containers from crane to block

**Overlays:**
- Crane reach envelope (semi-transparent arc)
- Hazmat glow on hazmat containers
- Degradation visual on crane (color shift from green → yellow → red based on health)
- Contention flash when two cranes target the same container
- Aggregator pulse when hold summary updates

**Panels:**
- Capability cards per crane (lift capacity, moves/hour, health, hazmat cert)
- Team summary panel (aggregate rate, gaps, moves completed/remaining)
- Event timeline (scrolling log with type icons and source colors)

### Military Operations (Future Skin)

Same protocol view, different spatial layout:
- Hex grid terrain (reuse ADR-031 Three.js terrain renderer)
- Platform icons instead of cranes
- Capability composition visualization instead of container flow

### Logistics (Future Skin)

- Warehouse/fleet layout
- Truck/drone routing
- Fulfillment queue instead of container queue

---

## Technology Stack

### Backend (Rust)

```
hive-viewer/
├── Cargo.toml
└── src/
    ├── main.rs              # Axum server entrypoint
    ├── relay/
    │   ├── mod.rs
    │   ├── ingest.rs        # Read events from stdin/TCP/file
    │   ├── buffer.rs        # Event buffer + state reconstruction
    │   └── broadcast.rs     # WebSocket broadcast to clients
    ├── ws/
    │   ├── mod.rs
    │   ├── handler.rs       # WebSocket upgrade + session management
    │   └── protocol.rs      # Event types (serde, ts-rs for codegen)
    └── replay/
        ├── mod.rs
        └── player.rs        # JSON file replay with time control
```

**Dependencies:**
- `axum` + `tokio` — async HTTP/WebSocket server
- `tokio-tungstenite` — WebSocket
- `serde` + `serde_json` — serialization
- `ts-rs` — TypeScript type generation from Rust structs (ADR-049 pattern)
- `tower-http` — CORS, static file serving
- `clap` — CLI arguments

### Frontend (React/TypeScript)

```
hive-viewer-ui/
├── package.json
├── tsconfig.json
├── vite.config.ts
└── src/
    ├── main.tsx
    ├── App.tsx
    ├── protocol/
    │   ├── types.ts          # Generated from Rust via ts-rs
    │   ├── connection.ts     # WebSocket client + reconnect
    │   └── state.ts          # Client-side state reconstruction
    ├── views/
    │   ├── ProtocolView/     # Domain-agnostic HIVE view
    │   │   ├── HierarchyTree.tsx
    │   │   ├── EventStream.tsx
    │   │   └── CapabilityCard.tsx
    │   └── PortOpsView/      # Port operations skin
    │       ├── BerthLayout.tsx
    │       ├── CraneSprite.tsx
    │       ├── ContainerQueue.tsx
    │       ├── TeamSummary.tsx
    │       └── ContentionFlash.tsx
    └── components/
        ├── Timeline.tsx       # Time scrubber for replay
        ├── MetricsPanel.tsx   # Cycle metrics display
        └── Layout.tsx         # Split-pane layout
```

**Dependencies:**
- `react` + `vite` — UI framework + build
- `three` + `@react-three/fiber` + `@react-three/drei` — 3D rendering
- `zustand` — state management
- `tailwindcss` — styling

### Monorepo Location

```
hive-sim/
├── port-ops/              # Existing Python sim
├── hive-viewer/           # NEW — Rust relay server
└── hive-viewer-ui/        # NEW — React frontend
```

---

## Implementation Phases

### Phase 1: Relay + Protocol View (MVP)

**Goal:** See HIVE events flowing through the hierarchy in real-time.

- Rust relay server reads METRICS JSON from stdin (pipe from Python sim)
- WebSocket broadcast to browser clients
- React frontend with Protocol View only:
  - Hierarchy tree (nodes as circles, edges as lines, color = status)
  - Event stream (scrolling log with icons)
  - Capability cards (one per entity)
- State snapshot on connect (new clients get current state immediately)

**Run:** `./run-phase1a.sh --max-cycles 30 | cargo run -p hive-viewer`

**Deliverable:** A browser window showing the HIVE hierarchy operating in real-time. Domain-agnostic — works for any HIVE sim that emits the event protocol.

### Phase 2: Port Operations Skin

**Goal:** GPA-ready demo — "that's my port."

- Three.js berth layout with crane positions
- Container queue visualization (color-coded blocks)
- Crane animation (boom movement on container move)
- Contention flash (when two cranes compete)
- Hazmat glow, degradation color shift
- Team summary panel

**Deliverable:** Split-pane view — protocol view on left, port spatial view on right. Compelling for GPA, Kia/Hyundai, DoW audiences.

### Phase 3: Replay + Polish

**Goal:** Demo-ready for any meeting, development tool for daily use.

- Replay engine: load recorded JSON, scrub timeline, pause/play/speed
- Polished UI: dark theme, smooth animations, responsive layout
- Multiple sim runs side-by-side (A/B comparison)
- Export: screenshot, GIF, or video capture of interesting moments

**Deliverable:** Production tool that works for demos, documentation, and development.

### Phase 4: Operational Simulation — "Watch the Port Work"

**Goal:** The viewer shows a complete port operation — not status dashboards, but actual operations unfolding spatially. A ship approaches, is directed to berth, cranes swing and lift containers off the vessel, trucks pull up under the crane, containers lower onto chassis, trucks drive to yard blocks. Every movement is driven by HIVE agent decisions flowing through the hierarchy.

This is the phase where the viewer becomes a **simulation visualization** rather than a **telemetry dashboard**.

**World Model (Sim):**
- Spatial state documents in HIVE state: vessel position on approach lane, crane boom angle + trolley position + spreader height, container coordinates (ship cell → crane → truck → yard slot), truck route waypoints
- Operational state machine per entity: crane has states (IDLE → ROTATING → TROLLEYING → LOWERING → GRIPPING → HOISTING → ROTATING → LOWERING → RELEASING → IDLE), truck has states (QUEUED → APPROACHING → UNDER_CRANE → LOADED → IN_TRANSIT → AT_YARD → UNLOADING → RETURNING)
- Container lifecycle: STOWED → PICKED → IN_TRANSIT → YARD_PLACED, with position at each stage
- Sequence coordination via HIVE: scheduler publishes discharge sequence, cranes claim containers, trucks claim transport jobs, aggregator tracks throughput

**Spatial Events (Protocol):**
- `spatial_update` event type: `{ entity_id, position: {x,y,z}, rotation, state, target? }`
- Emitted by sim whenever an entity moves or changes physical state
- Viewer interpolates smoothly between spatial updates (lerp position, slerp rotation)

**Viewer Animation:**
- Crane: boom rotates to target hold, trolley slides along boom, spreader descends, grips container (color change), hoists, rotates to truck side, lowers onto chassis
- Container: follows crane spreader while gripped, then follows truck while loaded
- Truck: drives along route waypoints (approach lane → under crane → exit lane → yard block)
- Vessel: arrives from right edge, docks at berth (one-time approach animation)

**Scenario:** ADR-030 MV Ever Forward — ship arrival triggers hierarchy formation, 20 containers discharged through the full crane-truck-yard pipeline, with equipment degradation and shift change events mid-operation.

**Deliverable:** Someone watching this says "that's a port working." The HIVE protocol view on the left shows *why* things are happening (agent decisions, capability changes, contention). The spatial view on the right shows *what* is happening (physical operations). Together they demonstrate HIVE coordinating real operations.

### Phase 5: Commander Convergence

**Goal:** Viewer becomes Commander's spectator mode.

- Share Three.js rendering components between viewer and Commander
- Commander adds interactive layer (player input) on top of viewer's display layer
- Same backend serves both viewer (passive) and Commander (interactive)

---

## Data Flow: Python Sim → Viewer

### Live Mode (Phase 0/1a)

```bash
# Option A: Pipe stdout
./run-phase1a.sh --max-cycles 30 2>/dev/null | hive-viewer --ingest stdin

# Option B: TCP socket (sim connects to viewer)
hive-viewer --ingest tcp://0.0.0.0:9100 &
./run-phase1a.sh --max-cycles 30 --viewer-addr localhost:9100
```

### Replay Mode

```bash
# Record
./run-phase1a.sh --max-cycles 30 2>/dev/null > sim-run-2026-02-07.jsonl

# Replay
hive-viewer --ingest file://sim-run-2026-02-07.jsonl --replay
```

### Future: Rust HIVE Nodes

When the Python sim is replaced by Rust HIVE nodes (ContainerLab topology), the nodes emit the same event protocol via TCP/WebSocket. The viewer connects directly — no relay needed. The relay server becomes the HIVE node's built-in consumer interface (ADR-043).

---

## Success Criteria

### Demo Effectiveness

| Criterion | Measure | Target |
|-----------|---------|--------|
| Time to understand | New viewer can see coordination happening | < 30 seconds |
| "That's my port" | GPA operators recognize the layout | Verbal confirmation |
| Transferability | Kia/Hyundai see it applies to their domain | Ask "can this work for us?" |

### Technical

| Criterion | Measure | Target |
|-----------|---------|--------|
| Latency | Sim event → browser render | < 100ms |
| Concurrent viewers | Browser clients watching same sim | 10+ |
| Replay fidelity | Recorded playback matches live | Identical |
| Frontend FPS | Smooth rendering during active sim | 60fps |

### Development Utility

| Criterion | Measure | Target |
|-----------|---------|--------|
| Debug value | Can identify agent misbehavior visually | Yes |
| State inspection | Click entity → see full HIVE document | Yes |
| Event filtering | Filter by source, type, priority | Yes |

---

## Alternatives Considered

### Unity/Unreal Engine

- Pro: Best-in-class 3D rendering, photorealistic port possible
- Con: Massive dev cost for asset pipelines, physics, cameras before wiring HIVE data
- Con: Standalone application, not embeddable in docs or shareable via URL
- Con: Separate tech stack from HIVE (Rust) and Commander (React/Three.js)
- Con: Audience is decision-makers, not gamers — they care about coordination, not polygons

### Python terminal dashboard (status quo)

- Pro: Already exists
- Con: Cannot convey hierarchy, spatial layout, or event flow
- Con: Not shareable, not demo-ready

### D3.js / SVG-only web

- Pro: Lighter than Three.js
- Con: Doesn't scale to 3D when we need it for Commander convergence
- Con: Less visual impact for demos

### React/Three.js web (chosen)

- Pro: Shared stack with Commander (ADR-031)
- Pro: Browser-native, shareable via URL, embeddable
- Pro: Production-quality 3D with reasonable dev cost
- Pro: Rust backend consistent with HIVE ecosystem
- Pro: Replay mode trivial to add

---

## References

- ADR-030: Port Operations Reference Implementation
- ADR-031: HIVE Commander — Tactical Capability RPG
- ADR-043: Consumer Interface Adapters
- ADR-049: Schema Extraction and Codegen (ts-rs pattern)
- ADR-052: LLM Runtime Strategy for Agentic Simulation
- Phase 1a multi-agent simulation (port-ops/run-phase1a.sh)

---

**Decision Record:**
- **Proposed:** 2026-02-07
- **Phase 1 (Protocol View) Complete:** 2026-02-07
- **Phase 2 (Port Ops Skin) Complete:** 2026-02-08
- **Phase 3 (Replay Engine) Complete:** 2026-02-09
- **Phase 4 (Operational Simulation):** In Progress

**Authors:** Kit Plummer, Claude
