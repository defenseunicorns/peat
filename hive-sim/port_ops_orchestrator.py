"""
Port-ops simulation entity orchestrator.

Maps composition keys to role tuples and manages entity initialization
for the container port simulation. Each key in _COMPOSITION_MAP corresponds
to a spawn shorthand used in topology definitions.

Entity tuple format: (role_name, persona_file, name_template)
  - role_name: key into lifecycle._ROLE_CONFIGS
  - persona_file: persona markdown filename (without .md)
  - name_template: instance naming pattern ({n} replaced with index)
"""

from dataclasses import dataclass, field
from typing import Optional

from lifecycle import get_role_config, RoleConfig


# Composition map: shorthand key -> (role, persona, name_template)
_COMPOSITION_MAP: dict[str, tuple[str, str, str]] = {
    "g": ("signaler", "signaler", "signaler-{n}"),
}


@dataclass
class Entity:
    """A simulation entity (agent instance)."""

    entity_id: str
    role: str
    role_config: RoleConfig
    hierarchy_level: int
    visibility_range_m: float
    state: str = "idle"
    metadata: dict = field(default_factory=dict)


def spawn_entity(
    composition_key: str,
    index: int = 0,
    *,
    visibility_range_override: Optional[float] = None,
) -> Entity:
    """Spawn a new entity from a composition key.

    Args:
        composition_key: Single-char key from _COMPOSITION_MAP.
        index: Instance index for name generation.
        visibility_range_override: Override default visibility range.

    Raises:
        KeyError: If composition_key is not recognized.
    """
    role_name, _persona, name_template = _COMPOSITION_MAP[composition_key]
    config = get_role_config(role_name)

    entity_id = name_template.format(n=index)
    vis_range = visibility_range_override or config.visibility_range_m

    return Entity(
        entity_id=entity_id,
        role=role_name,
        role_config=config,
        hierarchy_level=config.hierarchy_level,
        visibility_range_m=vis_range,
    )


def list_composition_keys() -> dict[str, str]:
    """Return mapping of composition keys to role names."""
    return {k: v[0] for k, v in _COMPOSITION_MAP.items()}
