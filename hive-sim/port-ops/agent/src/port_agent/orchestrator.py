"""
Phase 1b Orchestrator — multi-agent simulation runner.

Runs multiple agents as asyncio tasks in one Python process with a shared
HiveStateStore. Agents use BridgeAPI (duck-types MCP ClientSession) instead
of MCP stdio transport.

Target composition: 2c5w4t1s1a2x = 15 agents (cranes, operators, tractors,
scheduler, aggregator, sensors).
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

from .llm import create_provider, LLMProvider
from .loop import AgentLoop, SimulationClock, CycleMetrics

logger = logging.getLogger(__name__)

# Avoid importing bridge at module level — it's on a separate src path.
# We import inside methods after PYTHONPATH is set by the caller.


@dataclass
class AgentSpec:
    """Specification for a single agent in the orchestrator."""
    node_id: str
    role: str           # "crane" | "aggregator" | "berth_manager" | "operator" | "tractor" | "scheduler" | "sensor"
    persona: str        # persona filename without .md
    provider: str       # "anthropic" | "ollama" | "dry-run"
    model: str | None = None


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


# Letter → (role, persona, node_id_pattern)
_COMPOSITION_MAP: dict[str, tuple[str, str, str]] = {
    "c": ("crane",      "gantry-crane",     "crane-{n}"),
    "w": ("operator",   "crane-operator",   "op-{n}"),
    "t": ("tractor",    "yard-tractor",     "tractor-{n}"),
    "s": ("scheduler",  "ai-scheduler",     "scheduler-1"),
    "a": ("aggregator", "hold-aggregator",  "hold-agg-3"),
    "b": ("berth_manager", "berth-manager", "berth-mgr-{n}"),
    "x": ("sensor",     "sensor",           None),  # special: interleave load-cell / rfid
}

# Sensor node IDs cycle through these types
_SENSOR_IDS = ["load-cell-1", "rfid-1", "load-cell-2", "rfid-2"]


def parse_agent_composition(
    composition: str,
    provider: str,
    model: str | None = None,
) -> list[AgentSpec]:
    """
    Parse agent composition string into AgentSpec list.

    Format: ``<count><letter>`` pairs, e.g. ``2c5w4t1s1a2x``.
    Letters: c=crane, w=operator, t=tractor, s=scheduler, a=aggregator, x=sensor.
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
                node_id = _SENSOR_IDS[sensor_idx % len(_SENSOR_IDS)]
                sensor_idx += 1
            elif id_pattern and "{n}" in id_pattern:
                node_id = id_pattern.replace("{n}", str(i))
            else:
                node_id = id_pattern  # singleton like hold-agg-3, scheduler-1

            specs.append(AgentSpec(
                node_id=node_id,
                role=role,
                persona=persona,
                provider=provider,
                model=model,
            ))

    return specs


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

    def initialize_state(self):
        """Create shared HIVE state store and populate initial state."""
        from port_agent_bridge.hive_state import (
            HiveStateStore,
            create_crane_entity,
            create_operator_entity,
            create_tractor_entity,
            create_scheduler_entity,
            create_sensor_entity,
            create_container_queue,
            create_transport_queue,
            create_team_state,
            create_sample_containers,
        )

        self.store = HiveStateStore()
        hold_id = self.config.hold_id

        # Create container queue (shared by all cranes)
        containers = create_sample_containers(
            count=self.config.queue_size,
            hazmat_count=self.config.hazmat_count,
        )
        create_container_queue(self.store, hold_id, containers)

        # Create team state
        create_team_state(self.store, hold_id)
        team_doc = self.store.get_document("team_summaries", f"team_{hold_id}")

        # Initialize entity documents for each agent
        for spec in self.config.agents:
            if spec.role == "crane":
                entity_config = {
                    "lift_capacity_tons": 65,
                    "reach_rows": 22,
                    "moves_per_hour": 30,
                    "hold": self.config.hold_num,
                    "berth": self.config.berth,
                    "vessel": self.config.vessel,
                    "hazmat_classes": [1, 3, 8, 9],
                    "hazmat_cert_valid": True,
                }
                create_crane_entity(self.store, spec.node_id, entity_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "gantry_crane",
                        "status": "OPERATIONAL",
                        "capabilities": ["CONTAINER_LIFT", "HAZMAT_RATED"],
                    })
            elif spec.role == "operator":
                op_config = {
                    "proficiency": "expert",
                    "osha_cert_valid": True,
                    "hazmat_classes": [3, 8, 9],
                    "hazmat_cert_valid": spec.node_id == "op-1",  # op-1 certified, op-2 not
                    "hold": self.config.hold_num,
                    "berth": self.config.berth,
                }
                create_operator_entity(self.store, spec.node_id, op_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "operator",
                        "status": "AVAILABLE",
                        "capabilities": ["CRANE_OPERATION", "HAZMAT_HANDLING"],
                        "hazmat_cert_valid": op_config["hazmat_cert_valid"],
                    })
            elif spec.role == "aggregator":
                # Aggregator gets a lightweight entity doc
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
                            "hold": self.config.hold_num,
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
                # Berth manager gets a lightweight H3 entity doc
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
                        },
                    },
                )
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "berth_manager",
                        "status": "OPERATIONAL",
                        "capabilities": ["BERTH_AGGREGATION", "TRACTOR_REBALANCE"],
                    })

            elif spec.role == "tractor":
                tractor_config = {
                    "capacity_tons": 40,
                    "max_speed_kph": 25,
                    "hold": self.config.hold_num,
                    "berth": self.config.berth,
                }
                create_tractor_entity(self.store, spec.node_id, tractor_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "yard_tractor",
                        "status": "OPERATIONAL",
                        "capabilities": ["CONTAINER_TRANSPORT"],
                    })

            elif spec.role == "scheduler":
                sched_config = {
                    "hold": self.config.hold_num,
                    "berth": self.config.berth,
                    "vessel": self.config.vessel,
                }
                create_scheduler_entity(self.store, spec.node_id, sched_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "scheduler",
                        "status": "OPERATIONAL",
                        "capabilities": ["SCHEDULING", "RESOURCE_DISPATCH"],
                    })

            elif spec.role == "sensor":
                sensor_type = "LOAD_CELL" if "load-cell" in spec.node_id else "RFID"
                sensor_config = {
                    "sensor_type": sensor_type,
                    "hold": self.config.hold_num,
                    "berth": self.config.berth,
                }
                create_sensor_entity(self.store, spec.node_id, sensor_config)
                if team_doc:
                    team_doc.update_field(f"team_members.{spec.node_id}", {
                        "entity_type": "sensor",
                        "status": "OPERATIONAL",
                        "capabilities": [sensor_type],
                    })

        # Create transport queue (for tractors to claim discharged containers)
        has_tractors = any(s.role == "tractor" for s in self.config.agents)
        if has_tractors:
            create_transport_queue(self.store, hold_id)

        if team_doc:
            team_doc.update_field("moves_remaining", len(containers))
            team_doc.update_field("status", "ACTIVE")

        logger.info(
            f"Orchestrator initialized: {len(self.config.agents)} agents, "
            f"{self.config.queue_size} containers, hold={hold_id}"
        )

    def create_agents(self, personas_dir: Path):
        """Create AgentLoop instances for each agent spec."""
        from port_agent_bridge.bridge_api import BridgeAPI

        self.agents = []
        for spec in self.config.agents:
            persona_path = personas_dir / f"{spec.persona}.md"
            if not persona_path.exists():
                raise FileNotFoundError(f"Persona file not found: {persona_path}")

            # Per-agent BridgeAPI (duck-types MCP ClientSession)
            bridge = BridgeAPI(
                store=self.store,
                node_id=spec.node_id,
                role=spec.role,
                hold_id=self.config.hold_id,
            )

            # Per-agent LLM provider
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
        logger.info(
            f"\n{'='*60}\n"
            f"  HIVE Port Operations — Phase 1b (15-Node Hold Team)\n"
            f"  Agents: {', '.join(s.node_id for s in self.config.agents)}\n"
            f"  Queue: {self.config.queue_size} containers "
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
        print("\n" + "=" * 60, flush=True)
        print("  PHASE 1b MULTI-AGENT SUMMARY", flush=True)
        print("=" * 60, flush=True)

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
            # Action breakdown
            action_counts: dict[str, int] = {}
            for m in metrics:
                action_counts[m.action] = action_counts.get(m.action, 0) + 1
            for action, count in sorted(action_counts.items()):
                print(f"    {action}: {count}", flush=True)

        # Team state from HIVE
        if self.store:
            team_doc = self.store.get_document(
                "team_summaries", f"team_{self.config.hold_id}"
            )
            if team_doc:
                print(f"\n  TEAM STATE ({self.config.hold_id}):", flush=True)
                print(f"    moves_completed: {team_doc.get_field('moves_completed', 0)}", flush=True)
                print(f"    moves_remaining: {team_doc.get_field('moves_remaining', 0)}", flush=True)
                print(f"    moves_per_hour:  {team_doc.get_field('moves_per_hour', 0)}", flush=True)
                print(f"    status:          {team_doc.get_field('status', '?')}", flush=True)
                print(f"    gaps:            {len(team_doc.get_field('gap_analysis', []))}", flush=True)

            # Per-crane entity metrics
            for spec in self.config.agents:
                if spec.role == "crane":
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

            # Contention check: count contention retries (claims beaten by another crane)
            contention_retries = sum(
                1 for m_list in all_metrics.values()
                for m in m_list
                if m.contention_retry
            )
            print(f"\n  CONTENTION: {contention_retries} retries (claims beaten by another crane)", flush=True)

        print("\n" + "=" * 60, flush=True)
