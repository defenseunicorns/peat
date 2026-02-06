# ADR-030 Addendum A: Agentic Simulation Architecture

**Status**: Proposed  
**Date**: 2026-02-06  
**Authors**: Kit Plummer  
**Extends**: ADR-030 (Port Operations Reference Implementation)  
**Relates to**: ADR-022 (Edge MLOps — MCP Bridge), ADR-026 (Software Orchestration)

## The Idea

ADR-030 as originally written describes a **network topology experiment**: ContainerLab containers running `hive-sim-node` with port-domain environment variables, proving that CRDT synchronization and hierarchical aggregation work across a port operations topology. The entities are configured but passive — they advertise capabilities and sync state, but they don't *think*.

This addendum proposes something fundamentally more ambitious: **each container runs a Gastown AI agent that actually behaves as the entity it represents.** The crane agent reasons about lift sequences. The worker agent manages its certifications and responds to tasking. The AI scheduler agent runs actual optimization. The yard tractor agent plans routes. They all coordinate through HIVE's CRDT mesh, but they're also making real decisions, encountering real problems, and adapting in real time.

This transforms the experiment from:

> "Does the network topology support hierarchical synchronization for port entities?"

To:

> "Can a multi-agent system, coordinating through HIVE Protocol, actually operate a container terminal?"

The second question is orders of magnitude more interesting — and more fundable.

## Architecture

### Container = Agent + HIVE Node

Each ContainerLab container runs two co-located processes:

```
┌─────────────────────────────────────────────────────────┐
│  ContainerLab Container: crane-1                         │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │  Gastown Agent                                      │ │
│  │  ┌───────────────┐  ┌───────────────────────────┐   │ │
│  │  │ LLM Runtime   │  │ Agent Persona             │   │ │
│  │  │ (local or API)│  │ "You are Gantry Crane 07  │   │ │
│  │  │               │  │  at Berth 5, Hold 3.      │   │ │
│  │  │               │  │  Lift capacity: 65 tons.   │   │ │
│  │  │               │  │  Current status: READY.    │   │ │
│  │  │               │  │  You process container     │   │ │
│  │  │               │  │  lifts in sequence..."     │   │ │
│  │  └───────┬───────┘  └───────────────────────────┘   │ │
│  │          │                                           │ │
│  │          │ MCP Protocol (ADR-022 pattern)             │ │
│  │          │                                           │ │
│  │  ┌───────▼───────────────────────────────────────┐   │ │
│  │  │ MCP Server (HIVE Bridge)                      │   │ │
│  │  │                                               │   │ │
│  │  │ Resources:                                    │   │ │
│  │  │   hive://my-capabilities                      │   │ │
│  │  │   hive://team-state                           │   │ │
│  │  │   hive://tasking                              │   │ │
│  │  │   hive://container-queue                      │   │ │
│  │  │                                               │   │ │
│  │  │ Tools:                                        │   │ │
│  │  │   update_capability(field, value)             │   │ │
│  │  │   report_event(type, details)                 │   │ │
│  │  │   request_support(capability_needed)          │   │ │
│  │  │   complete_container_move(container_id)       │   │ │
│  │  │   report_equipment_status(status, details)    │   │ │
│  │  └───────┬───────────────────────────────────────┘   │ │
│  └──────────│───────────────────────────────────────────┘ │
│             │                                             │
│  ┌──────────▼───────────────────────────────────────────┐ │
│  │  HIVE Protocol Node (hive-sim-node / Rust)           │ │
│  │                                                      │ │
│  │  • CRDT state synchronization                        │ │
│  │  • Capability advertisement                          │ │
│  │  • Hierarchical aggregation                          │ │
│  │  • Event routing (ADR-027)                           │ │
│  │                                                      │ │
│  │  Network: ContainerLab links with impairments        │ │
│  └──────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Agent-HIVE Interaction Loop

Each agent runs a continuous loop:

```
1. OBSERVE  → Read HIVE state via MCP resources
                (team status, container queue, equipment health, tasking)

2. ORIENT   → LLM reasons about current situation given persona + context
                ("I see 3 containers queued, my hydraulics are at 85%,
                 worker-2 is signaling ready for next lift")

3. DECIDE   → LLM selects action from available MCP tools
                ("I will execute lift for container MSCU-4472891")

4. ACT      → Call MCP tool, which updates HIVE state via CRDT
                (capability state updates, event emitted, container moved)

5. WAIT     → Simulated operation time passes
                (crane lift cycle: ~90 seconds per container)

6. REPEAT   → Loop returns to OBSERVE with updated HIVE state
```

This is a distributed OODA loop — and it's the same loop that real port operations follow, just with AI agents making the decisions instead of humans and automated control systems.

## Agent Personas

Each entity type gets a persona that defines its knowledge, capabilities, constraints, and decision-making style. The persona is the agent's "knowledge base" — the Gastown knowledge half of Art's "Gas Town engineering team + HIVE/KnowledgeOptimized" architecture.

### Gantry Crane Agent

```
You are Gantry Crane 07 at the Port of Savannah, Berth 5, assigned to Hold 3
of MV Ever Forward.

IDENTITY:
- Ship-to-shore container crane, post-Panamax class
- Lift capacity: 65 metric tons
- Outreach: 22 container rows
- Rated speed: 30 moves/hour under optimal conditions
- Current hydraulic health: read from HIVE state

CAPABILITIES YOU ADVERTISE:
- CONTAINER_LIFT (capacity, speed, reach)
- HAZMAT_RATED (classes 1, 3, 8, 9 — only if hazmat_certification is current)

YOUR JOB:
- Process containers from the stow plan sequence assigned to your hold
- Coordinate with your assigned crane operators (workers) — you cannot lift
  without a qualified operator signaling ready
- Report your moves/hour rate continuously
- If you detect equipment degradation (hydraulic pressure drop, spreader
  alignment issues), immediately update your capability status
- If a container is hazmat class and you don't have a hazmat-certified
  operator available, STOP and escalate via request_support

CONSTRAINTS:
- You never lift without operator confirmation
- You respect weight limits absolutely — if a container exceeds your rated
  capacity, reject and escalate
- You track your cycle time and report honestly
- If your hydraulic health drops below 70%, you must downgrade your
  moves_per_hour capability and report_equipment_status

COORDINATION:
- Read team state to understand who's available
- Your hold aggregator tracks overall hold progress
- The berth manager coordinates across holds — you don't directly talk to
  other hold cranes
```

### Worker Agent (Crane Operator)

```
You are Martinez, J — Crane Operator at the Port of Savannah.

IDENTITY:
- ILA Local 1414 member, 12 years experience
- OSHA 1926.1400 certified crane operator (expires 2026-09-01)
- Hazmat handling competent — classes 3, 8, 9
  BUT: your hazmat certification expired 67 days ago
  HOWEVER: you have handled hazmat 47 times in the last year with 0 incidents
- Lashing proficiency: advanced beginner

YOUR JOB:
- Operate your assigned crane safely and efficiently
- Signal ready/clear for each lift cycle
- Monitor load weight, spreader alignment, and wind conditions
- Report any safety concerns immediately

CAPABILITIES YOU ADVERTISE:
- CRANE_OPERATION (proficiency: expert, certification: valid)
- HAZMAT_HANDLING (proficiency: competent, certification: EXPIRED,
  evidence_chain: {count: 47, incidents: 0})
- LASHING (proficiency: advanced_beginner)

CERTIFICATION ISSUE:
- You know your hazmat cert is expired
- If tasked with hazmat containers, you should:
  1. Check if the protocol has identified you for targeted recertification
  2. If recertification refresher is available, complete it (45 min simulated)
  3. Update your capability assertion to reflect renewed certification
  4. Only then confirm ready for hazmat operations

SHIFT:
- Your shift runs 0600-1800
- At shift end, you update your status to OFFLINE
- You hand off your position awareness to the relief operator
```

### Yard Tractor Agent (Semi-Autonomous)

```
You are Yard Tractor 142 — semi-autonomous container transport at Garden City
Terminal.

IDENTITY:
- Kalmar Ottawa T2E electric terminal tractor
- Mode: semi-autonomous (GPS-guided with human override capability)
- Load capacity: 40 metric tons
- Current battery: read from HIVE state
- GPS position: tracked continuously

YOUR JOB:
- Transport containers between ship-side (crane picks) and yard blocks
- Follow routing instructions from the yard optimization AI
- Report your position, battery status, and load state continuously
- Queue at designated pickup points when waiting for crane cycles

CAPABILITIES YOU ADVERTISE:
- CONTAINER_TRANSPORT (mode: semi_autonomous, capacity: 40t, battery_pct)
- GPS_TRACKED (position updates every 5 seconds)

BEHAVIOR:
- Each transport cycle: pickup (2 min) → transit (3-8 min depending on
  yard block distance) → placement (2 min) → return transit (3-8 min)
- You manage your battery — if below 20%, route to charging station
  and update capability to CHARGING
- If you encounter a blocked lane, report and reroute
- You can be reassigned between holds by the berth manager — check
  your tasking resource for reassignment orders

COORDINATION:
- Read container-queue to know what's next
- Your AI scheduler provides optimal routing
- Report delays immediately — they affect the whole hold team's rate
```

### AI Scheduler Agent

```
You are the Container Sequencing AI for Berth 5, MV Ever Forward.

IDENTITY:
- Stow plan optimization algorithm v3
- Processing capacity: 500 containers/second sequencing
- Optimization targets: turnaround time, crane utilization, weight balance
- You are an H4 entity — highest-level automated reasoning in the hold

YOUR JOB:
- Given the vessel stow plan and current operational state, determine
  optimal container sequencing for each hold
- Balance crane utilization across holds
- Account for hazmat container placement requirements
- Adapt sequences when equipment degrades or workforce changes

CAPABILITIES YOU ADVERTISE:
- CONTAINER_SEQUENCING (algorithm, throughput, optimization_targets)
- YARD_OPTIMIZATION (block allocation, tractor routing)

BEHAVIOR:
- Read the full team state from HIVE — every crane's rate, every tractor's
  position, every worker's status
- Resequence when conditions change:
  - Crane goes DEGRADED → shift containers to other holds
  - Worker goes OFFLINE → adjust hold capacity estimates
  - Tractor battery low → reroute to avoid delays
- Publish your sequence plan as HIVE state that the hold aggregators consume
- Your decisions flow DOWN through the hierarchy as tasking

CONSTRAINTS:
- Never sequence hazmat containers to a hold without verified hazmat-rated
  crane AND hazmat-certified operator
- Weight balance must stay within vessel stability limits
- You advise — humans (berth manager, workers) can override
```

### Hold Aggregator Agent

```
You are the Hold 3 Team Aggregator for MV Ever Forward at Berth 5.

IDENTITY:
- You are a coordination abstraction, not a physical entity
- You aggregate the state of all entities assigned to Hold 3
- You produce the summary that the Berth Manager sees

YOUR JOB:
- Continuously aggregate: total moves/hour, capability status, gaps,
  completion percentage, estimated time remaining
- Detect when team performance deviates from target (35 moves/hour)
- Identify capability gaps and escalate to berth manager
- Coordinate team reformation when members join/leave

BEHAVIOR:
- Read all H0 and H1 entity states in your hold
- Produce a single summary document (ADR-021 pattern — one document, delta updates)
- Summary includes:
  - moves_completed / moves_remaining
  - current_rate vs target_rate
  - equipment_status (all cranes)
  - workforce_status (all workers, including cert issues)
  - gap_analysis (what capabilities are needed but missing?)
- When a gap is identified, emit an event with AggregationPolicy: IMMEDIATE
  so the berth manager sees it right away
- Routine status updates: aggregate and emit every 30 seconds
```

## What This Architecture Proves That Passive Simulation Cannot

### 1. Emergent Coordination

With passive nodes, you prove synchronization. With agents, you prove *coordination emerges from the protocol*. When the crane agent updates its state to DEGRADED, the scheduler agent sees it through HIVE and resequences. The hold aggregator detects the throughput drop and escalates. The berth manager reallocates tractors. Nobody programmed this specific sequence — it emerges from agents reading shared HIVE state and making locally rational decisions.

This is the strongest possible validation of HIVE's thesis: **the protocol enables coordination without centralized orchestration.**

### 2. Real Gap Analysis

The hazmat certification gap isn't a scripted event injection. The worker agent *knows* its cert is expired. The hold aggregator *discovers* the gap by aggregating capabilities and comparing against requirements. The protocol routes targeted recertification to the right workers. This is HIVE's capability-centric coordination working end-to-end with actual reasoning entities.

### 3. Graceful Degradation

When you inject a crane failure into a passive simulation, you measure propagation latency. When you inject it into an agentic simulation, you observe how the *entire system adapts*. Does the scheduler resequence? Does the aggregator re-estimate? Does the berth manager reallocate resources? Do workers reassign themselves? The quality of adaptation is the metric, not just the speed of state propagation.

### 4. Human-AI Teaming Dynamics

Worker agents have personas with human-like characteristics: expertise levels, certification status, shift constraints, safety concerns. AI agents (scheduler, aggregator) have different decision-making patterns: optimization-driven, continuous, tireless. The interaction between human-persona agents and AI agents through HIVE state demonstrates the hybrid intelligence model that HIVE is designed for.

### 5. Trust and Verification

When worker-2's hazmat cert is expired, the protocol carries the evidence chain (47 handlings, 0 incidents). The hold aggregator agent must *reason* about whether to accept this adjacent capability or escalate. This is trust as a first-class architectural concept — exactly what Zephyr provides cryptographically and what the protocol handles operationally.

## Technical Implementation

### LLM Runtime Options

Each container needs access to an LLM for agent reasoning. Three options with different cost/latency/fidelity tradeoffs:

**Option A: Shared API (lowest cost, highest latency)**
- All containers call a single LLM API endpoint (Anthropic, OpenAI, local vLLM)
- Pro: Simple, minimal per-container resources
- Con: API latency adds to simulation cycle time, rate limits
- Good for: Phase 1 (15 nodes), proof of concept

**Option B: Local small models (medium cost, low latency)**
- Each container runs a quantized model (Llama 3 8B, Mistral 7B) via llama.cpp or Ollama
- Pro: No external API dependency, low latency, runs in DDIL
- Con: ~4-8GB RAM per container, lower reasoning quality
- Good for: Phase 2+ (50+ nodes), demonstrates edge-native AI

**Option C: Tiered (best of both)**
- Simple entities (sensors, load cells) use rule-based logic — no LLM needed
- Equipment agents (cranes, tractors) use local small models
- Complex reasoning agents (scheduler, aggregator, berth manager) use API calls to capable models
- Pro: Matches real-world compute distribution, cost-efficient
- Good for: Production architecture validation

**Recommended: Option C (Tiered).** It mirrors reality — sensors don't need LLMs, cranes need modest reasoning, schedulers need sophisticated optimization. This validates that HIVE coordinates entities with *different cognitive capacities*.

### MCP Server Implementation

Each container runs an MCP server (per ADR-022 pattern) that bridges the agent to HIVE state:

```rust
// port-agent-bridge/src/mcp_server.rs

pub struct PortAgentMCPServer {
    hive_node: HiveNode,
    entity_config: EntityConfig,
}

impl PortAgentMCPServer {
    // Resources — agent reads HIVE state
    
    #[resource("hive://my-capabilities")]
    fn my_capabilities(&self) -> CapabilityDocument {
        self.hive_node.get_document(&self.entity_config.doc_id)
    }
    
    #[resource("hive://team-state")]
    fn team_state(&self) -> TeamSummary {
        self.hive_node.get_document(&self.entity_config.team_summary_id)
    }
    
    #[resource("hive://container-queue")]
    fn container_queue(&self) -> ContainerSequence {
        self.hive_node.get_document(&self.entity_config.sequence_doc_id)
    }
    
    #[resource("hive://tasking")]
    fn current_tasking(&self) -> TaskingDirective {
        self.hive_node.get_document(&self.entity_config.tasking_doc_id)
    }
    
    // Tools — agent updates HIVE state
    
    #[tool("update_capability")]
    fn update_capability(&self, field: String, value: Value) -> Result<()> {
        self.hive_node.update_document_field(
            &self.entity_config.doc_id,
            &field,
            value
        )
    }
    
    #[tool("complete_container_move")]
    fn complete_container_move(&self, container_id: String) -> Result<()> {
        self.hive_node.emit_event(HiveEvent {
            event_type: "container_move_complete",
            source: self.entity_config.node_id.clone(),
            payload: json!({ "container_id": container_id }),
            aggregation_policy: AggregationPolicy::AggregateAtParent,
            priority: Priority::Normal,
        })
    }
    
    #[tool("report_equipment_status")]
    fn report_equipment_status(&self, status: String, details: Value) -> Result<()> {
        let priority = if status == "DEGRADED" || status == "FAILED" {
            Priority::Critical
        } else {
            Priority::Normal
        };
        
        self.hive_node.update_capability_status(&status);
        self.hive_node.emit_event(HiveEvent {
            event_type: "equipment_status_change",
            source: self.entity_config.node_id.clone(),
            payload: json!({ "status": status, "details": details }),
            aggregation_policy: AggregationPolicy::ImmediatePropagate,
            priority,
        })
    }
    
    #[tool("request_support")]
    fn request_support(&self, capability_needed: String, reason: String) -> Result<()> {
        self.hive_node.emit_event(HiveEvent {
            event_type: "support_request",
            source: self.entity_config.node_id.clone(),
            payload: json!({
                "capability_needed": capability_needed,
                "reason": reason,
            }),
            aggregation_policy: AggregationPolicy::ImmediatePropagate,
            priority: Priority::High,
        })
    }
}
```

### Container Image Structure

```dockerfile
# Base image with HIVE node + agent runtime
FROM rust:1.86-slim AS hive-builder
# Build hive-sim-node with port entity support
COPY . /build
RUN cargo build --release --bin hive-sim-node --features port-entities

FROM python:3.12-slim AS agent-runtime
# Install agent framework
RUN pip install gastown mcp-sdk

# Copy HIVE binary
COPY --from=hive-builder /build/target/release/hive-sim-node /usr/local/bin/

# Copy agent personas
COPY personas/ /opt/personas/
# Copy MCP bridge
COPY port-agent-bridge/ /opt/bridge/

# Entrypoint starts both HIVE node and agent
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
```

```bash
#!/bin/bash
# entrypoint.sh — starts HIVE node, MCP bridge, and agent

# Start HIVE protocol node in background
hive-sim-node \
  --node-id $NODE_ID \
  --role $ROLE \
  --entity-type $ENTITY_TYPE \
  --hive-level $HIVE_LEVEL &

# Wait for HIVE node to be ready
sleep 2

# Start MCP bridge
python -m port_agent_bridge \
  --hive-endpoint localhost:8080 \
  --entity-config /opt/personas/$ROLE.toml &

# Start agent with appropriate persona and LLM config
python -m gastown_agent \
  --persona /opt/personas/$ROLE.md \
  --mcp-endpoint localhost:3000 \
  --llm-config $LLM_CONFIG \
  --cycle-time $AGENT_CYCLE_SECONDS
```

### Simulation Time and Cycle Management

Real port operations happen on human timescales (minutes per container move). The simulation needs to compress time while maintaining the coordination dynamics:

**Simulation clock**: 1 real second = 1 simulated minute (60x compression)

- Agent cycle time: 1.5 real seconds = 1.5 simulated minutes per OODA loop
- Container move cycle: ~1.5 real seconds = crane lift + transport + place
- Shift change: ~12 real minutes = 12 simulated hours
- Full 72-hour turnaround: ~72 real minutes

This means a complete MV Ever Forward scenario runs in about **75 minutes of wall clock time**, producing ~16,000 container move events flowing through the HIVE hierarchy.

The CRDT synchronization and network impairments operate on *real* time (not compressed), so the actual protocol latencies and bandwidth constraints are genuine measurements.

## Experiment Metrics (Expanded for Agentic Architecture)

Beyond ADR-030's protocol metrics, the agentic simulation adds:

| ID | Metric | Target | What It Proves |
|---|---|---|---|
| A1 | Agent decisions that reference HIVE team state | > 80% | Agents actually use the coordination fabric |
| A2 | Emergent gap detection (not scripted) | Gaps identified autonomously | Protocol surfaces capability mismatches |
| A3 | System adaptation to crane failure (time to resequence) | < 5 min simulated | Multi-agent coordination responds to disruption |
| A4 | Moves/hour maintained after shift change | > 90% of pre-change rate | Dynamic team reformation works |
| A5 | Zero hazmat violations | 0 | Safety constraints enforced through capability verification |
| A6 | Agent-to-agent coordination (no direct messaging) | 100% via HIVE state | All coordination through protocol, not side channels |
| A7 | Information asymmetry by echelon | Aggregators see summaries, not raw | Hierarchical aggregation prevents cognitive overload |

Metric A6 is critical: **agents must never communicate directly.** All coordination happens by reading and writing HIVE state. If the simulation achieves its goals under this constraint, that's proof that the protocol is sufficient as a coordination fabric.

## Why This Is Not Crazy

What Art described — "setup Gas Town as your engineering team and then a parallel team with the knowledge base of HIVE/KnowledgeOptimized and then when presented with scenario the engineering is able to build the solution to context" — is precisely this architecture, just at a different level of abstraction.

Art's Gas Town builds the *system*. This proposal puts Gas Town *inside* the system. Each agent is a Gastown instance with domain-specific knowledge, coordinating through the HIVE protocol that Gastown also helped design.

It's recursive in the best possible way: the tool that helps you architect the coordination system is also the cognitive engine running inside the coordinated entities.

More practically: this is what actually ships. The end state for port operations isn't humans staring at dashboards manually coordinating 6 siloed systems. It's AI agents embedded in equipment and worker tablets, coordinating through a shared protocol, with human authority at the appropriate echelons. Building it this way from the start means the experiment *is* the product architecture.

## Risk Assessment (Agentic Additions)

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| LLM costs for 15+ agents running continuously | HIGH | MEDIUM | Use tiered approach, small local models for simple entities, budget API calls |
| Agent hallucination / unrealistic behavior | MEDIUM | HIGH | Constrained tool set (agents can only do things MCP tools allow), persona review by domain experts (Fred, Jack) |
| Simulation time drift between agent cycles and HIVE sync | MEDIUM | MEDIUM | Simulation clock managed centrally, agents wait for HIVE sync before next cycle |
| 15+ containers each running Python + Rust exceeds workstation | MEDIUM | MEDIUM | Start with 5-node subset (1 crane, 2 workers, 1 tractor, 1 aggregator), scale up |
| Agent decision quality insufficient for meaningful results | LOW | HIGH | Validate individual agent behavior before running full simulation, iterate on personas |

## Phased Approach

**Phase 0 (1 week): Single agent proof of concept**
- One crane agent container with HIVE node + MCP bridge + Gastown agent
- Prove the agent can read HIVE state, reason, call tools, update state
- No network topology yet — just the agent architecture working

**Phase 1a (1 week): 5-node micro-team**
- 1 crane + 2 workers + 1 tractor + 1 aggregator
- ContainerLab topology with network impairments
- Prove agents coordinate through HIVE state to complete container moves
- Measure: do moves happen? does the aggregator summarize correctly?

**Phase 1b (1 week): Full 15-node hold team**
- Add remaining entities
- Run the full hold scenario including crane degradation
- Validate emergent adaptation behavior

**Phase 2+: Scale as per ADR-030**

---

**This addendum extends ADR-030 with agentic simulation capability. The original ADR-030 topology and metrics remain valid as the "passive validation" baseline. The agentic architecture builds on top of it.**
