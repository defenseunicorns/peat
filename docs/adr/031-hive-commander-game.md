# ADR-031: Peat Commander - Tactical Capability RPG

## Status
**Superseded** — peat-commander has been removed from the workspace and will be redeveloped as an external repository. This ADR is retained for historical context.

*Previously: Proposed (TUI prototype complete)*

## Context

Industry feedback identified two critical gaps in Peat's current demonstration capability:

1. **Visualization Gap**: "You need to be able to visualize the hierarchy in some simple C2 map, video game kind of way - showing individual capabilities being aggregated into emergent capabilities, and tasking being redistributed back out by the player"

2. **Cross-Boundary Coordination Gap**: "Can you take one asset from one squad and another asset from a different squad and task them as a new group? This would be super valuable, especially if they are crossing ownership boundaries (operated by two different countries)"

Additionally, the concept of **coagency performance** - measuring how well human-machine-AI teams perform together - was identified as a research differentiator that Peat could demonstrate through interactive gameplay.

### Why Not RTS?

Real-time strategy games are chaotic, hard to follow in demos, and don't naturally emphasize **composition** - the core value of Peat. Players focus on micro-management and APM rather than thoughtful capability aggregation.

### Why D&D-Style Tactical RPG?

Dungeons & Dragons is fundamentally about **party composition**:
- "Do we have the right mix of capabilities to handle this encounter?"
- "Who has the skill we need? Can we combine abilities?"
- "The rogue can't pick the lock alone, but with the wizard's guidance spell..."

This is *exactly* what Peat does - matching task requirements to composed capabilities. The D&D framing makes this intuitive and memorable.

## Decision

Build **Peat Commander**, a turn-based tactical RPG that uses the actual Rust Peat reference implementation to coordinate heterogeneous assets on a 3D terrain map. The game emphasizes **capability composition** through D&D-style skill checks and party mechanics.

### Core Design Principles

1. **Composition is the game** - Victory comes from clever capability combinations, not twitch reflexes
2. **Turn-based for clarity** - Audience can follow the action in demos
3. **DM = Presenter** - The presenter controls scenarios, introduces challenges
4. **Skill checks = Capability matching** - D&D's core mechanic maps perfectly to Peat
5. **3D terrain map** - Spatial context with elevation, cover, and line-of-sight
6. **Hierarchy through zoom** - Zoomed out shows composed capabilities, drill down for details

---

## Game Design

### The Peat Party System

Instead of D&D's Fighter/Wizard/Rogue classes, Peat Commander has **Capability Classes**:

| Class | Role | Base Capabilities | D&D Analog |
|-------|------|-------------------|------------|
| **Sensor** | Detection, tracking | `DETECT`, `TRACK`, `IDENTIFY` | Ranger/Scout |
| **Scout** | Recon, mobility | `RECON`, `FAST_MOVE`, `STEALTH` | Rogue |
| **Striker** | Kinetic effects | `STRIKE`, `SUPPRESS`, `BREACH` | Fighter |
| **Support** | Logistics, comms | `RELAY`, `RESUPPLY`, `REPAIR` | Cleric |
| **Authority** | Human-in-loop | `AUTHORIZE`, `OVERRIDE`, `COMMAND` | Paladin |
| **Analyst** | AI/ML processing | `CLASSIFY`, `PREDICT`, `FUSE` | Wizard |

Each **piece** (asset) on the board is one of these classes with specific capability stats.

### Capability Stats (The "Character Sheet")

Every piece has a capability profile:

```
┌─────────────────────────────────────────────────────────────────┐
│  SCOUT DRONE "ALPHA-1"                          Class: Sensor  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  CAPABILITIES                        STATS                      │
│  ├── DETECT (EO)        +3          Movement: 4 hexes          │
│  ├── DETECT (IR)        +2          Range: 3 hexes             │
│  ├── TRACK              +2          Fuel: ████████░░ 80%       │
│  └── IDENTIFY           +1          Health: ██████████ 100%    │
│                                                                 │
│  SYNERGIES                                                      │
│  └── +1 to TRACK when paired with Analyst                      │
│                                                                 │
│  LIMITATIONS                                                    │
│  └── Cannot AUTHORIZE (requires Authority class)               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The Skill Check System

When a task requires capabilities, Peat Commander uses D&D-style skill checks:

```
┌─────────────────────────────────────────────────────────────────┐
│  ENCOUNTER: "Track High-Value Target at Grid C7"               │
│                                                                 │
│  REQUIRED CAPABILITIES:                                         │
│  ├── DETECT (any)       DC 10                                  │
│  ├── TRACK              DC 12                                  │
│  └── AUTHORIZE          DC 8  (human-in-loop required)         │
│                                                                 │
│  YOUR PARTY (assets in range):                                 │
│  ├── Scout Drone Alpha-1:  DETECT +3, TRACK +2                 │
│  ├── Ground Robot Beta-2:  DETECT +1                           │
│  └── Operator Human-1:     AUTHORIZE +4                        │
│                                                                 │
│  COMPOSITION BONUSES:                                           │
│  ├── Multi-sensor fusion: +2 to DETECT (two sensors)           │
│  ├── Elevation advantage: +1 to TRACK (drone on hill)          │
│  └── Authority present:   +1 to all checks                     │
│                                                                 │
│  ROLL RESULTS:                                                  │
│  ├── DETECT:    Roll 8 + 3 (drone) + 1 (robot) + 2 (fusion)    │
│  │              = 14 vs DC 10 ✓ SUCCESS                        │
│  ├── TRACK:     Roll 11 + 2 (drone) + 1 (elevation)            │
│  │              = 14 vs DC 12 ✓ SUCCESS                        │
│  └── AUTHORIZE: Roll 6 + 4 (human)                              │
│                 = 10 vs DC 8 ✓ SUCCESS                         │
│                                                                 │
│  OUTCOME: Target tracked! +50 points, target revealed on map   │
└─────────────────────────────────────────────────────────────────┘
```

### Turn Structure

Each round has phases that mirror Peat's coordination flow:

```
┌─────────────────────────────────────────────────────────────────┐
│                         ROUND STRUCTURE                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  PHASE 1: ENCOUNTER (DM/System)                                 │
│  ├── New objectives appear on map                               │
│  ├── Enemy actions resolve                                      │
│  └── Environmental changes (weather, comms, etc.)              │
│                                                                 │
│  PHASE 2: PLANNING (Commander)                                  │
│  ├── View capability requirements for each objective           │
│  ├── See Peat's recommended compositions                       │
│  └── Decide which objectives to pursue                         │
│                                                                 │
│  PHASE 3: MOVEMENT (All Players)                                │
│  ├── Each piece moves up to its movement value                 │
│  ├── Terrain affects movement (hills slow, roads speed)        │
│  └── Moving into proximity enables composition                  │
│                                                                 │
│  PHASE 4: COMPOSITION (Commander)                               │
│  ├── Group pieces into parties (drag-select or tap)            │
│  ├── See emergent capabilities from composition                │
│  └── Assign parties to objectives                               │
│                                                                 │
│  PHASE 5: RESOLUTION (System)                                   │
│  ├── Skill checks for each objective                           │
│  ├── Dice rolls with composition bonuses                       │
│  ├── Success/failure narration                                  │
│  └── Points awarded, map state updates                         │
│                                                                 │
│  PHASE 6: UPKEEP                                                │
│  ├── Fuel consumption                                           │
│  ├── Asset recovery (damaged pieces heal)                      │
│  └── Reinforcement spawns (if earned)                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The 3D Terrain Map

The game board is a 3D terrain map with tactical significance:

```
┌─────────────────────────────────────────────────────────────────┐
│                        3D TERRAIN MAP                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  TERRAIN TYPES                      EFFECTS                     │
│  ─────────────────────────────────────────────────────────────  │
│  🏔️ Hills/Elevation                +1 to DETECT/TRACK range    │
│                                    -1 to ground movement        │
│                                                                 │
│  🌲 Forest/Cover                   +2 to STEALTH checks         │
│                                    Blocks line-of-sight         │
│                                                                 │
│  🏢 Urban/Buildings                BREACH required to enter     │
│                                    Cover from STRIKE            │
│                                                                 │
│  🛣️ Roads                          +2 to ground movement        │
│                                    No cover                     │
│                                                                 │
│  📡 Comm Towers                    Extends RELAY range          │
│                                    High-value objective         │
│                                                                 │
│  ⚡ Jamming Zones                  -2 to all RELAY checks       │
│                                    FUSE capability negates      │
│                                                                 │
│  ZOOM LEVELS                                                    │
│  ─────────────────────────────────────────────────────────────  │
│  Strategic (zoomed out):  See composed capabilities as icons   │
│  Tactical (mid):          See individual pieces + composition   │
│  Detail (zoomed in):      See piece stats, terrain bonuses     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Hierarchy Through Zoom

The 3D map implements hierarchy visualization through zoom levels:

```
┌─────────────────────────────────────────────────────────────────┐
│  ZOOMED OUT (Commander's View)                                  │
│                                                                 │
│     [🎯 PERSISTENT_ISR]          [⚔️ STRIKE_PACKAGE]            │
│           Grid B4                      Grid D7                  │
│                                                                 │
│  You see: Composed capabilities as single icons                │
│  Hierarchy level: Formation / Task Force                        │
├─────────────────────────────────────────────────────────────────┤
│  TAP TO DRILL DOWN...                                           │
├─────────────────────────────────────────────────────────────────┤
│  ZOOMED IN (Operator's View)                                    │
│                                                                 │
│     [🎯 PERSISTENT_ISR]                                         │
│     ├── 🛩️ Scout Drone Alpha-1 (DETECT+3, TRACK+2)             │
│     ├── 🛩️ Scout Drone Alpha-2 (DETECT+2, TRACK+3)             │
│     └── 🤖 Analyst Bot Gamma-1 (CLASSIFY+3, FUSE+2)            │
│                                                                 │
│  You see: Individual pieces that compose the capability        │
│  Hierarchy level: Squad / Individual assets                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Random Draft (Force Composition)

At game start, players don't choose their pieces - they're dealt a random hand:

```
┌─────────────────────────────────────────────────────────────────┐
│  DRAFT PHASE                                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  BLUE TEAM receives:                 RED TEAM receives:         │
│  ├── 2x Sensor (Scout Drones)        ├── 3x Sensor             │
│  ├── 1x Scout (Recon Bot)            ├── 1x Scout              │
│  ├── 2x Striker (Strike Drones)      ├── 1x Striker            │
│  ├── 1x Support (Relay Node)         ├── 2x Support            │
│  └── 1x Authority (Human Op)         └── 1x Authority          │
│                                                                 │
│  "You have strong strike capability but limited ISR.            │
│   They have excellent sensors but weak kinetics.                │
│   How will you compose your way to victory?"                    │
│                                                                 │
│  OPTIONAL: Trade/Coalition                                      │
│  └── In multiplayer, teams can negotiate asset trades          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Encounter Types

The DM (presenter/system) introduces encounters that require specific compositions:

| Encounter | Required Capabilities | Composition Challenge |
|-----------|----------------------|----------------------|
| **Track HVT** | DETECT + TRACK + AUTHORIZE | Need sensor + authority |
| **Secure Area** | DETECT + SUPPRESS + RELAY | Continuous presence |
| **Strike Target** | DETECT + IDENTIFY + STRIKE + AUTHORIZE | Full kill chain |
| **Establish Comms** | RELAY + RELAY (redundancy) | Two support assets |
| **Breach Compound** | BREACH + SUPPRESS + DETECT | Combined arms |
| **Rescue Asset** | DETECT + FAST_MOVE + EXTRACT | Speed + awareness |
| **Jam Enemy** | FUSE + RELAY + COMMAND | Electronic warfare |

### Composition Bonuses (Synergies)

When pieces work together, they gain bonuses beyond their individual stats:

```
┌─────────────────────────────────────────────────────────────────┐
│  COMPOSITION SYNERGIES                                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MULTI-SENSOR FUSION                                            │
│  └── 2+ Sensors in party: +2 to all DETECT checks              │
│                                                                 │
│  HUMAN-MACHINE TEAMING                                          │
│  └── Authority + any other class: +1 to all checks             │
│                                                                 │
│  AI-AUGMENTED OPS                                               │
│  └── Analyst + Sensor: +2 to IDENTIFY, enables PREDICT         │
│                                                                 │
│  COMBINED ARMS                                                  │
│  └── Striker + Sensor + Authority: +3 to STRIKE               │
│                                                                 │
│  PERSISTENT PRESENCE                                            │
│  └── 2+ of same class: Redundancy - can lose one, keep bonus  │
│                                                                 │
│  MESH NETWORK                                                   │
│  └── 2+ Support in range: +1 to RELAY per additional node     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The DM Role (Presenter)

In demo mode, the presenter acts as Dungeon Master:

```
┌─────────────────────────────────────────────────────────────────┐
│  DM CONTROLS (Presenter Dashboard)                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ENCOUNTER DECK                                                 │
│  ├── [Draw Next Encounter]                                      │
│  ├── [Custom Encounter] - design on the fly                    │
│  └── [Boss Encounter] - high-stakes finale                     │
│                                                                 │
│  DIFFICULTY ADJUSTMENT                                          │
│  ├── DC Modifier: [-2] [-1] [0] [+1] [+2]                      │
│  └── "Make it easier/harder based on audience"                 │
│                                                                 │
│  DRAMA CONTROLS                                                 │
│  ├── [Introduce Complication] - jamming, weather, asset loss   │
│  ├── [Reinforcements] - give team new pieces                   │
│  └── [Narrate] - add flavor text to outcomes                   │
│                                                                 │
│  TEACHING MOMENTS                                               │
│  ├── [Highlight Composition] - show why this combo worked      │
│  ├── [Show Peat Recommendation] - "Peat suggests..."           │
│  └── [Pause for Q&A] - freeze game state                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Multiplayer Modes

### Mode 1: Solo Practice
- Player controls all pieces
- System acts as DM, drawing encounters
- Good for learning mechanics

### Mode 2: Commander + Operators
- One player as Commander (strategic view)
- Other players control individual pieces (operator view)
- Commander assigns objectives, operators execute movement
- **Best for conference demos**

### Mode 3: Head-to-Head
- Two commanders compete for objectives
- Each has their own randomly drafted pieces
- Contested objectives in shared map area
- **Best for game theory research**

### Mode 4: Coalition
- Multiple commanders with separate pieces
- Must negotiate to form cross-boundary parties
- Authority delegation mechanics
- **Best for demonstrating AUKUS/NATO scenarios**

### Audience Participation Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  CONFERENCE DEMO SETUP                                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Presenter creates game session                              │
│     └── Selects scenario, theme, difficulty                    │
│                                                                 │
│  2. Displays QR code / short link                               │
│     └── "Join at peat.game/ABC123"                             │
│                                                                 │
│  3. Attendees join on phones                                    │
│     ├── Select available piece to control                       │
│     └── See their piece's capability card                      │
│                                                                 │
│  4. Presenter acts as Commander + DM                            │
│     ├── Introduces encounters                                   │
│     ├── Asks "who has DETECT capability?"                      │
│     └── Forms parties from audience pieces                     │
│                                                                 │
│  5. Skill checks with audience                                  │
│     ├── "Alpha-1, roll for DETECT!"                            │
│     ├── Attendee taps to roll on their phone                   │
│     └── Result appears on main screen                          │
│                                                                 │
│  6. Narrate outcomes                                            │
│     └── "The scout drone spots the target - but wait,          │
│          we need AUTHORIZE! Who has Authority class?"          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Technology Stack

### Server-Side Multiplayer (Same as before)

```
┌─────────────────────────────────────────────────────────────────┐
│                        Peat Commander                           │
├─────────────────────────────────────────────────────────────────┤
│  Frontend (TypeScript/React)                                    │
│  ├── 3D Map Renderer (Three.js or React Three Fiber)           │
│  ├── Piece Visualization with capability cards                 │
│  ├── Zoom-based hierarchy view                                  │
│  ├── Skill check UI with dice animation                        │
│  ├── Mobile operator view (phone-optimized)                    │
│  └── DM dashboard                                               │
├─────────────────────────────────────────────────────────────────┤
│  WebSocket Connection                                           │
├─────────────────────────────────────────────────────────────────┤
│  Backend (Rust/Axum)                                            │
│  ├── Peat Reference Implementation                              │
│  │   ├── Capability Documents (Automerge CRDTs)                │
│  │   ├── Composition Engine (calculate synergies)              │
│  │   ├── Task-Capability Matching (DC calculation)             │
│  │   └── Skill Check Resolution                                │
│  ├── Game Session Management                                    │
│  ├── Turn/Phase State Machine                                   │
│  ├── Encounter Deck & DM Controls                               │
│  └── Metrics Collection                                         │
└─────────────────────────────────────────────────────────────────┘
```

### Repository Structure (Monorepo)

```
cap/
├── peat-protocol/
├── peat-mesh/
├── peat-sim/
├── peat-commander/           # Rust backend (Axum server)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── game/
│       │   ├── session.rs        # Game session management
│       │   ├── turn.rs           # Turn/phase state machine
│       │   ├── skill_check.rs    # D20 + modifiers resolution
│       │   ├── composition.rs    # Synergy calculation
│       │   └── encounter.rs      # Encounter deck
│       ├── ws/                   # WebSocket handlers
│       └── types/                # Shared types (ts-rs)
│
└── peat-commander-ui/        # Frontend (React/TypeScript)
    ├── package.json
    └── src/
        ├── components/
        │   ├── Map3D/            # Three.js terrain map
        │   ├── PieceCard/        # Capability stat cards
        │   ├── SkillCheck/       # Dice roll UI
        │   ├── PartyComposer/    # Drag-select grouping
        │   └── DMDashboard/      # Presenter controls
        └── views/
            ├── CommanderView/    # Strategic zoom level
            ├── OperatorView/     # Mobile piece control
            └── SpectatorView/    # Watch-only
```

---

## Validation Objectives

### 1. Capability Composition (Core Mechanic)

The skill check system directly validates Peat's composition engine:

```rust
// Does Peat's composition bonus calculation match player intuition?
fn calculate_party_modifier(party: &[Piece], check_type: Capability) -> i32 {
    let base = party.iter()
        .map(|p| p.get_modifier(check_type))
        .max()
        .unwrap_or(0);

    let synergy = calculate_synergies(party);
    let terrain = calculate_terrain_bonus(party, check_type);

    base + synergy + terrain
}
```

**Validation question**: When players form parties, do the computed bonuses feel right? Do synergies reward smart composition?

### 2. Hierarchy Visualization

The zoom mechanic validates that hierarchy is intuitive:

- **Zoomed out**: Do composed capabilities read as coherent units?
- **Drill down**: Is it clear how individual pieces contribute?
- **Cross-boundary**: Do coalition parties show ownership correctly?

### 3. Task-Capability Matching

Encounters validate that DC calculation makes sense:

- Are "impossible" encounters actually impossible without the right composition?
- Do "easy" encounters feel appropriately simple?
- Does Peat's recommendation system suggest good parties?

### 4. Cross-Boundary Coordination

Coalition mode validates AUKUS/NATO scenarios:

- Can players form parties across ownership boundaries?
- Does authority delegation work correctly?
- Is it clear which pieces belong to which owner?

---

## Theming

The D&D-style mechanics support multiple themes:

### Theme: Military Operations (Default)

| Core Concept | Military Skin |
|--------------|---------------|
| Piece classes | Platform types (UAV, UGV, Operator) |
| Capability checks | Mission capabilities |
| Encounters | Tactical objectives |
| Terrain | Operational environment |

### Theme: Cellular Biology

| Core Concept | Biology Skin |
|--------------|--------------|
| Piece classes | Cell types (T-Cell, Macrophage, etc.) |
| Capability checks | Immune responses |
| Encounters | Pathogen threats |
| Terrain | Body systems |

### Theme: Logistics

| Core Concept | Logistics Skin |
|--------------|----------------|
| Piece classes | Fleet assets (Truck, Drone, Warehouse) |
| Capability checks | Delivery capabilities |
| Encounters | Fulfillment challenges |
| Terrain | Supply chain network |

---

## MVP Scope

### Phase 1: Core Turn-Based Engine

- [ ] Turn structure (phases)
- [ ] Skill check system (d20 + modifiers)
- [ ] Basic composition bonuses
- [ ] 2D map with hex grid (3D in Phase 2)
- [ ] 5 piece classes
- [ ] 5 encounter types
- [ ] Solo mode
- [ ] Basic web UI

**Deliverable**: Single-player tactical demo

### Phase 2: Multiplayer + 3D Map

- [ ] WebSocket multiplayer
- [ ] Commander + Operator roles
- [ ] 3D terrain map with Three.js
- [ ] Zoom-based hierarchy view
- [ ] Mobile operator UI
- [ ] DM dashboard

**Deliverable**: Conference demo capability

### Phase 3: Head-to-Head + Coalition

- [ ] Competitive mode (two commanders)
- [ ] Coalition mode (asset trading)
- [ ] Cross-boundary parties
- [ ] Authority delegation
- [ ] Victory conditions and scoring

**Deliverable**: Game theory research platform

### Phase 4: Polish + Themes

- [ ] Full terrain system
- [ ] Dice roll animations
- [ ] Sound effects
- [ ] Alternative themes
- [ ] Campaign mode (linked scenarios)

**Deliverable**: Production-quality demo tool

---

## Success Criteria

### Demo Effectiveness

| Criterion | Measure | Target |
|-----------|---------|--------|
| Time to "get it" | New user understands composition | < 2 minutes |
| Engagement | "I was the drone" recall | > 90% |
| Teaching value | Audience asks about Peat | > 50% |

### Technical Validation

| Criterion | Measure | Target |
|-----------|---------|--------|
| Skill check accuracy | Computed DC matches intuition | > 80% |
| Composition bonuses | Synergies reward good play | Playtest verified |
| Turn latency | Phase transitions | < 500ms |

### Research Utility

| Criterion | Measure | Target |
|-----------|---------|--------|
| Composition data | Which synergies players discover | Logged |
| Strategy patterns | Winning compositions | Analyzed |
| Human-AI teaming | Peat recommendation acceptance | Tracked |

---

## Alternatives Considered

### RTS (Real-Time Strategy) - Rejected
- Pro: Exciting, immediate
- Con: Chaotic, hard to follow in demos
- Con: Doesn't emphasize composition (the core Peat value)
- Con: Favors micro-management over thoughtful coordination

### Pure Chess - Rejected
- Pro: Simple, well-understood
- Con: Fixed pieces, no composition
- Con: Abstract board doesn't show spatial/terrain context
- Con: No narrative drama

### Card Game (Magic-style) - Rejected
- Pro: Composition through deck building
- Con: No spatial element (loses 3D map visualization)
- Con: Abstract, not tangible for C2 demos

### D&D-Style Tactical RPG - Chosen
- Pro: Party composition is the core mechanic (exactly Peat)
- Pro: Skill checks map directly to capability matching
- Pro: Turn-based means audience can follow
- Pro: DM role fits presenter perfectly
- Pro: 3D terrain adds spatial context
- Pro: Memorable narrative ("remember when the drone rolled a nat 20?")

---

## Prototyping Notes

### TUI Prototype (peat-commander crate)

A terminal-based prototype was created to validate the intent-based command model concepts before investing in full graphical UI development.

**Location**: `peat-commander/` crate in the workspace

**What was built**:
- Procedural terrain generation using Perlin noise (water, plains, forest, hills, mountains, urban, base)
- Composed capability calculation from asset proximity (3-hex grouping)
- Objective spawning with capability requirements (DET, TRK, STR, AUTH)
- COA generation based on capability-to-objective matching
- Turn-based gameplay with Select Objective → Select COA → Execute flow
- Fog of war (enemies visible only within detection range)
- Fuel management (only regenerates at base tiles)

**Key learnings**:
1. The intent-based command model feels right - selecting objectives and COAs is more strategic than moving individual pieces
2. Composed capabilities effectively abstract away individual asset management
3. Terminal UI has fundamental limitations for spatial reasoning in tactical scenarios
4. A proper graphical UI (React Canvas/Three.js or game engine) is required for intuitive gameplay

**Run the prototype**:
```bash
cargo run -p peat-commander
```

**Controls**:
- `[1-3]` Select objective
- `[A-C]` Select COA
- `[ENTER]` Execute
- `[R]` New game
- `[Q]` Quit

**Recommendation**: The TUI prototype validated the core game mechanics. Future development should proceed with the React/Three.js frontend described in this ADR, using the TUI prototype as a reference implementation for game logic.

---

## References

- ADR-001: CAP Protocol POC
- ADR-004: Human-Machine Squad Composition
- ADR-014: Distributed Coordination Primitives
- Industry Feedback: Peat Hierarchy Visualization (2024-12-06)

---

*Organization: Defense Unicorns*
*URL: https://defenseunicorns.com*
