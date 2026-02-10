# Army vs Port Domain Comparison

Side-by-side mapping of HIVE's hierarchical aggregation protocol from the
original army/military domain to the port/maritime terminal domain.

## Hierarchy Mapping

| Tier | Army Domain | Port Domain | Role |
|------|-------------|-------------|------|
| Leaf node | Soldier | Port Worker / Sensor | Generates raw state data (position, status, sensor readings) |
| Tier 1 group | Squad (7-8 soldiers) | Hold Team (7-8 stevedores) | Workers in a single cargo hold on a vessel |
| Tier 1 leader | Squad Leader | Hold Foreman | Aggregates worker state into hold summary |
| Tier 2 group | Platoon (3-4 squads) | Berth Operation (3-4 holds) | All operations for one vessel at one berth |
| Tier 2 leader | Platoon Leader | Berth Supervisor | Aggregates hold summaries into vessel/berth summary |
| Tier 3 group | Company (3-4 platoons) | Terminal (3-4 berths) | Entire port terminal with multiple active berths |
| Tier 3 leader | Company Commander | Terminal Manager | Aggregates berth summaries into terminal-wide view |
| C2 | Battalion TOC | Terminal Operations Center (TOC) | Top-level operational picture |

## Aggregation Flow

### Army Domain

```
Soldier state (position, ammo, health)
  └─► Squad Leader aggregates → SquadSummary (readiness, strength, location)
        └─► Platoon Leader aggregates → PlatoonSummary (combat power, posture)
              └─► Company Commander aggregates → CompanySummary (force disposition)
                    └─► Battalion TOC (common operating picture)
```

### Port Domain

```
Port worker state (location, task, sensor readings)
  └─► Hold Foreman aggregates → HoldSummary (cargo progress, crew status, safety)
        └─► Berth Supervisor aggregates → BerthSummary (vessel loading %, crane status)
              └─► Terminal Manager aggregates → TerminalSummary (berth utilization, throughput)
                    └─► Terminal Operations Center (port-wide operational picture)
```

## Data Types per Tier

| Army Data | Port Data | Aggregation |
|-----------|-----------|-------------|
| Soldier position (GPS) | Worker position (RTLS/BLE beacon) | Centroid per hold |
| Ammo count | Cargo units moved | Sum per hold → per berth → per terminal |
| Health status (GREEN/AMBER/RED) | Safety status (clear/caution/stop) | Worst-case roll-up |
| Equipment status | Crane/equipment status | Availability percentage |
| Mission task state | Work order progress | Completion percentage |
| Contact reports | Incident reports | Count + severity roll-up |

## Phase 3 Metrics — Domain Comparison

### P3-1: Convergence

| Aspect | Army | Port |
|--------|------|------|
| What converges | Battalion TOC common operating picture | Terminal-wide operational dashboard |
| Input events | Soldier state updates (position, status) | Worker sensor readings, cargo scan events |
| Convergence path | Soldier → Squad → Platoon → Company → TOC | Worker → Hold → Berth → Terminal → TOC |
| Threshold | < 30 seconds | < 30 seconds |
| Why it matters | Commander needs timely situational awareness | Terminal manager needs real-time berth status for vessel scheduling |

### P3-2: Scaling

| Aspect | Army | Port |
|--------|------|------|
| Scale unit | Soldiers per company | Workers + sensors per terminal |
| Growth pattern | Add squads/platoons | Add berths, add holds per vessel, add workers per hold |
| O(n^2) failure mode | Every soldier syncs with every other soldier | Every sensor floods every other sensor |
| O(n log n) solution | Hierarchical aggregation — squad leaders summarize | Hold foreperson summarizes per hold, berth supervisor per berth |
| Validation | Message count per node grows < n | Message count per node grows < n |

### P3-3: End-to-End Latency

| Aspect | Army | Port |
|--------|------|------|
| Source | Soldier sensor (GPS, health) | Port worker sensor (RTLS tag, cargo scanner) |
| Destination | Battalion TOC display | Terminal Operations Center dashboard |
| Path | 3-4 aggregation hops | 3-4 aggregation hops |
| Threshold | < 10 seconds (P99) | < 10 seconds (P99) |
| Why it matters | Tactical decisions require near-real-time data | Crane scheduling, safety interlocks need timely data |

### P3-4: Isolation / Concurrent Operations

| Aspect | Army | Port |
|--------|------|------|
| Isolation boundary | Platoon (each platoon operates independently) | Berth (each vessel operation is independent) |
| Concurrent ops | Multiple platoons on different objectives | Multiple ships loading/unloading at different berths |
| Failure mode | Platoon A data leaks to Platoon B peers | Berth 1 cargo data leaks to Berth 2 workers |
| Validation | Zero cross-platoon peer document reception | Zero cross-berth peer document reception |
| Why it matters | Operational security, bandwidth efficiency | Safety (wrong cargo plan), regulatory (customs per vessel) |

## Communication Patterns

### Downward (Command Dissemination)

| Army | Port |
|------|------|
| Battalion issues OPORD to companies | Terminal issues work plan to berths |
| Company relays FRAGO to platoons | Berth supervisor relays revised stow plan to holds |
| Platoon relays orders to squads | Hold foreman relays task assignments to workers |
| Squad leader directs soldiers | Foreman directs stevedores |

### Upward (Status Aggregation)

| Army | Port |
|------|------|
| Soldiers report SITREP | Workers report task completion / sensor readings |
| Squad leader sends squad SITREP | Hold foreman sends hold progress report |
| Platoon leader sends platoon SITREP | Berth supervisor sends vessel loading status |
| Company sends company SITREP to TOC | Terminal manager sends terminal throughput to TOC |

## Bandwidth & Connectivity Constraints

| Constraint | Army Analog | Port Analog |
|------------|-------------|-------------|
| High bandwidth (1 Gbps) | Garrison / fiber-connected TOC | Shore-side terminal LAN |
| Medium bandwidth (100 Mbps) | Tactical WiFi / MANET | Shipboard WiFi / crane-mounted AP |
| Low bandwidth (1 Mbps) | HF radio / degraded SATCOM | Inside cargo hold (steel hull attenuation) |
| Very low bandwidth (256 Kbps) | Iridium SBD / contested spectrum | Ship-to-shore via SBD / offshore anchorage |

## Key Architectural Invariants (Both Domains)

1. **Hierarchical aggregation reduces message volume**: O(n log n) vs O(n^2)
2. **Each tier only communicates with its immediate neighbors**: workers talk to their foreman, not to the terminal manager
3. **Summaries flow up, commands flow down**: bidirectional but asymmetric
4. **Isolation between peer groups**: Berth 1 workers don't receive Berth 2 data at the peer level
5. **Leaders bridge tiers**: the hold foreman is the single point of aggregation for one hold
6. **The protocol is domain-agnostic**: the same HIVE hierarchical aggregation works for army, port, or any domain with a tree-structured organization
