"""
BridgeAPI — per-agent interface to a shared HiveStateStore.

Duck-types the MCP ClientSession interface (read_resource, list_tools, call_tool)
so AgentLoop works unchanged whether connected via MCP stdio or in-process bridge.

Phase 1a: Multiple agents share one HiveStateStore in a single asyncio process.
Phase 0 single-agent mode continues to work via MCP stdio transport.
"""

from __future__ import annotations

import json
import logging
import time
from dataclasses import dataclass, field
from typing import Any

from .hive_state import HiveStateStore

logger = logging.getLogger(__name__)


# ── Shim dataclasses matching MCP SDK shapes ─────────────────────────────────
# AgentLoop expects result.contents[0].text (read_resource)
# and result.content[0].text (call_tool) and result.tools (list_tools).

@dataclass
class TextContentShim:
    text: str
    type: str = "text"


@dataclass
class ReadResourceResultShim:
    contents: list[TextContentShim]


@dataclass
class CallToolResultShim:
    content: list[TextContentShim]


@dataclass
class ToolShim:
    name: str
    description: str
    inputSchema: dict


@dataclass
class ListToolsResultShim:
    tools: list[ToolShim]


# ── Crane tool definitions ───────────────────────────────────────────────────

CRANE_TOOLS = [
    ToolShim(
        name="update_capability",
        description=(
            "Update a capability assertion on this node. "
            "Use dot notation for nested fields (e.g., 'capabilities.container_lift.moves_per_hour')."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "field": {
                    "type": "string",
                    "description": "Dot-notation path to the field to update",
                },
                "value": {
                    "description": "New value for the field",
                },
            },
            "required": ["field", "value"],
        },
    ),
    ToolShim(
        name="complete_container_move",
        description=(
            "Report that a container move has been completed. "
            "This advances the queue and emits a move completion event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "The container ID that was moved (e.g., MSCU-4472891)",
                },
            },
            "required": ["container_id"],
        },
    ),
    ToolShim(
        name="report_equipment_status",
        description=(
            "Report a change in equipment status. Use when detecting degradation, "
            "failure, or recovery. Status: OPERATIONAL, DEGRADED, FAILED, MAINTENANCE."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["OPERATIONAL", "DEGRADED", "FAILED", "MAINTENANCE"],
                    "description": "New equipment status",
                },
                "details": {
                    "type": "string",
                    "description": "Description of the status change (e.g., 'Hydraulic pressure at 65%')",
                },
            },
            "required": ["status", "details"],
        },
    ),
    ToolShim(
        name="request_support",
        description=(
            "Escalate a capability gap to the hold aggregator / berth manager. "
            "Use when you cannot proceed without additional resources."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "capability_needed": {
                    "type": "string",
                    "description": "The capability type needed (e.g., HAZMAT_CERTIFIED_OPERATOR)",
                },
                "reason": {
                    "type": "string",
                    "description": "Why this capability is needed",
                },
            },
            "required": ["capability_needed", "reason"],
        },
    ),
]

# ── Operator tool definitions ────────────────────────────────────────────────

OPERATOR_TOOLS = [
    ToolShim(
        name="report_available",
        description=(
            "Set your availability status. Call at shift start, "
            "when going on break, or changing availability."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["AVAILABLE", "BUSY", "BREAK", "OFF_SHIFT"],
                    "description": "Your current availability status",
                },
                "details": {
                    "type": "string",
                    "description": "Additional details (e.g., 'Starting shift', 'Taking 15-min break')",
                },
            },
            "required": ["status"],
        },
    ),
    ToolShim(
        name="accept_assignment",
        description=(
            "Accept assignment to a crane. You will be paired with the crane "
            "until the move completes or you are released."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "crane_id": {
                    "type": "string",
                    "description": "The crane node_id to accept assignment for (e.g., crane-1)",
                },
            },
            "required": ["crane_id"],
        },
    ),
    ToolShim(
        name="complete_assignment",
        description=(
            "Release yourself from current crane assignment after the move completes. "
            "Returns you to AVAILABLE status."
        ),
        inputSchema={
            "type": "object",
            "properties": {},
        },
    ),
    ToolShim(
        name="report_hazmat_status",
        description=(
            "Report the result of a hazmat container inspection."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID inspected",
                },
                "assessment": {
                    "type": "string",
                    "enum": ["CLEARED", "HOLD", "ESCALATE"],
                    "description": "Inspection result",
                },
            },
            "required": ["container_id", "assessment"],
        },
    ),
]

# ── Aggregator tool definitions ──────────────────────────────────────────────

AGGREGATOR_TOOLS = [
    ToolShim(
        name="update_hold_summary",
        description=(
            "Update the hold-level team summary with aggregated metrics. "
            "Computes rates, totals, and status from all H1 entity states."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "moves_per_hour": {
                    "type": "number",
                    "description": "Computed aggregate moves/hour across all cranes",
                },
                "status": {
                    "type": "string",
                    "enum": ["FORMING", "ACTIVE", "DEGRADED", "PAUSED"],
                    "description": "Hold team status",
                },
                "summary": {
                    "type": "string",
                    "description": "Brief human-readable summary",
                },
            },
            "required": ["moves_per_hour", "status", "summary"],
        },
    ),
    ToolShim(
        name="emit_hold_event",
        description=(
            "Emit a hold-level event (rate drop, gap detected, milestone reached). "
            "Events propagate up the HIVE hierarchy."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "event_type": {
                    "type": "string",
                    "description": "Event type (e.g., rate_drop, gap_detected, milestone)",
                },
                "details": {
                    "type": "string",
                    "description": "Event details",
                },
                "priority": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "CRITICAL"],
                    "description": "Event priority",
                },
            },
            "required": ["event_type", "details", "priority"],
        },
    ),
]

# ── Tractor tool definitions ───────────────────────────────────────────────

TRACTOR_TOOLS = [
    ToolShim(
        name="transport_container",
        description=(
            "Claim a discharged container from the transport queue and deliver "
            "it to the destination yard block."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to transport (e.g., MSCU-4472891)",
                },
                "destination_block": {
                    "type": "string",
                    "description": "Destination yard block (e.g., YB-A01)",
                },
            },
            "required": ["container_id", "destination_block"],
        },
    ),
    ToolShim(
        name="report_position",
        description=(
            "Report current tractor position and status in the yard."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "zone": {"type": "string", "description": "Current zone (yard, berth, lane)"},
                "block": {"type": "string", "description": "Current block (YB-A, DEPOT, etc.)"},
                "status": {
                    "type": "string",
                    "enum": ["IDLE", "IN_TRANSIT", "LOADING", "CHARGING"],
                    "description": "Current movement status",
                },
            },
            "required": ["zone", "block", "status"],
        },
    ),
    ToolShim(
        name="request_charge",
        description=(
            "Request battery charging. Sets status to CHARGING and emits resupply event."
        ),
        inputSchema={
            "type": "object",
            "properties": {},
        },
    ),
    ToolShim(
        name="report_equipment_status",
        description=(
            "Report a change in equipment status. Status: OPERATIONAL, DEGRADED, FAILED, MAINTENANCE."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["OPERATIONAL", "DEGRADED", "FAILED", "MAINTENANCE"],
                    "description": "New equipment status",
                },
                "details": {
                    "type": "string",
                    "description": "Description of the status change",
                },
            },
            "required": ["status", "details"],
        },
    ),
]

# ── Scheduler tool definitions ─────────────────────────────────────────────

SCHEDULER_TOOLS = [
    ToolShim(
        name="rebalance_assignments",
        description=(
            "Reassign operators between cranes based on workload. "
            "Emits operator_reassigned events."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "assignments": {
                    "type": "object",
                    "description": "Mapping of operator_id → crane_id reassignments",
                },
            },
            "required": ["assignments"],
        },
    ),
    ToolShim(
        name="update_priority_queue",
        description=(
            "Reorder the unclaimed container priority queue."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "priority_order": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Ordered list of container IDs (highest priority first)",
                },
            },
            "required": ["priority_order"],
        },
    ),
    ToolShim(
        name="dispatch_resource",
        description=(
            "Assign a resource (tractor, operator) to an entity. "
            "Emits resource_dispatched event with HIGH priority."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "resource_type": {"type": "string", "description": "Type of resource (tractor, operator)"},
                "from_entity": {"type": "string", "description": "Source entity ID"},
                "to_entity": {"type": "string", "description": "Target entity ID"},
                "reason": {"type": "string", "description": "Reason for dispatch"},
            },
            "required": ["resource_type", "from_entity", "to_entity", "reason"],
        },
    ),
    ToolShim(
        name="assign_container",
        description=(
            "Assign a specific container to specific workers (crane, operator, tractor). "
            "Creates a container_assignment record tracking the full lifecycle. "
            "Status progresses: QUEUED → IN_PROGRESS → DISCHARGED → TRANSPORTED → SECURED."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to assign (e.g., MSCU-4472891)",
                },
                "assigned_crane": {
                    "type": "string",
                    "description": "Crane node_id to assign (e.g., crane-1)",
                },
                "assigned_operator": {
                    "type": "string",
                    "description": "Operator node_id to assign (e.g., op-1)",
                },
                "assigned_tractor": {
                    "type": "string",
                    "description": "Tractor node_id to assign (e.g., tractor-1)",
                },
            },
            "required": ["container_id", "assigned_crane"],
        },
    ),
    ToolShim(
        name="emit_schedule_event",
        description=(
            "Emit a schedule-level event up the HIVE hierarchy."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "event_type": {"type": "string", "description": "Event type"},
                "details": {"type": "string", "description": "Event details"},
                "priority": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "CRITICAL"],
                    "description": "Event priority",
                },
            },
            "required": ["event_type", "details", "priority"],
        },
    ),
]

# ── Berth Manager tool definitions ─────────────────────────────────────────

BERTH_MANAGER_TOOLS = [
    ToolShim(
        name="update_berth_summary",
        description=(
            "Update the berth-level summary with aggregated metrics from all holds. "
            "Computes total rate, hold statuses, and completion ETA."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "total_moves_per_hour": {
                    "type": "number",
                    "description": "Aggregate moves/hour across all holds",
                },
                "hold_statuses": {
                    "type": "object",
                    "description": "Mapping of hold_id → status",
                },
                "completion_eta_minutes": {
                    "type": "number",
                    "description": "Estimated minutes to complete all berth operations",
                },
                "summary": {
                    "type": "string",
                    "description": "Brief human-readable berth summary",
                },
            },
            "required": ["total_moves_per_hour", "hold_statuses", "completion_eta_minutes", "summary"],
        },
    ),
    ToolShim(
        name="emit_berth_event",
        description=(
            "Emit a berth-level event (cross-hold gap, hold gap escalation, milestone). "
            "Events propagate up the HIVE hierarchy with IMMEDIATE_PROPAGATE policy."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "event_type": {
                    "type": "string",
                    "description": "Event type (e.g., cross_hold_gap, hold_gap_escalation)",
                },
                "details": {
                    "type": "string",
                    "description": "Event details",
                },
                "priority": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "CRITICAL"],
                    "description": "Event priority",
                },
            },
            "required": ["event_type", "details", "priority"],
        },
    ),
    ToolShim(
        name="request_tractor_rebalance",
        description=(
            "Request tractor rebalancing between holds. "
            "Emits tractor_rebalance_requested event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "from_hold": {
                    "type": "string",
                    "description": "Hold to move tractor from",
                },
                "to_hold": {
                    "type": "string",
                    "description": "Hold to move tractor to",
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the rebalance request",
                },
            },
            "required": ["from_hold", "to_hold", "reason"],
        },
    ),
    ToolShim(
        name="reassign_worker",
        description=(
            "Reassign a worker (operator, lashing crew) from one hold to another. "
            "Use when a hold is underperforming due to workforce gaps. "
            "Emits worker_reassigned event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "worker_id": {
                    "type": "string",
                    "description": "Worker node_id to reassign (e.g., op-2)",
                },
                "from_hold": {
                    "type": "string",
                    "description": "Hold the worker is currently in",
                },
                "to_hold": {
                    "type": "string",
                    "description": "Hold to reassign the worker to",
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the reassignment",
                },
            },
            "required": ["worker_id", "from_hold", "to_hold", "reason"],
        },
    ),
    ToolShim(
        name="escalate_to_scheduler",
        description=(
            "Escalate a structural problem to the scheduler that requires "
            "stow plan changes or broader coordination. Use for crane failures, "
            "persistent throughput gaps, or issues beyond berth-level fixes. "
            "Emits scheduler_escalation event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "issue_type": {
                    "type": "string",
                    "enum": [
                        "crane_failure",
                        "persistent_throughput_gap",
                        "stow_plan_change_needed",
                        "workforce_shortage",
                        "equipment_cascade_failure",
                    ],
                    "description": "Category of structural issue",
                },
                "details": {
                    "type": "string",
                    "description": "Detailed description of the issue and impact",
                },
            },
            "required": ["issue_type", "details"],
        },
    ),
    ToolShim(
        name="update_hold_priority",
        description=(
            "Adjust the priority level of a hold to direct more or fewer "
            "shared resources to it. Higher priority holds get preferential "
            "tractor and worker allocation. Emits hold_priority_changed event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "hold_num": {
                    "type": "integer",
                    "description": "Hold number (1, 2, or 3)",
                },
                "priority_level": {
                    "type": "string",
                    "enum": ["LOW", "NORMAL", "HIGH", "CRITICAL"],
                    "description": "New priority level for the hold",
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for the priority change",
                },
            },
            "required": ["hold_num", "priority_level"],
        },
    ),
]

# ── Sensor tool definitions ────────────────────────────────────────────────

SENSOR_TOOLS = [
    ToolShim(
        name="emit_reading",
        description=(
            "Emit a sensor reading. If value diverges >5%% from expected, "
            "also emits an anomaly_detected event."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "reading_type": {
                    "type": "string",
                    "description": "Type of reading (weight, rfid_tag, temperature)",
                },
                "value": {"type": "number", "description": "Sensor reading value"},
                "unit": {"type": "string", "description": "Unit (tons, id, celsius)"},
                "container_id": {
                    "type": "string",
                    "description": "Associated container ID (optional)",
                },
            },
            "required": ["reading_type", "value", "unit"],
        },
    ),
    ToolShim(
        name="report_calibration",
        description=(
            "Report calibration status. If accuracy below threshold, emits CALIBRATION_DRIFT."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "accuracy_pct": {"type": "number", "description": "Current accuracy percentage (0-100)"},
                "drift": {"type": "number", "description": "Drift magnitude"},
                "status": {
                    "type": "string",
                    "enum": ["CALIBRATED", "DRIFTING", "NEEDS_RECALIBRATION"],
                    "description": "Calibration status",
                },
            },
            "required": ["accuracy_pct", "drift", "status"],
        },
    ),
]

# ── Lashing Crew tool definitions ────────────────────────────────────────────

LASHING_CREW_TOOLS = [
    ToolShim(
        name="secure_container",
        description=(
            "Secure a container after crane placement using twist-locks or lashing rods. "
            "Only call after receiving crane clear signal for this container."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to secure (e.g., MSCU-4472891)",
                },
                "lashing_type": {
                    "type": "string",
                    "enum": ["twist_lock", "lashing_rod"],
                    "description": "Type of securing method",
                },
            },
            "required": ["container_id", "lashing_type"],
        },
    ),
    ToolShim(
        name="report_lashing_complete",
        description=(
            "Report that a container has been fully secured and lashing is complete. "
            "Emits a lashing_complete event so the next operation can proceed."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID that was secured",
                },
            },
            "required": ["container_id"],
        },
    ),
    ToolShim(
        name="inspect_lashing",
        description=(
            "Inspect existing lashings on a container for integrity. "
            "Reports PASS, DEGRADED, or FAIL."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID to inspect",
                },
                "result": {
                    "type": "string",
                    "enum": ["PASS", "DEGRADED", "FAIL"],
                    "description": "Inspection result",
                },
            },
            "required": ["container_id", "result"],
        },
    ),
    ToolShim(
        name="request_lashing_tools",
        description=(
            "Request replacement lashing tools (twist-lock keys, lashing rods, turnbuckles). "
            "Use when tools are worn or damaged."
        ),
        inputSchema={
            "type": "object",
            "properties": {},
        },
    ),
]


# ── Signaler tool definitions ────────────────────────────────────────────────

SIGNALER_TOOLS = [
    ToolShim(
        name="signal_crane",
        description=(
            "Send a visual signal to the crane operator. "
            "Used to authorize or halt crane movements during lift/lower cycles."
        ),
        inputSchema={
            "type": "object",
            "properties": {
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
            "required": ["signal_type", "crane_id"],
        },
    ),
    ToolShim(
        name="report_hazard",
        description=(
            "Report a hazard observed in the operational zone. "
            "Triggers immediate halt of crane operations in the affected area."
        ),
        inputSchema={
            "type": "object",
            "properties": {
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
            "required": ["hazard_type", "location"],
        },
    ),
    ToolShim(
        name="confirm_ground_clear",
        description=(
            "Confirm that the ground zone beneath the crane is clear of "
            "personnel and obstructions. Required before any lift/lower operation."
        ),
        inputSchema={
            "type": "object",
            "properties": {},
        },
    ),
]


# ── BridgeAPI ────────────────────────────────────────────────────────────────

class BridgeAPI:
    """
    Per-agent interface to a shared HiveStateStore.

    Duck-types the MCP ClientSession interface so AgentLoop can use it
    transparently instead of a real MCP connection.
    """

    def __init__(
        self,
        store: HiveStateStore,
        node_id: str,
        role: str = "crane",
        hold_id: str = "hold-3",
        hold_num: int | None = None,
    ):
        self.store = store
        self.node_id = node_id
        self.role = role
        self.hold_id = hold_id
        self.hold_num = hold_num
        self._entity_doc_id = f"sim_doc_{node_id}"

    # ── Internal helpers ─────────────────────────────────────────────────

    def _get_entity_doc(self):
        return self.store.get_document("node_states", self._entity_doc_id)

    def _get_team_doc(self):
        return self.store.get_document("team_summaries", f"team_{self.hold_id}")

    def _get_queue_doc(self):
        return self.store.get_document("container_queues", f"queue_{self.hold_id}")

    # ── MCP-compatible interface ─────────────────────────────────────────

    async def read_resource(self, uri) -> ReadResourceResultShim:
        """Read a HIVE resource (same logic as server.py handlers)."""
        uri = str(uri)

        if uri == "hive://my-capabilities":
            doc = self._get_entity_doc()
            if doc:
                text = json.dumps(doc.fields, indent=2, default=str)
            else:
                text = json.dumps({"error": "Entity document not found"})

        elif uri == "hive://team-state":
            doc = self._get_team_doc()
            if doc:
                text = json.dumps(doc.fields, indent=2, default=str)
            else:
                text = json.dumps({"error": "Team state not found"})

        elif uri == "hive://container-queue":
            doc = self._get_queue_doc()
            if doc:
                fields = doc.fields.copy()
                all_containers = fields.get("containers", [])
                # Show next 5 unclaimed/uncompleted containers
                pending = [
                    c for c in all_containers
                    if c.get("status") != "COMPLETED" and not c.get("claimed_by")
                ]
                fields["next_containers"] = pending[:5]
                fields["containers"] = f"[{len(all_containers)} total, {len(pending)} pending, showing next 5]"
                text = json.dumps(fields, indent=2, default=str)
            else:
                text = json.dumps({"error": "Container queue not found"})

        elif uri == "hive://operator-assignments":
            requests = self.store.get_pending_operator_requests(self.hold_id)
            text = json.dumps(
                [r.fields for r in requests], indent=2, default=str
            )

        elif uri == "hive://transport-queue":
            tq = self.store.get_document("transport_queues", f"transport_{self.hold_id}")
            if tq:
                jobs = tq.fields.get("pending_jobs", [])
                pending = [j for j in jobs if j.get("status") == "PENDING"]
                text = json.dumps({
                    "hold_id": self.hold_id,
                    "pending_jobs": pending[:5],
                    "total_pending": len(pending),
                    "completed_count": tq.fields.get("completed_count", 0),
                }, indent=2, default=str)
            else:
                text = json.dumps({"error": "Transport queue not found"})

        elif uri == "hive://container-assignments":
            doc = self.store.get_document(
                "container_assignments", f"assignments_{self.hold_id}"
            )
            if doc:
                assignments = doc.fields.get("assignments", {})
                # Filter to show assignments relevant to this agent
                if self.role == "crane":
                    relevant = {
                        k: v for k, v in assignments.items()
                        if v.get("assigned_crane") == self.node_id
                    }
                elif self.role == "tractor":
                    relevant = {
                        k: v for k, v in assignments.items()
                        if v.get("assigned_tractor") == self.node_id
                    }
                elif self.role == "operator":
                    relevant = {
                        k: v for k, v in assignments.items()
                        if v.get("assigned_operator") == self.node_id
                    }
                else:
                    relevant = assignments
                text = json.dumps({
                    "hold_id": self.hold_id,
                    "assignments": relevant,
                    "total_assigned": len(assignments),
                    "status_breakdown": self.store.get_container_status_breakdown(
                        self.hold_id
                    ),
                }, indent=2, default=str)
            else:
                text = json.dumps({"error": "Container assignments not initialized"})

        elif uri == "hive://tasking":
            if self.role == "aggregator":
                text = self._read_aggregator_tasking()
            elif self.role == "berth_manager":
                text = self._read_berth_manager_tasking()
            elif self.role == "operator":
                text = self._read_operator_tasking()
            elif self.role == "tractor":
                text = self._read_tractor_tasking()
            elif self.role == "scheduler":
                text = self._read_scheduler_tasking()
            elif self.role == "sensor":
                text = self._read_sensor_tasking()
            elif self.role == "lashing_crew":
                text = self._read_lashing_crew_tasking()
            elif self.role == "signaler":
                text = self._read_signaler_tasking()
            else:
                text = self._read_crane_tasking()

        elif uri == "hive://debug/state-dump":
            dump = {"documents": {}, "events": self.store.get_events()}
            for col_name, col_docs in self.store._collections.items():
                dump["documents"][col_name] = {}
                for doc_id, doc in col_docs.items():
                    dump["documents"][col_name][doc_id] = doc.fields
            text = json.dumps(dump, indent=2, default=str)

        else:
            text = json.dumps({"error": f"Unknown resource: {uri}"})

        return ReadResourceResultShim(contents=[TextContentShim(text=text)])

    def _read_crane_tasking(self) -> str:
        entity = self._get_entity_doc()
        queue = self._get_queue_doc()
        # Get containers assigned to this crane
        my_assignments = self.store.get_container_assignments_by_role(
            self.hold_id, "assigned_crane", self.node_id
        )
        assigned_pending = [
            a for a in my_assignments
            if a.get("status") in ("QUEUED", "IN_PROGRESS")
        ]
        tasking = {
            "directive": "PROCESS_CONTAINERS",
            "assignment": entity.fields.get("assignment", {}) if entity else {},
            "containers_remaining": (
                queue.fields.get("total_containers", 0) - queue.fields.get("completed_count", 0)
                if queue else 0
            ),
            "assigned_containers": [a["container_id"] for a in assigned_pending],
            "total_assigned": len(my_assignments),
            "target_rate": 35,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_aggregator_tasking(self) -> str:
        team = self._get_team_doc()
        queue = self._get_queue_doc()
        # Aggregator only sees entities scoped to its own hold
        hold_members = {}
        if team:
            for mid, mdata in team.fields.get("team_members", {}).items():
                hold_members[mid] = mdata
        container_status = self.store.get_container_status_breakdown(self.hold_id)
        tasking = {
            "directive": "AGGREGATE_HOLD_STATUS",
            "hold_id": self.hold_id,
            "hold_num": self.hold_num,
            "team_size": len(hold_members),
            "moves_completed": team.fields.get("moves_completed", 0) if team else 0,
            "moves_remaining": team.fields.get("moves_remaining", 0) if team else 0,
            "gap_count": len(team.fields.get("gap_analysis", [])) if team else 0,
            "container_status_breakdown": container_status,
            "target_rate": 35,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_operator_tasking(self) -> str:
        entity = self._get_entity_doc()
        requests = self.store.get_pending_operator_requests(self.hold_id)
        assigned_to = entity.fields.get("assignment", {}).get("assigned_to") if entity else None
        status = entity.fields.get("operational_status", "AVAILABLE") if entity else "UNKNOWN"
        tasking = {
            "directive": "OPERATE_CRANE",
            "status": status,
            "assigned_to": assigned_to,
            "pending_requests": [r.fields.get("crane_id") for r in requests],
            "instructions": (
                "Check in as AVAILABLE at shift start. Accept crane assignments when "
                "pending. Complete assignment after crane move finishes. Report hazmat "
                "status for hazmat containers. Take breaks as needed."
            ),
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_tractor_tasking(self) -> str:
        entity = self._get_entity_doc()
        tq = self.store.get_document("transport_queues", f"transport_{self.hold_id}")
        pending_jobs = []
        if tq:
            pending_jobs = [
                j for j in tq.fields.get("pending_jobs", [])
                if j.get("status") == "PENDING"
            ]
        # Get containers specifically assigned to this tractor
        my_assignments = self.store.get_container_assignments_by_role(
            self.hold_id, "assigned_tractor", self.node_id
        )
        assigned_discharged = [
            a["container_id"] for a in my_assignments
            if a.get("status") == "DISCHARGED"
        ]
        tasking = {
            "directive": "TRANSPORT_CONTAINERS",
            "position": entity.fields.get("position", {}) if entity else {},
            "battery_pct": entity.get_field("equipment_health.battery_pct", 100) if entity else 100,
            "pending_transport_jobs": pending_jobs[:3],
            "assigned_containers": assigned_discharged,
            "trips_completed": entity.get_field("metrics.trips_completed", 0) if entity else 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_scheduler_tasking(self) -> str:
        team = self._get_team_doc()
        queue = self._get_queue_doc()
        members = team.fields.get("team_members", {}) if team else {}
        container_status = self.store.get_container_status_breakdown(self.hold_id)
        # Get unassigned containers from queue
        unassigned = []
        if queue:
            assign_doc = self.store.get_document(
                "container_assignments", f"assignments_{self.hold_id}"
            )
            assigned_ids = set()
            if assign_doc:
                assigned_ids = set(assign_doc.fields.get("assignments", {}).keys())
            for c in queue.fields.get("containers", []):
                cid = c.get("container_id")
                if cid and cid not in assigned_ids and c.get("status") != "COMPLETED":
                    unassigned.append(cid)
        tasking = {
            "directive": "COORDINATE_HOLD",
            "hold_id": self.hold_id,
            "team_size": len(members),
            "team_members": {
                k: {"status": v.get("status"), "entity_type": v.get("entity_type")}
                for k, v in members.items()
            },
            "moves_completed": team.get_field("moves_completed", 0) if team else 0,
            "moves_remaining": team.get_field("moves_remaining", 0) if team else 0,
            "gap_count": len(team.get_field("gap_analysis", [])) if team else 0,
            "container_status_breakdown": container_status,
            "unassigned_containers": unassigned[:10],
            "total_unassigned": len(unassigned),
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_sensor_tasking(self) -> str:
        entity = self._get_entity_doc()
        tasking = {
            "directive": "EMIT_READINGS",
            "sensor_type": entity.get_field("sensor_type", "UNKNOWN") if entity else "UNKNOWN",
            "calibration": entity.fields.get("calibration", {}) if entity else {},
            "readings_emitted": entity.get_field("metrics.readings_emitted", 0) if entity else 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_lashing_crew_tasking(self) -> str:
        entity = self._get_entity_doc()
        # Find containers recently discharged by cranes (available for lashing)
        events = self.store.get_events()
        pending_containers = []
        for evt in events:
            if evt.get("event_type") == "container_move_complete":
                pending_containers.append({
                    "container_id": evt.get("container_id"),
                    "crane_id": evt.get("source"),
                })
        tasking = {
            "directive": "SECURE_CONTAINERS",
            "containers_secured": entity.get_field("metrics.containers_secured", 0) if entity else 0,
            "lashings_inspected": entity.get_field("metrics.lashings_inspected", 0) if entity else 0,
            "equipment_health": entity.fields.get("equipment_health", {}) if entity else {},
            "pending_containers": pending_containers[-5:],
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_signaler_tasking(self) -> str:
        entity = self._get_entity_doc()
        # Check crane states in the hold to determine active operations
        team = self._get_team_doc()
        crane_state = "idle"
        assigned_crane = "crane-1"
        if team:
            members = team.fields.get("team_members", {})
            for mid, mdata in members.items():
                if mdata.get("entity_type") == "gantry_crane":
                    assigned_crane = mid
                    if mdata.get("status") == "OPERATIONAL":
                        crane_state = "LIFTING"
                    break
        tasking = {
            "directive": "SIGNAL_OPERATIONS",
            "crane_state": crane_state,
            "assigned_crane": assigned_crane,
            "ground_clear": True,
            "personnel_in_zone": 0,
            "active_hazards": [],
            "signals_sent": entity.get_field("metrics.signals_sent", 0) if entity else 0,
            "hazards_reported": entity.get_field("metrics.hazards_reported", 0) if entity else 0,
        }
        return json.dumps(tasking, indent=2, default=str)

    def _read_berth_manager_tasking(self) -> str:
        # Aggregate all hold docs from team_summaries into berth-level context
        hold_summaries = []
        total_remaining = 0
        for doc_id, doc in self.store._collections.get("team_summaries", {}).items():
            fields = doc.fields
            hold_id = fields.get("hold_id", doc_id.replace("team_", ""))
            moves_per_hour = fields.get("moves_per_hour", 0)
            status = fields.get("status", "UNKNOWN")
            gap_count = len(fields.get("gap_analysis", []))
            remaining = fields.get("moves_remaining", 0)
            total_remaining += remaining

            # Count workers and equipment by type per hold
            members = fields.get("team_members", {})
            worker_counts = {}
            equipment_status = {}
            for mid, mdata in members.items():
                etype = mdata.get("entity_type", "unknown")
                worker_counts[etype] = worker_counts.get(etype, 0) + 1
                # Track degraded/failed equipment
                mstatus = mdata.get("status", "UNKNOWN")
                if mstatus in ("DEGRADED", "FAILED"):
                    equipment_status[mid] = mstatus

            hold_summaries.append({
                "hold_id": hold_id,
                "moves_per_hour": moves_per_hour,
                "status": status,
                "priority": fields.get("priority", "NORMAL"),
                "gap_count": gap_count,
                "gap_details": fields.get("gap_analysis", []),
                "moves_remaining": remaining,
                "moves_completed": fields.get("moves_completed", 0),
                "target_rate": fields.get("target_moves_per_hour", 35),
                "worker_counts": worker_counts,
                "degraded_equipment": equipment_status,
            })
        tasking = {
            "directive": "AGGREGATE_BERTH_STATUS",
            "berth_id": "berth-5",
            "hold_count": len(hold_summaries),
            "hold_summaries": hold_summaries,
            "total_moves_remaining": total_remaining,
            "priority": "NORMAL",
        }
        return json.dumps(tasking, indent=2, default=str)

    async def list_tools(self) -> ListToolsResultShim:
        """Return tools appropriate for this agent's role."""
        if self.role == "aggregator":
            return ListToolsResultShim(tools=list(AGGREGATOR_TOOLS))
        elif self.role == "berth_manager":
            return ListToolsResultShim(tools=list(BERTH_MANAGER_TOOLS))
        elif self.role == "operator":
            return ListToolsResultShim(tools=list(OPERATOR_TOOLS))
        elif self.role == "tractor":
            return ListToolsResultShim(tools=list(TRACTOR_TOOLS))
        elif self.role == "scheduler":
            return ListToolsResultShim(tools=list(SCHEDULER_TOOLS))
        elif self.role == "sensor":
            return ListToolsResultShim(tools=list(SENSOR_TOOLS))
        elif self.role == "lashing_crew":
            return ListToolsResultShim(tools=list(LASHING_CREW_TOOLS))
        elif self.role == "signaler":
            return ListToolsResultShim(tools=list(SIGNALER_TOOLS))
        return ListToolsResultShim(tools=list(CRANE_TOOLS))

    async def call_tool(self, name: str, arguments: dict) -> CallToolResultShim:
        """Dispatch to tool handler and return shim result."""
        handler = {
            # Crane tools
            "update_capability": self._tool_update_capability,
            "complete_container_move": self._tool_complete_container_move,
            "report_equipment_status": self._tool_report_equipment_status,
            "request_support": self._tool_request_support,
            # Operator tools
            "report_available": self._tool_report_available,
            "accept_assignment": self._tool_accept_assignment,
            "complete_assignment": self._tool_complete_assignment,
            "report_hazmat_status": self._tool_report_hazmat_status,
            # Aggregator tools
            "update_hold_summary": self._tool_update_hold_summary,
            "emit_hold_event": self._tool_emit_hold_event,
            # Berth manager tools
            "update_berth_summary": self._tool_update_berth_summary,
            "emit_berth_event": self._tool_emit_berth_event,
            "request_tractor_rebalance": self._tool_request_tractor_rebalance,
            "reassign_worker": self._tool_reassign_worker,
            "escalate_to_scheduler": self._tool_escalate_to_scheduler,
            "update_hold_priority": self._tool_update_hold_priority,
            # Tractor tools
            "transport_container": self._tool_transport_container,
            "report_position": self._tool_report_position,
            "request_charge": self._tool_request_charge,
            # Scheduler tools
            "rebalance_assignments": self._tool_rebalance_assignments,
            "update_priority_queue": self._tool_update_priority_queue,
            "dispatch_resource": self._tool_dispatch_resource,
            "assign_container": self._tool_assign_container,
            "emit_schedule_event": self._tool_emit_schedule_event,
            # Sensor tools
            "emit_reading": self._tool_emit_reading,
            "report_calibration": self._tool_report_calibration,
            # Lashing crew tools
            "secure_container": self._tool_secure_container,
            "report_lashing_complete": self._tool_report_lashing_complete,
            "inspect_lashing": self._tool_inspect_lashing,
            "request_lashing_tools": self._tool_request_lashing_tools,
            # Signaler tools
            "signal_crane": self._tool_signal_crane,
            "report_hazard": self._tool_report_hazard,
            "confirm_ground_clear": self._tool_confirm_ground_clear,
        }.get(name)

        if handler is None:
            return CallToolResultShim(content=[TextContentShim(text=f"Unknown tool: {name}")])

        text = await handler(arguments)
        return CallToolResultShim(content=[TextContentShim(text=text)])

    # ── Crane tool handlers ──────────────────────────────────────────────

    async def _tool_update_capability(self, arguments: dict) -> str:
        field_path = arguments["field"]
        value = arguments["value"]

        doc = self._get_entity_doc()
        if doc:
            doc.update_field(field_path, value)
            self.store.emit_event({
                "event_type": "capability_update",
                "source": self.node_id,
                "field": field_path,
                "value": value,
                "aggregation_policy": "AGGREGATE_AT_PARENT",
                "priority": "NORMAL",
            })
            logger.info(f"METRICS: capability_update node={self.node_id} field={field_path} value={value}")
            return f"Updated {field_path} to {value}. HIVE state synced."
        return "Error: entity document not found"

    async def _tool_complete_container_move(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        queue = self._get_queue_doc()
        entity = self._get_entity_doc()

        if not queue or not entity:
            return "Error: queue or entity not found"

        # Check operator assignment — crane cannot move without an operator
        team = self._get_team_doc()
        if team:
            members = team.fields.get("team_members", {})
            assigned_operator = None
            for mid, mdata in members.items():
                if mdata.get("entity_type") == "operator":
                    op_doc = self.store.get_document("node_states", f"sim_doc_{mid}")
                    if op_doc and op_doc.get_field("assignment.assigned_to") == self.node_id:
                        assigned_operator = mid
                        break

            if assigned_operator is None:
                # Post an operator request so operators can see it
                self.store.request_operator(self.node_id, self.hold_id)
                logger.info(
                    f"METRICS: crane_blocked_no_operator node={self.node_id} container={container_id}"
                )
                return (
                    f"Error: No operator assigned to {self.node_id}. "
                    f"Cannot move container {container_id} without an operator. "
                    f"Operator request posted."
                )

            # Check hazmat certification if needed
            containers = queue.fields.get("containers", [])
            target = next((c for c in containers if c["container_id"] == container_id), None)
            if target and target.get("hazmat"):
                op_doc = self.store.get_document("node_states", f"sim_doc_{assigned_operator}")
                if op_doc:
                    hazmat_cert = op_doc.get_field("capabilities.hazmat_handling.certification_valid", False)
                    if not hazmat_cert:
                        logger.info(
                            f"METRICS: hazmat_cert_fail node={self.node_id} "
                            f"operator={assigned_operator} container={container_id}"
                        )
                        return (
                            f"Error: Operator {assigned_operator} not hazmat-certified. "
                            f"Cannot move hazmat container {container_id}."
                        )

        # Attempt atomic claim via lock
        claimed = await self.store.claim_next_container(
            self.hold_id, container_id, self.node_id
        )
        if not claimed:
            logger.info(
                f"METRICS: claim_failed node={self.node_id} container={container_id}"
            )
            return f"Error: container {container_id} already claimed by another crane"

        # Container was claimed — update metrics
        containers = queue.fields.get("containers", [])
        weight = next(
            (c["weight_tons"] for c in containers if c["container_id"] == container_id),
            25.0,
        )

        # Update container assignment status: QUEUED/IN_PROGRESS → DISCHARGED
        self.store.update_container_status(self.hold_id, container_id, "DISCHARGED")

        # Update crane entity metrics
        moves = entity.get_field("metrics.moves_completed", 0)
        entity.update_field("metrics.moves_completed", moves + 1)
        total_tons = entity.get_field("metrics.total_tons_lifted", 0.0)
        entity.update_field("metrics.total_tons_lifted", total_tons + weight)

        # Emit event
        self.store.emit_event({
            "event_type": "container_move_complete",
            "source": self.node_id,
            "container_id": container_id,
            "weight_tons": weight,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        # Update team summary
        team = self._get_team_doc()
        if team:
            team_completed = team.get_field("moves_completed", 0)
            team.update_field("moves_completed", team_completed + 1)
            remaining = team.get_field("moves_remaining", 0)
            team.update_field("moves_remaining", max(0, remaining - 1))

        # Enqueue for tractor transport (if transport queue exists)
        containers = queue.fields.get("containers", [])
        target_c = next((c for c in containers if c["container_id"] == container_id), None)
        dest = target_c.get("destination_block", "YB-A01") if target_c else "YB-A01"
        self.store.enqueue_transport_job(self.hold_id, container_id, dest)

        logger.info(
            f"METRICS: container_move_complete node={self.node_id} "
            f"container={container_id} weight={weight}t "
            f"total_moves={moves + 1}"
        )

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "crane_discharge",
                "crane_id": self.node_id,
                "container_id": container_id,
                "container_index": queue.fields.get("completed_count", 1) - 1,
                "weight_tons": weight,
                "destination_block": dest,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        completed_count = queue.fields.get("completed_count", 0)
        total_containers = queue.fields["total_containers"]
        remaining_count = total_containers - completed_count

        # Emit scenario_complete when all containers discharged
        if completed_count >= total_containers:
            scenario_complete_event = {
                "event_type": "spatial_update",
                "source": self.node_id,
                "priority": "CRITICAL",
                "details": {
                    "operation": "scenario_complete",
                    "total_containers": completed_count,
                },
            }
            print(json.dumps(scenario_complete_event), flush=True)

        return (
            f"Container {container_id} move completed. "
            f"Weight: {weight}t. Total moves: {moves + 1}. "
            f"Remaining in queue: {remaining_count}."
        )

    async def _tool_report_equipment_status(self, arguments: dict) -> str:
        status = arguments["status"]
        details = arguments["details"]

        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        entity.update_field("operational_status", status)

        priority = "NORMAL"
        if status in ("DEGRADED", "FAILED"):
            priority = "CRITICAL"

        self.store.emit_event({
            "event_type": "equipment_status_change",
            "source": self.node_id,
            "status": status,
            "details": details,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": priority,
        })

        logger.info(
            f"METRICS: equipment_status_change node={self.node_id} "
            f"status={status} details={details}"
        )
        return f"Equipment status updated to {status}. Event propagated with {priority} priority."

    async def _tool_request_support(self, arguments: dict) -> str:
        capability_needed = arguments["capability_needed"]
        reason = arguments["reason"]

        self.store.emit_event({
            "event_type": "support_request",
            "source": self.node_id,
            "capability_needed": capability_needed,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        team = self._get_team_doc()
        if team:
            gaps = team.get_field("gap_analysis", [])
            gaps.append({
                "capability": capability_needed,
                "reason": reason,
                "reported_by": self.node_id,
                "timestamp_us": int(time.time() * 1_000_000),
            })
            team.update_field("gap_analysis", gaps)

        logger.info(
            f"METRICS: support_request node={self.node_id} "
            f"capability={capability_needed} reason={reason}"
        )
        return (
            f"Support request submitted: need {capability_needed}. "
            f"Reason: {reason}. Escalated to hold aggregator."
        )

    # ── Operator tool handlers ────────────────────────────────────────────

    async def _tool_report_available(self, arguments: dict) -> str:
        status = arguments["status"]
        details = arguments.get("details", "")

        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        entity.update_field("operational_status", status)
        if status == "BREAK":
            entity.update_field("shift.status", "BREAK")
        elif status == "OFF_SHIFT":
            entity.update_field("shift.status", "OFF_SHIFT")
        elif status == "AVAILABLE":
            entity.update_field("shift.status", "ON_SHIFT")

        self.store.emit_event({
            "event_type": "operator_status_change",
            "source": self.node_id,
            "status": status,
            "details": details,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: operator_status_change node={self.node_id} status={status}"
        )
        return f"Status updated to {status}. {details}"

    async def _tool_accept_assignment(self, arguments: dict) -> str:
        crane_id = arguments["crane_id"]

        assigned = await self.store.assign_operator(self.node_id, crane_id)
        if not assigned:
            return f"Error: Could not assign to {crane_id} — already assigned or not found"

        # Clear the pending request if one exists
        req_id = f"req_{self.hold_id}_{crane_id}"
        req_doc = self.store.get_document("operator_requests", req_id)
        if req_doc:
            req_doc.update_field("status", "ASSIGNED")
            req_doc.update_field("assigned_operator", self.node_id)

        self.store.emit_event({
            "event_type": "operator_assigned",
            "source": self.node_id,
            "crane_id": crane_id,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "operator_assign",
                "operator_id": self.node_id,
                "crane_id": crane_id,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: operator_assigned node={self.node_id} crane={crane_id}"
        )
        return f"Assigned to {crane_id}. Ready to support crane operations."

    async def _tool_complete_assignment(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        crane_id = entity.get_field("assignment.assigned_to")
        if crane_id is None:
            return "Error: not currently assigned to any crane"

        released = await self.store.release_operator(self.node_id)
        if not released:
            return "Error: could not release assignment"

        self.store.emit_event({
            "event_type": "operator_released",
            "source": self.node_id,
            "crane_id": crane_id,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "operator_release",
                "operator_id": self.node_id,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: operator_released node={self.node_id} crane={crane_id}"
        )
        return f"Released from {crane_id}. Status: AVAILABLE."

    async def _tool_report_hazmat_status(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        assessment = arguments["assessment"]

        entity = self._get_entity_doc()
        if entity:
            inspections = entity.get_field("metrics.hazmat_inspections", 0)
            entity.update_field("metrics.hazmat_inspections", inspections + 1)

        self.store.emit_event({
            "event_type": "hazmat_inspection",
            "source": self.node_id,
            "container_id": container_id,
            "assessment": assessment,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH" if assessment != "CLEARED" else "NORMAL",
        })

        logger.info(
            f"METRICS: hazmat_inspection node={self.node_id} "
            f"container={container_id} assessment={assessment}"
        )
        return f"Hazmat inspection for {container_id}: {assessment}."

    # ── Aggregator tool handlers ─────────────────────────────────────────

    async def _tool_update_hold_summary(self, arguments: dict) -> str:
        moves_per_hour = arguments["moves_per_hour"]
        status = arguments["status"]
        summary = arguments["summary"]

        team = self._get_team_doc()
        if not team:
            return "Error: team summary not found"

        team.update_field("moves_per_hour", moves_per_hour)
        team.update_field("status", status)

        self.store.emit_event({
            "event_type": "hold_summary_update",
            "source": self.node_id,
            "hold_id": self.hold_id,
            "moves_per_hour": moves_per_hour,
            "status": status,
            "summary": summary,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: hold_summary_update node={self.node_id} "
            f"rate={moves_per_hour} status={status}"
        )
        return (
            f"Hold summary updated: {moves_per_hour} moves/hr, status={status}. "
            f"{summary}"
        )

    async def _tool_emit_hold_event(self, arguments: dict) -> str:
        event_type = arguments["event_type"]
        details = arguments["details"]
        priority = arguments["priority"]

        self.store.emit_event({
            "event_type": event_type,
            "source": self.node_id,
            "hold_id": self.hold_id,
            "details": details,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": priority,
        })

        logger.info(
            f"METRICS: hold_event node={self.node_id} "
            f"type={event_type} priority={priority}"
        )
        return f"Hold event emitted: {event_type} ({priority}). {details}"

    # ── Berth Manager tool handlers ─────────────────────────────────────

    async def _tool_update_berth_summary(self, arguments: dict) -> str:
        total_moves_per_hour = arguments["total_moves_per_hour"]
        hold_statuses = arguments["hold_statuses"]
        completion_eta_minutes = arguments["completion_eta_minutes"]
        summary = arguments["summary"]

        entity = self._get_entity_doc()
        if entity:
            summaries = entity.get_field("metrics.summaries_produced", 0)
            entity.update_field("metrics.summaries_produced", summaries + 1)

        self.store.emit_event({
            "event_type": "berth_summary_update",
            "source": self.node_id,
            "total_moves_per_hour": total_moves_per_hour,
            "hold_statuses": hold_statuses,
            "completion_eta_minutes": completion_eta_minutes,
            "summary": summary,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: berth_summary_update node={self.node_id} "
            f"rate={total_moves_per_hour} eta={completion_eta_minutes}min"
        )
        return (
            f"Berth summary updated: {total_moves_per_hour} moves/hr, "
            f"ETA {completion_eta_minutes} min. {summary}"
        )

    async def _tool_emit_berth_event(self, arguments: dict) -> str:
        event_type = arguments["event_type"]
        details = arguments["details"]
        priority = arguments["priority"]

        entity = self._get_entity_doc()
        if entity:
            events_emitted = entity.get_field("metrics.events_emitted", 0)
            entity.update_field("metrics.events_emitted", events_emitted + 1)

        self.store.emit_event({
            "event_type": event_type,
            "source": self.node_id,
            "details": details,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": priority,
        })

        logger.info(
            f"METRICS: berth_event node={self.node_id} "
            f"type={event_type} priority={priority}"
        )
        return f"Berth event emitted: {event_type} ({priority}). {details}"

    async def _tool_request_tractor_rebalance(self, arguments: dict) -> str:
        from_hold = arguments["from_hold"]
        to_hold = arguments["to_hold"]
        reason = arguments["reason"]

        entity = self._get_entity_doc()
        if entity:
            rebalances = entity.get_field("metrics.rebalance_requests", 0)
            entity.update_field("metrics.rebalance_requests", rebalances + 1)

        self.store.emit_event({
            "event_type": "tractor_rebalance_requested",
            "source": self.node_id,
            "from_hold": from_hold,
            "to_hold": to_hold,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: tractor_rebalance node={self.node_id} "
            f"{from_hold}→{to_hold} reason={reason}"
        )
        return f"Tractor rebalance requested: {from_hold} → {to_hold}. Reason: {reason}"

    async def _tool_reassign_worker(self, arguments: dict) -> str:
        worker_id = arguments["worker_id"]
        from_hold = arguments["from_hold"]
        to_hold = arguments["to_hold"]
        reason = arguments["reason"]

        entity = self._get_entity_doc()
        if entity:
            reassignments = entity.get_field("metrics.worker_reassignments", 0)
            entity.update_field("metrics.worker_reassignments", reassignments + 1)

        self.store.emit_event({
            "event_type": "worker_reassigned",
            "source": self.node_id,
            "worker_id": worker_id,
            "from_hold": from_hold,
            "to_hold": to_hold,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: worker_reassigned node={self.node_id} "
            f"worker={worker_id} {from_hold}→{to_hold} reason={reason}"
        )
        return (
            f"Worker {worker_id} reassigned: {from_hold} → {to_hold}. "
            f"Reason: {reason}"
        )

    async def _tool_escalate_to_scheduler(self, arguments: dict) -> str:
        issue_type = arguments["issue_type"]
        details = arguments["details"]

        entity = self._get_entity_doc()
        if entity:
            escalations = entity.get_field("metrics.scheduler_escalations", 0)
            entity.update_field("metrics.scheduler_escalations", escalations + 1)

        self.store.emit_event({
            "event_type": "scheduler_escalation",
            "source": self.node_id,
            "issue_type": issue_type,
            "details": details,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "CRITICAL",
        })

        logger.info(
            f"METRICS: scheduler_escalation node={self.node_id} "
            f"issue_type={issue_type} details={details}"
        )
        return (
            f"Escalated to scheduler: {issue_type}. "
            f"Details: {details}. Priority: CRITICAL."
        )

    async def _tool_update_hold_priority(self, arguments: dict) -> str:
        hold_num = arguments["hold_num"]
        priority_level = arguments["priority_level"]
        reason = arguments.get("reason", "")

        hold_id = f"hold-{hold_num}"

        # Update the team summary doc with the new priority
        team_doc = self.store.get_document("team_summaries", f"team_{hold_id}")
        if team_doc:
            team_doc.update_field("priority", priority_level)

        entity = self._get_entity_doc()
        if entity:
            priority_changes = entity.get_field("metrics.hold_priority_changes", 0)
            entity.update_field("metrics.hold_priority_changes", priority_changes + 1)

        self.store.emit_event({
            "event_type": "hold_priority_changed",
            "source": self.node_id,
            "hold_id": hold_id,
            "hold_num": hold_num,
            "priority_level": priority_level,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: hold_priority_changed node={self.node_id} "
            f"hold={hold_id} priority={priority_level} reason={reason}"
        )
        return (
            f"Hold {hold_id} priority updated to {priority_level}. "
            f"{('Reason: ' + reason) if reason else ''}"
        )

    # ── Tractor tool handlers ────────────────────────────────────────────

    async def _tool_transport_container(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        destination = arguments["destination_block"]

        claimed = await self.store.claim_transport_job(
            self.hold_id, container_id, self.node_id
        )
        if not claimed:
            return f"Error: transport job for {container_id} already claimed or not found"

        entity = self._get_entity_doc()
        if entity:
            entity.update_field("position.status", "IN_TRANSIT")
            entity.update_field("position.block", destination)
            trips = entity.get_field("metrics.trips_completed", 0)
            entity.update_field("metrics.trips_completed", trips + 1)

        # Complete the job and update container assignment status
        self.store.complete_transport_job(self.hold_id, container_id)
        self.store.update_container_status(self.hold_id, container_id, "TRANSPORTED")

        self.store.emit_event({
            "event_type": "container_transported",
            "source": self.node_id,
            "container_id": container_id,
            "destination": destination,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        spatial_event = {
            "event_type": "spatial_update",
            "source": self.node_id,
            "priority": "ROUTINE",
            "details": {
                "operation": "tractor_transport",
                "tractor_id": self.node_id,
                "container_id": container_id,
                "destination_block": destination,
            },
        }
        print(json.dumps(spatial_event), flush=True)

        logger.info(
            f"METRICS: container_transported node={self.node_id} "
            f"container={container_id} dest={destination}"
        )
        return f"Container {container_id} transported to {destination}."

    async def _tool_report_position(self, arguments: dict) -> str:
        zone = arguments["zone"]
        block = arguments["block"]
        status = arguments["status"]

        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        entity.update_field("position.zone", zone)
        entity.update_field("position.block", block)
        entity.update_field("position.status", status)

        logger.info(
            f"METRICS: position_update node={self.node_id} zone={zone} block={block} status={status}"
        )
        return f"Position updated: {zone}/{block}, status={status}."

    async def _tool_request_charge(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if not entity:
            return "Error: entity document not found"

        entity.update_field("position.status", "CHARGING")
        entity.update_field("operational_status", "CHARGING")

        self.store.emit_event({
            "event_type": "RESUPPLY_REQUESTED",
            "source": self.node_id,
            "priority": "HIGH",
            "details": {
                "resource": "battery_pct",
                "equipment_id": self.node_id,
                "current_level": entity.get_field("equipment_health.battery_pct", 0),
                "eta_sim_minutes": 5.0,
            },
        })

        logger.info(f"METRICS: charge_requested node={self.node_id}")
        return "Charging requested. Status: CHARGING."

    # ── Scheduler tool handlers ──────────────────────────────────────────

    async def _tool_rebalance_assignments(self, arguments: dict) -> str:
        assignments = arguments.get("assignments", {})

        entity = self._get_entity_doc()
        if entity:
            rebalances = entity.get_field("metrics.rebalances", 0)
            entity.update_field("metrics.rebalances", rebalances + 1)

        for op_id, crane_id in assignments.items():
            self.store.emit_event({
                "event_type": "operator_reassigned",
                "source": self.node_id,
                "operator_id": op_id,
                "crane_id": crane_id,
                "aggregation_policy": "AGGREGATE_AT_PARENT",
                "priority": "NORMAL",
            })

        logger.info(
            f"METRICS: rebalance_assignments node={self.node_id} "
            f"assignments={len(assignments)}"
        )
        return f"Rebalanced {len(assignments)} assignments."

    async def _tool_update_priority_queue(self, arguments: dict) -> str:
        priority_order = arguments.get("priority_order", [])

        self.store.emit_event({
            "event_type": "queue_reordered",
            "source": self.node_id,
            "priority_order": priority_order,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: queue_reordered node={self.node_id} count={len(priority_order)}"
        )
        return f"Priority queue updated with {len(priority_order)} entries."

    async def _tool_dispatch_resource(self, arguments: dict) -> str:
        resource_type = arguments["resource_type"]
        from_entity = arguments["from_entity"]
        to_entity = arguments["to_entity"]
        reason = arguments["reason"]

        entity = self._get_entity_doc()
        if entity:
            dispatches = entity.get_field("metrics.dispatches", 0)
            entity.update_field("metrics.dispatches", dispatches + 1)

        self.store.emit_event({
            "event_type": "resource_dispatched",
            "source": self.node_id,
            "resource_type": resource_type,
            "from_entity": from_entity,
            "to_entity": to_entity,
            "reason": reason,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "HIGH",
        })

        logger.info(
            f"METRICS: resource_dispatched node={self.node_id} "
            f"type={resource_type} {from_entity}→{to_entity}"
        )
        return f"Dispatched {resource_type} from {from_entity} to {to_entity}. Reason: {reason}"

    async def _tool_assign_container(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        assigned_crane = arguments.get("assigned_crane")
        assigned_operator = arguments.get("assigned_operator")
        assigned_tractor = arguments.get("assigned_tractor")

        ok = self.store.assign_container(
            hold_id=self.hold_id,
            container_id=container_id,
            assigned_crane=assigned_crane,
            assigned_operator=assigned_operator,
            assigned_tractor=assigned_tractor,
        )
        if not ok:
            return f"Error: could not assign container {container_id}"

        parts = [f"crane={assigned_crane}"]
        if assigned_operator:
            parts.append(f"operator={assigned_operator}")
        if assigned_tractor:
            parts.append(f"tractor={assigned_tractor}")

        logger.info(
            f"METRICS: container_assignment node={self.node_id} "
            f"container={container_id} {' '.join(parts)}"
        )
        return (
            f"Container {container_id} assigned: {', '.join(parts)}. "
            f"Status: QUEUED → tracking through lifecycle."
        )

    async def _tool_emit_schedule_event(self, arguments: dict) -> str:
        event_type = arguments["event_type"]
        details = arguments["details"]
        priority = arguments["priority"]

        self.store.emit_event({
            "event_type": event_type,
            "source": self.node_id,
            "details": details,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": priority,
        })

        logger.info(
            f"METRICS: schedule_event node={self.node_id} type={event_type} priority={priority}"
        )
        return f"Schedule event emitted: {event_type} ({priority})."

    # ── Sensor tool handlers ─────────────────────────────────────────────

    async def _tool_emit_reading(self, arguments: dict) -> str:
        reading_type = arguments["reading_type"]
        value = arguments["value"]
        unit = arguments["unit"]
        container_id = arguments.get("container_id")

        entity = self._get_entity_doc()
        if entity:
            readings = entity.get_field("metrics.readings_emitted", 0)
            entity.update_field("metrics.readings_emitted", readings + 1)

        event = {
            "event_type": "sensor_reading",
            "source": self.node_id,
            "reading_type": reading_type,
            "value": value,
            "unit": unit,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "ROUTINE",
        }
        if container_id:
            event["container_id"] = container_id
        self.store.emit_event(event)

        # Anomaly detection: >5% divergence from expected
        expected = 25.0 if reading_type == "weight" else 100.0
        if abs(value - expected) / expected > 0.05:
            self.store.emit_event({
                "event_type": "anomaly_detected",
                "source": self.node_id,
                "reading_type": reading_type,
                "value": value,
                "expected": expected,
                "aggregation_policy": "IMMEDIATE_PROPAGATE",
                "priority": "HIGH",
            })
            if entity:
                anomalies = entity.get_field("metrics.anomalies_detected", 0)
                entity.update_field("metrics.anomalies_detected", anomalies + 1)

        logger.info(
            f"METRICS: sensor_reading node={self.node_id} type={reading_type} value={value}{unit}"
        )
        return f"Reading emitted: {reading_type}={value}{unit}."

    async def _tool_report_calibration(self, arguments: dict) -> str:
        accuracy_pct = arguments["accuracy_pct"]
        drift = arguments["drift"]
        status = arguments["status"]

        entity = self._get_entity_doc()
        if entity:
            entity.update_field("calibration.accuracy_pct", accuracy_pct)
            entity.update_field("calibration.drift", drift)
            entity.update_field("calibration.status", status)

        # Emit CALIBRATION_DRIFT if below threshold
        if accuracy_pct < 95.0:
            self.store.emit_event({
                "event_type": "CALIBRATION_DRIFT",
                "source": self.node_id,
                "priority": "HIGH" if accuracy_pct < 85.0 else "NORMAL",
                "details": {
                    "sensor_id": self.node_id,
                    "accuracy_pct": accuracy_pct,
                    "drift": drift,
                    "status": status,
                },
            })

        logger.info(
            f"METRICS: calibration node={self.node_id} "
            f"accuracy={accuracy_pct}% drift={drift} status={status}"
        )
        return f"Calibration reported: {accuracy_pct}% accuracy, drift={drift}, status={status}."

    # ── Lashing Crew tool handlers ──────────────────────────────────────

    async def _tool_secure_container(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        lashing_type = arguments["lashing_type"]

        entity = self._get_entity_doc()
        if entity:
            secured = entity.get_field("metrics.containers_secured", 0)
            entity.update_field("metrics.containers_secured", secured + 1)

        self.store.emit_event({
            "event_type": "container_secured",
            "source": self.node_id,
            "container_id": container_id,
            "lashing_type": lashing_type,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: container_secured node={self.node_id} "
            f"container={container_id} type={lashing_type}"
        )
        return (
            f"Container {container_id} secured with {lashing_type}. "
            f"Total secured: {(entity.get_field('metrics.containers_secured', 0) if entity else 0)}."
        )

    async def _tool_report_lashing_complete(self, arguments: dict) -> str:
        container_id = arguments["container_id"]

        self.store.emit_event({
            "event_type": "lashing_complete",
            "source": self.node_id,
            "container_id": container_id,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(
            f"METRICS: lashing_complete node={self.node_id} container={container_id}"
        )
        return f"Lashing complete for container {container_id}. Ready for next operation."

    async def _tool_inspect_lashing(self, arguments: dict) -> str:
        container_id = arguments["container_id"]
        result = arguments["result"]

        entity = self._get_entity_doc()
        if entity:
            inspections = entity.get_field("metrics.lashings_inspected", 0)
            entity.update_field("metrics.lashings_inspected", inspections + 1)

        priority = "NORMAL"
        if result == "FAIL":
            priority = "CRITICAL"
        elif result == "DEGRADED":
            priority = "HIGH"

        self.store.emit_event({
            "event_type": "lashing_inspection",
            "source": self.node_id,
            "container_id": container_id,
            "result": result,
            "aggregation_policy": "IMMEDIATE_PROPAGATE" if result != "PASS" else "AGGREGATE_AT_PARENT",
            "priority": priority,
        })

        logger.info(
            f"METRICS: lashing_inspection node={self.node_id} "
            f"container={container_id} result={result}"
        )
        return f"Lashing inspection for {container_id}: {result}."

    async def _tool_request_lashing_tools(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if entity:
            entity.update_field("equipment_health.lashing_tools_status", "REQUESTED")

        self.store.emit_event({
            "event_type": "RESUPPLY_REQUESTED",
            "source": self.node_id,
            "priority": "HIGH",
            "details": {
                "resource": "lashing_tools",
                "equipment_id": self.node_id,
                "eta_sim_minutes": 3.0,
            },
        })

        logger.info(f"METRICS: lashing_tools_requested node={self.node_id}")
        return "Lashing tools requested. Replacement equipment en route."

    # ── Signaler tool handlers ─────────────────────────────────────────

    async def _tool_signal_crane(self, arguments: dict) -> str:
        signal_type = arguments["signal_type"]
        crane_id = arguments["crane_id"]

        entity = self._get_entity_doc()
        if entity:
            signals = entity.get_field("metrics.signals_sent", 0)
            entity.update_field("metrics.signals_sent", signals + 1)

        priority = "CRITICAL" if signal_type == "STOP" else "NORMAL"
        self.store.emit_event({
            "event_type": "crane_signal",
            "source": self.node_id,
            "signal_type": signal_type,
            "crane_id": crane_id,
            "aggregation_policy": "IMMEDIATE_PROPAGATE" if signal_type == "STOP" else "AGGREGATE_AT_PARENT",
            "priority": priority,
        })

        logger.info(
            f"METRICS: crane_signal node={self.node_id} "
            f"signal={signal_type} crane={crane_id}"
        )
        return f"Signal {signal_type} sent to {crane_id}."

    async def _tool_report_hazard(self, arguments: dict) -> str:
        hazard_type = arguments["hazard_type"]
        location = arguments["location"]

        entity = self._get_entity_doc()
        if entity:
            hazards = entity.get_field("metrics.hazards_reported", 0)
            entity.update_field("metrics.hazards_reported", hazards + 1)

        self.store.emit_event({
            "event_type": "hazard_reported",
            "source": self.node_id,
            "hazard_type": hazard_type,
            "location": location,
            "aggregation_policy": "IMMEDIATE_PROPAGATE",
            "priority": "CRITICAL",
        })

        logger.info(
            f"METRICS: hazard_reported node={self.node_id} "
            f"type={hazard_type} location={location}"
        )
        return f"Hazard reported: {hazard_type} at {location}. All crane ops halted."

    async def _tool_confirm_ground_clear(self, arguments: dict) -> str:
        entity = self._get_entity_doc()
        if entity:
            clears = entity.get_field("metrics.ground_clears", 0)
            entity.update_field("metrics.ground_clears", clears + 1)

        self.store.emit_event({
            "event_type": "ground_clear_confirmed",
            "source": self.node_id,
            "aggregation_policy": "AGGREGATE_AT_PARENT",
            "priority": "NORMAL",
        })

        logger.info(f"METRICS: ground_clear node={self.node_id}")
        return "Ground zone confirmed clear. Safe to proceed with crane operations."
