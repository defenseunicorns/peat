# ADR-031: HIVE Commander - Interactive Capability Demonstration Game

## Status
Proposed

## Context

Industry feedback identified two critical gaps in HIVE's current demonstration capability:

1. **Visualization Gap**: "You need to be able to visualize the hierarchy in some simple C2 map, video game kind of way - showing individual capabilities being aggregated into emergent capabilities, and tasking being redistributed back out by the player"

2. **Cross-Boundary Coordination Gap**: "Can you take one asset from one squad and another asset from a different squad and task them as a new group? This would be super valuable, especially if they are crossing ownership boundaries (operated by two different countries)"

Additionally, the concept of **coagency performance** - measuring how well human-machine-AI teams perform together - was identified as a research differentiator that HIVE could demonstrate through interactive gameplay.

Current demonstration approaches (slides, architecture diagrams, static scenarios) fail to convey the dynamic nature of capability aggregation and the intuitive power of HIVE's coordination model. A playable demonstration would:

- Make the value proposition immediately tangible to investors and program managers
- Force crystallization of the capability query and composition API
- Provide a platform for game-theoretic validation of coordination dynamics
- Generate data on human-HIVE teaming effectiveness (coagency metrics)

## Decision

Build **HIVE Commander**, an RTS-style interactive demonstration game that uses the actual Rust HIVE reference implementation to coordinate heterogeneous assets within an operational area.

### Technology Stack

**Server-Side Multiplayer Only**

```
┌─────────────────────────────────────────────────────────────────┐
│                        HIVE Commander                           │
├─────────────────────────────────────────────────────────────────┤
│  Frontend (TypeScript/React)                                    │
│  ├── Hosted at revolveteam.com/commander                       │
│  ├── Map Renderer (Canvas/WebGL or Leaflet)                    │
│  ├── Asset Visualization                                        │
│  ├── Capability Tree View                                       │
│  ├── Task Assignment UI                                         │
│  ├── Mobile-optimized participant view                         │
│  └── Metrics Dashboard                                          │
├─────────────────────────────────────────────────────────────────┤
│  WebSocket Connection                                           │
├─────────────────────────────────────────────────────────────────┤
│  Backend (Rust/Axum) - api.revolveteam.com                     │
│  ├── HIVE Reference Implementation                              │
│  │   ├── Capability Documents (Automerge CRDTs)                │
│  │   ├── Hierarchical Aggregation Engine                       │
│  │   ├── Task-to-Capability Matching                           │
│  │   └── Cross-Boundary Composition                            │
│  ├── Game Session Management                                    │
│  │   ├── Create/join sessions                                  │
│  │   ├── Role assignment                                        │
│  │   └── Real-time state broadcast                             │
│  ├── Simulation Engine                                          │
│  │   ├── Asset State Management                                │
│  │   ├── Movement/Behavior Models                              │
│  │   └── Event Generation                                       │
│  └── Metrics Collection                                         │
│      ├── Coordination Latency                                   │
│      ├── Task Completion Rates                                  │
│      └── Coagency Performance Scores                           │
└─────────────────────────────────────────────────────────────────┘
```

**Why Server-Only:**
- Simpler architecture, one deployment target
- All game logic in Rust, validated against real HIVE implementation
- Multiplayer is the primary use case (demos, presentations, research)
- No WASM compilation complexity
- Easier to iterate on game balance (server-side updates only)

### Cross-Platform Client Strategy

For optimal mobile experience, especially for Asset Operators joining via phone, a native app provides better performance and UX than mobile web.

**Framework Comparison:**

| Framework | Web | iOS | Android | DX | Notes |
|-----------|-----|-----|---------|-----|-------|
| **Expo** | ✓ | ✓ | ✓ | ⭐⭐⭐ | Best choice - same React, batteries included |
| React Native + RN Web | ✓ | ✓ | ✓ | ⭐⭐ | More control, more config |
| Capacitor | ✓ | ✓ | ✓ | ⭐⭐ | Web-first wrapper, less native feel |
| Tauri Mobile | ✓ | ✓ | ✓ | ⭐ | Rust-native but adds complexity |

**Recommended: Expo (React Native)**

Expo provides the best developer experience for cross-platform React:

- **Expo Go**: Scan QR code → instantly test on your phone during development
- **Expo Web**: Same codebase renders to web browser
- **EAS Build**: Cloud builds for iOS/Android without local Xcode/Android Studio
- **OTA Updates**: Push updates without app store review cycle

**Architecture with Expo:**

```
┌─────────────────────────────────────────────────────────────────┐
│                        Shared Code                              │
│  ├── WebSocket client (connects to api.revolveteam.com)        │
│  ├── Game state management (Zustand or similar)                │
│  ├── HIVE message types (TypeScript interfaces)                │
│  └── Core UI components (maps, asset icons, task cards)        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────────┐       ┌──────────────────────┐       │
│  │   Commander View     │       │   Operator View      │       │
│  │   (Large screens)    │       │   (Phone-native)     │       │
│  │                      │       │                      │       │
│  │  • Full tactical map │       │  • Asset-centric UI  │       │
│  │  • Capability tree   │       │  • Touch-optimized   │       │
│  │  • Task assignment   │       │  • Task notifications│       │
│  │  • Metrics dashboard │       │  • Quick actions     │       │
│  │                      │       │                      │       │
│  │  Optimized for:      │       │  Runs on:            │       │
│  │  Desktop/tablet web  │       │  iOS, Android, Web   │       │
│  └──────────────────────┘       └──────────────────────┘       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Single Codebase Option:**

One Expo app that adapts based on screen size and role:

```typescript
// Responsive role-based rendering
function App() {
  const { role } = useGameSession();
  const isLargeScreen = useMediaQuery('(min-width: 1024px)');
  
  if (role === 'commander' || isLargeScreen) {
    return <CommanderView />;
  }
  return <OperatorView />;
}
```

**Native App Benefits for Demos:**

| Benefit | Impact |
|---------|--------|
| App Store presence | "Download HIVE Commander" - legitimacy |
| Push notifications | Task assignments ping operators |
| Better performance | Smoother map interactions on phones |
| Native gestures | Pinch-zoom, swipe feels right |
| Offline resilience | Reconnects gracefully after signal loss |
| No browser chrome | Full-screen immersive experience |

**Expo Development Workflow:**

```
┌─────────────────────────────────────────────────────────────────┐
│                     Development                                 │
│                                                                 │
│   $ npx expo start                                             │
│                                                                 │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐           │
│   │ Expo Go     │  │ iOS Sim    │  │ Web Browser │           │
│   │ (your      │  │             │  │ localhost   │           │
│   │  phone)    │  │             │  │             │           │
│   │ Scan QR    │  │             │  │             │           │
│   └─────────────┘  └─────────────┘  └─────────────┘           │
│         │                │                │                    │
│         └────────────────┴────────────────┘                    │
│                          │                                     │
│                    Hot Reload                                  │
│                 (instant updates)                              │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                     Production                                  │
│                                                                 │
│   $ eas build --platform all                                   │
│                                                                 │
│   EAS Cloud builds:                                            │
│   ├── iOS .ipa → App Store / TestFlight                       │
│   ├── Android .apk/.aab → Play Store                          │
│   └── Web bundle → Cloudflare Pages                           │
│                                                                 │
│   OTA Updates (post-launch):                                   │
│   $ eas update --branch production                             │
│   → Users get updates without app store review                 │
└─────────────────────────────────────────────────────────────────┘
```

**Phased Approach to Native:**

| Phase | Client Strategy |
|-------|-----------------|
| Phase 1 | Web-only (React/Vite), validate core gameplay |
| Phase 2 | Evaluate Expo migration based on mobile feedback |
| Phase 3+ | Native app in App Store, web fallback for quick joins |

**Conference Demo Flow with Native App:**

```
Before talk:
  "Download HIVE Commander from the App Store"
  (or scan QR for web fallback)

During talk:
  1. Presenter shares game code: "HIVE-7X3K"
  2. Attendees open app → Enter code → Pick asset
  3. Push notification: "You are Scout Drone Alpha-1"
  4. Live coordination begins
  
After talk:
  App stays installed → future engagement
```

## Game Design

### Design Philosophy

HIVE Commander is designed as three things simultaneously:

1. **Presentation Tool**: Live demos where conference audiences participate
2. **Sales Tool**: Themed versions that speak directly to target verticals
3. **Research Platform**: Head-to-head play generates real game theory data

The core insight: **HIVE's coordination mechanics are domain-agnostic**. The same capability aggregation that coordinates drones and operators also coordinates cells and researchers, or trucks and dispatchers. Different themes make this tangible to different audiences.

### Multiplayer Modes

| Mode | Players | Use Case |
|------|---------|----------|
| **Solo** | 1 | Learning, practice, async demos |
| **Cooperative** | 2-8 | Coalition coordination demo, team training |
| **Head-to-Head** | 2 (+ observers) | Competitive demo, game theory research |
| **Audience Play** | 10-50 | Conference presentations, workshops |

#### Head-to-Head Mode

Two commanders compete for objectives in a shared operational space:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Shared Operational Area                      │
│                                                                 │
│   ┌───────────────────┐       ┌───────────────────┐            │
│   │    BLUE TEAM      │       │     RED TEAM      │            │
│   │    (Player 1)     │       │    (Player 2)     │            │
│   │                   │       │                   │            │
│   │  🛩️🛩️🛩️ Drones    │       │   Drones 🛩️🛩️🛩️   │            │
│   │  🤖🤖 Robots      │       │    Robots 🤖🤖    │            │
│   │  👤 Operator      │       │   Operator 👤     │            │
│   └───────────────────┘       └───────────────────┘            │
│                                                                 │
│                    🎯 Contested Objectives 🎯                   │
│                                                                 │
│   Win condition: Control objectives through superior           │
│   capability composition and task coordination                 │
└─────────────────────────────────────────────────────────────────┘
```

**What head-to-head reveals:**
- Which coordination strategies win under pressure
- How quickly players can recompose after losses
- Whether HIVE's recommendations improve competitive performance
- Emergent tactics that inform real operational doctrine

#### Audience Participation Mode

For conference demos and workshops, attendees join via phone/laptop:

```
┌─────────────────────────────────────────────────────────────────┐
│                      Presenter View                             │
│                   (Main Screen / Projector)                     │
│                                                                 │
│   Full operational map, all assets, capability tree            │
│   Presenter acts as C2, assigns high-level tasks               │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Attendee 1  │     │  Attendee 2  │     │  Attendee 3  │
│  "Alpha-1"   │     │  "Alpha-2"   │     │  "Bravo-1"   │
│              │     │              │     │              │
│  Controls:   │     │  Controls:   │     │  Controls:   │
│  Scout Drone │     │  Ground Bot  │     │  Strike UAV  │
│              │     │              │     │              │
│  Phone UI:   │     │  Phone UI:   │     │  Phone UI:   │
│  - Status    │     │  - Status    │     │  - Status    │
│  - Position  │     │  - Position  │     │  - Position  │
│  - Execute   │     │  - Execute   │     │  - Execute   │
└──────────────┘     └──────────────┘     └──────────────┘
```

### Role System

The game supports a hierarchy of roles that mirror real command structures:

```
┌─────────────────────────────────────────────────────────────────┐
│                         ROLE HIERARCHY                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  COMMANDER (1 per team)                                        │
│  ├── Full map view, all assets visible                         │
│  ├── Capability tree showing aggregated capabilities           │
│  ├── Assign tasks to squads or individual assets               │
│  ├── Create/dissolve task forces                               │
│  ├── See HIVE recommendations, accept/reject                   │
│  └── Coalition: can request assets from allied commanders      │
│                                                                 │
│  SQUAD LEADER (optional, 1 per squad)                          │
│  ├── Squad-level map view                                       │
│  ├── Controls task distribution within squad                   │
│  ├── Can suggest task force composition to Commander           │
│  └── Receives tasks from Commander, delegates to operators     │
│                                                                 │
│  ASSET OPERATOR (1 per asset, or 1 controlling multiple)       │
│  ├── Asset-centric view (own position, local area)            │
│  ├── Receives task assignments                                  │
│  ├── Executes movement and actions                             │
│  ├── Reports status (auto or manual)                           │
│  └── Mobile-optimized UI                                        │
│                                                                 │
│  OBSERVER (unlimited)                                           │
│  ├── Read-only full map view                                   │
│  ├── Metrics dashboard                                          │
│  └── Good for: investors watching, researchers, spectators    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Demo Configurations

| Scenario | Commander | Squad Leaders | Asset Operators | Observers |
|----------|-----------|---------------|-----------------|-----------|
| **Solo practice** | 1 (you) | 0 | 0 (AI-controlled) | 0 |
| **Investor demo** | 1 (investor) | 0 | 1-2 (you/team) | 0-2 |
| **Workshop** | 1 (presenter) | 2-4 | 10-40 (attendees) | 0-10 |
| **Head-to-head** | 2 (competing) | 0-2 each | 0-10 each | unlimited |
| **Coalition** | 2-3 (allied) | 1-2 each | 5-15 each | unlimited |

### Join Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│   1. Scan QR / visit revolveteam.com/commander/join/HIVE-7X3K  │
│                                                                  │
│   2. Select Role:                                                │
│      ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│      │  COMMANDER  │  │  OPERATOR   │  │  OBSERVER   │          │
│      │  (if open)  │  │             │  │             │          │
│      └─────────────┘  └─────────────┘  └─────────────┘          │
│                                                                  │
│   3. If Operator, pick available asset:                         │
│      ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│      │ Scout Drone │  │ Ground Bot  │  │ Strike UAV  │          │
│      │   Alpha-1   │  │   Alpha-2   │  │   Bravo-1   │          │
│      │  ✓ Available│  │  ✓ Available│  │  ✗ Taken    │          │
│      └─────────────┘  └─────────────┘  └─────────────┘          │
│                                                                  │
│   4. Join game, receive role-appropriate UI                     │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Why This Works

**For the Commander (presenter/investor):**
- They experience the cognitive load of coordination
- HIVE's recommendations feel like genuine help
- "I couldn't have tracked all that manually"

**For Asset Operators (audience):**
- They're engaged, not passive observers
- They feel the task come down from C2
- "So THAT'S why it picked me for this task"
- Creates memorable "I was the drone" stories

**For Researchers:**
- Real human coordination behavior, not simulated
- Multiple skill levels interacting
- Natural variance for statistical analysis

**Why this works for presentations:**
- Audience is *engaged*, not passive
- They *feel* the coordination problem firsthand
- "I was the drone operator" creates memorable demos
- Natural Q&A: "Why did HIVE pick my asset for that task?"

### Theming Architecture

The game separates **core mechanics** from **presentation layer**:

```
┌─────────────────────────────────────────────────────────────────┐
│                      HIVE Core Engine                           │
│  (Domain-agnostic coordination)                                 │
│                                                                 │
│  • Capability documents                                         │
│  • Hierarchical aggregation                                     │
│  • Task-capability matching                                     │
│  • Cross-boundary composition                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                    Theme Adapter Layer
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   MILITARY   │     │   BIOLOGY    │     │   LOGISTICS  │
│    Theme     │     │    Theme     │     │    Theme     │
└──────────────┘     └──────────────┘     └──────────────┘
```

#### Theme: Military Operations (Default)

| Core Concept | Military Skin |
|--------------|---------------|
| Asset | Platform (drone, robot, vehicle) |
| Operator | Warfighter, Commander |
| Capability | ISR, Strike, Transport, Comms |
| Task | Mission (track, secure, neutralize) |
| Group | Squad, Platoon, Company |
| Boundary | National ownership, OPCON/ADCON |

**Visual style**: Tactical map, military iconography, MIL-STD colors

#### Theme: Cellular Biology

| Core Concept | Biology Skin |
|--------------|--------------|
| Asset | Cell, Organelle, Molecule |
| Operator | Researcher, Lab AI |
| Capability | Sense, Signal, Transport, Metabolize |
| Task | Response (detect pathogen, synthesize, migrate) |
| Group | Tissue, Organ, System |
| Boundary | Membrane, Blood-brain barrier |

**Visual style**: Microscopy aesthetic, organic shapes, bioluminescent colors

**Target audience**: Biotech, pharma, synthetic biology, DARPA BTO

#### Theme: Supply Chain / Logistics

| Core Concept | Logistics Skin |
|--------------|----------------|
| Asset | Truck, Warehouse, Drone, Forklift |
| Operator | Dispatcher, Manager |
| Capability | Transport, Store, Sort, Deliver |
| Task | Order fulfillment, Restock, Route |
| Group | Fleet, Region, Network |
| Boundary | Company ownership, Contractor |

**Visual style**: Clean industrial, familiar logistics iconography

**Target audience**: Ports, warehousing, last-mile delivery, manufacturing

#### Theme: Manufacturing

| Core Concept | Manufacturing Skin |
|--------------|-------------------|
| Asset | Robot arm, AGV, Sensor, Station |
| Operator | Supervisor, Quality AI |
| Capability | Weld, Assemble, Inspect, Transport |
| Task | Build order, Quality check, Changeover |
| Group | Cell, Line, Plant |
| Boundary | OEM vs Supplier, Union jurisdiction |

**Visual style**: Factory floor, industrial automation aesthetic

**Target audience**: Industry 4.0, automotive, aerospace manufacturing

#### Theme: Agriculture

| Core Concept | Agriculture Skin |
|--------------|-----------------|
| Asset | Tractor, Drone, Sensor, Irrigation |
| Operator | Farmer, Agronomist AI |
| Capability | Till, Plant, Spray, Harvest, Monitor |
| Task | Field operation, Pest response, Harvest |
| Group | Field, Farm, Cooperative |
| Boundary | Property lines, Water rights |

**Visual style**: Aerial field views, agricultural equipment

**Target audience**: AgTech, precision agriculture, farming cooperatives

### Theme Configuration

Themes are defined as configuration, not code:

```yaml
# themes/military.yaml
theme:
  name: "Military Operations"
  id: military
  
  terminology:
    asset: "Platform"
    operator: "Warfighter"
    capability: "Capability"
    task: "Mission"
    group: "Unit"
    
  asset_types:
    - id: scout_uav
      name: "Scout UAV"
      icon: "assets/military/scout_uav.svg"
      capabilities: [SENSOR_EO, SENSOR_IR, RECON]
      
    - id: strike_uav
      name: "Strike UAV"  
      icon: "assets/military/strike_uav.svg"
      capabilities: [SENSOR_EO, STRIKE_PRECISION, LOITER]
      
  task_types:
    - id: track_target
      name: "Track Target"
      icon: "assets/military/track.svg"
      required_capabilities: [SENSOR_*, TRACK_OBJECT]
      
  colors:
    friendly: "#4A90D9"
    hostile: "#D94A4A"
    neutral: "#808080"
    
  map_style: "tactical"
```

```yaml
# themes/biology.yaml
theme:
  name: "Cellular Systems"
  id: biology
  
  terminology:
    asset: "Cell"
    operator: "Researcher"
    capability: "Function"
    task: "Response"
    group: "Tissue"
    
  asset_types:
    - id: tcell
      name: "T-Cell"
      icon: "assets/biology/tcell.svg"
      capabilities: [DETECT_ANTIGEN, SIGNAL, ATTACK]
      
    - id: macrophage
      name: "Macrophage"
      icon: "assets/biology/macrophage.svg"
      capabilities: [ENGULF, PRESENT_ANTIGEN, SIGNAL]
      
  colors:
    friendly: "#4AD98A"
    hostile: "#D94A4A"
    neutral: "#E8E8E8"
    
  map_style: "microscopy"
```

### Core Gameplay Loop

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   OBSERVE    │────▶│    DECIDE    │────▶│     ACT      │
│              │     │              │     │              │
│ View map     │     │ Select task  │     │ Assign task  │
│ See caps     │     │ Choose assets│     │ HIVE matches │
│ Track status │     │ Form groups  │     │ Assets move  │
└──────────────┘     └──────────────┘     └──────────────┘
       ▲                                         │
       │                                         │
       └─────────────────────────────────────────┘
                    Game State Updates
```

### Asset Types and Capabilities

| Asset Type | Platform | Advertised Capabilities |
|------------|----------|------------------------|
| Scout Drone | UAV | `SENSOR_EO`, `SENSOR_IR`, `RECON` |
| Strike Drone | UAV | `SENSOR_EO`, `STRIKE_PRECISION`, `LOITER` |
| Ground Robot | UGV | `SENSOR_ACOUSTIC`, `TRANSPORT_LIGHT`, `PATROL` |
| Relay Node | Fixed | `COMMS_RELAY`, `MESH_EXTEND` |
| Operator | Human | `AUTHORIZE`, `OVERRIDE`, `INTEL_ANALYSIS` |
| AI Agent | Software | `TRACK_OBJECT`, `CLASSIFY`, `PREDICT_PATH` |

### Emergent Capabilities (Hierarchical Aggregation)

When assets are grouped, HIVE computes emergent capabilities:

```
Individual Assets              Squad Aggregation           Emergent Capability
─────────────────              ─────────────────           ───────────────────

Scout Drone (SENSOR_EO)    ┐
Scout Drone (SENSOR_IR)    ├──▶  Alpha Squad         ──▶  PERSISTENT_ISR
AI Agent (TRACK_OBJECT)    ┘     3 assets                  (continuous tracking)

Strike Drone (STRIKE)      ┐
Operator (AUTHORIZE)       ├──▶  Bravo Squad         ──▶  AUTHORIZED_STRIKE
AI Agent (CLASSIFY)        ┘     3 assets                  (human-in-loop strike)

Ground Robot (PATROL)      ┐
Scout Drone (SENSOR_EO)    ├──▶  Charlie Squad       ──▶  AREA_SECURITY
Relay Node (COMMS_RELAY)   ┘     3 assets                  (persistent presence)
```

### Task Types

| Task | Required Capabilities | Success Criteria |
|------|----------------------|------------------|
| Track Target | `SENSOR_*` + `TRACK_OBJECT` | Continuous track maintained |
| Secure Area | `PATROL` + `SENSOR_*` | No incursions undetected |
| Strike Target | `STRIKE_*` + `AUTHORIZE` | Target neutralized |
| Extend Comms | `COMMS_RELAY` | Coverage area increased |
| Escort Asset | `TRANSPORT_*` + `SENSOR_*` | Asset reaches destination |

### Cross-Boundary Task Forces (Point 2 Validation)

The game explicitly supports creating ad-hoc task forces across ownership boundaries:

```
┌─────────────────────────────────────────────────────────────────┐
│                     Operational Area                            │
│                                                                 │
│   ┌─────────────┐              ┌─────────────┐                 │
│   │  US Squad   │              │  UK Squad   │                 │
│   │  (Blue)     │              │  (Blue)     │                 │
│   │             │              │             │                 │
│   │  🛩️ Scout   │              │  🛩️ Strike  │                 │
│   │  🤖 Robot   │              │  👤 Operator│                 │
│   └─────────────┘              └─────────────┘                 │
│          │                            │                         │
│          │     Player drag-selects    │                         │
│          │     across boundaries      │                         │
│          ▼                            ▼                         │
│   ┌─────────────────────────────────────────┐                  │
│   │         Ad-Hoc Task Force               │                  │
│   │         (Coalition - Purple)            │                  │
│   │                                         │                  │
│   │  🛩️ US Scout + 🛩️ UK Strike + 👤 UK Op  │                  │
│   │                                         │                  │
│   │  Emergent: AUTHORIZED_STRIKE_WITH_ISR   │                  │
│   └─────────────────────────────────────────┘                  │
│                                                                 │
│   HIVE handles:                                                │
│   • Capability composition across owners                       │
│   • Authority delegation (UK Op authorizes US asset strike)   │
│   • State sync between national systems                        │
└─────────────────────────────────────────────────────────────────┘
```

### UI Components

**1. Operational Map**
- 2D top-down view of operational area
- Assets shown as icons with ownership coloring
- Coverage areas visualized (sensor ranges, patrol zones)
- Targets and objectives marked
- Drag-select for ad-hoc grouping

**2. Capability Tree**
- Hierarchical view: Assets → Squads → Formation → Operational
- Real-time capability aggregation visualization
- Emergent capabilities highlighted
- Click-to-expand details

**3. Task Panel**
- Available task types
- Drag-drop task assignment
- HIVE's capability matching shown (which assets can fulfill)
- Confidence/suitability scores

**4. Metrics Dashboard**
- Coordination latency (time from task to execution)
- Task completion rate
- Coverage percentage
- Coagency score (human-machine team effectiveness)

## Validation Objectives

### 1. Capability Algebra Validation

The game provides empirical testing of capability composition rules:

```rust
// Example composition rule - does this feel right in gameplay?
fn compose_isr(assets: &[Asset]) -> Option<EmergentCapability> {
    let has_sensor = assets.iter().any(|a| a.has_capability("SENSOR_*"));
    let has_tracking = assets.iter().any(|a| a.has_capability("TRACK_OBJECT"));
    let has_persistence = assets.len() >= 2; // Redundancy for persistence
    
    if has_sensor && has_tracking && has_persistence {
        Some(EmergentCapability::PersistentISR {
            coverage: calculate_coverage(assets),
            track_capacity: count_track_slots(assets),
        })
    } else {
        None
    }
}
```

**Validation question**: When players form groups, do the emergent capabilities match their intuition? If not, the composition rules need adjustment.

### 2. Task-Capability Matching

When a player assigns a task, HIVE must select appropriate assets:

```rust
fn match_task_to_assets(
    task: &Task,
    available: &[Asset],
    constraints: &Constraints,
) -> TaskAssignment {
    // HIVE's matching algorithm runs here
    // Game reveals whether selections feel "smart"
}
```

**Validation question**: Does HIVE pick the assets the player would have picked? Does it explain why?

### 3. Cross-Boundary Coordination

The game explicitly tests coalition scenarios:

- US assets working with UK assets
- Authority delegation across ownership boundaries
- Information sharing constraints (some data stays national)

**Validation question**: Can HIVE correctly model OPCON/ADCON distinctions?

### 4. Degradation Behavior

The game can simulate asset loss, comms degradation, and network partitions:

- Remove an asset mid-mission
- Introduce communications delay
- Partition the network

**Validation question**: Does HIVE gracefully degrade? Do emergent capabilities recompute correctly?

### 5. Coagency Performance Metrics

The game can measure human-HIVE teaming effectiveness:

| Metric | Description | Target |
|--------|-------------|--------|
| Decision Latency | Time from situation change to task assignment | < 10s |
| Trust Calibration | Player acceptance rate of HIVE recommendations | > 80% |
| Override Rate | How often player overrides HIVE's matching | < 20% |
| Task Success | Successful task completions | > 90% |
| Coordination Overhead | Player actions per task | < 5 |

These metrics directly support research into human-machine teaming and provide data for DARPA/academic proposals.

## Integration Points with HIVE Reference Implementation

### Required APIs

The game server needs these interfaces from hive-core:

```rust
// Capability Document Operations
trait CapabilityStore {
    fn advertise(&mut self, asset_id: AssetId, capabilities: Vec<Capability>);
    fn get_capabilities(&self, asset_id: AssetId) -> Vec<Capability>;
    fn get_aggregated(&self, group_id: GroupId) -> AggregatedCapabilities;
}

// Hierarchical Aggregation
trait HierarchyManager {
    fn create_group(&mut self, name: &str, members: Vec<AssetId>) -> GroupId;
    fn dissolve_group(&mut self, group_id: GroupId);
    fn compute_emergent(&self, group_id: GroupId) -> Vec<EmergentCapability>;
}

// Task Matching
trait TaskMatcher {
    fn find_capable(&self, task: &Task) -> Vec<(AssetId, Suitability)>;
    fn assign_task(&mut self, task: Task, assets: Vec<AssetId>) -> Assignment;
}

// Cross-Boundary Composition
trait CoalitionManager {
    fn create_task_force(&mut self, name: &str, assets: Vec<AssetId>) -> TaskForceId;
    fn delegate_authority(&mut self, from: Owner, to: Owner, scope: AuthScope);
}
```

### WebSocket Message Protocol

Client-server communication uses JSON messages over WebSocket:

```typescript
// Client → Server
type ClientMessage = 
  | { type: "join", sessionId: string, role: Role }
  | { type: "create_game", theme: string, mode: GameMode }
  | { type: "assign_task", taskType: string, target: Target }
  | { type: "create_task_force", name: string, assetIds: string[] }
  | { type: "move_asset", assetId: string, position: Position }
  | { type: "accept_recommendation", recommendationId: string }
  | { type: "reject_recommendation", recommendationId: string };

// Server → Client
type ServerMessage =
  | { type: "game_state", state: GameState }
  | { type: "asset_update", asset: Asset }
  | { type: "capability_update", groupId: string, capabilities: Capability[] }
  | { type: "task_assigned", task: Task, assets: string[] }
  | { type: "recommendation", id: string, suggestion: Suggestion }
  | { type: "metrics_update", metrics: Metrics }
  | { type: "player_joined", player: Player }
  | { type: "game_over", winner: Team, stats: GameStats };
```

### Axum Server Structure

```rust
// Main server setup
#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api/games", post(create_game))
        .route("/api/games/:id", get(get_game))
        .route("/ws", get(websocket_handler))
        .with_state(AppState::new());
    
    axum::Server::bind(&"0.0.0.0:3030".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// WebSocket handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

// Game session management
struct GameSession {
    id: SessionId,
    theme: Theme,
    mode: GameMode,
    hive: HiveEngine,  // The actual HIVE reference implementation
    players: HashMap<PlayerId, PlayerConnection>,
    state: GameState,
}
```

## MVP Scope

### Phase 1: Core Engine + Multiplayer Foundation (5 weeks)

**Goal**: Two players can join a game, one as Commander, one as Asset Operator

- [ ] Axum WebSocket server with session management
- [ ] HIVE core engine integration (capability store, aggregation)
- [ ] React frontend with basic map renderer
- [ ] Commander view: full map, asset list, task panel
- [ ] Operator view: mobile-friendly, asset-centric
- [ ] 10-20 assets on a simple map
- [ ] 3 asset types (drone, robot, operator)
- [ ] Basic capability advertisement and aggregation
- [ ] 2-3 task types with capability matching
- [ ] Join flow: create game → share link → join with role
- [ ] Military theme (default)

**Deliverable**: Playable 2-player demo at revolveteam.com/commander

### Phase 2: Head-to-Head + Scaling (4 weeks)

**Goal**: Competitive mode and support for more players

- [ ] Head-to-head mode (red vs blue teams)
- [ ] Support 10+ concurrent players per session
- [ ] Victory conditions and scoring
- [ ] Real-time metrics display
- [ ] Observer role (watch-only)
- [ ] Game replay/playback foundation

**Deliverable**: Competitive demos, early game theory experiments

### Phase 3: Audience Participation (3 weeks)

**Goal**: Conference-ready presentation tool

- [ ] Mobile-friendly player UI
- [ ] QR code join flow
- [ ] Presenter dashboard
- [ ] 10-50 concurrent participants
- [ ] Simplified asset operator controls
- [ ] Live metrics visualization

**Deliverable**: Conference demo capability, workshop tool

### Phase 4: Cross-Boundary + Coalition (3 weeks)

**Goal**: Demonstrate coalition task forces (Point 2)

- [ ] Multi-owner assets (US, UK, AUS colors)
- [ ] Ad-hoc task force creation via drag-select
- [ ] Authority delegation UI
- [ ] Ownership-aware capability sharing
- [ ] OPCON/ADCON modeling

**Deliverable**: AUKUS/NATO demo scenarios

### Phase 5: Themes + Vertical Demos (4 weeks)

**Goal**: Themed versions for target markets

- [ ] Biology theme (DARPA BTO, biotech)
- [ ] Logistics theme (ports, warehousing)
- [ ] Manufacturing theme (Industry 4.0)
- [ ] Agriculture theme (AgTech)
- [ ] Theme switcher in UI

**Deliverable**: Vertical-specific sales demos

### Phase 6: Metrics + Coagency Research (3 weeks)

**Goal**: Capture human-HIVE teaming data

- [ ] Comprehensive metrics dashboard
- [ ] Session recording/playback
- [ ] Coagency score calculation
- [ ] Head-to-head analytics
- [ ] Export data for academic analysis

**Deliverable**: Research-ready platform for human factors studies, game theory papers

## Deployment Architecture

### Primary: Cloud Multiplayer Server

Multiplayer is the primary deployment mode, enabling demos, presentations, and research:

```
┌──────────────────────────────────────────────────────────────┐
│                    revolveteam.com                           │
├──────────────────────────────────────────────────────────────┤
│  Static Assets (Vercel/Cloudflare Pages)                    │
│  ├── /                     Main website                     │
│  ├── /commander/           Game client (React + WASM)       │
│  └── /commander/join/:id   Direct game session link         │
├──────────────────────────────────────────────────────────────┤
│  API Subdomain: api.revolveteam.com                         │
│  (Fly.io / Railway / Render)                                │
│                                                              │
│  Axum Server                                                │
│  ├── WebSocket endpoint (/ws)                               │
│  ├── Session management                                     │
│  │   ├── Create game → returns session ID                  │
│  │   ├── Join game → WebSocket connection                  │
│  │   └── List public games                                  │
│  ├── HIVE coordination engine                               │
│  ├── Theme configuration serving                            │
│  └── Metrics collection                                     │
├──────────────────────────────────────────────────────────────┤
│  Database (optional, for persistence)                       │
│  ├── Session state (Redis or in-memory)                    │
│  ├── Match history (Postgres/SQLite)                        │
│  └── Research data export                                   │
└──────────────────────────────────────────────────────────────┘
```

### Session Flow

```
Conference Presenter                    Audience Members
       │                                      │
       ▼                                      │
┌──────────────┐                              │
│ Create Game  │                              │
│ Select Theme │                              │
│ Set Mode     │                              │
└──────┬───────┘                              │
       │                                      │
       ▼                                      │
┌──────────────┐    QR Code / Short Link     │
│ Session ID:  │ ───────────────────────────▶ │
│ HIVE-7X3K    │    revolveteam.com/         │
│              │    commander/join/7X3K       │
└──────┬───────┘                              │
       │                                      ▼
       │                              ┌──────────────┐
       │                              │ Join Game    │
       │                              │ Select Role  │
       │                              │ Pick Asset   │
       │                              └──────┬───────┘
       │                                     │
       ▼                                     ▼
┌─────────────────────────────────────────────────────┐
│                  Live Game Session                  │
│                                                     │
│  Presenter: Commander view, full map               │
│  Attendees: Asset operator views, mobile-friendly  │
│  All: Real-time sync via WebSocket                 │
└─────────────────────────────────────────────────────┘
```

### Infrastructure Cost Estimates

| Component | Service | Cost (MVP) | Cost (Scale) |
|-----------|---------|------------|--------------|
| Static hosting | Cloudflare Pages | Free | Free |
| Game server | Fly.io | $5-20/mo | $50-200/mo |
| Database | Fly.io Postgres | Free tier | $15/mo |
| Session state | Fly.io Redis | Free tier | $15/mo |
| **Total** | | **~$25/mo** | **~$230/mo** |

Fly.io's scale-to-zero means costs are minimal when not actively running demos.

## Game Theory Validation

HIVE Commander provides a controlled environment for studying coordination dynamics. Head-to-head play creates natural experimental conditions.

### Competitive Dynamics Research

Head-to-head mode enables rigorous study of coordination under adversarial pressure:

#### Experiment 1: Coordination Speed Competition

**Setup**: Equal assets, symmetric map, contested objectives
**Measure**: Time from objective appearance to task completion
**Hypothesis**: HIVE-assisted coordination outperforms manual coordination

```
Trial Design:
┌─────────────────────────────────────────────────────────────────┐
│  Round 1: Both players use HIVE recommendations                │
│  Round 2: Both players ignore HIVE, manual coordination        │
│  Round 3: Player A uses HIVE, Player B manual (swap roles)     │
│                                                                 │
│  Measure: Win rate, task completion time, coordination errors  │
└─────────────────────────────────────────────────────────────────┘
```

#### Experiment 2: Capability Composition Strategies

**Setup**: Asymmetric starting assets, multiple viable strategies
**Measure**: Which capability compositions win in competition
**Output**: Empirical data on optimal team composition

**Research questions:**
- Do players discover the same emergent capabilities HIVE predicts?
- Are there compositions HIVE misses that humans find?
- How do winning strategies change with different asset mixes?

#### Experiment 3: Degradation Under Pressure

**Setup**: Mid-game asset attrition (simulated losses)
**Measure**: Recovery time, recomposition quality
**Hypothesis**: HIVE's real-time reaggregation enables faster recovery

```
Attrition Scenarios:
┌─────────────────────────────────────────────────────────────────┐
│  Scenario A: Random 30% asset loss                             │
│  Scenario B: Targeted loss of key capabilities                 │
│  Scenario C: Network partition (temporary loss of visibility)  │
│                                                                 │
│  Measure: Time to reestablish capability coverage              │
│  Measure: Task failure rate during recovery                    │
└─────────────────────────────────────────────────────────────────┘
```

#### Experiment 4: Coalition Formation Dynamics

**Setup**: Three or more players with distinct assets
**Measure**: When and how players form alliances
**Research questions:**
- What capability gaps drive coalition formation?
- How stable are cross-boundary task forces?
- Does HIVE's visibility into allied capabilities change behavior?

### Coagency Performance Metrics

The game measures human-HIVE teaming effectiveness across multiple dimensions:

| Metric | Description | Collection Method |
|--------|-------------|-------------------|
| **Decision Latency** | Time from situation change to task assignment | Timestamp deltas |
| **Trust Calibration** | Player acceptance rate of HIVE recommendations | Accept/reject logging |
| **Override Rate** | How often player overrides HIVE's matching | Action comparison |
| **Task Success** | Successful task completions | Outcome tracking |
| **Coordination Overhead** | Player actions per task | Input counting |
| **Recovery Speed** | Time to reestablish after disruption | State monitoring |
| **Competitive Win Rate** | Wins when using HIVE vs manual | Match outcomes |

### Academic Research Potential

HIVE Commander data could support papers on:

1. **Human-AI Teaming**: "Coordination Assistance Effects on Team Performance Under Adversarial Pressure"

2. **Game Theory**: "Emergent Coalition Formation in Heterogeneous Multi-Agent Competition"

3. **Military Operations Research**: "Hierarchical Capability Aggregation for Scalable C2"

4. **Human Factors**: "Trust Development in AI-Assisted Coordination Systems"

5. **Distributed Systems**: "CRDT-Based Real-Time State Synchronization for Interactive Simulation"

### Data Collection Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     HIVE Commander Session                      │
├─────────────────────────────────────────────────────────────────┤
│  Event Stream                                                   │
│  ├── Player actions (with timestamps)                          │
│  ├── HIVE recommendations (accepted/rejected)                  │
│  ├── Asset state changes                                        │
│  ├── Task assignments and completions                          │
│  ├── Capability recomputations                                 │
│  └── Victory/defeat events                                      │
├─────────────────────────────────────────────────────────────────┤
│  Export Formats                                                 │
│  ├── JSON event log (raw)                                      │
│  ├── CSV summary statistics                                    │
│  ├── Replay file (full session reconstruction)                 │
│  └── R/Python-ready dataframes                                 │
└─────────────────────────────────────────────────────────────────┘
```

### IRB Considerations

If used for formal research:
- Informed consent for data collection
- Anonymization of player identities
- Opt-out mechanism for session recording
- Data retention policies

The game can operate in "research mode" (full logging) or "demo mode" (minimal logging) based on context.

## Success Criteria

### Demo Effectiveness

| Criterion | Measure | Target |
|-----------|---------|--------|
| Time to "get it" | New user understands value prop | < 3 minutes |
| Audience engagement | Conference attendees who participate | > 50% |
| Memorable demos | "I was the drone" recall rate | > 80% |
| Theme recognition | Users see relevance to their domain | > 70% |

### Technical Validation

| Criterion | Measure | Target |
|-----------|---------|--------|
| API completeness | Game can express all demo scenarios | 100% |
| Server performance | Concurrent sessions supported | 10+ |
| Multiplayer latency | Action-to-update round trip | < 100ms |
| Concurrent players | Audience mode participants per session | 50+ |
| Theme switching | Change themes via config | Yes |
| Mobile responsive | Operator UI works on phone | Yes |

### Research Utility

| Criterion | Measure | Target |
|-----------|---------|--------|
| Data export | Session data usable in R/Python | Yes |
| Replay fidelity | Can reconstruct any session | 100% |
| Metric coverage | All coagency metrics captured | 100% |
| Head-to-head validity | Controlled experimental conditions | IRB-ready |

### Business Impact

| Criterion | Measure | Target |
|-----------|---------|--------|
| Investor comprehension | "I get it now" reactions | > 80% |
| Sales demo conversion | Leads to follow-up meeting | > 40% |
| Theme relevance | Each theme generates vertical leads | 2+ per theme |
| Conference value | Invitations to present/demo | 3+ events |

## Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Capability algebra doesn't feel intuitive | Confuses players | Medium | Iterate rules based on playtest feedback |
| Scope creep into "real game" | Delays demo | High | Strict phase gates, "it's a demo first" mantra |
| WebSocket scaling issues | Breaks large audiences | Low | Load test early, Fly.io auto-scaling |
| Mobile experience poor | Audience participation suffers | Medium | Mobile-first design for participant UI |
| Conference WiFi unreliable | Demo fails live | Medium | Use mobile hotspot, test beforehand |
| Latency too high for real-time feel | Poor UX | Low | Fly.io edge deployment, optimize message size |
| HIVE core API not game-ready | Blocks integration | Medium | Define game requirements early, adapt API |
| Players don't understand roles | Confusion during demo | Medium | Clear onboarding flow, role tooltips |

### Scope Control

To prevent "game creep," each phase has a clear stopping point:

1. **Phase 1 exit**: Two players can play (Commander + Operator), deployable
2. **Phase 2 exit**: Head-to-head works, 10+ concurrent players
3. **Phase 3 exit**: 30+ audience members can participate via phone
4. **Phase 4 exit**: Coalition scenarios playable with cross-boundary task forces
5. **Phase 5 exit**: Three themes fully functional
6. **Phase 6 exit**: Data export works, one research study designed

Each phase delivers standalone value. Phases can be paused if higher-priority work emerges.

## Alternatives Considered

### Axum Server + React Frontend (Chosen)
- Pro: Server-side multiplayer is the primary use case
- Pro: All game logic in Rust, validated against real HIVE implementation
- Pro: React ecosystem is mature for web + mobile (via Expo)
- Pro: WebSocket protocol allows any client (web, native, CLI)
- Pro: Monorepo keeps types in sync (ts-rs for TypeScript generation)
- Pro: No WASM compilation complexity
- Pro: Easier iteration (server-side updates only for game balance)
- Con: Requires internet for play (acceptable for demo use case)

### Local-Only WASM (Rejected)
- Pro: No server costs, runs entirely in browser
- Con: No multiplayer - the primary use case
- Con: WASM compilation adds complexity
- Con: Misses the core value: Commander + Operators playing together

### Tauri Desktop App (Rejected)
- Pro: Works offline, no internet required
- Pro: All Rust stack
- Con: Distribution friction (downloads, installs)
- Con: Can't easily share sessions via link
- Con: Multiplayer requires networking anyway - defeats the purpose
- Con: Conference demos usually have internet (or hotspot)
- Con: Mobile support immature compared to React Native/Expo

### Pure Bevy Game Engine (Rejected)
- Pro: All Rust, game-engine features
- Con: Web deployment requires WASM anyway
- Con: Overkill for 2D tactical map
- Con: Learning curve for team

### Unity/Unreal (Rejected)
- Pro: Mature game engines
- Con: Can't integrate HIVE Rust code directly
- Con: Massive overhead for simple 2D tactical demo
- Con: Licensing complexity

### Mobile Framework Alternatives

**Capacitor (Not chosen):**
- Pro: Wraps existing web app in native shell
- Con: Still fundamentally a web view, less native feel
- Con: Performance ceiling lower than React Native

**Flutter (Not chosen):**
- Pro: Excellent cross-platform, good performance
- Con: Dart language - different ecosystem from TypeScript/React
- Con: Can't share code with web React components
- Con: Learning curve for team familiar with React

**Native Swift/Kotlin (Not chosen):**
- Pro: Best possible native performance
- Con: Two separate codebases (iOS + Android)
- Con: No web support - would need third codebase
- Con: Much higher development cost

**Expo/React Native (Chosen for Phase 2+):**
- Pro: Same React mental model as web
- Pro: Single codebase → iOS, Android, Web
- Pro: Expo Go enables instant phone testing
- Pro: EAS Build eliminates Xcode/Android Studio pain
- Pro: OTA updates bypass app store review
- Con: Some native features require ejecting from Expo (unlikely for our use case)

## References

- ADR-001: CAP Protocol POC
- ADR-004: Human-Machine Squad Composition
- ADR-014: Distributed Coordination Primitives
- ADR-018: AI Model Capability Advertisement
- Industry Feedback: HIVE Hierarchy Visualization (2024-12-06)

## Decision

Proceed with **Axum server + React frontend** architecture for HIVE Commander, with both components in the **existing cap workspace** (monorepo approach).

### Repository Structure

```
cap/
├── hive-protocol/
├── hive-mesh/
├── hive-sim/
├── hive-commander/           # Rust backend (Axum server) - workspace member
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── game/             # Game session logic
│       ├── ws/               # WebSocket handlers
│       └── types/            # Shared types (generates TypeScript via ts-rs)
│
└── hive-commander-ui/        # Frontend (NOT a cargo crate, just npm project)
    ├── package.json          # React/TypeScript/Vite
    ├── tsconfig.json
    └── src/
```

### Why Monorepo

1. **Private repository**: The cap repo is not yet public, and we're not publishing crates to crates.io
2. **Type sharing**: Backend can generate TypeScript types from Rust structs using `ts-rs`
3. **Tight coupling**: Game server directly consumes `hive-protocol` APIs - changes should be atomic
4. **Single CI**: API breaks are caught immediately when HIVE traits change
5. **Future flexibility**: Can split out when/if we go public

### CI Integration

Add a new CI job for the frontend:

```yaml
  commander-ui:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: hive-commander-ui
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - run: npm ci
      - run: npm run build
      - run: npm test
```

### Targets

1. MVP (Phase 1): Playable 2-player demo
2. Web deployment on revolveteam.com/commander
3. Mobile-friendly participant UI (Expo migration in Phase 2+ if needed)
4. Metrics collection for coagency research

---

*Organization: (r)evolve - Revolve Team LLC*
*URL: https://revolveteam.com*
