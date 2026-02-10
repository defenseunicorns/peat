"""
Entity orchestrator for the port terminal simulation.

Creates and manages simulation entities from role configs.  Each entity
is a lightweight wrapper that holds state, a persona reference, and a
zone scope.  The orchestrator owns the entity registry and drives the
per-tick decision cycle.
"""

from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from typing import Any, Optional

from .lifecycle import HierarchyLevel, RoleConfig, get_role_config


# ---------------------------------------------------------------------------
# Composition characters — maps roles to their persona files
# ---------------------------------------------------------------------------

_COMPOSITION: dict[str, str] = {
    "yard_manager": "personas/yard-manager.md",
    "stacking_crane": "personas/stacking-crane.md",
    "gate_scanner": "personas/gate-scanner.md",
    "rfid_reader": "personas/gate-scanner.md",  # shares scanner persona (equipment)
    "gate_worker": "personas/gate-worker.md",
}


@dataclass
class Entity:
    """A simulation entity instantiated from a role config."""
    entity_id: str
    role: str
    config: RoleConfig
    persona_path: str
    zone_scope: str
    state: dict[str, Any] = field(default_factory=dict)
    subordinate_ids: list[str] = field(default_factory=list)
    superior_id: Optional[str] = None

    @property
    def level(self) -> HierarchyLevel:
        return self.config.level


class Orchestrator:
    """Manages entity lifecycle and the per-tick decision loop."""

    def __init__(self) -> None:
        self._entities: dict[str, Entity] = {}

    # -- Entity creation -----------------------------------------------------

    def create_entity(
        self,
        role: str,
        zone_scope: str,
        *,
        entity_id: Optional[str] = None,
    ) -> Entity:
        """Instantiate a new entity for the given role.

        Args:
            role: Registered role name (must exist in _ROLE_CONFIGS).
            zone_scope: Spatial scope this entity manages (e.g. "yard-north").
            entity_id: Optional explicit ID; a UUID is generated when omitted.

        Returns:
            The newly created Entity.

        Raises:
            KeyError: If the role is not registered.
            KeyError: If the role has no composition character.
        """
        config = get_role_config(role)
        persona_path = _COMPOSITION[role]
        eid = entity_id or f"{role}-{uuid.uuid4().hex[:8]}"

        entity = Entity(
            entity_id=eid,
            role=role,
            config=config,
            persona_path=persona_path,
            zone_scope=zone_scope,
        )

        # Pre-populate state for virtual roles (no lifecycle boot required)
        if config.lifecycle is None:
            entity.state["status"] = "active"

        # Managed equipment gets a startup sequence
        if config.lifecycle == "managed":
            entity.state["status"] = "starting"
            # Initialize subsystem states
            for sub in config.subsystems:
                entity.state[f"subsystem_{sub.name}"] = {
                    "kind": sub.kind,
                    "status": "nominal",
                }
            entity.state["status"] = "active"

        self._entities[eid] = entity
        return entity

    def create_stacking_crane(
        self,
        yard_block_id: str,
        zone_scope: str,
        *,
        entity_id: Optional[str] = None,
    ) -> Entity:
        """Create a stacking crane entity assigned to a yard block.

        The crane is linked as equipment under the given yard block and
        receives the block's slot map in its initial state.
        """
        crane = self.create_entity("stacking_crane", zone_scope, entity_id=entity_id)
        crane.state["yard_block"] = yard_block_id
        crane.state["current_task"] = None
        crane.state["position"] = {"row": 0, "bay": 0}
        crane.state["hoist_load_kg"] = 0.0
        self.link(yard_block_id, crane.entity_id)
        return crane


    # -- Hierarchy wiring ----------------------------------------------------

    def link(self, superior_id: str, subordinate_id: str) -> None:
        """Establish a superior-subordinate relationship."""
        sup = self._entities[superior_id]
        sub = self._entities[subordinate_id]
        sup.subordinate_ids.append(subordinate_id)
        sub.superior_id = superior_id

    # -- Queries -------------------------------------------------------------

    def get_entity(self, entity_id: str) -> Entity:
        return self._entities[entity_id]

    def entities_by_role(self, role: str) -> list[Entity]:
        return [e for e in self._entities.values() if e.role == role]

    def entities_at_level(self, level: HierarchyLevel) -> list[Entity]:
        return [e for e in self._entities.values() if e.level == level]

    def subordinates_of(self, entity_id: str) -> list[Entity]:
        parent = self._entities[entity_id]
        return [self._entities[sid] for sid in parent.subordinate_ids]

    # -- Tick ----------------------------------------------------------------

    def tick(self, decide_fn) -> dict[str, Any]:
        """Run one decision cycle for every active entity.

        Entities are processed bottom-up (H1 -> H4) so that higher-level
        coordinators see fresh subordinate state.

        Args:
            decide_fn: Callable(entity, orchestrator) -> dict of actions.

        Returns:
            Mapping of entity_id -> action results.
        """
        results: dict[str, Any] = {}

        ordered = sorted(
            self._entities.values(),
            key=lambda e: e.config.level.value,
        )

        for entity in ordered:
            if entity.state.get("status") != "active":
                continue
            results[entity.entity_id] = decide_fn(entity, self)

        return results
