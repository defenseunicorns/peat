"""Agent orchestrator for port-ops simulation.

Manages entity lifecycle, LLM tiering, and event dispatch.
Sensor entities are automatically tagged with llm_tier='rule_based'
so they bypass the LLM path entirely.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from . import llm


# Entity types that use rule-based logic (no LLM calls)
RULE_BASED_TYPES: frozenset[str] = frozenset(
    {
        "load_cell",
        "rfid_reader",
        "temperature",
        "pressure",
        "humidity",
        "optical",
        "acoustic",
        "thermal",
    }
)


@dataclass
class Entity:
    """A simulated agent entity in the port-ops simulation."""

    entity_id: str
    entity_type: str
    llm_tier: str = "llm"
    state: dict = field(default_factory=dict)
    config: dict = field(default_factory=dict)
    events: list[llm.Event] = field(default_factory=list)


@dataclass
class Composition:
    """A named collection of entities forming a simulation scenario."""

    name: str
    entities: dict[str, dict] = field(default_factory=dict)


class Orchestrator:
    """Manages sim entities with LLM tiering.

    Sensors are automatically tagged rule_based. The orchestrator runs
    tick-based simulation steps, routing each entity's decision through
    the appropriate tier (rule-based or LLM).
    """

    def __init__(self, llm_client: llm.LLMClient | None = None):
        self.entities: dict[str, Entity] = {}
        self.tick: int = 0
        self.event_log: list[llm.Event] = []
        self.llm_client = llm_client
        self._llm_call_count: int = 0

    def add_entity(
        self,
        entity_id: str,
        entity_type: str,
        config: dict | None = None,
    ) -> Entity:
        """Add an entity, auto-tagging sensors as rule_based."""
        llm_tier = "rule_based" if entity_type in RULE_BASED_TYPES else "llm"
        entity = Entity(
            entity_id=entity_id,
            entity_type=entity_type,
            llm_tier=llm_tier,
            config=config or {},
        )
        self.entities[entity_id] = entity
        return entity

    def remove_entity(self, entity_id: str) -> None:
        """Remove an entity from the simulation."""
        self.entities.pop(entity_id, None)

    def load_composition(self, composition: Composition) -> list[Entity]:
        """Load a composition, adding all its entities.

        Compatible with any composition format: entities are dicts with
        at minimum 'entity_type', and optionally 'config'.
        """
        added = []
        for eid, spec in composition.entities.items():
            entity_type = spec.get("entity_type", "unknown")
            config = spec.get("config", {})
            entity = self.add_entity(eid, entity_type, config=config)
            added.append(entity)
        return added

    def step(self, sensor_readings: dict[str, float] | None = None) -> list[llm.Event]:
        """Advance simulation one tick.

        Args:
            sensor_readings: Optional mapping of entity_id -> reading value
                for sensor entities. If not provided, sensors use their
                nominal values.

        Returns:
            List of events produced during this tick.
        """
        readings = sensor_readings or {}
        tick_events: list[llm.Event] = []

        for entity_id, entity in self.entities.items():
            context: dict[str, Any] = {"tick": self.tick}
            if entity_id in readings:
                context["reading"] = readings[entity_id]

            result = llm.decide(entity, context, self.llm_client)

            if result.llm_called:
                self._llm_call_count += 1

            entity.events.extend(result.events)
            tick_events.extend(result.events)

        self.event_log.extend(tick_events)
        self.tick += 1
        return tick_events

    def get_llm_call_count(self) -> int:
        """Return total LLM API calls made during simulation."""
        return self._llm_call_count

    def get_entity_states(self) -> dict[str, dict]:
        """Return current state of all entities."""
        return {
            eid: {
                "entity_type": e.entity_type,
                "llm_tier": e.llm_tier,
                "state": dict(e.state),
            }
            for eid, e in self.entities.items()
        }

    def get_sensor_entities(self) -> list[Entity]:
        """Return all entities using rule-based (sensor) logic."""
        return [e for e in self.entities.values() if e.llm_tier == "rule_based"]

    def get_llm_entities(self) -> list[Entity]:
        """Return all entities using LLM-tier logic."""
        return [e for e in self.entities.values() if e.llm_tier == "llm"]
