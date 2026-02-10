"""
Bridge API — tool definitions and handlers for the port terminal simulation.

Each role has a set of tools that its LLM decision function can invoke.
Tools read/write shared simulation state and return structured results.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable


# ---------------------------------------------------------------------------
# Tool definition
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class ToolDef:
    """Describes a single tool available to a role."""
    name: str
    description: str
    handler: Callable[..., dict[str, Any]]


# ---------------------------------------------------------------------------
# Shared simulation state (injected at runtime)
# ---------------------------------------------------------------------------

class SimState:
    """Thin wrapper around the mutable simulation state store."""

    def __init__(self) -> None:
        self.yard_summaries: dict[str, dict[str, Any]] = {}
        self.block_assignments: dict[str, str] = {}  # container_id -> block_id
        self.tractor_routes: dict[str, dict[str, Any]] = {}  # tractor_id -> route
        self.congestion_events: list[dict[str, Any]] = []

    def read_all_block_summaries(self) -> list[dict[str, Any]]:
        """Return every yard-block summary currently stored."""
        return list(self.yard_summaries.values())


# ---------------------------------------------------------------------------
# Yard Manager tools (H3)
# ---------------------------------------------------------------------------

def _handle_update_yard_summary(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Publish a consolidated yard zone summary."""
    zone = params.get("zone", "default")
    state.yard_summaries[zone] = params
    return {"status": "ok", "zone": zone}


def _handle_assign_yard_block(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Direct a container to a specific yard block."""
    container_id = params.get("container_id")
    block_id = params.get("block_id")
    if container_id and block_id:
        state.block_assignments[container_id] = block_id
        return {"status": "assigned", "container_id": container_id, "block_id": block_id}
    # Crane rebalance action
    if params.get("action") == "rebalance_crane":
        return {
            "status": "crane_rebalanced",
            "from_block": params.get("from_block"),
            "to_block": params.get("to_block"),
        }
    return {"status": "noop"}


def _handle_route_tractor(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Set a tractor's routing path through the yard."""
    tractor_id = params.get("tractor_id")
    if tractor_id is None:
        return {"status": "error", "reason": "missing tractor_id"}
    state.tractor_routes[tractor_id] = {
        "target_block": params.get("target_block"),
        "container_id": params.get("container_id"),
    }
    return {"status": "routed", "tractor_id": tractor_id}


def _handle_report_congestion(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Flag a congestion event (may trigger escalation)."""
    state.congestion_events.append(params)
    escalated = params.get("escalate_to") is not None
    return {"status": "escalated" if escalated else "recorded", "event": params}


# -- Tool registry ----------------------------------------------------------

YARD_MANAGER_TOOLS: list[ToolDef] = [
    ToolDef(
        name="update_yard_summary",
        description="Publish consolidated yard zone status aggregating all subordinate block summaries.",
        handler=_handle_update_yard_summary,
    ),
    ToolDef(
        name="assign_yard_block",
        description="Direct a container or crane to a specific yard block.",
        handler=_handle_assign_yard_block,
    ),
    ToolDef(
        name="route_tractor",
        description="Set a tractor's routing path through the yard to a target block.",
        handler=_handle_route_tractor,
    ),
    ToolDef(
        name="report_congestion",
        description="Flag a congestion event for mitigation or TOC escalation.",
        handler=_handle_report_congestion,
    ),
]

# -- Convenience lookup -----------------------------------------------------

_TOOL_REGISTRY: dict[str, dict[str, ToolDef]] = {
    "yard_manager": {t.name: t for t in YARD_MANAGER_TOOLS},
}


def execute_tool(
    role: str,
    tool_name: str,
    params: dict[str, Any],
    state: SimState,
) -> dict[str, Any]:
    """Execute a tool call for a given role.

    Raises KeyError if the role or tool is not registered.
    """
    tool = _TOOL_REGISTRY[role][tool_name]
    return tool.handler(state, params)


def tools_for_role(role: str) -> list[ToolDef]:
    """Return all tools available to a role (for LLM tool-use prompts)."""
    return list(_TOOL_REGISTRY.get(role, {}).values())


def tasking_context(state: SimState) -> dict[str, Any]:
    """Build the read context for yard-manager tasking.

    The yard manager sees all yard block summaries so it can make
    zone-wide routing and balancing decisions.
    """
    return {
        "block_summaries": state.read_all_block_summaries(),
        "active_routes": dict(state.tractor_routes),
        "recent_congestion": state.congestion_events[-10:],
    }
