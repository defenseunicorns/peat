"""
Port-ops simulation bridge API.

Defines tool schemas for each role that the LLM bridge can invoke.
Each role has a set of tools corresponding to its physical actions
and reporting capabilities. Tasking reads from the simulation state
to provide context for tool invocations.
"""

from dataclasses import dataclass
from typing import Any, Optional


@dataclass
class ToolSchema:
    """Schema for a bridge API tool."""

    name: str
    description: str
    parameters: dict[str, Any]
    required_params: list[str]


# --- Signaler Tools ---

SIGNALER_TOOLS: list[ToolSchema] = [
    ToolSchema(
        name="signal_crane",
        description=(
            "Send a visual signal to the crane operator. "
            "Used to authorize or halt crane movements during lift/lower cycles."
        ),
        parameters={
            "signal_type": {
                "type": "string",
                "enum": ["HOIST", "LOWER", "STOP", "CLEAR"],
                "description": "The type of signal to send",
            },
            "crane_id": {
                "type": "string",
                "description": "Identifier of the target crane",
            },
        },
        required_params=["signal_type", "crane_id"],
    ),
    ToolSchema(
        name="report_hazard",
        description=(
            "Report a hazard observed in the operational zone. "
            "Triggers immediate halt of crane operations in the affected area."
        ),
        parameters={
            "hazard_type": {
                "type": "string",
                "enum": ["personnel", "obstruction", "equipment", "weather", "other"],
                "description": "Category of the observed hazard",
            },
            "location": {
                "type": "string",
                "description": "Location description relative to the crane and load",
            },
        },
        required_params=["hazard_type", "location"],
    ),
    ToolSchema(
        name="confirm_ground_clear",
        description=(
            "Confirm that the ground zone beneath the crane is clear of "
            "personnel and obstructions. Required before any lift/lower operation."
        ),
        parameters={},
        required_params=[],
    ),
]


def get_tools_for_role(role_name: str) -> list[ToolSchema]:
    """Return the tool schemas available for a given role.

    Raises:
        KeyError: If the role has no registered tools.
    """
    tools_by_role: dict[str, list[ToolSchema]] = {
        "signaler": SIGNALER_TOOLS,
    }
    return tools_by_role[role_name]


def get_tasking_context(role_name: str, sim_state: dict) -> dict:
    """Extract role-relevant context from simulation state for LLM tasking.

    Args:
        role_name: The role requesting context.
        sim_state: Current simulation state dict.

    Returns:
        Filtered context dict relevant to the role's decision-making.
    """
    if role_name == "signaler":
        return {
            "crane_state": sim_state.get("crane_state", "unknown"),
            "ground_clear": sim_state.get("ground_clear", False),
            "personnel_in_zone": sim_state.get("personnel_in_zone", 0),
            "active_hazards": sim_state.get("active_hazards", []),
            "cycle_phase": sim_state.get("cycle_phase", "idle"),
        }

    return {}
