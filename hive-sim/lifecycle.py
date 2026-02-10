"""
Port-ops simulation role lifecycle management.

Defines role configurations and physical action sets for each worker type
in the container port simulation. Roles are organized by hierarchy level
(H1 = ground crew, H2 = equipment operators, H3 = supervisors).
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import FrozenSet


class PhysicalAction(Enum):
    """Physical actions available to port-ops roles."""

    # Signaler actions
    SIGNAL_HOIST = auto()
    SIGNAL_LOWER = auto()
    SIGNAL_STOP = auto()
    SIGNAL_CLEAR = auto()

    # Crane operator actions (future)
    CRANE_HOIST = auto()
    CRANE_LOWER = auto()
    CRANE_TROLLEY = auto()
    CRANE_SLEW = auto()

    # Stevedore actions (future)
    LASH_CONTAINER = auto()
    UNLASH_CONTAINER = auto()
    INSPECT_TWIST_LOCK = auto()


@dataclass(frozen=True)
class RoleConfig:
    """Configuration for a port-ops simulation role."""

    name: str
    hierarchy_level: int
    physical_actions: FrozenSet[PhysicalAction]
    requires_line_of_sight: bool = False
    heavy_subsystems: bool = False
    visibility_range_m: float = 100.0
    description: str = ""


_ROLE_CONFIGS: dict[str, RoleConfig] = {
    "signaler": RoleConfig(
        name="signaler",
        hierarchy_level=1,
        physical_actions=frozenset({
            PhysicalAction.SIGNAL_HOIST,
            PhysicalAction.SIGNAL_LOWER,
            PhysicalAction.SIGNAL_STOP,
            PhysicalAction.SIGNAL_CLEAR,
        }),
        requires_line_of_sight=True,
        heavy_subsystems=False,
        visibility_range_m=150.0,
        description="Visual hand-signal communication between crane operator and ground crew",
    ),
}


def get_role_config(role_name: str) -> RoleConfig:
    """Get the configuration for a named role.

    Raises:
        KeyError: If the role name is not recognized.
    """
    return _ROLE_CONFIGS[role_name]


def list_roles() -> list[str]:
    """Return all registered role names."""
    return list(_ROLE_CONFIGS.keys())
