"""
Role lifecycle configuration for the port terminal simulation.

Defines _ROLE_CONFIGS for all hierarchy levels in the terminal (ADR-051).
Each role specifies its hierarchy level, lifecycle type, zone scope,
and relationships to other roles.

Lifecycle types:
  - "managed"  — Has startup/shutdown sequences, health checks
  - None       — Virtual role, always-on coordination entity
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class HierarchyLevel(Enum):
    """Terminal hierarchy levels per ADR-051."""
    H0 = 0  # Automated equipment (scanners, RFID readers)
    H1 = 1  # Equipment / team leads (cranes, tractors, gate workers)
    H2 = 2  # Yard block (group of container stacks)
    H3 = 3  # Yard/Gate manager (zone coordinator)
    H4 = 4  # Terminal Operations Center (TOC)


@dataclass(frozen=True)
class SubsystemSpec:
    """Specification for an equipment subsystem."""
    name: str
    kind: str  # actuator type: "winch", "rotary", "linear"
    description: str = ""


@dataclass(frozen=True)
class RoleConfig:
    """Immutable configuration for a simulation role."""
    level: HierarchyLevel
    zone: str
    lifecycle: Optional[str]  # None = virtual (always-on)
    subordinates: tuple[str, ...] = ()
    superior: Optional[str] = None
    description: str = ""
    min_subordinates: int = 0
    subsystems: tuple[SubsystemSpec, ...] = ()


# ---------------------------------------------------------------------------
# Role registry
# ---------------------------------------------------------------------------

_ROLE_CONFIGS: dict[str, RoleConfig] = {
    "yard_manager": RoleConfig(
        level=HierarchyLevel.H3,
        zone="yard",
        lifecycle=None,  # virtual — no startup/shutdown
        subordinates=("yard_block",),
        superior="toc",
        description="Zone coordinator for yard blocks, stacking cranes, and tractor routing",
        min_subordinates=4,
    ),
    "stacking_crane": RoleConfig(
        level=HierarchyLevel.H1,
        zone="yard",
        lifecycle="managed",
        subordinates=(),
        superior="yard_manager",
        description="RTG crane — stacks/retrieves containers in yard blocks by row/bay/tier",
        subsystems=(
            SubsystemSpec(
                name="hoist",
                kind="winch",
                description="Vertical lift/lower via spreader and cables",
            ),
            SubsystemSpec(
                name="trolley",
                kind="rotary",
                description="Horizontal traverse across yard block width",
            ),
            SubsystemSpec(
                name="gantry_travel",
                kind="linear",
                description="Longitudinal travel along yard block length",
            ),
        ),
    ),
    "gate_scanner": RoleConfig(
        level=HierarchyLevel.H0,
        zone="gate",
        lifecycle="managed",  # physical equipment with startup/shutdown
        subordinates=(),
        superior="gate_worker",
        description="Automated container damage detection and weight verification at gate lane",
    ),
    "rfid_reader": RoleConfig(
        level=HierarchyLevel.H0,
        zone="gate",
        lifecycle="managed",
        subordinates=(),
        superior="gate_worker",
        description="Automated container identification via ISO 18000-6C / EPC GEN2 RFID",
    ),
    "gate_worker": RoleConfig(
        level=HierarchyLevel.H1,
        zone="gate",
        lifecycle="managed",
        subordinates=("gate_scanner", "rfid_reader"),
        superior="gate_manager",
        description="Truck processing, document verification, and seal inspection at gate lane",
        min_subordinates=2,
    ),
}


def get_role_config(role_name: str) -> RoleConfig:
    """Look up a role configuration by name.

    Raises KeyError if the role is not registered.
    """
    return _ROLE_CONFIGS[role_name]


def registered_roles() -> list[str]:
    """Return all registered role names."""
    return list(_ROLE_CONFIGS.keys())


def roles_at_level(level: HierarchyLevel) -> list[str]:
    """Return role names at the given hierarchy level."""
    return [name for name, cfg in _ROLE_CONFIGS.items() if cfg.level == level]
