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
        # Stacking crane state
        self.slot_map: dict[str, dict[str, Any]] = {}  # "block:row:bay:tier" -> container
        self.crane_positions: dict[str, dict[str, Any]] = {}  # crane_id -> position/status
        # Gate operations state
        self.scan_results: dict[str, dict[str, Any]] = {}  # container_id -> scan
        self.truck_log: list[dict[str, Any]] = []  # processed trucks
        self.seal_inspections: dict[str, dict[str, Any]] = {}  # container_id -> seal

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



# ---------------------------------------------------------------------------
# Stacking Crane tools (H1)
# ---------------------------------------------------------------------------

def _handle_stack_container(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Place a container at a specific row/bay/tier slot in the yard block."""
    yard_block = params.get("yard_block", "")
    row = params.get("row", 0)
    bay = params.get("bay", 0)
    tier = params.get("tier", 0)
    container_id = params.get("container_id")
    crane_id = params.get("crane_id")

    slot_key = f"{yard_block}:{row}:{bay}:{tier}"
    if slot_key in state.slot_map:
        return {
            "status": "error",
            "reason": "slot_occupied",
            "slot": slot_key,
        }

    state.slot_map[slot_key] = {
        "container_id": container_id,
        "stacked_by": crane_id,
    }
    return {
        "status": "stacked",
        "container_id": container_id,
        "slot": slot_key,
    }


def _handle_retrieve_container(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Pick up a container from a yard slot for outbound transfer."""
    yard_block = params.get("yard_block", "")
    row = params.get("row", 0)
    bay = params.get("bay", 0)
    tier = params.get("tier", 0)
    container_id = params.get("container_id")

    slot_key = f"{yard_block}:{row}:{bay}:{tier}"
    stored = state.slot_map.get(slot_key)
    if stored is None:
        return {"status": "error", "reason": "slot_empty", "slot": slot_key}

    if stored.get("container_id") != container_id:
        return {
            "status": "error",
            "reason": "container_mismatch",
            "expected": container_id,
            "found": stored.get("container_id"),
        }

    del state.slot_map[slot_key]
    return {
        "status": "retrieved",
        "container_id": container_id,
        "slot": slot_key,
    }


def _handle_report_position(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Broadcast crane position, task status, and subsystem health."""
    crane_id = params.get("crane_id")
    if crane_id is None:
        return {"status": "error", "reason": "missing crane_id"}

    state.crane_positions[crane_id] = {
        "position": params.get("position", {}),
        "status": params.get("status", "unknown"),
        "yard_block": params.get("yard_block"),
        "fault": params.get("fault"),
        "container_id": params.get("container_id"),
    }
    return {"status": "reported", "crane_id": crane_id}


# ---------------------------------------------------------------------------
# Gate Scanner tools (H0)
# ---------------------------------------------------------------------------

def _handle_scan_container(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Perform an optical and weight scan of a container."""
    container_id = params.get("container_id", "UNKNOWN")
    state.scan_results[container_id] = params
    flagged = params.get("damage_detected", False) or not params.get("weight_within_tolerance", True)
    return {"status": "flagged" if flagged else "ok", "container_id": container_id}


# ---------------------------------------------------------------------------
# Gate Worker tools (H1)
# ---------------------------------------------------------------------------

def _handle_verify_documents(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Check truck/container documents against expected manifest."""
    container_id = params.get("container_id", "UNKNOWN")
    valid = params.get("documents_valid", False)
    return {"status": "valid" if valid else "invalid", "container_id": container_id}


def _handle_process_truck(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Complete truck processing (release or reject)."""
    container_id = params.get("container_id", "UNKNOWN")
    action = params.get("action", "release")
    state.truck_log.append(params)
    return {"status": action, "container_id": container_id}


def _handle_inspect_seal(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Record seal inspection result for a container."""
    container_id = params.get("container_id", "UNKNOWN")
    state.seal_inspections[container_id] = params
    intact = params.get("seal_intact", True) and params.get("seal_number_match", True)
    return {"status": "intact" if intact else "compromised", "container_id": container_id}


def _handle_gate_equipment_status(
    state: SimState, params: dict[str, Any],
) -> dict[str, Any]:
    """Report gate equipment or lane operational status."""
    return {"status": "recorded", "details": params}


# -- Tool registries --------------------------------------------------------

GATE_SCANNER_TOOLS: list[ToolDef] = [
    ToolDef(
        name="scan_container",
        description="Perform optical and weight scan of a container at the gate lane.",
        handler=_handle_scan_container,
    ),
    ToolDef(
        name="report_equipment_status",
        description="Report scanner operational status (OPERATIONAL, DEGRADED, FAILED).",
        handler=_handle_gate_equipment_status,
    ),
]

GATE_WORKER_TOOLS: list[ToolDef] = [
    ToolDef(
        name="verify_documents",
        description="Check truck/container documents (bill of lading, customs clearance) against manifest.",
        handler=_handle_verify_documents,
    ),
    ToolDef(
        name="process_truck",
        description="Complete truck processing — release to yard or reject with reason code.",
        handler=_handle_process_truck,
    ),
    ToolDef(
        name="inspect_seal",
        description="Record seal inspection result (intact, broken, number mismatch).",
        handler=_handle_inspect_seal,
    ),
    ToolDef(
        name="report_equipment_status",
        description="Report gate lane operational status.",
        handler=_handle_gate_equipment_status,
    ),
]

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

STACKING_CRANE_TOOLS: list[ToolDef] = [
    ToolDef(
        name="stack_container",
        description="Place a container at the assigned row/bay/tier slot in the yard block.",
        handler=_handle_stack_container,
    ),
    ToolDef(
        name="retrieve_container",
        description="Pick up a container from a yard slot for outbound transfer.",
        handler=_handle_retrieve_container,
    ),
    ToolDef(
        name="report_position",
        description="Broadcast crane position, task status, and subsystem health.",
        handler=_handle_report_position,
    ),
]


# -- Convenience lookup -----------------------------------------------------

_TOOL_REGISTRY: dict[str, dict[str, ToolDef]] = {
    "yard_manager": {t.name: t for t in YARD_MANAGER_TOOLS},
    "stacking_crane": {t.name: t for t in STACKING_CRANE_TOOLS},
    "gate_scanner": {t.name: t for t in GATE_SCANNER_TOOLS},
    "rfid_reader": {t.name: t for t in GATE_SCANNER_TOOLS},
    "gate_worker": {t.name: t for t in GATE_WORKER_TOOLS},
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
