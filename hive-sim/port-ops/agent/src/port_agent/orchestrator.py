"""
Multi-hold orchestrator — multi-agent simulation runner.

Runs multiple agents as asyncio tasks in one Python process with a shared
HiveStateStore. Agents use BridgeAPI (duck-types MCP ClientSession) instead
of MCP stdio transport.

Phase 1c composition: 2c5w4t1s1a1b2l1g2x = 18 agents (single hold).
Phase 2  composition: 3h(2c5w2l1g4t1a2x)1b1s = 3 holds × 17 + 2 shared = 53 agents.
"""

from __future__ import annotations

import asyncio
import json
import logging
import re
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .llm import create_provider, create_provider_from_tier, load_tier_config, LLMProvider, TierConfig
from .loop import AgentLoop, SimulationClock, CycleMetrics

logger = logging.getLogger(__name__)

# Avoid importing bridge at module level — it's on a separate src path.
# We import inside methods after PYTHONPATH is set by the caller.


@dataclass
class AgentSpec:
    """Specification for a single agent in the orchestrator."""
    node_id: str
    role: str           # "crane" | "aggregator" | "berth_manager" | "operator" | "tractor" | "scheduler" | "sensor" | "lashing_crew" | "signaler"
    persona: str        # persona filename without .md
    provider: str       # "anthropic" | "ollama" | "dry-run"
    model: str | None = None
    hold_num: int | None = None  # None for shared roles (berth_manager, scheduler)


@dataclass
class OrchestratorConfig:
    """Configuration for the multi-agent orchestrator."""
    hold_id: str = "hold-3"
    hold_num: int = 3
    berth: str = "berth-5"
    vessel: str = "MV Ever Forward"
    queue_size: int = 20
    hazmat_count: int = 3
    max_cycles: int = 15
    cycle_delay_sim_minutes: float = 1.5
    time_compression: float = 600.0
    agents: list[AgentSpec] = field(default_factory=list)
    num_holds: int = 1              # 1 = single hold (Phase 1c), 3 = multi-hold (Phase 2)
    hold_nums: list[int] = field(default_factory=lambda: [3])
    shared_tractor_count: int = 8  # Phase 2: berth-level shared tractor pool
    tier_config: TierConfig | None = None  # Tiered LLM config (overrides per-agent provider)


# Letter → (role, persona, node_id_pattern)
_COMPOSITION_MAP: dict[str, tuple[str, str, str]] = {
    "c": ("crane",      "gantry-crane",     "crane-{n}"),
    "w": ("operator",   "crane-operator",   "op-{n}"),
    "t": ("tractor",    "yard-tractor",     "tractor-{n}"),
    "s": ("scheduler",  "ai-scheduler",     "scheduler-1"),
    "a": ("aggregator", "hold-aggregator",  "hold-agg-3"),
    "b": ("berth_manager", "berth-manager", "berth-mgr-{n}"),
    "x": ("sensor",     "sensor",           None),  # special: interleave load-cell / rfid
    "l": ("lashing_crew", "lashing-crew",   "lasher-{n}"),
    "g": ("signaler",     "signaler",       "signaler-{n}"),
}

# Sensor node IDs cycle through these types
_SENSOR_IDS = ["load-cell-1", "rfid-1", "load-cell-2", "rfid-2"]


def _parse_flat_composition(
    composition: str,
    provider: str,
    model: str | None = None,
    hold_num: int | None = None,
    hold_prefix: str = "",
) -> list[AgentSpec]:
    """
    Parse a flat composition string (no hold multiplier) into AgentSpec list.

    Format: ``<count><letter>`` pairs, e.g. ``2c5w4t1s1a2x``.
    When hold_prefix is set (e.g. "h1-"), node IDs are prefixed for hold scoping.
    """
    specs: list[AgentSpec] = []
    pairs = re.findall(r"(\d+)([a-z])", composition)
    if not pairs:
        raise ValueError(
            f"Invalid composition: {composition!r}. "
            f"Expected format like '2c5w4t1s1a2x'."
        )

    sensor_idx = 0
    for count_str, letter in pairs:
        count = int(count_str)
        entry = _COMPOSITION_MAP.get(letter)
        if entry is None:
            raise ValueError(
                f"Unknown role letter '{letter}' in composition. "
                f"Supported: {', '.join(f'{k}={v[0]}' for k, v in _COMPOSITION_MAP.items())}"
            )

        role, persona, id_pattern = entry
        for i in range(1, count + 1):
            if letter == "x":
                base_id = _SENSOR_IDS[sensor_idx % len(_SENSOR_IDS)]
                node_id = f"{hold_prefix}{base_id}" if hold_prefix else base_id
                sensor_idx += 1
            elif id_pattern and "{n}" in id_pattern:
                base_id = id_pattern.replace("{n}", str(i))
                node_id = f"{hold_prefix}{base_id}" if hold_prefix else base_id
            else:
                # Singleton — scope to hold if within hold group
                if hold_prefix and "{n}" not in (id_pattern or ""):
                    node_id = f"{hold_prefix}{id_pattern}" if id_pattern else id_pattern
                else:
                    node_id = id_pattern

            specs.append(AgentSpec(
                node_id=node_id,
                role=role,
                persona=persona,
                provider=provider,
                model=model,
                hold_num=hold_num,
            ))

    return specs


def parse_agent_composition(
    composition: str,
    provider: str,
    model: str | None = None,
) -> tuple[list[AgentSpec], int, list[int]]:
    """
    Parse agent composition string into AgentSpec list.

    Supports two formats:
    - Flat:       ``2c5w4t1s1a2x`` (single hold, backward compat)
    - Multi-hold: ``3h(2c5w2l1g4t1a2x)1b1s`` (3 holds + shared roles)

    Returns:
        (specs, num_holds, hold_nums)
    """
    # Check for multi-hold format: Nh(...)...
    multi_match = re.match(r"(\d+)h\(([^)]+)\)(.*)", composition)
    if multi_match:
        num_holds = int(multi_match.group(1))
        hold_inner = multi_match.group(2)
        shared_suffix = multi_match.group(3)
        hold_nums = list(range(1, num_holds + 1))

        specs: list[AgentSpec] = []

        # Parse per-hold agents
        for hold_num in hold_nums:
            hold_prefix = f"h{hold_num}-"
            hold_specs = _parse_flat_composition(
                hold_inner, provider, model,
                hold_num=hold_num,
                hold_prefix=hold_prefix,
            )
            specs.extend(hold_specs)

        # Parse shared agents (no hold scope)
        if shared_suffix.strip():
            shared_specs = _parse_flat_composition(
                shared_suffix, provider, model,
                hold_num=None,
                hold_prefix="",
            )
            specs.extend(shared_specs)

        return specs, num_holds, hold_nums

    # Flat format (single hold, backward compat)
    specs = _parse_flat_composition(composition, provider, model)
    return specs, 1, [3]


class Orchestrator:
    """
    Multi-agent orchestrator for Phase 1a.

    Creates one shared HiveStateStore, initializes state, then runs
    multiple AgentLoop instances as concurrent asyncio tasks.
    """

    def __init__(self, config: OrchestratorConfig):
        self.config = config
        self.store = None       # Set in initialize_state
        self.agents: list[AgentLoop] = []
        self.clock = SimulationClock(compression_ratio=config.time_compression)

    def _hold_id_for(self, hold_num: int) -> str:
        return f"hold-{hold_num}"

    def initialize_state(self):
        """Create shared HIVE state store and populate initial state.

        Multi-hold: creates per-hold container queues, team summaries,
        and transport queues.  Shared roles (berth_manager, scheduler)
        span all holds.
        """
        from port_agent_bridge.hive_state import (
            HiveStateStore,
            create_crane_entity,
            create_operator_entity,
            create_tractor_entity,
            create_scheduler_entity,
            create_sensor_entity,
            create_lashing_crew_entity,
            create_signaler_entity,
            create_container_queue,
            create_transport_queue,
            create_team_state,
            create_sample_containers,
            create_shared_tractor_pool,
        )

        self.store = HiveStateStore()

        # Create per-hold shared resources
        for hold_num in self.config.hold_nums:
            hold_id = self._hold_id_for(hold_num)
            containers = create_sample_containers(
                count=self.config.queue_size,
                hazmat_count=self.config.hazmat_count,
            )
            create_container_queue(self.store, hold_id, containers)
            create_team_state(self.store, hold_id)

            self.store.create_container_assignments(hold_id)
            team_doc = self.store.get_document("team_summaries", f"team_{hold_id}")
            if team_doc:
                team_doc.update_field("moves_remaining", len(containers))
                team_doc.update_field("status", "ACTIVE")

            # Transport queue per hold
            has_tractors = any(
                s.role == "tractor" and s.hold_num == hold_num
                for s in self.config.agents
            )
            if has_tractors:
                create_transport_queue(self.store, hold_id)

        # Labor constraint defaults (ILA Local 1414)
        _LABOR_DEFAULTS = {
            "shift_start_minutes": 0.0,
            "shift_elapsed_hours": 0.0,
            "consecutive_hours": 0.0,
            "max_consecutive_hours": 6.0,
            "break_duration_min": 30.0,
            "shift_duration_hours": 12.0,
            "remaining_shift_hours": 12.0,
            "on_break": False,
            "break_eligible": False,
            "break_required": False,
            "shift_ended": False,
            "breaks_taken": 0,
        }

        # Initialize entity documents for each agent
        for spec in self.config.agents:
            # Determine which hold this agent belongs to
            hold_num = spec.hold_num if spec.hold_num is not None else self.config.hold_nums[0]
            hold_id = self._hold_id_for(hold_num)
            team_doc = self.store.get_document("team_summaries", f"team_{hold_id}")

            if spec.role == "crane":
                entity_config = {
                    "lift_capacity_tons": 65,
                    "reach_rows": 22,
                    "moves_per_hour": 30,
                    "hold": hold_num,
                    "berth": self.config.berth,
                    "vessel": self.config.vessel,
                    "hazmat_classes": [1, 3, 8, 9],
                    "hazmat_cert_valid": True,
                }
                create_crane_entity(self.store, spec.node_id, entity_config)
                # Add labor constraints to entity doc
                entity_doc = self.store.get_document("node_states", f"sim_doc_{spec.node_id}")
                if entity_doc:
                    entity_doc.update_field("labor", {
                        **_LABOR_DEFAULTS,
                        "hazmat_cert_required": True,
                        "minimum_crew": 2,
                    })
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "gantry_crane",
                        "status": "OPERATIONAL",
                        "capabilities": ["CONTAINER_LIFT", "HAZMAT_RATED"],
                    })

            elif spec.role == "operator":
                # First operator per hold gets hazmat cert
                is_first_op = not any(
                    s.node_id != spec.node_id
                    and s.role == "operator"
                    and s.hold_num == spec.hold_num
                    and self.config.agents.index(s) < self.config.agents.index(spec)
                    for s in self.config.agents
                )
                op_config = {
                    "proficiency": "expert",
                    "osha_cert_valid": True,
                    "hazmat_classes": [3, 8, 9],
                    "hazmat_cert_valid": is_first_op,
                    "hold": hold_num,
                    "berth": self.config.berth,
                }
                create_operator_entity(self.store, spec.node_id, op_config)
                # Add labor constraints to entity doc
                entity_doc = self.store.get_document("node_states", f"sim_doc_{spec.node_id}")
                if entity_doc:
                    entity_doc.update_field("labor", dict(_LABOR_DEFAULTS))
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "operator",
                        "status": "AVAILABLE",
                        "capabilities": ["CRANE_OPERATION", "HAZMAT_HANDLING"],
                        "hazmat_cert_valid": op_config["hazmat_cert_valid"],
                    })

            elif spec.role == "aggregator":
                self.store.create_document(
                    collection="node_states",
                    doc_id=f"sim_doc_{spec.node_id}",
                    fields={
                        "node_id": spec.node_id,
                        "entity_type": "hold_aggregator",
                        "hive_level": "H2",
                        "operational_status": "OPERATIONAL",
                        "assignment": {
                            "berth": self.config.berth,
                            "hold": hold_num,
                            "vessel": self.config.vessel,
                        },
                    },
                )
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "hold_aggregator",
                        "status": "OPERATIONAL",
                        "capabilities": ["AGGREGATION"],
                    })

            elif spec.role == "berth_manager":
                # Berth manager spans all holds — no hold-scoped team doc
                self.store.create_document(
                    collection="node_states",
                    doc_id=f"sim_doc_{spec.node_id}",
                    fields={
                        "node_id": spec.node_id,
                        "entity_type": "berth_manager",
                        "hive_level": "H3",
                        "operational_status": "OPERATIONAL",
                        "assignment": {
                            "berth": self.config.berth,
                            "vessel": self.config.vessel,
                        },
                        "metrics": {
                            "summaries_produced": 0,
                            "events_emitted": 0,
                            "rebalance_requests": 0,
                            "worker_reassignments": 0,
                            "scheduler_escalations": 0,
                            "hold_priority_changes": 0,
                        },
                    },
                )

            elif spec.role == "tractor":
                tractor_config = {
                    "capacity_tons": 40,
                    "max_speed_kph": 25,
                    "hold": hold_num,
                    "berth": self.config.berth,
                }
                create_tractor_entity(self.store, spec.node_id, tractor_config)
                # Add labor constraints to entity doc
                entity_doc = self.store.get_document("node_states", f"sim_doc_{spec.node_id}")
                if entity_doc:
                    entity_doc.update_field("labor", dict(_LABOR_DEFAULTS))
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "yard_tractor",
                        "status": "OPERATIONAL",
                        "capabilities": ["CONTAINER_TRANSPORT"],
                    })

            elif spec.role == "scheduler":
                # Scheduler spans all holds
                sched_config = {
                    "hold": hold_num,
                    "berth": self.config.berth,
                    "vessel": self.config.vessel,
                }
                create_scheduler_entity(self.store, spec.node_id, sched_config)

            elif spec.role == "sensor":
                sensor_type = "LOAD_CELL" if "load-cell" in spec.node_id else "RFID"
                sensor_config = {
                    "sensor_type": sensor_type,
                    "hold": hold_num,
                    "berth": self.config.berth,
                }
                create_sensor_entity(self.store, spec.node_id, sensor_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "sensor",
                        "status": "OPERATIONAL",
                        "capabilities": [sensor_type],
                    })

            elif spec.role == "lashing_crew":
                lashing_config = {
                    "hold": hold_num,
                    "berth": self.config.berth,
                }
                create_lashing_crew_entity(self.store, spec.node_id, lashing_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "lashing_crew",
                        "status": "OPERATIONAL",
                        "capabilities": ["CONTAINER_SECURING"],
                    })

            elif spec.role == "signaler":
                signaler_config = {
                    "hold": hold_num,
                    "berth": self.config.berth,
                }
                create_signaler_entity(self.store, spec.node_id, signaler_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "signaler",
                        "status": "OPERATIONAL",
                        "capabilities": ["VISUAL_SIGNALING"],
                    })

        # Phase 2: Create shared tractor pool at berth level
        if self.config.shared_tractor_count > 0:
            berth_id = self.config.berth
            create_shared_tractor_pool(self.store, berth_id)

            for i in range(1, self.config.shared_tractor_count + 1):
                tractor_id = f"shared-tractor-{i}"
                tractor_config = {
                    "capacity_tons": 40,
                    "max_speed_kph": 25,
                    "hold": None,  # Not assigned to any hold initially
                    "berth": berth_id,
                }
                create_tractor_entity(self.store, tractor_id, tractor_config)
                # Mark as shared in entity doc
                tractor_doc = self.store.get_document("node_states", f"sim_doc_{tractor_id}")
                if tractor_doc:
                    tractor_doc.update_field("assignment.shared", True)
                    tractor_doc.update_field("assignment.hold_assignment", None)

                # Register in berth-level pool
                self.store.register_shared_tractor(berth_id, tractor_id)

                # Add shared tractors as agents so they participate in the simulation
                self.config.agents.append(AgentSpec(
                    node_id=tractor_id,
                    role="tractor",
                    persona="yard-tractor",
                    provider=self.config.agents[0].provider if self.config.agents else "dry-run",
                    model=self.config.agents[0].model if self.config.agents else None,
                ))

            logger.info(
                f"Phase 2: {self.config.shared_tractor_count} shared tractors "
                f"instantiated at berth level ({berth_id})"
            )

        total_agents = len(self.config.agents)
        holds_str = ", ".join(self._hold_id_for(h) for h in self.config.hold_nums)
        logger.info(
            f"Orchestrator initialized: {total_agents} agents across "
            f"{self.config.num_holds} hold(s) [{holds_str}], "
            f"{self.config.queue_size} containers/hold"
        )
        if self.config.tier_config:
            tier_counts: dict[str, int] = {}
            for spec in self.config.agents:
                tier = self.config.tier_config.role_mapping.get(spec.role, "unknown")
                tier_counts[tier] = tier_counts.get(tier, 0) + 1
            tier_str = ", ".join(f"{t}: {c}" for t, c in sorted(tier_counts.items()))
            logger.info(f"LLM tiers: {tier_str}")

    def create_agents(self, personas_dir: Path):
        """Create AgentLoop instances for each agent spec."""
        from port_agent_bridge.bridge_api import BridgeAPI

        self.agents = []
        for spec in self.config.agents:
            persona_path = personas_dir / f"{spec.persona}.md"
            if not persona_path.exists():
                raise FileNotFoundError(f"Persona file not found: {persona_path}")

            # Determine hold_id for this agent
            hold_num = spec.hold_num if spec.hold_num is not None else self.config.hold_nums[0]
            hold_id = self._hold_id_for(hold_num)

            # Per-agent BridgeAPI (duck-types MCP ClientSession)
            bridge = BridgeAPI(
                store=self.store,
                node_id=spec.node_id,
                role=spec.role,
                hold_id=hold_id,
                hold_num=hold_num,
            )

            # Per-agent LLM provider — tier config overrides per-agent provider
            if self.config.tier_config:
                llm = create_provider_from_tier(self.config.tier_config, spec.role)
            else:
                provider_kwargs = {}
                if spec.model:
                    provider_kwargs["model"] = spec.model
                provider_kwargs["role"] = spec.role
                llm = create_provider(spec.provider, **provider_kwargs)

            agent = AgentLoop(
                node_id=spec.node_id,
                persona_path=str(persona_path),
                llm=llm,
                mcp_client=bridge,
                clock=self.clock,
                max_cycles=self.config.max_cycles,
                cycle_delay_sim_minutes=self.config.cycle_delay_sim_minutes,
                role=spec.role,
            )
            self.agents.append(agent)

        logger.info(f"Created {len(self.agents)} agent loops")

    async def run(self) -> dict[str, list[CycleMetrics]]:
        """Run all agents concurrently and collect metrics."""
        phase = "Phase 2 (Multi-Hold)" if self.config.num_holds > 1 else "Phase 1c"
        holds_str = ", ".join(self._hold_id_for(h) for h in self.config.hold_nums)
        logger.info(
            f"\n{'='*60}\n"
            f"  HIVE Port Operations — {phase}\n"
            f"  Holds: {self.config.num_holds} [{holds_str}]\n"
            f"  Agents: {len(self.config.agents)} total\n"
            f"  Queue: {self.config.queue_size} containers/hold "
            f"({self.config.hazmat_count} hazmat)\n"
            f"  Max cycles: {self.config.max_cycles}\n"
            f"  Time compression: {self.config.time_compression}x\n"
            f"{'='*60}"
        )

        # Emit vessel approach spatial event so viewer animates ship arrival
        vessel_approach = {
            "event_type": "spatial_update",
            "source": "orchestrator",
            "priority": "ROUTINE",
            "details": {
                "operation": "vessel_approach",
                "vessel_name": "MV Ever Forward",
                "berth_id": "berth-1",
            },
        }
        print(json.dumps(vessel_approach), flush=True)

        # Run all agent loops concurrently.
        # The yield points in AgentLoop.run_cycle() (asyncio.sleep(0) between
        # observe and decide) allow concurrent agents to overlap phases,
        # creating natural contention when multiple cranes target the same container.
        results = await asyncio.gather(
            *(agent.run() for agent in self.agents),
            return_exceptions=True,
        )

        # Collect metrics by agent
        all_metrics: dict[str, list[CycleMetrics]] = {}
        for agent, result in zip(self.agents, results):
            if isinstance(result, Exception):
                logger.error(f"Agent {agent.node_id} failed: {result}")
                all_metrics[agent.node_id] = []
            else:
                all_metrics[agent.node_id] = result

        self._print_summary(all_metrics)
        return all_metrics

    def _print_summary(self, all_metrics: dict[str, list[CycleMetrics]]):
        """Print final multi-agent summary."""
        phase = "PHASE 2 MULTI-HOLD" if self.config.num_holds > 1 else "MULTI-AGENT"
        print("\n" + "=" * 60, flush=True)
        print(f"  {phase} SUMMARY", flush=True)
        print("=" * 60, flush=True)

        # LLM tier breakdown
        if self.config.tier_config:
            tier_counts: dict[str, list[str]] = {}
            for spec in self.config.agents:
                tier = self.config.tier_config.role_mapping.get(spec.role, "unknown")
                tier_counts.setdefault(tier, []).append(spec.node_id)
            print("\n  LLM TIERS:", flush=True)
            for tier, nodes in sorted(tier_counts.items()):
                desc = self.config.tier_config.tiers.get(tier, {}).get("description", "")
                print(f"    {tier} ({len(nodes)}): {desc}", flush=True)
                # API cost tracking: only api and hybrid tiers incur API costs
                if tier in ("api", "hybrid"):
                    api_agents = [n for n in nodes if n in all_metrics]
                    api_cycles = sum(len(all_metrics[n]) for n in api_agents)
                    print(f"      API-eligible cycles: {api_cycles}", flush=True)

        # Per-agent summary
        for node_id, metrics in all_metrics.items():
            actions = sum(1 for m in metrics if m.action != "wait")
            waits = sum(1 for m in metrics if m.action == "wait")
            successes = sum(1 for m in metrics if m.success)
            failures = sum(1 for m in metrics if not m.success)
            print(
                f"\n  {node_id}: {len(metrics)} cycles, "
                f"{actions} actions, {waits} waits, "
                f"{successes} ok, {failures} fail",
                flush=True,
            )
            action_counts: dict[str, int] = {}
            for m in metrics:
                action_counts[m.action] = action_counts.get(m.action, 0) + 1
            for action, count in sorted(action_counts.items()):
                print(f"    {action}: {count}", flush=True)

        # Per-hold team state from HIVE
        if self.store:
            for hold_num in self.config.hold_nums:
                hold_id = self._hold_id_for(hold_num)
                team_doc = self.store.get_document(
                    "team_summaries", f"team_{hold_id}"
                )
                if team_doc:
                    print(f"\n  HOLD STATE ({hold_id}):", flush=True)
                    print(f"    moves_completed: {team_doc.get_field('moves_completed', 0)}", flush=True)
                    print(f"    moves_remaining: {team_doc.get_field('moves_remaining', 0)}", flush=True)
                    print(f"    moves_per_hour:  {team_doc.get_field('moves_per_hour', 0)}", flush=True)
                    print(f"    status:          {team_doc.get_field('status', '?')}", flush=True)
                    print(f"    gaps:            {len(team_doc.get_field('gap_analysis', []))}", flush=True)

                # Per-crane entity metrics for this hold
                for spec in self.config.agents:
                    if spec.role == "crane" and (spec.hold_num == hold_num or (spec.hold_num is None and hold_num == self.config.hold_nums[0])):
                        entity = self.store.get_document("node_states", f"sim_doc_{spec.node_id}")
                        if entity:
                            print(
                                f"    {spec.node_id}: "
                                f"moves={entity.get_field('metrics.moves_completed', 0)}, "
                                f"tons={entity.get_field('metrics.total_tons_lifted', 0.0):.1f}t",
                                flush=True,
                            )

            # Event log summary
            events = self.store.get_events()
            event_types: dict[str, int] = {}
            event_sources: dict[str, int] = {}
            for e in events:
                et = e.get("event_type", "unknown")
                es = e.get("source", "unknown")
                event_types[et] = event_types.get(et, 0) + 1
                event_sources[es] = event_sources.get(es, 0) + 1

            print(f"\n  EVENTS ({len(events)} total):", flush=True)
            for et, count in sorted(event_types.items()):
                print(f"    {et}: {count}", flush=True)
            print(f"\n  EVENTS BY SOURCE:", flush=True)
            for es, count in sorted(event_sources.items()):
                print(f"    {es}: {count}", flush=True)

            contention_retries = sum(
                1 for m_list in all_metrics.values()
                for m in m_list
                if m.contention_retry
            )
            print(f"\n  CONTENTION: {contention_retries} retries (claims beaten by another crane)", flush=True)

            # Shared tractor pool summary
            pool = self.store.get_document(
                "shared_tractor_pools", f"pool_{self.config.berth}"
            )
            if pool:
                tractors = pool.fields.get("tractors", {})
                assigned = sum(1 for t in tractors.values() if t.get("hold_assignment"))
                unassigned = len(tractors) - assigned
                print(f"\n  SHARED TRACTOR POOL ({len(tractors)} total):", flush=True)
                print(f"    assigned:   {assigned}", flush=True)
                print(f"    unassigned: {unassigned}", flush=True)
                for tid, tdata in tractors.items():
                    hold = tdata.get("hold_assignment", "unassigned")
                    print(f"    {tid}: {hold or 'unassigned'}", flush=True)

            # tractor_reassigned events
            reassign_events = [
                e for e in events if e.get("event_type") == "tractor_reassigned"
            ]
            if reassign_events:
                print(f"\n  TRACTOR REASSIGNMENTS ({len(reassign_events)}):", flush=True)
                for re_evt in reassign_events:
                    print(
                        f"    {re_evt.get('tractor_id')}: "
                        f"{re_evt.get('from_hold', 'none')} → {re_evt.get('to_hold')} "
                        f"({re_evt.get('reason', '')})",
                        flush=True,
                    )

        print("\n" + "=" * 60, flush=True)
